//! `registry` (pure part): the write guard and value encoding.

use raumakey_model::identity::TIP_CLSID;
use raumakey_windows::registry::{
    RegRoot, RegValue, RegValueError, RegView, is_owned_path, value_type,
};

fn tip_clsid_key() -> String {
    format!("Software\\Classes\\CLSID\\{TIP_CLSID}")
}

#[test]
fn owned_paths_are_the_product_root_and_the_tip_class() {
    let clsid = tip_clsid_key();
    let lower = clsid.to_ascii_lowercase();
    let allowed = [
        "Software\\Raumakey".to_owned(),
        "Software\\Raumakey\\Install".to_owned(),
        "SOFTWARE\\RAUMAKEY\\Tests-1-2\\a\\b".to_owned(),
        "software\\raumakey".to_owned(),
        clsid.clone(),
        format!("{clsid}\\InprocServer32"),
        lower.clone(),
        format!("{lower}\\inprocserver32"),
    ];
    for root in [RegRoot::CurrentUser, RegRoot::LocalMachine] {
        for path in &allowed {
            assert!(is_owned_path(root, path), "{root:?} {path}");
        }
    }
}

#[test]
fn everything_else_is_refused() {
    let clsid = tip_clsid_key();
    let refused = [
        String::new(),
        "Software".to_owned(),
        "Software\\".to_owned(),
        "\\Software\\Raumakey".to_owned(),
        "Software\\Raumakey\\".to_owned(),
        "Software\\\\Raumakey".to_owned(),
        "Software\\RaumakeyEvil".to_owned(),
        "Software\\Raumakey.".to_owned(),
        "Software\\Raumakey Evil".to_owned(),
        "Software\\Raumake".to_owned(),
        "Software\\Raumakeу".to_owned(),
        "Software/Raumakey".to_owned(),
        "Software\\Raumakey/..".to_owned(),
        "Software\\Raumakey\\..".to_owned(),
        "Software\\Raumakey\\..\\Microsoft".to_owned(),
        "Software\\Raumakey\\.".to_owned(),
        "Software\\Raumakey\\a\\..\\..\\Microsoft".to_owned(),
        "Software\\Raumakey\\a\0b".to_owned(),
        "Software\\Raumakey\0".to_owned(),
        "Software\\WOW6432Node\\Raumakey".to_owned(),
        "Software\\MyQ\\EVKey".to_owned(),
        "Software\\EVKey\\TsfCache".to_owned(),
        "Software\\Microsoft\\Windows\\CurrentVersion\\Run".to_owned(),
        format!("Software\\Microsoft\\CTF\\TIP\\{TIP_CLSID}"),
        "Software\\Classes".to_owned(),
        "Software\\Classes\\CLSID".to_owned(),
        "Software\\Classes\\CLSID\\{00000000-0000-0000-0000-000000000000}".to_owned(),
        format!("{clsid}x"),
        format!("{clsid}\\..\\{{00000000-0000-0000-0000-000000000000}}"),
        clsid
            .trim_start_matches("Software\\Classes\\CLSID\\")
            .to_owned(),
        format!(
            "Software\\Classes\\CLSID\\{}",
            TIP_CLSID.to_string().trim_matches(|c| c == '{' || c == '}')
        ),
        format!("Software\\Classes\\WOW6432Node\\CLSID\\{TIP_CLSID}"),
        format!("Software\\Classes\\CLSID\\{TIP_CLSID}\\"),
        "SYSTEM\\CurrentControlSet\\Services".to_owned(),
    ];
    for root in [RegRoot::CurrentUser, RegRoot::LocalMachine] {
        for path in &refused {
            assert!(!is_owned_path(root, path), "{root:?} {path:?}");
        }
    }
}

#[test]
fn views_map_to_wow64_access_flags() {
    assert_eq!(RegView::Default.sam_flags(), 0);
    assert_eq!(RegView::Wow64_64.sam_flags(), 0x100);
    assert_eq!(RegView::Wow64_32.sam_flags(), 0x200);
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn strings_decode_with_or_without_terminators() {
    let mut data = utf16le("Tiếng Việt");
    assert_eq!(
        RegValue::from_raw(value_type::REG_SZ, &data),
        RegValue::String("Tiếng Việt".to_owned())
    );
    data.extend_from_slice(&[0, 0, 0, 0]);
    assert_eq!(
        RegValue::from_raw(value_type::REG_SZ, &data),
        RegValue::String("Tiếng Việt".to_owned())
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_EXPAND_SZ, &utf16le("%SystemRoot%\0")),
        RegValue::ExpandString("%SystemRoot%".to_owned())
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_SZ, &[]),
        RegValue::String(String::new())
    );
}

