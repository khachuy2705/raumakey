//! The run mode: `raumakey.exe` without arguments (plan §10.2, D38–D45; design §21, §22). This
//! is what the user starts by double-clicking the portable `raumakey.exe`, and what the
//! Startup-folder shortcut and the elevated logon task start.
//!
//! [`run_controller`] is the whole sequence, over the [`Desktop`] port so that it is tested
//! with a fake; `crate::desktop` holds the Windows desktop.
//!
//! 1. **Single instance.** The controller takes its named mutex
//!    (`Local\Raumakey.<sidhash>.controller`: one controller per user and session). When
//!    another controller holds it, this start asks that controller to show its settings panel
//!    ([`Desktop::signal_running_instance`], D45: it may come to the front, and the registered
//!    message `Raumakey.<sidhash>.show-panel` goes to every top-level window), then ends at
//!    once with exit code 0 whatever the request did; it says nothing and opens no file. A
//!    mutex that this process may not open (access denied: an elevated controller of the same
//!    user holds it, which lets the request through UIPI) counts as held by another controller
//!    as well.
//! 2. **Settings.** [`Desktop::open_settings`] opens the configuration beside `raumakey.exe`
//!    or in `%LOCALAPPDATA%\Raumakey` (`crate::settings`, D40); on the first run it writes
//!    `raumakey.toml` with the defaults. Opening never fails: whatever cannot be read leaves
//!    its defaults in effect and is reported.
//! 3. **Typing.** [`Desktop::start_typing`] starts the keyboard hook (`crate::typing`, D39)
//!    with the settings' runtime and macro snapshots. A hook that cannot start ends the start:
//!    without it Raumakey cannot type ([`typing_error_text`], [`EXIT_FAILED`]).
//! 4. **Start with Windows (D41).** When `startup = "shortcut"` is saved, the shortcut
//!    `Raumakey.lnk` in the user's Startup folder is created again when it is missing or
//!    starts an exe that no longer exists (this exe was moved) ([`needs_repair`],
//!    [`shortcut_state`]). A shortcut of another copy that still exists is never re-pointed,
//!    one that cannot be read is left alone, an exe in the temporary folder repairs nothing,
//!    and `elevated-task` never repairs a shortcut. A repair that fails is told by the tray
//!    ([`heal_notice`](crate::notice::heal_notice)); the controller runs on.
//! 5. **Tray.** [`Desktop::run_tray`] runs the tray in the saved mode ([`TrayStart`]) until
//!    Exit, `WM_CLOSE`, or the end of the session (`crate::tray`): the hidden window, its
//!    notification icon, the poll timer, the settings worker, and the message loop. Once the
//!    icon is added, the settings panel opens when `[system] show_panel_on_start` is set (the
//!    schema default, so the first run always shows it, D43), then the problems found at the
//!    start ([`start_notice`](crate::notice::start_notice)) are shown once; the welcome of
//!    the first run only without the panel, which shows the same. Its exit code
//!    ([`LOOP_EXIT_CODE`](crate::tray::LOOP_EXIT_CODE), 0) becomes the exit code of the
//!    process; a code outside `0..=255` becomes [`EXIT_FAILED`].
//! 6. **Stop.** [`Desktop::stop_typing`] runs on every way out of the tray (the tray stops the
//!    hook itself on Exit and at the end of the session; this covers a tray that failed or a
//!    window that was lost), then the mutex is given back.
//! 7. **Errors.** The controller has no console: an error is shown to the user
//!    ([`Desktop::report_error`], a message box naming the failed operation and its code,
//!    [`start_error_text`], [`typing_error_text`]) after the mutex was given back, and the
//!    process exits with [`EXIT_FAILED`].
//!
//! Not in this step (design §21.6): the tabbed settings of §11 (the one-page panel of D43
//! replaces them), the IPC hub (there is no TIP to serve), and the `migrate` import.

use std::path::Path;

use raumakey_config::StartupMode;
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::host::ExeName;
use raumakey_windows::error::{WinError, WinErrorKind, WinResult};

