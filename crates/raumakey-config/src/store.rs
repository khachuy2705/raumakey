//! Atomic file storage with optimistic revisions (design §8, plan §5).

use core::fmt;
use core::fmt::Write as _;
use core::marker::PhantomData;
use std::fs::{self, File, Metadata, OpenOptions, TryLockError};
use std::io::{self, Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::document::{Loaded, StoreDocument};
use crate::error::ConfigError;

/// Lowercase hexadecimal SHA-256 of `data`.
pub fn sha256_hex(data: &[u8]) -> String {
    let digest = Sha256::digest(data);
    let mut text = String::with_capacity(64);
    for byte in digest.iter() {
        let _ = write!(text, "{byte:02x}");
    }
    text
}

/// SHA-256 of `data`.
fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Replaces a store file with a fully written temporary file.
///
/// The Windows adapter implements this with `ReplaceFileW`; [`StdReplacer`] uses
/// [`std::fs::rename`]. Implementations must not panic. A replace does not have to look atomic
/// to readers: [`AtomicStore`] readers that find the target missing or unopenable wait for the
/// writer's lock and read again.
pub trait FileReplacer: Send + Sync {
    /// Replaces `target` with `replacement`.
    ///
    /// Both paths are in the same directory and `replacement` is closed and flushed. `target`
    /// may not exist yet (first commit). On success `replacement` no longer exists.
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()>;
}

/// [`FileReplacer`] built on [`std::fs::rename`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StdReplacer;

impl FileReplacer for StdReplacer {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        fs::rename(replacement, target)
    }
}

/// Distinguishes temporary files written by this process.
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Longest pause between two attempts to take the store lock.
const MAX_LOCK_PAUSE: Duration = Duration::from_millis(25);

/// Size limit of a store file; see [`AtomicStore::MAX_FILE_BYTES`].
const FILE_LIMIT: u64 = 16 * 1024 * 1024;

/// Bytes [`AtomicStore::poll_revision`] reads before it falls back to a full decode; enough for
/// the canonical header of a TOML document (`version = 1\nrevision = <19 digits>\n`).
const HEAD_BYTES: u64 = 64;

/// Holds a lock on `<file>.lock` and releases it explicitly on drop.
struct LockGuard {
    file: File,
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Size and modification time of a store file. While both are unchanged,
/// [`AtomicStore::poll_changed`] does not read the file again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
}

impl Stamp {
    fn of(metadata: &Metadata) -> Option<Self> {
        Some(Self {
            len: metadata.len(),
            modified: metadata.modified().ok()?,
        })
    }

    /// `stamp`, when it describes a file of `bytes` (a file that changed while it was read
    /// gets no stamp, so that it is read again next time).
    fn matching(stamp: Option<Self>, bytes: &[u8]) -> Option<Self> {
        stamp.filter(|stamp| u64::try_from(bytes.len()).is_ok_and(|len| len == stamp.len))
    }
}

/// The content of the store file as a store instance last saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Content {
    /// There was no file.
    Missing,
    /// The file held bytes with this SHA-256.
    Bytes([u8; 32]),
    /// The file was larger than [`AtomicStore::MAX_FILE_BYTES`] and was not read.
    Oversized,
}

impl Content {
    /// Whether `current` (`None`: no file) is this content.
    fn is(self, current: Option<&[u8]>) -> bool {
        match (self, current) {
            (Self::Missing, None) => true,
            (Self::Bytes(digest), Some(bytes)) => sha256(bytes) == digest,
            _ => false,
        }
    }
}

/// What a store instance last loaded or committed, so that it notices a writer that changed
/// the file without a new revision (design §8), such as a hand edit.
#[derive(Clone, Copy, Debug)]
struct Seen {
    /// Revision of the file; `None` when it could not be decoded.
    revision: Option<u64>,
    content: Content,
    stamp: Option<Stamp>,
}

impl Seen {
    const MISSING: Self = Self {
        revision: Some(0),
        content: Content::Missing,
        stamp: None,
    };
}

/// A whole store file, read through one handle.
struct Snapshot {
    bytes: Vec<u8>,
    stamp: Option<Stamp>,
}

