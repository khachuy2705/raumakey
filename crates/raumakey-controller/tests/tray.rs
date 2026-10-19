//! The tray state machine (`tray`): the menu of D42 and its checks, the icon and tooltip, the
//! effect of each choice (the typing hook, the settings worker, message boxes), the messages
//! of the hidden window (the icon, the poll timer, `TaskbarCreated`, the end of the session),
//! the hotkey's mode changes from the hook observer, and the worker's answers. Pure: these
//! tests run on every host.

use std::cell::Cell;
use std::path::PathBuf;

use raumakey_config::{ConfigError, StartupMode};
use raumakey_controller::exclusions::{ExclusionsRequest, ExclusionsState, default_apps};
use raumakey_controller::notice::{
    Notice, NoticeKind, Question, about_notice, elevated_task_notice, open_error_notice,
    recovery_notice, save_error_notice, startup_error_notice, status_notice, temporary_exe_notice,
};
use raumakey_controller::panel::{PanelRequest, PanelState};
use raumakey_controller::settings::{Change, DocumentStatus, Location, Recovery, Settings, Status};
use raumakey_controller::tray::{
    Effect, Event, FOREGROUND, ForegroundApp, ICON_APP, IconState, InputMode, LOOP_EXIT_CODE,
    MENU_ABOUT, MENU_EXIT, MENU_MACROS, MENU_MODERN_TONE, MENU_OPEN_CONFIG, MENU_PANEL,
    MENU_SIMPLE_TELEX, MENU_SPELL_CHECK, MENU_STARTUP, MENU_TELEX, MENU_VIETNAMESE_MODE, MENU_VIQR,
    MENU_VNI, POLL_INTERVAL_MS, POLL_TIMER, RegisteredMessages, Reply, SHOW_PANEL_COMPONENT, START,
    TOOLTIP_ENGLISH, TRAY_CALLBACK, TRAY_ICON_ID, Tray, TrayCommand, TrayStart, WAKE, coalesce,
    with_change,
};
use raumakey_controller::worker::{
    Answer, OpenError, Outcome, PollReport, Saved, SettingsState, ShortcutSync, StartupError,
    WorkerCommand,
};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::host::ExeName;
use raumakey_model::options::{InputMethod, OutputCharset};
use raumakey_model::policy::{AppRule, RuleAction};
use raumakey_resources::{ICON_ENGLISH, ICON_VIETNAMESE};
use raumakey_windows::error::{E_ACCESSDENIED, WinError};
use raumakey_windows::menu::MenuItem;
use raumakey_windows::names::{SidHash, is_valid_component, message_name};
use raumakey_windows::shell::{NIN_KEYSELECT, NIN_SELECT, TOOLTIP_CAPACITY, WM_CONTEXTMENU};
use raumakey_windows::window::{WM_APP, WM_CLOSE, WM_ENDSESSION, WM_TIMER, is_postable};

use InputMode::{English, Vietnamese};

/// A registered message number standing in for `TaskbarCreated`, which Windows registers at
/// run time.
const TASKBAR_CREATED: u32 = 0xC0DE;
/// A registered message number standing in for the request of a second start to show the
/// panel (`Raumakey.<sidhash>.show-panel`, D45).
const SHOW_PANEL: u32 = 0xC123;

const WM_NULL: u32 = 0x0000;
const WM_CREATE: u32 = 0x0001;
const WM_QUERYENDSESSION: u32 = 0x0011;
const WM_MOUSEMOVE: u32 = 0x0200;
const WM_LBUTTONDOWN: u32 = 0x0201;
const WM_LBUTTONUP: u32 = 0x0202;
const WM_LBUTTONDBLCLK: u32 = 0x0203;
const WM_RBUTTONUP: u32 = 0x0205;
const NIN_POPUPOPEN: u32 = 0x0406;
/// `WM_ENDSESSION` `lParam` flags: the Restart Manager closes the application; sign-out.
const ENDSESSION_CLOSEAPP: isize = 0x0000_0001;
const ENDSESSION_LOGOFF: isize = 0x8000_0000_u32 as i32 as isize;

/// The answer of a handled message: the window procedure returns 0.
const ANSWERED: Option<isize> = Some(0);

/// A window message: number, `wParam`, `lParam`.
type Message = (u32, usize, isize);

fn config_path() -> PathBuf {
    PathBuf::from(r"D:\Tools\Raumakey\raumakey.toml")
}

fn clean() -> Status {
    Status {
        config: DocumentStatus::Loaded { issues: Vec::new() },
        macros: DocumentStatus::Missing,
        app_rules: DocumentStatus::Missing,
        skipped_macros: Vec::new(),
    }
}

/// The runtime snapshot of a fresh `raumakey.toml` (the defaults, revision 1) in `mode`.
fn runtime(mode: InputMode) -> RuntimeSnapshot {
    let mut runtime = RuntimeSnapshot::defaults_v1();
    runtime.config_revision = 1;
    runtime.vietnamese_mode = mode.is_vietnamese();
    runtime.options.enabled = mode.is_vietnamese();
    runtime
}

fn start(mode: InputMode) -> TrayStart {
    TrayStart {
        runtime: runtime(mode),
        startup: StartupMode::None,
        location: Location::ExeDir,
        config_path: Some(config_path()),
        status: clean(),
        notices: Vec::new(),
        temporary_exe: false,
        show_panel: false,
        exclusions: default_apps(),
    }
}

/// The registered messages of the tests: `TaskbarCreated` and the show-panel request.
fn messages() -> RegisteredMessages {
    RegisteredMessages {
        taskbar_created: TASKBAR_CREATED,
        show_panel: SHOW_PANEL,
    }
}

/// The effects of an exit, in order.
fn exit_effects() -> Vec<Effect> {
    vec![
        Effect::StopTyping,
        Effect::DismissNotices,
        Effect::ClosePanel,
        Effect::CloseMacroList,
        Effect::CloseExclusions,
        Effect::FlushInbox,
        Effect::FinishWorker,
        Effect::RemoveIcon,
        Effect::EndLoop,
    ]
}

/// A poll that reloaded files, which are now in `status`.
fn polled(status: Status) -> Outcome {
    Outcome::Polled(PollReport {
        status: Some(status),
        ..PollReport::default()
    })
}

/// The icon's callback message (`NOTIFYICON_VERSION_4`): the anchor in `wParam`, the
/// notification and the icon id in `lParam`.
fn callback(code: u32, icon_id: u16, x: i16, y: i16) -> Message {
    let wparam = usize::from(x as u16) | (usize::from(y as u16) << 16);
    let lparam = ((u32::from(icon_id) << 16) | code) as i32 as isize;
    (TRAY_CALLBACK.id(), wparam, lparam)
}

/// A left click on the tray icon.
fn click() -> Message {
    callback(NIN_SELECT, TRAY_ICON_ID, 30, 40)
}

fn taskbar_created() -> Message {
    (TASKBAR_CREATED, 0, 0)
}

fn tick() -> Message {
    (WM_TIMER, POLL_TIMER, 0)
}

thread_local! {
    /// The clock of [`send`], in milliseconds.
    static CLOCK: Cell<u32> = const { Cell::new(0) };
}

/// `message`, a minute after the message sent before it: no two messages sent this way make
/// a double-click. The tests of the double-click give the time themselves ([`send_at`]).
fn send(tray: &mut Tray, message: Message) -> Reply {
    let now = CLOCK.with(|clock| {
        let now = clock.get().wrapping_add(60_000);
        clock.set(now);
        now
    });
    send_at(tray, message, now)
}

/// `message`, handled at `now_ms`.
fn send_at(tray: &mut Tray, (message, wparam, lparam): Message, now_ms: u32) -> Reply {
    tray.on_message_at(message, wparam, lparam, now_ms)
}

fn answered(effects: Vec<Effect>) -> Reply {
    Reply {
        result: ANSWERED,
        effects,
    }
}

/// The default processing of `DefWindowProcW`, with no effect.
fn default() -> Reply {
    Reply {
        result: None,
        effects: Vec::new(),
    }
}

fn icon(mode: InputMode, method: InputMethod) -> IconState {
    IconState::new(mode, method)
}

fn worker(seq: u64, command: WorkerCommand) -> Effect {
    Effect::Worker { seq, command }
}

fn hook_runtime(runtime: RuntimeSnapshot) -> Effect {
    Effect::HookRuntime(Box::new(runtime))
}

fn choose(tray: &mut Tray, command: TrayCommand) -> Vec<Effect> {
    tray.menu_choice(Some(command.id()))
}

/// The effects of [`START`] for an icon showing `shown`: the icon is added, then the
/// application in the foreground is read (D47).
fn started(shown: IconState) -> Vec<Effect> {
    vec![Effect::ShowIcon(shown), Effect::ReadForeground(None)]
}

/// A tray whose message loop runs: [`START`] added its icon.
fn running_with(start: TrayStart) -> Tray {
    let mode = InputMode::from_vietnamese(start.runtime.vietnamese_mode);
    let method = start.runtime.options.input_method;
    let mut tray = Tray::new(start, messages());
    assert_eq!(
        send(&mut tray, (START.id(), 0, 0)),
        answered(started(icon(mode, method)))
    );
    tray
}

fn running(mode: InputMode) -> Tray {
    running_with(start(mode))
}

/// An answer of the worker carrying `state`.
fn answer(seq: u64, outcome: Outcome, state: Option<SettingsState>) -> Event {
    Event::Worker(Answer {
        seq,
        outcome,
        state: state.map(Box::new),
    })
}

fn state(runtime: RuntimeSnapshot, startup: StartupMode) -> SettingsState {
    SettingsState {
        runtime,
        startup,
        show_panel_on_start: false,
        exclusions: default_apps(),
    }
}

fn applied(change: Change, result: Result<Saved, ConfigError>) -> Outcome {
    Outcome::Applied { change, result }
}

/// The checks of the menu of `tray`, by command.
fn checks(tray: &Tray) -> Vec<(u16, bool)> {
    tray.menu()
        .into_iter()
        .filter_map(|item| match item {
            MenuItem::Command { id, checked, .. } => Some((id, checked)),
            MenuItem::Separator => None,
        })
        .collect()
}

fn checked(tray: &Tray, command: TrayCommand) -> bool {
    checks(tray)
        .into_iter()
        .find(|(id, _)| *id == command.id())
        .map(|(_, checked)| checked)
        .unwrap()
}

// --- Modes, icons, tooltips -------------------------------------------------------------

#[test]
fn toggling_switches_between_the_two_modes() {
    assert_eq!(Vietnamese.toggled(), English);
    assert_eq!(English.toggled(), Vietnamese);
    assert!(Vietnamese.is_vietnamese());
    assert!(!English.is_vietnamese());
    assert_eq!(InputMode::from_vietnamese(true), Vietnamese);
    assert_eq!(InputMode::from_vietnamese(false), English);
    assert_eq!(Vietnamese.icon_id(), ICON_VIETNAMESE);
    assert_eq!(English.icon_id(), ICON_ENGLISH);
}

/// The tooltip names the mode, and in Vietnamese mode the input method (D42).
#[test]
fn the_tooltip_names_the_mode_and_the_input_method() {
    let cases = [
        (
            Vietnamese,
            InputMethod::Telex,
            "Raumakey: đang gõ tiếng Việt (Telex)",
        ),
        (
            Vietnamese,
            InputMethod::Vni,
            "Raumakey: đang gõ tiếng Việt (VNI)",
        ),
        (
            Vietnamese,
            InputMethod::Viqr,
            "Raumakey: đang gõ tiếng Việt (VIQR)",
        ),
        (
            Vietnamese,
            InputMethod::SimpleTelex,
            "Raumakey: đang gõ tiếng Việt (Telex đơn giản)",
        ),
        (English, InputMethod::Telex, "Raumakey: đang gõ tiếng Anh"),
        (English, InputMethod::Vni, "Raumakey: đang gõ tiếng Anh"),
    ];
    for (mode, method, tooltip) in cases {
        assert_eq!(icon(mode, method).tooltip(), tooltip, "{mode:?} {method:?}");
        assert_eq!(icon(mode, method).icon_id(), mode.icon_id());
    }
    assert_eq!(TOOLTIP_ENGLISH, "Raumakey: đang gõ tiếng Anh");
    // A user-defined key map has a tooltip of its own.
    let user = icon(Vietnamese, InputMethod::UserDefined).tooltip();
    assert!(user.starts_with("Raumakey: đang gõ tiếng Việt ("), "{user}");
    for method in [
        InputMethod::Telex,
        InputMethod::SimpleTelex,
        InputMethod::Vni,
        InputMethod::Viqr,
        InputMethod::UserDefined,
    ] {
        for mode in [Vietnamese, English] {
            let tooltip = icon(mode, method).tooltip();
            assert!(
                tooltip.encode_utf16().count() < TOOLTIP_CAPACITY,
                "{tooltip}"
            );
            assert!(!tooltip.contains('\0'));
        }
    }
}

// --- The menu ---------------------------------------------------------------------------

/// D42 with "Bảng cấu hình…" first (D43, design §22.4): flat, these labels in this order,
/// separators between the groups.
#[test]
fn the_menu_has_the_labels_of_d42_in_order_after_the_panel() {
    let tray = running(Vietnamese);
    let labels: Vec<Option<&str>> = tray
        .menu()
        .into_iter()
        .map(|item| match item {
            MenuItem::Command { text, .. } => Some(text),
            MenuItem::Separator => None,
        })
        .collect();
    assert_eq!(
        labels,
        [
            Some("Bảng cấu hình…"),
            None,
            Some("Gõ tiếng Việt (Ctrl+Shift)"),
            None,
            Some("Telex"),
            Some("Telex đơn giản"),
            Some("VNI"),
            Some("VIQR"),
            None,
            Some("Kiểm tra chính tả"),
            Some("Đặt dấu kiểu mới (oà, uý)"),
            Some("Gõ tắt (macro)"),
            None,
            Some("Khởi động cùng Windows"),
            Some("Mở file cấu hình"),
            Some("Giới thiệu"),
            None,
            Some("Thoát"),
        ]
    );
    assert_eq!(MENU_PANEL, "Bảng cấu hình\u{2026}");
    assert_eq!(MENU_VIETNAMESE_MODE, "Gõ tiếng Việt (Ctrl+Shift)");
    assert_eq!([MENU_TELEX, MENU_VNI, MENU_VIQR], ["Telex", "VNI", "VIQR"]);
    assert_eq!(MENU_SIMPLE_TELEX, "Telex đơn giản");
    assert_eq!(MENU_SPELL_CHECK, "Kiểm tra chính tả");
    assert_eq!(MENU_MODERN_TONE, "Đặt dấu kiểu mới (oà, uý)");
    assert_eq!(MENU_MACROS, "Gõ tắt (macro)");
    assert_eq!(MENU_STARTUP, "Khởi động cùng Windows");
    assert_eq!(MENU_OPEN_CONFIG, "Mở file cấu hình");
    assert_eq!(MENU_ABOUT, "Giới thiệu");
    assert_eq!(MENU_EXIT, "Thoát");
    for item in tray.menu() {
        item.validate().unwrap();
        if let MenuItem::Command { disabled, .. } = item {
            assert!(!disabled);
        }
    }
}

#[test]
fn command_ids_are_distinct_non_zero_and_round_trip() {
    let mut ids: Vec<u16> = TrayCommand::ALL
        .iter()
        .map(|command| command.id())
        .collect();
    for command in TrayCommand::ALL {
        assert_ne!(command.id(), 0, "{command:?}");
        assert_eq!(TrayCommand::from_id(command.id()), Some(command));
    }
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), TrayCommand::ALL.len());
    assert_eq!(TrayCommand::from_id(0), None);
    assert_eq!(TrayCommand::from_id(999), None);
    // The commands of D42 keep their ids; "Bảng cấu hình…" is new, first in the menu.
    let numbered: Vec<(TrayCommand, u16)> = TrayCommand::ALL
        .iter()
        .map(|command| (*command, command.id()))
        .collect();
    assert_eq!(
        numbered,
        [
            (TrayCommand::Panel, 12),
            (TrayCommand::VietnameseMode, 1),
            (TrayCommand::Telex, 2),
            (TrayCommand::SimpleTelex, 13),
            (TrayCommand::Vni, 3),
            (TrayCommand::Viqr, 4),
            (TrayCommand::SpellCheck, 5),
            (TrayCommand::ModernTone, 6),
            (TrayCommand::Macros, 7),
            (TrayCommand::Startup, 8),
            (TrayCommand::OpenConfig, 9),
            (TrayCommand::About, 10),
            (TrayCommand::Exit, 11),
        ]
    );
    // Every command is in the menu once.
    let tray = running(Vietnamese);
    let mut menu: Vec<u16> = checks(&tray).into_iter().map(|(id, _)| id).collect();
    menu.sort_unstable();
    assert_eq!(menu, ids);
}

