//! Configuration updates on their way to the hook thread.

use core::fmt;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, TryLockError};

use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_macro::MacroTable;
use raumakey_session::{SnapshotError, build_macro_table, check_runtime_snapshot};

/// A configuration change for the typing hook.
///
/// `Debug` shows the kind and the revision only: a macro snapshot holds macro text.
#[derive(Clone)]
pub enum TypingUpdate {
    /// A new runtime snapshot (options, key maps, hotkeys, rules, and the mode it carries),
    /// as the controller's settings produce it from `raumakey.toml`.
    Runtime(RuntimeSnapshot),
    /// A new macro snapshot, from `raumakey-macros.toml`.
    Macros(MacroSnapshot),
    /// The Vietnamese mode chosen in the tray (true for Vietnamese).
    Mode(bool),
}

impl fmt::Debug for TypingUpdate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(snapshot) => f
                .debug_struct("Runtime")
                .field("config_revision", &snapshot.config_revision)
                .finish_non_exhaustive(),
            Self::Macros(snapshot) => f
                .debug_struct("Macros")
                .field("macro_revision", &snapshot.macro_revision)
                .field("entries", &snapshot.entries.len())
                .finish(),
            Self::Mode(mode) => f.debug_tuple("Mode").field(mode).finish(),
        }
    }
}

/// An update checked and prepared on the posting thread, ready for the core.
pub(crate) enum Prepared {
    /// A runtime snapshot that passed `check_runtime_snapshot`.
    Runtime(Arc<RuntimeSnapshot>),
    /// A macro table built from the macro snapshot of `revision`.
    Macros {
        revision: u64,
        table: Arc<MacroTable>,
    },
    /// A mode.
    Mode(bool),
}

/// How many kinds of update there are: [`Mailbox::rearm`] puts the rearmed ones ahead of
/// every pending one by this many posting numbers.
const KINDS: u64 = 3;

/// The latest posted update of each kind, with its posting number, and the last runtime
/// snapshot and macro table that a hook thread took.
#[derive(Default)]
struct Slot {
    posted: u64,
    runtime: Option<(u64, Arc<RuntimeSnapshot>)>,
    macros: Option<(u64, u64, Arc<MacroTable>)>,
    mode: Option<(u64, bool)>,
    taken_runtime: Option<Arc<RuntimeSnapshot>>,
    taken_macros: Option<(u64, Arc<MacroTable>)>,
}

impl Slot {
    /// Everything posted, oldest first, leaving the slot empty; what is taken is kept for
    /// [`Mailbox::rearm`].
    fn drain(&mut self) -> Vec<Prepared> {
        let mut updates: Vec<(u64, Prepared)> = Vec::with_capacity(3);
        if let Some((order, snapshot)) = self.runtime.take() {
            self.taken_runtime = Some(Arc::clone(&snapshot));
            updates.push((order, Prepared::Runtime(snapshot)));
        }
        if let Some((order, revision, table)) = self.macros.take() {
            self.taken_macros = Some((revision, Arc::clone(&table)));
            updates.push((order, Prepared::Macros { revision, table }));
        }
        if let Some((order, mode)) = self.mode.take() {
            updates.push((order, Prepared::Mode(mode)));
        }
        updates.sort_by_key(|(order, _)| *order);
        updates.into_iter().map(|(_, update)| update).collect()
    }

    fn is_empty(&self) -> bool {
        self.runtime.is_none() && self.macros.is_none() && self.mode.is_none()
    }
}

/// The mode a hook thread had after its last event: unknown, English, Vietnamese.
const MODE_UNKNOWN: u8 = 0;
const MODE_ENGLISH: u8 = 1;
const MODE_VIETNAMESE: u8 = 2;

/// Hands configuration updates from any thread to the hook thread.
///
/// [`post`](Self::post) does the expensive part on the posting thread (it checks a runtime
/// snapshot and builds the macro table of a macro snapshot, which can hold thousands of
/// entries) and keeps the result, replacing an update of the same kind that the hook thread
/// has not taken yet. The handler takes what was posted at its next event, before the key,
/// and applies it in posting order, so the last update posted wins.
///
/// The hook thread never waits: it reads a flag, and takes the updates only when it gets the
/// lock at once (`try_lock`). A poster holds the lock only to store one value, so a busy lock
/// just leaves the updates for the next event.
///
/// When Windows removed a hook and the run mode starts a new hook thread on the same mailbox,
/// [`rearm`](Self::rearm) hands the new handler what the last one had: the runtime snapshot
/// and the macro table it took, and the mode it had after its last event (which the hotkey
/// may have changed), ahead of what was posted since.
///
/// `Debug` shows only whether something is waiting.
#[derive(Default)]
pub struct Mailbox {
    waiting: AtomicBool,
    slot: Mutex<Slot>,
    /// The mode of the hook thread after its last event.
    mode: AtomicU8,
}

