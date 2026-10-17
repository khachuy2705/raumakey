//! `VS_VERSIONINFO` encoding and version parsing.

use crate::support::{Rng, find, parse_res, parse_version_node, u32_at};
use raumakey_resources::{
    FileType, LANG_EN_US, LANG_VIETNAMESE, RT_VERSION, ResWriter, ResourceError, VS_FF_PRERELEASE,
    VersionInfo, version_quad,
};

const FIXED_SIGNATURE: u32 = 0xFEEF_04BD;

#[test]
fn version_quad_parses_three_and_four_components() {
    assert_eq!(version_quad("0.1.0"), Ok([0, 1, 0, 0]));
    assert_eq!(version_quad("1.2.3.4"), Ok([1, 2, 3, 4]));
    assert_eq!(
        version_quad("65535.65535.65535.65535"),
        Ok([65535, 65535, 65535, 65535])
    );
}

#[test]
fn version_quad_ignores_semver_suffixes() {
    assert_eq!(version_quad("1.2.3-beta.1"), Ok([1, 2, 3, 0]));
    assert_eq!(version_quad("1.2.3+build.5"), Ok([1, 2, 3, 0]));
    assert_eq!(version_quad("1.2.3-rc-1.2+meta.7"), Ok([1, 2, 3, 0]));
}

#[test]
fn version_quad_accepts_every_semver_form() {
    for (text, quad) in [
        ("0.0.0", [0, 0, 0, 0]),
        ("10.20.30.40", [10, 20, 30, 40]),
        ("1.2.3-0", [1, 2, 3, 0]),
        ("1.2.3-rc.0", [1, 2, 3, 0]),
        // Alphanumeric pre-release identifiers may start with zero.
        ("1.2.3-0a.01b", [1, 2, 3, 0]),
        ("1.2.3-x-y.z", [1, 2, 3, 0]),
        // Build metadata identifiers may have leading zeros.
        ("1.2.3+001", [1, 2, 3, 0]),
        ("1.2.3+build-5.007", [1, 2, 3, 0]),
        ("1.0.0-alpha.10.beta+exp.sha.5114f85", [1, 0, 0, 0]),
    ] {
        assert_eq!(version_quad(text), Ok(quad), "{text:?}");
    }
}

#[test]
fn version_quad_rejects_malformed_text() {
    for text in [
        "",
        "1",
        "1.2",
        "1.2.3.4.5",
        "1.2.x",
        "1..3",
        ".1.2",
        "1.2.3.",
        "65536.0.0",
        "-1.2.3",
        "+1.2.3",
        "1.+2.3",
        " 1.2.3",
        "1.2.3 ",
        "1.2.3-",
        "1.2.3+",
        "1.2.3-a b",
        "1.2.3-a\0",
        "1.2.3.4-",
        "١.٢.٣",
        // Numeric components have no leading zeros.
        "01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2.3.04",
        "00.1.2",
        "01.002.0003",
        // Pre-release and build-metadata identifiers are non-empty.
        "1.2.3-..",
        "1.2.3-.a",
        "1.2.3-a.",
        "1.2.3-a..b",
        "1.2.3+..",
        "1.2.3+.a",
        "1.2.3+a.",
        "1.2.3+a..b",
        "1.2.3-a+.b",
        // Numeric pre-release identifiers have no leading zeros.
        "1.2.3-01",
        "1.2.3-00",
        "1.2.3-rc.01",
        "1.2.3-rc.007+meta",
    ] {
        assert_eq!(
            version_quad(text),
            Err(ResourceError::InvalidVersion),
            "{text:?}"
        );
    }
}

