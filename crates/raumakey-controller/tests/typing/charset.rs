//! D54: the hook types in the charset of the options. The engine works in Unicode; the blind
//! document encodes the text it deletes (one Backspace per character of the encoded text, as
//! the application holds it) and the text it inserts, with `raumakey-converter`.

use raumakey_controller::typing::{AUTOCOMPLETE_PLACEHOLDER, BlindDocument, TypingUpdate};
use raumakey_converter::{Charset, typed_text};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::options::{EngineOptions, OutputCharset};
use raumakey_session::{DocumentPort, EditRequest};

use crate::support::{NOTEPAD, Sim, backspace_keys, unicode_keys};

/// The converter's charset of `charset`.
fn converter(charset: OutputCharset) -> Charset {
    charset
        .name()
        .parse()
        .expect("the converter knows every output charset")
}

/// `text` in `charset`, as the application holds it (`raumakey_converter::typed_text`).
fn encoded(text: &str, charset: OutputCharset) -> String {
    typed_text(text, converter(charset)).expect("small text")
}

/// The keys that type `text`.
fn typing(text: &str) -> Vec<raumakey_windows::input::SyntheticKey> {
    text.encode_utf16().flat_map(unicode_keys).collect()
}

/// `count` Backspaces.
fn backspaces(count: usize) -> Vec<raumakey_windows::input::SyntheticKey> {
    (0..count).flat_map(|_| backspace_keys()).collect()
}

/// The keys of an edit that replaces `expected` by `insert` in `charset`.
fn edit_keys(
    charset: OutputCharset,
    expected: &str,
    insert: &str,
    guard: bool,
) -> Vec<raumakey_windows::input::SyntheticKey> {
    let expected: Vec<u16> = expected.encode_utf16().collect();
    let insert: Vec<u16> = insert.encode_utf16().collect();
    let mut document = BlindDocument::with_charset(charset);
    document
        .replace_before_caret(&EditRequest::new(&expected, &insert).with_autocomplete_guard(guard))
        .expect("fits");
    document.into_keys()
}

#[test]
fn d54_every_output_charset_is_a_converter_charset() {
    for charset in OutputCharset::ALL {
        assert_eq!(converter(charset).name(), charset.name());
    }
}

#[test]
fn d54_an_edit_deletes_and_types_in_the_charset() {
    for charset in OutputCharset::ALL {
        let screen = encoded("tiê", charset);
        let deleted = encoded("ê", charset).chars().count();
        let mut expected = backspaces(deleted);
        expected.extend(typing(&encoded("ế", charset)));
        assert_eq!(
            edit_keys(charset, "ê", "ế", false),
            expected,
            "{charset:?} on {screen:?}"
        );
    }
    // VNI Windows writes `ê` as a letter and a mark: two Backspaces.
    assert_eq!(encoded("ê", OutputCharset::VniWindows).chars().count(), 2);
    assert_eq!(
        edit_keys(OutputCharset::VniWindows, "ê", "ế", false)[..4],
        backspaces(2)[..]
    );
    // Unicode is typed as it is.
    assert_eq!(
        edit_keys(OutputCharset::Unicode, "a", "â", false),
        [backspace_keys().to_vec(), typing("â")].concat()
    );
    // The default port types Unicode.
    let expected: Vec<u16> = "a".encode_utf16().collect();
    let insert: Vec<u16> = "â".encode_utf16().collect();
    let mut document = BlindDocument::new();
    document
        .replace_before_caret(&EditRequest::new(&expected, &insert))
        .expect("fits");
    assert_eq!(
        document.into_keys(),
        edit_keys(OutputCharset::Unicode, "a", "â", false)
    );
}

#[test]
fn d54_the_autocomplete_guard_comes_first_in_any_charset() {
    let charset = OutputCharset::Tcvn3;
    let mut expected = typing(
        &char::from_u32(AUTOCOMPLETE_PLACEHOLDER.into())
            .unwrap()
            .to_string(),
    );
    expected.extend(backspaces(encoded("ê", charset).chars().count() + 1));
    expected.extend(typing(&encoded("ế", charset)));
    assert_eq!(edit_keys(charset, "ê", "ế", true), expected);
}

/// Notepad with the runtime snapshot of `charset` in effect.
fn notepad_in(charset: OutputCharset) -> Sim {
    let sim = Sim::notepad();
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.options = EngineOptions {
        charset,
        ..EngineOptions::defaults_v2()
    };
    sim.mailbox
        .post(TypingUpdate::Runtime(snapshot))
        .expect("the session core takes the snapshot");
    sim
}

/// The texts as the applications of each charset hold them, written out: TCVN3 as the
/// Windows-1252 characters of its bytes (shown by the `.Vn` fonts), Windows-1258 as the
/// characters of its code page (a letter and a combining tone mark where it has no precomposed
/// letter), Unicode decomposed.
#[test]
fn d54_the_charsets_type_these_characters() {
    for (charset, expected) in [
        (OutputCharset::Unicode, "tiếng việt"),
        (OutputCharset::Tcvn3, "tiÕng viÖt"),
        (OutputCharset::Cp1258, "tiê\u{301}ng viê\u{323}t"),
        (
            OutputCharset::UnicodeNfd,
            // Canonical order: the dot below (class 220) before the circumflex (230).
            "tie\u{302}\u{301}ng vie\u{323}\u{302}t",
        ),
    ] {
        let mut sim = notepad_in(charset);
        sim.type_text("tieengs vieetj");
        assert_eq!(sim.text(NOTEPAD), expected, "{charset:?}");
    }
}

#[test]
fn d54_typing_in_a_charset_gives_the_text_in_that_charset() {
    for charset in OutputCharset::ALL {
        let mut sim = notepad_in(charset);
        sim.type_text("tieengs vieetj nguwowif");
        assert_eq!(
            sim.text(NOTEPAD),
            encoded("tiếng việt người", charset),
            "{charset:?}"
        );
        // Backspace restores the state before, in the charset too.
        sim.press(raumakey_model::input::vk::BACK);
        assert_eq!(
            sim.text(NOTEPAD),
            encoded("tiếng việt ngươi", charset),
            "{charset:?}"
        );
    }
}

#[test]
fn d54_a_change_of_charset_ends_the_word_in_progress() {
    let mut sim = notepad_in(OutputCharset::Unicode);
    sim.type_text("tiee");
    assert_eq!(sim.text(NOTEPAD), "tiê");
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.options = EngineOptions {
        charset: OutputCharset::VniWindows,
        ..EngineOptions::defaults_v2()
    };
    sim.mailbox
        .post(TypingUpdate::Runtime(snapshot))
        .expect("the session core takes the snapshot");
    sim.type_char('s');
    assert_eq!(
        sim.text(NOTEPAD),
        "tiês",
        "the Unicode `ê` is never deleted as VNI"
    );
    sim.type_text(" aa");
    assert_eq!(
        sim.text(NOTEPAD),
        format!("tiês {}", encoded("â", OutputCharset::VniWindows))
    );
}
