//! Typed migration errors.

use core::fmt;
use std::io;

use raumakey_config::ConfigError;
use raumakey_model::diag::DiagCode;

/// A Raumakey document that a migration reads and writes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DocumentKind {
    /// `config.toml`.
    Config,
    /// `macros.toml`.
    Macros,
    /// `app_rules.toml`.
    AppRules,
    /// `migration.json`.
    Record,
}

impl DocumentKind {
    /// File name of the document inside the data root.
    pub const fn file_name(self) -> &'static str {
        match self {
            Self::Config => "config.toml",
            Self::Macros => "macros.toml",
            Self::AppRules => "app_rules.toml",
            Self::Record => "migration.json",
        }
    }
}

impl fmt::Display for DocumentKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.file_name())
    }
}

/// Why a migration step failed.
///
/// No variant carries source content, setting values, macro text, or paths, so an error can be
/// shown or logged (through [`MigrateError::diag_code`]) without leaking user data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrateError {
    /// A read-only source port failed (access denied, sharing violation, ...).
    SourceIo(io::ErrorKind),
    /// A source file is larger than the reader accepts
    /// ([`MAX_SOURCE_BYTES`](crate::MAX_SOURCE_BYTES) for [`StdSourceFs`](crate::StdSourceFs)).
    ///
    /// [`discover`](fn@crate::discover) does not fail with this error: it records the file as a
    /// source whose content is quarantined.
    SourceTooLarge {
        /// Size of the file in bytes.
        size: u64,
    },
    /// The parsed migration names no source, so there is nothing to import.
    NoSources,
    /// A Raumakey document could not be loaded or committed.
    Store {
        /// The document.
        document: DocumentKind,
        /// The store error.
        error: ConfigError,
    },
    /// A committed document did not read back with the expected revision and checksum.
    Verification {
        /// The document.
        document: DocumentKind,
    },
    /// The macro document has no identifier left for a new entry.
    MacroIdsExhausted,
    /// The migration failed after it started writing, and a document could not be restored
    /// (another writer changed it, or its store failed). The migration record says
    /// [`MigrationStatus::Failed`](raumakey_config::MigrationStatus::Failed) when it could be
    /// written.
    RollbackFailed {
        /// The failure that started the rollback.
        cause: Box<MigrateError>,
        /// The first document that could not be restored.
        document: DocumentKind,
    },
}

impl MigrateError {
    /// The error a source port reports for an I/O failure of kind `kind`.
    pub fn from_io_kind(kind: io::ErrorKind) -> Self {
        Self::SourceIo(kind)
    }

    /// Stable diagnostic code (`8000..=8099`) for the typed diagnostics log
    /// ([`Component::Migrate`](raumakey_model::diag::Component::Migrate)).
    pub const fn diag_code(&self) -> DiagCode {
        DiagCode(match self {
            Self::SourceIo(_) => 8001,
            Self::SourceTooLarge { .. } => 8002,
            Self::NoSources => 8003,
            Self::Store { .. } => 8004,
            Self::Verification { .. } => 8005,
            Self::MacroIdsExhausted => 8006,
            Self::RollbackFailed { .. } => 8007,
        })
    }
}

impl fmt::Display for MigrateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceIo(kind) => write!(f, "an EVKey source could not be read: {kind}"),
            Self::SourceTooLarge { size } => {
                write!(f, "an EVKey source is too large ({size} bytes)")
            }
            Self::NoSources => f.write_str("no EVKey data was found"),
            Self::Store { document, error } => write!(f, "{document}: {error}"),
            Self::Verification { document } => {
                write!(f, "{document} did not verify after the commit")
            }
            Self::MacroIdsExhausted => f.write_str("no macro identifier is left"),
            Self::RollbackFailed { cause, document } => {
                write!(f, "{cause}; restoring {document} failed")
            }
        }
    }
}

impl std::error::Error for MigrateError {}
