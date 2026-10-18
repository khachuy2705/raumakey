//! Message boxes for the controller's error reports and simple yes/no questions (design §20.6
//! item 2: the controller is a GUI-subsystem program that attaches to a console only for its
//! subcommands, so the running controller reports errors in a box; plan §10.2).
//!
//! [`MessageKind`] (buttons and icon), [`MessageResponse`] (the answer), and
//! [`message_box_texts`] (the checked strings) are pure. On Windows, [`message_box`] shows a
//! modal `MessageBoxW` and waits for the answer. Every box gets its text wrapped at spaces
//! first ([`crate::word_wrap`]), so that Windows never cuts a Vietnamese word.
//!
//! The text and caption are shown to the user and never logged. Called from a window handler,
//! the box runs a modal loop: messages for that window that arrive meanwhile get the default
//! processing (see [`crate::window`]). A window thread that must go on handling its messages
//! while a box is open shows it with `spawn_message_box` instead: the box runs on a thread of
//! its own, its answer goes to a callback, and `DetachedMessageBox::dismiss` closes it
//! without waiting (`dismiss_message_boxes` posts the button of
//! [`MessageKind::dismiss_answer`] to the box).

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

    /// The button that closes a box of this kind without an answer of the user: No for a
    /// question (it has no Cancel button, so `WM_CLOSE` and Esc do not close it), OK otherwise.
    pub const fn dismiss_answer(self) -> MessageResponse {
        match self {
            Self::Question => MessageResponse::No,
            Self::Information | Self::Warning | Self::Error => MessageResponse::Ok,
        }
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

    /// The button's id (`IDOK`, `IDYES`, `IDNO`), as `MessageBoxW` returns it.
    pub const fn id(self) -> i32 {
        match self {
            Self::Ok => 1,
            Self::Yes => 6,
            Self::No => 7,
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
/// The text is wrapped at spaces first
/// ([`wrap_for_message_box`](crate::word_wrap::wrap_for_message_box)) to lines narrow enough
/// that the box keeps each as it is: `MessageBoxW` itself would cut a long Vietnamese line
/// inside a word.
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
    use crate::word_wrap::wrap_for_message_box;

    // A text with a NUL fails before anything is measured.
    message_box_texts(text, caption)?;
    let (text, caption) = message_box_texts(&wrap_for_message_box(text), caption)?;
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

#[cfg(windows)]
pub use detached::{DetachedMessageBox, dismiss_message_boxes, spawn_message_box};

#[cfg(windows)]
mod detached {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumThreadWindows, GetClassNameW, PostMessageW, WM_COMMAND,
    };
    use windows::core::BOOL;

    use super::{MessageKind, MessageResponse, message_box, message_box_texts};
    use crate::error::{WinError, WinResult};
    use crate::panic::{guard_bool, guard_value};

    /// The window class of dialog boxes, which message boxes are.
    const DIALOG_CLASS: &str = "#32770";

    /// Name of the threads that show detached boxes.
    const THREAD_NAME: &str = "raumakey-message-box";

    /// What the box's thread and its owner share.
    #[derive(Debug)]
    struct Shared {
        kind: MessageKind,
        /// The id of the box's thread, once it runs (0 before).
        thread_id: AtomicU32,
        /// The owner dismissed it: a box not shown yet is never shown.
        dismissed: AtomicBool,
        /// The box was closed (or never shown).
        closed: AtomicBool,
    }

    /// A message box shown on a thread of its own ([`spawn_message_box`]).
    ///
    /// Dropping it leaves the box open; [`dismiss`](Self::dismiss) closes it.
    #[derive(Debug)]
    pub struct DetachedMessageBox {
        shared: Arc<Shared>,
    }

    impl DetachedMessageBox {
        /// True once the box was closed, by the user or by [`dismiss`](Self::dismiss), or
        /// when it could not be shown.
        pub fn is_closed(&self) -> bool {
            self.shared.closed.load(Ordering::Acquire)
        }

        /// Closes the box without waiting: posts the button of
        /// [`MessageKind::dismiss_answer`] to it ([`dismiss_message_boxes`]). A box whose
        /// thread has not shown it yet is not shown at all. A box that its thread is creating
        /// at this very moment may stay open; it closes with the process.
        pub fn dismiss(&self) {
            self.shared.dismissed.store(true, Ordering::Release);
            let thread_id = self.shared.thread_id.load(Ordering::Acquire);
            if thread_id != 0 && !self.is_closed() {
                dismiss_message_boxes(thread_id, self.shared.kind.dismiss_answer());
            }
        }
    }

    /// Shows a message box without an owner on a new thread and returns at once; when the box
    /// closes, `on_close` gets the answer on that thread (`None` when it was dismissed before
    /// it was shown, or could not be shown). Fails with `E_INVALIDARG`, before any thread
    /// starts, when `text` or `caption` contains a NUL, and fails when the thread cannot
    /// start.
    pub fn spawn_message_box(
        text: &str,
        caption: &str,
        kind: MessageKind,
        on_close: impl FnOnce(Option<MessageResponse>) + Send + 'static,
    ) -> WinResult<DetachedMessageBox> {
        message_box_texts(text, caption)?;
        let shared = Arc::new(Shared {
            kind,
            thread_id: AtomicU32::new(0),
            dismissed: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        });
        let text = text.to_owned();
        let caption = caption.to_owned();
        let on_thread = Arc::clone(&shared);
        std::thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                // SAFETY: no preconditions.
                let thread_id = unsafe { GetCurrentThreadId() };
                on_thread.thread_id.store(thread_id, Ordering::Release);
                let answer = if on_thread.dismissed.load(Ordering::Acquire) {
                    None
                } else {
                    guard_value(None, || message_box(None, &text, &caption, kind).ok())
                };
                on_thread.closed.store(true, Ordering::Release);
                guard_value((), || on_close(answer));
            })
            .map_err(|error| WinError::from_io(&error, "message box thread"))?;
        Ok(DetachedMessageBox { shared })
    }

    /// What the enumeration of [`dismiss_message_boxes`] carries.
    struct Dismissal {
        command: usize,
        posted: usize,
    }

    /// Posts the button `answer` (`WM_COMMAND`) to every dialog window (class `#32770`, as a
    /// message box is) of thread `thread_id`, and returns how many it posted to. Never waits.
    pub fn dismiss_message_boxes(thread_id: u32, answer: MessageResponse) -> usize {
        let mut dismissal = Dismissal {
            command: answer.id() as usize,
            posted: 0,
        };
        // SAFETY: the callback runs during the call only and receives a pointer to
        // `dismissal`, which outlives the call; it is never kept.
        let _ = unsafe {
            EnumThreadWindows(
                thread_id,
                Some(dismiss_window),
                LPARAM(&mut dismissal as *mut Dismissal as isize),
            )
        };
        dismissal.posted
    }

    /// The callback of [`dismiss_message_boxes`]: posts the command to a dialog window.
    unsafe extern "system" fn dismiss_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
        guard_bool(|| {
            // SAFETY: `lparam` is the `Dismissal` of the enumeration that calls this.
            let dismissal = unsafe { &mut *(lparam.0 as *mut Dismissal) };
            let mut class = [0_u16; 16];
            // SAFETY: the buffer is writable for its whole length; USER validates `hwnd`.
            let len = unsafe { GetClassNameW(hwnd, &mut class) };
            let len = usize::try_from(len).unwrap_or(0).min(class.len());
            if String::from_utf16_lossy(&class[..len]) == DIALOG_CLASS {
                // SAFETY: posts plain values (the button id, no pointers) to a window of this
                // process.
                let posted = unsafe {
                    PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(dismissal.command), LPARAM(0))
                };
                if posted.is_ok() {
                    dismissal.posted += 1;
                }
            }
            true
        })
    }
}
