//! Read-only source ports (plan §9).
//!
//! The importer reads EVKey data only through [`LegacyRegistry`] and [`SourceFs`]. Neither
//! trait has a method that writes, renames, deletes, or changes permissions, so a migration
//! cannot modify its sources whatever it does with them. A Windows adapter of
//! [`LegacyRegistry`] must open keys below `HKEY_CURRENT_USER` with read access only (it lives
//! outside this platform-independent crate); [`StdSourceFs`] opens files with read access only.

use core::fmt;
use std::fs::{self, File};
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use crate::error::MigrateError;

/// Registry type code of a string value (`REG_SZ`).
pub const REG_SZ: u32 = 1;
/// Registry type code of a string value with environment references (`REG_EXPAND_SZ`).
pub const REG_EXPAND_SZ: u32 = 2;
/// Registry type code of a binary value (`REG_BINARY`).
pub const REG_BINARY: u32 = 3;
/// Registry type code of a little-endian 32-bit value (`REG_DWORD`).
pub const REG_DWORD: u32 = 4;
/// Registry type code of a string list (`REG_MULTI_SZ`).
pub const REG_MULTI_SZ: u32 = 7;
/// Registry type code of a little-endian 64-bit value (`REG_QWORD`).
pub const REG_QWORD: u32 = 11;

/// Largest source file [`StdSourceFs`] reads (16 MiB). EVKey settings and macro files are far
/// smaller; a larger file is reported as [`MigrateError::SourceTooLarge`] without being read,
/// and [`discover`](fn@crate::discover) quarantines its content.
pub const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;

/// One registry value as read, with its registry type.
///
/// An adapter returns string data that is not well-formed UTF-16 (an odd byte count, an
/// unpaired surrogate) as [`LegacyValue::Other`] with the original type code and bytes; the
/// importer then decodes it itself or quarantines it as undecodable.
///
/// The [`fmt::Debug`] output shows the type and the data size, never the data.
#[derive(Clone, PartialEq, Eq)]
pub enum LegacyValue {
    /// `REG_SZ`, without the terminating NUL.
    String(String),
    /// `REG_EXPAND_SZ`, without the terminating NUL; references are not expanded.
    ExpandString(String),
    /// `REG_MULTI_SZ`: the strings of the list, without separators and terminators.
    MultiString(Vec<String>),
    /// `REG_DWORD`.
    Dword(u32),
    /// `REG_QWORD`.
    Qword(u64),
    /// `REG_BINARY`.
    Binary(Vec<u8>),
    /// Any other type, and string data that could not be decoded.
    Other {
        /// Registry type code.
        kind: u32,
        /// Raw data.
        bytes: Vec<u8>,
    },
}

impl LegacyValue {
    /// Registry type code of the value (`REG_SZ`, ... or the code carried by
    /// [`LegacyValue::Other`]).
    pub fn type_code(&self) -> u32 {
        match self {
            Self::String(_) => REG_SZ,
            Self::ExpandString(_) => REG_EXPAND_SZ,
            Self::MultiString(_) => REG_MULTI_SZ,
            Self::Dword(_) => REG_DWORD,
            Self::Qword(_) => REG_QWORD,
            Self::Binary(_) => REG_BINARY,
            Self::Other { kind, .. } => *kind,
        }
    }

    /// Size of the data in bytes: UTF-8 bytes for strings (plus one separator per string of a
    /// list), 4 or 8 for numbers, and the raw length otherwise.
    pub fn data_len(&self) -> usize {
        match self {
            Self::String(text) | Self::ExpandString(text) => text.len(),
            Self::MultiString(items) => items.iter().fold(0usize, |total, item| {
                total.saturating_add(item.len()).saturating_add(1)
            }),
            Self::Dword(_) => 4,
            Self::Qword(_) => 8,
            Self::Binary(bytes) | Self::Other { bytes, .. } => bytes.len(),
        }
    }
}