/// The checks follow the settings in effect; exactly one input method is checked.
#[test]
fn the_checks_follow_the_settings_in_effect() {
    let tray = running(Vietnamese);
    assert_eq!(
        checks(&tray),
        [
            (TrayCommand::Panel.id(), false),
            (TrayCommand::VietnameseMode.id(), true),
            (TrayCommand::Telex.id(), true),
            (TrayCommand::SimpleTelex.id(), false),
            (TrayCommand::Vni.id(), false),
            (TrayCommand::Viqr.id(), false),
            (TrayCommand::SpellCheck.id(), true),
            (TrayCommand::ModernTone.id(), false),
            (TrayCommand::Macros.id(), true),
            (TrayCommand::Startup.id(), false),
            (TrayCommand::OpenConfig.id(), false),
            (TrayCommand::About.id(), false),
            (TrayCommand::Exit.id(), false),
        ]
    );
    let mut settings = start(English);
    settings.runtime.options.input_method = InputMethod::Viqr;
    settings.runtime.options.spell_check = false;
    settings.runtime.options.modern_tone = true;
    settings.runtime.options.macros_enabled = false;
    settings.startup = StartupMode::Shortcut;
    let tray = running_with(settings);
    assert_eq!(
        checks(&tray),
        [
            (TrayCommand::Panel.id(), false),
            (TrayCommand::VietnameseMode.id(), false),
            (TrayCommand::Telex.id(), false),
            (TrayCommand::SimpleTelex.id(), false),
            (TrayCommand::Vni.id(), false),
            (TrayCommand::Viqr.id(), true),
            (TrayCommand::SpellCheck.id(), false),
            (TrayCommand::ModernTone.id(), true),
            (TrayCommand::Macros.id(), false),
            (TrayCommand::Startup.id(), true),
            (TrayCommand::OpenConfig.id(), false),
            (TrayCommand::About.id(), false),
            (TrayCommand::Exit.id(), false),
        ]
    );
    // The elevated logon task (set from the command line) starts Raumakey as well.
    let mut elevated = start(Vietnamese);
    elevated.startup = StartupMode::ElevatedTask;
    assert!(checked(&running_with(elevated), TrayCommand::Startup));
}

// --- The start ---------------------------------------------------------------------------

#[test]
fn start_adds_the_icon_in_the_saved_mode_then_shows_the_start_notices_once() {
    let notice = Notice {
        kind: NoticeKind::Information,
        text: "chào".to_owned(),
        question: None,
    };
    let mut settings = start(English);
    settings.runtime.options.input_method = InputMethod::Vni;
    settings.notices = vec![notice.clone()];
    let mut tray = Tray::new(settings, messages());
    assert_eq!(tray.mode(), English);
    assert_eq!(
        send(&mut tray, (START.id(), 0, 0)),
        answered(vec![
            Effect::ShowIcon(icon(English, InputMethod::Vni)),
            Effect::ReadForeground(None),
            Effect::Notify(notice),
        ])
    );
    // Another start (anyone may post one) adds nothing.
    assert_eq!(send(&mut tray, (START.id(), 0, 0)), answered(Vec::new()));
}

/// A damaged file at the start: "yes" moves it aside through the worker.
#[test]
fn yes_to_the_recovery_question_sends_recover_to_the_worker() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        tray.answered_yes(Question::Recover),
        [worker(1, WorkerCommand::Recover)]
    );
}

/// Message boxes run on threads of their own: a "yes" arrives later, through the inbox.
#[test]
fn a_yes_from_a_message_box_arrives_as_an_event() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        tray.on_event(Event::Answered(Question::Recover)),
        [worker(1, WorkerCommand::Recover)]
    );
    // After an exit nothing acts on it any more.
    choose(&mut tray, TrayCommand::Exit);
    assert_eq!(tray.on_event(Event::Answered(Question::Recover)), []);
}

/// Several hotkey changes waiting in the inbox count as their last one; other events keep
/// their order.
#[test]
fn hotkey_events_are_coalesced_to_the_last_one() {
    let poll = Event::Worker(Answer {
        seq: 1,
        outcome: Outcome::Polled(PollReport::default()),
        state: None,
    });
    assert_eq!(
        coalesce(vec![
            Event::Hotkey(false),
            poll.clone(),
            Event::Hotkey(true),
            Event::Answered(Question::Recover),
            Event::Hotkey(false),
        ]),
        [
            poll.clone(),
            Event::Answered(Question::Recover),
            Event::Hotkey(false)
        ]
    );
    assert_eq!(coalesce(vec![poll.clone()]), [poll]);
    assert_eq!(coalesce(Vec::new()), []);
}

/// At exit the hotkey changes still in the inbox are saved (the last one); nothing else of
/// the inbox acts any more.
#[test]
fn exit_saves_the_last_hotkey_change_left_in_the_inbox() {
    let mut tray = running(Vietnamese);
    assert_eq!(choose(&mut tray, TrayCommand::Exit), exit_effects());
    assert_eq!(
        tray.flush_inbox(vec![
            Event::Hotkey(false),
            Event::Answered(Question::Recover),
            Event::Hotkey(true),
            Event::Hotkey(false),
        ]),
        [worker(
            1,
            WorkerCommand::Apply(Change::VietnameseMode(false))
        )]
    );
    assert_eq!(tray.mode(), English);
    assert_eq!(
        tray.flush_inbox(vec![Event::Answered(Question::Recover)]),
        []
    );
}

// --- The icon ------------------------------------------------------------------------------

/// A click switches the mode at once: the icon, the hook (its mode only), and the worker,
/// which saves it.
#[test]
fn a_click_or_the_keyboard_switches_the_mode_of_the_icon_the_hook_and_the_file() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        send(&mut tray, click()),
        answered(vec![
            Effect::UpdateIcon(icon(English, InputMethod::Telex)),
            Effect::HookMode(false),
            worker(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
        ])
    );
    assert_eq!(tray.mode(), English);
    assert!(!tray.runtime().vietnamese_mode);
    assert!(!tray.runtime().options.enabled);
    assert_eq!(
        send(&mut tray, callback(NIN_KEYSELECT, TRAY_ICON_ID, 0, 0)),
        answered(vec![
            Effect::UpdateIcon(icon(Vietnamese, InputMethod::Telex)),
            Effect::HookMode(true),
            worker(2, WorkerCommand::Apply(Change::VietnameseMode(true))),
        ])
    );
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn a_right_click_opens_the_menu_at_its_anchor() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        send(&mut tray, callback(WM_CONTEXTMENU, TRAY_ICON_ID, -20, 300)),
        answered(vec![Effect::OpenMenu { x: -20, y: 300 }])
    );
    assert_eq!(tray.mode(), Vietnamese, "opening the menu changes nothing");
}

#[test]
fn other_notifications_and_other_icons_change_nothing() {
    let mut tray = running(Vietnamese);
    for code in [
        WM_MOUSEMOVE,
        WM_LBUTTONDOWN,
        WM_LBUTTONUP,
        WM_RBUTTONUP,
        NIN_POPUPOPEN,
    ] {
        assert_eq!(
            send(&mut tray, callback(code, TRAY_ICON_ID, 1, 2)),
            answered(Vec::new()),
            "{code:#x}"
        );
    }
    for code in [NIN_SELECT, NIN_KEYSELECT, WM_CONTEXTMENU, WM_LBUTTONDBLCLK] {
        assert_eq!(
            send(&mut tray, callback(code, TRAY_ICON_ID + 1, 1, 2)),
            answered(Vec::new()),
            "another icon, {code:#x}"
        );
    }
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn before_the_start_the_icon_does_not_exist_and_nothing_acts() {
    let mut tray = Tray::new(start(Vietnamese), messages());
    assert!(!tray.is_running());
    for message in [
        click(),
        callback(WM_CONTEXTMENU, TRAY_ICON_ID, 1, 2),
        taskbar_created(),
        tick(),
    ] {
        assert_eq!(
            send(&mut tray, message),
            answered(Vec::new()),
            "{message:x?}"
        );
    }
    assert_eq!(choose(&mut tray, TrayCommand::VietnameseMode), []);
    assert_eq!(tray.on_event(Event::Hotkey(false)), []);
    assert_eq!(tray.answered_yes(Question::Recover), []);
    assert_eq!(tray.mode(), Vietnamese);
    assert_eq!(
        send(&mut tray, (START.id(), 0, 0)),
        answered(started(icon(Vietnamese, InputMethod::Telex)))
    );
    assert!(tray.is_running());
}

#[test]
fn matrix_58_taskbar_created_adds_the_icon_again_without_changing_the_mode() {
    let mut tray = running(Vietnamese);
    send(&mut tray, click());
    for _ in 0..2 {
        assert_eq!(
            send(&mut tray, taskbar_created()),
            answered(vec![Effect::ShowIcon(icon(English, InputMethod::Telex))])
        );
        assert_eq!(tray.mode(), English);
    }
}

// --- The double-click (D46) -----------------------------------------------------------------

/// The notification `code` of the tray icon (the anchor does not matter).
fn icon_event(code: u32) -> Message {
    callback(code, TRAY_ICON_ID, 30, 40)
}

/// The left button is pressed on the icon.
fn press() -> Message {
    icon_event(WM_LBUTTONDOWN)
}

/// The left button is released.
fn release() -> Message {
    icon_event(WM_LBUTTONUP)
}

/// The second press of a double-click.
fn double_click() -> Message {
    icon_event(WM_LBUTTONDBLCLK)
}

/// The effects of a switch to `mode` (Telex) whose save is command `seq`.
fn switched(mode: InputMode, seq: u64) -> Vec<Effect> {
    let vietnamese = mode.is_vietnamese();
    vec![
        Effect::UpdateIcon(icon(mode, InputMethod::Telex)),
        Effect::HookMode(vietnamese),
        worker(
            seq,
            WorkerCommand::Apply(Change::VietnameseMode(vietnamese)),
        ),
    ]
}

/// `effects` followed by `more`.
fn then(mut effects: Vec<Effect>, more: Vec<Effect>) -> Vec<Effect> {
    effects.extend(more);
    effects
}

/// A press, a release, and the `NIN_SELECT` of a click: the mode switches at once, at the
/// select (an input method must switch before the user types; nothing waits for a second
/// click).
#[test]
fn a_click_switches_at_its_select_without_waiting() {
    let mut tray = running(Vietnamese);
    assert_eq!(send_at(&mut tray, press(), 1_000), answered(Vec::new()));
    assert_eq!(send_at(&mut tray, release(), 1_080), answered(Vec::new()));
    assert_eq!(
        send_at(&mut tray, click(), 1_090),
        answered(switched(English, 1))
    );
    assert_eq!(tray.mode(), English);
}

/// The shell reports a double-click as press, release, select, double-click, release,
/// select: the first click switched the mode, the double-click switches it back (through the
/// way of every mode change: icon, hook, file) and opens the panel as the menu does, and the
/// select of the second click is swallowed.
#[test]
fn a_double_click_opens_the_panel_and_keeps_the_mode() {
    let mut tray = running(Vietnamese);
    let state = tray.panel_state();
    send_at(&mut tray, press(), 1_000);
    send_at(&mut tray, release(), 1_080);
    assert_eq!(
        send_at(&mut tray, click(), 1_090),
        answered(switched(English, 1))
    );
    assert_eq!(
        send_at(&mut tray, double_click(), 1_250),
        answered(then(
            switched(Vietnamese, 2),
            vec![open_panel(1, state.clone())]
        ))
    );
    assert_eq!(send_at(&mut tray, release(), 1_330), answered(Vec::new()));
    assert_eq!(send_at(&mut tray, click(), 1_340), answered(Vec::new()));
    assert_eq!(tray.mode(), Vietnamese);
    assert!(tray.runtime().vietnamese_mode);
    assert_eq!(tray.panel_state(), state);

    // From English likewise, and a double-click while the panel is open brings it to the
    // front (a new opening).
    let mut tray = running(English);
    send_at(&mut tray, click(), 10_000);
    let effects = send_at(&mut tray, double_click(), 10_200).effects;
    assert_eq!(effects[..3], switched(English, 2)[..]);
    assert!(matches!(effects[3], Effect::OpenPanel { serial: 1, .. }));
    assert_eq!(tray.mode(), English);
    send_at(&mut tray, click(), 10_300);
    send_at(&mut tray, click(), 20_000);
    let effects = send_at(&mut tray, double_click(), 20_100).effects;
    assert!(matches!(
        effects.last(),
        Some(Effect::OpenPanel { serial: 2, .. })
    ));
    assert_eq!(tray.mode(), English);
}

/// When the shell sends no select after the double-click, nothing is swallowed: a click that
/// follows, even within the double-click time, switches the mode, and so does one after it.
#[test]
fn without_a_second_select_the_next_click_still_switches() {
    let mut tray = running(Vietnamese);
    send_at(&mut tray, press(), 1_000);
    send_at(&mut tray, release(), 1_080);
    send_at(&mut tray, click(), 1_090);
    send_at(&mut tray, double_click(), 1_250);
    send_at(&mut tray, release(), 1_330);
    assert_eq!(tray.mode(), Vietnamese);
    // Within the double-click time of the double-click: its press shows it is a new click
    // (the open panel follows it).
    assert_eq!(send_at(&mut tray, press(), 1_400), answered(Vec::new()));
    let effects = send_at(&mut tray, click(), 1_450).effects;
    assert_eq!(
        effects,
        then(switched(English, 3), vec![refresh(tray.panel_state())])
    );

    // A select alone more than the double-click time after the release of the second press
    // is a click too; within it, it is the second click's select, late.
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_090);
    send_at(&mut tray, double_click(), 1_250);
    send_at(&mut tray, release(), 1_330);
    let effects = send_at(&mut tray, click(), 1_831).effects;
    assert_eq!(
        effects,
        then(switched(English, 3), vec![refresh(tray.panel_state())])
    );
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_090);
    send_at(&mut tray, double_click(), 1_250);
    send_at(&mut tray, release(), 1_330);
    assert_eq!(send_at(&mut tray, click(), 1_830), answered(Vec::new()));

    // A shell that reports no release: a select more than the double-click time and 2 s after
    // the double-click is a click.
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_090);
    send_at(&mut tray, double_click(), 1_250);
    let effects = send_at(&mut tray, click(), 3_751).effects;
    assert_eq!(
        effects,
        then(switched(English, 3), vec![refresh(tray.panel_state())])
    );
}

/// The panels that `effects` open.
fn panels_opened(effects: &[Effect]) -> usize {
    effects
        .iter()
        .filter(|effect| matches!(effect, Effect::OpenPanel { .. }))
        .count()
}

/// Sends each message at its time to `tray`; the number of panels they opened.
fn play(tray: &mut Tray, messages: &[(Message, u32)]) -> usize {
    messages
        .iter()
        .map(|&(message, at)| panels_opened(&send_at(tray, message, at).effects))
        .sum()
}

/// A double-click as the shell reports it, from its first press at `at`: press, release,
/// select, double-click, release, select.
fn double_click_at(at: u32) -> [(Message, u32); 6] {
    [
        (press(), at),
        (release(), at + 60),
        (click(), at + 70),
        (double_click(), at + 150),
        (release(), at + 210),
        (click(), at + 220),
    ]
}

/// The select of the second click comes when the second press is released, however long it
/// was held: it is swallowed within the double-click time after that release, whenever the
/// double-click itself came, and a tray that sees no release swallows it within the
/// double-click time and 2 s after the double-click (the reviewer's probe: 200 ms, the second
/// press held 240 ms). The mode stays as it was, and the panel opens once.
#[test]
fn the_select_of_a_second_press_held_long_is_swallowed() {
    let mut tray = running_with(start(Vietnamese)).with_double_click_ms(200);
    let opened = play(
        &mut tray,
        &[
            (press(), 1_000),
            (release(), 1_070),
            (click(), 1_080),
            (double_click(), 1_180),
            (release(), 1_415),
            (click(), 1_420),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, Vietnamese));

    let mut tray = running_with(start(Vietnamese)).with_double_click_ms(200);
    let opened = play(
        &mut tray,
        &[
            (press(), 1_000),
            (click(), 1_080),
            (double_click(), 1_180),
            (click(), 1_420),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, Vietnamese));

    // Held for seconds while the poll timer ticks.
    let mut tray = running_with(start(Vietnamese)).with_double_click_ms(200);
    let opened = play(
        &mut tray,
        &[
            (press(), 1_000),
            (release(), 1_070),
            (click(), 1_080),
            (double_click(), 1_180),
            (tick(), 3_180),
            (tick(), 5_180),
            (tick(), 7_180),
            (release(), 8_000),
            (click(), 8_010),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, Vietnamese));
    // The click after it is a click.
    play(
        &mut tray,
        &[(press(), 8_100), (release(), 8_150), (click(), 8_160)],
    );
    assert_eq!(tray.mode(), English);
}

