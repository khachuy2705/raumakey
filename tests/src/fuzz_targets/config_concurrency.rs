//! `config_concurrency`: commit schedules of several `AtomicStore` handles on one `config.toml`
//! (plan §5, design §8, matrix #46, and the matrix pass criteria's "concurrent config update").
//!
//! Each handle stands for another process or thread that writes the file. The target models
//! the file (its revision and bytes) and what every handle last loaded or committed, and checks
//! each result against the rules `AtomicStore` documents.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, PoisonError, RwLock};
use std::thread;

use raumakey_config::{
    AtomicStore, ConfigDocument, ConfigError, FileReplacer, StdReplacer, StoreDocument,
};
use raumakey_model::options::InputMethod;

use super::reader::Reader;

/// Most store handles.
const MAX_HANDLES: usize = 4;
/// Most operations per input.
const MAX_OPERATIONS: usize = 12;
/// Revision steps of a hand edit: the same revision (a text editor) or a higher one.
const HAND_EDIT_STEPS: [u64; 4] = [0, 0, 1, 5];
/// Prefix of the scratch folders under the system temporary folder.
const SCRATCH_PREFIX: &str = "raumakey-fuzz-config";
/// Attempts to find an unused scratch folder name.
const SCRATCH_ATTEMPTS: usize = 64;

/// A new folder under the system temporary folder, removed (best effort) when dropped.
struct Scratch {
    path: PathBuf,
}

