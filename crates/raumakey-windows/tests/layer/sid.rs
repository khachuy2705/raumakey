//! `sid`: user SID and session of the current and other processes (read-only).

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::names::sid_hash16;
use raumakey_windows::security::is_valid_sid_string;
use raumakey_windows::sid::{
    current_process_is_elevated, current_session_id, current_user_sid_hash,
    current_user_sid_string, process_session_id, process_user_sid_string,
};

use crate::support::TestToken;

#[test]
fn the_current_identity_is_the_process_identity_while_a_thread_impersonates() {
    let sid = current_user_sid_string().unwrap();
    let session = current_session_id().unwrap();
    let elevated = current_process_is_elevated().unwrap();
    for (token, what) in [
        (TestToken::low_integrity(), "low integrity"),
        (TestToken::app_container(), "AppContainer"),
    ] {
        let (seen_sid, seen_session, seen_elevated) = token.impersonate(|| {
            (
                current_user_sid_string(),
                current_session_id(),
                current_process_is_elevated(),
            )
        });
        assert_eq!(seen_sid.unwrap(), sid, "{what}");
        assert_eq!(seen_session.unwrap(), session, "{what}");
        assert_eq!(seen_elevated.unwrap(), elevated, "{what}");
    }
}

#[test]
fn current_user_sid_is_a_canonical_sid_string() {
    let sid = current_user_sid_string().unwrap();
    assert!(sid.starts_with("S-1-"), "{sid}");
    assert!(is_valid_sid_string(&sid));
    assert_eq!(current_user_sid_hash().unwrap(), sid_hash16(&sid));
}

#[test]
fn the_current_process_belongs_to_the_current_user_and_session() {
    let pid = std::process::id();
    assert_eq!(
        process_user_sid_string(pid).unwrap(),
        current_user_sid_string().unwrap()
    );
    assert_eq!(
        process_session_id(pid).unwrap(),
        current_session_id().unwrap()
    );
}

#[test]
fn missing_processes_are_errors() {
    // PIDs are multiples of four; this one never exists.
    let error = process_user_sid_string(0xFFFF_FFF1).unwrap_err();
    assert_ne!(error.kind(), WinErrorKind::Other, "{error}");
    assert!(process_session_id(0xFFFF_FFF1).is_err());
}
