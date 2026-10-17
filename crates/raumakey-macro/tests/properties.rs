//! Property tests for the macro crate (plan §4, design §15.1).

use proptest::collection::vec;
use proptest::prelude::*;
use proptest::sample::select;
use raumakey_macro::{
    MAX_REPLACEMENT_SCALARS, MacroStore, MacroTable, adapt_case, case_pattern, export_text,
    export_text_checked, is_legacy_exportable, normalize_entry, parse_legacy_text,
};
use raumakey_model::macros::{
    Boundary, BoundaryPolicy, CaseMode, CasePattern, CasePolicy, MacroEntry, MacroId, MacroLookup,
};
use unicode_normalization::is_nfc;

const VIETNAMESE_LOWER: &str =
    "aàáảãạăằắẳẵặâầấẩẫậbcdđeèéẻẽẹêềếểễệghiìíỉĩịklmnoòóỏõọôồốổỗộơờớởỡợpqrstuùúủũụưừứửữựvxyỳýỷỹỵ";

fn vietnamese_letter() -> impl Strategy<Value = char> {
    let lower: Vec<char> = VIETNAMESE_LOWER.chars().collect();
    (select(lower), any::<bool>()).prop_map(|(c, upper)| {
        if upper {
            c.to_uppercase().next().unwrap_or(c)
        } else {
            c
        }
    })
}

fn vietnamese_word(max: usize) -> impl Strategy<Value = String> {
    vec(vietnamese_letter(), 1..=max).prop_map(String::from_iter)
}

/// Any scalar, biased toward Vietnamese letters, ASCII, separators, and combining marks.
fn mixed_char() -> impl Strategy<Value = char> {
    prop_oneof![
        4 => vietnamese_letter(),
        2 => proptest::char::range(' ', '~'),
        1 => select(vec!['\t', '\r', '\n', '\0', '\u{00A0}', '\u{2028}', '\u{FEFF}', ':', '#', ';', '/']),
        1 => proptest::char::range('\u{0300}', '\u{036F}'),
        2 => any::<char>(),
    ]
}

fn mixed_string(max: usize) -> impl Strategy<Value = String> {
    vec(mixed_char(), 0..=max).prop_map(String::from_iter)
}

fn case_pattern_strategy() -> impl Strategy<Value = CasePattern> {
    select(vec![
        CasePattern::Lower,
        CasePattern::Title,
        CasePattern::Upper,
        CasePattern::Mixed,
    ])
}

fn boundary_strategy() -> impl Strategy<Value = Boundary> {
    select(vec![Boundary::Whitespace, Boundary::Punctuation])
}

fn case_mode_strategy() -> impl Strategy<Value = CaseMode> {
    select(vec![CaseMode::Exact, CaseMode::Adaptive])
}

fn entry(id: u64, trigger: String, replacement: String) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger,
        replacement,
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    }
}

/// Arbitrary (often invalid) entries with small, colliding ids and random settings.
fn arbitrary_entry() -> impl Strategy<Value = MacroEntry> {
    (
        0u64..6,
        prop_oneof![vietnamese_word(4), mixed_string(6)],
        mixed_string(12),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(id, trigger, replacement, enabled, whitespace_only, exact_case)| MacroEntry {
                enabled,
                boundary: if whitespace_only {
                    BoundaryPolicy::Whitespace
                } else {
                    BoundaryPolicy::Delimiter
                },
                case_policy: if exact_case {
                    CasePolicy::Exact
                } else {
                    CasePolicy::Adapt
                },
                ..entry(id, trigger, replacement)
            },
        )
}

/// A scalar valid in a trigger (whitespace and controls are replaced, not rejected). `:` and
/// comment characters stay possible, so some triggers are valid but not exportable.
fn trigger_char() -> impl Strategy<Value = char> {
    mixed_char().prop_map(|c| {
        if c.is_whitespace() || c.is_control() {
            'q'
        } else {
            c
        }
    })
}

/// A scalar valid in a replacement (NUL is replaced). CR, LF, and edge whitespace stay possible,
/// so some replacements are valid but not exportable.
fn replacement_char() -> impl Strategy<Value = char> {
    mixed_char().prop_map(|c| if c == '\0' { 'r' } else { c })
}

