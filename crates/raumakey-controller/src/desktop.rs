//! The run mode on Windows: the [`Desktop`] of [`crate::run`] and the glue that carries out
//! the [`Effect`]s of the tray state machine ([`crate::tray`]) on the real notification icon,
//! popup menu, message boxes, typing hook, and settings worker (plan §10.2, D38–D42; design
//! §21; matrix #58).
//!
//! This module is thin: every decision is made in `crate::run`, `crate::tray`,
//! `crate::worker`, and `crate::notice`, which are tested on every host, and every Win32 call
//! is a `raumakey-windows` wrapper.
//!
//! - [`WinDesktop`] holds the single-instance mutex (`raumakey_windows::single_instance`,
//!   component [`CONTROLLER_INSTANCE`]), opens the settings
//!   (`Settings::open_for_current_exe`), starts the typing hook ([`TypingHook`]) with their
//!   snapshots and the hook observer, reads and repairs the Startup-folder shortcut, runs
//!   [`run_tray`], and shows errors in a message box.
//! - [`run_tray`] loads the V and E icons from `raumakey.exe` itself at the small icon size
//!   ([`ModeIcons`]), starts the settings worker thread ([`WorkerThread`], which now owns the
//!   settings), registers the window class, creates the tray window ([`create_tray_window`]),
//!   and runs the message loop on the calling thread. The window is destroyed after the loop
//!   has ended.
//! - [`create_tray_window`] creates a hidden top-level window: a message-only window would
//!   never receive the `TaskbarCreated` broadcast. When the controller runs elevated (the
//!   elevated logon task, or "Run as administrator"), it lets the shell's two messages,
//!   `TaskbarCreated` and the icon's callback, through User Interface Privilege Isolation
//!   (`ChangeWindowMessageFilterEx`, through `Window::allow_message_from_lower_integrity`).
//!   It starts the poll timer ([`POLL_TIMER`], every [`POLL_INTERVAL_MS`]) and posts
//!   [`START`].
//! - [`TrayWindow`] is the window's handler: it passes each message to [`Tray::on_message`],
//!   carries out the effects on a [`TrayShell`] (icon, menu, notices) and on
//!   [`TrayServices`] (hook and worker), and then drains the inbox ([`Tray::on_event`]), the
//!   hotkey changes coalesced ([`coalesce`]). At each tick of the poll timer it first checks
//!   the typing hook ([`TrayServices::keep_typing_alive`]), which Windows may have removed
//!   silently, and makes sure the icon is shown. [`ShellTray`] (over a [`NotifyArea`],
//!   [`WinNotifyArea`] for real) and [`WinServices`] are the real ones; tests use fakes on a
//!   real window, message loop, and timer.
//!
//! # Threads and messages
//!
//! | Thread | Owns | Talks to the tray by |
//! |---|---|---|
//! | main (tray) | the window, the icon, the menu, the [`Tray`] | — |
//! | hook (`raumakey_windows::hooks`) | `WH_KEYBOARD_LL`, `WH_MOUSE_LL`, the session core | the observer: [`Event::Hotkey`] into the inbox, then [`WAKE`] |
//! | settings worker ([`crate::worker`]) | `Settings`, the files, the shortcut, Notepad | [`Event::Worker`] answers into the inbox, then [`WAKE`] |
//! | one per notice (`raumakey_windows::dialog::spawn_message_box`) | a message box without owner | a "yes" as [`Event::Answered`] into the inbox, then [`WAKE`] |
//!
//! The tray thread sends the worker numbered commands (`WorkerThread::send`) and hands the
//! hook modes and runtime snapshots (`TypingHook::update`); the worker hands the hook macro
//! snapshots. The window handler does no file, network, or engine work: `Settings::apply`,
//! `poll`, and `recover` may wait up to two seconds for a store lock. The observer runs
//! inside the hook callback and only sends into a channel and posts a message.
//!
//! A posted message that arrives while the menu's modal loop runs inside the handler gets the
//! default processing and is lost (`raumakey_windows::window`), so the events travel in the
//! inbox, a channel that the handler drains after every message it handles: a lost [`WAKE`]
//! is made up by the next message, at the latest the next tick of the poll timer. A lost
//! tick only delays a poll. Message boxes run no modal loop on the tray's thread: each runs
//! on a thread of its own, without an owner window (so it has its own taskbar button and
//! cannot get lost behind other windows), and the tray keeps handling `WM_CLOSE`,
//! `WM_ENDSESSION`, `TaskbarCreated`, clicks, and the timer while it is open.
//!
//! # Exit and failures
//!
//! Exit, `WM_CLOSE`, and the end of the session stop the hook first (typing is plain at
//! once), close the open message boxes without waiting for them, save the hotkey changes
//! left in the inbox, let the worker finish the commands sent and save what is unsaved (at
//! most [`WORKER_FINISH_TIMEOUT`], so a change made just before is saved), remove the icon,
//! and end the loop with 0, all inside the message: after `WM_ENDSESSION` the process may end
//! at any time. Any other way out stops the hook too: when the window is destroyed while the
//! menu is open (a `WM_CLOSE` reaches `DefWindowProcW` during its modal loop), the handler
//! takes the same steps when it is freed; when the tray cannot start, the run mode stops the
//! hook. A box that is still open when the process ends goes with it. A panic in the handler
//! is contained by the window procedure's guard (`raumakey_windows::window`); one in the
//! observer by the hook handler's guard; one in a worker command by the worker; one in a box's
//! callback on its thread.
//!
//! If the icon cannot be added (Explorer has not created the taskbar yet, as may happen to the
//! elevated logon task), or a change of it fails (Explorer went away without
//! `TaskbarCreated`), the controller keeps running and adds the icon again at the next tick
//! of the poll timer, or when `TaskbarCreated` arrives. If `TaskbarCreated` itself cannot be
//! registered (`RegisterWindowMessageW` fails), the controller runs without it. A
//! `TaskbarCreated` without an Explorer restart finds the icon in place: the icon is updated.
//! If the poll timer cannot be started, the controller runs without reloading hand edits or
//! retrying the icon.
//!
//! The glue tests (`tests/desktop.rs`) run a real hidden window, message loop, and timer with
//! a fake shell and fake services. They also read back that the tray window is top-level and
//! that the window of an elevated controller lets the shell's two messages through. A
//! successful build and these tests are no evidence that the real icon, menu, hook, files,
//! and broadcasts behave on Windows.
//!
//! # Manual check list
//!
//! On a test desktop with no other Vietnamese input method running, copy the package's (or
//! the build's) `raumakey.exe` into an empty folder the user can write, such as
//! `D:\Tools\Raumakey`, and start it without arguments (or with the ignored test
//! `manual_portable_run`, `tests/manual.rs`). Record each result.
//!
//! 1. **First run:** a welcome box names `D:\Tools\Raumakey\raumakey.toml` "(cạnh
//!    raumakey.exe)"; after OK the V icon is in the notification area with the tooltip
//!    "Raumakey: đang gõ tiếng Việt (Telex)", and `raumakey.toml` exists beside the exe.
//!    Notepad: `tieengs vieetj` types "tiếng việt".
//! 2. **Icon:** click it: E, tooltip "Raumakey: đang gõ tiếng Anh", `aa` stays `aa`; click
//!    again: V. Select it with the keyboard (Win+B, arrows) and press Space or Enter. Tap
//!    Ctrl+Shift: the icon follows the hotkey each time.
//! 3. **Menu:** right-click: the items of D42 in order, with separators; "Gõ tiếng Việt
//!    (Ctrl+Shift)" checked in V mode, exactly one of Telex/VNI/VIQR checked. Choose VNI: the
//!    tooltip names VNI and `tie61ng vie65t` types "tiếng việt" at once; VIQR likewise. Turn
//!    off "Kiểm tra chính tả", turn on "Đặt dấu kiểu mới (oà, uý)" (`hoaf` types "hoà"),
//!    turn off "Gõ tắt (macro)" (a macro of `raumakey-macros.toml` no longer expands). After
//!    each choice `raumakey.toml` holds the new value (open it with "Mở file cấu hình").
//! 4. **Second run:** "Thoát", start again: no welcome box, every choice of 2–3 kept,
//!    including the mode. A start while it runs: no second icon, exit code 0.
//! 5. **"Mở file cấu hình":** Notepad opens `raumakey.toml`. Delete the file while Raumakey
//!    runs, choose it again: a box says there is no file yet.
//! 6. **Hand edit:** in Notepad set `method = "viqr"`, save: within about 2 s the tooltip and
//!    the menu show VIQR and typing follows. Break the TOML (delete a `]`), save: one warning
//!    box, the last settings stay. Fix it and save: the fixed settings apply, no box.
//! 7. **Damaged at start:** exit, break `raumakey.toml`, start: a box asks whether to move
//!    the file aside; "No" keeps it (a menu change then warns once that it cannot be saved),
//!    "Yes" leaves `raumakey.toml.bak-<ms>` and a fresh `raumakey.toml`.
//! 8. **"Khởi động cùng Windows":** turn it on: `Raumakey.lnk` appears in `shell:startup`,
//!    pointing at this `raumakey.exe`, and `startup = "shortcut"` is saved. Sign out and in:
//!    Raumakey starts in the saved mode. Turn it off: the shortcut is gone, `startup =
//!    "none"`. With `startup = "elevated-task"` written by hand, the item is checked and
//!    choosing it explains `raumakey.exe startup none` as administrator.
//! 9. **Moved exe:** with the item on, exit, move the folder to `D:\Tools\Raumakey2`, start
//!    the moved exe: the shortcut now points at the new path (no box); sign out and in.
//! 10. **"Giới thiệu":** the box shows "Raumakey <version>", "Bộ gõ tiếng Việt cho
//!     Windows.", the path of `raumakey.toml` with "(cạnh raumakey.exe)", GPL-3.0-or-later,
//!     and https://github.com/khachuy2705/raumakey. A copy in `C:\Program Files\Raumakey`
//!     (copied there as administrator) shows a path in `%LOCALAPPDATA%\Raumakey`.
//! 11. **Exit stops typing at once:** type `tiee` in Notepad, choose "Thoát": the icon goes,
//!     `s` then types a plain `s`, and no key is stuck down. Also `taskkill /IM raumakey.exe`
//!     (without `/F`) with the menu closed and with it open: the icon goes, typing is plain.
//! 12. **Explorer restart:** switch to E and VNI, restart Windows Explorer (Task Manager):
//!     the icon comes back as E with its tooltip (matrix #58).
//! 13. **Sign out while it runs:** no error box, no hang; a menu change made just before is
//!     in `raumakey.toml` afterwards.
//! 14. **Elevated:** start it with "Run as administrator" and repeat 2, 3, and 12; an elevated
//!     PowerShell types Vietnamese then.
//! 15. **Unwritable folder:** with `raumakey.toml` beside the exe made read-only, change a
//!     setting: one warning box says it cannot be saved and the change stays in effect. Make
//!     the file writable again: within about 2 s the change is in the file (the retry), and
//!     a change before "Thoát" is saved at the exit.
//! 16. **Boxes never block:** on a first run, leave the welcome box open and run
//!     `taskkill /PID <pid>` (without `/F`): the process ends within a second and the box
//!     goes with it. With a box open, click the icon (it switches), restart Explorer (the icon
//!     comes back), and sign out (no hang).
//! 17. **Temporary folder:** open the package ZIP in Explorer and run `raumakey.exe` from
//!     inside it: "Khởi động cùng Windows" explains that Raumakey must be extracted first and
//!     makes no shortcut; a saved `shortcut` is not repaired there.
//! 18. **Two copies:** with copy A's shortcut in `shell:startup` and A still in place, start
//!     copy B with `startup = "shortcut"`: the shortcut still points at A. Delete A, start B:
//!     the shortcut now points at B.
//! 19. **Hand edit of `startup`:** set `startup = "shortcut"` by hand: within about 2 s the
//!     shortcut appears; set `"none"`: the shortcut of this exe goes. `raumakey.exe startup
//!     shortcut` from a prompt saves `startup = "shortcut"`, and the menu shows it checked.

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_model::identity::PRODUCT_NAME;
use raumakey_windows::dialog::{
    DetachedMessageBox, MessageKind, MessageResponse, message_box, spawn_message_box,
};
use raumakey_windows::error::{E_UNEXPECTED, WinError, WinResult};
use raumakey_windows::icon::{Icon, IconKind, IconSize, icon_size};
use raumakey_windows::menu::{MenuItem, PopupMenu};
use raumakey_windows::module::Module;
use raumakey_windows::names::CONTROLLER_INSTANCE;
use raumakey_windows::process::open_in_notepad;
use raumakey_windows::shell::{
    NotifyIcon, create_shortcut, read_shortcut, remove_shortcut, taskbar_created_message,
};
use raumakey_windows::sid::current_process_is_elevated;
use raumakey_windows::single_instance::SingleInstance;
use raumakey_windows::window::{
    WM_TIMER, Window, WindowClass, WindowHandle, WindowHandler, post_quit, run_message_loop,
};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};

