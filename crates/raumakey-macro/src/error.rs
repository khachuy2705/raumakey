//! Typed macro errors. No variant carries a trigger, a replacement, or any other user text, so
//! errors can be shown, logged as codes, or sent over IPC without leaking macro content.

use core::fmt;

use raumakey_model::macros::MacroId;
use serde::{Deserialize, Serialize};

/// Why a trigger or replacement failed validation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EntryError {
    /// The trigger is empty.
    EmptyTrigger,
    /// The trigger has more than [`MAX_TRIGGER_SCALARS`](crate::validate::MAX_TRIGGER_SCALARS)
    /// scalars after NFC normalization.
    TriggerTooLong,
    /// The trigger contains a whitespace character.
    TriggerWhitespace,
    /// The trigger contains a control character.
    TriggerControl,
    /// The replacement is empty.
    EmptyReplacement,
    /// The replacement has more than
    /// [`MAX_REPLACEMENT_SCALARS`](crate::validate::MAX_REPLACEMENT_SCALARS) scalars after NFC
    /// normalization.
    ReplacementTooLong,
    /// The replacement contains NUL.
    ReplacementNul,
}

impl EntryError {
    /// Stable reason code (kebab-case), suitable for diagnostics and migration records.
    pub const fn code(self) -> &'static str {
        match self {
            Self::EmptyTrigger => "empty-trigger",
            Self::TriggerTooLong => "trigger-too-long",
            Self::TriggerWhitespace => "trigger-whitespace",
            Self::TriggerControl => "trigger-control",
            Self::EmptyReplacement => "empty-replacement",
            Self::ReplacementTooLong => "replacement-too-long",
            Self::ReplacementNul => "replacement-nul",
        }
    }
}

impl fmt::Display for EntryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyTrigger => "macro trigger is empty",
            Self::TriggerTooLong => "macro trigger is too long",
            Self::TriggerWhitespace => "macro trigger contains whitespace",
            Self::TriggerControl => "macro trigger contains a control character",
            Self::EmptyReplacement => "macro replacement is empty",
            Self::ReplacementTooLong => "macro replacement is too long",
            Self::ReplacementNul => "macro replacement contains NUL",
        })
    }
}

impl std::error::Error for EntryError {}

/// Macro table or store failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MacroError {
    /// An entry failed validation.
    InvalidEntry {
        /// Identifier of the rejected entry.
        id: MacroId,
        /// The first violated rule.
        reason: EntryError,
    },
    /// Two entries share an identifier.
    DuplicateId(MacroId),
    /// The caller's expected revision is stale; nothing was changed.
    Conflict {
        /// Revision the caller based its change on.
        expected: u64,
        /// Current revision.
        actual: u64,
    },
    /// No entry has this identifier.
    NotFound(MacroId),
    /// No identifier is left above the largest one in use.
    IdSpaceExhausted,
    /// The revision counter cannot be incremented.
    RevisionExhausted,
}

impl fmt::Display for MacroError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidEntry { id, reason } => write!(f, "macro {}: {reason}", id.0),
            Self::DuplicateId(id) => write!(f, "macro id {} is used more than once", id.0),
            Self::Conflict { expected, actual } => write!(
                f,
                "macro revision conflict: expected {expected}, current {actual}"
            ),
            Self::NotFound(id) => write!(f, "macro {} does not exist", id.0),
            Self::IdSpaceExhausted => f.write_str("no macro id is available"),
            Self::RevisionExhausted => f.write_str("macro revision cannot be incremented"),
        }
    }
}

impl std::error::Error for MacroError {}
