//! The configuration lifecycle of the portable controller (D38, D40).
//!
//! Raumakey is one portable `raumakey.exe` that the user starts directly, and its configuration
//! lives beside it. [`Settings`] owns that configuration for the run mode and the tray:
//!
//! - **Where.** [`Settings::open`] resolves the data root with
//!   [`DataRoot::resolve_with`](raumakey_config::DataRoot::resolve_with): the folder of
//!   `raumakey.exe` when it can be written ([`Location::ExeDir`]), otherwise
//!   `%LOCALAPPDATA%\Raumakey` ([`Location::LocalAppData`]). When neither can be used
//!   ([`Location::Unavailable`]: no absolute executable folder and no local application data
//!   folder), the settings live in memory: they start from the defaults, every change takes
//!   effect, and none is saved ([`ConfigError::NotFound`]).
//! - **First run.** When `raumakey.toml` does not exist, the defaults are committed at once, so
//!   the file exists from then on ([`OpenReport::first_run`]; a failure to write it is
//!   [`OpenReport::create_error`], and the controller runs on the defaults anyway). The second
//!   run loads the file and writes nothing. `raumakey-macros.toml` and
//!   `raumakey-app-rules.toml` are read when they exist and are never created or written here.
//! - **Snapshots.** [`Settings::runtime_snapshot`] and [`Settings::macro_snapshot`] are what the
//!   session core consumes (`ServiceCore::apply_runtime_snapshot` and `build_macro_table`):
//!   the runtime snapshot always passes [`RuntimeSnapshot::validate`] (including the key map of
//!   a user-defined method), and the macro snapshot always passes [`MacroSnapshot::validate`]
//!   and macro validation. The engine options come from
//!   [`ConfigDocument::engine_options`], the live mode from `[input] enabled`, the hotkeys from
//!   `[hotkeys]` (default toggle `Ctrl+Shift`, D5), diagnostics from `[system] diagnostics`,
//!   and the compatibility rules from `raumakey-app-rules.toml` (the D13 defaults without it).
//! - **Changes from the tray** ([`Change`], [`Settings::apply`]) are committed at once. When the
//!   commit reports a conflict, because the file was edited by hand or by another writer since
//!   it was loaded, the file is loaded again, the change is applied again on what it holds, and
//!   the commit is retried (up to three times). A change takes effect even when it cannot be
//!   saved; the error says why it was not.
//! - **Hand edits.** [`Settings::poll`] (the tray calls it every two seconds) notices a file
//!   that changed on disk, even without a new revision, loads it again, and returns the new
//!   snapshots ([`Reloaded`]).
//!
//! # Damaged files
//!
//! A file that cannot be read or decoded ([`DocumentStatus::Unreadable`]) never stops the
//! controller and is never overwritten silently. At start the defaults are in effect; when a
//! file breaks while the controller runs (a hand edit saved half-way), the last good settings
//! stay in effect. Changes take effect but are not saved: the store refuses to commit over a
//! corrupt file or one of a newer schema version, and [`Settings::apply`] returns that error.
//! Nothing is lost until the user decides: [`Settings::recover`] moves the damaged
//! `raumakey.toml` aside (to `raumakey.toml.bak-<unix-ms>`) and writes the settings in effect in
//! its place, and fixing the file by hand brings it back through [`Settings::poll`].
//!
//! # What is reported
//!
//! [`Status`] lists, per file, whether it is missing, loaded with the fields that took their
//! defaults ([`FieldIssue`]: schema paths and reason codes, never values), unreadable, or
//! unusable, and which macro entries were left out ([`EntryIssue`]: positions, ids, and error
//! codes). Lists longer than the channel's limits ([`MAX_KEYMAPS`], [`MAX_HOTKEYS`],
//! [`MAX_MACROS`]) are cut for the snapshot and reported on their path
//! ([`IssueReason::OutOfRange`] on `keymaps`, `hotkeys.toggle_mode`, `hotkeys.hard_reset`, or
//! `macro`); the documents keep every element. Beyond [`MAX_KEYMAPS`] key maps, only the key
//! map that the input method uses (if any) is carried, as it is the only one the engine reads.
//! Compatibility rules that the session core would refuse make `raumakey-app-rules.toml`
//! [`DocumentStatus::Unusable`], with the D13 defaults in effect.
//!
//! Nothing here logs. The [`Debug`](fmt::Debug) output of [`Settings`] shows the location,
//! paths, revisions, the macro count, and the status, never macro text.

