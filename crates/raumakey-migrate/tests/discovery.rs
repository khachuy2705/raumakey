//! Source discovery: registry keys, folder precedence, and fingerprints.

use std::io;
use std::path::{Path, PathBuf};

use raumakey_config::{SourceFingerprint, sha256_hex};
use raumakey_migrate::{
    FakeFs, FakeRegistry, LegacyRegistry, LegacyValue, MACRO_TEXT_FILE, MigrateError,
    SETTINGS_INI_FILE, SETTINGS_REGISTRY_PATH, SETTINGS_TREE_FILE, SourceKind,
    TSF_CACHE_REGISTRY_PATH, discover,
};

const NO_FOLDERS: [&str; 0] = [];

fn settings_registry() -> FakeRegistry {
    let mut registry = FakeRegistry::new();
    registry
        .set_value(SETTINGS_REGISTRY_PATH, "SampleFlag", LegacyValue::Dword(1))
        .set_value(
            SETTINGS_REGISTRY_PATH,
            "SampleList",
            LegacyValue::String("game.exe;editor.exe".into()),
        )
        .set_value(
            &format!("{TSF_CACHE_REGISTRY_PATH}\\v1"),
            "notepad.exe",
            LegacyValue::Dword(1),
        );
    registry
}

/// A change applied to a registry fixture.
type Change = Box<dyn Fn(&mut FakeRegistry)>;

fn registry_fingerprint(registry: &FakeRegistry) -> (u64, String) {
    let report = discover(registry, &FakeFs::new(), NO_FOLDERS).unwrap();
    let source = report.source(SourceKind::Registry).unwrap();
    (source.size, source.sha256.clone())
}

#[test]
fn constants_name_the_documented_sources() {
    assert_eq!(SETTINGS_REGISTRY_PATH, "Software\\MyQ\\EVKey");
    assert_eq!(TSF_CACHE_REGISTRY_PATH, "Software\\EVKey\\TsfCache");
    assert_eq!(SETTINGS_INI_FILE, "setting.ini");
    assert_eq!(SETTINGS_TREE_FILE, "setting.tree");
    assert_eq!(MACRO_TEXT_FILE, "evkmacro.txt");
    let names: Vec<&str> = SourceKind::ALL.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(
        names,
        [
            "registry",
            "tsf-cache",
            "settings-ini",
            "settings-tree",
            "macro-text"
        ]
    );
}

#[test]
fn nothing_is_discovered_without_sources() {
    let report = discover(&FakeRegistry::new(), &FakeFs::new(), ["missing"]).unwrap();
    assert!(report.is_empty());
    assert!(report.sources.is_empty());
    assert_eq!(report.folder, None);
    assert!(report.fingerprints().is_empty());
}

#[test]
fn registry_keys_are_sources() {
    let report = discover(&settings_registry(), &FakeFs::new(), NO_FOLDERS).unwrap();
    let kinds: Vec<SourceKind> = report.sources.iter().map(|source| source.kind).collect();
    assert_eq!(kinds, [SourceKind::Registry, SourceKind::TsfCache]);
    let settings = report.source(SourceKind::Registry).unwrap();
    assert_eq!(settings.location, "HKCU\\Software\\MyQ\\EVKey");
    assert_eq!(settings.sha256.len(), 64);
    assert!(settings.size > 0);
    assert!(!settings.too_large());
    let cache = report.source(SourceKind::TsfCache).unwrap();
    assert_eq!(cache.location, "HKCU\\Software\\EVKey\\TsfCache");
    assert_ne!(cache.sha256, settings.sha256);
}

#[test]
fn an_empty_registry_key_is_still_a_source() {
    let mut registry = FakeRegistry::new();
    registry.create_key(SETTINGS_REGISTRY_PATH);
    let report = discover(&registry, &FakeFs::new(), NO_FOLDERS).unwrap();
    assert_eq!(report.sources.len(), 1);
    assert_eq!(report.sources[0].kind, SourceKind::Registry);
}

/// The canonical encoding documented in the `discover` module, built by hand.
#[test]
fn registry_fingerprint_hashes_the_documented_canonical_encoding() {
    let mut registry = FakeRegistry::new();
    registry.set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::Dword(1));
    let mut expected = b"raumakey-registry-v1\0".to_vec();
    expected.extend(1u64.to_le_bytes()); // one value
    expected.extend(1u64.to_le_bytes()); // name length
    expected.extend(b"A");
    expected.push(4); // Dword tag
    expected.extend(4u32.to_le_bytes()); // REG_DWORD
    expected.extend(1u32.to_le_bytes()); // data
    expected.extend(0u64.to_le_bytes()); // no subkeys
    expected.extend(0u64.to_le_bytes()); // no skipped subkeys
    let (size, sha256) = registry_fingerprint(&registry);
    assert_eq!(size, u64::try_from(expected.len()).unwrap());
    assert_eq!(sha256, sha256_hex(&expected));
}

