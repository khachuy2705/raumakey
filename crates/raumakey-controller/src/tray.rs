//! The controller's notification-area icon as a pure state machine (plan §10.2, D42–D47;
//! design §21.5, §22; matrix #58): the settings that the icon, the menu, and the settings panel
//! show, the icon and tooltip, the tray menu, and what to do with each message of the hidden
//! window, each menu choice, each mode change of the hotkey, each request of the panel, each
//! answer of the settings worker, and each application that comes to the foreground. The
//! Windows glue in `crate::desktop` passes them in and carries out the [`Effect`]s on the real
//! notification icon, popup menu, message boxes, settings panel, typing hook, and worker
//! thread; everything here is tested on every host.
//!
//! # Settings
//!
//! The tray starts from what the run mode read from `raumakey.toml` ([`TrayStart`]): the
//! runtime snapshot in effect (mode, input method, options), the startup mode, and whether the
//! panel opens at start. Each choice changes them at once: the icon, menu, and panel follow,
//! the typing hook gets the new mode
//! ([`Effect::HookMode`]) or the new runtime snapshot ([`Effect::HookRuntime`], built by
//! [`with_change`] exactly as `crate::settings::Settings` builds it after the change), and the
//! settings worker commits the change ([`Effect::Worker`] with
//! [`WorkerCommand::Apply`]). A commit that fails is shown once per kind of error
//! (`crate::notice::save_error_notice`; for the start with Windows, the text of the shortcut),
//! the change stays in effect, and the worker retries it at every poll and before it ends. An
//! answer that wrote nothing ([`Saved::Unchanged`]) is no save that worked.
//!
//! **Notices** ([`Effect::Notify`]) never wait: the glue shows each message box on a thread
//! of its own, and a "yes" to its question comes back through the inbox
//! ([`Event::Answered`]), so the tray's thread keeps handling its messages while a box is
//! open. An exit closes the open boxes ([`Effect::DismissNotices`]) and never waits for them.
//!
//! **Hotkey changes** drained together count as their last one ([`coalesce`]); those left in
//! the inbox when the hook stops at an exit are saved ([`Tray::flush_hotkeys`]).
//!
//! **From the temporary folder** (an exe run from inside a ZIP, [`TrayStart::temporary_exe`])
//! "Khởi động cùng Windows" makes no shortcut and asks the user to extract Raumakey first.
//!
//! **Every runtime snapshot handed to the hook carries the mode in effect** (the one the icon
//! shows, unless an excluded application is in front). Applying a runtime snapshot sets the
//! hook's mode to the one it carries, so the tray is the only one that hands runtime
//! snapshots to the hook. When the hotkey changed the mode in the hook ([`Event::Hotkey`],
//! from the hook observer), the tray takes the mode over, saves it, and hands it back to the
//! hook ([`Effect::HookMode`]): a snapshot handed to the hook before the tray learned of the
//! hotkey (with the mode in effect then) is undone by it.
//!
//! **Answers of the worker.** Each command is numbered ([`Effect::Worker`] `seq`). An answer
//! that carries the settings in effect ([`Answer::state`]) is taken over only when it answers
//! the last command sent: an older answer does not know of a change still on its way, and the
//! answer to that change brings everything (`crate::worker`). Taking settings over refreshes
//! the icon and the menu, and hands the hook the new snapshot unless it differs from the one
//! in effect only in its revision. This is how a hand edit reaches the hook.
//!
//! **Macro list saves.** When the macro list editor (D44, `crate::macro_dialog`) wrote
//! `raumakey-macros.toml` ([`Event::MacrosSaved`]), the worker polls at once, so the hook gets
//! the new macros now rather than at the next tick. While a poll is on its way (it may have
//! read the file before the save), the worker polls once more as soon as that poll answered;
//! saves meanwhile ask for that one poll only.
//!
//! # Messages
//!
//! | Message | While running | Answer |
//! |---|---|---|
//! | [`START`] (posted once when the message loop starts) | adds the icon ([`Effect::ShowIcon`]), reads the application in front ([`Effect::ReadForeground`]), opens the panel when it opens at start ([`Effect::OpenPanel`]), then shows the start notices | 0 |
//! | [`TRAY_CALLBACK`] `NIN_SELECT` (click) | switches the mode at once, or completes a double-click (see [the icon](#the-icon-d42-d46-d47)) | 0 |
//! | [`TRAY_CALLBACK`] `WM_LBUTTONDBLCLK` (double-click) | switches back the mode its first click switched, and opens the panel or brings it to the front ([`Effect::OpenPanel`]) | 0 |
//! | [`TRAY_CALLBACK`] `WM_LBUTTONDOWN` (a press) | nothing: a click begins | 0 |
//! | [`TRAY_CALLBACK`] `NIN_KEYSELECT` (Space or Enter on the selected icon) | switches the mode | 0 |
//! | [`TRAY_CALLBACK`] `WM_CONTEXTMENU` (right click, Shift+F10, menu key) | opens the menu at the anchor ([`Effect::OpenMenu`]) | 0 |
//! | [`TRAY_CALLBACK`], other notifications or another icon id | nothing | 0 |
//! | `WM_TIMER` of [`POLL_TIMER`] (every [`POLL_INTERVAL_MS`]) | makes sure the icon is shown ([`Effect::EnsureIcon`]: an add or a change that failed is tried again), reads the application in front again ([`Effect::ReadForeground`]), and asks the worker to poll unless a poll is on its way | 0 |
//! | [`WAKE`] (the worker, the hook observer, a message box, the panel, or the macro list editor left something in the inbox) | nothing: the glue drains the inbox ([`Tray::on_event`]) | 0 |
//! | [`FOREGROUND`] (the foreground watcher; `wParam`: the window that came to the front) | reads the application of that window ([`Effect::ReadForeground`]) | 0 |
//! | `TaskbarCreated` (Explorer restarted) | adds the icon again with the current state (matrix #58) | 0 |
//! | `Raumakey.<sidhash>.show-panel` ([`SHOW_PANEL_COMPONENT`], a second start, D45) | opens the panel or brings it to the front ([`Effect::OpenPanel`]) | 0 |
//! | `WM_CLOSE`; `WM_ENDSESSION` with a non-zero `wParam` (the session ends) | exits | 0 |
//! | anything else, and `WM_ENDSESSION` of a cancelled end | nothing | `DefWindowProcW` |
//!
//! The two registered messages are numbered at run time ([`RegisteredMessages`]); one whose
//! registration failed is never matched, and the tray runs without it.
//!
//! Exiting is, in order, [`Effect::StopTyping`] (typing is plain at once),
//! [`Effect::DismissNotices`], [`Effect::ClosePanel`], [`Effect::CloseMacroList`] (none of
//! them waits; the editor closes without saving, by design), [`Effect::FlushHotkeys`]
//! (hotkey changes left in the inbox are saved),
//! [`Effect::FinishWorker`] (the worker runs what was sent and saves what is unsaved, for a
//! bounded time), [`Effect::RemoveIcon`], and [`Effect::EndLoop`] (the message loop ends with
//! [`LOOP_EXIT_CODE`]), all before the window procedure returns: after `WM_ENDSESSION` the
//! process may end at any moment. `WM_CLOSE` is always answered, never left to
//! `DefWindowProcW`, which would destroy the window under the running tray. Before [`START`]
//! the icon does not exist, so its messages, the timer, `TaskbarCreated`, the show-panel
//! request, menu choices, and events do nothing, while `WM_CLOSE` and the end of the session
//! still exit. After an exit every message, choice, and event is answered without effect, so
//! a message still queued behind it (such as a `TaskbarCreated`) cannot bring the icon back.
//!
//! The glue hands every message over with a clock in milliseconds ([`Tray::on_message_at`]:
//! `GetTickCount`, which wraps around after 49.7 days, so times are compared by their
//! difference); only the icon's messages use it.
//!
//! # The icon (D42, D46, D47)
//!
//! The image shows the mode in effect, V or E, and the tooltip names it with the input method
//! ([`IconState`]); while an excluded application is in front, the image shows E and the
//! tooltip names the application ([`TOOLTIP_EXCLUDED_BEFORE`], its executable,
//! [`TOOLTIP_EXCLUDED_AFTER`]; a long name loses characters in its middle, so the tooltip
//! keeps within the shell's 127 units).
//!
//! **Click and double-click (D46).** A click switches the mode at once, at its `NIN_SELECT`:
//! an input method must switch before the user types, so nothing waits for a second click.
//! The tray remembers when the click came and the mode before it. A double-click opens the
//! panel as "Bảng cấu hình…" does and leaves the mode as it was before its first click: that
//! mode comes back the way of every mode change (the icon, [`Effect::HookMode`], the worker;
//! nothing when it is back already, after the hotkey), then [`Effect::OpenPanel`] follows. The
//! shell reports a double-click as press, release, select, `WM_LBUTTONDBLCLK`, release, and,
//! depending on the version of Windows, a second select:
//!
//! - `WM_LBUTTONDBLCLK` is the double-click. The mode is restored when the remembered click
//!   came within the double-click time before it ([`Tray::with_double_click_ms`]: the user's
//!   `GetDoubleClickTime`, [`DEFAULT_DOUBLE_CLICK_MS`] until the glue sets it); the panel opens
//!   in any case.
//! - A select within the double-click time after a `WM_LBUTTONDBLCLK` is its second click and
//!   is swallowed, once, unless a press (`WM_LBUTTONDOWN`) came between them: a click that
//!   follows is never lost, whatever the shell sends.
//! - For a shell that sends no `WM_LBUTTONDBLCLK`, a select within the double-click time of
//!   the remembered click is the double-click.
//!
//! A double-click takes both its clicks: the click after it is a click again. A selection with
//! the keyboard (`NIN_KEYSELECT`) switches the mode once; it and the menu end a double-click
//! that a click may have begun.
//!
//! **Excluded applications (D47).** `raumakey.toml` lists applications in which Raumakey types
//! English only (`[exclusions]`: Remote Desktop Connection and the like). The settings put one
//! rule per application, with Vietnamese and macros off, at the head of the compatibility
//! rules of the runtime snapshot, and the typing hook types English there. The tray shows it.
//! The glue reads the application in front ([`ForegroundApp`]: the executable of the window's
//! process and the window's class) for [`Effect::ReadForeground`]: at [`START`], whenever the
//! foreground watcher reports another window ([`FOREGROUND`]), and at every tick of the poll
//! timer, since a message posted while the menu's modal loop runs is lost
//! (`raumakey_windows::window`); it passes the result to [`Tray::on_foreground`]. The
//! application is excluded when the compatibility rules of the runtime snapshot in effect
//! forbid Vietnamese in it (`raumakey_policy::CompatibilityPolicy`, for its executable and
//! class, never as a password field); this is worked out again only when the application in
//! front or the rules change. An application that cannot be read (no window, one that is gone,
//! a process that cannot be queried) is not excluded.
//!
//! While an excluded application is in front, only the icon changes: the mode in effect, the
//! menu's check, the panel, and the hook's mode stay. The hotkey, a click, the menu, and the
//! panel switch the mode in effect (saved, handed to the hook) while the icon stays E, and
//! the icon shows the new mode once the application leaves. A click on the icon brings the
//! taskbar to the front first, so the icon shows the mode in effect when the click acts. New
//! rules taken over from the worker (a hand edit of `[exclusions]`) apply to the application
//! in front at once.
//!
//! # Settings panel (D43, D45)
//!
//! The panel ([`crate::panel`]) runs on a thread of its own; the tray is the only source of its
//! data ([`Tray::panel_state`]). It opens from "Bảng cấu hình…", from a double-click on the
//! icon (D46), from the request of a second start, and at [`START`] when `[system]
//! show_panel_on_start` is set; while it is open, each of these shows the state in effect in it
//! and brings it to the front. Every opening is numbered
//! ([`Effect::OpenPanel`] `serial`), and only the close of the latest one
//! ([`Event::PanelClosed`]) counts, so the close of a panel that was replaced meanwhile does
//! not stop the refreshes of the new one.
//!
//! While a panel is open, the tray keeps the state it last sent: after every message, menu
//! choice, and event whose effects change what the panel shows (the hotkey, a click on the
//! icon, the menu, a hand edit taken over from the worker), [`Effect::RefreshPanel`] follows
//! the other effects; after every request of the panel ([`Event::Panel`]) it follows even
//! when nothing changed, because the panel's check boxes toggle themselves and a refused or
//! empty request must put them back. Nothing is sent while no panel is open.
//!
//! The panel's requests ([`PanelRequest`]) carry target values and go the way of the menu:
//! the mode as a click on the icon, the method as the menu (nothing for the method in
//! effect), the switches and "Mặc định" ([`Change::TypingDefaults`]) as a change of the
//! runtime snapshot (the hook gets it, the worker saves it), and the panel at start to the
//! worker only (the hook does not see it). "Khởi động cùng Windows" is absolute: on from
//! `none` makes the shortcut (from the temporary folder, the request to extract Raumakey
//! first), off from `shortcut` removes it, off from `elevated-task` explains the elevated
//! command line, and a request for the value in effect does nothing; a `Change::Startup`
//! (without the shortcut) does nothing either. "Mở file cấu hình" goes to the worker, and
//! "Danh sách gõ tắt…" opens the macro list editor of the data folder, or brings the open one
//! to the front ([`Effect::OpenMacroList`]), when there is a folder.
//!
//! # Menu (D42, D43)
//!
//! [`Tray::menu`], flat: "Bảng cấu hình…", separator, "Gõ tiếng Việt (Ctrl+Shift)" (check),
//! separator, "Telex", "VNI", "VIQR" (the method in effect is checked; none for a
//! user-defined key map), separator, "Kiểm tra chính tả", "Đặt dấu kiểu mới (oà, uý)", "Gõ tắt
//! (macro)" (checks), separator, "Khởi động cùng Windows" (check), "Mở file cấu hình", "Giới
//! thiệu", separator, "Thoát". The commands of D42 keep their ids; "Bảng cấu hình…" is 12.
//!
//! - "Bảng cấu hình…" opens the settings panel, or brings the open one to the front.
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
//! The tray menu runs a modal loop inside the window's handler, which leaves the window's
//! messages to `DefWindowProcW` (see `raumakey_windows::window`); it answers a `WM_CLOSE`
//! that arrives meanwhile by destroying the window. The handler is then freed when it
//! returns, and [`Tray::ends_loop_when_dropped`] tells the glue that the loop must end anyway
//! (after the steps of an exit). Message boxes, the panel, and the macro list editor run no
//! loop on this thread.
//!
//! Strings shown to the user are new Raumakey strings in Vietnamese (plan §10.2,
//! `crate::notice`, `crate::panel`). Nothing here holds keystrokes, document text, or macro
//! text; `Debug` of the tray shows its mode, method, startup mode, whether the panel is open,
//! whether an excluded application is in front, and counters.

