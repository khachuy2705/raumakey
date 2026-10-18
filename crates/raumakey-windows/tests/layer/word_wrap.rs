//! `word_wrap`: wrapping at spaces only (pure, with a fake measure: six pixels per character)
//! and the width of a message box line; on Windows, wrapping for real labels and check boxes
//! (Common Controls 5 and 6) in dialogs that are never activated and are moved off-screen
//! before Windows would show them.
//!
//! What a control really draws is read back from its pixels: the control paints itself into a
//! memory bitmap (`WM_PRINTCLIENT`), and the rows with ink in its text area are counted as
//! lines. The texts drawn that way use letters without ascenders or descenders, so that lines
//! drawn one under another never touch.

use raumakey_windows::word_wrap::{message_text_width, wrap_words};

/// The fake measure: six pixels per character, whatever the character.
fn width(text: &str) -> i32 {
    6 * i32::try_from(text.chars().count()).unwrap()
}

/// Wraps `text` with [`width`] to `columns` characters.
fn wrap(text: &str, columns: i32) -> String {
    wrap_words(text, 6 * columns, width)
}

const AUTO_RESTORE: &str = "Giữ nguyên phím đã gõ khi từ không phải tiếng Việt";

#[test]
fn a_text_that_fits_is_left_as_it_is() {
    assert_eq!(wrap("Gõ tiếng Việt", 13), "Gõ tiếng Việt");
    assert_eq!(wrap(AUTO_RESTORE, 100), AUTO_RESTORE);
    assert_eq!(wrap("", 10), "");
}

/// Each line takes as many words as fit, breaks only where a space was, and the space at the
/// break goes.
#[test]
fn lines_break_at_spaces_only_and_take_as_many_words_as_fit() {
    assert_eq!(
        wrap(AUTO_RESTORE, 30),
        "Giữ nguyên phím đã gõ khi từ\nkhông phải tiếng Việt"
    );
    for columns in 5..=60 {
        let wrapped = wrap(AUTO_RESTORE, columns);
        let lines: Vec<&str> = wrapped.split('\n').collect();
        assert_eq!(lines.join(" "), AUTO_RESTORE, "{columns}");
        for (index, line) in lines.iter().enumerate() {
            // A word wider than the line stands alone (see below).
            assert!(
                width(line) <= 6 * columns || !line.contains(' '),
                "{columns}: {line:?}"
            );
            assert!(!line.starts_with(' ') && !line.ends_with(' '), "{line:?}");
            if let Some(next) = lines.get(index + 1) {
                let first = next.split(' ').next().unwrap();
                assert!(
                    width(&format!("{line} {first}")) > 6 * columns,
                    "{columns}: {first:?} would fit after {line:?}"
                );
            }
        }
    }
}

/// The measure gets whole lines: a measure that is not the sum of its words' widths (kerning,
/// overhang) decides as it measures the line.
#[test]
fn whole_lines_are_measured() {
    let mut asked = Vec::new();
    let overhead = |text: &str| {
        asked.push(text.to_owned());
        width(text) + 10
    };
    assert_eq!(wrap_words("aa bb cc", 6 * 5 + 10, overhead), "aa bb\ncc");
    assert!(asked.contains(&"aa bb".to_owned()), "{asked:?}");
    assert!(asked.contains(&"aa bb cc".to_owned()), "{asked:?}");
}

/// Line breaks of the text stay as they are (`\n`, `\r\n`, a lone `\r`); the breaks added are
/// `\n`.
#[test]
fn line_breaks_of_the_text_are_kept() {
    assert_eq!(wrap("a b\r\nc d\ne\rf", 100), "a b\r\nc d\ne\rf");
    assert_eq!(wrap("aa bb\r\ncc dd\nee", 2), "aa\nbb\r\ncc\ndd\nee");
    assert_eq!(wrap("a\n\nb", 10), "a\n\nb");
    assert_eq!(wrap("\n", 10), "\n");
    assert_eq!(wrap("\r\n\r\n", 10), "\r\n\r\n");
    assert_eq!(wrap("a\n", 10), "a\n");
}