#[test]
fn undecodable_data_is_kept_raw() {
    let odd = [0x61, 0x00, 0x62];
    assert_eq!(
        RegValue::from_raw(value_type::REG_SZ, &odd),
        RegValue::Other(value_type::REG_SZ, odd.to_vec())
    );
    let lone_surrogate = [0x00, 0xD8, 0x61, 0x00];
    assert_eq!(
        RegValue::from_raw(value_type::REG_SZ, &lone_surrogate),
        RegValue::Other(value_type::REG_SZ, lone_surrogate.to_vec())
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_DWORD, &[1, 2, 3]),
        RegValue::Other(value_type::REG_DWORD, vec![1, 2, 3])
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_QWORD, &[1; 4]),
        RegValue::Other(value_type::REG_QWORD, vec![1; 4])
    );
    assert_eq!(
        RegValue::from_raw(5, &[0, 0, 0, 1]),
        RegValue::Other(5, vec![0, 0, 0, 1])
    );
}

#[test]
fn numbers_and_binary_decode() {
    assert_eq!(
        RegValue::from_raw(value_type::REG_DWORD, &0x1234_5678_u32.to_le_bytes()),
        RegValue::Dword(0x1234_5678)
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_QWORD, &u64::MAX.to_le_bytes()),
        RegValue::Qword(u64::MAX)
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_BINARY, &[9, 8, 7]),
        RegValue::Binary(vec![9, 8, 7])
    );
}

#[test]
fn multi_strings_stop_at_the_empty_string() {
    let data = utf16le("a\0bc\0\0");
    assert_eq!(
        RegValue::from_raw(value_type::REG_MULTI_SZ, &data),
        RegValue::MultiString(vec!["a".to_owned(), "bc".to_owned()])
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_MULTI_SZ, &utf16le("a\0b")),
        RegValue::MultiString(vec!["a".to_owned(), "b".to_owned()])
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_MULTI_SZ, &utf16le("\0\0")),
        RegValue::MultiString(Vec::new())
    );
    assert_eq!(
        RegValue::from_raw(value_type::REG_MULTI_SZ, &[]),
        RegValue::MultiString(Vec::new())
    );
}

#[test]
fn encoding_round_trips_and_rejects_unrepresentable_values() {
    let values = [
        RegValue::String("Raumakey ế 😀".to_owned()),
        RegValue::String(String::new()),
        RegValue::ExpandString("%LOCALAPPDATA%\\Raumakey".to_owned()),
        RegValue::MultiString(vec!["a".to_owned(), "ế".to_owned()]),
        RegValue::MultiString(Vec::new()),
        RegValue::Dword(7),
        RegValue::Qword(1 << 40),
        RegValue::Binary(vec![0, 1, 2]),
        RegValue::Other(0, vec![5]),
    ];
    for value in values {
        let (kind, data) = value.to_raw().unwrap();
        assert_eq!(kind, value.value_type());
        assert_eq!(RegValue::from_raw(kind, &data), value, "{value:?}");
    }
    let (_, data) = RegValue::String("ab".to_owned()).to_raw().unwrap();
    assert_eq!(data, utf16le("ab\0"));
    let (_, data) = RegValue::MultiString(vec!["a".to_owned()])
        .to_raw()
        .unwrap();
    assert_eq!(data, utf16le("a\0\0"));
    assert_eq!(
        RegValue::String("a\0b".to_owned()).to_raw(),
        Err(RegValueError::InteriorNul)
    );
    assert_eq!(
        RegValue::MultiString(vec!["a".to_owned(), String::new()]).to_raw(),
        Err(RegValueError::EmptyMultiStringEntry)
    );
}

#[test]
fn debug_output_of_values_does_not_print_string_content() {
    let debug = format!("{:?}", RegValue::String("secret".to_owned()));
    assert!(!debug.contains("secret"), "{debug}");
    let debug = format!("{:?}", RegValue::MultiString(vec!["secret".to_owned()]));
    assert!(!debug.contains("secret"), "{debug}");
}