use core::fmt;
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raumakey_config::{ConfigError, StartupMode};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_policy::CompatibilityPolicy;
use raumakey_windows::menu::MenuItem;
use raumakey_windows::shell::{NotifyIconEvent, TOOLTIP_CAPACITY, decode_notify_icon_message};
use raumakey_windows::window::{
    AppMessage, REGISTERED_MESSAGE_FIRST, REGISTERED_MESSAGE_LAST, WM_CLOSE, WM_ENDSESSION,
    WM_TIMER,
};

use crate::notice::{
    Notice, Question, about_notice, elevated_task_notice, open_error_notice, recovery_notice,
    save_error_notice, startup_error_notice, status_notice, temporary_exe_notice,
};
use crate::panel::{PanelRequest, PanelState};
use crate::settings::{Change, DocumentStatus, Location, Recovery, Status};
use crate::worker::{
    Answer, Outcome, Saved, SettingsState, ShortcutSync, StartupError, WorkerCommand,
};

// `ICON_APP`, `ICON_VIETNAMESE`, and `ICON_ENGLISH`: the icon groups that `build.rs` embeds,
// with the ids that `raumakey-resources` (a build-dependency only) assigns.
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
/// Posted to the hidden window by the foreground watcher
/// (`raumakey_windows::foreground::ForegroundWatcher`) when another window came to the
/// foreground; `wParam` is that window's handle value (`WM_APP + 4`).
pub const FOREGROUND: AppMessage = app_message(4);
/// Id of the window timer that asks the worker to poll the configuration files.
pub const POLL_TIMER: usize = 1;
/// Interval of [`POLL_TIMER`]: a hand edit is picked up within about two seconds (D40).
pub const POLL_INTERVAL_MS: u32 = 2_000;
/// Exit code of the message loop after an exit.
pub const LOOP_EXIT_CODE: i32 = 0;
/// Component of the registered message `Raumakey.<sidhash>.show-panel`, which a second start
/// posts to every top-level window to have the running controller show its settings panel
/// (D45; `raumakey_windows::window::register_user_message`).
pub const SHOW_PANEL_COMPONENT: &str = "show-panel";
/// The double-click time of a tray made by [`Tray::new`], in milliseconds: the default of
/// Windows. The glue gives the tray the user's own ([`Tray::with_double_click_ms`]).
pub const DEFAULT_DOUBLE_CLICK_MS: u32 = 500;

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
/// Tooltip while an excluded application is in front (D47), before its name: "Raumakey:
/// typing English in ".
pub const TOOLTIP_EXCLUDED_BEFORE: &str = "Raumakey: đang gõ tiếng Anh trong ";
/// Tooltip while an excluded application is in front, after its name: " (excluded
/// application)".
pub const TOOLTIP_EXCLUDED_AFTER: &str = " (ứng dụng loại trừ)";

