use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::TryRecvError;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use raumakey_config::{
    AppRulesDocument, AtomicStore, ConfigDocument, ConfigError, FileReplacer, IssueReason,
    MacroDocument, MigrationRecord, MigrationStatus, StdReplacer, StoreDocument,
};
use raumakey_model::macros::{MacroEntry, MacroId};
use raumakey_model::options::InputMethod;

fn std_replacer() -> Arc<dyn FileReplacer> {
    Arc::new(StdReplacer)
}

fn config_store(dir: &Path) -> AtomicStore<ConfigDocument> {
    AtomicStore::new(dir, std_replacer())
}

fn temp_files(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.contains(".tmp-"))
        .collect()
}

fn doc_with_method(method: InputMethod) -> ConfigDocument {
    let mut doc = ConfigDocument::defaults();
    doc.input.method = method;
    doc
}

/// Renames like `StdReplacer`, then damages the target, as a torn or lying write would.
struct CorruptingReplacer {
    remaining_corruptions: AtomicUsize,
    calls: AtomicUsize,
}

impl CorruptingReplacer {
    fn new(corruptions: usize) -> Self {
        Self {
            remaining_corruptions: AtomicUsize::new(corruptions),
            calls: AtomicUsize::new(0),
        }
    }
}

impl FileReplacer for CorruptingReplacer {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        fs::rename(replacement, target)?;
        let corrupt = self
            .remaining_corruptions
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
            .is_ok();
        if corrupt {
            let mut file = fs::OpenOptions::new().append(true).open(target)?;
            file.write_all(b"\n[[[ torn")?;
        }
        Ok(())
    }
}

/// Fails before touching anything.
struct FailingReplacer(io::ErrorKind);

impl FileReplacer for FailingReplacer {
    fn replace(&self, _replacement: &Path, _target: &Path) -> io::Result<()> {
        Err(io::Error::from(self.0))
    }
}

#[test]
fn store_paths_follow_the_document_file_name() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    assert_eq!(store.path(), dir.path().join("raumakey.toml"));
    assert_eq!(store.lock_path(), dir.path().join("raumakey.toml.lock"));
    let macros: AtomicStore<MacroDocument> = AtomicStore::new(dir.path(), std_replacer());
    assert_eq!(macros.path(), dir.path().join("raumakey-macros.toml"));
}

#[test]
fn a_missing_file_is_not_found_and_defaults_on_request() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    assert_eq!(store.load().unwrap_err(), ConfigError::NotFound);
    let loaded = store.load_or_default().unwrap();
    assert!(loaded.is_clean());
    assert_eq!(loaded.document, ConfigDocument::defaults());
    assert_eq!(store.poll_revision().unwrap(), 0);
}

#[test]
fn first_commit_expects_revision_zero_and_yields_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let doc = doc_with_method(InputMethod::Vni);
    assert_eq!(store.commit(0, &doc).unwrap(), 1);
    let loaded = store.load().unwrap();
    assert!(loaded.is_clean());
    let mut expected = doc.clone();
    expected.revision = 1;
    assert_eq!(loaded.document, expected);
    assert_eq!(
        fs::read_to_string(store.path()).unwrap(),
        expected.encode().unwrap()
    );
    assert_eq!(store.poll_revision().unwrap(), 1);
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn commit_ignores_the_document_revision_and_increments_the_stored_one() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let mut doc = ConfigDocument::defaults();
    doc.revision = 42;
    assert_eq!(store.commit(0, &doc).unwrap(), 1);
    assert_eq!(store.commit(1, &doc).unwrap(), 2);
    assert_eq!(store.commit(2, &doc).unwrap(), 3);
    assert_eq!(store.load().unwrap().document.revision, 3);
}