use crate::notice::{Notice, NoticeKind};
use crate::run::{Desktop, Opened, ShortcutState, TypingStartError, shortcut_state};
use crate::settings::{Settings, is_temporary_path};
use crate::system::{shortcut_spec, startup_shortcut_path};
use crate::tray::{
    Effect, Event, IconState, InputMode, LOOP_EXIT_CODE, POLL_INTERVAL_MS, POLL_TIMER, START,
    TRAY_CALLBACK, TRAY_ICON_ID, Tray, TrayStart, WAKE, coalesce,
};
use crate::typing::{Mailbox, TypingError, TypingHook, TypingUpdate};
use crate::worker::{SettingsWorker, WorkerCommand, WorkerPorts, WorkerThread};

/// Label of the tray window's class (`Raumakey.controller.<serial>`).
pub const WINDOW_CLASS_LABEL: &str = "controller";
/// Title of the hidden tray window (never shown; it names the window in window lists).
pub const WINDOW_TITLE: &str = PRODUCT_NAME;
/// How long an exit waits for the worker to finish the commands already sent.
pub const WORKER_FINISH_TIMEOUT: Duration = Duration::from_secs(3);

/// Where the hook observer and the worker post [`WAKE`]: the tray window, once it exists.
pub type WakeTarget = Arc<OnceLock<WindowHandle>>;

