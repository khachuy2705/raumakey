//! The keyboard open/close compartment and the Vietnamese mode of the core, kept in step.

use raumakey_tip::adapter::mode::{ModeMirror, mode_of, open_close_value};

#[test]
fn activation_shows_the_mode_of_the_core_in_the_compartment() {
    let mut mirror = ModeMirror::default();
    assert_eq!(mirror.sync(true), Some(true), "nothing written yet");
    assert_eq!(mirror.sync(true), None, "already shown");
}

#[test]
fn a_local_toggle_is_written_to_the_compartment() {
    let mut mirror = ModeMirror::default();
    mirror.sync(true);
    assert_eq!(mirror.sync(false), Some(false));
    assert_eq!(mirror.sync(false), None);
    assert_eq!(mirror.sync(true), Some(true));
}

#[test]
fn a_compartment_change_sets_the_mode_of_the_core() {
    let mut mirror = ModeMirror::default();
    mirror.sync(true);
    // The language bar or the input indicator closed the keyboard.
    assert_eq!(mirror.changed(Some(false)), Some(false));
    assert_eq!(
        mirror.sync(false),
        None,
        "the core followed: nothing to write"
    );
}

#[test]
fn the_notification_of_a_write_asks_for_the_mode_the_core_has() {
    let mut mirror = ModeMirror::default();
    assert_eq!(mirror.sync(false), Some(false));
    // TSF tells every sink, the writer included.
    assert_eq!(mirror.changed(Some(false)), Some(false));
    assert_eq!(mirror.sync(false), None);
}

#[test]
fn a_failed_write_is_tried_again_after_the_next_key() {
    let mut mirror = ModeMirror::default();
    assert_eq!(mirror.sync(true), Some(true));
    mirror.write_failed();
    assert_eq!(mirror.sync(true), Some(true));
}

#[test]
fn a_cleared_compartment_leaves_the_mode_and_is_written_again() {
    let mut mirror = ModeMirror::default();
    mirror.sync(true);
    assert_eq!(mirror.changed(None), None);
    assert_eq!(mirror.sync(true), Some(true));
}

#[test]
fn the_compartment_holds_a_nonzero_number_while_the_keyboard_is_open() {
    assert_eq!(open_close_value(true), 1);
    assert_eq!(open_close_value(false), 0);
    assert_eq!(mode_of(Some(0)), Some(false));
    assert_eq!(mode_of(Some(1)), Some(true));
    assert_eq!(mode_of(Some(-1)), Some(true));
    assert_eq!(mode_of(None), None, "an empty compartment says nothing");
}
