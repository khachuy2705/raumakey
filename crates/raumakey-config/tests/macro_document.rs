use raumakey_config::{ConfigError, FieldIssue, IssueReason, MacroDocument, StoreDocument};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};

const EXAMPLE: &str = r#"version = 1
revision = 4
[[macro]]
id = 1
trigger = "vn"
replacement = "Việt Nam"
[[macro]]
id = 2
trigger = "hcm"
replacement = "Thành phố Hồ Chí Minh"
enabled = false
boundary = "whitespace"
case = "exact"
"#;

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

fn issue(path: &str, reason: IssueReason) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason,
    }
}

#[test]
fn defaults_are_empty_v1() {
    let doc = MacroDocument::default_document();
    assert_eq!(doc.version, 1);
    assert_eq!(doc.revision, 0);
    assert!(doc.entries.is_empty());
    assert!(doc.extensions.is_empty());
    assert_eq!(MacroDocument::default(), doc);
    assert_eq!(MacroDocument::FILE_NAME, "raumakey-macros.toml");
    assert_eq!(MacroDocument::CURRENT_VERSION, 1);
}

#[test]
fn macro_array_decodes_with_entry_defaults() {
    let loaded = MacroDocument::decode(EXAMPLE).unwrap();
    assert!(loaded.issues.is_empty());
    let doc = loaded.document;
    assert_eq!(doc.revision, 4);
    let mut second = entry(2, "hcm", "Thành phố Hồ Chí Minh");
    second.enabled = false;
    second.boundary = BoundaryPolicy::Whitespace;
    second.case_policy = CasePolicy::Exact;
    assert_eq!(doc.entries, vec![entry(1, "vn", "Việt Nam"), second]);
}

#[test]
fn macro_document_round_trips_as_an_array_of_tables() {
    let doc = MacroDocument::decode(EXAMPLE).unwrap().document;
    let text = doc.encode().unwrap();
    assert!(text.contains("[[macro]]"), "{text}");
    assert!(text.starts_with("version = 1\nrevision = 4\n"), "{text}");
    let back = MacroDocument::decode(&text).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, doc);
}

#[test]
fn awkward_text_survives_a_round_trip() {
    let mut doc = MacroDocument::default_document();
    doc.entries = vec![
        entry(1, "q\"uote", "line one\nline two\ttab \\ backslash"),
        entry(2, "ctl", "\u{0}\u{1}\u{7f}\u{1b}"),
        entry(3, "emoji", "😀 Tiếng Việt ''' \"\"\""),
        entry(5, "crlf", "windows\r\nline\rending\n"),
        entry(u64::try_from(i64::MAX).unwrap(), "big", "id"),
    ];
    let back = MacroDocument::decode(&doc.encode().unwrap()).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, doc);
}

#[test]
fn ids_beyond_the_toml_integer_range_cannot_be_encoded() {
    let mut doc = MacroDocument::default_document();
    doc.entries = vec![entry(u64::MAX, "x", "y")];
    assert_eq!(doc.encode().unwrap_err(), ConfigError::Corrupt);
}

/// `(path, reason code)` pairs, which is how a caller would display issues.
fn codes(issues: &[FieldIssue]) -> Vec<(String, &'static str)> {
    issues
        .iter()
        .map(|issue| (issue.path.clone(), issue.reason.code()))
        .collect()
}

fn pairs(expected: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
    expected
        .iter()
        .map(|(path, code)| ((*path).to_owned(), *code))
        .collect()
}

