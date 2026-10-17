//! Immutable macro lookup table (plan §4; matrix D47, D48, D50, D52).

use core::fmt;
use std::collections::BTreeMap;

use raumakey_model::macros::{
    Boundary, BoundaryPolicy, CaseMode, CasePolicy, MacroEntry, MacroExpansion, MacroId,
    MacroLookup,
};

use crate::case::{adapt_case, case_pattern};
use crate::error::MacroError;
use crate::validate::{MAX_REPLACEMENT_SCALARS, exceeds_scalars, normalize_entry, to_nfc};

/// Typed text with more scalars than this is never looked up.
///
/// Text that can match a valid trigger has at most 2048 scalars: a trigger has at most
/// [`MAX_TRIGGER_SCALARS`](crate::validate::MAX_TRIGGER_SCALARS) scalars, lowercasing expands a
/// scalar into at most 2, and canonical decomposition into at most 4. The limit therefore never
/// hides a match; it only bounds the work spent on unexpected input.
pub const MAX_LOOKUP_INPUT_SCALARS: usize = 4096;

#[cfg(test)]
thread_local! {
    /// Tables built on the current thread (test instrumentation: snapshot laziness).
    static BUILDS: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// Number of tables built so far on the current thread.
#[cfg(test)]
pub(crate) fn table_builds() -> usize {
    BUILDS.with(core::cell::Cell::get)
}

/// One enabled entry as the lookup needs it.
#[derive(Clone)]
struct Slot {
    id: MacroId,
    replacement: String,
    boundary: BoundaryPolicy,
    case_policy: CasePolicy,
}

/// Immutable, validated macro table; the engine's [`MacroLookup`] implementation.
///
/// Every entry is validated and stored NFC (see [`normalize_entry`]). Only enabled entries are
/// indexed: by exact NFC trigger and by lowercase NFC trigger. A table never changes after it is
/// built, so one `Arc<MacroTable>` can serve concurrent requests as an immutable snapshot.
///
/// Lookup rules:
///
/// 1. The typed text is normalized to NFC (callers should already pass NFC; this is defensive).
/// 2. Only entries whose [`BoundaryPolicy`] allows the boundary are candidates:
///    [`BoundaryPolicy::Whitespace`] entries expand only at [`Boundary::Whitespace`].
/// 3. Entries whose trigger equals the typed text are tried first. Only with
///    [`CaseMode::Adaptive`], entries whose lowercase trigger equals the lowercase typed text are
///    tried next. Within a step the lowest [`MacroId`] wins.
/// 4. The whole typed word must match; a trigger never matches a prefix, suffix, or substring.
/// 5. A trigger typed exactly as stored inserts the replacement as stored, in both case modes.
///    A case-insensitive match (step 3, [`CaseMode::Adaptive`] only) of a [`CasePolicy::Adapt`]
///    entry adapts the replacement to the [`case_pattern`] of the typed text (see
///    [`adapt_case`]); a [`CasePolicy::Exact`] entry is inserted as stored. If adaptation would
///    exceed [`MAX_REPLACEMENT_SCALARS`], the stored text is used, so an expansion never exceeds
///    that limit.
///
/// Rule 5 is a Raumakey decision: adaptation reacts to a case *difference* between the typed
/// text and the stored trigger. A trigger stored as `UBND` therefore inserts `Ủy ban nhân dân`
/// when typed `UBND`, while a trigger stored as `ko` still gives `KHÔNG` for `KO` and `Không`
/// for `Ko`. Case-specific entries such as `VN` and `vn` each insert their own replacement
/// without [`CasePolicy::Exact`].
///
/// The [`fmt::Debug`] output shows only the entry count, never macro text.
///
/// ```
/// use raumakey_macro::MacroTable;
/// use raumakey_model::macros::{
///     Boundary, BoundaryPolicy, CaseMode, CasePolicy, MacroEntry, MacroId, MacroLookup,
/// };
///
/// let table = MacroTable::new(vec![MacroEntry {
///     id: MacroId(1),
///     trigger: "ko".into(),
///     replacement: "không".into(),
///     enabled: true,
///     boundary: BoundaryPolicy::Delimiter,
///     case_policy: CasePolicy::Adapt,
/// }])
/// .unwrap();
/// let hit = table.lookup("KO", Boundary::Whitespace, CaseMode::Adaptive).unwrap();
/// assert_eq!(hit.replacement, "KHÔNG");
/// assert!(table.lookup("kok", Boundary::Whitespace, CaseMode::Adaptive).is_none());
/// ```
#[derive(Clone, Default)]
pub struct MacroTable {
    /// Enabled entries in ascending id order.
    slots: Vec<Slot>,
    /// NFC trigger → slot indices, ascending id.
    exact: BTreeMap<String, Vec<usize>>,
    /// Lowercase NFC trigger → slot indices, ascending id.
    folded: BTreeMap<String, Vec<usize>>,
}

impl MacroTable {
    /// Validates and normalizes every entry (enabled or not) and builds a table.
    ///
    /// Fails with [`MacroError::InvalidEntry`] for the first invalid entry, or with
    /// [`MacroError::DuplicateId`] when two entries share an identifier.
    pub fn new(entries: impl IntoIterator<Item = MacroEntry>) -> Result<Self, MacroError> {
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
        Ok(Self::from_normalized(&normalized))
    }

    /// Builds a table from entries that are already normalized and have unique identifiers.
    pub(crate) fn from_normalized(entries: &[MacroEntry]) -> Self {
        #[cfg(test)]
        BUILDS.with(|builds| builds.set(builds.get().saturating_add(1)));
        let mut enabled: Vec<&MacroEntry> = entries.iter().filter(|e| e.enabled).collect();
        enabled.sort_by_key(|e| e.id);
        let mut table = Self::default();
        for entry in enabled {
            let index = table.slots.len();
            table
                .exact
                .entry(entry.trigger.clone())
                .or_default()
                .push(index);
            table
                .folded
                .entry(fold_case(&entry.trigger))
                .or_default()
                .push(index);
            table.slots.push(Slot {
                id: entry.id,
                replacement: entry.replacement.clone(),
                boundary: entry.boundary,
                case_policy: entry.case_policy,
            });
        }
        table
    }

    /// Number of enabled entries, i.e. entries that can expand.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// True when no entry can expand.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The first candidate (lowest id) whose boundary policy allows `boundary`.
    fn first_eligible(&self, candidates: Option<&Vec<usize>>, boundary: Boundary) -> Option<&Slot> {
        candidates?
            .iter()
            .filter_map(|&index| self.slots.get(index))
            .find(|slot| boundary_allows(slot.boundary, boundary))
    }
}

impl MacroLookup for MacroTable {
    /// Looks up a whole typed word at a boundary; see [`MacroTable`] for the rules.
    fn lookup(
        &self,
        typed: &str,
        boundary: Boundary,
        case_mode: CaseMode,
    ) -> Option<MacroExpansion> {
        if typed.is_empty()
            || self.slots.is_empty()
            || exceeds_scalars(typed, MAX_LOOKUP_INPUT_SCALARS)
        {
            return None;
        }
        let typed = to_nfc(typed);
        // `folded` is true only for a case-insensitive match, i.e. when the typed case differs
        // from the stored trigger; only such a match adapts the replacement.
        let (slot, folded) = match self.first_eligible(self.exact.get(&typed), boundary) {
            Some(slot) => (slot, false),
            None if case_mode == CaseMode::Adaptive => (
                self.first_eligible(self.folded.get(&fold_case(&typed)), boundary)?,
                true,
            ),
            None => return None,
        };
        let replacement = if folded && slot.case_policy == CasePolicy::Adapt {
            let adapted = adapt_case(&slot.replacement, case_pattern(&typed));
            if exceeds_scalars(&adapted, MAX_REPLACEMENT_SCALARS) {
                slot.replacement.clone()
            } else {
                adapted
            }
        } else {
            slot.replacement.clone()
        };
        Some(MacroExpansion {
            id: slot.id,
            replacement,
        })
    }
}

impl fmt::Debug for MacroTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroTable")
            .field("len", &self.slots.len())
            .finish_non_exhaustive()
    }
}

/// Whether an entry with `policy` may expand at `boundary`.
fn boundary_allows(policy: BoundaryPolicy, boundary: Boundary) -> bool {
    match policy {
        BoundaryPolicy::Delimiter => true,
        BoundaryPolicy::Whitespace => boundary == Boundary::Whitespace,
    }
}

/// Case-insensitive key of NFC text: its lowercase form, renormalized to NFC.
fn fold_case(nfc: &str) -> String {
    to_nfc(&nfc.to_lowercase())
}

/// The smallest identifier used by more than one entry.
pub(crate) fn first_duplicate_id(entries: &[MacroEntry]) -> Option<MacroId> {
    let mut ids: Vec<MacroId> = entries.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    ids.windows(2).find_map(|pair| match pair {
        [a, b] if a == b => Some(*a),
        _ => None,
    })
}
