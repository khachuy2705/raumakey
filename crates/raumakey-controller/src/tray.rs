//! The controller's notification-area icon as a pure state machine (plan §10.2; design §11;
//! matrix #58): the input mode that the icon shows, the icon and tooltip of each mode, the
//! tray menu, and what to do with each message of the hidden window. The Windows glue in
//! `crate::desktop` passes the window's messages to [`Tray::on_message`] and carries out the
//! [`Effect`]s on the real notification icon and popup menu; everything here is tested on
//! every host.
//!
//! # Mode
//!
//! The mode starts from the engine defaults v1 ([`initial_mode`]: Vietnamese). The controller
//! keeps its own mode for now: a click on the icon, the keyboard, or the menu switches it and
//! the icon ([`ICON_VIETNAMESE`], "V"; [`ICON_ENGLISH`], "E"). The TIP does not follow it yet:
//! once the IPC hub exists, the controller will publish the mode (`ModeChanged` and the
//! runtime snapshot) and the TIP will follow it; until then each TIP keeps the mode of its own
//! snapshot (defaults v1). The mode is not saved in `config.toml` either: that waits for the
//! configuration lifecycle.
//!
//! # Messages
//!
//! | Message | While running | Answer |
//! |---|---|---|
//! | [`START`] (posted once when the message loop starts) | adds the icon ([`Effect::ShowIcon`]) | 0 |
//! | [`TRAY_CALLBACK`] `NIN_SELECT` (click), `NIN_KEYSELECT` (keyboard) | switches the mode ([`Effect::UpdateIcon`]) | 0 |
//! | [`TRAY_CALLBACK`] `WM_CONTEXTMENU` (right click, Shift+F10, menu key) | opens the menu at the anchor ([`Effect::OpenMenu`]) | 0 |
//! | [`TRAY_CALLBACK`], other notifications or another icon id | nothing | 0 |
//! | `TaskbarCreated` (Explorer restarted) | adds the icon again with the current mode, without changing it (matrix #58) | 0 |
//! | `WM_CLOSE`; `WM_ENDSESSION` with a non-zero `wParam` (the session ends) | exits ([`Effect::Exit`]) | 0 |
//! | anything else, and `WM_ENDSESSION` of a cancelled end | nothing | `DefWindowProcW` |
//!
//! [`Effect::Exit`] removes the icon and ends the message loop with [`LOOP_EXIT_CODE`]; the
//! window is destroyed once the loop has ended. `WM_CLOSE` is always answered, never left to
//! `DefWindowProcW`, which would destroy the window under the running tray. Before [`START`]
//! the icon does not exist, so its messages, `TaskbarCreated`, and menu choices do nothing,
//! while `WM_CLOSE` and the end of the session still exit. After an exit every message is
//! answered without effect, so a message still queued behind it (such as a `TaskbarCreated`)
//! cannot bring the icon back.
//!
//! # Menu
//!
//! [`menu_items`]: a check item for Vietnamese mode ([`MENU_VIETNAMESE_MODE`]), a separator,
//! and Exit ([`MENU_EXIT`]). Choosing the check item switches the mode; [`Tray::menu_choice`]
//! takes the command that the menu returned.
//!
//! # Losing the window
//!
//! A modal loop that runs inside the window's handler (the tray menu) leaves the window's
//! messages to `DefWindowProcW` (see `raumakey_windows::window`), which answers a `WM_CLOSE`
//! that arrives meanwhile by destroying the window. The handler is then freed when it
//! returns, and [`Tray::ends_loop_when_dropped`] tells the glue that the loop must end anyway.
//!
//! Strings shown to the user are new Raumakey strings in Vietnamese (plan §10.2). Nothing here
//! holds user content.

use raumakey_model::options::EngineOptions;
use raumakey_windows::menu::MenuItem;
use raumakey_windows::shell::{NotifyIconEvent, decode_notify_icon_message};
use raumakey_windows::window::{
    AppMessage, REGISTERED_MESSAGE_FIRST, REGISTERED_MESSAGE_LAST, WM_CLOSE, WM_ENDSESSION,
};

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
/// Exit code of the message loop after [`Effect::Exit`].
pub const LOOP_EXIT_CODE: i32 = 0;