/// One document file with atomic, revision-checked commits.
///
/// A commit takes an exclusive lock on `<file>.lock`, re-reads the on-disk revision, fails
/// with [`ConfigError::Conflict`] if it differs from the writer's expectation (matrix #46),
/// writes revision + 1 to a temporary file in the same directory, flushes it, swaps it in with
/// the injected [`FileReplacer`], and verifies the result by reading it back and comparing
/// SHA-256 checksums. A missing file has revision 0.
///
/// The files are meant to be edited by hand too, and an editor rarely increases `revision`.
/// So each store instance also remembers what it last loaded or committed: a commit based on
/// that revision fails with [`ConfigError::Conflict`] when the file's content changed since
/// (design §8: never overwrite another writer silently), and [`AtomicStore::poll_changed`]
/// reports such a change, which [`AtomicStore::poll_revision`] cannot see. A store that has
/// not loaded the file relies on the revision alone.
///
/// Readers take no lock. When a read finds the file missing or unopenable, a writer may be in
/// the middle of a swap, so the read is retried once under a shared lock, which waits for any
/// commit in progress. Files larger than [`AtomicStore::MAX_FILE_BYTES`] are never read in full.
pub struct AtomicStore<D> {
    dir: PathBuf,
    path: PathBuf,
    lock_path: PathBuf,
    replacer: Arc<dyn FileReplacer>,
    lock_timeout: Duration,
    subscribers: Mutex<Vec<Sender<u64>>>,
    /// What this instance last loaded, committed, or set aside; `None` before any of them.
    seen: Mutex<Option<Seen>>,
    document: PhantomData<fn() -> D>,
}

impl<D> fmt::Debug for AtomicStore<D> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AtomicStore")
            .field("path", &self.path)
            .field("lock_timeout", &self.lock_timeout)
            .finish_non_exhaustive()
    }
}

impl<D: StoreDocument> AtomicStore<D> {
    /// How long a commit (or a reader waiting out a commit) waits for the lock by default.
    pub const DEFAULT_LOCK_TIMEOUT: Duration = Duration::from_secs(10);

    /// Largest store file (16 MiB). A larger file is [`ConfigError::Corrupt`] for every
    /// operation, so it is never parsed and never overwritten; a document whose encoding would
    /// be larger is refused by [`AtomicStore::commit`] with [`ConfigError::IoFailure`].
    pub const MAX_FILE_BYTES: u64 = FILE_LIMIT;

    /// A store for `D::FILE_NAME` inside `dir`.
    pub fn new(dir: impl Into<PathBuf>, replacer: Arc<dyn FileReplacer>) -> Self {
        let dir = dir.into();
        let path = dir.join(D::FILE_NAME);
        let lock_path = dir.join(format!("{}.lock", D::FILE_NAME));
        Self {
            dir,
            path,
            lock_path,
            replacer,
            lock_timeout: Self::DEFAULT_LOCK_TIMEOUT,
            subscribers: Mutex::new(Vec::new()),
            seen: Mutex::new(None),
            document: PhantomData,
        }
    }

