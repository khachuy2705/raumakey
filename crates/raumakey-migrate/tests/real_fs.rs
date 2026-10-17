//! A run against real files in a temp directory: sources are read and never changed.

mod common;

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::{MACRO_TEXT, SETTING_INI, SETTING_TREE, evkey_registry};
use raumakey_config::{
    AppRulesDocument, AtomicStore, ConfigDocument, FileReplacer, MacroDocument, MigrationRecord,
    MigrationStatus, StdReplacer, sha256_hex,
};
use raumakey_migrate::{
    MigrationOutcome, MigrationStores, SourceKind, StdSourceFs, commit, discover, parse,
};

const NOW: u64 = 1_700_000_000_000;

/// Name, bytes, length, modification time, and read-only flag of every entry, plus the
/// directory's own modification time.
type Snapshot = (
    BTreeMap<String, (Vec<u8>, u64, SystemTime, bool)>,
    SystemTime,
);

fn snapshot(dir: &Path) -> Snapshot {
    let entries = fs::read_dir(dir)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            let metadata = entry.metadata().unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                (
                    fs::read(entry.path()).unwrap(),
                    metadata.len(),
                    metadata.modified().unwrap(),
                    metadata.permissions().readonly(),
                ),
            )
        })
        .collect();
    (entries, fs::metadata(dir).unwrap().modified().unwrap())
}

fn set_readonly(path: &Path, readonly: bool) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_readonly(readonly);
    fs::set_permissions(path, permissions).unwrap();
}

/// Writes the fixture files with an old modification time and marks them read-only, so that
/// any write would either fail or show up as a new time.
fn write_sources(dir: &Path) {
    fs::create_dir(dir).unwrap();
    let old = UNIX_EPOCH + Duration::from_secs(978_307_200); // 2001-01-01
    for (name, bytes) in [
        ("setting.ini", SETTING_INI),
        ("setting.tree", SETTING_TREE),
        ("evkmacro.txt", MACRO_TEXT),
    ] {
        let path = dir.join(name);
        fs::write(&path, bytes).unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        set_readonly(&path, true);
    }
}

/// Clears the read-only flags on drop, so that the temp directory can be removed even when
/// the test fails.
struct Writable<'a>(&'a Path);

impl Drop for Writable<'_> {
    fn drop(&mut self) {
        if let Ok(entries) = fs::read_dir(self.0) {
            for entry in entries.flatten() {
                if let Ok(metadata) = entry.metadata() {
                    let mut permissions = metadata.permissions();
                    #[allow(clippy::permissions_set_readonly_false)]
                    permissions.set_readonly(false);
                    let _ = fs::set_permissions(entry.path(), permissions);
                }
            }
        }
    }
}

#[test]
fn a_real_file_system_import_leaves_the_sources_untouched() {
    let root = tempfile::tempdir().unwrap();
    let source_dir = root.path().join("EVKey");
    write_sources(&source_dir);
    let _writable = Writable(&source_dir);
    let before = snapshot(&source_dir);

    let data = root.path().join("data");
    let replacer: Arc<dyn FileReplacer> = Arc::new(StdReplacer);
    let config: AtomicStore<ConfigDocument> = AtomicStore::new(&data, Arc::clone(&replacer));
    let macros: AtomicStore<MacroDocument> = AtomicStore::new(&data, Arc::clone(&replacer));
    let app_rules: AtomicStore<AppRulesDocument> = AtomicStore::new(&data, Arc::clone(&replacer));
    let record: AtomicStore<MigrationRecord> = AtomicStore::new(&data, replacer);
    let stores = MigrationStores {
        config: &config,
        macros: &macros,
        app_rules: &app_rules,
        record: &record,
    };
    let registry = evkey_registry();
    let reader = StdSourceFs::new();
    let missing = root.path().join("not-here");

    let report = discover(
        &registry,
        &reader,
        [missing.as_path(), source_dir.as_path()],
    )
    .unwrap();
    assert_eq!(report.folder.as_deref(), Some(source_dir.as_path()));
    assert_eq!(report.sources.len(), 5);
    for source in &report.sources {
        if matches!(source.kind, SourceKind::Registry | SourceKind::TsfCache) {
            continue;
        }
        let bytes = fs::read(&source.location).unwrap();
        assert_eq!(source.sha256, sha256_hex(&bytes), "{:?}", source.kind);
        assert_eq!(source.size, u64::try_from(bytes.len()).unwrap());
    }

    let first = commit(&parse(&report), &stores, NOW).unwrap();
    assert_eq!(first.outcome, MigrationOutcome::Imported);
    assert_eq!(first.imported.macros, 3);
    assert_eq!(first.imported.named_sets, 2);
    assert_eq!(
        record.load().unwrap().document.status,
        MigrationStatus::Completed
    );

    let again = discover(&registry, &reader, [source_dir.as_path()]).unwrap();
    let second = commit(&parse(&again), &stores, NOW + 1).unwrap();
    assert_eq!(second.outcome, MigrationOutcome::NoChange);

    // Same names, bytes, sizes, times, and attributes; nothing was added to the folder.
    assert_eq!(snapshot(&source_dir), before);
    assert!(!data.starts_with(&source_dir));
}
