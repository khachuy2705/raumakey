//! The key tracker: modifiers, keys held down (the repeat flag that a low-level hook does not
//! have), the Caps Lock toggle, releases the hook missed, the synthetic left Ctrl of AltGr,
//! and dead keys. The thread key state of a hook thread is stale, so the text of a key comes
//! from the tracker's state.

use raumakey_controller::typing::{KeyTracker, Transition};
use raumakey_model::input::vk;
use raumakey_windows::hooks::KeyboardEvent;

use raumakey_windows::hooks::HookDecision;

use crate::support::{
    FAKE_LCTRL_SCAN, FakePlatform, NOTEPAD, Sim, VK_QUOTE, WORDPAD, admin_prompt, notepad, scan_of,
    wordpad,
};

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
fn a_withheld_key_is_no_missed_release_but_goes_with_one() {
    let mut tracker = KeyTracker::new(false);
    let a = u32::from(b'A');
    tracker.observe(&event(a, true));
    tracker.settle(a, false, false);
    assert!(tracker.is_withheld(a));
    assert!(
        !tracker.forget_missed_releases(0, |_| false),
        "the physical state never saw it down"
    );
    assert!(tracker.is_down(a));
    tracker.observe(&event(vk::LWIN, true));
    assert!(
        tracker.forget_missed_releases(0, |_| false),
        "Win was missed"
    );
    assert!(
        !tracker.is_down(a),
        "a withheld key goes with a missed release: its own may be lost too"
    );
}

#[test]
fn a_repeat_that_reaches_the_system_ends_the_withholding() {
    let mut tracker = KeyTracker::new(false);
    let a = u32::from(b'A');
    tracker.observe(&event(a, true));
    tracker.settle(a, false, false);
    tracker.observe(&event(a, true));
    tracker.settle(a, true, true);
    assert!(!tracker.is_withheld(a));
    tracker.observe(&event(a, true));
    tracker.settle(a, true, false);
    assert!(
        !tracker.is_withheld(a),
        "the system saw a key-down of this press"
    );
    tracker.observe(&event(a, false));
    assert!(!tracker.is_withheld(a) && !tracker.is_down(a));
}

#[test]
fn modifiers_are_synced_with_the_physical_state_both_ways() {
    let mut tracker = KeyTracker::new(false);
    tracker.observe(&event(vk::LCONTROL, true));
    tracker.observe(&event(vk::SHIFT, true));
    let physical = [vk::RSHIFT, vk::LWIN];
    assert!(
        tracker.sync_modifiers(false, |key| physical.contains(&key)),
        "left Ctrl turned out released"
    );
    let modifiers = tracker.modifiers(false);
    assert!(modifiers.shift && modifiers.win && !modifiers.ctrl);
    assert!(!tracker.is_down(vk::SHIFT), "generic codes are dropped");
    assert!(!tracker.sync_modifiers(false, |key| physical.contains(&key)));
}

