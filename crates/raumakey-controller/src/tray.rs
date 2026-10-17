//! The controller's notification-area icon as a pure state machine (plan §10.2, D42; design
//! §21.5; matrix #58): the settings that the icon and the menu show, the icon and tooltip, the
//! tray menu, and what to do with each message of the hidden window, each menu choice, each
//! mode change of the hotkey, and each answer of the settings worker. The Windows glue in
//! `crate::desktop` passes them in and carries out the [`Effect`]s on the real notification
//! icon, popup menu, message boxes, typing hook, and worker thread; everything here is tested
//! on every host.
//!
//! # Settings
//!
//! The tray starts from what the run mode read from `raumakey.toml` ([`TrayStart`]): the
//! runtime snapshot in effect (mode, input method, options) and the startup mode. Each choice
//! changes them at once: the icon and menu follow, the typing hook gets the new mode
//! ([`Effect::HookMode`]) or the new runtime snapshot ([`Effect::HookRuntime`], built by
//! [`with_change`] exactly as `crate::settings::Settings` builds it after the change), and the
//! settings worker commits the change ([`Effect::Worker`] with
//! [`WorkerCommand::Apply`]). A commit that fails is shown once per kind of error
//! (`crate::notice::save_error_notice`), and the change stays in effect.
//!
//! **Every runtime snapshot handed to the hook carries the mode the icon shows.** Applying a
//! runtime snapshot sets the hook's mode to the one it carries, so the tray is the only one
//! that hands runtime snapshots to the hook. When the hotkey changed the mode in the hook
//! ([`Event::Hotkey`], from the hook observer), the tray takes the mode over, saves it, and
//! hands it back to the hook ([`Effect::HookMode`]): a snapshot handed to the hook before the
//! tray learned of the hotkey (with the mode shown then) is undone by it.
//!
//! **Answers of the worker.** Each command is numbered ([`Effect::Worker`] `seq`). An answer
//! that carries the settings in effect ([`Answer::state`]) is taken over only when it answers
//! the last command sent: an older answer does not know of a change still on its way, and the
//! answer to that change brings everything (`crate::worker`). Taking settings over refreshes
//! the icon and the menu, and hands the hook the new snapshot unless it differs from the one
//! in effect only in its revision. This is how a hand edit reaches the hook.
//!
//! # Messages
//!
//! | Message | While running | Answer |
//! |---|---|---|
//! | [`START`] (posted once when the message loop starts) | adds the icon ([`Effect::ShowIcon`]), then shows the start notices | 0 |
//! | [`TRAY_CALLBACK`] `NIN_SELECT` (click), `NIN_KEYSELECT` (keyboard) | switches the mode | 0 |
//! | [`TRAY_CALLBACK`] `WM_CONTEXTMENU` (right click, Shift+F10, menu key) | opens the menu at the anchor ([`Effect::OpenMenu`]) | 0 |
//! | [`TRAY_CALLBACK`], other notifications or another icon id | nothing | 0 |
//! | `WM_TIMER` of [`POLL_TIMER`] (every [`POLL_INTERVAL_MS`]) | asks the worker to poll, unless a poll is on its way | 0 |
//! | [`WAKE`] (the worker or the hook observer left something in the inbox) | nothing: the glue drains the inbox ([`Tray::on_event`]) | 0 |
//! | `TaskbarCreated` (Explorer restarted) | adds the icon again with the current state (matrix #58) | 0 |
//! | `WM_CLOSE`; `WM_ENDSESSION` with a non-zero `wParam` (the session ends) | exits | 0 |
//! | anything else, and `WM_ENDSESSION` of a cancelled end | nothing | `DefWindowProcW` |
//!
//! Exiting is [`Effect::StopTyping`] (typing is plain at once), [`Effect::RemoveIcon`], and
//! [`Effect::EndLoop`] (the glue lets the worker finish what was sent, for a bounded time,
//! and ends the message loop with [`LOOP_EXIT_CODE`]), all before the window procedure
//! returns: after `WM_ENDSESSION` the process may end at any moment. `WM_CLOSE` is always
//! answered, never left to `DefWindowProcW`, which would destroy the window under the running
//! tray. Before [`START`] the icon does not exist, so its messages, the timer,
//! `TaskbarCreated`, menu choices, and events do nothing, while `WM_CLOSE` and the end of the
//! session still exit. After an exit every message, choice, and event is answered without
//! effect, so a message still queued behind it (such as a `TaskbarCreated`) cannot bring the
//! icon back.
//!
//! # Menu (D42)
//!
//! [`Tray::menu`], flat: "Gõ tiếng Việt (Ctrl+Shift)" (check), separator, "Telex", "VNI",
//! "VIQR" (the method in effect is checked; none for a user-defined key map), separator,
//! "Kiểm tra chính tả", "Đặt dấu kiểu mới (oà, uý)", "Gõ tắt (macro)" (checks), separator,
//! "Khởi động cùng Windows" (check), "Mở file cấu hình", "Giới thiệu", separator, "Thoát".
//!
//! - "Khởi động cùng Windows" asks the worker to create the shortcut and save `shortcut`, or
//!   to remove it and save `none` ([`WorkerCommand::Startup`]); the check follows the worker's
//!   answer. When the elevated logon task starts Raumakey (`elevated-task`, set from the
//!   command line), the item is checked and only explains that `raumakey.exe startup none`
//!   must run with administrator rights.
//! - "Mở file cấu hình" asks the worker to open `raumakey.toml` in Notepad.
//! - "Giới thiệu" shows `crate::notice::about_notice`.
//!
//! # Losing the window
//!
//! A modal loop that runs inside the window's handler (the tray menu, a message box) leaves
//! the window's messages to `DefWindowProcW` (see `raumakey_windows::window`), which answers
//! a `WM_CLOSE` that arrives meanwhile by destroying the window. The handler is then freed
//! when it returns, and [`Tray::ends_loop_when_dropped`] tells the glue that the loop must end
//! anyway (after stopping the hook and the worker).
//!
//! Strings shown to the user are new Raumakey strings in Vietnamese (plan §10.2,
//! `crate::notice`). Nothing here holds keystrokes, document text, or macro text; `Debug` of
//! the tray shows its mode, method, startup mode, and counters.