/// Tooltip of the icon in Vietnamese mode: "Raumakey: typing Vietnamese".
pub const TOOLTIP_VIETNAMESE: &str = "Raumakey: đang gõ tiếng Việt";
/// Tooltip of the icon in English mode: "Raumakey: typing English".
pub const TOOLTIP_ENGLISH: &str = "Raumakey: đang gõ tiếng Anh";
/// The check item of Vietnamese mode: "Type Vietnamese" (mnemonic G).
pub const MENU_VIETNAMESE_MODE: &str = "&Gõ tiếng Việt";
/// The Exit item: "Exit" (mnemonic T).
pub const MENU_EXIT: &str = "&Thoát";

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
    /// The mode of an `enabled` flag (`EngineOptions::enabled`, `[input] enabled`).
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

    /// The tooltip of the icon in this mode.
    pub const fn tooltip(self) -> &'static str {
        match self {
            Self::Vietnamese => TOOLTIP_VIETNAMESE,
            Self::English => TOOLTIP_ENGLISH,
        }
    }
}

/// The mode the controller starts in: that of the engine defaults v1, until the
/// configuration lifecycle loads `config.toml`.
pub fn initial_mode() -> InputMode {
    InputMode::from_vietnamese(EngineOptions::defaults_v1().enabled)
}

/// The commands of the tray menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrayCommand {
    /// The check item: switches Vietnamese mode.
    VietnameseMode,
    /// Exit: removes the icon and ends the controller.
    Exit,
}

impl TrayCommand {
    /// Every command.
    pub const ALL: [Self; 2] = [Self::VietnameseMode, Self::Exit];

    /// The menu command id (never 0, which a menu returns for "nothing chosen").
    pub const fn id(self) -> u16 {
        match self {
            Self::VietnameseMode => 1,
            Self::Exit => 2,
        }
    }

    /// The command with menu id `id`.
    pub const fn from_id(id: u16) -> Option<Self> {
        match id {
            1 => Some(Self::VietnameseMode),
            2 => Some(Self::Exit),
            _ => None,
        }
    }
}

/// The tray menu in `mode`: Vietnamese mode (checked when on), a separator, and Exit.
pub fn menu_items(mode: InputMode) -> [MenuItem<'static>; 3] {
    [
        MenuItem::command(TrayCommand::VietnameseMode.id(), MENU_VIETNAMESE_MODE)
            .checked(mode.is_vietnamese()),
        MenuItem::Separator,
        MenuItem::command(TrayCommand::Exit.id(), MENU_EXIT),
    ]
}

/// What the glue does for a message or a menu choice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Effect {
    /// Add the icon showing the mode; if it was added before, add it again (`TaskbarCreated`).
    ShowIcon(InputMode),
    /// Show the mode on the icon: its image and tooltip.
    UpdateIcon(InputMode),
    /// Open the tray menu ([`Tray::menu`]) at the anchor point, in screen coordinates, and
    /// pass the chosen command to [`Tray::menu_choice`].
    OpenMenu {
        /// Horizontal screen coordinate.
        x: i32,
        /// Vertical screen coordinate.
        y: i32,
    },
    /// Remove the icon and end the message loop with [`LOOP_EXIT_CODE`].
    Exit,
}

/// The answer to one window message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Reply {
    /// `Some(result)`: the window procedure returns `result`. `None`: the message gets the
    /// default processing of `DefWindowProcW`.
    pub result: Option<isize>,
    /// What the glue does before it returns.
    pub effect: Option<Effect>,
}

impl Reply {
    /// A message the tray handles: the window procedure returns 0.
    const fn handled(effect: Option<Effect>) -> Self {
        Self {
            result: Some(0),
            effect,
        }
    }

    /// A message left to `DefWindowProcW`.
    const DEFAULT: Self = Self {
        result: None,
        effect: None,
    };
}