/// The first select may be handled late after its press, and the double-click late after it
/// (the first switch waited on a busy Explorer): the double-click still restores the mode from
/// before its first click, within the double-click time and 2 s after that click (the
/// reviewer's probe: select at 1100, double-click at 1700).
#[test]
fn a_late_double_click_still_restores_the_mode_before_its_first_click() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        play(&mut tray, &[(click(), 1_100), (double_click(), 1_700)]),
        1
    );
    assert_eq!(tray.mode(), Vietnamese);

    let late = |double_click_at: u32| {
        [
            (press(), 1_000),
            (release(), 1_090),
            (click(), 1_100),
            (double_click(), double_click_at),
            (release(), double_click_at + 10),
            (click(), double_click_at + 20),
        ]
    };
    let mut tray = running(Vietnamese);
    assert_eq!(play(&mut tray, &late(3_600)), 1);
    assert_eq!(tray.mode(), Vietnamese);
    // Beyond that bound the click is not its first one: the panel opens, the switch stays.
    let mut tray = running(Vietnamese);
    assert_eq!(play(&mut tray, &late(3_601)), 1);
    assert_eq!(tray.mode(), English);
}

/// Once the shell reports presses, its double-click is the only double-click: a second press
/// that is no double-click (too late, or the mouse moved) comes as a press, and two such
/// clicks switch twice, however close their selects (the reviewer's probe: presses 520 ms
/// apart at 500 ms, the first held 400 ms). A double-click reported once is as good as a press.
#[test]
fn with_presses_reported_two_selects_are_two_clicks() {
    let mut tray = running(Vietnamese);
    send_at(&mut tray, press(), 1_000);
    send_at(&mut tray, release(), 1_400);
    assert_eq!(
        send_at(&mut tray, click(), 1_400),
        answered(switched(English, 1))
    );
    send_at(&mut tray, press(), 1_520);
    send_at(&mut tray, release(), 1_600);
    assert_eq!(
        send_at(&mut tray, click(), 1_600),
        answered(switched(Vietnamese, 2))
    );

    // Two quick clicks that the shell did not take for a double-click.
    let mut tray = running(Vietnamese);
    send_at(&mut tray, press(), 1_000);
    send_at(&mut tray, release(), 1_050);
    assert_eq!(
        send_at(&mut tray, click(), 1_060),
        answered(switched(English, 1))
    );
    send_at(&mut tray, press(), 1_150);
    send_at(&mut tray, release(), 1_200);
    assert_eq!(
        send_at(&mut tray, click(), 1_210),
        answered(switched(Vietnamese, 2))
    );

    // One press is enough, for good.
    let mut tray = running(Vietnamese);
    play(
        &mut tray,
        &[(press(), 1_000), (release(), 1_050), (click(), 1_060)],
    );
    assert_eq!(
        send_at(&mut tray, click(), 30_000),
        answered(switched(Vietnamese, 2))
    );
    assert_eq!(
        send_at(&mut tray, click(), 30_100),
        answered(switched(English, 3))
    );

    // So is one double-click.
    let mut tray = running(Vietnamese);
    play(&mut tray, &[(click(), 1_000), (double_click(), 1_100)]);
    assert_eq!(tray.mode(), Vietnamese);
    assert_eq!(
        send_at(&mut tray, click(), 10_000).effects[..3],
        switched(English, 3)[..]
    );
    assert_eq!(
        send_at(&mut tray, click(), 10_100).effects[..3],
        switched(Vietnamese, 4)[..]
    );
}

/// A triple click is a double-click and a click (the shell reports the third press as a
/// press); a quadruple click is two double-clicks. With or without the second selects.
#[test]
fn a_triple_click_switches_once_and_a_quadruple_click_opens_the_panel_twice() {
    let mut tray = running(Vietnamese);
    let mut opened = play(&mut tray, &double_click_at(1_000));
    opened += play(
        &mut tray,
        &[(press(), 1_300), (release(), 1_360), (click(), 1_370)],
    );
    assert_eq!((opened, tray.mode()), (1, English));
    opened += play(
        &mut tray,
        &[
            (double_click(), 1_450),
            (release(), 1_510),
            (click(), 1_520),
        ],
    );
    assert_eq!((opened, tray.mode()), (2, Vietnamese));

    let mut tray = running(Vietnamese);
    let opened = play(
        &mut tray,
        &[
            (press(), 1_000),
            (release(), 1_060),
            (click(), 1_070),
            (double_click(), 1_150),
            (release(), 1_210),
            (press(), 1_300),
            (release(), 1_360),
            (click(), 1_370),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, English));
    let opened = play(&mut tray, &[(double_click(), 1_450), (release(), 1_510)]);
    assert_eq!((opened, tray.mode()), (1, Vietnamese));
}

/// A real click right after a double-click switches the mode, within the double-click time
/// or after it.
#[test]
fn a_click_right_after_a_double_click_switches() {
    for after in [10, 499, 500, 501, 5_000] {
        let mut tray = running(Vietnamese);
        assert_eq!(play(&mut tray, &double_click_at(1_000)), 1);
        let first = 1_220 + after;
        play(
            &mut tray,
            &[
                (press(), first),
                (release(), first + 50),
                (click(), first + 60),
            ],
        );
        assert_eq!(tray.mode(), English, "{after} ms after the double-click");
    }
}

/// A busy tray handles the shell's messages late, then close together: the times they are
/// handled at count, and a double-click still keeps the mode and opens the panel once.
#[test]
fn a_busy_tray_still_tells_a_double_click() {
    // Busy after the first switch.
    let mut tray = running(Vietnamese);
    let opened = play(
        &mut tray,
        &[
            (press(), 1_000),
            (release(), 1_060),
            (click(), 1_070),
            (double_click(), 3_000),
            (release(), 3_001),
            (click(), 3_002),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, Vietnamese));
    // Busy after the double-click (the panel opening).
    let mut tray = running(Vietnamese);
    let opened = play(
        &mut tray,
        &[
            (press(), 1_000),
            (release(), 1_060),
            (click(), 1_070),
            (double_click(), 1_150),
            (release(), 6_000),
            (click(), 6_001),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, Vietnamese));
    // Busy from the first press on: everything at once.
    let mut tray = running(Vietnamese);
    let opened = play(
        &mut tray,
        &[
            (press(), 9_000),
            (release(), 9_000),
            (click(), 9_000),
            (double_click(), 9_000),
            (release(), 9_000),
            (click(), 9_000),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, Vietnamese));
}

/// The clock wraps around after 49.7 days, so a remembered click or double-click whose time
/// has passed is forgotten at the next tick of the poll timer: it does not come back when the
/// clock comes round to its time again. A tick forgets nothing whose time has not passed.
#[test]
fn a_tick_forgets_a_click_and_a_double_click_whose_time_has_passed() {
    // A shell without presses: the click.
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_000);
    send_at(&mut tray, tick(), 4_000);
    assert_eq!(
        send_at(&mut tray, click(), 1_300),
        answered(switched(Vietnamese, 3))
    );
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_000);
    send_at(&mut tray, tick(), 1_200);
    assert_eq!(
        play(&mut tray, &[(click(), 1_300)]),
        1,
        "a fresh click is kept"
    );

    // The double-click whose second select never came, released or not.
    for released in [true, false] {
        let mut tray = running(Vietnamese);
        play(
            &mut tray,
            &[(press(), 1_000), (release(), 1_060), (click(), 1_070)],
        );
        send_at(&mut tray, double_click(), 1_150);
        if released {
            send_at(&mut tray, release(), 1_210);
        }
        send_at(&mut tray, tick(), 4_000);
        assert_eq!(
            send_at(&mut tray, click(), 1_300).effects[..3],
            switched(English, 4)[..],
            "released: {released}"
        );
    }
}

/// The glue gives the tray the user's double-click time again at every tick of the poll
/// timer: a change in the mouse settings counts from then on.
#[test]
fn the_double_click_time_may_change_while_the_tray_runs() {
    let mut tray = running(Vietnamese);
    tray.set_double_click_ms(300);
    send_at(&mut tray, click(), 1_000);
    assert_eq!(
        send_at(&mut tray, click(), 1_301),
        answered(switched(Vietnamese, 2))
    );
    tray.set_double_click_ms(900);
    send_at(&mut tray, click(), 3_000);
    assert_eq!(play(&mut tray, &[(click(), 3_900)]), 1);
    assert_eq!(tray.mode(), Vietnamese);
}

/// A shell that reports no double-click: a second select within the double-click time of the
/// first makes the double-click; one after it is a click of its own.
#[test]
fn two_selects_within_the_double_click_time_make_a_double_click() {
    let mut tray = running(Vietnamese);
    let state = tray.panel_state();
    assert_eq!(
        send_at(&mut tray, click(), 1_000),
        answered(switched(English, 1))
    );
    assert_eq!(
        send_at(&mut tray, click(), 1_500),
        answered(then(switched(Vietnamese, 2), vec![open_panel(1, state)]))
    );
    assert_eq!(tray.mode(), Vietnamese);
    // The double-click took both clicks: the next one is a click (the open panel follows it).
    let effects = send_at(&mut tray, click(), 1_600).effects;
    assert_eq!(
        effects,
        then(switched(English, 3), vec![refresh(tray.panel_state())])
    );
}

#[test]
fn two_selects_further_apart_switch_twice() {
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_000);
    assert_eq!(
        send_at(&mut tray, click(), 1_501),
        answered(switched(Vietnamese, 2))
    );
    // The user's double-click time counts, to the millisecond.
    let mut tray = running_with(start(Vietnamese)).with_double_click_ms(300);
    send_at(&mut tray, click(), 1_000);
    assert_eq!(
        send_at(&mut tray, click(), 1_301),
        answered(switched(Vietnamese, 2))
    );
    let mut tray = running_with(start(Vietnamese)).with_double_click_ms(300);
    send_at(&mut tray, click(), 1_000);
    let effects = send_at(&mut tray, click(), 1_300).effects;
    assert!(matches!(effects.last(), Some(Effect::OpenPanel { .. })));
}

/// The clock wraps around after 49.7 days: times are compared by their difference.
#[test]
fn the_double_click_time_is_measured_across_the_wrap_of_the_clock() {
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), u32::MAX - 99);
    let effects = send_at(&mut tray, click(), 200).effects;
    assert!(matches!(effects.last(), Some(Effect::OpenPanel { .. })));
    assert_eq!(tray.mode(), Vietnamese);

    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), u32::MAX - 300);
    send_at(&mut tray, double_click(), u32::MAX - 10);
    assert_eq!(send_at(&mut tray, click(), 50), answered(Vec::new()));
    assert_eq!(tray.mode(), Vietnamese);

    // The shell's double-click across the wrap.
    let mut tray = running(Vietnamese);
    let opened = play(
        &mut tray,
        &[
            (press(), u32::MAX - 150),
            (release(), u32::MAX - 100),
            (click(), u32::MAX - 90),
            (double_click(), 49),
            (release(), 120),
            (click(), 130),
        ],
    );
    assert_eq!((opened, tray.mode()), (1, Vietnamese));

    // A clock that went back is far apart, never a double-click.
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_000);
    assert_eq!(
        send_at(&mut tray, click(), 900),
        answered(switched(Vietnamese, 2))
    );
}

/// The double-click restores the mode from before its first click only when that click came
/// within the double-click time and 2 s before it, and only when the mode is not back already
/// (the hotkey).
#[test]
fn a_double_click_restores_only_a_mode_its_first_click_changed() {
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_000);
    tray.on_event(Event::Hotkey(true));
    assert_eq!(tray.mode(), Vietnamese);
    let effects = send_at(&mut tray, double_click(), 1_200).effects;
    assert!(
        matches!(effects[..], [Effect::OpenPanel { serial: 1, .. }]),
        "{effects:?}"
    );
    assert_eq!(tray.mode(), Vietnamese);

    // A double-click without a click shortly before opens the panel and changes nothing.
    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_000);
    let effects = send_at(&mut tray, double_click(), 3_501).effects;
    assert!(
        matches!(effects[..], [Effect::OpenPanel { serial: 1, .. }]),
        "{effects:?}"
    );
    assert_eq!(tray.mode(), English);
}

/// The keyboard selects the icon one switch at a time; it never makes a double-click, and a
/// right click in between ends a double-click too.
#[test]
fn the_keyboard_and_the_menu_never_make_a_double_click() {
    let key_select = || icon_event(NIN_KEYSELECT);
    let mut tray = running(Vietnamese);
    assert_eq!(
        send_at(&mut tray, key_select(), 1_000),
        answered(switched(English, 1))
    );
    assert_eq!(
        send_at(&mut tray, key_select(), 1_100),
        answered(switched(Vietnamese, 2))
    );
    assert_eq!(
        send_at(&mut tray, click(), 1_200),
        answered(switched(English, 3))
    );
    assert_eq!(
        send_at(&mut tray, key_select(), 1_300),
        answered(switched(Vietnamese, 4))
    );
    assert_eq!(
        send_at(&mut tray, click(), 1_400),
        answered(switched(English, 5))
    );
    send_at(
        &mut tray,
        callback(WM_CONTEXTMENU, TRAY_ICON_ID, 1, 2),
        1_500,
    );
    assert_eq!(
        send_at(&mut tray, click(), 1_600),
        answered(switched(Vietnamese, 6))
    );

    // Between the first click and the shell's double-click, they take that click from the
    // double-click: the panel opens, and the mode stays as they left it.
    let menu = || callback(WM_CONTEXTMENU, TRAY_ICON_ID, 1, 2);
    let second = [
        (double_click(), 1_150),
        (release(), 1_210),
        (click(), 1_220),
    ];
    let mut tray = running(Vietnamese);
    play(
        &mut tray,
        &[(press(), 1_000), (release(), 1_060), (click(), 1_070)],
    );
    send_at(&mut tray, menu(), 1_100);
    assert_eq!(play(&mut tray, &second), 1);
    assert_eq!(tray.mode(), English);
    let mut tray = running(Vietnamese);
    play(
        &mut tray,
        &[(press(), 1_000), (release(), 1_060), (click(), 1_070)],
    );
    send_at(&mut tray, key_select(), 1_100);
    send_at(&mut tray, key_select(), 1_110);
    assert_eq!(play(&mut tray, &second), 1);
    assert_eq!(tray.mode(), English);
}

#[test]
fn a_double_click_before_the_start_or_after_the_exit_does_nothing() {
    let mut tray = Tray::new(start(Vietnamese), messages());
    for (message, at) in double_click_at(1_000) {
        assert_eq!(send_at(&mut tray, message, at), answered(Vec::new()));
    }
    // Nothing of it was kept: a shell that reports no presses, as far as the tray knows.
    send(&mut tray, (START.id(), 0, 0));
    assert_eq!(play(&mut tray, &[(click(), 2_000), (click(), 2_100)]), 1);
    assert_eq!(tray.mode(), Vietnamese);

    let mut tray = running(Vietnamese);
    send_at(&mut tray, click(), 1_000);
    choose(&mut tray, TrayCommand::Exit);
    for (message, at) in [
        (press(), 1_050),
        (double_click(), 1_100),
        (release(), 1_150),
        (click(), 1_200),
    ] {
        assert_eq!(send_at(&mut tray, message, at), answered(Vec::new()));
    }
    assert_eq!(tray.mode(), English);
}

// --- The excluded application in front (D47) ----------------------------------------------

/// The tooltip while Remote Desktop Connection is in front.
const MSTSC_TOOLTIP: &str = "Raumakey: đang gõ tiếng Anh trong mstsc.exe (ứng dụng loại trừ)";

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

/// The application `name` in front, its window class unknown.
fn app(name: &str) -> Option<ForegroundApp> {
    Some(ForegroundApp {
        exe: exe(name),
        window_class: None,
    })
}

/// The application `name` in front with a window of `class`.
fn app_in(name: &str, class: &str) -> Option<ForegroundApp> {
    Some(ForegroundApp {
        exe: exe(name),
        window_class: Some(class.to_owned()),
    })
}

/// A rule for `name` (narrowed to `class`) that allows Vietnamese or not.
fn rule(name: &str, class: Option<&str>, vietnamese: bool) -> AppRule {
    AppRule {
        exe: exe(name),
        window_class: class.map(str::to_owned),
        action: RuleAction {
            vietnamese: Some(vietnamese),
            macros: Some(vietnamese),
            ..RuleAction::default()
        },
    }
}

/// The runtime snapshot of `mode` with the rule that `[exclusions]` makes for `mstsc.exe`
/// first among the compatibility rules.
fn excluding_mstsc(mode: InputMode) -> RuntimeSnapshot {
    let mut runtime = runtime(mode);
    runtime
        .compatibility
        .rules
        .insert(0, rule("mstsc.exe", None, false));
    runtime
}

/// A running tray in `mode` whose rules exclude `mstsc.exe`.
fn running_excluding_mstsc(mode: InputMode) -> Tray {
    let mut start = start(mode);
    start.runtime = excluding_mstsc(mode);
    running_with(start)
}

