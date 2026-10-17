//! Read-only source ports: registry values, the in-memory fakes, the `std::fs` reader, and the
//! typed error.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use raumakey_config::{
    AppRulesDocument, ConfigDocument, ConfigError, MacroDocument, MigrationRecord, StoreDocument,
};
use raumakey_migrate::{
    DocumentKind, FakeFs, FakeRegistry, LegacyRegistry, LegacyValue, MAX_SOURCE_BYTES,
    MigrateError, REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_QWORD, REG_SZ, SourceFs,
    StdSourceFs,
};

#[test]
fn legacy_values_report_registry_type_codes() {
    assert_eq!(LegacyValue::String("a".into()).type_code(), REG_SZ);
    assert_eq!(
        LegacyValue::ExpandString("a".into()).type_code(),
        REG_EXPAND_SZ
    );
    assert_eq!(LegacyValue::Binary(vec![1]).type_code(), REG_BINARY);
    assert_eq!(LegacyValue::Dword(7).type_code(), REG_DWORD);
    assert_eq!(
        LegacyValue::MultiString(Vec::new()).type_code(),
        REG_MULTI_SZ
    );
    assert_eq!(LegacyValue::Qword(7).type_code(), REG_QWORD);
    let other = LegacyValue::Other {
        kind: 5,
        bytes: vec![0, 0, 0, 1],
    };
    assert_eq!(other.type_code(), 5);
    assert_eq!(
        [
            REG_SZ,
            REG_EXPAND_SZ,
            REG_BINARY,
            REG_DWORD,
            REG_MULTI_SZ,
            REG_QWORD
        ],
        [1, 2, 3, 4, 7, 11]
    );
}

#[test]
fn legacy_value_sizes_count_data_bytes() {
    assert_eq!(LegacyValue::String("ab".into()).data_len(), 2);
    assert_eq!(LegacyValue::ExpandString("é".into()).data_len(), 2);
    // Each string plus one separator.
    assert_eq!(
        LegacyValue::MultiString(vec!["ab".into(), "c".into()]).data_len(),
        5
    );
    assert_eq!(LegacyValue::Dword(0).data_len(), 4);
    assert_eq!(LegacyValue::Qword(0).data_len(), 8);
    assert_eq!(LegacyValue::Binary(vec![0; 9]).data_len(), 9);
    assert_eq!(
        LegacyValue::Other {
            kind: 0,
            bytes: vec![1; 3]
        }
        .data_len(),
        3
    );
}

#[test]
fn legacy_value_debug_shows_no_data() {
    let values = [
        LegacyValue::String("private-setting".into()),
        LegacyValue::MultiString(vec!["private-setting".into()]),
        LegacyValue::Binary(b"private-setting".to_vec()),
    ];
    for value in values {
        let text = format!("{value:?}");
        assert!(!text.contains("private"), "{text}");
    }
}

#[test]
fn fake_registry_reads_keys_case_insensitively() {
    let mut registry = FakeRegistry::new();
    registry
        .set_value("Software\\Vendor\\App", "Name", LegacyValue::Dword(1))
        .set_value(
            "Software\\Vendor\\App\\Sub",
            "Inner",
            LegacyValue::String("x".into()),
        )
        .create_key("Software\\Vendor\\App\\Empty");

    let values = registry.values("software\\VENDOR\\app").unwrap().unwrap();
    assert_eq!(values, vec![("Name".to_owned(), LegacyValue::Dword(1))]);
    let mut subkeys = registry.subkeys("Software\\Vendor\\App").unwrap().unwrap();
    subkeys.sort();
    assert_eq!(subkeys, ["Empty", "Sub"]);
    assert_eq!(
        registry.subkeys("Software\\Vendor").unwrap().unwrap(),
        ["App"]
    );
    assert_eq!(
        registry.values("Software\\Vendor\\App\\Empty").unwrap(),
        Some(Vec::new())
    );
    assert_eq!(
        registry.subkeys("Software\\Vendor\\App\\Sub").unwrap(),
        Some(Vec::new())
    );
}

#[test]
fn fake_registry_reports_missing_keys_as_none() {
    let mut registry = FakeRegistry::new();
    registry.create_key("Software\\Present");
    assert_eq!(registry.values("Software\\Absent").unwrap(), None);
    assert_eq!(registry.subkeys("Software\\Absent").unwrap(), None);
    assert_eq!(
        registry.values("Software\\Present").unwrap(),
        Some(Vec::new())
    );
}