    /// Replaces the lock wait limit.
    #[must_use]
    pub fn with_lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }

    /// Path of the document file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Path of the lock file (`<file>.lock`). It is never deleted.
    pub fn lock_path(&self) -> &Path {
        &self.lock_path
    }

    /// Loads the document. A missing file is [`ConfigError::NotFound`].
    ///
    /// The store remembers what it read, even a file it could not decode, as the basis of the
    /// next [`AtomicStore::commit`] and [`AtomicStore::poll_changed`].
    pub fn load(&self) -> Result<Loaded<D>, ConfigError> {
        let snapshot = match self.read_settled(|| read_capped(&self.path)) {
            Ok(snapshot) => snapshot,
            Err(error) => {
                match error {
                    ConfigError::NotFound => self.remember(Seen::MISSING),
                    ConfigError::Corrupt => self.remember(Seen {
                        revision: None,
                        content: Content::Oversized,
                        stamp: fs::metadata(&self.path)
                            .ok()
                            .and_then(|metadata| Stamp::of(&metadata)),
                    }),
                    _ => {}
                }
                return Err(error);
            }
        };
        let loaded = decode_bytes::<D>(&snapshot.bytes);
        self.remember(Seen {
            revision: loaded
                .as_ref()
                .ok()
                .map(|loaded| loaded.document.revision()),
            content: Content::Bytes(sha256(&snapshot.bytes)),
            stamp: Stamp::matching(snapshot.stamp, &snapshot.bytes),
        });
        loaded
    }

    /// Loads the document, or [`StoreDocument::default_document`] when the file does not exist.
    ///
    /// Every other failure, including [`ConfigError::Corrupt`], is returned so that the caller
    /// can run on defaults without overwriting the damaged file.
    pub fn load_or_default(&self) -> Result<Loaded<D>, ConfigError> {
        match self.load() {
            Err(ConfigError::NotFound) => Ok(Loaded::clean(D::default_document())),
            other => other,
        }
    }

    /// The revision currently on disk; 0 when the file does not exist.
    ///
    /// Cheap enough for a polling loop: it reads only the first bytes of a file that starts
    /// with the canonical header the encoder writes ([`StoreDocument::revision_from_head`]),
    /// and takes no lock unless it has to wait out a commit in progress. The rest of such a
    /// file is not validated here; [`AtomicStore::load`] and [`AtomicStore::commit`] report a
    /// corrupt file. Any other file is decoded in full.
    ///
    /// An edit that keeps the revision (typically a hand edit) does not show here; see
    /// [`AtomicStore::poll_changed`].
    pub fn poll_revision(&self) -> Result<u64, ConfigError> {
        match self.read_settled(|| self.peek_revision()) {
            Err(ConfigError::NotFound) => Ok(0),
            other => other,
        }
    }

    /// Whether the file differs from what this store last loaded or committed.
    ///
    /// Unlike [`AtomicStore::poll_revision`], this notices an edit that kept the revision. It
    /// is `true` before this store first loads, commits, or sets the file aside, and `false`
    /// while the file still holds what this store last loaded (decodable or not) or committed,
    /// or is still missing. The file's size and modification time are compared first, so an
    /// unchanged file is not read; an edit that keeps both may go unnoticed here (some file
    /// systems keep times coarsely), but [`AtomicStore::commit`] still refuses to overwrite it.
    pub fn poll_changed(&self) -> Result<bool, ConfigError> {
        let Some(seen) = self.seen() else {
            return Ok(true);
        };
        match self.read_settled(|| self.peek_changed(&seen)) {
            Err(ConfigError::NotFound) => Ok(seen.content != Content::Missing),
            other => other,
        }
    }

    /// Receives the new revision after every successful commit through this store instance.
    pub fn subscribe(&self) -> Receiver<u64> {
        let (sender, receiver) = mpsc::channel();
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(sender);
        receiver
    }

    /// Writes `document` as revision `expected_revision + 1`.
    ///
    /// Fails with [`ConfigError::Conflict`] when the on-disk revision is not
    /// `expected_revision`; the first commit of a missing file expects 0 and yields 1. It also
    /// fails with `Conflict` (then `expected == actual`) when this store last loaded or
    /// committed the file at `expected_revision`, or loaded a file it could not decode, and the
    /// file's content has changed since: another writer, such as a hand edit, changed it
    /// without a new revision. Load again and reapply the change. The revision stored in
    /// `document` is ignored. Nothing is written when the call fails:
    ///
    /// - [`ConfigError::Corrupt`] or [`ConfigError::UnsupportedVersion`]: the file on disk is
    ///   damaged, too large, or of another version (it is never overwritten; see
    ///   [`AtomicStore::set_aside`]), or `document` holds a value its format cannot represent
    ///   or does not carry the current version. [`AtomicStore::load`] tells the two apart.
    /// - [`ConfigError::IoFailure`]: the lock timed out, the encoded document is larger than
    ///   [`AtomicStore::MAX_FILE_BYTES`], or the write could not be verified (the previous
    ///   content is then restored on a best-effort basis).
    ///
    /// No temporary file is left behind on failure.
    pub fn commit(&self, expected_revision: u64, document: &D) -> Result<u64, ConfigError> {
        fs::create_dir_all(&self.dir)?;
        let guard = self.lock()?;
        self.remove_stale_temps();
        let previous = read_optional(&self.path)?;
        let actual = match &previous {
            Some(bytes) => decode_bytes::<D>(bytes)?.document.revision(),
            None => 0,
        };
        if actual != expected_revision || self.changed_since_seen(actual, previous.as_deref()) {
            return Err(ConfigError::Conflict {
                expected: expected_revision,
                actual,
            });
        }
        // Decoders read only revisions that can be incremented; this guards other documents.
        let next = actual.checked_add(1).ok_or(ConfigError::IoFailure)?;
        let mut document = document.clone();
        document.set_revision(next);
        let text = document.encode()?;
        if exceeds_limit(text.len()) {
            return Err(ConfigError::IoFailure);
        }
        let stamp = self.write_verified(text.as_bytes(), previous.as_deref())?;
        self.remember(Seen {
            revision: Some(next),
            content: Content::Bytes(sha256(text.as_bytes())),
            stamp: Stamp::matching(stamp, text.as_bytes()),
        });
        drop(guard);
        self.notify(next);
        Ok(next)
    }

    /// Moves the current file aside (to `<file>.bak-<unix-ms>[-n]`) under the store lock.
    ///
    /// This is the explicit recovery path for a corrupt or unsupported file: afterwards the
    /// store is empty and the next commit expects revision 0. Returns the backup path, or
    /// `None` when there was no file.
    pub fn set_aside(&self) -> Result<Option<PathBuf>, ConfigError> {
        if !self.dir.try_exists()? {
            self.remember(Seen::MISSING);
            return Ok(None);
        }
        let _guard = self.lock()?;
        if !self.path.try_exists()? {
            self.remember(Seen::MISSING);
            return Ok(None);
        }
        let backup = self.free_backup_path()?;
        fs::rename(&self.path, &backup)?;
        self.remember(Seen::MISSING);
        Ok(Some(backup))
    }

    fn seen(&self) -> Option<Seen> {
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn remember(&self, seen: Seen) {
        *self.seen.lock().unwrap_or_else(PoisonError::into_inner) = Some(seen);
    }

    /// Whether another writer replaced the content that a commit at `revision` is based on:
    /// the file this store last loaded or committed at that revision, or a file it could not
    /// decode (no commit can be based on one). `current` is the content now (`None`: no file).
    fn changed_since_seen(&self, revision: u64, current: Option<&[u8]>) -> bool {
        self.seen().is_some_and(|seen| {
            seen.revision
                .is_none_or(|seen_revision| seen_revision == revision)
                && !seen.content.is(current)
        })
    }

    /// The revision of the file, from its header when possible, else from a full decode.
    fn peek_revision(&self) -> Result<u64, ConfigError> {
        let mut file = File::open(&self.path)?;
        if file.metadata()?.len() > Self::MAX_FILE_BYTES {
            return Err(ConfigError::Corrupt);
        }
        let mut head = Vec::new();
        (&mut file).take(HEAD_BYTES).read_to_end(&mut head)?;
        if let Some(revision) = D::revision_from_head(&head) {
            return Ok(revision);
        }
        let bytes = read_rest_capped(file, head)?;
        decode_bytes::<D>(&bytes).map(|loaded| loaded.document.revision())
    }

    /// Whether the file differs from `seen`; reads it only when its stamp changed.
    fn peek_changed(&self, seen: &Seen) -> Result<bool, ConfigError> {
        let file = File::open(&self.path)?;
        let stamp = file
            .metadata()
            .ok()
            .and_then(|metadata| Stamp::of(&metadata));
        if seen.content != Content::Missing && stamp.is_some() && stamp == seen.stamp {
            return Ok(false);
        }
        let bytes = match read_rest_capped(file, Vec::new()) {
            Ok(bytes) => bytes,
            // Too large to be what this store read or wrote, unless it saw this very file
            // (same stamp, handled above).
            Err(ConfigError::Corrupt) => return Ok(true),
            Err(error) => return Err(error),
        };
        if !seen.content.is(Some(&bytes)) {
            return Ok(true);
        }
        // The same bytes under a new stamp (rewritten unchanged): keep the new stamp, so that
        // the next poll does not read the file again.
        let mut current = self.seen.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(current) = current.as_mut()
            && current.content == seen.content
        {
            current.stamp = Stamp::matching(stamp, &bytes);
        }
        Ok(false)
    }

    /// Runs `read`; when it fails the way a read racing a swap can (missing file, access
    /// denied, sharing violation), waits for any commit in progress and runs it once more.
    fn read_settled<T>(&self, read: impl Fn() -> Result<T, ConfigError>) -> Result<T, ConfigError> {
        match read() {
            Err(ConfigError::NotFound | ConfigError::PermissionDenied | ConfigError::IoFailure) => {
                let _guard = self.lock_shared()?;
                read()
            }
            other => other,
        }
    }

    /// Exclusive lock for writers; creates the lock file.
    fn lock(&self) -> Result<LockGuard, ConfigError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_path)?;
        self.acquire(file, File::try_lock)
    }

    /// Shared lock for readers; `None` when no writer ever created the lock file.
    fn lock_shared(&self) -> Result<Option<LockGuard>, ConfigError> {
        let file = match OpenOptions::new().read(true).open(&self.lock_path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        self.acquire(file, File::try_lock_shared).map(Some)
    }

    /// Polls `try_lock` with growing pauses until it succeeds or the lock timeout passes.
    fn acquire(
        &self,
        file: File,
        try_lock: fn(&File) -> Result<(), TryLockError>,
    ) -> Result<LockGuard, ConfigError> {
        let deadline = Instant::now().checked_add(self.lock_timeout);
        let mut pause = Duration::from_millis(1);
        loop {
            match try_lock(&file) {
                Ok(()) => return Ok(LockGuard { file }),
                Err(TryLockError::WouldBlock) => {}
                Err(TryLockError::Error(error)) => return Err(error.into()),
            }
            if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
                return Err(ConfigError::IoFailure);
            }
            thread::sleep(pause);
            pause = pause.saturating_mul(2).min(MAX_LOCK_PAUSE);
        }
    }

    /// Removes temporary files left by a writer that died mid-commit. Temporary files exist
    /// only while their writer holds the lock, so under the lock every one of them is stale.
    fn remove_stale_temps(&self) {
        let prefix = format!("{}.tmp-", D::FILE_NAME);
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return;
        };
        for entry in entries.flatten() {
            let stale = entry
                .file_name()
                .to_str()
                .is_some_and(|name| name.starts_with(&prefix));
            if stale {
                let _ = fs::remove_file(entry.path());
            }
        }
    }

    fn temp_path(&self) -> PathBuf {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        self.dir.join(format!(
            "{}.tmp-{}-{sequence}",
            D::FILE_NAME,
            std::process::id()
        ))
    }

    fn free_backup_path(&self) -> Result<PathBuf, ConfigError> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        let base = format!("{}.bak-{stamp}", D::FILE_NAME);
        let mut candidate = self.dir.join(&base);
        let mut suffix = 0u32;
        while candidate.try_exists()? {
            suffix = suffix.checked_add(1).ok_or(ConfigError::IoFailure)?;
            candidate = self.dir.join(format!("{base}-{suffix}"));
        }
        Ok(candidate)
    }

    /// Writes `bytes` to a flushed temporary file and swaps it in. The temporary file never
    /// outlives this call.
    fn swap_in(&self, bytes: &[u8]) -> Result<(), ConfigError> {
        let temp = self.temp_path();
        let result =
            write_synced(&temp, bytes).and_then(|()| self.replacer.replace(&temp, &self.path));
        if result.is_err() || temp.exists() {
            let _ = fs::remove_file(&temp);
        }
        result.map_err(ConfigError::from)
    }

    /// Swaps `bytes` in and reads them back; returns the stamp of the verified file.
    fn write_verified(
        &self,
        bytes: &[u8],
        previous: Option<&[u8]>,
    ) -> Result<Option<Stamp>, ConfigError> {
        self.swap_in(bytes)?;
        if let Ok(back) = read_capped(&self.path)
            && sha256(&back.bytes) == sha256(bytes)
        {
            return Ok(back.stamp);
        }
        match previous {
            Some(old) => {
                let _ = self.swap_in(old);
            }
            None => {
                let _ = fs::remove_file(&self.path);
            }
        }
        Err(ConfigError::IoFailure)
    }

    fn notify(&self, revision: u64) {
        self.subscribers
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .retain(|subscriber| subscriber.send(revision).is_ok());
    }
}

