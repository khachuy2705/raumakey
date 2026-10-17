//! Typed conversion errors. No variant carries text, clipboard content, or paths.

use core::fmt;
use std::io;

/// Failure reported by a [`crate::ClipboardPort`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ClipboardError {
    /// The clipboard could not be opened (another process holds it).
    Unavailable,
    /// The clipboard content could not be read.
    ReadFailed,
    /// The clipboard content could not be replaced.
    WriteFailed,
}

impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unavailable => "clipboard is unavailable",
            Self::ReadFailed => "clipboard could not be read",
            Self::WriteFailed => "clipboard could not be written",
        })
    }
}

impl std::error::Error for ClipboardError {}

/// Why a conversion was refused or failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConvertError {
    /// The input is larger than [`crate::MAX_INPUT_BYTES`].
    InputTooLarge,
    /// The input is not valid: bytes that are not valid in the byte form of the source charset
    /// (UTF-8/UTF-16 for the Unicode-based charsets), or clipboard text that is not well-formed
    /// UTF-16.
    MalformedInput,
    /// Clipboard conversion was requested while a secure (password) field has focus.
    SecureField,
    /// The clipboard changed after it was read; nothing was written.
    OwnershipConflict,
    /// The clipboard port failed.
    Clipboard(ClipboardError),
    /// The source and destination are the same file.
    SameFile,
    /// The destination exists and overwriting was not requested.
    DestinationExists,
    /// A path cannot be used: the source is not a regular file; the destination has no file name
    /// or is a directory; or, on Windows, the destination is a device path (`\\.\…`, or `\\?\…`
    /// other than the long-path forms `\\?\C:\…` and `\\?\UNC\…`) or its file name is not one
    /// Windows stores and opens unchanged: it ends in a dot or space, names a stream
    /// (`out.txt:s`), contains a character Windows forbids in names, or is a reserved device name
    /// such as `NUL` or `COM1.txt`. [`crate::convert_file`] describes each case.
    InvalidPath,
    /// A file system operation failed.
    Io(io::ErrorKind),
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputTooLarge => f.write_str("input is too large to convert"),
            Self::MalformedInput => f.write_str("input is not valid in the source charset"),
            Self::SecureField => f.write_str("conversion is disabled in secure fields"),
            Self::OwnershipConflict => {
                f.write_str("clipboard changed during conversion; nothing was written")
            }
            Self::Clipboard(error) => write!(f, "{error}"),
            Self::SameFile => f.write_str("source and destination are the same file"),
            Self::DestinationExists => f.write_str("destination already exists"),
            Self::InvalidPath => f.write_str("path cannot be used for conversion"),
            Self::Io(kind) => write!(f, "file system error: {kind}"),
        }
    }
}

impl std::error::Error for ConvertError {}

impl From<ClipboardError> for ConvertError {
    fn from(error: ClipboardError) -> Self {
        Self::Clipboard(error)
    }
}

impl From<io::Error> for ConvertError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}
