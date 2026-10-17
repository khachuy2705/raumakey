//! Committing a migration into temp-dir stores: verification, rollback, idempotence, preview.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::SystemTime;

use common::{FOLDER, MACRO_TEXT, TONE_MAP, evkey_fs, evkey_registry};
use raumakey_config::{
    AppRulesDocument, AtomicStore, CommitResult, ConfigDocument, ConfigError, FileReplacer,
    ImportedCounts, MacroDocument, MigrationRecord, MigrationStatus, StdReplacer, StoreDocument,
    sha256_hex,
};
use raumakey_migrate::{
    DocumentKind, FakeFs, FakeRegistry, MigrateError, MigrationOutcome, MigrationStores,
    ParsedMigration, SourceKind, commit, discover, parse, parse_with_field_map, preview,
};
use raumakey_model::host::ExeName;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::policy::{NamedSet, RuleAction};

const NOW: u64 = 1_700_000_000_000;

fn std_replacer() -> Arc<dyn FileReplacer> {
    Arc::new(StdReplacer)
}

/// Fails every replace.
struct FailingReplacer;

impl FileReplacer for FailingReplacer {
    fn replace(&self, _replacement: &Path, _target: &Path) -> io::Result<()> {
        Err(io::Error::other("injected failure"))
    }
}

/// Fails only the replace with the given one-based call number.
struct FailOnCall {
    call: usize,
    calls: AtomicUsize,
}

impl FailOnCall {
    fn shared(call: usize) -> Arc<dyn FileReplacer> {
        Arc::new(Self {
            call,
            calls: AtomicUsize::new(0),
        })
    }
}

impl FileReplacer for FailOnCall {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if call == self.call {
            return Err(io::Error::other("injected failure"));
        }
        fs::rename(replacement, target)
    }
}

/// Replaces normally, then changes another file (as a concurrent editor would).
struct TamperingReplacer {
    other: PathBuf,
}

impl FileReplacer for TamperingReplacer {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        fs::rename(replacement, target)?;
        let mut file = fs::OpenOptions::new().append(true).open(&self.other)?;
        file.write_all(b"\n# changed by another editor\n")
    }
}

struct Stores {
    config: AtomicStore<ConfigDocument>,
    macros: AtomicStore<MacroDocument>,
    app_rules: AtomicStore<AppRulesDocument>,
    record: AtomicStore<MigrationRecord>,
}

impl Stores {
    fn open(dir: &Path) -> Self {
        Self::open_with(
            dir,
            [
                std_replacer(),
                std_replacer(),
                std_replacer(),
                std_replacer(),
            ],
        )
    }

    fn open_with(dir: &Path, replacers: [Arc<dyn FileReplacer>; 4]) -> Self {
        let [config, macros, app_rules, record] = replacers;
        Self {
            config: AtomicStore::new(dir, config),
            macros: AtomicStore::new(dir, macros),
            app_rules: AtomicStore::new(dir, app_rules),
            record: AtomicStore::new(dir, record),
        }
    }

    fn view(&self) -> MigrationStores<'_> {
        MigrationStores {
            config: &self.config,
            macros: &self.macros,
            app_rules: &self.app_rules,
            record: &self.record,
        }
    }
}

fn macro_entry(id: u64, trigger: &str, replacement: &str) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    }
}

fn names(list: &[&str]) -> Vec<ExeName> {
    list.iter()
        .map(|name| ExeName::new(name).unwrap())
        .collect()
}

/// Seeds config, macros, and app rules at revision 1. `ko` and the named set
/// `evkey-import-2` conflict with the fixture.
fn seed(dir: &Path) {
    let stores = Stores::open(dir);
    stores
        .config
        .commit(0, &ConfigDocument::defaults())
        .unwrap();
    let macros = MacroDocument {
        entries: vec![macro_entry(5, "ko", "có"), macro_entry(2, "hn", "Hà Nội")],
        ..MacroDocument::default()
    };
    stores.macros.commit(0, &macros).unwrap();
    let mut rules = AppRulesDocument::default();
    rules.compatibility.named_sets.push(NamedSet {
        name: "evkey-import-2".to_owned(),
        members: names(&["kept.exe"]),
        action: RuleAction {
            reset_on_mouse: Some(true),
            ..RuleAction::default()
        },
    });
    stores.app_rules.commit(0, &rules).unwrap();
}