/// A non-whitespace scalar valid in a replacement.
fn replacement_edge() -> impl Strategy<Value = char> {
    replacement_char().prop_map(|c| if c.is_whitespace() { 'e' } else { c })
}

/// Valid entries; most are exportable by construction, a minority are not.
fn valid_entry() -> impl Strategy<Value = MacroEntry> {
    let trigger = prop_oneof![
        vietnamese_word(6),
        vec(trigger_char(), 1..=10).prop_map(String::from_iter),
    ];
    let replacement = prop_oneof![
        (
            vietnamese_word(6),
            vec(prop_oneof![vietnamese_letter(), Just(' ')], 0..20)
        )
            .prop_map(|(head, tail)| format!("{head}{}", String::from_iter(tail).trim_end())),
        (
            replacement_edge(),
            vec(replacement_char(), 0..=20),
            replacement_edge()
        )
            .prop_map(|(first, middle, last)| {
                let mut text = String::from(first);
                text.extend(middle);
                text.push(last);
                text
            }),
        vec(replacement_char(), 1..=12).prop_map(String::from_iter),
        // Whitespace-only replacements are kept verbatim by the import.
        vec(
            select(vec![
                ' ', '\t', '\u{00A0}', '\u{0085}', '\u{2028}', '\u{3000}'
            ]),
            1..=4
        )
        .prop_map(String::from_iter),
    ];
    (trigger, replacement).prop_map(|(t, r)| entry(0, t, r))
}

/// One store operation; identifiers are small so that operations hit existing entries.
#[derive(Clone, Debug)]
enum StoreOp {
    Insert(MacroEntry),
    Upsert(MacroEntry),
    Remove(u64),
    InsertMany(Vec<MacroEntry>),
    Snapshot,
}

fn store_entry() -> impl Strategy<Value = MacroEntry> {
    (
        0u64..8,
        vietnamese_word(2),
        vietnamese_word(4),
        any::<bool>(),
    )
        .prop_map(|(id, trigger, replacement, enabled)| MacroEntry {
            enabled,
            ..entry(id, trigger, replacement)
        })
}

fn store_op() -> impl Strategy<Value = StoreOp> {
    prop_oneof![
        3 => store_entry().prop_map(StoreOp::Insert),
        3 => store_entry().prop_map(StoreOp::Upsert),
        2 => (0u64..8).prop_map(StoreOp::Remove),
        1 => vec(store_entry(), 0..4).prop_map(StoreOp::InsertMany),
        2 => Just(StoreOp::Snapshot),
    ]
}

fn with_prefix(prefix: &[u8], body: &[u8]) -> Vec<u8> {
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice(body);
    bytes
}

