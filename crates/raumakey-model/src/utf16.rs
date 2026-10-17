//! UTF-16 validation and boundary helpers.

use core::fmt;

/// Position of the first ill-formed code unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Utf16Error {
    /// Index of the offending code unit.
    pub index: usize,
}

impl fmt::Display for Utf16Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ill-formed UTF-16 at unit {}", self.index)
    }
}

impl std::error::Error for Utf16Error {}

/// Returns true for a high (leading) surrogate.
pub const fn is_high_surrogate(unit: u16) -> bool {
    matches!(unit, 0xD800..=0xDBFF)
}

/// Returns true for a low (trailing) surrogate.
pub const fn is_low_surrogate(unit: u16) -> bool {
    matches!(unit, 0xDC00..=0xDFFF)
}

/// Validates that `units` is well-formed UTF-16 (no unpaired surrogates).
pub fn validate(units: &[u16]) -> Result<(), Utf16Error> {
    let mut index = 0;
    while index < units.len() {
        let unit = units[index];
        if is_high_surrogate(unit) {
            match units.get(index + 1) {
                Some(&next) if is_low_surrogate(next) => index += 2,
                _ => return Err(Utf16Error { index }),
            }
        } else if is_low_surrogate(unit) {
            return Err(Utf16Error { index });
        } else {
            index += 1;
        }
    }
    Ok(())
}

/// Returns true when `units` is well-formed UTF-16.
pub fn is_well_formed(units: &[u16]) -> bool {
    validate(units).is_ok()
}

/// Returns true when a cut at `index` does not split a surrogate pair.
/// `index == units.len()` is a boundary; indices past the end are not.
pub fn is_scalar_boundary(units: &[u16], index: usize) -> bool {
    if index > units.len() {
        return false;
    }
    if index == 0 || index == units.len() {
        return true;
    }
    !(is_high_surrogate(units[index - 1]) && is_low_surrogate(units[index]))
}

/// Number of code points; each unpaired surrogate counts as one.
pub fn scalar_count(units: &[u16]) -> usize {
    char::decode_utf16(units.iter().copied()).count()
}

/// Length in units of the last code point (0 for empty input).
pub fn last_scalar_len(units: &[u16]) -> usize {
    match units {
        [] => 0,
        [.., high, low] if is_high_surrogate(*high) && is_low_surrogate(*low) => 2,
        _ => 1,
    }
}

/// Returns the character when `units` encodes exactly one well-formed scalar.
pub fn single_scalar(units: &[u16]) -> Option<char> {
    let mut decoded = char::decode_utf16(units.iter().copied());
    let first = decoded.next()?.ok()?;
    if decoded.next().is_some() {
        return None;
    }
    Some(first)
}

/// Encodes a string as UTF-16.
pub fn encode(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

/// Decodes well-formed UTF-16.
pub fn decode(units: &[u16]) -> Result<String, Utf16Error> {
    validate(units)?;
    String::from_utf16(units).map_err(|_| Utf16Error { index: 0 })
}
