//! Mutable macro store with optimistic revisions (plan §4).
//!
//! The store holds the authoritative macro list of its owner (the controller's worker thread).
//! Every change names the revision it was based on: a stale revision fails with
//! [`MacroError::Conflict`] and changes nothing, so a writer never silently overwrites a change
//! it has not seen. Readers take immutable [`MacroTable`] snapshots.
//!
//! Persistence lives in `raumakey-config` (`MacroDocument`); [`MacroStore::from_entries`] and
//! [`MacroStore::entries`] convert from and to its entry list.
//!
//! # Loading stored data
//!
//! Load user data (`macros.toml`, imports kept from earlier versions) with
//! [`MacroStore::from_entries_lenient`], never with [`MacroStore::from_entries`]. Data written
//! before plan D29 and D30 can hold entries that are invalid now: triggers of 33–64 scalars,
//! and replacements that do not fit one engine insert with a delimiter. The lenient loader
//! reports each of them as an [`EntryIssue`] with its input position, and every valid entry
//! keeps working; the strict loader would refuse the whole list and disable every macro.
//!
//! The store does not hold the rejected entries, so [`MacroStore::entries`] leaves them out.
//! The caller must keep them verbatim, by the positions the issues name, and write them back
//! unchanged with the store's entries rather than drop them, so that the user can still see,
//! fix, or export them. This crate has no API for that yet; the controller's configuration
//! lifecycle must provide it.

use core::fmt;
use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock};

use raumakey_model::macros::{MacroEntry, MacroId};

use crate::error::MacroError;
use crate::table::{MacroTable, first_duplicate_id};
use crate::validate::normalize_entry;

/// An entry skipped by [`MacroStore::from_entries_lenient`].
///
/// Carries only the input position and a typed error, never macro text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntryIssue {
    /// Zero-based position of the entry in the input.
    pub index: usize,
    /// Why the entry was skipped: [`MacroError::InvalidEntry`] or [`MacroError::DuplicateId`].
    pub error: MacroError,
}

/// Validated macro list with a revision counter and a lazily built lookup snapshot.
///
/// Invariants: every entry is valid and NFC (see [`normalize_entry`]), identifiers are unique,
/// and the entry order is the order given to [`MacroStore::from_entries`] followed by
/// insertions.
///
/// Every successful change increments the revision by one. Failed calls change neither the
/// entries nor the revision. Errors are reported in this order: [`MacroError::Conflict`],
/// [`MacroError::InvalidEntry`] or [`MacroError::NotFound`], [`MacroError::RevisionExhausted`],
/// [`MacroError::IdSpaceExhausted`].
///
/// New entries receive the identifier `max(id) + 1` (`1` for an empty store); the identifier of
/// a removed entry with the largest id can therefore be reused, which revisions make safe.
///
/// Cost: [`MacroStore::insert`] takes constant amortized time apart from validating the entry,
/// [`MacroStore::upsert`] and [`MacroStore::remove`] take time linear in the number of entries,
/// and no change rebuilds the lookup table: the first [`MacroStore::snapshot`] after a change
/// builds it once. [`MacroStore::insert_many`] adds a whole batch (for example a legacy import)
/// in one revision.
///
/// The [`fmt::Debug`] output shows only the revision and the entry count.
#[derive(Clone)]
pub struct MacroStore {
    revision: u64,
    entries: Vec<MacroEntry>,
    /// Largest identifier in use; `None` when the store is empty.
    max_id: Option<u64>,
    /// Largest identifier of an entry that [`MacroStore::from_entries_lenient`] skipped. The
    /// caller writes such entries back verbatim, so new entries never reuse their identifiers.
    reserved_max: Option<u64>,
    /// Lookup table of the current entries, built by the first snapshot after a change.
    table: OnceLock<Arc<MacroTable>>,
}

impl Default for MacroStore {
    fn default() -> Self {
        Self::new()
    }
}

