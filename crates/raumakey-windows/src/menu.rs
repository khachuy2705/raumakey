//! Popup menus, such as the notification icon's context menu (plan §10.2).
//!
//! [`MenuItem`] and its `AppendMenuW` [`flags`] are pure. On Windows, [`PopupMenu`] owns a
//! menu (`CreatePopupMenu`, `DestroyMenu` on drop), appends items, and shows it with
//! [`PopupMenu::track`].
//!
//! Tracking follows Microsoft's guidance for notification-icon menus: the owner window becomes
//! the foreground window before `TrackPopupMenuEx` (otherwise the menu does not close when the
//! user clicks elsewhere), and a `WM_NULL` is posted to it afterwards (otherwise the menu may
//! close at once the next time it is shown). `TPM_RETURNCMD | TPM_NONOTIFY` make the call return
//! the chosen command instead of sending `WM_COMMAND`. Tracking runs a modal loop: called from a
//! window handler, messages for that window that arrive meanwhile get the default processing
//! (see [`crate::window`]).

use core::fmt;

use crate::error::{E_INVALIDARG, WinError, WinResult};

/// `AppendMenuW` flag bits used by [`MenuItem::flags`].
pub mod flags {
    /// `MF_STRING`: a text item.
    pub const MF_STRING: u32 = 0x0000;
    /// `MF_GRAYED`: disabled and drawn grayed.
    pub const MF_GRAYED: u32 = 0x0001;
    /// `MF_CHECKED`: shows a check mark.
    pub const MF_CHECKED: u32 = 0x0008;
    /// `MF_SEPARATOR`: a horizontal dividing line.
    pub const MF_SEPARATOR: u32 = 0x0800;
}

/// One entry of a popup menu. `Debug` shows the length of the text, never the text, which
/// may name user content.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuItem<'a> {
    /// A command: choosing it makes [`PopupMenu::track`] return `id`.
    Command {
        /// Command id, `1..=0xFFFF` (0 means "nothing chosen").
        id: u16,
        /// Text; `&` marks the mnemonic, `\t` separates a shortcut hint.
        text: &'a str,
        /// Shows a check mark.
        checked: bool,
        /// Cannot be chosen and is drawn grayed.
        disabled: bool,
    },
    /// A separator line.
    Separator,
}

impl fmt::Debug for MenuItem<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Command {
                id,
                text,
                checked,
                disabled,
            } => f
                .debug_struct("Command")
                .field("id", id)
                .field("text_len", &text.len())
                .field("checked", checked)
                .field("disabled", disabled)
                .finish(),
            Self::Separator => f.write_str("Separator"),
        }
    }
}

impl<'a> MenuItem<'a> {
    /// An enabled, unchecked command.
    pub const fn command(id: u16, text: &'a str) -> Self {
        Self::Command {
            id,
            text,
            checked: false,
            disabled: false,
        }
    }

    /// The same item with its check mark set or cleared (separators are unchanged).
    #[must_use]
    pub const fn checked(self, checked: bool) -> Self {
        match self {
            Self::Command {
                id, text, disabled, ..
            } => Self::Command {
                id,
                text,
                checked,
                disabled,
            },
            Self::Separator => Self::Separator,
        }
    }

    /// The same item disabled or enabled (separators are unchanged).
    #[must_use]
    pub const fn disabled(self, disabled: bool) -> Self {
        match self {
            Self::Command {
                id, text, checked, ..
            } => Self::Command {
                id,
                text,
                checked,
                disabled,
            },
            Self::Separator => Self::Separator,
        }
    }

    /// The `AppendMenuW` flags of the item.
    pub const fn flags(&self) -> u32 {
        match self {
            Self::Command {
                checked, disabled, ..
            } => {
                let mut bits = flags::MF_STRING;
                if *checked {
                    bits |= flags::MF_CHECKED;
                }
                if *disabled {
                    bits |= flags::MF_GRAYED;
                }
                bits
            }
            Self::Separator => flags::MF_SEPARATOR,
        }
    }

    /// Checks that a command id is not 0 (which `track` reports as "nothing chosen") and that
    /// its text contains no NUL (which would cut it short). Fails with `E_INVALIDARG`.
    pub fn validate(&self) -> WinResult<()> {
        match self {
            Self::Command { id: 0, .. } => Err(WinError::new(E_INVALIDARG, "menu command id")),
            Self::Command { text, .. } if text.contains('\0') => {
                Err(WinError::new(E_INVALIDARG, "menu item text"))
            }
            Self::Command { .. } | Self::Separator => Ok(()),
        }
    }
}

#[cfg(windows)]
pub use win::PopupMenu;

