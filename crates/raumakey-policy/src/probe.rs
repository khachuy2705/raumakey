//! TSF capability probe cache (plan §6, clean-room spec §9 item 6).
//!
//! The cache is a compatibility hint, not ground truth: it keeps at most one record per
//! executable, never more than [`MAX_PROBE_RECORDS`] records, and resolution only trusts records
//! of the current [`PROBE_VERSION`]. Records of other versions are kept so that a build of that
//! version can still read its own results.
//!
//! A full cache keeps taking this build's results unless every record in it comes from a newer
//! probe version. A current record of a new executable takes the place of the least useful
//! record, whose loss costs at most a re-probe of that host: the earliest record of an older
//! probe version, then the earliest successful current probe, then the earliest failed current
//! probe. Records of a newer probe version are never evicted, because this build cannot recreate
//! them, so a full cache that holds nothing else rejects the results of new executables with
//! [`PolicyError::ProbeCacheFull`].
//! Records of other versions only take the place of records older than [`PROBE_VERSION`].

use core::cmp::Ordering;

use raumakey_model::host::ExeName;
use raumakey_model::policy::{PROBE_VERSION, ProbeRecord};

use crate::PolicyError;

/// Maximum number of records a [`ProbeCache`] holds.
pub const MAX_PROBE_RECORDS: usize = 1024;

/// Probe records, at most one per executable, in first-insertion order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProbeCache {
    records: Vec<ProbeRecord>,
}

/// Where [`ProbeCache::upsert`] puts a record.
#[derive(Clone, Copy, Debug)]
enum Slot {
    /// In place of the stored record of the same executable, at this index.
    Replace(usize),
    /// At the end; the cache has room.
    Append,
    /// At the end, after evicting the record at this index.
    Evict(usize),
}

/// How readily a full cache evicts a record, lowest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Rank {
    /// A record of an older probe version, which neither this build nor a newer one uses.
    Stale,
    /// A successful current probe: losing it costs a re-probe.
    Succeeded,
    /// A failed current probe: losing it costs a re-probe, and the host gets TSF until then.
    Failed,
}

/// The eviction rank of `record`; records of a newer probe version are never evicted.
fn eviction_rank(record: &ProbeRecord) -> Option<Rank> {
    match record.probe_version.cmp(&PROBE_VERSION) {
        Ordering::Less => Some(Rank::Stale),
        Ordering::Equal if record.tsf_range_edit => Some(Rank::Succeeded),
        Ordering::Equal => Some(Rank::Failed),
        Ordering::Greater => None,
    }
}

impl ProbeCache {
    /// An empty cache.
    pub const fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    /// Builds a cache from persisted records by upserting them in order.
    ///
    /// Duplicates collapse into one record per executable with the newest probe version (the
    /// later record wins between equal versions). When the records do not all fit, later records
    /// displace earlier ones as [`upsert`](Self::upsert) describes, and records that cannot make
    /// room are dropped.
    pub fn from_records<I>(records: I) -> Self
    where
        I: IntoIterator<Item = ProbeRecord>,
    {
        let mut cache = Self::new();
        for record in records {
            // A superseded or overflowing record is only a lost hint; the host is re-probed.
            let _ = cache.upsert(record);
        }
        cache
    }

    /// Inserts the record of `record.exe`, or replaces it in place.
    ///
    /// When the executable is new and the cache already holds [`MAX_PROBE_RECORDS`] records, a
    /// record of the current [`PROBE_VERSION`] evicts the earliest record of an older probe
    /// version, else the earliest current record whose probe succeeded, else the earliest current
    /// record whose probe failed. A record of another version only evicts the earliest record
    /// older than [`PROBE_VERSION`]. Records of a newer version are never evicted.
    ///
    /// # Errors
    ///
    /// Nothing changes on error.
    ///
    /// - [`PolicyError::NewerProbeVersion`] when the stored record of the executable has a newer
    ///   probe version; it is kept.
    /// - [`PolicyError::ProbeCacheFull`] when the executable is new, the cache is full, and no
    ///   record may be evicted for this one: every record comes from a newer probe version, or
    ///   the new record is not current and no record is older than [`PROBE_VERSION`].
    pub fn upsert(&mut self, record: ProbeRecord) -> Result<(), PolicyError> {
        match self.slot(&record.exe, record.probe_version)? {
            Slot::Replace(index) => {
                if let Some(stored) = self.records.get_mut(index) {
                    *stored = record;
                }
            }
            Slot::Append => self.records.push(record),
            Slot::Evict(index) => {
                // `slot` found the index in `self.records`, which has not changed since.
                self.records.remove(index);
                self.records.push(record);
            }
        }
        Ok(())
    }

    /// The record of `exe`, whatever its probe version.
    pub fn get(&self, exe: &ExeName) -> Option<&ProbeRecord> {
        self.records.iter().find(|r| r.exe == *exe)
    }

    /// The record of `exe` when it has the current [`PROBE_VERSION`]; the only kind of record
    /// that policy resolution uses.
    pub fn current(&self, exe: &ExeName) -> Option<&ProbeRecord> {
        self.get(exe).filter(|r| r.probe_version == PROBE_VERSION)
    }

    /// All records, in first-insertion order.
    pub fn records(&self) -> &[ProbeRecord] {
        &self.records
    }

    /// Number of records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// True when the cache holds no record.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The records, for persistence.
    pub fn into_records(self) -> Vec<ProbeRecord> {
        self.records
    }

    /// True when [`upsert`](Self::upsert) would store a current-version record of `exe`.
    pub(crate) fn can_store_current(&self, exe: &ExeName) -> bool {
        self.slot(exe, PROBE_VERSION).is_ok()
    }

    /// Where a record of `exe` with `probe_version` goes (see [`upsert`](Self::upsert)).
    fn slot(&self, exe: &ExeName, probe_version: u32) -> Result<Slot, PolicyError> {
        if let Some((index, stored)) = self.records.iter().enumerate().find(|(_, r)| r.exe == *exe)
        {
            return if stored.probe_version > probe_version {
                Err(PolicyError::NewerProbeVersion {
                    stored: stored.probe_version,
                })
            } else {
                Ok(Slot::Replace(index))
            };
        }
        if self.records.len() < MAX_PROBE_RECORDS {
            return Ok(Slot::Append);
        }
        let evictable = if probe_version == PROBE_VERSION {
            Rank::Failed
        } else {
            Rank::Stale
        };
        self.records
            .iter()
            .enumerate()
            .filter_map(|(index, r)| {
                eviction_rank(r)
                    .filter(|rank| *rank <= evictable)
                    .map(|rank| (rank, index))
            })
            .min()
            .map(|(_, index)| Slot::Evict(index))
            .ok_or(PolicyError::ProbeCacheFull)
    }
}
