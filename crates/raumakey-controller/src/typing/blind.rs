//! The document port of a keyboard hook, which cannot read the application's text.

use core::fmt;

use raumakey_session::{DocumentPort, EditError, EditRequest};
use raumakey_windows::input::{SyntheticKey, keyboard_inputs_for_backspaces, unicode_text_inputs};

/// A [`DocumentPort`] that turns the edit of a key into synthetic keys: one Backspace per
/// character to delete, then the inserted text as `KEYEVENTF_UNICODE` keys, all tagged with
/// `RAUMAKEY_INJECTED_TAG`, collected for the handler to send after the core returns.
///
/// The hook edits blind (design §21.2): it cannot check the text before the caret, so the
/// request's `expected` units, the text the engine owns, only say how many characters to
/// delete. A Backspace deletes one character, so a surrogate pair takes one Backspace; a
/// precomposed letter (NFC, the default normalization) is one character too, but each
/// combining mark of a decomposed one (NFD) is its own character and takes its own
/// Backspace, which applications that delete a whole grapheme cluster at once would not
/// match.
///
/// The edit is refused with [`EditError::Denied`], and nothing is collected, when it does not
/// fit the input builders (more than
/// [`MAX_BACKSPACES`](raumakey_windows::input::MAX_BACKSPACES) characters to delete, more
/// than [`MAX_TEXT_UNITS`](raumakey_windows::input::MAX_TEXT_UNITS) units, or ill-formed
/// text), or when it is the second edit of the same key: the core then resets the word and
/// lets the key through.
///
/// `Debug` shows only how many keys were collected.
#[derive(Default)]
pub struct BlindDocument {
    keys: Vec<SyntheticKey>,
    edited: bool,
}

impl BlindDocument {
    /// A port with nothing collected, for one key.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether the core edited through this port.
    pub fn edited(&self) -> bool {
        self.edited
    }

    /// The keys of the edit, in sending order; empty without an edit.
    pub fn into_keys(self) -> Vec<SyntheticKey> {
        self.keys
    }
}

impl DocumentPort for BlindDocument {
    fn replace_before_caret(&mut self, request: &EditRequest<'_>) -> Result<(), EditError> {
        if self.edited {
            return Err(EditError::Denied);
        }
        let characters = char::decode_utf16(request.expected().iter().copied()).count();
        let characters = u32::try_from(characters).map_err(|_| EditError::Denied)?;
        let mut keys = keyboard_inputs_for_backspaces(characters).map_err(|_| EditError::Denied)?;
        keys.extend(unicode_text_inputs(request.insert()).map_err(|_| EditError::Denied)?);
        self.keys = keys;
        self.edited = true;
        Ok(())
    }
}

impl fmt::Debug for BlindDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlindDocument")
            .field("edited", &self.edited)
            .field("keys", &self.keys.len())
            .finish()
    }
}
