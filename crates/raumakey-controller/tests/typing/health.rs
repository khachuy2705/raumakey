//! Hook health: Windows removes a low-level hook that times out, without telling its thread.
//! The run mode compares the last event the hook saw with the last input of the session
//! (`GetLastInputInfo`) and restarts the hook when the system saw input well after it.

use raumakey_controller::typing::{HOOK_SILENCE_LIMIT_MS, hook_lost};

#[test]
fn input_the_hook_saw_keeps_it_alive() {
    assert!(!hook_lost(Some(5000), 1000, 5000, true));
    assert!(!hook_lost(
        Some(5000),
        1000,
        5000 + HOOK_SILENCE_LIMIT_MS,
        true
    ));
}

#[test]
fn input_well_after_the_last_event_of_the_hook_means_it_is_gone() {
    assert_eq!(HOOK_SILENCE_LIMIT_MS, 1000);
    assert!(hook_lost(
        Some(5000),
        1000,
        5001 + HOOK_SILENCE_LIMIT_MS,
        true
    ));
    assert!(hook_lost(Some(5000), 1000, 60_000, true));
}

#[test]
fn before_its_first_event_the_hook_is_measured_from_its_start() {
    assert!(!hook_lost(None, 1000, 1000, true));
    assert!(!hook_lost(None, 1000, 2000, true));
    assert!(hook_lost(None, 1000, 2001, true));
}

#[test]
fn an_event_older_than_the_start_does_not_move_the_reference_back() {
    // The heartbeat of a restarted hook starts empty; a tick before the restart cannot be
    // its, but the comparison stays sound if it were.
    assert!(!hook_lost(Some(500), 1000, 1900, true));
}

#[test]
fn input_the_hook_is_not_meant_to_see_does_not_count() {
    // An elevated window in front, the lock screen, or no foreground window at all: Windows
    // does not call the hook for that input.
    assert!(!hook_lost(Some(5000), 1000, 60_000, false));
}

#[test]
fn input_older_than_the_last_event_is_no_sign() {
    assert!(!hook_lost(Some(5000), 1000, 3000, true));
}

#[test]
fn ticks_wrap_around_after_49_days() {
    let last = u32::MAX - 100;
    assert!(!hook_lost(Some(last), last, last.wrapping_add(500), true));
    assert!(hook_lost(Some(last), last, last.wrapping_add(1500), true));
    assert!(
        !hook_lost(Some(5), last, 400, true),
        "an event after the wrap"
    );
}