/// Spaces inside a line stay; the spaces at a break go, however many; spaces at the start of
/// a line stay with its first word, and spaces at its end go.
#[test]
fn spaces_inside_a_line_stay_and_those_at_breaks_and_ends_go() {
    assert_eq!(wrap("aaa  bbb   ccc", 8), "aaa  bbb\nccc");
    assert_eq!(wrap("aaa  bbb   ccc", 100), "aaa  bbb   ccc");
    assert_eq!(wrap("  aaa bbb  ", 100), "  aaa bbb");
    assert_eq!(wrap("  aaa bbb", 5), "  aaa\nbbb");
    assert_eq!(wrap("aaa \nbbb   \r\nccc", 100), "aaa\nbbb\r\nccc");
    assert_eq!(wrap("a\n   \nb", 10), "a\n\nb");
    assert_eq!(wrap("   ", 10), "");
}

/// A word wider than the line (a path) stays whole, alone on its line.
#[test]
fn a_word_wider_than_the_line_stays_whole_on_its_own_line() {
    let path = r"C:\Users\Anh\AppData\Local\Raumakey\raumakey.toml";
    assert_eq!(
        wrap(&format!("Tệp: {path} ở đây"), 12),
        format!("Tệp:\n{path}\nở đây")
    );
    assert_eq!(wrap("abcdefghij", 3), "abcdefghij");
    assert_eq!(wrap("ab abcdefghij cd", 3), "ab\nabcdefghij\ncd");
    assert_eq!(wrap("a b c", 0), "a\nb\nc");
    assert_eq!(wrap_words("a b c", -5, width), "a\nb\nc");
}

/// Only the ASCII space breaks: a no-break space (U+00A0), a tab, and every other character
/// belong to the word.
#[test]
fn only_ascii_spaces_break() {
    assert_eq!(wrap("Ko\u{a0}→\u{a0}Không x", 9), "Ko\u{a0}→\u{a0}Không\nx");
    assert_eq!(wrap("a\tb c", 3), "a\tb\nc");
    assert_eq!(wrap("a\u{3000}b c", 3), "a\u{3000}b\nc");
}

/// Wrapping a wrapped text again at the same width changes nothing.
#[test]
fn wrapping_again_changes_nothing() {
    let texts = [
        AUTO_RESTORE,
        "Viết hoa theo cách gõ (Ko → Không, KO → KHÔNG)",
        "(trong %LOCALAPPDATA%\\Raumakey, vì thư mục của raumakey.exe không ghi được hoặc là thư \
         mục được bảo vệ)",
        "  aaa  bbb   ccc  \r\n\r\nddd eee\rfff ggg\n",
        "ab abcdefghij cd",
    ];
    for text in texts {
        for columns in [0, 1, 3, 8, 13, 21, 34, 55, 200] {
            let once = wrap(text, columns);
            assert_eq!(wrap(&once, columns), once, "{text:?} at {columns}");
        }
    }
}

/// The widest line of a message box: 300 pixels at 96 DPI (scaled with the DPI), at most 50
/// average characters, and at most a quarter of the work area, which counts as at least 25
/// average characters; the pixel limit holds always.
#[test]
fn message_box_lines_stay_well_inside_what_message_box_w_allows() {
    // A common screen at 100 %: the pixel limit.
    assert_eq!(message_text_width(1296, 96, 7), 300);
    assert_eq!(message_text_width(1920, 96, 7), 300);
    // 150 %: everything scales.
    assert_eq!(message_text_width(2880, 144, 10), 450);
    // A small font: 50 of its characters.
    assert_eq!(message_text_width(1296, 96, 5), 250);
    // Small screens: a quarter of the work area, but at least 25 characters.
    assert_eq!(message_text_width(1024, 96, 7), 256);
    assert_eq!(message_text_width(640, 96, 7), 175);
    // An unknown work area counts as the smallest.
    assert_eq!(message_text_width(0, 96, 7), 175);
    assert_eq!(message_text_width(-5, 96, 7), 175);
    // A huge font never lifts the pixel limit.
    assert_eq!(message_text_width(100, 96, 20), 300);
    // An unknown DPI counts as 96, an unknown character width as no limit of its own.
    assert_eq!(message_text_width(1296, 0, 7), 300);
    assert_eq!(message_text_width(1296, 96, 0), 300);
    assert_eq!(message_text_width(1024, 96, 0), 256);
    assert_eq!(message_text_width(0, 96, 0), 1);
}