use core::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use raumakey_config::{
    AppRulesDocument, AtomicStore, ConfigDocument, ConfigError, DataRoot, FieldIssue, FileReplacer,
    IssueReason, Loaded, MacroDocument, StartupMode, StoreDocument, is_writable_dir,
};
use raumakey_ipc::error::IpcError;
use raumakey_ipc::message::{MAX_HOTKEYS, MAX_KEYMAPS, MAX_MACROS, MacroSnapshot, RuntimeSnapshot};
use raumakey_macro::{EntryIssue, MacroStore};
use raumakey_model::options::InputMethod;
use raumakey_model::policy::CompatibilityConfig;

/// How long a commit waits for a store lock. The tray commits on its own thread, so it does
/// not wait the store's default of ten seconds.
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// How often [`Settings::apply`] loads the file again after a conflict before it gives up.
const CONFLICT_RETRIES: u32 = 3;

/// Where the configuration lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Location {
    /// The folder of `raumakey.exe` (D40).
    ExeDir,
    /// `%LOCALAPPDATA%\Raumakey`, because the folder of `raumakey.exe` cannot be written.
    LocalAppData,
    /// No folder can be used: the settings live in memory and are not saved.
    Unavailable,
}

/// The state of one configuration file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DocumentStatus {
    /// The file does not exist; its defaults are in effect.
    Missing,
    /// The file was read. `issues` lists the fields that took their defaults, the entries and
    /// elements that were set aside or dropped (matrix #45), and the lists that were cut to a
    /// snapshot limit; by schema path, never with values.
    Loaded {
        /// What was not taken from the file as written.
        issues: Vec<FieldIssue>,
    },
    /// The file cannot be read or decoded ([`ConfigError::Corrupt`],
    /// [`ConfigError::UnsupportedVersion`], [`ConfigError::PermissionDenied`], ...). The
    /// defaults (at start) or the last good settings stay in effect, and the file is never
    /// overwritten implicitly.
    Unreadable(ConfigError),
    /// The file was read, but the session core would refuse what it holds
    /// ([`RuntimeSnapshot::validate`] failed with this error); its defaults are in effect.
    Unusable(IpcError),
}

/// The state of every configuration file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    /// `raumakey.toml`.
    pub config: DocumentStatus,
    /// `raumakey-macros.toml`.
    pub macros: DocumentStatus,
    /// `raumakey-app-rules.toml`.
    pub app_rules: DocumentStatus,
    /// Macro entries of `raumakey-macros.toml` that are left out of the macro snapshot: entries
    /// that are invalid now (plan D29, D30) and repeated ids, by their position among the
    /// decoded entries (see `MacroStore::from_entries_lenient`). The file keeps them.
    pub skipped_macros: Vec<EntryIssue>,
}

impl Status {
    const NOTHING: Self = Self {
        config: DocumentStatus::Missing,
        macros: DocumentStatus::Missing,
        app_rules: DocumentStatus::Missing,
        skipped_macros: Vec::new(),
    };
}

/// What [`Settings::open`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenReport {
    /// Where the configuration lives.
    pub location: Location,
    /// The path of `raumakey.toml`; `None` for [`Location::Unavailable`].
    pub config_path: Option<PathBuf>,
    /// `raumakey.toml` did not exist, so the defaults are in effect and were written (see
    /// [`OpenReport::create_error`]).
    pub first_run: bool,
    /// Why the defaults could not be written on the first run (always
    /// [`ConfigError::NotFound`] for [`Location::Unavailable`]). The next change tries again.
    pub create_error: Option<ConfigError>,
    /// The state of every file after the open ([`Settings::status`]).
    pub status: Status,
}

/// A change made from the tray.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    /// Vietnamese mode on or off (`[input] enabled`, and the live mode of the snapshot).
    VietnameseMode(bool),
    /// The input method (`[input] method`). A user-defined method without a usable key map is
    /// stored, while the engine runs on the default method
    /// ([`ConfigDocument::engine_options`]).
    InputMethod(InputMethod),
    /// Spell checking (`[input] spell_check`).
    SpellCheck(bool),
    /// Modern tone placement (`[input] modern_tone`).
    ModernTone(bool),
    /// Macro expansion (`[macros] enabled`).
    Macros(bool),
    /// How Raumakey starts with Windows (`[system] startup`). Only the choice is stored; the
    /// caller creates or removes the shortcut (`crate::startup`).
    Startup(StartupMode),
}

