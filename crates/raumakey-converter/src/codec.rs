//! Text and byte conversion entry points.

use unicode_normalization::{IsNormalized, UnicodeNormalization, is_nfc_quick};

use crate::charset::Charset;
use crate::error::ConvertError;
use crate::legacy::{ByteSink, Legacy, TextSink, byte_units, text_units};
use crate::ncr::{self, Radix};
use crate::options::ConvertOptions;
use crate::{utf8_literal, viqr};

/// Largest input, in bytes (UTF-8 for text), any conversion accepts.
pub const MAX_INPUT_BYTES: usize = 16 * 1024 * 1024;

/// Encoded text and whether anything was lost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Conversion {
    /// The converted text.
    pub text: String,
    /// True when at least one character could not be represented exactly in the target charset
    /// (it was replaced by `?`, or a TCVN3 uppercase toned letter was written in lowercase).
    pub lossy: bool,
}

/// Decodes `text` written in `from` to Unicode NFC.
///
/// Decoding never fails on content: anything that is not meaningful in `from` is kept as it is.
pub fn decode(text: &str, from: Charset) -> Result<String, ConvertError> {
    check_size(text.len())?;
    Ok(decode_text(text, from))
}

/// Encodes Unicode `text` in `to`. See [`encode_with_report`] for loss reporting.
pub fn encode(text: &str, to: Charset) -> Result<String, ConvertError> {
    encode_with_report(text, to).map(|conversion| conversion.text)
}

/// Encodes Unicode `text` in `to` and reports whether anything was lost.
///
/// The input is normalized to NFC first (NFD for [`Charset::UnicodeNfd`]).
pub fn encode_with_report(text: &str, to: Charset) -> Result<Conversion, ConvertError> {
    check_size(text.len())?;
    Ok(encode_text(text, to))
}

/// Converts `text` from `from` to `to`, applying `options` in between.
pub fn convert(
    text: &str,
    from: Charset,
    to: Charset,
    options: ConvertOptions,
) -> Result<String, ConvertError> {
    convert_with_report(text, from, to, options).map(|conversion| conversion.text)
}

/// Converts `text` from `from` to `to`, applying `options` in between, and reports whether
/// anything was lost.
pub fn convert_with_report(
    text: &str,
    from: Charset,
    to: Charset,
    options: ConvertOptions,
) -> Result<Conversion, ConvertError> {
    check_size(text.len())?;
    let unicode = options.apply(decode_text(text, from));
    Ok(encode_text(&unicode, to))
}

/// Encoded bytes and whether anything was lost.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ByteConversion {
    /// The converted bytes.
    pub bytes: Vec<u8>,
    /// True when at least one character could not be represented exactly in the target charset
    /// (it was replaced by `?`, or a TCVN3 uppercase toned letter was written in lowercase).
    pub lossy: bool,
}

/// Decodes the byte form of `from` (a file's content) to Unicode NFC.
///
/// Legacy charsets read raw bytes. The other charsets read Unicode text: UTF-8, with or without
/// a byte order mark, or UTF-16 with a byte order mark; anything else is
/// [`ConvertError::MalformedInput`].
pub fn decode_bytes(bytes: &[u8], from: Charset) -> Result<String, ConvertError> {
    check_size(bytes.len())?;
    decode_bytes_unchecked(bytes, from)
}

/// Encodes Unicode `text` in the byte form of `to`. See [`encode_bytes_with_report`].
pub fn encode_bytes(text: &str, to: Charset) -> Result<Vec<u8>, ConvertError> {
    encode_bytes_with_report(text, to).map(|conversion| conversion.bytes)
}

/// Encodes Unicode `text` in the byte form of `to` and reports whether anything was lost.
///
/// Legacy charsets write raw bytes, replacing characters they cannot represent with `?`. The
/// other charsets write their text form as UTF-8 without a byte order mark.
pub fn encode_bytes_with_report(text: &str, to: Charset) -> Result<ByteConversion, ConvertError> {
    check_size(text.len())?;
    Ok(encode_bytes_unchecked(text, to))
}

/// Converts the byte form of `from` to the byte form of `to`, applying `options` in between.
pub fn convert_bytes(
    bytes: &[u8],
    from: Charset,
    to: Charset,
    options: ConvertOptions,
) -> Result<ByteConversion, ConvertError> {
    check_size(bytes.len())?;
    let unicode = options.apply(decode_bytes_unchecked(bytes, from)?);
    Ok(encode_bytes_unchecked(&unicode, to))
}

const fn legacy_of(charset: Charset) -> Option<Legacy> {
    match charset {
        Charset::Tcvn3 => Some(Legacy::Tcvn3),
        Charset::VniWindows => Some(Legacy::VniWindows),
        Charset::Viscii => Some(Legacy::Viscii),
        Charset::Cp1258 => Some(Legacy::Cp1258),
        Charset::Unicode
        | Charset::UnicodeNfd
        | Charset::Viqr
        | Charset::NcrDecimal
        | Charset::NcrHex
        | Charset::Utf8Literal => None,
    }
}

