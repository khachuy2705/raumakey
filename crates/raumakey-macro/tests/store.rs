//! Macro store: optimistic revisions, id allocation, snapshots (plan §4).

use std::sync::Arc;

use raumakey_macro::{EntryError, EntryIssue, MacroError, MacroStore, parse_legacy_text};
use raumakey_model::macros::{
    Boundary, BoundaryPolicy, CaseMode, CasePolicy, MacroEntry, MacroExpansion, MacroId,
    MacroLookup,
};

fn entry(id: u64, trigger: &str, replacement: &str) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    }
}

fn ids(store: &MacroStore) -> Vec<u64> {
    store.list().iter().map(|e| e.id.0).collect()
}

fn store_with(revision: u64, entries: Vec<MacroEntry>) -> MacroStore {
    MacroStore::from_entries(revision, entries).unwrap()
}

#[test]
fn new_store_is_empty_at_revision_zero() {
    let store = MacroStore::new();
    assert_eq!(store.revision(), 0);
    assert!(store.is_empty());
    assert_eq!(store.len(), 0);
    assert!(store.list().is_empty());
    assert_eq!(store.next_id(), Ok(MacroId(1)));
}

#[test]
fn upsert_of_an_unknown_id_inserts_with_an_allocated_id() {
    let mut store = MacroStore::new();
    assert_eq!(store.upsert(0, entry(0, "ko", "không")), Ok(1));
    assert_eq!(store.upsert(1, entry(77, "dc", "được")), Ok(2));
    assert_eq!(ids(&store), [1, 2]);
    assert_eq!(store.get(MacroId(2)).unwrap().trigger, "dc");
    assert_eq!(store.get(MacroId(77)), None);
    assert_eq!(store.revision(), 2);
}

/// D29: creating a macro whose trigger has more than 32 scalars fails with a typed error and
/// changes nothing.
#[test]
fn d29_creating_a_macro_with_a_trigger_longer_than_32_scalars_fails() {
    let mut store = MacroStore::new();
    assert_eq!(
        store.upsert(0, entry(0, &"b".repeat(33), "x")),
        Err(MacroError::InvalidEntry {
            id: MacroId(0),
            reason: EntryError::TriggerTooLong,
        })
    );
    assert!(store.is_empty());
    assert_eq!(store.revision(), 0);
    assert_eq!(store.upsert(0, entry(0, &"b".repeat(32), "x")), Ok(1));
}

/// D30: creating a macro whose replacement does not fit one insert fails with a typed error.
#[test]
fn d30_creating_a_macro_with_a_replacement_beyond_the_insert_limit_fails() {
    let mut store = MacroStore::new();
    assert_eq!(
        store.upsert(0, entry(0, "ko", &"ệ".repeat(2731))),
        Err(MacroError::InvalidEntry {
            id: MacroId(0),
            reason: EntryError::ReplacementTooLong,
        })
    );
    assert!(store.is_empty());
    assert_eq!(
        store.upsert(0, entry(0, "ko", &format!("{}a", "ệ".repeat(2730)))),
        Ok(1)
    );
}

#[test]
fn upsert_of_a_known_id_replaces_the_entry_in_place() {
    let mut store = store_with(
        4,
        vec![entry(1, "a", "x"), entry(2, "b", "y"), entry(3, "c", "z")],
    );
    let replacement = MacroEntry {
        enabled: false,
        ..entry(2, "bb", "yy")
    };
    assert_eq!(store.upsert(4, replacement.clone()), Ok(5));
    assert_eq!(ids(&store), [1, 2, 3]);
    assert_eq!(store.get(MacroId(2)), Some(&replacement));
    assert_eq!(store.len(), 3);
}