fn fixture_parse() -> ParsedMigration {
    parse(&discover(&evkey_registry(), &evkey_fs(), [FOLDER]).unwrap())
}

fn mapped_parse() -> ParsedMigration {
    parse_with_field_map(
        &discover(&evkey_registry(), &evkey_fs(), [FOLDER]).unwrap(),
        TONE_MAP,
    )
}

type Snapshot = BTreeMap<String, (Vec<u8>, SystemTime)>;

/// Every file of `dir` with its bytes and modification time.
fn snapshot(dir: &Path) -> Snapshot {
    fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().to_string_lossy().into_owned();
            let bytes = fs::read(entry.path()).unwrap();
            let modified = entry.metadata().unwrap().modified().unwrap();
            (name, (bytes, modified))
        })
        .collect()
}

fn bytes_of(snapshot: &Snapshot, name: &str) -> Vec<u8> {
    snapshot[name].0.clone()
}

fn temp_files(dir: &Path) -> Vec<String> {
    snapshot(dir)
        .into_keys()
        .filter(|name| name.contains(".tmp-"))
        .collect()
}

fn without_revision<D: StoreDocument>(mut document: D) -> D {
    document.set_revision(0);
    document
}

#[test]
fn a_full_import_commits_the_documents_and_records_completion() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let before = snapshot(dir.path());
    let stores = Stores::open(dir.path());
    let parsed = fixture_parse();

    let report = commit(&parsed, &stores.view(), NOW).unwrap();
    assert_eq!(report.outcome, MigrationOutcome::Imported);
    assert_eq!(
        report.imported,
        ImportedCounts {
            settings: 0,
            macros: 2,
            named_sets: 1,
            probes: 0,
        }
    );
    assert_eq!(report.macro_conflicts, 2);
    assert_eq!(report.named_set_conflicts, 1);
    assert_eq!(report.quarantined, 3);

    let macros = stores.macros.load().unwrap().document;
    assert_eq!(macros.revision, 2);
    let entries: Vec<(u64, &str, &str)> = macros
        .entries
        .iter()
        .map(|entry| {
            (
                entry.id.0,
                entry.trigger.as_str(),
                entry.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(
        entries,
        [
            (5, "ko", "có"),
            (2, "hn", "Hà Nội"),
            (6, "dc", "được"),
            (7, "vn", "Việt Nam"),
        ]
    );

    let rules = stores.app_rules.load().unwrap().document;
    assert_eq!(rules.revision, 2);
    let sets = &rules.compatibility.named_sets;
    assert_eq!(sets.len(), 5);
    assert_eq!(sets[3].name, "evkey-import-2");
    assert_eq!(sets[3].members, names(&["kept.exe"]));
    assert_eq!(sets[3].action.reset_on_mouse, Some(true));
    assert_eq!(
        sets[4],
        NamedSet {
            name: "evkey-import-1".to_owned(),
            members: names(&["chat.exe", "mail.exe"]),
            action: RuleAction::default(),
        }
    );

    // FIELD_MAP is empty, so config.toml is not rewritten.
    let after = snapshot(dir.path());
    assert_eq!(after["raumakey.toml"], before["raumakey.toml"]);

    let record = stores.record.load().unwrap().document;
    assert_eq!(record.status, MigrationStatus::Completed);
    assert_eq!(record.revision, 2);
    assert_eq!(report.record_revision, 2);
    assert_eq!(record.started_at_ms, NOW);
    assert_eq!(record.completed_at_ms, NOW);
    assert_eq!(record.sources, parsed.sources());
    assert_eq!(record.imported, report.imported);
    assert_eq!(record.quarantined, parsed.quarantined());
    assert_eq!(&record.preserved, parsed.preserved());
    let mut checksums = BTreeMap::new();
    checksums.insert(
        "raumakey-app-rules.toml".to_owned(),
        sha256_hex(&bytes_of(&after, "raumakey-app-rules.toml")),
    );
    checksums.insert(
        "raumakey-macros.toml".to_owned(),
        sha256_hex(&bytes_of(&after, "raumakey-macros.toml")),
    );
    let expected = CommitResult {
        config_revision: 1,
        macros_revision: 2,
        app_rules_revision: 2,
        checksums,
    };
    assert_eq!(record.result.as_ref(), Some(&expected));
    assert_eq!(report.result, Some(expected));
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn an_import_into_an_empty_data_root_creates_the_documents() {
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    let report = commit(&fixture_parse(), &stores.view(), NOW).unwrap();
    assert_eq!(report.imported.macros, 3);
    assert_eq!(report.macro_conflicts, 1);
    assert_eq!(report.imported.named_sets, 2);
    let macros = stores.macros.load().unwrap().document;
    let ids: Vec<u64> = macros.entries.iter().map(|entry| entry.id.0).collect();
    assert_eq!(ids, [1, 2, 3]);
    let rules = stores.app_rules.load().unwrap().document;
    // The D13 defaults of a missing file are kept, and the imported sets follow them.
    let set_names: Vec<&str> = rules
        .compatibility
        .named_sets
        .iter()
        .map(|set| set.name.as_str())
        .collect();
    assert_eq!(
        set_names,
        [
            "chromium-family",
            "terminals",
            "games",
            "evkey-import-1",
            "evkey-import-2"
        ]
    );
    assert!(!stores.config.path().exists());
    let result = report.result.unwrap();
    assert_eq!(result.config_revision, 0);
    assert_eq!(result.macros_revision, 1);
    assert_eq!(result.app_rules_revision, 1);
}

#[test]
fn committed_documents_are_verified_by_reading_them_back() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let stores = Stores::open(dir.path());
    let report = commit(&mapped_parse(), &stores.view(), NOW).unwrap();
    let result = report.result.unwrap();
    assert_eq!(result.checksums.len(), 3);
    for (file, checksum) in &result.checksums {
        let bytes = fs::read(dir.path().join(file)).unwrap();
        assert_eq!(&sha256_hex(&bytes), checksum, "{file}");
    }
    assert_eq!(
        stores.config.load().unwrap().document.revision,
        result.config_revision
    );
    assert_eq!(
        stores.macros.load().unwrap().document.revision,
        result.macros_revision
    );
    assert_eq!(
        stores.app_rules.load().unwrap().document.revision,
        result.app_rules_revision
    );
}

#[test]
fn mapped_settings_are_committed_only_when_they_change_the_config() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let stores = Stores::open(dir.path());
    let report = commit(&mapped_parse(), &stores.view(), NOW).unwrap();
    assert_eq!(report.imported.settings, 1);
    let config = stores.config.load().unwrap().document;
    assert!(config.input.modern_tone);
    assert_eq!(config.revision, 2);
    assert_eq!(
        without_revision(config.clone()),
        without_revision({
            let mut expected = ConfigDocument::defaults();
            expected.input.modern_tone = true;
            expected
        })
    );

    // A changed source with the same setting value leaves config.toml alone.
    let mut fs = evkey_fs();
    let mut macro_text = MACRO_TEXT.to_vec();
    macro_text.extend_from_slice("zz:mới\n".as_bytes());
    fs.add_file(format!("{FOLDER}/evkmacro.txt"), macro_text);
    let parsed = parse_with_field_map(
        &discover(&evkey_registry(), &fs, [FOLDER]).unwrap(),
        TONE_MAP,
    );
    let config_before = fs::read(stores.config.path()).unwrap();
    let report = commit(&parsed, &stores.view(), NOW + 1).unwrap();
    assert_eq!(report.imported.settings, 1);
    assert_eq!(report.result.unwrap().config_revision, 2);
    assert_eq!(fs::read(stores.config.path()).unwrap(), config_before);
}

#[test]
fn a_failing_second_store_rolls_back_every_document() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let config_before = Stores::open(dir.path()).config.load().unwrap().document;
    let before = snapshot(dir.path());
    let stores = Stores::open_with(
        dir.path(),
        [
            std_replacer(),
            Arc::new(FailingReplacer),
            std_replacer(),
            std_replacer(),
        ],
    );
    let parsed = mapped_parse();

    let error = commit(&parsed, &stores.view(), NOW).unwrap_err();
    assert_eq!(
        error,
        MigrateError::Store {
            document: DocumentKind::Macros,
            error: ConfigError::IoFailure,
        }
    );
    // config.toml was committed (revision 2) and restored (revision 3).
    let config = stores.config.load().unwrap().document;
    assert_eq!(config.revision, 3);
    assert_eq!(without_revision(config), without_revision(config_before));
    let after = snapshot(dir.path());
    assert_eq!(
        after["raumakey-macros.toml"],
        before["raumakey-macros.toml"]
    );
    assert_eq!(
        after["raumakey-app-rules.toml"],
        before["raumakey-app-rules.toml"]
    );
    let record = stores.record.load().unwrap().document;
    assert_eq!(record.status, MigrationStatus::RolledBack);
    assert_eq!(record.result, None);
    assert_eq!(record.sources, parsed.sources());
    assert_eq!(record.imported, ImportedCounts::default());
    assert_eq!(record.completed_at_ms, NOW);
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn a_failing_third_store_restores_the_first_two() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let seeded = Stores::open(dir.path());
    let config_before = seeded.config.load().unwrap().document;
    let macros_before = seeded.macros.load().unwrap().document;
    let before = snapshot(dir.path());
    let stores = Stores::open_with(
        dir.path(),
        [
            std_replacer(),
            std_replacer(),
            Arc::new(FailingReplacer),
            std_replacer(),
        ],
    );

    let error = commit(&mapped_parse(), &stores.view(), NOW).unwrap_err();
    assert_eq!(
        error,
        MigrateError::Store {
            document: DocumentKind::AppRules,
            error: ConfigError::IoFailure,
        }
    );
    assert_eq!(
        without_revision(stores.config.load().unwrap().document),
        without_revision(config_before)
    );
    let macros = stores.macros.load().unwrap().document;
    assert_eq!(macros.revision, 3);
    assert_eq!(without_revision(macros), without_revision(macros_before));
    assert_eq!(
        snapshot(dir.path())["raumakey-app-rules.toml"],
        before["raumakey-app-rules.toml"]
    );
    assert_eq!(
        stores.record.load().unwrap().document.status,
        MigrationStatus::RolledBack
    );
}

