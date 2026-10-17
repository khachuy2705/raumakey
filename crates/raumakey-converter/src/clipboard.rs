//! Clipboard conversion through a platform port.

use crate::charset::Charset;
use crate::codec::{MAX_INPUT_BYTES, convert_with_report};
use crate::error::{ClipboardError, ConvertError};
use crate::options::ConvertOptions;

/// Clipboard text and the sequence number that identifies it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipboardText {
    /// The text as UTF-16 code units: the `CF_UNICODETEXT` content up to, not including, its
    /// terminating NUL. It is passed on unvalidated; [`convert_clipboard`] validates it.
    pub units: Vec<u16>,
    /// The clipboard sequence number of this content, read as [`ClipboardPort::read_text`]
    /// requires.
    pub sequence: u32,
}

/// Access to the system clipboard. The Windows adapter lives in `raumakey-windows`.
///
/// Each method must keep the clipboard open for its whole duration (on Windows, from
/// `OpenClipboard` to `CloseClipboard`): no other process can change the clipboard while it is
/// open, and [`convert_clipboard`] relies on that to never overwrite content newer than the text
/// it converted. There is deliberately no unconditional write.
///
/// # Examples
///
/// An in-memory port:
///
/// ```
/// use raumakey_converter::{
///     Charset, ClipboardError, ClipboardOutcome, ClipboardPort, ClipboardText, ConvertOptions,
///     convert_clipboard,
/// };
///
/// /// Every change advances the sequence number, as on Windows.
/// struct MemoryClipboard {
///     sequence: u32,
///     units: Vec<u16>,
/// }
///
/// impl ClipboardPort for MemoryClipboard {
///     fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError> {
///         let units = self.units.clone();
///         Ok(Some(ClipboardText { units, sequence: self.sequence }))
///     }
///
///     fn write_text_if_unchanged(
///         &mut self,
///         expected_sequence: u32,
///         units: &[u16],
///     ) -> Result<bool, ClipboardError> {
///         if self.sequence != expected_sequence {
///             return Ok(false);
///         }
///         self.units = units.to_vec();
///         self.sequence = self.sequence.wrapping_add(1);
///         Ok(true)
///     }
/// }
///
/// let mut clipboard = MemoryClipboard {
///     sequence: 1,
///     units: "TiÕng ViÖt".encode_utf16().collect(),
/// };
/// let (from, to, options) = (Charset::Tcvn3, Charset::Unicode, ConvertOptions::default());
/// let outcome = convert_clipboard(&mut clipboard, from, to, options, false);
/// assert_eq!(outcome, Ok(ClipboardOutcome::Converted { lossy: false }));
/// assert_eq!(String::from_utf16(&clipboard.units).unwrap(), "Tiếng Việt");
/// ```
///
/// The conditional write has no default, so a port that leaves it out does not compile:
///
/// ```compile_fail,E0046
/// use raumakey_converter::{ClipboardError, ClipboardPort, ClipboardText};
///
/// struct ReadOnlyClipboard;
///
/// impl ClipboardPort for ReadOnlyClipboard {
///     fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError> {
///         Ok(None)
///     }
/// }
/// ```
pub trait ClipboardPort {
    /// Reads the clipboard text; `Ok(None)` when the clipboard holds no text.
    ///
    /// The sequence number must be read after the text and before the clipboard is closed (on
    /// Windows: `OpenClipboard`, `GetClipboardData(CF_UNICODETEXT)`,
    /// `GetClipboardSequenceNumber`, `CloseClipboard`). Read after closing, it could belong to
    /// newer content from another process; read before the text, it would miss the change
    /// delayed rendering makes when the owner renders the text on request, and that conversion
    /// would end in [`ConvertError::OwnershipConflict`].
    fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError>;

    /// Replaces the clipboard content with the text `units` only if the clipboard sequence number
    /// still equals `expected_sequence`; returns whether it wrote.
    ///
    /// The comparison and the write must happen while the clipboard stays open (on Windows:
    /// `OpenClipboard`, compare `GetClipboardSequenceNumber`, and only on a match
    /// `EmptyClipboard` and `SetClipboardData(CF_UNICODETEXT)`, then `CloseClipboard`), so no
    /// other process can change the clipboard between the check and the write. The adapter adds
    /// the terminating NUL.
    fn write_text_if_unchanged(
        &mut self,
        expected_sequence: u32,
        units: &[u16],
    ) -> Result<bool, ClipboardError>;
}

/// What [`convert_clipboard`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardOutcome {
    /// The clipboard held no text; nothing was written.
    NoText,
    /// Conversion produced the same text; nothing was written.
    Unchanged,
    /// The converted text replaced the clipboard content.
    Converted {
        /// Some characters could not be represented exactly in the target charset.
        lossy: bool,
    },
}

/// Converts the clipboard text from `from` to `to`, applying `options`.
///
/// Refuses with [`ConvertError::SecureField`] before touching the clipboard when a secure field
/// has focus, and with [`ConvertError::MalformedInput`] (writing nothing) when the clipboard text
/// is not well-formed UTF-16. Otherwise it reads the text with its sequence number, converts it,
/// and writes the result only if the sequence number is still the one read with the text; if
/// another application changed the clipboard in the meantime it writes nothing and returns
/// [`ConvertError::OwnershipConflict`]. It never writes old content back (there is no restore
/// step), so newer clipboard content is never overwritten.
pub fn convert_clipboard<P: ClipboardPort + ?Sized>(
    port: &mut P,
    from: Charset,
    to: Charset,
    options: ConvertOptions,
    secure_field: bool,
) -> Result<ClipboardOutcome, ConvertError> {
    if secure_field {
        return Err(ConvertError::SecureField);
    }
    let Some(read) = port.read_text()? else {
        return Ok(ClipboardOutcome::NoText);
    };
    // A UTF-16 unit takes at least one UTF-8 byte, so this refuses only text that is too large,
    // before decoding it.
    if read.units.len() > MAX_INPUT_BYTES {
        return Err(ConvertError::InputTooLarge);
    }
    // Unpaired surrogates are refused: decoding them lossily would write U+FFFD back over them.
    let text = String::from_utf16(&read.units).map_err(|_| ConvertError::MalformedInput)?;
    let converted = convert_with_report(&text, from, to, options)?;
    if converted.text == text {
        return Ok(ClipboardOutcome::Unchanged);
    }
    let units: Vec<u16> = converted.text.encode_utf16().collect();
    if port.write_text_if_unchanged(read.sequence, &units)? {
        Ok(ClipboardOutcome::Converted {
            lossy: converted.lossy,
        })
    } else {
        Err(ConvertError::OwnershipConflict)
    }
}
