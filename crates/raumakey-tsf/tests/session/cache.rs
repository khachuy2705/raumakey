//! `OnKeyDown` reuses the result of `OnTestKeyDown` only while it is current: the same key,
//! the same document state, and the same configuration. Every other case computes the key
//! again, and a test alone never commits anything.

use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::InputMethod;
use raumakey_tip::session::{KeyStroke, SelectionChange, ServiceCore};

use crate::support::{FakeDocument, edit};

/// A core and a document holding "a" with the word in progress, and the key that would make
/// it "â".
fn word_a() -> (ServiceCore, FakeDocument, KeyStroke) {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let a = doc.char_key('a');
    (core, doc, a)
}

/// `OnKeyDown` for `key`, then the application when the key was not eaten.
fn key_down(core: &mut ServiceCore, doc: &mut FakeDocument, key: &KeyStroke) -> bool {
    let outcome = core.key_down(key, doc);
    if !outcome.eaten {
        doc.deliver(core, key);
    }
    outcome.eaten
}

#[test]
fn a_test_without_key_down_commits_nothing() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let a = doc.char_key('a');
    assert!(core.test_key_down(&a), "the letter would commit state");
    // TSF never delivers the key.
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "s", "no word was started by the test alone");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_test_of_an_edit_without_key_down_changes_nothing() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    assert_eq!(doc.attempts, 0, "a test never edits");
    doc.type_char(&mut core, 's');
    assert_eq!(
        doc.text(),
        "á",
        "the word still holds the letter, not the circumflex"
    );
}

#[test]
fn a_repeated_test_is_answered_the_same_and_applied_once() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    assert!(core.test_key_down(&a));
    assert!(key_down(&mut core, &mut doc, &a));
    assert_eq!(doc.edits, [edit(1, "â")]);
}

#[test]
fn key_down_without_a_test_computes_the_key() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let a = doc.char_key('a');
    assert!(!key_down(&mut core, &mut doc, &a));
    assert!(key_down(&mut core, &mut doc, &a));
    assert_eq!(doc.text(), "â");
}

#[test]
fn the_test_of_another_key_is_not_used() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    let s = doc.char_key('s');
    assert!(key_down(&mut core, &mut doc, &s));
    assert_eq!(doc.edits, [edit(1, "á")]);
}

#[test]
fn the_test_of_the_same_key_in_another_host_state_is_not_used() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    // Same virtual key and scan code, but the field turned secure in between.
    let mut secure = a.clone();
    secure.host.is_secure_field = true;
    assert!(!key_down(&mut core, &mut doc, &secure));
    assert_eq!(
        doc.attempts, 0,
        "the edit tested for the plain field is not applied"
    );
}

#[test]
fn a_macro_snapshot_between_test_and_key_down_is_honoured() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko");
    let space = doc.char_key(' ');
    assert!(core.test_key_down(&space));
    let snapshot = MacroSnapshot {
        macro_revision: 1,
        entries: vec![MacroEntry {
            id: MacroId(1),
            trigger: "ko".to_owned(),
            replacement: "không".to_owned(),
            enabled: true,
            boundary: BoundaryPolicy::Delimiter,
            case_policy: CasePolicy::Adapt,
        }],
    };
    core.apply_macro_snapshot(&snapshot).expect("valid macros");
    assert!(key_down(&mut core, &mut doc, &space));
    assert_eq!(doc.text(), "không ");
}

#[test]
fn a_runtime_snapshot_between_test_and_key_down_is_honoured() {
    let (mut core, mut doc, _) = word_a();
    let s = doc.char_key('s');
    assert!(core.test_key_down(&s), "Telex would add the acute");
    let mut vni = RuntimeSnapshot::defaults_v1();
    vni.config_revision = 1;
    vni.options.input_method = InputMethod::Vni;
    core.apply_runtime_snapshot(&vni).expect("valid snapshot");
    assert!(!key_down(&mut core, &mut doc, &s));
    assert_eq!(doc.text(), "as");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_mode_change_between_test_and_key_down_is_honoured() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    core.set_vietnamese_mode(false);
    assert!(!key_down(&mut core, &mut doc, &a));
    assert_eq!(doc.text(), "aa");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_reset_between_test_and_key_down_is_honoured() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    core.reset_all();
    assert!(!key_down(&mut core, &mut doc, &a));
    assert_eq!(doc.text(), "aa");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_reset_between_the_test_of_a_passed_letter_and_key_down_starts_a_new_word() {
    // The tested result of "a" lets the key through and would continue the word "ho"; after
    // the reset the letter starts a new word, so the tone lands on it alone.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ho");
    let a = doc.char_key('a');
    assert!(core.test_key_down(&a));
    core.reset_all();
    assert!(!key_down(&mut core, &mut doc, &a));
    doc.type_char(&mut core, 'f');
    assert_eq!(
        doc.text(),
        "hoà",
        "not \"hòa\": the word restarted at the letter"
    );
    assert_eq!(doc.edits, [edit(1, "à")]);
}

#[test]
fn a_selection_change_between_test_and_key_down_is_honoured() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    core.selection_changed(doc.id, SelectionChange::Other);
    assert!(!key_down(&mut core, &mut doc, &a));
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_focus_change_between_test_and_key_down_is_honoured() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    core.set_focus(Some(FakeDocument::new(2).id));
    core.set_focus(Some(doc.id));
    assert!(!key_down(&mut core, &mut doc, &a));
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_closed_document_between_test_and_key_down_starts_again() {
    let (mut core, mut doc, a) = word_a();
    assert!(core.test_key_down(&a));
    core.document_closed(doc.id);
    assert!(!key_down(&mut core, &mut doc, &a));
    assert_eq!(doc.attempts, 0);
}
