//! Message boxes for the controller's error reports and simple yes/no questions (design §20.6
//! item 2: the controller is a GUI-subsystem program that attaches to a console only for its
//! subcommands, so the running controller reports errors in a box; plan §10.2).
//!
//! [`MessageKind`] (buttons and icon), [`MessageResponse`] (the answer), and
//! [`message_box_texts`] (the checked strings) are pure. On Windows, [`message_box`] shows a
//! modal `MessageBoxW` and waits for the answer.
//!
//! The text and caption are shown to the user and never logged. Called from a window handler,
//! the box runs a modal loop: messages for that window that arrive meanwhile get the default
//! processing (see [`crate::window`]).

use crate::error::WinResult;
use crate::strings::WideString;

/// `MessageBoxW` style bits used by [`MessageKind::style`].
pub mod styles {
    /// `MB_OK`: one OK button.
    pub const MB_OK: u32 = 0x0000_0000;
    /// `MB_YESNO`: Yes and No buttons.
    pub const MB_YESNO: u32 = 0x0000_0004;
    /// `MB_ICONERROR`: the error icon.
    pub const MB_ICONERROR: u32 = 0x0000_0010;
    /// `MB_ICONQUESTION`: the question icon.
    pub const MB_ICONQUESTION: u32 = 0x0000_0020;
    /// `MB_ICONWARNING`: the warning icon.
    pub const MB_ICONWARNING: u32 = 0x0000_0030;
    /// `MB_ICONINFORMATION`: the information icon.
    pub const MB_ICONINFORMATION: u32 = 0x0000_0040;
    /// `MB_SETFOREGROUND`: the box becomes the foreground window.
    pub const MB_SETFOREGROUND: u32 = 0x0001_0000;
}

/// What a message box says and which buttons it offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageKind {
    /// Information, with an OK button.
    Information,
    /// A warning, with an OK button.
    Warning,
    /// An error report, with an OK button.
    Error,
    /// A yes/no question.
    Question,
}

impl MessageKind {
    /// The `MessageBoxW` style: buttons and icon, plus `MB_SETFOREGROUND`, because the
    /// controller runs in the background and its box must not open behind the active window.
    pub const fn style(self) -> u32 {
        let buttons_and_icon = match self {
            Self::Information => styles::MB_OK | styles::MB_ICONINFORMATION,
            Self::Warning => styles::MB_OK | styles::MB_ICONWARNING,
            Self::Error => styles::MB_OK | styles::MB_ICONERROR,
            Self::Question => styles::MB_YESNO | styles::MB_ICONQUESTION,
        };
        buttons_and_icon | styles::MB_SETFOREGROUND
    }
}

/// The button that closed a message box. Boxes with only an OK button also answer `Ok` when
/// closed with Esc or the close button; a yes/no question cannot be closed otherwise.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MessageResponse {
    /// OK (`IDOK`).
    Ok,
    /// Yes (`IDYES`).
    Yes,
    /// No (`IDNO`).
    No,
}

impl MessageResponse {
    /// Decodes a `MessageBoxW` result; `None` for the buttons [`MessageKind`] never shows and
    /// for 0 (failure).
    pub const fn from_id(id: i32) -> Option<Self> {
        match id {
            1 => Some(Self::Ok),
            6 => Some(Self::Yes),
            7 => Some(Self::No),
            _ => None,
        }
    }
}

/// The text and caption of a message box as NUL-terminated UTF-16 strings. Fails with
/// `E_INVALIDARG` when either contains a NUL, which would cut it short. [`message_box`] checks
/// its strings with this before it shows anything.
pub fn message_box_texts(text: &str, caption: &str) -> WinResult<(WideString, WideString)> {
    Ok((WideString::new(text)?, WideString::new(caption)?))
}

/// Shows a modal message box owned by `owner` and returns the answer (`MessageBoxW`). Fails
/// with `E_INVALIDARG` when `text` or `caption` contains a NUL, before anything is shown
/// ([`message_box_texts`]).
///
/// `owner` must be `None` or a window of the calling thread: the box disables its owner while
/// it is shown, which waits for the owner's thread to answer, so an owner whose thread waits
/// for the caller would deadlock.
#[cfg(windows)]
pub fn message_box(
    owner: Option<windows::Win32::Foundation::HWND>,
    text: &str,
    caption: &str,
    kind: MessageKind,
) -> WinResult<MessageResponse> {
    use windows::Win32::UI::WindowsAndMessaging::{MESSAGEBOX_STYLE, MessageBoxW};

    use crate::error::{E_UNEXPECTED, WinError};

    let (text, caption) = message_box_texts(text, caption)?;
    // SAFETY: both strings are NUL-terminated and outlive the call; USER validates the owner
    // handle, so any value is memory-safe (the thread rule above is about deadlocks).
    let answer = unsafe {
        MessageBoxW(
            owner,
            text.as_pcwstr(),
            caption.as_pcwstr(),
            MESSAGEBOX_STYLE(kind.style()),
        )
    };
    if answer.0 == 0 {
        return Err(WinError::last("MessageBoxW"));
    }
    MessageResponse::from_id(answer.0).ok_or(WinError::new(E_UNEXPECTED, "MessageBoxW answer"))
}
