//! Location of the Raumakey data root (design §8, plan D9).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raumakey_model::identity::PRODUCT_NAME;

use crate::diagnostics::DiagnosticsLog;
use crate::document::StoreDocument;
use crate::error::ConfigError;
use crate::store::{AtomicStore, FileReplacer};

/// The folder that holds every Raumakey store file and `logs/`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataRoot {
    path: PathBuf,
    portable: bool,
}

impl DataRoot {
    /// Marker file beside the executable that enables portable mode.
    pub const PORTABLE_MARKER: &'static str = "raumakey.portable";
    /// Data folder beside the executable in portable mode.
    pub const PORTABLE_DIR: &'static str = "data";
    /// Diagnostics folder inside the root.
    pub const LOGS_DIR: &'static str = "logs";

    /// Resolves the data root.
    ///
    /// Portable mode is enabled only by a [`DataRoot::PORTABLE_MARKER`] file in `exe_dir`
    /// (then the root is `exe_dir/data`); a folder of that name does not count, and portable
    /// mode is never inferred from write access. Otherwise the root is
    /// `<local_app_data>/Raumakey`.
    ///
    /// Both folders must be absolute paths, so that nothing is resolved against the current
    /// directory: a relative or empty `exe_dir` is [`ConfigError::NotFound`], and so is a
    /// missing, empty, or relative `local_app_data` when there is no marker.
    pub fn resolve(exe_dir: &Path, local_app_data: Option<&Path>) -> Result<Self, ConfigError> {
        if !exe_dir.is_absolute() {
            return Err(ConfigError::NotFound);
        }
        if is_file(&exe_dir.join(Self::PORTABLE_MARKER))? {
            return Ok(Self {
                path: exe_dir.join(Self::PORTABLE_DIR),
                portable: true,
            });
        }
        match local_app_data {
            Some(base) if base.is_absolute() => Ok(Self {
                path: base.join(PRODUCT_NAME),
                portable: false,
            }),
            _ => Err(ConfigError::NotFound),
        }
    }

    /// The root folder.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the root was selected by the portable marker.
    pub fn is_portable(&self) -> bool {
        self.portable
    }

    /// The diagnostics folder (`<root>/logs`).
    pub fn logs_dir(&self) -> PathBuf {
        self.path.join(Self::LOGS_DIR)
    }

    /// Creates the root and `logs/` if they do not exist.
    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }

    /// The store of document `D` inside the root.
    pub fn store<D: StoreDocument>(&self, replacer: Arc<dyn FileReplacer>) -> AtomicStore<D> {
        AtomicStore::new(self.path.clone(), replacer)
    }

    /// The diagnostics log inside `logs/`.
    pub fn diagnostics_log(&self, enabled: bool) -> DiagnosticsLog {
        DiagnosticsLog::new(self.logs_dir(), enabled)
    }
}

/// Whether `path` is a file (links are followed); a path that does not exist is not one.
fn is_file(path: &Path) -> Result<bool, ConfigError> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error.into()),
    }
}
