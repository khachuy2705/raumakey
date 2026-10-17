//! Document context: changes Raumakey did not make (matrix #19), focus and document changes
//! (#20), closed documents, the bound on document states, and engine errors.

use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::host::SessionId;
use raumakey_model::input::vk;
use raumakey_model::utf16;
use raumakey_tip::session::{EditError, MAX_DOCUMENTS, SelectionChange, ServiceCore};

use crate::support::{FakeDocument, edit, host, release};

#[test]
fn matrix_19_a_selection_change_by_the_user_resets_the_word() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    // The user clicks before the letter; OnEndEdit reports a change Raumakey did not make.
    doc.move_caret(0);
    core.selection_changed(doc.id, SelectionChange::Other);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "sa");
    assert_eq!(
        doc.attempts, 0,
        "the tone key never edits the text before the click"
    );
}

#[test]
fn matrix_19_a_reported_change_resets_even_when_the_caret_ends_where_it_was() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "a ");
    doc.type_char(&mut core, 'a');
    // A click right after the letter: the text before the caret looks unchanged, but the
    // selection change was not made by Raumakey.
    core.selection_changed(doc.id, SelectionChange::Other);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "a as");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn matrix_19_a_click_before_the_passed_key_is_reported_ends_the_word() {
    // The host has not reported the application typing the letter (it reports late, never,
    // or together with the click, which the COM layer reports as `Other`) when the user
    // clicks after "ba", where the text before the caret equals the word in progress.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ba b");
    let a = doc.char_key('a');
    assert!(core.test_key_down(&a));
    assert!(!core.key_down(&a, &mut doc).eaten);
    assert!(doc.app_key(&a).is_some());
    doc.move_caret(2);
    core.selection_changed(doc.id, SelectionChange::Other);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "bas ba");
    assert_eq!(
        doc.attempts, 0,
        "the tone key never edits the text before the click"
    );
}

#[test]
fn matrix_19_a_report_of_other_text_than_the_passed_key_ends_the_word() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let a = doc.char_key('a');
    assert!(core.test_key_down(&a));
    assert!(!core.key_down(&a, &mut doc).eaten);
    // The application typed something else (it changed the letter, or another change came
    // first).
    doc.replace_before_caret_behind(0, "b");
    core.selection_changed(doc.id, SelectionChange::Inserted(&utf16::encode("b")));
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "bs");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn matrix_19_a_mouse_press_ends_the_word() {
    // The COM layer reports every mouse button press in the host, before the host reports
    // the caret move it causes, if it ever does.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ba b");
    let a = doc.char_key('a');
    assert!(core.test_key_down(&a));
    assert!(!core.key_down(&a, &mut doc).eaten);
    core.mouse_down(&doc.host);
    doc.deliver(&mut core, &a);
    // The click put the caret after "ba"; the host never reports that.
    doc.move_caret(2);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "bas ba");
    assert_eq!(
        doc.attempts, 0,
        "the tone key never edits the text before the click"
    );
}

#[test]
fn matrix_19_a_mouse_press_after_the_last_report_ends_the_word() {
    // Every key has arrived and been reported, so no arrival is expected that the press
    // could spoil: the press itself must end the word.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ba ba");
    core.mouse_down(&doc.host);
    // The click put the caret after the first "ba", where the text before the caret equals
    // the word in progress; the host never reports that.
    doc.move_caret(2);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "bas ba");
    assert_eq!(
        doc.attempts, 0,
        "the tone key never edits the text before the click"
    );
}

#[test]
fn a_mouse_press_keeps_the_word_where_the_rules_say_so() {
    let mut core = ServiceCore::new();
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 1;
    snapshot.compatibility.default_reset_on_mouse = false;
    core.apply_runtime_snapshot(&snapshot)
        .expect("valid snapshot");
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    // A press that moves nothing, such as a click on a toolbar of the host.
    core.mouse_down(&doc.host);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "á");
}

#[test]
fn an_unreported_caret_move_is_caught_by_the_edit_check_when_the_text_differs() {
    // Without any report, the edit check is the only guard. It catches this move because
    // the text before the new caret differs from the word; the click tests above show why
    // it cannot be relied on.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    doc.move_caret(0);
    let press = doc.type_char(&mut core, 's');
    assert!(press.reached_app());
    assert_eq!(doc.failures, [EditError::RangeShorter { covered: 0 }]);
    assert_eq!(doc.text(), "sa");
}

#[test]
fn the_application_typing_a_passed_key_is_not_a_user_selection_change() {
    // `FakeDocument::press` reports every change the application makes, as OnEndEdit will:
    // the change that follows a key the core let through is that key arriving.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let first = doc.type_char(&mut core, 'a');
    assert!(first.reached_app());
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "á");
    assert_eq!(doc.edits, [edit(1, "á")]);
}

#[test]
fn only_the_first_change_after_a_passed_key_is_its_arrival() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    // `press` already reported the application typing the letter; the same report again
    // is not that key.
    core.selection_changed(doc.id, SelectionChange::Inserted(&utf16::encode("a")));
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "as");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn an_arrival_reported_after_the_next_key_down_ends_the_word() {
    // The arrival of a passed key is expected only until the next key-down: a later report
    // cannot be told from a change of the same text made by someone else.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let a = doc.char_key('a');
    assert!(core.test_key_down(&a));
    assert!(!core.key_down(&a, &mut doc).eaten);
    let typed = doc.app_key(&a).expect("the application types the letter");
    // Shift goes down before the host reports the letter.
    let shift = doc.key(vk::SHIFT);
    assert!(!core.test_key_down(&shift));
    core.selection_changed(doc.id, typed.report());
    release(&mut core, &shift);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "as");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_change_in_another_document_is_not_the_arrival() {
    let mut core = ServiceCore::new();
    let mut a = FakeDocument::new(1);
    let b = FakeDocument::new(2);
    let key = a.char_key('a');
    // The key reaches the application, which has not typed it yet.
    assert!(core.test_key_down(&key));
    assert!(!core.key_down(&key, &mut a).eaten);
    core.selection_changed(b.id, SelectionChange::Inserted(&key.text));
    a.deliver(&mut core, &key);
    a.type_char(&mut core, 'a');
    assert_eq!(
        a.text(),
        "â",
        "the arrival of the letter was expected in its own document"
    );
}