#[test]
fn a_failing_completion_record_rolls_back_the_documents() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let seeded = Stores::open(dir.path());
    let macros_before = seeded.macros.load().unwrap().document;
    let rules_before = seeded.app_rules.load().unwrap().document;
    let stores = Stores::open_with(
        dir.path(),
        [
            std_replacer(),
            std_replacer(),
            std_replacer(),
            FailOnCall::shared(2),
        ],
    );

    let error = commit(&fixture_parse(), &stores.view(), NOW).unwrap_err();
    assert_eq!(
        error,
        MigrateError::Store {
            document: DocumentKind::Record,
            error: ConfigError::IoFailure,
        }
    );
    assert_eq!(
        without_revision(stores.macros.load().unwrap().document),
        without_revision(macros_before)
    );
    assert_eq!(
        without_revision(stores.app_rules.load().unwrap().document),
        without_revision(rules_before)
    );
    let record = stores.record.load().unwrap().document;
    assert_eq!(record.status, MigrationStatus::RolledBack);
    assert_eq!(record.revision, 2);
}

/// Another editor changes `macros.toml` after the migration committed it: verification fails,
/// and the rollback restores what it can but never overwrites the other editor's change.
#[test]
fn a_concurrent_edit_fails_verification_and_is_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let rules_before = Stores::open(dir.path()).app_rules.load().unwrap().document;
    let tamper: Arc<dyn FileReplacer> = Arc::new(TamperingReplacer {
        other: dir.path().join("raumakey-macros.toml"),
    });
    let stores = Stores::open_with(
        dir.path(),
        [std_replacer(), std_replacer(), tamper, std_replacer()],
    );

    let error = commit(&fixture_parse(), &stores.view(), NOW).unwrap_err();
    assert_eq!(
        error,
        MigrateError::RollbackFailed {
            cause: Box::new(MigrateError::Verification {
                document: DocumentKind::Macros,
            }),
            document: DocumentKind::Macros,
        }
    );
    let macros = fs::read_to_string(stores.macros.path()).unwrap();
    assert!(macros.contains("changed by another editor"));
    assert_eq!(
        without_revision(stores.app_rules.load().unwrap().document),
        without_revision(rules_before)
    );
    assert_eq!(
        stores.record.load().unwrap().document.status,
        MigrationStatus::Failed
    );
}

