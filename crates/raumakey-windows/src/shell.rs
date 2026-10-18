//! Shell integration: the notification-area icon, `TaskbarCreated`, and shortcuts.
//!
//! [`tooltip_units`] and [`decode_notify_icon_message`] are pure. A [`NotifyIcon`] uses
//! `NOTIFYICON_VERSION_4`, so its window receives the callback message with the notification
//! and the icon id in `lParam` and the anchor point in `wParam`;
//! [`decode_notify_icon_message`] reads them. With version 4 the shell reports a click as
//! [`NIN_SELECT`], a selection with the keyboard as [`NIN_KEYSELECT`], and a request for the
//! shortcut menu (right click, Shift+F10, or the menu key) as [`WM_CONTEXTMENU`]; the Windows
//! SDK notification-icon sample recommends these over the raw mouse messages, which the shell
//! sends as well. Of those, the press and the release of the left button ([`WM_LBUTTONDOWN`],
//! [`WM_LBUTTONUP`]) and the second press of a double-click ([`WM_LBUTTONDBLCLK`]) are decoded
//! too, for a window that tells a double-click from two clicks: the shell reports a
//! double-click as a press, a release, [`NIN_SELECT`], [`WM_LBUTTONDBLCLK`], a release, and,
//! depending on the version of Windows, a second [`NIN_SELECT`]; a second press that is no
//! double-click (too late, or too far from the first) comes as [`WM_LBUTTONDOWN`].

/// Capacity of `NOTIFYICONDATAW::szTip` in UTF-16 units, including the terminating NUL.
pub const TOOLTIP_CAPACITY: usize = 128;

/// `NIN_SELECT` (`WM_USER`): the user selected the icon with the mouse (a click).
pub const NIN_SELECT: u32 = 0x0400;
/// `NINF_KEY`: the bit that marks a selection made with the keyboard.
pub const NINF_KEY: u32 = 0x0001;
/// `NIN_KEYSELECT` (`NIN_SELECT | NINF_KEY`): the user selected the icon with the keyboard
/// (Space or Enter on the focused icon).
pub const NIN_KEYSELECT: u32 = NIN_SELECT | NINF_KEY;
/// `WM_CONTEXTMENU`: the user asked for the icon's shortcut menu (right click, Shift+F10, or
/// the menu key).
pub const WM_CONTEXTMENU: u32 = 0x007B;
/// `WM_LBUTTONDOWN`: the left button was pressed on the icon.
pub const WM_LBUTTONDOWN: u32 = 0x0201;
/// `WM_LBUTTONUP`: the left button was released on the icon.
pub const WM_LBUTTONUP: u32 = 0x0202;
/// `WM_LBUTTONDBLCLK`: the left button was pressed on the icon a second time within the
/// double-click time (it takes the place of that press's `WM_LBUTTONDOWN`).
pub const WM_LBUTTONDBLCLK: u32 = 0x0203;

/// What a notification icon reports through its callback message (`NOTIFYICON_VERSION_4`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NotifyIconEvent {
    /// [`NIN_SELECT`]: the icon was clicked.
    Select,
    /// [`NIN_KEYSELECT`]: the icon was selected with the keyboard.
    KeySelect,
    /// [`WM_LBUTTONDOWN`]: the left button was pressed on the icon (a click begins).
    ButtonDown,
    /// [`WM_LBUTTONUP`]: the left button was released on the icon.
    ButtonUp,
    /// [`WM_LBUTTONDBLCLK`]: the second press of a double-click.
    DoubleClick,
    /// [`WM_CONTEXTMENU`]: the shortcut menu was requested; it opens at the anchor point.
    ContextMenu {
        /// Horizontal screen coordinate of the anchor (negative on monitors left of the
        /// primary one).
        x: i32,
        /// Vertical screen coordinate of the anchor (negative on monitors above the primary
        /// one).
        y: i32,
    },
    /// Any other notification, by its code: the other mouse messages (`WM_MOUSEMOVE`, the
    /// right and middle buttons), `NIN_POPUPOPEN`, the balloon notifications, and so on.
    Other(u16),
}

/// A decoded callback message of a notification icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NotifyIconMessage {
    /// The id the icon was added with. Version 4 reports 16 bits, so a window should give its
    /// icons ids below `0x10000`.
    pub icon_id: u16,
    /// What happened.
    pub event: NotifyIconEvent,
}

/// Low word of `value` (`LOWORD`).
const fn low_word(value: usize) -> u16 {
    (value & 0xFFFF) as u16
}