/// The `[[macro]]` table whose `id` is `id`, from encoded text.
fn macro_table(text: &str, id: i64) -> toml::Table {
    let parsed: toml::Table = toml::from_str(text).unwrap();
    parsed["macro"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(toml::Value::as_table)
        .find(|table| table.get("id").and_then(toml::Value::as_integer) == Some(id))
        .cloned()
        .unwrap_or_else(|| panic!("no macro {id} in {text}"))
}

/// Matrix #45 for macros: a bad optional field takes its default and is reported on its own
/// path; an entry whose identity (`id`, `trigger`, `replacement`) cannot be read is set aside
/// verbatim and written back; every other entry is kept.
#[test]
fn matrix_45_bad_fields_are_isolated_per_field_and_per_entry() {
    let text = r#"version = 1
revision = 2
future = { kept = true }
[[macro]]
id = 1
trigger = "ok"
replacement = "fine"
[[macro]]
id = 2
replacement = "secret replacement without a trigger"
[[macro]]
id = -3
trigger = "negative"
replacement = "id"
[[macro]]
id = 4
trigger = "policy"
replacement = "bad boundary"
boundary = "sometimes"
[[macro]]
id = 5
trigger = "last"
replacement = "kept too"
"#;
    let loaded = MacroDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("macro[1].trigger", "missing"),
            ("macro[2].id", "out-of-range"),
            ("macro[3].boundary", "invalid-value"),
        ])
    );
    let doc = &loaded.document;
    assert_eq!(doc.revision, 2);
    assert_eq!(
        doc.entries,
        vec![
            entry(1, "ok", "fine"),
            entry(4, "policy", "bad boundary"),
            entry(5, "last", "kept too"),
        ]
    );
    assert!(doc.extensions.contains_key("future"));
    let rendered = format!("{:?}", loaded.issues);
    for secret in ["secret", "negative", "sometimes"] {
        assert!(!rendered.contains(secret), "{rendered}");
    }

    // The two entries that could not be decoded are written back unchanged, after the others.
    let encoded = doc.encode().unwrap();
    assert!(
        encoded.contains("secret replacement without a trigger"),
        "{encoded}"
    );
    assert!(encoded.contains("trigger = \"negative\""), "{encoded}");
    let again = MacroDocument::decode(&encoded).unwrap();
    assert_eq!(&again.document, doc);
    assert_eq!(
        codes(&again.issues),
        pairs(&[
            ("macro[3].trigger", "missing"),
            ("macro[4].id", "out-of-range")
        ])
    );
}

/// Matrix #45: plausible hand edits of optional fields never cost the whole macro.
#[test]
fn matrix_45_bad_optional_fields_take_their_defaults() {
    let text = r#"version = 1
[[macro]]
id = 1
trigger = "a"
replacement = "sentence boundary"
boundary = "sentence"
[[macro]]
id = 2
trigger = "b"
replacement = "yes is not a boolean"
enabled = "yes"
[[macro]]
id = 3
trigger = "c"
replacement = "capitalized keywords"
case = "Exact"
boundary = "Whitespace"
"#;
    let loaded = MacroDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("macro[0].boundary", "invalid-value"),
            ("macro[1].enabled", "invalid-type"),
            ("macro[2].boundary", "invalid-value"),
            ("macro[2].case", "invalid-value"),
        ])
    );
    assert_eq!(
        loaded.document.entries,
        vec![
            entry(1, "a", "sentence boundary"),
            entry(2, "b", "yes is not a boolean"),
            entry(3, "c", "capitalized keywords"),
        ]
    );
}

/// Unknown keys inside `[[macro]]` entries survive a rewrite and stay with their entry even
/// when the entry list is reordered or shortened.
#[test]
fn unknown_keys_inside_entries_survive_and_follow_their_entry() {
    let text = r#"version = 1
[[macro]]
id = 1
trigger = "a"
replacement = "first"
note = "future field"
meta = { color = "red", tags = ["x", "y"] }
[[macro]]
id = 2
trigger = "b"
replacement = "second"
[[macro]]
id = 3
trigger = "c"
replacement = "third"
note = "third note"
"#;
    let loaded = MacroDocument::decode(text).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let encoded = loaded.document.encode().unwrap();
    let first = macro_table(&encoded, 1);
    assert_eq!(first["note"].as_str(), Some("future field"));
    assert_eq!(first["meta"]["color"].as_str(), Some("red"));
    assert!(!macro_table(&encoded, 2).contains_key("note"));
    assert_eq!(
        macro_table(&encoded, 3)["note"].as_str(),
        Some("third note")
    );
    assert_eq!(
        MacroDocument::decode(&encoded).unwrap().document,
        loaded.document
    );

    // Reorder and drop entries as an editor would: unknown keys follow the macro id.
    let mut edited = loaded.document.clone();
    edited.entries.retain(|entry| entry.id != MacroId(1));
    edited.entries.reverse();
    let encoded = edited.encode().unwrap();
    assert!(!encoded.contains("future field"), "{encoded}");
    assert_eq!(
        macro_table(&encoded, 3)["note"].as_str(),
        Some("third note")
    );
    assert!(!macro_table(&encoded, 2).contains_key("note"));
}

