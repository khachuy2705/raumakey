//! Macro table lookup (plan §4; matrix D47, D48, D50, D52).

use std::sync::Arc;

use raumakey_macro::{EntryError, MAX_REPLACEMENT_SCALARS, MacroError, MacroTable};
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

fn table(entries: Vec<MacroEntry>) -> MacroTable {
    MacroTable::new(entries).unwrap()
}

fn expansion(id: u64, replacement: &str) -> Option<MacroExpansion> {
    Some(MacroExpansion {
        id: MacroId(id),
        replacement: replacement.to_owned(),
    })
}

fn ws(t: &MacroTable, typed: &str, mode: CaseMode) -> Option<MacroExpansion> {
    t.lookup(typed, Boundary::Whitespace, mode)
}

#[test]
fn d47_trigger_expands_at_whitespace_boundary() {
    let t = table(vec![entry(1, "ko", "không")]);
    assert_eq!(ws(&t, "ko", CaseMode::Adaptive), expansion(1, "không"));
    assert_eq!(ws(&t, "ko", CaseMode::Exact), expansion(1, "không"));
}

#[test]
fn d47_delimiter_entries_expand_at_punctuation() {
    let t = table(vec![entry(1, "ko", "không")]);
    assert_eq!(
        t.lookup("ko", Boundary::Punctuation, CaseMode::Adaptive),
        expansion(1, "không")
    );
}

#[test]
fn whitespace_policy_entries_expand_only_at_whitespace() {
    let t = table(vec![MacroEntry {
        boundary: BoundaryPolicy::Whitespace,
        ..entry(1, "ko", "không")
    }]);
    assert_eq!(
        t.lookup("ko", Boundary::Punctuation, CaseMode::Adaptive),
        None
    );
    assert_eq!(
        t.lookup("ko", Boundary::Whitespace, CaseMode::Adaptive),
        expansion(1, "không")
    );
}

#[test]
fn boundary_eligibility_is_checked_before_the_tie_break() {
    let t = table(vec![
        MacroEntry {
            boundary: BoundaryPolicy::Whitespace,
            ..entry(1, "ko", "một")
        },
        entry(2, "ko", "hai"),
    ]);
    assert_eq!(
        t.lookup("ko", Boundary::Whitespace, CaseMode::Exact),
        expansion(1, "một")
    );
    assert_eq!(
        t.lookup("ko", Boundary::Punctuation, CaseMode::Exact),
        expansion(2, "hai")
    );
}

#[test]
fn d48_lookup_matches_whole_words_only() {
    let t = table(vec![entry(1, "ko", "không")]);
    // The whole word matches...
    assert_eq!(ws(&t, "ko", CaseMode::Adaptive), expansion(1, "không"));
    assert_eq!(ws(&t, "ko", CaseMode::Exact), expansion(1, "không"));
    // ...but a word that merely contains, starts, or ends with the trigger does not.
    for typed in ["ako", "kok", "k", "o", "koko", "ko ", " ko", "xkoy", "Ako"] {
        assert_eq!(ws(&t, typed, CaseMode::Adaptive), None, "{typed:?}");
        assert_eq!(ws(&t, typed, CaseMode::Exact), None, "{typed:?}");
    }
}

#[test]
fn d50_case_detection_ignores_digits_and_punctuation_in_the_typed_trigger() {
    let t = table(vec![
        entry(1, "2k", "hai nghìn"),
        entry(2, "t.p", "thành phố"),
        entry(3, "k2o", "không hai"),
    ]);
    assert_eq!(ws(&t, "2K", CaseMode::Adaptive), expansion(1, "Hai nghìn"));
    assert_eq!(ws(&t, "2k", CaseMode::Adaptive), expansion(1, "hai nghìn"));
    assert_eq!(ws(&t, "T.P", CaseMode::Adaptive), expansion(2, "THÀNH PHỐ"));
    assert_eq!(ws(&t, "T.p", CaseMode::Adaptive), expansion(2, "Thành phố"));
    assert_eq!(ws(&t, "K2O", CaseMode::Adaptive), expansion(3, "KHÔNG HAI"));
    assert_eq!(ws(&t, "k2O", CaseMode::Adaptive), expansion(3, "không hai"));
    assert_eq!(ws(&t, "2K", CaseMode::Exact), None);
}

