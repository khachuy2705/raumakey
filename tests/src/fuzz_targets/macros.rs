//! `macro_files`: legacy macro text against the importer, `MacroTable`, and the exporter
//! (plan §4, matrix #44, #47–#52).

use raumakey_macro::{
    LegacyIssueReason, MAX_LEGACY_MACRO_LINES, MAX_REPLACEMENT_SCALARS, MacroTable, adapt_case,
    case_pattern, export_text_checked, is_legacy_exportable, normalize_entry, parse_legacy_text,
};
use raumakey_model::input::is_boundary_punctuation;
use raumakey_model::macros::{
    Boundary, BoundaryPolicy, CaseMode, CasePolicy, MacroEntry, MacroExpansion, MacroLookup,
};
use raumakey_model::transaction::MAX_INSERT_UNITS;
use unicode_normalization::{UnicodeNormalization, is_nfc};

/// Largest input in bytes.
const MAX_BYTES: usize = 64 * 1024;
/// Entries whose trigger is looked up in its case variants.
const LOOKED_UP_ENTRIES: usize = 12;
/// Words of the input looked up as typed text.
const TYPED_WORDS: usize = 12;
/// Longest typed word, in characters.
const MAX_WORD_CHARS: usize = 80;

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

/// The case-insensitive key of plan §4: lowercase NFC.
fn fold(text: &str) -> String {
    nfc(&nfc(text).to_lowercase())
}

/// D30: the text and one delimiter unit fit one insert in NFC and in NFD.
fn fits_one_insert(text: &str) -> bool {
    let nfc_units = text.nfc().map(char::len_utf16).sum::<usize>();
    let nfd_units = text.nfd().map(char::len_utf16).sum::<usize>();
    nfc_units.max(nfd_units) < MAX_INSERT_UNITS
}

/// The lookup rules `MacroTable` documents, over the imported entries, with each folded
/// trigger computed once: the plan §4 order (the exact trigger first, then, adaptive only, the
/// case-insensitive one; the lowest identifier wins), plus the table's rule 5, a code decision
/// of design §20.9 that the plan does not record yet and that departs from the plan §4
/// wording: only a case-insensitive match of an `Adapt` entry follows the typed case (an exact
/// match never does), and only within the replacement limit and one insert with a delimiter
/// in NFC and NFD (D30). The adaptation itself reuses
/// `adapt_case` and `case_pattern` of the crate under test, whose rules its unit tests pin;
/// this reference decides which entry wins and whether the replacement is adapted.
struct Reference<'a> {
    entries: &'a [MacroEntry],
    folded: Vec<String>,
}

impl<'a> Reference<'a> {
    fn new(entries: &'a [MacroEntry]) -> Self {
        Self {
            entries,
            folded: entries.iter().map(|entry| fold(&entry.trigger)).collect(),
        }
    }

    /// `MacroLookup::has_token_triggers` (D28): an enabled trigger contains boundary
    /// punctuation.
    fn has_token_triggers(&self) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.enabled && entry.trigger.chars().any(is_boundary_punctuation))
    }

    /// `MacroLookup::is_trigger_prefix` (D28): some enabled trigger starts with the NFC
    /// prefix, or, in adaptive mode, its fold starts with the prefix's fold. An empty prefix
    /// starts nothing.
    fn is_trigger_prefix(&self, prefix: &str, mode: CaseMode) -> bool {
        if prefix.is_empty() {
            return false;
        }
        let prefix = nfc(prefix);
        let folded = fold(&prefix);
        self.entries.iter().zip(&self.folded).any(|(entry, key)| {
            entry.enabled
                && (entry.trigger.starts_with(&prefix)
                    || (mode == CaseMode::Adaptive && key.starts_with(&folded)))
        })
    }

    fn lookup(&self, typed: &str, boundary: Boundary, mode: CaseMode) -> Option<MacroExpansion> {
        let typed = nfc(typed);
        let eligible = |entry: &MacroEntry| {
            entry.enabled
                && (entry.boundary == BoundaryPolicy::Delimiter || boundary == Boundary::Whitespace)
        };
        let exact = self
            .entries
            .iter()
            .filter(|entry| eligible(entry) && entry.trigger == typed)
            .min_by_key(|entry| entry.id);
        if let Some(entry) = exact {
            return Some(MacroExpansion {
                id: entry.id,
                replacement: entry.replacement.clone(),
            });
        }
        if mode == CaseMode::Exact {
            return None;
        }
        let folded = fold(&typed);
        let (entry, _) = self
            .entries
            .iter()
            .zip(&self.folded)
            .filter(|(entry, key)| eligible(entry) && **key == folded)
            .min_by_key(|(entry, _)| entry.id)?;
        let replacement = if entry.case_policy == CasePolicy::Adapt {
            let adapted = adapt_case(&entry.replacement, case_pattern(&typed));
            if adapted.chars().count() > MAX_REPLACEMENT_SCALARS || !fits_one_insert(&adapted) {
                entry.replacement.clone()
            } else {
                adapted
            }
        } else {
            entry.replacement.clone()
        };
        Some(MacroExpansion {
            id: entry.id,
            replacement,
        })
    }
}

