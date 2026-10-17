//! The input scopes of secure fields (matrix #29, design §14).
//!
//! The COM layer reads the input scopes of the focused context (`GUID_PROP_INPUTSCOPE`, an
//! `ITfInputScope` whose `GetInputScopes` lists them) and records the answer of
//! [`is_secure`] per document. `IS_PRIVATE` (61) asks a text service not to learn from the
//! field; it is not a password scope.

/// `IS_PASSWORD`, `IS_NUMERIC_PASSWORD`, `IS_NUMERIC_PIN`, `IS_ALPHANUMERIC_PIN`, and
/// `IS_ALPHANUMERIC_PIN_SET`: the input scopes of password and PIN fields.
pub const PASSWORD_SCOPES: [i32; 5] = [31, 63, 64, 65, 66];

/// True when `scopes` contain a password or PIN scope.
pub fn is_secure(scopes: &[i32]) -> bool {
    scopes.iter().any(|scope| PASSWORD_SCOPES.contains(scope))
}
