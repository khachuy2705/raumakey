//! The key tracker: modifiers, keys held down (the repeat flag that a low-level hook does not
//! have), the Caps Lock toggle, releases the hook missed, the synthetic left Ctrl of AltGr,
//! and dead keys. The thread key state of a hook thread is stale, so the text of a key comes
//! from the tracker's state.

use raumakey_controller::typing::{KeyTracker, Transition};
use raumakey_model::input::vk;
use raumakey_windows::hooks::KeyboardEvent;

use crate::support::{FAKE_LCTRL_SCAN, FakePlatform, NOTEPAD, Sim, VK_QUOTE, scan_of};

fn event(virtual_key: u32, down: bool) -> KeyboardEvent {
    KeyboardEvent {
        vk: virtual_key,
        scan: scan_of(virtual_key),
        extended: false,
        injected: false,
        injected_lower_il: false,
        alt_down: false,
        key_up: !down,
        time: 0,
        extra_info: 0,
    }
}

#[test]
fn a_key_down_while_the_key_is_down_is_a_repeat() {
    let mut tracker = KeyTracker::new(false);
    let a = u32::from(b'A');
    assert_eq!(
        tracker.observe(&event(a, true)),
        Transition::Down { repeat: false }
    );
    assert!(tracker.is_down(a));
    assert_eq!(
        tracker.observe(&event(a, true)),
        Transition::Down { repeat: true }
    );
    assert_eq!(
        tracker.observe(&event(a, true)),
        Transition::Down { repeat: true }
    );
    assert_eq!(tracker.observe(&event(a, false)), Transition::Up);
    assert!(!tracker.is_down(a));
    assert_eq!(
        tracker.observe(&event(a, true)),
        Transition::Down { repeat: false }
    );
    // Another key in between does not make a held key's next transition new.
    tracker.observe(&event(u32::from(b'B'), true));
    assert_eq!(
        tracker.observe(&event(a, true)),
        Transition::Down { repeat: true }
    );
}

#[test]
fn modifiers_follow_both_sides_of_each_key() {
    let mut tracker = KeyTracker::new(false);
    for key in [vk::RSHIFT, vk::LCONTROL, vk::LMENU, vk::RWIN] {
        tracker.observe(&event(key, true));
    }
    let modifiers = tracker.modifiers(false);
    assert!(modifiers.shift && modifiers.ctrl && modifiers.alt && modifiers.win);
    assert!(!modifiers.alt_gr && !modifiers.caps_lock);
    for key in [vk::RSHIFT, vk::LCONTROL, vk::LMENU, vk::RWIN] {
        tracker.observe(&event(key, false));
    }
    assert_eq!(tracker.modifiers(false), Default::default());
}

#[test]
fn right_alt_is_altgr_only_on_an_altgr_layout() {
    let mut tracker = KeyTracker::new(false);
    tracker.observe(&event(vk::RMENU, true));
    let altgr = tracker.modifiers(true);
    assert!(altgr.alt_gr && altgr.alt);
    let snapshot = tracker.snapshot(true);
    assert!(snapshot.alt_gr && !snapshot.alt);
    let plain = tracker.modifiers(false);
    assert!(plain.alt && !plain.alt_gr);
    assert!(tracker.snapshot(false).alt);
}

#[test]
fn caps_lock_flips_once_per_press_from_its_seed() {
    let mut tracker = KeyTracker::new(true);
    assert!(tracker.caps_lock());
    tracker.observe(&event(vk::CAPITAL, true));
    assert!(!tracker.caps_lock());
    tracker.observe(&event(vk::CAPITAL, true));
    assert!(!tracker.caps_lock(), "an auto-repeat does not toggle");
    tracker.observe(&event(vk::CAPITAL, false));
    assert!(!tracker.caps_lock());
    tracker.observe(&event(vk::CAPITAL, true));
    assert!(tracker.caps_lock());
    assert!(tracker.modifiers(false).caps_lock);
    assert!(tracker.snapshot(false).caps_lock);
}

#[test]
fn the_synthetic_left_ctrl_of_altgr_is_not_tracked() {
    let mut tracker = KeyTracker::new(false);
    let mut fake = event(vk::LCONTROL, true);
    fake.scan = FAKE_LCTRL_SCAN;
    assert_eq!(tracker.observe(&fake), Transition::SyntheticCtrl);
    tracker.observe(&event(vk::RMENU, true));
    assert!(!tracker.is_down(vk::LCONTROL));
    assert!(!tracker.modifiers(true).ctrl);
    fake.key_up = true;
    assert_eq!(tracker.observe(&fake), Transition::SyntheticCtrl);
    // A real left Ctrl is tracked.
    assert_eq!(
        tracker.observe(&event(vk::LCONTROL, true)),
        Transition::Down { repeat: false }
    );
    assert!(tracker.modifiers(true).ctrl);
}

