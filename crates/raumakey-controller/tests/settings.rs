//! The configuration lifecycle of the portable controller (`raumakey_controller::settings`,
//! D40): where `raumakey.toml` lies, the first run, loading, changes from the tray, hand edits,
//! damaged files, and the snapshots the session core consumes.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raumakey_config::{
    AtomicStore, ConfigDocument, ConfigError, ExclusionsSection, FieldIssue, IssueReason,
    StartupMode, StdReplacer, StoreDocument, is_writable_dir,
};
use raumakey_controller::exclusions::ExclusionsRequest;
#[cfg(windows)]
use raumakey_controller::settings::is_protected_dir;
use raumakey_controller::settings::{
    Change, DocumentStatus, Location, OpenReport, Recovery, Settings, is_temporary_path,
};
use raumakey_ipc::message::{
    MAX_APP_RULES, MAX_HOTKEYS, MAX_KEYMAPS, MAX_NAMED_SETS, RuntimeSnapshot,
};
use raumakey_macro::{EntryError, EntryIssue, MacroError, MacroTable};
use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::hotkey::{Hotkey, HotkeyConfig};
use raumakey_model::macros::MacroId;
use raumakey_model::options::{EngineOptions, InputMethod, OutputCharset};
use raumakey_model::policy::{AppRule, CompatibilityConfig, HostPolicy, RuleAction};
use raumakey_policy::CompatibilityPolicy;

/// A folder for `raumakey.exe` and a local application data folder.
struct Folders {
    exe: tempfile::TempDir,
    local: tempfile::TempDir,
}

impl Folders {
    fn new() -> Self {
        Self {
            exe: tempfile::tempdir().unwrap(),
            local: tempfile::tempdir().unwrap(),
        }
    }

    fn exe(&self) -> &Path {
        self.exe.path()
    }

    fn local(&self) -> &Path {
        self.local.path()
    }

    /// Opens with a writable executable folder.
    fn open(&self) -> (Settings, OpenReport) {
        Settings::open(self.exe(), Some(self.local()), &|_| true, &|_| false)
    }

    /// Opens with an executable folder that cannot be written.
    fn open_unwritable(&self) -> (Settings, OpenReport) {
        Settings::open(self.exe(), Some(self.local()), &|_| false, &|_| false)
    }

    fn config(&self) -> PathBuf {
        self.exe().join("raumakey.toml")
    }

    fn macros(&self) -> PathBuf {
        self.exe().join("raumakey-macros.toml")
    }

    fn app_rules(&self) -> PathBuf {
        self.exe().join("raumakey-app-rules.toml")
    }
}

/// Every name in `dir`, sorted.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// The configuration document stored in `path`.
fn stored(path: &Path) -> ConfigDocument {
    ConfigDocument::decode(&fs::read_to_string(path).unwrap())
        .unwrap()
        .document
}

fn issue(path: &str, reason: IssueReason) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason,
    }
}

fn hotkey(text: &str) -> Hotkey {
    text.parse().unwrap()
}

fn loaded_clean() -> DocumentStatus {
    DocumentStatus::Loaded { issues: Vec::new() }
}

/// The rule that excludes `exe` (D47): neither Vietnamese nor macros.
fn exclusion(exe: &str) -> AppRule {
    AppRule {
        exe: ExeName::new(exe).unwrap(),
        window_class: None,
        action: RuleAction {
            vietnamese: Some(false),
            macros: Some(false),
            ..RuleAction::default()
        },
    }
}

/// `compatibility` with the rule of each of `apps` before its own rules.
fn with_exclusions(apps: &[&str], mut compatibility: CompatibilityConfig) -> CompatibilityConfig {
    let mut rules: Vec<AppRule> = apps.iter().map(|exe| exclusion(exe)).collect();
    rules.append(&mut compatibility.rules);
    compatibility.rules = rules;
    compatibility
}

/// The policy that `runtime` gives a window of class `class` in `exe`.
fn policy_of(runtime: &RuntimeSnapshot, exe: &str, class: Option<&str>) -> HostPolicy {
    let mut host = HostContext::new(SessionId::from_u128(1), ExeName::new(exe).unwrap());
    host.window_class = class.map(str::to_owned);
    CompatibilityPolicy::new(Arc::new(runtime.compatibility.clone())).resolve(&host)
}

/// Checks that the session core would install both snapshots.
fn assert_consumable(settings: &Settings) {
    let runtime = settings.runtime_snapshot();
    runtime.validate().unwrap();
    if runtime.options.input_method == InputMethod::UserDefined {
        let id = runtime.options.user_keymap_id.as_deref();
        assert!(
            runtime
                .keymaps
                .iter()
                .any(|map| Some(map.id.as_str()) == id)
        );
    }
    let macros = settings.macro_snapshot();
    macros.validate().unwrap();
    MacroTable::new(macros.entries.iter().cloned()).unwrap();
}

#[test]
fn the_first_run_writes_the_defaults_into_the_executable_folder() {
    let folders = Folders::new();
    let (settings, report) = folders.open();
    assert!(report.first_run);
    assert_eq!(report.location, Location::ExeDir);
    assert_eq!(
        report.config_path.as_deref(),
        Some(folders.config().as_path())
    );
    assert_eq!(report.create_error, None);
    assert_eq!(report.status.config, loaded_clean());
    assert_eq!(report.status.macros, DocumentStatus::Missing);
    assert_eq!(report.status.app_rules, DocumentStatus::Missing);
    assert!(report.status.skipped_macros.is_empty());
    assert_eq!(settings.location(), Location::ExeDir);
    assert_eq!(settings.config_path(), Some(folders.config().as_path()));
    assert_eq!(settings.status(), &report.status);

    let mut expected = ConfigDocument::defaults();
    expected.revision = 1;
    assert_eq!(stored(&folders.config()), expected);
    assert_eq!(settings.document(), &expected);
    // Only the configuration and its lock: the macros and app rules files are not created,
    // and nothing lands in the local application data folder.
    assert_eq!(
        listing(folders.exe()),
        vec!["raumakey.toml".to_owned(), "raumakey.toml.lock".to_owned()]
    );
    assert!(listing(folders.local()).is_empty());
}

/// With the real probe, the executable folder holds exactly the configuration and its lock.
#[test]
fn the_real_write_probe_leaves_only_the_configuration_behind() {
    let folders = Folders::new();
    let (_settings, report) = Settings::open(
        folders.exe(),
        Some(folders.local()),
        &is_writable_dir,
        &|_| false,
    );
    assert_eq!(report.location, Location::ExeDir);
    assert!(report.first_run);
    assert_eq!(
        listing(folders.exe()),
        vec!["raumakey.toml".to_owned(), "raumakey.toml.lock".to_owned()]
    );
}

#[test]
fn the_default_snapshots_are_defaults_v2_with_the_ctrl_shift_toggle() {
    let folders = Folders::new();
    let (settings, _) = folders.open();
    let runtime = settings.runtime_snapshot();
    assert_eq!(runtime.config_revision, 1);
    assert_eq!(runtime.options, EngineOptions::defaults_v2());
    assert!(runtime.vietnamese_mode);
    assert!(settings.vietnamese_mode());
    assert!(!runtime.diagnostics_enabled);
    assert!(runtime.keymaps.is_empty());
    assert_eq!(runtime.hotkeys, HotkeyConfig::default());
    assert_eq!(runtime.hotkeys.toggle_mode, vec![hotkey("Ctrl+Shift")]);
    // The D13 defaults, after a rule for each application excluded by default (D47).
    assert_eq!(
        runtime.compatibility,
        with_exclusions(
            ExclusionsSection::DEFAULT_APPS,
            CompatibilityConfig::defaults_v1()
        )
    );
    // The product options are version 2 (D51): no spell checking.
    let defaults = RuntimeSnapshot {
        options: EngineOptions::defaults_v2(),
        ..RuntimeSnapshot::defaults_v1()
    };
    assert_eq!(
        RuntimeSnapshot {
            config_revision: 0,
            compatibility: CompatibilityConfig::defaults_v1(),
            ..runtime.clone()
        },
        defaults
    );
    assert_eq!(settings.macro_snapshot().macro_revision, 0);
    assert!(settings.macro_snapshot().entries.is_empty());
    assert_consumable(&settings);
}

#[test]
fn the_second_run_loads_the_file_and_keeps_the_users_values() {
    let folders = Folders::new();
    {
        let (mut settings, _) = folders.open();
        settings
            .apply(Change::InputMethod(InputMethod::Vni))
            .unwrap();
        settings.apply(Change::SpellCheck(true)).unwrap();
        settings.apply(Change::VietnameseMode(false)).unwrap();
    }
    let before = fs::read(folders.config()).unwrap();
    let (settings, report) = folders.open();
    assert!(!report.first_run);
    assert_eq!(report.location, Location::ExeDir);
    assert_eq!(report.create_error, None);
    assert_eq!(report.status.config, loaded_clean());
    let document = settings.document();
    assert_eq!(document.revision, 4);
    assert_eq!(document.input.method, InputMethod::Vni);
    assert!(document.input.spell_check);
    assert!(!document.input.enabled);
    let runtime = settings.runtime_snapshot();
    assert_eq!(runtime.config_revision, 4);
    assert_eq!(runtime.options.input_method, InputMethod::Vni);
    assert!(runtime.options.spell_check);
    assert!(!runtime.options.enabled);
    assert!(!runtime.vietnamese_mode);
    assert!(!settings.vietnamese_mode());
    // Loading does not write.
    assert_eq!(fs::read(folders.config()).unwrap(), before);
}