#[test]
fn a_stale_expected_revision_conflicts_without_writing() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    store.commit(0, &doc_with_method(InputMethod::Vni)).unwrap();
    let before = fs::read(store.path()).unwrap();
    assert_eq!(
        store.commit(0, &doc_with_method(InputMethod::Viqr)),
        Err(ConfigError::Conflict {
            expected: 0,
            actual: 1
        })
    );
    assert_eq!(
        store.commit(5, &doc_with_method(InputMethod::Viqr)),
        Err(ConfigError::Conflict {
            expected: 5,
            actual: 1
        })
    );
    assert_eq!(fs::read(store.path()).unwrap(), before);
    assert!(temp_files(dir.path()).is_empty());
}

/// Finding (design §8): a hand edit that keeps `revision` is another writer too. A commit
/// based on what this store loaded fails with `Conflict` instead of overwriting the edit;
/// once the store loads the edited file, the change goes through on top of it.
#[test]
fn a_hand_edit_that_keeps_the_revision_is_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    store.commit(0, &doc_with_method(InputMethod::Vni)).unwrap();
    let loaded = store.load().unwrap().document;
    assert_eq!(loaded.revision, 1);
    let edited = fs::read_to_string(store.path())
        .unwrap()
        .replace("method = \"vni\"", "method = \"viqr\"");
    fs::write(store.path(), &edited).unwrap();

    let mut change = loaded;
    change.input.spell_check = false;
    assert_eq!(
        store.commit(1, &change),
        Err(ConfigError::Conflict {
            expected: 1,
            actual: 1
        })
    );
    assert_eq!(fs::read_to_string(store.path()).unwrap(), edited);
    assert!(temp_files(dir.path()).is_empty());

    let mut change = store.load().unwrap().document;
    assert_eq!(change.input.method, InputMethod::Viqr);
    change.input.spell_check = false;
    assert_eq!(store.commit(1, &change), Ok(2));
    let result = store.load().unwrap().document;
    assert_eq!(result.input.method, InputMethod::Viqr);
    assert!(!result.input.spell_check);
}

/// The same holds for an edit right after this store's own commit, and for a file created by
/// hand after a load found none (a file without `revision` reads as revision 0).
#[test]
fn edits_after_a_commit_or_after_a_missing_file_are_not_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    assert_eq!(store.commit(0, &ConfigDocument::defaults()), Ok(1));
    let edited = fs::read_to_string(store.path())
        .unwrap()
        .replace("diagnostics = false", "diagnostics = true");
    fs::write(store.path(), &edited).unwrap();
    assert_eq!(
        store.commit(1, &ConfigDocument::defaults()),
        Err(ConfigError::Conflict {
            expected: 1,
            actual: 1
        })
    );
    assert_eq!(fs::read_to_string(store.path()).unwrap(), edited);

    let other = config_store(&dir.path().join("other"));
    let defaults = other.load_or_default().unwrap().document;
    fs::create_dir_all(dir.path().join("other")).unwrap();
    let by_hand = "version = 1\n[input]\nmethod = \"vni\"\n";
    fs::write(other.path(), by_hand).unwrap();
    assert_eq!(
        other.commit(0, &defaults),
        Err(ConfigError::Conflict {
            expected: 0,
            actual: 0
        })
    );
    assert_eq!(fs::read_to_string(other.path()).unwrap(), by_hand);
    assert_eq!(
        other.load().unwrap().document.input.method,
        InputMethod::Vni
    );
    assert_eq!(other.commit(0, &defaults), Ok(1));
}