/// Opens the settings panel: "Settings panel…" (with an ellipsis character).
pub const MENU_PANEL: &str = "Bảng cấu hình\u{2026}";
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

/// What the icon shows: its image (the mode) and its tooltip (the mode and the method, or the
/// excluded application in front).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct IconState {
    /// The mode of the image: V or E (always E while `excluded`).
    pub mode: InputMode,
    /// The input method in effect, named by the tooltip in Vietnamese mode.
    pub method: InputMethod,
    /// The excluded application in the foreground (D47), named by the tooltip.
    pub excluded: Option<ExeName>,
}

impl IconState {
    /// The icon of `mode` with `method` in effect.
    pub const fn new(mode: InputMode, method: InputMethod) -> Self {
        Self {
            mode,
            method,
            excluded: None,
        }
    }

    /// The icon while the excluded application `exe` is in front, `method` in effect: E.
    pub const fn excluded(exe: ExeName, method: InputMethod) -> Self {
        Self {
            mode: InputMode::English,
            method,
            excluded: Some(exe),
        }
    }

    /// The icon group of the image.
    pub const fn icon_id(&self) -> u16 {
        self.mode.icon_id()
    }

    /// The tooltip. With an excluded application in front, [`TOOLTIP_EXCLUDED_BEFORE`], its
    /// name, and [`TOOLTIP_EXCLUDED_AFTER`], within the shell's 127 units: a name too long
    /// for them loses characters in its middle (an ellipsis takes their place).
    pub fn tooltip(&self) -> Cow<'static, str> {
        if let Some(exe) = &self.excluded {
            let room = TOOLTIP_UNITS
                - TOOLTIP_EXCLUDED_BEFORE.encode_utf16().count()
                - TOOLTIP_EXCLUDED_AFTER.encode_utf16().count();
            let name = shortened(exe.as_str(), room);
            return Cow::Owned(format!(
                "{TOOLTIP_EXCLUDED_BEFORE}{name}{TOOLTIP_EXCLUDED_AFTER}"
            ));
        }
        Cow::Borrowed(match (self.mode, self.method) {
            (InputMode::English, _) => TOOLTIP_ENGLISH,
            (InputMode::Vietnamese, InputMethod::Telex) => TOOLTIP_TELEX,
            (InputMode::Vietnamese, InputMethod::Vni) => TOOLTIP_VNI,
            (InputMode::Vietnamese, InputMethod::Viqr) => TOOLTIP_VIQR,
            (InputMode::Vietnamese, InputMethod::UserDefined) => TOOLTIP_USER_DEFINED,
        })
    }
}

