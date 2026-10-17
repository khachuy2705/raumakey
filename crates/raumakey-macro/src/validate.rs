//! Entry validation and NFC normalization (plan §4).
//!
//! Triggers and replacements are stored NFC. Length limits count Unicode scalars of the NFC
//! form, so text typed or imported in decomposed form (NFD) is accepted when its composed form
//! fits. A replacement must also fit one engine insert with a delimiter in either output
//! normalization (plan D30, [`fits_one_insert`]).

use raumakey_model::macros::MacroEntry;
use raumakey_model::transaction::MAX_INSERT_UNITS;
use unicode_normalization::{UnicodeNormalization, is_nfc};

use crate::error::EntryError;

/// Maximum trigger length in Unicode scalars (after NFC normalization): 32, the default
/// context limit of the engine (plan D29). A trigger typed one key per scalar then fits the
/// word the engine tracks with the defaults; a trigger that takes more keys than the
/// configured context limit (marks and tones add keys) still cannot expand.
pub const MAX_TRIGGER_SCALARS: usize = 32;

/// Maximum replacement length in Unicode scalars (after NFC normalization). A replacement must
/// also fit one insert ([`fits_one_insert`]), which is the tighter limit for astral and
/// decomposing text.
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
/// A replacement has 1..=[`MAX_REPLACEMENT_SCALARS`] scalars after NFC normalization, fits one
/// insert with a delimiter in NFC and in NFD ([`fits_one_insert`], plan D30), and contains no
/// NUL. Whitespace, line breaks, and other control characters are allowed. When several rules
/// are violated, the first one in this order is reported: empty, too long (either limit), NUL.
///
/// ```
/// use raumakey_macro::{EntryError, normalize_replacement};
///
/// // "ệ" is one UTF-16 unit in NFC and three in NFD: 2,731 of them are 8,193 units in NFD.
/// assert!(normalize_replacement(&"ệ".repeat(2730)).is_ok());
/// assert_eq!(
///     normalize_replacement(&"ệ".repeat(2731)),
///     Err(EntryError::ReplacementTooLong)
/// );
/// ```
pub fn normalize_replacement(replacement: &str) -> Result<String, EntryError> {
    if replacement.is_empty() {
        return Err(EntryError::EmptyReplacement);
    }
    let nfc =
        bounded_nfc(replacement, MAX_REPLACEMENT_SCALARS).ok_or(EntryError::ReplacementTooLong)?;
    if !fits_one_insert(&nfc) {
        return Err(EntryError::ReplacementTooLong);
    }
    if nfc.contains('\0') {
        return Err(EntryError::ReplacementNul);
    }
    Ok(nfc)
}

/// Whether `text` and one delimiter unit fit one engine insert of
/// [`MAX_INSERT_UNITS`] UTF-16 units in NFC and in NFD output (plan D30, §2.7).
///
/// The engine inserts a replacement in the configured normalization followed by a printable
/// delimiter (or alone, before a replayed Enter or Tab), so a replacement that passes this
/// check always expands; `Preserve` output is the stored NFC text. The case adaptation of
/// [`MacroTable`](crate::MacroTable) keeps the stored text when its result would not pass.
pub fn fits_one_insert(text: &str) -> bool {
    let nfc: usize = text.nfc().map(char::len_utf16).sum();
    let nfd: usize = text.nfd().map(char::len_utf16).sum();
    nfc.max(nfd) < MAX_INSERT_UNITS
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