#[test]
fn stale_revisions_conflict_and_change_nothing() {
    let mut store = store_with(1, vec![entry(1, "ko", "không")]);
    assert_eq!(
        store.upsert(0, entry(1, "ko", "khác")),
        Err(MacroError::Conflict {
            expected: 0,
            actual: 1
        })
    );
    assert_eq!(
        store.insert(2, entry(0, "dc", "được")),
        Err(MacroError::Conflict {
            expected: 2,
            actual: 1
        })
    );
    assert_eq!(
        store.remove(7, MacroId(1)),
        Err(MacroError::Conflict {
            expected: 7,
            actual: 1
        })
    );
    assert_eq!(store.revision(), 1);
    assert_eq!(store.list(), [entry(1, "ko", "không")]);
}

#[test]
fn two_writers_on_the_same_revision_exactly_one_succeeds() {
    let mut store = store_with(3, vec![entry(1, "ko", "không")]);
    let seen_by_both = store.revision();
    assert_eq!(store.upsert(seen_by_both, entry(1, "ko", "first")), Ok(4));
    assert_eq!(
        store.upsert(seen_by_both, entry(1, "ko", "second")),
        Err(MacroError::Conflict {
            expected: 3,
            actual: 4
        })
    );
    assert_eq!(store.get(MacroId(1)).unwrap().replacement, "first");
}

#[test]
fn upsert_validates_and_stores_nfc() {
    let mut store = MacroStore::new();
    assert_eq!(
        store.upsert(0, entry(5, "k o", "x")),
        Err(MacroError::InvalidEntry {
            id: MacroId(5),
            reason: EntryError::TriggerWhitespace,
        })
    );
    assert_eq!(store.revision(), 0);
    assert!(store.is_empty());

    assert_eq!(
        store.upsert(
            0,
            entry(0, "vie\u{0302}\u{0323}t", "Vie\u{0302}\u{0323}t Nam")
        ),
        Ok(1)
    );
    let stored = store.get(MacroId(1)).unwrap();
    assert_eq!(stored.trigger, "việt");
    assert_eq!(stored.replacement, "Việt Nam");
}

#[test]
fn insert_always_allocates_a_new_id() {
    let mut store = store_with(0, vec![entry(1, "ko", "không")]);
    assert_eq!(store.insert(0, entry(1, "dc", "được")), Ok((1, MacroId(2))));
    assert_eq!(store.get(MacroId(1)).unwrap().trigger, "ko");
    assert_eq!(store.get(MacroId(2)).unwrap().trigger, "dc");
}

#[test]
fn insert_rejects_invalid_entries() {
    let mut store = MacroStore::new();
    assert_eq!(
        store.insert(0, entry(9, "ko", "")),
        Err(MacroError::InvalidEntry {
            id: MacroId(9),
            reason: EntryError::EmptyReplacement,
        })
    );
    assert_eq!(store.revision(), 0);
}

#[test]
fn remove_deletes_and_bumps_the_revision() {
    let mut store = store_with(
        10,
        vec![entry(1, "a", "x"), entry(2, "b", "y"), entry(3, "c", "z")],
    );
    assert_eq!(store.remove(10, MacroId(2)), Ok(11));
    assert_eq!(ids(&store), [1, 3]);
    assert_eq!(
        store.remove(11, MacroId(2)),
        Err(MacroError::NotFound(MacroId(2)))
    );
    assert_eq!(store.revision(), 11);
}

#[test]
fn new_ids_are_max_plus_one() {
    let mut store = store_with(0, vec![entry(5, "a", "x"), entry(2, "b", "y")]);
    assert_eq!(store.next_id(), Ok(MacroId(6)));
    let (revision, id) = store.insert(0, entry(0, "c", "z")).unwrap();
    assert_eq!((revision, id), (1, MacroId(6)));
    store.remove(1, MacroId(6)).unwrap();
    assert_eq!(store.next_id(), Ok(MacroId(6)));
    store.remove(2, MacroId(5)).unwrap();
    assert_eq!(store.next_id(), Ok(MacroId(3)));
}