#[cfg(windows)]
mod win {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use raumakey_windows::dialog_box::{Dialog, DialogHandler, run_dialog, spawn_dialog};
    use raumakey_windows::dialog_template::{Control, DialogRect, DialogTemplate, styles};
    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::word_wrap::{line_width, wrap_for_control};
    use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
        CreateSolidBrush, DIB_RGB_COLORS, DT_CALCRECT, DT_NOPREFIX, DT_SINGLELINE, DeleteDC,
        DeleteObject, DrawTextW, FillRect, GetDC, GetDIBits, HDC, HGDIOBJ, ReleaseDC, SelectObject,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetDlgItem, GetWindowRect, PRF_CLIENT, PRF_ERASEBKGND, SMTO_NORMAL,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SendMessageTimeoutW, SendMessageW, SetWindowPos,
        WM_DPICHANGED, WM_GETFONT, WM_GETTEXT, WM_GETTEXTLENGTH, WM_PRINTCLIENT,
    };

    use crate::common_controls_v6::CommonControls6;

    const LABEL: u16 = 100;
    const CHECK_BOX: u16 = 101;
    const PUSH_BUTTON: u16 = 102;
    const EDIT: u16 = 103;

    /// How long a test waits for a dialog's thread.
    const WAIT: Duration = Duration::from_secs(10);

    /// Words of letters without ascenders or descenders ([`ink_lines`]).
    const PLAIN: &str = "ocean move cove amuse cameo wax ever crane snow onus razor more same \
                         noon came sure worm";

    /// Texts of the panel and the macro list editor that Windows cut inside words.
    const VIETNAMESE: [&str; 4] = [
        "Giữ nguyên phím đã gõ khi từ không phải tiếng Việt",
        "Viết hoa theo cách gõ (Ko → Không, KO → KHÔNG)",
        "(trong %LOCALAPPDATA%\\Raumakey, vì thư mục của raumakey.exe không ghi được hoặc là \
         thư mục được bảo vệ)",
        "Chọn một dòng để sửa. Mục mới được bật sẵn; xoá rồi thêm lại một mục sẽ đặt lại các \
         tuỳ chọn của nó.",
    ];

    fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
        DialogRect::new(x, y, cx, cy)
    }

    /// A dialog off every monitor, never activated, with a label, a multi-line check box, a
    /// push button, and an edit; the label and the check box are tall enough for several
    /// lines.
    fn template() -> DialogTemplate {
        DialogTemplate::new("Raumakey test", 300, 200)
            .at(-30_000, -30_000)
            .with_ex_style(styles::WS_EX_NOACTIVATE)
            .with_control(
                Control::label(LABEL, "", rect(5, 5, 280, 70)).with_style(styles::SS_NOPREFIX),
            )
            .with_control(
                Control::check_box(CHECK_BOX, "", rect(5, 80, 280, 70))
                    .with_style(styles::BS_MULTILINE),
            )
            .with_control(Control::push_button(PUSH_BUTTON, "", rect(5, 155, 60, 14)))
            .with_control(Control::edit(EDIT, rect(70, 155, 60, 12)))
    }

    /// Moves `dialog` off every monitor before Windows shows it.
    fn move_off_screen(dialog: &Dialog) {
        dialog.move_to(-32_000, -32_000).unwrap();
    }

    /// Sets the width of `control` to `width` pixels (its height stays). The label and the
    /// check box have no border, so their client area is as wide.
    fn set_client_width(control: HWND, width: i32) {
        let mut client = RECT::default();
        // SAFETY: `client` is a valid out buffer.
        unsafe { GetClientRect(control, &mut client) }.unwrap();
        // SAFETY: a control of this process; no pointers.
        unsafe {
            SetWindowPos(
                control,
                None,
                0,
                0,
                width,
                client.bottom,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .unwrap();
    }

    /// Runs `body` with a device context of `control` that has the control's font selected.
    fn with_font_of<T>(control: HWND, body: impl FnOnce(HDC) -> T) -> T {
        let font = send(control, WM_GETFONT, 0, 0);
        // SAFETY: a device context of a window of this process, released below; the font is
        // the control's own, which outlives the call, and it is deselected before the release.
        unsafe {
            let dc = GetDC(Some(control));
            let previous = SelectObject(dc, HGDIOBJ(font as *mut core::ffi::c_void));
            let result = body(dc);
            SelectObject(dc, previous);
            ReleaseDC(Some(control), dc);
            result
        }
    }

    /// The width of `line` drawn on one line with the font of `control`.
    fn single_line_width(control: HWND, line: &str) -> i32 {
        if line.is_empty() {
            // An empty slice lends a dangling pointer, which DrawTextW reads.
            return 0;
        }
        with_font_of(control, |dc| {
            let mut units: Vec<u16> = line.encode_utf16().collect();
            let mut rect = RECT::default();
            // SAFETY: the call only reads the text (no DT_MODIFYSTRING); `rect` is a valid
            // in/out rectangle.
            unsafe {
                DrawTextW(
                    dc,
                    &mut units,
                    &mut rect,
                    DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX,
                )
            };
            rect.right
        })
    }

    /// Sends `message` and returns the answer; fails the test when it is not answered in time.
    fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
        let mut result = 0_usize;
        // SAFETY: the pointers some callers pass in `lparam` point to data that outlives the
        // call (the receiver is in this process); `result` is a valid out pointer.
        let answered = unsafe {
            SendMessageTimeoutW(
                hwnd,
                message,
                WPARAM(wparam),
                LPARAM(lparam),
                SMTO_NORMAL,
                10_000,
                Some(&mut result),
            )
        };
        assert_ne!(answered.0, 0, "message {message:#x} was answered in time");
        result as isize
    }

    /// The text of `control`, read with `WM_GETTEXT` (from any thread).
    fn window_text(control: HWND) -> String {
        let length = send(control, WM_GETTEXTLENGTH, 0, 0);
        let mut buffer = vec![0_u16; usize::try_from(length).unwrap() + 1];
        let copied = send(
            control,
            WM_GETTEXT,
            buffer.len(),
            buffer.as_mut_ptr() as isize,
        );
        String::from_utf16(&buffer[..usize::try_from(copied).unwrap()]).unwrap()
    }

    /// The control `id` of `dialog` (from any thread).
    fn item_of(dialog: HWND, id: u16) -> HWND {
        // SAFETY: only reads the dialog's list of children.
        unsafe { GetDlgItem(Some(dialog), i32::from(id)) }.unwrap()
    }

    /// How many lines of text `control` draws: it paints itself into a memory bitmap
    /// (`WM_PRINTCLIENT`), and each run of rows with ink right of the check box's room (the
    /// part of the client area that `line_width` leaves out) counts as one line.
    fn ink_lines(control: HWND) -> usize {
        let mut client = RECT::default();
        // SAFETY: `client` is a valid out buffer.
        unsafe { GetClientRect(control, &mut client) }.unwrap();
        let (width, height) = (client.right, client.bottom);
        let text_left = width - line_width(control).unwrap();
        // SAFETY: a memory device context and bitmap of the screen's format, freed below; the
        // control paints into it on this thread (it belongs to it), and the pixels are read
        // back before the bitmap goes.
        unsafe {
            let screen = GetDC(None);
            let memory = CreateCompatibleDC(Some(screen));
            let bitmap = CreateCompatibleBitmap(screen, width, height);
            let previous = SelectObject(memory, HGDIOBJ(bitmap.0));
            let brush = CreateSolidBrush(COLORREF(0x00FF_00FF));
            FillRect(memory, &client, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
            SendMessageW(
                control,
                WM_PRINTCLIENT,
                Some(WPARAM(memory.0 as usize)),
                Some(LPARAM((PRF_CLIENT | PRF_ERASEBKGND) as isize)),
            );
            SelectObject(memory, previous);
            // The pixels, top row first, 32 bits each.
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut pixels = vec![0_u32; usize::try_from(width * height).unwrap()];
            let rows = GetDIBits(
                memory,
                bitmap,
                0,
                u32::try_from(height).unwrap(),
                Some(pixels.as_mut_ptr().cast()),
                &mut info,
                DIB_RGB_COLORS,
            );
            assert_eq!(rows, height, "the pixels were read");
            let _ = DeleteObject(HGDIOBJ(bitmap.0));
            let _ = DeleteDC(memory);
            ReleaseDC(None, screen);
            let width = usize::try_from(width).unwrap();
            let text_left = usize::try_from(text_left).unwrap();
            // The bottom right corner is below the last line.
            let background = pixels[pixels.len() - 1];
            let mut lines = 0;
            let mut in_line = false;
            for row in pixels.chunks(width) {
                let ink = row[text_left..].iter().any(|&pixel| pixel != background);
                if ink && !in_line {
                    lines += 1;
                }
                in_line = ink;
            }
            lines
        }
    }

    /// Runs `check` in `init` of a test dialog on a fresh thread (with Common Controls 6 when
    /// `v6`), ends the dialog, and returns what `check` found wrong.
    fn in_dialog(
        v6: bool,
        check: impl FnOnce(&Dialog) -> Vec<String> + Send + 'static,
    ) -> Vec<String> {
        struct Check<F> {
            check: Option<F>,
            found: mpsc::Sender<Vec<String>>,
        }
        impl<F: FnOnce(&Dialog) -> Vec<String>> DialogHandler for Check<F> {
            fn init(&mut self, dialog: &Dialog) {
                move_off_screen(dialog);
                if let Some(check) = self.check.take() {
                    let _ = self.found.send(check(dialog));
                }
                dialog.end(0);
            }
        }
        let (found, found_receiver) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            let _v6 = v6.then(CommonControls6::activate);
            let template = template().build().unwrap();
            let handler = Check {
                check: Some(check),
                found,
            };
            run_dialog(&template, None, Box::new(handler)).unwrap();
        });
        let found = found_receiver.recv_timeout(WAIT).expect("the check ran");
        thread.join().unwrap();
        found
    }

    /// The lines of `text` joined with spaces again.
    fn unwrapped(text: &str) -> String {
        text.split('\n').collect::<Vec<_>>().join(" ")
    }

    /// For a label and a check box, at many widths: every line that `set_text_wrapped` shows
    /// is drawn as one line by the control itself, so Windows wrapped nothing more.
    fn controls_draw_the_wrapped_lines_as_they_are(v6: bool) {
        let found = in_dialog(v6, |dialog| {
            let mut found = Vec::new();
            for id in [LABEL, CHECK_BOX] {
                let control = dialog.item(id).unwrap();
                for width in (120..=320).step_by(5) {
                    set_client_width(control, width);
                    dialog.set_text_wrapped(id, PLAIN).unwrap();
                    let shown = dialog.text(id).unwrap();
                    let lines = shown.split('\n').count();
                    let drawn = ink_lines(control);
                    if drawn != lines || unwrapped(&shown) != PLAIN {
                        found.push(format!("{id} at {width}: {lines} lines, {drawn} drawn"));
                    }
                }
            }
            found
        });
        assert_eq!(found, Vec::<String>::new(), "Common Controls 6: {v6}");
    }

    #[test]
    fn common_controls_5_draw_the_wrapped_lines_as_they_are() {
        controls_draw_the_wrapped_lines_as_they_are(false);
    }

    #[test]
    fn common_controls_6_draw_the_wrapped_lines_as_they_are() {
        controls_draw_the_wrapped_lines_as_they_are(true);
    }

    /// Vietnamese texts in a label and a check box, each too long for one line: whole words
    /// on each line, and each line fits the control's line width on one line (but a word
    /// wider than that, alone on its line).
    #[test]
    fn vietnamese_texts_are_wrapped_at_spaces_to_lines_that_fit() {
        for v6 in [false, true] {
            let found = in_dialog(v6, |dialog| {
                let mut found = Vec::new();
                for id in [LABEL, CHECK_BOX] {
                    let control = dialog.item(id).unwrap();
                    for width in [150, 200, 250] {
                        set_client_width(control, width);
                        let limit = line_width(control).unwrap();
                        for text in VIETNAMESE {
                            dialog.set_text_wrapped(id, text).unwrap();
                            let shown = dialog.text(id).unwrap();
                            if unwrapped(&shown) != text || !shown.contains('\n') {
                                found.push(format!("{id} at {width}: {shown:?}"));
                            }
                            for line in shown.split('\n') {
                                let width = single_line_width(control, line);
                                if width > limit && line.contains(' ') {
                                    found.push(format!("{id}: {line:?} {width} > {limit}"));
                                }
                            }
                        }
                    }
                }
                found
            });
            assert_eq!(found, Vec::<String>::new(), "Common Controls 6: {v6}");
        }
    }

    /// The line width of a label is its client width; a check box leaves room for its box.
    #[test]
    fn a_check_box_line_leaves_room_for_the_box() {
        let found = in_dialog(true, |dialog| {
            let label = dialog.item(LABEL).unwrap();
            let check_box = dialog.item(CHECK_BOX).unwrap();
            set_client_width(label, 200);
            set_client_width(check_box, 200);
            vec![
                line_width(label).unwrap().to_string(),
                line_width(check_box).unwrap().to_string(),
            ]
        });
        assert_eq!(found[0], "200");
        let check_box: i32 = found[1].parse().unwrap();
        assert!((150..=184).contains(&check_box), "{check_box}");
    }

    /// `set_text_wrapped` wraps the texts of labels and check boxes only: a push button or an
    /// edit, a text with a NUL, and an id that names no control fail, and the text stays.
    #[test]
    fn only_labels_and_check_boxes_take_wrapped_texts() {
        let found = in_dialog(false, |dialog| {
            let mut found = Vec::new();
            for id in [PUSH_BUTTON, EDIT] {
                dialog.set_text(id, "giữ").unwrap();
                match dialog.set_text_wrapped(id, PLAIN) {
                    Err(error) if error.kind() == WinErrorKind::InvalidInput => {}
                    other => found.push(format!("{id}: {other:?}")),
                }
                if dialog.text(id).unwrap() != "giữ" {
                    found.push(format!("{id}: the text changed"));
                }
                if wrap_for_control(dialog.item(id).unwrap(), PLAIN).is_ok() {
                    found.push(format!("{id}: wrap_for_control worked"));
                }
            }
            dialog.set_text(LABEL, "giữ").unwrap();
            match dialog.set_text_wrapped(LABEL, "a\0b") {
                Err(error) if error.kind() == WinErrorKind::InvalidInput => {}
                other => found.push(format!("NUL: {other:?}")),
            }
            if dialog.text(LABEL).unwrap() != "giữ" {
                found.push("NUL: the text changed".to_owned());
            }
            if dialog.set_text_wrapped(999, PLAIN).is_ok() {
                found.push("999 worked".to_owned());
            }
            found
        });
        assert_eq!(found, Vec::<String>::new());
    }

    /// After a change of DPI (`WM_DPICHANGED`), the dialog wraps the texts set with
    /// `set_text_wrapped` again for the controls' new widths; a text set later with `set_text`
    /// is left alone.
    #[test]
    fn texts_are_wrapped_again_after_a_change_of_dpi() {
        struct Wrapping(mpsc::Sender<isize>);
        impl DialogHandler for Wrapping {
            fn init(&mut self, dialog: &Dialog) {
                move_off_screen(dialog);
                set_client_width(dialog.item(LABEL).unwrap(), 320);
                set_client_width(dialog.item(CHECK_BOX).unwrap(), 320);
                dialog.set_text_wrapped(LABEL, PLAIN).unwrap();
                dialog.set_text_wrapped(CHECK_BOX, PLAIN).unwrap();
                dialog.set_text(CHECK_BOX, "giữ").unwrap();
                let _ = self.0.send(dialog.hwnd().0 as isize);
            }
        }
        let (sender, hwnds) = mpsc::channel();
        let (closed_sender, closed) = mpsc::channel();
        let template = template().build().unwrap();
        let running = spawn_dialog(template, Wrapping(sender), move |result| {
            let _ = closed_sender.send(result);
        })
        .unwrap();
        let dialog = HWND(hwnds.recv_timeout(WAIT).unwrap() as *mut core::ffi::c_void);
        let label = item_of(dialog, LABEL);
        let before = window_text(label).split('\n').count();
        set_client_width(label, 150);
        let mut frame = RECT::default();
        // SAFETY: `frame` is a valid out buffer.
        unsafe { GetWindowRect(dialog, &mut frame) }.unwrap();
        // 96 DPI in both words, and the frame it has: the dialog keeps its place and size.
        send(
            dialog,
            WM_DPICHANGED,
            96 | (96 << 16),
            &raw const frame as isize,
        );
        let limit = line_width(label).unwrap();
        let deadline = Instant::now() + WAIT;
        loop {
            let shown = window_text(label);
            let fits = shown
                .split('\n')
                .all(|line| single_line_width(label, line) <= limit);
            if fits && shown.split('\n').count() > before {
                assert_eq!(unwrapped(&shown), PLAIN);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "wrapped again in time: {shown:?}"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(window_text(item_of(dialog, CHECK_BOX)), "giữ");
        running.dismiss();
        closed.recv_timeout(WAIT).expect("the dialog closed");
    }
}