/// Whether `len` bytes exceed the store file limit.
fn exceeds_limit(len: usize) -> bool {
    u64::try_from(len).map_or(true, |len| len > FILE_LIMIT)
}

/// Reads a whole store file and its stamp; a file above the limit is [`ConfigError::Corrupt`].
fn read_capped(path: &Path) -> Result<Snapshot, ConfigError> {
    let file = File::open(path)?;
    let metadata = file.metadata().ok();
    let expected = metadata.as_ref().map_or(0, Metadata::len);
    if expected > FILE_LIMIT {
        return Err(ConfigError::Corrupt);
    }
    let stamp = metadata.as_ref().and_then(Stamp::of);
    let capacity = usize::try_from(expected).unwrap_or(0);
    let bytes = read_rest_capped(file, Vec::with_capacity(capacity))?;
    Ok(Snapshot { bytes, stamp })
}

/// Appends the rest of `file` to `bytes`, reading at most one byte past the limit.
fn read_rest_capped(file: File, mut bytes: Vec<u8>) -> Result<Vec<u8>, ConfigError> {
    let already = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let budget = FILE_LIMIT.saturating_add(1).saturating_sub(already);
    file.take(budget).read_to_end(&mut bytes)?;
    if exceeds_limit(bytes.len()) {
        return Err(ConfigError::Corrupt);
    }
    Ok(bytes)
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>, ConfigError> {
    match read_capped(path) {
        Ok(snapshot) => Ok(Some(snapshot.bytes)),
        Err(ConfigError::NotFound) => Ok(None),
        Err(error) => Err(error),
    }
}

fn decode_bytes<D: StoreDocument>(bytes: &[u8]) -> Result<Loaded<D>, ConfigError> {
    let text = core::str::from_utf8(bytes).map_err(|_| ConfigError::Corrupt)?;
    D::decode(text)
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}