#[test]
fn exhausted_id_space_is_an_error_not_a_panic() {
    let mut store = store_with(0, vec![entry(u64::MAX, "a", "x")]);
    assert_eq!(store.next_id(), Err(MacroError::IdSpaceExhausted));
    assert_eq!(
        store.insert(0, entry(0, "b", "y")),
        Err(MacroError::IdSpaceExhausted)
    );
    assert_eq!(
        store.upsert(0, entry(1, "b", "y")),
        Err(MacroError::IdSpaceExhausted)
    );
    assert_eq!(store.revision(), 0);
    // Replacing the existing entry needs no new id.
    assert_eq!(store.upsert(0, entry(u64::MAX, "a", "xx")), Ok(1));
}

#[test]
fn exhausted_revision_is_an_error_not_a_panic() {
    let mut store = store_with(u64::MAX, vec![entry(1, "a", "x")]);
    assert_eq!(
        store.upsert(u64::MAX, entry(1, "a", "y")),
        Err(MacroError::RevisionExhausted)
    );
    assert_eq!(
        store.remove(u64::MAX, MacroId(1)),
        Err(MacroError::RevisionExhausted)
    );
    assert_eq!(
        store.insert(u64::MAX, entry(0, "b", "y")),
        Err(MacroError::RevisionExhausted)
    );
    assert_eq!(
        store.insert_many(u64::MAX, vec![entry(0, "b", "y")]),
        Err(MacroError::RevisionExhausted)
    );
    assert_eq!(store.get(MacroId(1)).unwrap().replacement, "x");
    assert_eq!(ids(&store), [1]);
    assert_eq!(store.revision(), u64::MAX);
}

#[test]
fn insert_many_adds_every_entry_in_one_revision() {
    let mut store = store_with(5, vec![entry(1, "a", "x"), entry(4, "b", "y")]);
    assert_eq!(
        store.insert_many(
            5,
            vec![
                entry(0, "ko", "không"),
                entry(9, "dc", "được"),
                entry(9, "vn", "Việt Nam"),
            ]
        ),
        Ok((6, vec![MacroId(5), MacroId(6), MacroId(7)]))
    );
    assert_eq!(store.revision(), 6);
    assert_eq!(ids(&store), [1, 4, 5, 6, 7]);
    assert_eq!(store.get(MacroId(6)).unwrap().trigger, "dc");
    assert_eq!(store.next_id(), Ok(MacroId(8)));
    let table = store.snapshot();
    assert_eq!(table.len(), 5);
    assert_eq!(
        table
            .lookup("vn", Boundary::Whitespace, CaseMode::Exact)
            .map(|e| e.id),
        Some(MacroId(7))
    );
}

#[test]
fn insert_many_validates_every_entry_before_changing_anything() {
    let mut store = store_with(2, vec![entry(1, "a", "x")]);
    let before = store.snapshot();
    assert_eq!(
        store.insert_many(
            2,
            vec![entry(7, "ok", "x"), entry(8, "k o", "y"), entry(9, "", "z")]
        ),
        Err(MacroError::InvalidEntry {
            id: MacroId(8),
            reason: EntryError::TriggerWhitespace,
        })
    );
    assert_eq!(store.revision(), 2);
    assert_eq!(ids(&store), [1]);
    assert!(Arc::ptr_eq(&before, &store.snapshot()));
}

#[test]
fn insert_many_stores_nfc() {
    let mut store = MacroStore::new();
    store
        .insert_many(
            0,
            vec![entry(0, "vie\u{0302}\u{0323}t", "Vie\u{0302}\u{0323}t Nam")],
        )
        .unwrap();
    let stored = store.get(MacroId(1)).unwrap();
    assert_eq!(stored.trigger, "việt");
    assert_eq!(stored.replacement, "Việt Nam");
}

#[test]
fn insert_many_reports_errors_in_the_documented_order() {
    // Conflict before invalid entries.
    let mut store = store_with(3, vec![]);
    assert_eq!(
        store.insert_many(2, vec![entry(0, "k o", "x")]),
        Err(MacroError::Conflict {
            expected: 2,
            actual: 3
        })
    );
    // Invalid entries before an exhausted revision.
    let mut store = store_with(u64::MAX, vec![]);
    assert_eq!(
        store.insert_many(u64::MAX, vec![entry(4, "", "x")]),
        Err(MacroError::InvalidEntry {
            id: MacroId(4),
            reason: EntryError::EmptyTrigger,
        })
    );
    // An exhausted revision before an exhausted id space.
    let mut store = store_with(u64::MAX, vec![entry(u64::MAX, "a", "x")]);
    assert_eq!(
        store.insert_many(u64::MAX, vec![entry(0, "b", "y")]),
        Err(MacroError::RevisionExhausted)
    );
}