/// High word of the low 32 bits of `value` (`HIWORD`).
const fn high_word(value: usize) -> u16 {
    ((value >> 16) & 0xFFFF) as u16
}

/// Decodes the parameters of a version-4 callback message: `LOWORD(lParam)` is the
/// notification and `HIWORD(lParam)` the icon id; for [`WM_CONTEXTMENU`] the anchor point is
/// `GET_X_LPARAM(wParam)`, `GET_Y_LPARAM(wParam)` (signed 16-bit screen coordinates). Bits
/// above the low 32 bits of either parameter are ignored.
pub const fn decode_notify_icon_message(wparam: usize, lparam: isize) -> NotifyIconMessage {
    let code = low_word(lparam as usize);
    let event = match code as u32 {
        NIN_SELECT => NotifyIconEvent::Select,
        NIN_KEYSELECT => NotifyIconEvent::KeySelect,
        WM_LBUTTONDOWN => NotifyIconEvent::ButtonDown,
        WM_LBUTTONUP => NotifyIconEvent::ButtonUp,
        WM_LBUTTONDBLCLK => NotifyIconEvent::DoubleClick,
        WM_CONTEXTMENU => NotifyIconEvent::ContextMenu {
            x: low_word(wparam) as i16 as i32,
            y: high_word(wparam) as i16 as i32,
        },
        _ => NotifyIconEvent::Other(code),
    };
    NotifyIconMessage {
        icon_id: high_word(lparam as usize),
        event,
    }
}

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
    use std::rc::Rc;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
    };
    use windows::Win32::UI::Shell::{
        IShellLinkW, NIF_ICON, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NIM_SETVERSION, NOTIFY_ICON_MESSAGE, NOTIFYICON_VERSION_4, NOTIFYICONDATAW,
        Shell_NotifyIconW, ShellLink,
    };
    use windows::core::Interface;

    use super::tooltip_units;
    use crate::com::ComApartment;
    use crate::error::{E_FAIL, E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::icon::Icon;
    use crate::strings::{WideString, until_nul};
    use crate::window::register_message;

    /// `SLGP_RAWPATH`: return the stored target path without resolution.
    const SLGP_RAWPATH: u32 = 0x4;
    /// Text buffer size used when reading shortcut fields.
    const FIELD_CAPACITY: usize = 32_768;

    /// The message id Explorer broadcasts after it (re)creates the taskbar; the controller
    /// re-adds its icon without changing mode (matrix #58).
    pub fn taskbar_created_message() -> WinResult<u32> {
        register_message("TaskbarCreated")
            .map_err(|error| error.with_context("RegisterWindowMessageW(TaskbarCreated)"))
    }

    /// The controller's notification-area icon (`NOTIFYICON_VERSION_4`). It is removed on drop.
    /// The icon belongs to the thread of its window and is not `Send`.
    ///
    /// It shares ownership of the [`Icon`] it shows (`Rc<Icon>`): every later change
    /// ([`set_tooltip`](Self::set_tooltip), [`set_icon`](Self::set_icon)) and every
    /// [`readd`](Self::readd) after `TaskbarCreated` sends the icon handle to the shell again,
    /// so the icon must live as long as the notification icon may send it.
    pub struct NotifyIcon {
        data: NOTIFYICONDATAW,
        /// The icon whose handle `data` holds.
        icon: Rc<Icon>,
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
        /// Adds icon `id` of window `hwnd`, which receives `callback_message`, showing `icon`.
        pub fn add(
            hwnd: HWND,
            id: u32,
            callback_message: u32,
            icon: Rc<Icon>,
            tooltip: &str,
        ) -> WinResult<Self> {
            let data = NOTIFYICONDATAW {
                cbSize: size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: id,
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
                uCallbackMessage: callback_message,
                hIcon: icon.handle(),
                szTip: tooltip_units(tooltip),
                ..Default::default()
            };
            let mut tray = Self {
                data,
                icon,
                added: false,
            };
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

        /// Replaces the icon image. The new icon is kept even when the shell rejects the change
        /// (for example while Explorer restarts), so the next [`readd`](Self::readd) shows it;
        /// the previous icon is released after the call.
        pub fn set_icon(&mut self, icon: Rc<Icon>) -> WinResult<()> {
            self.data.hIcon = icon.handle();
            let previous = core::mem::replace(&mut self.icon, icon);
            let result = self.notify(NIM_MODIFY, "Shell_NotifyIconW(NIM_MODIFY)");
            drop(previous);
            result
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
