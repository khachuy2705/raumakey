//! Macros and Unicode through the core with the real `raumakey_macro::MacroTable` (matrix
//! #47–#54): expansion only at a boundary, Shift+Space, case adaptation, macros while
//! Vietnamese is off, NFD input matching an NFC trigger, NFD output counted in UTF-16 units,
//! and supplementary scalars next to the word.

use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::Normalization;
use raumakey_tip::session::ServiceCore;

use crate::support::{FakeDocument, edit};

fn core_with_macros(
    entries: &[(&str, &str)],
    change: impl FnOnce(&mut RuntimeSnapshot),
) -> ServiceCore {
    let mut core = ServiceCore::new();
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 1;
    change(&mut snapshot);
    core.apply_runtime_snapshot(&snapshot)
        .expect("valid snapshot");
    let macros = MacroSnapshot {
        macro_revision: 1,
        entries: entries
            .iter()
            .zip(1..)
            .map(|(&(trigger, replacement), id)| MacroEntry {
                id: MacroId(id),
                trigger: trigger.to_owned(),
                replacement: replacement.to_owned(),
                enabled: true,
                boundary: BoundaryPolicy::Delimiter,
                case_policy: CasePolicy::Adapt,
            })
            .collect(),
    };
    core.apply_macro_snapshot(&macros).expect("valid macros");
    core
}

#[test]
fn matrix_47_a_trigger_and_space_become_the_replacement_and_the_space() {
    let mut core = core_with_macros(&[("ko", "không")], |_| {});
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko ");
    assert_eq!(doc.text(), "không ");
    assert_eq!(doc.edits, [edit(2, "không ")]);
}

#[test]
fn matrix_48_a_trigger_inside_a_word_does_not_expand() {
    let mut core = core_with_macros(&[("ko", "không")], |_| {});
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "kok ");
    assert_eq!(doc.text(), "kok ");
    assert!(doc.edits.is_empty());
}

#[test]
fn matrix_49_shift_space_commits_the_trigger_unchanged() {
    let mut core = core_with_macros(&[("ko", "không")], |_| {});
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko");
    let mut shift_space = doc.char_key(' ');
    shift_space.modifiers.shift = true;
    doc.press(&mut core, &shift_space);
    assert_eq!(doc.text(), "ko ");
    assert!(doc.edits.is_empty());
}

#[test]
fn matrix_50_the_replacement_follows_the_case_of_the_trigger() {
    let mut core = core_with_macros(&[("ko", "không")], |_| {});
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "KO Ko ko ");
    assert_eq!(doc.text(), "KHÔNG Không không ");
}

#[test]
fn matrix_51_macros_expand_while_vietnamese_is_off_when_enabled() {
    let mut core = core_with_macros(&[("ko", "không")], |s| {
        s.options.macros_when_disabled = true;
    });
    core.set_vietnamese_mode(false);
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "aa ko ");
    assert_eq!(doc.text(), "aa không ");
}

#[test]
fn matrix_52_nfd_input_matches_an_nfc_trigger_and_gives_nfc() {
    // A layout that types the circumflex as a combining mark after its vowel: the input is
    // NFD, while the trigger and the normalization of defaults v1 are NFC.
    let mut core = core_with_macros(&[("tôi", "chúng tôi")], |_| {});
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "to\u{302}i ");
    assert_eq!(doc.text(), "ch\u{FA}ng t\u{F4}i ", "precomposed: NFC");
    assert_eq!(doc.edits.last(), Some(&edit(3, "chúng tôi ")));
}

#[test]
fn matrix_53_nfd_output_is_counted_in_utf16_units() {
    let mut core = core_with_macros(&[("tôi", "chúng tôi")], |s| {
        s.options.normalization = Normalization::Nfd;
    });
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "tooi ");
    let expected = "chu\u{301}ng to\u{302}i ";
    assert_eq!(doc.text(), expected);
    // "tôi" in NFD is t, o, U+0302, i: four units before the caret.
    assert_eq!(doc.edits.last(), Some(&edit(4, expected)));
}

#[test]
fn matrix_54_a_supplementary_scalar_before_the_word_is_never_touched() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    // An emoji the application put before the caret, outside the engine's word.
    doc.replace_before_caret_behind(0, "😀");
    doc.type_text(&mut core, "aas");
    assert_eq!(doc.text(), "😀ấ");
    assert_eq!(doc.edits, [edit(1, "â"), edit(1, "ấ")]);
    doc.type_key(&mut core, raumakey_model::input::vk::BACK);
    doc.type_key(&mut core, raumakey_model::input::vk::BACK);
    doc.type_key(&mut core, raumakey_model::input::vk::BACK);
    assert_eq!(
        doc.text(),
        "😀",
        "backspace never splits the surrogate pair"
    );
}