#[test]
fn insert_many_fails_when_the_ids_do_not_fit() {
    let mut store = store_with(0, vec![entry(u64::MAX - 1, "a", "x")]);
    assert_eq!(
        store.insert_many(0, vec![entry(0, "b", "y"), entry(0, "c", "z")]),
        Err(MacroError::IdSpaceExhausted)
    );
    assert_eq!(store.revision(), 0);
    assert_eq!(store.len(), 1);
    assert_eq!(
        store.insert_many(0, vec![entry(0, "b", "y")]),
        Ok((1, vec![MacroId(u64::MAX)]))
    );
    assert_eq!(
        store.insert_many(1, vec![entry(0, "c", "z")]),
        Err(MacroError::IdSpaceExhausted)
    );
}

#[test]
fn insert_many_of_nothing_changes_nothing() {
    let mut store = store_with(3, vec![entry(1, "a", "x")]);
    let before = store.snapshot();
    assert_eq!(store.insert_many(3, Vec::new()), Ok((3, Vec::new())));
    assert_eq!(store.revision(), 3);
    assert!(Arc::ptr_eq(&before, &store.snapshot()));
    assert_eq!(
        store.insert_many(2, Vec::new()),
        Err(MacroError::Conflict {
            expected: 2,
            actual: 3
        })
    );
    // No revision is needed for an empty batch.
    let mut full = store_with(u64::MAX, vec![]);
    assert_eq!(
        full.insert_many(u64::MAX, Vec::new()),
        Ok((u64::MAX, Vec::new()))
    );
}

#[test]
fn a_legacy_import_merges_into_a_store_in_one_revision() {
    let mut store = store_with(7, vec![entry(1, "ko", "không")]);
    let import = parse_legacy_text("vn:Việt Nam\ndc:được\n".as_bytes());
    assert_eq!(
        store.insert_many(7, import.entries),
        Ok((8, vec![MacroId(2), MacroId(3)]))
    );
    assert_eq!(
        store
            .snapshot()
            .lookup("VN", Boundary::Whitespace, CaseMode::Adaptive)
            .map(|e| e.replacement),
        Some("VIỆT NAM".to_owned())
    );
}

#[test]
fn ids_stay_max_plus_one_across_every_kind_of_change() {
    let mut store = store_with(0, vec![entry(3, "a", "x")]);
    let (revision, ids) = store
        .insert_many(0, vec![entry(0, "b", "y"), entry(0, "c", "z")])
        .unwrap();
    assert_eq!(ids, [MacroId(4), MacroId(5)]);
    let revision = store.remove(revision, MacroId(5)).unwrap();
    assert_eq!(store.next_id(), Ok(MacroId(5)));
    let revision = store.upsert(revision, entry(4, "b", "changed")).unwrap();
    assert_eq!(store.next_id(), Ok(MacroId(5)));
    let revision = store.remove(revision, MacroId(3)).unwrap();
    assert_eq!(store.next_id(), Ok(MacroId(5)));
    let revision = store.remove(revision, MacroId(4)).unwrap();
    assert_eq!(store.next_id(), Ok(MacroId(1)));
    assert_eq!(
        store.insert(revision, entry(0, "d", "w")),
        Ok((revision + 1, MacroId(1)))
    );
}

#[test]
fn from_entries_keeps_revision_order_and_metadata() {
    let input = vec![
        entry(3, "c", "z"),
        MacroEntry {
            enabled: false,
            boundary: BoundaryPolicy::Whitespace,
            case_policy: CasePolicy::Exact,
            ..entry(1, "a", "x")
        },
        entry(2, "b", "y"),
    ];
    let store = store_with(42, input.clone());
    assert_eq!(store.revision(), 42);
    assert_eq!(store.list(), input.as_slice());
    assert_eq!(store.entries(), input);
    assert_eq!(store.len(), 3);
}

