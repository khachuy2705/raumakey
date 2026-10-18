//! The configuration lifecycle of the portable controller (D38, D40).
//!
//! Raumakey is one portable `raumakey.exe` that the user starts directly, and its configuration
//! lives beside it. [`Settings`] owns that configuration for the run mode and the tray:
//!
//! - **Where.** [`Settings::open`] resolves the data root with
//!   [`DataRoot::resolve_with`](raumakey_config::DataRoot::resolve_with)
//!   ([`Location::ExeDir`] or [`Location::LocalAppData`]):
//!   1. the folder of `raumakey.exe` when `raumakey.toml` exists there, even when the folder
//!      cannot be written (it is then neither probed nor checked; saving fails, typically with
//!      [`ConfigError::PermissionDenied`], and the caller reports that);
//!   2. otherwise that folder when it is not protected and can be written;
//!   3. otherwise `%LOCALAPPDATA%\Raumakey`, which is also used when the executable's folder is
//!      unknown or not absolute (`current_exe` failed), without looking at it.
//!
//!   [`Settings::open_for_current_exe`] protects Program Files, Program Files (x86), the
//!   Windows folder, and the user's temp folder ([`is_protected_dir`]), and probes with
//!   [`is_writable_dir`]. So an elevated and a normal run of a copy in Program Files share one
//!   file under `%LOCALAPPDATA%`, and a copy that Explorer runs from inside a ZIP (out of the
//!   temp folder, which it cleans later) keeps nothing there. Trade-off: an elevated run reads
//!   the configuration beside `raumakey.exe` like a normal run does, so for a copy in a folder
//!   that the user's ordinary (medium-integrity) programs can write, such as Downloads, the
//!   elevated controller reads a file that those programs can change (they could replace
//!   `raumakey.exe` there as well). When no folder can be used ([`Location::Unavailable`]: no
//!   usable executable folder and no local application data folder), the settings live in
//!   memory: they start from the defaults, every change takes effect, and none is saved
//!   ([`ConfigError::NotFound`]).
//! - **First run.** When `raumakey.toml` does not exist, the defaults are committed at once, so
//!   the file exists from then on ([`OpenReport::first_run`]; a failure to write it is
//!   [`OpenReport::create_error`], and the controller runs on the defaults anyway; when another
//!   writer created the file meanwhile, that file is loaded instead). The second run loads the
//!   file and writes nothing. `raumakey-macros.toml` and
//!   `raumakey-app-rules.toml` are read when they exist and are never created or written here.
//! - **Snapshots.** [`Settings::runtime_snapshot`] and [`Settings::macro_snapshot`] are what the
//!   session core consumes (`ServiceCore::apply_runtime_snapshot` and `build_macro_table`):
//!   the runtime snapshot always passes [`RuntimeSnapshot::validate`] (including the key map of
//!   a user-defined method), and the macro snapshot always passes [`MacroSnapshot::validate`]
//!   and macro validation. The engine options come from
//!   [`ConfigDocument::engine_options`], the live mode from `[input] enabled`, the hotkeys from
//!   `[hotkeys]` (default toggle `Ctrl+Shift`, D5), diagnostics from `[system] diagnostics`,
//!   and the compatibility rules from `raumakey-app-rules.toml` (the D13 defaults without it)
//!   after the rules of the excluded applications.
//! - **Excluded applications (D47).** Each application of `[exclusions] apps` in
//!   `raumakey.toml` (by default
//!   [`ExclusionsSection::DEFAULT_APPS`](raumakey_config::ExclusionsSection::DEFAULT_APPS),
//!   the Remote Desktop and Hyper-V clients) becomes a rule for its executable, without a
//!   window class, that sets `vietnamese = false` and `macros = false`, placed before the rules
//!   of `raumakey-app-rules.toml`. The policy reads the rules narrowed to the host's window
//!   class first, then the other rules of the executable in order, then the named sets, and
//!   the first that sets a field decides it: an exclusion wins over every rule without a
//!   window class and every named set of that file, while a window-class rule there for the
//!   same executable still decides, in windows of its class, the fields it sets. So in an
//!   excluded application nothing is transformed and no macro expands, whatever the mode
//!   (`[macros] when_disabled` included), and the mode hotkey still switches the global mode.
//!   A hand edit of `[exclusions]` takes effect through [`Settings::poll`] like any other, and
//!   changes from the tray menu and the panel keep the list, since a commit writes the whole
//!   document.
//! - **Changes from the tray menu and the settings panel** ([`Change`], [`Settings::apply`])
//!   are committed at once. When the commit reports a conflict, because the file was edited
//!   by hand or by another writer since it was loaded, the file is loaded again, the change is
//!   applied again on what it holds, and the commit is retried (up to three times). A change
//!   takes effect even when it cannot be saved; the error says why it was not. Such a change
//!   is kept: it stays in effect over later reloads, the next commit writes it, and
//!   [`Settings::save_unsaved`] retries it (the worker does at every poll and before it ends).
//!   The kept changes are the last of each kind, applied again in the order they were last
//!   made, which ends as the whole sequence did: "Mặc định" ([`Change::TypingDefaults`])
//!   resets a switch changed before it, and a switch changed after it stays.
//! - **Hand edits.** [`Settings::poll`] (called every two seconds) notices a file that changed
//!   on disk, even without a new revision, loads it again, and returns the new snapshots
//!   ([`Reloaded`]). A file whose last read failed in a way that may pass (another program
//!   held it open, [`ConfigError::IoFailure`]; access denied for a moment,
//!   [`ConfigError::PermissionDenied`]) is read again on every poll until it reads.
//! - **Threads.** [`Settings`] is `Send` (and `Sync`): the controller owns it on a worker
//!   thread. [`Settings::apply`], [`Settings::poll`], and [`Settings::recover`] do file I/O and
//!   may wait up to two seconds for a store lock, so they do not belong on the tray's window
//!   thread.
//!
//! # Damaged files
//!
//! A file that cannot be read or decoded ([`DocumentStatus::Unreadable`]) never stops the
//! controller and is never overwritten silently. At start the defaults are in effect; when a
//! file breaks while the controller runs (a hand edit saved half-way), the last good settings
//! stay in effect. Changes take effect but are not saved: the store refuses to commit over a
//! corrupt file or one of a newer schema version, and [`Settings::apply`] returns that error.
//! Nothing is lost until the user decides: [`Settings::recover`] loads the file once more (and
//! takes it when it reads now), else moves the damaged `raumakey.toml` aside (to
//! `raumakey.toml.bak-<unix-ms>`) and writes the settings in effect in its place
//! ([`Recovery`]); fixing the file by hand brings it back through [`Settings::poll`].
//!
//! # What is reported
//!
//! [`Status`] lists, per file, whether it is missing, loaded with the fields that took their
//! defaults ([`FieldIssue`]: schema paths and reason codes, never values), unreadable, or
//! unusable, and which macro entries were left out ([`EntryIssue`]: positions, ids, and error
//! codes). Lists longer than the channel's limits ([`MAX_KEYMAPS`], [`MAX_HOTKEYS`],
//! [`MAX_MACROS`], [`MAX_APP_RULES`]) are cut for the snapshot and reported on their path
//! ([`IssueReason::OutOfRange`] on `keymaps`, `hotkeys.toggle_mode`, `hotkeys.hard_reset`,
//! `macro`, `exclusions.apps`, or `rule`); the documents keep every element. Beyond
//! [`MAX_KEYMAPS`] key maps, only the key map that the input method uses (if any) is carried,
//! as it is the only one the engine reads. The excluded applications take the first of the
//! [`MAX_APP_RULES`] rules, so the last rules of `raumakey-app-rules.toml` are cut when they
//! no longer fit after them, and `rule` is reported on that file. Compatibility rules that
//! the session core would refuse make `raumakey-app-rules.toml` [`DocumentStatus::Unusable`],
//! with the D13 defaults in effect (after the exclusions, as always). After a commit, the status
//! of `raumakey.toml` is what a load of the written file reports: fields that took their
//! defaults were written with them and are not reported any more. A commit writes the
//! canonical encoding, so comments and the layout of a hand-edited file are not kept.
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
use raumakey_ipc::message::{
    MAX_APP_RULES, MAX_HOTKEYS, MAX_KEYMAPS, MAX_MACROS, MacroSnapshot, RuntimeSnapshot,
};
use raumakey_macro::{EntryIssue, MacroStore};
use raumakey_model::host::ExeName;
use raumakey_model::options::InputMethod;
use raumakey_model::policy::{AppRule, CompatibilityConfig, RuleAction};