impl Scratch {
    fn new() -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let base = std::env::temp_dir();
        for _ in 0..SCRATCH_ATTEMPTS {
            let number = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("{SCRATCH_PREFIX}-{}-{number}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::from(io::ErrorKind::AlreadyExists))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// One of 256 valid configurations, chosen by `variant`.
fn document(variant: u8) -> ConfigDocument {
    const METHODS: [InputMethod; 3] = [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr];
    let mut document = ConfigDocument::defaults();
    let input = &mut document.input;
    input.method = METHODS[usize::from(variant % 3)];
    input.modern_tone = variant & 4 != 0;
    input.spell_check = variant & 8 != 0;
    input.free_marking = variant & 16 != 0;
    // 8, 25, …, 127: inside the valid range 8..=128.
    input.context_limit = 8 + 17 * u32::from(variant >> 5);
    document
}

/// The revision a commit expects: mostly the current one, sometimes a stale, future, or
/// unrelated one.
fn expected_revision(reader: &mut Reader<'_>, actual: u64) -> u64 {
    match reader.below(8) {
        0..=3 => actual,
        4 => actual.wrapping_sub(1),
        5 => actual.wrapping_add(1),
        6 => 0,
        _ => u64::from(reader.byte()),
    }
}

/// The file as the model expects it.
#[derive(Default)]
struct Disk {
    /// Revision on disk; 0 without a file.
    revision: u64,
    /// File bytes; `None` without a file.
    bytes: Option<Vec<u8>>,
    /// The document the file holds; `None` without a file.
    document: Option<ConfigDocument>,
}

/// What a handle last loaded or committed: the basis on which `AtomicStore` refuses to
/// overwrite another writer's change that kept the revision (design §8).
#[derive(Clone)]
struct Seen {
    revision: u64,
    bytes: Option<Vec<u8>>,
}

/// A schedule run against one scratch folder.
struct Schedule<'a> {
    dir: &'a Path,
    stores: Vec<AtomicStore<ConfigDocument>>,
    seen: Vec<Option<Seen>>,
    disk: Disk,
}

impl<'a> Schedule<'a> {
    fn new(dir: &'a Path, handles: usize) -> Self {
        let mut schedule = Self {
            dir,
            stores: Vec::new(),
            seen: vec![None; handles],
            disk: Disk::default(),
        };
        schedule.stores = (0..handles).map(|_| schedule.store()).collect();
        schedule
    }

    /// A new handle on the file, as another process would open it.
    fn store(&self) -> AtomicStore<ConfigDocument> {
        let replacer: Arc<dyn FileReplacer> = Arc::new(StdReplacer);
        AtomicStore::new(self.dir, replacer)
    }

    fn path(&self) -> PathBuf {
        self.dir.join(<ConfigDocument as StoreDocument>::FILE_NAME)
    }

    /// The file's bytes; `None` when it does not exist.
    fn file_bytes(&self) -> Option<Vec<u8>> {
        match fs::read(self.path()) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => panic!("the store file is unreadable: {error}"),
        }
    }

    /// Whether `handle` last saw the revision on disk with other content: a commit based on it
    /// would overwrite another writer, so it must conflict (design §8).
    fn changed_since_seen(&self, handle: usize) -> bool {
        self.seen[handle].as_ref().is_some_and(|seen| {
            seen.revision == self.disk.revision && seen.bytes != self.disk.bytes
        })
    }

    /// A commit that must fail left the file as it was.
    fn assert_unchanged(&self) {
        assert!(
            self.file_bytes() == self.disk.bytes,
            "a failed commit changed the file"
        );
    }

    /// Records that `document` was committed as `revision`; returns what the writer saw.
    fn committed(&mut self, revision: u64, document: &ConfigDocument) -> Seen {
        let Some(bytes) = self.file_bytes() else {
            panic!("a successful commit left no file");
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            panic!("a commit wrote a file that is not UTF-8");
        };
        let mut expected = document.clone();
        expected.set_revision(revision);
        match ConfigDocument::decode(text) {
            Ok(loaded) => assert!(
                loaded.issues.is_empty() && loaded.document == expected,
                "the file does not hold the committed document"
            ),
            Err(error) => panic!("a committed file does not decode: {error:?}"),
        }
        self.disk = Disk {
            revision,
            bytes: Some(bytes.clone()),
            document: Some(expected),
        };
        Seen {
            revision,
            bytes: Some(bytes),
        }
    }

    fn load(&mut self, handle: usize) {
        let result = self.stores[handle].load();
        match (result, &self.disk.document) {
            (Err(ConfigError::NotFound), None) => {}
            (Ok(loaded), Some(expected)) => assert!(
                loaded.issues.is_empty() && loaded.document == *expected,
                "load returned another document than the file holds"
            ),
            (Ok(_), None) => panic!("load found a file that does not exist"),
            (Err(error), _) => panic!("load failed with {error:?}"),
        }
        self.seen[handle] = Some(Seen {
            revision: self.disk.revision,
            bytes: self.disk.bytes.clone(),
        });
    }

    fn poll(&self, handle: usize) {
        let polled = self.stores[handle].poll_revision();
        assert!(
            polled == Ok(self.disk.revision),
            "poll_revision differs from the revision on disk"
        );
    }

    fn commit(&mut self, handle: usize, expected: u64, document: &ConfigDocument) {
        let actual = self.disk.revision;
        let succeeds = expected == actual && !self.changed_since_seen(handle);
        let result = self.stores[handle].commit(expected, document);
        if succeeds {
            assert!(
                result == Ok(actual + 1),
                "a commit on the current revision failed: {result:?}"
            );
            let seen = self.committed(actual + 1, document);
            self.seen[handle] = Some(seen);
        } else {
            assert!(
                result == Err(ConfigError::Conflict { expected, actual }),
                "a commit that must conflict returned {result:?}"
            );
            self.assert_unchanged();
        }
    }

    /// A commit through a handle that never loaded the file: only the revision decides.
    fn fresh_commit(&mut self, expected: u64, document: &ConfigDocument) {
        let actual = self.disk.revision;
        let result = self.store().commit(expected, document);
        if expected == actual {
            assert!(
                result == Ok(actual + 1),
                "a new handle's commit on the current revision failed: {result:?}"
            );
            self.committed(actual + 1, document);
        } else {
            assert!(
                result == Err(ConfigError::Conflict { expected, actual }),
                "a new handle's commit that must conflict returned {result:?}"
            );
            self.assert_unchanged();
        }
    }

    /// Commits of several handles (a handle may race itself), all expecting one revision, on
    /// threads released together: matrix #46.
    fn race(&mut self, reader: &mut Reader<'_>) {
        let count = 2 + reader.below(MAX_HANDLES - 1);
        let racers: Vec<usize> = (0..count)
            .map(|_| reader.below(self.stores.len()))
            .collect();
        let expected = expected_revision(reader, self.disk.revision);
        let documents: Vec<ConfigDocument> =
            racers.iter().map(|_| document(reader.byte())).collect();
        let actual = self.disk.revision;
        let may_win: Vec<bool> = racers
            .iter()
            .map(|&handle| expected == actual && !self.changed_since_seen(handle))
            .collect();
        let results = race(&self.stores, &racers, expected, &documents);
        let winners: Vec<usize> = (0..count).filter(|&index| results[index].is_ok()).collect();
        if !may_win.contains(&true) {
            assert!(
                results
                    .iter()
                    .all(|result| *result == Err(ConfigError::Conflict { expected, actual })),
                "racing commits that must all conflict did not"
            );
            self.assert_unchanged();
            return;
        }
        assert!(
            winners.len() == 1,
            "{} racing commits on one revision succeeded",
            winners.len()
        );
        let winner = winners[0];
        assert!(
            may_win[winner] && results[winner] == Ok(actual + 1),
            "the winning commit had to conflict, or wrote another revision"
        );
        let after = Err(ConfigError::Conflict {
            expected,
            actual: actual + 1,
        });
        let before = Err(ConfigError::Conflict { expected, actual });
        for (index, result) in results.iter().enumerate() {
            if index != winner {
                assert!(
                    *result == after || (!may_win[index] && *result == before),
                    "a losing commit returned {result:?}"
                );
            }
        }
        let seen = self.committed(actual + 1, &documents[winner]);
        self.seen[racers[winner]] = Some(seen);
    }

    /// Another writer, such as a text editor, rewrites the file without the store.
    fn hand_edit(&mut self, step: u64, mut document: ConfigDocument) {
        let revision = self.disk.revision + step;
        document.set_revision(revision);
        let text = match document.encode() {
            Ok(text) => text,
            Err(error) => panic!("a valid configuration does not encode: {error:?}"),
        };
        if let Err(error) = fs::write(self.path(), &text) {
            panic!("the hand edit could not write the file: {error}");
        }
        self.disk = Disk {
            revision,
            bytes: Some(text.into_bytes()),
            document: Some(document),
        };
    }

    /// A new handle sees what the schedule left, and no temporary file outlived a commit.
    fn finish(&self) {
        let store = self.store();
        match (store.load(), &self.disk.document) {
            (Err(ConfigError::NotFound), None) => {}
            (Ok(loaded), Some(expected)) => assert!(
                loaded.issues.is_empty() && loaded.document == *expected,
                "a new handle loads another document than the schedule left"
            ),
            (result, _) => panic!(
                "a new handle cannot load what the schedule left: {:?}",
                result.err()
            ),
        }
        assert!(
            store.poll_revision() == Ok(self.disk.revision),
            "a new handle polls another revision than the schedule left"
        );
        let entries = match fs::read_dir(self.dir) {
            Ok(entries) => entries,
            Err(error) => panic!("the scratch folder is unreadable: {error}"),
        };
        for entry in entries.flatten() {
            assert!(
                !entry.file_name().to_string_lossy().contains(".tmp-"),
                "a temporary file outlived its commit"
            );
        }
    }
}

/// Runs the commits of `racers` on threads that start together and returns their results in
/// order. The threads wait on a lock that is released once all of them exist, so a thread that
/// cannot start never leaves the others waiting.
fn race(
    stores: &[AtomicStore<ConfigDocument>],
    racers: &[usize],
    expected: u64,
    documents: &[ConfigDocument],
) -> Vec<Result<u64, ConfigError>> {
    let gate = RwLock::new(());
    thread::scope(|scope| {
        let start = gate.write().unwrap_or_else(PoisonError::into_inner);
        let mut threads = Vec::with_capacity(racers.len());
        for (&racer, document) in racers.iter().zip(documents) {
            let store = &stores[racer];
            let gate = &gate;
            let spawned = thread::Builder::new().spawn_scoped(scope, move || {
                drop(gate.read().unwrap_or_else(PoisonError::into_inner));
                store.commit(expected, document)
            });
            match spawned {
                Ok(thread) => threads.push(thread),
                Err(error) => {
                    drop(start);
                    panic!("the target could not start a thread: {error}");
                }
            }
        }
        drop(start);
        threads
            .into_iter()
            .map(|thread| match thread.join() {
                Ok(result) => result,
                Err(_) => panic!("a racing commit panicked"),
            })
            .collect()
    })
}

pub(crate) fn run(data: &[u8]) {
    let scratch = match Scratch::new() {
        Ok(scratch) => scratch,
        Err(error) => panic!("the target needs a writable temporary folder: {error}"),
    };
    let mut reader = Reader::new(data);
    let handles = 1 + reader.below(MAX_HANDLES);
    let mut schedule = Schedule::new(&scratch.path, handles);
    for _ in 0..MAX_OPERATIONS {
        if reader.is_empty() {
            break;
        }
        match reader.below(8) {
            0 => schedule.load(reader.below(handles)),
            1 | 2 => {
                let handle = reader.below(handles);
                let expected = expected_revision(&mut reader, schedule.disk.revision);
                schedule.commit(handle, expected, &document(reader.byte()));
            }
            3 | 4 => schedule.race(&mut reader),
            5 => {
                let step = reader.pick(&HAND_EDIT_STEPS);
                schedule.hand_edit(step, document(reader.byte()));
            }
            6 => schedule.poll(reader.below(handles)),
            _ => {
                let expected = expected_revision(&mut reader, schedule.disk.revision);
                schedule.fresh_commit(expected, &document(reader.byte()));
            }
        }
    }
    schedule.finish();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> Scratch {
        match Scratch::new() {
            Ok(scratch) => scratch,
            Err(error) => panic!("temporary folder: {error}"),
        }
    }

    #[test]
    fn a_schedule_ends_where_the_model_says() {
        let scratch = scratch();
        let mut schedule = Schedule::new(&scratch.path, 2);
        schedule.commit(0, 0, &document(7));
        schedule.load(1);
        // Another writer keeps revision 1: both handles saw it with other content.
        schedule.hand_edit(0, document(9));
        schedule.commit(1, 1, &document(10));
        schedule.commit(0, 1, &document(11));
        assert_eq!(schedule.disk.revision, 1);
        // A handle that never loaded relies on the revision alone.
        schedule.fresh_commit(1, &document(12));
        schedule.commit(1, 2, &document(13));
        assert_eq!(schedule.disk.revision, 3);
        schedule.poll(0);
        schedule.finish();
    }

    /// The model is not vacuous: a store that overwrote another writer's change would be
    /// caught. Here the model believes that handle 0 saw other content than the file holds.
    #[test]
    #[should_panic(expected = "a commit that must conflict returned Ok(2)")]
    fn a_commit_over_another_writers_change_is_caught() {
        let scratch = scratch();
        let mut schedule = Schedule::new(&scratch.path, 1);
        schedule.commit(0, 0, &document(1));
        schedule.seen[0] = Some(Seen {
            revision: 1,
            bytes: Some(b"another writer".to_vec()),
        });
        schedule.commit(0, 1, &document(2));
    }
}