#[test]
fn hotkeys_and_diagnostics_come_from_the_file() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 2\n[hotkeys]\ntoggle_mode = [\"Alt+Z\"]\nhard_reset = []\n\
         [system]\ndiagnostics = true\n",
    )
    .unwrap();
    let (settings, report) = folders.open();
    assert_eq!(report.status.config, loaded_clean());
    let runtime = settings.runtime_snapshot();
    assert_eq!(runtime.hotkeys.toggle_mode, vec![hotkey("Alt+Z")]);
    assert!(runtime.hotkeys.hard_reset.is_empty());
    assert!(runtime.diagnostics_enabled);
    assert_consumable(&settings);
}

#[test]
fn an_unwritable_executable_folder_falls_back_to_local_app_data() {
    let folders = Folders::new();
    let (settings, report) = folders.open_unwritable();
    let config = folders.local().join("Raumakey").join("raumakey.toml");
    assert_eq!(report.location, Location::LocalAppData);
    assert!(report.first_run);
    assert_eq!(report.config_path.as_deref(), Some(config.as_path()));
    assert_eq!(settings.location(), Location::LocalAppData);
    assert!(config.is_file());
    assert!(listing(folders.exe()).is_empty());
    // The second run finds it there.
    let (_, report) = folders.open_unwritable();
    assert!(!report.first_run);
    assert_eq!(report.status.config, loaded_clean());
}

/// The executable's folder is unknown (`current_exe` failed) or not absolute: the settings
/// live under the local application data folder, and nothing is asked about the folder.
#[test]
fn an_unknown_executable_folder_uses_local_app_data() {
    let local = tempfile::tempdir().unwrap();
    let config = local.path().join("Raumakey").join("raumakey.toml");
    for exe_dir in ["", "relative"] {
        let (mut settings, report) = Settings::open(
            Path::new(exe_dir),
            Some(local.path()),
            &|_| panic!("probed"),
            &|_| panic!("asked whether protected"),
        );
        assert_eq!(report.location, Location::LocalAppData, "{exe_dir:?}");
        assert_eq!(report.config_path.as_deref(), Some(config.as_path()));
        assert_eq!(report.create_error, None);
        settings.apply(Change::SpellCheck(true)).unwrap();
        assert!(stored(&config).input.spell_check);
        settings.apply(Change::SpellCheck(false)).unwrap();
    }
}

/// A `raumakey.toml` beside the executable is used even when the folder cannot be written.
#[test]
fn an_existing_configuration_beside_the_executable_is_used() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 3\n[input]\nmethod = \"vni\"\n",
    )
    .unwrap();
    let (settings, report) = Settings::open(
        folders.exe(),
        Some(folders.local()),
        &|_| panic!("probed"),
        &|_| panic!("asked whether protected"),
    );
    assert_eq!(report.location, Location::ExeDir);
    assert!(!report.first_run);
    assert_eq!(settings.document().input.method, InputMethod::Vni);
    assert!(listing(folders.local()).is_empty());
}

/// A protected folder is never probed and holds nothing afterwards.
#[test]
fn a_protected_executable_folder_uses_local_app_data() {
    let folders = Folders::new();
    let (_, report) = Settings::open(
        folders.exe(),
        Some(folders.local()),
        &|_| panic!("probed"),
        &|_| true,
    );
    assert_eq!(report.location, Location::LocalAppData);
    assert!(listing(folders.exe()).is_empty());
    assert!(
        folders
            .local()
            .join("Raumakey")
            .join("raumakey.toml")
            .is_file()
    );
}

/// The protection check of `open_for_current_exe`: Program Files, Program Files (x86), the
/// Windows folder, and the temp folder (Explorer runs an executable from inside a ZIP out of
/// it), each also directly, and a temp folder also in its `\\?\` form.
#[cfg(windows)]
#[test]
fn program_files_windows_and_temp_are_protected() {
    use raumakey_windows::known_folder::{program_files, program_files_x86, windows_dir};
    let folders = [
        program_files().unwrap(),
        program_files_x86().unwrap(),
        windows_dir().unwrap(),
        std::env::temp_dir(),
    ];
    for folder in folders {
        assert!(is_protected_dir(&folder), "{folder:?}");
        assert!(is_protected_dir(&folder.join("Raumakey")), "{folder:?}");
    }
    let extracted = tempfile::tempdir().unwrap();
    assert!(is_protected_dir(extracted.path()));
    let verbatim = fs::canonicalize(extracted.path()).unwrap();
    assert!(
        verbatim.to_string_lossy().starts_with(r"\\?\"),
        "{verbatim:?}"
    );
    assert!(is_protected_dir(&verbatim));
}

#[cfg(windows)]
#[test]
fn other_folders_are_not_protected() {
    let local = raumakey_windows::known_folder::local_app_data().unwrap();
    assert!(!is_protected_dir(&local.join("Raumakey portable")));
    assert!(!is_protected_dir(Path::new("C:\\Raumakey")));
    assert!(!is_protected_dir(Path::new("relative")));
    assert!(!is_protected_dir(Path::new("")));
}

/// The integration step owns `Settings` on a worker thread.
#[test]
fn settings_move_to_a_worker_thread() {
    fn send_and_sync<T: Send + Sync>() {}
    send_and_sync::<Settings>();
    send_and_sync::<OpenReport>();
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    let worker = std::thread::spawn(move || {
        settings.apply(Change::ModernTone(true)).unwrap();
        settings.poll();
        settings
    });
    let settings = worker.join().unwrap();
    assert!(settings.document().input.modern_tone);
    assert!(stored(&folders.config()).input.modern_tone);
}

/// A commit writes the defaults of the fields that took them, so they are not reported any
/// more; entries set aside verbatim still are, as a new load would report them.
#[test]
fn a_commit_refreshes_the_reported_issues() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nmethod = \"klingon\"\n\
         [[keymaps]]\nname = \"No id\"\nbindings = []\n",
    )
    .unwrap();
    let (mut settings, report) = folders.open();
    let DocumentStatus::Loaded { issues } = &report.status.config else {
        panic!("{:?}", report.status.config);
    };
    assert!(issues.contains(&issue("input.method", IssueReason::InvalidValue)));
    assert!(issues.len() > 1, "{issues:?}");
    settings.apply(Change::SpellCheck(true)).unwrap();
    let (_, reopened) = folders.open();
    assert_eq!(settings.status().config, reopened.status.config);
    let DocumentStatus::Loaded { issues } = &settings.status().config else {
        panic!("{:?}", settings.status().config);
    };
    assert!(!issues.is_empty());
    assert!(!issues.contains(&issue("input.method", IssueReason::InvalidValue)));
}

/// Opens `path` for reading without sharing it, as another program that locks the file does.
#[cfg(windows)]
fn lock(path: &Path) -> fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
        .unwrap()
}

#[cfg(windows)]
fn is_transient(status: &DocumentStatus) -> bool {
    matches!(
        status,
        DocumentStatus::Unreadable(ConfigError::IoFailure | ConfigError::PermissionDenied)
    )
}

/// A file that another program holds open without sharing during one poll is read again by
/// the next polls, although it did not change.
#[cfg(windows)]
#[test]
fn a_file_locked_during_a_poll_is_read_again_afterwards() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    settings
        .apply(Change::InputMethod(InputMethod::Vni))
        .unwrap();
    let handle = lock(&folders.config());
    let reloaded = settings.poll().unwrap();
    assert!(reloaded.runtime.is_none());
    assert!(
        is_transient(&settings.status().config),
        "{:?}",
        settings.status()
    );
    // The last good settings stay, and an unchanged failure is not reported again.
    assert_eq!(
        settings.runtime_snapshot().options.input_method,
        InputMethod::Vni
    );
    assert!(settings.poll().is_none());
    drop(handle);
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(runtime.options.input_method, InputMethod::Vni);
    assert_eq!(settings.status().config, loaded_clean());
    assert!(settings.poll().is_none());
}

/// Recovery loads the file once more first: a file that reads again is taken as it is, not
/// moved aside.
#[cfg(windows)]
#[test]
fn recover_adopts_a_file_that_reads_again() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 4\n[input]\nmethod = \"viqr\"\n",
    )
    .unwrap();
    let handle = lock(&folders.config());
    let (mut settings, report) = folders.open();
    assert!(is_transient(&report.status.config), "{:?}", report.status);
    assert_eq!(settings.document().input.method, InputMethod::Telex);
    drop(handle);
    assert_eq!(settings.recover(), Ok(Recovery::Adopted));
    assert_eq!(settings.document().input.method, InputMethod::Viqr);
    assert_eq!(settings.document().revision, 4);
    assert_eq!(settings.status().config, loaded_clean());
    let names = listing(folders.exe());
    assert!(names.contains(&"raumakey.toml".to_owned()), "{names:?}");
    assert!(
        !names.iter().any(|name| name.contains(".bak-")),
        "{names:?}"
    );
}

