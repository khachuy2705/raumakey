//! The run mode: `raumakey.exe` without arguments (plan §10.2; design §11 and §13). This is
//! what the Startup-folder shortcut and the elevated logon task start.
//!
//! [`run_controller`] is the whole sequence, over the [`Desktop`] port so that it is tested
//! with a fake; `crate::desktop` holds the Windows desktop.
//!
//! 1. **Single instance.** The controller takes its named mutex
//!    (`Local\Raumakey.<sidhash>.controller`: one controller per user and session). When
//!    another controller holds it, this start ends at once with exit code 0, says nothing,
//!    and does not signal the first instance. A mutex that this process may not open (access
//!    denied: an elevated controller of the same user holds it) counts as held by another
//!    controller as well.
//! 2. **Tray.** While the mutex is held, the tray runs in [`initial_mode`]: the hidden window,
//!    its notification icon, and the message loop (`crate::tray`), until Exit, `WM_CLOSE`, or
//!    the end of the session. Its exit code ([`LOOP_EXIT_CODE`](crate::tray::LOOP_EXIT_CODE),
//!    0) becomes the exit code of the process; a code outside `0..=255` becomes
//!    [`EXIT_FAILED`].
//! 3. **Errors.** The controller has no console: an error is shown to the user
//!    ([`Desktop::report_error`], a message box naming the failed operation and its code,
//!    [`start_error_text`]) after the mutex was given back, and the process exits with
//!    [`EXIT_FAILED`].
//!
//! Not in this step (plan §14.5 step 5): the configuration lifecycle (the mode is not read
//! from or saved to `config.toml`), the IPC server hub and its snapshots (the TIP does not
//! follow the controller's mode yet), the worker thread, the hook thread (D24 is an open
//! question, design §20.8 item 2), the fallback transport, and the panel with its tabs.

use raumakey_windows::error::{WinError, WinErrorKind, WinResult};

use crate::cli::{EXIT_FAILED, EXIT_OK};
use crate::tray::{InputMode, initial_mode};

/// The desktop side effects of the run mode.
pub trait Desktop {
    /// Takes the controller's single-instance mutex: `Ok(true)` when this process holds it
    /// now (until [`release_instance`](Self::release_instance)), `Ok(false)` when another
    /// controller of this user and session holds it.
    fn acquire_instance(&mut self) -> WinResult<bool>;

    /// Runs the tray in `mode` (the hidden window, its notification icon, and the message
    /// loop) until it ends; returns the exit code of the message loop.
    fn run_tray(&mut self, mode: InputMode) -> WinResult<i32>;

    /// Gives the single-instance mutex back.
    fn release_instance(&mut self);

    /// Shows `text` to the user and waits until it is acknowledged.
    fn report_error(&mut self, text: &str);
}

/// Runs the controller on `desktop` (see the module documentation) and returns the exit code
/// of the process.
pub fn run_controller(desktop: &mut dyn Desktop) -> u8 {
    match desktop.acquire_instance() {
        Ok(true) => {}
        Ok(false) => return EXIT_OK,
        Err(error) if error.kind() == WinErrorKind::AccessDenied => return EXIT_OK,
        Err(error) => {
            desktop.report_error(&start_error_text(&error));
            return EXIT_FAILED;
        }
    }
    let ran = desktop.run_tray(initial_mode());
    desktop.release_instance();
    match ran {
        Ok(code) => u8::try_from(code).unwrap_or(EXIT_FAILED),
        Err(error) => {
            desktop.report_error(&start_error_text(&error));
            EXIT_FAILED
        }
    }
}

/// The message shown when the controller cannot start: "Raumakey could not start", then the
/// HRESULT and the static name of the failed operation. It holds nothing else.
pub fn start_error_text(error: &WinError) -> String {
    format!(
        "Không khởi động được Raumakey.\n\nLỗi 0x{:08X} tại {}.",
        error.code() as u32,
        error.context()
    )
}