/// The UTF-16 units a tooltip may hold (`raumakey_windows::shell::TOOLTIP_CAPACITY` without
/// its NUL).
const TOOLTIP_UNITS: usize = TOOLTIP_CAPACITY - 1;

/// `text` within `units` UTF-16 units: whole when it fits, else its start and its end around
/// an ellipsis, never splitting a character.
fn shortened(text: &str, units: usize) -> Cow<'_, str> {
    if text.encode_utf16().count() <= units {
        return Cow::Borrowed(text);
    }
    let room = units.saturating_sub(1);
    let tail_room = room / 2;
    let head_room = room - tail_room;
    let mut head = String::new();
    let mut used = 0;
    for c in text.chars() {
        if used + c.len_utf16() > head_room {
            break;
        }
        used += c.len_utf16();
        head.push(c);
    }
    let mut tail = Vec::new();
    let mut used = 0;
    for c in text.chars().rev() {
        if used + c.len_utf16() > tail_room {
            break;
        }
        used += c.len_utf16();
        tail.push(c);
    }
    head.push('\u{2026}');
    head.extend(tail.into_iter().rev());
    Cow::Owned(head)
}

/// The application of the window in the foreground, as the glue read it
/// ([`Effect::ReadForeground`]): what tells whether it is excluded (D47).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ForegroundApp {
    /// The lowercase executable basename of its process.
    pub exe: ExeName,
    /// The class of the window, when it could be read.
    pub window_class: Option<String>,
}

/// The commands of the tray menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrayCommand {
    /// Opens the settings panel, or brings the open one to the front.
    Panel,
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
    pub const ALL: [Self; 12] = [
        Self::Panel,
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

    /// The menu command id (never 0, which a menu returns for "nothing chosen"). The commands
    /// of D42 keep their numbers; [`TrayCommand::Panel`] came later.
    pub const fn id(self) -> u16 {
        match self {
            Self::Panel => 12,
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
            Self::Panel => MENU_PANEL,
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
/// default method, "Mặc định" ([`Change::TypingDefaults`]) on the default method and switches
/// of [`EngineOptions::defaults_v1`] (the mode, the key map id, the normalization, and the
/// context limit stay), and the startup mode and the panel at start, which the hook does not
/// see, changing nothing.
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
        Change::FreeMarking(on) => options.free_marking = on,
        Change::AutoRestore(on) => options.auto_restore_invalid = on,
        Change::Macros(on) => options.macros_enabled = on,
        Change::MacrosWhenEnglish(on) => options.macros_when_disabled = on,
        Change::MacroAutoCase(on) => options.auto_case_macro = on,
        Change::TypingDefaults => {
            let defaults = EngineOptions::defaults_v1();
            options.input_method = defaults.input_method;
            options.modern_tone = defaults.modern_tone;
            options.free_marking = defaults.free_marking;
            options.spell_check = defaults.spell_check;
            options.auto_restore_invalid = defaults.auto_restore_invalid;
            options.macros_enabled = defaults.macros_enabled;
            options.macros_when_disabled = defaults.macros_when_disabled;
            options.auto_case_macro = defaults.auto_case_macro;
        }
        Change::Startup(_) | Change::ShowPanelOnStart(_) => {}
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
    /// `raumakey.exe` runs from the temporary folder: "Khởi động cùng Windows" makes no
    /// shortcut to it.
    pub temporary_exe: bool,
    /// `[system] show_panel_on_start` in effect: the settings panel opens once the icon is
    /// added, and the panel's check box shows it.
    pub show_panel: bool,
}

/// The registered messages the tray answers, as `RegisterWindowMessageW` numbered them for
/// this session. A number outside the registered range (`0xC000..=0xFFFF`), such as the 0 of
/// a failed registration (the [`Default`]), is never taken for its message: the tray then
/// runs without it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RegisteredMessages {
    /// `TaskbarCreated` (`raumakey_windows::shell::taskbar_created_message`): Explorer
    /// restarted (matrix #58).
    pub taskbar_created: u32,
    /// `Raumakey.<sidhash>.show-panel` ([`SHOW_PANEL_COMPONENT`]): a second start asks for the
    /// settings panel (D45).
    pub show_panel: u32,
}

/// `message` when it is a registered message number.
fn registered(message: u32) -> Option<u32> {
    (REGISTERED_MESSAGE_FIRST..=REGISTERED_MESSAGE_LAST)
        .contains(&message)
        .then_some(message)
}

/// Something another thread left in the tray's inbox (then it posts [`WAKE`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The toggle hotkey changed the mode in the hook (true: Vietnamese), from the hook
    /// observer.
    Hotkey(bool),
    /// An answer of the settings worker.
    Worker(Answer),
    /// The user answered yes to the question of a notice (message boxes run on threads of
    /// their own, `crate::desktop`).
    Answered(Question),
    /// A request of the settings panel, which runs on a thread of its own.
    Panel(PanelRequest),
    /// The panel opened by [`Effect::OpenPanel`] with this `serial` (or by a later one that
    /// brought it to the front) was closed.
    PanelClosed(u64),
    /// The macro list editor (D44) wrote `raumakey-macros.toml`.
    MacrosSaved,
}

