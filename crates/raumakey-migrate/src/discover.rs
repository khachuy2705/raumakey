//! Source discovery (plan §9).
//!
//! [`discover`] looks for EVKey data in two registry keys below `HKEY_CURRENT_USER` and in one
//! EVKey folder, reads it once through the read-only ports, and fingerprints it. The returned
//! [`DiscoveryReport`] keeps the bytes it read, so [`parse`](fn@crate::parse) works on exactly the
//! data that was fingerprinted even if a source changes afterwards.
//!
//! # Sources
//!
//! | Kind | Location | Use |
//! |---|---|---|
//! | [`SourceKind::Registry`] | `HKCU\` [`SETTINGS_REGISTRY_PATH`] | settings |
//! | [`SourceKind::TsfCache`] | `HKCU\` [`TSF_CACHE_REGISTRY_PATH`] | compatibility hints only |
//! | [`SourceKind::SettingsIni`] | [`SETTINGS_INI_FILE`] in the EVKey folder | settings |
//! | [`SourceKind::SettingsTree`] | [`SETTINGS_TREE_FILE`] in the EVKey folder | preserved, never parsed |
//! | [`SourceKind::MacroText`] | [`MACRO_TEXT_FILE`] in the EVKey folder | macros |
//!
//! # Folder precedence
//!
//! `folders` lists candidate EVKey folders in priority order, the user-chosen folder first. The
//! first folder that holds at least one of the three files is the EVKey folder, and every file
//! is read from it only: a later folder never fills in a file that the chosen folder lacks, so
//! one import never mixes two EVKey installations. File names match ignoring ASCII case; when a
//! case-sensitive file system holds several spellings, the exact spelling wins.
//!
//! # Fingerprints
//!
//! A file's fingerprint is the SHA-256 and size of its bytes. A file larger than the reader's
//! limit is still a source, but its content is not read: its size is the reported size, its
//! `sha256` is empty, and [`parse`](fn@crate::parse) quarantines it as `too-large`.
//!
//! A registry key has no byte form, so its fingerprint is the SHA-256 and length of a canonical
//! encoding that does not depend on the order in which the port returns values and subkeys.
//! Integers are little-endian, `len` is a `u64`, and `str` is `len` followed by UTF-8 bytes:
//!
//! ```text
//! key      = "raumakey-registry-v1" 0x00 node
//! node     = len(values) value* len(subkeys) subkey* len(skipped) str(name)*
//! value    = str(name) tag:u8 type:u32 data       (sorted by name, then by encoding)
//! subkey   = str(name) node                       (sorted by name)
//! data     = str(text)                            tag 1 String, 2 ExpandString
//!          | len(items) str(item)*                tag 3 MultiString
//!          | u32                                  tag 4 Dword
//!          | u64                                  tag 5 Qword
//!          | len bytes                            tag 6 Binary, 7 Other
//! ```
//!
//! `type` is the registry type code ([`LegacyValue::type_code`](crate::LegacyValue::type_code)).
//! `skipped` lists the subkeys that were not read: names that cannot address a subkey (empty or
//! containing `\`) and subkeys beyond the walk limits (8 levels, 1,024 keys per source).

use core::fmt;
use std::path::{Path, PathBuf};

use raumakey_config::{SourceFingerprint, sha256_hex};

use crate::error::MigrateError;
use crate::ports::{LegacyRegistry, MAX_SOURCE_BYTES, SourceFs};
use crate::registry::{RegistryNode, canonical_bytes, read_tree};

/// EVKey settings key below `HKEY_CURRENT_USER`.
pub const SETTINGS_REGISTRY_PATH: &str = "Software\\MyQ\\EVKey";
/// EVKey TSF capability cache below `HKEY_CURRENT_USER` (compatibility hints, not truth).
pub const TSF_CACHE_REGISTRY_PATH: &str = "Software\\EVKey\\TsfCache";
/// EVKey settings file.
pub const SETTINGS_INI_FILE: &str = "setting.ini";
/// EVKey settings tree file; preserved as raw bytes, never parsed.
pub const SETTINGS_TREE_FILE: &str = "setting.tree";
/// EVKey macro file (`trigger:replacement` lines).
pub const MACRO_TEXT_FILE: &str = "evkmacro.txt";