impl Mailbox {
    /// An empty mailbox.
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks and prepares `update` on the calling thread and leaves it for the hook thread.
    ///
    /// # Errors
    ///
    /// The [`SnapshotError`] for which the session core would refuse the snapshot; nothing is
    /// posted then, and the hook keeps the last good configuration.
    pub fn post(&self, update: TypingUpdate) -> Result<(), SnapshotError> {
        let prepared = match update {
            TypingUpdate::Runtime(snapshot) => {
                check_runtime_snapshot(&snapshot)?;
                Prepared::Runtime(Arc::new(snapshot))
            }
            TypingUpdate::Macros(snapshot) => Prepared::Macros {
                revision: snapshot.macro_revision,
                table: build_macro_table(&snapshot)?,
            },
            TypingUpdate::Mode(mode) => Prepared::Mode(mode),
        };
        let mut slot = lock(&self.slot);
        slot.posted = slot.posted.wrapping_add(1);
        let order = slot.posted;
        match prepared {
            Prepared::Runtime(snapshot) => slot.runtime = Some((order, snapshot)),
            Prepared::Macros { revision, table } => slot.macros = Some((order, revision, table)),
            Prepared::Mode(mode) => slot.mode = Some((order, mode)),
        }
        self.waiting.store(true, Ordering::Release);
        Ok(())
    }

    /// For a new hook thread on this mailbox, after the last one stopped: queues again the
    /// runtime snapshot and the macro table that the last one took, and the mode it had after
    /// its last event, each unless an update of its kind is waiting already; they come ahead
    /// of every update posted since, which therefore still wins. Nothing is queued for a
    /// mailbox whose updates no hook thread took.
    pub fn rearm(&self) {
        let mode = match self.mode.load(Ordering::Acquire) {
            MODE_ENGLISH => Some(false),
            MODE_VIETNAMESE => Some(true),
            _ => None,
        };
        let mut slot = lock(&self.slot);
        let slot = &mut *slot;
        for order in [
            slot.runtime.as_mut().map(|(order, _)| order),
            slot.macros.as_mut().map(|(order, _, _)| order),
            slot.mode.as_mut().map(|(order, _)| order),
        ]
        .into_iter()
        .flatten()
        {
            *order = order.wrapping_add(KINDS);
        }
        slot.posted = slot.posted.wrapping_add(KINDS);
        if slot.runtime.is_none()
            && let Some(snapshot) = &slot.taken_runtime
        {
            slot.runtime = Some((1, Arc::clone(snapshot)));
        }
        if slot.macros.is_none()
            && let Some((revision, table)) = &slot.taken_macros
        {
            slot.macros = Some((2, *revision, Arc::clone(table)));
        }
        if slot.mode.is_none()
            && let Some(mode) = mode
        {
            slot.mode = Some((3, mode));
        }
        if !slot.is_empty() {
            self.waiting.store(true, Ordering::Release);
        }
    }

    /// What was posted since the last call, oldest first; empty when nothing was, or when a
    /// poster holds the lock right now (the updates then wait for the next call). Never
    /// blocks.
    pub(crate) fn take(&self) -> Vec<Prepared> {
        if !self.waiting.load(Ordering::Acquire) {
            return Vec::new();
        }
        let mut slot = match self.slot.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner(),
            Err(TryLockError::WouldBlock) => return Vec::new(),
        };
        self.waiting.store(false, Ordering::Release);
        slot.drain()
    }

    /// Records the mode of the hook thread after an event, for [`rearm`](Self::rearm). One
    /// atomic store; never blocks.
    pub(crate) fn note_mode(&self, vietnamese: bool) {
        let mode = if vietnamese {
            MODE_VIETNAMESE
        } else {
            MODE_ENGLISH
        };
        self.mode.store(mode, Ordering::Release);
    }
}

impl fmt::Debug for Mailbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox")
            .field("waiting", &self.waiting.load(Ordering::Acquire))
            .field(
                "mode_known",
                &(self.mode.load(Ordering::Acquire) != MODE_UNKNOWN),
            )
            .finish_non_exhaustive()
    }
}

/// Locks `slot`; a poisoned lock (a panic while it was held, which storing a value cannot
/// cause) still holds a consistent slot.
fn lock(slot: &Mutex<Slot>) -> MutexGuard<'_, Slot> {
    slot.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