/// Where the tray is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Phase {
    /// The window exists; the message loop has not handled [`START`] yet.
    Starting,
    /// The icon was added (or will be once the shell exists).
    Running,
    /// Exit happened: the icon was removed and the loop ends.
    Exiting,
}

/// The state of the tray: its mode and where it is in its life.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tray {
    mode: InputMode,
    /// The registered `TaskbarCreated` message; `None` when registration failed.
    taskbar_created: Option<u32>,
    phase: Phase,
}

impl Tray {
    /// A tray in `mode` before its start. `taskbar_created` is the registered
    /// `TaskbarCreated` message (`raumakey_windows::shell::taskbar_created_message`); a number
    /// outside the registered range (`0xC000..=0xFFFF`), such as the 0 of a failed
    /// registration, is never taken for it.
    pub fn new(mode: InputMode, taskbar_created: u32) -> Self {
        let registered =
            (REGISTERED_MESSAGE_FIRST..=REGISTERED_MESSAGE_LAST).contains(&taskbar_created);
        Self {
            mode,
            taskbar_created: registered.then_some(taskbar_created),
            phase: Phase::Starting,
        }
    }

    /// The current mode.
    pub fn mode(&self) -> InputMode {
        self.mode
    }

    /// The tray menu for the current mode.
    pub fn menu(&self) -> [MenuItem<'static>; 3] {
        menu_items(self.mode)
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

    /// True while the message loop runs and nothing has ended it yet: if the window goes away
    /// now (destroyed by `DefWindowProcW` during a modal loop), the glue must end the loop
    /// itself. False before [`START`] (no loop runs yet) and after an exit (it ended it).
    pub fn ends_loop_when_dropped(&self) -> bool {
        self.phase == Phase::Running
    }

    /// The answer to window message `message` with its parameters (see the module
    /// documentation).
    pub fn on_message(&mut self, message: u32, wparam: usize, lparam: isize) -> Reply {
        if message == START.id() {
            return Reply::handled(self.start());
        }
        if message == TRAY_CALLBACK.id() {
            return Reply::handled(self.icon_message(wparam, lparam));
        }
        if self.taskbar_created == Some(message) {
            return Reply::handled(self.running().then_some(Effect::ShowIcon(self.mode)));
        }
        match message {
            WM_CLOSE => Reply::handled(self.exit()),
            WM_ENDSESSION if wparam != 0 => Reply::handled(self.exit()),
            _ => Reply::DEFAULT,
        }
    }

    /// The effect of the command chosen in the menu (`None`: the menu was dismissed).
    pub fn menu_choice(&mut self, chosen: Option<u16>) -> Option<Effect> {
        if !self.running() {
            return None;
        }
        match chosen.and_then(TrayCommand::from_id)? {
            TrayCommand::VietnameseMode => Some(self.toggle()),
            TrayCommand::Exit => self.exit(),
        }
    }

    fn running(&self) -> bool {
        self.phase == Phase::Running
    }

    fn start(&mut self) -> Option<Effect> {
        if self.phase != Phase::Starting {
            return None;
        }
        self.phase = Phase::Running;
        Some(Effect::ShowIcon(self.mode))
    }

    fn icon_message(&mut self, wparam: usize, lparam: isize) -> Option<Effect> {
        let message = decode_notify_icon_message(wparam, lparam);
        if !self.running() || message.icon_id != TRAY_ICON_ID {
            return None;
        }
        match message.event {
            NotifyIconEvent::Select | NotifyIconEvent::KeySelect => Some(self.toggle()),
            NotifyIconEvent::ContextMenu { x, y } => Some(Effect::OpenMenu { x, y }),
            NotifyIconEvent::Other(_) => None,
        }
    }

    fn toggle(&mut self) -> Effect {
        self.mode = self.mode.toggled();
        Effect::UpdateIcon(self.mode)
    }

    fn exit(&mut self) -> Option<Effect> {
        if self.phase == Phase::Exiting {
            return None;
        }
        self.phase = Phase::Exiting;
        Some(Effect::Exit)
    }
}