use core::fmt;
use std::path::PathBuf;

use raumakey_config::{ConfigError, StartupMode};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_windows::menu::MenuItem;
use raumakey_windows::shell::{NotifyIconEvent, decode_notify_icon_message};
use raumakey_windows::window::{
    AppMessage, REGISTERED_MESSAGE_FIRST, REGISTERED_MESSAGE_LAST, WM_CLOSE, WM_ENDSESSION,
    WM_TIMER,
};

use crate::notice::{
    Notice, Question, about_notice, elevated_task_notice, open_error_notice, recovery_notice,
    save_error_notice, startup_error_notice, status_notice,
};
use crate::settings::{Change, DocumentStatus, Location, Recovery, Status};
use crate::worker::{Answer, Outcome, SettingsState, StartupError, WorkerCommand};

// `ICON_VIETNAMESE` and `ICON_ENGLISH`: the icon groups that `build.rs` embeds, with the ids
// that `raumakey-resources` (a build-dependency only) assigns.
include!(concat!(env!("OUT_DIR"), "/icon_ids.rs"));

/// Id of the tray icon among the icons of the hidden window.
pub const TRAY_ICON_ID: u16 = 1;
/// The callback message of the tray icon (`WM_APP + 1`).
pub const TRAY_CALLBACK: AppMessage = app_message(1);
/// Posted once to the hidden window when its message loop starts (`WM_APP + 2`): the tray
/// adds its icon.
pub const START: AppMessage = app_message(2);
/// Posted to the hidden window by the settings worker and the hook observer after they left
/// an [`Event`] in the tray's inbox (`WM_APP + 3`).
pub const WAKE: AppMessage = app_message(3);
/// Id of the window timer that asks the worker to poll the configuration files.
pub const POLL_TIMER: usize = 1;
/// Interval of [`POLL_TIMER`]: a hand edit is picked up within about two seconds (D40).
pub const POLL_INTERVAL_MS: u32 = 2_000;
/// Exit code of the message loop after an exit.
pub const LOOP_EXIT_CODE: i32 = 0;

