//! `Debug` output never shows keystrokes, document text, or macro text (design §14, plan
//! §0.4): a single `{:?}` in a log or panic message must not leak them.

use raumakey_ipc::message::MacroSnapshot;
use raumakey_model::input::vk;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::utf16;
use raumakey_session::{SelectionChange, ServiceCore};

use crate::support::FakeDocument;

/// Fails when `debug` shows `text` as characters, as the list of its UTF-16 units (the
/// derived `Debug` of a `Vec<u16>`), or shows any non-ASCII unit of it as a number.
fn assert_hidden(debug: &str, text: &str) {
    assert!(!debug.contains(text), "{text:?} shown: {debug}");
    let units: Vec<String> = text.encode_utf16().map(|unit| unit.to_string()).collect();
    assert!(
        !debug.contains(&units.join(", ")),
        "the units of {text:?} shown: {debug}"
    );
    for unit in text.encode_utf16().filter(|unit| *unit >= 0x80) {
        assert!(
            !debug.contains(&unit.to_string()),
            "a unit of {text:?} shown: {debug}"
        );
    }
}

fn secret_macros() -> MacroSnapshot {
    MacroSnapshot {
        macro_revision: 1,
        entries: vec![MacroEntry {
            id: MacroId(1),
            trigger: "mkz".to_owned(),
            replacement: "mật khẩu".to_owned(),
            enabled: true,
            boundary: BoundaryPolicy::Delimiter,
            case_policy: CasePolicy::Adapt,
        }],
    }
}

#[test]
fn key_strokes_hide_their_key_and_text() {
    let doc = FakeDocument::new(1);
    let key = doc.char_key('ữ');
    let debug = format!("{key:?}");
    assert_hidden(&debug, "ữ");
    for field in ["text", "virtual_key", "scan_code"] {
        assert!(!debug.contains(field), "{field} shown: {debug}");
    }
}

#[test]
fn edit_requests_hide_document_text() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "uwow");
    assert_eq!(doc.text(), "ươ");
    let debug = doc.request_debug.join("\n");
    assert!(!debug.is_empty());
    assert_hidden(&debug, "ươ");
    assert!(!debug.contains("expected"), "{debug}");
}

#[test]
fn reported_changes_hide_the_inserted_text() {
    let text = utf16::encode("ữ");
    let debug = format!("{:?}", SelectionChange::Inserted(&text));
    assert_hidden(&debug, "ữ");
    assert!(debug.contains("units"), "{debug}");
}

#[test]
fn key_down_outcomes_hide_the_replayed_key() {
    let mut core = ServiceCore::new();
    core.apply_macro_snapshot(&secret_macros())
        .expect("valid macros");
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "mkz");
    let enter = doc.key(vk::RETURN);
    let press = doc.press(&mut core, &enter);
    assert!(press.outcome.replay.is_some());
    let debug = format!("{:?}", press.outcome);
    assert!(!debug.contains("virtual_key"), "{debug}");
    assert!(!debug.contains(&vk::RETURN.to_string()), "{debug}");
}

#[test]
fn the_core_hides_words_documents_and_macros() {
    let mut core = ServiceCore::new();
    core.apply_macro_snapshot(&secret_macros())
        .expect("valid macros");
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "bis maatj");
    assert_eq!(doc.text(), "bí mật");
    let tested = doc.char_key('k');
    assert!(core.test_key_down(&tested));
    let debug = format!("{core:?}");
    for text in ["mật", "bí", "mật khẩu", "mkz"] {
        assert_hidden(&debug, text);
    }
    let config = format!("{:?}", core.config());
    for text in ["mật khẩu", "mkz"] {
        assert_hidden(&config, text);
    }
}