/// Finding: `raumakey-macro` gives a new macro `max(id) + 1`, so after the macro with the
/// largest id is removed, the next new macro reuses its id. Unknown keys belong to the macro
/// identified by id *and* trigger, so the newcomer never inherits the deleted macro's keys.
#[test]
fn a_new_macro_that_reuses_a_deleted_id_starts_without_unknown_keys() {
    let text = r#"version = 1
[[macro]]
id = 1
trigger = "vn"
replacement = "Việt Nam"
[[macro]]
id = 2
trigger = "old"
replacement = "deleted soon"
future_scope = ["word.exe"]
"#;
    let loaded = MacroDocument::decode(text).unwrap();
    assert!(loaded.is_clean(), "{:?}", loaded.issues);
    let mut doc = loaded.document;
    // An editor removes macro 2 and then adds a macro, which receives id 2 again.
    doc.entries.retain(|entry| entry.id != MacroId(2));
    doc.entries.push(entry(2, "new", "unrelated"));
    let encoded = doc.encode().unwrap();
    let newcomer = macro_table(&encoded, 2);
    assert_eq!(newcomer["trigger"].as_str(), Some("new"));
    assert!(!newcomer.contains_key("future_scope"), "{encoded}");
    assert!(!encoded.contains("word.exe"), "{encoded}");
    assert!(MacroDocument::decode(&encoded).unwrap().is_clean());
}

/// Unknown keys follow edits of a macro's replacement and switches, but not a new trigger:
/// a renamed macro loses them rather than risk handing them to another macro.
#[test]
fn unknown_keys_follow_edits_that_keep_the_id_and_trigger() {
    let text = r#"version = 1
[[macro]]
id = 1
trigger = "vn"
replacement = "Việt Nam"
note = "future field"
"#;
    let doc = MacroDocument::decode(text).unwrap().document;
    let mut edited = doc.clone();
    edited.entries[0].replacement = "Cộng hòa".into();
    edited.entries[0].enabled = false;
    edited.entries[0].case_policy = CasePolicy::Exact;
    let encoded = edited.encode().unwrap();
    assert_eq!(
        macro_table(&encoded, 1)["note"].as_str(),
        Some("future field"),
        "{encoded}"
    );
    let mut renamed = doc;
    renamed.entries[0].trigger = "vnm".into();
    let encoded = renamed.encode().unwrap();
    assert!(!macro_table(&encoded, 1).contains_key("note"), "{encoded}");
}

#[test]
fn non_table_entries_and_a_non_array_are_reported() {
    let loaded = MacroDocument::decode("version = 1\nmacro = [1, \"two\"]\n").unwrap();
    assert_eq!(
        loaded.issues,
        vec![
            issue("macro[0]", IssueReason::InvalidType),
            issue("macro[1]", IssueReason::InvalidType),
        ]
    );
    let loaded = MacroDocument::decode("version = 1\nmacro = 5\n").unwrap();
    assert_eq!(
        loaded.issues,
        vec![issue("macro", IssueReason::InvalidType)]
    );
    assert!(loaded.document.entries.is_empty());
}

/// Semantic validation (NFC, lengths, duplicate ids) belongs to `raumakey-macro`.
#[test]
fn decoding_is_structural_only() {
    let text = r#"version = 1
[[macro]]
id = 7
trigger = ""
replacement = ""
[[macro]]
id = 7
trigger = "a b"
replacement = "dup"
"#;
    let loaded = MacroDocument::decode(text).unwrap();
    assert!(loaded.issues.is_empty());
    assert_eq!(loaded.document.entries.len(), 2);
}

#[test]
fn version_and_syntax_errors() {
    assert_eq!(
        MacroDocument::decode("version = 9\n").unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    assert_eq!(
        MacroDocument::decode("[[macro]\n").unwrap_err(),
        ConfigError::Corrupt
    );
}

/// Duplicate ids are left to `raumakey-macro`, but a rewrite must not move unknown keys from
/// one of them to another.
#[test]
fn unknown_keys_stay_with_their_entry_when_ids_repeat() {
    let text = r#"version = 1
[[macro]]
id = 7
trigger = "a"
replacement = "first"
[[macro]]
id = 7
trigger = "b"
replacement = "second"
note = "belongs to b"
"#;
    let loaded = MacroDocument::decode(text).unwrap();
    assert!(loaded.issues.is_empty());
    let encoded = loaded.document.encode().unwrap();
    let parsed: toml::Table = toml::from_str(&encoded).unwrap();
    let entries = parsed["macro"].as_array().unwrap();
    assert_eq!(entries[0]["trigger"].as_str(), Some("a"));
    assert!(entries[0].get("note").is_none(), "{encoded}");
    assert_eq!(
        entries[1]["note"].as_str(),
        Some("belongs to b"),
        "{encoded}"
    );
    assert_eq!(
        MacroDocument::decode(&encoded).unwrap().document,
        loaded.document
    );
}
