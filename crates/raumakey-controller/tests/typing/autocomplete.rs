//! D49: an application that completes text inline (a browser's address bar, Excel) selects
//! the rest of a suggestion after each typed character, so the first Backspace of a blind
//! edit removes only that selection and `aa` turns into `aâ`. In a host whose policy sets the
//! autocomplete guard (the default `autocomplete` named set), the hook types a placeholder
//! before the Backspaces, which replaces the selection, and deletes it with one more
//! Backspace.

use raumakey_controller::typing::{AUTOCOMPLETE_PLACEHOLDER, BlindDocument};
use raumakey_model::input::vk;
use raumakey_session::{DocumentPort, EditRequest};
use raumakey_windows::hooks::HookDecision;
use raumakey_windows::input::MAX_BACKSPACES;

use crate::support::{FakePlatform, Sim, backspace_keys, unicode_keys, window};

/// A browser window.
const BROWSER: isize = 0x0005_0050;

/// What the address bar of every test completes.
const HISTORY: [&str; 3] = ["amazon.com", "vietnamnet.vn", "tienganh.vn"];

/// `exe` in front, its window completing [`HISTORY`] inline.
fn address_bar(exe: &str) -> Sim {
    let mut sim = Sim::new(FakePlatform {
        foreground: Some(window(BROWSER, exe)),
        ..FakePlatform::default()
    });
    sim.complete_inline(BROWSER, &HISTORY);
    sim
}

#[test]
fn d49_without_the_guard_a_backspace_removes_only_the_completion() {
    // An application outside the `autocomplete` set shows what the guard prevents.
    let mut sim = address_bar("notepad.exe");
    sim.type_char('a');
    assert_eq!(sim.text(BROWSER), "amazon.com", "the rest is selected");
    sim.type_char('a');
    assert_eq!(sim.text(BROWSER), "aâ");
}

#[test]
fn d49_typing_where_the_host_completes_inline_gives_the_vietnamese_text() {
    for exe in [
        "chrome.exe",
        "msedge.exe",
        "firefox.exe",
        "browser.exe",
        "excel.exe",
    ] {
        let mut sim = address_bar(exe);
        sim.type_char('a');
        assert_eq!(sim.text(BROWSER), "amazon.com", "{exe}");
        sim.type_char('a');
        assert_eq!(sim.text(BROWSER), "â", "{exe}");

        let mut sim = address_bar(exe);
        sim.type_text("vieetj");
        assert_eq!(sim.text(BROWSER), "việt", "{exe}");

        let mut sim = address_bar(exe);
        sim.type_text("tiee");
        assert_eq!(sim.text(BROWSER), "tiê", "{exe}");
        sim.type_text("ngs");
        assert_eq!(sim.text(BROWSER), "tiếng", "{exe}");
    }
}

/// A Backspace inside a word is an edit of the engine (semantic Backspace), so it is guarded
/// too: it deletes the last letter and the completion together, and the engine and the text
/// stay in step for the rest of the word.
#[test]
fn d49_a_backspace_inside_a_word_deletes_the_letter_and_the_completion() {
    let mut sim = address_bar("chrome.exe");
    sim.type_text("vie");
    assert_eq!(sim.text(BROWSER), "vietnamnet.vn");
    assert_eq!(sim.press(vk::BACK), HookDecision::Swallow);
    assert_eq!(sim.text(BROWSER), "vi");
    sim.type_text("eetj");
    assert_eq!(sim.text(BROWSER), "việt");

    let mut sim = address_bar("chrome.exe");
    sim.type_char('a');
    sim.press(vk::BACK);
    assert_eq!(sim.text(BROWSER), "");
    sim.type_text("aa");
    assert_eq!(sim.text(BROWSER), "â");
}

/// With no word in progress the engine owns nothing, so Backspace reaches the application
/// untouched and removes only the completion, as the application intends.
#[test]
fn d49_a_backspace_without_a_word_reaches_the_application() {
    let mut sim = address_bar("chrome.exe");
    sim.type_text("vieetj ");
    sim.click();
    let before = sim.sent().len();
    assert_eq!(sim.press(vk::BACK), HookDecision::PassThrough);
    assert_eq!(sim.sent().len(), before);
    assert_eq!(sim.text(BROWSER), "việt");
}

#[test]
fn d49_the_guard_types_a_placeholder_and_deletes_it_with_one_more_backspace() {
    let mut sim = address_bar("chrome.exe");
    sim.type_char('a');
    let before = sim.sent().len();
    sim.type_char('a');
    let [back_down, back_up] = backspace_keys();
    let [mark_down, mark_up] = unicode_keys(AUTOCOMPLETE_PLACEHOLDER);
    let [hat_down, hat_up] = unicode_keys(0xE2);
    assert_eq!(
        sim.sent()[before..],
        [
            mark_down, mark_up, back_down, back_up, back_down, back_up, hat_down, hat_up
        ]
    );
}

#[test]
fn d49_a_guarded_request_puts_the_placeholder_before_the_backspaces() {
    let expected: Vec<u16> = "a😀".encode_utf16().collect();
    let insert: Vec<u16> = "â".encode_utf16().collect();
    let mut document = BlindDocument::new();
    document
        .replace_before_caret(&EditRequest::new(&expected, &insert).with_autocomplete_guard(true))
        .expect("fits");
    let [back_down, back_up] = backspace_keys();
    let [mark_down, mark_up] = unicode_keys(AUTOCOMPLETE_PLACEHOLDER);
    let [hat_down, hat_up] = unicode_keys(0xE2);
    assert_eq!(
        document.into_keys(),
        [
            mark_down, mark_up, back_down, back_up, back_down, back_up, back_down, back_up,
            hat_down, hat_up
        ],
        "one Backspace per character, and one for the placeholder"
    );
}

#[test]
fn d49_a_guarded_request_that_deletes_nothing_only_types() {
    // Typed text replaces a selected completion by itself.
    let insert: Vec<u16> = "ư".encode_utf16().collect();
    let mut document = BlindDocument::new();
    document
        .replace_before_caret(&EditRequest::new(&[], &insert).with_autocomplete_guard(true))
        .expect("fits");
    assert_eq!(document.into_keys(), unicode_keys(0x01B0));
}

#[test]
fn d49_the_placeholder_backspace_counts_against_the_input_builders() {
    let longest = vec![0x61; usize::try_from(MAX_BACKSPACES).expect("small")];
    let mut document = BlindDocument::new();
    assert!(
        document
            .replace_before_caret(
                &EditRequest::new(&longest, &[0x62]).with_autocomplete_guard(true)
            )
            .is_err()
    );
    assert!(!document.edited());
    let fits = &longest[1..];
    document
        .replace_before_caret(&EditRequest::new(fits, &[0x62]).with_autocomplete_guard(true))
        .expect("fits with the placeholder's Backspace");
    assert_eq!(
        document.into_keys().len(),
        2 + 2 * usize::try_from(MAX_BACKSPACES).expect("small") + 2
    );
}

#[test]
fn d49_the_placeholder_is_a_narrow_no_break_space() {
    // Whitespace, so that a browser does not complete the text that ends with it, and
    // nothing a user types inside a word.
    assert_eq!(AUTOCOMPLETE_PLACEHOLDER, 0x202F);
}
