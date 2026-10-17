//! The typing hook on Windows: the hook thread with the [`TypingHandler`], the Windows
//! [`TypingPlatform`], and the API the run mode calls.
//!
//! This module is thin: every decision is made by the handler, which is tested on every
//! host, and every Win32 call is a `raumakey-windows` wrapper. A successful build and its
//! read-only tests are no evidence that the hook types on Windows; the manual check list of
//! the [`typing`](super) module is.

use core::fmt;
use std::sync::Arc;

use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_model::host::ExeName;
use raumakey_session::SnapshotError;
use raumakey_windows::error::{WinError, WinResult};
use raumakey_windows::foreground::{
    current_process_is_elevated, foreground_window, process_image_basename, process_is_elevated,
};
use raumakey_windows::hooks::HookThread;
use raumakey_windows::input::{self, SendReport, SyntheticKey};
use raumakey_windows::keyboard::{self, KeyboardSnapshot, KlidSource, Translation, klid_from_hkl};

use super::handler::TypingHandler;
use super::mailbox::{Mailbox, TypingUpdate};
use super::platform::{Foreground, LayoutInfo, TypingPlatform};

/// Why the typing hook did not start.
#[derive(Debug)]
pub enum TypingError {
    /// An initial snapshot was refused; no hook was installed.
    Snapshot(SnapshotError),
    /// The hook thread or its hooks could not be started (`SetWindowsHookExW`).
    Hook(WinError),
}

impl fmt::Display for TypingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Snapshot(error) => write!(f, "the typing configuration was refused: {error}"),
            Self::Hook(error) => write!(f, "the keyboard hook could not start: {error}"),
        }
    }
}

impl std::error::Error for TypingError {}

/// The running typing hook: a thread that owns `WH_KEYBOARD_LL` and `WH_MOUSE_LL` and runs
/// a [`TypingHandler`] over the Windows platform (plan D39).
///
/// Dropping it stops the thread like [`stop`](Self::stop), ignoring the result.
#[derive(Debug)]
pub struct TypingHook {
    mailbox: Arc<Mailbox>,
    thread: HookThread,
}

impl TypingHook {
    /// Builds the handler with `runtime` and `macros` on the calling thread (the snapshots
    /// are checked and the macro table is built here; the Caps Lock toggle of the desktop
    /// and whether this process is elevated are read here), then starts the hook thread and
    /// returns once both hooks are installed.
    ///
    /// `observer` is called on the hook thread, from inside the hook callback, whenever the
    /// toggle hotkey changes the mode, with the new mode (true for Vietnamese); it must return
    /// at once (post a message to the tray window) and must not call back into this hook.
    ///
    /// # Errors
    ///
    /// [`TypingError::Snapshot`] when a snapshot is refused, before anything is installed;
    /// [`TypingError::Hook`] when the thread or a hook cannot start.
    pub fn start(
        runtime: RuntimeSnapshot,
        macros: MacroSnapshot,
        observer: impl FnMut(bool) + Send + 'static,
    ) -> Result<Self, TypingError> {
        let mailbox = Arc::new(Mailbox::new());
        mailbox
            .post(TypingUpdate::Runtime(runtime))
            .map_err(TypingError::Snapshot)?;
        mailbox
            .post(TypingUpdate::Macros(macros))
            .map_err(TypingError::Snapshot)?;
        let handler = TypingHandler::new(WinPlatform::default(), Arc::clone(&mailbox), observer);
        let thread = HookThread::spawn(Box::new(handler)).map_err(TypingError::Hook)?;
        Ok(Self { mailbox, thread })
    }

    /// Hands `update` to the hook thread, which applies it at its next key or mouse press,
    /// before handling it. Never waits for the hook thread.
    ///
    /// # Errors
    ///
    /// The [`SnapshotError`] of a refused snapshot; the hook keeps its configuration.
    pub fn update(&self, update: TypingUpdate) -> Result<(), SnapshotError> {
        self.mailbox.post(update)
    }

    /// The mailbox of the hook thread, for posting updates from another thread
    /// ([`Mailbox::post`]).
    pub fn mailbox(&self) -> Arc<Mailbox> {
        Arc::clone(&self.mailbox)
    }

    /// Uninstalls both hooks and ends the thread, waiting at most 2 s
    /// (`raumakey_windows::hooks::HookThread::stop`).
    ///
    /// # Errors
    ///
    /// `ERROR_TIMEOUT` when the thread did not end in time; it is detached then.
    pub fn stop(self) -> WinResult<()> {
        self.thread.stop()
    }
}

/// The process facts of the last foreground window, which take kernel objects to read.
#[derive(Clone, Debug)]
struct ProcessFacts {
    window: isize,
    process_id: u32,
    executable: Option<ExeName>,
    elevated: Option<bool>,
}

