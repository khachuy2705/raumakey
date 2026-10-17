//! Read-only facts about the foreground window and processes.
//!
//! Window titles and full image paths are never returned: only the lowercase executable
//! basename and window class names, which the design allows in diagnostics.

/// `ES_PASSWORD` edit style.
const ES_PASSWORD: u32 = 0x0020;

/// True for an Edit or RichEdit control (class compared ASCII case-insensitively) with the
/// `ES_PASSWORD` style. Password fields of other UI frameworks are detected through TSF input
/// scopes in the TIP instead.
pub fn is_password_edit(class: &str, style: u32) -> bool {
    let class = class.to_ascii_lowercase();
    (class == "edit" || class.starts_with("richedit")) && style & ES_PASSWORD != 0
}

#[cfg(windows)]
pub use win::{
    ForegroundInfo, ForegroundWindow, foreground_info, foreground_window, process_image_basename,
    window_class_name,
};

#[cfg(windows)]
pub use crate::sid::{current_process_is_elevated, process_is_elevated};

#[cfg(windows)]
mod win {
    use raumakey_model::host::ExeName;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo,
        GetWindowLongW, GetWindowThreadProcessId,
    };
    use windows::core::PWSTR;

    use super::is_password_edit;
    use crate::error::{E_FAIL, ResultExt, WinError, WinResult, win32};
    use crate::handle;
    use crate::sid::process_is_elevated;

    /// Longest image path accepted (the NT path limit).
    const MAX_IMAGE_PATH: usize = 32_768;

    /// The foreground window and its owner.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ForegroundInfo {
        /// Window handle value.
        pub hwnd: isize,
        /// Owning process id.
        pub process_id: u32,
        /// Owning (GUI) thread id.
        pub thread_id: u32,
        /// Lowercase executable basename, when the process can be queried.
        pub exe: Option<ExeName>,
        /// Class of the foreground window.
        pub window_class: Option<String>,
        /// Class of the window with keyboard focus in that thread.
        pub focus_window_class: Option<String>,
        /// The focused control is a password Edit control (see
        /// [`super::is_password_edit`]).
        pub focus_is_password: bool,
        /// Whether the process is elevated; `None` when its token cannot be read (typically an
        /// elevated process seen from a non-elevated caller).
        pub is_elevated: Option<bool>,
    }

    /// The foreground window and the window with keyboard focus in its thread, without the
    /// process queries of [`ForegroundInfo`]: a few window-manager reads, cheap enough for
    /// every key of a low-level keyboard hook.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ForegroundWindow {
        /// Foreground window handle value.
        pub hwnd: isize,
        /// Handle value of the window with keyboard focus in the foreground thread
        /// (`GetGUIThreadInfo`), 0 when there is none or it cannot be read.
        pub focus_hwnd: isize,
        /// Owning process id.
        pub process_id: u32,
        /// Owning (GUI) thread id.
        pub thread_id: u32,
        /// Class of the foreground window.
        pub window_class: Option<String>,
        /// Class of the window with keyboard focus.
        pub focus_window_class: Option<String>,
        /// The focused control is a password Edit control (see
        /// [`super::is_password_edit`]).
        pub focus_is_password: bool,
    }

    /// Describes the foreground window and its focus; `None` when there is no foreground
    /// window (secure desktop, lock screen, or a window that is being destroyed).
    pub fn foreground_window() -> Option<ForegroundWindow> {
        // SAFETY: no preconditions.
        let hwnd = unsafe { GetForegroundWindow() };
        if hwnd.is_invalid() {
            return None;
        }
        let mut process_id = 0_u32;
        // SAFETY: valid window handle value (stale handles fail gracefully) and out pointer.
        let thread_id = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process_id)) };
        if thread_id == 0 {
            return None;
        }
        let mut gui = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `gui` is a valid, size-initialized out structure.
        let focus = match unsafe { GetGUIThreadInfo(thread_id, &mut gui) } {
            Ok(()) if !gui.hwndFocus.is_invalid() => Some(gui.hwndFocus),
            _ => None,
        };
        let focus_window_class = focus.and_then(|focus| window_class_name(focus.0 as isize));
        let focus_is_password = match (focus, focus_window_class.as_deref()) {
            (Some(focus), Some(class)) => is_password_edit(class, window_style(focus)),
            _ => false,
        };
        Some(ForegroundWindow {
            hwnd: hwnd.0 as isize,
            focus_hwnd: focus.map_or(0, |focus| focus.0 as isize),
            process_id,
            thread_id,
            window_class: window_class_name(hwnd.0 as isize),
            focus_window_class,
            focus_is_password,
        })
    }

    /// Describes the foreground window; `None` when there is none (secure desktop, lock
    /// screen, or a window that is being destroyed).
    pub fn foreground_info() -> Option<ForegroundInfo> {
        let window = foreground_window()?;
        Some(ForegroundInfo {
            hwnd: window.hwnd,
            process_id: window.process_id,
            thread_id: window.thread_id,
            exe: process_image_basename(window.process_id).ok(),
            window_class: window.window_class,
            focus_window_class: window.focus_window_class,
            focus_is_password: window.focus_is_password,
            is_elevated: process_is_elevated(window.process_id).ok(),
        })
    }

    fn window_style(hwnd: HWND) -> u32 {
        // SAFETY: reading a window long has no memory preconditions; stale handles yield 0.
        unsafe { GetWindowLongW(hwnd, GWL_STYLE) as u32 }
    }

    /// Class name of window `hwnd`; `None` for 0 or a window that no longer exists.
    pub fn window_class_name(hwnd: isize) -> Option<String> {
        if hwnd == 0 {
            return None;
        }
        let mut buffer = [0_u16; 257];
        // SAFETY: the buffer is writable for its whole length.
        let len = unsafe { GetClassNameW(HWND(hwnd as *mut core::ffi::c_void), &mut buffer) };
        let len = usize::try_from(len).ok().filter(|len| *len > 0)?;
        Some(String::from_utf16_lossy(buffer.get(..len)?))
    }

    /// Lowercase executable basename of process `pid` (`QueryFullProcessImageNameW`). The full
    /// path is read but never returned.
    pub fn process_image_basename(pid: u32) -> WinResult<ExeName> {
        // SAFETY: plain call; the handle is owned below.
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
            .context("OpenProcess")?;
        // SAFETY: OpenProcess returned a new handle owned by nobody else.
        let process = unsafe { handle::take(process) };
        let mut buffer = vec![0_u16; 1024];
        loop {
            let mut len = u32::try_from(buffer.len()).unwrap_or(u32::MAX);
            // SAFETY: the buffer is writable for `len` units; `len` is a valid in/out pointer.
            let result = unsafe {
                QueryFullProcessImageNameW(
                    handle::raw(&process),
                    PROCESS_NAME_FORMAT(0),
                    PWSTR(buffer.as_mut_ptr()),
                    &mut len,
                )
            };
            match result {
                Ok(()) => {
                    let path = String::from_utf16_lossy(buffer.get(..len as usize).unwrap_or(&[]));
                    return ExeName::from_path(&path)
                        .map_err(|_| WinError::new(E_FAIL, "process image name"));
                }
                Err(error)
                    if error.code().0
                        == crate::error::hresult_from_win32(win32::ERROR_INSUFFICIENT_BUFFER)
                        && buffer.len() < MAX_IMAGE_PATH =>
                {
                    buffer = vec![0_u16; buffer.len() * 4];
                }
                Err(error) => {
                    return Err(WinError::new(error.code().0, "QueryFullProcessImageNameW"));
                }
            }
        }
    }
}
