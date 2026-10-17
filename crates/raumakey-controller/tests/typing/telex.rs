//! Words typed blind: every edit is Backspaces, then the inserted text as Unicode keys, all
//! tagged, sent from the hook and arriving after the key they replace, which is blocked.

use raumakey_model::input::vk;
use raumakey_windows::hooks::HookDecision;

use crate::support::{NOTEPAD, Sim, backspace_keys, unicode_keys};

#[test]
fn d39_telex_words_are_typed_with_backspaces_and_unicode_keys() {
    let mut sim = Sim::notepad();
    sim.type_text("tieengs vieetj ");
    assert_eq!(sim.text(NOTEPAD), "tiếng việt ");
}

#[test]
fn an_edit_is_its_backspaces_then_its_text_and_blocks_the_key_it_replaces() {
    let mut sim = Sim::notepad();
    assert_eq!(sim.type_char('a'), HookDecision::PassThrough);
    assert!(
        sim.sent().is_empty(),
        "a letter that extends the word is not edited"
    );
    assert_eq!(sim.type_char('a'), HookDecision::Swallow);
    let [back_down, back_up] = backspace_keys();
    let [hat_down, hat_up] = unicode_keys(0xE2);
    assert_eq!(
        sim.platform().sends,
        [vec![back_down, back_up, hat_down, hat_up]]
    );
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn every_key_of_a_word_passes_or_is_replaced_never_both() {
    let mut sim = Sim::notepad();
    let decisions = sim.type_text("tieengs");
    assert_eq!(
        decisions,
        [
            HookDecision::PassThrough,
            HookDecision::PassThrough,
            HookDecision::PassThrough,
            HookDecision::Swallow,
            HookDecision::PassThrough,
            HookDecision::PassThrough,
            HookDecision::Swallow,
        ]
    );
    assert_eq!(sim.platform().sends.len(), 2, "one send per replaced key");
    assert_eq!(sim.text(NOTEPAD), "tiếng");
}

#[test]
fn matrix_17_backspace_restores_the_state_before_the_last_key() {
    let mut sim = Sim::notepad();
    sim.type_text("aa");
    assert_eq!(sim.text(NOTEPAD), "â");
    assert_eq!(sim.press(vk::BACK), HookDecision::Swallow);
    assert_eq!(sim.text(NOTEPAD), "a");
    // Typing on continues the restored word.
    sim.type_char('a');
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn backspace_steps_back_through_the_tones_of_a_word() {
    let mut sim = Sim::notepad();
    sim.type_text("vieetj");
    assert_eq!(sim.text(NOTEPAD), "việt");
    sim.press(vk::BACK);
    assert_eq!(sim.text(NOTEPAD), "viêt");
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "viết");
}

#[test]
fn keys_typed_fast_overlap_and_still_make_the_word() {
    // Fast typing presses the next key before the last one is up.
    let mut sim = Sim::notepad();
    for (key, down) in [
        (u32::from(b'T'), true),
        (u32::from(b'I'), true),
        (u32::from(b'T'), false),
        (u32::from(b'E'), true),
        (u32::from(b'I'), false),
        (u32::from(b'E'), false),
        (u32::from(b'E'), true),
        (u32::from(b'S'), true),
        (u32::from(b'E'), false),
        (u32::from(b'S'), false),
    ] {
        if down {
            sim.down(key);
        } else {
            sim.up(key);
        }
    }
    assert_eq!(sim.text(NOTEPAD), "tiế");
}

#[test]
fn shift_types_capitals_and_the_word_keeps_its_case() {
    let mut sim = Sim::notepad();
    sim.type_text("Tieengs Vieetj");
    assert_eq!(sim.text(NOTEPAD), "Tiếng Việt");
}

#[test]
fn a_macro_free_sentence_with_punctuation_is_typed() {
    let mut sim = Sim::notepad();
    sim.type_text("xin chaof. ");
    assert_eq!(sim.text(NOTEPAD), "xin chào. ");
}
