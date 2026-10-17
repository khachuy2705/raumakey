//! Password input scopes (matrix #29: secure fields bypass every transformation).

use raumakey_tip::adapter::scope::{PASSWORD_SCOPES, is_secure};

/// `IS_DEFAULT`, `IS_URL`, `IS_EMAIL_USERNAME`, `IS_LOGINNAME`, `IS_NUMBER`, and
/// `IS_PRIVATE` (no learning; not a password).
const ORDINARY: [i32; 6] = [0, 1, 4, 6, 29, 61];

#[test]
fn password_and_pin_scopes_are_secure_fields() {
    // IS_PASSWORD, IS_NUMERIC_PASSWORD, IS_NUMERIC_PIN, IS_ALPHANUMERIC_PIN,
    // IS_ALPHANUMERIC_PIN_SET.
    assert_eq!(PASSWORD_SCOPES, [31, 63, 64, 65, 66]);
    for scope in PASSWORD_SCOPES {
        assert!(is_secure(&[scope]), "{scope}");
    }
}

#[test]
fn other_scopes_are_not_secure_fields() {
    for scope in ORDINARY {
        assert!(!is_secure(&[scope]), "{scope}");
    }
    assert!(!is_secure(&ORDINARY));
}

#[test]
fn a_password_scope_anywhere_in_the_list_makes_a_secure_field() {
    assert!(is_secure(&[0, 1, 31]));
    assert!(is_secure(&[64, 0]));
}

#[test]
fn a_field_without_scopes_is_not_a_secure_field() {
    assert!(!is_secure(&[]));
}
