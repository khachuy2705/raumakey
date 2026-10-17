//! `strings`: UTF-16 conversions for Win32 calls, and reading strings into a growing buffer
//! (driven by a fake that follows the `GetModuleFileNameW` contract: a string that does not fit
//! is truncated to the buffer length minus one, NUL-terminated, and the buffer length is
//! returned).

use raumakey_windows::error::{E_ACCESSDENIED, WinError, WinErrorKind, WinResult, win32};
use raumakey_windows::strings::{WideString, from_wide_until_nul, read_growing, to_wide};

/// `MAX_PATH`, the first buffer size of a module path.
const FIRST: usize = 260;
/// The longest path Windows supports plus the NUL.
const MAX: usize = 32_768;

fn fake_module_file_name(path: &[u16]) -> impl FnMut(&mut [u16]) -> WinResult<usize> + '_ {
    move |buffer: &mut [u16]| {
        if path.len() < buffer.len() {
            buffer[..path.len()].copy_from_slice(path);
            buffer[path.len()] = 0;
            Ok(path.len())
        } else {
            let fitting = buffer.len() - 1;
            buffer[..fitting].copy_from_slice(&path[..fitting]);
            buffer[fitting] = 0;
            Ok(buffer.len())
        }
    }
}

fn long_path(units: usize) -> Vec<u16> {
    let mut path: Vec<u16> = r"\\?\C:\".encode_utf16().collect();
    path.extend((0..units - path.len()).map(|index| u16::from(b'a' + (index % 26) as u8)));
    path
}

#[test]
fn a_string_longer_than_the_first_buffer_is_read_completely() {
    let path = long_path(1000);
    let mut calls = 0;
    let mut fill = fake_module_file_name(&path);
    let read = read_growing(FIRST, MAX, "GetModuleFileNameW", |buffer| {
        calls += 1;
        fill(buffer)
    })
    .unwrap();
    assert_eq!(read, path);
    assert_eq!(calls, 3, "260 -> 520 -> 1040 units");
}

#[test]
fn strings_that_just_fit_or_just_do_not_fit_are_complete() {
    for units in [FIRST - 1, FIRST, FIRST + 1, MAX - 1] {
        let path = long_path(units);
        let read = read_growing(
            FIRST,
            MAX,
            "GetModuleFileNameW",
            fake_module_file_name(&path),
        )
        .unwrap();
        assert_eq!(read.len(), units);
        assert_eq!(read, path);
    }
}

#[test]
fn a_string_that_never_fits_is_an_insufficient_buffer_error_of_the_operation() {
    let path = long_path(MAX);
    let error = read_growing(
        FIRST,
        MAX,
        "GetModuleFileNameW",
        fake_module_file_name(&path),
    )
    .unwrap_err();
    assert!(error.is_win32(win32::ERROR_INSUFFICIENT_BUFFER), "{error}");
    assert_eq!(error.context(), "GetModuleFileNameW");
}

#[test]
fn errors_of_the_call_are_returned_unchanged() {
    let error = read_growing(FIRST, MAX, "GetModuleFileNameW", |_| {
        Err(WinError::new(E_ACCESSDENIED, "the call"))
    })
    .unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::AccessDenied);
    assert_eq!(error.context(), "the call");
}

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