use crate::system::is_inside_folder;

/// How long a commit waits for a store lock, shorter than the store's default of ten seconds
/// so that a change from the tray is answered soon.
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

/// A change made from the tray menu or the settings panel (D42, D43).
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
    /// Marks typed after the final consonant (`[input] free_marking`).
    FreeMarking(bool),
    /// Restoring the typed keys of a word that is not Vietnamese
    /// (`[input] auto_restore_invalid`).
    AutoRestore(bool),
    /// Macro expansion (`[macros] enabled`).
    Macros(bool),
    /// Macro expansion in English mode too (`[macros] when_disabled`).
    MacrosWhenEnglish(bool),
    /// Macro replacements that follow the case of the typed trigger (`[macros] auto_case`).
    MacroAutoCase(bool),
    /// How Raumakey starts with Windows (`[system] startup`). Only the choice is stored; the
    /// caller creates or removes the shortcut (`crate::startup`).
    Startup(StartupMode),
    /// The settings panel opens when the controller starts (`[system] show_panel_on_start`).
    ShowPanelOnStart(bool),
    /// "Mặc định" of the panel: the input method, `modern_tone`, `free_marking`,
    /// `spell_check`, `auto_restore_invalid`, and the three switches of `[macros]` take the
    /// schema defaults ([`ConfigDocument::defaults`]). Nothing else changes: not the mode,
    /// the key map of a user method, the normalization, the context limit, the hotkeys, or
    /// `[system]`.
    TypingDefaults,
}