#[test]
fn version_tree_has_exact_lengths_and_padding() {
    let mut info = VersionInfo::new(FileType::App, [1, 2, 3, 4], [1, 2, 3, 0]);
    info.set_string("A", "bc").unwrap();
    let bytes = info.to_bytes().unwrap();
    assert_eq!(bytes.len(), 240);

    let root = parse_version_node(&bytes, 0);
    assert_eq!(root.key, "VS_VERSION_INFO");
    assert_eq!(root.length, 240);
    assert_eq!(root.value_length, 52);
    assert_eq!(root.kind, 0);
    assert_eq!(root.children.len(), 2);

    let string_file_info = &root.children[0];
    assert_eq!(string_file_info.key, "StringFileInfo");
    assert_eq!(string_file_info.offset, 92);
    assert_eq!(string_file_info.length, 78);
    assert_eq!(string_file_info.value_length, 0);
    assert_eq!(string_file_info.kind, 1);

    let table = string_file_info.child("040904B0");
    assert_eq!(table.offset, 128);
    assert_eq!(table.length, 42);
    assert_eq!(table.value_length, 0);
    assert_eq!(table.kind, 1);

    let string = table.child("A");
    assert_eq!(string.offset, 152);
    assert_eq!(string.length, 18);
    assert_eq!(string.value_length, 3, "characters including the NUL");
    assert_eq!(string.kind, 1);
    assert_eq!(string.text(), "bc");

    let var_file_info = &root.children[1];
    assert_eq!(var_file_info.key, "VarFileInfo");
    assert_eq!(var_file_info.offset, 172);
    assert_eq!(var_file_info.length, 68);
    assert_eq!(var_file_info.value_length, 0);
    assert_eq!(var_file_info.kind, 1);

    let translation = var_file_info.child("Translation");
    assert_eq!(translation.offset, 204);
    assert_eq!(translation.length, 36);
    assert_eq!(translation.value_length, 4);
    assert_eq!(translation.kind, 0);
    assert_eq!(translation.value, [0x09, 0x04, 0xB0, 0x04]);
}

#[test]
fn fixed_file_info_carries_versions_and_file_type() {
    let info = VersionInfo::new(FileType::App, [1, 2, 3, 4], [5, 6, 7, 8]);
    let bytes = info.to_bytes().unwrap();
    let root = parse_version_node(&bytes, 0);
    let fixed = &root.value;
    assert_eq!(fixed.len(), 52);
    assert_eq!(u32_at(fixed, 0), FIXED_SIGNATURE);
    assert_eq!(u32_at(fixed, 4), 0x0001_0000, "dwStrucVersion");
    assert_eq!(u32_at(fixed, 8), 0x0001_0002, "dwFileVersionMS");
    assert_eq!(u32_at(fixed, 12), 0x0003_0004, "dwFileVersionLS");
    assert_eq!(u32_at(fixed, 16), 0x0005_0006, "dwProductVersionMS");
    assert_eq!(u32_at(fixed, 20), 0x0007_0008, "dwProductVersionLS");
    assert_eq!(u32_at(fixed, 24), 0x3F, "dwFileFlagsMask");
    assert_eq!(u32_at(fixed, 28), 0, "dwFileFlags");
    assert_eq!(
        u32_at(fixed, 32),
        0x0004_0004,
        "dwFileOS = VOS_NT_WINDOWS32"
    );
    assert_eq!(u32_at(fixed, 36), 1, "dwFileType = VFT_APP");
    assert_eq!(u32_at(fixed, 40), 0, "dwFileSubtype");
    assert_eq!(u32_at(fixed, 44), 0, "dwFileDateMS");
    assert_eq!(u32_at(fixed, 48), 0, "dwFileDateLS");

    let dll = VersionInfo::new(FileType::Dll, [1, 2, 3, 4], [5, 6, 7, 8]);
    let bytes = dll.to_bytes().unwrap();
    let root = parse_version_node(&bytes, 0);
    assert_eq!(u32_at(&root.value, 36), 2, "dwFileType = VFT_DLL");
}

