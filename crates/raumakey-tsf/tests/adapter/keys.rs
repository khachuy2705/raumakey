//! Key messages: `wParam`/`lParam` decoding, the modifier state, the text of a key, AltGr's
//! synthetic left Ctrl, and the keyboard layout.

use raumakey_model::input::{MAX_EVENT_TEXT_UNITS, Modifiers, vk};
use raumakey_tip::adapter::keys::{
    AltGrFilter, KeyMessage, Layout, LayoutTracker, ModifierKeys, key_text,
};
use raumakey_windows::keyboard::{KeyboardSnapshot, Translation};

/// Scan codes of the keys these tests press.
const SCAN_CTRL: u32 = 0x1D;
const SCAN_ALT: u32 = 0x38;
const SCAN_E: u32 = 0x12;

/// A key transition as TSF reports it.
fn message(virtual_key: u32, scan_code: u32, extended: bool, down: bool) -> KeyMessage {
    KeyMessage {
        virtual_key,
        scan_code,
        extended,
        repeat: !down,
        released: !down,
    }
}

fn left_ctrl(down: bool) -> KeyMessage {
    message(vk::CONTROL, SCAN_CTRL, false, down)
}

fn right_ctrl(down: bool) -> KeyMessage {
    message(vk::CONTROL, SCAN_CTRL, true, down)
}

fn right_alt(down: bool) -> KeyMessage {
    message(vk::MENU, SCAN_ALT, true, down)
}

fn letter_e(down: bool) -> KeyMessage {
    message(u32::from(b'E'), SCAN_E, false, down)
}

/// The modifier keys with `held` keys down and `toggled` keys toggled on.
fn keys(held: &[u32], toggled: &[u32]) -> ModifierKeys {
    ModifierKeys::read(|key| held.contains(&key), |key| toggled.contains(&key))
}

#[test]
fn a_first_key_down_is_decoded_from_wparam_and_lparam() {
    // Repeat count 1, scan code 0x1E, no flags.
    let key = KeyMessage::decode(0x41, 0x001E_0001);
    assert_eq!(key.virtual_key, 0x41);
    assert_eq!(key.scan_code, 0x1E);
    assert!(!key.extended);
    assert!(!key.repeat);
    assert!(!key.released);
}

#[test]
fn an_auto_repeat_is_a_key_that_was_already_down() {
    // Bit 30: the key was down before this message.
    let key = KeyMessage::decode(0x41, 0x401E_0001);
    assert!(key.repeat);
    assert!(!key.released);
}

#[test]
fn right_alt_is_the_alt_key_with_the_extended_flag() {
    // Bit 24: extended key (the E0 prefix); bit 29: Alt is down.
    let right = KeyMessage::decode(vk::MENU as usize, 0x2138_0001);
    assert_eq!(right.virtual_key, vk::MENU);
    assert_eq!(right.scan_code, SCAN_ALT);
    assert!(right.extended);
    let left = KeyMessage::decode(vk::MENU as usize, 0x2038_0001);
    assert!(!left.extended);
}

#[test]
fn a_key_up_decodes_alike_from_a_sign_extended_or_zero_extended_lparam() {
    // Bits 30 and 31 are set in every key-up; 64-bit hosts may pass either form.
    let flags = 0xC01E_0001_u32;
    for lparam in [flags as i32 as isize, flags as usize as isize] {
        let key = KeyMessage::decode(0x41, lparam);
        assert!(key.released, "{lparam:#x}");
        assert!(key.repeat, "{lparam:#x}");
        assert_eq!(key.scan_code, 0x1E);
        assert!(!key.extended);
        assert_eq!(key.virtual_key, 0x41);
    }
}

#[test]
fn the_virtual_key_is_the_low_word_of_wparam() {
    assert_eq!(
        KeyMessage::decode(0x0001_0041, 0x001E_0001).virtual_key,
        0x41
    );
}

#[test]
fn the_debug_output_of_a_key_message_hides_the_key() {
    let shown = format!("{:?}", KeyMessage::decode(0x41, 0x001E_0001));
    for hidden in ["41", "65", "1e", "1E", "30"] {
        assert!(!shown.contains(hidden), "{shown}");
    }
}