impl MacroStore {
    /// An empty store at revision 0.
    pub fn new() -> Self {
        Self::build(0, Vec::new())
    }

    /// Builds a store from persisted entries at `revision`, keeping their order.
    ///
    /// Every entry is validated and normalized to NFC. Fails with
    /// [`MacroError::InvalidEntry`] for the first invalid entry or [`MacroError::DuplicateId`]
    /// when two entries share an identifier. For user data use
    /// [`from_entries_lenient`](Self::from_entries_lenient) instead (see the [module
    /// documentation](self)): data stored before plan D29 and D30 can hold entries that are
    /// invalid now.
    pub fn from_entries(
        revision: u64,
        entries: impl IntoIterator<Item = MacroEntry>,
    ) -> Result<Self, MacroError> {
        let mut normalized = Vec::new();
        for entry in entries {
            let id = entry.id;
            let entry =
                normalize_entry(entry).map_err(|reason| MacroError::InvalidEntry { id, reason })?;
            normalized.push(entry);
        }
        if let Some(id) = first_duplicate_id(&normalized) {
            return Err(MacroError::DuplicateId(id));
        }
        Ok(Self::build(revision, normalized))
    }

    /// Builds a store from persisted entries, skipping the ones that cannot be kept.
    ///
    /// Invalid entries and later entries that reuse an identifier already kept are skipped and
    /// reported with their input position; everything else is kept in order. Use this to load
    /// user data, where one bad entry must not disable every macro: entries stored under the
    /// limits before plan D29 and D30 (a 40-scalar trigger, a replacement beyond one insert)
    /// are reported as [`EntryError::TriggerTooLong`](crate::EntryError::TriggerTooLong) or
    /// [`EntryError::ReplacementTooLong`](crate::EntryError::ReplacementTooLong). The skipped
    /// entries are not kept here; the caller must keep them verbatim, by the reported
    /// positions, and write them back unchanged instead of dropping them (see the [module
    /// documentation](self)).
    pub fn from_entries_lenient(
        revision: u64,
        entries: impl IntoIterator<Item = MacroEntry>,
    ) -> (Self, Vec<EntryIssue>) {
        let mut kept = Vec::new();
        let mut seen = BTreeSet::new();
        let mut issues = Vec::new();
        let mut reserved_max = None;
        for (index, entry) in entries.into_iter().enumerate() {
            let id = entry.id;
            match normalize_entry(entry) {
                Err(reason) => {
                    reserved_max = reserved_max.max(Some(id.0));
                    issues.push(EntryIssue {
                        index,
                        error: MacroError::InvalidEntry { id, reason },
                    });
                }
                Ok(entry) => {
                    if seen.insert(id) {
                        kept.push(entry);
                    } else {
                        issues.push(EntryIssue {
                            index,
                            error: MacroError::DuplicateId(id),
                        });
                    }
                }
            }
        }
        let mut store = Self::build(revision, kept);
        store.reserved_max = reserved_max;
        (store, issues)
    }

    /// Current revision.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// All entries (enabled or not) in store order.
    pub fn list(&self) -> &[MacroEntry] {
        &self.entries
    }

    /// A copy of all entries in store order, for persisting as a `MacroDocument`.
    pub fn entries(&self) -> Vec<MacroEntry> {
        self.entries.clone()
    }

    /// The entry with identifier `id`.
    pub fn get(&self, id: MacroId) -> Option<&MacroEntry> {
        self.entries.iter().find(|entry| entry.id == id)
    }

    /// Number of entries (enabled or not).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the store has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The identifier the next new entry receives: `max(id) + 1`, or `1` when empty. The
    /// maximum also covers entries that [`MacroStore::from_entries_lenient`] skipped.
    pub fn next_id(&self) -> Result<MacroId, MacroError> {
        match self.max_id.max(self.reserved_max) {
            None => Ok(MacroId(1)),
            Some(max) => max
                .checked_add(1)
                .map(MacroId)
                .ok_or(MacroError::IdSpaceExhausted),
        }
    }

