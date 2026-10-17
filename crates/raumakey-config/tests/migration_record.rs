use std::collections::BTreeMap;

use raumakey_config::{
    CommitResult, ConfigError, FieldIssue, ImportedCounts, IssueReason, MigrationRecord,
    MigrationStatus, QuarantinedField, SourceFingerprint, StoreDocument, sha256_hex,
};
use serde_json::json;

fn issue(path: &str, reason: IssueReason) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason,
    }
}

fn completed_record() -> MigrationRecord {
    let mut checksums = BTreeMap::new();
    checksums.insert("config.toml".to_owned(), sha256_hex(b"config"));
    checksums.insert("macros.toml".to_owned(), sha256_hex(b"macros"));
    MigrationRecord {
        version: 1,
        revision: 2,
        status: MigrationStatus::Completed,
        started_at_ms: 1_700_000_000_000,
        completed_at_ms: 1_700_000_000_500,
        sources: vec![
            SourceFingerprint {
                kind: "macro-text".into(),
                location: "C:\\Tools\\EVKey\\evkmacro.txt".into(),
                sha256: sha256_hex(b"macro file"),
                size: 10,
            },
            SourceFingerprint {
                kind: "registry".into(),
                location: "HKCU\\Software\\MyQ\\EVKey".into(),
                sha256: sha256_hex(b""),
                size: 0,
            },
        ],
        imported: ImportedCounts {
            settings: 0,
            macros: 12,
            named_sets: 1,
            probes: 3,
        },
        quarantined: vec![QuarantinedField {
            source: "macro-text".into(),
            field: "line 7".into(),
            reason_code: "invalid-line".into(),
        }],
        preserved: json!({ "registry": { "SomeValue": [1, 2, 3] } }),
        result: Some(CommitResult {
            config_revision: 5,
            macros_revision: 2,
            app_rules_revision: 9,
            checksums,
        }),
        extensions: serde_json::Map::new(),
    }
}

#[test]
fn defaults_are_not_started() {
    let record = MigrationRecord::default_document();
    assert_eq!(record.version, 1);
    assert_eq!(record.revision, 0);
    assert_eq!(record.status, MigrationStatus::NotStarted);
    assert_eq!(record.started_at_ms, 0);
    assert_eq!(record.completed_at_ms, 0);
    assert!(record.sources.is_empty());
    assert_eq!(record.imported, ImportedCounts::default());
    assert!(record.quarantined.is_empty());
    assert_eq!(record.preserved, json!({}));
    assert_eq!(record.result, None);
    assert!(record.extensions.is_empty());
    assert_eq!(MigrationRecord::default(), record);
    assert_eq!(MigrationRecord::FILE_NAME, "migration.json");
    assert_eq!(MigrationRecord::CURRENT_VERSION, 1);
}

#[test]
fn status_names_are_kebab_case() {
    let names = [
        (MigrationStatus::NotStarted, "not-started"),
        (MigrationStatus::InProgress, "in-progress"),
        (MigrationStatus::Completed, "completed"),
        (MigrationStatus::RolledBack, "rolled-back"),
        (MigrationStatus::Failed, "failed"),
    ];
    for (status, name) in names {
        assert_eq!(status.as_str(), name);
        assert_eq!(serde_json::to_value(status).unwrap(), json!(name));
    }
}

#[test]
fn sha256_hex_is_lowercase_hex() {
    assert_eq!(
        sha256_hex(b"abc"),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(sha256_hex(b"").len(), 64);
}

#[test]
fn record_round_trips_as_json() {
    let record = completed_record();
    let text = record.encode().unwrap();
    let value: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["version"], json!(1));
    assert_eq!(value["status"], json!("completed"));
    assert_eq!(value["imported"]["macros"], json!(12));
    assert_eq!(value["result"]["app_rules_revision"], json!(9));
    assert_eq!(value["sources"][0]["kind"], json!("macro-text"));
    let back = MigrationRecord::decode(&text).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, record);
}

#[test]
fn a_record_without_result_round_trips() {
    let mut record = MigrationRecord::default_document();
    record.status = MigrationStatus::RolledBack;
    record.preserved = serde_json::Value::Null;
    let back = MigrationRecord::decode(&record.encode().unwrap()).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, record);
}