fn utf16(text: &str, little_endian: bool) -> Vec<u8> {
    let mut bytes = if little_endian {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for unit in text.encode_utf16() {
        let pair = if little_endian {
            unit.to_le_bytes()
        } else {
            unit.to_be_bytes()
        };
        bytes.extend_from_slice(&pair);
    }
    bytes
}

proptest! {
    #[test]
    fn adaptation_is_deterministic_and_nfc(text in mixed_string(40), pattern in case_pattern_strategy()) {
        let first = adapt_case(&text, pattern);
        prop_assert_eq!(&first, &adapt_case(&text, pattern));
        prop_assert!(is_nfc(&first));
        prop_assert_eq!(case_pattern(&text), case_pattern(&text));
    }

    #[test]
    fn upper_adaptation_is_idempotent(text in mixed_string(40)) {
        let upper = adapt_case(&text, CasePattern::Upper);
        prop_assert_eq!(adapt_case(&upper, CasePattern::Upper), upper);
    }

    #[test]
    fn upper_adaptation_of_uppercase_vietnamese_is_identity(word in vietnamese_word(12)) {
        let upper = word.to_uppercase();
        prop_assert_eq!(adapt_case(&upper, CasePattern::Upper), upper);
    }

    #[test]
    fn vietnamese_adaptation_produces_the_requested_shape(word in vietnamese_word(12)) {
        let lower = word.to_lowercase();
        let title = adapt_case(&lower, CasePattern::Title);
        let upper = adapt_case(&lower, CasePattern::Upper);
        prop_assert_eq!(case_pattern(&lower), CasePattern::Lower);
        prop_assert_eq!(case_pattern(&title), CasePattern::Title);
        prop_assert_eq!(title.chars().count(), lower.chars().count());
        prop_assert_eq!(title.to_lowercase(), lower.clone());
        if lower.chars().count() > 1 {
            prop_assert_eq!(case_pattern(&upper), CasePattern::Upper);
        }
        prop_assert_eq!(adapt_case(&lower, CasePattern::Lower), lower.clone());
        prop_assert_eq!(adapt_case(&lower, CasePattern::Mixed), lower);
    }

    #[test]
    fn lookup_never_panics_on_arbitrary_strings(
        entries in vec(arbitrary_entry(), 0..8),
        typed in prop_oneof![mixed_string(24), vietnamese_word(4)],
        boundary in boundary_strategy(),
        mode in case_mode_strategy(),
    ) {
        let (store, _issues) = MacroStore::from_entries_lenient(1, entries.clone());
        let table = store.snapshot();
        if let Some(hit) = table.lookup(&typed, boundary, mode) {
            prop_assert!(!hit.replacement.is_empty());
            prop_assert!(is_nfc(&hit.replacement));
            prop_assert!(hit.replacement.chars().count() <= MAX_REPLACEMENT_SCALARS);
            let source = store.get(hit.id);
            prop_assert!(source.is_some_and(|e| e.enabled));
        }
        // A strict table either matches the lenient one or reports an error; it never panics.
        if let Ok(strict) = MacroTable::new(entries) {
            prop_assert_eq!(strict.lookup(&typed, boundary, mode), table.lookup(&typed, boundary, mode));
        }
    }

    #[test]
    fn a_trigger_typed_as_stored_finds_its_lowest_id_entry_unchanged(
        entries in vec((vietnamese_word(3), vietnamese_word(6)), 1..10),
        boundary in boundary_strategy(),
        mode in case_mode_strategy(),
    ) {
        // Triggers have random case. Typed exactly as stored, a trigger inserts the stored
        // replacement in both case modes (no adaptation without a case difference).
        let entries: Vec<MacroEntry> = entries
            .into_iter()
            .zip(1u64..)
            .map(|((t, r), id)| entry(id, t, r))
            .collect();
        let table = MacroTable::new(entries.clone()).unwrap();
        for e in &entries {
            let hit = table.lookup(&e.trigger, boundary, mode).unwrap();
            let lowest = entries.iter().find(|other| other.trigger == e.trigger).unwrap();
            prop_assert_eq!(hit.id, lowest.id);
            prop_assert_eq!(&hit.replacement, &lowest.replacement);
        }
    }

    #[test]
    fn adaptive_lookup_ignores_the_case_of_vietnamese_triggers(
        trigger in vietnamese_word(5),
        replacement in vietnamese_word(8),
        flips in vec(any::<bool>(), 5),
    ) {
        let table = MacroTable::new(vec![entry(1, trigger.to_lowercase(), replacement)]).unwrap();
        let typed: String = trigger
            .chars()
            .zip(flips.iter().copied().chain(std::iter::repeat(false)))
            .map(|(c, flip)| {
                if flip {
                    c.to_uppercase().next().unwrap_or(c)
                } else {
                    c
                }
            })
            .collect();
        let hit = table.lookup(&typed, Boundary::Punctuation, CaseMode::Adaptive);
        prop_assert!(hit.is_some());
    }

    #[test]
    fn store_snapshots_match_a_table_rebuilt_from_the_entries(
        ops in vec(store_op(), 0..24),
        probes in vec(vietnamese_word(2), 1..6),
    ) {
        let mut store = MacroStore::new();
        let mut snapshots = Vec::new();
        for op in ops {
            let before = store.revision();
            let changed = match op {
                StoreOp::Insert(e) => store.insert(before, e).is_ok(),
                StoreOp::Upsert(e) => store.upsert(before, e).is_ok(),
                StoreOp::Remove(id) => store.remove(before, MacroId(id)).is_ok(),
                StoreOp::InsertMany(batch) => {
                    let len = batch.len();
                    let (revision, ids) = store.insert_many(before, batch).unwrap();
                    prop_assert_eq!(ids.len(), len);
                    prop_assert_eq!(revision, store.revision());
                    len > 0
                }
                StoreOp::Snapshot => {
                    snapshots.push((store.snapshot(), store.entries()));
                    false
                }
            };
            // A successful change advances the revision by exactly one.
            prop_assert_eq!(store.revision(), before + u64::from(changed));
            // Identifiers stay unique and the next one is max + 1.
            let mut ids: Vec<u64> = store.list().iter().map(|e| e.id.0).collect();
            let expected_next = ids.iter().max().map_or(1, |max| max + 1);
            prop_assert_eq!(store.next_id(), Ok(MacroId(expected_next)));
            ids.sort_unstable();
            ids.dedup();
            prop_assert_eq!(ids.len(), store.len());
        }
        snapshots.push((store.snapshot(), store.entries()));
        // Every snapshot, including ones taken before later changes, equals a table rebuilt
        // from the entries of its time.
        for (snapshot, entries) in snapshots {
            let rebuilt = MacroTable::new(entries).unwrap();
            prop_assert_eq!(snapshot.len(), rebuilt.len());
            for typed in &probes {
                for mode in [CaseMode::Exact, CaseMode::Adaptive] {
                    for boundary in [Boundary::Whitespace, Boundary::Punctuation] {
                        prop_assert_eq!(
                            snapshot.lookup(typed, boundary, mode),
                            rebuilt.lookup(typed, boundary, mode)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn parse_legacy_text_never_panics_on_arbitrary_bytes(
        body in vec(any::<u8>(), 0..2048),
        prefix in select(vec![&[][..], &[0xEF, 0xBB, 0xBF][..], &[0xFF, 0xFE][..], &[0xFE, 0xFF][..]]),
    ) {
        let import = parse_legacy_text(&with_prefix(prefix, &body));
        for (index, e) in import.entries.iter().enumerate() {
            prop_assert_eq!(e.id, MacroId(index as u64 + 1));
            prop_assert_eq!(normalize_entry(e.clone()), Ok(e.clone()));
        }
        prop_assert!(import.issues.iter().all(|issue| issue.line >= 1));
        prop_assert!(import.issues.windows(2).all(|w| w[0].line < w[1].line));
    }

    #[test]
    fn parse_legacy_text_never_panics_on_arbitrary_text(text in mixed_string(256)) {
        for bytes in [text.as_bytes().to_vec(), utf16(&text, true), utf16(&text, false)] {
            let import = parse_legacy_text(&bytes);
            for e in &import.entries {
                prop_assert_eq!(normalize_entry(e.clone()), Ok(e.clone()));
            }
        }
    }

    #[test]
    fn export_then_parse_round_trips_valid_entries(entries in vec(valid_entry(), 0..8)) {
        // Exactly the exportable entries come back, normalized and in order; the others are
        // reported as skipped by the export and never corrupt neighboring lines.
        let entries: Vec<MacroEntry> = entries
            .into_iter()
            .zip(1u64..)
            .map(|(e, id)| MacroEntry { id: MacroId(id), ..e })
            .collect();
        let expected: Vec<(String, String)> = entries
            .iter()
            .filter(|e| is_legacy_exportable(e))
            .map(|e| {
                let n = normalize_entry(e.clone()).unwrap();
                (n.trigger, n.replacement)
            })
            .collect();
        let expected_skipped: Vec<MacroId> = entries
            .iter()
            .filter(|e| !is_legacy_exportable(e))
            .map(|e| e.id)
            .collect();
        let export = export_text_checked(&entries);
        prop_assert_eq!(&export.skipped, &expected_skipped);
        let text = export_text(&entries);
        prop_assert_eq!(&text, &export.text);
        for bytes in [
            text.as_bytes().to_vec(),
            with_prefix(&[0xEF, 0xBB, 0xBF], text.as_bytes()),
            utf16(&text, true),
            utf16(&text, false),
        ] {
            let import = parse_legacy_text(&bytes);
            prop_assert!(import.issues.is_empty(), "{:?}", import.issues);
            let parsed: Vec<(String, String)> = import
                .entries
                .iter()
                .map(|e| (e.trigger.clone(), e.replacement.clone()))
                .collect();
            prop_assert_eq!(&parsed, &expected);
        }
    }
}
