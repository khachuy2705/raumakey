//! Parsing: every source field is imported, preserved, or quarantined (plan D18, matrix #44,
//! #45).

mod common;

use common::{FOLDER, TONE_MAP, evkey_fs, evkey_registry, fs_with, utf16le};
use raumakey_config::{QuarantinedField, sha256_hex};
use raumakey_macro::LegacyIssueReason;
use raumakey_migrate::{
    Disposition, FIELD_MAP, FakeFs, FakeRegistry, FieldRef, FieldReport, LegacyRegistry,
    LegacyValue, MAX_FIELD_BYTES, MAX_FIELDS_PER_SOURCE, MAX_PRESERVED_BYTES,
    MAX_RECORDED_QUARANTINE, MigrateError, ParsedMigration, QuarantineReason, REG_MULTI_SZ, REG_SZ,
    SETTINGS_REGISTRY_PATH, SourceKind, TSF_CACHE_REGISTRY_PATH, discover, parse,
    parse_with_field_map,
};
use raumakey_model::host::ExeName;
use raumakey_model::policy::RuleAction;
use serde_json::json;

const SECRET: &str = "SECRET-7f3a";

fn parse_fakes(registry: &FakeRegistry, fs: &FakeFs) -> ParsedMigration {
    parse(&discover(registry, fs, [FOLDER]).unwrap())
}

fn parsed_with_map(registry: &FakeRegistry, fs: &FakeFs) -> ParsedMigration {
    parse_with_field_map(&discover(registry, fs, [FOLDER]).unwrap(), TONE_MAP)
}

fn reports(parsed: &ParsedMigration, source: SourceKind) -> Vec<(FieldRef, Disposition)> {
    parsed
        .fields()
        .iter()
        .filter(|report| report.source == source)
        .map(|report| (report.field.clone(), report.disposition.clone()))
        .collect()
}

fn disposition(parsed: &ParsedMigration, source: SourceKind, field: &FieldRef) -> Disposition {
    let found: Vec<&FieldReport> = parsed
        .fields()
        .iter()
        .filter(|report| report.source == source && &report.field == field)
        .collect();
    assert_eq!(found.len(), 1, "{source:?} {field:?}");
    found[0].disposition.clone()
}

fn reg(path: &str) -> FieldRef {
    FieldRef::Registry(path.to_owned())
}

fn ini(section: &str, key: &str, line: usize) -> FieldRef {
    FieldRef::Ini {
        section: section.to_owned(),
        key: key.to_owned(),
        line,
    }
}

fn quarantined(reason: QuarantineReason) -> Disposition {
    Disposition::Quarantined(reason)
}

fn named(name: &str) -> Disposition {
    Disposition::NamedSet(name.to_owned())
}

fn names(list: &[&str]) -> Vec<ExeName> {
    list.iter()
        .map(|name| ExeName::new(name).unwrap())
        .collect()
}