#[test]
fn disabled_entries_never_expand() {
    let t = table(vec![MacroEntry {
        enabled: false,
        ..entry(1, "ko", "không")
    }]);
    assert_eq!(ws(&t, "ko", CaseMode::Adaptive), None);
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
}

#[test]
fn len_counts_enabled_entries() {
    let t = table(vec![
        entry(1, "a", "x"),
        MacroEntry {
            enabled: false,
            ..entry(2, "b", "y")
        },
        entry(3, "c", "z"),
    ]);
    assert_eq!(t.len(), 2);
    assert!(!t.is_empty());
}

#[test]
fn exact_mode_does_not_fold_case() {
    let t = table(vec![entry(1, "ko", "không")]);
    assert_eq!(ws(&t, "KO", CaseMode::Exact), None);
    assert_eq!(ws(&t, "Ko", CaseMode::Exact), None);
}

#[test]
fn d50_adaptive_mode_folds_case_and_adapts_the_replacement() {
    let t = table(vec![entry(1, "ko", "không")]);
    assert_eq!(ws(&t, "ko", CaseMode::Adaptive), expansion(1, "không"));
    assert_eq!(ws(&t, "Ko", CaseMode::Adaptive), expansion(1, "Không"));
    assert_eq!(ws(&t, "KO", CaseMode::Adaptive), expansion(1, "KHÔNG"));
    assert_eq!(ws(&t, "kO", CaseMode::Adaptive), expansion(1, "không"));
}

#[test]
fn d50_vietnamese_triggers_fold_case() {
    let t = table(vec![
        entry(1, "đc", "địa chỉ"),
        entry(2, "k", "không"),
        entry(3, "vn", "Việt Nam"),
    ]);
    assert_eq!(ws(&t, "ĐC", CaseMode::Adaptive), expansion(1, "ĐỊA CHỈ"));
    assert_eq!(ws(&t, "Đc", CaseMode::Adaptive), expansion(1, "Địa chỉ"));
    assert_eq!(ws(&t, "K", CaseMode::Adaptive), expansion(2, "Không"));
    assert_eq!(ws(&t, "VN", CaseMode::Adaptive), expansion(3, "VIỆT NAM"));
    assert_eq!(ws(&t, "Vn", CaseMode::Adaptive), expansion(3, "Việt Nam"));
}

#[test]
fn exact_trigger_match_wins_over_case_insensitive_match() {
    // Both entries use the default case policy: case-specific entries need no CasePolicy::Exact.
    let t = table(vec![entry(1, "vn", "văn nghệ"), entry(2, "VN", "Việt Nam")]);
    assert_eq!(ws(&t, "VN", CaseMode::Adaptive), expansion(2, "Việt Nam"));
    assert_eq!(ws(&t, "vn", CaseMode::Adaptive), expansion(1, "văn nghệ"));
    // No exact entry for "Vn": both fold to "vn" and the lowest id wins.
    assert_eq!(ws(&t, "Vn", CaseMode::Adaptive), expansion(1, "Văn nghệ"));
}

#[test]
fn uppercase_trigger_typed_as_stored_inserts_the_stored_replacement() {
    // Raumakey decision: the replacement is adapted only when the typed case differs from the
    // stored trigger, so an acronym trigger typed as defined is not shouted.
    let t = table(vec![entry(1, "UBND", "Ủy ban nhân dân")]);
    assert_eq!(
        ws(&t, "UBND", CaseMode::Adaptive),
        expansion(1, "Ủy ban nhân dân")
    );
    assert_eq!(
        ws(&t, "UBND", CaseMode::Exact),
        expansion(1, "Ủy ban nhân dân")
    );
    // Typed in another case, the typed case pattern still applies.
    assert_eq!(
        ws(&t, "ubnd", CaseMode::Adaptive),
        expansion(1, "Ủy ban nhân dân")
    );
    assert_eq!(
        ws(&t, "Ubnd", CaseMode::Adaptive),
        expansion(1, "Ủy ban nhân dân")
    );
}

