//! Legacy 8-bit charsets: TCVN3 (VN3), VNI Windows, VISCII, and Windows-1258.
//!
//! Each charset works on [`Unit`]s: raw bytes, plus (in text form only) characters that have no
//! Windows-1252 byte and therefore cannot stand for a legacy byte. Such characters pass through
//! unchanged in text form and are replaced by `?` in byte form.
//!
//! Encoding rule for a character that is not a letter of the charset: it is written as its
//! Windows-1252 byte when that byte is *free* in the charset (decodes back to the same
//! character); otherwise it is replaced by `?` and the conversion is reported as lossy.

mod tcvn3;
mod viscii;
mod vni;
mod windows1258;

use crate::codepage::{cp1252_to_byte, cp1252_to_char};

/// The legacy charsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Legacy {
    Tcvn3,
    VniWindows,
    Viscii,
    Cp1258,
}

/// One element of legacy text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unit {
    /// A byte of the charset.
    Byte(u8),
    /// A character with no Windows-1252 byte (text form only).
    Char(char),
}

/// Output of an encoder.
pub(crate) trait Sink {
    /// Writes one byte of the target charset.
    fn byte(&mut self, byte: u8);
    /// Writes a character that has no Windows-1252 byte; returns false when it was replaced.
    fn unmapped(&mut self, c: char) -> bool;
}

/// Text form: bytes become their Windows-1252 characters, unmapped characters are kept.
#[derive(Debug, Default)]
pub(crate) struct TextSink(pub(crate) String);

impl Sink for TextSink {
    fn byte(&mut self, byte: u8) {
        self.0.push(cp1252_to_char(byte));
    }

    fn unmapped(&mut self, c: char) -> bool {
        self.0.push(c);
        true
    }
}

/// Byte form: unmapped characters become `?`.
#[derive(Debug, Default)]
pub(crate) struct ByteSink(pub(crate) Vec<u8>);

impl Sink for ByteSink {
    fn byte(&mut self, byte: u8) {
        self.0.push(byte);
    }

    fn unmapped(&mut self, _c: char) -> bool {
        self.0.push(b'?');
        false
    }
}

/// The units of the text form of legacy bytes.
pub(crate) fn text_units(text: &str) -> impl Iterator<Item = Unit> + '_ {
    text.chars()
        .map(|c| cp1252_to_byte(c).map_or(Unit::Char(c), Unit::Byte))
}

/// The units of raw legacy bytes.
pub(crate) fn byte_units(bytes: &[u8]) -> impl Iterator<Item = Unit> + '_ {
    bytes.iter().copied().map(Unit::Byte)
}

impl Legacy {
    /// Decodes units to Unicode (not yet normalized).
    pub(crate) fn decode(self, units: impl Iterator<Item = Unit>) -> String {
        let table: fn(u8) -> char = match self {
            Self::VniWindows => return vni::decode(units),
            Self::Tcvn3 => tcvn3::decode_byte,
            Self::Viscii => viscii::decode_byte,
            Self::Cp1258 => windows1258::decode_byte,
        };
        units
            .map(|unit| match unit {
                Unit::Byte(byte) => table(byte),
                Unit::Char(c) => c,
            })
            .collect()
    }

    /// Encodes NFC `text`; returns true when anything was lost.
    pub(crate) fn encode(self, text: &str, sink: &mut impl Sink) -> bool {
        let mut lossy = false;
        for c in text.chars() {
            if !self.encode_char(c, sink) {
                lossy = true;
            }
        }
        lossy
    }

    /// Encodes one character; returns false when it could not be represented exactly.
    fn encode_char(self, c: char, sink: &mut impl Sink) -> bool {
        let handled = match self {
            Self::Tcvn3 => tcvn3::encode_letter(c, sink),
            Self::VniWindows => vni::encode_letter(c, sink),
            Self::Viscii => viscii::encode_letter(c, sink),
            Self::Cp1258 => windows1258::encode_char(c, sink),
        };
        if let Some(exact) = handled {
            return exact;
        }
        match cp1252_to_byte(c) {
            Some(byte) if self.is_free(byte) => {
                sink.byte(byte);
                true
            }
            Some(_) => {
                sink.byte(b'?');
                false
            }
            None => sink.unmapped(c),
        }
    }

    /// True when the byte, alone, decodes to its Windows-1252 character and never combines.
    fn is_free(self, byte: u8) -> bool {
        match self {
            Self::Tcvn3 => tcvn3::is_free(byte),
            Self::VniWindows => vni::is_free(byte),
            Self::Viscii => viscii::is_free(byte),
            Self::Cp1258 => windows1258::is_free(byte),
        }
    }
}
