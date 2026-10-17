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

#[cfg(windows)]
mod win {
    use raumakey_windows::dialog::{MessageKind, MessageResponse, message_box};

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