#[test]
fn the_synthetic_left_ctrl_in_the_physical_state_is_ignored_under_altgr() {
    let mut tracker = KeyTracker::new(false);
    let physical = [vk::LCONTROL, vk::RMENU];
    assert!(!tracker.sync_modifiers(true, |key| physical.contains(&key)));
    let modifiers = tracker.modifiers(true);
    assert!(modifiers.alt_gr && !modifiers.ctrl);
    // Without AltGr, both are real.
    let mut tracker = KeyTracker::new(false);
    tracker.sync_modifiers(false, |key| physical.contains(&key));
    let modifiers = tracker.modifiers(false);
    assert!(modifiers.ctrl && modifiers.alt);
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
fn caps_lock_toggled_in_an_elevated_window_is_read_again_at_the_next_document() {
    // The hook is not called for the elevated window, so it never sees Caps Lock there.
    let mut sim = Sim::notepad();
    sim.type_text("xin ");
    sim.focus(Some(admin_prompt()));
    sim.press(vk::CAPITAL);
    sim.focus(Some(wordpad()));
    sim.type_text("tieengs");
    assert_eq!(
        sim.text(WORDPAD),
        "TIẾNG",
        "not `TIếng`: the edits take the case the application types"
    );
}

#[test]
fn caps_lock_toggled_in_an_elevated_window_is_read_again_after_a_second_away() {
    // Back in the same window, the hook sees no other document; the time away tells.
    let mut sim = Sim::notepad();
    sim.type_text("xin ");
    sim.focus(Some(admin_prompt()));
    sim.press(vk::CAPITAL);
    sim.type_text("x");
    sim.wait(1500);
    sim.focus(Some(notepad()));
    sim.type_text("tieengs");
    assert_eq!(sim.text(NOTEPAD), "xin TIẾNG", "not `xin TIếng`");
}

#[test]
fn caps_lock_toggled_on_the_lock_screen_is_read_again_after_an_idle_gap() {
    let mut sim = Sim::notepad();
    sim.type_text("xin ");
    sim.lock();
    sim.press(vk::CAPITAL);
    sim.wait(3000);
    sim.unlock();
    sim.type_text("tieengs");
    assert_eq!(sim.text(NOTEPAD), "xin TIẾNG");
}

#[test]
fn caps_lock_is_not_read_again_while_keys_come_quickly_in_one_window() {
    let mut sim = Sim::notepad();
    sim.type_char('x');
    let reads = sim.platform().caps_reads;
    sim.type_text("in tieengs vieetj");
    sim.press(vk::CAPITAL);
    sim.type_text(" aa");
    assert_eq!(sim.platform().caps_reads, reads);
    assert_eq!(sim.text(NOTEPAD), "xin tiếng việt Â");
}

#[test]
fn a_gap_of_a_second_reads_caps_lock_again_and_a_shorter_one_does_not() {
    let mut sim = Sim::notepad();
    sim.type_char('x');
    let reads = sim.platform().caps_reads;
    sim.wait(900);
    sim.type_char('x');
    assert_eq!(sim.platform().caps_reads, reads, "0.9 s is no idle gap");
    sim.wait(1000);
    sim.type_char('x');
    assert_eq!(sim.platform().caps_reads, reads + 1);
}

#[test]
fn a_letter_the_hook_replaced_and_that_is_still_held_is_no_missed_release() {
    // Rollover: the second `a` is replaced (blocked) and still held when `s` goes down. The
    // system never saw it down, so the physical state says it is up.
    let mut sim = Sim::notepad();
    sim.type_char('a');
    let a = u32::from(b'A');
    assert_eq!(sim.down(a), HookDecision::Swallow);
    let reads = sim.platform().caps_reads;
    sim.down(u32::from(b'S'));
    assert!(sim.handler.tracker().is_down(a), "still held");
    assert_eq!(
        sim.platform().caps_reads,
        reads,
        "nothing was taken for a missed release"
    );
    sim.up(a);
    sim.up(u32::from(b'S'));
    assert!(!sim.handler.tracker().is_down(a));
    assert_eq!(sim.text(NOTEPAD), "ấ");
}

#[test]
fn a_ctrl_held_where_the_hook_did_not_see_it_turns_letters_into_shortcuts() {
    // Ctrl went down in an elevated window; the user switched back while holding it.
    let mut sim = Sim::notepad();
    sim.unseen(vk::LCONTROL, true);
    sim.type_text("aa");
    assert!(
        sim.platform().sends.is_empty(),
        "nothing is edited, so no Backspace becomes Ctrl+Backspace"
    );
    sim.up(vk::LCONTROL);
    sim.type_text("aa");
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn a_shift_held_where_the_hook_did_not_see_it_still_gives_capital_edits() {
    let mut sim = Sim::notepad();
    sim.unseen(vk::LSHIFT, true);
    sim.press(u32::from(b'A'));
    sim.press(u32::from(b'A'));
    sim.unseen(vk::LSHIFT, false);
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
        sim.down(key);
        assert!(
            !sim.handler.tracker().modifiers(true).ctrl,
            "the synthetic left Ctrl that the physical state shows is not Ctrl"
        );
        sim.up(key);
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
