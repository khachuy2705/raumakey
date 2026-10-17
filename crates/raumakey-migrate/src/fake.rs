//! In-memory source fakes for tests and dry runs.
//!
//! [`FakeRegistry`] and [`FakeFs`] implement the read-only ports from data held in memory. Their
//! builder methods take `&mut self`; the port methods take `&self` and never change anything.
//! Both can inject a failure for a path. Their [`fmt::Debug`] output shows counts only, never
//! the stored data.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::error::MigrateError;
use crate::ports::{LegacyRegistry, LegacyValue, MAX_SOURCE_BYTES, SourceFs};

/// One key of a [`FakeRegistry`].
#[derive(Clone, Default)]
struct FakeKey {
    /// Last path component as first spelled.
    name: String,
    /// Values in insertion order.
    values: Vec<(String, LegacyValue)>,
    /// Position in `values` by case-folded name.
    index: BTreeMap<String, usize>,
}

/// In-memory [`LegacyRegistry`].
///
/// Keys are addressed by backslash-separated paths and compare case-insensitively, like the
/// Windows registry. Creating a key creates its ancestors. Values keep their insertion order
/// (the importer must not depend on it); setting a value whose name exists (ignoring case)
/// replaces it.
#[derive(Clone, Default)]
pub struct FakeRegistry {
    /// Keys by lowercase path.
    keys: BTreeMap<String, FakeKey>,
    /// Injected failures by lowercase path.
    failures: BTreeMap<String, MigrateError>,
}

impl FakeRegistry {
    /// An empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates the key at `path` and its ancestors (existing keys are kept).
    pub fn create_key(&mut self, path: &str) -> &mut Self {
        let mut prefix = String::new();
        for component in path.split('\\') {
            if !prefix.is_empty() {
                prefix.push('\\');
            }
            prefix.push_str(component);
            self.keys.entry(fold(&prefix)).or_insert_with(|| FakeKey {
                name: component.to_owned(),
                ..FakeKey::default()
            });
        }
        self
    }

    /// Sets the value `name` of the key at `path`, creating the key. A value whose name matches
    /// (ignoring case) is replaced in place, taking the new spelling.
    pub fn set_value(&mut self, path: &str, name: &str, value: LegacyValue) -> &mut Self {
        self.create_key(path);
        if let Some(key) = self.keys.get_mut(&fold(path)) {
            let entry = (name.to_owned(), value);
            match key
                .index
                .get(&fold(name))
                .and_then(|&position| key.values.get_mut(position))
            {
                Some(slot) => *slot = entry,
                None => {
                    key.index.insert(fold(name), key.values.len());
                    key.values.push(entry);
                }
            }
        }
        self
    }

    /// Makes every read of the key at `path` fail with `error`.
    pub fn fail(&mut self, path: &str, error: MigrateError) -> &mut Self {
        self.failures.insert(fold(path), error);
        self
    }

    fn check(&self, path: &str) -> Result<String, MigrateError> {
        let folded = fold(path);
        match self.failures.get(&folded) {
            Some(error) => Err(error.clone()),
            None => Ok(folded),
        }
    }
}

impl LegacyRegistry for FakeRegistry {
    fn values(&self, path: &str) -> Result<Option<Vec<(String, LegacyValue)>>, MigrateError> {
        let folded = self.check(path)?;
        Ok(self.keys.get(&folded).map(|key| key.values.clone()))
    }

    fn subkeys(&self, path: &str) -> Result<Option<Vec<String>>, MigrateError> {
        let folded = self.check(path)?;
        if !self.keys.contains_key(&folded) {
            return Ok(None);
        }
        let prefix = format!("{folded}\\");
        let names = self
            .keys
            .iter()
            .filter(|(key_path, _)| {
                key_path
                    .strip_prefix(&prefix)
                    .is_some_and(|rest| !rest.is_empty() && !rest.contains('\\'))
            })
            .map(|(_, key)| key.name.clone())
            .collect();
        Ok(Some(names))
    }
}

