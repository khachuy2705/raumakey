//! Read-only facts about the foreground window and processes, and a watcher of the foreground
//! window (Windows).
//!
//! Window titles and full image paths are never returned: only the lowercase executable
//! basename and window class names, which the design allows in diagnostics.
//!
//! On Windows, [`foreground_window`] and [`foreground_info`] describe the window in the
//! foreground; [`describe_window`] (with the focus of the window's thread),
//! [`window_process_id`], [`process_image_basename`], and [`window_class_name`] describe any
//! window; and a [`ForegroundWatcher`] (an out-of-context `EVENT_SYSTEM_FOREGROUND` hook) posts
//! a private message to a window of its thread each time another window comes to the
//! foreground, so the controller's icon can follow the application in front (D47).

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
    ForegroundInfo, ForegroundWatcher, ForegroundWindow, describe_window, foreground_info,
    foreground_window, process_image_basename, window_class_name, window_process_id,
};

#[cfg(windows)]
pub use crate::sid::{current_process_is_elevated, process_is_elevated};

#[cfg(windows)]
mod win {
    use core::cell::RefCell;

    use raumakey_model::host::ExeName;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
    use windows::Win32::UI::WindowsAndMessaging::{
        EVENT_SYSTEM_FOREGROUND, GUITHREADINFO, GWL_STYLE, GetClassNameW, GetForegroundWindow,
        GetGUIThreadInfo, GetWindowLongW, GetWindowThreadProcessId, WINEVENT_OUTOFCONTEXT,
    };
    use windows::core::PWSTR;

