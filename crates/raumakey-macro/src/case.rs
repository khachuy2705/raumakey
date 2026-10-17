//! Case pattern detection and replacement case adaptation (plan §4, matrix D50).
//!
//! Only *cased* letters (characters with the Unicode `Lowercase` or `Uppercase` property) take
//! part in pattern detection; digits, punctuation, symbols, and uncased letters are ignored.
//! Every function here is total: it accepts any string and never panics.

use raumakey_model::macros::CasePattern;

use crate::validate::to_nfc;

/// Classifies the case shape of a typed trigger.
///
/// - No uppercase letter (including text without any cased letter) → [`CasePattern::Lower`].
/// - The first cased letter is uppercase and every other cased letter is lowercase, which
///   includes a single uppercase letter → [`CasePattern::Title`].
/// - More than one cased letter and all of them uppercase → [`CasePattern::Upper`].
/// - Anything else → [`CasePattern::Mixed`].
///
/// ```
/// use raumakey_macro::case_pattern;
/// use raumakey_model::macros::CasePattern;
///
/// assert_eq!(case_pattern("ko"), CasePattern::Lower);
/// assert_eq!(case_pattern("Ko"), CasePattern::Title);
/// assert_eq!(case_pattern("KO"), CasePattern::Upper);
/// assert_eq!(case_pattern("kO"), CasePattern::Mixed);
/// ```
pub fn case_pattern(typed: &str) -> CasePattern {
    let mut cased = typed.chars().filter_map(|c| {
        if c.is_uppercase() {
            Some(true)
        } else if c.is_lowercase() {
            Some(false)
        } else {
            None
        }
    });
    let Some(first_upper) = cased.next() else {
        return CasePattern::Lower;
    };
    let (mut rest_has_upper, mut rest_has_lower) = (false, false);
    for upper in cased {
        if upper {
            rest_has_upper = true;
        } else {
            rest_has_lower = true;
        }
    }
    match (first_upper, rest_has_upper, rest_has_lower) {
        (false, false, _) => CasePattern::Lower,
        (true, false, _) => CasePattern::Title,
        (true, true, false) => CasePattern::Upper,
        _ => CasePattern::Mixed,
    }
}

/// Adapts a stored replacement to the case pattern of the typed trigger.
///
/// - [`CasePattern::Lower`] and [`CasePattern::Mixed`] keep the replacement as stored.
/// - [`CasePattern::Title`] uppercases the first alphabetic character and keeps the rest as
///   stored, so proper nouns inside the replacement survive (`"việt Nam"` → `"Việt Nam"`).
/// - [`CasePattern::Upper`] applies the full Unicode uppercase mapping to the whole text
///   (`"straße"` → `"STRASSE"`).
///
/// The result is always NFC, because a case mapping can produce a non-NFC sequence. The function
/// is deterministic, and [`CasePattern::Upper`] is idempotent.
///
/// ```
/// use raumakey_macro::adapt_case;
/// use raumakey_model::macros::CasePattern;
///
/// assert_eq!(adapt_case("không", CasePattern::Title), "Không");
/// assert_eq!(adapt_case("không", CasePattern::Upper), "KHÔNG");
/// ```
pub fn adapt_case(replacement: &str, pattern: CasePattern) -> String {
    match pattern {
        CasePattern::Lower | CasePattern::Mixed => to_nfc(replacement),
        CasePattern::Upper => to_nfc(&replacement.to_uppercase()),
        CasePattern::Title => {
            // The first letter's uppercase mapping can have several scalars (`ß` → `SS`); every
            // other character is copied verbatim.
            let mut adapted = String::with_capacity(replacement.len());
            let mut pending = true;
            for c in replacement.chars() {
                if pending && c.is_alphabetic() {
                    pending = false;
                    adapted.extend(c.to_uppercase());
                } else {
                    adapted.push(c);
                }
            }
            to_nfc(&adapted)
        }
    }
}
