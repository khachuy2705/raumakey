//! The hook classifies the hotkeys itself (D5, D24, D39): the toggle Ctrl+Shift fires on the
//! release of its last modifier and is reported to the observer; the hard reset swallows its
//! key. A mode chosen in the tray is applied, not reported back.

use raumakey_controller::typing::TypingUpdate;
use raumakey_model::input::vk;
use raumakey_windows::hooks::HookDecision;

use crate::support::{NOTEPAD, Sim};

#[test]
fn d5_ctrl_shift_toggles_the_mode_and_the_observer_is_told() {
    let mut sim = Sim::notepad();
    sim.tap_ctrl_shift();
    assert!(!sim.handler.core().vietnamese_mode());
    assert_eq!(sim.modes(), [false]);
    sim.type_text("aa ");
    assert_eq!(sim.text(NOTEPAD), "aa ");
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [false, true]);
    sim.type_text("aa");
    assert_eq!(sim.text(NOTEPAD), "aa â");
}

#[test]
fn d5_the_toggle_works_with_the_right_hand_keys_too() {
    let mut sim = Sim::notepad();
    for (key, down) in [
        (vk::RCONTROL, true),
        (vk::RSHIFT, true),
        (vk::RCONTROL, false),
        (vk::RSHIFT, false),
    ] {
        if down {
            sim.down(key);
        } else {
            sim.up(key);
        }
    }
    assert_eq!(sim.modes(), [false]);
}

#[test]
fn d24_the_toggle_ends_the_word_in_progress() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    sim.tap_ctrl_shift();
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [false, true]);
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "tiês");
}

#[test]
fn d24_a_key_typed_during_the_chord_cancels_the_toggle() {
    let mut sim = Sim::notepad();
    sim.chord(&[vk::LCONTROL, vk::LSHIFT], u32::from(b'T'));
    assert!(sim.modes().is_empty());
    assert!(sim.handler.core().vietnamese_mode());
}

#[test]
fn d24_a_click_while_ctrl_shift_is_held_cancels_the_toggle() {
    let mut sim = Sim::notepad();
    sim.down(vk::LCONTROL);
    sim.down(vk::LSHIFT);
    sim.click();
    sim.up(vk::LSHIFT);
    sim.up(vk::LCONTROL);
    assert!(sim.modes().is_empty());
}

#[test]
fn d5_the_hard_reset_swallows_its_key_and_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    let (down, up) = sim.chord(&[vk::LCONTROL, vk::LSHIFT, vk::LMENU], vk::F12);
    assert_eq!((down, up), (HookDecision::Swallow, HookDecision::Swallow));
    assert!(sim.modes().is_empty(), "the mode did not change");
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "tiês");
}

#[test]
fn ctrl_shift_toggles_after_win_l_whose_releases_the_hook_missed() {
    // Win+L: the hook sees Win and L go down; their releases happen on the lock screen.
    let mut sim = Sim::notepad();
    sim.down(vk::LWIN);
    sim.down(u32::from(b'L'));
    sim.lock();
    sim.up(u32::from(b'L'));
    sim.up(vk::LWIN);
    sim.wait(5000);
    sim.unlock();
    sim.tap_ctrl_shift();
    assert_eq!(
        sim.modes(),
        [false],
        "the first Ctrl+Shift after unlocking works"
    );
}

#[test]
fn ctrl_shift_toggles_after_ctrl_alt_del_whose_releases_the_hook_missed() {
    let mut sim = Sim::notepad();
    sim.down(vk::LCONTROL);
    sim.down(vk::LMENU);
    sim.down(vk::DELETE);
    sim.lock();
    sim.up(vk::DELETE);
    sim.up(vk::LMENU);
    sim.up(vk::LCONTROL);
    sim.unlock();
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [false]);
}

#[test]
fn ctrl_shift_f12_is_no_hard_reset_after_a_missed_alt_release() {
    let mut sim = Sim::notepad();
    sim.down(vk::LMENU);
    sim.lock();
    sim.up(vk::LMENU);
    sim.unlock();
    sim.type_text("tiee");
    let (down, up) = sim.chord(&[vk::LCONTROL, vk::LSHIFT], vk::F12);
    assert_eq!(
        (down, up),
        (HookDecision::PassThrough, HookDecision::PassThrough),
        "F12 is not swallowed"
    );
}

#[test]
fn a_mode_chosen_in_the_tray_is_applied_at_the_next_key_and_not_reported_back() {
    let mut sim = Sim::notepad();
    sim.mailbox
        .post(TypingUpdate::Mode(false))
        .expect("a mode is always accepted");
    assert!(
        sim.handler.core().vietnamese_mode(),
        "nothing changes before the hook's next event"
    );
    sim.type_text("aa");
    assert!(!sim.handler.core().vietnamese_mode());
    assert_eq!(sim.text(NOTEPAD), "aa");
    assert!(sim.modes().is_empty());
    // The hotkey toggles from the mode the tray chose.
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [true]);
}