fn utf16_raw(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn ini_only(text: &[u8]) -> ParsedMigration {
    parse_fakes(&FakeRegistry::new(), &fs_with(&[("setting.ini", text)]))
}

#[test]
fn the_field_map_ships_empty_for_settings() {
    assert!(FIELD_MAP.is_empty());
    assert_eq!(MAX_FIELD_BYTES, 64 * 1024);
}

#[test]
fn registry_values_are_classified() {
    let p = SETTINGS_REGISTRY_PATH;
    let mut registry = FakeRegistry::new();
    registry
        .set_value(
            p,
            "Apps",
            LegacyValue::String("game.exe; Editor.exe".into()),
        )
        .set_value(
            p,
            "Bad",
            LegacyValue::Other {
                kind: REG_SZ,
                bytes: vec![0x41],
            },
        )
        .set_value(p, "Big", LegacyValue::Qword(2))
        .set_value(p, "Blob", LegacyValue::Binary(vec![0, 255]))
        .set_value(p, "Flag", LegacyValue::Dword(1))
        .set_value(
            p,
            "Huge",
            LegacyValue::String("x".repeat(MAX_FIELD_BYTES + 1)),
        )
        .set_value(
            p,
            "More",
            LegacyValue::MultiString(vec!["chat.exe".into(), "Mail.exe".into()]),
        )
        .set_value(p, "Name", LegacyValue::String("plain text".into()))
        .set_value(p, "Path", LegacyValue::ExpandString("%APPDATA%\\x".into()))
        .set_value(
            p,
            "Raw",
            LegacyValue::Other {
                kind: 5,
                bytes: vec![1, 2, 3, 4],
            },
        )
        .set_value(
            p,
            "Wide",
            LegacyValue::Other {
                kind: REG_SZ,
                bytes: utf16_raw("tool.exe\0"),
            },
        )
        .set_value(
            p,
            "WideList",
            LegacyValue::Other {
                kind: REG_MULTI_SZ,
                bytes: utf16_raw("plain\0text\0\0"),
            },
        )
        .set_value(&format!("{p}\\Sub"), "Inner", LegacyValue::Dword(3));
    let parsed = parse_fakes(&registry, &FakeFs::new());

    assert_eq!(
        reports(&parsed, SourceKind::Registry),
        [
            (reg("Apps"), named("evkey-import-1")),
            (reg("Bad"), quarantined(QuarantineReason::Undecodable)),
            (reg("Big"), Disposition::Preserved),
            (reg("Blob"), Disposition::Preserved),
            (reg("Flag"), Disposition::Preserved),
            (reg("Huge"), quarantined(QuarantineReason::TooLarge)),
            (reg("More"), named("evkey-import-2")),
            (reg("Name"), Disposition::Preserved),
            (reg("Path"), Disposition::Preserved),
            (reg("Raw"), Disposition::Preserved),
            (reg("Wide"), named("evkey-import-3")),
            (reg("WideList"), Disposition::Preserved),
            (reg("Sub\\Inner"), Disposition::Preserved),
        ]
    );
    let sets = parsed.named_sets();
    assert_eq!(sets.len(), 3);
    assert_eq!(sets[0].name, "evkey-import-1");
    assert_eq!(sets[0].members, names(&["game.exe", "editor.exe"]));
    assert_eq!(sets[1].members, names(&["chat.exe", "mail.exe"]));
    assert_eq!(sets[2].members, names(&["tool.exe"]));
    assert!(sets.iter().all(|set| set.action == RuleAction::default()));
    assert_eq!(parsed.settings_count(), 0);
}

#[test]
fn preserved_registry_values_keep_their_raw_form() {
    let p = SETTINGS_REGISTRY_PATH;
    let mut registry = FakeRegistry::new();
    registry
        .set_value(
            p,
            "Apps",
            LegacyValue::String("game.exe; Editor.exe".into()),
        )
        .set_value(p, "Big", LegacyValue::Qword(u64::MAX))
        .set_value(p, "Blob", LegacyValue::Binary(vec![0, 255]))
        .set_value(p, "Flag", LegacyValue::Dword(1))
        .set_value(
            p,
            "More",
            LegacyValue::MultiString(vec!["chat.exe".into(), "Mail.exe".into()]),
        )
        .set_value(p, "Path", LegacyValue::ExpandString("%APPDATA%\\x".into()))
        .set_value(
            p,
            "Raw",
            LegacyValue::Other {
                kind: 5,
                bytes: vec![1, 2, 3, 4],
            },
        )
        .set_value(
            p,
            "Bad",
            LegacyValue::Other {
                kind: REG_SZ,
                bytes: vec![0x41],
            },
        )
        .set_value(&format!("{p}\\Sub"), "Inner", LegacyValue::Dword(3));
    let parsed = parse_fakes(&registry, &FakeFs::new());
    let preserved = &parsed.preserved()["registry"];
    assert_eq!(preserved["location"], json!("HKCU\\Software\\MyQ\\EVKey"));
    let values = &preserved["values"];
    assert_eq!(
        values["Apps"],
        json!({"type": "string", "data": "game.exe; Editor.exe"})
    );
    assert_eq!(values["Big"], json!({"type": "qword", "data": u64::MAX}));
    assert_eq!(values["Blob"], json!({"type": "binary", "hex": "00ff"}));
    assert_eq!(values["Flag"], json!({"type": "dword", "data": 1}));
    assert_eq!(
        values["More"],
        json!({"type": "multi-string", "data": ["chat.exe", "Mail.exe"]})
    );
    assert_eq!(
        values["Path"],
        json!({"type": "expand-string", "data": "%APPDATA%\\x"})
    );
    assert_eq!(
        values["Raw"],
        json!({"type": "other", "kind": 5, "hex": "01020304"})
    );
    assert_eq!(values.get("Bad"), None);
    assert_eq!(
        preserved["subkeys"]["Sub"]["values"]["Inner"],
        json!({"type": "dword", "data": 3})
    );
    assert_eq!(preserved["subkeys"]["Sub"]["subkeys"], json!({}));
}

#[test]
fn tsf_cache_hints_are_preserved_but_never_imported() {
    let mut registry = FakeRegistry::new();
    let v1 = format!("{TSF_CACHE_REGISTRY_PATH}\\v1");
    registry
        .set_value(&v1, "game.exe", LegacyValue::Dword(1))
        .set_value(&v1, "List", LegacyValue::String("a.exe;b.exe".into()));
    let parsed = parse_fakes(&registry, &FakeFs::new());
    assert_eq!(
        reports(&parsed, SourceKind::TsfCache),
        [
            (reg("v1\\List"), Disposition::Preserved),
            (reg("v1\\game.exe"), Disposition::Preserved),
        ]
    );
    assert!(parsed.named_sets().is_empty());
    assert_eq!(parsed.settings_count(), 0);
    let preserved = &parsed.preserved()["tsf-cache"];
    assert_eq!(
        preserved["location"],
        json!("HKCU\\Software\\EVKey\\TsfCache")
    );
    assert_eq!(
        preserved["subkeys"]["v1"]["values"]["game.exe"],
        json!({"type": "dword", "data": 1})
    );
}

#[test]
fn ini_entries_and_lines_are_classified() {
    let mut text = b"[Apps]\nExcluded = game.exe, editor.exe\n[Options]\nFlag=1\nflag=2\nno separator\nBad=\xff\nHuge=".to_vec();
    text.extend(vec![b'x'; MAX_FIELD_BYTES + 1]);
    text.extend(b"\n; comment\n\n");
    let parsed = ini_only(&text);
    assert_eq!(
        reports(&parsed, SourceKind::SettingsIni),
        [
            (ini("Apps", "Excluded", 2), named("evkey-import-1")),
            (
                FieldRef::Line(4),
                quarantined(QuarantineReason::DuplicateKey)
            ),
            (ini("Options", "flag", 5), Disposition::Preserved),
            (FieldRef::Line(6), quarantined(QuarantineReason::Malformed)),
            (
                FieldRef::Line(7),
                quarantined(QuarantineReason::Undecodable)
            ),
            (
                ini("Options", "Huge", 8),
                quarantined(QuarantineReason::TooLarge)
            ),
        ]
    );
    assert_eq!(
        parsed.named_sets()[0].members,
        names(&["game.exe", "editor.exe"])
    );
    let preserved = &parsed.preserved()["settings-ini"];
    assert_eq!(
        std::path::Path::new(preserved["location"].as_str().unwrap()),
        std::path::Path::new("evkey/setting.ini")
    );
    assert_eq!(preserved["encoding"], json!("utf-8"));
    assert_eq!(
        preserved["sections"],
        json!({
            "Apps": {"Excluded": "game.exe, editor.exe"},
            "Options": {"flag": "2"},
        })
    );
}

#[test]
fn utf16_ini_files_are_read() {
    let parsed = ini_only(&utf16le("[Options]\r\nFlag=1\r\n"));
    assert_eq!(
        reports(&parsed, SourceKind::SettingsIni),
        [(ini("Options", "Flag", 2), Disposition::Preserved)]
    );
    assert_eq!(
        parsed.preserved()["settings-ini"]["encoding"],
        json!("utf-16le")
    );
}

#[test]
fn named_sets_number_registry_values_before_ini_entries() {
    let mut registry = FakeRegistry::new();
    registry.set_value(
        SETTINGS_REGISTRY_PATH,
        "ZList",
        LegacyValue::String("z.exe".into()),
    );
    let parsed = parse_fakes(
        &registry,
        &fs_with(&[("setting.ini", b"[A]\nList=a.exe\n")]),
    );
    let sets: Vec<(&str, &[ExeName])> = parsed
        .named_sets()
        .iter()
        .map(|set| (set.name.as_str(), set.members.as_slice()))
        .collect();
    assert_eq!(
        sets,
        [
            ("evkey-import-1", names(&["z.exe"]).as_slice()),
            ("evkey-import-2", names(&["a.exe"]).as_slice()),
        ]
    );
}

#[test]
fn the_settings_tree_is_preserved_as_base64_and_never_parsed() {
    let tree = [0x00, 0x01, 0x02, 0xFD, 0xFE, 0xFF, b'[', b'a', b']', 0x0A];
    let parsed = parse_fakes(&FakeRegistry::new(), &fs_with(&[("setting.tree", &tree)]));
    assert_eq!(
        reports(&parsed, SourceKind::SettingsTree),
        [(FieldRef::Content, Disposition::Preserved)]
    );
    let preserved = &parsed.preserved()["settings-tree"];
    assert_eq!(preserved["size"], json!(10));
    assert_eq!(preserved["sha256"], json!(sha256_hex(&tree)));
    assert_eq!(preserved["base64"], json!("AAEC/f7/W2FdCg=="));
    assert!(parsed.named_sets().is_empty());

    for (bytes, encoded) in [
        (&b"\xff"[..], "/w=="),
        (&b"\xff\xff"[..], "//8="),
        (&b""[..], ""),
    ] {
        let parsed = parse_fakes(&FakeRegistry::new(), &fs_with(&[("setting.tree", bytes)]));
        assert_eq!(
            parsed.preserved()["settings-tree"]["base64"],
            json!(encoded)
        );
    }
}

#[test]
fn macros_are_imported_and_bad_lines_quarantined_by_number() {
    let text = "; header\nko:kho\u{302}ng\nbroken\ndc : được\n:x\n";
    let parsed = parse_fakes(
        &FakeRegistry::new(),
        &fs_with(&[("evkmacro.txt", text.as_bytes())]),
    );
    let macros: Vec<(u64, &str, &str)> = parsed
        .macros()
        .iter()
        .map(|entry| {
            (
                entry.id.0,
                entry.trigger.as_str(),
                entry.replacement.as_str(),
            )
        })
        .collect();
    assert_eq!(macros, [(1, "ko", "không"), (2, "dc", "được")]);
    assert_eq!(
        reports(&parsed, SourceKind::MacroText),
        [
            (
                FieldRef::Line(3),
                quarantined(QuarantineReason::Macro(LegacyIssueReason::MissingSeparator))
            ),
            (
                FieldRef::Line(5),
                quarantined(QuarantineReason::Macro(LegacyIssueReason::EmptyTrigger))
            ),
        ]
    );
    assert!(parsed.preserved().get("macro-text").is_none());
}

#[test]
fn the_fixture_set_is_classified_completely() {
    let parsed = parse_fakes(&evkey_registry(), &evkey_fs());
    assert_eq!(parsed.sources().len(), 5);
    assert_eq!(
        reports(&parsed, SourceKind::Registry),
        [
            (reg("SampleApps"), named("evkey-import-1")),
            (reg("SampleMode"), Disposition::Preserved),
            (reg("SampleTitle"), Disposition::Preserved),
            (reg("SampleTone"), Disposition::Preserved),
            (reg("Hotkeys\\SampleToggle"), Disposition::Preserved),
        ]
    );
    assert_eq!(
        reports(&parsed, SourceKind::TsfCache),
        [
            (reg("v1\\notepad.exe"), Disposition::Preserved),
            (reg("v1\\winword.exe"), Disposition::Preserved),
        ]
    );
    assert_eq!(
        reports(&parsed, SourceKind::SettingsIni),
        [
            (ini("General", "SampleMode", 3), Disposition::Preserved),
            (ini("General", "SampleCharset", 4), Disposition::Preserved),
            (
                ini("Exceptions", "SampleExcluded", 6),
                named("evkey-import-2")
            ),
            (ini("Exceptions", "SampleNote", 7), Disposition::Preserved),
            (ini("Options", "SampleTone", 9), Disposition::Preserved),
            (ini("Options", "SampleSpell", 10), Disposition::Preserved),
            (FieldRef::Line(11), quarantined(QuarantineReason::Malformed)),
        ]
    );
    assert_eq!(
        reports(&parsed, SourceKind::SettingsTree),
        [(FieldRef::Content, Disposition::Preserved)]
    );
    assert_eq!(parsed.macros().len(), 4);
    assert_eq!(
        parsed.quarantined(),
        [
            QuarantinedField {
                source: "settings-ini".into(),
                field: "line 11".into(),
                reason_code: "malformed".into(),
            },
            QuarantinedField {
                source: "macro-text".into(),
                field: "line 6".into(),
                reason_code: "missing-separator".into(),
            },
            QuarantinedField {
                source: "macro-text".into(),
                field: "line 7".into(),
                reason_code: "empty-trigger".into(),
            },
        ]
    );
    assert_eq!(parsed.quarantined_count(), 3);
    let counts = parsed.preserved_counts();
    assert_eq!(counts.get(&SourceKind::Registry), Some(&4));
    assert_eq!(counts.get(&SourceKind::TsfCache), Some(&2));
    assert_eq!(counts.get(&SourceKind::SettingsIni), Some(&5));
    assert_eq!(counts.get(&SourceKind::SettingsTree), Some(&1));
    assert_eq!(counts.get(&SourceKind::MacroText), None);
    assert_eq!(parsed.settings_count(), 0);
}

#[test]
fn quarantine_records_carry_no_content() {
    let mut registry = FakeRegistry::new();
    registry
        .set_value(
            SETTINGS_REGISTRY_PATH,
            "Huge",
            LegacyValue::String(SECRET.repeat(MAX_FIELD_BYTES / SECRET.len() + 1)),
        )
        .set_value(
            SETTINGS_REGISTRY_PATH,
            "Bad",
            LegacyValue::Other {
                kind: REG_SZ,
                bytes: SECRET.as_bytes().to_vec(),
            },
        );
    let mut ini_text = Vec::new();
    ini_text.extend(SECRET.as_bytes());
    ini_text.extend(b"\xff\n");
    ini_text.extend(format!("{SECRET} without separator\n[{SECRET}\n").as_bytes());
    ini_text.extend(format!("[S]\nKey={}\n", SECRET.repeat(7000)).as_bytes());
    let macro_text = format!("{SECRET}\n{SECRET} x:y\n");
    let parsed = parse_fakes(
        &registry,
        &fs_with(&[
            ("setting.ini", &ini_text),
            ("evkmacro.txt", macro_text.as_bytes()),
        ]),
    );
    // Registry: too large, undecodable. File: undecodable, two malformed lines, too large.
    // Macros: no separator, whitespace in the trigger.
    assert_eq!(parsed.quarantined_count(), 8);
    let record = serde_json::to_string(&parsed.quarantined()).unwrap();
    assert!(!record.contains(SECRET), "{record}");
    assert!(!parsed.preserved().to_string().contains(SECRET));
    assert!(!format!("{parsed:?}").contains(SECRET));
    for report in parsed.fields() {
        assert!(!format!("{report:?}").contains(SECRET), "{report:?}");
    }
}

#[test]
fn reason_codes_are_stable() {
    let codes = [
        (QuarantineReason::Undecodable, "undecodable"),
        (QuarantineReason::TooLarge, "too-large"),
        (QuarantineReason::Malformed, "malformed"),
        (QuarantineReason::DuplicateKey, "duplicate-key"),
        (QuarantineReason::InvalidName, "invalid-name"),
        (QuarantineReason::InvalidValue, "invalid-value"),
        (QuarantineReason::LimitExceeded, "limit-exceeded"),
        (
            QuarantineReason::Macro(LegacyIssueReason::MissingSeparator),
            "missing-separator",
        ),
        (
            QuarantineReason::Macro(LegacyIssueReason::InvalidEncoding),
            "invalid-encoding",
        ),
    ];
    for (reason, code) in codes {
        assert_eq!(reason.code(), code);
    }
}

#[test]
fn record_names_identify_fields_without_values() {
    let cases = [
        (reg("Sub\\Name"), "Sub\\Name".to_owned()),
        (
            FieldRef::RegistryKey("Sub\\Deep".to_owned()),
            "Sub\\Deep\\".to_owned(),
        ),
        (ini("S", "K", 7), "line 7".to_owned()),
        (FieldRef::Line(3), "line 3".to_owned()),
        (FieldRef::MacroEntry(4), "entry 4".to_owned()),
        (FieldRef::Content, "content".to_owned()),
        (reg("a\u{1}b"), "a\u{fffd}b".to_owned()),
        (reg(&"n".repeat(300)), "n".repeat(128)),
    ];
    for (field, name) in cases {
        assert_eq!(field.record_name(), name);
    }
}

#[test]
fn oversized_files_are_quarantined_whole() {
    let mut fs = FakeFs::new().with_max_bytes(16);
    fs.add_file("evkey/setting.ini", vec![b'a'; 17])
        .add_file("evkey/setting.tree", vec![0; 17])
        .add_file("evkey/evkmacro.txt", vec![b'k'; 17]);
    let parsed = parse_fakes(&FakeRegistry::new(), &fs);
    let whole: Vec<(SourceKind, FieldRef, Disposition)> = parsed
        .fields()
        .iter()
        .map(|report| {
            (
                report.source,
                report.field.clone(),
                report.disposition.clone(),
            )
        })
        .collect();
    let too_large = quarantined(QuarantineReason::TooLarge);
    assert_eq!(
        whole,
        [
            (
                SourceKind::SettingsIni,
                FieldRef::Content,
                too_large.clone()
            ),
            (
                SourceKind::SettingsTree,
                FieldRef::Content,
                too_large.clone()
            ),
            (SourceKind::MacroText, FieldRef::Content, too_large),
        ]
    );
    assert!(parsed.preserved().get("settings-ini").is_none());
    assert!(parsed.preserved().get("settings-tree").is_none());
    assert!(parsed.macros().is_empty());
    assert_eq!(parsed.sources()[0].sha256, "");
    assert_eq!(parsed.sources()[0].size, 17);
}

/// Matrix #44: registry and file settings both exist.
#[test]
fn matrix_44_registry_and_ini_settings_are_preserved_separately() {
    let mut registry = FakeRegistry::new();
    registry
        .set_value(SETTINGS_REGISTRY_PATH, "SampleTone", LegacyValue::Dword(1))
        .set_value(
            SETTINGS_REGISTRY_PATH,
            "SampleList",
            LegacyValue::String("a.exe".into()),
        );
    let fs = fs_with(&[(
        "setting.ini",
        b"[Options]\nSampleTone=0\nSampleList=b.exe\n",
    )]);

    let parsed = parse_fakes(&registry, &fs);
    assert_eq!(
        parsed.preserved()["registry"]["values"]["SampleTone"],
        json!({"type": "dword", "data": 1})
    );
    assert_eq!(
        parsed.preserved()["settings-ini"]["sections"]["Options"]["SampleTone"],
        json!("0")
    );
    assert_eq!(
        disposition(&parsed, SourceKind::Registry, &reg("SampleTone")),
        Disposition::Preserved
    );
    assert_eq!(
        disposition(
            &parsed,
            SourceKind::SettingsIni,
            &ini("Options", "SampleTone", 2)
        ),
        Disposition::Preserved
    );
    let sets: Vec<&str> = parsed
        .named_sets()
        .iter()
        .map(|set| set.name.as_str())
        .collect();
    assert_eq!(sets, ["evkey-import-1", "evkey-import-2"]);
    assert_eq!(parsed.named_sets()[0].members, names(&["a.exe"]));
    assert_eq!(parsed.settings_count(), 0);

    // With a mapping for the field, the registry value wins and the file value is kept raw.
    let mapped = parsed_with_map(&registry, &fs);
    assert_eq!(
        disposition(&mapped, SourceKind::Registry, &reg("SampleTone")),
        Disposition::Imported
    );
    assert_eq!(
        disposition(
            &mapped,
            SourceKind::SettingsIni,
            &ini("Options", "SampleTone", 2)
        ),
        Disposition::Preserved
    );
    assert_eq!(mapped.settings_count(), 1);
    assert_eq!(
        mapped.preserved()["settings-ini"]["sections"]["Options"]["SampleTone"],
        json!("0")
    );
}

/// Matrix #44: precedence falls through to the file when the registry value is unusable.
#[test]
fn matrix_44_an_invalid_registry_value_falls_back_to_the_file() {
    let mut registry = FakeRegistry::new();
    registry.set_value(SETTINGS_REGISTRY_PATH, "sampletone", LegacyValue::Dword(7));
    let fs = fs_with(&[("setting.ini", b"[OPTIONS]\nSampleTone=1\n")]);
    let mapped = parsed_with_map(&registry, &fs);
    assert_eq!(
        disposition(&mapped, SourceKind::Registry, &reg("sampletone")),
        quarantined(QuarantineReason::InvalidValue)
    );
    assert_eq!(
        disposition(
            &mapped,
            SourceKind::SettingsIni,
            &ini("OPTIONS", "SampleTone", 2)
        ),
        Disposition::Imported
    );
    assert_eq!(mapped.settings_count(), 1);

    let file_only = parsed_with_map(&FakeRegistry::new(), &fs);
    assert_eq!(file_only.settings_count(), 1);
    let neither = parsed_with_map(
        &FakeRegistry::new(),
        &fs_with(&[("setting.ini", b"[Options]\nSampleTone=x\n")]),
    );
    assert_eq!(neither.settings_count(), 0);
    assert_eq!(
        disposition(
            &neither,
            SourceKind::SettingsIni,
            &ini("Options", "SampleTone", 2)
        ),
        quarantined(QuarantineReason::InvalidValue)
    );
}

/// Matrix #45: one corrupt field is quarantined; the others are kept.
#[test]
fn matrix_45_one_corrupt_field_is_quarantined_alone() {
    let mut registry = FakeRegistry::new();
    registry
        .set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::Dword(1))
        .set_value(
            SETTINGS_REGISTRY_PATH,
            "B",
            LegacyValue::Other {
                kind: REG_SZ,
                bytes: vec![0xFF],
            },
        )
        .set_value(SETTINGS_REGISTRY_PATH, "C", LegacyValue::String("c".into()));
    let fs = fs_with(&[("setting.ini", b"[S]\nA=1\nB=2\nC=\xff\nD=4\nE=5\n")]);
    let parsed = parse_fakes(&registry, &fs);
    assert_eq!(
        reports(&parsed, SourceKind::Registry),
        [
            (reg("A"), Disposition::Preserved),
            (reg("B"), quarantined(QuarantineReason::Undecodable)),
            (reg("C"), Disposition::Preserved),
        ]
    );
    assert_eq!(
        parsed.preserved()["settings-ini"]["sections"],
        json!({"S": {"A": "1", "B": "2", "D": "4", "E": "5"}})
    );
    assert_eq!(
        parsed.quarantined(),
        [
            QuarantinedField {
                source: "registry".into(),
                field: "B".into(),
                reason_code: "undecodable".into(),
            },
            QuarantinedField {
                source: "settings-ini".into(),
                field: "line 4".into(),
                reason_code: "undecodable".into(),
            },
        ]
    );
}