#[test]
fn a_rollback_that_cannot_restore_a_document_records_failed() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let stores = Stores::open_with(
        dir.path(),
        [
            FailOnCall::shared(2),
            Arc::new(FailingReplacer),
            std_replacer(),
            std_replacer(),
        ],
    );

    let error = commit(&mapped_parse(), &stores.view(), NOW).unwrap_err();
    assert_eq!(
        error,
        MigrateError::RollbackFailed {
            cause: Box::new(MigrateError::Store {
                document: DocumentKind::Macros,
                error: ConfigError::IoFailure,
            }),
            document: DocumentKind::Config,
        }
    );
    // The migrated config could not be restored and says so in the record.
    assert!(stores.config.load().unwrap().document.input.modern_tone);
    assert_eq!(
        stores.record.load().unwrap().document.status,
        MigrationStatus::Failed
    );
}

#[test]
fn an_unwritable_record_aborts_before_any_document_changes() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let before = snapshot(dir.path());
    let stores = Stores::open_with(
        dir.path(),
        [
            std_replacer(),
            std_replacer(),
            std_replacer(),
            Arc::new(FailingReplacer),
        ],
    );
    let error = commit(&mapped_parse(), &stores.view(), NOW).unwrap_err();
    assert_eq!(
        error,
        MigrateError::Store {
            document: DocumentKind::Record,
            error: ConfigError::IoFailure,
        }
    );
    let after = snapshot(dir.path());
    for file in [
        "raumakey.toml",
        "raumakey-macros.toml",
        "raumakey-app-rules.toml",
    ] {
        assert_eq!(after[file], before[file], "{file}");
    }
    assert!(!after.contains_key("raumakey-migration.json"));
}

