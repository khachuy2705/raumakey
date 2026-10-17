//! Entry validation and NFC normalization (plan §4).
//!
//! Triggers and replacements are stored NFC. Length limits count Unicode scalars of the NFC
//! form, so text typed or imported in decomposed form (NFD) is accepted when its composed form
//! fits.

use raumakey_model::macros::MacroEntry;
use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::error::EntryError;

/// Maximum trigger length in Unicode scalars (after NFC normalization).
pub const MAX_TRIGGER_SCALARS: usize = 64;

/// Maximum replacement length in Unicode scalars (after NFC normalization).
pub const MAX_REPLACEMENT_SCALARS: usize = 4096;

/// Upper bound on the number of scalars one scalar decomposes into (canonical decompositions
/// produce at most 4; 18 is the compatibility maximum). NFC therefore shrinks text by at most
/// this factor, which lets oversized input be rejected before normalization allocates.
const MAX_DECOMPOSITION_FACTOR: usize = 18;

/// Validates a trigger and returns its NFC form.
///
/// A trigger has 1..=[`MAX_TRIGGER_SCALARS`] scalars after NFC normalization and contains no
/// whitespace and no control character. When several rules are violated, the first one in this
/// order is reported: empty, too long, whitespace, control.
///
/// ```
/// use raumakey_macro::{EntryError, normalize_trigger};
///
/// assert_eq!(normalize_trigger("vie\u{0302}\u{0323}t").unwrap(), "việt");
/// assert_eq!(normalize_trigger("k o"), Err(EntryError::TriggerWhitespace));
/// ```
pub fn normalize_trigger(trigger: &str) -> Result<String, EntryError> {
    if trigger.is_empty() {
        return Err(EntryError::EmptyTrigger);
    }
    let nfc = bounded_nfc(trigger, MAX_TRIGGER_SCALARS).ok_or(EntryError::TriggerTooLong)?;
    if nfc.chars().any(char::is_whitespace) {
        return Err(EntryError::TriggerWhitespace);
    }
    if nfc.chars().any(char::is_control) {
        return Err(EntryError::TriggerControl);
    }
    Ok(nfc)
}

/// Validates a replacement and returns its NFC form.
///
/// A replacement has 1..=[`MAX_REPLACEMENT_SCALARS`] scalars after NFC normalization and
/// contains no NUL. Whitespace, line breaks, and other control characters are allowed. When
/// several rules are violated, the first one in this order is reported: empty, too long, NUL.
pub fn normalize_replacement(replacement: &str) -> Result<String, EntryError> {
    if replacement.is_empty() {
        return Err(EntryError::EmptyReplacement);
    }
    let nfc =
        bounded_nfc(replacement, MAX_REPLACEMENT_SCALARS).ok_or(EntryError::ReplacementTooLong)?;
    if nfc.contains('\0') {
        return Err(EntryError::ReplacementNul);
    }
    Ok(nfc)
}

/// Validates an entry and returns it with NFC trigger and replacement.
///
/// The identifier, enabled flag, boundary policy, and case policy are kept. Trigger errors are
/// reported before replacement errors.
pub fn normalize_entry(entry: MacroEntry) -> Result<MacroEntry, EntryError> {
    let trigger = normalize_trigger(&entry.trigger)?;
    let replacement = normalize_replacement(&entry.replacement)?;
    Ok(MacroEntry {
        trigger,
        replacement,
        ..entry
    })
}

/// Returns the NFC form of `text` as an owned string.
pub(crate) fn to_nfc(text: &str) -> String {
    if is_nfc(text) {
        text.to_owned()
    } else {
        text.nfc().collect()
    }
}

/// Returns true when `text` has more than `limit` scalars, inspecting at most `limit + 1`.
pub(crate) fn exceeds_scalars(text: &str, limit: usize) -> bool {
    text.chars().nth(limit).is_some()
}

/// NFC form of `text`, or `None` when it has more than `max` scalars.
fn bounded_nfc(text: &str, max: usize) -> Option<String> {
    if exceeds_scalars(text, max.saturating_mul(MAX_DECOMPOSITION_FACTOR)) {
        return None;
    }
    let nfc = to_nfc(text);
    if exceeds_scalars(&nfc, max) {
        None
    } else {
        Some(nfc)
    }
}