/// A registry whose subkey names cannot all be addressed.
struct OddNames;

impl LegacyRegistry for OddNames {
    fn values(&self, path: &str) -> Result<Option<Vec<(String, LegacyValue)>>, MigrateError> {
        Ok(if path == SETTINGS_REGISTRY_PATH {
            Some(vec![("A".to_owned(), LegacyValue::Dword(1))])
        } else if path == format!("{SETTINGS_REGISTRY_PATH}\\Ok") {
            Some(vec![("B".to_owned(), LegacyValue::Dword(2))])
        } else {
            None
        })
    }

    fn subkeys(&self, path: &str) -> Result<Option<Vec<String>>, MigrateError> {
        Ok(if path == SETTINGS_REGISTRY_PATH {
            Some(vec!["x\\y".to_owned(), "Ok".to_owned(), String::new()])
        } else {
            Some(Vec::new())
        })
    }
}

#[test]
fn unreadable_registry_subkeys_are_quarantined() {
    let parsed = parse(&discover(&OddNames, &FakeFs::new(), [FOLDER]).unwrap());
    assert_eq!(
        reports(&parsed, SourceKind::Registry),
        [
            (reg("A"), Disposition::Preserved),
            (reg("Ok\\B"), Disposition::Preserved),
            (
                FieldRef::RegistryKey(String::new()),
                quarantined(QuarantineReason::InvalidName)
            ),
            (
                FieldRef::RegistryKey("x\\y".to_owned()),
                quarantined(QuarantineReason::InvalidName)
            ),
        ]
    );

    let mut deep = FakeRegistry::new();
    let levels: Vec<String> = (1..=9).map(|level| format!("L{level}")).collect();
    deep.set_value(
        &format!("{SETTINGS_REGISTRY_PATH}\\{}", levels.join("\\")),
        "V",
        LegacyValue::Dword(1),
    );
    let parsed = parse_fakes(&deep, &FakeFs::new());
    assert_eq!(
        reports(&parsed, SourceKind::Registry),
        [(
            FieldRef::RegistryKey(levels.join("\\")),
            quarantined(QuarantineReason::LimitExceeded)
        )]
    );
}