    /// Replaces the entry with `entry.id`, or inserts `entry` as a new entry when no entry has
    /// that identifier. A new entry always receives [`MacroStore::next_id`]; the identifier it
    /// carried is ignored, so the store remains the only authority for identifiers.
    ///
    /// Returns the new revision.
    pub fn upsert(&mut self, expected_revision: u64, entry: MacroEntry) -> Result<u64, MacroError> {
        self.check_revision(expected_revision)?;
        let id = entry.id;
        let entry =
            normalize_entry(entry).map_err(|reason| MacroError::InvalidEntry { id, reason })?;
        let revision = self.next_revision()?;
        match self.entries.iter_mut().find(|existing| existing.id == id) {
            Some(existing) => *existing = entry,
            None => {
                let id = self.next_id()?;
                self.push(MacroEntry { id, ..entry });
            }
        }
        self.commit(revision);
        Ok(revision)
    }

    /// Appends `entry` as a new entry with identifier [`MacroStore::next_id`], whatever
    /// identifier it carried. Returns the new revision and the assigned identifier.
    pub fn insert(
        &mut self,
        expected_revision: u64,
        entry: MacroEntry,
    ) -> Result<(u64, MacroId), MacroError> {
        self.check_revision(expected_revision)?;
        let carried = entry.id;
        let entry = normalize_entry(entry).map_err(|reason| MacroError::InvalidEntry {
            id: carried,
            reason,
        })?;
        let revision = self.next_revision()?;
        let id = self.next_id()?;
        self.push(MacroEntry { id, ..entry });
        self.commit(revision);
        Ok((revision, id))
    }

    /// Appends every entry of a batch, in order, as new entries in **one** revision; use it to
    /// merge a legacy import into the store.
    ///
    /// The entries receive consecutive identifiers starting at [`MacroStore::next_id`],
    /// whatever identifiers they carried. Every entry is validated before anything changes: an
    /// invalid entry fails the whole batch with [`MacroError::InvalidEntry`] naming the
    /// identifier it carried, and nothing is added. A batch that would need an identifier above
    /// `u64::MAX` fails with [`MacroError::IdSpaceExhausted`]. An empty batch changes nothing and
    /// returns the current revision.
    ///
    /// Returns the new revision and the assigned identifiers in input order.
    pub fn insert_many(
        &mut self,
        expected_revision: u64,
        entries: impl IntoIterator<Item = MacroEntry>,
    ) -> Result<(u64, Vec<MacroId>), MacroError> {
        self.check_revision(expected_revision)?;
        let mut batch = Vec::new();
        for entry in entries {
            let carried = entry.id;
            let entry = normalize_entry(entry).map_err(|reason| MacroError::InvalidEntry {
                id: carried,
                reason,
            })?;
            batch.push(entry);
        }
        if batch.is_empty() {
            return Ok((self.revision, Vec::new()));
        }
        let revision = self.next_revision()?;
        let first = self.next_id()?.0;
        let last = u64::try_from(batch.len())
            .ok()
            .and_then(|count| first.checked_add(count.saturating_sub(1)))
            .ok_or(MacroError::IdSpaceExhausted)?;
        let mut ids = Vec::with_capacity(batch.len());
        self.entries.reserve(batch.len());
        for (entry, id) in batch.into_iter().zip((first..=last).map(MacroId)) {
            ids.push(id);
            self.push(MacroEntry { id, ..entry });
        }
        self.commit(revision);
        Ok((revision, ids))
    }

