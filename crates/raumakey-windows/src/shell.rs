//! Shell integration: the notification-area icon, `TaskbarCreated`, and shortcuts.

/// Capacity of `NOTIFYICONDATAW::szTip` in UTF-16 units, including the terminating NUL.
pub const TOOLTIP_CAPACITY: usize = 128;

/// A tooltip buffer: `text` truncated to 127 units at a character boundary (a surrogate pair is
/// never split), NUL-terminated and NUL-padded.
pub fn tooltip_units(text: &str) -> [u16; TOOLTIP_CAPACITY] {
    let mut units = [0_u16; TOOLTIP_CAPACITY];
    let mut len = 0;
    for c in text.chars() {
        let mut buffer = [0_u16; 2];
        let encoded = c.encode_utf16(&mut buffer);
        let end = len + encoded.len();
        if end >= TOOLTIP_CAPACITY {
            break;
        }
        if let Some(slot) = units.get_mut(len..end) {
            slot.copy_from_slice(encoded);
        }
        len = end;
    }
    units
}

#[cfg(windows)]
pub use win::{
    NotifyIcon, ShortcutSpec, create_shortcut, read_shortcut, remove_shortcut,
    taskbar_created_message,
};

#[cfg(windows)]
mod win {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::{Path, PathBuf};

    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{
        IShellLinkW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NIM_SETVERSION, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
        Shell_NotifyIconW, ShellLink,
    };
    use windows::Win32::UI::WindowsAndMessaging::{HICON, RegisterWindowMessageW};
    use windows::core::{Interface, w};

    use super::tooltip_units;
    use crate::com::ComApartment;
    use crate::error::{E_FAIL, E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::strings::{WideString, until_nul};

    /// `SLGP_RAWPATH`: return the stored target path without resolution.
    const SLGP_RAWPATH: u32 = 0x4;
    /// Text buffer size used when reading shortcut fields.
    const FIELD_CAPACITY: usize = 32_768;

    /// The message id Explorer broadcasts after it (re)creates the taskbar; the controller
    /// re-adds its icon without changing mode (matrix #58).
    pub fn taskbar_created_message() -> WinResult<u32> {
        // SAFETY: the name is a static NUL-terminated string.
        let message = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
        if message == 0 {
            return Err(WinError::last("RegisterWindowMessageW(TaskbarCreated)"));
        }
        Ok(message)
    }

    /// The controller's notification-area icon (`NOTIFYICON_VERSION_4`). It is removed on drop.
    /// The icon belongs to the thread of its window and is not `Send`.
    pub struct NotifyIcon {
        data: NOTIFYICONDATAW,
        added: bool,
    }

    impl core::fmt::Debug for NotifyIcon {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            // Copied out by value: NOTIFYICONDATAW is packed on x86, so its fields must not be
            // borrowed.
            let id = self.data.uID;
            f.debug_struct("NotifyIcon")
                .field("id", &id)
                .field("added", &self.added)
                .finish_non_exhaustive()
        }
    }

    impl NotifyIcon {
        /// Adds icon `id` of window `hwnd`, which receives `callback_message`.
        pub fn add(
            hwnd: HWND,
            id: u32,
            callback_message: u32,
            icon: HICON,
            tooltip: &str,
        ) -> WinResult<Self> {
            let data = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: id,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
                uCallbackMessage: callback_message,
                hIcon: icon,
                szTip: tooltip_units(tooltip),
                ..Default::default()
            };
            let mut tray = Self { data, added: false };
            tray.readd()?;
            Ok(tray)
        }

        /// Adds the icon again, as needed after `TaskbarCreated`.
        pub fn readd(&mut self) -> WinResult<()> {
            self.notify(NIM_ADD, "Shell_NotifyIconW(NIM_ADD)")?;
            self.added = true;
            self.data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            self.notify(NIM_SETVERSION, "Shell_NotifyIconW(NIM_SETVERSION)")
        }

        /// Replaces the icon image.
        pub fn set_icon(&mut self, icon: HICON) -> WinResult<()> {
            self.data.hIcon = icon;
            self.notify(NIM_MODIFY, "Shell_NotifyIconW(NIM_MODIFY)")
        }

        /// Replaces the tooltip.
        pub fn set_tooltip(&mut self, tooltip: &str) -> WinResult<()> {
            self.data.szTip = tooltip_units(tooltip);
            self.notify(NIM_MODIFY, "Shell_NotifyIconW(NIM_MODIFY)")
        }

        /// Removes the icon.
        pub fn delete(mut self) -> WinResult<()> {
            self.remove()
        }

        fn remove(&mut self) -> WinResult<()> {
            if !self.added {
                return Ok(());
            }
            self.added = false;
            self.notify(NIM_DELETE, "Shell_NotifyIconW(NIM_DELETE)")
        }

