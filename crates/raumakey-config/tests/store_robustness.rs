//! Store behavior beyond the happy path: the pinned commit sequence, cheap polling, size and
//! revision limits, and readers racing a non-atomic replace (`ReplaceFileW`).

use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use raumakey_config::{
    AtomicStore, ConfigDocument, ConfigError, FileReplacer, MacroDocument, MigrationRecord,
    StdReplacer, StoreDocument,
};
use raumakey_model::macros::{MacroEntry, MacroId};
use raumakey_model::options::InputMethod;

/// Size limit of a store file (plan §5 review: 16 MiB).
const LIMIT: u64 = 16 * 1024 * 1024;

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

/// Whether another handle could take the lock right now (it is released again at once).
fn lock_is_free(lock_path: &Path) -> bool {
    let probe = OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    match probe.try_lock() {
        Ok(()) => {
            probe.unlock().unwrap();
            true
        }
        Err(TryLockError::WouldBlock) => false,
        Err(TryLockError::Error(error)) => panic!("{error}"),
    }
}

/// What a replacer saw at the moment the store handed it the new file.
#[derive(Debug)]
struct Observation {
    lock_free: bool,
    replacement: PathBuf,
    target: PathBuf,
    replacement_text: String,
    target_before: Option<String>,
}

/// Records the state of the store at replace time, then renames like `StdReplacer`.
struct ProbingReplacer {
    lock_path: PathBuf,
    seen: Mutex<Vec<Observation>>,
}

impl FileReplacer for ProbingReplacer {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        let observation = Observation {
            lock_free: lock_is_free(&self.lock_path),
            replacement: replacement.to_owned(),
            target: target.to_owned(),
            replacement_text: fs::read_to_string(replacement)?,
            target_before: fs::read_to_string(target).ok(),
        };
        self.seen.lock().unwrap().push(observation);
        fs::rename(replacement, target)
    }
}

/// Plan §5, pinned: lock -> re-read -> revision + 1 -> temporary file in the same directory,
/// fully written -> replace (lock still held, target still the old file) -> verified ->
/// lock released.
#[test]
fn commit_follows_the_planned_sequence() {
    let dir = tempfile::tempdir().unwrap();
    let probe = Arc::new(ProbingReplacer {
        lock_path: dir.path().join("raumakey.toml.lock"),
        seen: Mutex::new(Vec::new()),
    });
    let store: AtomicStore<ConfigDocument> = AtomicStore::new(dir.path(), probe.clone());
    assert_eq!(
        store.commit(0, &doc_with_method(InputMethod::Vni)).unwrap(),
        1
    );
    let first_file = fs::read_to_string(store.path()).unwrap();
    assert_eq!(
        store
            .commit(1, &doc_with_method(InputMethod::Viqr))
            .unwrap(),
        2
    );
    let second_file = fs::read_to_string(store.path()).unwrap();

    let seen = probe.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    let prefix = format!("raumakey.toml.tmp-{}-", std::process::id());
    for (observation, (expected_revision, before)) in
        seen.iter().zip([(1, None), (2, Some(first_file.clone()))])
    {
        assert!(!observation.lock_free, "replace ran without the lock");
        assert_eq!(observation.target, store.path());
        assert_eq!(observation.replacement.parent(), Some(dir.path()));
        let name = observation
            .replacement
            .file_name()
            .unwrap()
            .to_str()
            .unwrap();
        assert!(name.starts_with(&prefix), "{name}");
        let staged = ConfigDocument::decode(&observation.replacement_text).unwrap();
        assert!(staged.is_clean());
        assert_eq!(staged.document.revision, expected_revision);
        assert_eq!(observation.target_before, before);
    }
    assert_eq!(seen[0].replacement_text, first_file);
    assert_eq!(seen[1].replacement_text, second_file);
    assert!(lock_is_free(store.lock_path()));
    assert!(temp_files(dir.path()).is_empty());
}

