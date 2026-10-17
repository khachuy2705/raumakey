//! The tray state machine (`tray`): the input mode that the notification icon shows, the icon
//! and tooltip of each mode, the menu, and the answer to each window message of the hidden
//! window (plan §10.2; design §11; matrix #58). Pure: these tests run on every host.

use raumakey_controller::tray::{
    Effect, InputMode, LOOP_EXIT_CODE, MENU_EXIT, MENU_VIETNAMESE_MODE, Reply, START,
    TOOLTIP_ENGLISH, TOOLTIP_VIETNAMESE, TRAY_CALLBACK, TRAY_ICON_ID, Tray, TrayCommand,
    initial_mode, menu_items,
};
use raumakey_model::options::EngineOptions;
use raumakey_resources::{ICON_ENGLISH, ICON_VIETNAMESE};
use raumakey_windows::menu::MenuItem;
use raumakey_windows::shell::{NIN_KEYSELECT, NIN_SELECT, TOOLTIP_CAPACITY, WM_CONTEXTMENU};
use raumakey_windows::window::{WM_APP, WM_CLOSE, WM_ENDSESSION, is_postable};

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

fn send(tray: &mut Tray, (message, wparam, lparam): Message) -> Reply {
    tray.on_message(message, wparam, lparam)
}

fn answered(effect: Option<Effect>) -> Reply {
    Reply {
        result: ANSWERED,
        effect,
    }
}

/// The default processing of `DefWindowProcW`, with no effect.
const DEFAULT: Reply = Reply {
    result: None,
    effect: None,
};

/// A tray in `mode` whose message loop runs: [`START`] added its icon.
fn running(mode: InputMode) -> Tray {
    let mut tray = Tray::new(mode, TASKBAR_CREATED);
    assert_eq!(
        tray.on_message(START.id(), 0, 0),
        answered(Some(Effect::ShowIcon(mode)))
    );
    tray
}

#[test]
fn the_mode_starts_from_the_engine_defaults_v1() {
    assert_eq!(initial_mode(), Vietnamese);
    assert_eq!(
        initial_mode().is_vietnamese(),
        EngineOptions::defaults_v1().enabled
    );
    assert_eq!(InputMode::from_vietnamese(true), Vietnamese);
    assert_eq!(InputMode::from_vietnamese(false), English);
}

#[test]
fn toggling_switches_between_the_two_modes() {
    assert_eq!(Vietnamese.toggled(), English);
    assert_eq!(English.toggled(), Vietnamese);
    assert!(Vietnamese.is_vietnamese());
    assert!(!English.is_vietnamese());
}

#[test]
fn each_mode_shows_its_own_icon_and_a_tooltip_naming_raumakey() {
    assert_eq!(Vietnamese.icon_id(), ICON_VIETNAMESE);
    assert_eq!(English.icon_id(), ICON_ENGLISH);
    assert_eq!(Vietnamese.tooltip(), TOOLTIP_VIETNAMESE);
    assert_eq!(English.tooltip(), TOOLTIP_ENGLISH);
    assert_ne!(TOOLTIP_VIETNAMESE, TOOLTIP_ENGLISH);
    for tooltip in [TOOLTIP_VIETNAMESE, TOOLTIP_ENGLISH] {
        assert!(tooltip.starts_with("Raumakey"), "{tooltip}");
        // The tooltip buffer of the notification icon holds it without cutting it.
        assert!(
            tooltip.encode_utf16().count() < TOOLTIP_CAPACITY,
            "{tooltip}"
        );
        assert!(!tooltip.contains('\0'));
    }
    // New Vietnamese strings, with diacritics.
    assert!(
        TOOLTIP_VIETNAMESE.contains("tiếng Việt"),
        "{TOOLTIP_VIETNAMESE}"
    );
    assert!(TOOLTIP_ENGLISH.contains("tiếng Anh"), "{TOOLTIP_ENGLISH}");
}

#[test]
fn the_menu_offers_vietnamese_mode_as_a_check_item_and_exit() {
    let vietnamese_mode = TrayCommand::VietnameseMode.id();
    let exit = TrayCommand::Exit.id();
    assert_eq!(
        menu_items(Vietnamese),
        [
            MenuItem::command(vietnamese_mode, MENU_VIETNAMESE_MODE).checked(true),
            MenuItem::Separator,
            MenuItem::command(exit, MENU_EXIT),
        ]
    );
    assert_eq!(
        menu_items(English),
        [
            MenuItem::command(vietnamese_mode, MENU_VIETNAMESE_MODE),
            MenuItem::Separator,
            MenuItem::command(exit, MENU_EXIT),
        ]
    );
    for mode in [Vietnamese, English] {
        for item in menu_items(mode) {
            item.validate().unwrap();
        }
    }
    // New Vietnamese strings with one mnemonic each.
    assert!(
        MENU_VIETNAMESE_MODE.contains("tiếng Việt"),
        "{MENU_VIETNAMESE_MODE}"
    );
    assert!(MENU_EXIT.contains("Thoát"), "{MENU_EXIT}");
    for text in [MENU_VIETNAMESE_MODE, MENU_EXIT] {
        assert_eq!(text.matches('&').count(), 1, "{text}");
    }
    // The tray's menu follows its mode.
    let mut tray = running(Vietnamese);
    assert_eq!(tray.menu(), menu_items(Vietnamese));
    send(&mut tray, click());
    assert_eq!(tray.menu(), menu_items(English));
}