#[test]
fn from_entries_normalizes_to_nfc() {
    let store = store_with(1, vec![entry(1, "vie\u{0302}\u{0323}t", "x")]);
    assert_eq!(store.list()[0].trigger, "việt");
}

#[test]
fn from_entries_rejects_invalid_entries_and_duplicate_ids() {
    assert_eq!(
        MacroStore::from_entries(1, vec![entry(1, "a", "x"), entry(2, "", "y")]).err(),
        Some(MacroError::InvalidEntry {
            id: MacroId(2),
            reason: EntryError::EmptyTrigger,
        })
    );
    assert_eq!(
        MacroStore::from_entries(1, vec![entry(4, "a", "x"), entry(4, "b", "y")]).err(),
        Some(MacroError::DuplicateId(MacroId(4)))
    );
}

/// D29 and D30 tightened the limits, so data stored before them can hold entries that are
/// invalid now: a 40-scalar trigger, or a replacement that no longer fits one insert. Loading
/// user data therefore goes through `from_entries_lenient`, which reports them by input
/// position while every valid entry keeps working; `from_entries` refuses the whole list.
#[test]
fn d29_d30_entries_stored_under_the_old_limits_load_leniently_as_issues() {
    let stored = vec![
        entry(1, "ko", "không"),
        entry(2, &"b".repeat(40), "x"),
        entry(3, "big", &"\u{1F600}".repeat(4096)),
        entry(4, "vn", "Việt Nam"),
    ];
    assert_eq!(
        MacroStore::from_entries(7, stored.clone()).err(),
        Some(MacroError::InvalidEntry {
            id: MacroId(2),
            reason: EntryError::TriggerTooLong,
        })
    );
    let (store, issues) = MacroStore::from_entries_lenient(7, stored);
    assert_eq!(
        issues,
        [
            EntryIssue {
                index: 1,
                error: MacroError::InvalidEntry {
                    id: MacroId(2),
                    reason: EntryError::TriggerTooLong,
                },
            },
            EntryIssue {
                index: 2,
                error: MacroError::InvalidEntry {
                    id: MacroId(3),
                    reason: EntryError::ReplacementTooLong,
                },
            },
        ]
    );
    assert_eq!(store.revision(), 7);
    assert_eq!(ids(&store), [1, 4]);
    let table = store.snapshot();
    for (typed, replacement) in [("ko", "không"), ("vn", "Việt Nam")] {
        assert_eq!(
            table
                .lookup(typed, Boundary::Whitespace, CaseMode::Exact)
                .map(|expansion| expansion.replacement),
            Some(replacement.to_owned()),
            "{typed}"
        );
    }
}

#[test]
fn lenient_loading_skips_bad_entries_and_reports_their_positions() {
    let (store, issues) = MacroStore::from_entries_lenient(
        9,
        vec![
            entry(1, "a", "x"),
            entry(2, "b c", "y"),
            entry(1, "d", "z"),
            entry(3, "e", "w"),
        ],
    );
    assert_eq!(store.revision(), 9);
    assert_eq!(ids(&store), [1, 3]);
    assert_eq!(store.get(MacroId(1)).unwrap().trigger, "a");
    assert_eq!(
        issues,
        [
            EntryIssue {
                index: 1,
                error: MacroError::InvalidEntry {
                    id: MacroId(2),
                    reason: EntryError::TriggerWhitespace,
                },
            },
            EntryIssue {
                index: 2,
                error: MacroError::DuplicateId(MacroId(1)),
            },
        ]
    );
}