/// The icon while `name` is in front (Telex in effect).
fn excluded_icon(name: &str) -> IconState {
    IconState::excluded(exe(name), InputMethod::Telex)
}

#[test]
fn the_tooltip_of_an_excluded_application_names_it() {
    let shown = excluded_icon("mstsc.exe");
    assert_eq!(shown.tooltip(), MSTSC_TOOLTIP);
    assert_eq!(shown.mode, English);
    assert_eq!(shown.icon_id(), English.icon_id());
    assert_eq!(
        IconState::excluded(exe("mstsc.exe"), InputMethod::Vni).tooltip(),
        MSTSC_TOOLTIP,
        "the method is not named"
    );
    // The shell keeps 127 units: a long name loses characters in its middle, never the start,
    // its end, or the words around it, and a character outside the BMP is never split.
    let before = "Raumakey: đang gõ tiếng Anh trong ";
    let after = " (ứng dụng loại trừ)";
    for name in [
        format!("{}.exe", "a".repeat(251)),
        format!("{}.exe", "đ".repeat(120)),
        format!("{}x.exe", "😀".repeat(100)),
    ] {
        let tooltip = IconState::excluded(exe(&name), InputMethod::Telex).tooltip();
        assert!(
            tooltip.encode_utf16().count() < TOOLTIP_CAPACITY,
            "{tooltip}"
        );
        assert!(tooltip.starts_with(before), "{tooltip}");
        assert!(tooltip.ends_with(&format!(".exe{after}")), "{tooltip}");
        let shown = &tooltip[before.len()..tooltip.len() - after.len()];
        let (head, tail) = shown.split_once('\u{2026}').unwrap();
        assert!(name.starts_with(head) && name.ends_with(tail), "{shown}");
        assert!(head.encode_utf16().count() >= 30, "{shown}");
        assert!(
            tooltip.encode_utf16().count() >= TOOLTIP_CAPACITY - 3,
            "{tooltip}"
        );
    }
    // A name that fits is shown whole.
    let fits = format!("{}.exe", "b".repeat(69));
    let tooltip = IconState::excluded(exe(&fits), InputMethod::Telex).tooltip();
    assert_eq!(tooltip, format!("{before}{fits}{after}"));
    assert_eq!(tooltip.encode_utf16().count(), TOOLTIP_CAPACITY - 1);
}

/// The foreground watcher's message has the glue read the window it names.
#[test]
fn the_message_of_the_foreground_watcher_reads_the_window_it_names() {
    let mut tray = Tray::new(start(Vietnamese), messages());
    assert_eq!(
        send(&mut tray, (FOREGROUND.id(), 0x1234, 0)),
        answered(Vec::new()),
        "before the start"
    );
    assert_eq!(tray.on_foreground(app("mstsc.exe")), []);
    let mut tray = running(Vietnamese);
    assert_eq!(
        send(&mut tray, (FOREGROUND.id(), 0x1234, 0)),
        answered(vec![Effect::ReadForeground(Some(0x1234))])
    );
    choose(&mut tray, TrayCommand::Exit);
    assert_eq!(
        send(&mut tray, (FOREGROUND.id(), 0x1234, 0)),
        answered(Vec::new())
    );
    assert_eq!(tray.on_foreground(app("mstsc.exe")), []);
}

/// Remote Desktop Connection in front: E, and the tooltip names it; the mode in effect (the
/// file, the menu, the panel, the hook) does not change, and it shows again once another
/// application is in front.
#[test]
fn an_excluded_application_in_front_shows_e_and_names_it() {
    let mut tray = running_excluding_mstsc(Vietnamese);
    let panel = tray.panel_state();
    assert_eq!(
        tray.on_foreground(app("mstsc.exe")),
        [Effect::UpdateIcon(excluded_icon("mstsc.exe"))]
    );
    assert_eq!(tray.icon(), excluded_icon("mstsc.exe"));
    assert_eq!(tray.mode(), Vietnamese);
    assert!(checked(&tray, TrayCommand::VietnameseMode));
    assert_eq!(tray.panel_state(), panel);
    // The same application again changes nothing.
    assert_eq!(tray.on_foreground(app("mstsc.exe")), []);
    assert_eq!(
        tray.on_foreground(app("notepad.exe")),
        [Effect::UpdateIcon(icon(Vietnamese, InputMethod::Telex))]
    );
    assert_eq!(tray.on_foreground(app("notepad.exe")), []);
    // In English mode the tooltip still says why.
    let mut tray = running_excluding_mstsc(English);
    assert_eq!(
        tray.on_foreground(app("mstsc.exe")),
        [Effect::UpdateIcon(excluded_icon("mstsc.exe"))]
    );
    assert_eq!(
        tray.on_foreground(app("explorer.exe")),
        [Effect::UpdateIcon(icon(English, InputMethod::Telex))]
    );
}

/// The hotkey still switches the mode in effect while an excluded application is in front:
/// it is saved and the hook gets it, the icon stays E, and the new mode shows once the
/// application leaves.
#[test]
fn the_hotkey_while_excluded_switches_the_mode_in_effect_and_the_icon_stays_e() {
    let mut tray = running_excluding_mstsc(Vietnamese);
    tray.on_foreground(app("mstsc.exe"));
    assert_eq!(
        tray.on_event(Event::Hotkey(false)),
        [
            Effect::HookMode(false),
            worker(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
        ]
    );
    assert_eq!(tray.mode(), English);
    assert!(!checked(&tray, TrayCommand::VietnameseMode));
    assert_eq!(tray.icon(), excluded_icon("mstsc.exe"));
    // A select that arrives while it is still in front acts on the mode in effect too.
    assert_eq!(
        send(&mut tray, click()),
        answered(vec![
            Effect::HookMode(true),
            worker(2, WorkerCommand::Apply(Change::VietnameseMode(true))),
        ])
    );
    assert_eq!(
        tray.on_foreground(app("notepad.exe")),
        [Effect::UpdateIcon(icon(Vietnamese, InputMethod::Telex))]
    );
}

/// A click on the icon brings the taskbar (Explorer) to the front first: the icon shows the
/// mode in effect, the click switches it as always, and Remote Desktop Connection shows E
/// again when it comes back.
#[test]
fn a_click_acts_on_the_mode_in_effect_after_the_taskbar_came_to_the_front() {
    let mut tray = running_excluding_mstsc(Vietnamese);
    tray.on_foreground(app("mstsc.exe"));
    assert_eq!(
        tray.on_foreground(app_in("explorer.exe", "Shell_TrayWnd")),
        [Effect::UpdateIcon(icon(Vietnamese, InputMethod::Telex))]
    );
    assert_eq!(send(&mut tray, click()), answered(switched(English, 1)));
    assert_eq!(
        tray.on_foreground(app("mstsc.exe")),
        [Effect::UpdateIcon(excluded_icon("mstsc.exe"))]
    );
    assert_eq!(
        tray.on_foreground(app("notepad.exe")),
        [Effect::UpdateIcon(icon(English, InputMethod::Telex))]
    );
}

/// A new runtime snapshot (a hand edit of `[exclusions]`, through the worker) applies to the
/// application in front at once.
#[test]
fn a_new_snapshot_applies_to_the_application_in_front_at_once() {
    let mut tray = running_excluding_mstsc(Vietnamese);
    tray.on_foreground(app("mstsc.exe"));
    let without = runtime(Vietnamese);
    assert_eq!(
        tray.on_event(answer(
            0,
            polled(clean()),
            Some(state(without.clone(), StartupMode::None))
        )),
        [
            Effect::UpdateIcon(icon(Vietnamese, InputMethod::Telex)),
            hook_runtime(without),
        ]
    );
    let with = excluding_mstsc(Vietnamese);
    assert_eq!(
        tray.on_event(answer(
            0,
            polled(clean()),
            Some(state(with.clone(), StartupMode::None))
        )),
        [
            Effect::UpdateIcon(excluded_icon("mstsc.exe")),
            hook_runtime(with),
        ]
    );
    // A snapshot that excludes another application.
    let mut other = runtime(Vietnamese);
    other.compatibility.rules = vec![rule("vmconnect.exe", None, false)];
    tray.on_event(answer(
        0,
        polled(clean()),
        Some(state(other, StartupMode::None)),
    ));
    assert_eq!(tray.icon(), icon(Vietnamese, InputMethod::Telex));
    assert_eq!(
        tray.on_foreground(app("vmconnect.exe")),
        [Effect::UpdateIcon(excluded_icon("vmconnect.exe"))]
    );
}

/// Only a rule that forbids Vietnamese excludes; an application that cannot be read is not
/// excluded; a rule narrowed to a window class needs that class.
#[test]
fn only_a_known_application_whose_rules_forbid_vietnamese_is_excluded() {
    let mut tray = running_excluding_mstsc(Vietnamese);
    tray.on_foreground(app("mstsc.exe"));
    assert_eq!(
        tray.on_foreground(None),
        [Effect::UpdateIcon(icon(Vietnamese, InputMethod::Telex))]
    );
    assert_eq!(tray.on_foreground(None), []);

    let mut start = start(Vietnamese);
    start.runtime.compatibility.rules = vec![
        rule("rdcman.exe", Some("WindowsForms10.Window.8.app.0.1"), false),
        rule("code.exe", None, true),
    ];
    let mut tray = running_with(start);
    assert_eq!(tray.on_foreground(app("code.exe")), []);
    assert_eq!(tray.on_foreground(app("rdcman.exe")), []);
    assert_eq!(tray.on_foreground(app_in("rdcman.exe", "Other")), []);
    assert_eq!(
        tray.on_foreground(app_in("rdcman.exe", "windowsforms10.window.8.app.0.1")),
        [Effect::UpdateIcon(excluded_icon("rdcman.exe"))]
    );
}

/// The application in front is known by the class the typing hook resolves the rules with:
/// the class of the window with the keyboard focus in the foreground window's thread, else
/// the class of the foreground window itself. So a rule narrowed to a class (in
/// `raumakey-app-rules.toml`, the one way past `[exclusions]`, design §22.7) shows on the icon
/// as it acts on typing.
#[test]
fn the_application_in_front_is_known_by_the_class_of_its_focus() {
    let class = |name: &str| Some(name.to_owned());
    let mstsc = exe("mstsc.exe");
    let in_edit = ForegroundApp::new(mstsc.clone(), class("#32770"), class("Edit"));
    assert_eq!(in_edit.exe, mstsc);
    assert_eq!(in_edit.window_class, class("Edit"));
    let no_focus = ForegroundApp::new(mstsc.clone(), class("#32770"), None);
    assert_eq!(no_focus.window_class, class("#32770"));
    assert_eq!(ForegroundApp::new(mstsc, None, None).window_class, None);

    // `[exclusions]` lists mstsc.exe, and an app rule lets the edit boxes of its dialog type
    // Vietnamese.
    let mut start = start(Vietnamese);
    start.runtime = excluding_mstsc(Vietnamese);
    start
        .runtime
        .compatibility
        .rules
        .push(rule("mstsc.exe", Some("Edit"), true));
    let mut tray = running_with(start);
    assert_eq!(
        tray.on_foreground(Some(no_focus)),
        [Effect::UpdateIcon(excluded_icon("mstsc.exe"))]
    );
    assert_eq!(
        tray.on_foreground(Some(in_edit)),
        [Effect::UpdateIcon(icon(Vietnamese, InputMethod::Telex))]
    );
}

/// The icon added again (`TaskbarCreated`, the tick) shows the excluded application, and the
/// open panel does not change.
#[test]
fn the_icon_added_again_shows_the_excluded_application_and_the_panel_stays() {
    let mut start = start(Vietnamese);
    start.runtime = excluding_mstsc(Vietnamese);
    start.show_panel = true;
    let mut tray = Tray::new(start, messages());
    send(&mut tray, (START.id(), 0, 0));
    assert_eq!(
        tray.on_foreground(app("mstsc.exe")),
        [Effect::UpdateIcon(excluded_icon("mstsc.exe"))]
    );
    assert_eq!(
        send(&mut tray, taskbar_created()),
        answered(vec![Effect::ShowIcon(excluded_icon("mstsc.exe"))])
    );
    assert_eq!(
        send(&mut tray, tick()).effects[..2],
        [
            Effect::EnsureIcon(excluded_icon("mstsc.exe")),
            Effect::ReadForeground(None),
        ]
    );
}

// --- Menu choices ------------------------------------------------------------------------

#[test]
fn choosing_vietnamese_mode_in_the_menu_switches_the_mode() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        choose(&mut tray, TrayCommand::VietnameseMode),
        [
            Effect::UpdateIcon(icon(English, InputMethod::Telex)),
            Effect::HookMode(false),
            worker(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
        ]
    );
    assert!(!checked(&tray, TrayCommand::VietnameseMode));
}

/// A new input method reaches the hook as a runtime snapshot, with the mode the icon shows.
#[test]
fn choosing_an_input_method_hands_the_hook_a_snapshot_with_the_current_mode() {
    let mut tray = running(Vietnamese);
    send(&mut tray, click());
    let mut expected = runtime(English);
    expected.options.input_method = InputMethod::Vni;
    assert_eq!(
        choose(&mut tray, TrayCommand::Vni),
        [
            Effect::UpdateIcon(icon(English, InputMethod::Vni)),
            hook_runtime(expected.clone()),
            worker(
                2,
                WorkerCommand::Apply(Change::InputMethod(InputMethod::Vni))
            ),
        ]
    );
    assert_eq!(*tray.runtime(), expected);
    assert!(checked(&tray, TrayCommand::Vni));
    assert!(!checked(&tray, TrayCommand::Telex));
    expected.options.input_method = InputMethod::Viqr;
    assert_eq!(
        choose(&mut tray, TrayCommand::Viqr),
        [
            Effect::UpdateIcon(icon(English, InputMethod::Viqr)),
            hook_runtime(expected),
            worker(
                3,
                WorkerCommand::Apply(Change::InputMethod(InputMethod::Viqr))
            ),
        ]
    );
    // The method in effect again: nothing to do.
    assert_eq!(choose(&mut tray, TrayCommand::Viqr), []);
    // D53: Simple Telex.
    let mut simple = expected_after(&tray, InputMethod::SimpleTelex);
    let effects = choose(&mut tray, TrayCommand::SimpleTelex);
    simple.config_revision = tray.runtime().config_revision;
    assert_eq!(
        effects,
        [
            Effect::UpdateIcon(icon(English, InputMethod::SimpleTelex)),
            hook_runtime(simple.clone()),
            worker(
                4,
                WorkerCommand::Apply(Change::InputMethod(InputMethod::SimpleTelex))
            ),
        ]
    );
    assert!(checked(&tray, TrayCommand::SimpleTelex));
    assert!(!checked(&tray, TrayCommand::Viqr));
}

/// The runtime snapshot of `tray` with `method` in effect.
fn expected_after(tray: &Tray, method: InputMethod) -> RuntimeSnapshot {
    let mut expected = tray.runtime().clone();
    expected.options.input_method = method;
    expected
}

/// D54: the charset of the panel reaches the hook and the file like any switch.
#[test]
fn the_charset_reaches_the_hook_and_the_worker() {
    let mut tray = with_panel(start(Vietnamese));
    let mut expected = tray.runtime().clone();
    expected.options.charset = OutputCharset::Tcvn3;
    let effects = panel_request(&mut tray, set(Change::Charset(OutputCharset::Tcvn3)));
    assert_eq!(
        effects,
        [
            hook_runtime(expected.clone()),
            worker(
                1,
                WorkerCommand::Apply(Change::Charset(OutputCharset::Tcvn3))
            ),
            refresh(tray.panel_state()),
        ]
    );
    assert_eq!(*tray.runtime(), expected);
    assert_eq!(tray.panel_state().charset, OutputCharset::Tcvn3);
    assert_eq!(
        with_change(&expected, Change::TypingDefaults)
            .options
            .charset,
        OutputCharset::Unicode
    );
}

#[test]
fn spell_check_modern_tone_and_macros_switch_in_the_hook_and_the_file() {
    let mut tray = running(Vietnamese);
    let mut expected = runtime(Vietnamese);
    expected.options.spell_check = false;
    assert_eq!(
        choose(&mut tray, TrayCommand::SpellCheck),
        [
            hook_runtime(expected.clone()),
            worker(1, WorkerCommand::Apply(Change::SpellCheck(false))),
        ]
    );
    expected.options.modern_tone = true;
    assert_eq!(
        choose(&mut tray, TrayCommand::ModernTone),
        [
            hook_runtime(expected.clone()),
            worker(2, WorkerCommand::Apply(Change::ModernTone(true))),
        ]
    );
    expected.options.macros_enabled = false;
    assert_eq!(
        choose(&mut tray, TrayCommand::Macros),
        [
            hook_runtime(expected.clone()),
            worker(3, WorkerCommand::Apply(Change::Macros(false))),
        ]
    );
    assert!(!checked(&tray, TrayCommand::SpellCheck));
    assert!(checked(&tray, TrayCommand::ModernTone));
    assert!(!checked(&tray, TrayCommand::Macros));
    expected.options.spell_check = true;
    assert_eq!(
        choose(&mut tray, TrayCommand::SpellCheck),
        [
            hook_runtime(expected),
            worker(4, WorkerCommand::Apply(Change::SpellCheck(true))),
        ]
    );
}

