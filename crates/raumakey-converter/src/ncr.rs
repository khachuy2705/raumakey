//! Numeric character references: `&#7871;` (decimal) and `&#x1EBF;` (hexadecimal).
//!
//! Encoding writes every non-ASCII scalar as a reference, plus an `&` that is followed by `#`
//! (otherwise it would be read back as the start of a reference). Decoding accepts both forms
//! and leaves malformed references, NUL, surrogates, and out-of-range values as literal text.

use core::fmt::Write as _;

/// Longest digit run accepted in one reference (leading zeros included).
const MAX_DIGITS: usize = 8;

/// Number base of the references written by [`encode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Radix {
    Decimal,
    Hex,
}

/// Encodes NFC `text` with references.
pub(crate) fn encode(text: &str, radix: Radix) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let starts_reference = c == '&' && chars.peek() == Some(&'#');
        if c.is_ascii() && !starts_reference {
            out.push(c);
            continue;
        }
        // Writing to a `String` cannot fail.
        let _ = match radix {
            Radix::Decimal => write!(out, "&#{};", u32::from(c)),
            Radix::Hex => write!(out, "&#x{:X};", u32::from(c)),
        };
    }
    out
}

/// Replaces every well-formed reference in `text` by its character.
pub(crate) fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(position) = rest.find("&#") {
        let (before, from_ampersand) = rest.split_at(position);
        out.push_str(before);
        match parse_reference(from_ampersand) {
            Some((c, length)) => {
                out.push(c);
                rest = from_ampersand.get(length..).unwrap_or_default();
            }
            None => {
                out.push('&');
                rest = from_ampersand.get(1..).unwrap_or_default();
            }
        }
    }
    out.push_str(rest);
    out
}

/// Parses a reference at the start of `text`, returning its character and byte length.
fn parse_reference(text: &str) -> Option<(char, usize)> {
    let body = text.strip_prefix("&#")?;
    let (radix, prefix_length, digits_and_rest): (u32, usize, &str) =
        match body.strip_prefix(['x', 'X']) {
            Some(hex) => (16, 3, hex),
            None => (10, 2, body),
        };
    let digit_count = digits_and_rest
        .bytes()
        .take(MAX_DIGITS + 1)
        .take_while(|byte| char::from(*byte).is_digit(radix))
        .count();
    if digit_count == 0 || digit_count > MAX_DIGITS {
        return None;
    }
    if digits_and_rest.as_bytes().get(digit_count) != Some(&b';') {
        return None;
    }
    let digits = digits_and_rest.get(..digit_count)?;
    let value = u32::from_str_radix(digits, radix).ok()?;
    if value == 0 {
        return None;
    }
    let c = char::from_u32(value)?;
    let length = prefix_length.checked_add(digit_count)?.checked_add(1)?;
    Some((c, length))
}
