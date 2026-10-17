//! What ends a word in the hook (design §21.2): a mouse press, another foreground or focus
//! window, and a Ctrl, Alt, or Win chord. After that, nothing edits the text typed before.

use raumakey_model::input::vk;
use raumakey_windows::hooks::HookDecision;

use crate::support::{NOTEPAD, Sim, WORDPAD, notepad, wordpad};

#[test]
fn matrix_19_a_mouse_press_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    assert_eq!(sim.text(NOTEPAD), "tiê");
    sim.click();
    let sends = sim.platform().sends.len();
    assert_eq!(sim.type_char('s'), HookDecision::PassThrough);
    assert_eq!(
        sim.platform().sends.len(),
        sends,
        "nothing is edited after the click"
    );
    assert_eq!(sim.text(NOTEPAD), "tiês");
}

#[test]
fn matrix_20_another_foreground_window_ends_the_word_and_is_never_edited() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    sim.focus(Some(wordpad()));
    let sends = sim.platform().sends.len();
    // In Notepad, `s` would rewrite `tiê`; WordPad gets the key and nothing else.
    assert_eq!(sim.type_char('s'), HookDecision::PassThrough);
    assert_eq!(sim.platform().sends.len(), sends);
    assert_eq!(sim.text(WORDPAD), "s");
    assert_eq!(sim.text(NOTEPAD), "tiê");
    // Back in Notepad the word has ended too.
    sim.focus(Some(notepad()));
    assert_eq!(sim.type_char('s'), HookDecision::PassThrough);
    assert_eq!(sim.text(NOTEPAD), "tiês");
    // A new word types as usual.
    sim.type_text(" aa");
    assert_eq!(sim.text(NOTEPAD), "tiês â");
}

#[test]
fn matrix_20_a_word_never_continues_in_another_window_with_the_same_process() {
    let mut sim = Sim::notepad();
    sim.type_text("aa");
    let mut second = notepad();
    second.window = NOTEPAD + 0x100;
    second.focus = second.window + 1;
    sim.focus(Some(second));
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD + 0x100), "s");
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn another_focus_window_inside_the_foreground_window_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    let mut moved = notepad();
    moved.focus = NOTEPAD + 7;
    sim.focus(Some(moved));
    let sends = sim.platform().sends.len();
    sim.type_char('s');
    assert_eq!(sim.platform().sends.len(), sends);
    assert_eq!(sim.text(NOTEPAD), "tiês");
}

#[test]
fn matrix_21_a_ctrl_chord_passes_and_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    let (down, up) = sim.chord(&[vk::LCONTROL], u32::from(b'C'));
    assert_eq!(
        (down, up),
        (HookDecision::PassThrough, HookDecision::PassThrough)
    );
    let sends = sim.platform().sends.len();
    sim.type_char('s');
    assert_eq!(sim.platform().sends.len(), sends);
    assert_eq!(sim.text(NOTEPAD), "tiês");
}

#[test]
fn alt_and_win_chords_pass_and_end_the_word() {
    for modifier in [vk::LMENU, vk::LWIN] {
        let mut sim = Sim::notepad();
        sim.type_text("tiee");
        let (down, _) = sim.chord(&[modifier], vk::TAB);
        assert_eq!(down, HookDecision::PassThrough, "{modifier:#x}");
        sim.type_char('s');
        assert_eq!(sim.text(NOTEPAD), "tiês", "{modifier:#x}");
    }
}

#[test]
fn a_lone_alt_tap_ends_the_word() {
    // Alt pressed and released alone puts the window's menu bar into menu mode.
    for alt in [vk::LMENU, vk::RMENU] {
        let mut sim = Sim::notepad();
        sim.type_text("tiee");
        assert_eq!(sim.press(alt), HookDecision::PassThrough);
        sim.type_char('s');
        assert_eq!(sim.text(NOTEPAD), "tiês", "{alt:#x}");
    }
}

#[test]
fn f10_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    assert_eq!(sim.press(0x79), HookDecision::PassThrough);
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "tiês");
}

#[test]
fn alt_held_with_another_key_is_no_lone_tap() {
    // Alt+Shift switches the input language; it opens no menu.
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    sim.down(vk::LMENU);
    sim.press(vk::LSHIFT);
    sim.up(vk::LMENU);
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "tiế");
}

#[test]
fn an_altgr_tap_is_no_lone_alt_on_an_altgr_layout() {
    let mut sim = Sim::new(crate::support::FakePlatform {
        altgr_layout: true,
        ..crate::support::FakePlatform::notepad()
    });
    sim.type_text("tiee");
    let fake = sim.physical(vk::LCONTROL, crate::support::FAKE_LCTRL_SCAN, true);
    sim.event(fake);
    sim.press(vk::RMENU);
    let fake = sim.physical(vk::LCONTROL, crate::support::FAKE_LCTRL_SCAN, false);
    sim.event(fake);
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "tiế");
}

#[test]
fn a_navigation_key_passes_and_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    assert_eq!(sim.press(vk::LEFT), HookDecision::PassThrough);
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "tiês");
}