/// Posts [`WAKE`] to the tray window if it exists (an event left before is drained at
/// [`START`]). Never waits.
fn wake(target: &WakeTarget) {
    if let Some(window) = target.get() {
        // A full queue or a window that is gone loses only the notification.
        let _ = window.post(WAKE.id(), WPARAM(0), LPARAM(0));
    }
}

/// The desktop of this Windows session for the `raumakey.exe` at `exe`.
#[derive(Debug)]
pub struct WinDesktop {
    exe: PathBuf,
    /// The single-instance mutex, while this process holds it.
    instance: Option<SingleInstance>,
    /// The settings, until the worker takes them.
    settings: Option<Settings>,
    /// The typing hook, until the tray takes it.
    typing: Option<TypingHook>,
    /// The tray's inbox: the observer and the worker send into it.
    sender: Sender<Event>,
    /// Its other end, until the tray takes it.
    inbox: Option<Receiver<Event>>,
    wake: WakeTarget,
}

impl WinDesktop {
    /// The desktop for the `raumakey.exe` at `exe`; nothing is acquired yet.
    pub fn new(exe: PathBuf) -> Self {
        let (sender, inbox) = mpsc::channel();
        Self {
            exe,
            instance: None,
            settings: None,
            typing: None,
            sender,
            inbox: Some(inbox),
            wake: WakeTarget::default(),
        }
    }
}