/// Tooltip of the icon in English mode: "Raumakey: typing English".
pub const TOOLTIP_ENGLISH: &str = "Raumakey: đang gõ tiếng Anh";
/// Tooltip in Vietnamese mode with Telex.
pub const TOOLTIP_TELEX: &str = "Raumakey: đang gõ tiếng Việt (Telex)";
/// Tooltip in Vietnamese mode with VNI.
pub const TOOLTIP_VNI: &str = "Raumakey: đang gõ tiếng Việt (VNI)";
/// Tooltip in Vietnamese mode with VIQR.
pub const TOOLTIP_VIQR: &str = "Raumakey: đang gõ tiếng Việt (VIQR)";
/// Tooltip in Vietnamese mode with a user-defined key map ("own input method").
pub const TOOLTIP_USER_DEFINED: &str = "Raumakey: đang gõ tiếng Việt (kiểu gõ riêng)";

/// The check item of Vietnamese mode: "Type Vietnamese (Ctrl+Shift)".
pub const MENU_VIETNAMESE_MODE: &str = "Gõ tiếng Việt (Ctrl+Shift)";
/// The input method Telex.
pub const MENU_TELEX: &str = "Telex";
/// The input method VNI.
pub const MENU_VNI: &str = "VNI";
/// The input method VIQR.
pub const MENU_VIQR: &str = "VIQR";
/// Spell checking: "Check spelling".
pub const MENU_SPELL_CHECK: &str = "Kiểm tra chính tả";
/// Modern tone placement: "New-style tone marks (oà, uý)".
pub const MENU_MODERN_TONE: &str = "Đặt dấu kiểu mới (oà, uý)";
/// Macro expansion: "Shorthand (macro)".
pub const MENU_MACROS: &str = "Gõ tắt (macro)";
/// The Startup-folder shortcut: "Start with Windows".
pub const MENU_STARTUP: &str = "Khởi động cùng Windows";
/// Open `raumakey.toml` in Notepad: "Open the configuration file".
pub const MENU_OPEN_CONFIG: &str = "Mở file cấu hình";
/// About: "About".
pub const MENU_ABOUT: &str = "Giới thiệu";
/// Exit: "Exit".
pub const MENU_EXIT: &str = "Thoát";

/// `WM_APP + offset`, checked at compile time.
const fn app_message(offset: u32) -> AppMessage {
    match AppMessage::new(offset) {
        Some(message) => message,
        None => panic!("offset outside the WM_APP range"),
    }
}

/// The input mode that the controller shows and switches.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputMode {
    /// Vietnamese mode on: the "V" icon.
    Vietnamese,
    /// Vietnamese mode off: the "E" icon.
    English,
}

impl InputMode {
    /// The mode of an `enabled` flag (`[input] enabled`, `RuntimeSnapshot::vietnamese_mode`).
    pub const fn from_vietnamese(vietnamese: bool) -> Self {
        if vietnamese {
            Self::Vietnamese
        } else {
            Self::English
        }
    }

    /// True in Vietnamese mode.
    pub const fn is_vietnamese(self) -> bool {
        matches!(self, Self::Vietnamese)
    }

    /// The other mode.
    pub const fn toggled(self) -> Self {
        match self {
            Self::Vietnamese => Self::English,
            Self::English => Self::Vietnamese,
        }
    }

    /// The icon group of the mode in the controller's resources (`raumakey-resources`).
    pub const fn icon_id(self) -> u16 {
        match self {
            Self::Vietnamese => ICON_VIETNAMESE,
            Self::English => ICON_ENGLISH,
        }
    }
}

/// What the icon shows: its image (the mode) and its tooltip (the mode and the method).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IconState {
    /// The mode: the V or E image.
    pub mode: InputMode,
    /// The input method in effect, named by the tooltip in Vietnamese mode.
    pub method: InputMethod,
}

impl IconState {
    /// The icon group of the image.
    pub const fn icon_id(self) -> u16 {
        self.mode.icon_id()
    }

    /// The tooltip.
    pub const fn tooltip(self) -> &'static str {
        match (self.mode, self.method) {
            (InputMode::English, _) => TOOLTIP_ENGLISH,
            (InputMode::Vietnamese, InputMethod::Telex) => TOOLTIP_TELEX,
            (InputMode::Vietnamese, InputMethod::Vni) => TOOLTIP_VNI,
            (InputMode::Vietnamese, InputMethod::Viqr) => TOOLTIP_VIQR,
            (InputMode::Vietnamese, InputMethod::UserDefined) => TOOLTIP_USER_DEFINED,
        }
    }
}