/// Neither folder can be used: the controller still runs, on the defaults, and every change
/// takes effect without being saved.
#[test]
fn without_a_usable_folder_the_settings_live_in_memory() {
    let (mut settings, report) = Settings::open(Path::new("relative"), None, &|_| true, &|_| false);
    assert_eq!(report.location, Location::Unavailable);
    assert_eq!(report.config_path, None);
    assert!(report.first_run);
    assert_eq!(report.create_error, Some(ConfigError::NotFound));
    assert_eq!(report.status.config, DocumentStatus::Missing);
    assert_eq!(
        settings.runtime_snapshot().options,
        EngineOptions::defaults_v2()
    );
    assert_eq!(
        settings.apply(Change::VietnameseMode(false)),
        Err(ConfigError::NotFound)
    );
    assert!(!settings.vietnamese_mode());
    assert!(!settings.runtime_snapshot().vietnamese_mode);
    assert!(settings.poll().is_none());
    assert_consumable(&settings);
}

#[test]
fn field_issues_are_reported_by_path_without_values() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nmethod = \"klingon\"\nspell_check = true\n",
    )
    .unwrap();
    let (settings, report) = folders.open();
    assert!(!report.first_run);
    assert_eq!(
        report.status.config,
        DocumentStatus::Loaded {
            issues: vec![issue("input.method", IssueReason::InvalidValue)]
        }
    );
    // The bad field took its default; the good one is kept.
    assert_eq!(settings.document().input.method, InputMethod::Telex);
    assert!(settings.document().input.spell_check);
    assert!(!format!("{report:?}").contains("klingon"));
    assert!(!format!("{settings:?}").contains("klingon"));
}

/// A damaged file never stops the controller and is never overwritten silently: the
/// controller runs on the defaults, changes take effect without being saved, and only an
/// explicit recovery sets the file aside.
#[test]
fn a_corrupt_file_runs_on_defaults_and_is_kept_until_recovered() {
    let folders = Folders::new();
    let corrupt = b"[[[ not a configuration".to_vec();
    fs::write(folders.config(), &corrupt).unwrap();
    let (mut settings, report) = folders.open();
    assert!(!report.first_run);
    assert_eq!(report.create_error, None);
    assert_eq!(
        report.status.config,
        DocumentStatus::Unreadable(ConfigError::Corrupt)
    );
    assert_eq!(
        settings.runtime_snapshot().options,
        EngineOptions::defaults_v2()
    );
    assert_consumable(&settings);

    assert_eq!(
        settings.apply(Change::VietnameseMode(false)),
        Err(ConfigError::Corrupt)
    );
    assert!(!settings.vietnamese_mode());
    assert_eq!(fs::read(folders.config()).unwrap(), corrupt);
    // An unchanged damaged file is not reported again.
    assert!(settings.poll().is_none());

    let Ok(Recovery::Replaced {
        backup: Some(backup),
        saved: Ok(()),
    }) = settings.recover()
    else {
        panic!("not replaced");
    };
    assert_eq!(backup.parent(), Some(folders.exe()));
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("raumakey.toml.bak-"), "{name}");
    assert_eq!(fs::read(&backup).unwrap(), corrupt);
    // The settings in effect, with the change, are in the new file.
    let document = stored(&folders.config());
    assert!(!document.input.enabled);
    assert_eq!(settings.status().config, loaded_clean());
    assert_eq!(settings.document().revision, document.revision);
    settings.apply(Change::SpellCheck(true)).unwrap();
    assert!(stored(&folders.config()).input.spell_check);
    // Nothing is left to recover.
    assert_eq!(settings.recover(), Ok(Recovery::NotNeeded));
}

#[test]
fn a_file_of_a_newer_version_is_never_overwritten() {
    let folders = Folders::new();
    let newer = b"version = 99\nrevision = 1\n".to_vec();
    fs::write(folders.config(), &newer).unwrap();
    let (mut settings, report) = folders.open();
    assert_eq!(
        report.status.config,
        DocumentStatus::Unreadable(ConfigError::UnsupportedVersion)
    );
    assert_eq!(
        settings.apply(Change::ModernTone(true)),
        Err(ConfigError::UnsupportedVersion)
    );
    assert!(settings.document().input.modern_tone);
    assert!(settings.runtime_snapshot().options.modern_tone);
    assert_eq!(fs::read(folders.config()).unwrap(), newer);
}

#[test]
fn every_tray_change_is_committed_at_once() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    let changes = [
        Change::VietnameseMode(false),
        Change::InputMethod(InputMethod::Viqr),
        Change::SpellCheck(true),
        Change::ModernTone(true),
        Change::Macros(false),
        Change::Charset(OutputCharset::Tcvn3),
        Change::Startup(StartupMode::Shortcut),
    ];
    for (index, change) in changes.into_iter().enumerate() {
        settings.apply(change).unwrap();
        let on_disk = stored(&folders.config());
        assert_eq!(
            on_disk.revision,
            u64::try_from(index).unwrap() + 2,
            "{change:?}"
        );
        assert_eq!(&on_disk, settings.document(), "{change:?}");
        assert_eq!(
            settings.runtime_snapshot().config_revision,
            on_disk.revision
        );
    }
    let document = stored(&folders.config());
    assert!(!document.input.enabled);
    assert_eq!(document.input.method, InputMethod::Viqr);
    assert!(document.input.spell_check);
    assert!(document.input.modern_tone);
    assert!(!document.macros.enabled);
    assert_eq!(document.input.charset, OutputCharset::Tcvn3);
    assert_eq!(document.system.startup, StartupMode::Shortcut);
    let options = &settings.runtime_snapshot().options;
    assert!(!options.enabled);
    assert_eq!(options.input_method, InputMethod::Viqr);
    assert!(options.spell_check);
    assert!(options.modern_tone);
    assert!(!options.macros_enabled);
    assert_eq!(options.charset, OutputCharset::Tcvn3);
    // D54: "Mặc định" puts the charset of "Cách gõ" back to Unicode.
    settings.apply(Change::TypingDefaults).unwrap();
    assert_eq!(
        stored(&folders.config()).input.charset,
        OutputCharset::Unicode
    );
    settings.apply(Change::Startup(StartupMode::None)).unwrap();
    assert_eq!(stored(&folders.config()).system.startup, StartupMode::None);
}

/// The switches of the settings panel (D43) are committed at once like the menu's: each to
/// its own key, and only that key changes.
#[test]
fn the_panel_switches_are_committed_to_their_own_keys() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    let defaults = ConfigDocument::defaults();
    assert!(defaults.input.free_marking);
    assert!(defaults.input.auto_restore_invalid);
    assert!(!defaults.macros.when_disabled);
    assert!(defaults.macros.auto_case);
    assert!(defaults.system.show_panel_on_start);
    let changes = [
        Change::FreeMarking(false),
        Change::AutoRestore(false),
        Change::MacrosWhenEnglish(true),
        Change::MacroAutoCase(false),
        Change::ShowPanelOnStart(false),
    ];
    for (index, change) in changes.into_iter().enumerate() {
        settings.apply(change).unwrap();
        let on_disk = stored(&folders.config());
        assert_eq!(
            on_disk.revision,
            u64::try_from(index).unwrap() + 2,
            "{change:?}"
        );
        assert_eq!(&on_disk, settings.document(), "{change:?}");
    }
    let mut expected = ConfigDocument::defaults();
    expected.revision = 6;
    expected.input.free_marking = false;
    expected.input.auto_restore_invalid = false;
    expected.macros.when_disabled = true;
    expected.macros.auto_case = false;
    expected.system.show_panel_on_start = false;
    assert_eq!(stored(&folders.config()), expected);
    let options = &settings.runtime_snapshot().options;
    assert!(!options.free_marking);
    assert!(!options.auto_restore_invalid);
    assert!(options.macros_when_disabled);
    assert!(!options.auto_case_macro);
    assert_consumable(&settings);
    // The same values again write nothing.
    for change in changes {
        settings.apply(change).unwrap();
    }
    assert_eq!(stored(&folders.config()).revision, 6);
}

/// "Mặc định" (D43) resets the groups "Cách gõ" and "Gõ tắt" to the schema defaults and
/// nothing else: not the mode, the start with Windows, the panel at start, the normalization,
/// or the key map of a user method.
#[test]
fn typing_defaults_reset_the_typing_and_macro_switches_only() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nenabled = false\nmethod = \"user\"\n\
         user_keymap = \"mine\"\nnormalization = \"nfd\"\nmodern_tone = true\n\
         free_marking = false\nspell_check = true\nauto_restore_invalid = false\n\
         context_limit = 64\n[macros]\nenabled = false\nwhen_disabled = true\n\
         auto_case = false\n[system]\nstartup = \"shortcut\"\nshow_panel_on_start = false\n\
         diagnostics = true\n[hotkeys]\ntoggle_mode = [\"Ctrl+Alt\"]\n\
         [[keymaps]]\nid = \"mine\"\nversion = 1\nname = \"Mine\"\n\
         bindings = [ { key = \"s\", actions = [\"tone:acute\"] } ]\n",
    )
    .unwrap();
    let (mut settings, report) = folders.open();
    assert_eq!(report.status.config, loaded_clean());
    let before = settings.document().clone();
    assert_eq!(
        settings.runtime_snapshot().options.input_method,
        InputMethod::UserDefined
    );
    settings.apply(Change::TypingDefaults).unwrap();
    let defaults = ConfigDocument::defaults();
    let mut expected = before.clone();
    expected.revision = 2;
    expected.input.method = defaults.input.method;
    expected.input.modern_tone = defaults.input.modern_tone;
    expected.input.free_marking = defaults.input.free_marking;
    expected.input.spell_check = defaults.input.spell_check;
    expected.input.auto_restore_invalid = defaults.input.auto_restore_invalid;
    expected.macros.enabled = defaults.macros.enabled;
    expected.macros.when_disabled = defaults.macros.when_disabled;
    expected.macros.auto_case = defaults.macros.auto_case;
    assert_eq!(*settings.document(), expected);
    assert_eq!(stored(&folders.config()), expected);
    // Kept: the mode, the normalization, the context limit, the key map, the hotkeys, and the
    // whole of [system].
    assert!(!expected.input.enabled);
    assert_eq!(expected.input.user_keymap.as_deref(), Some("mine"));
    assert_eq!(expected.input.context_limit, 64);
    assert_eq!(expected.system, before.system);
    assert_eq!(expected.hotkeys, before.hotkeys);
    assert_eq!(expected.keymaps, before.keymaps);
    let options = &settings.runtime_snapshot().options;
    assert_eq!(options.input_method, InputMethod::Telex);
    assert!(!options.enabled);
    assert_consumable(&settings);
    // Defaults again: nothing to write.
    settings.apply(Change::TypingDefaults).unwrap();
    assert_eq!(stored(&folders.config()).revision, 2);
}

