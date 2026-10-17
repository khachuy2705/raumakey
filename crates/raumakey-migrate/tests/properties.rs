//! Property tests: the parsers accept any input without panicking and keep their invariants.

use std::collections::BTreeSet;

use proptest::prelude::*;
use raumakey_macro::normalize_entry;
use raumakey_migrate::{
    Disposition, FakeFs, FakeRegistry, IniDocument, LegacyValue, MAX_RECORDED_QUARANTINE,
    SETTINGS_REGISTRY_PATH, TSF_CACHE_REGISTRY_PATH, discover, exe_list, parse, parse_ini,
};

/// Invariants of every parse result.
fn check_ini(document: &IniDocument) {
    let mut previous = 0;
    let mut keys = BTreeSet::new();
    for entry in &document.entries {
        assert!(entry.line > previous, "entries out of order");
        previous = entry.line;
        assert!(!entry.key.is_empty());
        assert_eq!(entry.key.trim(), entry.key);
        assert_eq!(entry.value.trim(), entry.value);
        assert_eq!(entry.section.trim(), entry.section);
        assert!(
            keys.insert((entry.section.to_lowercase(), entry.key.to_lowercase())),
            "duplicate key kept"
        );
    }
    assert!(
        document
            .issues
            .windows(2)
            .all(|pair| pair[0].line <= pair[1].line)
    );
    assert!(document.issues.iter().all(|issue| issue.line >= 1));
}

fn ini_line() -> impl Strategy<Value = String> {
    prop_oneof![
        "[A-Za-z ]{0,8}".prop_map(|name| format!("[{name}]")),
        "[A-Za-z]{0,6}".prop_map(|name| format!("[{name}")),
        ("[A-Za-z ]{0,8}", "[ -~]{0,16}").prop_map(|(key, value)| format!("{key}={value}")),
        ("[a-c]{1,2}", "[a-z.;, ]{0,12}").prop_map(|(key, value)| format!("{key} = {value}")),
        "[;#][ -~]{0,12}",
        "[ \t]{0,3}",
        "\\PC{0,16}",
    ]
}

fn encode(text: &str, encoding: usize) -> Vec<u8> {
    match encoding {
        0 => text.as_bytes().to_vec(),
        1 => [&[0xEF, 0xBB, 0xBF][..], text.as_bytes()].concat(),
        2 => [0xFF, 0xFE]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
            .collect(),
        _ => [0xFE, 0xFF]
            .into_iter()
            .chain(text.encode_utf16().flat_map(u16::to_be_bytes))
            .collect(),
    }
}

fn legacy_value() -> impl Strategy<Value = LegacyValue> {
    prop_oneof![
        any::<String>().prop_map(LegacyValue::String),
        "[a-z]{1,6}\\.exe([;, ][a-z]{1,6}\\.exe){0,3}".prop_map(LegacyValue::String),
        any::<String>().prop_map(LegacyValue::ExpandString),
        prop::collection::vec(any::<String>(), 0..4).prop_map(LegacyValue::MultiString),
        any::<u32>().prop_map(LegacyValue::Dword),
        any::<u64>().prop_map(LegacyValue::Qword),
        prop::collection::vec(any::<u8>(), 0..64).prop_map(LegacyValue::Binary),
        (any::<u32>(), prop::collection::vec(any::<u8>(), 0..64))
            .prop_map(|(kind, bytes)| LegacyValue::Other { kind, bytes }),
        (1u32..=7, prop::collection::vec(any::<u8>(), 0..64))
            .prop_map(|(kind, bytes)| LegacyValue::Other { kind, bytes }),
    ]
}