/// The tray builds the hook's snapshot of a change exactly as the settings do after it, so
/// the hook never runs on other settings than the file's.
#[test]
fn a_change_builds_the_snapshot_that_the_settings_build() {
    let exe = tempfile::tempdir().unwrap();
    let (mut settings, _) = Settings::open(exe.path(), None, &|_| true, &|_| false);
    let changes = [
        Change::InputMethod(InputMethod::Vni),
        Change::VietnameseMode(false),
        Change::SpellCheck(false),
        Change::ModernTone(true),
        Change::Macros(false),
        Change::FreeMarking(false),
        Change::AutoRestore(false),
        Change::MacrosWhenEnglish(true),
        Change::MacroAutoCase(false),
        Change::ShowPanelOnStart(false),
        Change::InputMethod(InputMethod::Viqr),
        Change::TypingDefaults,
        Change::VietnameseMode(true),
        Change::Startup(StartupMode::Shortcut),
        Change::ModernTone(true),
        Change::InputMethod(InputMethod::Telex),
        Change::Macros(true),
        Change::FreeMarking(true),
        Change::AutoRestore(true),
        Change::MacrosWhenEnglish(false),
        Change::MacroAutoCase(true),
        Change::ShowPanelOnStart(true),
        Change::SpellCheck(true),
        Change::InputMethod(InputMethod::Vni),
        Change::TypingDefaults,
    ];
    for change in changes {
        let mut predicted = with_change(settings.runtime_snapshot(), change);
        settings.apply(change).unwrap();
        predicted.config_revision = settings.runtime_snapshot().config_revision;
        assert_eq!(predicted, *settings.runtime_snapshot(), "{change:?}");
    }
}

/// "Mặc định" with a user-defined method and its key map: the method returns to the default,
/// as in the settings, while the key map and its id stay.
#[test]
fn typing_defaults_build_the_snapshot_that_the_settings_build_for_a_user_method() {
    let exe = tempfile::tempdir().unwrap();
    std::fs::write(
        exe.path().join("raumakey.toml"),
        "version = 1\nrevision = 1\n[input]\nmethod = \"user\"\nuser_keymap = \"mine\"\n\
         modern_tone = true\nspell_check = false\n[macros]\nwhen_disabled = true\n\
         [[keymaps]]\nid = \"mine\"\nversion = 1\nname = \"Mine\"\n\
         bindings = [ { key = \"s\", actions = [\"tone:acute\"] } ]\n",
    )
    .unwrap();
    let (mut settings, _) = Settings::open(exe.path(), None, &|_| true, &|_| false);
    assert_eq!(
        settings.runtime_snapshot().options.input_method,
        InputMethod::UserDefined
    );
    let mut predicted = with_change(settings.runtime_snapshot(), Change::TypingDefaults);
    settings.apply(Change::TypingDefaults).unwrap();
    predicted.config_revision = settings.runtime_snapshot().config_revision;
    assert_eq!(predicted, *settings.runtime_snapshot());
    assert_eq!(predicted.options.input_method, InputMethod::Telex);
    assert_eq!(predicted.options.user_keymap_id.as_deref(), Some("mine"));
    assert_eq!(predicted.keymaps.len(), 1);
}

#[test]
fn startup_on_and_off_go_to_the_worker() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        choose(&mut tray, TrayCommand::Startup),
        [worker(1, WorkerCommand::Startup(true))]
    );
    assert!(checked(&tray, TrayCommand::Startup));
    assert_eq!(tray.startup(), StartupMode::Shortcut);
    assert_eq!(
        choose(&mut tray, TrayCommand::Startup),
        [worker(2, WorkerCommand::Startup(false))]
    );
    assert_eq!(tray.startup(), StartupMode::None);
}

/// From the temporary folder (inside a ZIP) the start with Windows is refused with the
/// request to extract Raumakey first; nothing changes. Turning a saved shortcut off still
/// works.
#[test]
fn startup_from_the_temporary_folder_asks_to_extract_raumakey_first() {
    let mut settings = start(Vietnamese);
    settings.temporary_exe = true;
    let mut tray = running_with(settings);
    assert_eq!(
        choose(&mut tray, TrayCommand::Startup),
        [Effect::Notify(temporary_exe_notice())]
    );
    assert_eq!(tray.startup(), StartupMode::None);
    assert!(!checked(&tray, TrayCommand::Startup));
    let mut settings = start(Vietnamese);
    settings.temporary_exe = true;
    settings.startup = StartupMode::Shortcut;
    let mut tray = running_with(settings);
    assert_eq!(
        choose(&mut tray, TrayCommand::Startup),
        [worker(1, WorkerCommand::Startup(false))]
    );
}

/// The elevated logon task needs administrator rights: the menu only explains.
#[test]
fn turning_off_the_elevated_task_explains_the_elevated_command_line() {
    let mut settings = start(Vietnamese);
    settings.startup = StartupMode::ElevatedTask;
    let mut tray = running_with(settings);
    assert_eq!(
        choose(&mut tray, TrayCommand::Startup),
        [Effect::Notify(elevated_task_notice())]
    );
    assert_eq!(tray.startup(), StartupMode::ElevatedTask);
    assert!(checked(&tray, TrayCommand::Startup));
}

#[test]
fn open_config_goes_to_the_worker_and_about_is_shown() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        choose(&mut tray, TrayCommand::OpenConfig),
        [worker(1, WorkerCommand::OpenConfig)]
    );
    assert_eq!(
        choose(&mut tray, TrayCommand::About),
        [Effect::Notify(about_notice(
            Location::ExeDir,
            Some(&config_path())
        ))]
    );
    let mut nowhere = start(Vietnamese);
    nowhere.location = Location::Unavailable;
    nowhere.config_path = None;
    let mut tray = running_with(nowhere);
    assert_eq!(
        choose(&mut tray, TrayCommand::About),
        [Effect::Notify(about_notice(Location::Unavailable, None))]
    );
}

#[test]
fn a_dismissed_menu_or_an_unknown_command_changes_nothing() {
    let mut tray = running(English);
    assert_eq!(tray.menu_choice(None), []);
    assert_eq!(tray.menu_choice(Some(0)), []);
    assert_eq!(tray.menu_choice(Some(999)), []);
    assert_eq!(tray.mode(), English);
    assert!(tray.ends_loop_when_dropped());
}

/// Exit stops the hook first (typing is plain at once), closes the open notices, the panel,
/// the macro list editor, and the exclusions dialog without waiting for them, saves the
/// hotkey changes left in the inbox, lets the worker finish, and only then removes the icon
/// and ends the loop.
#[test]
fn exit_stops_the_hook_finishes_the_worker_then_removes_the_icon() {
    let mut tray = running(Vietnamese);
    assert_eq!(choose(&mut tray, TrayCommand::Exit), exit_effects());
    assert_eq!(
        exit_effects(),
        [
            Effect::StopTyping,
            Effect::DismissNotices,
            Effect::ClosePanel,
            Effect::CloseMacroList,
            Effect::CloseExclusions,
            Effect::FlushInbox,
            Effect::FinishWorker,
            Effect::RemoveIcon,
            Effect::EndLoop,
        ]
    );
    assert!(!tray.ends_loop_when_dropped(), "Exit ends the loop itself");
    assert!(!tray.is_running());
    // Messages and events still queued behind the exit are answered without effect: a
    // TaskbarCreated must not add the removed icon again.
    for message in [
        (START.id(), 0, 0),
        click(),
        callback(WM_CONTEXTMENU, TRAY_ICON_ID, 1, 2),
        taskbar_created(),
        tick(),
        (WM_CLOSE, 0, 0),
        (WM_ENDSESSION, 1, 0),
    ] {
        assert_eq!(
            send(&mut tray, message),
            answered(Vec::new()),
            "{message:x?}"
        );
    }
    assert_eq!(choose(&mut tray, TrayCommand::Exit), []);
    assert_eq!(choose(&mut tray, TrayCommand::VietnameseMode), []);
    assert_eq!(tray.on_event(Event::Hotkey(false)), []);
    assert_eq!(tray.mode(), Vietnamese);
    assert_eq!(LOOP_EXIT_CODE, 0);
}

/// `WM_CLOSE` (`taskkill`) and the end of the session exit like the menu: the hook stops
/// before the window procedure returns, after which the process may end at any time.
#[test]
fn close_and_the_end_of_the_session_exit_like_the_menu() {
    for message in [
        (WM_CLOSE, 0, 0),
        (WM_ENDSESSION, 1, 0),
        (WM_ENDSESSION, 1, ENDSESSION_CLOSEAPP),
        (WM_ENDSESSION, 1, ENDSESSION_LOGOFF),
    ] {
        let mut tray = running(English);
        assert_eq!(
            send(&mut tray, message),
            answered(exit_effects()),
            "{message:x?}"
        );
        assert!(!tray.ends_loop_when_dropped());
    }
    // Before the start too; the start then adds no icon.
    let mut tray = Tray::new(start(Vietnamese), messages());
    assert_eq!(send(&mut tray, (WM_CLOSE, 0, 0)), answered(exit_effects()));
    assert_eq!(send(&mut tray, (START.id(), 0, 0)), answered(Vec::new()));
}

#[test]
fn a_cancelled_end_of_the_session_changes_nothing() {
    let mut tray = running(Vietnamese);
    assert_eq!(send(&mut tray, (WM_ENDSESSION, 0, 0)), default());
    assert!(tray.ends_loop_when_dropped());
}

// --- The hotkey (hook observer) ------------------------------------------------------------

/// The hook changed its mode itself (Ctrl+Shift): the icon follows, the worker saves it, and
/// the mode goes back to the hook, so that a snapshot handed to it meanwhile (with the mode
/// the icon showed then) cannot undo the hotkey.
#[test]
fn the_hotkey_changes_the_icon_and_is_saved() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        tray.on_event(Event::Hotkey(false)),
        [
            Effect::UpdateIcon(icon(English, InputMethod::Telex)),
            Effect::HookMode(false),
            worker(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
        ]
    );
    assert_eq!(tray.mode(), English);
    assert!(!tray.runtime().vietnamese_mode);
    // Every later snapshot for the hook carries the hotkey's mode.
    let Effect::HookRuntime(snapshot) = &choose(&mut tray, TrayCommand::Vni)[1] else {
        panic!("a runtime snapshot");
    };
    assert!(!snapshot.vietnamese_mode);
    assert!(!snapshot.options.enabled);
}

// --- The poll timer ------------------------------------------------------------------------

/// Every two seconds the timer asks the worker to poll; while a poll is on its way, ticks ask
/// nothing more, so a slow disk never piles them up. Each tick reads the application in front
/// again too, in case a message of the foreground watcher was lost (D47).
#[test]
fn the_timer_asks_the_worker_to_poll_once_at_a_time() {
    assert_eq!(POLL_INTERVAL_MS, 2_000);
    assert_ne!(POLL_TIMER, 0);
    let mut tray = running(Vietnamese);
    let shown = Effect::EnsureIcon(icon(Vietnamese, InputMethod::Telex));
    let read = Effect::ReadForeground(None);
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![
            shown.clone(),
            read.clone(),
            worker(1, WorkerCommand::Poll)
        ])
    );
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![shown.clone(), read.clone()])
    );
    assert_eq!(
        tray.on_event(answer(1, Outcome::Polled(PollReport::default()), None)),
        []
    );
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![
            shown.clone(),
            read.clone(),
            worker(2, WorkerCommand::Poll)
        ])
    );
    // A command that panicked frees the poll as well.
    assert_eq!(tray.on_event(answer(2, Outcome::Failed, None)), []);
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![shown, read, worker(3, WorkerCommand::Poll)])
    );
    // Another timer of the window is not the poll timer.
    assert_eq!(send(&mut tray, (WM_TIMER, POLL_TIMER + 1, 0)), default());
}

/// Every tick makes sure the icon is shown: an icon that could not be added (no taskbar yet)
/// or whose change failed is added again, in the state the tray shows now.
#[test]
fn every_tick_makes_sure_the_icon_is_shown() {
    let mut tray = running(Vietnamese);
    send(&mut tray, click());
    let effects = send(&mut tray, tick()).effects;
    assert_eq!(
        effects[0],
        Effect::EnsureIcon(icon(English, InputMethod::Telex))
    );
}

/// The worker and the hook observer post `WAKE` after leaving something in the inbox; the
/// glue drains the inbox, so the message itself only needs an answer.
#[test]
fn wake_is_answered_without_effect() {
    let mut tray = running(Vietnamese);
    assert_eq!(send(&mut tray, (WAKE.id(), 0, 0)), answered(Vec::new()));
}

// --- Answers of the worker -----------------------------------------------------------------

/// A hand edit, reloaded by the poll: the tray takes the new settings over, the icon and
/// menu follow, and the hook gets the new snapshot, mode included.
#[test]
fn a_reload_refreshes_the_tray_and_hands_the_new_snapshot_to_the_hook() {
    let mut tray = running(Vietnamese);
    send(&mut tray, tick());
    let mut edited = runtime(English);
    edited.config_revision = 2;
    edited.options.input_method = InputMethod::Viqr;
    let effects = tray.on_event(answer(
        1,
        polled(clean()),
        Some(state(edited.clone(), StartupMode::Shortcut)),
    ));
    assert_eq!(
        effects,
        [
            Effect::UpdateIcon(icon(English, InputMethod::Viqr)),
            hook_runtime(edited.clone()),
        ]
    );
    assert_eq!(*tray.runtime(), edited);
    assert_eq!(tray.mode(), English);
    assert!(checked(&tray, TrayCommand::Viqr));
    assert!(checked(&tray, TrayCommand::Startup));
}

/// The answer to a commit carries the new revision only: nothing for the hook or the icon.
#[test]
fn settings_that_differ_only_in_revision_change_nothing_for_the_hook() {
    let mut tray = running(Vietnamese);
    let effects = choose(&mut tray, TrayCommand::Vni);
    assert_eq!(effects.len(), 3);
    let mut saved = tray.runtime().clone();
    saved.config_revision = 2;
    let change = Change::InputMethod(InputMethod::Vni);
    assert_eq!(
        tray.on_event(answer(
            1,
            applied(change, Ok(Saved::Written)),
            Some(state(saved.clone(), StartupMode::None))
        )),
        []
    );
    assert_eq!(*tray.runtime(), saved);
}

/// An answer older than a change still on its way would undo it: its settings are not taken
/// over until the worker has answered everything sent.
#[test]
fn an_answer_older_than_the_last_command_does_not_undo_a_change() {
    let mut tray = running(Vietnamese);
    send(&mut tray, tick()); // seq 1: a poll
    choose(&mut tray, TrayCommand::Vni); // seq 2: VNI, still on its way
    let mut stale = runtime(Vietnamese);
    stale.options.input_method = InputMethod::Viqr;
    assert_eq!(
        tray.on_event(answer(
            1,
            polled(clean()),
            Some(state(stale, StartupMode::None))
        )),
        []
    );
    assert_eq!(tray.icon().method, InputMethod::Vni);
    // The answer to the last command brings what the worker has now (the edit and VNI).
    let mut current = runtime(Vietnamese);
    current.config_revision = 3;
    current.options.input_method = InputMethod::Vni;
    current.options.modern_tone = true;
    let change = Change::InputMethod(InputMethod::Vni);
    assert_eq!(
        tray.on_event(answer(
            2,
            applied(change, Ok(Saved::Written)),
            Some(state(current.clone(), StartupMode::None))
        )),
        [hook_runtime(current)]
    );
}

/// A reload answered just before the answer to "Mở file cấu hình" still reaches the hook:
/// every answer carries the settings in effect, and the last one is taken over.
#[test]
fn a_reload_overtaken_by_another_command_arrives_with_that_command_s_answer() {
    let mut tray = running(Vietnamese);
    send(&mut tray, tick()); // seq 1: a poll that finds a hand edit
    choose(&mut tray, TrayCommand::OpenConfig); // seq 2
    let mut edited = runtime(Vietnamese);
    edited.config_revision = 2;
    edited.options.input_method = InputMethod::Vni;
    let reloaded = Some(state(edited.clone(), StartupMode::None));
    assert_eq!(
        tray.on_event(answer(1, polled(clean()), reloaded.clone())),
        []
    );
    assert_eq!(
        tray.on_event(answer(2, Outcome::Opened(Ok(())), reloaded)),
        [
            Effect::UpdateIcon(icon(Vietnamese, InputMethod::Vni)),
            hook_runtime(edited),
        ]
    );
}