    /// Removes the entry with identifier `id`. Returns the new revision.
    pub fn remove(&mut self, expected_revision: u64, id: MacroId) -> Result<u64, MacroError> {
        self.check_revision(expected_revision)?;
        if self.get(id).is_none() {
            return Err(MacroError::NotFound(id));
        }
        let revision = self.next_revision()?;
        self.entries.retain(|entry| entry.id != id);
        if self.max_id == Some(id.0) {
            self.max_id = max_id(&self.entries);
        }
        self.commit(revision);
        Ok(revision)
    }

    /// Immutable lookup table of the current entries.
    ///
    /// The table is built on the first call after a change; later calls return the same `Arc`
    /// until the next successful change. Earlier snapshots never change.
    pub fn snapshot(&self) -> Arc<MacroTable> {
        Arc::clone(
            self.table
                .get_or_init(|| Arc::new(MacroTable::from_normalized(&self.entries))),
        )
    }

    fn build(revision: u64, entries: Vec<MacroEntry>) -> Self {
        Self {
            revision,
            max_id: max_id(&entries),
            reserved_max: None,
            entries,
            table: OnceLock::new(),
        }
    }

    /// Appends a validated entry whose identifier is not in use.
    fn push(&mut self, entry: MacroEntry) {
        self.max_id = self.max_id.max(Some(entry.id.0));
        self.entries.push(entry);
    }

    fn check_revision(&self, expected: u64) -> Result<(), MacroError> {
        if expected == self.revision {
            Ok(())
        } else {
            Err(MacroError::Conflict {
                expected,
                actual: self.revision,
            })
        }
    }

    fn next_revision(&self) -> Result<u64, MacroError> {
        self.revision
            .checked_add(1)
            .ok_or(MacroError::RevisionExhausted)
    }

    /// Records a successful change: sets the revision and drops the stale lookup table (the
    /// next snapshot builds a new one).
    fn commit(&mut self, revision: u64) {
        self.revision = revision;
        self.table = OnceLock::new();
    }
}

/// Largest identifier among `entries`.
fn max_id(entries: &[MacroEntry]) -> Option<u64> {
    entries.iter().map(|entry| entry.id.0).max()
}

impl fmt::Debug for MacroStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroStore")
            .field("revision", &self.revision)
            .field("len", &self.entries.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};

    use super::MacroStore;
    use crate::table::table_builds;

    fn entry(trigger: String) -> MacroEntry {
        MacroEntry {
            id: MacroId(0),
            trigger,
            replacement: "x".to_owned(),
            enabled: true,
            boundary: BoundaryPolicy::Delimiter,
            case_policy: CasePolicy::Adapt,
        }
    }

    #[test]
    fn changes_build_no_table_until_a_snapshot_is_taken() {
        let start = table_builds();
        let mut store = MacroStore::new();
        for revision in 0..200 {
            store
                .insert(revision, entry(format!("k{revision}")))
                .unwrap();
        }
        store.upsert(200, entry("k0".to_owned())).unwrap();
        store.remove(201, MacroId(1)).unwrap();
        store
            .insert_many(202, (0..50).map(|n| entry(format!("m{n}"))))
            .unwrap();
        assert_eq!(table_builds() - start, 0);

        let first = store.snapshot();
        let second = store.snapshot();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(table_builds() - start, 1);
        assert_eq!(first.len(), 250);

        store.remove(203, MacroId(2)).unwrap();
        assert_eq!(table_builds() - start, 1);
        assert_eq!(store.snapshot().len(), 249);
        assert_eq!(table_builds() - start, 2);
    }

    #[test]
    fn loading_builds_no_table_until_a_snapshot_is_taken() {
        let start = table_builds();
        let entries = (1..=100).map(|n| MacroEntry {
            id: MacroId(n),
            ..entry(format!("k{n}"))
        });
        let store = MacroStore::from_entries(1, entries).unwrap();
        let clone = store.clone();
        assert_eq!(table_builds() - start, 0);
        assert_eq!(store.snapshot().len(), 100);
        assert_eq!(clone.snapshot().len(), 100);
        assert_eq!(table_builds() - start, 2);
    }
}