#[test]
fn snapshot_follows_the_store_and_old_snapshots_stay_unchanged() {
    let mut store = MacroStore::new();
    let empty = store.snapshot();
    store.upsert(0, entry(0, "ko", "không")).unwrap();
    let first = store.snapshot();
    assert!(
        empty
            .lookup("ko", Boundary::Whitespace, CaseMode::Exact)
            .is_none()
    );
    assert_eq!(
        first.lookup("ko", Boundary::Whitespace, CaseMode::Exact),
        Some(MacroExpansion {
            id: MacroId(1),
            replacement: "không".into()
        })
    );
    store.remove(1, MacroId(1)).unwrap();
    assert!(
        first
            .lookup("ko", Boundary::Whitespace, CaseMode::Exact)
            .is_some()
    );
    assert!(
        store
            .snapshot()
            .lookup("ko", Boundary::Whitespace, CaseMode::Exact)
            .is_none()
    );
}

#[test]
fn snapshot_excludes_disabled_entries() {
    let store = store_with(
        1,
        vec![
            MacroEntry {
                enabled: false,
                ..entry(1, "ko", "không")
            },
            entry(2, "dc", "được"),
        ],
    );
    let table = store.snapshot();
    assert_eq!(table.len(), 1);
    assert!(
        table
            .lookup("ko", Boundary::Whitespace, CaseMode::Exact)
            .is_none()
    );
}

#[test]
fn snapshot_is_shared_until_the_next_change() {
    let mut store = store_with(1, vec![entry(1, "ko", "không")]);
    assert!(Arc::ptr_eq(&store.snapshot(), &store.snapshot()));
    let before = store.snapshot();
    store.upsert(1, entry(1, "ko", "khong")).unwrap();
    assert!(!Arc::ptr_eq(&before, &store.snapshot()));
}

#[test]
fn snapshot_is_usable_as_the_engine_lookup_port() {
    let store = store_with(1, vec![entry(1, "ko", "không")]);
    let port: Arc<dyn MacroLookup> = store.snapshot();
    assert_eq!(
        port.lookup("KO", Boundary::Punctuation, CaseMode::Adaptive)
            .map(|e| e.replacement),
        Some("KHÔNG".to_owned())
    );
}

#[test]
fn entries_round_trip_through_from_entries() {
    let mut store = MacroStore::new();
    store.upsert(0, entry(0, "ko", "không")).unwrap();
    store.upsert(1, entry(0, "dc", "được")).unwrap();
    let reloaded = MacroStore::from_entries(store.revision(), store.entries()).unwrap();
    assert_eq!(reloaded.list(), store.list());
    assert_eq!(reloaded.revision(), store.revision());
    assert_eq!(reloaded.next_id(), store.next_id());
}

#[test]
fn debug_output_does_not_reveal_macro_text() {
    let store = store_with(3, vec![entry(1, "bimat", "noidung")]);
    let text = format!("{store:?}");
    assert!(!text.contains("bimat"), "{text}");
    assert!(!text.contains("noidung"), "{text}");
    assert!(text.contains('3'), "{text}");
}

#[test]
fn d29_d30_new_ids_never_reuse_an_entry_skipped_by_lenient_loading() {
    // The caller writes skipped entries back verbatim, so their identifiers stay taken.
    let stored = vec![
        entry(1, "ko", "không"),
        entry(5, &"b".repeat(40), "x"),
        entry(3, "vn", "Việt Nam"),
    ];
    let (mut store, issues) = MacroStore::from_entries_lenient(2, stored);
    assert_eq!(issues.len(), 1);
    assert_eq!(ids(&store), [1, 3]);
    assert_eq!(store.next_id(), Ok(MacroId(6)));

    let revision = store.upsert(2, entry(0, "hn", "Hà Nội")).unwrap();
    assert_eq!(ids(&store), [1, 3, 6]);
    let revision = store.remove(revision, MacroId(6)).unwrap();
    let revision = store.remove(revision, MacroId(3)).unwrap();
    assert_eq!(
        store.next_id(),
        Ok(MacroId(6)),
        "removing kept entries never frees id 5"
    );
    let (_, id) = store.insert(revision, entry(0, "sg", "Sài Gòn")).unwrap();
    assert_eq!(id, MacroId(6));
}