/// A change that could not be saved stays in effect; the user is told once per error.
#[test]
fn a_save_error_is_shown_once_and_the_change_stays() {
    let mut tray = running(Vietnamese);
    choose(&mut tray, TrayCommand::SpellCheck);
    let in_effect = tray.runtime().clone();
    let change = Change::SpellCheck(false);
    let denied = Err(ConfigError::PermissionDenied);
    assert_eq!(
        tray.on_event(answer(
            1,
            applied(change, denied),
            Some(state(in_effect.clone(), StartupMode::None))
        )),
        [Effect::Notify(save_error_notice(
            Some(&config_path()),
            ConfigError::PermissionDenied
        ))]
    );
    assert!(!tray.runtime().options.spell_check);
    choose(&mut tray, TrayCommand::ModernTone);
    let change = Change::ModernTone(true);
    assert_eq!(
        tray.on_event(answer(2, applied(change, denied), None)),
        [],
        "the same error again"
    );
    // Once a save works again, a new failure is shown again.
    choose(&mut tray, TrayCommand::Macros);
    assert_eq!(
        tray.on_event(answer(
            3,
            applied(Change::Macros(false), Ok(Saved::Written)),
            None
        )),
        []
    );
    choose(&mut tray, TrayCommand::Macros);
    assert_eq!(
        tray.on_event(answer(4, applied(Change::Macros(true), denied), None)),
        [Effect::Notify(save_error_notice(
            Some(&config_path()),
            ConfigError::PermissionDenied
        ))]
    );
}

/// A damaged file refuses every commit: the notice asks about recovery.
#[test]
fn a_damaged_file_that_refuses_a_save_offers_recovery() {
    let mut tray = running(Vietnamese);
    choose(&mut tray, TrayCommand::SpellCheck);
    let effects = tray.on_event(answer(
        1,
        applied(Change::SpellCheck(false), Err(ConfigError::Corrupt)),
        None,
    ));
    let [Effect::Notify(notice)] = effects.as_slice() else {
        panic!("{effects:?}");
    };
    assert_eq!(notice.question, Some(Question::Recover));
    assert_eq!(
        tray.answered_yes(Question::Recover),
        [worker(2, WorkerCommand::Recover)]
    );
    let recovered = Ok(Recovery::Replaced {
        backup: Some(PathBuf::from(r"D:\Tools\Raumakey\raumakey.toml.bak-1")),
        saved: Ok(()),
    });
    let effects = tray.on_event(answer(
        2,
        Outcome::Recovered(recovered.clone()),
        Some(state(tray.runtime().clone(), StartupMode::None)),
    ));
    assert_eq!(
        effects,
        [Effect::Notify(
            recovery_notice(Some(&config_path()), &recovered).unwrap()
        )]
    );
}

#[test]
fn a_shortcut_that_could_not_be_made_is_shown_and_the_check_follows_the_worker() {
    let mut tray = running(Vietnamese);
    choose(&mut tray, TrayCommand::Startup);
    assert!(checked(&tray, TrayCommand::Startup));
    let error = StartupError::Shortcut(WinError::new(E_ACCESSDENIED, "IPersistFile::Save"));
    let effects = tray.on_event(answer(
        1,
        Outcome::Startup {
            on: true,
            result: Err(error),
        },
        Some(state(tray.runtime().clone(), StartupMode::None)),
    ));
    assert_eq!(
        effects,
        [Effect::Notify(startup_error_notice(
            Some(&config_path()),
            true,
            error
        ))]
    );
    assert!(!checked(&tray, TrayCommand::Startup));
}

/// An answer that wrote nothing is no save that worked: the same failure stays told.
#[test]
fn an_answer_that_wrote_nothing_does_not_count_as_a_save() {
    let mut tray = running(Vietnamese);
    let denied = Err(ConfigError::PermissionDenied);
    choose(&mut tray, TrayCommand::SpellCheck);
    let told = tray.on_event(answer(1, applied(Change::SpellCheck(false), denied), None));
    assert_eq!(told.len(), 1);
    choose(&mut tray, TrayCommand::Telex);
    choose(&mut tray, TrayCommand::Vni);
    assert_eq!(
        tray.on_event(answer(
            2,
            applied(Change::InputMethod(InputMethod::Vni), Ok(Saved::Unchanged)),
            None
        )),
        []
    );
    choose(&mut tray, TrayCommand::Macros);
    assert_eq!(
        tray.on_event(answer(3, applied(Change::Macros(false), denied), None)),
        [],
        "still the same failure"
    );
}

/// The poll retries what could not be saved: a retry that worked counts as a save, one that
/// fails again the same way is not told again.
#[test]
fn a_poll_retry_counts_as_a_save_when_it_worked() {
    let mut tray = running(Vietnamese);
    let denied = Err(ConfigError::PermissionDenied);
    choose(&mut tray, TrayCommand::SpellCheck);
    assert_eq!(
        tray.on_event(answer(1, applied(Change::SpellCheck(false), denied), None))
            .len(),
        1
    );
    send(&mut tray, tick());
    let retried = |result| {
        Outcome::Polled(PollReport {
            retry: Some(result),
            ..PollReport::default()
        })
    };
    assert_eq!(
        tray.on_event(answer(2, retried(Err(ConfigError::PermissionDenied)), None)),
        []
    );
    send(&mut tray, tick());
    assert_eq!(tray.on_event(answer(3, retried(Ok(())), None)), []);
    // Told again after the retry worked.
    choose(&mut tray, TrayCommand::Macros);
    assert_eq!(
        tray.on_event(answer(4, applied(Change::Macros(false), denied), None))
            .len(),
        1
    );
}

/// A startup choice whose shortcut was set but which could not be saved is told once, with
/// the text of the shortcut.
#[test]
fn a_startup_choice_that_was_not_saved_is_told_once_with_its_own_text() {
    let mut tray = running(Vietnamese);
    choose(&mut tray, TrayCommand::Startup);
    let error = StartupError::Save(ConfigError::IoFailure);
    let outcome = Outcome::Startup {
        on: true,
        result: Err(error),
    };
    assert_eq!(
        tray.on_event(answer(1, outcome.clone(), None)),
        [Effect::Notify(startup_error_notice(
            Some(&config_path()),
            true,
            error
        ))]
    );
    choose(&mut tray, TrayCommand::Startup);
    choose(&mut tray, TrayCommand::Startup);
    assert_eq!(tray.on_event(answer(3, outcome, None)), []);
}

/// A hand edit of `[system] startup` that the worker could not follow is told: the temporary
/// folder, or a shortcut that could not be made or removed.
#[test]
fn a_hand_edited_startup_that_could_not_be_followed_is_told() {
    let mut tray = running(Vietnamese);
    let report = |shortcut| {
        Outcome::Polled(PollReport {
            status: Some(clean()),
            shortcut: Some(shortcut),
            retry: None,
        })
    };
    send(&mut tray, tick());
    assert_eq!(
        tray.on_event(answer(1, report(ShortcutSync::RefusedTemporary), None)),
        [Effect::Notify(temporary_exe_notice())]
    );
    send(&mut tray, tick());
    let error = WinError::new(E_ACCESSDENIED, "IPersistFile::Save");
    assert_eq!(
        tray.on_event(answer(
            2,
            report(ShortcutSync::Failed { on: false, error }),
            None
        )),
        [Effect::Notify(startup_error_notice(
            Some(&config_path()),
            false,
            StartupError::Shortcut(error)
        ))]
    );
    for quiet in [
        ShortcutSync::Created,
        ShortcutSync::Removed,
        ShortcutSync::Unchanged,
    ] {
        assert_eq!(
            tray.on_event(answer(99, report(quiet), None)),
            [],
            "{quiet:?}"
        );
    }
}

#[test]
fn opening_the_configuration_reports_a_missing_file() {
    let mut tray = running(Vietnamese);
    choose(&mut tray, TrayCommand::OpenConfig);
    assert_eq!(
        tray.on_event(answer(1, Outcome::Opened(Err(OpenError::NoFile)), None)),
        [Effect::Notify(open_error_notice(
            Some(&config_path()),
            OpenError::NoFile
        ))]
    );
    choose(&mut tray, TrayCommand::OpenConfig);
    assert_eq!(tray.on_event(answer(2, Outcome::Opened(Ok(())), None)), []);
}

/// A reload that finds a new problem tells the user once; problems already told at the start
/// are not told again.
#[test]
fn a_reload_tells_new_problems_once() {
    let damaged = Status {
        config: DocumentStatus::Unreadable(ConfigError::Corrupt),
        ..clean()
    };
    let mut tray = running(Vietnamese);
    send(&mut tray, tick());
    let effects = tray.on_event(answer(
        1,
        polled(damaged.clone()),
        Some(state(tray.runtime().clone(), StartupMode::None)),
    ));
    assert_eq!(
        effects,
        [Effect::Notify(
            status_notice(Some(&config_path()), &clean(), &damaged).unwrap()
        )]
    );
    send(&mut tray, tick());
    // Fixed, then broken the same way again: told again.
    assert_eq!(
        tray.on_event(answer(
            2,
            polled(clean()),
            Some(state(tray.runtime().clone(), StartupMode::None))
        )),
        []
    );
    send(&mut tray, tick());
    assert_eq!(
        tray.on_event(answer(
            3,
            polled(damaged.clone()),
            Some(state(tray.runtime().clone(), StartupMode::None))
        ))
        .len(),
        1
    );
    // Known at the start already: nothing new.
    let mut known = start(Vietnamese);
    known.status = damaged.clone();
    let mut tray = running_with(known);
    send(&mut tray, tick());
    assert_eq!(
        tray.on_event(answer(
            1,
            polled(damaged),
            Some(state(tray.runtime().clone(), StartupMode::None))
        )),
        []
    );
}

// --- The settings panel (D43, D45) ---------------------------------------------------------

fn open_panel(serial: u64, state: PanelState) -> Effect {
    Effect::OpenPanel { serial, state }
}

fn refresh(state: PanelState) -> Effect {
    Effect::RefreshPanel(state)
}

fn is_refresh(effect: &Effect) -> bool {
    matches!(effect, Effect::RefreshPanel(_))
}

fn panel_request(tray: &mut Tray, request: PanelRequest) -> Vec<Effect> {
    tray.on_event(Event::Panel(request))
}

fn set(change: Change) -> PanelRequest {
    PanelRequest::Change(change)
}

/// A running tray whose panel was opened from the menu (panel 1).
fn with_panel(start: TrayStart) -> Tray {
    let mut tray = running_with(start);
    let state = tray.panel_state();
    assert_eq!(
        choose(&mut tray, TrayCommand::Panel),
        [open_panel(1, state)]
    );
    tray
}

/// The panel shows the settings the tray holds: the runtime snapshot, the start with Windows,
/// the panel at start, and where the file lives.
#[test]
fn the_panel_state_is_built_from_the_settings_of_the_tray() {
    let mut settings = start(English);
    settings.startup = StartupMode::Shortcut;
    settings.show_panel = true;
    settings.runtime.options.free_marking = false;
    let tray = Tray::new(settings.clone(), messages());
    assert_eq!(
        tray.panel_state(),
        PanelState::new(
            &settings.runtime,
            StartupMode::Shortcut,
            true,
            Location::ExeDir,
            Some(&config_path())
        )
    );
}

/// `show_panel_on_start` (the default, so the first run always shows it): the start adds the
/// icon, opens the panel, then shows the start notices. Without it no panel opens.
#[test]
fn the_start_opens_the_panel_when_it_is_to_show_at_start() {
    let notice = Notice {
        kind: NoticeKind::Warning,
        text: "cảnh báo".to_owned(),
        question: None,
    };
    let mut settings = start(Vietnamese);
    settings.show_panel = true;
    settings.notices = vec![notice.clone()];
    let mut tray = Tray::new(settings, messages());
    let state = tray.panel_state();
    assert!(state.show_on_start);
    assert_eq!(
        send(&mut tray, (START.id(), 0, 0)),
        answered(vec![
            Effect::ShowIcon(icon(Vietnamese, InputMethod::Telex)),
            Effect::ReadForeground(None),
            open_panel(1, state),
            Effect::Notify(notice),
        ])
    );
    // Without it (`running` checks the start's effects): no panel, so nothing to refresh.
    let mut tray = running(Vietnamese);
    assert!(!send(&mut tray, click()).effects.iter().any(is_refresh));
}

/// "Bảng cấu hình…" and the request of a second start (D45) open the panel; while it is open,
/// they bring it to the front with the state in effect. Each opening is numbered.
#[test]
fn the_menu_and_a_second_start_open_the_panel_or_bring_it_to_the_front() {
    let mut tray = running(Vietnamese);
    let state = tray.panel_state();
    assert_eq!(
        choose(&mut tray, TrayCommand::Panel),
        [open_panel(1, state.clone())]
    );
    assert_eq!(
        send(&mut tray, (SHOW_PANEL, 0, 0)),
        answered(vec![open_panel(2, state.clone())])
    );
    choose(&mut tray, TrayCommand::SpellCheck);
    let changed = tray.panel_state();
    assert!(!changed.spell_check);
    assert_eq!(
        choose(&mut tray, TrayCommand::Panel),
        [open_panel(3, changed)]
    );
}

/// The request of a second start acts only while the tray runs: before its start (the icon
/// is not there yet) and after an exit it is answered without effect.
#[test]
fn the_show_panel_request_acts_only_while_the_tray_runs() {
    let mut tray = Tray::new(start(Vietnamese), messages());
    assert_eq!(send(&mut tray, (SHOW_PANEL, 0, 0)), answered(Vec::new()));
    assert_eq!(
        send(&mut tray, (START.id(), 0, 0)),
        answered(started(icon(Vietnamese, InputMethod::Telex)))
    );
    choose(&mut tray, TrayCommand::Exit);
    assert_eq!(send(&mut tray, (SHOW_PANEL, 0, 0)), answered(Vec::new()));
    assert_eq!(choose(&mut tray, TrayCommand::Panel), []);
}

/// The registered message is named for the user: `Raumakey.<sidhash>.show-panel`.
#[test]
fn the_show_panel_message_is_named_for_the_user() {
    assert_eq!(SHOW_PANEL_COMPONENT, "show-panel");
    assert!(is_valid_component(SHOW_PANEL_COMPONENT));
    let hash = SidHash::parse("0123456789abcdef").unwrap();
    assert_eq!(
        message_name(&hash, SHOW_PANEL_COMPONENT).unwrap(),
        "Raumakey.0123456789abcdef.show-panel"
    );
}

/// A switch of the panel goes the way of the menu (the hook gets the snapshot, the worker
/// saves the change), and the panel gets the whole state back.
#[test]
fn a_switch_of_the_panel_goes_the_way_of_the_menu_and_the_panel_is_refreshed() {
    type Expect = fn(&mut RuntimeSnapshot);
    let cases: [(Change, Expect); 8] = [
        (Change::SpellCheck(false), |r| r.options.spell_check = false),
        (Change::ModernTone(true), |r| r.options.modern_tone = true),
        (Change::FreeMarking(false), |r| {
            r.options.free_marking = false
        }),
        (Change::AutoRestore(false), |r| {
            r.options.auto_restore_invalid = false;
        }),
        (Change::Macros(false), |r| r.options.macros_enabled = false),
        (Change::MacrosWhenEnglish(true), |r| {
            r.options.macros_when_disabled = true;
        }),
        (Change::MacroAutoCase(false), |r| {
            r.options.auto_case_macro = false;
        }),
        (Change::Macros(true), |r| r.options.macros_enabled = true),
    ];
    let mut tray = with_panel(start(Vietnamese));
    let mut expected = runtime(Vietnamese);
    for (seq, (change, expect)) in (1..).zip(cases) {
        expect(&mut expected);
        let effects = panel_request(&mut tray, set(change));
        assert_eq!(
            effects,
            [
                hook_runtime(expected.clone()),
                worker(seq, WorkerCommand::Apply(change)),
                refresh(tray.panel_state()),
            ],
            "{change:?}"
        );
        assert_eq!(*tray.runtime(), expected, "{change:?}");
    }
    let state = tray.panel_state();
    assert!(!state.spell_check && state.modern_tone && !state.free_marking);
    assert!(!state.auto_restore && state.macros && state.macros_when_english);
    assert!(!state.macro_auto_case);
}