impl Change {
    /// Whether `document` holds the value of this change already.
    fn holds_in(self, document: &ConfigDocument) -> bool {
        match self {
            Self::VietnameseMode(on) => document.input.enabled == on,
            Self::InputMethod(method) => document.input.method == method,
            Self::SpellCheck(on) => document.input.spell_check == on,
            Self::ModernTone(on) => document.input.modern_tone == on,
            Self::Macros(on) => document.macros.enabled == on,
            Self::Startup(mode) => document.system.startup == mode,
        }
    }

    fn apply_to(self, document: &mut ConfigDocument) {
        match self {
            Self::VietnameseMode(on) => document.input.enabled = on,
            Self::InputMethod(method) => document.input.method = method,
            Self::SpellCheck(on) => document.input.spell_check = on,
            Self::ModernTone(on) => document.input.modern_tone = on,
            Self::Macros(on) => document.macros.enabled = on,
            Self::Startup(mode) => document.system.startup = mode,
        }
    }
}

/// What [`Settings::poll`] loaded again. [`Settings::status`] tells how each file was read.
#[derive(Clone, PartialEq, Eq)]
pub struct Reloaded {
    /// The runtime snapshot now in effect, when `raumakey.toml` or `raumakey-app-rules.toml`
    /// changed and its new content took effect (not when it became unreadable).
    pub runtime: Option<RuntimeSnapshot>,
    /// The macro snapshot now in effect, when `raumakey-macros.toml` changed and its new
    /// content took effect.
    pub macros: Option<Arc<MacroSnapshot>>,
}

impl fmt::Debug for Reloaded {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Reloaded")
            .field(
                "runtime",
                &self.runtime.as_ref().map(|runtime| runtime.config_revision),
            )
            .field("macros", &self.macros)
            .finish()
    }
}

/// The stores of the data root.
struct Stores {
    config: AtomicStore<ConfigDocument>,
    macros: AtomicStore<MacroDocument>,
    app_rules: AtomicStore<AppRulesDocument>,
}

impl Stores {
    fn new(root: &DataRoot) -> Self {
        let replacer = replacer();
        Self {
            config: root
                .store(Arc::clone(&replacer))
                .with_lock_timeout(LOCK_TIMEOUT),
            macros: root
                .store(Arc::clone(&replacer))
                .with_lock_timeout(LOCK_TIMEOUT),
            app_rules: root.store(replacer).with_lock_timeout(LOCK_TIMEOUT),
        }
    }
}

/// The file replacer of the stores: `ReplaceFileW` on Windows.
fn replacer() -> Arc<dyn FileReplacer> {
    #[cfg(windows)]
    {
        Arc::new(raumakey_windows::file_replace::ReplaceFileReplacer)
    }
    #[cfg(not(windows))]
    {
        Arc::new(raumakey_config::StdReplacer)
    }
}

/// The outcome of reading one file.
enum Read<D> {
    Found(Loaded<D>),
    Missing,
    Failed(ConfigError),
}

fn read<D: StoreDocument>(store: &AtomicStore<D>) -> Read<D> {
    match store.load() {
        Ok(loaded) => Read::Found(loaded),
        Err(ConfigError::NotFound) => Read::Missing,
        Err(error) => Read::Failed(error),
    }
}

/// Whether the file of `store` should be read again: it changed since the store last read or
/// wrote it, or it cannot even be checked, unless the last read failed with that very error
/// (so that a file that stays unreadable is not reported every two seconds).
fn needs_reading<D: StoreDocument>(store: &AtomicStore<D>, last: &DocumentStatus) -> bool {
    match store.poll_changed() {
        Ok(changed) => changed,
        Err(error) => *last != DocumentStatus::Unreadable(error),
    }
}

/// The configuration of the portable controller (see the [module documentation](self)).
pub struct Settings {
    location: Location,
    /// `None` for [`Location::Unavailable`].
    stores: Option<Stores>,
    /// The configuration in effect, including changes that could not be saved.
    config: ConfigDocument,
    /// How `raumakey.toml` was last read, before the snapshot checks.
    config_read: DocumentStatus,
    /// The compatibility data of `raumakey-app-rules.toml`, before the snapshot check.
    compatibility: CompatibilityConfig,
    /// How `raumakey-app-rules.toml` was last read, before the snapshot check.
    app_rules_read: DocumentStatus,
    runtime: RuntimeSnapshot,
    macros: Arc<MacroSnapshot>,
    status: Status,
}