#[test]
fn vietnamese_acronym_triggers_typed_as_stored_keep_the_stored_replacement() {
    let t = table(vec![
        entry(1, "HĐND", "Hội đồng nhân dân"),
        entry(2, "TP", "thành phố"),
    ]);
    assert_eq!(
        ws(&t, "HĐND", CaseMode::Adaptive),
        expansion(1, "Hội đồng nhân dân")
    );
    assert_eq!(ws(&t, "TP", CaseMode::Adaptive), expansion(2, "thành phố"));
    assert_eq!(ws(&t, "Tp", CaseMode::Adaptive), expansion(2, "Thành phố"));
    assert_eq!(ws(&t, "tp", CaseMode::Adaptive), expansion(2, "thành phố"));
}

#[test]
fn title_case_trigger_typed_as_stored_inserts_the_stored_replacement() {
    let t = table(vec![entry(1, "Ko", "không")]);
    assert_eq!(ws(&t, "Ko", CaseMode::Adaptive), expansion(1, "không"));
    assert_eq!(ws(&t, "KO", CaseMode::Adaptive), expansion(1, "KHÔNG"));
    assert_eq!(ws(&t, "ko", CaseMode::Adaptive), expansion(1, "không"));
}

#[test]
fn ties_resolve_to_the_lowest_id_regardless_of_order() {
    let t = table(vec![
        entry(5, "ko", "five"),
        entry(3, "ko", "three"),
        entry(9, "KO", "nine"),
    ]);
    assert_eq!(ws(&t, "ko", CaseMode::Exact), expansion(3, "three"));
    assert_eq!(ws(&t, "Ko", CaseMode::Adaptive), expansion(3, "Three"));
    // "KO" matches entry 9 exactly, so it is inserted as stored.
    assert_eq!(ws(&t, "KO", CaseMode::Adaptive), expansion(9, "nine"));
}

#[test]
fn exact_case_policy_inserts_as_stored() {
    let t = table(vec![MacroEntry {
        case_policy: CasePolicy::Exact,
        ..entry(1, "ko", "không")
    }]);
    assert_eq!(ws(&t, "KO", CaseMode::Adaptive), expansion(1, "không"));
    assert_eq!(ws(&t, "Ko", CaseMode::Adaptive), expansion(1, "không"));
}

#[test]
fn exact_mode_inserts_as_stored() {
    let t = table(vec![entry(1, "Ko", "không")]);
    assert_eq!(ws(&t, "Ko", CaseMode::Exact), expansion(1, "không"));
}

#[test]
fn d52_decomposed_typed_text_matches_a_composed_trigger() {
    let t = table(vec![entry(1, "việt", "Việt Nam")]);
    let typed = "vie\u{0302}\u{0323}t";
    assert_eq!(ws(&t, typed, CaseMode::Exact), expansion(1, "Việt Nam"));
    assert_eq!(ws(&t, typed, CaseMode::Adaptive), expansion(1, "Việt Nam"));
    assert_eq!(
        ws(&t, "VIE\u{0302}\u{0323}T", CaseMode::Adaptive),
        expansion(1, "VIỆT NAM")
    );
}

#[test]
fn triggers_equal_only_after_nfc_tie_break_to_the_lowest_id() {
    // One trigger is given decomposed, the other composed; both are stored as "việt", so they
    // collide and the lowest identifier wins whatever the input order and form.
    for (nfd_id, nfc_id) in [(3, 7), (7, 3)] {
        let lowest = nfd_id.min(nfc_id);
        let replacement = |id: u64| format!("id {id}");
        let t = table(vec![
            entry(nfc_id, "việt", &replacement(nfc_id)),
            entry(nfd_id, "vie\u{0302}\u{0323}t", &replacement(nfd_id)),
        ]);
        for typed in ["việt", "vie\u{0302}\u{0323}t"] {
            assert_eq!(
                ws(&t, typed, CaseMode::Exact),
                expansion(lowest, &replacement(lowest))
            );
            assert_eq!(
                ws(&t, typed, CaseMode::Adaptive),
                expansion(lowest, &replacement(lowest))
            );
        }
        assert_eq!(
            ws(&t, "VIỆT", CaseMode::Adaptive),
            expansion(lowest, &replacement(lowest).to_uppercase())
        );
    }
}