/// The conflict check reads the file after the lock is taken, so a writer that finished while
/// this one waited is never overwritten.
#[test]
fn commit_reads_the_revision_only_after_taking_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let store = Arc::new(config_store(dir.path()));
    let holder = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(store.lock_path())
        .unwrap();
    holder.lock().unwrap();
    let waiting = {
        let store = Arc::clone(&store);
        thread::spawn(move || store.commit(0, &doc_with_method(InputMethod::Vni)))
    };
    thread::sleep(Duration::from_millis(100));
    // Another writer commits revision 1 while the first one waits for the lock.
    let mut other = ConfigDocument::defaults();
    other.revision = 1;
    fs::write(store.path(), other.encode().unwrap()).unwrap();
    holder.unlock().unwrap();
    assert_eq!(
        waiting.join().unwrap(),
        Err(ConfigError::Conflict {
            expected: 0,
            actual: 1
        })
    );
    assert_eq!(store.load().unwrap().document, other);
}

/// Polling reads only the canonical header the encoder writes (`version` then `revision`), so
/// it stays cheap for a large file; the rest of the file is validated by `load` and `commit`.
#[test]
fn poll_revision_reads_only_the_canonical_header() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let mut doc = ConfigDocument::defaults();
    doc.revision = 41;
    let text = doc.encode().unwrap();
    assert!(text.starts_with("version = 1\nrevision = 41\n"), "{text}");
    assert_eq!(
        ConfigDocument::revision_from_head(text.as_bytes()),
        Some(41)
    );

    // A body that is never parsed: polling still answers from the header alone.
    fs::write(store.path(), format!("{text}[broken\n")).unwrap();
    assert_eq!(store.poll_revision(), Ok(41));
    assert_eq!(store.load().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(
        store.commit(41, &ConfigDocument::defaults()).unwrap_err(),
        ConfigError::Corrupt
    );

    // Hand-written files without the canonical header are decoded in full.
    fs::write(store.path(), "# comment first\nversion = 1\nrevision = 7\n").unwrap();
    assert_eq!(store.poll_revision(), Ok(7));
    fs::write(store.path(), "version = 1\r\nrevision = 8\r\n").unwrap();
    assert_eq!(store.poll_revision(), Ok(8));
    fs::write(store.path(), "# comment first\nrevision = = 7\n").unwrap();
    assert_eq!(store.poll_revision(), Err(ConfigError::Corrupt));
}

/// The header is trusted only when it is exactly what the encoder writes and decode would
/// read the same revision; anything else is left to a full decode.
#[test]
fn revision_from_head_recognises_only_canonical_headers() {
    let max = 0x7FFF_FFFF_FFFF_FFFE_u64;
    let accepted = [
        ("version = 1\nrevision = 0\n", 0),
        ("version = 1\nrevision = 12\n[input]\n", 12),
        (&format!("version = 1\nrevision = {max}\n") as &str, max),
    ];
    for (head, revision) in accepted {
        assert_eq!(
            ConfigDocument::revision_from_head(head.as_bytes()),
            Some(revision),
            "{head:?}"
        );
        assert_eq!(
            MacroDocument::revision_from_head(head.as_bytes()),
            Some(revision)
        );
    }
    for head in [
        "",
        "version = 1\nrevision = 12",
        "version = 1\nrevision = 012\n",
        "version = 1\nrevision = +12\n",
        "version = 1\nrevision = 1_2\n",
        "version = 1\nrevision = 12 # note\n",
        "version = 1\nrevision = 9223372036854775807\n",
        "version = 1\nrevision = 99999999999999999999\n",
        "version = 2\nrevision = 12\n",
        "version = 0\nrevision = 12\n",
        "version = 1\r\nrevision = 12\r\n",
        "\u{feff}version = 1\nrevision = 12\n",
        "revision = 12\nversion = 1\n",
        " version = 1\nrevision = 12\n",
    ] {
        assert_eq!(
            ConfigDocument::revision_from_head(head.as_bytes()),
            None,
            "{head:?}"
        );
    }
    // JSON allows a later duplicate key to win, so its header is never trusted.
    assert_eq!(
        MigrationRecord::revision_from_head(b"{\n  \"version\": 1,\n  \"revision\": 3,\n"),
        None
    );
}