/// The events drained from the inbox, with the hotkey changes coalesced: only the last one
/// counts (the hook has that mode already), at its own place; the other events keep their
/// order.
pub fn coalesce(events: Vec<Event>) -> Vec<Event> {
    let last_hotkey = events
        .iter()
        .rposition(|event| matches!(event, Event::Hotkey(_)));
    events
        .into_iter()
        .enumerate()
        .filter(|(index, event)| !matches!(event, Event::Hotkey(_)) || Some(*index) == last_hotkey)
        .map(|(_, event)| event)
        .collect()
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
    /// mode in effect.
    HookRuntime(Box<RuntimeSnapshot>),
    /// Send `command`, numbered `seq`, to the settings worker.
    Worker {
        /// The command's number; its answer carries it ([`Answer::seq`]).
        seq: u64,
        /// The command.
        command: WorkerCommand,
    },
    /// Make sure the icon is shown with this state: add it again when it is not shown, or
    /// when its last change failed (every tick of the poll timer).
    EnsureIcon(IconState),
    /// Read the application of window `window` (`None`: of the window in the foreground now)
    /// and pass it to [`Tray::on_foreground`]; an application that cannot be read (no window,
    /// one that is gone, a process that cannot be queried) is passed as `None`.
    ReadForeground(Option<isize>),
    /// Open the settings panel showing `state`; when a panel is open, show `state` in it and
    /// bring it to the front instead. The panel open afterwards carries `serial` (a new number
    /// for every opening): its [`Event::PanelClosed`] names it, so the close of a panel that
    /// was replaced meanwhile is not taken for the close of the open one.
    OpenPanel {
        /// The number of this opening.
        serial: u64,
        /// What the panel shows.
        state: PanelState,
    },
    /// Show this state in the open panel, if one is open.
    RefreshPanel(PanelState),
    /// Close the open panel, if any, without waiting for it.
    ClosePanel,
    /// Open the macro list editor of `raumakey-macros.toml` in this data folder (D44), or
    /// bring the open one to the front.
    OpenMacroList(PathBuf),
    /// Close the open macro list editor, if any, without waiting for it and without saving.
    CloseMacroList,
    /// Stop the typing hook (both hooks are removed; typing is plain at once).
    StopTyping,
    /// Close every open notice without waiting for it.
    DismissNotices,
    /// Take the events left in the inbox and pass them to [`Tray::flush_hotkeys`], so that a
    /// hotkey change made just before the exit is saved.
    FlushHotkeys,
    /// Let the worker finish the commands sent and save what is unsaved (bounded).
    FinishWorker,
    /// Remove the icon.
    RemoveIcon,
    /// End the message loop with [`LOOP_EXIT_CODE`].
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
    /// The registered show-panel request of a second start; `None` when registration failed.
    show_panel_message: Option<u32>,
    /// `[system] show_panel_on_start` in effect (the panel opens at [`START`] when set).
    show_panel_on_start: bool,
    /// The state last sent to the open panel; `None` while no panel is open.
    panel: Option<PanelState>,
    /// The number of the last [`Effect::OpenPanel`].
    panel_serial: u64,
    phase: Phase,
    /// The number of the last worker command sent.
    sent: u64,
    /// The number of the poll on its way, if one is.
    polling: Option<u64>,
    /// The macro list was saved while a poll was on its way: poll once more after it.
    poll_again: bool,
    /// The kind of the last save error told; cleared by a save that works.
    save_error: Option<ConfigError>,
    /// `raumakey.exe` runs from the temporary folder: no shortcut is made to it.
    temporary_exe: bool,
    /// The longest time between the clicks of a double-click, in milliseconds.
    double_click_ms: u32,
    /// The last click on the icon that switched the mode, which a second click may make the
    /// first of a double-click.
    click: Option<Click>,
    /// The time of the last double-click notification, while the shell may still follow it
    /// with the `NIN_SELECT` of its second click.
    double_clicked: Option<u32>,
    /// The application in the foreground, as the glue last read it (`None`: unknown).
    foreground: Option<ForegroundApp>,
    /// That application, when the compatibility rules in effect exclude it (D47): the icon
    /// shows E and names it.
    excluded: Option<ExeName>,
}

/// A click on the icon that switched the mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Click {
    /// When it arrived (the glue's clock, milliseconds).
    at: u32,
    /// The mode before it, which a double-click restores.
    before: InputMode,
}

impl fmt::Debug for Tray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Tray")
            .field("mode", &self.mode())
            .field("method", &self.runtime.options.input_method)
            .field("config_revision", &self.runtime.config_revision)
            .field("startup", &self.startup)
            .field("location", &self.location)
            .field("show_panel_on_start", &self.show_panel_on_start)
            .field("panel_open", &self.panel.is_some())
            .field("panel_serial", &self.panel_serial)
            .field("excluded", &self.excluded.is_some())
            .field("phase", &self.phase)
            .field("sent", &self.sent)
            .field("polling", &self.polling)
            .finish_non_exhaustive()
    }
}

impl Tray {
    /// A tray before its start, answering the registered `messages` (see
    /// [`RegisteredMessages`] for a registration that failed).
    pub fn new(start: TrayStart, messages: RegisteredMessages) -> Self {
        Self {
            runtime: start.runtime,
            startup: start.startup,
            location: start.location,
            config_path: start.config_path,
            reported: start.status,
            notices: start.notices,
            taskbar_created: registered(messages.taskbar_created),
            show_panel_message: registered(messages.show_panel),
            show_panel_on_start: start.show_panel,
            panel: None,
            panel_serial: 0,
            phase: Phase::Starting,
            sent: 0,
            polling: None,
            poll_again: false,
            save_error: None,
            temporary_exe: start.temporary_exe,
            double_click_ms: DEFAULT_DOUBLE_CLICK_MS,
            click: None,
            double_clicked: None,
            foreground: None,
            excluded: None,
        }
    }

