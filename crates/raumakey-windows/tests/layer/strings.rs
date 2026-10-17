//! `strings`: UTF-16 conversions for Win32 calls.

use raumakey_windows::strings::{WideString, from_wide_until_nul, to_wide};

#[test]
fn to_wide_appends_exactly_one_nul() {
    assert_eq!(to_wide(""), vec![0]);
    assert_eq!(to_wide("ab"), vec![0x61, 0x62, 0]);
    assert_eq!(to_wide("ế"), vec![0x1EBF, 0]);
    assert_eq!(to_wide("😀"), vec![0xD83D, 0xDE00, 0]);
}

#[test]
fn from_wide_stops_at_the_first_nul_or_the_end() {
    assert_eq!(from_wide_until_nul(&[0x61, 0x62, 0, 0x63]), "ab");
    assert_eq!(from_wide_until_nul(&[0x61, 0x62]), "ab");
    assert_eq!(from_wide_until_nul(&[]), "");
    assert_eq!(from_wide_until_nul(&[0]), "");
    // Ill-formed UTF-16 is replaced rather than rejected.
    assert_eq!(from_wide_until_nul(&[0xD800, 0x61, 0]), "\u{FFFD}a");
}

#[test]
fn wide_string_rejects_interior_nul() {
    assert!(WideString::new("a\0b").is_err());
    let wide = WideString::new("Software\\Raumakey").unwrap();
    assert_eq!(wide.as_units_with_nul().last(), Some(&0));
    assert_eq!(wide.as_units().len(), "Software\\Raumakey".len());
    assert!(!wide.as_units().contains(&0));
    assert_eq!(wide.to_string_lossy(), "Software\\Raumakey");
}

#[cfg(windows)]
#[test]
fn wide_string_builds_from_os_strings_and_paths() {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    use std::path::Path;

    let path = Path::new("C:\\Temp\\ế.txt");
    let wide = WideString::from_os(path.as_os_str()).unwrap();
    assert_eq!(
        wide.as_units(),
        "C:\\Temp\\ế.txt".encode_utf16().collect::<Vec<_>>()
    );
    // Unpaired surrogates survive the OsStr round trip.
    let raw = OsString::from_wide(&[0x61, 0xD800]);
    let wide = WideString::from_os(&raw).unwrap();
    assert_eq!(wide.as_units(), &[0x61, 0xD800]);
    let pcwstr = wide.as_pcwstr();
    assert!(!pcwstr.is_null());
}
