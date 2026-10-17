//! The configuration lifecycle of the portable controller (`raumakey_controller::settings`,
//! D40): where `raumakey.toml` lies, the first run, loading, changes from the tray, hand edits,
//! damaged files, and the snapshots the session core consumes.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raumakey_config::{
    AtomicStore, ConfigDocument, ConfigError, FieldIssue, IssueReason, StartupMode, StdReplacer,
    StoreDocument, is_writable_dir,
};
use raumakey_controller::settings::{Change, DocumentStatus, Location, OpenReport, Settings};
use raumakey_ipc::message::{MAX_HOTKEYS, MAX_KEYMAPS, MAX_NAMED_SETS, RuntimeSnapshot};
use raumakey_macro::{EntryError, EntryIssue, MacroError, MacroTable};
use raumakey_model::hotkey::{Hotkey, HotkeyConfig};
use raumakey_model::macros::MacroId;
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_model::policy::CompatibilityConfig;

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
        Settings::open(self.exe(), Some(self.local()), &|_| true)
    }

    /// Opens with an executable folder that cannot be written.
    fn open_unwritable(&self) -> (Settings, OpenReport) {
        Settings::open(self.exe(), Some(self.local()), &|_| false)
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
    let (_settings, report) =
        Settings::open(folders.exe(), Some(folders.local()), &is_writable_dir);
    assert_eq!(report.location, Location::ExeDir);
    assert!(report.first_run);
    assert_eq!(
        listing(folders.exe()),
        vec!["raumakey.toml".to_owned(), "raumakey.toml.lock".to_owned()]
    );
}

#[test]
fn the_default_snapshots_are_defaults_v1_with_the_ctrl_shift_toggle() {
    let folders = Folders::new();
    let (settings, _) = folders.open();
    let runtime = settings.runtime_snapshot();
    assert_eq!(runtime.config_revision, 1);
    assert_eq!(runtime.options, EngineOptions::defaults_v1());
    assert!(runtime.vietnamese_mode);
    assert!(settings.vietnamese_mode());
    assert!(!runtime.diagnostics_enabled);
    assert!(runtime.keymaps.is_empty());
    assert_eq!(runtime.hotkeys, HotkeyConfig::default());
    assert_eq!(runtime.hotkeys.toggle_mode, vec![hotkey("Ctrl+Shift")]);
    assert_eq!(runtime.compatibility, CompatibilityConfig::defaults_v1());
    let defaults = RuntimeSnapshot::defaults_v1();
    assert_eq!(
        RuntimeSnapshot {
            config_revision: 0,
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
        settings.apply(Change::SpellCheck(false)).unwrap();
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
    assert!(!document.input.spell_check);
    assert!(!document.input.enabled);
    let runtime = settings.runtime_snapshot();
    assert_eq!(runtime.config_revision, 4);
    assert_eq!(runtime.options.input_method, InputMethod::Vni);
    assert!(!runtime.options.spell_check);
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

/// Neither folder can be used: the controller still runs, on the defaults, and every change
/// takes effect without being saved.
#[test]
fn without_a_usable_folder_the_settings_live_in_memory() {
    let (mut settings, report) = Settings::open(Path::new("relative"), None, &|_| true);
    assert_eq!(report.location, Location::Unavailable);
    assert_eq!(report.config_path, None);
    assert!(report.first_run);
    assert_eq!(report.create_error, Some(ConfigError::NotFound));
    assert_eq!(report.status.config, DocumentStatus::Missing);
    assert_eq!(
        settings.runtime_snapshot().options,
        EngineOptions::defaults_v1()
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
        "version = 1\nrevision = 1\n[input]\nmethod = \"klingon\"\nspell_check = false\n",
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
    assert!(!settings.document().input.spell_check);
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
        EngineOptions::defaults_v1()
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

    let backup = settings.recover().unwrap().unwrap();
    assert_eq!(backup.parent(), Some(folders.exe()));
    let name = backup.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("raumakey.toml.bak-"), "{name}");
    assert_eq!(fs::read(&backup).unwrap(), corrupt);
    // The settings in effect, with the change, are in the new file.
    let document = stored(&folders.config());
    assert!(!document.input.enabled);
    assert_eq!(settings.status().config, loaded_clean());
    assert_eq!(settings.document().revision, document.revision);
    settings.apply(Change::SpellCheck(false)).unwrap();
    assert!(!stored(&folders.config()).input.spell_check);
    // Nothing is left to recover.
    assert_eq!(settings.recover(), Ok(None));
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
        Change::SpellCheck(false),
        Change::ModernTone(true),
        Change::Macros(false),
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
    assert!(!document.input.spell_check);
    assert!(document.input.modern_tone);
    assert!(!document.macros.enabled);
    assert_eq!(document.system.startup, StartupMode::Shortcut);
    let options = &settings.runtime_snapshot().options;
    assert!(!options.enabled);
    assert_eq!(options.input_method, InputMethod::Viqr);
    assert!(!options.spell_check);
    assert!(options.modern_tone);
    assert!(!options.macros_enabled);
    settings.apply(Change::Startup(StartupMode::None)).unwrap();
    assert_eq!(stored(&folders.config()).system.startup, StartupMode::None);
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
    settings.apply(Change::SpellCheck(false)).unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.input.method, InputMethod::Viqr);
    assert!(!document.input.spell_check);
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
    assert_eq!(runtime.options, EngineOptions::defaults_v1());
    assert_eq!(settings.status().config, DocumentStatus::Missing);
    assert!(settings.poll().is_none());
    settings.apply(Change::SpellCheck(false)).unwrap();
    let document = stored(&folders.config());
    assert_eq!(document.revision, 1);
    assert_eq!(document.input.method, InputMethod::Telex);
    assert!(!document.input.spell_check);
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
    settings.apply(Change::SpellCheck(false)).unwrap();
    let document = stored(&folders.config());
    assert!(document.input.modern_tone);
    assert!(!document.input.spell_check);
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
    assert_eq!(runtime.compatibility, CompatibilityConfig::defaults_v1());
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
    assert_eq!(
        settings.runtime_snapshot().compatibility,
        CompatibilityConfig::defaults_v1()
    );
    assert_consumable(&settings);
}