/// Unsaved changes are applied again in the order of their last application, so "Mặc định"
/// and single switches that interleave while the file cannot be written end the way they were
/// made: a switch after the defaults stays, a switch before them is reset. The settings start
/// from a file with VNI (so "Mặc định" changes something), which is then damaged.
#[test]
fn unsaved_typing_defaults_and_single_switches_keep_their_order() {
    let cases: [(&[Change], bool, InputMethod, bool); 4] = [
        // The switch, then the defaults: reset.
        (
            &[Change::SpellCheck(true), Change::TypingDefaults],
            false,
            InputMethod::Telex,
            false,
        ),
        // The defaults, then the switch: it stays.
        (
            &[Change::TypingDefaults, Change::SpellCheck(true)],
            true,
            InputMethod::Telex,
            false,
        ),
        // A switch before and another after the defaults.
        (
            &[
                Change::InputMethod(InputMethod::Viqr),
                Change::SpellCheck(true),
                Change::TypingDefaults,
                Change::ModernTone(true),
            ],
            false,
            InputMethod::Telex,
            true,
        ),
        // The defaults, a switch, and the defaults again: the last defaults win.
        (
            &[
                Change::TypingDefaults,
                Change::InputMethod(InputMethod::Viqr),
                Change::SpellCheck(true),
                Change::TypingDefaults,
            ],
            false,
            InputMethod::Telex,
            false,
        ),
    ];
    for (changes, spell_check, method, modern_tone) in cases {
        let folders = Folders::new();
        fs::write(
            folders.config(),
            "version = 1
revision = 1
[input]
method = \"vni\"
",
        )
        .unwrap();
        let (mut settings, _) = folders.open();
        assert_eq!(settings.document().input.method, InputMethod::Vni);
        fs::write(folders.config(), "[[[ damaged").unwrap();
        for change in changes {
            assert_eq!(
                settings.apply(*change),
                Err(ConfigError::Corrupt),
                "{change:?}"
            );
        }
        let in_effect = &settings.document().input;
        assert_eq!(in_effect.spell_check, spell_check, "{changes:?}");
        assert_eq!(in_effect.method, method, "{changes:?}");
        assert_eq!(in_effect.modern_tone, modern_tone, "{changes:?}");
        // The user fixes the file by hand (VNI, spell check on, modern tone): the reload
        // applies the unsaved changes again on it, and the retry writes them.
        fs::write(
            folders.config(),
            "version = 1\nrevision = 1\n[input]\nmethod = \"vni\"\nspell_check = true\n\
             modern_tone = true\n",
        )
        .unwrap();
        settings.poll().unwrap();
        assert_eq!(settings.save_unsaved(), Ok(true), "{changes:?}");
        let document = stored(&folders.config());
        assert_eq!(document.input.spell_check, spell_check, "{changes:?}");
        assert_eq!(document.input.method, method, "{changes:?}");
        assert_eq!(document.input.modern_tone, modern_tone, "{changes:?}");
    }
}

/// A change to the value the settings hold already writes nothing.
#[test]
fn a_change_to_the_current_value_writes_nothing() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    let before = fs::read(folders.config()).unwrap();
    settings.apply(Change::VietnameseMode(true)).unwrap();
    settings
        .apply(Change::InputMethod(InputMethod::Telex))
        .unwrap();
    settings.apply(Change::Startup(StartupMode::None)).unwrap();
    assert_eq!(fs::read(folders.config()).unwrap(), before);
    assert_eq!(settings.document().revision, 1);
}

#[test]
fn the_snapshots_are_valid_for_every_input_method() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    for method in [InputMethod::Vni, InputMethod::Viqr, InputMethod::Telex] {
        settings.apply(Change::InputMethod(method)).unwrap();
        assert_eq!(settings.runtime_snapshot().options.input_method, method);
        assert_consumable(&settings);
        let (reopened, _) = folders.open();
        assert_eq!(reopened.runtime_snapshot(), settings.runtime_snapshot());
    }
    // A user method without a key map is kept in the file, while the engine runs on the
    // default method (`ConfigDocument::engine_options`).
    settings
        .apply(Change::InputMethod(InputMethod::UserDefined))
        .unwrap();
    assert_eq!(settings.document().input.method, InputMethod::UserDefined);
    assert_eq!(
        settings.runtime_snapshot().options.input_method,
        InputMethod::Telex
    );
    assert_consumable(&settings);
}

#[test]
fn a_user_method_with_its_key_map_reaches_the_snapshot() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nmethod = \"user\"\nuser_keymap = \"mine\"\n\
         [[keymaps]]\nid = \"mine\"\nversion = 1\nname = \"Mine\"\n\
         bindings = [ { key = \"s\", actions = [\"tone:acute\"] } ]\n",
    )
    .unwrap();
    let (settings, report) = folders.open();
    assert_eq!(report.status.config, loaded_clean());
    let runtime = settings.runtime_snapshot();
    assert_eq!(runtime.options.input_method, InputMethod::UserDefined);
    assert_eq!(runtime.options.user_keymap_id.as_deref(), Some("mine"));
    assert_eq!(runtime.keymaps.len(), 1);
    assert_consumable(&settings);
}

/// The tray polls every two seconds; a hand edit (which rarely changes `revision`) is
/// picked up once.
#[test]
fn a_hand_edit_is_picked_up_by_the_poll() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    assert!(settings.poll().is_none());
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nmethod = \"viqr\"\nmodern_tone = true\n",
    )
    .unwrap();
    let reloaded = settings.poll().unwrap();
    let runtime = reloaded.runtime.unwrap();
    assert!(reloaded.macros.is_none());
    assert_eq!(runtime.options.input_method, InputMethod::Viqr);
    assert!(runtime.options.modern_tone);
    assert_eq!(&runtime, settings.runtime_snapshot());
    assert_eq!(settings.document().input.method, InputMethod::Viqr);
    assert_eq!(settings.status().config, loaded_clean());
    assert!(settings.poll().is_none());
    // A change after the edit builds on the edited document.
    settings.apply(Change::SpellCheck(true)).unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.input.method, InputMethod::Viqr);
    assert!(document.input.spell_check);
    assert_eq!(document.revision, 2);
}

/// A hand edit that breaks the file keeps the last good settings in effect; fixing the file
/// brings the new settings.
#[test]
fn a_hand_edit_that_breaks_the_file_keeps_the_last_good_settings() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    settings
        .apply(Change::InputMethod(InputMethod::Vni))
        .unwrap();
    fs::write(folders.config(), "[input\nmethod = ").unwrap();
    let reloaded = settings.poll().unwrap();
    assert!(reloaded.runtime.is_none());
    assert_eq!(
        settings.status().config,
        DocumentStatus::Unreadable(ConfigError::Corrupt)
    );
    assert_eq!(
        settings.runtime_snapshot().options.input_method,
        InputMethod::Vni
    );
    assert!(settings.poll().is_none());
    fs::write(
        folders.config(),
        "version = 1\nrevision = 2\n[input]\nmethod = \"viqr\"\n",
    )
    .unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(runtime.options.input_method, InputMethod::Viqr);
    assert_eq!(settings.status().config, loaded_clean());
}

#[test]
fn a_deleted_file_brings_the_defaults_and_the_next_change_writes_it_again() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    settings
        .apply(Change::InputMethod(InputMethod::Vni))
        .unwrap();
    fs::remove_file(folders.config()).unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(runtime.options, EngineOptions::defaults_v2());
    assert_eq!(settings.status().config, DocumentStatus::Missing);
    assert!(settings.poll().is_none());
    settings.apply(Change::SpellCheck(true)).unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 1);
    assert_eq!(document.input.method, InputMethod::Telex);
    assert!(document.input.spell_check);
}

/// The file was edited by hand after the settings last saw it, and nothing polled in between:
/// the commit reports a conflict, and the change is applied again on the edited document.
#[test]
fn a_change_over_an_unseen_hand_edit_keeps_the_edit() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    // Same revision, other content: only the store's content check notices it.
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nmodern_tone = true\n",
    )
    .unwrap();
    settings.apply(Change::SpellCheck(true)).unwrap();
    let document = stored(&folders.config());
    assert!(document.input.modern_tone);
    assert!(document.input.spell_check);
    assert_eq!(document.revision, 2);
    assert_eq!(settings.document(), &document);
    assert!(settings.runtime_snapshot().options.modern_tone);
    assert!(settings.poll().is_none());
}

