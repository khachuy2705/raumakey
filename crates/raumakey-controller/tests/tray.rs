//! The tray state machine (`tray`): the menu of D42 and its checks, the icon and tooltip, the
//! effect of each choice (the typing hook, the settings worker, message boxes), the messages
//! of the hidden window (the icon, the poll timer, `TaskbarCreated`, the end of the session),
//! the hotkey's mode changes from the hook observer, and the worker's answers. Pure: these
//! tests run on every host.

use std::path::PathBuf;

use raumakey_config::{ConfigError, StartupMode};
use raumakey_controller::notice::{
    Notice, NoticeKind, Question, about_notice, elevated_task_notice, open_error_notice,
    recovery_notice, save_error_notice, startup_error_notice, status_notice, temporary_exe_notice,
};
use raumakey_controller::settings::{Change, DocumentStatus, Location, Recovery, Settings, Status};
use raumakey_controller::tray::{
    Effect, Event, IconState, InputMode, LOOP_EXIT_CODE, MENU_ABOUT, MENU_EXIT, MENU_MACROS,
    MENU_MODERN_TONE, MENU_OPEN_CONFIG, MENU_SPELL_CHECK, MENU_STARTUP, MENU_TELEX,
    MENU_VIETNAMESE_MODE, MENU_VIQR, MENU_VNI, POLL_INTERVAL_MS, POLL_TIMER, Reply, START,
    TOOLTIP_ENGLISH, TRAY_CALLBACK, TRAY_ICON_ID, Tray, TrayCommand, TrayStart, WAKE, coalesce,
    with_change,
};
use raumakey_controller::worker::{
    Answer, OpenError, Outcome, PollReport, Saved, SettingsState, ShortcutSync, StartupError,
    WorkerCommand,
};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::options::InputMethod;
use raumakey_resources::{ICON_ENGLISH, ICON_VIETNAMESE};
use raumakey_windows::error::{E_ACCESSDENIED, WinError};
use raumakey_windows::menu::MenuItem;
use raumakey_windows::shell::{NIN_KEYSELECT, NIN_SELECT, TOOLTIP_CAPACITY, WM_CONTEXTMENU};
use raumakey_windows::window::{WM_APP, WM_CLOSE, WM_ENDSESSION, WM_TIMER, is_postable};

use InputMode::{English, Vietnamese};

/// A registered message number standing in for `TaskbarCreated`, which Windows registers at
/// run time.
const TASKBAR_CREATED: u32 = 0xC0DE;

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
    }
}