#[test]
fn d52_decomposed_entries_are_stored_composed() {
    let t = table(vec![entry(
        1,
        "vie\u{0302}\u{0323}t",
        "Vie\u{0302}\u{0323}t Nam",
    )]);
    assert_eq!(ws(&t, "việt", CaseMode::Exact), expansion(1, "Việt Nam"));
}

#[test]
fn new_rejects_invalid_entries_with_their_id() {
    let result = MacroTable::new(vec![entry(1, "ok", "x"), entry(4, "k o", "x")]);
    assert_eq!(
        result.err(),
        Some(MacroError::InvalidEntry {
            id: MacroId(4),
            reason: EntryError::TriggerWhitespace,
        })
    );
}

#[test]
fn new_validates_disabled_entries_too() {
    let result = MacroTable::new(vec![MacroEntry {
        enabled: false,
        ..entry(2, "ko", "")
    }]);
    assert_eq!(
        result.err(),
        Some(MacroError::InvalidEntry {
            id: MacroId(2),
            reason: EntryError::EmptyReplacement,
        })
    );
}

#[test]
fn new_rejects_duplicate_ids() {
    let result = MacroTable::new(vec![entry(1, "a", "x"), entry(1, "b", "y")]);
    assert_eq!(result.err(), Some(MacroError::DuplicateId(MacroId(1))));
}

#[test]
fn empty_table_never_expands() {
    let t = MacroTable::default();
    assert!(t.is_empty());
    assert_eq!(ws(&t, "ko", CaseMode::Adaptive), None);
}

#[test]
fn empty_and_oversized_typed_text_never_expands() {
    let t = table(vec![entry(1, "ko", "không")]);
    assert_eq!(ws(&t, "", CaseMode::Adaptive), None);
    let huge = "ko".repeat(100_000);
    assert_eq!(ws(&t, &huge, CaseMode::Adaptive), None);
}

#[test]
fn adaptation_that_would_exceed_the_limit_falls_back_to_the_stored_text() {
    // Each "ß" uppercases to "SS", doubling the length.
    let long = "ß".repeat(MAX_REPLACEMENT_SCALARS);
    let t = table(vec![entry(1, "ss", &long)]);
    assert_eq!(ws(&t, "SS", CaseMode::Adaptive), expansion(1, &long));

    let title = format!("ß{}", "a".repeat(MAX_REPLACEMENT_SCALARS - 1));
    let t = table(vec![entry(2, "t", &title)]);
    assert_eq!(ws(&t, "T", CaseMode::Adaptive), expansion(2, &title));

    let short = "ßa".to_owned();
    let t = table(vec![entry(3, "t", &short)]);
    assert_eq!(ws(&t, "T", CaseMode::Adaptive), expansion(3, "SSa"));
}

#[test]
fn table_is_a_shareable_lookup_port() {
    let shared: Arc<dyn MacroLookup> = Arc::new(table(vec![entry(1, "ko", "không")]));
    let clone = Arc::clone(&shared);
    let handle =
        std::thread::spawn(move || clone.lookup("ko", Boundary::Whitespace, CaseMode::Exact));
    assert_eq!(handle.join().unwrap(), expansion(1, "không"));
}

#[test]
fn debug_output_does_not_reveal_macro_text() {
    let t = table(vec![entry(1, "bimat", "noidung")]);
    let text = format!("{t:?}");
    assert!(!text.contains("bimat"), "{text}");
    assert!(!text.contains("noidung"), "{text}");
}