#[test]
fn raumakey_version_info_has_the_standard_strings() {
    let info = VersionInfo::raumakey(
        FileType::Dll,
        "0.1.0",
        "Raumakey text input processor",
        "raumakey_tip",
        "raumakey_tip.dll",
    )
    .unwrap();
    assert_eq!(info.file_version(), [0, 1, 0, 0]);
    assert_eq!(info.product_version(), [0, 1, 0, 0]);
    assert_eq!(info.file_type(), FileType::Dll);
    assert_eq!(info.file_flags(), 0);

    let bytes = info.to_bytes().unwrap();
    let root = parse_version_node(&bytes, 0);
    assert_eq!(usize::from(root.length), bytes.len());
    let table = root.child("StringFileInfo").child("040904B0");
    let expected = [
        ("CompanyName", "Raumakey contributors"),
        ("FileDescription", "Raumakey text input processor"),
        ("FileVersion", "0.1.0.0"),
        ("InternalName", "raumakey_tip"),
        (
            "LegalCopyright",
            "Copyright (C) 2026 Raumakey contributors. Licensed under GPL-3.0-or-later.",
        ),
        ("OriginalFilename", "raumakey_tip.dll"),
        ("ProductName", "Raumakey"),
        ("ProductVersion", "0.1.0"),
    ];
    assert_eq!(table.children.len(), expected.len());
    for (node, (key, value)) in table.children.iter().zip(expected) {
        assert_eq!(node.key, key);
        assert_eq!(node.kind, 1, "{key}");
        assert_eq!(node.text(), value, "{key}");
        assert_eq!(
            usize::from(node.value_length),
            value.encode_utf16().count() + 1,
            "{key} wValueLength counts characters including the NUL"
        );
    }
    assert_eq!(info.string("ProductName"), Some("Raumakey"));
    assert_eq!(info.string("Missing"), None);

    let translation = root.child("VarFileInfo").child("Translation");
    assert_eq!(translation.value, [0x09, 0x04, 0xB0, 0x04]);
    assert_eq!(u32_at(&root.value, 36), 2, "VFT_DLL");
}

#[test]
fn prerelease_versions_set_the_prerelease_flag() {
    let info = VersionInfo::raumakey(
        FileType::App,
        "1.2.3-beta.1",
        "Raumakey Vietnamese input method",
        "raumakey",
        "raumakey.exe",
    )
    .unwrap();
    assert_eq!(info.file_version(), [1, 2, 3, 0]);
    assert_eq!(info.file_flags(), 0x02, "VS_FF_PRERELEASE");
    assert_eq!(info.string("FileVersion"), Some("1.2.3.0"));
    assert_eq!(info.string("ProductVersion"), Some("1.2.3-beta.1"));
    let bytes = info.to_bytes().unwrap();
    let root = parse_version_node(&bytes, 0);
    assert_eq!(u32_at(&root.value, 28), 0x02);
    assert_eq!(u32_at(&root.value, 36), 1, "VFT_APP");

    assert_eq!(
        VersionInfo::raumakey(FileType::App, "1.2", "d", "i", "o.exe"),
        Err(ResourceError::InvalidVersion)
    );
}

#[test]
fn set_string_replaces_in_place_and_validates_text() {
    let mut info = VersionInfo::new(FileType::App, [1, 0, 0, 0], [1, 0, 0, 0]);
    info.set_string("First", "1").unwrap();
    info.set_string("Second", "2").unwrap();
    info.set_string("First", "one").unwrap();
    let bytes = info.to_bytes().unwrap();
    let root = parse_version_node(&bytes, 0);
    let table = root.child("StringFileInfo").child("040904B0");
    let keys: Vec<&str> = table
        .children
        .iter()
        .map(|node| node.key.as_str())
        .collect();
    assert_eq!(keys, ["First", "Second"]);
    assert_eq!(table.children[0].text(), "one");

    assert_eq!(info.set_string("", "x"), Err(ResourceError::InvalidText));
    assert_eq!(info.set_string("K\0", "x"), Err(ResourceError::InvalidText));
    assert_eq!(info.set_string("K", "x\0"), Err(ResourceError::InvalidText));
}

#[test]
fn translation_follows_language_and_code_page() {
    let mut info = VersionInfo::new(FileType::App, [1, 0, 0, 0], [1, 0, 0, 0]);
    info.set_translation(LANG_VIETNAMESE, 1200);
    info.set_string("ProductName", "Raumakey").unwrap();
    let bytes = info.to_bytes().unwrap();
    let root = parse_version_node(&bytes, 0);
    let table = root.child("StringFileInfo").child("042A04B0");
    assert_eq!(table.children.len(), 1);
    let translation = root.child("VarFileInfo").child("Translation");
    assert_eq!(translation.value, [0x2A, 0x04, 0xB0, 0x04]);
}