/// Another writer committed a new revision meanwhile.
#[test]
fn a_change_over_a_newer_revision_is_applied_again() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    let other: AtomicStore<ConfigDocument> = AtomicStore::new(folders.exe(), Arc::new(StdReplacer));
    let mut document = other.load().unwrap().document;
    document.input.method = InputMethod::Vni;
    assert_eq!(other.commit(1, &document).unwrap(), 2);
    settings.apply(Change::Macros(false)).unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 3);
    assert_eq!(document.input.method, InputMethod::Vni);
    assert!(!document.macros.enabled);
    assert_eq!(
        settings.runtime_snapshot().options.input_method,
        InputMethod::Vni
    );
}

/// Macro text that must never appear in formatted output.
const SECRET_TRIGGER: &str = "zqxjtrigger";
const SECRET_REPLACEMENT: &str = "Bí mật thay thế";

fn macros_file(entries: &[(u64, &str, &str)]) -> String {
    let mut text = String::from("version = 1\nrevision = 3\n");
    for (id, trigger, replacement) in entries {
        write!(
            text,
            "[[macro]]\nid = {id}\ntrigger = \"{trigger}\"\nreplacement = \"{replacement}\"\n"
        )
        .unwrap();
    }
    text
}

#[test]
fn macros_are_loaded_when_the_file_exists_and_unusable_entries_are_left_out() {
    let folders = Folders::new();
    let long_trigger = "t".repeat(33);
    fs::write(
        folders.macros(),
        macros_file(&[
            (1, SECRET_TRIGGER, SECRET_REPLACEMENT),
            (2, &long_trigger, "x"),
            (1, "again", "y"),
        ]),
    )
    .unwrap();
    let (settings, report) = folders.open();
    assert_eq!(report.status.macros, loaded_clean());
    assert_eq!(
        report.status.skipped_macros,
        vec![
            EntryIssue {
                index: 1,
                error: MacroError::InvalidEntry {
                    id: MacroId(2),
                    reason: EntryError::TriggerTooLong
                },
            },
            EntryIssue {
                index: 2,
                error: MacroError::DuplicateId(MacroId(1)),
            },
        ]
    );
    let macros = settings.macro_snapshot();
    assert_eq!(macros.macro_revision, 3);
    assert_eq!(macros.entries.len(), 1);
    assert_eq!(macros.entries[0].trigger, SECRET_TRIGGER);
    assert_consumable(&settings);
    // The file is read, never written.
    assert_eq!(
        fs::read_to_string(folders.macros()).unwrap(),
        macros_file(&[
            (1, SECRET_TRIGGER, SECRET_REPLACEMENT),
            (2, &long_trigger, "x"),
            (1, "again", "y"),
        ])
    );
    for text in [format!("{settings:?}"), format!("{report:?}")] {
        assert!(!text.contains(SECRET_TRIGGER), "{text}");
        assert!(!text.contains(SECRET_REPLACEMENT), "{text}");
    }
}

#[test]
fn a_macros_file_that_appears_later_is_picked_up_by_the_poll() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    fs::write(
        folders.macros(),
        macros_file(&[(7, SECRET_TRIGGER, SECRET_REPLACEMENT)]),
    )
    .unwrap();
    let reloaded = settings.poll().unwrap();
    assert!(reloaded.runtime.is_none());
    let macros = reloaded.macros.unwrap();
    assert_eq!(macros.macro_revision, 3);
    assert_eq!(macros.entries.len(), 1);
    assert!(Arc::ptr_eq(&macros, settings.macro_snapshot()));
    assert_eq!(settings.status().macros, loaded_clean());
    assert!(settings.poll().is_none());
    assert_consumable(&settings);
    let text = format!("{settings:?}");
    assert!(!text.contains(SECRET_TRIGGER), "{text}");
    assert!(!text.contains(SECRET_REPLACEMENT), "{text}");
}

#[test]
fn a_corrupt_macros_file_leaves_no_macros_in_effect() {
    let folders = Folders::new();
    fs::write(folders.macros(), "[[macro]\n").unwrap();
    let (settings, report) = folders.open();
    assert_eq!(
        report.status.macros,
        DocumentStatus::Unreadable(ConfigError::Corrupt)
    );
    assert!(settings.macro_snapshot().entries.is_empty());
    assert_consumable(&settings);
}

#[test]
fn app_rules_are_loaded_when_the_file_exists() {
    let folders = Folders::new();
    fs::write(
        folders.app_rules(),
        "version = 1\nrevision = 1\nclipboard_enabled = true\n",
    )
    .unwrap();
    let (mut settings, report) = folders.open();
    assert_eq!(report.status.app_rules, loaded_clean());
    let compatibility = &settings.runtime_snapshot().compatibility;
    assert!(compatibility.clipboard_enabled);
    // Without the file the D13 defaults apply; a file without named sets has none.
    assert!(compatibility.named_sets.is_empty());
    assert_consumable(&settings);
    fs::remove_file(folders.app_rules()).unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(
        runtime.compatibility,
        with_exclusions(
            ExclusionsSection::DEFAULT_APPS,
            CompatibilityConfig::defaults_v1()
        )
    );
    assert_eq!(settings.status().app_rules, DocumentStatus::Missing);
}

/// The session core refuses a snapshot beyond the channel's limits, so lists that are too long
/// are cut to the limit and reported on their path.
#[test]
fn lists_beyond_the_snapshot_limits_are_cut_and_reported() {
    let folders = Folders::new();
    let mut text = String::from("version = 1\nrevision = 1\n[input]\nmethod = \"user\"\n");
    let last = MAX_KEYMAPS;
    writeln!(text, "user_keymap = \"m{last}\"").unwrap();
    text.push_str("[hotkeys]\ntoggle_mode = [");
    for key in 1..=MAX_HOTKEYS + 1 {
        write!(text, "\"Ctrl+Alt+F{key}\", ").unwrap();
    }
    text.push_str("]\n");
    for index in 0..=MAX_KEYMAPS {
        write!(
            text,
            "[[keymaps]]\nid = \"m{index}\"\nversion = 1\nname = \"Map\"\n\
             bindings = [ {{ key = \"s\", actions = [\"tone:acute\"] }} ]\n"
        )
        .unwrap();
    }
    fs::write(folders.config(), text).unwrap();
    let (settings, report) = folders.open();
    assert_eq!(
        report.status.config,
        DocumentStatus::Loaded {
            issues: vec![
                issue("keymaps", IssueReason::OutOfRange),
                issue("hotkeys.toggle_mode", IssueReason::OutOfRange),
            ]
        }
    );
    let runtime = settings.runtime_snapshot();
    assert_eq!(runtime.hotkeys.toggle_mode.len(), MAX_HOTKEYS);
    // Only the key map in use is needed by the engine.
    assert_eq!(runtime.keymaps.len(), 1);
    assert_eq!(runtime.keymaps[0].id, format!("m{last}"));
    assert_eq!(runtime.options.input_method, InputMethod::UserDefined);
    // The document keeps everything, so a commit writes it all back.
    assert_eq!(settings.document().keymaps.len(), MAX_KEYMAPS + 1);
    assert_consumable(&settings);
}

#[test]
fn app_rules_the_session_core_would_refuse_fall_back_to_the_defaults() {
    let folders = Folders::new();
    let mut text = String::from("version = 1\nrevision = 1\n");
    for index in 0..=MAX_NAMED_SETS {
        write!(
            text,
            "[[named_set]]\nname = \"set{index}\"\nmembers = [\"a.exe\"]\n"
        )
        .unwrap();
    }
    fs::write(folders.app_rules(), text).unwrap();
    let (settings, report) = folders.open();
    let DocumentStatus::Unusable(error) = report.status.app_rules else {
        panic!("{:?}", report.status.app_rules);
    };
    assert!(
        matches!(
            error,
            raumakey_ipc::error::IpcError::LimitExceeded {
                limit: raumakey_ipc::error::Limit::NamedSets,
                ..
            }
        ),
        "{error:?}"
    );
    // The exclusions of `raumakey.toml` stay in effect over the defaults.
    assert_eq!(
        settings.runtime_snapshot().compatibility,
        with_exclusions(
            ExclusionsSection::DEFAULT_APPS,
            CompatibilityConfig::defaults_v1()
        )
    );
    assert_consumable(&settings);
}