#[test]
fn keys_whose_release_was_missed_are_forgotten() {
    let mut tracker = KeyTracker::new(false);
    for key in [vk::LCONTROL, vk::LSHIFT, u32::from(b'L')] {
        tracker.observe(&event(key, true));
    }
    let mut asked = Vec::new();
    let forgot = tracker.forget_missed_releases(u32::from(b'L'), |key| {
        asked.push(key);
        key == vk::LSHIFT
    });
    assert!(forgot);
    assert!(
        !asked.contains(&u32::from(b'L')),
        "the key being handled is not asked"
    );
    assert!(!tracker.is_down(vk::LCONTROL));
    assert!(tracker.is_down(vk::LSHIFT));
    assert!(tracker.is_down(u32::from(b'L')));
    assert!(!tracker.forget_missed_releases(0, |_| true));
}

#[test]
fn caps_lock_on_at_the_start_types_capitals() {
    let mut sim = Sim::new(FakePlatform {
        caps_lock: true,
        ..FakePlatform::notepad()
    });
    sim.type_text("tieengs vieetj");
    assert_eq!(sim.text(NOTEPAD), "TIẾNG VIỆT");
}

#[test]
fn the_caps_lock_key_switches_capitals_on_and_off() {
    let mut sim = Sim::notepad();
    sim.down(vk::CAPITAL);
    sim.down(vk::CAPITAL);
    sim.up(vk::CAPITAL);
    sim.type_text("vieetj ");
    sim.press(vk::CAPITAL);
    sim.type_text("vieetj");
    assert_eq!(sim.text(NOTEPAD), "VIỆT việt");
}

#[test]
fn shift_with_caps_lock_types_lowercase_edits() {
    // The application types the keys that pass with its own state; the edit's text comes
    // from the hook's view, which must agree.
    let mut sim = Sim::new(FakePlatform {
        caps_lock: true,
        ..FakePlatform::notepad()
    });
    sim.down(vk::LSHIFT);
    sim.press(u32::from(b'A'));
    sim.press(u32::from(b'A'));
    sim.up(vk::LSHIFT);
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn a_ctrl_whose_release_was_missed_does_not_turn_letters_into_shortcuts() {
    // Ctrl+Alt+Del: the secure desktop takes the input, and the hook never sees the releases.
    let mut sim = Sim::notepad();
    sim.down(vk::LCONTROL);
    sim.down(vk::LMENU);
    sim.lose_release(vk::LMENU);
    sim.lose_release(vk::LCONTROL);
    sim.type_text("aa");
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn caps_lock_is_read_again_from_the_desktop_after_missed_releases() {
    // Win+L, then Caps Lock on the lock screen: the hook sees neither.
    let mut sim = Sim::notepad();
    sim.down(vk::LWIN);
    sim.down(u32::from(b'L'));
    sim.lose_release(u32::from(b'L'));
    sim.lose_release(vk::LWIN);
    sim.platform().caps_lock = true;
    sim.type_text("aa");
    assert_eq!(sim.text(NOTEPAD), "Â");
}

#[test]
fn the_key_after_a_dead_key_ends_the_word() {
    // The application composes the dead key with the next key, which the hook cannot see.
    let mut sim = Sim::new(FakePlatform {
        dead_quote: true,
        ..FakePlatform::notepad()
    });
    sim.press(VK_QUOTE);
    sim.type_text("aa");
    assert_eq!(
        sim.text(NOTEPAD),
        "áa",
        "the engine never edited the composed letter"
    );
    sim.type_text(" aa");
    assert_eq!(sim.text(NOTEPAD), "áa â");
}

#[test]
fn matrix_22_altgr_characters_are_typed_and_are_no_shortcut() {
    // Windows sends a synthetic left Ctrl before right Alt on an AltGr layout.
    let mut sim = Sim::new(FakePlatform {
        altgr_layout: true,
        ..FakePlatform::notepad()
    });
    for key in [0x45, 0x51] {
        let fake = sim.physical(vk::LCONTROL, FAKE_LCTRL_SCAN, true);
        sim.event(fake);
        sim.down(vk::RMENU);
        assert!(!sim.handler.tracker().modifiers(true).ctrl);
        sim.press(key);
        sim.up(vk::RMENU);
        let fake = sim.physical(vk::LCONTROL, FAKE_LCTRL_SCAN, false);
        sim.event(fake);
    }
    assert_eq!(sim.text(NOTEPAD), "€@");
    assert!(sim.platform().sends.is_empty());
    assert!(!sim.handler.tracker().is_down(vk::LCONTROL));
    assert!(sim.modes().is_empty(), "AltGr is no Ctrl+Alt hotkey");
    sim.type_text(" aa");
    assert_eq!(sim.text(NOTEPAD), "€@ â");
}
