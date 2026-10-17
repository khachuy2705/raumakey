//! `foreground`: read-only queries about windows and processes.

use raumakey_windows::foreground::is_password_edit;

#[test]
fn password_detection_needs_an_edit_class_and_the_password_style() {
    assert!(is_password_edit("Edit", 0x20));
    assert!(is_password_edit("EDIT", 0x5001_00A0));
    assert!(is_password_edit("RichEdit20W", 0x20));
    assert!(is_password_edit("RICHEDIT50W", 0x20));
    assert!(!is_password_edit("Edit", 0x5001_0080));
    assert!(!is_password_edit("Chrome_RenderWidgetHostHWND", 0x20));
    assert!(!is_password_edit("", 0x20));
}

#[cfg(windows)]
mod win {
    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::foreground::{
        current_process_is_elevated, foreground_info, process_image_basename, window_class_name,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

    #[test]
    fn the_image_basename_of_this_process_is_the_test_binary() {
        let exe = std::env::current_exe().unwrap();
        let expected = exe.file_name().unwrap().to_string_lossy().to_lowercase();
        let name = process_image_basename(std::process::id()).unwrap();
        assert_eq!(name.as_str(), expected);
        let error = process_image_basename(0xFFFF_FFF1).unwrap_err();
        assert_ne!(error.kind(), WinErrorKind::Other, "{error}");
    }

    #[test]
    fn the_desktop_window_has_its_system_class() {
        // SAFETY: no preconditions.
        let desktop = unsafe { GetDesktopWindow() };
        assert_eq!(
            window_class_name(desktop.0 as isize).as_deref(),
            Some("#32769")
        );
        assert_eq!(window_class_name(0), None);
    }

    #[test]
    fn foreground_queries_are_safe_without_a_foreground_window() {
        let _ = current_process_is_elevated().unwrap();
        if let Some(info) = foreground_info() {
            assert_ne!(info.hwnd, 0);
            assert_ne!(info.thread_id, 0);
            // Debug output shows no window title (titles are never read).
            let _ = format!("{info:?}");
        }
    }
}