/// "Gõ tiếng Việt" and the method list of the panel act as the menu does.
#[test]
fn the_mode_and_the_method_of_the_panel_act_as_the_menu_does() {
    let mut tray = with_panel(start(Vietnamese));
    let effects = panel_request(&mut tray, set(Change::VietnameseMode(false)));
    assert_eq!(
        effects,
        [
            Effect::UpdateIcon(icon(English, InputMethod::Telex)),
            Effect::HookMode(false),
            worker(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
            refresh(tray.panel_state()),
        ]
    );
    assert!(!tray.panel_state().vietnamese);
    let mut vni = runtime(English);
    vni.options.input_method = InputMethod::Vni;
    let effects = panel_request(&mut tray, set(Change::InputMethod(InputMethod::Vni)));
    assert_eq!(
        effects,
        [
            Effect::UpdateIcon(icon(English, InputMethod::Vni)),
            hook_runtime(vni),
            worker(
                2,
                WorkerCommand::Apply(Change::InputMethod(InputMethod::Vni))
            ),
            refresh(tray.panel_state()),
        ]
    );
    assert_eq!(tray.panel_state().method, InputMethod::Vni);
    // The method in effect again: nothing to do, but the panel is refreshed all the same.
    assert_eq!(
        panel_request(&mut tray, set(Change::InputMethod(InputMethod::Vni))),
        [refresh(tray.panel_state())]
    );
}

/// "Mở bảng này khi Raumakey chạy" is saved only: the hook does not see it.
#[test]
fn the_panel_at_start_is_saved_without_the_hook() {
    let mut settings = start(Vietnamese);
    settings.show_panel = true;
    let mut tray = running_with_panel_at_start(settings);
    let effects = panel_request(&mut tray, set(Change::ShowPanelOnStart(false)));
    let state = tray.panel_state();
    assert!(!state.show_on_start);
    assert_eq!(
        effects,
        [
            worker(1, WorkerCommand::Apply(Change::ShowPanelOnStart(false))),
            refresh(state),
        ]
    );
}

/// A running tray that opened its panel at the start (panel 1).
fn running_with_panel_at_start(start: TrayStart) -> Tray {
    assert!(start.show_panel);
    let mut tray = Tray::new(start, messages());
    let effects = send(&mut tray, (START.id(), 0, 0)).effects;
    assert!(
        matches!(effects[2], Effect::OpenPanel { serial: 1, .. }),
        "{effects:?}"
    );
    tray
}

/// "Mặc định": the hook gets the snapshot with the typing and macro switches at their
/// defaults (the mode stays), the icon shows the default method, and the worker saves it.
#[test]
fn typing_defaults_reach_the_icon_the_hook_and_the_worker() {
    let mut settings = start(English);
    settings.runtime.options.input_method = InputMethod::Viqr;
    settings.runtime.options.spell_check = true;
    settings.runtime.options.modern_tone = true;
    settings.runtime.options.macros_when_disabled = true;
    settings.runtime.options.charset = OutputCharset::VniWindows;
    let mut tray = with_panel(settings);
    let effects = panel_request(&mut tray, set(Change::TypingDefaults));
    // The product defaults are version 2 (D51): no spell checking.
    let mut expected = runtime(English);
    expected.options.spell_check = false;
    assert_eq!(
        effects,
        [
            Effect::UpdateIcon(icon(English, InputMethod::Telex)),
            hook_runtime(expected.clone()),
            worker(1, WorkerCommand::Apply(Change::TypingDefaults)),
            refresh(tray.panel_state()),
        ]
    );
    assert_eq!(*tray.runtime(), expected);
}

/// The start with Windows goes through the shortcut only (`PanelRequest::Startup`): a
/// `Change::Startup` from the panel changes nothing, and the box is put back.
#[test]
fn a_startup_change_without_the_shortcut_changes_nothing() {
    let mut tray = with_panel(start(Vietnamese));
    assert_eq!(
        panel_request(&mut tray, set(Change::Startup(StartupMode::Shortcut))),
        [refresh(tray.panel_state())]
    );
    assert_eq!(tray.startup(), StartupMode::None);
}

/// "Khởi động cùng Windows" of the panel is absolute: on makes the shortcut, off removes it,
/// and the value in effect does nothing (the box is put back to it).
#[test]
fn the_start_with_windows_of_the_panel_is_absolute() {
    let mut tray = with_panel(start(Vietnamese));
    let effects = panel_request(&mut tray, PanelRequest::Startup(true));
    assert_eq!(tray.panel_state().startup, StartupMode::Shortcut);
    assert_eq!(
        effects,
        [
            worker(1, WorkerCommand::Startup(true)),
            refresh(tray.panel_state())
        ]
    );
    assert_eq!(
        panel_request(&mut tray, PanelRequest::Startup(true)),
        [refresh(tray.panel_state())]
    );
    let effects = panel_request(&mut tray, PanelRequest::Startup(false));
    assert_eq!(tray.startup(), StartupMode::None);
    assert_eq!(
        effects,
        [
            worker(2, WorkerCommand::Startup(false)),
            refresh(tray.panel_state())
        ]
    );
    assert_eq!(
        panel_request(&mut tray, PanelRequest::Startup(false)),
        [refresh(tray.panel_state())]
    );
}

/// A start with Windows the tray refuses puts the box back: from the temporary folder with
/// the request to extract Raumakey first; the elevated logon task only explains.
#[test]
fn a_refused_start_with_windows_puts_the_box_back() {
    let mut settings = start(Vietnamese);
    settings.temporary_exe = true;
    let mut tray = with_panel(settings);
    let effects = panel_request(&mut tray, PanelRequest::Startup(true));
    let state = tray.panel_state();
    assert_eq!(state.startup, StartupMode::None);
    assert_eq!(
        effects,
        [Effect::Notify(temporary_exe_notice()), refresh(state)]
    );
    let mut settings = start(Vietnamese);
    settings.startup = StartupMode::ElevatedTask;
    let mut tray = with_panel(settings);
    let effects = panel_request(&mut tray, PanelRequest::Startup(false));
    let state = tray.panel_state();
    assert_eq!(state.startup, StartupMode::ElevatedTask);
    assert_eq!(
        effects,
        [
            Effect::Notify(elevated_task_notice()),
            refresh(state.clone())
        ]
    );
    assert_eq!(
        panel_request(&mut tray, PanelRequest::Startup(true)),
        [refresh(state)],
        "the task starts Raumakey already"
    );
}

/// "Mở file cấu hình" goes to the worker; "Danh sách gõ tắt…" opens the macro list of the
/// data folder, when there is one.
#[test]
fn the_buttons_open_the_configuration_and_the_macro_list() {
    let mut tray = with_panel(start(Vietnamese));
    assert_eq!(
        panel_request(&mut tray, PanelRequest::OpenConfig),
        [
            worker(1, WorkerCommand::OpenConfig),
            refresh(tray.panel_state())
        ]
    );
    assert_eq!(
        panel_request(&mut tray, PanelRequest::MacroList),
        [
            Effect::OpenMacroList(PathBuf::from(r"D:\Tools\Raumakey")),
            refresh(tray.panel_state())
        ]
    );
    let mut nowhere = start(Vietnamese);
    nowhere.location = Location::Unavailable;
    nowhere.config_path = None;
    let mut tray = with_panel(nowhere);
    assert_eq!(
        panel_request(&mut tray, PanelRequest::MacroList),
        [refresh(tray.panel_state())]
    );
}

/// While the panel is open, every change made another way reaches it: the hotkey, a click on
/// the icon, the menu, a hand edit (the worker's answer). Nothing is sent when nothing
/// changed.
#[test]
fn changes_made_another_way_refresh_the_open_panel() {
    let mut tray = with_panel(start(Vietnamese));
    let effects = tray.on_event(Event::Hotkey(false));
    assert_eq!(effects.last(), Some(&refresh(tray.panel_state())));
    assert!(!tray.panel_state().vietnamese);
    let effects = send(&mut tray, click()).effects;
    assert_eq!(effects.last(), Some(&refresh(tray.panel_state())));
    assert!(tray.panel_state().vietnamese);
    let effects = choose(&mut tray, TrayCommand::SpellCheck);
    assert_eq!(effects.last(), Some(&refresh(tray.panel_state())));
    assert!(!tray.panel_state().spell_check);
    // A tick only polls; WAKE and a dismissed menu change nothing.
    assert!(!send(&mut tray, tick()).effects.iter().any(is_refresh));
    assert!(
        !send(&mut tray, (WAKE.id(), 0, 0))
            .effects
            .iter()
            .any(is_refresh)
    );
    assert_eq!(tray.menu_choice(None), []);
    // The poll found a hand edit: VNI, the start with Windows, no panel at start.
    let mut edited = runtime(Vietnamese);
    edited.config_revision = 2;
    edited.options.input_method = InputMethod::Vni;
    edited.options.spell_check = false;
    let answer = Answer {
        seq: 4,
        outcome: polled(clean()),
        state: Some(Box::new(SettingsState {
            runtime: edited.clone(),
            startup: StartupMode::Shortcut,
            show_panel_on_start: false,
            exclusions: default_apps(),
        })),
    };
    let effects = tray.on_event(Event::Worker(answer));
    let state = tray.panel_state();
    assert_eq!(state.method, InputMethod::Vni);
    assert_eq!(state.startup, StartupMode::Shortcut);
    assert!(!state.show_on_start);
    assert_eq!(
        effects,
        [
            Effect::UpdateIcon(icon(Vietnamese, InputMethod::Vni)),
            hook_runtime(edited.clone()),
            refresh(state),
        ]
    );
    // Only `show_panel_on_start` edited by hand: the panel alone follows.
    send(&mut tray, tick());
    let answer = Answer {
        seq: 5,
        outcome: polled(clean()),
        state: Some(Box::new(SettingsState {
            runtime: edited,
            startup: StartupMode::Shortcut,
            show_panel_on_start: true,
            exclusions: default_apps(),
        })),
    };
    assert_eq!(
        tray.on_event(Event::Worker(answer)),
        [refresh(tray.panel_state())]
    );
    assert!(tray.panel_state().show_on_start);
}

/// No panel, no refresh: neither before one was opened nor after it closed; a request from a
/// panel the tray does not know as open still acts.
#[test]
fn nothing_is_refreshed_while_no_panel_is_open() {
    let mut tray = running(Vietnamese);
    assert!(!send(&mut tray, click()).effects.iter().any(is_refresh));
    let mut expected = runtime(English);
    expected.options.spell_check = false;
    assert_eq!(
        panel_request(&mut tray, set(Change::SpellCheck(false))),
        [
            hook_runtime(expected),
            worker(2, WorkerCommand::Apply(Change::SpellCheck(false)))
        ]
    );
    let state = tray.panel_state();
    assert_eq!(
        choose(&mut tray, TrayCommand::Panel),
        [open_panel(1, state)]
    );
    assert_eq!(tray.on_event(Event::PanelClosed(1)), []);
    assert!(!send(&mut tray, click()).effects.iter().any(is_refresh));
    assert!(!tray.on_event(Event::Hotkey(false)).iter().any(is_refresh));
    assert!(
        !panel_request(&mut tray, PanelRequest::OpenConfig)
            .iter()
            .any(is_refresh)
    );
}

/// The close of a panel that was replaced meanwhile (it closed while a second start or the
/// menu opened the next one) does not stop the refreshes of the new one.
#[test]
fn the_close_of_a_replaced_panel_does_not_stop_the_refreshes() {
    let mut tray = with_panel(start(Vietnamese));
    let state = tray.panel_state();
    assert_eq!(
        send(&mut tray, (SHOW_PANEL, 0, 0)),
        answered(vec![open_panel(2, state)])
    );
    assert_eq!(tray.on_event(Event::PanelClosed(1)), []);
    let effects = send(&mut tray, click()).effects;
    assert_eq!(effects.last(), Some(&refresh(tray.panel_state())));
    assert_eq!(tray.on_event(Event::PanelClosed(2)), []);
    assert!(!send(&mut tray, click()).effects.iter().any(is_refresh));
}

/// Exit closes the open panel without waiting (after the hook and the notices, before the
/// worker finishes); nothing acts on it afterwards.
#[test]
fn exit_closes_the_panel_and_nothing_follows() {
    let mut tray = with_panel(start(Vietnamese));
    assert_eq!(choose(&mut tray, TrayCommand::Exit), exit_effects());
    assert_eq!(panel_request(&mut tray, PanelRequest::OpenConfig), []);
    assert_eq!(tray.on_event(Event::PanelClosed(1)), []);
    assert_eq!(send(&mut tray, (SHOW_PANEL, 0, 0)), answered(Vec::new()));
    assert_eq!(
        tray.flush_inbox(vec![Event::Hotkey(false), Event::PanelClosed(1)]),
        [worker(
            1,
            WorkerCommand::Apply(Change::VietnameseMode(false))
        )]
    );
}

/// D50: "Đóng" of the panel exits Raumakey exactly as "Thoát" of the menu does (the panel,
/// the editors, and the icon go, what is unsaved is saved); a second one does nothing.
#[test]
fn d50_close_on_the_panel_exits_like_the_menu() {
    let mut tray = with_panel(start(Vietnamese));
    assert_eq!(panel_request(&mut tray, PanelRequest::Exit), exit_effects());
    assert_eq!(panel_request(&mut tray, PanelRequest::Exit), []);
    assert_eq!(tray.on_event(Event::PanelClosed(1)), []);
    assert_eq!(choose(&mut tray, TrayCommand::Exit), []);
}

/// Panel events coalesce like any event but the hotkey: they keep their order.
#[test]
fn panel_events_keep_their_order_among_the_events() {
    let events = vec![
        Event::Panel(PanelRequest::OpenConfig),
        Event::Hotkey(false),
        Event::PanelClosed(3),
        Event::Hotkey(true),
        Event::Panel(set(Change::Macros(false))),
    ];
    assert_eq!(
        coalesce(events),
        [
            Event::Panel(PanelRequest::OpenConfig),
            Event::PanelClosed(3),
            Event::Hotkey(true),
            Event::Panel(set(Change::Macros(false))),
        ]
    );
}

// --- The macro list editor (D44) --------------------------------------------------------------

fn no_reload() -> Outcome {
    Outcome::Polled(PollReport::default())
}

/// The editor wrote `raumakey-macros.toml`: the worker polls at once, so the typing hook gets
/// the new macros now rather than at the next tick; the tick that follows does not poll again
/// while that poll is on its way.
#[test]
fn a_save_of_the_macro_list_makes_the_worker_poll_at_once() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        tray.on_event(Event::MacrosSaved),
        [worker(1, WorkerCommand::Poll)]
    );
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![
            Effect::EnsureIcon(icon(Vietnamese, InputMethod::Telex)),
            Effect::ReadForeground(None),
        ])
    );
    assert_eq!(tray.on_event(answer(1, no_reload(), None)), []);
    assert_eq!(
        send(&mut tray, tick()).effects[2],
        worker(2, WorkerCommand::Poll)
    );
}

/// A save while a poll is on its way (it may have read the file before the save is not
/// lost): the worker polls once more as soon as that poll has answered, also when it
/// panicked; saves meanwhile ask for that one poll only. The answer's settings are taken over
/// before the next poll is sent.
#[test]
fn a_save_during_a_poll_polls_again_once_it_answered() {
    let mut tray = running(Vietnamese);
    send(&mut tray, tick()); // seq 1: a poll
    assert_eq!(tray.on_event(Event::MacrosSaved), []);
    assert_eq!(tray.on_event(Event::MacrosSaved), []);
    let mut edited = runtime(Vietnamese);
    edited.config_revision = 2;
    edited.options.spell_check = false;
    assert_eq!(
        tray.on_event(answer(
            1,
            no_reload(),
            Some(state(edited.clone(), StartupMode::None))
        )),
        [hook_runtime(edited), worker(2, WorkerCommand::Poll)]
    );
    assert_eq!(tray.on_event(answer(2, no_reload(), None)), []);
    // A poll that panicked frees the way as well.
    send(&mut tray, tick()); // seq 3
    assert_eq!(tray.on_event(Event::MacrosSaved), []);
    assert_eq!(
        tray.on_event(answer(3, Outcome::Failed, None)),
        [worker(4, WorkerCommand::Poll)]
    );
    // The answer to another command leaves the poll on its way.
    choose(&mut tray, TrayCommand::OpenConfig); // seq 5
    assert_eq!(tray.on_event(Event::MacrosSaved), []);
    assert_eq!(tray.on_event(answer(5, Outcome::Opened(Ok(())), None)), []);
    assert_eq!(
        tray.on_event(answer(4, no_reload(), None)),
        [worker(6, WorkerCommand::Poll)]
    );
}

/// Before the start and after an exit a save asks nothing; the exit closes the editor without
/// waiting, after the panel (it closes without saving, by design).
#[test]
fn the_macro_list_editor_is_closed_at_the_exit() {
    let mut tray = Tray::new(start(Vietnamese), messages());
    assert_eq!(tray.on_event(Event::MacrosSaved), []);
    send(&mut tray, (START.id(), 0, 0));
    let effects = choose(&mut tray, TrayCommand::Exit);
    let panel = effects.iter().position(|e| *e == Effect::ClosePanel);
    let editor = effects.iter().position(|e| *e == Effect::CloseMacroList);
    let worker_finishes = effects.iter().position(|e| *e == Effect::FinishWorker);
    assert!(panel < editor && editor < worker_finishes, "{effects:?}");
    assert_eq!(tray.on_event(Event::MacrosSaved), []);
    assert_eq!(tray.flush_inbox(vec![Event::MacrosSaved]), []);
}

// --- The exclusions dialog (D48) ---------------------------------------------------------------