impl Desktop for WinDesktop {
    fn acquire_instance(&mut self) -> WinResult<bool> {
        let instance = SingleInstance::acquire(CONTROLLER_INSTANCE)?;
        let acquired = instance.is_some();
        if acquired {
            self.instance = instance;
        }
        Ok(acquired)
    }

    fn open_settings(&mut self) -> Opened {
        let (settings, report) = Settings::open_for_current_exe();
        let opened = Opened {
            report,
            runtime: settings.runtime_snapshot().clone(),
            startup: settings.document().system.startup,
            temporary_exe: is_temporary_path(&self.exe),
        };
        self.settings = Some(settings);
        opened
    }

    fn start_typing(&mut self) -> Result<(), TypingStartError> {
        let settings = self.settings.as_ref().ok_or(TypingStartError::Refused)?;
        let sender = self.sender.clone();
        let target = Arc::clone(&self.wake);
        // On the hook thread, inside the hook callback: hand the mode on and return.
        let observer = move |vietnamese: bool| {
            let _ = sender.send(Event::Hotkey(vietnamese));
            wake(&target);
        };
        let hook = TypingHook::start(
            settings.runtime_snapshot().clone(),
            MacroSnapshot::clone(settings.macro_snapshot()),
            observer,
        )
        .map_err(|error| match error {
            TypingError::Snapshot(_) => TypingStartError::Refused,
            TypingError::Hook(error) => TypingStartError::Hook(error),
        })?;
        self.typing = Some(hook);
        Ok(())
    }

    fn startup_shortcut(&mut self) -> WinResult<ShortcutState> {
        read_startup_shortcut(&self.exe)
    }

    fn create_startup_shortcut(&mut self) -> WinResult<()> {
        create_shortcut(&startup_shortcut_path()?, &shortcut_spec(&self.exe))
    }

    fn run_tray(&mut self, start: TrayStart) -> WinResult<i32> {
        let settings = self
            .settings
            .take()
            .ok_or(WinError::new(E_UNEXPECTED, "settings of the tray"))?;
        let inbox = self
            .inbox
            .take()
            .ok_or(WinError::new(E_UNEXPECTED, "inbox of the tray"))?;
        let parts = TrayParts {
            start,
            settings,
            typing: self.typing.take(),
            exe: self.exe.clone(),
            sender: self.sender.clone(),
            inbox,
            wake: Arc::clone(&self.wake),
        };
        run_tray(parts)
    }

    fn stop_typing(&mut self) {
        if let Some(hook) = self.typing.take() {
            // A hook thread that does not end in time is detached; its hooks go with it.
            let _ = hook.stop();
        }
    }

    fn release_instance(&mut self) {
        self.instance = None;
    }

    fn report_error(&mut self, text: &str) {
        // Nothing is left to try when the box itself cannot be shown.
        let _ = message_box(None, text, PRODUCT_NAME, MessageKind::Error);
    }
}

/// What `Raumakey.lnk` in the user's Startup folder starts, seen from the `raumakey.exe` at
/// `exe` (`crate::run::shortcut_state`).
fn read_startup_shortcut(exe: &Path) -> WinResult<ShortcutState> {
    let lnk = startup_shortcut_path()?;
    match lnk.try_exists() {
        Ok(false) => return Ok(ShortcutState::Missing),
        Ok(true) => {}
        Err(error) => return Err(WinError::from_io(&error, "startup shortcut")),
    }
    let spec = read_shortcut(&lnk)?;
    // A target that cannot be checked counts as existing: it is never re-pointed.
    let exists = spec.target.try_exists().unwrap_or(true);
    Ok(shortcut_state(&spec.target, &spec.arguments, exe, exists))
}

