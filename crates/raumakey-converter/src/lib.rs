//! Raumakey Vietnamese charset, clipboard, and file text conversion.
//!
//! Every conversion goes through Unicode NFC: [`decode`] reads text written in a [`Charset`],
//! [`ConvertOptions`] change case or remove diacritics, and [`encode`] writes the result in the
//! target charset. [`convert`] does all three.
//!
//! # Text form and byte form
//!
//! The legacy 8-bit charsets (TCVN3, VNI Windows, VISCII, Windows-1258) reach Unicode text,
//! the clipboard, and documents as the Windows-1252 characters of their bytes (TCVN3
//! `TiÕng ViÖt` is "Tiếng Việt"). The text functions use that form; [`decode_bytes`],
//! [`encode_bytes`], and [`convert_bytes`] use the raw bytes (for files). Characters without a
//! Windows-1252 byte cannot stand for a legacy byte, so text form passes them through
//! unchanged while byte form writes `?`.
//!
//! # Loss
//!
//! Decoding never loses information. Encoding is lossy when the target charset cannot represent
//! a character: it is written as `?` (or, for a TCVN3 uppercase toned letter, as the lowercase
//! letter, because TCVN3 has no such codes). The `*_with_report` functions, [`ClipboardOutcome`],
//! and [`FileOutcome`] report it.
//!
//! # Safety and privacy
//!
//! [`convert_clipboard`] refuses secure fields and never writes over clipboard content that
//! changed after it was read; [`convert_file`] never writes the source file and never replaces
//! an existing file unless asked. Inputs are limited to [`MAX_INPUT_BYTES`]. Nothing in this
//! crate logs, and no error carries text, clipboard content, or paths.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod charset;
mod clipboard;
mod codec;
mod codepage;
mod error;
mod file;
mod legacy;
mod letters;
mod ncr;
mod options;
mod utf8_literal;
mod viqr;

pub use charset::{Charset, ParseCharsetError};
pub use clipboard::{ClipboardOutcome, ClipboardPort, ClipboardText, convert_clipboard};
pub use codec::{
    ByteConversion, Conversion, MAX_INPUT_BYTES, convert, convert_bytes, convert_with_report,
    decode, decode_bytes, encode, encode_bytes, encode_bytes_with_report, encode_with_report,
    typed_text,
};
pub use error::{ClipboardError, ConvertError};
pub use file::{FileOutcome, convert_file};
pub use options::{CaseTransform, ConvertOptions, ParseCaseTransformError};