#[test]
fn command_ids_are_distinct_non_zero_and_round_trip() {
    for command in TrayCommand::ALL {
        assert_ne!(command.id(), 0, "{command:?}");
        assert_eq!(TrayCommand::from_id(command.id()), Some(command));
    }
    assert_ne!(TrayCommand::VietnameseMode.id(), TrayCommand::Exit.id());
    assert_eq!(TrayCommand::from_id(0), None);
    assert_eq!(TrayCommand::from_id(99), None);
}

#[test]
fn the_private_messages_are_postable_and_the_loop_ends_with_0() {
    assert_eq!(TRAY_CALLBACK.id(), WM_APP + 1);
    assert_eq!(START.id(), WM_APP + 2);
    assert!(is_postable(TRAY_CALLBACK.id()));
    assert!(is_postable(START.id()));
    assert_eq!(TRAY_ICON_ID, 1);
    assert_eq!(LOOP_EXIT_CODE, 0);
}

#[test]
fn start_adds_the_icon_with_the_current_mode_once() {
    for mode in [Vietnamese, English] {
        let mut tray = Tray::new(mode, TASKBAR_CREATED);
        assert_eq!(
            tray.on_message(START.id(), 0, 0),
            answered(Some(Effect::ShowIcon(mode)))
        );
        // Another start (anyone may post one) adds nothing.
        assert_eq!(tray.on_message(START.id(), 0, 0), answered(None));
        assert_eq!(tray.mode(), mode);
    }
}

#[test]
fn a_click_or_the_keyboard_switches_the_mode_and_its_icon() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        send(&mut tray, click()),
        answered(Some(Effect::UpdateIcon(English)))
    );
    assert_eq!(tray.mode(), English);
    assert_eq!(
        send(&mut tray, callback(NIN_KEYSELECT, TRAY_ICON_ID, 0, 0)),
        answered(Some(Effect::UpdateIcon(Vietnamese)))
    );
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn a_right_click_opens_the_menu_at_its_anchor() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        send(&mut tray, callback(WM_CONTEXTMENU, TRAY_ICON_ID, -20, 300)),
        answered(Some(Effect::OpenMenu { x: -20, y: 300 }))
    );
    assert_eq!(tray.mode(), Vietnamese, "opening the menu changes nothing");
}

#[test]
fn other_notifications_and_other_icons_change_nothing() {
    let mut tray = running(Vietnamese);
    // The shell also sends the raw mouse messages of a click; only NIN_SELECT switches.
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
            answered(None),
            "{code:#x}"
        );
    }
    for code in [NIN_SELECT, NIN_KEYSELECT, WM_CONTEXTMENU] {
        assert_eq!(
            send(&mut tray, callback(code, TRAY_ICON_ID + 1, 1, 2)),
            answered(None),
            "another icon, {code:#x}"
        );
    }
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn before_the_start_the_icon_does_not_exist_and_its_messages_change_nothing() {
    let mut tray = Tray::new(Vietnamese, TASKBAR_CREATED);
    for message in [
        click(),
        callback(WM_CONTEXTMENU, TRAY_ICON_ID, 1, 2),
        taskbar_created(),
    ] {
        assert_eq!(send(&mut tray, message), answered(None), "{message:x?}");
    }
    assert_eq!(tray.mode(), Vietnamese);
    assert_eq!(
        tray.menu_choice(Some(TrayCommand::VietnameseMode.id())),
        None
    );
    // The start adds the icon then, with the mode unchanged.
    assert_eq!(
        tray.on_message(START.id(), 0, 0),
        answered(Some(Effect::ShowIcon(Vietnamese)))
    );
}

#[test]
fn matrix_58_taskbar_created_adds_the_icon_again_without_changing_the_mode() {
    for mode in [Vietnamese, English] {
        let mut tray = running(Vietnamese);
        if mode == English {
            send(&mut tray, click());
        }
        // Explorer may restart any number of times.
        for _ in 0..2 {
            assert_eq!(
                send(&mut tray, taskbar_created()),
                answered(Some(Effect::ShowIcon(mode)))
            );
            assert_eq!(tray.mode(), mode);
            assert_eq!(tray.menu(), menu_items(mode));
        }
    }
}