/// The commands of the tray menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrayCommand {
    /// Switches Vietnamese mode.
    VietnameseMode,
    /// Telex.
    Telex,
    /// VNI.
    Vni,
    /// VIQR.
    Viqr,
    /// Switches spell checking.
    SpellCheck,
    /// Switches modern tone placement.
    ModernTone,
    /// Switches macro expansion.
    Macros,
    /// Switches the Startup-folder shortcut.
    Startup,
    /// Opens `raumakey.toml` in Notepad.
    OpenConfig,
    /// Shows "Giới thiệu".
    About,
    /// Exit: stops typing, removes the icon, and ends the controller.
    Exit,
}

impl TrayCommand {
    /// Every command, in menu order.
    pub const ALL: [Self; 11] = [
        Self::VietnameseMode,
        Self::Telex,
        Self::Vni,
        Self::Viqr,
        Self::SpellCheck,
        Self::ModernTone,
        Self::Macros,
        Self::Startup,
        Self::OpenConfig,
        Self::About,
        Self::Exit,
    ];

    /// The menu command id (never 0, which a menu returns for "nothing chosen").
    pub const fn id(self) -> u16 {
        match self {
            Self::VietnameseMode => 1,
            Self::Telex => 2,
            Self::Vni => 3,
            Self::Viqr => 4,
            Self::SpellCheck => 5,
            Self::ModernTone => 6,
            Self::Macros => 7,
            Self::Startup => 8,
            Self::OpenConfig => 9,
            Self::About => 10,
            Self::Exit => 11,
        }
    }

    /// The command with menu id `id`.
    pub fn from_id(id: u16) -> Option<Self> {
        Self::ALL.into_iter().find(|command| command.id() == id)
    }

    /// The label of the command.
    const fn label(self) -> &'static str {
        match self {
            Self::VietnameseMode => MENU_VIETNAMESE_MODE,
            Self::Telex => MENU_TELEX,
            Self::Vni => MENU_VNI,
            Self::Viqr => MENU_VIQR,
            Self::SpellCheck => MENU_SPELL_CHECK,
            Self::ModernTone => MENU_MODERN_TONE,
            Self::Macros => MENU_MACROS,
            Self::Startup => MENU_STARTUP,
            Self::OpenConfig => MENU_OPEN_CONFIG,
            Self::About => MENU_ABOUT,
            Self::Exit => MENU_EXIT,
        }
    }

    fn item(self, checked: bool) -> MenuItem<'static> {
        MenuItem::command(self.id(), self.label()).checked(checked)
    }
}

/// `runtime` after `change`, as `crate::settings::Settings` builds the snapshot of the changed
/// document (only its revision differs, which the commit sets): the mode in both
/// `vietnamese_mode` and `options.enabled`, a user-defined method without its key map on the
/// default method, and a startup mode, which the hook does not see, changing nothing.
pub fn with_change(runtime: &RuntimeSnapshot, change: Change) -> RuntimeSnapshot {
    let mut next = runtime.clone();
    let options = &mut next.options;
    match change {
        Change::VietnameseMode(on) => {
            next.vietnamese_mode = on;
            options.enabled = on;
        }
        Change::InputMethod(method) => {
            let usable = method != InputMethod::UserDefined
                || next
                    .keymaps
                    .iter()
                    .any(|map| Some(map.id.as_str()) == options.user_keymap_id.as_deref());
            options.input_method = if usable {
                method
            } else {
                EngineOptions::defaults_v1().input_method
            };
        }
        Change::SpellCheck(on) => options.spell_check = on,
        Change::ModernTone(on) => options.modern_tone = on,
        Change::Macros(on) => options.macros_enabled = on,
        Change::Startup(_) => {}
    }
    next
}

/// Whether the hook would run the same with `a` as with `b`: they differ at most in their
/// revision.
fn same_for_hook(a: &RuntimeSnapshot, b: &RuntimeSnapshot) -> bool {
    a.options == b.options
        && a.keymaps == b.keymaps
        && a.hotkeys == b.hotkeys
        && a.compatibility == b.compatibility
        && a.vietnamese_mode == b.vietnamese_mode
        && a.diagnostics_enabled == b.diagnostics_enabled
}