    /// The same tray with the user's double-click time (`GetDoubleClickTime`, through
    /// `raumakey_windows::input::double_click_time`), in milliseconds.
    #[must_use]
    pub fn with_double_click_ms(mut self, double_click_ms: u32) -> Self {
        self.double_click_ms = double_click_ms;
        self
    }

    /// The mode in effect: saved, checked in the menu and the panel, and handed to the hook.
    /// The icon shows it, or E while an excluded application is in front ([`Tray::icon`]).
    pub fn mode(&self) -> InputMode {
        InputMode::from_vietnamese(self.runtime.vietnamese_mode)
    }

    /// What the icon shows: the mode in effect, or E while an excluded application is in
    /// front.
    pub fn icon(&self) -> IconState {
        let method = self.runtime.options.input_method;
        match &self.excluded {
            Some(exe) => IconState::excluded(exe.clone(), method),
            None => IconState::new(self.mode(), method),
        }
    }

    /// The application in the foreground is now `app` (`None`: unknown), as the glue read it
    /// for [`Effect::ReadForeground`]: the icon shows whether the rules in effect exclude it
    /// (see the [module documentation](self)). Only while the tray runs.
    pub fn on_foreground(&mut self, app: Option<ForegroundApp>) -> Vec<Effect> {
        if !self.is_running() || self.foreground == app {
            return Vec::new();
        }
        let before = self.icon();
        self.foreground = app;
        self.excluded = self.exclusion();
        let mut effects = Vec::new();
        if self.icon() != before {
            effects.push(Effect::UpdateIcon(self.icon()));
        }
        self.follow_panel(&mut effects, false);
        effects
    }

    /// The application in the foreground when the compatibility rules of the runtime snapshot
    /// in effect forbid Vietnamese in it, as the hook resolves them for a window of its
    /// executable and class (an unknown class matches no rule narrowed to one; a password
    /// field, which the icon does not follow, plays no part).
    fn exclusion(&self) -> Option<ExeName> {
        let app = self.foreground.as_ref()?;
        let mut host = HostContext::new(SessionId::default(), app.exe.clone());
        host.window_class.clone_from(&app.window_class);
        let policy = CompatibilityPolicy::new(Arc::new(self.runtime.compatibility.clone()));
        (!policy.resolve(&host).vietnamese_allowed).then(|| app.exe.clone())
    }

    /// The runtime snapshot in effect, as the hook last got it from the tray.
    pub fn runtime(&self) -> &RuntimeSnapshot {
        &self.runtime
    }

    /// The startup mode the menu shows.
    pub fn startup(&self) -> StartupMode {
        self.startup
    }

    /// What the settings panel shows for the settings in effect.
    pub fn panel_state(&self) -> PanelState {
        PanelState::new(
            &self.runtime,
            self.startup,
            self.show_panel_on_start,
            self.location,
            self.config_path.as_deref(),
        )
    }