#[test]
fn choosing_vietnamese_mode_in_the_menu_switches_the_mode() {
    let mut tray = running(Vietnamese);
    let vietnamese_mode = Some(TrayCommand::VietnameseMode.id());
    assert_eq!(
        tray.menu_choice(vietnamese_mode),
        Some(Effect::UpdateIcon(English))
    );
    assert_eq!(tray.mode(), English);
    assert_eq!(
        tray.menu_choice(vietnamese_mode),
        Some(Effect::UpdateIcon(Vietnamese))
    );
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn a_dismissed_menu_or_an_unknown_command_changes_nothing() {
    let mut tray = running(English);
    assert_eq!(tray.menu_choice(None), None);
    assert_eq!(tray.menu_choice(Some(0)), None);
    assert_eq!(tray.menu_choice(Some(99)), None);
    assert_eq!(tray.mode(), English);
    assert!(tray.ends_loop_when_dropped());
}

#[test]
fn exit_from_the_menu_ends_the_tray_and_nothing_brings_the_icon_back() {
    let mut tray = running(Vietnamese);
    assert_eq!(
        tray.menu_choice(Some(TrayCommand::Exit.id())),
        Some(Effect::Exit)
    );
    assert!(!tray.ends_loop_when_dropped(), "Exit ends the loop itself");
    // Messages still queued behind the exit are answered without effect: a TaskbarCreated
    // must not add the removed icon again.
    for message in [
        (START.id(), 0, 0),
        click(),
        callback(WM_CONTEXTMENU, TRAY_ICON_ID, 1, 2),
        taskbar_created(),
        (WM_CLOSE, 0, 0),
        (WM_ENDSESSION, 1, 0),
    ] {
        assert_eq!(send(&mut tray, message), answered(None), "{message:x?}");
    }
    assert_eq!(tray.menu_choice(Some(TrayCommand::Exit.id())), None);
    assert_eq!(
        tray.menu_choice(Some(TrayCommand::VietnameseMode.id())),
        None
    );
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn close_and_the_end_of_the_session_exit_like_the_menu() {
    for message in [
        (WM_CLOSE, 0, 0),
        (WM_ENDSESSION, 1, 0),
        (WM_ENDSESSION, 1, ENDSESSION_CLOSEAPP),
        (WM_ENDSESSION, 1, ENDSESSION_LOGOFF),
    ] {
        let mut tray = running(English);
        // Always answered: `DefWindowProcW` would destroy the window under the running tray.
        assert_eq!(
            send(&mut tray, message),
            answered(Some(Effect::Exit)),
            "{message:x?}"
        );
        assert!(!tray.ends_loop_when_dropped());
        assert_eq!(tray.mode(), English);
    }
    // Before the start too; the start then adds no icon.
    let mut tray = Tray::new(Vietnamese, TASKBAR_CREATED);
    assert_eq!(
        tray.on_message(WM_CLOSE, 0, 0),
        answered(Some(Effect::Exit))
    );
    assert_eq!(tray.on_message(START.id(), 0, 0), answered(None));
}

#[test]
fn a_cancelled_end_of_the_session_changes_nothing() {
    let mut tray = running(Vietnamese);
    assert_eq!(tray.on_message(WM_ENDSESSION, 0, 0), DEFAULT);
    assert!(tray.ends_loop_when_dropped());
    assert_eq!(
        send(&mut tray, click()),
        answered(Some(Effect::UpdateIcon(English)))
    );
}

#[test]
fn other_messages_get_the_default_processing() {
    let mut tray = running(Vietnamese);
    for message in [
        WM_NULL,
        WM_CREATE,
        WM_QUERYENDSESSION,
        WM_APP,
        WM_APP + 3,
        0xC000,
        TASKBAR_CREATED + 1,
    ] {
        assert_eq!(tray.on_message(message, 1, 2), DEFAULT, "{message:#x}");
    }
    assert_eq!(tray.mode(), Vietnamese);
}

#[test]
fn only_a_registered_message_number_is_taken_for_taskbar_created() {
    // Registration failed (0) or a number outside the registered range.
    let mut tray = Tray::new(Vietnamese, 0);
    tray.on_message(START.id(), 0, 0);
    assert_eq!(tray.on_message(WM_NULL, 0, 0), DEFAULT);
    let mut tray = Tray::new(Vietnamese, 0x1_0000);
    tray.on_message(START.id(), 0, 0);
    assert_eq!(tray.on_message(0x1_0000, 0, 0), DEFAULT);
    let mut tray = Tray::new(Vietnamese, WM_CLOSE);
    tray.on_message(START.id(), 0, 0);
    assert_eq!(
        tray.on_message(WM_CLOSE, 0, 0),
        answered(Some(Effect::Exit))
    );
    let mut tray = Tray::new(Vietnamese, TRAY_CALLBACK.id());
    tray.on_message(START.id(), 0, 0);
    assert_eq!(
        send(&mut tray, click()),
        answered(Some(Effect::UpdateIcon(English)))
    );
}

#[test]
fn the_loop_must_end_when_a_running_tray_loses_its_window() {
    let tray = Tray::new(Vietnamese, TASKBAR_CREATED);
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
    let tray = Tray::new(Vietnamese, TASKBAR_CREATED);
    assert_eq!(
        tray.shell_messages(),
        vec![TASKBAR_CREATED, TRAY_CALLBACK.id()]
    );
    for message in tray.shell_messages() {
        assert!(is_postable(message), "{message:#x}");
    }
    // Without a registered TaskbarCreated only the icon's callback is left.
    assert_eq!(
        Tray::new(Vietnamese, 0).shell_messages(),
        vec![TRAY_CALLBACK.id()]
    );
}