/// Returns `count` DWORD values for the settings key.
struct BulkRegistry {
    count: usize,
    bytes: usize,
}

impl LegacyRegistry for BulkRegistry {
    fn values(&self, path: &str) -> Result<Option<Vec<(String, LegacyValue)>>, MigrateError> {
        if path != SETTINGS_REGISTRY_PATH {
            return Ok(None);
        }
        Ok(Some(
            (0..self.count)
                .map(|index| {
                    let value = if self.bytes == 0 {
                        LegacyValue::Dword(1)
                    } else {
                        LegacyValue::String("x".repeat(self.bytes))
                    };
                    (format!("v{index:06}"), value)
                })
                .collect(),
        ))
    }

    fn subkeys(&self, _path: &str) -> Result<Option<Vec<String>>, MigrateError> {
        Ok(Some(Vec::new()))
    }
}

#[test]
fn field_reports_are_capped_per_source() {
    let registry = BulkRegistry {
        count: MAX_FIELDS_PER_SOURCE + 5,
        bytes: 0,
    };
    let parsed = parse(&discover(&registry, &FakeFs::new(), [FOLDER]).unwrap());
    let registry_reports = reports(&parsed, SourceKind::Registry);
    assert_eq!(registry_reports.len(), MAX_FIELDS_PER_SOURCE + 1);
    assert_eq!(
        registry_reports.last(),
        Some(&(
            FieldRef::Content,
            quarantined(QuarantineReason::LimitExceeded)
        ))
    );
}