impl fmt::Debug for FakeRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let values: usize = self.keys.values().map(|key| key.values.len()).sum();
        f.debug_struct("FakeRegistry")
            .field("keys", &self.keys.len())
            .field("values", &values)
            .field("failures", &self.failures.len())
            .finish()
    }
}

/// Case folding used for registry names.
fn fold(text: &str) -> String {
    text.to_lowercase()
}

/// In-memory [`SourceFs`].
///
/// Paths compare exactly (as [`Path`] values). Adding a file or directory adds its ancestors as
/// directories. Reads of a file above [`FakeFs::max_bytes`] fail with
/// [`MigrateError::SourceTooLarge`], like [`StdSourceFs`](crate::StdSourceFs).
#[derive(Clone)]
pub struct FakeFs {
    files: BTreeMap<PathBuf, Vec<u8>>,
    dirs: BTreeSet<PathBuf>,
    failures: BTreeMap<PathBuf, MigrateError>,
    max_bytes: u64,
}

impl Default for FakeFs {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeFs {
    /// An empty file system whose reads accept files up to [`MAX_SOURCE_BYTES`].
    pub fn new() -> Self {
        Self {
            files: BTreeMap::new(),
            dirs: BTreeSet::new(),
            failures: BTreeMap::new(),
            max_bytes: MAX_SOURCE_BYTES,
        }
    }

    /// Replaces the largest file size [`SourceFs::read`] accepts.
    #[must_use]
    pub fn with_max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Largest file size [`SourceFs::read`] accepts, in bytes.
    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    /// Adds (or replaces) the file `path` with `bytes`.
    pub fn add_file(&mut self, path: impl Into<PathBuf>, bytes: Vec<u8>) -> &mut Self {
        let path = path.into();
        self.add_ancestors(&path);
        self.files.insert(path, bytes);
        self
    }

    /// Adds the directory `path`.
    pub fn add_dir(&mut self, path: impl Into<PathBuf>) -> &mut Self {
        let path = path.into();
        self.add_ancestors(&path);
        self.dirs.insert(path);
        self
    }

    /// Makes every read or listing of `path` fail with `error`.
    pub fn fail(&mut self, path: impl Into<PathBuf>, error: MigrateError) -> &mut Self {
        self.failures.insert(path.into(), error);
        self
    }

    /// The content of the file `path`, for assertions.
    pub fn file(&self, path: &Path) -> Option<&[u8]> {
        self.files.get(path).map(Vec::as_slice)
    }

    fn add_ancestors(&mut self, path: &Path) {
        for ancestor in path.ancestors().skip(1) {
            if !ancestor.as_os_str().is_empty() {
                self.dirs.insert(ancestor.to_path_buf());
            }
        }
    }

    fn check(&self, path: &Path) -> Result<(), MigrateError> {
        match self.failures.get(path) {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

impl SourceFs for FakeFs {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, MigrateError> {
        self.check(path)?;
        let Some(bytes) = self.files.get(path) else {
            return Ok(None);
        };
        let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if size > self.max_bytes {
            return Err(MigrateError::SourceTooLarge { size });
        }
        Ok(Some(bytes.clone()))
    }

    fn list(&self, dir: &Path) -> Result<Option<Vec<PathBuf>>, MigrateError> {
        self.check(dir)?;
        if !self.dirs.contains(dir) {
            return Ok(None);
        }
        let mut entries: Vec<PathBuf> = self
            .files
            .keys()
            .chain(self.dirs.iter())
            .filter(|path| path.parent() == Some(dir))
            .cloned()
            .collect();
        entries.sort();
        entries.dedup();
        Ok(Some(entries))
    }
}

impl fmt::Debug for FakeFs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeFs")
            .field("files", &self.files.len())
            .field("dirs", &self.dirs.len())
            .field("failures", &self.failures.len())
            .field("max_bytes", &self.max_bytes)
            .finish()
    }
}