use crate::cli::{EXIT_FAILED, EXIT_OK};
use crate::notice::{heal_notice, start_notice, win_error_text};
use crate::settings::OpenReport;
use crate::tray::TrayStart;

/// What [`Desktop::open_settings`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Opened {
    /// The open's report: where, the first run, and the state of every file.
    pub report: OpenReport,
    /// The runtime snapshot in effect (the hook starts with it, the tray shows it).
    pub runtime: RuntimeSnapshot,
    /// `[system] startup` in effect.
    pub startup: StartupMode,
    /// The running `raumakey.exe` lies in the temporary folder
    /// ([`is_temporary_path`](crate::settings::is_temporary_path)): no shortcut is made to it.
    pub temporary_exe: bool,
    /// `[system] show_panel_on_start` in effect: the tray opens the settings panel at start.
    pub show_panel_on_start: bool,
    /// `[exclusions] apps` in effect (`Settings::excluded_apps`), which the exclusions dialog
    /// shows (D48).
    pub exclusions: Vec<ExeName>,
}

/// The shortcut `Raumakey.lnk` in the user's Startup folder ([`shortcut_state`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShortcutState {
    /// There is none.
    Missing,
    /// It starts this `raumakey.exe` without arguments.
    PointsHere,
    /// It starts an exe that no longer exists: a copy that was moved or deleted.
    PointsToMissing,
    /// It starts another copy of Raumakey that still exists (or passes arguments): it is that
    /// copy's, and is never re-pointed.
    PointsToOther,
}

/// The state of a shortcut whose target is `target` (which exists or not) with `arguments`,
/// seen from the `raumakey.exe` at `exe`.
pub fn shortcut_state(
    target: &Path,
    arguments: &str,
    exe: &Path,
    target_exists: bool,
) -> ShortcutState {
    if shortcut_points_to(target, arguments, exe) {
        ShortcutState::PointsHere
    } else if target_exists {
        ShortcutState::PointsToOther
    } else {
        ShortcutState::PointsToMissing
    }
}

/// Why the typing hook did not start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypingStartError {
    /// The session core refused the settings' snapshots (the settings validate them, so this
    /// is not expected).
    Refused,
    /// The hook thread or its hooks could not be installed.
    Hook(WinError),
}

/// The desktop side effects of the run mode.
pub trait Desktop {
    /// Takes the controller's single-instance mutex: `Ok(true)` when this process holds it
    /// now (until [`release_instance`](Self::release_instance)), `Ok(false)` when another
    /// controller of this user and session holds it.
    fn acquire_instance(&mut self) -> WinResult<bool>;

    /// Opens the configuration of the running `raumakey.exe` and keeps it for the tray's
    /// settings worker. Never fails.
    fn open_settings(&mut self) -> Opened;

    /// Starts the typing hook with the snapshots of the opened settings.
    fn start_typing(&mut self) -> Result<(), TypingStartError>;

    /// What the shortcut in the user's Startup folder is.
    fn startup_shortcut(&mut self) -> WinResult<ShortcutState>;

    /// Creates (or replaces) the shortcut in the user's Startup folder, starting the running
    /// `raumakey.exe` without arguments.
    fn create_startup_shortcut(&mut self) -> WinResult<()>;

    /// Runs the tray from `start` (the hidden window, its notification icon, the settings
    /// worker, and the message loop) until it ends; returns the exit code of the message
    /// loop. The tray owns the typing hook while it runs.
    fn run_tray(&mut self, start: TrayStart) -> WinResult<i32>;

    /// Stops the typing hook if it still runs.
    fn stop_typing(&mut self);

    /// Gives the single-instance mutex back.
    fn release_instance(&mut self);

    /// Shows `text` to the user and waits until it is acknowledged.
    fn report_error(&mut self, text: &str);

    /// Asks the controller that holds the instance to show its settings panel (D45), without
    /// waiting and without telling whether anything answered: a start that is not the
    /// instance ends with exit code 0 either way.
    fn signal_running_instance(&mut self);
}

