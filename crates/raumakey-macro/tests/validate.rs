//! Entry validation and NFC normalization (plan §4).

use raumakey_macro::{
    EntryError, MAX_REPLACEMENT_SCALARS, MAX_TRIGGER_SCALARS, normalize_entry,
    normalize_replacement, normalize_trigger,
};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use serde::{Deserialize, Serialize};

/// "ệ" in canonical decomposition: three scalars that compose to one.
const E_NFD: &str = "e\u{0302}\u{0323}";

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

#[test]
fn trigger_and_replacement_are_stored_nfc() {
    assert_eq!(normalize_trigger("vie\u{0302}\u{0323}t").unwrap(), "việt");
    assert_eq!(
        normalize_replacement("Vie\u{0302}\u{0323}t Nam").unwrap(),
        "Việt Nam"
    );
}

#[test]
fn normalize_entry_keeps_metadata_and_normalizes_text() {
    let raw = MacroEntry {
        enabled: false,
        boundary: BoundaryPolicy::Whitespace,
        case_policy: CasePolicy::Exact,
        ..entry(7, "vn\u{0300}", "Vie\u{0302}\u{0323}t Nam")
    };
    let normalized = normalize_entry(raw).unwrap();
    assert_eq!(normalized.id, MacroId(7));
    assert_eq!(normalized.trigger, "vǹ");
    assert_eq!(normalized.replacement, "Việt Nam");
    assert!(!normalized.enabled);
    assert_eq!(normalized.boundary, BoundaryPolicy::Whitespace);
    assert_eq!(normalized.case_policy, CasePolicy::Exact);
}

#[test]
fn already_valid_entries_are_unchanged() {
    let valid = entry(1, "ko", "không");
    assert_eq!(normalize_entry(valid.clone()).unwrap(), valid);
}

#[test]
fn empty_trigger_is_rejected() {
    assert_eq!(normalize_trigger(""), Err(EntryError::EmptyTrigger));
    assert_eq!(
        normalize_entry(entry(1, "", "x")),
        Err(EntryError::EmptyTrigger)
    );
}

#[test]
fn trigger_whitespace_is_rejected() {
    for trigger in [
        "k o",
        "ko\t",
        " ko",
        "ko\n",
        "k\u{00A0}o",
        "k\u{3000}o",
        "k\u{2028}o",
    ] {
        assert_eq!(
            normalize_trigger(trigger),
            Err(EntryError::TriggerWhitespace),
            "{:?}",
            trigger.escape_unicode().to_string()
        );
    }
}

#[test]
fn trigger_control_characters_are_rejected() {
    for trigger in [
        "k\u{0000}o",
        "k\u{0001}",
        "\u{007F}k",
        "k\u{0080}",
        "k\u{009F}",
    ] {
        assert_eq!(
            normalize_trigger(trigger),
            Err(EntryError::TriggerControl),
            "{:?}",
            trigger.escape_unicode().to_string()
        );
    }
}

#[test]
fn trigger_length_limit_counts_nfc_scalars() {
    let max = "a".repeat(MAX_TRIGGER_SCALARS);
    assert_eq!(normalize_trigger(&max).unwrap(), max);
    let over = "a".repeat(MAX_TRIGGER_SCALARS + 1);
    assert_eq!(normalize_trigger(&over), Err(EntryError::TriggerTooLong));

    // 64 decomposed letters are 192 scalars but only 64 after NFC.
    let decomposed = E_NFD.repeat(MAX_TRIGGER_SCALARS);
    assert_eq!(
        normalize_trigger(&decomposed).unwrap(),
        "ệ".repeat(MAX_TRIGGER_SCALARS)
    );
    let decomposed_over = E_NFD.repeat(MAX_TRIGGER_SCALARS + 1);
    assert_eq!(
        normalize_trigger(&decomposed_over),
        Err(EntryError::TriggerTooLong)
    );
}

#[test]
fn trigger_supplementary_scalars_count_once() {
    let emoji = "\u{1F600}".repeat(MAX_TRIGGER_SCALARS);
    assert_eq!(normalize_trigger(&emoji).unwrap(), emoji);
}

#[test]
fn very_long_trigger_is_rejected_as_too_long() {
    let huge = "a".repeat(1_000_000);
    assert_eq!(normalize_trigger(&huge), Err(EntryError::TriggerTooLong));
}

#[test]
fn trigger_errors_have_documented_precedence() {
    // empty > too long > whitespace > control
    let long_with_space = format!("{} b", "a".repeat(MAX_TRIGGER_SCALARS));
    assert_eq!(
        normalize_trigger(&long_with_space),
        Err(EntryError::TriggerTooLong)
    );
    assert_eq!(
        normalize_trigger("a \u{0001}"),
        Err(EntryError::TriggerWhitespace)
    );
}

#[test]
fn empty_replacement_is_rejected() {
    assert_eq!(normalize_replacement(""), Err(EntryError::EmptyReplacement));
    assert_eq!(
        normalize_entry(entry(1, "ko", "")),
        Err(EntryError::EmptyReplacement)
    );
}

#[test]
fn replacement_nul_is_rejected() {
    assert_eq!(
        normalize_replacement("kh\u{0000}ng"),
        Err(EntryError::ReplacementNul)
    );
}

#[test]
fn replacement_allows_whitespace_and_other_controls() {
    let text = " dòng 1\r\n\tdòng 2\u{0001} ";
    assert_eq!(normalize_replacement(text).unwrap(), text);
}

#[test]
fn replacement_length_limit_counts_nfc_scalars() {
    let max = "x".repeat(MAX_REPLACEMENT_SCALARS);
    assert_eq!(normalize_replacement(&max).unwrap(), max);
    let over = "x".repeat(MAX_REPLACEMENT_SCALARS + 1);
    assert_eq!(
        normalize_replacement(&over),
        Err(EntryError::ReplacementTooLong)
    );
    let decomposed = E_NFD.repeat(MAX_REPLACEMENT_SCALARS);
    assert_eq!(
        normalize_replacement(&decomposed).unwrap().chars().count(),
        MAX_REPLACEMENT_SCALARS
    );
    let huge = "x".repeat(2_000_000);
    assert_eq!(
        normalize_replacement(&huge),
        Err(EntryError::ReplacementTooLong)
    );
}

#[test]
fn entry_reports_trigger_errors_before_replacement_errors() {
    assert_eq!(
        normalize_entry(entry(1, "k o", "")),
        Err(EntryError::TriggerWhitespace)
    );
}

#[test]
fn entry_error_codes_are_stable_and_match_serde_names() {
    #[derive(Serialize, Deserialize)]
    struct Wrapper {
        reason: EntryError,
    }
    let all = [
        (EntryError::EmptyTrigger, "empty-trigger"),
        (EntryError::TriggerTooLong, "trigger-too-long"),
        (EntryError::TriggerWhitespace, "trigger-whitespace"),
        (EntryError::TriggerControl, "trigger-control"),
        (EntryError::EmptyReplacement, "empty-replacement"),
        (EntryError::ReplacementTooLong, "replacement-too-long"),
        (EntryError::ReplacementNul, "replacement-nul"),
    ];
    for (error, code) in all {
        assert_eq!(error.code(), code);
        let text = toml::to_string(&Wrapper { reason: error }).unwrap();
        assert_eq!(text.trim(), format!("reason = \"{code}\""));
        let back: Wrapper = toml::from_str(&text).unwrap();
        assert_eq!(back.reason, error);
        assert!(!error.to_string().is_empty());
    }
}