#[test]
fn registry_fingerprints_do_not_depend_on_read_order() {
    let mut first = FakeRegistry::new();
    first
        .set_value(SETTINGS_REGISTRY_PATH, "B", LegacyValue::Dword(2))
        .set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::String("x".into()))
        .set_value(
            &format!("{SETTINGS_REGISTRY_PATH}\\Zeta"),
            "Z",
            LegacyValue::Qword(9),
        )
        .set_value(
            &format!("{SETTINGS_REGISTRY_PATH}\\Alpha"),
            "Y",
            LegacyValue::Binary(vec![1, 2]),
        );
    let mut second = FakeRegistry::new();
    second
        .set_value(
            &format!("{SETTINGS_REGISTRY_PATH}\\Alpha"),
            "Y",
            LegacyValue::Binary(vec![1, 2]),
        )
        .set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::String("x".into()))
        .set_value(
            &format!("{SETTINGS_REGISTRY_PATH}\\Zeta"),
            "Z",
            LegacyValue::Qword(9),
        )
        .set_value(SETTINGS_REGISTRY_PATH, "B", LegacyValue::Dword(2));
    assert_eq!(registry_fingerprint(&first), registry_fingerprint(&second));
}

#[test]
fn registry_fingerprints_change_with_every_value() {
    let base = || {
        let mut registry = FakeRegistry::new();
        registry
            .set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::Dword(1))
            .set_value(
                &format!("{SETTINGS_REGISTRY_PATH}\\Deep\\Er\\Est"),
                "N",
                LegacyValue::String("x".into()),
            );
        registry
    };
    let original = registry_fingerprint(&base());
    let variants: Vec<Change> = vec![
        Box::new(|r| {
            r.set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::Dword(2));
        }),
        Box::new(|r| {
            r.set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::Qword(1));
        }),
        Box::new(|r| {
            r.set_value(
                SETTINGS_REGISTRY_PATH,
                "A",
                LegacyValue::Other {
                    kind: 4,
                    bytes: 1u32.to_le_bytes().to_vec(),
                },
            );
        }),
        Box::new(|r| {
            r.set_value(SETTINGS_REGISTRY_PATH, "A2", LegacyValue::Dword(1));
        }),
        Box::new(|r| {
            r.set_value(
                &format!("{SETTINGS_REGISTRY_PATH}\\Deep\\Er\\Est"),
                "N",
                LegacyValue::ExpandString("x".into()),
            );
        }),
        Box::new(|r| {
            r.create_key(&format!("{SETTINGS_REGISTRY_PATH}\\Deep\\New"));
        }),
    ];
    for (index, change) in variants.iter().enumerate() {
        let mut registry = base();
        change(&mut registry);
        assert_ne!(registry_fingerprint(&registry), original, "variant {index}");
    }
}

/// A registry whose subkey disappears between the listing and the read.
struct VanishingRegistry;

impl LegacyRegistry for VanishingRegistry {
    fn values(&self, path: &str) -> Result<Option<Vec<(String, LegacyValue)>>, MigrateError> {
        Ok((path == SETTINGS_REGISTRY_PATH).then(|| vec![("A".to_owned(), LegacyValue::Dword(1))]))
    }

    fn subkeys(&self, path: &str) -> Result<Option<Vec<String>>, MigrateError> {
        Ok((path == SETTINGS_REGISTRY_PATH).then(|| vec!["Gone".to_owned()]))
    }
}

#[test]
fn vanished_subkeys_are_ignored() {
    let mut plain = FakeRegistry::new();
    plain.set_value(SETTINGS_REGISTRY_PATH, "A", LegacyValue::Dword(1));
    let vanishing = discover(&VanishingRegistry, &FakeFs::new(), NO_FOLDERS).unwrap();
    let source = vanishing.source(SourceKind::Registry).unwrap();
    assert_eq!(
        (source.size, source.sha256.clone()),
        registry_fingerprint(&plain)
    );
}

#[test]
fn the_first_folder_with_an_evkey_file_wins() {
    let mut fs = FakeFs::new();
    fs.add_file("chosen/evkmacro.txt", b"ko:khong\n".to_vec())
        .add_file("found/setting.ini", b"[a]\nb=1\n".to_vec())
        .add_file("found/evkmacro.txt", b"dc:duoc\n".to_vec());
    let report = discover(&FakeRegistry::new(), &fs, ["chosen", "found"]).unwrap();
    assert_eq!(report.folder, Some(PathBuf::from("chosen")));
    assert_eq!(report.sources.len(), 1);
    let macros = report.source(SourceKind::MacroText).unwrap();
    assert_eq!(
        Path::new(&macros.location),
        Path::new("chosen/evkmacro.txt")
    );
    assert_eq!(macros.sha256, sha256_hex(b"ko:khong\n"));
    assert_eq!(macros.size, 9);
    // A later folder never fills in a file the chosen folder lacks.
    assert!(report.source(SourceKind::SettingsIni).is_none());
}

