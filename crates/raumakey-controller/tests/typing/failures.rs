//! Sending fails, or the edit cannot be sent: the word ends, nothing is sent again, and the
//! key is never lost without what replaces it. The blind document turns an edit request into
//! keys within the limits of the input builders.

use raumakey_controller::typing::BlindDocument;
use raumakey_model::input::vk;
use raumakey_session::{DocumentPort, EditError, EditRequest};
use raumakey_windows::hooks::HookDecision;
use raumakey_windows::input::{MAX_BACKSPACES, MAX_TEXT_UNITS};

use crate::support::{NOTEPAD, Sim, backspace_keys, unicode_keys};

#[test]
fn a_send_that_injects_nothing_lets_the_key_through_and_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.platform().send_limit = Some(0);
    sim.type_text("tie");
    assert_eq!(
        sim.type_char('e'),
        HookDecision::PassThrough,
        "the key is not lost"
    );
    assert_eq!(sim.text(NOTEPAD), "tiee");
    let attempts = sim.platform().sends.len();
    assert_eq!(attempts, 1);
    // The word ended: nothing is edited (or sent again) for the rest of it.
    sim.type_char('s');
    assert_eq!(sim.platform().sends.len(), attempts);
    assert_eq!(sim.text(NOTEPAD), "tiees");
    // Sending works again for the next word.
    sim.platform().send_limit = None;
    sim.type_text(" aa");
    assert_eq!(sim.text(NOTEPAD), "tiees â");
}

#[test]
fn a_partial_send_blocks_the_key_and_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tie");
    sim.platform().send_limit = Some(1);
    // Only the first Backspace reached the application: the key, which would arrive ahead of
    // it, is blocked, and the text is unknown from then on.
    assert_eq!(sim.type_char('e'), HookDecision::Swallow);
    assert_eq!(sim.text(NOTEPAD), "ti");
    sim.platform().send_limit = None;
    let attempts = sim.platform().sends.len();
    sim.type_char('s');
    assert_eq!(
        sim.platform().sends.len(),
        attempts,
        "never retried blindly"
    );
    assert_eq!(sim.text(NOTEPAD), "tis");
}

#[test]
fn a_failed_backspace_restore_lets_backspace_through() {
    let mut sim = Sim::notepad();
    sim.type_text("aa");
    sim.platform().send_limit = Some(0);
    assert_eq!(sim.press(vk::BACK), HookDecision::PassThrough);
    assert_eq!(sim.text(NOTEPAD), "");
}

#[test]
fn an_edit_becomes_one_backspace_per_character_then_the_text() {
    let mut document = BlindDocument::new();
    assert!(!document.edited());
    let expected: Vec<u16> = "a😀".encode_utf16().collect();
    let insert: Vec<u16> = "â".encode_utf16().collect();
    document
        .replace_before_caret(&EditRequest::new(&expected, &insert))
        .expect("fits");
    assert!(document.edited());
    let [down, up] = backspace_keys();
    let [hat_down, hat_up] = unicode_keys(0xE2);
    assert_eq!(
        document.into_keys(),
        [down, up, down, up, hat_down, hat_up],
        "a surrogate pair is one character for Backspace"
    );
}

#[test]
fn one_key_edits_at_most_once() {
    let mut document = BlindDocument::new();
    let units = [0x61];
    document
        .replace_before_caret(&EditRequest::new(&units, &units))
        .expect("fits");
    assert_eq!(
        document.replace_before_caret(&EditRequest::new(&units, &units)),
        Err(EditError::Denied)
    );
    assert_eq!(document.into_keys().len(), 4);
}

#[test]
fn an_edit_beyond_the_input_builders_is_refused_and_sends_nothing() {
    let long_delete = vec![0x61; usize::try_from(MAX_BACKSPACES).expect("small") + 1];
    let mut document = BlindDocument::new();
    assert_eq!(
        document.replace_before_caret(&EditRequest::new(&long_delete, &[0x61])),
        Err(EditError::Denied)
    );
    let long_insert = vec![0x61; MAX_TEXT_UNITS + 1];
    assert_eq!(
        document.replace_before_caret(&EditRequest::new(&[0x61], &long_insert)),
        Err(EditError::Denied)
    );
    assert!(!document.edited());
    assert!(document.into_keys().is_empty());
}