proptest! {
    #[test]
    fn the_ini_parser_accepts_any_bytes(bytes in prop::collection::vec(any::<u8>(), 0..4096)) {
        check_ini(&parse_ini(&bytes));
    }

    #[test]
    fn the_ini_parser_reads_every_encoding_alike(
        lines in prop::collection::vec(ini_line(), 0..48),
        crlf in any::<bool>(),
    ) {
        let text = lines.join(if crlf { "\r\n" } else { "\n" });
        let utf8 = parse_ini(&encode(&text, 0));
        check_ini(&utf8);
        prop_assert!(utf8.entries.len() + utf8.issues.len() <= lines.len().max(1) * 2);
        for encoding in 1..4 {
            let other = parse_ini(&encode(&text, encoding));
            prop_assert_eq!(&other.entries, &utf8.entries);
            prop_assert_eq!(&other.issues, &utf8.issues);
        }
    }

    #[test]
    fn the_exe_list_heuristic_accepts_any_text(text in any::<String>()) {
        if let Some(members) = exe_list(&text) {
            prop_assert!(!members.is_empty());
            let unique: BTreeSet<&str> = members.iter().map(|member| member.as_str()).collect();
            prop_assert_eq!(unique.len(), members.len());
            for member in &members {
                prop_assert!(member.as_str().ends_with(".exe"));
                prop_assert_eq!(member.as_str().to_lowercase(), member.as_str());
            }
        }
    }

    #[test]
    fn the_exe_list_heuristic_reads_generated_lists(
        stems in prop::collection::vec("[a-zA-Z0-9_-]{1,12}", 1..8),
        separators in prop::collection::vec(prop::sample::select(vec![",", ";", "|", " ", "\t", "\r\n", "\n", " ; "]), 1..8),
    ) {
        let mut text = String::new();
        for (index, stem) in stems.iter().enumerate() {
            text.push_str(stem);
            text.push_str(".EXE");
            text.push_str(separators[index % separators.len()]);
        }
        let mut expected = Vec::new();
        for stem in &stems {
            let name = format!("{}.exe", stem.to_lowercase());
            if !expected.contains(&name) {
                expected.push(name);
            }
        }
        let members: Vec<String> = exe_list(&text)
            .unwrap()
            .into_iter()
            .map(String::from)
            .collect();
        prop_assert_eq!(members, expected);
    }

    #[test]
    fn parsing_accepts_any_sources(
        values in prop::collection::vec(("[A-Za-z0-9 ._-]{0,10}", any::<bool>(), legacy_value()), 0..10),
        hints in prop::collection::vec(("[a-z]{1,8}\\.exe", any::<u32>()), 0..4),
        ini in prop::collection::vec(any::<u8>(), 0..512),
        ini_text in prop::collection::vec(ini_line(), 0..16),
        macros in prop::collection::vec(any::<u8>(), 0..512),
        tree in prop::collection::vec(any::<u8>(), 0..64),
        text_ini in any::<bool>(),
    ) {
        let mut registry = FakeRegistry::new();
        for (name, nested, value) in &values {
            let path = if *nested {
                format!("{SETTINGS_REGISTRY_PATH}\\Sub")
            } else {
                SETTINGS_REGISTRY_PATH.to_owned()
            };
            registry.set_value(&path, name, value.clone());
        }
        for (exe, flag) in &hints {
            registry.set_value(&format!("{TSF_CACHE_REGISTRY_PATH}\\v1"), exe, LegacyValue::Dword(*flag));
        }
        let ini_bytes = if text_ini { ini_text.join("\n").into_bytes() } else { ini };
        let mut fs = FakeFs::new();
        fs.add_file("evkey/setting.ini", ini_bytes)
            .add_file("evkey/evkmacro.txt", macros)
            .add_file("evkey/setting.tree", tree);
        let report = discover(&registry, &fs, ["evkey"]).unwrap();
        let parsed = parse(&report);

        let fingerprints = report.fingerprints();
        prop_assert_eq!(parsed.sources(), fingerprints.as_slice());
        let names: BTreeSet<&str> = parsed.named_sets().iter().map(|set| set.name.as_str()).collect();
        prop_assert_eq!(names.len(), parsed.named_sets().len());
        for report in parsed.fields() {
            if let Disposition::NamedSet(name) = &report.disposition {
                prop_assert!(names.contains(name.as_str()));
            }
        }
        for entry in parsed.macros() {
            let normalized = normalize_entry(entry.clone());
            prop_assert_eq!(normalized.as_ref(), Ok(entry));
        }
        prop_assert!(parsed.quarantined().len() <= MAX_RECORDED_QUARANTINE + 1);
        prop_assert!(serde_json::to_string(parsed.preserved()).is_ok());
        prop_assert_eq!(parsed.settings_count(), 0);
    }
}