/// D47: each application of `[exclusions]` becomes a rule that allows neither Vietnamese nor
/// macros, before the rules of `raumakey-app-rules.toml`. Exe-only rules apply in order and the
/// first that sets a field decides it, so an exclusion wins over every exe-only rule and named
/// set of that file; a window-class rule of the file for the same application still decides,
/// for its class, the fields it sets.
#[test]
fn excluded_applications_win_over_the_app_rules_but_not_their_window_class_rules() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[exclusions]\napps = [\"MSTSC.EXE\", \"msrdc.exe\"]\n",
    )
    .unwrap();
    fs::write(
        folders.app_rules(),
        r#"version = 1
revision = 1
[[rule]]
exe = "mstsc.exe"
vietnamese = true
macros = true
[[rule]]
exe = "mstsc.exe"
window_class = "TscShellContainerClass"
vietnamese = true
[[rule]]
exe = "notepad.exe"
macros = false
[[named_set]]
name = "remote"
members = ["msrdc.exe"]
action = { vietnamese = true, macros = true }
"#,
    )
    .unwrap();
    let (settings, report) = folders.open();
    assert_eq!(report.status.config, loaded_clean());
    assert_eq!(report.status.app_rules, loaded_clean());
    let runtime = settings.runtime_snapshot();
    let rules = &runtime.compatibility.rules;
    assert_eq!(rules.len(), 5);
    assert_eq!(rules[..2], [exclusion("mstsc.exe"), exclusion("msrdc.exe")]);
    assert_eq!(
        rules[2].action.vietnamese,
        Some(true),
        "the file's rules follow"
    );
    assert_eq!(runtime.compatibility.named_sets.len(), 1);

    for (exe, class) in [
        ("mstsc.exe", None),
        ("mstsc.exe", Some("Edit")),
        ("msrdc.exe", None),
    ] {
        let policy = policy_of(runtime, exe, class);
        assert!(!policy.vietnamese_allowed, "{exe} {class:?}");
        assert!(!policy.macro_allowed, "{exe} {class:?}");
    }
    let shell = policy_of(runtime, "mstsc.exe", Some("tscshellcontainerclass"));
    assert!(shell.vietnamese_allowed, "the window-class rule decides");
    assert!(
        !shell.macro_allowed,
        "it does not set macros: the exclusion does"
    );
    let notepad = policy_of(runtime, "notepad.exe", None);
    assert!(notepad.vietnamese_allowed);
    assert!(!notepad.macro_allowed);
    assert_consumable(&settings);
}

/// The channel carries at most `MAX_APP_RULES` rules. The exclusions come first, so when they
/// and the rules of `raumakey-app-rules.toml` are too many together, the last rules of that
/// file are cut and `rule` is reported on it; an exclusion list beyond the limit is cut itself
/// and reported on `exclusions.apps`. The files keep everything.
#[test]
fn rules_beyond_the_snapshot_limit_are_cut_after_the_exclusions() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[exclusions]\napps = [\"a.exe\", \"b.exe\"]\n",
    )
    .unwrap();
    let mut rules = String::from("version = 1\nrevision = 1\n");
    for index in 0..MAX_APP_RULES - 1 {
        write!(
            rules,
            "[[rule]]\nexe = \"app{index}.exe\"\nreset_on_mouse = false\n"
        )
        .unwrap();
    }
    fs::write(folders.app_rules(), rules).unwrap();
    let (mut settings, report) = folders.open();
    assert_eq!(report.status.config, loaded_clean());
    assert_eq!(
        report.status.app_rules,
        DocumentStatus::Loaded {
            issues: vec![issue("rule", IssueReason::OutOfRange)]
        }
    );
    let rules = &settings.runtime_snapshot().compatibility.rules;
    assert_eq!(rules.len(), MAX_APP_RULES);
    assert_eq!(rules[..2], [exclusion("a.exe"), exclusion("b.exe")]);
    let last = format!("app{}.exe", MAX_APP_RULES - 3);
    assert_eq!(rules[MAX_APP_RULES - 1].exe.as_str(), last);
    assert_consumable(&settings);

    // More exclusions than the limit: they are cut too, and no rule of the file is left.
    let mut text = String::from("version = 1\nrevision = 1\n[exclusions]\napps = [");
    for index in 0..=MAX_APP_RULES {
        write!(text, "\"x{index}.exe\", ").unwrap();
    }
    text.push_str("]\n");
    fs::write(folders.config(), text).unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(
        settings.status().config,
        DocumentStatus::Loaded {
            issues: vec![issue("exclusions.apps", IssueReason::OutOfRange)]
        }
    );
    assert_eq!(
        settings.status().app_rules,
        DocumentStatus::Loaded {
            issues: vec![issue("rule", IssueReason::OutOfRange)]
        }
    );
    let rules = &runtime.compatibility.rules;
    assert_eq!(rules.len(), MAX_APP_RULES);
    assert_eq!(rules[0], exclusion("x0.exe"));
    let last = format!("x{}.exe", MAX_APP_RULES - 1);
    assert_eq!(rules[MAX_APP_RULES - 1], exclusion(&last));
    assert_eq!(settings.document().exclusions.apps.len(), MAX_APP_RULES + 1);
    assert_consumable(&settings);
}

/// While `raumakey-app-rules.toml` cannot be read, its last good rules stay in effect; when
/// more exclusions then push some of them past the limit, the cut is reported on
/// `exclusions.apps` of `raumakey.toml`, whose exclusions push them out, since the status of
/// the unreadable file holds no issues. Once that file reads again, the cut is reported on it.
#[test]
fn rules_cut_while_the_app_rules_file_is_unreadable_are_reported_on_the_exclusions() {
    let folders = Folders::new();
    let config = |apps: usize| {
        let mut text = String::from("version = 1\nrevision = 1\n[exclusions]\napps = [");
        for index in 0..apps {
            write!(text, "\"x{index}.exe\", ").unwrap();
        }
        text.push_str("]\n");
        text
    };
    fs::write(folders.config(), config(2)).unwrap();
    let mut rules = String::from("version = 1\nrevision = 1\n");
    for index in 0..MAX_APP_RULES - 4 {
        write!(
            rules,
            "[[rule]]\nexe = \"app{index}.exe\"\nreset_on_mouse = false\n"
        )
        .unwrap();
    }
    fs::write(folders.app_rules(), &rules).unwrap();
    let (mut settings, report) = folders.open();
    assert_eq!(report.status.config, loaded_clean());
    assert_eq!(report.status.app_rules, loaded_clean());

    fs::write(folders.app_rules(), "[[[ damaged").unwrap();
    assert!(settings.poll().unwrap().runtime.is_none());
    assert_eq!(
        settings.status().app_rules,
        DocumentStatus::Unreadable(ConfigError::Corrupt)
    );

    // Five exclusions and the last good rules are one rule too many.
    fs::write(folders.config(), config(5)).unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(
        settings.status().config,
        DocumentStatus::Loaded {
            issues: vec![issue("exclusions.apps", IssueReason::OutOfRange)]
        }
    );
    assert_eq!(
        settings.status().app_rules,
        DocumentStatus::Unreadable(ConfigError::Corrupt)
    );
    let cut = &runtime.compatibility.rules;
    assert_eq!(cut.len(), MAX_APP_RULES);
    assert_eq!(cut[4], exclusion("x4.exe"));
    let last = format!("app{}.exe", MAX_APP_RULES - 6);
    assert_eq!(cut[MAX_APP_RULES - 1].exe.as_str(), last);
    assert_consumable(&settings);

    fs::write(folders.app_rules(), &rules).unwrap();
    settings.poll().unwrap();
    assert_eq!(settings.status().config, loaded_clean());
    assert_eq!(
        settings.status().app_rules,
        DocumentStatus::Loaded {
            issues: vec![issue("rule", IssueReason::OutOfRange)]
        }
    );
}

/// A hand edit of `[exclusions]` is picked up by the poll like any other: the new runtime
/// snapshot carries the new list, and an empty list leaves the rules of the app-rules file
/// (here the D13 defaults) alone.
#[test]
fn a_hand_edit_of_the_exclusions_reaches_the_runtime_snapshot() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    assert!(!policy_of(settings.runtime_snapshot(), "mstsc.exe", None).vietnamese_allowed);
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[exclusions]\napps = [\"PuTTY.exe\"]\n",
    )
    .unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(&runtime, settings.runtime_snapshot());
    assert_eq!(
        runtime.compatibility,
        with_exclusions(&["putty.exe"], CompatibilityConfig::defaults_v1())
    );
    assert!(!policy_of(&runtime, "putty.exe", None).vietnamese_allowed);
    assert!(policy_of(&runtime, "mstsc.exe", None).vietnamese_allowed);
    assert_eq!(settings.status().config, loaded_clean());

    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[exclusions]\napps = []\n",
    )
    .unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(runtime.compatibility, CompatibilityConfig::defaults_v1());
    assert!(settings.poll().is_none());
}

/// Changes from the tray menu and the panel commit the whole document: the exclusions and the
/// unknown keys of `[exclusions]` are written back and stay in effect.
#[test]
fn changes_from_the_panel_keep_the_exclusions() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[exclusions]\napps = [\"putty.exe\", \"mstsc.exe\"]\n\
         x_note = \"kept\"\n",
    )
    .unwrap();
    let (mut settings, _) = folders.open();
    settings.apply(Change::SpellCheck(true)).unwrap();
    settings.apply(Change::TypingDefaults).unwrap();
    settings.apply(Change::VietnameseMode(false)).unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 4);
    assert_eq!(
        document.exclusions.apps,
        [
            ExeName::new("putty.exe").unwrap(),
            ExeName::new("mstsc.exe").unwrap()
        ]
    );
    assert!(document.exclusions.extensions.contains_key("x_note"));
    assert_eq!(
        settings.runtime_snapshot().compatibility,
        with_exclusions(
            &["putty.exe", "mstsc.exe"],
            CompatibilityConfig::defaults_v1()
        )
    );
}