fn decode_bytes_unchecked(bytes: &[u8], from: Charset) -> Result<String, ConvertError> {
    match legacy_of(from) {
        Some(legacy) => Ok(nfc(&legacy.decode(byte_units(bytes)))),
        None => Ok(decode_text(&unicode_from_bytes(bytes)?, from)),
    }
}

fn encode_bytes_unchecked(text: &str, to: Charset) -> ByteConversion {
    match legacy_of(to) {
        Some(legacy) => {
            let mut sink = ByteSink::default();
            let lossy = legacy.encode(&nfc(text), &mut sink);
            ByteConversion {
                bytes: sink.0,
                lossy,
            }
        }
        None => {
            let Conversion { text, lossy } = encode_text(text, to);
            // A leading U+FEFF would be read back as a byte order mark; keep it by writing one.
            let bytes = if text.starts_with('\u{FEFF}') {
                [UTF8_BOM.as_slice(), text.as_bytes()].concat()
            } else {
                text.into_bytes()
            };
            ByteConversion { bytes, lossy }
        }
    }
}

const UTF8_BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];

/// Reads Unicode text: UTF-8 (optionally with a byte order mark) or UTF-16 with one.
fn unicode_from_bytes(bytes: &[u8]) -> Result<String, ConvertError> {
    if let Some(rest) = bytes.strip_prefix(&UTF8_BOM) {
        return utf8(rest);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return utf16(rest, u16::from_le_bytes);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return utf16(rest, u16::from_be_bytes);
    }
    utf8(bytes)
}

fn utf8(bytes: &[u8]) -> Result<String, ConvertError> {
    core::str::from_utf8(bytes)
        .map(str::to_owned)
        .map_err(|_| ConvertError::MalformedInput)
}

fn utf16(bytes: &[u8], unit: fn([u8; 2]) -> u16) -> Result<String, ConvertError> {
    let (pairs, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(ConvertError::MalformedInput);
    }
    char::decode_utf16(pairs.iter().copied().map(unit))
        .collect::<Result<String, _>>()
        .map_err(|_| ConvertError::MalformedInput)
}

pub(crate) fn check_size(length: usize) -> Result<(), ConvertError> {
    if length > MAX_INPUT_BYTES {
        Err(ConvertError::InputTooLarge)
    } else {
        Ok(())
    }
}

pub(crate) fn nfc(text: &str) -> String {
    if is_nfc_quick(text.chars()) == IsNormalized::Yes {
        text.to_owned()
    } else {
        text.nfc().collect()
    }
}

pub(crate) fn decode_text(text: &str, from: Charset) -> String {
    let decoded = match from {
        Charset::Unicode | Charset::UnicodeNfd => return nfc(text),
        Charset::NcrDecimal | Charset::NcrHex => ncr::decode(text),
        Charset::Utf8Literal => utf8_literal::decode(text),
        Charset::Viqr => viqr::decode(text),
        Charset::Tcvn3 => Legacy::Tcvn3.decode(text_units(text)),
        Charset::VniWindows => Legacy::VniWindows.decode(text_units(text)),
        Charset::Viscii => Legacy::Viscii.decode(text_units(text)),
        Charset::Cp1258 => Legacy::Cp1258.decode(text_units(text)),
    };
    nfc(&decoded)
}

pub(crate) fn encode_text(text: &str, to: Charset) -> Conversion {
    let exact = |text: String| Conversion { text, lossy: false };
    match to {
        Charset::Unicode => exact(nfc(text)),
        Charset::UnicodeNfd => exact(text.nfd().collect()),
        Charset::NcrDecimal => exact(ncr::encode(&nfc(text), Radix::Decimal)),
        Charset::NcrHex => exact(ncr::encode(&nfc(text), Radix::Hex)),
        Charset::Utf8Literal => exact(utf8_literal::encode(&nfc(text))),
        Charset::Viqr => exact(viqr::encode(&nfc(text))),
        Charset::Tcvn3 => encode_legacy(Legacy::Tcvn3, &nfc(text)),
        Charset::VniWindows => encode_legacy(Legacy::VniWindows, &nfc(text)),
        Charset::Viscii => encode_legacy(Legacy::Viscii, &nfc(text)),
        Charset::Cp1258 => encode_legacy(Legacy::Cp1258, &nfc(text)),
    }
}

fn encode_legacy(legacy: Legacy, text: &str) -> Conversion {
    let mut sink = TextSink::default();
    let lossy = legacy.encode(text, &mut sink);
    Conversion {
        text: sink.0,
        lossy,
    }
}