#[test]
fn folders_without_evkey_files_are_skipped() {
    let mut fs = FakeFs::new();
    fs.add_file("empty/readme.txt", b"x".to_vec())
        .add_dir("empty/evkmacro.txt")
        .add_file("third/setting.ini", b"[a]\n".to_vec())
        .add_file("third/setting.tree", vec![0, 1, 2])
        .add_file("third/evkmacro.txt", b"ko:khong\n".to_vec());
    let report = discover(&FakeRegistry::new(), &fs, ["absent", "empty", "third"]).unwrap();
    assert_eq!(report.folder, Some(PathBuf::from("third")));
    let kinds: Vec<SourceKind> = report.sources.iter().map(|source| source.kind).collect();
    assert_eq!(
        kinds,
        [
            SourceKind::SettingsIni,
            SourceKind::SettingsTree,
            SourceKind::MacroText
        ]
    );
}

#[test]
fn file_names_match_case_insensitively() {
    let mut fs = FakeFs::new();
    fs.add_file("dir/SETTING.INI", b"[a]\n".to_vec())
        .add_file("dir/EvkMacro.TXT", b"ko:khong\n".to_vec());
    let report = discover(&FakeRegistry::new(), &fs, ["dir"]).unwrap();
    let ini = report.source(SourceKind::SettingsIni).unwrap();
    assert_eq!(Path::new(&ini.location), Path::new("dir/SETTING.INI"));
    let macros = report.source(SourceKind::MacroText).unwrap();
    assert_eq!(Path::new(&macros.location), Path::new("dir/EvkMacro.TXT"));
}

#[test]
fn the_exact_spelling_wins_over_other_spellings() {
    let mut fs = FakeFs::new();
    fs.add_file("dir/Setting.ini", b"other".to_vec())
        .add_file("dir/setting.ini", b"exact".to_vec());
    let report = discover(&FakeRegistry::new(), &fs, ["dir"]).unwrap();
    let ini = report.source(SourceKind::SettingsIni).unwrap();
    assert_eq!(ini.sha256, sha256_hex(b"exact"));
}

#[test]
fn every_source_kind_is_listed_in_a_fixed_order() {
    let mut fs = FakeFs::new();
    fs.add_file("dir/evkmacro.txt", b"ko:khong\n".to_vec())
        .add_file("dir/setting.tree", vec![9; 4])
        .add_file("dir/setting.ini", b"[a]\n".to_vec());
    let report = discover(&settings_registry(), &fs, ["dir"]).unwrap();
    let kinds: Vec<SourceKind> = report.sources.iter().map(|source| source.kind).collect();
    assert_eq!(kinds, SourceKind::ALL);
    let fingerprints = report.fingerprints();
    let expected: Vec<SourceFingerprint> = report
        .sources
        .iter()
        .map(|source| SourceFingerprint {
            kind: source.kind.as_str().to_owned(),
            location: source.location.clone(),
            sha256: source.sha256.clone(),
            size: source.size,
        })
        .collect();
    assert_eq!(fingerprints, expected);
    let tree = report.source(SourceKind::SettingsTree).unwrap();
    assert_eq!(tree.fingerprint(), expected[3]);
}

#[test]
fn oversized_files_are_sources_whose_content_is_not_read() {
    let mut fs = FakeFs::new().with_max_bytes(8);
    fs.add_file("dir/setting.tree", vec![7; 9])
        .add_file("dir/evkmacro.txt", b"ko:khong".to_vec());
    let report = discover(&FakeRegistry::new(), &fs, ["dir"]).unwrap();
    let tree = report.source(SourceKind::SettingsTree).unwrap();
    assert!(tree.too_large());
    assert_eq!(tree.size, 9);
    assert_eq!(tree.sha256, "");
    let macros = report.source(SourceKind::MacroText).unwrap();
    assert!(!macros.too_large());
    assert_eq!(macros.sha256, sha256_hex(b"ko:khong"));
}

#[test]
fn port_failures_abort_discovery() {
    let denied = MigrateError::SourceIo(io::ErrorKind::PermissionDenied);

    let mut registry = settings_registry();
    registry.fail(TSF_CACHE_REGISTRY_PATH, denied.clone());
    assert_eq!(
        discover(&registry, &FakeFs::new(), NO_FOLDERS).unwrap_err(),
        denied
    );

    let mut fs = FakeFs::new();
    fs.add_file("dir/setting.ini", b"[a]\n".to_vec())
        .fail("dir", denied.clone());
    assert_eq!(
        discover(&FakeRegistry::new(), &fs, ["dir"]).unwrap_err(),
        denied
    );

    let mut fs = FakeFs::new();
    fs.add_file("dir/setting.ini", b"[a]\n".to_vec())
        .fail("dir/setting.ini", denied.clone());
    assert_eq!(
        discover(&FakeRegistry::new(), &fs, ["dir"]).unwrap_err(),
        denied
    );
}

#[test]
fn discovered_sources_debug_shows_no_content() {
    let mut fs = FakeFs::new();
    fs.add_file("dir/evkmacro.txt", b"private:macro text".to_vec());
    let mut registry = FakeRegistry::new();
    registry.set_value(
        SETTINGS_REGISTRY_PATH,
        "N",
        LegacyValue::String("private-setting".into()),
    );
    let report = discover(&registry, &fs, ["dir"]).unwrap();
    let text = format!("{report:?}");
    assert!(!text.contains("private"), "{text}");
}