    /// The tray menu for the settings in effect (D42, with "Bảng cấu hình…" first, D43).
    pub fn menu(&self) -> Vec<MenuItem<'static>> {
        let options = &self.runtime.options;
        let method = options.input_method;
        vec![
            TrayCommand::Panel.item(false),
            MenuItem::Separator,
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

    /// The messages that (non-elevated) other processes send to the hidden window:
    /// `TaskbarCreated` and the icon's callback from the shell, and the show-panel request of
    /// a second start from a normal prompt (D45). An elevated controller lets them through
    /// User Interface Privilege Isolation (`Window::allow_message_from_lower_integrity`); a
    /// non-elevated one does not need to.
    pub fn shell_messages(&self) -> Vec<u32> {
        self.taskbar_created
            .into_iter()
            .chain(self.show_panel_message)
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

    /// The answer to window message `message` with its parameters, handled at `now_ms` (see
    /// the [module documentation](self)). `now_ms` is a clock in milliseconds that wraps
    /// around (`GetTickCount`, through `raumakey_windows::input::tick_count`); only the icon's
    /// messages use it, to tell a double-click from two clicks.
    pub fn on_message_at(
        &mut self,
        message: u32,
        wparam: usize,
        lparam: isize,
        now_ms: u32,
    ) -> Reply {
        let mut reply = self.message(message, wparam, lparam, now_ms);
        self.follow_panel(&mut reply.effects, false);
        reply
    }

    fn message(&mut self, message: u32, wparam: usize, lparam: isize, now_ms: u32) -> Reply {
        if message == START.id() {
            return Reply::handled(self.start());
        }
        if message == TRAY_CALLBACK.id() {
            return Reply::handled(self.icon_message(wparam, lparam, now_ms));
        }
        if message == WAKE.id() {
            return Reply::handled(Vec::new());
        }
        if message == FOREGROUND.id() {
            let effects = if self.is_running() {
                vec![Effect::ReadForeground(Some(wparam as isize))]
            } else {
                Vec::new()
            };
            return Reply::handled(effects);
        }
        if self.taskbar_created == Some(message) {
            let effects = if self.is_running() {
                vec![Effect::ShowIcon(self.icon())]
            } else {
                Vec::new()
            };
            return Reply::handled(effects);
        }
        if self.show_panel_message == Some(message) {
            let effects = if self.is_running() {
                self.open_panel()
            } else {
                Vec::new()
            };
            return Reply::handled(effects);
        }
        match message {
            WM_TIMER if wparam == POLL_TIMER => Reply::handled(self.tick()),
            WM_CLOSE => Reply::handled(self.exit()),
            WM_ENDSESSION if wparam != 0 => Reply::handled(self.exit()),
            _ => Reply::default_processing(),
        }
    }

    /// The effects of the command chosen in the menu (`None`: the menu was dismissed).
    pub fn menu_choice(&mut self, chosen: Option<u16>) -> Vec<Effect> {
        let mut effects = self.choice(chosen);
        self.follow_panel(&mut effects, false);
        effects
    }

    fn choice(&mut self, chosen: Option<u16>) -> Vec<Effect> {
        if !self.is_running() {
            return Vec::new();
        }
        let Some(command) = chosen.and_then(TrayCommand::from_id) else {
            return Vec::new();
        };
        let options = &self.runtime.options;
        match command {
            TrayCommand::Panel => self.open_panel(),
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
        // The panel's check boxes toggle themselves: whatever a request did (or refused), the
        // panel is shown the state again.
        let from_panel = matches!(event, Event::Panel(_));
        let mut effects = match event {
            Event::Hotkey(vietnamese) => self.set_mode(InputMode::from_vietnamese(vietnamese)),
            Event::Worker(answer) => self.on_answer(answer),
            Event::Answered(question) => self.answered_yes(question),
            Event::Panel(request) => self.panel_request(request),
            Event::PanelClosed(serial) => {
                if serial == self.panel_serial {
                    self.panel = None;
                }
                Vec::new()
            }
            Event::MacrosSaved => self.poll_now(),
        };
        self.follow_panel(&mut effects, from_panel);
        effects
    }

    /// At exit ([`Effect::FlushHotkeys`]): the events left in the inbox once the hook has
    /// stopped. The last hotkey change among them is saved (the worker finishes after this);
    /// everything else is dropped.
    pub fn flush_hotkeys(&mut self, events: Vec<Event>) -> Vec<Effect> {
        let last = events.into_iter().rev().find_map(|event| match event {
            Event::Hotkey(vietnamese) => Some(vietnamese),
            Event::Worker(_)
            | Event::Answered(_)
            | Event::Panel(_)
            | Event::PanelClosed(_)
            | Event::MacrosSaved => None,
        });
        let Some(vietnamese) = last else {
            return Vec::new();
        };
        self.runtime = with_change(&self.runtime, Change::VietnameseMode(vietnamese));
        vec![self.command(WorkerCommand::Apply(Change::VietnameseMode(vietnamese)))]
    }

    /// The user answered yes to `question` of a [`Notice`].
    pub fn answered_yes(&mut self, question: Question) -> Vec<Effect> {
        if !self.is_running() {
            return Vec::new();
        }
        let mut effects = match question {
            Question::Recover => vec![self.command(WorkerCommand::Recover)],
        };
        self.follow_panel(&mut effects, false);
        effects
    }

    /// While a panel is open: appends [`Effect::RefreshPanel`] to `effects` when the state it
    /// shows is no longer the one last sent, or always when `force`d.
    fn follow_panel(&mut self, effects: &mut Vec<Effect>, force: bool) {
        let Some(sent) = &self.panel else {
            return;
        };
        let now = self.panel_state();
        if force || *sent != now {
            self.panel = Some(now.clone());
            effects.push(Effect::RefreshPanel(now));
        }
    }

    /// Opens the panel, or brings the open one to the front, with the state in effect.
    fn open_panel(&mut self) -> Vec<Effect> {
        self.panel_serial += 1;
        let state = self.panel_state();
        self.panel = Some(state.clone());
        vec![Effect::OpenPanel {
            serial: self.panel_serial,
            state,
        }]
    }

    /// A request of the panel: as the menu would do it, with absolute values.
    fn panel_request(&mut self, request: PanelRequest) -> Vec<Effect> {
        match request {
            PanelRequest::Change(change) => match change {
                Change::VietnameseMode(on) => self.set_mode(InputMode::from_vietnamese(on)),
                Change::InputMethod(method) => self.choose_method(method),
                Change::ShowPanelOnStart(on) => {
                    self.show_panel_on_start = on;
                    vec![self.command(WorkerCommand::Apply(change))]
                }
                // Only `PanelRequest::Startup` sets the start with Windows: it goes with the
                // shortcut.
                Change::Startup(_) => Vec::new(),
                Change::SpellCheck(_)
                | Change::ModernTone(_)
                | Change::FreeMarking(_)
                | Change::AutoRestore(_)
                | Change::Macros(_)
                | Change::MacrosWhenEnglish(_)
                | Change::MacroAutoCase(_)
                | Change::TypingDefaults => self.change(change),
            },
            PanelRequest::Startup(on) => match (self.startup, on) {
                // The menu's switch: on (or the request to extract Raumakey first), off, or
                // the explanation of the elevated logon task.
                (StartupMode::None, true)
                | (StartupMode::Shortcut, false)
                | (StartupMode::ElevatedTask, false) => self.switch_startup(),
                (StartupMode::None, false)
                | (StartupMode::Shortcut | StartupMode::ElevatedTask, true) => Vec::new(),
            },
            PanelRequest::OpenConfig => vec![self.command(WorkerCommand::OpenConfig)],
            PanelRequest::MacroList => self
                .config_path
                .as_deref()
                .and_then(Path::parent)
                .map(|folder| Effect::OpenMacroList(folder.to_path_buf()))
                .into_iter()
                .collect(),
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
        // The application in front is read once the icon exists, so a change it makes to the
        // icon follows the add.
        let mut effects = vec![Effect::ShowIcon(self.icon()), Effect::ReadForeground(None)];
        if self.show_panel_on_start {
            effects.extend(self.open_panel());
        }
        effects.extend(self.notices.drain(..).map(Effect::Notify));
        effects
    }

    fn icon_message(&mut self, wparam: usize, lparam: isize, now_ms: u32) -> Vec<Effect> {
        let message = decode_notify_icon_message(wparam, lparam);
        if !self.is_running() || message.icon_id != TRAY_ICON_ID {
            return Vec::new();
        }
        match message.event {
            NotifyIconEvent::Select => self.select(now_ms),
            NotifyIconEvent::DoubleClick => self.double_click(now_ms),
            NotifyIconEvent::ButtonDown => {
                // A new click: its select is never the second one of a double-click.
                self.double_clicked = None;
                Vec::new()
            }
            NotifyIconEvent::KeySelect => {
                self.click = None;
                self.set_mode(self.mode().toggled())
            }
            NotifyIconEvent::ContextMenu { x, y } => {
                self.click = None;
                vec![Effect::OpenMenu { x, y }]
            }
            NotifyIconEvent::Other(_) => Vec::new(),
        }
    }

    /// Whether `now_ms` is within the double-click time after `then_ms` (the clock wraps
    /// around; a clock that went back is far apart).
    fn within_double_click(&self, then_ms: u32, now_ms: u32) -> bool {
        now_ms.wrapping_sub(then_ms) <= self.double_click_ms
    }

    /// The select of a click at `now_ms`: swallowed when it is the second click of a
    /// double-click that was reported already, the double-click itself when it follows a
    /// click within the double-click time (a shell that reports no double-click), and
    /// otherwise a switch of the mode, at once.
    fn select(&mut self, now_ms: u32) -> Vec<Effect> {
        if let Some(at) = self.double_clicked.take()
            && self.within_double_click(at, now_ms)
        {
            return Vec::new();
        }
        if let Some(click) = self.click.take()
            && self.within_double_click(click.at, now_ms)
        {
            return self.panel_after(Some(click));
        }
        let before = self.mode();
        self.click = Some(Click { at: now_ms, before });
        self.set_mode(before.toggled())
    }

    /// The double-click notification at `now_ms`: the panel, after the mode from before its
    /// first click. A select that follows it within the double-click time, before a new
    /// press, belongs to it.
    fn double_click(&mut self, now_ms: u32) -> Vec<Effect> {
        self.double_clicked = Some(now_ms);
        let click = self
            .click
            .take()
            .filter(|click| self.within_double_click(click.at, now_ms));
        self.panel_after(click)
    }

    /// Opens the panel for a double-click whose first click was `click`: the mode it changed
    /// is changed back first (nothing when the mode is back already).
    fn panel_after(&mut self, click: Option<Click>) -> Vec<Effect> {
        let mut effects = match click {
            Some(click) if click.before != self.mode() => self.set_mode(click.before),
            Some(_) | None => Vec::new(),
        };
        effects.extend(self.open_panel());
        effects
    }

    /// A tick of the poll timer: the icon must be shown, the application in front is read
    /// again (a [`FOREGROUND`] may have been lost), and the worker polls unless a poll is on
    /// its way.
    fn tick(&mut self) -> Vec<Effect> {
        if !self.is_running() {
            return Vec::new();
        }
        let mut effects = vec![
            Effect::EnsureIcon(self.icon()),
            Effect::ReadForeground(None),
        ];
        if self.polling.is_none() {
            effects.push(self.command(WorkerCommand::Poll));
        }
        effects
    }

    /// The macro list was saved: the worker polls at once, so the hook gets the new macros
    /// now; while a poll is on its way (it may have read the file before the save), once more
    /// as soon as that poll answered.
    fn poll_now(&mut self) -> Vec<Effect> {
        if self.polling.is_some() {
            self.poll_again = true;
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
            StartupMode::None if self.temporary_exe => {
                vec![Effect::Notify(temporary_exe_notice())]
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
        self.panel = None;
        vec![
            Effect::StopTyping,
            Effect::DismissNotices,
            Effect::ClosePanel,
            Effect::CloseMacroList,
            Effect::FlushHotkeys,
            Effect::FinishWorker,
            Effect::RemoveIcon,
            Effect::EndLoop,
        ]
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
            Outcome::Applied {
                result: Ok(Saved::Written),
                ..
            }
            | Outcome::Startup {
                result: Ok(Saved::Written),
                ..
            } => self.saved(),
            Outcome::Applied {
                result: Ok(Saved::Unchanged),
                ..
            }
            | Outcome::Startup {
                result: Ok(Saved::Unchanged),
                ..
            }
            | Outcome::Opened(Ok(()))
            | Outcome::Failed => {}
            Outcome::Applied {
                result: Err(error), ..
            } => {
                let notice = save_error_notice(path, error);
                effects.extend(self.save_failed(error, notice));
            }
            Outcome::Startup {
                on,
                result: Err(StartupError::Save(error)),
            } => {
                let notice = startup_error_notice(path, on, StartupError::Save(error));
                effects.extend(self.save_failed(error, notice));
            }
            Outcome::Startup {
                on,
                result: Err(error @ (StartupError::Shortcut(_) | StartupError::Temporary)),
            } => effects.push(Effect::Notify(startup_error_notice(path, on, error))),
            Outcome::Polled(report) => {
                if let Some(status) = report.status {
                    if let Some(notice) = status_notice(path, &self.reported, &status) {
                        effects.push(Effect::Notify(notice));
                    }
                    self.reported = status;
                }
                match report.shortcut {
                    Some(ShortcutSync::RefusedTemporary) => {
                        effects.push(Effect::Notify(temporary_exe_notice()));
                    }
                    Some(ShortcutSync::Failed { on, error }) => {
                        let error = StartupError::Shortcut(error);
                        effects.push(Effect::Notify(startup_error_notice(path, on, error)));
                    }
                    Some(
                        ShortcutSync::Created | ShortcutSync::Removed | ShortcutSync::Unchanged,
                    )
                    | None => {}
                }
                match report.retry {
                    Some(Ok(())) => self.saved(),
                    Some(Err(error)) => {
                        let notice = save_error_notice(path, error);
                        effects.extend(self.save_failed(error, notice));
                    }
                    None => {}
                }
            }
            Outcome::Recovered(result) => {
                match result {
                    Ok(Recovery::Replaced { saved: Ok(()), .. }) => self.saved(),
                    // Nothing was written, but the file reads now.
                    Ok(Recovery::Adopted) => {
                        self.reported.config = DocumentStatus::Loaded { issues: Vec::new() };
                    }
                    Ok(Recovery::NotNeeded | Recovery::Replaced { .. }) | Err(_) => {}
                }
                effects.extend(recovery_notice(path, &result).map(Effect::Notify));
            }
            Outcome::Opened(Err(error)) => {
                effects.push(Effect::Notify(open_error_notice(path, error)));
            }
        }
        // Last, so that the answer was taken over as the answer to the last command.
        if self.polling.is_none() && self.poll_again {
            self.poll_again = false;
            effects.push(self.command(WorkerCommand::Poll));
        }
        effects
    }

    /// Takes over the settings in effect that the worker answered; new compatibility rules
    /// apply to the application in front at once.
    fn adopt(&mut self, state: SettingsState) -> Vec<Effect> {
        let before = self.icon();
        let changed = !same_for_hook(&self.runtime, &state.runtime);
        let rules_changed = self.runtime.compatibility != state.runtime.compatibility;
        self.runtime = state.runtime;
        self.startup = state.startup;
        self.show_panel_on_start = state.show_panel_on_start;
        if rules_changed {
            self.excluded = self.exclusion();
        }
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
    fn save_failed(&mut self, error: ConfigError, notice: Notice) -> Option<Effect> {
        let same = self
            .save_error
            .is_some_and(|told| core::mem::discriminant(&told) == core::mem::discriminant(&error));
        if same {
            return None;
        }
        self.save_error = Some(error);
        Some(Effect::Notify(notice))
    }
}
