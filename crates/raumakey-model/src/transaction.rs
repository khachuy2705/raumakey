//! Edit transactions (clean-room spec §3.4) and a pure text simulator used as a test oracle.

use core::fmt;

use crate::input::{InputEvent, InputKind, ReplayKey, vk};
use crate::utf16;

/// Maximum UTF-16 units one transaction may insert.
pub const MAX_INSERT_UNITS: usize = 8192;

/// The single way the engine asks a host to change text.
///
/// `delete_before` counts host-visible UTF-16 code units before the caret.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EditTransaction {
    /// UTF-16 units to delete before the caret.
    pub delete_before: u32,
    /// Well-formed UTF-16 to insert at the caret after deleting.
    pub insert_utf16: Vec<u16>,
    /// The original event must not reach the application.
    pub consume_original: bool,
    /// Key to replay after applying the edit (requires `consume_original`).
    pub replay_key: Option<ReplayKey>,
    /// End the composition and discard engine state after applying.
    pub reset_context: bool,
}

/// Why a transaction cannot be applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransactionError {
    /// `insert_utf16` is not well-formed UTF-16.
    MalformedInsert,
    /// More units requested than the confirmed context holds.
    DeleteExceedsContext {
        /// Requested deletion.
        requested: u32,
        /// Units available.
        available: u32,
    },
    /// More units requested than host policy allows.
    DeleteExceedsPolicy {
        /// Requested deletion.
        requested: u32,
        /// Policy maximum.
        max: u32,
    },
    /// Insert longer than [`MAX_INSERT_UNITS`].
    InsertTooLong,
    /// The deletion would split a surrogate pair.
    SplitsSurrogate,
    /// `replay_key` without `consume_original`.
    ReplayWithoutConsume,
}

impl fmt::Display for TransactionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MalformedInsert => f.write_str("insert text is not well-formed UTF-16"),
            Self::DeleteExceedsContext { .. } => f.write_str("deletion exceeds available context"),
            Self::DeleteExceedsPolicy { .. } => f.write_str("deletion exceeds host policy"),
            Self::InsertTooLong => f.write_str("insert text too long"),
            Self::SplitsSurrogate => f.write_str("deletion would split a surrogate pair"),
            Self::ReplayWithoutConsume => f.write_str("replay requires consuming the original key"),
        }
    }
}

impl std::error::Error for TransactionError {}

impl EditTransaction {
    /// Let the original event through and change nothing.
    pub fn pass_through() -> Self {
        Self::default()
    }

    /// Let the original event through and end the composition/state.
    pub fn pass_through_reset() -> Self {
        Self {
            reset_context: true,
            ..Self::default()
        }
    }

    /// Consume the original event, delete `delete_before` units, and insert `insert`.
    pub fn replace(delete_before: u32, insert: &str) -> Self {
        Self {
            delete_before,
            insert_utf16: utf16::encode(insert),
            consume_original: true,
            replay_key: None,
            reset_context: false,
        }
    }

    /// True when the transaction neither edits nor consumes anything.
    pub fn is_pass_through(&self) -> bool {
        !self.consume_original
            && self.delete_before == 0
            && self.insert_utf16.is_empty()
            && self.replay_key.is_none()
    }

    /// The insert text as a string.
    pub fn insert_string(&self) -> String {
        String::from_utf16_lossy(&self.insert_utf16)
    }

    /// Validates against the confirmed context length and the host policy maximum.
    pub fn validate(&self, available_units: u32, max_delete: u32) -> Result<(), TransactionError> {
        if !utf16::is_well_formed(&self.insert_utf16) {
            return Err(TransactionError::MalformedInsert);
        }
        if self.insert_utf16.len() > MAX_INSERT_UNITS {
            return Err(TransactionError::InsertTooLong);
        }
        if self.delete_before > available_units {
            return Err(TransactionError::DeleteExceedsContext {
                requested: self.delete_before,
                available: available_units,
            });
        }
        if self.delete_before > max_delete {
            return Err(TransactionError::DeleteExceedsPolicy {
                requested: self.delete_before,
                max: max_delete,
            });
        }
        if self.replay_key.is_some() && !self.consume_original {
            return Err(TransactionError::ReplayWithoutConsume);
        }
        Ok(())
    }
}

/// Pure model of a text field with the caret at the end. Applies transactions exactly as the
/// compatibility matrix describes and is used as the test oracle by every suite.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextSimulator {
    text: Vec<u16>,
}

impl TextSimulator {
    /// Empty field.
    pub fn new() -> Self {
        Self::default()
    }

    /// Field pre-filled with `text`.
    pub fn with_text(text: &str) -> Self {
        Self {
            text: utf16::encode(text),
        }
    }

    /// Current UTF-16 content.
    pub fn units(&self) -> &[u16] {
        &self.text
    }

    /// Current content as a string.
    pub fn as_string(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    /// Applies `tx` produced for `event`. On error the text is unchanged.
    pub fn apply(
        &mut self,
        event: &InputEvent,
        tx: &EditTransaction,
    ) -> Result<(), TransactionError> {
        let available = u32::try_from(self.text.len()).unwrap_or(u32::MAX);
        tx.validate(available, u32::MAX)?;
        let cut = self.text.len() - tx.delete_before as usize;
        if !utf16::is_scalar_boundary(&self.text, cut) {
            return Err(TransactionError::SplitsSurrogate);
        }
        let mut next = self.text[..cut].to_vec();
        next.extend_from_slice(&tx.insert_utf16);
        if !tx.consume_original {
            natural_effect(&mut next, event);
        }
        if let Some(replay) = tx.replay_key {
            if event.virtual_key == Some(replay.virtual_key) {
                natural_effect(&mut next, event);
            } else {
                replay_effect(&mut next, replay.virtual_key);
            }
        }
        self.text = next;
        Ok(())
    }
}

fn natural_effect(text: &mut Vec<u16>, event: &InputEvent) {
    match event.kind {
        InputKind::Character | InputKind::Separator => text.extend_from_slice(&event.text_utf16),
        InputKind::Backspace => delete_last_scalar(text),
        _ => {}
    }
}

fn replay_effect(text: &mut Vec<u16>, virtual_key: u32) {
    match virtual_key {
        vk::RETURN => text.push(u16::from(b'\r')),
        vk::TAB => text.push(u16::from(b'\t')),
        vk::SPACE => text.push(u16::from(b' ')),
        vk::BACK => delete_last_scalar(text),
        _ => {}
    }
}

fn delete_last_scalar(text: &mut Vec<u16>) {
    let len = utf16::last_scalar_len(text);
    text.truncate(text.len() - len);
}