    use super::is_password_edit;
    use crate::error::{E_FAIL, ResultExt, WinError, WinResult, win32};
    use crate::handle;
    use crate::panic::guard_value;
    use crate::sid::process_is_elevated;
    use crate::window::{AppMessage, WindowHandle};

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
        describe_window(hwnd.0 as isize)
    }

    /// Describes window `hwnd` as [`foreground_window`] describes the foreground window: its
    /// process and thread, its class, and the window with keyboard focus in its (GUI) thread;
    /// `None` for 0 or a window that no longer exists. For the window that came to the
    /// foreground (a [`ForegroundWatcher`] names it), the focus is the one its thread has now.
    pub fn describe_window(hwnd: isize) -> Option<ForegroundWindow> {
        if hwnd == 0 {
            return None;
        }
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        let mut process_id = 0_u32;
        // SAFETY: a window handle value (a stale or made-up one makes the call fail, returning
        // 0) and a valid out pointer.
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

    /// The process of window `hwnd` (`GetWindowThreadProcessId`); `None` for 0 or a window
    /// that no longer exists.
    pub fn window_process_id(hwnd: isize) -> Option<u32> {
        if hwnd == 0 {
            return None;
        }
        let mut process_id = 0_u32;
        // SAFETY: a window handle value (a stale one makes the call fail) and a valid out
        // pointer.
        let thread_id = unsafe {
            GetWindowThreadProcessId(HWND(hwnd as *mut core::ffi::c_void), Some(&mut process_id))
        };
        (thread_id != 0 && process_id != 0).then_some(process_id)
    }

    /// Watches the foreground window of the session: each time another window comes to the
    /// foreground (`EVENT_SYSTEM_FOREGROUND`, also between the windows of one thread or of this
    /// process), it posts a private message to a target window with the new foreground
    /// window's handle value in `wParam` (0 for none). The hook is removed (`UnhookWinEvent`)
    /// when the watcher is dropped.
    ///
    /// The hook is out of context (`WINEVENT_OUTOFCONTEXT`): no code is loaded into other
    /// processes, and the system calls the callback on the installing thread, from its message
    /// queue, while that thread waits for messages (in `GetMessageW`, `PeekMessageW`, or a
    /// modal loop such as a menu's). The callback does nothing but post the message (a full
    /// queue or a window that is gone loses it), and a panic in it is contained. A thread
    /// that pumps no messages gets no events, and while it is busy, the events wait. A message
    /// posted to a window whose handler runs a modal loop reaches `DefWindowProcW` instead of
    /// the handler (see [`crate::window`]): the owner should not rely on every change
    /// arriving, and should look at the foreground again from time to time.
    ///
    /// It belongs to the installing thread, so it is not `Send`:
    ///
    /// ```compile_fail
    /// fn assert_send<T: Send>() {}
    /// assert_send::<raumakey_windows::foreground::ForegroundWatcher>();
    /// ```
    #[derive(Debug)]
    pub struct ForegroundWatcher {
        hook: HWINEVENTHOOK,
    }

    impl ForegroundWatcher {
        /// Installs the watcher on the calling thread; each change of the foreground window
        /// is posted to `target` as `message`.
        pub fn install(target: WindowHandle, message: AppMessage) -> WinResult<Self> {
            // SAFETY: `foreground_proc` is a WINEVENTPROC that never unwinds; an out-of-context
            // hook needs no module and reaches no other process; every process and thread is
            // watched.
            let hook = unsafe {
                SetWinEventHook(
                    EVENT_SYSTEM_FOREGROUND,
                    EVENT_SYSTEM_FOREGROUND,
                    None,
                    Some(foreground_proc),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                )
            };
            if hook.is_invalid() {
                return Err(WinError::new(E_FAIL, "SetWinEventHook"));
            }
            // No event arrives before this: the callback runs only while this thread waits
            // for messages.
            watch(hook.0 as isize, target, message);
            Ok(Self { hook })
        }
    }

    impl Drop for ForegroundWatcher {
        fn drop(&mut self) {
            // SAFETY: the hook was installed by `install` on this thread (the watcher is not
            // `Send`) and is removed once.
            let _ = unsafe { UnhookWinEvent(self.hook) };
            unwatch(self.hook.0 as isize);
        }
    }

    /// Where the events of one hook of this thread go.
    #[derive(Clone, Copy, Debug)]
    struct Watch {
        /// The hook's handle value.
        hook: isize,
        target: WindowHandle,
        message: AppMessage,
    }

    thread_local! {
        /// The watchers installed on this thread; their callbacks run on it.
        static WATCHES: RefCell<Vec<Watch>> = const { RefCell::new(Vec::new()) };
    }

    fn watch(hook: isize, target: WindowHandle, message: AppMessage) {
        WATCHES.with(|watches| {
            watches.borrow_mut().push(Watch {
                hook,
                target,
                message,
            });
        });
    }

    fn unwatch(hook: isize) {
        let _ = WATCHES.try_with(|watches| {
            if let Ok(mut watches) = watches.try_borrow_mut() {
                watches.retain(|watch| watch.hook != hook);
            }
        });
    }

    /// Posts the foreground window `hwnd` of `event` to the target of `hook`, when it is a
    /// watcher of this thread and the event is a change of the foreground window.
    fn deliver(hook: isize, event: u32, hwnd: isize) {
        if event != EVENT_SYSTEM_FOREGROUND {
            return;
        }
        let _ = WATCHES.try_with(|watches| {
            let Ok(watches) = watches.try_borrow() else {
                return;
            };
            for watch in watches.iter().filter(|watch| watch.hook == hook) {
                // A full queue or a window that is gone loses only this change.
                let _ = watch
                    .target
                    .post(watch.message.id(), WPARAM(hwnd as usize), LPARAM(0));
            }
        });
    }

    /// The WinEvent procedure of every [`ForegroundWatcher`]. Never unwinds.
    unsafe extern "system" fn foreground_proc(
        hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        _id_object: i32,
        _id_child: i32,
        _event_thread: u32,
        _event_time: u32,
    ) {
        guard_value((), || deliver(hook.0 as isize, event, hwnd.0 as isize));
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

    #[cfg(test)]
    mod tests {
        //! The callback of the foreground watcher, called directly with a made-up hook and
        //! window (nothing comes to the foreground), and a real hook that is installed and
        //! removed on a test thread without waiting for an event.

        use core::cell::RefCell;
        use std::rc::Rc;

        use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows::Win32::UI::Accessibility::HWINEVENTHOOK;
        use windows::Win32::UI::WindowsAndMessaging::{
            DispatchMessageW, EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_MENUSTART, MSG, PM_REMOVE,
            PeekMessageW,
        };

        use super::{ForegroundWatcher, WATCHES, foreground_proc, unwatch, watch};
        use crate::window::{AppMessage, Window, WindowClass};

        const FOREGROUND: AppMessage = match AppMessage::new(4) {
            Some(message) => message,
            None => panic!("offset outside the WM_APP range"),
        };

        /// A made-up hook handle value, never one of a real hook.
        const FAKE_HOOK: isize = 0x0BAD_0001;

        /// Runs `body` on a new thread, which owns the windows and watchers it creates.
        fn on_fresh_thread(body: impl FnOnce() + Send + 'static) {
            if let Err(payload) = std::thread::spawn(body).join() {
                std::panic::resume_unwind(payload);
            }
        }

        /// A message-only window that records the `wParam` of each [`FOREGROUND`] it gets.
        fn recording_window(label: &str) -> (Window, Rc<RefCell<Vec<usize>>>) {
            let seen = Rc::new(RefCell::new(Vec::new()));
            let log = Rc::clone(&seen);
            let class = WindowClass::register(label).unwrap();
            let handler = move |_: HWND, message: u32, wparam: WPARAM, _: LPARAM| {
                if message == FOREGROUND.id() {
                    log.borrow_mut().push(wparam.0);
                    return Some(LRESULT(0));
                }
                None
            };
            let window = Window::create_message_only(&class, "", Box::new(handler)).unwrap();
            (window, seen)
        }

        /// Dispatches every message waiting in this thread's queue.
        fn pump() {
            let mut message = MSG::default();
            // SAFETY: `message` is a valid out buffer; the messages belong to this thread.
            while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                // SAFETY: `message` was filled by PeekMessageW.
                unsafe { DispatchMessageW(&message) };
            }
        }

        /// Calls the WinEvent procedure as the system would.
        fn event(hook: isize, event: u32, hwnd: isize) {
            // SAFETY: the procedure reads only its arguments, which are plain values here.
            unsafe {
                foreground_proc(
                    HWINEVENTHOOK(hook as *mut core::ffi::c_void),
                    event,
                    HWND(hwnd as *mut core::ffi::c_void),
                    0,
                    0,
                    0,
                    0,
                );
            }
        }

        #[test]
        fn a_change_of_the_foreground_is_posted_to_the_target_of_its_hook() {
            on_fresh_thread(|| {
                let (window, seen) = recording_window("unit-foreground-post");
                watch(FAKE_HOOK, window.handle(), FOREGROUND);
                event(FAKE_HOOK, EVENT_SYSTEM_FOREGROUND, 0x1234);
                // Another event, another hook, and no window: only the last is posted (0).
                event(FAKE_HOOK, EVENT_SYSTEM_MENUSTART, 0x5678);
                event(FAKE_HOOK + 1, EVENT_SYSTEM_FOREGROUND, 0x5678);
                event(FAKE_HOOK, EVENT_SYSTEM_FOREGROUND, 0);
                pump();
                assert_eq!(*seen.borrow(), [0x1234, 0]);
                unwatch(FAKE_HOOK);
                event(FAKE_HOOK, EVENT_SYSTEM_FOREGROUND, 0x1234);
                pump();
                assert_eq!(*seen.borrow(), [0x1234, 0], "nothing after it is removed");
                drop(window);
            });
        }

        #[test]
        fn a_target_that_is_gone_loses_only_the_event() {
            on_fresh_thread(|| {
                let (window, seen) = recording_window("unit-foreground-gone");
                let target = window.handle();
                drop(window);
                watch(FAKE_HOOK, target, FOREGROUND);
                event(FAKE_HOOK, EVENT_SYSTEM_FOREGROUND, 0x1234);
                pump();
                assert!(seen.borrow().is_empty());
                unwatch(FAKE_HOOK);
            });
        }

        #[test]
        fn a_real_hook_is_installed_and_removed_on_its_thread() {
            on_fresh_thread(|| {
                let (window, _seen) = recording_window("unit-foreground-real");
                let watcher = ForegroundWatcher::install(window.handle(), FOREGROUND).unwrap();
                let hook = watcher.hook.0 as isize;
                assert!(WATCHES.with(|watches| watches.borrow().iter().any(|w| w.hook == hook)));
                drop(watcher);
                assert!(WATCHES.with(|watches| watches.borrow().is_empty()));
                drop(window);
            });
        }
    }
}
