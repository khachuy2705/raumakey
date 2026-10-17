//! What the typing front end needs from Windows, as a port.

use raumakey_model::host::ExeName;
use raumakey_windows::input::{SendReport, SyntheticKey};
use raumakey_windows::keyboard::{KeyboardSnapshot, Translation};

/// What the hook reads about the foreground window for a key: the document identity (the
/// foreground window and the window with keyboard focus in its thread) and the facts of the
/// host context.
///
/// It holds no window title and no path, only the executable basename and a window class,
/// which the design allows in diagnostics; `Debug` shows them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Foreground {
    /// The foreground window handle value.
    pub window: isize,
    /// The window with keyboard focus in the foreground thread, 0 when there is none.
    pub focus: isize,
    /// The process of the foreground window.
    pub process_id: u32,
    /// The thread of the foreground window, whose keyboard layout translates the keys.
    pub thread_id: u32,
    /// Lowercase executable basename of the process; `None` when it cannot be read.
    pub executable: Option<ExeName>,
    /// Class of the focused window, or of the foreground window when nothing has the focus.
    pub window_class: Option<String>,
    /// The focused control is an Edit control with `ES_PASSWORD` (D25: the only secure
    /// field a hook can recognize; password fields of browsers and other frameworks are not).
    pub secure_field: bool,
    /// Whether the process is elevated; `None` when its token cannot be read, which is
    /// typical of an elevated process seen from a process that is not.
    pub elevated: Option<bool>,
}

/// A keyboard layout as the hook uses it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LayoutInfo {
    /// Its KLID, such as `00000409`, for the host context of the keys.
    pub name: String,
    /// The layout has AltGr (`raumakey_windows::keyboard::is_altgr_layout`).
    pub altgr: bool,
}

/// Windows, as the [`TypingHandler`](super::TypingHandler) uses it.
///
/// The Windows implementation is a thin layer over `raumakey-windows`; tests use a fake.
/// Every method is called on the hook thread from inside the hook callback, so each must
/// return quickly and must not block (no file I/O, no lock another thread holds, no IPC),
/// and must not call back into the handler.
pub trait TypingPlatform {
    /// Whether this process runs elevated. Asked once, when the handler is built.
    fn process_is_elevated(&mut self) -> bool;

    /// The foreground window, its focus, and its process; `None` when there is no
    /// foreground window. Asked for every key that Raumakey did not inject.
    fn foreground(&mut self) -> Option<Foreground>;

    /// The keyboard layout (`HKL` value) of thread `thread_id`.
    fn keyboard_layout(&mut self, thread_id: u32) -> isize;

    /// Describes `layout`; asked once for each layout change, not for every key.
    fn describe_layout(&mut self, layout: isize) -> LayoutInfo;

    /// The text `virtual_key` and `scan_code` type on `layout` with the key state `state`
    /// (the tracker's), without changing any keyboard state (`ToUnicodeEx` with the flag that
    /// leaves dead keys alone).
    fn translate(
        &mut self,
        virtual_key: u32,
        scan_code: u32,
        state: &KeyboardSnapshot,
        layout: isize,
    ) -> Translation;

    /// Whether `virtual_key` is physically down now (`GetAsyncKeyState`). Used only to forget
    /// keys whose release the hook missed.
    fn key_is_down(&mut self, virtual_key: u32) -> bool;

    /// Whether Caps Lock is on, as the desktop reports it. Asked when the handler is built
    /// and when the hook may have missed Caps Lock presses.
    fn caps_lock_on(&mut self) -> bool;

    /// Injects `keys` (`SendInput`) and reports how many the system accepted: all of them,
    /// some, or none (a failure is none).
    fn send(&mut self, keys: &[SyntheticKey]) -> SendReport;
}
