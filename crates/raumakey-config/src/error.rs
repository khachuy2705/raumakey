//! Store error type (clean-room spec §4 `ConfigError`).

use core::fmt;
use std::io;

/// Why a configuration operation failed.
///
/// No variant carries file content, field values, or paths, so an error can be reported or
/// logged without leaking user data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ConfigError {
    /// The file or the data root does not exist.
    NotFound,
    /// The file is not valid for its format (syntax, encoding, or top-level shape) or is
    /// larger than [`AtomicStore::MAX_FILE_BYTES`](crate::AtomicStore::MAX_FILE_BYTES), or a
    /// document holds a value its file format cannot represent. A corrupt file is never
    /// overwritten implicitly.
    Corrupt,
    /// The file declares a schema version this build cannot read, or a document to be written
    /// does not carry the current schema version.
    UnsupportedVersion,
    /// The operating system denied access.
    PermissionDenied,
    /// The on-disk revision differs from the revision the writer expected (matrix #46), or
    /// (then `expected == actual`) the file changed without a new revision since the store
    /// last loaded or committed it, as a hand edit does. The writer reloads and retries.
    Conflict {
        /// Revision the writer based its change on.
        expected: u64,
        /// Revision found on disk.
        actual: u64,
    },
    /// Any other I/O failure, including a lock timeout, a failed read-back verification, and a
    /// document whose encoding is too large to store.
    IoFailure,
}

impl ConfigError {
    /// Maps an I/O error kind to a store error.
    pub fn from_io_kind(kind: io::ErrorKind) -> Self {
        match kind {
            io::ErrorKind::NotFound => Self::NotFound,
            io::ErrorKind::PermissionDenied | io::ErrorKind::ReadOnlyFilesystem => {
                Self::PermissionDenied
            }
            _ => Self::IoFailure,
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        Self::from_io_kind(error.kind())
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => f.write_str("configuration not found"),
            Self::Corrupt => f.write_str("configuration is corrupt"),
            Self::UnsupportedVersion => f.write_str("configuration version is not supported"),
            Self::PermissionDenied => f.write_str("access to configuration was denied"),
            Self::Conflict { expected, actual } if expected == actual => write!(
                f,
                "configuration changed on disk without a new revision (revision {actual})"
            ),
            Self::Conflict { expected, actual } => write!(
                f,
                "configuration revision conflict (expected {expected}, found {actual})"
            ),
            Self::IoFailure => f.write_str("configuration I/O failed"),
        }
    }
}

impl std::error::Error for ConfigError {}