fn check_lookup(table: &MacroTable, reference: &Reference<'_>, typed: &str) {
    if typed.chars().count() > MAX_WORD_CHARS {
        return;
    }
    for boundary in [Boundary::Whitespace, Boundary::Punctuation] {
        for mode in [CaseMode::Exact, CaseMode::Adaptive] {
            let found = table.lookup(typed, boundary, mode);
            let expected = reference.lookup(typed, boundary, mode);
            assert!(
                found == expected,
                "lookup ({boundary:?}, {mode:?}) differs from the documented lookup rules"
            );
            if let Some(expansion) = found {
                assert!(is_nfc(&expansion.replacement), "an expansion is not NFC");
                assert!(
                    expansion.replacement.chars().count() <= MAX_REPLACEMENT_SCALARS,
                    "an expansion exceeds the replacement limit"
                );
                assert!(
                    fits_one_insert(&expansion.replacement),
                    "an expansion does not fit one insert with a delimiter (D30)"
                );
            }
        }
    }
}

/// The token queries for `typed` at every place the engine asks them: after each boundary
/// punctuation character, and for the whole text.
fn check_prefixes(table: &MacroTable, reference: &Reference<'_>, typed: &str) {
    if typed.chars().count() > MAX_WORD_CHARS {
        return;
    }
    let ends = typed
        .char_indices()
        .filter(|&(_, c)| is_boundary_punctuation(c))
        .map(|(index, c)| index + c.len_utf8())
        .chain([typed.len()]);
    for end in ends {
        let prefix = typed.get(..end).unwrap_or_default();
        for mode in [CaseMode::Exact, CaseMode::Adaptive] {
            assert!(
                table.is_trigger_prefix(prefix, mode) == reference.is_trigger_prefix(prefix, mode),
                "is_trigger_prefix ({mode:?}) differs from the documented rule"
            );
        }
    }
}

/// Case variants of a trigger: as stored, lower, upper, title, and alternating.
fn case_variants(trigger: &str) -> [String; 5] {
    let mut title = String::new();
    let mut alternating = String::new();
    for (index, c) in trigger.chars().enumerate() {
        if index == 0 {
            title.extend(c.to_uppercase());
        } else {
            title.extend(c.to_lowercase());
        }
        if index % 2 == 0 {
            alternating.extend(c.to_uppercase());
        } else {
            alternating.extend(c.to_lowercase());
        }
    }
    [
        trigger.to_owned(),
        trigger.to_lowercase(),
        trigger.to_uppercase(),
        title,
        alternating,
    ]
}