#[test]
fn an_idempotent_rerun_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let stores = Stores::open(dir.path());
    let first = commit(&fixture_parse(), &stores.view(), NOW).unwrap();
    let before = snapshot(dir.path());

    let second = commit(&fixture_parse(), &stores.view(), NOW + 1).unwrap();
    assert_eq!(second.outcome, MigrationOutcome::NoChange);
    assert_eq!(second.record_revision, first.record_revision);
    assert_eq!(second.imported, ImportedCounts::default());
    assert_eq!(second.result, None);
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn the_rerun_check_ignores_the_order_of_recorded_sources() {
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    commit(&fixture_parse(), &stores.view(), NOW).unwrap();
    let mut record = stores.record.load().unwrap().document;
    record.sources.reverse();
    stores.record.commit(record.revision, &record).unwrap();
    let before = snapshot(dir.path());
    let report = commit(&fixture_parse(), &stores.view(), NOW).unwrap();
    assert_eq!(report.outcome, MigrationOutcome::NoChange);
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn a_changed_source_is_imported_again() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let stores = Stores::open(dir.path());
    let first = commit(&fixture_parse(), &stores.view(), NOW).unwrap();

    let mut fs = evkey_fs();
    let mut macro_text = MACRO_TEXT.to_vec();
    macro_text.extend_from_slice("zz:mới\n".as_bytes());
    fs.add_file(format!("{FOLDER}/evkmacro.txt"), macro_text);
    let parsed = parse(&discover(&evkey_registry(), &fs, [FOLDER]).unwrap());
    let second = commit(&parsed, &stores.view(), NOW + 5).unwrap();
    assert_eq!(second.outcome, MigrationOutcome::Imported);
    assert_eq!(second.imported.macros, 1);
    assert_eq!(second.imported.named_sets, 0);
    assert_eq!(second.macro_conflicts, 4);
    assert_eq!(second.named_set_conflicts, 2);
    assert!(second.record_revision > first.record_revision);
    let macros = stores.macros.load().unwrap().document;
    let last = macros.entries.last().unwrap();
    assert_eq!((last.id.0, last.trigger.as_str()), (8, "zz"));
    let record = stores.record.load().unwrap().document;
    assert_eq!(record.sources, parsed.sources());
    assert_eq!(record.started_at_ms, NOW + 5);
}

#[test]
fn an_interrupted_import_converges_when_run_again() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let stores = Stores::open(dir.path());
    let parsed = fixture_parse();
    commit(&parsed, &stores.view(), NOW).unwrap();
    // Simulate a crash after the documents were written: the record still says in-progress.
    let mut record = stores.record.load().unwrap().document;
    record.status = MigrationStatus::InProgress;
    record.result = None;
    stores.record.commit(record.revision, &record).unwrap();
    let macros_before = fs::read(stores.macros.path()).unwrap();

    let report = commit(&parsed, &stores.view(), NOW + 1).unwrap();
    assert_eq!(report.outcome, MigrationOutcome::Imported);
    assert_eq!(report.imported.macros, 0);
    assert_eq!(report.imported.named_sets, 0);
    assert_eq!(fs::read(stores.macros.path()).unwrap(), macros_before);
    assert_eq!(
        stores.record.load().unwrap().document.status,
        MigrationStatus::Completed
    );
}

