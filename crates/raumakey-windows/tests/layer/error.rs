//! `error`: HRESULT/Win32 mapping and privacy of the display form.

use std::io;

use raumakey_windows::error::{
    E_ACCESSDENIED, E_FAIL, E_UNEXPECTED, WinError, WinErrorKind, hresult_from_win32,
};

#[test]
fn win32_codes_map_to_facility_win32_hresults() {
    assert_eq!(hresult_from_win32(0), 0);
    assert_eq!(hresult_from_win32(5) as u32, 0x8007_0005);
    assert_eq!(hresult_from_win32(2) as u32, 0x8007_0002);
    assert_eq!(hresult_from_win32(1460) as u32, 0x8007_05B4);
    // Values that already look like HRESULTs pass through unchanged.
    assert_eq!(hresult_from_win32(0x8000_4005) as u32, 0x8000_4005);
    assert_eq!(E_ACCESSDENIED as u32, 0x8007_0005);
    assert_eq!(E_FAIL as u32, 0x8000_4005);
    assert_eq!(E_UNEXPECTED as u32, 0x8000_FFFF);
}

#[test]
fn win32_code_is_recovered_only_for_facility_win32() {
    let error = WinError::from_win32(183, "CreateMutexW");
    assert_eq!(error.code() as u32, 0x8007_00B7);
    assert_eq!(error.win32_code(), Some(183));
    assert_eq!(error.context(), "CreateMutexW");
    assert_eq!(WinError::new(E_FAIL, "x").win32_code(), None);
}

#[test]
fn kinds_classify_common_failures() {
    let cases = [
        (2, WinErrorKind::NotFound),
        (3, WinErrorKind::NotFound),
        (1168, WinErrorKind::NotFound),
        (5, WinErrorKind::AccessDenied),
        (183, WinErrorKind::AlreadyExists),
        (80, WinErrorKind::AlreadyExists),
        (1460, WinErrorKind::TimedOut),
        (258, WinErrorKind::TimedOut),
        (995, WinErrorKind::Cancelled),
        (1223, WinErrorKind::Cancelled),
        (231, WinErrorKind::Busy),
        (32, WinErrorKind::Busy),
        (87, WinErrorKind::InvalidInput),
        (123, WinErrorKind::InvalidInput),
        (50, WinErrorKind::NotSupported),
        (109, WinErrorKind::BrokenPipe),
        (233, WinErrorKind::BrokenPipe),
        // ERROR_INVALID_LABEL: a pipe server below medium integrity.
        (1299, WinErrorKind::UntrustedPeer),
        (1, WinErrorKind::Other),
    ];
    for (code, kind) in cases {
        assert_eq!(WinError::from_win32(code, "op").kind(), kind, "code {code}");
    }
    assert_eq!(
        WinError::new(E_ACCESSDENIED, "op").kind(),
        WinErrorKind::AccessDenied
    );
    assert_eq!(WinError::new(E_FAIL, "op").kind(), WinErrorKind::Other);
    assert_eq!(
        WinError::new(0x8000_4004_u32 as i32, "op").kind(),
        WinErrorKind::Cancelled
    );
}

#[test]
fn display_shows_only_the_static_context_and_the_code() {
    let error = WinError::from_win32(5, "OpenProcessToken");
    let text = error.to_string();
    assert!(text.contains("OpenProcessToken"), "{text}");
    assert!(text.contains("0x80070005"), "{text}");
    assert_eq!(
        text,
        format!("{error}"),
        "Display is deterministic and has no other fields"
    );
}

#[test]
fn with_context_replaces_the_context_only() {
    let error = WinError::from_win32(5, "a").with_context("b");
    assert_eq!(error.context(), "b");
    assert_eq!(error.win32_code(), Some(5));
}

#[test]
fn io_errors_round_trip_win32_codes() {
    let io_error: io::Error = WinError::from_win32(5, "op").into();
    assert_eq!(io_error.raw_os_error(), Some(5));
    assert_eq!(io_error.kind(), io::ErrorKind::PermissionDenied);
    let other: io::Error = WinError::new(E_FAIL, "op").into();
    assert_eq!(other.raw_os_error(), None);
    assert_eq!(other.kind(), io::ErrorKind::Other);
    let back = WinError::from_io(&io::Error::from_raw_os_error(2), "read");
    assert_eq!(back.kind(), WinErrorKind::NotFound);
    assert_eq!(back.context(), "read");
}

#[cfg(windows)]
#[test]
fn windows_core_errors_convert_with_their_code() {
    let core = windows::core::Error::from_hresult(windows::core::HRESULT(E_ACCESSDENIED));
    let error = WinError::from(core);
    assert_eq!(error.code(), E_ACCESSDENIED);
    // An empty windows error never turns into a success code.
    let empty = WinError::from(windows::core::Error::empty());
    assert_eq!(empty.code(), E_FAIL);
}