/// What the run mode hands the tray when it starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayStart {
    /// The runtime snapshot in effect (`Settings::runtime_snapshot`), which the hook started
    /// with: the saved mode, method, and options.
    pub runtime: RuntimeSnapshot,
    /// `[system] startup` in effect.
    pub startup: StartupMode,
    /// Where the configuration lives.
    pub location: Location,
    /// The path of `raumakey.toml`; `None` for [`Location::Unavailable`].
    pub config_path: Option<PathBuf>,
    /// The state of the files at the start, which the start notices told already.
    pub status: Status,
    /// Shown once the icon is added: the first run, the problems found at the start, a
    /// Startup shortcut that could not be repaired.
    pub notices: Vec<Notice>,
}

/// Something another thread left in the tray's inbox (then it posts [`WAKE`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The toggle hotkey changed the mode in the hook (true: Vietnamese), from the hook
    /// observer.
    Hotkey(bool),
    /// An answer of the settings worker.
    Worker(Answer),
}

/// What the glue does for a message, a menu choice, an event, or an answer to a question.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Add the icon showing this state; if it was added before, add it again
    /// (`TaskbarCreated`).
    ShowIcon(IconState),
    /// Show this state on the icon: its image and tooltip.
    UpdateIcon(IconState),
    /// Open the tray menu ([`Tray::menu`]) at the anchor point, in screen coordinates, and
    /// pass the chosen command to [`Tray::menu_choice`].
    OpenMenu {
        /// Horizontal screen coordinate.
        x: i32,
        /// Vertical screen coordinate.
        y: i32,
    },
    /// Show a message box and wait for it; when it asks a question and the answer is yes,
    /// pass the question to [`Tray::answered_yes`].
    Notify(Notice),
    /// Hand the mode to the typing hook (`TypingUpdate::Mode`; true: Vietnamese).
    HookMode(bool),
    /// Hand this runtime snapshot to the typing hook (`TypingUpdate::Runtime`); it carries the
    /// mode the icon shows.
    HookRuntime(Box<RuntimeSnapshot>),
    /// Send `command`, numbered `seq`, to the settings worker.
    Worker {
        /// The command's number; its answer carries it ([`Answer::seq`]).
        seq: u64,
        /// The command.
        command: WorkerCommand,
    },
    /// Stop the typing hook (both hooks are removed; typing is plain at once).
    StopTyping,
    /// Remove the icon.
    RemoveIcon,
    /// Let the worker finish the commands sent (bounded) and end the message loop with
    /// [`LOOP_EXIT_CODE`].
    EndLoop,
}

/// The answer to one window message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reply {
    /// `Some(result)`: the window procedure returns `result`. `None`: the message gets the
    /// default processing of `DefWindowProcW`.
    pub result: Option<isize>,
    /// What the glue does, in order, before it returns.
    pub effects: Vec<Effect>,
}

impl Reply {
    /// A message the tray handles: the window procedure returns 0.
    fn handled(effects: Vec<Effect>) -> Self {
        Self {
            result: Some(0),
            effects,
        }
    }

    /// A message left to `DefWindowProcW`.
    fn default_processing() -> Self {
        Self {
            result: None,
            effects: Vec::new(),
        }
    }
}

/// Where the tray is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Phase {
    /// The window exists; the message loop has not handled [`START`] yet.
    Starting,
    /// The icon was added (or will be once the shell exists).
    Running,
    /// Exit happened: the hook stopped, the icon was removed, and the loop ends.
    Exiting,
}

/// The state of the tray (see the [module documentation](self)).
#[derive(Clone)]
pub struct Tray {
    runtime: RuntimeSnapshot,
    startup: StartupMode,
    location: Location,
    config_path: Option<PathBuf>,
    /// The state of the files last told to the user.
    reported: Status,
    /// Shown at [`START`].
    notices: Vec<Notice>,
    /// The registered `TaskbarCreated` message; `None` when registration failed.
    taskbar_created: Option<u32>,
    phase: Phase,
    /// The number of the last worker command sent.
    sent: u64,
    /// The number of the poll on its way, if one is.
    polling: Option<u64>,
    /// The kind of the last save error told; cleared by a save that works.
    save_error: Option<ConfigError>,
}

impl fmt::Debug for Tray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tray")
            .field("mode", &self.mode())
            .field("method", &self.runtime.options.input_method)
            .field("config_revision", &self.runtime.config_revision)
            .field("startup", &self.startup)
            .field("location", &self.location)
            .field("phase", &self.phase)
            .field("sent", &self.sent)
            .field("polling", &self.polling)
            .finish_non_exhaustive()
    }
}