#[test]
fn preserved_data_stays_within_its_budget() {
    let registry = BulkRegistry {
        count: 200,
        bytes: 60_000,
    };
    let parsed = parse(&discover(&registry, &FakeFs::new(), [FOLDER]).unwrap());
    let registry_reports = reports(&parsed, SourceKind::Registry);
    assert_eq!(registry_reports.len(), 200);
    assert_eq!(registry_reports[0].1, Disposition::Preserved);
    assert_eq!(
        registry_reports[199].1,
        quarantined(QuarantineReason::LimitExceeded)
    );
    let size = serde_json::to_string_pretty(parsed.preserved())
        .unwrap()
        .len();
    assert!(size <= MAX_PRESERVED_BYTES, "{size}");
    assert!(size > MAX_PRESERVED_BYTES / 2, "{size}");
}

#[test]
fn the_recorded_quarantine_list_is_capped() {
    let text = "bad line\n".repeat(MAX_RECORDED_QUARANTINE + 5);
    let parsed = ini_only(text.as_bytes());
    assert_eq!(parsed.quarantined_count(), MAX_RECORDED_QUARANTINE + 5);
    let recorded = parsed.quarantined();
    assert_eq!(recorded.len(), MAX_RECORDED_QUARANTINE + 1);
    assert_eq!(
        recorded.last(),
        Some(&QuarantinedField {
            source: "*".into(),
            field: "+5".into(),
            reason_code: "limit-exceeded".into(),
        })
    );
}