/// Everything [`run_tray`] needs.
#[derive(Debug)]
pub struct TrayParts {
    /// What the tray starts from.
    pub start: TrayStart,
    /// The settings, for the worker.
    pub settings: Settings,
    /// The running typing hook (the tray stops it on every way out).
    pub typing: Option<TypingHook>,
    /// The running `raumakey.exe`, for the Startup-folder shortcut.
    pub exe: PathBuf,
    /// Into the tray's inbox, for the worker.
    pub sender: Sender<Event>,
    /// The tray's inbox.
    pub inbox: Receiver<Event>,
    /// Where the worker posts [`WAKE`]; set to the tray window here.
    pub wake: WakeTarget,
}

/// Runs the tray on the calling thread until its message loop ends, and returns the loop's
/// exit code (see the module documentation).
pub fn run_tray(parts: TrayParts) -> WinResult<i32> {
    let TrayParts {
        start,
        settings,
        typing,
        exe,
        sender,
        inbox,
        wake: target,
    } = parts;
    let icons = ModeIcons::load(Module::executable()?, icon_size(IconKind::Small)?)?;
    // Without the registered message (0) the tray still runs; only an Explorer restart does
    // not bring the icon back.
    let tray = Tray::new(start, taskbar_created_message().unwrap_or(0));
    let class = WindowClass::register(WINDOW_CLASS_LABEL)?;
    let temporary_exe = is_temporary_path(&exe);
    let ports = WinWorkerPorts {
        exe,
        mailbox: typing.as_ref().map(TypingHook::mailbox),
    };
    let deliver_target = Arc::clone(&target);
    let sender_for_notices = sender.clone();
    let worker = SettingsWorker::new(settings, ports).with_temporary_exe(temporary_exe);
    let worker = WorkerThread::spawn(worker, move |answer| {
        let _ = sender.send(Event::Worker(answer));
        wake(&deliver_target);
    })
    .map_err(|error| WinError::from_io(&error, "settings worker thread"))?;
    let services = WinServices {
        hook: typing,
        worker: Some(worker),
        inbox,
    };
    // An elevation check that fails counts as not elevated: the filter stays closed.
    let elevated = current_process_is_elevated().unwrap_or(false);
    let window = create_tray_window(
        &class,
        tray,
        ShellTray::new(icons, sender_for_notices, Arc::clone(&target)),
        services,
        elevated,
        POLL_INTERVAL_MS,
    )?;
    let _ = target.set(window.handle());
    let code = run_message_loop();
    drop(window);
    code
}

/// Creates the hidden top-level tray window of `class` whose handler runs `tray` on `shell`
/// and `services`, lets the shell's messages through UIPI when `elevated`, starts the poll
/// timer every `poll_interval_ms`, and posts [`START`], so that the icon is added once the
/// message loop runs.
pub fn create_tray_window<S: TrayShell + 'static, V: TrayServices + 'static>(
    class: &WindowClass,
    tray: Tray,
    shell: S,
    services: V,
    elevated: bool,
    poll_interval_ms: u32,
) -> WinResult<Window> {
    let shell_messages = tray.shell_messages();
    let handler = TrayWindow::new(tray, shell, services);
    let window = Window::create_hidden(class, WINDOW_TITLE, Box::new(handler))?;
    if elevated {
        for message in shell_messages {
            // Without it the controller still runs; only the shell's message is lost (the
            // icon would not come back after an Explorer restart, or would not answer).
            let _ = window.allow_message_from_lower_integrity(message);
        }
    }
    // Without the timer the controller still types; only hand edits are not reloaded.
    let _ = window.set_timer(POLL_TIMER, poll_interval_ms);
    window.handle().post(START.id(), WPARAM(0), LPARAM(0))?;
    Ok(window)
}

/// What the tray asks of the shell; [`ShellTray`] is the real one. Every method runs on the
/// thread of the tray window, inside its handler, and only the menu waits (while the user
/// holds it open).
pub trait TrayShell {
    /// Adds the icon showing `icon` (its image and tooltip) for window `owner`; if it was
    /// added before, adds it again (`TaskbarCreated`) or, when it still exists, updates it.
    fn show(&mut self, owner: HWND, icon: IconState);

    /// Shows `icon` on the icon, if the icon exists; a change that fails leaves the icon to
    /// [`ensure`](Self::ensure).
    fn update(&mut self, icon: IconState);

    /// Adds the icon showing `icon` again when it is not shown: its add or its last change
    /// failed (every tick of the poll timer).
    fn ensure(&mut self, owner: HWND, icon: IconState);