fn open_exclusions(serial: u64, state: ExclusionsState) -> Effect {
    Effect::OpenExclusions { serial, state }
}

fn refresh_exclusions(state: ExclusionsState) -> Effect {
    Effect::RefreshExclusions(state)
}

fn is_exclusions_refresh(effect: &Effect) -> bool {
    matches!(effect, Effect::RefreshExclusions(_))
}

fn exclusions_request(tray: &mut Tray, request: ExclusionsRequest) -> Vec<Effect> {
    tray.on_event(Event::Exclusions(request))
}

/// The default exclusions followed by `more`.
fn defaults_and(more: &[&str]) -> Vec<ExeName> {
    let mut apps = default_apps();
    apps.extend(more.iter().map(|name| exe(name)));
    apps
}

/// A running tray whose exclusions dialog was opened from the panel (dialog 1).
fn with_exclusions_dialog(start: TrayStart) -> Tray {
    let mut tray = with_panel(start);
    let state = tray.exclusions_state();
    assert_eq!(
        panel_request(&mut tray, PanelRequest::Exclusions),
        [open_exclusions(1, state), refresh(tray.panel_state())]
    );
    tray
}

/// The dialog shows the list in effect and whether its edits are saved.
#[test]
fn the_exclusions_state_is_built_from_the_settings_of_the_tray() {
    let mut settings = start(Vietnamese);
    settings.exclusions = vec![exe("mstsc.exe"), exe("putty.exe")];
    let tray = Tray::new(settings, messages());
    assert_eq!(
        tray.exclusions_state(),
        ExclusionsState {
            apps: vec![exe("mstsc.exe"), exe("putty.exe")],
            can_save: true,
        }
    );
    let mut nowhere = start(Vietnamese);
    nowhere.location = Location::Unavailable;
    nowhere.config_path = None;
    let tray = Tray::new(nowhere, messages());
    assert_eq!(
        tray.exclusions_state(),
        ExclusionsState {
            apps: default_apps(),
            can_save: false,
        }
    );
}

/// "Ứng dụng loại trừ…" of the panel opens the dialog, also without a folder (the edits then
/// take effect in memory); while it is open, the button brings it to the front with the state
/// in effect. Each opening is numbered.
#[test]
fn the_panel_button_opens_the_exclusions_dialog_or_brings_it_to_the_front() {
    let mut tray = with_exclusions_dialog(start(Vietnamese));
    let state = tray.exclusions_state();
    assert_eq!(
        panel_request(&mut tray, PanelRequest::Exclusions),
        [open_exclusions(2, state), refresh(tray.panel_state())]
    );
    let mut nowhere = start(Vietnamese);
    nowhere.location = Location::Unavailable;
    nowhere.config_path = None;
    let tray = with_exclusions_dialog(nowhere);
    assert!(!tray.exclusions_state().can_save);
}

/// An edit goes to the worker as it is; the tray does not work out the list or the rules
/// itself (no snapshot for the hook), and the dialog is shown the list in effect again at
/// once.
#[test]
fn an_edit_of_the_dialog_goes_to_the_worker_and_the_dialog_is_shown_the_list_again() {
    let mut tray = with_exclusions_dialog(start(Vietnamese));
    let add = ExclusionsRequest::Add(exe("notepad.exe"));
    assert_eq!(
        exclusions_request(&mut tray, add.clone()),
        [
            worker(1, WorkerCommand::Exclusions(add)),
            refresh_exclusions(tray.exclusions_state()),
        ]
    );
    assert_eq!(tray.exclusions_state().apps, default_apps());
    assert_eq!(*tray.runtime(), runtime(Vietnamese));
    for request in [
        ExclusionsRequest::Remove(exe("mstsc.exe")),
        ExclusionsRequest::Defaults,
    ] {
        let effects = exclusions_request(&mut tray, request.clone());
        assert!(
            matches!(&effects[..], [Effect::Worker { command, .. }, Effect::RefreshExclusions(_)]
                if *command == WorkerCommand::Exclusions(request.clone())),
            "{effects:?}"
        );
    }
}

/// The worker's answer brings the new list and its rules: the dialog shows the list, the hook
/// gets the snapshot, and the icon shows E at once when the application in front is the one
/// just excluded. A save that worked is no error to tell.
#[test]
fn the_answer_brings_the_new_list_to_the_dialog_the_hook_and_the_icon() {
    let mut tray = with_exclusions_dialog(start(Vietnamese));
    assert_eq!(tray.on_foreground(app("notepad.exe")), []);
    let add = ExclusionsRequest::Add(exe("notepad.exe"));
    exclusions_request(&mut tray, add.clone());
    let mut excluding = runtime(Vietnamese);
    excluding.config_revision = 2;
    excluding
        .compatibility
        .rules
        .insert(0, rule("notepad.exe", None, false));
    let mut settings = state(excluding.clone(), StartupMode::None);
    settings.exclusions = defaults_and(&["notepad.exe"]);
    let effects = tray.on_event(answer(
        1,
        Outcome::Exclusions {
            request: add,
            result: Ok(Saved::Written),
        },
        Some(settings),
    ));
    let shown = tray.exclusions_state();
    assert_eq!(shown.apps, defaults_and(&["notepad.exe"]));
    assert_eq!(
        effects,
        [
            Effect::UpdateIcon(excluded_icon("notepad.exe")),
            hook_runtime(excluding),
            refresh_exclusions(shown),
        ]
    );
}

/// An edit that could not be saved is told once per kind of error, like any change, and the
/// edit is in effect (the answer carries it).
#[test]
fn an_exclusion_edit_that_cannot_be_saved_is_told_once() {
    let mut tray = with_exclusions_dialog(start(Vietnamese));
    let request = ExclusionsRequest::Remove(exe("mstsc.exe"));
    let mut settings = state(runtime(Vietnamese), StartupMode::None);
    settings.exclusions = default_apps().split_off(1);
    exclusions_request(&mut tray, request.clone());
    let failed = |seq| {
        answer(
            seq,
            Outcome::Exclusions {
                request: request.clone(),
                result: Err(ConfigError::PermissionDenied),
            },
            Some(settings.clone()),
        )
    };
    let effects = tray.on_event(failed(1));
    assert_eq!(
        effects,
        [
            Effect::Notify(save_error_notice(
                Some(&config_path()),
                ConfigError::PermissionDenied
            )),
            refresh_exclusions(tray.exclusions_state()),
        ]
    );
    exclusions_request(&mut tray, request.clone());
    assert_eq!(tray.on_event(failed(2)), [], "told already");
    // A save that works ends it: the next failure is told again.
    exclusions_request(&mut tray, request.clone());
    tray.on_event(answer(
        3,
        Outcome::Exclusions {
            request: request.clone(),
            result: Ok(Saved::Written),
        },
        Some(settings.clone()),
    ));
    exclusions_request(&mut tray, request.clone());
    assert!(matches!(tray.on_event(failed(4))[..], [Effect::Notify(_)]));
}

/// A hand edit of `[exclusions]` taken over from a poll reaches the open dialog; a poll that
/// changes nothing refreshes nothing, and nothing is sent once the dialog closed.
#[test]
fn a_hand_edit_of_the_exclusions_refreshes_the_open_dialog() {
    let mut tray = with_exclusions_dialog(start(Vietnamese));
    send(&mut tray, tick()); // seq 1
    assert!(
        !tray
            .on_event(answer(
                1,
                polled(clean()),
                Some(state(runtime(Vietnamese), StartupMode::None))
            ))
            .iter()
            .any(is_exclusions_refresh)
    );
    send(&mut tray, tick()); // seq 2
    let mut edited = runtime(Vietnamese);
    edited
        .compatibility
        .rules
        .insert(0, rule("putty.exe", None, false));
    let mut settings = state(edited.clone(), StartupMode::None);
    settings.exclusions = vec![exe("putty.exe")];
    let effects = tray.on_event(answer(2, polled(clean()), Some(settings.clone())));
    assert_eq!(
        effects,
        [
            hook_runtime(edited),
            refresh_exclusions(ExclusionsState {
                apps: vec![exe("putty.exe")],
                can_save: true,
            }),
        ]
    );
    // Closed: no refresh, also when the list changes again.
    assert_eq!(tray.on_event(Event::ExclusionsClosed(1)), []);
    send(&mut tray, tick()); // seq 3
    settings.exclusions = Vec::new();
    settings.runtime = runtime(Vietnamese);
    assert!(
        !tray
            .on_event(answer(3, polled(clean()), Some(settings)))
            .iter()
            .any(is_exclusions_refresh)
    );
    assert!(tray.exclusions_state().apps.is_empty());
}

/// The close of a dialog that was replaced meanwhile does not stop the refreshes of the new
/// one; an edit from a dialog the tray does not know as open still acts.
#[test]
fn the_close_of_a_replaced_exclusions_dialog_does_not_stop_the_refreshes() {
    let mut tray = with_exclusions_dialog(start(Vietnamese));
    panel_request(&mut tray, PanelRequest::Exclusions); // dialog 2
    assert_eq!(tray.on_event(Event::ExclusionsClosed(1)), []);
    let effects = exclusions_request(&mut tray, ExclusionsRequest::Defaults);
    assert_eq!(
        effects.last(),
        Some(&refresh_exclusions(tray.exclusions_state()))
    );
    assert_eq!(tray.on_event(Event::ExclusionsClosed(2)), []);
    assert_eq!(
        exclusions_request(&mut tray, ExclusionsRequest::Defaults),
        [worker(
            2,
            WorkerCommand::Exclusions(ExclusionsRequest::Defaults)
        )]
    );
}

/// Exit closes the dialog without waiting, after the panel and the macro list editor and
/// before the worker finishes; nothing acts on it afterwards, and its close left in the inbox
/// is dropped (its edits are not: see the next test).
#[test]
fn exit_closes_the_exclusions_dialog_and_nothing_follows() {
    let mut tray = with_exclusions_dialog(start(Vietnamese));
    let effects = choose(&mut tray, TrayCommand::Exit);
    assert_eq!(effects, exit_effects());
    let editor = effects.iter().position(|e| *e == Effect::CloseMacroList);
    let dialog = effects.iter().position(|e| *e == Effect::CloseExclusions);
    let worker_finishes = effects.iter().position(|e| *e == Effect::FinishWorker);
    assert!(editor < dialog && dialog < worker_finishes, "{effects:?}");
    assert_eq!(
        exclusions_request(&mut tray, ExclusionsRequest::Defaults),
        []
    );
    assert_eq!(tray.on_event(Event::ExclusionsClosed(1)), []);
    assert_eq!(panel_request(&mut tray, PanelRequest::Exclusions), []);
    assert_eq!(tray.flush_inbox(vec![Event::ExclusionsClosed(1)]), []);
    // Before the start the dialog's events do nothing either.
    let mut tray = Tray::new(start(Vietnamese), messages());
    assert_eq!(
        exclusions_request(&mut tray, ExclusionsRequest::Defaults),
        []
    );
}

/// At exit the edits of the exclusions dialog left in the inbox go to the worker before it
/// finishes, in their order, with the last hotkey change at its place among them: each took
/// effect for the user the moment it was made. Nothing else of the inbox acts.
#[test]
fn exit_sends_the_exclusions_edits_left_in_the_inbox_to_the_worker() {
    let mut tray = running(Vietnamese);
    assert_eq!(choose(&mut tray, TrayCommand::Exit), exit_effects());
    let add = ExclusionsRequest::Add(exe("putty.exe"));
    assert_eq!(
        tray.flush_inbox(vec![
            Event::Exclusions(add.clone()),
            Event::Hotkey(false),
            Event::ExclusionsClosed(1),
            Event::Panel(PanelRequest::OpenConfig),
            Event::Exclusions(ExclusionsRequest::Defaults),
            Event::Hotkey(true),
            Event::MacrosSaved,
        ]),
        [
            worker(1, WorkerCommand::Exclusions(add)),
            worker(2, WorkerCommand::Exclusions(ExclusionsRequest::Defaults)),
            worker(3, WorkerCommand::Apply(Change::VietnameseMode(true))),
        ]
    );
    assert_eq!(tray.mode(), Vietnamese);
    assert_eq!(tray.flush_inbox(Vec::new()), []);
}

/// The dialog's events keep their order among the events, like the panel's.
#[test]
fn exclusions_events_keep_their_order_among_the_events() {
    let events = vec![
        Event::Exclusions(ExclusionsRequest::Add(exe("a.exe"))),
        Event::Hotkey(false),
        Event::ExclusionsClosed(3),
        Event::Hotkey(true),
        Event::Exclusions(ExclusionsRequest::Defaults),
    ];
    assert_eq!(
        coalesce(events),
        [
            Event::Exclusions(ExclusionsRequest::Add(exe("a.exe"))),
            Event::ExclusionsClosed(3),
            Event::Hotkey(true),
            Event::Exclusions(ExclusionsRequest::Defaults),
        ]
    );
}

/// The panel's icon is the app icon of `raumakey.exe`, as `raumakey-resources` numbers it.
#[test]
fn the_app_icon_id_is_the_one_of_the_resources() {
    assert_eq!(ICON_APP, raumakey_resources::ICON_APP);
    assert_ne!(ICON_APP, ICON_VIETNAMESE);
    assert_ne!(ICON_APP, ICON_ENGLISH);
}

// --- Window plumbing -----------------------------------------------------------------------

#[test]
fn the_private_messages_are_postable() {
    assert_eq!(TRAY_CALLBACK.id(), WM_APP + 1);
    assert_eq!(START.id(), WM_APP + 2);
    assert_eq!(WAKE.id(), WM_APP + 3);
    assert_eq!(FOREGROUND.id(), WM_APP + 4);
    for message in [TRAY_CALLBACK, START, WAKE, FOREGROUND] {
        assert!(is_postable(message.id()));
    }
    assert_eq!(TRAY_ICON_ID, 1);
}

#[test]
fn other_messages_get_the_default_processing() {
    let mut tray = running(Vietnamese);
    for message in [
        WM_NULL,
        WM_CREATE,
        WM_QUERYENDSESSION,
        WM_APP,
        WM_APP + 5,
        0xC000,
        TASKBAR_CREATED + 1,
        SHOW_PANEL + 1,
    ] {
        assert_eq!(send(&mut tray, (message, 1, 2)), default(), "{message:#x}");
    }
    assert_eq!(tray.mode(), Vietnamese);
}

/// A registration that failed (0) or any number outside the registered range is never taken
/// for `TaskbarCreated` or the show-panel request.
#[test]
fn only_a_registered_message_number_is_taken_for_taskbar_created_or_show_panel() {
    for number in [0, 0x1_0000, WM_CLOSE] {
        for messages in [
            RegisteredMessages {
                taskbar_created: number,
                show_panel: 0,
            },
            RegisteredMessages {
                taskbar_created: 0,
                show_panel: number,
            },
        ] {
            let mut tray = Tray::new(start(Vietnamese), messages);
            send(&mut tray, (START.id(), 0, 0));
            let expected = match number {
                WM_CLOSE => answered(exit_effects()),
                _ => default(),
            };
            assert_eq!(send(&mut tray, (number, 0, 0)), expected, "{messages:?}");
            assert_eq!(tray.shell_messages(), [TRAY_CALLBACK.id()], "{messages:?}");
        }
    }
    assert_eq!(RegisteredMessages::default().taskbar_created, 0);
    assert_eq!(RegisteredMessages::default().show_panel, 0);
}

#[test]
fn the_loop_must_end_when_a_running_tray_loses_its_window() {
    let tray = Tray::new(start(Vietnamese), messages());
    assert!(
        !tray.ends_loop_when_dropped(),
        "before the start no loop runs yet"
    );
    let mut tray = running(Vietnamese);
    assert!(tray.ends_loop_when_dropped());
    send(&mut tray, (WM_CLOSE, 0, 0));
    assert!(!tray.ends_loop_when_dropped(), "Exit ended it already");
}

/// An elevated controller lets `TaskbarCreated`, the show-panel request of a second start
/// from a normal prompt (D45), and the icon's callback through UIPI.
#[test]
fn an_elevated_controller_lets_the_shell_messages_through() {
    let tray = Tray::new(start(Vietnamese), messages());
    assert_eq!(
        tray.shell_messages(),
        vec![TASKBAR_CREATED, SHOW_PANEL, TRAY_CALLBACK.id()]
    );
    for message in tray.shell_messages() {
        assert!(is_postable(message), "{message:#x}");
    }
    assert_eq!(
        Tray::new(start(Vietnamese), RegisteredMessages::default()).shell_messages(),
        vec![TRAY_CALLBACK.id()]
    );
}

/// `Debug` of the tray shows its state; it holds nothing typed and no macro text.
#[test]
fn the_tray_debug_output_names_its_state() {
    let tray = running(Vietnamese);
    let shown = format!("{tray:?}");
    assert!(shown.contains("Tray"), "{shown}");
    assert!(shown.contains("Vietnamese"), "{shown}");
}