impl Tray {
    /// A tray before its start. `taskbar_created` is the registered `TaskbarCreated` message
    /// (`raumakey_windows::shell::taskbar_created_message`); a number outside the registered
    /// range (`0xC000..=0xFFFF`), such as the 0 of a failed registration, is never taken for
    /// it.
    pub fn new(start: TrayStart, taskbar_created: u32) -> Self {
        let registered =
            (REGISTERED_MESSAGE_FIRST..=REGISTERED_MESSAGE_LAST).contains(&taskbar_created);
        Self {
            runtime: start.runtime,
            startup: start.startup,
            location: start.location,
            config_path: start.config_path,
            reported: start.status,
            notices: start.notices,
            taskbar_created: registered.then_some(taskbar_created),
            phase: Phase::Starting,
            sent: 0,
            polling: None,
            save_error: None,
        }
    }

    /// The mode the icon shows.
    pub fn mode(&self) -> InputMode {
        InputMode::from_vietnamese(self.runtime.vietnamese_mode)
    }

    /// What the icon shows.
    pub fn icon(&self) -> IconState {
        IconState {
            mode: self.mode(),
            method: self.runtime.options.input_method,
        }
    }

    /// The runtime snapshot in effect, as the hook last got it from the tray.
    pub fn runtime(&self) -> &RuntimeSnapshot {
        &self.runtime
    }

    /// The startup mode the menu shows.
    pub fn startup(&self) -> StartupMode {
        self.startup
    }