pub(crate) fn run(data: &[u8]) {
    let bytes = &data[..data.len().min(MAX_BYTES)];
    let import = parse_legacy_text(bytes);

    // Issues carry line numbers in file order (or line 0 for the whole input) and reason codes.
    let mut previous_line = 0;
    for issue in &import.issues {
        if issue.reason == LegacyIssueReason::TooLarge {
            assert!(
                issue.line == 0 && import.entries.is_empty() && import.issues.len() == 1,
                "a too-large input still imported something"
            );
            continue;
        }
        assert!(issue.line > previous_line, "issues are not in line order");
        previous_line = issue.line;
    }
    let breaks = bytes
        .iter()
        .filter(|byte| matches!(byte, b'\n' | b'\r'))
        .count();
    assert!(
        previous_line <= breaks + 1,
        "an issue names a line past the end of the input"
    );
    assert!(
        import.entries.len() + import.issues.len() <= MAX_LEGACY_MACRO_LINES,
        "more macro lines than the import limit"
    );

    // Every imported entry is valid, NFC, numbered in file order, with the default settings,
    // and exportable unless its trigger starts with U+FEFF (which reads as a byte order mark).
    for (index, entry) in import.entries.iter().enumerate() {
        assert!(
            entry.id.0 == u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1),
            "imported identifiers are not 1, 2, 3, …"
        );
        assert!(
            normalize_entry(entry.clone()).as_ref() == Ok(entry),
            "an imported entry is invalid or not NFC"
        );
        assert!(
            entry.enabled
                && entry.boundary == BoundaryPolicy::Delimiter
                && entry.case_policy == CasePolicy::Adapt,
            "an imported entry does not have the default settings"
        );
        assert!(
            is_legacy_exportable(entry) || entry.trigger.starts_with('\u{feff}'),
            "an imported entry cannot be exported again"
        );
    }

    // The entries build a table; lookups follow the plan §4 rules for stored triggers, their
    // case variants, and words of the input.
    let table = match MacroTable::new(import.entries.clone()) {
        Ok(table) => table,
        Err(error) => panic!("imported entries do not build a table: {error}"),
    };
    assert!(
        table.len() == import.entries.len(),
        "the table lost an entry"
    );
    let reference = Reference::new(&import.entries);
    assert!(
        table.has_token_triggers() == reference.has_token_triggers(),
        "has_token_triggers differs from the documented rule"
    );
    for entry in import.entries.iter().take(LOOKED_UP_ENTRIES) {
        for variant in case_variants(&entry.trigger) {
            check_lookup(&table, &reference, &variant);
            check_prefixes(&table, &reference, &variant);
        }
        let decomposed: String = entry.trigger.nfd().collect();
        check_lookup(&table, &reference, &decomposed);
        check_prefixes(&table, &reference, &decomposed);
    }
    let text = String::from_utf8_lossy(bytes);
    for word in text
        .split(|c: char| c.is_whitespace() || c == ':')
        .filter(|word| !word.is_empty())
        .take(TYPED_WORDS)
    {
        check_lookup(&table, &reference, word);
        check_prefixes(&table, &reference, word);
    }

    // Export writes every representable entry and reports the others (`is_legacy_exportable`
    // of the crate under test defines which); the written entries read back unchanged.
    let export = export_text_checked(&import.entries);
    let skipped: Vec<_> = import
        .entries
        .iter()
        .filter(|entry| !is_legacy_exportable(entry))
        .map(|entry| entry.id)
        .collect();
    assert!(
        export.skipped == skipped,
        "export skipped other entries than it reports"
    );
    let again = parse_legacy_text(export.text.as_bytes());
    assert!(
        again.issues.is_empty(),
        "exported text does not import cleanly"
    );
    let written: Vec<(&str, &str)> = import
        .entries
        .iter()
        .filter(|entry| is_legacy_exportable(entry))
        .map(|entry| (entry.trigger.as_str(), entry.replacement.as_str()))
        .collect();
    let read: Vec<(&str, &str)> = again
        .entries
        .iter()
        .map(|entry| (entry.trigger.as_str(), entry.replacement.as_str()))
        .collect();
    assert!(written == read, "export then import changed an entry");
    assert!(
        export_text_checked(&again.entries).text == export.text,
        "export is not a fixed point"
    );
}