    /// Shows the popup menu of `items` for window `owner` at (`x`, `y`) in screen
    /// coordinates, and returns the chosen command id, or `None` when it was dismissed.
    fn open_menu(
        &mut self,
        owner: HWND,
        x: i32,
        y: i32,
        items: &[MenuItem<'static>],
    ) -> Option<u16>;

    /// Shows `notice` without waiting for it: a message box on a thread of its own. A "yes"
    /// to its question arrives later in the inbox ([`Event::Answered`]).
    fn notify(&mut self, notice: Notice);

    /// Closes every notice that is still open, without waiting.
    fn dismiss_notices(&mut self);

    /// Removes the icon, if it exists.
    fn remove(&mut self);
}

/// The notification icon as the shell sees it; [`WinNotifyArea`] is the real one.
/// [`ShellTray`] keeps track of whether the icon is shown and adds it again when not.
pub trait NotifyArea {
    /// Adds the icon showing `icon` for window `owner` (or adds it again); whether the
    /// shell took it.
    fn add(&mut self, owner: HWND, icon: IconState) -> bool;
    /// Changes the image and tooltip of the icon; whether the shell took the change.
    fn modify(&mut self, icon: IconState) -> bool;
    /// Removes the icon.
    fn delete(&mut self);
}

/// What the tray asks of the typing hook and the settings worker; [`WinServices`] is the
/// real one. Every method runs on the thread of the tray window and returns at once, except
/// [`finish_worker`](Self::finish_worker) and [`stop_typing`](Self::stop_typing), which run
/// only on the way out.
pub trait TrayServices {
    /// Hands the mode to the hook (`TypingUpdate::Mode`).
    fn hook_mode(&mut self, vietnamese: bool);
    /// Hands a runtime snapshot to the hook (`TypingUpdate::Runtime`).
    fn hook_runtime(&mut self, runtime: RuntimeSnapshot);
    /// Stops the hook (waits at most 2 s for its thread).
    fn stop_typing(&mut self);
    /// Installs the hook again when Windows removed it (`TypingHook::ensure_alive`); called
    /// at every tick of the poll timer while the tray runs.
    fn keep_typing_alive(&mut self);
    /// Queues a command, numbered `seq`, for the worker.
    fn send(&mut self, seq: u64, command: WorkerCommand);
    /// Lets the worker finish the commands sent, for a bounded time.
    fn finish_worker(&mut self);
    /// The next event of the inbox, if one is waiting.
    fn next_event(&mut self) -> Option<Event>;
}

/// The handler of the tray window: passes each message to the [`Tray`], carries out its
/// effects on the shell and the services, and drains the inbox.
///
/// When it is freed while the tray runs (the window was destroyed under it, see the module
/// documentation), it stops the hook, removes the icon, lets the worker finish, and ends the
/// message loop with [`LOOP_EXIT_CODE`], so the process never lingers without its icon while
/// it holds the single-instance mutex, and never leaves the hook running without its tray.
pub struct TrayWindow<S: TrayShell, V: TrayServices> {
    tray: Tray,
    shell: S,
    services: V,
}

impl<S: TrayShell, V: TrayServices> core::fmt::Debug for TrayWindow<S, V> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TrayWindow")
            .field("tray", &self.tray)
            .finish_non_exhaustive()
    }
}

impl<S: TrayShell, V: TrayServices> TrayWindow<S, V> {
    /// The handler for `tray` on `shell` and `services`.
    pub fn new(tray: Tray, shell: S, services: V) -> Self {
        Self {
            tray,
            shell,
            services,
        }
    }

    /// Carries out `effects` in order, with the effects of a menu choice or of a yes in
    /// their place.
    fn run(&mut self, owner: HWND, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::ShowIcon(icon) => self.shell.show(owner, icon),
                Effect::UpdateIcon(icon) => self.shell.update(icon),
                Effect::OpenMenu { x, y } => {
                    let items = self.tray.menu();
                    let chosen = self.shell.open_menu(owner, x, y, &items);
                    let next = self.tray.menu_choice(chosen);
                    self.run(owner, next);
                }
                Effect::Notify(notice) => self.shell.notify(notice),
                Effect::EnsureIcon(icon) => self.shell.ensure(owner, icon),
                Effect::HookMode(vietnamese) => self.services.hook_mode(vietnamese),
                Effect::HookRuntime(runtime) => self.services.hook_runtime(*runtime),
                Effect::Worker { seq, command } => self.services.send(seq, command),
                Effect::StopTyping => self.services.stop_typing(),
                Effect::DismissNotices => self.shell.dismiss_notices(),
                Effect::FlushHotkeys => {
                    let events = self.take_events();
                    let next = self.tray.flush_hotkeys(events);
                    self.run(owner, next);
                }
                Effect::FinishWorker => self.services.finish_worker(),
                Effect::RemoveIcon => self.shell.remove(),
                Effect::EndLoop => post_quit(LOOP_EXIT_CODE),
            }
        }
    }

    /// Every event waiting in the inbox now.
    fn take_events(&mut self) -> Vec<Event> {
        core::iter::from_fn(|| self.services.next_event()).collect()
    }

    /// Takes the events waiting in the inbox while the tray runs, the hotkey changes among
    /// them coalesced ([`coalesce`]), until the inbox is empty (an effect may add to it).
    fn drain(&mut self, owner: HWND) {
        while self.tray.is_running() {
            let events = self.take_events();
            if events.is_empty() {
                break;
            }
            for event in coalesce(events) {
                let effects = self.tray.on_event(event);
                self.run(owner, effects);
            }
        }
    }
}

