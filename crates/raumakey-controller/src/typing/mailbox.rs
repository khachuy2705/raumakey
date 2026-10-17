//! Configuration updates on their way to the hook thread.

use core::fmt;
use std::sync::atomic::{AtomicBool, Ordering};
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
    Runtime(Box<RuntimeSnapshot>),
    /// A macro table built from the macro snapshot of `revision`.
    Macros {
        revision: u64,
        table: Arc<MacroTable>,
    },
    /// A mode.
    Mode(bool),
}

/// The latest posted update of each kind, with its posting number.
#[derive(Default)]
struct Slot {
    posted: u64,
    runtime: Option<(u64, Box<RuntimeSnapshot>)>,
    macros: Option<(u64, u64, Arc<MacroTable>)>,
    mode: Option<(u64, bool)>,
}

impl Slot {
    /// Everything posted, oldest first, leaving the slot empty.
    fn drain(&mut self) -> Vec<Prepared> {
        let mut updates: Vec<(u64, Prepared)> = Vec::with_capacity(3);
        if let Some((order, snapshot)) = self.runtime.take() {
            updates.push((order, Prepared::Runtime(snapshot)));
        }
        if let Some((order, revision, table)) = self.macros.take() {
            updates.push((order, Prepared::Macros { revision, table }));
        }
        if let Some((order, mode)) = self.mode.take() {
            updates.push((order, Prepared::Mode(mode)));
        }
        updates.sort_by_key(|(order, _)| *order);
        updates.into_iter().map(|(_, update)| update).collect()
    }
}

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
/// `Debug` shows only whether something is waiting.
#[derive(Default)]
pub struct Mailbox {
    waiting: AtomicBool,
    slot: Mutex<Slot>,
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
                Prepared::Runtime(Box::new(snapshot))
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
}

impl fmt::Debug for Mailbox {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Mailbox")
            .field("waiting", &self.waiting.load(Ordering::Acquire))
            .finish_non_exhaustive()
    }
}

/// Locks `slot`; a poisoned lock (a panic while it was held, which storing a value cannot
/// cause) still holds a consistent slot.
fn lock(slot: &Mutex<Slot>) -> MutexGuard<'_, Slot> {
    slot.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