impl fmt::Debug for Settings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Settings")
            .field("location", &self.location)
            .field("config_path", &self.config_path())
            .field("config_revision", &self.config.revision)
            .field("vietnamese_mode", &self.config.input.enabled)
            .field("macros", &self.macros)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Settings {
    /// Opens the configuration of a `raumakey.exe` in `exe_dir` (see the [module
    /// documentation](self)): resolves the data root, asking `is_writable` whether `exe_dir`
    /// can be written ([`is_writable_dir`] is the real probe), loads `raumakey.toml`, writes
    /// the defaults when it does not exist, and loads `raumakey-macros.toml` and
    /// `raumakey-app-rules.toml` when they exist.
    ///
    /// Never fails: whatever cannot be read leaves its defaults in effect and is reported.
    pub fn open(
        exe_dir: &Path,
        local_app_data: Option<&Path>,
        is_writable: &dyn Fn(&Path) -> bool,
    ) -> (Self, OpenReport) {
        let Ok(root) = DataRoot::resolve_with(exe_dir, local_app_data, is_writable) else {
            let settings = Self::in_memory();
            let report = OpenReport {
                location: Location::Unavailable,
                config_path: None,
                first_run: true,
                create_error: Some(ConfigError::NotFound),
                status: settings.status.clone(),
            };
            return (settings, report);
        };
        let location = if root.is_exe_dir() {
            Location::ExeDir
        } else {
            Location::LocalAppData
        };
        let stores = Stores::new(&root);
        let mut first_run = false;
        let mut create_error = None;
        let (config, config_read) = match read(&stores.config) {
            Read::Found(loaded) => (
                loaded.document,
                DocumentStatus::Loaded {
                    issues: loaded.issues,
                },
            ),
            Read::Missing => {
                first_run = true;
                let mut defaults = ConfigDocument::defaults();
                match stores.config.commit(0, &defaults) {
                    Ok(revision) => {
                        defaults.revision = revision;
                        (defaults, DocumentStatus::Loaded { issues: Vec::new() })
                    }
                    Err(error) => {
                        create_error = Some(error);
                        (defaults, DocumentStatus::Missing)
                    }
                }
            }
            Read::Failed(error) => (
                ConfigDocument::defaults(),
                DocumentStatus::Unreadable(error),
            ),
        };
        let (compatibility, app_rules_read) = match read(&stores.app_rules) {
            Read::Found(loaded) => (
                loaded.document.compatibility,
                DocumentStatus::Loaded {
                    issues: loaded.issues,
                },
            ),
            Read::Missing => (CompatibilityConfig::defaults_v1(), DocumentStatus::Missing),
            Read::Failed(error) => (
                CompatibilityConfig::defaults_v1(),
                DocumentStatus::Unreadable(error),
            ),
        };
        let mut settings = Self {
            location,
            stores: None,
            config,
            config_read,
            compatibility,
            app_rules_read,
            runtime: RuntimeSnapshot::defaults_v1(),
            macros: Arc::new(MacroSnapshot::empty(0)),
            status: Status::NOTHING,
        };
        match read(&stores.macros) {
            Read::Found(loaded) => settings.install_macros(loaded),
            Read::Missing => {}
            Read::Failed(error) => settings.status.macros = DocumentStatus::Unreadable(error),
        }
        settings.stores = Some(stores);
        settings.rebuild();
        let report = OpenReport {
            location,
            config_path: settings.config_path().map(Path::to_path_buf),
            first_run,
            create_error,
            status: settings.status.clone(),
        };
        (settings, report)
    }

    /// [`Settings::open`] for the running `raumakey.exe`: its folder
    /// ([`std::env::current_exe`]), the local application data folder (`FOLDERID_LocalAppData`
    /// on Windows, else `%LOCALAPPDATA%`), and the real write probe.
    pub fn open_for_current_exe() -> (Self, OpenReport) {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let local_app_data = local_app_data();
        Self::open(&exe_dir, local_app_data.as_deref(), &is_writable_dir)
    }

    /// Settings without a folder: the defaults, in memory.
    fn in_memory() -> Self {
        let mut settings = Self {
            location: Location::Unavailable,
            stores: None,
            config: ConfigDocument::defaults(),
            config_read: DocumentStatus::Missing,
            compatibility: CompatibilityConfig::defaults_v1(),
            app_rules_read: DocumentStatus::Missing,
            runtime: RuntimeSnapshot::defaults_v1(),
            macros: Arc::new(MacroSnapshot::empty(0)),
            status: Status::NOTHING,
        };
        settings.rebuild();
        settings
    }

    /// Where the configuration lives.
    pub fn location(&self) -> Location {
        self.location
    }

    /// The path of `raumakey.toml`; `None` for [`Location::Unavailable`].
    pub fn config_path(&self) -> Option<&Path> {
        self.stores.as_ref().map(|stores| stores.config.path())
    }

    /// The configuration in effect, including changes that could not be saved. Its `revision`
    /// is that of the file it was last loaded from or committed to.
    pub fn document(&self) -> &ConfigDocument {
        &self.config
    }

    /// Whether Vietnamese mode is on.
    pub fn vietnamese_mode(&self) -> bool {
        self.config.input.enabled
    }

    /// The runtime snapshot of the settings in effect; it passes
    /// [`RuntimeSnapshot::validate`].
    pub fn runtime_snapshot(&self) -> &RuntimeSnapshot {
        &self.runtime
    }

    /// The macro snapshot in effect; it passes [`MacroSnapshot::validate`] and macro
    /// validation.
    pub fn macro_snapshot(&self) -> &Arc<MacroSnapshot> {
        &self.macros
    }

    /// The state of every file, as last read.
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// Applies `change` and commits it at once (see the [module documentation](self)).
    ///
    /// A change to the value in effect writes nothing. The change takes effect (in
    /// [`Settings::document`] and the snapshots) even when it cannot be saved. Errors:
    ///
    /// - [`ConfigError::Corrupt`], [`ConfigError::UnsupportedVersion`]: `raumakey.toml` is
    ///   damaged or of a newer version; it is kept as it is (see [`Settings::recover`]).
    /// - [`ConfigError::Conflict`]: the file kept changing under every retry.
    /// - [`ConfigError::NotFound`]: [`Location::Unavailable`], nothing can be saved.
    /// - [`ConfigError::PermissionDenied`], [`ConfigError::IoFailure`]: the file could not be
    ///   written (or read again after a conflict).
    pub fn apply(&mut self, change: Change) -> Result<(), ConfigError> {
        if change.holds_in(&self.config) {
            return Ok(());
        }
        change.apply_to(&mut self.config);
        self.rebuild();
        let mut retries = 0;
        loop {
            let Some(stores) = self.stores.as_ref() else {
                return Err(ConfigError::NotFound);
            };
            let error = match stores.config.commit(self.config.revision, &self.config) {
                Ok(revision) => {
                    self.config.revision = revision;
                    if !matches!(self.config_read, DocumentStatus::Loaded { .. }) {
                        self.config_read = DocumentStatus::Loaded { issues: Vec::new() };
                    }
                    self.rebuild();
                    return Ok(());
                }
                Err(error) => error,
            };
            if !matches!(error, ConfigError::Conflict { .. }) || retries == CONFLICT_RETRIES {
                return Err(error);
            }
            retries += 1;
            match read(&stores.config) {
                Read::Found(loaded) => {
                    self.config = loaded.document;
                    self.config_read = DocumentStatus::Loaded {
                        issues: loaded.issues,
                    };
                }
                Read::Missing => {
                    self.config = ConfigDocument::defaults();
                    self.config_read = DocumentStatus::Missing;
                }
                Read::Failed(error) => {
                    // The settings in effect (with the change) stay; the file is kept.
                    self.config_read = DocumentStatus::Unreadable(error);
                    self.rebuild();
                    return Err(error);
                }
            }
            let holds = change.holds_in(&self.config);
            change.apply_to(&mut self.config);
            self.rebuild();
            if holds {
                return Ok(());
            }
        }
    }

    /// Checks whether a file changed on disk since it was last read or written, even without a
    /// new revision, and loads every changed file again (the tray calls this every two
    /// seconds).
    ///
    /// `None` when nothing changed (or nothing can be read: [`Location::Unavailable`]).
    /// Otherwise [`Settings::status`] describes the new state, and [`Reloaded`] carries the
    /// snapshots that took effect: a missing file brings its defaults, while a file that cannot
    /// be read keeps the last good settings in effect (and is reported once, not on every
    /// poll while it stays so).
    pub fn poll(&mut self) -> Option<Reloaded> {
        let stores = self.stores.as_ref()?;
        let config = needs_reading(&stores.config, &self.config_read).then(|| read(&stores.config));
        let app_rules =
            needs_reading(&stores.app_rules, &self.app_rules_read).then(|| read(&stores.app_rules));
        let macros =
            needs_reading(&stores.macros, &self.status.macros).then(|| read(&stores.macros));
        let mut runtime_changed = false;
        let mut status_changed = false;
        match config {
            None => {}
            Some(Read::Found(loaded)) => {
                self.config = loaded.document;
                self.config_read = DocumentStatus::Loaded {
                    issues: loaded.issues,
                };
                runtime_changed = true;
            }
            Some(Read::Missing) => {
                self.config = ConfigDocument::defaults();
                self.config_read = DocumentStatus::Missing;
                runtime_changed = true;
            }
            Some(Read::Failed(error)) => {
                status_changed |= replace(&mut self.config_read, DocumentStatus::Unreadable(error));
            }
        }
        match app_rules {
            None => {}
            Some(Read::Found(loaded)) => {
                self.compatibility = loaded.document.compatibility;
                self.app_rules_read = DocumentStatus::Loaded {
                    issues: loaded.issues,
                };
                runtime_changed = true;
            }
            Some(Read::Missing) => {
                self.compatibility = CompatibilityConfig::defaults_v1();
                self.app_rules_read = DocumentStatus::Missing;
                runtime_changed = true;
            }
            Some(Read::Failed(error)) => {
                status_changed |=
                    replace(&mut self.app_rules_read, DocumentStatus::Unreadable(error));
            }
        }
        let mut macros_changed = false;
        match macros {
            None => {}
            Some(Read::Found(loaded)) => {
                self.install_macros(loaded);
                macros_changed = true;
            }
            Some(Read::Missing) => {
                self.macros = Arc::new(MacroSnapshot::empty(0));
                self.status.macros = DocumentStatus::Missing;
                self.status.skipped_macros.clear();
                macros_changed = true;
            }
            Some(Read::Failed(error)) => {
                status_changed |=
                    replace(&mut self.status.macros, DocumentStatus::Unreadable(error));
            }
        }
        if runtime_changed || status_changed {
            self.rebuild();
        }
        if !(runtime_changed || status_changed || macros_changed) {
            return None;
        }
        Some(Reloaded {
            runtime: runtime_changed.then(|| self.runtime.clone()),
            macros: macros_changed.then(|| Arc::clone(&self.macros)),
        })
    }

    /// Recovers from a `raumakey.toml` that cannot be read ([`DocumentStatus::Unreadable`]):
    /// moves it aside to `raumakey.toml.bak-<unix-ms>` and writes the settings in effect in
    /// its place, as revision 1. Only an explicit choice of the user should call this.
    ///
    /// Returns the path of the backup, or `None` when the file was readable (nothing is done)
    /// or was gone already.
    pub fn recover(&mut self) -> Result<Option<PathBuf>, ConfigError> {
        if !matches!(self.config_read, DocumentStatus::Unreadable(_)) {
            return Ok(None);
        }
        let Some(stores) = self.stores.as_ref() else {
            return Ok(None);
        };
        let backup = stores.config.set_aside()?;
        self.config_read = DocumentStatus::Missing;
        let committed = stores.config.commit(0, &self.config);
        if let Ok(revision) = committed {
            self.config.revision = revision;
            self.config_read = DocumentStatus::Loaded { issues: Vec::new() };
        }
        self.rebuild();
        committed.map(|_| backup)
    }

    /// Takes the entries of a loaded macro document into effect.
    fn install_macros(&mut self, loaded: Loaded<MacroDocument>) {
        let revision = loaded.document.revision;
        let (store, skipped) = MacroStore::from_entries_lenient(revision, loaded.document.entries);
        let mut entries = store.entries();
        let mut issues = loaded.issues;
        if entries.len() > MAX_MACROS {
            entries.truncate(MAX_MACROS);
            issues.push(out_of_range("macro"));
        }
        self.macros = Arc::new(MacroSnapshot {
            macro_revision: revision,
            entries,
        });
        self.status.macros = DocumentStatus::Loaded { issues };
        self.status.skipped_macros = skipped;
    }

    /// Builds the runtime snapshot of the settings in effect and the status of the files it
    /// comes from.
    fn rebuild(&mut self) {
        let built = build_runtime(&self.config, &self.compatibility);
        self.runtime = built.snapshot;
        self.status.config = match (&self.config_read, built.config_error) {
            (DocumentStatus::Loaded { .. }, Some(error)) => DocumentStatus::Unusable(error),
            (DocumentStatus::Loaded { issues }, None) => DocumentStatus::Loaded {
                issues: issues.iter().cloned().chain(built.config_issues).collect(),
            },
            (other, _) => other.clone(),
        };
        self.status.app_rules = match (&self.app_rules_read, built.app_rules_error) {
            (DocumentStatus::Loaded { .. }, Some(error)) => DocumentStatus::Unusable(error),
            (other, _) => other.clone(),
        };
    }
}