/// Runs the controller on `desktop` (see the module documentation) and returns the exit code
/// of the process.
pub fn run_controller(desktop: &mut dyn Desktop) -> u8 {
    match desktop.acquire_instance() {
        Ok(true) => {}
        Ok(false) => {
            desktop.signal_running_instance();
            return EXIT_OK;
        }
        Err(error) if error.kind() == WinErrorKind::AccessDenied => {
            desktop.signal_running_instance();
            return EXIT_OK;
        }
        Err(error) => {
            desktop.report_error(&start_error_text(&error));
            return EXIT_FAILED;
        }
    }
    let ran = run_instance(desktop);
    desktop.release_instance();
    match ran {
        Ok(code) => u8::try_from(code).unwrap_or(EXIT_FAILED),
        Err(text) => {
            desktop.report_error(&text);
            EXIT_FAILED
        }
    }
}

/// Steps 2 to 6, while the instance is held; an error is the text to show.
fn run_instance(desktop: &mut dyn Desktop) -> Result<i32, String> {
    let opened = desktop.open_settings();
    if let Err(error) = desktop.start_typing() {
        return Err(typing_error_text(&error));
    }
    // The panel opened at the start says what the welcome would.
    let welcome = !opened.show_panel_on_start;
    let mut notices: Vec<_> = start_notice(&opened.report, welcome).into_iter().collect();
    if opened.startup == StartupMode::Shortcut && !opened.temporary_exe {
        let shortcut = desktop.startup_shortcut();
        if needs_repair(opened.startup, opened.temporary_exe, &shortcut)
            && let Err(error) = desktop.create_startup_shortcut()
        {
            notices.push(heal_notice(&error));
        }
    }
    let Opened {
        report,
        runtime,
        startup,
        temporary_exe,
        show_panel_on_start,
        exclusions,
    } = opened;
    let start = TrayStart {
        runtime,
        startup,
        location: report.location,
        config_path: report.config_path,
        status: report.status,
        notices,
        temporary_exe,
        show_panel: show_panel_on_start,
        exclusions,
    };
    let ran = desktop.run_tray(start);
    desktop.stop_typing();
    ran.map_err(|error| start_error_text(&error))
}

/// Whether the start must create the Startup shortcut again (D41): `shortcut` is saved, the
/// exe is not in the temporary folder, and the shortcut is missing or starts an exe that no
/// longer exists (this exe was moved). A shortcut of another copy that still exists, or one
/// that cannot be read, is left alone; `elevated-task` never repairs a shortcut.
pub fn needs_repair(
    startup: StartupMode,
    temporary_exe: bool,
    shortcut: &WinResult<ShortcutState>,
) -> bool {
    startup == StartupMode::Shortcut
        && !temporary_exe
        && matches!(
            shortcut,
            Ok(ShortcutState::Missing | ShortcutState::PointsToMissing)
        )
}

/// Whether a shortcut with `target` and `arguments` starts `exe` without arguments. Paths are
/// compared the way Windows compares them: ASCII case and `/` against `\` do not matter.
pub fn shortcut_points_to(target: &Path, arguments: &str, exe: &Path) -> bool {
    let normalize = |path: &Path| {
        path.to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase()
    };
    arguments.trim().is_empty()
        && !target.as_os_str().is_empty()
        && normalize(target) == normalize(exe)
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

/// The message shown when the typing hook cannot start: Raumakey cannot type.
pub fn typing_error_text(error: &TypingStartError) -> String {
    match error {
        TypingStartError::Refused => {
            "Không khởi động được bộ gõ của Raumakey: cấu hình gõ bị từ chối.".to_owned()
        }
        TypingStartError::Hook(error) => format!(
            "Không khởi động được bộ gõ của Raumakey: Windows không cho cài hook bàn phím.\n\n\
             Chi tiết: {}.",
            win_error_text(error)
        ),
    }
}
