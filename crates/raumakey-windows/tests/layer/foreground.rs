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
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::{Duration, Instant};

    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::foreground::{
        ForegroundWatcher, current_process_is_elevated, foreground_info, foreground_window,
        process_image_basename, window_class_name, window_process_id,
    };
    use raumakey_windows::window::{AppMessage, Window, WindowClass};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetDesktopWindow, MSG, PM_REMOVE, PeekMessageW,
    };

    use crate::support::on_fresh_thread;

    /// The private message of the watchers of these tests.
    const FOREGROUND: AppMessage = match AppMessage::new(4) {
        Some(message) => message,
        None => panic!("offset outside the WM_APP range"),
    };

    /// A message-only window of this thread that records the `wParam` of each [`FOREGROUND`].
    fn recording_window(label: &str) -> (Window, Rc<RefCell<Vec<usize>>>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&seen);
        let class = WindowClass::register(label).unwrap();
        let handler = move |_: HWND, message: u32, wparam: WPARAM, _: LPARAM| {
            (message == FOREGROUND.id()).then(|| {
                log.borrow_mut().push(wparam.0);
                LRESULT(0)
            })
        };
        let window = Window::create_message_only(&class, "", Box::new(handler)).unwrap();
        (window, seen)
    }

    #[test]
    fn the_process_of_a_window_is_read() {
        on_fresh_thread(|| {
            let (window, _seen) = recording_window("test-foreground-process");
            let hwnd = window.hwnd().0 as isize;
            assert_eq!(window_process_id(hwnd), Some(std::process::id()));
            drop(window);
            assert_eq!(window_process_id(hwnd), None, "a window that is gone");
            assert_eq!(window_process_id(0), None);
        });
    }

    /// Installing the out-of-context hook only observes: nothing comes to the foreground, and
    /// no event is awaited (a manual test below waits for them).
    #[test]
    fn foreground_watchers_are_installed_and_removed_on_their_thread() {
        on_fresh_thread(|| {
            let (window, _seen) = recording_window("test-foreground-watcher");
            let first = ForegroundWatcher::install(window.handle(), FOREGROUND).unwrap();
            let second = ForegroundWatcher::install(window.handle(), FOREGROUND).unwrap();
            let _ = format!("{first:?}");
            drop(first);
            drop(second);
            drop(window);
        });
    }

    /// Runs a message loop for 15 s with a watcher installed: switch between a few windows
    /// (Alt+Tab, or click them) meanwhile. Prints the executable and the class of each window
    /// that came to the foreground. Manual test only.
    #[test]
    #[ignore = "manual: switch between windows within 15 s"]
    fn manual_foreground_watcher_reports_each_switch() {
        on_fresh_thread(|| {
            let (window, seen) = recording_window("test-foreground-manual");
            let watcher = ForegroundWatcher::install(window.handle(), FOREGROUND).unwrap();
            let deadline = Instant::now() + Duration::from_secs(15);
            let mut message = MSG::default();
            while Instant::now() < deadline {
                // SAFETY: `message` is a valid out buffer; the messages belong to this thread.
                while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
                    // SAFETY: `message` was filled by PeekMessageW.
                    unsafe { DispatchMessageW(&message) };
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            drop(watcher);
            let seen = seen.borrow();
            assert!(!seen.is_empty(), "no window came to the foreground");
            for &hwnd in seen.iter() {
                let hwnd = hwnd as isize;
                let exe = window_process_id(hwnd).and_then(|pid| process_image_basename(pid).ok());
                println!("{exe:?} {:?}", window_class_name(hwnd));
            }
        });
    }

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

    #[test]
    fn the_foreground_window_names_its_focus_without_querying_the_process() {
        // Read-only; the desktop of a test run may have no foreground window at all.
        if let Some(window) = foreground_window() {
            assert_ne!(window.hwnd, 0);
            assert_ne!(window.thread_id, 0);
            assert_ne!(window.process_id, 0);
            if window.focus_hwnd == 0 {
                assert_eq!(window.focus_window_class, None);
                assert!(!window.focus_is_password);
            }
            let _ = format!("{window:?}");
            if let Some(info) = foreground_info()
                && info.hwnd == window.hwnd
            {
                assert_eq!(info.process_id, window.process_id);
                assert_eq!(info.thread_id, window.thread_id);
            }
        }
    }
}