        fn notify(&self, message: NOTIFY_ICON_MESSAGE, context: &'static str) -> WinResult<()> {
            // SAFETY: `data` is a fully initialized NOTIFYICONDATAW with its size set.
            if unsafe { Shell_NotifyIconW(message, &self.data) }.as_bool() {
                Ok(())
            } else {
                Err(WinError::new(E_FAIL, context))
            }
        }
    }

    impl Drop for NotifyIcon {
        fn drop(&mut self) {
            let _ = self.remove();
        }
    }

    /// Contents of a `.lnk` shortcut.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct ShortcutSpec {
        /// Target executable.
        pub target: PathBuf,
        /// Command-line arguments (already quoted as needed).
        pub arguments: String,
        /// Working directory.
        pub working_dir: Option<PathBuf>,
        /// Description (tooltip in Explorer).
        pub description: String,
        /// Icon file and index.
        pub icon: Option<(PathBuf, i32)>,
    }

    fn require_lnk(path: &Path) -> WinResult<()> {
        let is_lnk = path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lnk"));
        if is_lnk {
            Ok(())
        } else {
            Err(WinError::new(E_INVALIDARG, "shortcut path"))
        }
    }

    fn new_link() -> WinResult<IShellLinkW> {
        // SAFETY: COM is initialized on this thread by the caller's apartment guard.
        unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
            .context("CoCreateInstance(ShellLink)")
    }

    /// Creates (or overwrites) the shortcut `lnk` (which must end in `.lnk`). COM is
    /// initialized for the call when the thread has no apartment yet.
    pub fn create_shortcut(lnk: &Path, spec: &ShortcutSpec) -> WinResult<()> {
        require_lnk(lnk)?;
        let _apartment = ComApartment::sta()?;
        let link = new_link()?;
        let target = WideString::from_os(spec.target.as_os_str())?;
        let arguments = WideString::new(&spec.arguments)?;
        let description = WideString::new(&spec.description)?;
        // SAFETY: every string is NUL-terminated and outlives its call.
        unsafe {
            link.SetPath(target.as_pcwstr())
                .context("IShellLinkW::SetPath")?;
            link.SetArguments(arguments.as_pcwstr())
                .context("IShellLinkW::SetArguments")?;
            link.SetDescription(description.as_pcwstr())
                .context("IShellLinkW::SetDescription")?;
        }
        if let Some(dir) = &spec.working_dir {
            let dir = WideString::from_os(dir.as_os_str())?;
            // SAFETY: as above.
            unsafe { link.SetWorkingDirectory(dir.as_pcwstr()) }
                .context("IShellLinkW::SetWorkingDirectory")?;
        }
        if let Some((icon, index)) = &spec.icon {
            let icon = WideString::from_os(icon.as_os_str())?;
            // SAFETY: as above.
            unsafe { link.SetIconLocation(icon.as_pcwstr(), *index) }
                .context("IShellLinkW::SetIconLocation")?;
        }
        let file: IPersistFile = link.cast().context("IShellLinkW as IPersistFile")?;
        let path = WideString::from_os(lnk.as_os_str())?;
        // SAFETY: the path is NUL-terminated and outlives the call.
        unsafe { file.Save(path.as_pcwstr(), true) }.context("IPersistFile::Save")
    }

    /// Reads a shortcut back (verification and tests).
    pub fn read_shortcut(lnk: &Path) -> WinResult<ShortcutSpec> {
        require_lnk(lnk)?;
        let _apartment = ComApartment::sta()?;
        let link = new_link()?;
        let file: IPersistFile = link.cast().context("IShellLinkW as IPersistFile")?;
        let path = WideString::from_os(lnk.as_os_str())?;
        // SAFETY: the path is NUL-terminated and outlives the call.
        unsafe { file.Load(path.as_pcwstr(), STGM_READ) }.context("IPersistFile::Load")?;
        let mut buffer = vec![0_u16; FIELD_CAPACITY];
        // SAFETY: the buffer is writable for its whole length; no find data is requested.
        unsafe { link.GetPath(&mut buffer, core::ptr::null_mut(), SLGP_RAWPATH) }
            .context("IShellLinkW::GetPath")?;
        let target = path_of(&buffer);
        // SAFETY: as above.
        unsafe { link.GetArguments(&mut buffer) }.context("IShellLinkW::GetArguments")?;
        let arguments = String::from_utf16_lossy(until_nul(&buffer));
        // SAFETY: as above.
        unsafe { link.GetDescription(&mut buffer) }.context("IShellLinkW::GetDescription")?;
        let description = String::from_utf16_lossy(until_nul(&buffer));
        // SAFETY: as above.
        unsafe { link.GetWorkingDirectory(&mut buffer) }
            .context("IShellLinkW::GetWorkingDirectory")?;
        let working_dir = Some(path_of(&buffer)).filter(|dir| !dir.as_os_str().is_empty());
        let mut index = 0_i32;
        // SAFETY: as above; `index` is a valid out pointer.
        unsafe { link.GetIconLocation(&mut buffer, &mut index) }
            .context("IShellLinkW::GetIconLocation")?;
        let icon_path = path_of(&buffer);
        let icon = (!icon_path.as_os_str().is_empty()).then_some((icon_path, index));
        Ok(ShortcutSpec {
            target,
            arguments,
            working_dir,
            description,
            icon,
        })
    }

    fn path_of(buffer: &[u16]) -> PathBuf {
        PathBuf::from(OsString::from_wide(until_nul(buffer)))
    }

    /// Deletes the shortcut `lnk` (which must end in `.lnk`); returns whether it existed.
    pub fn remove_shortcut(lnk: &Path) -> WinResult<bool> {
        require_lnk(lnk)?;
        match std::fs::remove_file(lnk) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(WinError::from_io(&error, "remove shortcut")),
        }
    }
}
