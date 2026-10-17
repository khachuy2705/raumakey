//! Location of the Raumakey data root (design §8, decision D40, which replaces plan D9).
//!
//! Raumakey ships as one portable `raumakey.exe` that the user starts directly (D38), and its
//! configuration lives beside it ([`DataRoot::resolve_with`] has the exact rule):
//!
//! 1. When `raumakey.toml` exists beside `raumakey.exe`, that folder is the root, even when it
//!    cannot be written (saving then fails, and the controller reports it).
//! 2. Otherwise the folder of `raumakey.exe` is the root when it can be written and is not
//!    protected. Which folders are protected is the caller's decision: the controller protects
//!    Program Files, Program Files (x86), the Windows folder, and the user's temp folder, so
//!    that elevated and normal runs of an installed copy share one file, and a copy that
//!    Explorer runs from inside a ZIP (out of the temp folder, which it cleans later) keeps
//!    nothing there.
//! 3. Otherwise the root is `%LOCALAPPDATA%\Raumakey`.
//!
//! There is no marker file and no `data` subfolder any more.
//!
//! Trade-off: an elevated run reads the configuration beside the executable just as a normal
//! run does, so when `raumakey.exe` lies in a folder that the user's ordinary
//! (medium-integrity) programs can write, such as Downloads, the elevated controller reads a
//! file that those programs can change. The file holds settings only, and the same programs can
//! replace `raumakey.exe` itself in such a folder.
//!
//! The executable's folder usually holds other files of the user too, so every file Raumakey
//! keeps there has a name that says it belongs to Raumakey (see [`StoreDocument::FILE_NAME`],
//! [`DataRoot::LOGS_DIR`], and [`WRITE_PROBE_FILE`]).

use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raumakey_model::identity::PRODUCT_NAME;

use crate::config::ConfigDocument;
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

    /// Resolves the data root with the real write probe, [`is_writable_dir`], and the caller's
    /// `is_protected` (see [`DataRoot::resolve_with`]).
    pub fn resolve(
        exe_dir: &Path,
        local_app_data: Option<&Path>,
        is_protected: &dyn Fn(&Path) -> bool,
    ) -> Result<Self, ConfigError> {
        Self::resolve_with(exe_dir, local_app_data, &is_writable_dir, is_protected)
    }

    /// Resolves the data root (D40):
    ///
    /// 1. `exe_dir`, when `exe_dir/raumakey.toml` is a file, without asking `is_protected` or
    ///    `is_writable`;
    /// 2. `exe_dir`, when `is_protected(exe_dir)` is `false` and then `is_writable(exe_dir)` is
    ///    `true` (a protected folder is never probed);
    /// 3. otherwise `<local_app_data>/Raumakey`.
    ///
    /// Nothing is resolved against the current directory: a relative or empty `exe_dir` (the
    /// caller could not find the executable) is neither looked at nor asked about, so the root
    /// is the local application data folder; a missing, empty, or relative `local_app_data` is
    /// [`ConfigError::NotFound`] when the executable's folder is not used.
    pub fn resolve_with(
        exe_dir: &Path,
        local_app_data: Option<&Path>,
        is_writable: &dyn Fn(&Path) -> bool,
        is_protected: &dyn Fn(&Path) -> bool,
    ) -> Result<Self, ConfigError> {
        let in_exe_dir = exe_dir.is_absolute()
            && (exe_dir.join(ConfigDocument::FILE_NAME).is_file()
                || (!is_protected(exe_dir) && is_writable(exe_dir)));
        if in_exe_dir {
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

/// Name of the file that [`is_writable_dir`] creates and deletes.
pub const WRITE_PROBE_FILE: &str = "raumakey-write-probe.tmp";

/// How often [`is_writable_dir`] tries to create its file when another probe (of a second
/// Raumakey starting at the same moment) holds the name.
const PROBE_ATTEMPTS: u32 = 2;

/// Whether this process can create and delete files in `dir`: the real probe of
/// [`DataRoot::resolve`].
///
/// It creates the empty file [`WRITE_PROBE_FILE`] with `create_new`, so that no existing file
/// is opened, truncated, or changed, and deletes it again at once; nothing else in `dir` is
/// touched. The name is always the same, so a probe file that an earlier launch could not
/// delete is removed first and never joined by others. The folder counts as writable only
/// when the delete succeeds too (a file that is gone already counts as deleted), because a
/// commit replaces files. A folder that does not exist, a path that is not a folder, and an
/// empty path cannot be written. Whether a later commit succeeds still depends on the files
/// themselves (a read-only `raumakey.toml`, for example), which the store reports.
pub fn is_writable_dir(dir: &Path) -> bool {
    if dir.as_os_str().is_empty() || !dir.is_dir() {
        return false;
    }
    let probe = dir.join(WRITE_PROBE_FILE);
    for _ in 0..PROBE_ATTEMPTS {
        // A probe file left behind by an earlier launch, or one that a second probe is about
        // to delete anyway.
        remove_probe(&probe);
        match OpenOptions::new().write(true).create_new(true).open(&probe) {
            Ok(file) => {
                drop(file);
                return remove_probe(&probe);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) => return false,
        }
    }
    false
}

/// Deletes the probe file; whether it is gone (also when it was gone already).
fn remove_probe(probe: &Path) -> bool {
    match fs::remove_file(probe) {
        Ok(()) => true,
        Err(error) => error.kind() == io::ErrorKind::NotFound,
    }
}