/// Prefix of registry source locations.
const HKCU_PREFIX: &str = "HKCU\\";

/// What a source is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SourceKind {
    /// Registry settings (`HKCU\Software\MyQ\EVKey`).
    Registry,
    /// Registry TSF capability cache (`HKCU\Software\EVKey\TsfCache`).
    TsfCache,
    /// `setting.ini`.
    SettingsIni,
    /// `setting.tree`.
    SettingsTree,
    /// `evkmacro.txt`.
    MacroText,
}

impl SourceKind {
    /// Every kind, in the order in which [`discover`] lists sources.
    pub const ALL: [SourceKind; 5] = [
        Self::Registry,
        Self::TsfCache,
        Self::SettingsIni,
        Self::SettingsTree,
        Self::MacroText,
    ];

    /// Stable name, used as [`SourceFingerprint::kind`], as the key of the source's raw data
    /// in the migration record's `preserved` object, and as
    /// [`QuarantinedField::source`](raumakey_config::QuarantinedField::source).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Registry => "registry",
            Self::TsfCache => "tsf-cache",
            Self::SettingsIni => "settings-ini",
            Self::SettingsTree => "settings-tree",
            Self::MacroText => "macro-text",
        }
    }
}

impl fmt::Display for SourceKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The data read from a source.
#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Content {
    /// A registry key.
    Registry(RegistryNode),
    /// The bytes of a file.
    File(Vec<u8>),
    /// A file larger than the reader accepts; nothing was read.
    TooLarge,
}

/// One source found by [`discover`], with the data that was read from it.
///
/// The [`fmt::Debug`] output shows the kind, size, and checksum, never the location or the
/// data.
#[derive(Clone, PartialEq, Eq)]
pub struct DiscoveredSource {
    /// What the source is.
    pub kind: SourceKind,
    /// `HKCU\...` registry path or file path.
    pub location: String,
    /// Size in bytes (of the canonical encoding for a registry key).
    pub size: u64,
    /// Lowercase hexadecimal SHA-256 of the bytes (of the canonical encoding for a registry
    /// key); empty when the file was too large to read.
    pub sha256: String,
    pub(crate) content: Content,
}

impl DiscoveredSource {
    /// The fingerprint recorded in `raumakey-migration.json`.
    pub fn fingerprint(&self) -> SourceFingerprint {
        SourceFingerprint {
            kind: self.kind.as_str().to_owned(),
            location: self.location.clone(),
            sha256: self.sha256.clone(),
            size: self.size,
        }
    }

    /// Whether the file was too large to read; its content is then quarantined.
    pub fn too_large(&self) -> bool {
        matches!(self.content, Content::TooLarge)
    }

    fn from_bytes(kind: SourceKind, location: String, bytes: Vec<u8>) -> Self {
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size > MAX_SOURCE_BYTES {
            return Self::too_large_file(kind, location, size);
        }
        Self {
            kind,
            location,
            size,
            sha256: sha256_hex(&bytes),
            content: Content::File(bytes),
        }
    }

    fn too_large_file(kind: SourceKind, location: String, size: u64) -> Self {
        Self {
            kind,
            location,
            size,
            sha256: String::new(),
            content: Content::TooLarge,
        }
    }
}

impl fmt::Debug for DiscoveredSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveredSource")
            .field("kind", &self.kind)
            .field("size", &self.size)
            .field("sha256", &self.sha256)
            .field("too_large", &self.too_large())
            .finish_non_exhaustive()
    }
}

/// What [`discover`] found.
///
/// Removing an element of [`DiscoveryReport::sources`] before [`parse`](fn@crate::parse) leaves
/// that source out of the import.
///
/// The [`fmt::Debug`] output shows the sources (see [`DiscoveredSource`]) but not the folder.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct DiscoveryReport {
    /// The EVKey folder the files were read from, if any file was found.
    pub folder: Option<PathBuf>,
    /// The sources, in the order of [`SourceKind::ALL`].
    pub sources: Vec<DiscoveredSource>,
}

