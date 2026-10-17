//! Raumakey macro table, case adaptation, and persistence model (implementation plan §4).
//!
//! - [`MacroTable`]: immutable, validated lookup table; the engine's
//!   [`MacroLookup`](raumakey_model::macros::MacroLookup) implementation. Lookup is exact-word,
//!   NFC-based, boundary-aware, and deterministic (lowest identifier wins). A trigger typed
//!   exactly as stored inserts its replacement as stored; only a case difference adapts it.
//!   It also answers the engine's token queries for triggers with boundary punctuation
//!   (plan D28).
//! - [`MacroStore`]: the authoritative macro list with optimistic revisions;
//!   [`MacroStore::snapshot`] publishes immutable tables for concurrent readers, and
//!   [`MacroStore::insert_many`] merges a batch (such as a legacy import) in one revision.
//! - [`case_pattern`] and [`adapt_case`]: case shape of a typed trigger and the matching case
//!   adaptation of a replacement (`ko` → `không`, `Ko` → `Không`, `KO` → `KHÔNG`).
//! - [`normalize_entry`], [`normalize_trigger`], [`normalize_replacement`]: validation limits and
//!   NFC normalization applied to every stored entry. A trigger has at most
//!   [`MAX_TRIGGER_SCALARS`] (32) scalars (plan D29); a replacement at most
//!   [`MAX_REPLACEMENT_SCALARS`] scalars and must fit one engine insert with a delimiter in NFC
//!   and NFD ([`fits_one_insert`], plan D30), so every valid macro can expand.
//! - [`parse_legacy_text`], [`export_text_checked`], and [`export_text`]: the legacy
//!   `trigger:replacement` text format, with size limits on import and a report of the entries
//!   an export cannot hold.
//!
//! The crate has no platform code and never logs. Its errors, issues, and the `Debug` output of
//! its own types carry identifiers, counts, line numbers, and reason codes only, never macro
//! triggers or replacements. Every function accepts arbitrary input without panicking.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod case;
pub mod error;
pub mod legacy;
pub mod store;
pub mod table;
pub mod validate;

pub use case::{adapt_case, case_pattern};
pub use error::{EntryError, MacroError};
pub use legacy::{
    LegacyEncoding, LegacyExport, LegacyIssue, LegacyIssueReason, LegacyMacroImport,
    MAX_LEGACY_MACRO_LINES, MAX_LEGACY_TEXT_BYTES, detect_encoding, export_text,
    export_text_checked, is_legacy_exportable, parse_legacy_text,
};
pub use store::{EntryIssue, MacroStore};
pub use table::{MAX_LOOKUP_INPUT_SCALARS, MacroTable};
pub use validate::{
    MAX_REPLACEMENT_SCALARS, MAX_TRIGGER_SCALARS, fits_one_insert, normalize_entry,
    normalize_replacement, normalize_trigger,
};
