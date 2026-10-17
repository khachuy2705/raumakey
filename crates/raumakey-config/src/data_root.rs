//! Location of the Raumakey data root (design §8, decision D40, which replaces plan D9).
//!
//! Raumakey ships as one portable `raumakey.exe` that the user starts directly (D38), and its
//! configuration lives beside it: the data root is the folder of `raumakey.exe` whenever that
//! folder can be written. Only when it cannot (Program Files, a read-only share) is the root
//! `%LOCALAPPDATA%\Raumakey`. There is no marker file and no `data` subfolder any more.
//!
//! The executable's folder usually holds other files of the user too, so every file Raumakey
//! keeps there has a name that says it belongs to Raumakey (see [`StoreDocument::FILE_NAME`]
//! and [`DataRoot::LOGS_DIR`]).

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use raumakey_model::identity::PRODUCT_NAME;

use crate::diagnostics::DiagnosticsLog;
use crate::document::StoreDocument;
use crate::error::ConfigError;
use crate::store::{AtomicStore, FileReplacer};

/// The folder that holds every Raumakey store file and [`DataRoot::LOGS_DIR`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataRoot {
    path: PathBuf,
    exe_dir: bool,
}

impl DataRoot {
    /// Diagnostics folder inside the root.
    pub const LOGS_DIR: &'static str = "raumakey-logs";

    /// Resolves the data root with the real write probe, [`is_writable_dir`].
    ///
    /// See [`DataRoot::resolve_with`].
    pub fn resolve(exe_dir: &Path, local_app_data: Option<&Path>) -> Result<Self, ConfigError> {
        Self::resolve_with(exe_dir, local_app_data, &is_writable_dir)
    }

    /// Resolves the data root (D40): `exe_dir` itself when `is_writable(exe_dir)` says that it
    /// can be written, otherwise `<local_app_data>/Raumakey`.
    ///
    /// `is_writable` is asked about `exe_dir` only, and only when `exe_dir` is absolute. Both
    /// folders must be absolute paths, so that nothing is resolved against the current
    /// directory: a relative or empty `exe_dir` is [`ConfigError::NotFound`] (before any
    /// probe), and so is a missing, empty, or relative `local_app_data` when `exe_dir` cannot
    /// be written.
    pub fn resolve_with(
        exe_dir: &Path,
        local_app_data: Option<&Path>,
        is_writable: &dyn Fn(&Path) -> bool,
    ) -> Result<Self, ConfigError> {
        if !exe_dir.is_absolute() {
            return Err(ConfigError::NotFound);
        }
        if is_writable(exe_dir) {
            return Ok(Self {
                path: exe_dir.to_path_buf(),
                exe_dir: true,
            });
        }
        match local_app_data {
            Some(base) if base.is_absolute() => Ok(Self {
                path: base.join(PRODUCT_NAME),
                exe_dir: false,
            }),
            _ => Err(ConfigError::NotFound),
        }
    }

    /// The root folder.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether the root is the folder of the executable; `false` when it is the fallback
    /// under the local application data folder.
    pub fn is_exe_dir(&self) -> bool {
        self.exe_dir
    }

    /// The diagnostics folder (`<root>/raumakey-logs`).
    pub fn logs_dir(&self) -> PathBuf {
        self.path.join(Self::LOGS_DIR)
    }

    /// Creates the root and [`DataRoot::LOGS_DIR`] if they do not exist.
    ///
    /// A store creates the root by itself on its first commit, so call this only when the
    /// diagnostics log is needed: in the executable's folder the logs folder is one more entry
    /// among the user's files.
    pub fn ensure_dirs(&self) -> Result<(), ConfigError> {
        fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }

    /// The store of document `D` inside the root.
    pub fn store<D: StoreDocument>(&self, replacer: Arc<dyn FileReplacer>) -> AtomicStore<D> {
        AtomicStore::new(self.path.clone(), replacer)
    }

    /// The diagnostics log inside [`DataRoot::LOGS_DIR`].
    pub fn diagnostics_log(&self, enabled: bool) -> DiagnosticsLog {
        DiagnosticsLog::new(self.logs_dir(), enabled)
    }
}

/// Distinguishes the probe files of one process.
static PROBE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// How many names [`is_writable_dir`] tries when a probe name is taken already.
const PROBE_ATTEMPTS: u32 = 4;

/// Whether this process can create and delete files in `dir`: the real probe of
/// [`DataRoot::resolve`].
///
/// It creates a new, uniquely named empty file (`raumakey-write-probe-<pid>-<time>-<n>.tmp`)
/// with `create_new`, so that no existing file is opened, truncated, or changed, and deletes
/// it again at once; nothing else in `dir` is touched. A name that is taken already is
/// skipped (up to a few attempts). The folder counts as writable only when the delete
/// succeeds too, because a commit replaces files; when the delete fails, the probe file is
/// left behind (it cannot be removed). A folder that does not exist, a path that is not a
/// folder, and an empty path cannot be written. Whether a later commit succeeds still depends
/// on the files themselves (a read-only `raumakey.toml`, for example), which the store
/// reports.
pub fn is_writable_dir(dir: &Path) -> bool {
    if dir.as_os_str().is_empty() || !dir.is_dir() {
        return false;
    }
    for _ in 0..PROBE_ATTEMPTS {
        let probe = dir.join(probe_name());
        match OpenOptions::new().write(true).create_new(true).open(&probe) {
            Ok(file) => {
                drop(file);
                return fs::remove_file(&probe).is_ok();
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return false,
        }
    }
    false
}

/// A probe file name that no other probe of this process uses.
fn probe_name() -> String {
    let sequence = PROBE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    format!(
        "raumakey-write-probe-{}-{stamp}-{sequence}.tmp",
        std::process::id()
    )
}