/// Finding: `poll_revision` cannot see an edit that keeps the revision. `poll_changed`
/// compares the file with what this store last loaded or committed, so a controller notices
/// such an edit and reloads.
#[test]
fn poll_changed_notices_edits_that_keep_the_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    assert_eq!(store.poll_changed(), Ok(true), "nothing loaded yet");
    assert!(store.load_or_default().unwrap().is_clean());
    assert_eq!(store.poll_changed(), Ok(false), "still missing");
    assert_eq!(store.commit(0, &ConfigDocument::defaults()), Ok(1));
    assert_eq!(store.poll_changed(), Ok(false), "its own commit");
    let watcher = config_store(dir.path());
    assert_eq!(watcher.poll_changed(), Ok(true));
    watcher.load().unwrap();
    assert_eq!(watcher.poll_changed(), Ok(false));

    let edited = fs::read_to_string(store.path())
        .unwrap()
        .replace("diagnostics = false", "diagnostics = true");
    fs::write(store.path(), &edited).unwrap();
    assert_eq!(store.poll_revision(), Ok(1), "the revision did not move");
    assert_eq!(store.poll_changed(), Ok(true));
    assert_eq!(watcher.poll_changed(), Ok(true));
    assert!(store.load().unwrap().document.system.diagnostics);
    assert_eq!(store.poll_changed(), Ok(false));
    assert_eq!(
        watcher.poll_changed(),
        Ok(true),
        "each store compares with what it saw"
    );

    // Writing the same bytes again is not a change.
    fs::write(store.path(), &edited).unwrap();
    assert_eq!(store.poll_changed(), Ok(false));
    assert_eq!(store.poll_changed(), Ok(false));

    // A commit through another store, a damaged file, and a deleted file are changes.
    watcher.load().unwrap();
    assert_eq!(store.commit(1, &ConfigDocument::defaults()), Ok(2));
    assert_eq!(watcher.poll_changed(), Ok(true));
    let corrupt = "version = 1\nrevision = = 2\n";
    fs::write(store.path(), corrupt).unwrap();
    assert_eq!(store.poll_changed(), Ok(true));
    assert_eq!(store.load().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(
        store.poll_changed(),
        Ok(false),
        "a damaged file that was read is not reported again"
    );
    fs::remove_file(store.path()).unwrap();
    assert_eq!(store.poll_changed(), Ok(true));
    assert_eq!(store.load().unwrap_err(), ConfigError::NotFound);
    assert_eq!(store.poll_changed(), Ok(false));
}

/// A conflict found from the file's content names the same revision twice; its text says
/// what happened instead of "expected 1, found 1".
#[test]
fn conflicts_describe_themselves() {
    assert_eq!(
        ConfigError::Conflict {
            expected: 1,
            actual: 2
        }
        .to_string(),
        "configuration revision conflict (expected 1, found 2)"
    );
    assert_eq!(
        ConfigError::Conflict {
            expected: 3,
            actual: 3
        }
        .to_string(),
        "configuration changed on disk without a new revision (revision 3)"
    );
}

fn race_round(
    stores: [Arc<AtomicStore<ConfigDocument>>; 2],
    expected: u64,
) -> [Result<u64, ConfigError>; 2] {
    let barrier = Arc::new(Barrier::new(2));
    let methods = [InputMethod::Vni, InputMethod::Viqr];
    let handles: Vec<_> = stores
        .into_iter()
        .zip(methods)
        .map(|(store, method)| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                store.commit(expected, &doc_with_method(method))
            })
        })
        .collect();
    let mut results = handles.into_iter().map(|h| h.join().unwrap());
    [results.next().unwrap(), results.next().unwrap()]
}

fn assert_exactly_one_wins(results: [Result<u64, ConfigError>; 2], expected: u64) -> usize {
    let winners: Vec<usize> = (0..2).filter(|&i| results[i].is_ok()).collect();
    assert_eq!(winners.len(), 1, "{results:?}");
    let winner = winners[0];
    assert_eq!(results[winner], Ok(expected + 1));
    assert_eq!(
        results[1 - winner],
        Err(ConfigError::Conflict {
            expected,
            actual: expected + 1
        })
    );
    winner
}

/// Matrix #46: two writers with the same expected revision on one shared store.
#[test]
fn matrix_46_concurrent_commits_on_a_shared_store_have_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(config_store(dir.path()));
    for round in 0..60u64 {
        let results = race_round([Arc::clone(&store), Arc::clone(&store)], round);
        let winner = assert_exactly_one_wins(results, round);
        let on_disk = store.load().unwrap().document;
        assert_eq!(on_disk.revision, round + 1);
        let expected_method = [InputMethod::Vni, InputMethod::Viqr][winner];
        assert_eq!(on_disk.input.method, expected_method);
    }
    assert!(temp_files(dir.path()).is_empty());
}