    /// The tray menu for the settings in effect (D42).
    pub fn menu(&self) -> Vec<MenuItem<'static>> {
        let options = &self.runtime.options;
        let method = options.input_method;
        vec![
            TrayCommand::VietnameseMode.item(self.runtime.vietnamese_mode),
            MenuItem::Separator,
            TrayCommand::Telex.item(method == InputMethod::Telex),
            TrayCommand::Vni.item(method == InputMethod::Vni),
            TrayCommand::Viqr.item(method == InputMethod::Viqr),
            MenuItem::Separator,
            TrayCommand::SpellCheck.item(options.spell_check),
            TrayCommand::ModernTone.item(options.modern_tone),
            TrayCommand::Macros.item(options.macros_enabled),
            MenuItem::Separator,
            TrayCommand::Startup.item(self.startup != StartupMode::None),
            TrayCommand::OpenConfig.item(false),
            TrayCommand::About.item(false),
            MenuItem::Separator,
            TrayCommand::Exit.item(false),
        ]
    }

    /// The messages that the (non-elevated) shell sends to the hidden window: `TaskbarCreated`
    /// and the icon's callback. An elevated controller lets them through User Interface
    /// Privilege Isolation (`Window::allow_message_from_lower_integrity`); a non-elevated one
    /// does not need to.
    pub fn shell_messages(&self) -> Vec<u32> {
        self.taskbar_created
            .into_iter()
            .chain([TRAY_CALLBACK.id()])
            .collect()
    }

    /// True after [`START`] and before an exit: only then do menu choices, events, and
    /// answers act, so the glue drains the inbox only then.
    pub fn is_running(&self) -> bool {
        self.phase == Phase::Running
    }

    /// True while the message loop runs and nothing has ended it yet: if the window goes away
    /// now (destroyed by `DefWindowProcW` during a modal loop), the glue must stop the hook
    /// and the worker and end the loop itself. False before [`START`] (no loop runs yet) and
    /// after an exit (it ended it).
    pub fn ends_loop_when_dropped(&self) -> bool {
        self.is_running()
    }

    /// The answer to window message `message` with its parameters (see the [module
    /// documentation](self)).
    pub fn on_message(&mut self, message: u32, wparam: usize, lparam: isize) -> Reply {
        if message == START.id() {
            return Reply::handled(self.start());
        }
        if message == TRAY_CALLBACK.id() {
            return Reply::handled(self.icon_message(wparam, lparam));
        }
        if message == WAKE.id() {
            return Reply::handled(Vec::new());
        }
        if self.taskbar_created == Some(message) {
            let effects = if self.is_running() {
                vec![Effect::ShowIcon(self.icon())]
            } else {
                Vec::new()
            };
            return Reply::handled(effects);
        }
        match message {
            WM_TIMER if wparam == POLL_TIMER => Reply::handled(self.poll()),
            WM_CLOSE => Reply::handled(self.exit()),
            WM_ENDSESSION if wparam != 0 => Reply::handled(self.exit()),
            _ => Reply::default_processing(),
        }
    }

    /// The effects of the command chosen in the menu (`None`: the menu was dismissed).
    pub fn menu_choice(&mut self, chosen: Option<u16>) -> Vec<Effect> {
        if !self.is_running() {
            return Vec::new();
        }
        let Some(command) = chosen.and_then(TrayCommand::from_id) else {
            return Vec::new();
        };
        let options = &self.runtime.options;
        match command {
            TrayCommand::VietnameseMode => self.set_mode(self.mode().toggled()),
            TrayCommand::Telex => self.choose_method(InputMethod::Telex),
            TrayCommand::Vni => self.choose_method(InputMethod::Vni),
            TrayCommand::Viqr => self.choose_method(InputMethod::Viqr),
            TrayCommand::SpellCheck => self.change(Change::SpellCheck(!options.spell_check)),
            TrayCommand::ModernTone => self.change(Change::ModernTone(!options.modern_tone)),
            TrayCommand::Macros => self.change(Change::Macros(!options.macros_enabled)),
            TrayCommand::Startup => self.switch_startup(),
            TrayCommand::OpenConfig => vec![self.command(WorkerCommand::OpenConfig)],
            TrayCommand::About => vec![Effect::Notify(about_notice(
                self.location,
                self.config_path.as_deref(),
            ))],
            TrayCommand::Exit => self.exit(),
        }
    }

    /// The effects of `event`, drained from the inbox (see the [module
    /// documentation](self)).
    pub fn on_event(&mut self, event: Event) -> Vec<Effect> {
        if !self.is_running() {
            return Vec::new();
        }
        match event {
            Event::Hotkey(vietnamese) => self.set_mode(InputMode::from_vietnamese(vietnamese)),
            Event::Worker(answer) => self.on_answer(answer),
        }
    }

    /// The user answered yes to `question` of a [`Notice`].
    pub fn answered_yes(&mut self, question: Question) -> Vec<Effect> {
        if !self.is_running() {
            return Vec::new();
        }
        match question {
            Question::Recover => vec![self.command(WorkerCommand::Recover)],
        }
    }

    /// Numbers `command` for the worker.
    fn command(&mut self, command: WorkerCommand) -> Effect {
        self.sent += 1;
        if command == WorkerCommand::Poll {
            self.polling = Some(self.sent);
        }
        Effect::Worker {
            seq: self.sent,
            command,
        }
    }

    fn start(&mut self) -> Vec<Effect> {
        if self.phase != Phase::Starting {
            return Vec::new();
        }
        self.phase = Phase::Running;
        let mut effects = vec![Effect::ShowIcon(self.icon())];
        effects.extend(self.notices.drain(..).map(Effect::Notify));
        effects
    }

    fn icon_message(&mut self, wparam: usize, lparam: isize) -> Vec<Effect> {
        let message = decode_notify_icon_message(wparam, lparam);
        if !self.is_running() || message.icon_id != TRAY_ICON_ID {
            return Vec::new();
        }
        match message.event {
            NotifyIconEvent::Select | NotifyIconEvent::KeySelect => {
                self.set_mode(self.mode().toggled())
            }
            NotifyIconEvent::ContextMenu { x, y } => vec![Effect::OpenMenu { x, y }],
            NotifyIconEvent::Other(_) => Vec::new(),
        }
    }

    fn poll(&mut self) -> Vec<Effect> {
        if !self.is_running() || self.polling.is_some() {
            return Vec::new();
        }
        vec![self.command(WorkerCommand::Poll)]
    }

    /// The mode becomes `mode`: the icon, the hook (its mode only), and the file.
    fn set_mode(&mut self, mode: InputMode) -> Vec<Effect> {
        let vietnamese = mode.is_vietnamese();
        let before = self.icon();
        self.runtime = with_change(&self.runtime, Change::VietnameseMode(vietnamese));
        let mut effects = Vec::new();
        if self.icon() != before {
            effects.push(Effect::UpdateIcon(self.icon()));
        }
        effects.push(Effect::HookMode(vietnamese));
        effects.push(self.command(WorkerCommand::Apply(Change::VietnameseMode(vietnamese))));
        effects
    }

    fn choose_method(&mut self, method: InputMethod) -> Vec<Effect> {
        if self.runtime.options.input_method == method {
            return Vec::new();
        }
        self.change(Change::InputMethod(method))
    }

    /// A change of the runtime snapshot other than the mode: the icon when it shows it, the
    /// hook (the whole snapshot, with the current mode), and the file.
    fn change(&mut self, change: Change) -> Vec<Effect> {
        let before = self.icon();
        self.runtime = with_change(&self.runtime, change);
        let mut effects = Vec::new();
        if self.icon() != before {
            effects.push(Effect::UpdateIcon(self.icon()));
        }
        effects.push(Effect::HookRuntime(Box::new(self.runtime.clone())));
        effects.push(self.command(WorkerCommand::Apply(change)));
        effects
    }

    fn switch_startup(&mut self) -> Vec<Effect> {
        match self.startup {
            StartupMode::ElevatedTask => vec![Effect::Notify(elevated_task_notice())],
            StartupMode::Shortcut => {
                self.startup = StartupMode::None;
                vec![self.command(WorkerCommand::Startup(false))]
            }
            StartupMode::None => {
                self.startup = StartupMode::Shortcut;
                vec![self.command(WorkerCommand::Startup(true))]
            }
        }
    }

    fn exit(&mut self) -> Vec<Effect> {
        if self.phase == Phase::Exiting {
            return Vec::new();
        }
        self.phase = Phase::Exiting;
        vec![Effect::StopTyping, Effect::RemoveIcon, Effect::EndLoop]
    }

    fn on_answer(&mut self, answer: Answer) -> Vec<Effect> {
        if self.polling == Some(answer.seq) {
            self.polling = None;
        }
        let mut effects = Vec::new();
        if answer.seq == self.sent
            && let Some(state) = answer.state
        {
            effects.extend(self.adopt(*state));
        }
        let path = self.config_path.clone();
        let path = path.as_deref();
        match answer.outcome {
            Outcome::Applied { result: Ok(()), .. } | Outcome::Startup { result: Ok(()), .. } => {
                self.saved();
            }
            Outcome::Applied {
                result: Err(error), ..
            }
            | Outcome::Startup {
                result: Err(StartupError::Save(error)),
                ..
            } => effects.extend(self.save_failed(error)),
            Outcome::Startup {
                on,
                result: Err(error @ StartupError::Shortcut(_)),
            } => effects.push(Effect::Notify(startup_error_notice(path, on, error))),
            Outcome::Polled(Some(status)) => {
                if let Some(notice) = status_notice(path, &self.reported, &status) {
                    effects.push(Effect::Notify(notice));
                }
                self.reported = status;
            }
            Outcome::Recovered(result) => {
                if matches!(
                    result,
                    Ok(Recovery::Adopted | Recovery::Replaced { saved: Ok(()), .. })
                ) {
                    self.saved();
                }
                effects.extend(recovery_notice(path, &result).map(Effect::Notify));
            }
            Outcome::Opened(Err(error)) => {
                effects.push(Effect::Notify(open_error_notice(path, error)));
            }
            Outcome::Polled(None) | Outcome::Opened(Ok(())) | Outcome::Failed => {}
        }
        effects
    }

    /// Takes over the settings in effect that the worker answered.
    fn adopt(&mut self, state: SettingsState) -> Vec<Effect> {
        let before = self.icon();
        let changed = !same_for_hook(&self.runtime, &state.runtime);
        self.runtime = state.runtime;
        self.startup = state.startup;
        let mut effects = Vec::new();
        if self.icon() != before {
            effects.push(Effect::UpdateIcon(self.icon()));
        }
        if changed {
            effects.push(Effect::HookRuntime(Box::new(self.runtime.clone())));
        }
        effects
    }

    /// `raumakey.toml` was written: a later failure is told again, and so is a later damage
    /// of the file, which the worker's own write does not show to the poll.
    fn saved(&mut self) {
        self.save_error = None;
        self.reported.config = DocumentStatus::Loaded { issues: Vec::new() };
    }

    /// A change could not be saved: told once per kind of error until a save works.
    fn save_failed(&mut self, error: ConfigError) -> Option<Effect> {
        let same = self
            .save_error
            .is_some_and(|told| core::mem::discriminant(&told) == core::mem::discriminant(&error));
        if same {
            return None;
        }
        self.save_error = Some(error);
        Some(Effect::Notify(save_error_notice(
            self.config_path.as_deref(),
            error,
        )))
    }
}