impl Change {
    /// Whether `document` holds the value of this change already.
    fn holds_in(self, document: &ConfigDocument) -> bool {
        let (input, macros) = (&document.input, &document.macros);
        match self {
            Self::VietnameseMode(on) => input.enabled == on,
            Self::InputMethod(method) => input.method == method,
            Self::SpellCheck(on) => input.spell_check == on,
            Self::ModernTone(on) => input.modern_tone == on,
            Self::FreeMarking(on) => input.free_marking == on,
            Self::AutoRestore(on) => input.auto_restore_invalid == on,
            Self::Macros(on) => macros.enabled == on,
            Self::MacrosWhenEnglish(on) => macros.when_disabled == on,
            Self::MacroAutoCase(on) => macros.auto_case == on,
            Self::Startup(mode) => document.system.startup == mode,
            Self::ShowPanelOnStart(on) => document.system.show_panel_on_start == on,
            Self::TypingDefaults => {
                let mut reset = document.clone();
                Self::TypingDefaults.apply_to(&mut reset);
                reset == *document
            }
        }
    }

    fn apply_to(self, document: &mut ConfigDocument) {
        let (input, macros) = (&mut document.input, &mut document.macros);
        match self {
            Self::VietnameseMode(on) => input.enabled = on,
            Self::InputMethod(method) => input.method = method,
            Self::SpellCheck(on) => input.spell_check = on,
            Self::ModernTone(on) => input.modern_tone = on,
            Self::FreeMarking(on) => input.free_marking = on,
            Self::AutoRestore(on) => input.auto_restore_invalid = on,
            Self::Macros(on) => macros.enabled = on,
            Self::MacrosWhenEnglish(on) => macros.when_disabled = on,
            Self::MacroAutoCase(on) => macros.auto_case = on,
            Self::Startup(mode) => document.system.startup = mode,
            Self::ShowPanelOnStart(on) => document.system.show_panel_on_start = on,
            Self::TypingDefaults => {
                let ConfigDocument {
                    input: defaults,
                    macros: macro_defaults,
                    ..
                } = ConfigDocument::defaults();
                input.method = defaults.method;
                input.modern_tone = defaults.modern_tone;
                input.free_marking = defaults.free_marking;
                input.spell_check = defaults.spell_check;
                input.auto_restore_invalid = defaults.auto_restore_invalid;
                macros.enabled = macro_defaults.enabled;
                macros.when_disabled = macro_defaults.when_disabled;
                macros.auto_case = macro_defaults.auto_case;
            }
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

/// What [`Settings::recover`] did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// `raumakey.toml` was not unreadable: nothing was done.
    NotNeeded,
    /// `raumakey.toml` reads again (another program had held it, for example): its settings
    /// are in effect, and nothing was moved.
    Adopted,
    /// The damaged file was moved aside and the settings in effect were to be written in its
    /// place.
    Replaced {
        /// Where the damaged file was moved (`raumakey.toml.bak-<unix-ms>`); `None` when it
        /// was gone already.
        backup: Option<PathBuf>,
        /// Whether the settings in effect were written as the new `raumakey.toml`. When not,
        /// the file is missing and the next change tries again.
        saved: Result<(), ConfigError>,
    },
}

/// The stores of the data root.
struct Stores {
    config: AtomicStore<ConfigDocument>,
    macros: AtomicStore<MacroDocument>,
    app_rules: AtomicStore<AppRulesDocument>,
}

impl Stores {
    fn new(root: &DataRoot, replacer: Arc<dyn FileReplacer>) -> Self {
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

/// The file replacer of the stores: `ReplaceFileW` on Windows. The macro list editor
/// ([`crate::macro_list`]) writes `raumakey-macros.toml` with it too.
pub(crate) fn replacer() -> Arc<dyn FileReplacer> {
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

/// `raumakey.toml` as [`Settings::open`] finds it.
struct Initial {
    config: ConfigDocument,
    config_read: DocumentStatus,
    first_run: bool,
    create_error: Option<ConfigError>,
}

impl Initial {
    fn found(loaded: Loaded<ConfigDocument>) -> Self {
        Self {
            config: loaded.document,
            config_read: DocumentStatus::Loaded {
                issues: loaded.issues,
            },
            first_run: false,
            create_error: None,
        }
    }

    fn unreadable(error: ConfigError) -> Self {
        Self {
            config: ConfigDocument::defaults(),
            config_read: DocumentStatus::Unreadable(error),
            first_run: false,
            create_error: None,
        }
    }
}

/// The first run: `raumakey.toml` was not there, so the defaults are written. When another
/// writer created the file meanwhile (the commit reports a conflict), that file is loaded and
/// reported instead, as on any later run.
fn create_defaults(store: &AtomicStore<ConfigDocument>) -> Initial {
    let mut config = ConfigDocument::defaults();
    match store.commit(0, &config) {
        Ok(revision) => {
            config.revision = revision;
            Initial {
                config,
                config_read: DocumentStatus::Loaded { issues: Vec::new() },
                first_run: true,
                create_error: None,
            }
        }
        Err(error @ ConfigError::Conflict { .. }) => match read(store) {
            Read::Found(loaded) => Initial::found(loaded),
            Read::Failed(error) => Initial::unreadable(error),
            Read::Missing => Initial {
                config,
                config_read: DocumentStatus::Missing,
                first_run: true,
                create_error: Some(error),
            },
        },
        Err(error) => Initial {
            config,
            config_read: DocumentStatus::Missing,
            first_run: true,
            create_error: Some(error),
        },
    }
}

/// Whether the file of `store` should be read again: the last read failed in a way that may
/// pass (another program held the file open, access was denied for a moment), or the file
/// changed since the store last read or wrote it, or it cannot even be checked, unless the
/// last read failed with that very error. A failed read leaves the store's record of the last
/// good content in place, so after a passing failure the file looks unchanged and only reading
/// it again ends the failure. A read that fails again the same way is not reported again
/// ([`Settings::poll`] compares the status).
fn needs_reading<D: StoreDocument>(store: &AtomicStore<D>, last: &DocumentStatus) -> bool {
    if matches!(
        last,
        DocumentStatus::Unreadable(ConfigError::IoFailure | ConfigError::PermissionDenied)
    ) {
        return true;
    }
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
    /// Changes in effect that no commit has written yet, the last of each setting
    /// ([`Settings::save_unsaved`]).
    unsaved: Vec<Change>,
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
            .field("unsaved", &self.unsaved.len())
            .finish_non_exhaustive()
    }
}

impl Settings {
    /// Opens the configuration of a `raumakey.exe` in `exe_dir` (see the [module
    /// documentation](self)): resolves the data root, asking `is_protected` whether Raumakey
    /// keeps no configuration in `exe_dir` ([`is_protected_dir`] is the real check) and then
    /// `is_writable` whether it can be written ([`is_writable_dir`] is the real probe), unless
    /// `exe_dir` holds `raumakey.toml` already or is not absolute; loads `raumakey.toml`,
    /// writes the defaults when it does not exist, and loads `raumakey-macros.toml` and
    /// `raumakey-app-rules.toml` when they exist.
    ///
    /// Never fails: whatever cannot be read leaves its defaults in effect and is reported.
    pub fn open(
        exe_dir: &Path,
        local_app_data: Option<&Path>,
        is_writable: &dyn Fn(&Path) -> bool,
        is_protected: &dyn Fn(&Path) -> bool,
    ) -> (Self, OpenReport) {
        Self::open_with(
            exe_dir,
            local_app_data,
            is_writable,
            is_protected,
            replacer(),
        )
    }

    /// [`Settings::open`] with the stores' file replacer.
    fn open_with(
        exe_dir: &Path,
        local_app_data: Option<&Path>,
        is_writable: &dyn Fn(&Path) -> bool,
        is_protected: &dyn Fn(&Path) -> bool,
        replacer: Arc<dyn FileReplacer>,
    ) -> (Self, OpenReport) {
        let Ok(root) = DataRoot::resolve_with(exe_dir, local_app_data, is_writable, is_protected)
        else {
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
        let stores = Stores::new(&root, replacer);
        let Initial {
            config,
            config_read,
            first_run,
            create_error,
        } = match read(&stores.config) {
            Read::Found(loaded) => Initial::found(loaded),
            Read::Missing => create_defaults(&stores.config),
            Read::Failed(error) => Initial::unreadable(error),
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
            unsaved: Vec::new(),
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
    /// ([`std::env::current_exe`]; when that fails, the local application data folder is
    /// used), the local application data folder (`FOLDERID_LocalAppData` on Windows, else
    /// `%LOCALAPPDATA%`), the real protection check [`is_protected_dir`], and the real write
    /// probe [`is_writable_dir`].
    pub fn open_for_current_exe() -> (Self, OpenReport) {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let local_app_data = local_app_data();
        Self::open(
            &exe_dir,
            local_app_data.as_deref(),
            &is_writable_dir,
            &is_protected_dir,
        )
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
            unsaved: Vec::new(),
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
    ///
    /// A change that could not be saved is kept ([`Settings::has_unsaved`]): it stays in effect
    /// over later reloads, the next commit writes it with its own change, and
    /// [`Settings::save_unsaved`] retries it. Without a folder ([`Location::Unavailable`])
    /// nothing is kept, since nothing can ever be saved.
    pub fn apply(&mut self, change: Change) -> Result<(), ConfigError> {
        if change.holds_in(&self.config) {
            return Ok(());
        }
        change.apply_to(&mut self.config);
        self.rebuild();
        if self.stores.is_none() {
            return Err(ConfigError::NotFound);
        }
        self.remember(change);
        self.commit_unsaved()
    }

    /// Whether a change in effect was not saved yet (see [`Settings::apply`]).
    pub fn has_unsaved(&self) -> bool {
        !self.unsaved.is_empty()
    }

    /// Commits the changes that could not be saved before, on what the file holds now:
    /// `Ok(true)` when they were written, `Ok(false)` when there were none. On failure they
    /// stay in effect and are kept for the next try. The worker calls this at every poll and
    /// before it ends.
    pub fn save_unsaved(&mut self) -> Result<bool, ConfigError> {
        if self.unsaved.is_empty() {
            return Ok(false);
        }
        self.commit_unsaved().map(|()| true)
    }

    /// Keeps `change` as unsaved, replacing an earlier change of the same kind; the kept
    /// changes stay in the order they were last made, so applying them again ends as the
    /// whole sequence of changes did.
    fn remember(&mut self, change: Change) {
        let kind = core::mem::discriminant(&change);
        self.unsaved
            .retain(|kept| core::mem::discriminant(kept) != kind);
        self.unsaved.push(change);
    }

    /// Applies the unsaved changes on the document in effect (after a reload).
    fn apply_unsaved(&mut self) {
        for change in &self.unsaved {
            change.apply_to(&mut self.config);
        }
    }

    /// Commits the settings in effect, which hold every unsaved change. When the commit
    /// reports a conflict, the file is loaded again, the unsaved changes are applied again on
    /// what it holds, and the commit is retried (up to three times). Success clears them.
    fn commit_unsaved(&mut self) -> Result<(), ConfigError> {
        let mut retries = 0;
        loop {
            let Some(stores) = self.stores.as_ref() else {
                return Err(ConfigError::NotFound);
            };
            let error = match stores.config.commit(self.config.revision, &self.config) {
                Ok(revision) => {
                    self.committed(revision);
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
                    // The settings in effect (with the changes) stay; the file is kept.
                    self.config_read = DocumentStatus::Unreadable(error);
                    self.rebuild();
                    return Err(error);
                }
            }
            let holds = self
                .unsaved
                .iter()
                .all(|change| change.holds_in(&self.config));
            self.apply_unsaved();
            self.rebuild();
            if holds {
                // The file holds them already: nothing is left to write.
                self.unsaved.clear();
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
                // Changes not saved yet stay in effect on the edited file.
                self.apply_unsaved();
                runtime_changed = true;
            }
            Some(Read::Missing) => {
                self.config = ConfigDocument::defaults();
                self.config_read = DocumentStatus::Missing;
                self.apply_unsaved();
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

    /// Recovers from a `raumakey.toml` that cannot be read ([`DocumentStatus::Unreadable`]).
    /// Only an explicit choice of the user should call this.
    ///
    /// The file is loaded once more first: when it reads now (the failure has passed), its
    /// settings take effect and nothing is moved ([`Recovery::Adopted`]). Otherwise it is
    /// moved aside to `raumakey.toml.bak-<unix-ms>`, and the settings in effect are written in
    /// its place as revision 1 ([`Recovery::Replaced`], which names the backup even when that
    /// write fails). [`Recovery::NotNeeded`] when the file was not unreadable.
    ///
    /// Fails, with nothing changed, when the file cannot be moved aside.
    pub fn recover(&mut self) -> Result<Recovery, ConfigError> {
        if !matches!(self.config_read, DocumentStatus::Unreadable(_)) {
            return Ok(Recovery::NotNeeded);
        }
        let Some(stores) = self.stores.as_ref() else {
            return Ok(Recovery::NotNeeded);
        };
        let backup = match read(&stores.config) {
            Read::Found(loaded) => {
                self.config = loaded.document;
                self.config_read = DocumentStatus::Loaded {
                    issues: loaded.issues,
                };
                self.apply_unsaved();
                self.rebuild();
                return Ok(Recovery::Adopted);
            }
            Read::Missing => None,
            Read::Failed(_) => stores.config.set_aside()?,
        };
        self.config_read = DocumentStatus::Missing;
        let saved = match stores.config.commit(0, &self.config) {
            Ok(revision) => {
                self.committed(revision);
                Ok(())
            }
            Err(error) => {
                self.rebuild();
                Err(error)
            }
        };
        Ok(Recovery::Replaced { backup, saved })
    }

    /// `raumakey.toml` now holds [`Settings::document`] as `revision`: its status becomes what
    /// a load of that file would report.
    fn committed(&mut self, revision: u64) {
        // The commit wrote the whole document in effect, unsaved changes included.
        self.unsaved.clear();
        self.config.revision = revision;
        self.config_read = DocumentStatus::Loaded {
            issues: committed_issues(&self.config),
        };
        self.rebuild();
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
            (DocumentStatus::Loaded { issues }, None) => DocumentStatus::Loaded {
                issues: issues
                    .iter()
                    .cloned()
                    .chain(built.app_rules_issues)
                    .collect(),
            },
            (other, _) => other.clone(),
        };
    }
}

/// The issues that a load of the file written for `document` reports: a commit writes the
/// defaults of the fields that took them, so only what is reported on every load is left
/// (entries set aside verbatim, a user method without its key map). The store writes exactly
/// this encoding.
fn committed_issues(document: &ConfigDocument) -> Vec<FieldIssue> {
    document
        .encode()
        .and_then(|text| ConfigDocument::decode(&text))
        .map(|loaded| loaded.issues)
        .unwrap_or_default()
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
    /// Lists of `raumakey-app-rules.toml` cut to a snapshot limit (`rule`, after the
    /// exclusions).
    app_rules_issues: Vec<FieldIssue>,
}

/// The rule of an application of `[exclusions]` (D47): neither Vietnamese nor macros.
fn exclusion_rule(exe: &ExeName) -> AppRule {
    AppRule {
        exe: exe.clone(),
        window_class: None,
        action: RuleAction {
            vietnamese: Some(false),
            macros: Some(false),
            ..RuleAction::default()
        },
    }
}

/// Puts the rule of each application of `apps` before the rules of `compatibility`, keeping at
/// most [`MAX_APP_RULES`]: the exclusions first (beyond the limit, `exclusions.apps` is cut and
/// reported in `config_issues`), then as many rules of the app-rules file as still fit (when
/// some do not, `rule` is reported in `app_rules_issues`).
fn put_exclusions_first(
    compatibility: &mut CompatibilityConfig,
    apps: &[ExeName],
    config_issues: &mut Vec<FieldIssue>,
    app_rules_issues: &mut Vec<FieldIssue>,
) {
    let excluded = apps.get(..MAX_APP_RULES).unwrap_or(apps);
    if excluded.len() < apps.len() {
        config_issues.push(out_of_range("exclusions.apps"));
    }
    let room = MAX_APP_RULES - excluded.len();
    if compatibility.rules.len() > room {
        compatibility.rules.truncate(room);
        app_rules_issues.push(out_of_range("rule"));
    }
    let mut rules: Vec<AppRule> = excluded.iter().map(exclusion_rule).collect();
    rules.append(&mut compatibility.rules);
    compatibility.rules = rules;
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
    let mut app_rules_issues = Vec::new();
    let without_exclusions = snapshot.compatibility.clone();
    put_exclusions_first(
        &mut snapshot.compatibility,
        &config.exclusions.apps,
        &mut config_issues,
        &mut app_rules_issues,
    );
    if snapshot.validate().is_err() {
        // Not expected: every name of a decoded document reads back as itself.
        snapshot.compatibility = without_exclusions;
        app_rules_issues.clear();
        config_issues.push(FieldIssue {
            path: "exclusions.apps".to_owned(),
            reason: IssueReason::InvalidValue,
        });
    }
    Built {
        snapshot,
        config_issues,
        config_error,
        app_rules_error,
        app_rules_issues,
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

/// Whether Raumakey keeps no configuration in `dir` even when it can be written: the
/// protection check of [`Settings::open_for_current_exe`] (see the [module
/// documentation](self)).
///
/// `dir` is protected when a file in it lies inside Program Files, Program Files (x86), or the
/// Windows folder (`raumakey_windows::known_folder`), or inside the user's temp folder
/// ([`std::env::temp_dir`]): Explorer runs an executable from inside a ZIP out of the temp
/// folder and deletes it later. The check is lexical ([`is_inside_folder`]), on each path as
/// given and in its canonical form (links and short names resolved), so that `C:\PROGRA~1` or
/// a `\\?\` path still match. A folder that cannot be looked up protects nothing; a relative
/// or empty `dir` is not protected (and is never used as the root). A 32-bit `raumakey.exe` on
/// 64-bit Windows sees only Program Files (x86) as its Program Files.
pub fn is_protected_dir(dir: &Path) -> bool {
    let mut folders = Vec::new();
    #[cfg(windows)]
    {
        use raumakey_windows::known_folder::{program_files, program_files_x86, windows_dir};
        folders.extend(
            [program_files(), program_files_x86(), windows_dir()]
                .into_iter()
                .flatten(),
        );
    }
    folders.push(std::env::temp_dir());
    let resolved: Vec<PathBuf> = folders.iter().filter_map(|f| canonical(f)).collect();
    folders.extend(resolved);
    [Some(dir.to_path_buf()), canonical(dir)]
        .into_iter()
        .flatten()
        .any(|dir| {
            let file = dir.join(ConfigDocument::FILE_NAME);
            folders.iter().any(|folder| is_inside_folder(&file, folder))
        })
}

/// Whether `path` (such as the running `raumakey.exe`) lies inside the user's temporary folder
/// ([`std::env::temp_dir`]), where Explorer runs an executable from inside a ZIP and deletes it
/// later: a Startup shortcut to it would soon start nothing (D41). Compared like
/// [`is_protected_dir`], as given and in canonical form; a relative or empty path is not
/// temporary.
pub fn is_temporary_path(path: &Path) -> bool {
    let temp = std::env::temp_dir();
    let folders: Vec<PathBuf> = [Some(temp.clone()), canonical(&temp)]
        .into_iter()
        .flatten()
        .collect();
    let parent_resolved = path
        .parent()
        .and_then(canonical)
        .zip(path.file_name())
        .map(|(dir, name)| dir.join(name));
    [Some(path.to_path_buf()), parent_resolved]
        .into_iter()
        .flatten()
        .any(|path| folders.iter().any(|folder| is_inside_folder(&path, folder)))
}

/// `path` with links and short names resolved and without the `\\?\` prefix; `None` when it
/// cannot be resolved or does not lie on a drive.
fn canonical(path: &Path) -> Option<PathBuf> {
    let resolved = std::fs::canonicalize(path).ok()?;
    let plain = resolved.to_str()?.strip_prefix(r"\\?\")?;
    (!plain.starts_with(r"UNC\")).then(|| PathBuf::from(plain))
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;

    use raumakey_config::StdReplacer;

    use super::*;

    /// A replacer whose every swap fails, so that every commit fails after its checks.
    struct FailingReplacer;

    impl FileReplacer for FailingReplacer {
        fn replace(&self, _replacement: &Path, _target: &Path) -> io::Result<()> {
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        }
    }

    /// The damaged file was moved aside, but the settings could not be written in its place:
    /// the backup is still reported, so that the user can find the old file.
    #[test]
    fn recover_reports_the_backup_when_the_new_file_cannot_be_written() {
        let exe = tempfile::tempdir().unwrap();
        let config = exe.path().join("raumakey.toml");
        fs::write(&config, "[[[ not a configuration").unwrap();
        let (mut settings, _) = Settings::open_with(
            exe.path(),
            None,
            &|_| true,
            &|_| false,
            Arc::new(FailingReplacer),
        );
        let Ok(Recovery::Replaced {
            backup: Some(backup),
            saved,
        }) = settings.recover()
        else {
            panic!("not replaced");
        };
        assert_eq!(saved, Err(ConfigError::PermissionDenied));
        assert_eq!(fs::read(&backup).unwrap(), b"[[[ not a configuration");
        assert!(!config.exists());
        assert_eq!(settings.status().config, DocumentStatus::Missing);
    }

    /// Another writer created `raumakey.toml` between the open's read (missing) and its
    /// commit of the defaults: that file is loaded and reported, not a create error.
    #[test]
    fn a_configuration_created_meanwhile_is_loaded_instead_of_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let store: AtomicStore<ConfigDocument> =
            AtomicStore::new(dir.path(), Arc::new(StdReplacer));
        assert_eq!(store.load().unwrap_err(), ConfigError::NotFound);
        let theirs = "version = 1\nrevision = 5\n[input]\nmethod = \"vni\"\n";
        fs::write(dir.path().join("raumakey.toml"), theirs).unwrap();
        let initial = create_defaults(&store);
        assert_eq!(initial.create_error, None);
        assert!(!initial.first_run);
        assert_eq!(initial.config.revision, 5);
        assert_eq!(initial.config.input.method, InputMethod::Vni);
        assert_eq!(
            initial.config_read,
            DocumentStatus::Loaded { issues: Vec::new() }
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("raumakey.toml")).unwrap(),
            theirs
        );
    }
}