/// Matrix #46: the same race between two independent store instances (as two processes).
#[test]
fn matrix_46_concurrent_commits_across_store_instances_have_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let first = Arc::new(config_store(dir.path()));
    let second = Arc::new(config_store(dir.path()));
    for round in 0..60u64 {
        let results = race_round([Arc::clone(&first), Arc::clone(&second)], round);
        assert_exactly_one_wins(results, round);
    }
    assert_eq!(first.poll_revision().unwrap(), 60);
    assert_eq!(second.poll_revision().unwrap(), 60);
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn a_syntax_corrupt_file_is_reported_and_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let corrupt = b"version = 1\nrevision = = 3\n".to_vec();
    fs::write(store.path(), &corrupt).unwrap();
    assert_eq!(store.load().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(store.load_or_default().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(store.poll_revision().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(
        store.commit(0, &ConfigDocument::defaults()).unwrap_err(),
        ConfigError::Corrupt
    );
    assert_eq!(fs::read(store.path()).unwrap(), corrupt);
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn a_file_that_is_not_utf8_is_corrupt() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    fs::write(store.path(), [0xff, 0xfe, b'v', 0x00]).unwrap();
    assert_eq!(store.load().unwrap_err(), ConfigError::Corrupt);
}

#[test]
fn a_newer_version_is_unsupported_and_never_overwritten() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let newer = b"version = 2\nrevision = 5\n".to_vec();
    fs::write(store.path(), &newer).unwrap();
    assert_eq!(store.load().unwrap_err(), ConfigError::UnsupportedVersion);
    assert_eq!(
        store.commit(5, &ConfigDocument::defaults()).unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    assert_eq!(fs::read(store.path()).unwrap(), newer);
}

/// Coverage gap: `load_or_default` turns only a missing file into defaults. A file of another
/// version, or one that cannot be read, is reported instead, so a caller never takes it for
/// "no configuration yet" (and a commit never overwrites it).
#[test]
fn load_or_default_defaults_only_a_missing_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let newer = "version = 2\nrevision = 5\n";
    fs::write(store.path(), newer).unwrap();
    assert_eq!(
        store.load_or_default().unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    assert_eq!(fs::read_to_string(store.path()).unwrap(), newer);

    // A folder in place of the file cannot be read as one.
    let unreadable = config_store(&dir.path().join("sub"));
    fs::create_dir_all(unreadable.path()).unwrap();
    let expected = if cfg!(windows) {
        ConfigError::PermissionDenied
    } else {
        ConfigError::IoFailure
    };
    assert_eq!(unreadable.load_or_default().unwrap_err(), expected);
    assert_eq!(unreadable.load().unwrap_err(), expected);
    assert_eq!(
        unreadable.commit(0, &ConfigDocument::defaults()),
        Err(expected)
    );
    assert!(unreadable.path().is_dir());
}

#[test]
fn field_issues_reach_the_caller_and_a_commit_repairs_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    fs::write(
        store.path(),
        "version = 1\nrevision = 3\n[input]\nmethod = \"vni\"\ncontext_limit = 1\n",
    )
    .unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.issues.len(), 1);
    assert_eq!(loaded.issues[0].path, "input.context_limit");
    assert_eq!(loaded.issues[0].reason, IssueReason::OutOfRange);
    assert_eq!(loaded.document.input.method, InputMethod::Vni);
    assert_eq!(store.commit(3, &loaded.document).unwrap(), 4);
    let repaired = store.load().unwrap();
    assert!(repaired.is_clean());
    assert_eq!(repaired.document.input.context_limit, 32);
    assert_eq!(repaired.document.input.method, InputMethod::Vni);
}