#[test]
fn fake_registry_replaces_a_value_with_the_same_name() {
    let mut registry = FakeRegistry::new();
    registry
        .set_value("K", "A", LegacyValue::Dword(1))
        .set_value("K", "B", LegacyValue::Dword(2))
        .set_value("K", "a", LegacyValue::Dword(3));
    let values = registry.values("K").unwrap().unwrap();
    assert_eq!(
        values,
        vec![
            ("a".to_owned(), LegacyValue::Dword(3)),
            ("B".to_owned(), LegacyValue::Dword(2)),
        ]
    );
}

#[test]
fn fake_registry_injects_failures() {
    let mut registry = FakeRegistry::new();
    registry.create_key("Software\\Locked").fail(
        "Software\\Locked",
        MigrateError::SourceIo(io::ErrorKind::PermissionDenied),
    );
    assert_eq!(
        registry.values("software\\locked"),
        Err(MigrateError::SourceIo(io::ErrorKind::PermissionDenied))
    );
    assert_eq!(
        registry.subkeys("Software\\Locked"),
        Err(MigrateError::SourceIo(io::ErrorKind::PermissionDenied))
    );
}

#[test]
fn fake_fs_reads_files_and_lists_directories() {
    let mut fs = FakeFs::new();
    fs.add_file("root/evkey/setting.ini", b"[a]\n".to_vec())
        .add_file("root/evkey/evkmacro.txt", b"ko:khong\n".to_vec())
        .add_dir("root/evkey/nested")
        .add_dir("root/empty");

    assert_eq!(
        fs.read(Path::new("root/evkey/setting.ini")).unwrap(),
        Some(b"[a]\n".to_vec())
    );
    assert_eq!(fs.read(Path::new("root/evkey/missing.ini")).unwrap(), None);
    assert_eq!(fs.read(Path::new("root/evkey")).unwrap(), None);
    let listed = fs.list(Path::new("root/evkey")).unwrap().unwrap();
    assert_eq!(
        listed,
        vec![
            PathBuf::from("root/evkey/evkmacro.txt"),
            PathBuf::from("root/evkey/nested"),
            PathBuf::from("root/evkey/setting.ini"),
        ]
    );
    assert_eq!(
        fs.list(Path::new("root")).unwrap().unwrap(),
        vec![PathBuf::from("root/empty"), PathBuf::from("root/evkey")]
    );
    assert_eq!(fs.list(Path::new("root/empty")).unwrap(), Some(Vec::new()));
    assert_eq!(fs.list(Path::new("root/absent")).unwrap(), None);
    assert_eq!(fs.list(Path::new("root/evkey/setting.ini")).unwrap(), None);
    assert_eq!(
        fs.file(Path::new("root/evkey/evkmacro.txt")),
        Some(&b"ko:khong\n"[..])
    );
}

#[test]
fn fake_fs_refuses_files_over_its_cap() {
    let mut fs = FakeFs::new().with_max_bytes(4);
    fs.add_file("d/small", b"1234".to_vec())
        .add_file("d/big", b"12345".to_vec());
    assert_eq!(
        fs.read(Path::new("d/small")).unwrap(),
        Some(b"1234".to_vec())
    );
    assert_eq!(
        fs.read(Path::new("d/big")),
        Err(MigrateError::SourceTooLarge { size: 5 })
    );
    assert_eq!(FakeFs::new().max_bytes(), MAX_SOURCE_BYTES);
}

#[test]
fn fake_fs_injects_failures() {
    let mut fs = FakeFs::new();
    fs.add_file("d/locked", b"x".to_vec())
        .fail(
            "d/locked",
            MigrateError::SourceIo(io::ErrorKind::PermissionDenied),
        )
        .fail("d", MigrateError::SourceIo(io::ErrorKind::Other));
    assert_eq!(
        fs.read(Path::new("d/locked")),
        Err(MigrateError::SourceIo(io::ErrorKind::PermissionDenied))
    );
    assert_eq!(
        fs.list(Path::new("d")),
        Err(MigrateError::SourceIo(io::ErrorKind::Other))
    );
}

#[test]
fn fakes_debug_shows_no_content() {
    let mut registry = FakeRegistry::new();
    registry.set_value("K", "N", LegacyValue::String("private-value".into()));
    let mut fs = FakeFs::new();
    fs.add_file("d/evkmacro.txt", b"private:macro".to_vec());
    let text = format!("{registry:?} {fs:?}");
    assert!(!text.contains("private"), "{text}");
}