#[cfg(windows)]
mod win {
    use core::fmt;

    use windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DestroyMenu, GetMenuItemCount, GetSystemMetrics, HMENU,
        MENU_ITEM_FLAGS, PostMessageW, SM_MENUDROPALIGNMENT, SetForegroundWindow, TPM_LEFTALIGN,
        TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTALIGN, TPM_RIGHTBUTTON, TrackPopupMenuEx, WM_NULL,
    };
    use windows::core::PCWSTR;

    use super::MenuItem;
    use crate::error::{ResultExt, WinError, WinResult};
    use crate::strings::WideString;

    /// An owned popup menu, destroyed (`DestroyMenu`) on drop. It belongs to the creating
    /// thread, so it is not `Send`:
    ///
    /// ```compile_fail
    /// fn assert_send<T: Send>() {}
    /// assert_send::<raumakey_windows::menu::PopupMenu>();
    /// ```
    pub struct PopupMenu {
        handle: HMENU,
    }

    impl fmt::Debug for PopupMenu {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("PopupMenu")
                .field("handle", &self.handle.0)
                .finish()
        }
    }

    impl PopupMenu {
        /// Creates an empty popup menu (`CreatePopupMenu`).
        pub fn new() -> WinResult<Self> {
            // SAFETY: no preconditions; the handle is owned by the result.
            let handle = unsafe { CreatePopupMenu() }.context("CreatePopupMenu")?;
            Ok(Self { handle })
        }

        /// Appends `item` (`AppendMenuW`) after checking it with [`MenuItem::validate`].
        pub fn append(&mut self, item: MenuItem<'_>) -> WinResult<()> {
            item.validate()?;
            let flags = MENU_ITEM_FLAGS(item.flags());
            match item {
                MenuItem::Command { id, text, .. } => {
                    let text = WideString::new(text)?;
                    // SAFETY: a live menu; the text is NUL-terminated and outlives the call
                    // (the menu copies it).
                    unsafe { AppendMenuW(self.handle, flags, usize::from(id), text.as_pcwstr()) }
                }
                // SAFETY: a live menu; separators take no text.
                MenuItem::Separator => unsafe {
                    AppendMenuW(self.handle, flags, 0, PCWSTR::null())
                },
            }
            .context("AppendMenuW")
        }

        /// Number of items (`GetMenuItemCount`).
        pub fn item_count(&self) -> WinResult<usize> {
            // SAFETY: a live menu handle.
            let count = unsafe { GetMenuItemCount(Some(self.handle)) };
            usize::try_from(count).map_err(|_| WinError::last("GetMenuItemCount"))
        }

        /// The menu handle (valid while `self` lives).
        pub fn handle(&self) -> HMENU {
            self.handle
        }

        /// Shows the menu at `point` (screen coordinates) for `owner`, a window of the calling
        /// thread, and returns the chosen command, or `None` when the menu was dismissed or
        /// could not be shown.
        ///
        /// `owner` becomes the foreground window first and receives a `WM_NULL` afterwards
        /// (see the module documentation). The menu opens on the side given by
        /// `SM_MENUDROPALIGNMENT` and accepts either mouse button. The menu's modal loop sends
        /// messages to `owner`, so an owner on another thread that waits for the caller would
        /// deadlock.
        pub fn track(&self, owner: HWND, point: POINT) -> Option<u16> {
            // SAFETY: no memory preconditions; a failure only leaves the foreground unchanged.
            let _ = unsafe { SetForegroundWindow(owner) };
            // SAFETY: no preconditions.
            let right_aligned = unsafe { GetSystemMetrics(SM_MENUDROPALIGNMENT) } != 0;
            let alignment = if right_aligned {
                TPM_RIGHTALIGN
            } else {
                TPM_LEFTALIGN
            };
            let flags = TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON | alignment;
            // SAFETY: a live menu owned by `self` and no exclusion rectangle; USER validates
            // the owner handle, so any value is memory-safe (the same-thread rule above is
            // about deadlocks).
            let chosen =
                unsafe { TrackPopupMenuEx(self.handle, flags.0, point.x, point.y, owner, None) };
            // SAFETY: posts a message without pointer arguments.
            let _ = unsafe { PostMessageW(Some(owner), WM_NULL, WPARAM(0), LPARAM(0)) };
            // With TPM_RETURNCMD the result is the command id, or 0 for none.
            u16::try_from(chosen.0).ok().filter(|id| *id != 0)
        }
    }

    impl Drop for PopupMenu {
        fn drop(&mut self) {
            // SAFETY: the menu is owned by `self` and destroyed once.
            let _ = unsafe { DestroyMenu(self.handle) };
        }
    }
}