#[test]
fn a_read_back_mismatch_fails_and_restores_the_previous_file() {
    let dir = tempfile::tempdir().unwrap();
    config_store(dir.path())
        .commit(0, &doc_with_method(InputMethod::Vni))
        .unwrap();
    let before = fs::read(dir.path().join("raumakey.toml")).unwrap();
    let replacer = Arc::new(CorruptingReplacer::new(1));
    let store: AtomicStore<ConfigDocument> = AtomicStore::new(dir.path(), replacer.clone());
    assert_eq!(
        store.commit(1, &doc_with_method(InputMethod::Viqr)),
        Err(ConfigError::IoFailure)
    );
    assert_eq!(replacer.calls.load(Ordering::SeqCst), 2, "write + restore");
    assert_eq!(fs::read(store.path()).unwrap(), before);
    assert_eq!(store.load().unwrap().document.revision, 1);
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn a_read_back_mismatch_on_the_first_commit_leaves_no_file() {
    let dir = tempfile::tempdir().unwrap();
    let store: AtomicStore<ConfigDocument> =
        AtomicStore::new(dir.path(), Arc::new(CorruptingReplacer::new(usize::MAX)));
    assert_eq!(
        store.commit(0, &ConfigDocument::defaults()),
        Err(ConfigError::IoFailure)
    );
    assert_eq!(store.load().unwrap_err(), ConfigError::NotFound);
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn a_failed_replace_reports_the_error_and_removes_the_temp_file() {
    let dir = tempfile::tempdir().unwrap();
    config_store(dir.path())
        .commit(0, &ConfigDocument::defaults())
        .unwrap();
    let before = fs::read(dir.path().join("raumakey.toml")).unwrap();
    for (kind, error) in [
        (
            io::ErrorKind::PermissionDenied,
            ConfigError::PermissionDenied,
        ),
        (io::ErrorKind::Other, ConfigError::IoFailure),
    ] {
        let store: AtomicStore<ConfigDocument> =
            AtomicStore::new(dir.path(), Arc::new(FailingReplacer(kind)));
        assert_eq!(store.commit(1, &ConfigDocument::defaults()), Err(error));
        assert!(temp_files(dir.path()).is_empty());
        assert_eq!(fs::read(store.path()).unwrap(), before);
    }
}

#[test]
fn an_unencodable_document_is_refused_before_anything_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let store: AtomicStore<MacroDocument> = AtomicStore::new(dir.path(), std_replacer());
    let mut doc = MacroDocument::default();
    doc.entries.push(MacroEntry {
        id: MacroId(u64::MAX),
        trigger: "t".into(),
        replacement: "r".into(),
        enabled: true,
        boundary: Default::default(),
        case_policy: Default::default(),
    });
    assert_eq!(store.commit(0, &doc), Err(ConfigError::Corrupt));
    let foreign = MacroDocument {
        version: 7,
        ..MacroDocument::default()
    };
    assert_eq!(
        store.commit(0, &foreign),
        Err(ConfigError::UnsupportedVersion)
    );
    assert_eq!(store.load().unwrap_err(), ConfigError::NotFound);
    assert!(temp_files(dir.path()).is_empty());
}

#[test]
fn stale_temp_files_of_a_crashed_writer_are_removed_by_the_next_commit() {
    let dir = tempfile::tempdir().unwrap();
    let stale = dir.path().join("raumakey.toml.tmp-4242-7");
    fs::write(&stale, "half written").unwrap();
    let unrelated = dir.path().join("raumakey-macros.toml.tmp-4242-7");
    fs::write(&unrelated, "other store").unwrap();
    config_store(dir.path())
        .commit(0, &ConfigDocument::defaults())
        .unwrap();
    assert!(!stale.exists());
    assert!(unrelated.exists());
}

#[test]
fn subscribers_hear_each_committed_revision() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let first = store.subscribe();
    let dropped = store.subscribe();
    drop(dropped);
    store.commit(0, &ConfigDocument::defaults()).unwrap();
    let second = store.subscribe();
    store.commit(1, &ConfigDocument::defaults()).unwrap();
    assert!(store.commit(0, &ConfigDocument::defaults()).is_err());
    assert_eq!(first.try_recv(), Ok(1));
    assert_eq!(first.try_recv(), Ok(2));
    assert_eq!(first.try_recv(), Err(TryRecvError::Empty));
    assert_eq!(second.try_recv(), Ok(2));
    assert_eq!(second.try_recv(), Err(TryRecvError::Empty));
}