/// A file above the size limit is not read into memory and parsed; it is corrupt for every
/// operation and never overwritten.
#[test]
fn an_oversized_file_is_corrupt_and_left_alone() {
    assert_eq!(AtomicStore::<ConfigDocument>::MAX_FILE_BYTES, LIMIT);
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    let mut text = String::from("version = 1\nrevision = 3\n# ");
    let padding = usize::try_from(LIMIT).unwrap();
    text.push_str(&"x".repeat(padding));
    text.push('\n');
    assert!(
        ConfigDocument::decode(&text).is_ok(),
        "valid TOML on its own"
    );
    fs::write(store.path(), &text).unwrap();
    assert_eq!(store.load().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(store.load_or_default().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(store.poll_revision().unwrap_err(), ConfigError::Corrupt);
    assert_eq!(
        store.commit(3, &ConfigDocument::defaults()).unwrap_err(),
        ConfigError::Corrupt
    );
    assert_eq!(
        fs::metadata(store.path()).unwrap().len(),
        u64::try_from(text.len()).unwrap()
    );
    assert!(temp_files(dir.path()).is_empty());
}

/// A document whose encoding would exceed the limit is refused before anything is written,
/// because the store could never load the result again.
#[test]
fn a_document_too_large_to_store_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let store: AtomicStore<MacroDocument> = AtomicStore::new(dir.path(), std_replacer());
    let mut doc = MacroDocument::default();
    let chunk = "ừ".repeat(1024 * 1024);
    for id in 0..6 {
        doc.entries.push(MacroEntry {
            id: MacroId(id),
            trigger: format!("t{id}"),
            replacement: chunk.clone(),
            enabled: true,
            boundary: Default::default(),
            case_policy: Default::default(),
        });
    }
    assert!(u64::try_from(doc.encode().unwrap().len()).unwrap() > LIMIT);
    assert_eq!(store.commit(0, &doc), Err(ConfigError::IoFailure));
    assert_eq!(store.load().unwrap_err(), ConfigError::NotFound);
    assert!(temp_files(dir.path()).is_empty());
}

/// Matrix #46 never wedges: a hand-edited revision that could not be incremented loads as 0
/// (reported), so the next commit succeeds instead of failing forever.
#[test]
fn an_exhausted_revision_restarts_instead_of_wedging_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let store = config_store(dir.path());
    fs::write(
        store.path(),
        "version = 1\nrevision = 9223372036854775807\n",
    )
    .unwrap();
    let loaded = store.load().unwrap();
    assert_eq!(loaded.document.revision, 0);
    assert_eq!(
        loaded
            .issues
            .iter()
            .map(|issue| (issue.path.as_str(), issue.reason.code()))
            .collect::<Vec<_>>(),
        [("revision", "out-of-range")]
    );
    assert_eq!(store.poll_revision(), Ok(0));
    assert_eq!(store.commit(0, &loaded.document), Ok(1));

    // The largest revision a TOML file can carry is still reached by a commit, once.
    fs::write(
        store.path(),
        "version = 1\nrevision = 9223372036854775806\n",
    )
    .unwrap();
    assert_eq!(store.poll_revision(), Ok(9_223_372_036_854_775_806));
    assert_eq!(
        store.commit(9_223_372_036_854_775_806, &ConfigDocument::defaults()),
        Ok(9_223_372_036_854_775_807)
    );
    assert_eq!(store.poll_revision(), Ok(0));
    assert_eq!(store.commit(0, &ConfigDocument::defaults()), Ok(1));

    let json: AtomicStore<MigrationRecord> = AtomicStore::new(dir.path(), std_replacer());
    fs::write(
        json.path(),
        "{\"version\": 1, \"revision\": 18446744073709551615}",
    )
    .unwrap();
    assert_eq!(json.load().unwrap().document.revision, 0);
    assert_eq!(json.commit(0, &MigrationRecord::default()), Ok(1));
}

