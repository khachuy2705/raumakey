//! Keys the hook lets through untouched: Raumakey's own (matrix #39), those of a window that
//! runs with more privileges than Raumakey (UIPI would drop what the hook sends), those of a
//! password Edit control (D25, matrix #29), and those typed without a foreground window.

use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::vk;
use raumakey_windows::hooks::HookDecision;

use crate::support::{ADMIN, FakePlatform, NOTEPAD, Sim, admin_prompt, notepad, window};

#[test]
fn matrix_39_raumakey_injected_keys_pass_untouched() {
    let mut sim = Sim::notepad();
    sim.type_char('a');
    let reads = sim.platform().foreground_reads;
    for (virtual_key, down) in [(vk::LSHIFT, true), (u32::from(b'A'), true)] {
        let mut event = sim.physical(virtual_key, 0x2A, down);
        event.injected = true;
        event.extra_info = RAUMAKEY_INJECTED_TAG as usize;
        assert_eq!(sim.handler.on_key(&event), HookDecision::PassThrough);
    }
    assert_eq!(
        sim.platform().foreground_reads,
        reads,
        "nothing is read for an injected key"
    );
    assert!(sim.platform().sends.is_empty());
    assert!(
        !sim.handler.tracker().is_down(vk::LSHIFT),
        "an injected Shift is not the user's"
    );
    // The word in progress never saw the injected key.
    sim.type_char('a');
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn keys_injected_by_other_programs_are_typed_like_physical_ones() {
    let mut sim = Sim::notepad();
    for virtual_key in [u32::from(b'A'), u32::from(b'A')] {
        for down in [true, false] {
            let mut event = sim.physical(virtual_key, 0x1E, down);
            event.injected = true;
            event.extra_info = 0x1234;
            sim.event(event);
        }
    }
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn an_elevated_foreground_gets_every_key_untouched() {
    let mut sim = Sim::new(FakePlatform {
        foreground: Some(admin_prompt()),
        ..FakePlatform::default()
    });
    let decisions = sim.type_text("tieengs");
    assert!(
        decisions
            .iter()
            .all(|decision| *decision == HookDecision::PassThrough)
    );
    assert!(
        sim.platform().sends.is_empty(),
        "UIPI would drop every key sent"
    );
    assert_eq!(sim.text(ADMIN), "tieengs");
}

#[test]
fn a_foreground_of_unknown_elevation_counts_as_elevated() {
    let mut unknown = window(ADMIN, "lsass.exe");
    unknown.elevated = None;
    let mut sim = Sim::new(FakePlatform {
        foreground: Some(unknown),
        ..FakePlatform::default()
    });
    sim.type_text("aa");
    assert!(sim.platform().sends.is_empty());
    assert_eq!(sim.text(ADMIN), "aa");
}

#[test]
fn an_elevated_raumakey_types_in_an_elevated_window() {
    let mut sim = Sim::new(FakePlatform {
        foreground: Some(admin_prompt()),
        elevated: true,
        ..FakePlatform::default()
    });
    sim.type_text("tieengs");
    assert_eq!(sim.text(ADMIN), "tiếng");
}

#[test]
fn the_word_ends_when_an_elevated_window_takes_the_keys() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    sim.focus(Some(admin_prompt()));
    sim.type_char('x');
    assert_eq!(sim.text(ADMIN), "x");
    sim.focus(Some(notepad()));
    let sends = sim.platform().sends.len();
    sim.type_char('s');
    assert_eq!(sim.platform().sends.len(), sends);
    assert_eq!(sim.text(NOTEPAD), "tiês");
}

#[test]
fn hotkeys_are_not_classified_in_an_elevated_window() {
    let mut sim = Sim::new(FakePlatform {
        foreground: Some(admin_prompt()),
        ..FakePlatform::default()
    });
    sim.tap_ctrl_shift();
    assert!(sim.handler.core().vietnamese_mode());
    assert!(sim.modes().is_empty());
}

#[test]
fn matrix_29_a_password_edit_gets_every_key_untouched() {
    let mut password = notepad();
    password.secure_field = true;
    let mut sim = Sim::new(FakePlatform {
        foreground: Some(password),
        ..FakePlatform::default()
    });
    sim.type_text("tieengs aa");
    assert!(sim.platform().sends.is_empty());
    assert_eq!(sim.text(NOTEPAD), "tieengs aa");
}

#[test]
fn matrix_29_a_field_that_turns_into_a_password_edit_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    let mut password = notepad();
    password.secure_field = true;
    sim.focus(Some(password));
    sim.type_char('s');
    sim.focus(Some(notepad()));
    sim.type_char('s');
    assert_eq!(sim.text(NOTEPAD), "tiêss");
}

#[test]
fn without_a_foreground_window_keys_pass_and_hotkeys_still_work() {
    let mut sim = Sim::new(FakePlatform::default());
    let decisions = sim.type_text("aa");
    assert!(
        decisions
            .iter()
            .all(|decision| *decision == HookDecision::PassThrough)
    );
    assert!(sim.platform().sends.is_empty());
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [false]);
}