/// Names pasted from Explorer or Task Manager (a path, a name without `.exe`) take effect as the
/// executable name they stand for; unusable names in `[exclusions] apps` are reported by their
/// schema path, never by their value. A commit writes only the normalized, usable names, so
/// the report ends there and the file shows what is matched.
#[test]
fn unusable_excluded_names_are_reported_by_path_until_a_commit_drops_them() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[exclusions]\n\
         apps = ['D:\\Secret\\', 'C:\\Windows\\System32\\mstsc.exe', 7, \"PuTTY\"]\n",
    )
    .unwrap();
    let (mut settings, report) = folders.open();
    assert_eq!(
        report.status.config,
        DocumentStatus::Loaded {
            issues: vec![
                issue("exclusions.apps[0]", IssueReason::InvalidValue),
                issue("exclusions.apps[2]", IssueReason::InvalidType),
            ]
        }
    );
    assert!(!format!("{report:?}").contains("Secret"));
    assert_eq!(
        settings.runtime_snapshot().compatibility,
        with_exclusions(
            &["mstsc.exe", "putty.exe"],
            CompatibilityConfig::defaults_v1()
        )
    );
    settings.apply(Change::SpellCheck(true)).unwrap();
    assert_eq!(settings.status().config, loaded_clean());
    assert_eq!(
        stored(&folders.config()).exclusions.apps,
        [
            ExeName::new("mstsc.exe").unwrap(),
            ExeName::new("putty.exe").unwrap()
        ]
    );
}

/// `İ` (U+0130) lowercases to two characters, so this executable name of 134 characters is
/// 264 once lowercased: longer than an executable name may be.
fn too_long_once_lowercased() -> String {
    format!("{}.exe", "\u{130}".repeat(130))
}

/// An excluded name that the session core could not take is left out on its own and reported
/// by its index; the other exclusions stay in effect.
#[test]
fn an_excluded_name_too_long_once_lowercased_leaves_the_others_in_effect() {
    let folders = Folders::new();
    fs::write(
        folders.config(),
        format!(
            "version = 1\nrevision = 1\n[exclusions]\napps = [\"mstsc.exe\", \"{}\", \"putty\"]\n",
            too_long_once_lowercased()
        ),
    )
    .unwrap();
    let (settings, report) = folders.open();
    assert_eq!(
        report.status.config,
        DocumentStatus::Loaded {
            issues: vec![issue("exclusions.apps[1]", IssueReason::InvalidValue)]
        }
    );
    let runtime = settings.runtime_snapshot();
    assert_eq!(
        runtime.compatibility,
        with_exclusions(
            &["mstsc.exe", "putty.exe"],
            CompatibilityConfig::defaults_v1()
        )
    );
    assert!(!policy_of(runtime, "mstsc.exe", None).vietnamese_allowed);
    assert_consumable(&settings);
}

/// The same name in a rule of `raumakey-app-rules.toml` is a problem of that rule alone: the
/// file stays usable and its other rules stay in effect.
#[test]
fn a_rule_for_a_name_too_long_once_lowercased_leaves_the_other_rules_in_effect() {
    let folders = Folders::new();
    fs::write(
        folders.app_rules(),
        format!(
            "version = 1\nrevision = 1\n[[rule]]\nexe = \"{}\"\nvietnamese = false\n\
             [[rule]]\nexe = \"notepad.exe\"\nmacros = false\n",
            too_long_once_lowercased()
        ),
    )
    .unwrap();
    let (settings, report) = folders.open();
    let DocumentStatus::Loaded { issues } = &report.status.app_rules else {
        panic!("{:?}", report.status.app_rules);
    };
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].path.starts_with("rule[0]"), "{issues:?}");
    let runtime = settings.runtime_snapshot();
    let notepad = policy_of(runtime, "notepad.exe", None);
    assert!(notepad.vietnamese_allowed);
    assert!(!notepad.macro_allowed, "the file's other rule is in effect");
    assert_consumable(&settings);
}

/// A change that could not be saved is kept for a retry; once the user fixes the damaged file
/// by hand, the reload keeps the change in effect on top of the fixed file, and the retry
/// writes both.
#[test]
fn a_change_that_could_not_be_saved_is_kept_and_saved_by_a_retry() {
    let folders = Folders::new();
    fs::write(folders.config(), "[[[ damaged").unwrap();
    let (mut settings, _) = folders.open();
    assert!(!settings.has_unsaved());
    assert_eq!(
        settings.apply(Change::SpellCheck(true)),
        Err(ConfigError::Corrupt)
    );
    assert!(settings.has_unsaved());
    // Still damaged: the retry fails the same way and keeps the change.
    assert_eq!(settings.save_unsaved(), Err(ConfigError::Corrupt));
    assert!(settings.has_unsaved());
    assert_eq!(fs::read(folders.config()).unwrap(), b"[[[ damaged");
    fs::write(
        folders.config(),
        "version = 1\nrevision = 3\n[input]\nmodern_tone = true\n",
    )
    .unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert!(runtime.options.modern_tone, "the hand edit is taken");
    assert!(
        runtime.options.spell_check,
        "the unsaved change stays in effect"
    );
    assert_eq!(settings.save_unsaved(), Ok(true));
    assert!(!settings.has_unsaved());
    let document = stored(&folders.config());
    assert!(document.input.modern_tone);
    assert!(document.input.spell_check);
    assert_eq!(document.revision, 4);
    assert_eq!(settings.save_unsaved(), Ok(false), "nothing left to save");
}

/// The next change that is saved writes the earlier unsaved one as well, even when the file
/// changed meanwhile (the commit's conflict loads it again).
#[test]
fn a_later_save_writes_the_earlier_unsaved_change_too() {
    let folders = Folders::new();
    fs::write(folders.config(), "[[[ damaged").unwrap();
    let (mut settings, _) = folders.open();
    assert!(settings.apply(Change::Macros(false)).is_err());
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nmethod = \"vni\"\n",
    )
    .unwrap();
    settings.apply(Change::ModernTone(true)).unwrap();
    assert!(!settings.has_unsaved());
    let document = stored(&folders.config());
    assert_eq!(document.input.method, InputMethod::Vni);
    assert!(!document.macros.enabled);
    assert!(document.input.modern_tone);
}

/// Of two unsaved changes of the same setting, the last one is kept.
#[test]
fn the_last_unsaved_change_of_a_setting_wins() {
    let folders = Folders::new();
    fs::write(folders.config(), "[[[ damaged").unwrap();
    let (mut settings, _) = folders.open();
    let _ = settings.apply(Change::InputMethod(InputMethod::Vni));
    let _ = settings.apply(Change::InputMethod(InputMethod::Viqr));
    fs::write(folders.config(), "version = 1\nrevision = 1\n").unwrap();
    settings.poll().unwrap();
    assert_eq!(settings.save_unsaved(), Ok(true));
    assert_eq!(stored(&folders.config()).input.method, InputMethod::Viqr);
}

/// Without a folder nothing can ever be saved, so nothing is kept for a retry.
#[test]
fn nothing_is_kept_for_a_retry_without_a_folder() {
    let (mut settings, report) = Settings::open(Path::new(""), None, &|_| true, &|_| false);
    assert_eq!(report.location, Location::Unavailable);
    assert_eq!(
        settings.apply(Change::Macros(false)),
        Err(ConfigError::NotFound)
    );
    assert!(!settings.has_unsaved());
    assert_eq!(settings.save_unsaved(), Ok(false));
}

// --- Edits of the exclusions dialog (D48) ---------------------------------------------------

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn exes(names: &[&str]) -> Vec<ExeName> {
    names.iter().map(|name| exe(name)).collect()
}

/// The default exclusions followed by `more`.
fn defaults_and(more: &[&str]) -> Vec<ExeName> {
    let mut apps = ExclusionsSection::default().apps;
    apps.extend(exes(more));
    apps
}

/// Each edit of the exclusions dialog takes effect in the list and the runtime snapshot and is
/// committed at once; an edit of what is in effect already writes nothing.
#[test]
fn an_exclusion_edit_takes_effect_and_is_committed_at_once() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    assert_eq!(settings.excluded_apps(), &defaults_and(&[])[..]);

    settings
        .edit_exclusions(ExclusionsRequest::Add(exe("notepad.exe")))
        .unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 2);
    assert_eq!(document.exclusions.apps, defaults_and(&["notepad.exe"]));
    assert_eq!(settings.document(), &document);
    assert_eq!(
        settings.excluded_apps(),
        &defaults_and(&["notepad.exe"])[..]
    );
    let runtime = settings.runtime_snapshot();
    assert_eq!(
        runtime.compatibility,
        with_exclusions(
            &[
                "mstsc.exe",
                "msrdc.exe",
                "rdcman.exe",
                "vmconnect.exe",
                "notepad.exe"
            ],
            CompatibilityConfig::defaults_v1()
        )
    );
    assert!(!policy_of(runtime, "notepad.exe", None).vietnamese_allowed);
    assert_eq!(runtime.config_revision, 2);
    assert_consumable(&settings);

    // Listed already: nothing is written.
    settings
        .edit_exclusions(ExclusionsRequest::Add(exe("notepad.exe")))
        .unwrap();
    assert_eq!(stored(&folders.config()).revision, 2);

    settings
        .edit_exclusions(ExclusionsRequest::Remove(exe("mstsc.exe")))
        .unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 3);
    assert_eq!(
        document.exclusions.apps,
        exes(&["msrdc.exe", "rdcman.exe", "vmconnect.exe", "notepad.exe"])
    );
    assert!(policy_of(settings.runtime_snapshot(), "mstsc.exe", None).vietnamese_allowed);
    settings
        .edit_exclusions(ExclusionsRequest::Remove(exe("mstsc.exe")))
        .unwrap();
    assert_eq!(stored(&folders.config()).revision, 3, "not listed: nothing");

    settings
        .edit_exclusions(ExclusionsRequest::Defaults)
        .unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 4);
    assert_eq!(document.exclusions.apps, defaults_and(&[]));
    settings
        .edit_exclusions(ExclusionsRequest::Defaults)
        .unwrap();
    assert_eq!(
        stored(&folders.config()).revision,
        4,
        "the defaults already"
    );
    assert!(settings.poll().is_none());
}