/// Matrix #45 for the migration record.
#[test]
fn matrix_45_corrupt_fields_are_isolated_and_unknown_keys_kept() {
    let text = r#"{
        "version": 1,
        "revision": 4,
        "status": "half-done",
        "started_at_ms": -1,
        "completed_at_ms": 99,
        "sources": [
            {"kind": "registry", "location": "HKCU\\Software\\MyQ\\EVKey", "sha256": "00", "size": 1},
            {"kind": "settings-ini", "location": "secret-location"},
            "not an object"
        ],
        "imported": {"macros": 3, "settings": "many"},
        "quarantined": [],
        "result": {"config_revision": "x"},
        "future_key": {"nested": [true]}
    }"#;
    let loaded = MigrationRecord::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("status", "invalid-value"),
            ("started_at_ms", "out-of-range"),
            ("sources[1].sha256", "missing"),
            ("sources[1].size", "missing"),
            ("sources[2]", "invalid-type"),
            ("imported.settings", "invalid-type"),
            ("result.config_revision", "invalid-type"),
            ("result.macros_revision", "missing"),
            ("result.app_rules_revision", "missing"),
        ])
    );
    let record = &loaded.document;
    assert_eq!(record.revision, 4);
    assert_eq!(record.status, MigrationStatus::NotStarted);
    assert_eq!(record.started_at_ms, 0);
    assert_eq!(record.completed_at_ms, 99);
    assert_eq!(record.sources.len(), 2);
    assert_eq!(record.sources[1].kind, "settings-ini");
    assert_eq!(record.sources[1].sha256, "");
    assert_eq!(record.sources[1].size, 0);
    assert_eq!(record.imported.macros, 3);
    assert_eq!(record.imported.settings, 0);
    assert_eq!(record.result, Some(CommitResult::default()));
    assert_eq!(record.extensions["future_key"], json!({"nested": [true]}));
    assert!(!format!("{:?}", loaded.issues).contains("secret"));

    let again = MigrationRecord::decode(&record.encode().unwrap()).unwrap();
    assert!(again.issues.is_empty());
    assert_eq!(&again.document, record);
}

#[test]
fn json_errors_and_versions() {
    for text in ["{", "[1, 2]", "\"text\"", "{\"version\": 1,}"] {
        assert_eq!(
            MigrationRecord::decode(text).unwrap_err(),
            ConfigError::Corrupt,
            "{text}"
        );
    }
    assert_eq!(
        MigrationRecord::decode("{\"version\": 2}").unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    for text in [
        "{\"version\": \"1\"}",
        "{\"version\": 1.0}",
        "{\"version\": 1.5}",
        "{\"version\": null}",
        "{\"version\": [1]}",
    ] {
        assert_eq!(
            MigrationRecord::decode(text).unwrap_err(),
            ConfigError::UnsupportedVersion,
            "{text}"
        );
    }
    let lenient = MigrationRecord::decode("\u{feff}{\"result\": null}").unwrap();
    assert_eq!(lenient.issues, vec![issue("version", IssueReason::Missing)]);
    assert_eq!(lenient.document, MigrationRecord::default_document());
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

/// Unknown keys inside `imported`, `result`, `sources[i]`, and `quarantined[i]` survive a
/// rewrite: a newer build's additions are not stripped by an older build that updates the
/// record.
#[test]
fn unknown_keys_inside_nested_objects_survive() {
    let text = r#"{
        "version": 1,
        "revision": 2,
        "status": "completed",
        "sources": [
            {"kind": "registry", "location": "HKCU\\Software\\MyQ\\EVKey", "sha256": "aa", "size": 1, "future_src": "s0"},
            {"kind": "macro-text", "location": "evkmacro.txt", "sha256": "bb", "size": 2}
        ],
        "imported": {"macros": 3, "future_count": 7},
        "quarantined": [
            {"source": "registry", "field": "Hotkey", "reason_code": "unmapped", "future_q": [1]}
        ],
        "preserved": {},
        "result": {
            "config_revision": 1, "macros_revision": 2, "app_rules_revision": 3,
            "checksums": {"config.toml": "cc"},
            "future_res": {"nested": true}
        }
    }"#;
    let loaded = MigrationRecord::decode(text).unwrap();
    assert!(loaded.is_clean(), "{:?}", loaded.issues);
    let mut record = loaded.document;
    let check = |record: &MigrationRecord| {
        let encoded = record.encode().unwrap();
        let value: serde_json::Value = serde_json::from_str(&encoded).unwrap();
        assert_eq!(value["imported"]["future_count"], json!(7), "{encoded}");
        assert_eq!(
            value["result"]["future_res"],
            json!({"nested": true}),
            "{encoded}"
        );
        let source = |kind: &str| {
            value["sources"]
                .as_array()
                .unwrap()
                .iter()
                .find(|source| source["kind"] == json!(kind))
                .unwrap()
                .clone()
        };
        assert_eq!(source("registry")["future_src"], json!("s0"), "{encoded}");
        assert!(
            source("macro-text").get("future_src").is_none(),
            "{encoded}"
        );
        assert_eq!(value["quarantined"][0]["future_q"], json!([1]), "{encoded}");
        // The bookkeeping that carries nested keys is not written at the top level.
        for key in value.as_object().unwrap().keys() {
            assert!(!key.starts_with("future"), "{key} leaked to the top level");
        }
        encoded
    };
    let encoded = check(&record);
    assert_eq!(MigrationRecord::decode(&encoded).unwrap().document, record);
    // Unknown keys of a source follow it by kind and location.
    record.sources.reverse();
    check(&record);
}
