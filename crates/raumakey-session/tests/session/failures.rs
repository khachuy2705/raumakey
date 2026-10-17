//! Apply failures (matrix #41): the core never commits the next state when the edit session
//! fails; it resets the word and lets the key reach the application.

use raumakey_session::{EditError, ServiceCore};

use crate::support::{Behavior, FakeDocument};

/// Types "a", makes the document answer the next edit with `behavior`, and types "a" again.
fn second_a_with(behavior: Behavior) -> (ServiceCore, FakeDocument, bool) {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    doc.behavior = behavior;
    let press = doc.type_char(&mut core, 'a');
    assert!(press.claimed);
    (core, doc, press.outcome.eaten)
}

#[test]
fn matrix_41_a_range_shorter_than_delete_before_is_not_committed() {
    let (mut core, mut doc, eaten) = second_a_with(Behavior::Shorter(0));
    assert!(!eaten, "the key passes through");
    assert_eq!(doc.failures, [EditError::RangeShorter { covered: 0 }]);
    assert!(doc.edits.is_empty());
    assert_eq!(doc.text(), "aa", "the application typed the key itself");

    // The word was reset, not committed as "â": the next keys start a new word.
    doc.behavior = Behavior::Accept;
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "aas");
    assert!(
        doc.edits.is_empty(),
        "no edit was attempted after the reset"
    );
    assert_eq!(doc.attempts, 1);
}

#[test]
fn matrix_41_the_key_after_a_failed_edit_starts_a_new_word() {
    // The next key arrives before the application reports the failed key, so only the state
    // the failure left behind decides what happens: a reset, neither the rejected "â" nor
    // the old word "a".
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    doc.behavior = Behavior::Shorter(0);
    let a = doc.char_key('a');
    assert!(core.test_key_down(&a));
    assert!(!core.key_down(&a, &mut doc).eaten, "the key passes through");
    doc.behavior = Behavior::Accept;
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.attempts, 1, "no edit after the failed one");
    assert_eq!(doc.text(), "aa");
}

#[test]
fn a_denied_edit_session_is_not_committed() {
    let (mut core, mut doc, eaten) = second_a_with(Behavior::Deny);
    assert!(!eaten);
    assert_eq!(doc.failures, [EditError::Denied]);
    assert_eq!(doc.text(), "aa");
    doc.behavior = Behavior::Accept;
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "aaa", "the reset word does not continue");
    assert!(doc.edits.is_empty());
}

#[test]
fn text_changed_behind_the_core_is_detected_and_never_overwritten() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    // The application replaced the letter without moving the caret or telling anyone.
    doc.replace_before_caret_behind(1, "b");
    let press = doc.type_char(&mut core, 's');
    assert!(!press.outcome.eaten);
    assert_eq!(doc.failures, [EditError::TextMismatch]);
    assert_eq!(doc.text(), "bs");
}

#[test]
fn a_failed_edit_keeps_working_afterwards() {
    let (mut core, mut doc, _) = second_a_with(Behavior::Shorter(0));
    doc.behavior = Behavior::Accept;
    doc.type_char(&mut core, ' ');
    doc.type_text(&mut core, "as");
    assert_eq!(doc.text(), "aa á");
}