/// Removing every application leaves an empty list, which is written as `apps = []`: it
/// excludes nothing, also once the file is read again, and the rules of the app-rules file
/// (here the D13 defaults) stay alone.
#[test]
fn removing_every_application_writes_an_empty_list() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    for app in ExclusionsSection::default().apps {
        settings
            .edit_exclusions(ExclusionsRequest::Remove(app))
            .unwrap();
    }
    assert!(settings.excluded_apps().is_empty());
    assert!(
        fs::read_to_string(folders.config())
            .unwrap()
            .contains("apps = []")
    );
    assert!(stored(&folders.config()).exclusions.apps.is_empty());
    assert_eq!(
        settings.runtime_snapshot().compatibility,
        CompatibilityConfig::defaults_v1()
    );
    let (reopened, _) = folders.open();
    assert!(reopened.excluded_apps().is_empty());
}

/// An edit that cannot be saved stays in effect, survives a hand edit that fixes the file (on
/// top of it), and the retry writes it, as every change does.
#[test]
fn an_exclusion_edit_that_cannot_be_saved_is_kept_and_saved_by_a_retry() {
    let folders = Folders::new();
    fs::write(folders.config(), "[[[ damaged").unwrap();
    let (mut settings, _) = folders.open();
    assert_eq!(
        settings.edit_exclusions(ExclusionsRequest::Add(exe("notepad.exe"))),
        Err(ConfigError::Corrupt)
    );
    assert!(settings.has_unsaved());
    assert_eq!(
        settings.excluded_apps(),
        &defaults_and(&["notepad.exe"])[..]
    );
    assert!(!policy_of(settings.runtime_snapshot(), "notepad.exe", None).vietnamese_allowed);
    assert_eq!(settings.save_unsaved(), Err(ConfigError::Corrupt));
    assert_eq!(fs::read(folders.config()).unwrap(), b"[[[ damaged");

    fs::write(
        folders.config(),
        "version = 1\nrevision = 3\n[exclusions]\napps = [\"putty.exe\"]\n",
    )
    .unwrap();
    let runtime = settings.poll().unwrap().runtime.unwrap();
    assert_eq!(
        settings.excluded_apps(),
        &exes(&["putty.exe", "notepad.exe"])[..]
    );
    assert_eq!(
        runtime.compatibility,
        with_exclusions(
            &["putty.exe", "notepad.exe"],
            CompatibilityConfig::defaults_v1()
        )
    );
    assert_eq!(settings.save_unsaved(), Ok(true));
    assert!(!settings.has_unsaved());
    let document = stored(&folders.config());
    assert_eq!(document.revision, 4);
    assert_eq!(
        document.exclusions.apps,
        exes(&["putty.exe", "notepad.exe"])
    );
}

/// Unsaved edits are kept per application, the last of each, and "Mặc định" replaces every
/// edit before it; together with an unsaved setting, they end as the whole sequence did once
/// they are applied again on a hand-edited file.
#[test]
fn unsaved_exclusion_edits_are_kept_per_application_in_their_order() {
    let folders = Folders::new();
    fs::write(folders.config(), "[[[ damaged").unwrap();
    let (mut settings, _) = folders.open();
    for request in [
        ExclusionsRequest::Add(exe("a.exe")),
        ExclusionsRequest::Add(exe("b.exe")),
        ExclusionsRequest::Remove(exe("a.exe")),
        ExclusionsRequest::Remove(exe("mstsc.exe")),
    ] {
        assert!(settings.edit_exclusions(request).is_err());
    }
    assert!(settings.apply(Change::SpellCheck(true)).is_err());
    let in_effect = exes(&["msrdc.exe", "rdcman.exe", "vmconnect.exe", "b.exe"]);
    assert_eq!(settings.excluded_apps(), &in_effect[..]);
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[exclusions]\napps = [\"mstsc.exe\", \"putty.exe\", \
         \"a.exe\"]\n",
    )
    .unwrap();
    settings.poll().unwrap();
    assert_eq!(settings.excluded_apps(), &exes(&["putty.exe", "b.exe"])[..]);
    assert!(settings.runtime_snapshot().options.spell_check);
    assert_eq!(settings.save_unsaved(), Ok(true));
    let document = stored(&folders.config());
    assert_eq!(document.exclusions.apps, exes(&["putty.exe", "b.exe"]));
    assert!(document.input.spell_check);

    // "Mặc định" replaces the edits before it; an edit after it stays.
    fs::write(folders.config(), "[[[ damaged").unwrap();
    assert_eq!(
        settings.poll().unwrap().runtime,
        None,
        "the last good settings stay"
    );
    for request in [
        ExclusionsRequest::Add(exe("c.exe")),
        ExclusionsRequest::Defaults,
        ExclusionsRequest::Add(exe("d.exe")),
    ] {
        assert!(settings.edit_exclusions(request).is_err());
    }
    fs::write(
        folders.config(),
        "version = 1\nrevision = 5\n[exclusions]\napps = [\"putty.exe\", \"c.exe\"]\n",
    )
    .unwrap();
    settings.poll().unwrap();
    assert_eq!(settings.excluded_apps(), &defaults_and(&["d.exe"])[..]);
    assert_eq!(settings.save_unsaved(), Ok(true));
    assert_eq!(
        stored(&folders.config()).exclusions.apps,
        defaults_and(&["d.exe"])
    );
}

/// The file was edited by hand after the settings last saw it: the commit reports a conflict,
/// the file is loaded again, and the edit is made again on what it holds.
#[test]
fn an_exclusion_edit_over_an_unseen_hand_edit_keeps_the_edit() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    fs::write(
        folders.config(),
        "version = 1\nrevision = 1\n[input]\nmodern_tone = true\n[exclusions]\n\
         apps = [\"putty.exe\"]\n",
    )
    .unwrap();
    settings
        .edit_exclusions(ExclusionsRequest::Add(exe("notepad.exe")))
        .unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 2);
    assert!(document.input.modern_tone);
    assert_eq!(
        document.exclusions.apps,
        exes(&["putty.exe", "notepad.exe"])
    );
    assert_eq!(settings.document(), &document);
    assert!(settings.poll().is_none());
    // An edit that the hand edit holds already writes nothing more.
    fs::write(
        folders.config(),
        "version = 1\nrevision = 2\n[exclusions]\napps = [\"putty.exe\"]\n",
    )
    .unwrap();
    settings
        .edit_exclusions(ExclusionsRequest::Remove(exe("notepad.exe")))
        .unwrap();
    assert_eq!(settings.excluded_apps(), &exes(&["putty.exe"])[..]);
    assert!(!settings.has_unsaved());
}

/// Without a folder the edits take effect in memory, and nothing is kept for a retry.
#[test]
fn exclusion_edits_without_a_folder_take_effect_in_memory() {
    let (mut settings, report) = Settings::open(Path::new(""), None, &|_| true, &|_| false);
    assert_eq!(report.location, Location::Unavailable);
    assert_eq!(
        settings.edit_exclusions(ExclusionsRequest::Add(exe("notepad.exe"))),
        Err(ConfigError::NotFound)
    );
    assert!(!settings.has_unsaved());
    assert_eq!(
        settings.excluded_apps(),
        &defaults_and(&["notepad.exe"])[..]
    );
    assert!(!policy_of(settings.runtime_snapshot(), "notepad.exe", None).vietnamese_allowed);
    assert_eq!(
        settings.edit_exclusions(ExclusionsRequest::Add(exe("notepad.exe"))),
        Ok(()),
        "in effect already"
    );
}

/// A file that another program holds open for a moment: the change is saved by the retry once
/// the file is free, without any reload.
#[cfg(windows)]
#[test]
fn a_change_blocked_by_another_program_is_saved_by_the_retry() {
    let folders = Folders::new();
    let (mut settings, _) = folders.open();
    let handle = lock(&folders.config());
    assert!(settings.apply(Change::SpellCheck(true)).is_err());
    assert!(settings.has_unsaved());
    drop(handle);
    assert_eq!(settings.save_unsaved(), Ok(true));
    assert!(stored(&folders.config()).input.spell_check);
}

/// The temporary folder, where Explorer runs an executable from inside a ZIP and deletes it
/// later: a shortcut to an executable there would soon point at nothing (D41).
#[test]
fn executables_in_the_temporary_folder_are_temporary() {
    let temp = std::env::temp_dir();
    assert!(is_temporary_path(
        &temp.join("Temp1_raumakey.zip").join("raumakey.exe")
    ));
    assert!(
        !is_temporary_path(&temp),
        "the folder itself is not inside it"
    );
    assert!(!is_temporary_path(Path::new("raumakey.exe")));
    assert!(!is_temporary_path(Path::new("")));
    let elsewhere = Path::new(r"Q:\Tools\Raumakey\raumakey.exe");
    assert!(!is_temporary_path(elsewhere));
}