#[test]
fn std_source_fs_reads_files_and_lists_directories() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("setting.ini"), b"[a]\r\nb=1\r\n").unwrap();
    fs::create_dir(dir.path().join("nested")).unwrap();
    let reader = StdSourceFs::new();

    assert_eq!(
        reader.read(&dir.path().join("setting.ini")).unwrap(),
        Some(b"[a]\r\nb=1\r\n".to_vec())
    );
    let listed: BTreeSet<PathBuf> = reader
        .list(dir.path())
        .unwrap()
        .unwrap()
        .into_iter()
        .collect();
    let expected: BTreeSet<PathBuf> = [dir.path().join("nested"), dir.path().join("setting.ini")]
        .into_iter()
        .collect();
    assert_eq!(listed, expected);
}

#[test]
fn std_source_fs_reports_missing_paths_and_directories_as_none() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("file.txt"), b"x").unwrap();
    let reader = StdSourceFs::default();
    assert_eq!(reader.read(&dir.path().join("absent.txt")).unwrap(), None);
    assert_eq!(reader.read(dir.path()).unwrap(), None);
    assert_eq!(reader.list(&dir.path().join("absent")).unwrap(), None);
    assert_eq!(reader.list(&dir.path().join("file.txt")).unwrap(), None);
}

#[test]
fn std_source_fs_refuses_files_over_its_cap() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.ini");
    fs::write(&path, [b'x'; 11]).unwrap();
    let reader = StdSourceFs::with_max_bytes(10);
    assert_eq!(reader.max_bytes(), 10);
    assert_eq!(
        reader.read(&path),
        Err(MigrateError::SourceTooLarge { size: 11 })
    );
    assert_eq!(StdSourceFs::new().max_bytes(), MAX_SOURCE_BYTES);
    assert_eq!(MAX_SOURCE_BYTES, 16 * 1024 * 1024);
    // Reading never changes the file.
    assert_eq!(fs::read(&path).unwrap(), [b'x'; 11]);
}

#[test]
fn io_errors_map_to_their_kind() {
    assert_eq!(
        MigrateError::from_io_kind(io::ErrorKind::PermissionDenied),
        MigrateError::SourceIo(io::ErrorKind::PermissionDenied)
    );
}

#[test]
fn errors_display_no_content_and_have_distinct_codes() {
    let errors = [
        MigrateError::SourceIo(io::ErrorKind::PermissionDenied),
        MigrateError::SourceTooLarge { size: 17 },
        MigrateError::NoSources,
        MigrateError::Store {
            document: DocumentKind::Macros,
            error: ConfigError::Corrupt,
        },
        MigrateError::Verification {
            document: DocumentKind::AppRules,
        },
        MigrateError::MacroIdsExhausted,
        MigrateError::RollbackFailed {
            cause: Box::new(MigrateError::Verification {
                document: DocumentKind::Config,
            }),
            document: DocumentKind::Config,
        },
    ];
    let mut codes = BTreeSet::new();
    for error in &errors {
        let text = error.to_string();
        assert!(!text.is_empty());
        assert!(codes.insert(error.diag_code().0), "{error:?}");
        assert!((8000..8100).contains(&error.diag_code().0));
    }
    assert!(errors[3].to_string().contains("raumakey-macros.toml"));
    assert!(errors[6].to_string().contains("raumakey.toml"));
}

#[test]
fn document_kinds_name_their_files() {
    assert_eq!(DocumentKind::Config.file_name(), "raumakey.toml");
    assert_eq!(DocumentKind::Macros.file_name(), "raumakey-macros.toml");
    assert_eq!(
        DocumentKind::AppRules.file_name(),
        "raumakey-app-rules.toml"
    );
    assert_eq!(DocumentKind::Record.file_name(), "raumakey-migration.json");
}

/// The names come from the documents themselves, so they cannot drift apart.
#[test]
fn document_kinds_use_the_store_file_names() {
    assert_eq!(DocumentKind::Config.file_name(), ConfigDocument::FILE_NAME);
    assert_eq!(DocumentKind::Macros.file_name(), MacroDocument::FILE_NAME);
    assert_eq!(
        DocumentKind::AppRules.file_name(),
        AppRulesDocument::FILE_NAME
    );
    assert_eq!(DocumentKind::Record.file_name(), MigrationRecord::FILE_NAME);
}