/// The effects of an exit, in order.
fn exit_effects() -> Vec<Effect> {
    vec![
        Effect::StopTyping,
        Effect::DismissNotices,
        Effect::FlushHotkeys,
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

fn send(tray: &mut Tray, (message, wparam, lparam): Message) -> Reply {
    tray.on_message(message, wparam, lparam)
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
    IconState { mode, method }
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

/// A tray whose message loop runs: [`START`] added its icon.
fn running_with(start: TrayStart) -> Tray {
    let mode = InputMode::from_vietnamese(start.runtime.vietnamese_mode);
    let method = start.runtime.options.input_method;
    let mut tray = Tray::new(start, TASKBAR_CREATED);
    assert_eq!(
        tray.on_message(START.id(), 0, 0),
        answered(vec![Effect::ShowIcon(icon(mode, method))])
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
    SettingsState { runtime, startup }
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

/// D42: flat, these labels in this order, separators between the groups.
#[test]
fn the_menu_has_the_labels_of_d42_in_order() {
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
            Some("Gõ tiếng Việt (Ctrl+Shift)"),
            None,
            Some("Telex"),
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
    assert_eq!(MENU_VIETNAMESE_MODE, "Gõ tiếng Việt (Ctrl+Shift)");
    assert_eq!([MENU_TELEX, MENU_VNI, MENU_VIQR], ["Telex", "VNI", "VIQR"]);
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
            (TrayCommand::VietnameseMode.id(), true),
            (TrayCommand::Telex.id(), true),
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
            (TrayCommand::VietnameseMode.id(), false),
            (TrayCommand::Telex.id(), false),
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
    let mut tray = Tray::new(settings, TASKBAR_CREATED);
    assert_eq!(tray.mode(), English);
    assert_eq!(
        tray.on_message(START.id(), 0, 0),
        answered(vec![
            Effect::ShowIcon(icon(English, InputMethod::Vni)),
            Effect::Notify(notice),
        ])
    );
    // Another start (anyone may post one) adds nothing.
    assert_eq!(tray.on_message(START.id(), 0, 0), answered(Vec::new()));
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
        tray.flush_hotkeys(vec![
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
        tray.flush_hotkeys(vec![Event::Answered(Question::Recover)]),
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
        WM_LBUTTONDBLCLK,
        WM_RBUTTONUP,
        NIN_POPUPOPEN,
    ] {
        assert_eq!(
            send(&mut tray, callback(code, TRAY_ICON_ID, 1, 2)),
            answered(Vec::new()),
            "{code:#x}"
        );
    }
    for code in [NIN_SELECT, NIN_KEYSELECT, WM_CONTEXTMENU] {
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
    let mut tray = Tray::new(start(Vietnamese), TASKBAR_CREATED);
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
        tray.on_message(START.id(), 0, 0),
        answered(vec![Effect::ShowIcon(icon(Vietnamese, InputMethod::Telex))])
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
    for change in [
        Change::InputMethod(InputMethod::Vni),
        Change::VietnameseMode(false),
        Change::SpellCheck(false),
        Change::ModernTone(true),
        Change::Macros(false),
        Change::InputMethod(InputMethod::Viqr),
        Change::VietnameseMode(true),
        Change::Startup(StartupMode::Shortcut),
        Change::InputMethod(InputMethod::Telex),
        Change::Macros(true),
    ] {
        let mut predicted = with_change(settings.runtime_snapshot(), change);
        settings.apply(change).unwrap();
        predicted.config_revision = settings.runtime_snapshot().config_revision;
        assert_eq!(predicted, *settings.runtime_snapshot(), "{change:?}");
    }
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

/// Exit stops the hook first (typing is plain at once), closes the open notices without
/// waiting for them, saves the hotkey changes left in the inbox, lets the worker finish, and
/// only then removes the icon and ends the loop.
#[test]
fn exit_stops_the_hook_finishes_the_worker_then_removes_the_icon() {
    let mut tray = running(Vietnamese);
    assert_eq!(choose(&mut tray, TrayCommand::Exit), exit_effects());
    assert_eq!(
        exit_effects(),
        [
            Effect::StopTyping,
            Effect::DismissNotices,
            Effect::FlushHotkeys,
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
    let mut tray = Tray::new(start(Vietnamese), TASKBAR_CREATED);
    assert_eq!(tray.on_message(WM_CLOSE, 0, 0), answered(exit_effects()));
    assert_eq!(tray.on_message(START.id(), 0, 0), answered(Vec::new()));
}

#[test]
fn a_cancelled_end_of_the_session_changes_nothing() {
    let mut tray = running(Vietnamese);
    assert_eq!(tray.on_message(WM_ENDSESSION, 0, 0), default());
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
/// nothing more, so a slow disk never piles them up.
#[test]
fn the_timer_asks_the_worker_to_poll_once_at_a_time() {
    assert_eq!(POLL_INTERVAL_MS, 2_000);
    assert_ne!(POLL_TIMER, 0);
    let mut tray = running(Vietnamese);
    let shown = Effect::EnsureIcon(icon(Vietnamese, InputMethod::Telex));
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![shown.clone(), worker(1, WorkerCommand::Poll)])
    );
    assert_eq!(send(&mut tray, tick()), answered(vec![shown.clone()]));
    assert_eq!(
        tray.on_event(answer(1, Outcome::Polled(PollReport::default()), None)),
        []
    );
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![shown.clone(), worker(2, WorkerCommand::Poll)])
    );
    // A command that panicked frees the poll as well.
    assert_eq!(tray.on_event(answer(2, Outcome::Failed, None)), []);
    assert_eq!(
        send(&mut tray, tick()),
        answered(vec![shown, worker(3, WorkerCommand::Poll)])
    );
    // Another timer of the window is not the poll timer.
    assert_eq!(tray.on_message(WM_TIMER, POLL_TIMER + 1, 0), default());
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
    assert_eq!(tray.on_message(WAKE.id(), 0, 0), answered(Vec::new()));
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

// --- Window plumbing -----------------------------------------------------------------------

#[test]
fn the_private_messages_are_postable() {
    assert_eq!(TRAY_CALLBACK.id(), WM_APP + 1);
    assert_eq!(START.id(), WM_APP + 2);
    assert_eq!(WAKE.id(), WM_APP + 3);
    for message in [TRAY_CALLBACK, START, WAKE] {
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
        WM_APP + 4,
        0xC000,
        TASKBAR_CREATED + 1,
    ] {
        assert_eq!(tray.on_message(message, 1, 2), default(), "{message:#x}");
    }
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn only_a_registered_message_number_is_taken_for_taskbar_created() {
    let mut tray = Tray::new(start(Vietnamese), 0);
    tray.on_message(START.id(), 0, 0);
    assert_eq!(tray.on_message(WM_NULL, 0, 0), default());
    let mut tray = Tray::new(start(Vietnamese), 0x1_0000);
    tray.on_message(START.id(), 0, 0);
    assert_eq!(tray.on_message(0x1_0000, 0, 0), default());
    let mut tray = Tray::new(start(Vietnamese), WM_CLOSE);
    tray.on_message(START.id(), 0, 0);
    assert_eq!(tray.on_message(WM_CLOSE, 0, 0), answered(exit_effects()));
}

#[test]
fn the_loop_must_end_when_a_running_tray_loses_its_window() {
    let tray = Tray::new(start(Vietnamese), TASKBAR_CREATED);
    assert!(
        !tray.ends_loop_when_dropped(),
        "before the start no loop runs yet"
    );
    let mut tray = running(Vietnamese);
    assert!(tray.ends_loop_when_dropped());
    send(&mut tray, (WM_CLOSE, 0, 0));
    assert!(!tray.ends_loop_when_dropped(), "Exit ended it already");
}

#[test]
fn an_elevated_controller_lets_the_shell_messages_through() {
    let tray = Tray::new(start(Vietnamese), TASKBAR_CREATED);
    assert_eq!(
        tray.shell_messages(),
        vec![TASKBAR_CREATED, TRAY_CALLBACK.id()]
    );
    for message in tray.shell_messages() {
        assert!(is_postable(message), "{message:#x}");
    }
    assert_eq!(
        Tray::new(start(Vietnamese), 0).shell_messages(),
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