#[test]
fn preview_matches_the_commit_and_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path());
    let stores = Stores::open(dir.path());
    let parsed = fixture_parse();
    let before = snapshot(dir.path());

    let current = stores.view().load_current().unwrap();
    let plan = preview(&parsed, &current);
    assert_eq!(snapshot(dir.path()), before);
    assert!(!plan.up_to_date);
    assert_eq!(plan.settings, 0);
    assert_eq!(plan.macro_count, 2);
    assert_eq!(plan.macro_conflicts, 2);
    assert_eq!(plan.named_sets, 1);
    assert_eq!(plan.named_set_conflicts, 1);
    assert_eq!(plan.quarantined, 3);
    assert_eq!(plan.preserved_counts, parsed.preserved_counts());
    assert_eq!(
        plan.preserved_counts.get(&SourceKind::SettingsIni),
        Some(&5)
    );

    let report = commit(&parsed, &stores.view(), NOW).unwrap();
    assert_eq!(report.imported.macros, plan.macro_count as u64);
    assert_eq!(report.macro_conflicts, plan.macro_conflicts as u64);
    assert_eq!(report.imported.named_sets, plan.named_sets as u64);
    assert_eq!(report.named_set_conflicts, plan.named_set_conflicts as u64);
    assert_eq!(report.quarantined, plan.quarantined as u64);

    let after = stores.view().load_current().unwrap();
    assert!(preview(&parsed, &after).up_to_date);
    assert_eq!(preview(&parsed, &after).macro_count, 0);
}

#[test]
fn preview_of_an_empty_data_root_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    let current = stores.view().load_current().unwrap();
    let plan = preview(&fixture_parse(), &current);
    assert_eq!(plan.macro_count, 3);
    assert!(snapshot(dir.path()).is_empty());
}

#[test]
fn a_corrupt_document_aborts_before_anything_is_written() {
    for (file, document) in [
        ("raumakey-app-rules.toml", DocumentKind::AppRules),
        ("raumakey-macros.toml", DocumentKind::Macros),
        ("raumakey.toml", DocumentKind::Config),
        ("raumakey-migration.json", DocumentKind::Record),
    ] {
        let dir = tempfile::tempdir().unwrap();
        seed(dir.path());
        fs::write(dir.path().join(file), b"[[[ not a document").unwrap();
        let before = snapshot(dir.path());
        let stores = Stores::open(dir.path());
        assert_eq!(
            commit(&fixture_parse(), &stores.view(), NOW).unwrap_err(),
            MigrateError::Store {
                document,
                error: ConfigError::Corrupt,
            },
            "{file}"
        );
        assert_eq!(snapshot(dir.path()), before, "{file}");
        assert!(stores.view().load_current().is_err());
    }
}

#[test]
fn a_migration_without_sources_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    let parsed = parse(&discover(&FakeRegistry::new(), &FakeFs::new(), [FOLDER]).unwrap());
    assert_eq!(
        commit(&parsed, &stores.view(), NOW).unwrap_err(),
        MigrateError::NoSources
    );
    assert!(snapshot(dir.path()).is_empty());
}

#[test]
fn imported_macros_get_identifiers_above_every_existing_one() {
    let dir = tempfile::tempdir().unwrap();
    // id 20 belongs to an entry without a replacement, which the document keeps verbatim.
    fs::write(
        dir.path().join("raumakey-macros.toml"),
        "version = 1\nrevision = 4\n\n[[macro]]\nid = 3\ntrigger = \"vie\u{302}\u{323}t\"\nreplacement = \"x\"\n\n[[macro]]\nid = 9\ntrigger = \"hn\"\nreplacement = \"y\"\n\n[[macro]]\nid = 20\ntrigger = \"dc\"\n",
    )
    .unwrap();
    let stores = Stores::open(dir.path());
    let text = "việt:Việt\ndc:được\nmoi:mới\n";
    let mut fs = FakeFs::new();
    fs.add_file(format!("{FOLDER}/evkmacro.txt"), text.as_bytes().to_vec());
    let parsed = parse(&discover(&FakeRegistry::new(), &fs, [FOLDER]).unwrap());

    let report = commit(&parsed, &stores.view(), NOW).unwrap();
    // `việt` matches the stored NFD trigger; `dc` matches the entry kept verbatim.
    assert_eq!(report.macro_conflicts, 2);
    assert_eq!(report.imported.macros, 1);
    let macros = stores.macros.load().unwrap();
    let last = macros.document.entries.last().unwrap();
    assert_eq!((last.id.0, last.trigger.as_str()), (21, "moi"));
    assert_eq!(macros.document.entry_extras.rejected.len(), 1);
    assert_eq!(macros.document.revision, 5);
}