impl fmt::Debug for LegacyValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyValue")
            .field("type", &self.type_code())
            .field("bytes", &self.data_len())
            .finish()
    }
}

/// Read-only access to the current user's registry (`HKEY_CURRENT_USER`).
///
/// A path is a key path below `HKEY_CURRENT_USER` with components separated by backslashes,
/// such as [`SETTINGS_REGISTRY_PATH`](crate::SETTINGS_REGISTRY_PATH); key names compare
/// case-insensitively. A key that does not exist is `Ok(None)`; any other failure is an error.
/// Implementations must not panic and should bound what they return (the walk reads at most 8
/// levels and 1,024 keys per source, but every listed name and value is held in memory).
pub trait LegacyRegistry {
    /// The values of the key at `path`, in any order (the importer sorts them).
    fn values(&self, path: &str) -> Result<Option<Vec<(String, LegacyValue)>>, MigrateError>;

    /// The names of the direct subkeys of the key at `path`, in any order.
    fn subkeys(&self, path: &str) -> Result<Option<Vec<String>>, MigrateError>;
}

/// Read-only access to files.
///
/// Implementations must not panic and must never create, write, rename, delete, or change the
/// permissions of anything.
pub trait SourceFs {
    /// The content of the regular file at `path`; `Ok(None)` when there is no such file (it
    /// does not exist, or it is a directory). A file above the reader's limit is
    /// [`MigrateError::SourceTooLarge`].
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, MigrateError>;

    /// The entries of the directory `dir`, as paths inside it, in any order; `Ok(None)` when
    /// there is no such directory.
    fn list(&self, dir: &Path) -> Result<Option<Vec<PathBuf>>, MigrateError>;
}

/// [`SourceFs`] on [`std::fs`], read-only.
///
/// Files are opened for reading only (never created or truncated), and at most
/// [`StdSourceFs::max_bytes`] bytes are read: a larger file fails with
/// [`MigrateError::SourceTooLarge`] without being read in full.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StdSourceFs {
    max_bytes: u64,
}

impl Default for StdSourceFs {
    fn default() -> Self {
        Self::new()
    }
}

impl StdSourceFs {
    /// A reader that accepts files up to [`MAX_SOURCE_BYTES`].
    pub const fn new() -> Self {
        Self::with_max_bytes(MAX_SOURCE_BYTES)
    }

    /// A reader that accepts files up to `max_bytes`.
    pub const fn with_max_bytes(max_bytes: u64) -> Self {
        Self { max_bytes }
    }

    /// Largest file this reader accepts, in bytes.
    pub const fn max_bytes(&self) -> u64 {
        self.max_bytes
    }
}

impl SourceFs for StdSourceFs {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, MigrateError> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if is_absent(&error) => return Ok(None),
            Err(error) => return Err(MigrateError::from_io_kind(error.kind())),
        };
        if !metadata.is_file() {
            return Ok(None);
        }
        if metadata.len() > self.max_bytes {
            return Err(MigrateError::SourceTooLarge {
                size: metadata.len(),
            });
        }
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if is_absent(&error) => return Ok(None),
            Err(error) => return Err(MigrateError::from_io_kind(error.kind())),
        };
        let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
        file.take(self.max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| MigrateError::from_io_kind(error.kind()))?;
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size > self.max_bytes {
            // The file grew after its size was checked.
            return Err(MigrateError::SourceTooLarge { size });
        }
        Ok(Some(bytes))
    }

    fn list(&self, dir: &Path) -> Result<Option<Vec<PathBuf>>, MigrateError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if is_absent(&error) => return Ok(None),
            Err(error) => return Err(MigrateError::from_io_kind(error.kind())),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| MigrateError::from_io_kind(error.kind()))?;
            paths.push(entry.path());
        }
        paths.sort();
        Ok(Some(paths))
    }
}

/// Whether an I/O error means that the file or directory is not there (as opposed to being
/// unreadable).
fn is_absent(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::NotFound | io::ErrorKind::NotADirectory
    )
}
