//! Typing through the core: one edit per transformation, pass-through letters, boundaries,
//! and semantic backspace (matrix #1, #3, #15, #17, #40).

use raumakey_model::input::vk;
use raumakey_tip::session::ServiceCore;

use crate::support::{FakeDocument, edit};

#[test]
fn matrix_1_telex_a_a_gives_a_circumflex_in_one_edit() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);

    let first = doc.type_char(&mut core, 'a');
    assert!(first.claimed, "the first letter commits engine state");
    assert!(
        first.reached_app(),
        "the application types the first letter itself"
    );
    assert!(doc.edits.is_empty());
    assert_eq!(doc.text(), "a");

    let second = doc.type_char(&mut core, 'a');
    assert!(second.claimed);
    assert!(second.outcome.eaten);
    assert_eq!(second.outcome.replay, None);
    assert_eq!(doc.edits, [edit(1, "â")]);
    assert_eq!(doc.attempts, 1);
    assert_eq!(doc.text(), "â");
    assert_eq!(
        doc.caret(),
        1,
        "the caret collapses to the end of the insert"
    );
}

#[test]
fn matrix_3_telex_a_s_gives_a_acute() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "as");
    assert_eq!(doc.edits, [edit(1, "á")]);
    assert_eq!(doc.text(), "á");
}

#[test]
fn a_whole_word_is_typed_with_one_edit_per_transformation() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "tieengs vieetj");
    assert_eq!(doc.text(), "tiếng việt");
    assert_eq!(
        doc.edits,
        [edit(1, "ê"), edit(3, "ếng"), edit(1, "ê"), edit(2, "ệt")]
    );
}

#[test]
fn matrix_15_an_invalid_converted_word_is_restored_at_the_boundary() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let presses = doc.type_text(&mut core, "after ");
    assert_eq!(doc.text(), "after ");
    assert_eq!(doc.edits, [edit(1, "à"), edit(4, "after ")]);
    let space = presses.last().expect("space");
    assert!(space.outcome.eaten, "the space is part of the replacement");
    // The boundary reset the word: the next letter starts a new one.
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "after s");
    assert_eq!(doc.edits.len(), 2);
}

#[test]
fn a_space_after_a_valid_word_reaches_the_application() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "as");
    let space = doc.type_char(&mut core, ' ');
    assert!(space.claimed, "the boundary commits the reset");
    assert!(space.reached_app());
    assert_eq!(doc.text(), "á ");
    // A space typed while no word is in progress needs nothing from the core.
    let again = doc.type_char(&mut core, ' ');
    assert!(!again.claimed);
    assert_eq!(doc.text(), "á  ");
}

#[test]
fn matrix_17_semantic_backspace_restores_the_previous_state() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "aa");
    let backspace = doc.type_key(&mut core, vk::BACK);
    assert!(backspace.outcome.eaten);
    assert_eq!(doc.text(), "a");
    assert_eq!(doc.edits, [edit(1, "â"), edit(1, "a")]);

    // The next backspace removes the last owned letter; then the engine is idle and the
    // application handles backspace itself.
    doc.type_key(&mut core, vk::BACK);
    assert_eq!(doc.text(), "");
    assert_eq!(doc.edits.last(), Some(&edit(1, "")));
    let idle = doc.type_key(&mut core, vk::BACK);
    assert!(!idle.claimed);
    assert_eq!(doc.edits.len(), 3);
}

#[test]
fn matrix_40_two_units_are_replaced_by_one_in_a_single_edit() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "eess");
    assert_eq!(doc.text(), "ês");
    let attempts = doc.attempts;

    let backspace = doc.type_key(&mut core, vk::BACK);
    assert!(backspace.outcome.eaten);
    assert_eq!(doc.attempts, attempts + 1, "one edit session");
    assert_eq!(doc.edits.last(), Some(&edit(2, "ế")));
    assert_eq!(doc.text(), "ế");
    assert_eq!(doc.caret(), 1);
}

#[test]
fn a_word_over_the_context_limit_passes_through_until_the_next_boundary() {
    // Plan §3.9: past `context_limit_codepoints` (32 by default) keys the word is no longer
    // owned and further letters pass through until a boundary. The application reports each
    // of those letters arriving; that must not restart the word in the middle of the token.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let long = "x".repeat(33);
    doc.type_text(&mut core, &long);
    doc.type_text(&mut core, "aa");
    assert_eq!(doc.text(), format!("{long}aa"));
    assert!(doc.edits.is_empty());
    doc.type_text(&mut core, " aa");
    assert_eq!(
        doc.text(),
        format!("{long}aa â"),
        "the boundary ends the long token"
    );
}

#[test]
fn a_backspace_inside_an_overlong_word_keeps_skipping_it() {
    // Plan §3.9: only a boundary ends the skipped word. The application deleting a letter is
    // the arrival of the Backspace the core let through, not a change by the user.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let long = "x".repeat(33);
    doc.type_text(&mut core, &long);
    let backspace = doc.type_key(&mut core, vk::BACK);
    assert!(backspace.reached_app());
    doc.type_text(&mut core, "aa");
    assert_eq!(doc.text(), format!("{}aa", "x".repeat(32)));
    assert!(doc.edits.is_empty());
}

#[test]
fn navigation_ends_the_word_and_reaches_the_application() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "a");
    let left = doc.type_key(&mut core, vk::LEFT);
    assert!(
        left.claimed,
        "the reset of the word is committed in key_down"
    );
    assert!(left.reached_app());
    doc.type_key(&mut core, vk::END);
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "aa", "the word ended at the arrow key");
    assert!(doc.edits.is_empty());

    // A navigation key while no word is in progress is a pure pass-through.
    let mut idle = FakeDocument::new(2);
    let right = idle.type_key(&mut core, vk::RIGHT);
    assert!(!right.claimed);
}

#[test]
fn modifier_keys_do_not_end_the_word() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let shift = doc.type_key(&mut core, vk::SHIFT);
    assert!(!shift.claimed);
    // Shift+A is Telex `a` on the lowercase letter already typed.
    doc.type_char(&mut core, 'A');
    assert_eq!(doc.text(), "â");
    assert_eq!(doc.edits, [edit(1, "â")]);
}

#[test]
fn lock_keys_do_not_end_the_word() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    // Caps Lock, Num Lock, Scroll Lock.
    for lock in [vk::CAPITAL, 0x90, 0x91] {
        let press = doc.type_key(&mut core, lock);
        assert!(!press.claimed, "lock key {lock:#x}");
    }
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "â");
}

#[test]
fn ctrl_shortcuts_end_the_word_without_being_eaten() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let mut copy = doc.key(u32::from(b'C'));
    copy.modifiers.ctrl = true;
    copy.text = vec![0x03];
    let press = doc.press(&mut core, &copy);
    assert!(press.reached_app());
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "aa", "matrix #21: the shortcut reset the word");
}