#[test]
fn matrix_20_a_focus_change_mid_word_never_deletes_across_documents() {
    let mut core = ServiceCore::new();
    let mut a = FakeDocument::new(1);
    let mut b = FakeDocument::new(2);
    a.type_char(&mut core, 'a');
    core.set_focus(Some(b.id));
    b.type_char(&mut core, 'a');
    assert_eq!(
        b.text(),
        "a",
        "the word of the other document does not continue here"
    );
    assert_eq!(b.attempts, 0);
    b.type_char(&mut core, 'a');
    assert_eq!(b.text(), "â", "the new document has its own word");
    core.set_focus(Some(a.id));
    a.type_char(&mut core, 's');
    assert_eq!(a.text(), "as", "the word ended when the focus left");
    assert_eq!(a.attempts, 0);
}

#[test]
fn matrix_20_a_key_from_another_document_is_a_document_change() {
    // No focus notification arrives; the key's own document tells.
    let mut core = ServiceCore::new();
    let mut a = FakeDocument::new(1);
    let mut b = FakeDocument::new(2);
    a.type_char(&mut core, 'a');
    b.type_char(&mut core, 'a');
    assert_eq!(b.text(), "a");
    a.type_char(&mut core, 's');
    assert_eq!(a.text(), "as");
    assert_eq!(a.attempts + b.attempts, 0, "no edit in either document");
}

#[test]
fn a_focus_change_away_from_every_document_ends_the_word() {
    let mut core = ServiceCore::new();
    let mut a = FakeDocument::new(1);
    a.type_char(&mut core, 'a');
    core.set_focus(None);
    a.type_char(&mut core, 'a');
    assert_eq!(a.text(), "aa");
}

#[test]
fn the_same_document_id_in_another_session_is_another_document() {
    let mut core = ServiceCore::new();
    let mut a = FakeDocument::new(1);
    let mut other_host = host(1);
    other_host.session_id = SessionId::from_u128(2);
    let mut other = FakeDocument::with_host(other_host);
    a.type_char(&mut core, 'a');
    other.type_char(&mut core, 'a');
    assert_eq!(other.text(), "a");
    assert_eq!(other.attempts, 0);
}

#[test]
fn closing_a_document_drops_its_state() {
    let mut core = ServiceCore::new();
    let mut a = FakeDocument::new(1);
    a.type_char(&mut core, 'a');
    assert_eq!(core.document_count(), 1);
    core.document_closed(a.id);
    assert_eq!(core.document_count(), 0);
    a.type_char(&mut core, 's');
    assert_eq!(a.text(), "as");
    assert_eq!(a.attempts, 0);
}

#[test]
fn document_states_are_bounded() {
    let mut core = ServiceCore::new();
    let total = MAX_DOCUMENTS + 8;
    let mut docs: Vec<FakeDocument> = (0..total)
        .map(|i| FakeDocument::new(u128::try_from(i).expect("small") + 100))
        .collect();
    for doc in &mut docs {
        doc.type_char(&mut core, 'a');
        assert!(core.document_count() <= MAX_DOCUMENTS);
    }
    assert_eq!(core.document_count(), MAX_DOCUMENTS);
    let last = docs.last_mut().expect("documents");
    last.type_char(&mut core, 'a');
    assert_eq!(last.text(), "â", "the most recent document keeps its word");
    // An evicted document comes back with a new state, within the bound.
    let first = docs.first_mut().expect("documents");
    first.type_text(&mut core, " aa");
    assert_eq!(first.text(), "a â");
    assert_eq!(core.document_count(), MAX_DOCUMENTS);
}

#[test]
fn an_engine_error_resets_the_word_and_passes_the_key() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let mut broken = doc.char_key('x');
    broken.text = vec![0xD800];
    let press = doc.press(&mut core, &broken);
    assert!(
        press.claimed,
        "the reset of the word is committed in key_down"
    );
    assert!(press.reached_app());
    doc.type_char(&mut core, 's');
    assert_eq!(doc.attempts, 0, "the word ended at the failing key");
}

#[test]
fn an_engine_error_ends_the_word_before_any_report_arrives() {
    // The next key arrives before the application reports anything about the failing key.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let mut broken = doc.char_key('x');
    broken.text = vec![0xD800];
    assert!(core.test_key_down(&broken));
    assert!(!core.key_down(&broken, &mut doc).eaten);
    doc.type_char(&mut core, 's');
    assert_eq!(doc.attempts, 0, "the word ended at the failing key");
    assert_eq!(doc.text(), "as");
}

#[test]
fn an_engine_error_without_a_word_is_not_claimed() {
    let mut core = ServiceCore::new();
    let doc = FakeDocument::new(1);
    let mut broken = doc.char_key('x');
    broken.text = vec![0xDC00];
    assert!(!core.test_key_down(&broken));
}