impl<S: TrayShell, V: TrayServices> WindowHandler for TrayWindow<S, V> {
    fn on_message(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        if message == WM_TIMER && wparam.0 == POLL_TIMER && self.tray.is_running() {
            self.services.keep_typing_alive();
        }
        let reply = self.tray.on_message(message, wparam.0, lparam.0);
        if reply.result.is_some() {
            self.run(hwnd, reply.effects);
            self.drain(hwnd);
        }
        reply.result.map(LRESULT)
    }
}

impl<S: TrayShell, V: TrayServices> Drop for TrayWindow<S, V> {
    fn drop(&mut self) {
        if self.tray.ends_loop_when_dropped() {
            // The order of an exit (`crate::tray`), without the window.
            self.services.stop_typing();
            self.shell.dismiss_notices();
            let events = self.take_events();
            for effect in self.tray.flush_hotkeys(events) {
                if let Effect::Worker { seq, command } = effect {
                    self.services.send(seq, command);
                }
            }
            self.services.finish_worker();
            self.shell.remove();
            post_quit(LOOP_EXIT_CODE);
        }
    }
}

/// The real hook and worker of the tray.
#[derive(Debug)]
pub struct WinServices {
    hook: Option<TypingHook>,
    worker: Option<WorkerThread>,
    inbox: Receiver<Event>,
}

impl TrayServices for WinServices {
    fn hook_mode(&mut self, vietnamese: bool) {
        if let Some(hook) = &self.hook {
            let _ = hook.update(TypingUpdate::Mode(vietnamese));
        }
    }

    fn hook_runtime(&mut self, runtime: RuntimeSnapshot) {
        if let Some(hook) = &self.hook {
            // The settings validate every snapshot; a refused one leaves the last good one.
            let _ = hook.update(TypingUpdate::Runtime(runtime));
        }
    }

    fn stop_typing(&mut self) {
        if let Some(hook) = self.hook.take() {
            let _ = hook.stop();
        }
    }

    fn keep_typing_alive(&mut self) {
        if let Some(hook) = &mut self.hook {
            // A failed check or restart is tried again at the next tick.
            let _ = hook.ensure_alive();
        }
    }

    fn send(&mut self, seq: u64, command: WorkerCommand) {
        if let Some(worker) = &self.worker {
            worker.send(seq, command);
        }
    }

    fn finish_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            let _ = worker.finish(WORKER_FINISH_TIMEOUT);
        }
    }

    fn next_event(&mut self) -> Option<Event> {
        self.inbox.try_recv().ok()
    }
}

/// The side effects of the worker on Windows.
#[derive(Debug)]
struct WinWorkerPorts {
    exe: PathBuf,
    mailbox: Option<Arc<Mailbox>>,
}

impl WorkerPorts for WinWorkerPorts {
    fn post_macros(&mut self, macros: &MacroSnapshot) {
        if let Some(mailbox) = &self.mailbox {
            // A refused table leaves the hook on its last good macros.
            let _ = mailbox.post(TypingUpdate::Macros(macros.clone()));
        }
    }

    fn shortcut_state(&mut self) -> WinResult<ShortcutState> {
        read_startup_shortcut(&self.exe)
    }

    fn create_shortcut(&mut self) -> WinResult<()> {
        create_shortcut(&startup_shortcut_path()?, &shortcut_spec(&self.exe))
    }

    fn remove_shortcut(&mut self) -> WinResult<bool> {
        remove_shortcut(&startup_shortcut_path()?)
    }

    fn open_in_notepad(&mut self, file: &Path) -> WinResult<()> {
        open_in_notepad(file)
    }
}

/// The V and E icons, loaded once at one size and shared with the notification icon.
#[derive(Debug)]
pub struct ModeIcons {
    vietnamese: Rc<Icon>,
    english: Rc<Icon>,
}

impl ModeIcons {
    /// Loads the icon of each mode ([`InputMode::icon_id`]) from `module` at `size`. Only
    /// `raumakey.exe` carries them (its `build.rs`).
    pub fn load(module: Module, size: IconSize) -> WinResult<Self> {
        Ok(Self {
            vietnamese: Rc::new(Icon::load(module, InputMode::Vietnamese.icon_id(), size)?),
            english: Rc::new(Icon::load(module, InputMode::English.icon_id(), size)?),
        })
    }

    /// The icon of `mode`.
    pub fn icon(&self, mode: InputMode) -> Rc<Icon> {
        Rc::clone(match mode {
            InputMode::Vietnamese => &self.vietnamese,
            InputMode::English => &self.english,
        })
    }
}

/// The real notification icon (`raumakey_windows::shell::NotifyIcon`).
#[derive(Debug)]
pub struct WinNotifyArea {
    icons: ModeIcons,
    /// The icon, once the shell accepted its first add.
    notify: Option<NotifyIcon>,
}