/// Replaces like `ReplaceFileW` can look to a concurrent reader: the target briefly does not
/// exist. Tells the test when the gap is open.
struct GappyReplacer {
    gap: Mutex<Option<Sender<()>>>,
    gap_length: Duration,
}

impl FileReplacer for GappyReplacer {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        match fs::remove_file(target) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if let Some(gap) = self.gap.lock().unwrap().take() {
            let _ = gap.send(());
        }
        thread::sleep(self.gap_length);
        fs::rename(replacement, target)
    }
}

/// Coverage gap: a reader that arrives while a writer swaps the file in waits for the writer
/// instead of seeing a missing file (which `load_or_default` would turn into defaults).
#[test]
fn readers_never_see_the_gap_of_a_non_atomic_replace() {
    for read in ["load", "load_or_default", "poll_revision"] {
        let dir = tempfile::tempdir().unwrap();
        config_store(dir.path())
            .commit(0, &doc_with_method(InputMethod::Vni))
            .unwrap();
        let (gap_open, gap) = mpsc::channel();
        let writer: Arc<AtomicStore<ConfigDocument>> = Arc::new(AtomicStore::new(
            dir.path(),
            Arc::new(GappyReplacer {
                gap: Mutex::new(Some(gap_open)),
                gap_length: Duration::from_millis(300),
            }),
        ));
        let committing = {
            let writer = Arc::clone(&writer);
            thread::spawn(move || writer.commit(1, &doc_with_method(InputMethod::Viqr)))
        };
        gap.recv().unwrap();
        let reader = config_store(dir.path());
        let revision = match read {
            "load" => reader.load().map(|loaded| loaded.document.revision),
            "load_or_default" => reader
                .load_or_default()
                .map(|loaded| loaded.document.revision),
            _ => reader.poll_revision(),
        };
        assert_eq!(revision, Ok(2), "{read}");
        assert_eq!(committing.join().unwrap(), Ok(2));
    }
}

/// Like `GappyReplacer`, but the target exists and cannot be opened (another process holds it
/// without sharing), as `ReplaceFileW` can leave it for a moment.
#[cfg(windows)]
struct LockedTargetReplacer {
    gap: Mutex<Option<Sender<()>>>,
}

#[cfg(windows)]
impl FileReplacer for LockedTargetReplacer {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        use std::os::windows::fs::OpenOptionsExt;
        let exclusive: File = OpenOptions::new().read(true).share_mode(0).open(target)?;
        if let Some(gap) = self.gap.lock().unwrap().take() {
            let _ = gap.send(());
        }
        thread::sleep(Duration::from_millis(300));
        drop(exclusive);
        fs::rename(replacement, target)
    }
}

#[cfg(windows)]
#[test]
fn readers_wait_out_a_target_that_is_briefly_unopenable() {
    let dir = tempfile::tempdir().unwrap();
    config_store(dir.path())
        .commit(0, &doc_with_method(InputMethod::Vni))
        .unwrap();
    let (gap_open, gap) = mpsc::channel();
    let writer: Arc<AtomicStore<ConfigDocument>> = Arc::new(AtomicStore::new(
        dir.path(),
        Arc::new(LockedTargetReplacer {
            gap: Mutex::new(Some(gap_open)),
        }),
    ));
    let committing = {
        let writer = Arc::clone(&writer);
        thread::spawn(move || writer.commit(1, &doc_with_method(InputMethod::Viqr)))
    };
    gap.recv().unwrap();
    let reader = config_store(dir.path());
    assert_eq!(reader.load().map(|loaded| loaded.document.revision), Ok(2));
    assert_eq!(committing.join().unwrap(), Ok(2));
}