#[test]
fn poll_revision_sees_commits_made_through_another_store() {
    let dir = tempfile::tempdir().unwrap();
    let writer = config_store(dir.path());
    let watcher = config_store(dir.path());
    assert_eq!(watcher.poll_revision().unwrap(), 0);
    writer.commit(0, &ConfigDocument::defaults()).unwrap();
    assert_eq!(watcher.poll_revision().unwrap(), 1);
    writer.commit(1, &ConfigDocument::defaults()).unwrap();
    assert_eq!(watcher.poll_revision().unwrap(), 2);
}

#[test]
fn commit_creates_a_missing_directory() {
    let dir = tempfile::tempdir().unwrap();
    let nested = dir.path().join("a").join("b");
    let store = config_store(&nested);
    assert_eq!(store.commit(0, &ConfigDocument::defaults()).unwrap(), 1);
    assert!(nested.join("raumakey.toml").is_file());
}

#[test]
fn a_held_lock_makes_commit_wait_then_time_out() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path()).with_lock_timeout(Duration::from_millis(150));
    let holder = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(store.lock_path())
        .unwrap();
    holder.lock().unwrap();
    let started = Instant::now();
    assert_eq!(
        store.commit(0, &ConfigDocument::defaults()),
        Err(ConfigError::IoFailure)
    );
    assert!(started.elapsed() >= Duration::from_millis(150));
    // A reader that finds no file while the lock is held waits for the holder, which may be a
    // writer swapping the file in; it never reports "missing" (and so defaults) meanwhile.
    let started = Instant::now();
    assert_eq!(store.load().unwrap_err(), ConfigError::IoFailure);
    assert!(started.elapsed() >= Duration::from_millis(150));
    assert_eq!(store.poll_revision().unwrap_err(), ConfigError::IoFailure);

    let release = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        holder.unlock().unwrap();
    });
    let patient = config_store(dir.path()).with_lock_timeout(Duration::from_secs(10));
    assert_eq!(patient.load().unwrap_err(), ConfigError::NotFound);
    assert_eq!(patient.commit(0, &ConfigDocument::defaults()).unwrap(), 1);
    release.join().unwrap();
}

#[test]
fn set_aside_moves_a_corrupt_file_so_the_store_can_start_over() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    assert_eq!(store.set_aside().unwrap(), None);
    let corrupt = b"[input\n".to_vec();
    fs::write(store.path(), &corrupt).unwrap();
    let backup: PathBuf = store.set_aside().unwrap().unwrap();
    assert_eq!(backup.parent(), Some(dir.path()));
    let name = backup.file_name().unwrap().to_str().unwrap().to_owned();
    assert!(name.starts_with("raumakey.toml.bak-"), "{name}");
    assert_eq!(fs::read(&backup).unwrap(), corrupt);
    assert_eq!(store.load().unwrap_err(), ConfigError::NotFound);
    assert_eq!(store.commit(0, &ConfigDocument::defaults()).unwrap(), 1);

    fs::write(store.path(), &corrupt).unwrap();
    let second = store.set_aside().unwrap().unwrap();
    assert_ne!(second, backup);
    assert!(backup.exists() && second.exists());
}