#[test]
fn the_modifier_keys_are_read_from_the_key_state() {
    let read = keys(
        &[vk::SHIFT, vk::CONTROL, vk::MENU, vk::LMENU, vk::RWIN],
        &[vk::CAPITAL],
    );
    assert_eq!(
        read,
        ModifierKeys {
            shift: true,
            ctrl: true,
            left_alt: true,
            right_alt: false,
            win: true,
            caps_lock: true,
        }
    );
    assert!(keys(&[vk::MENU, vk::RMENU], &[]).right_alt);
    assert!(!keys(&[vk::MENU, vk::RMENU], &[]).left_alt);
    assert!(keys(&[vk::LWIN], &[]).win);
    // A state that reports only the generic Alt code counts as left Alt.
    assert!(keys(&[vk::MENU], &[]).left_alt);
    // Caps Lock counts when it is toggled on, not while it is held.
    assert!(!keys(&[vk::CAPITAL], &[]).caps_lock);
    assert_eq!(keys(&[], &[]), ModifierKeys::default());
}

#[test]
fn right_alt_is_alt_on_a_layout_without_altgr() {
    let right = keys(&[vk::MENU, vk::RMENU], &[]);
    assert_eq!(
        right.modifiers(false),
        Modifiers {
            alt: true,
            ..Modifiers::default()
        }
    );
    assert_eq!(
        right.snapshot(false),
        KeyboardSnapshot {
            alt: true,
            ..KeyboardSnapshot::default()
        }
    );
}

#[test]
fn altgr_is_ctrl_and_alt_that_type_a_character_on_an_altgr_layout() {
    // Windows holds the synthetic left Ctrl down together with right Alt.
    let altgr = keys(&[vk::CONTROL, vk::MENU, vk::RMENU], &[]);
    assert_eq!(
        altgr.modifiers(true),
        Modifiers {
            ctrl: true,
            alt: true,
            alt_gr: true,
            ..Modifiers::default()
        },
        "matrix #22: not a shortcut"
    );
    assert_eq!(
        altgr.snapshot(true),
        KeyboardSnapshot {
            ctrl: true,
            alt_gr: true,
            ..KeyboardSnapshot::default()
        }
    );
}

#[test]
fn left_ctrl_and_left_alt_stay_a_shortcut_on_an_altgr_layout() {
    let chord = keys(&[vk::CONTROL, vk::MENU, vk::LMENU], &[]);
    assert_eq!(
        chord.modifiers(true),
        Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::default()
        }
    );
    assert_eq!(
        chord.snapshot(true),
        KeyboardSnapshot {
            ctrl: true,
            alt: true,
            ..KeyboardSnapshot::default()
        }
    );
}

#[test]
fn shift_caps_lock_and_the_windows_key_are_passed_on() {
    let held = keys(&[vk::SHIFT, vk::LWIN], &[vk::CAPITAL]);
    assert_eq!(
        held.modifiers(false),
        Modifiers {
            shift: true,
            caps_lock: true,
            win: true,
            ..Modifiers::default()
        }
    );
    assert_eq!(
        held.snapshot(false),
        KeyboardSnapshot {
            shift: true,
            caps_lock: true,
            win: true,
            ..KeyboardSnapshot::default()
        }
    );
}

#[test]
fn a_key_types_the_text_of_the_layout() {
    let typed = |units: &[u16]| {
        key_text(Translation {
            units: units.to_vec(),
            dead_key: false,
        })
    };
    assert_eq!(typed(&[0x61]), [0x61]);
    assert_eq!(typed(&[0xD83D, 0xDE00]), [0xD83D, 0xDE00]);
    assert!(typed(&[]).is_empty());
}

#[test]
fn a_dead_key_types_nothing_yet() {
    let dead = Translation {
        units: vec![0x00B4],
        dead_key: true,
    };
    assert!(key_text(dead).is_empty());
}

#[test]
fn a_key_that_types_more_than_an_event_holds_types_nothing() {
    let text = |len: usize| {
        key_text(Translation {
            units: vec![0x61; len],
            dead_key: false,
        })
    };
    assert_eq!(text(MAX_EVENT_TEXT_UNITS).len(), MAX_EVENT_TEXT_UNITS);
    assert!(text(MAX_EVENT_TEXT_UNITS + 1).is_empty());
}

#[test]
fn the_synthetic_left_ctrl_of_altgr_is_dropped_on_an_altgr_layout() {
    let mut filter = AltGrFilter::default();
    // AltGr+E: the synthetic left Ctrl goes down while right Alt is already physically down.
    assert!(filter.drops(&left_ctrl(true), true, true, || true));
    assert!(!filter.drops(&right_alt(true), true, true, || true));
    assert!(!filter.drops(&letter_e(true), true, true, || true));
    assert!(!filter.drops(&letter_e(false), false, true, || true));
    assert!(filter.drops(&left_ctrl(false), false, true, || false));
    assert!(!filter.drops(&right_alt(false), false, true, || false));
}