#[test]
fn macro_identifier_exhaustion_is_reported_before_writing() {
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    // The largest identifier `macros.toml` can hold (a TOML integer).
    let macros = MacroDocument {
        entries: vec![macro_entry(0x7FFF_FFFF_FFFF_FFFF, "max", "x")],
        ..MacroDocument::default()
    };
    stores.macros.commit(0, &macros).unwrap();
    let before = snapshot(dir.path());
    assert_eq!(
        commit(&fixture_parse(), &stores.view(), NOW).unwrap_err(),
        MigrateError::MacroIdsExhausted
    );
    assert_eq!(snapshot(dir.path()), before);
}

#[test]
fn the_record_and_report_hold_no_macro_text() {
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    let report = commit(&fixture_parse(), &stores.view(), NOW).unwrap();
    let record = fs::read_to_string(stores.record.path()).unwrap();
    for text in ["được", "Việt Nam", "duplicate trigger", "broken line"] {
        assert!(!record.contains(text), "{text}");
        assert!(!format!("{report:?}").contains(text), "{text}");
    }
    let current = stores.view().load_current().unwrap();
    assert!(!format!("{current:?}").contains("được"));
}

/// The largest record parsing can produce still fits `migration.json`'s size limit.
#[test]
fn a_maximal_record_still_commits() {
    let mut registry = FakeRegistry::new();
    let path = raumakey_migrate::SETTINGS_REGISTRY_PATH;
    for index in 0..200 {
        registry.set_value(
            path,
            &format!("big{index:03}"),
            raumakey_migrate::LegacyValue::String("\"".repeat(60_000)),
        );
    }
    for index in 0..raumakey_migrate::MAX_RECORDED_QUARANTINE + 10 {
        registry.set_value(
            path,
            &format!("{index:05}{}", "\\".repeat(300)),
            raumakey_migrate::LegacyValue::Other {
                kind: raumakey_migrate::REG_SZ,
                bytes: vec![0x41],
            },
        );
    }
    let parsed = parse(&discover(&registry, &FakeFs::new(), [FOLDER]).unwrap());
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    let report = commit(&parsed, &stores.view(), NOW).unwrap();
    assert_eq!(report.outcome, MigrationOutcome::Imported);
    let size = fs::metadata(stores.record.path()).unwrap().len();
    assert!(
        size <= AtomicStore::<MigrationRecord>::MAX_FILE_BYTES,
        "{size}"
    );
}

/// A merge whose `macros.toml` would exceed the store's size limit fails before anything is
/// written, including the migration record.
#[test]
fn an_oversized_document_aborts_before_anything_is_written() {
    // The longest valid replacement (D30: 8,191 UTF-16 units, 16,381 UTF-8 bytes). Before D30
    // this used 4,096 emoji, which no longer fit one insert with a delimiter. 1,023 lines keep
    // the legacy text under its 16 MiB limit while `macros.toml` exceeds the store's.
    const ENTRIES: usize = 1_023;
    let replacement = format!("{}a", "\u{1F600}".repeat(4095));
    let mut text = String::new();
    for index in 0..ENTRIES {
        text.push_str(&format!("m{index:04}:{replacement}\n"));
    }
    let mut fs = FakeFs::new();
    fs.add_file(format!("{FOLDER}/evkmacro.txt"), text.into_bytes());
    let parsed = parse(&discover(&FakeRegistry::new(), &fs, [FOLDER]).unwrap());
    assert_eq!(parsed.macros().len(), ENTRIES);
    let dir = tempfile::tempdir().unwrap();
    let stores = Stores::open(dir.path());
    assert_eq!(
        commit(&parsed, &stores.view(), NOW).unwrap_err(),
        MigrateError::Store {
            document: DocumentKind::Macros,
            error: ConfigError::IoFailure,
        }
    );
    assert!(snapshot(dir.path()).is_empty());
}