#[test]
fn oversized_version_blocks_are_rejected() {
    let mut info = VersionInfo::new(FileType::App, [1, 0, 0, 0], [1, 0, 0, 0]);
    info.set_string("Comments", &"x".repeat(40_000)).unwrap();
    assert_eq!(info.to_bytes(), Err(ResourceError::TooLarge));
    let mut writer = ResWriter::new();
    assert_eq!(writer.add_version_info(&info), Err(ResourceError::TooLarge));
    assert!(writer.is_empty());
}

#[test]
fn writer_stores_version_info_as_rt_version_one() {
    let info = VersionInfo::raumakey(
        FileType::App,
        "0.1.0",
        "Raumakey Vietnamese input method",
        "raumakey",
        "raumakey.exe",
    )
    .unwrap();
    let mut writer = ResWriter::new();
    writer.add_version_info(&info).unwrap();
    assert_eq!(
        writer.add_version_info(&info),
        Err(ResourceError::Duplicate)
    );
    let entries = parse_res(&writer.to_bytes());
    let entry = find(&entries, RT_VERSION, 1);
    assert_eq!(entry.language, LANG_EN_US);
    assert_eq!(entry.memory_flags, 0x0030, "MOVEABLE | PURE");
    assert_eq!(entry.data, info.to_bytes().unwrap());
    let root = parse_version_node(&entry.data, 0);
    assert_eq!(usize::from(root.length), entry.data.len());
    assert_eq!(u32_at(&root.value, 0), FIXED_SIGNATURE);
}

#[test]
fn random_string_tables_keep_every_length_consistent() {
    let mut rng = Rng::new(0x0056_4552_5349_4F4E);
    for round in 0..60 {
        let quad = [
            rng.below(0x10000) as u16,
            rng.below(0x10000) as u16,
            rng.below(0x10000) as u16,
            rng.below(0x10000) as u16,
        ];
        let file_type = if rng.below(2) == 0 {
            FileType::App
        } else {
            FileType::Dll
        };
        let mut info = VersionInfo::new(file_type, quad, quad);
        let mut expected: Vec<(String, String)> = Vec::new();
        for index in 0..rng.below(10) {
            let key = format!("K{index}{}", rng.text(6));
            let value = rng.text(30);
            info.set_string(&key, &value).unwrap();
            expected.push((key, value));
        }
        let bytes = info.to_bytes().unwrap();
        // `parse_version_node` checks alignment, padding, and that every wLength ends exactly at
        // the end of its value or last child.
        let root = parse_version_node(&bytes, 0);
        assert_eq!(usize::from(root.length), bytes.len(), "round {round}");
        assert_eq!(
            u32_at(&root.value, 8),
            (u32::from(quad[0]) << 16) | u32::from(quad[1])
        );
        assert_eq!(
            u32_at(&root.value, 12),
            (u32::from(quad[2]) << 16) | u32::from(quad[3])
        );
        let table = root.child("StringFileInfo").child("040904B0");
        assert_eq!(table.children.len(), expected.len(), "round {round}");
        for (node, (key, value)) in table.children.iter().zip(&expected) {
            assert_eq!(&node.key, key, "round {round}");
            assert_eq!(&node.text(), value, "round {round}");
            assert_eq!(
                usize::from(node.value_length),
                value.encode_utf16().count() + 1,
                "round {round}"
            );
        }
        let translation = root.child("VarFileInfo").child("Translation");
        assert_eq!(translation.value, [0x09, 0x04, 0xB0, 0x04]);
    }
}

#[test]
fn file_flags_keep_only_the_defined_bits() {
    let mut info = VersionInfo::new(FileType::App, [1, 0, 0, 0], [1, 0, 0, 0]);
    info.set_file_flags(0xFFFF_FFFF);
    assert_eq!(info.file_flags(), 0x3F, "VS_FFI_FILEFLAGSMASK");
    let bytes = info.to_bytes().unwrap();
    let root = parse_version_node(&bytes, 0);
    assert_eq!(u32_at(&root.value, 24), 0x3F, "dwFileFlagsMask");
    assert_eq!(u32_at(&root.value, 28), 0x3F, "dwFileFlags");

    info.set_file_flags(0x8000_0040 | VS_FF_PRERELEASE);
    assert_eq!(info.file_flags(), VS_FF_PRERELEASE);
    info.set_file_flags(0);
    assert_eq!(info.file_flags(), 0);
}