impl WinNotifyArea {
    /// The area showing `icons`; the icon is added by the first [`NotifyArea::add`].
    pub fn new(icons: ModeIcons) -> Self {
        Self {
            icons,
            notify: None,
        }
    }
}

impl NotifyArea for WinNotifyArea {
    fn add(&mut self, owner: HWND, icon: IconState) -> bool {
        match &mut self.notify {
            Some(notify) => {
                // The data first: an icon that still exists takes the change (and adding it
                // again would fail); after an Explorer restart the change fails and the icon
                // is added again with the current state.
                let changed = notify.set_icon(self.icons.icon(icon.mode)).is_ok()
                    & notify.set_tooltip(icon.tooltip()).is_ok();
                changed || notify.readd().is_ok()
            }
            None => {
                self.notify = NotifyIcon::add(
                    owner,
                    u32::from(TRAY_ICON_ID),
                    TRAY_CALLBACK.id(),
                    self.icons.icon(icon.mode),
                    icon.tooltip(),
                )
                .ok();
                self.notify.is_some()
            }
        }
    }

    fn modify(&mut self, icon: IconState) -> bool {
        let Some(notify) = &mut self.notify else {
            return false;
        };
        notify.set_icon(self.icons.icon(icon.mode)).is_ok()
            & notify.set_tooltip(icon.tooltip()).is_ok()
    }

    fn delete(&mut self) {
        if let Some(notify) = self.notify.take() {
            let _ = notify.delete();
        }
    }
}

/// The real shell of the tray: the notification icon (over a [`NotifyArea`]), the popup menu,
/// and message boxes on threads of their own.
#[derive(Debug)]
pub struct ShellTray<A: NotifyArea = WinNotifyArea> {
    area: A,
    /// The icon is shown: its last add or change worked.
    shown: bool,
    /// The notices shown so far that may still be open.
    notices: Vec<DetachedMessageBox>,
    /// Where a "yes" to a notice goes: the tray's inbox, then [`WAKE`].
    inbox: Sender<Event>,
    wake: WakeTarget,
}

impl ShellTray<WinNotifyArea> {
    /// A shell showing `icons`, answering notices into `inbox` (then [`WAKE`] to `wake`).
    pub fn new(icons: ModeIcons, inbox: Sender<Event>, wake: WakeTarget) -> Self {
        Self::with_area(WinNotifyArea::new(icons), inbox, wake)
    }
}

impl<A: NotifyArea> ShellTray<A> {
    /// A shell over `area`; the icon is added by the first [`TrayShell::show`].
    pub fn with_area(area: A, inbox: Sender<Event>, wake: WakeTarget) -> Self {
        Self {
            area,
            shown: false,
            notices: Vec::new(),
            inbox,
            wake,
        }
    }
}

impl<A: NotifyArea> TrayShell for ShellTray<A> {
    fn show(&mut self, owner: HWND, icon: IconState) {
        // Without a taskbar yet, the next tick or TaskbarCreated adds the icon.
        self.shown = self.area.add(owner, icon);
    }

    fn update(&mut self, icon: IconState) {
        if self.shown {
            self.shown = self.area.modify(icon);
        }
    }

    fn ensure(&mut self, owner: HWND, icon: IconState) {
        if !self.shown {
            self.shown = self.area.add(owner, icon);
        }
    }

    fn open_menu(
        &mut self,
        owner: HWND,
        x: i32,
        y: i32,
        items: &[MenuItem<'static>],
    ) -> Option<u16> {
        let mut menu = PopupMenu::new().ok()?;
        for item in items {
            menu.append(*item).ok()?;
        }
        menu.track(owner, POINT { x, y })
    }

    fn notify(&mut self, notice: Notice) {
        self.notices.retain(|shown| !shown.is_closed());
        let kind = match (notice.question, notice.kind) {
            (Some(_), _) => MessageKind::Question,
            (None, NoticeKind::Information) => MessageKind::Information,
            (None, NoticeKind::Warning) => MessageKind::Warning,
            (None, NoticeKind::Error) => MessageKind::Error,
        };
        let question = notice.question;
        let inbox = self.inbox.clone();
        let target = Arc::clone(&self.wake);
        // No owner: the box gets a taskbar button of its own and cannot get lost; on a
        // thread of its own, it never holds up the tray's messages.
        let shown = spawn_message_box(&notice.text, PRODUCT_NAME, kind, move |answer| {
            if answer == Some(MessageResponse::Yes)
                && let Some(question) = question
            {
                let _ = inbox.send(Event::Answered(question));
                wake(&target);
            }
        });
        // A box that cannot be shown loses only the notice.
        if let Ok(shown) = shown {
            self.notices.push(shown);
        }
    }

    fn dismiss_notices(&mut self) {
        for shown in self.notices.drain(..) {
            shown.dismiss();
        }
    }

    fn remove(&mut self) {
        self.area.delete();
        self.shown = false;
    }
}