/// Windows for the [`TypingHandler`]. The process of the foreground window is queried again
/// only when the foreground window changes.
#[derive(Debug, Default)]
struct WinPlatform {
    process: Option<ProcessFacts>,
}

impl WinPlatform {
    fn process_facts(&mut self, window: isize, process_id: u32) -> ProcessFacts {
        match &self.process {
            Some(facts) if facts.window == window && facts.process_id == process_id => {
                facts.clone()
            }
            _ => {
                let facts = ProcessFacts {
                    window,
                    process_id,
                    executable: process_image_basename(process_id).ok(),
                    elevated: process_is_elevated(process_id).ok(),
                };
                self.process = Some(facts.clone());
                facts
            }
        }
    }
}

/// The KLID of `layout` as its handle shows it, without the registry: a layout variant (such
/// as US-International) shows the KLID of its language. Only the host context carries it.
fn layout_name(layout: isize) -> String {
    match klid_from_hkl(layout as usize) {
        KlidSource::Direct(klid) => klid,
        KlidSource::LayoutId { language, .. } => format!("0000{language:04X}"),
    }
}

impl TypingPlatform for WinPlatform {
    fn process_is_elevated(&mut self) -> bool {
        current_process_is_elevated().unwrap_or(false)
    }

    fn foreground(&mut self) -> Option<Foreground> {
        let window = foreground_window()?;
        let facts = self.process_facts(window.hwnd, window.process_id);
        Some(Foreground {
            window: window.hwnd,
            focus: window.focus_hwnd,
            process_id: window.process_id,
            thread_id: window.thread_id,
            executable: facts.executable,
            window_class: window.focus_window_class.or(window.window_class),
            secure_field: window.focus_is_password,
            elevated: facts.elevated,
        })
    }

    fn keyboard_layout(&mut self, thread_id: u32) -> isize {
        keyboard::layout_for_thread(thread_id)
    }

    fn describe_layout(&mut self, layout: isize) -> LayoutInfo {
        LayoutInfo {
            name: layout_name(layout),
            altgr: keyboard::is_altgr_layout(layout),
        }
    }

    fn translate(
        &mut self,
        virtual_key: u32,
        scan_code: u32,
        state: &KeyboardSnapshot,
        layout: isize,
    ) -> Translation {
        keyboard::translate_key(virtual_key, scan_code, state, layout)
    }

    fn key_is_down(&mut self, virtual_key: u32) -> bool {
        keyboard::key_is_down(virtual_key)
    }

    fn caps_lock_on(&mut self) -> bool {
        KeyboardSnapshot::capture().caps_lock
    }

    fn send(&mut self, keys: &[SyntheticKey]) -> SendReport {
        input::send(keys).unwrap_or(SendReport {
            requested: keys.len(),
            sent: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    //! Read-only checks of the Windows platform and the start of the hook. Nothing here
    //! installs a hook or sends input; the desktop's key state (Caps Lock may be on) is read
    //! but never relied on.

    use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
    use raumakey_model::options::InputMethod;
    use raumakey_windows::keyboard::KeyboardSnapshot;

    use super::{TypingError, TypingHook, TypingPlatform, WinPlatform, layout_name};

    #[test]
    fn a_refused_snapshot_stops_the_start_before_any_hook_is_installed() {
        let mut runtime = RuntimeSnapshot::defaults_v1();
        runtime.options.input_method = InputMethod::UserDefined;
        runtime.options.user_keymap_id = Some("missing".to_owned());
        let started = TypingHook::start(runtime, MacroSnapshot::empty(0), |_| {});
        assert!(matches!(started, Err(TypingError::Snapshot(_))));
    }

    #[test]
    fn layout_names_come_from_the_handle_alone() {
        assert_eq!(layout_name(0x0409_0409), "00000409");
        assert_eq!(layout_name(0x0407_0407), "00000407");
        assert_eq!(layout_name(0xF001_0409_u32 as i32 as isize), "00000409");
    }

    #[test]
    fn the_windows_platform_reads_without_changing_anything() {
        let mut platform = WinPlatform::default();
        let _ = platform.process_is_elevated();
        let _ = platform.caps_lock_on();
        let _ = platform.key_is_down(0x10);
        if let Some(foreground) = platform.foreground() {
            assert_ne!(foreground.window, 0);
            // The process facts are cached for the same window.
            let again = platform.foreground();
            if let Some(again) = again
                && again.window == foreground.window
            {
                assert_eq!(again.executable, foreground.executable);
            }
        }
        let layout = platform.keyboard_layout(0);
        let info = platform.describe_layout(layout);
        assert_eq!(info.name.len(), 8, "{}", info.name);
        let space = platform.translate(0x20, 0x39, &KeyboardSnapshot::default(), layout);
        assert_eq!(space.units, [0x20]);
        let report = platform.send(&[]);
        assert_eq!((report.requested, report.sent), (0, 0), "nothing was sent");
    }
}