impl DiscoveryReport {
    /// Whether nothing was found.
    pub fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    /// The source of kind `kind`, if it was found.
    pub fn source(&self, kind: SourceKind) -> Option<&DiscoveredSource> {
        self.sources.iter().find(|source| source.kind == kind)
    }

    /// The fingerprints of the sources, in order.
    pub fn fingerprints(&self) -> Vec<SourceFingerprint> {
        self.sources
            .iter()
            .map(DiscoveredSource::fingerprint)
            .collect()
    }
}

impl fmt::Debug for DiscoveryReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DiscoveryReport")
            .field("has_folder", &self.folder.is_some())
            .field("sources", &self.sources)
            .finish()
    }
}

/// Finds and reads the EVKey sources (see the [module documentation](self)).
///
/// Only the read-only port methods are called. Any port failure aborts discovery with that
/// error, so a source that exists but cannot be read is never skipped silently; a file larger
/// than the reader accepts is not a failure (see [`DiscoveredSource::too_large`]).
pub fn discover<R, F, I>(registry: &R, fs: &F, folders: I) -> Result<DiscoveryReport, MigrateError>
where
    R: LegacyRegistry + ?Sized,
    F: SourceFs + ?Sized,
    I: IntoIterator,
    I::Item: AsRef<Path>,
{
    let mut report = DiscoveryReport::default();
    let keys = [
        (SourceKind::Registry, SETTINGS_REGISTRY_PATH),
        (SourceKind::TsfCache, TSF_CACHE_REGISTRY_PATH),
    ];
    for (kind, path) in keys {
        if let Some(node) = read_tree(registry, path)? {
            let bytes = canonical_bytes(&node);
            report.sources.push(DiscoveredSource {
                kind,
                location: format!("{HKCU_PREFIX}{path}"),
                size: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
                sha256: sha256_hex(&bytes),
                content: Content::Registry(node),
            });
        }
    }
    let files = [
        (SourceKind::SettingsIni, SETTINGS_INI_FILE),
        (SourceKind::SettingsTree, SETTINGS_TREE_FILE),
        (SourceKind::MacroText, MACRO_TEXT_FILE),
    ];
    for folder in folders {
        let folder = folder.as_ref();
        let Some(entries) = fs.list(folder)? else {
            continue;
        };
        let mut found = Vec::new();
        for (kind, name) in files {
            if let Some(source) = read_file(fs, &entries, kind, name)? {
                found.push(source);
            }
        }
        if !found.is_empty() {
            report.folder = Some(folder.to_path_buf());
            report.sources.extend(found);
            break;
        }
    }
    Ok(report)
}

/// Reads the file `name` from a folder listing: the exact spelling first, then the other
/// spellings that match ignoring ASCII case, in listing order. An entry that is not a file is
/// skipped.
fn read_file<F: SourceFs + ?Sized>(
    fs: &F,
    entries: &[PathBuf],
    kind: SourceKind,
    name: &str,
) -> Result<Option<DiscoveredSource>, MigrateError> {
    let exact = entries
        .iter()
        .filter(|path| file_name_of(path) == Some(name));
    let others = entries.iter().filter(|path| {
        file_name_of(path).is_some_and(|file| file != name && file.eq_ignore_ascii_case(name))
    });
    for path in exact.chain(others) {
        let location = path.to_string_lossy().into_owned();
        match fs.read(path) {
            Ok(Some(bytes)) => {
                return Ok(Some(DiscoveredSource::from_bytes(kind, location, bytes)));
            }
            Ok(None) => {}
            Err(MigrateError::SourceTooLarge { size }) => {
                return Ok(Some(DiscoveredSource::too_large_file(kind, location, size)));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(None)
}

/// The final component of `path`, when it is valid Unicode.
fn file_name_of(path: &Path) -> Option<&str> {
    path.file_name().and_then(|file| file.to_str())
}