#[test]
fn the_test_and_the_call_of_one_transition_are_decided_alike() {
    let mut filter = AltGrFilter::default();
    // OnTestKeyDown, then OnKeyDown of the same transition.
    assert!(filter.drops(&left_ctrl(true), true, true, || true));
    assert!(filter.drops(&left_ctrl(true), true, true, || true));
    // OnTestKeyUp, then OnKeyUp.
    assert!(filter.drops(&left_ctrl(false), false, true, || false));
    assert!(filter.drops(&left_ctrl(false), false, true, || false));
    // The next real left Ctrl press is kept.
    assert!(!filter.drops(&left_ctrl(true), true, true, || false));
    assert!(!filter.drops(&left_ctrl(false), false, true, || false));
}

#[test]
fn a_real_left_ctrl_is_kept() {
    let mut filter = AltGrFilter::default();
    assert!(!filter.drops(&left_ctrl(true), true, true, || false));
    assert!(!filter.drops(&letter_e(true), true, true, || false));
    assert!(!filter.drops(&left_ctrl(false), false, true, || false));
}

#[test]
fn right_ctrl_is_never_the_synthetic_ctrl() {
    let mut filter = AltGrFilter::default();
    assert!(!filter.drops(&right_ctrl(true), true, true, || true));
    assert!(!filter.drops(&right_ctrl(false), false, true, || true));
}

#[test]
fn layouts_without_altgr_keep_every_ctrl_without_reading_the_key_state() {
    let mut filter = AltGrFilter::default();
    let unread = || -> bool { panic!("the physical state of right Alt was read") };
    assert!(!filter.drops(&left_ctrl(true), true, false, unread));
    assert!(!filter.drops(&right_alt(true), true, false, unread));
    assert!(!filter.drops(&left_ctrl(false), false, false, unread));
}

#[test]
fn the_synthetic_release_is_dropped_whichever_release_comes_first() {
    let mut filter = AltGrFilter::default();
    assert!(filter.drops(&left_ctrl(true), true, true, || true));
    assert!(!filter.drops(&right_alt(true), true, true, || true));
    assert!(!filter.drops(&right_alt(false), false, true, || false));
    assert!(filter.drops(&left_ctrl(false), false, true, || false));
}

#[test]
fn an_auto_repeating_altgr_drops_every_synthetic_ctrl() {
    let mut filter = AltGrFilter::default();
    for _ in 0..3 {
        assert!(filter.drops(&left_ctrl(true), true, true, || true));
        assert!(!filter.drops(&right_alt(true), true, true, || true));
    }
    assert!(filter.drops(&left_ctrl(false), false, true, || false));
    assert!(!filter.drops(&right_alt(false), false, true, || false));
    assert!(!filter.drops(&left_ctrl(true), true, true, || false));
}

#[test]
fn the_side_specific_left_ctrl_code_is_handled_too() {
    let mut filter = AltGrFilter::default();
    let left = |down| message(vk::LCONTROL, SCAN_CTRL, false, down);
    assert!(filter.drops(&left(true), true, true, || true));
    assert!(filter.drops(&left(false), false, true, || false));
}

#[test]
fn a_reset_forgets_a_synthetic_ctrl_that_was_down() {
    let mut filter = AltGrFilter::default();
    assert!(filter.drops(&left_ctrl(true), true, true, || true));
    filter.reset();
    assert!(!filter.drops(&left_ctrl(false), false, true, || false));
}

#[test]
fn the_debug_output_of_the_filter_hides_the_last_key() {
    let mut filter = AltGrFilter::default();
    filter.drops(&letter_e(true), true, true, || false);
    let shown = format!("{filter:?}");
    for hidden in ["69", "45", "18", "12"] {
        assert!(!shown.contains(hidden), "{shown}");
    }
}

#[test]
fn a_layout_is_described_once_until_it_changes() {
    let mut tracker = LayoutTracker::default();
    assert_eq!(tracker.current(), None);
    let us = tracker
        .observe(0x0409_0409, |_| ("00000409".to_owned(), false))
        .cloned();
    assert_eq!(
        us,
        Some(Layout {
            handle: 0x0409_0409,
            name: "00000409".to_owned(),
            altgr: false,
        })
    );
    assert_eq!(
        tracker.observe(0x0409_0409, |_| panic!("described again")),
        None,
        "an unchanged layout is no change"
    );
    let german = tracker
        .observe(0x0407_0407, |handle| {
            assert_eq!(handle, 0x0407_0407);
            ("00000407".to_owned(), true)
        })
        .cloned();
    assert_eq!(german.map(|layout| layout.altgr), Some(true));
    assert_eq!(
        tracker.current().map(|layout| layout.name.as_str()),
        Some("00000407")
    );
}
