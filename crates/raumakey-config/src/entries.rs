//! Arrays of tables whose entries are model types without an extension map.

use std::collections::{BTreeMap, VecDeque};

use serde::Serialize;
use serde::ser::SerializeSeq;

use crate::fields::{Fields, Peek};

/// What the model type of an array-of-tables entry cannot hold (matrix #45, design §8).
///
/// Model entry types such as [`MacroEntry`](raumakey_model::macros::MacroEntry) have no
/// extension map, so a document keeps two kinds of data beside its entry list:
///
/// - **Unknown keys** of each decoded entry, paired with the entry's identity (for example a
///   key map's id, or a macro's id and trigger). When the document is encoded, each entry
///   receives the unknown keys of the first unused record with its identity, so the keys follow
///   their entry when an editor reorders, removes, or adds entries. Records whose entry no
///   longer exists are not written. The identity must tell entries apart for as long as a
///   document is edited: an entry that is given the identity of a removed one receives the
///   removed entry's unknown keys.
/// - **Rejected entries**: tables whose identity could not be read (a missing trigger, a
///   negative id, ...). They are kept verbatim and written back after the decoded entries, so a
///   commit never discards them; every load reports them again until the file is fixed.
#[derive(Clone, Debug, PartialEq)]
pub struct EntryExtras<K> {
    /// Unknown keys of decoded entries, each with the identity of its entry, in file order.
    ///
    /// Entries without unknown keys have no record, except when their identity repeats: then
    /// every occurrence has one (possibly empty), so that each record finds its own entry
    /// again.
    pub unknown_keys: Vec<(K, toml::Table)>,
    /// Entries that could not be decoded, verbatim, in file order.
    pub rejected: Vec<toml::Table>,
}

impl<K> Default for EntryExtras<K> {
    fn default() -> Self {
        Self {
            unknown_keys: Vec::new(),
            rejected: Vec::new(),
        }
    }
}

impl<K> EntryExtras<K> {
    /// Whether there is nothing to carry over.
    pub fn is_empty(&self) -> bool {
        self.unknown_keys.is_empty() && self.rejected.is_empty()
    }
}

impl<K: PartialEq> EntryExtras<K> {
    /// Unknown keys of the first entry with identity `id`.
    pub fn unknown_keys_of(&self, id: &K) -> Option<&toml::Table> {
        self.unknown_keys
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, table)| table)
    }
}

impl<K: Ord> EntryExtras<K> {
    /// The unknown keys to write for each identity of `ids`, in order (see [`match_in_order`]).
    pub(crate) fn assign<'s, 'k>(
        &'s self,
        ids: impl IntoIterator<Item = &'k K>,
    ) -> Vec<Option<&'s toml::Table>>
    where
        K: 'k,
        's: 'k,
    {
        match_in_order(self.unknown_keys.iter().map(|(id, table)| (id, table)), ids)
    }
}

/// Pairs each identity of `ids` with the first unused record of the same identity.
///
/// Records are consumed in their order, so entries that share an identity receive the records
/// of that identity in turn.
pub(crate) fn match_in_order<K: Ord, V>(
    records: impl IntoIterator<Item = (K, V)>,
    ids: impl IntoIterator<Item = K>,
) -> Vec<Option<V>> {
    let mut queues: BTreeMap<K, VecDeque<V>> = BTreeMap::new();
    for (id, value) in records {
        queues.entry(id).or_default().push_back(value);
    }
    ids.into_iter()
        .map(|id| queues.get_mut(&id).and_then(VecDeque::pop_front))
        .collect()
}

/// Decodes the array of tables `key` entry by entry (matrix #45).
///
/// `identify` checks, without consuming anything, whether an entry can be decoded; when it
/// returns `None` the table is kept verbatim in [`EntryExtras::rejected`]. Otherwise `build`
/// consumes the known keys and returns the entry with its identity, and the keys it left are
/// the entry's unknown keys. Elements that are not tables are dropped and reported.
pub(crate) fn decode_entries<T, K: Ord, I>(
    root: &mut Fields<'_, toml::Value>,
    key: &str,
    mut identify: impl FnMut(&mut Peek<'_, '_, toml::Value>) -> Option<I>,
    mut build: impl FnMut(&mut Fields<'_, toml::Value>, I) -> (T, K),
) -> (Vec<T>, EntryExtras<K>) {
    let mut entries = Vec::new();
    let mut records = Vec::new();
    let mut rejected = Vec::new();
    root.each_table(key, |root, path, table| {
        let mut fields = root.at(table, path);
        match identify(&mut fields.peeker()) {
            None => rejected.push(fields.into_rest()),
            Some(identity) => {
                let (entry, id) = build(&mut fields, identity);
                records.push((id, fields.into_rest()));
                entries.push(entry);
            }
        }
    });
    let extras = EntryExtras {
        unknown_keys: keep_records(records, toml::Table::is_empty),
        rejected,
    };
    (entries, extras)
}

/// Drops the empty records of identities that occur once.
///
/// When an identity repeats, every occurrence keeps its record (possibly empty), so that
/// matching in order ([`match_in_order`]) gives each record back to its own entry.
pub(crate) fn keep_records<K: Ord, M>(
    records: Vec<(K, M)>,
    is_empty: impl Fn(&M) -> bool,
) -> Vec<(K, M)> {
    let mut counts: BTreeMap<&K, usize> = BTreeMap::new();
    for (id, _) in &records {
        let count = counts.entry(id).or_default();
        *count = count.saturating_add(1);
    }
    let keep: Vec<bool> = records
        .iter()
        .map(|(id, map)| !is_empty(map) || counts.get(id).is_some_and(|&count| count > 1))
        .collect();
    records
        .into_iter()
        .zip(keep)
        .filter_map(|(record, keep)| keep.then_some(record))
        .collect()
}

/// An array of tables on the wire: decoded entries, then the rejected ones verbatim.
pub(crate) struct EntriesWire<'a, W> {
    entries: Vec<W>,
    rejected: &'a [toml::Table],
}

impl<'a, W> EntriesWire<'a, W> {
    /// Writes `entries` followed by `rejected`.
    pub(crate) fn new(entries: Vec<W>, rejected: &'a [toml::Table]) -> Self {
        Self { entries, rejected }
    }

    /// Whether the array would be empty (it is then left out).
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty() && self.rejected.is_empty()
    }
}

impl<W: Serialize> Serialize for EntriesWire<'_, W> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let len = self.entries.len().checked_add(self.rejected.len());
        let mut seq = serializer.serialize_seq(len)?;
        for entry in &self.entries {
            seq.serialize_element(entry)?;
        }
        for table in self.rejected {
            seq.serialize_element(table)?;
        }
        seq.end()
    }
}
