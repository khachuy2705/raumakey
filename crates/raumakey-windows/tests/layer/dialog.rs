//! `dialog`: message box styles, texts, and answers (pure). Showing a box needs a person to
//! close it, so that is a manual test.

use raumakey_windows::dialog::{MessageKind, MessageResponse, message_box_texts, styles};
use raumakey_windows::error::WinErrorKind;

/// `message_box` prepares its strings with `message_box_texts` before it shows anything, so a
/// text with a NUL never reaches `MessageBoxW`.
#[test]
fn texts_with_nul_are_rejected_before_any_box_is_shown() {
    let (text, caption) = message_box_texts("Không thể ghi cấu hình.", "Raumakey").unwrap();
    assert_eq!(text.to_string_lossy(), "Không thể ghi cấu hình.");
    assert_eq!(caption.to_string_lossy(), "Raumakey");
    for (text, caption) in [("a\0b", "Raumakey"), ("text", "Rau\0makey")] {
        let error = message_box_texts(text, caption).unwrap_err();
        assert_eq!(
            error.kind(),
            WinErrorKind::InvalidInput,
            "{text:?}, {caption:?}"
        );
    }
}

#[test]
fn message_kinds_map_to_buttons_icons_and_foreground() {
    assert_eq!(styles::MB_OK, 0x0000_0000);
    assert_eq!(styles::MB_YESNO, 0x0000_0004);
    assert_eq!(styles::MB_ICONERROR, 0x0000_0010);
    assert_eq!(styles::MB_ICONQUESTION, 0x0000_0020);
    assert_eq!(styles::MB_ICONWARNING, 0x0000_0030);
    assert_eq!(styles::MB_ICONINFORMATION, 0x0000_0040);
    assert_eq!(styles::MB_SETFOREGROUND, 0x0001_0000);

    assert_eq!(
        MessageKind::Information.style(),
        styles::MB_OK | styles::MB_ICONINFORMATION | styles::MB_SETFOREGROUND
    );
    assert_eq!(
        MessageKind::Warning.style(),
        styles::MB_OK | styles::MB_ICONWARNING | styles::MB_SETFOREGROUND
    );
    assert_eq!(
        MessageKind::Error.style(),
        styles::MB_OK | styles::MB_ICONERROR | styles::MB_SETFOREGROUND
    );
    assert_eq!(
        MessageKind::Question.style(),
        styles::MB_YESNO | styles::MB_ICONQUESTION | styles::MB_SETFOREGROUND
    );
}

#[test]
fn only_the_buttons_of_these_kinds_are_answers() {
    assert_eq!(MessageResponse::from_id(1), Some(MessageResponse::Ok));
    assert_eq!(MessageResponse::from_id(6), Some(MessageResponse::Yes));
    assert_eq!(MessageResponse::from_id(7), Some(MessageResponse::No));
    // IDCANCEL, IDABORT, IDRETRY, IDIGNORE, IDTRYAGAIN, IDCONTINUE, and failure.
    for other in [0, 2, 3, 4, 5, 10, 11, -1] {
        assert_eq!(MessageResponse::from_id(other), None, "{other}");
    }
}

/// A box closed without an answer of the user: "No" for a question (it has no Cancel, so
/// `WM_CLOSE` does not close it), OK otherwise.
#[test]
fn a_dismissed_box_answers_no_to_a_question_and_ok_otherwise() {
    assert_eq!(MessageKind::Question.dismiss_answer(), MessageResponse::No);
    for kind in [
        MessageKind::Information,
        MessageKind::Warning,
        MessageKind::Error,
    ] {
        assert_eq!(kind.dismiss_answer(), MessageResponse::Ok, "{kind:?}");
    }
    for response in [
        MessageResponse::Ok,
        MessageResponse::Yes,
        MessageResponse::No,
    ] {
        assert_eq!(MessageResponse::from_id(response.id()), Some(response));
    }
}

