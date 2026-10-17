//! Error type shared by the xtask commands.

use std::fmt;
use std::path::Path;

/// A task failure with a human-readable message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(String);

/// Result alias used by every task.
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Creates an error from a message.
    pub fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }

    /// Wraps an I/O error with the operation and path it concerns.
    pub fn io(action: &str, path: &Path, error: std::io::Error) -> Self {
        Self(format!("cannot {action} {}: {error}", path.display()))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self(format!("JSON error: {error}"))
    }
}