#[test]
fn every_document_type_commits_and_loads() {
    let dir = tempfile::tempdir().unwrap();

    let macros: AtomicStore<MacroDocument> = AtomicStore::new(dir.path(), std_replacer());
    let mut macro_doc = MacroDocument::default();
    macro_doc.entries.push(MacroEntry {
        id: MacroId(1),
        trigger: "vn".into(),
        replacement: "Việt Nam".into(),
        enabled: true,
        boundary: Default::default(),
        case_policy: Default::default(),
    });
    assert_eq!(macros.commit(0, &macro_doc).unwrap(), 1);
    assert_eq!(macros.load().unwrap().document.entries, macro_doc.entries);

    let rules: AtomicStore<AppRulesDocument> = AtomicStore::new(dir.path(), std_replacer());
    let rules_doc = rules.load_or_default().unwrap().document;
    assert_eq!(rules.commit(0, &rules_doc).unwrap(), 1);
    assert_eq!(
        rules.load().unwrap().document.compatibility,
        rules_doc.compatibility
    );

    let migration: AtomicStore<MigrationRecord> = AtomicStore::new(dir.path(), std_replacer());
    let record = MigrationRecord {
        status: MigrationStatus::InProgress,
        ..MigrationRecord::default()
    };
    assert_eq!(migration.commit(0, &record).unwrap(), 1);
    let loaded = migration.load().unwrap().document;
    assert_eq!(loaded.status, MigrationStatus::InProgress);
    assert_eq!(loaded.revision, 1);
    assert!(dir.path().join("raumakey-migration.json").is_file());
}

/// The controller polls while a worker commits: readers must never make a commit fail, and a
/// reader must only ever observe complete revisions.
#[test]
fn concurrent_pollers_never_break_commits_or_see_partial_files() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(config_store(dir.path()));
    store.commit(0, &ConfigDocument::defaults()).unwrap();
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let pollers: Vec<_> = (0..3)
        .map(|_| {
            let watcher = config_store(dir.path());
            let done = Arc::clone(&done);
            thread::spawn(move || {
                let mut last = 0;
                let mut polls = 0u32;
                while !done.load(Ordering::SeqCst) {
                    let revision = watcher.poll_revision().unwrap();
                    assert!(revision >= last, "revision went backwards");
                    last = revision;
                    polls += 1;
                }
                polls
            })
        })
        .collect();
    for expected in 1..=40u64 {
        assert_eq!(
            store.commit(expected, &doc_with_method(InputMethod::Vni)),
            Ok(expected + 1)
        );
    }
    done.store(true, Ordering::SeqCst);
    for poller in pollers {
        assert!(poller.join().unwrap() > 0);
    }
    assert!(temp_files(dir.path()).is_empty());
}

/// Matrix #45 end to end: loading a hand-edited `raumakey-macros.toml` and committing it again
/// keeps every macro, including entries that could not be decoded and keys this build does not
/// know.
#[test]
fn a_routine_commit_never_erases_macros_this_build_cannot_read() {
    let dir = tempfile::tempdir().unwrap();
    let store: AtomicStore<MacroDocument> = AtomicStore::new(dir.path(), std_replacer());
    fs::write(
        store.path(),
        r#"version = 1
revision = 3
[[macro]]
id = 1
trigger = "kept"
replacement = "sentence boundary from a newer build"
boundary = "sentence"
[[macro]]
id = 2
trigger = "flag"
replacement = "enabled is not a boolean"
enabled = "yes"
[[macro]]
replacement = "an entry without id or trigger"
[[macro]]
id = 4
trigger = "noted"
replacement = "has a future key"
note = "future field"
"#,
    )
    .unwrap();
    let loaded = store.load().unwrap();
    assert!(!loaded.is_clean());
    assert_eq!(store.commit(3, &loaded.document).unwrap(), 4);
    let written = fs::read_to_string(store.path()).unwrap();
    for kept in [
        "trigger = \"kept\"",
        "trigger = \"flag\"",
        "an entry without id or trigger",
        "note = \"future field\"",
    ] {
        assert!(written.contains(kept), "{kept} missing from\n{written}");
    }
    let reloaded = store.load().unwrap().document;
    assert_eq!(reloaded.entries.len(), 3);

    // What is not kept (crate docs): a value this build does not accept in a known field takes
    // the field's default, and the commit writes that default.
    assert!(!written.contains("boundary = \"sentence\""), "{written}");
    assert!(!written.contains("enabled = \"yes\""), "{written}");
    let kept = reloaded
        .entries
        .iter()
        .find(|entry| entry.trigger == "kept")
        .unwrap();
    assert_eq!(kept.boundary, Default::default());
}