/// Stores `next` in `slot`; whether it differs from what was there.
fn replace(slot: &mut DocumentStatus, next: DocumentStatus) -> bool {
    let changed = *slot != next;
    *slot = next;
    changed
}

fn out_of_range(path: &str) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason: IssueReason::OutOfRange,
    }
}

/// A runtime snapshot and what it could not take as configured.
struct Built {
    snapshot: RuntimeSnapshot,
    /// Lists of `raumakey.toml` cut to a snapshot limit.
    config_issues: Vec<FieldIssue>,
    /// `raumakey.toml` could not make a valid snapshot (not expected for a decoded document).
    config_error: Option<IpcError>,
    /// The compatibility rules could not make a valid snapshot.
    app_rules_error: Option<IpcError>,
}

/// Builds the runtime snapshot of `config` with `compatibility`, so that it passes
/// [`RuntimeSnapshot::validate`] (see the module documentation for what is cut or replaced).
fn build_runtime(config: &ConfigDocument, compatibility: &CompatibilityConfig) -> Built {
    let mut config_issues = Vec::new();
    let options = config.engine_options();
    let mut keymaps = config.keymaps.clone();
    if keymaps.len() > MAX_KEYMAPS {
        // The engine reads only the key map of a user-defined method.
        let selected = match options.input_method {
            InputMethod::UserDefined => options.user_keymap_id.as_deref(),
            InputMethod::Telex | InputMethod::Vni | InputMethod::Viqr => None,
        };
        keymaps.retain(|map| Some(map.id.as_str()) == selected);
        config_issues.push(out_of_range("keymaps"));
    }
    let mut hotkeys = config.hotkeys.clone();
    for (path, list) in [
        ("hotkeys.toggle_mode", &mut hotkeys.toggle_mode),
        ("hotkeys.hard_reset", &mut hotkeys.hard_reset),
    ] {
        if list.len() > MAX_HOTKEYS {
            list.truncate(MAX_HOTKEYS);
            config_issues.push(out_of_range(path));
        }
    }
    let mut snapshot = RuntimeSnapshot {
        config_revision: config.revision,
        options,
        keymaps,
        hotkeys,
        compatibility: CompatibilityConfig::defaults_v1(),
        vietnamese_mode: config.input.enabled,
        diagnostics_enabled: config.system.diagnostics,
    };
    let mut config_error = None;
    if let Err(error) = snapshot.validate() {
        config_error = Some(error);
        config_issues.clear();
        snapshot = defaults_with_mode(config);
    }
    let mut app_rules_error = None;
    let defaults = core::mem::replace(&mut snapshot.compatibility, compatibility.clone());
    if let Err(error) = snapshot.validate() {
        app_rules_error = Some(error);
        snapshot.compatibility = defaults;
    }
    Built {
        snapshot,
        config_issues,
        config_error,
        app_rules_error,
    }
}

/// [`RuntimeSnapshot::defaults_v1`] with the revision and the live mode of `config`.
fn defaults_with_mode(config: &ConfigDocument) -> RuntimeSnapshot {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = config.revision;
    snapshot.options.enabled = config.input.enabled;
    snapshot.vietnamese_mode = config.input.enabled;
    snapshot
}

/// The local application data folder of the user.
#[cfg(windows)]
fn local_app_data() -> Option<PathBuf> {
    raumakey_windows::known_folder::local_app_data()
        .ok()
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
}

/// The local application data folder of the user.
#[cfg(not(windows))]
fn local_app_data() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(PathBuf::from)
}