#[test]
fn parsing_is_deterministic() {
    let report = discover(&evkey_registry(), &evkey_fs(), [FOLDER]).unwrap();
    let first = parse(&report);
    let second = parse(&report);
    assert_eq!(first.fields(), second.fields());
    assert_eq!(first.preserved(), second.preserved());
    assert_eq!(first.named_sets(), second.named_sets());
    assert_eq!(first.macros(), second.macros());
    assert_eq!(first.sources(), report.fingerprints().as_slice());
}

#[test]
fn removing_a_discovered_source_leaves_it_out() {
    let mut report = discover(&evkey_registry(), &evkey_fs(), [FOLDER]).unwrap();
    report
        .sources
        .retain(|source| source.kind != SourceKind::MacroText);
    let parsed = parse(&report);
    assert!(parsed.macros().is_empty());
    assert_eq!(parsed.sources().len(), 4);
}

#[test]
fn debug_output_shows_counts_only() {
    let parsed = parse_fakes(&evkey_registry(), &evkey_fs());
    let text = format!("{parsed:?}");
    assert!(!text.contains("không"), "{text}");
    assert!(!text.contains("Bộ gõ"), "{text}");
}

#[test]
fn section_structure_counts_against_the_preserved_budget() {
    let mut text = String::new();
    for index in 0..9_000 {
        text.push_str(&format!("[{index:05}{}]\nk=v\n", "s".repeat(1_000)));
    }
    let parsed = ini_only(text.as_bytes());
    let size = serde_json::to_string_pretty(parsed.preserved())
        .unwrap()
        .len();
    assert!(size <= MAX_PRESERVED_BYTES, "{size}");
    let reports = reports(&parsed, SourceKind::SettingsIni);
    assert_eq!(reports.len(), 9_000);
    assert_eq!(reports[0].1, Disposition::Preserved);
    assert_eq!(
        reports[8_999].1,
        quarantined(QuarantineReason::LimitExceeded)
    );
}

#[test]
fn the_recorded_quarantine_fits_the_record_even_with_long_names() {
    let p = SETTINGS_REGISTRY_PATH;
    let mut registry = FakeRegistry::new();
    for index in 0..MAX_RECORDED_QUARANTINE + 10 {
        let name = format!("{index:05}{}", "\u{1F600}".repeat(300));
        registry.set_value(
            p,
            &name,
            LegacyValue::Other {
                kind: REG_SZ,
                bytes: vec![0x41],
            },
        );
    }
    let parsed = parse_fakes(&registry, &FakeFs::new());
    let recorded = serde_json::to_string_pretty(&parsed.quarantined()).unwrap();
    assert!(recorded.len() <= 2 * 1024 * 1024, "{}", recorded.len());
}
