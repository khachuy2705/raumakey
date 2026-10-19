//! The document port of a keyboard hook, which cannot read the application's text.

use core::fmt;
use std::borrow::Cow;

use raumakey_converter::{Charset, typed_text};
use raumakey_model::options::OutputCharset;
use raumakey_session::{DocumentPort, EditError, EditRequest};
use raumakey_windows::input::{SyntheticKey, keyboard_inputs_for_backspaces, unicode_text_inputs};

/// What a guarded edit types before its Backspaces (D49): U+202F NARROW NO-BREAK SPACE.
///
/// Typing it replaces a completion the host selected after the caret, as any typed character
/// does. It is whitespace, so a browser does not complete the text that now ends with it, and
/// no Vietnamese word holds it, so no completion of the host starts with the word and it.
pub const AUTOCOMPLETE_PLACEHOLDER: u16 = 0x202F;

/// A [`DocumentPort`] that turns the edit of a key into synthetic keys: one Backspace per
/// character to delete, then the inserted text as `KEYEVENTF_UNICODE` keys, all tagged with
/// `RAUMAKEY_INJECTED_TAG`, collected for the handler to send after the core returns.
///
/// The port types in its charset (D54, [`BlindDocument::with_charset`]): both the text it
/// deletes and the text it inserts are encoded first, so a VNI Windows `ê` (a letter and a
/// mark) takes two Backspaces. TCVN3 and VNI Windows are typed as the Windows-1252 characters
/// of their bytes, which the fonts made for them show as Vietnamese letters; Windows-1258 as
/// the characters of its code page (`ế` as `ê` and a combining acute).
///
/// The hook edits blind (design §21.2): it cannot check the text before the caret, so the
/// request's `expected` units, the text the engine owns, only say how many characters to
/// delete. A Backspace deletes one character, so a surrogate pair takes one Backspace; a
/// precomposed letter (NFC, the default normalization) is one character too, but each
/// combining mark of a decomposed one (NFD) is its own character and takes its own
/// Backspace, which applications that delete a whole grapheme cluster at once would not
/// match.
///
/// A host that completes text inline (a browser's address bar, Excel) selects the suggested
/// rest after the caret, and a Backspace then removes only that selection: the first
/// Backspace of an edit would be lost and `aa` would show `aâ`. When the request asks for the
/// [autocomplete guard](EditRequest::autocomplete_guard) (D49) and deletes anything, the keys
/// therefore start with [`AUTOCOMPLETE_PLACEHOLDER`], which replaces such a selection, and
/// take one more Backspace, which deletes the placeholder. Without a selection the
/// placeholder is typed and deleted again.
///
/// The edit is refused with [`EditError::Denied`], and nothing is collected, when it does not
/// fit the input builders (more than
/// [`MAX_BACKSPACES`](raumakey_windows::input::MAX_BACKSPACES) Backspaces, the
/// placeholder's included, more
/// than [`MAX_TEXT_UNITS`](raumakey_windows::input::MAX_TEXT_UNITS) units, or ill-formed
/// text), or when it is the second edit of the same key: the core then resets the word and
/// lets the key through.
///
/// `Debug` shows only how many keys were collected.
#[derive(Default)]
pub struct BlindDocument {
    keys: Vec<SyntheticKey>,
    edited: bool,
    charset: OutputCharset,
}

impl BlindDocument {
    /// A port with nothing collected, for one key, that types Unicode.
    pub fn new() -> Self {
        Self::default()
    }

    /// A port with nothing collected, for one key, that types in `charset` (D54).
    pub fn with_charset(charset: OutputCharset) -> Self {
        Self {
            charset,
            ..Self::default()
        }
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
        let expected = encoded(request.expected(), self.charset)?;
        let insert = encoded(request.insert(), self.charset)?;
        let characters = char::decode_utf16(expected.iter().copied()).count();
        let characters = u32::try_from(characters).map_err(|_| EditError::Denied)?;
        let guard = request.autocomplete_guard() && characters > 0;
        let (mut keys, backspaces) = if guard {
            let placeholder =
                unicode_text_inputs(&[AUTOCOMPLETE_PLACEHOLDER]).map_err(|_| EditError::Denied)?;
            let backspaces = characters.checked_add(1).ok_or(EditError::Denied)?;
            (placeholder, backspaces)
        } else {
            (Vec::new(), characters)
        };
        keys.extend(keyboard_inputs_for_backspaces(backspaces).map_err(|_| EditError::Denied)?);
        keys.extend(unicode_text_inputs(&insert).map_err(|_| EditError::Denied)?);
        self.keys = keys;
        self.edited = true;
        Ok(())
    }
}

/// `units` (well-formed UTF-16, from the core) in `charset`, as the application holds the text:
/// Unicode as it is, any other charset through `raumakey_converter::typed_text` (which reads
/// NFC; Windows-1258 as the characters of its code page). Every character is encoded on its
/// own, so the encoded text that an edit deletes is exactly the end of the encoded text before
/// it.
fn encoded(units: &[u16], charset: OutputCharset) -> Result<Cow<'_, [u16]>, EditError> {
    let target = match charset {
        OutputCharset::Unicode => return Ok(Cow::Borrowed(units)),
        OutputCharset::Tcvn3 => Charset::Tcvn3,
        OutputCharset::VniWindows => Charset::VniWindows,
        OutputCharset::UnicodeNfd => Charset::UnicodeNfd,
        OutputCharset::Cp1258 => Charset::Cp1258,
    };
    let text = String::from_utf16(units).map_err(|_| EditError::Denied)?;
    let text = typed_text(&text, target).map_err(|_| EditError::Denied)?;
    Ok(Cow::Owned(text.encode_utf16().collect()))
}

impl fmt::Debug for BlindDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlindDocument")
            .field("edited", &self.edited)
            .field("keys", &self.keys.len())
            .finish()
    }
}