#[cfg(windows)]
mod win {
    use std::sync::mpsc;
    use std::time::Duration;

    use raumakey_windows::dialog::{
        MessageKind, MessageResponse, dismiss_message_boxes, message_box, spawn_message_box,
    };
    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::window::{Window, WindowClass};
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WINDOW_EX_STYLE, WS_POPUP,
    };
    use windows::core::w;

    use crate::support::on_fresh_thread;

    /// Dismissing posts the answer to the dialog windows (class `#32770`, the class of a
    /// message box) of the box's thread, and to nothing else. This test uses a hidden window
    /// of the dialog class, so no box is shown.
    #[test]
    fn dismissing_posts_to_the_dialog_windows_of_the_thread_only() {
        on_fresh_thread(|| {
            // SAFETY: no preconditions.
            let thread = unsafe { GetCurrentThreadId() };
            assert_eq!(dismiss_message_boxes(thread, MessageResponse::No), 0);
            let class = WindowClass::register("test-not-a-box").unwrap();
            let other = Window::create_hidden(
                &class,
                "Raumakey test",
                Box::new(|_: HWND, _: u32, _, _| None),
            )
            .unwrap();
            assert_eq!(dismiss_message_boxes(thread, MessageResponse::No), 0);
            // SAFETY: the system dialog class, without WS_VISIBLE: nothing is shown; the
            // window is destroyed below on this thread.
            let dialog = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    w!("#32770"),
                    w!("Raumakey test"),
                    WS_POPUP,
                    0,
                    0,
                    0,
                    0,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .unwrap();
            assert_eq!(dismiss_message_boxes(thread, MessageResponse::No), 1);
            // SAFETY: a window of this thread.
            unsafe { DestroyWindow(dialog) }.unwrap();
            drop(other);
            // A thread that does not exist has no windows.
            assert_eq!(dismiss_message_boxes(u32::MAX, MessageResponse::Ok), 0);
        });
    }

    /// A text with a NUL is refused before a thread starts or a box is shown.
    #[test]
    fn a_detached_box_with_a_nul_is_refused_before_anything_is_shown() {
        let (sender, answers) = mpsc::channel();
        let error = spawn_message_box(
            "a\0b",
            "Raumakey",
            MessageKind::Information,
            move |answer| {
                let _ = sender.send(answer);
            },
        )
        .unwrap_err();
        assert_eq!(error.kind(), WinErrorKind::InvalidInput);
        assert!(answers.recv_timeout(Duration::from_millis(50)).is_err());
    }

    /// Shows a box on a thread of its own, then dismisses it after one second without waiting:
    /// it closes by itself, answering "No". Manual test.
    #[test]
    #[ignore = "manual: shows a message box for about a second"]
    fn manual_a_detached_box_is_dismissed() {
        let (sender, answers) = mpsc::channel();
        let shown = spawn_message_box(
            "Hộp này tự đóng sau một giây (thử nghiệm).",
            "Raumakey",
            MessageKind::Question,
            move |answer| {
                let _ = sender.send(answer);
            },
        )
        .unwrap();
        std::thread::sleep(Duration::from_secs(1));
        assert!(!shown.is_closed());
        shown.dismiss();
        let answer = answers.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(answer, Some(MessageResponse::No));
        assert!(shown.is_closed());
    }

    /// Shows two boxes; answer "Có" (Yes) to the question. Manual test.
    #[test]
    #[ignore = "manual: shows message boxes and waits for a person to answer"]
    fn manual_message_boxes_return_the_answer() {
        let response = message_box(
            None,
            "Không thể ghi cấu hình (thử nghiệm).",
            "Raumakey",
            MessageKind::Error,
        )
        .unwrap();
        assert_eq!(response, MessageResponse::Ok);
        let response = message_box(
            None,
            "Chọn \"Có\" để tiếp tục (thử nghiệm).",
            "Raumakey",
            MessageKind::Question,
        )
        .unwrap();
        assert_eq!(response, MessageResponse::Yes);
    }
}
