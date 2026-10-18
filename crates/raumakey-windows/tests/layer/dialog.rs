//! `dialog`: message box styles, texts, and answers (pure). Answering a box needs a person,
//! so that is a manual test. The real boxes of the other tests never reach the screen: hooks
//! of the box's own thread (`BoxHooks`) read the box once it is laid out, move it off every
//! monitor, drop its taskbar button, and refuse to activate it; then they either end it
//! before Windows first shows it, or leave it open (off-screen) for the test to dismiss, and
//! close it as the user would should the dismiss fail.

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

/// The command posted to close a box without an answer of the user: `IDNO` (7) for a
/// question, and `IDCANCEL` (2), as Esc sends, for a box with OK only, whose single button has
/// the id `IDCANCEL` on Windows 11 (`IDOK` would do nothing); the box then answers OK.
#[test]
fn a_box_is_dismissed_with_the_command_that_closes_it() {
    assert_eq!(MessageKind::Question.dismiss_command(), 7);
    for kind in [
        MessageKind::Information,
        MessageKind::Warning,
        MessageKind::Error,
    ] {
        assert_eq!(kind.dismiss_command(), 2, "{kind:?}");
    }
}

#[cfg(windows)]
mod win {
    use core::cell::{Cell, RefCell};
    use core::ffi::c_void;
    use std::sync::mpsc;
    use std::time::Duration;

    use raumakey_windows::dialog::{
        MessageKind, MessageResponse, dismiss_message_boxes, message_box, spawn_message_box,
        spawn_message_box_with,
    };
    use raumakey_windows::dialog_box::{Dialog, DialogHandler, spawn_dialog};
    use raumakey_windows::dialog_template::{Control, DialogRect, DialogTemplate, styles};
    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::panic::guard_value;
    use raumakey_windows::window::{AppMessage, Window, WindowClass};
    use raumakey_windows::word_wrap::wrap_for_message_box;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        DT_CALCRECT, DT_EDITCONTROL, DT_EXPANDTABS, DT_NOPREFIX, DT_SINGLELINE, DT_WORDBREAK,
        DrawTextW, GetDC, GetTextMetricsW, HGDIOBJ, ReleaseDC, SelectObject, TEXTMETRICW,
    };
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CWPRETSTRUCT, CallNextHookEx, CreateWindowExW, DestroyWindow, GWL_EXSTYLE, GWL_STYLE,
        GetClientRect, GetDlgItem, GetWindowLongW, GetWindowTextW, HCBT_ACTIVATE, HHOOK,
        PostMessageW, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SendMessageW,
        SetWindowLongW, SetWindowPos, SetWindowsHookExW, UnhookWindowsHookEx, WH_CALLWNDPROCRET,
        WH_CBT, WINDOW_EX_STYLE, WINDOWPOS, WM_CLOSE, WM_COMMAND, WM_GETFONT, WM_INITDIALOG,
        WM_SHOWWINDOW, WM_WINDOWPOSCHANGED, WS_CHILD, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
    };
    use windows::core::w;

    use crate::support::on_fresh_thread;

    /// Dismissing posts the command that closes a box to the dialog windows (class `#32770`,
    /// the class of a message box) of the box's thread, and to nothing else. This test uses a
    /// hidden window of the dialog class, so no box is shown.
    #[test]
    fn dismissing_posts_to_the_dialog_windows_of_the_thread_only() {
        on_fresh_thread(|| {
            // SAFETY: no preconditions.
            let thread = unsafe { GetCurrentThreadId() };
            assert_eq!(dismiss_message_boxes(thread, MessageKind::Question), 0);
            let class = WindowClass::register("test-not-a-box").unwrap();
            let other = Window::create_hidden(
                &class,
                "Raumakey test",
                Box::new(|_: HWND, _: u32, _, _| None),
            )
            .unwrap();
            assert_eq!(dismiss_message_boxes(thread, MessageKind::Question), 0);
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
            assert_eq!(dismiss_message_boxes(thread, MessageKind::Question), 1);
            // SAFETY: a window of this thread.
            unsafe { DestroyWindow(dialog) }.unwrap();
            drop(other);
            // A thread that does not exist has no windows.
            assert_eq!(dismiss_message_boxes(u32::MAX, MessageKind::Information), 0);
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

    /// What [`after_box_init`] read from the box: its text, how many lines its text control
    /// shows, and the width of each line drawn on one line.
    #[derive(Debug, Default)]
    struct Seen {
        text: String,
        shown_lines: i32,
        line_widths: Vec<i32>,
        text_width: i32,
    }

    thread_local! {
        /// What [`after_box_init`] saw on this thread.
        static SEEN: RefCell<Option<Seen>> = const { RefCell::new(None) };
        /// A window of this thread was shown ([`after_box_init`]).
        static SHOWN: Cell<bool> = const { Cell::new(false) };
        /// Where [`after_box_init`] reports each box it laid out instead of closing it.
        static REPORT: RefCell<Option<mpsc::Sender<isize>>> = const { RefCell::new(None) };
    }

    /// The message box's text control (`IDC_STATIC` of the box, 0xFFFF).
    const BOX_TEXT: i32 = 0xFFFF;

    /// The kinds of message boxes.
    const KINDS: [MessageKind; 4] = [
        MessageKind::Information,
        MessageKind::Warning,
        MessageKind::Error,
        MessageKind::Question,
    ];

    /// How long a test waits for a thread to answer.
    const WAIT: Duration = Duration::from_secs(10);

    /// How long a dismissed box may take to close.
    const CLOSING: Duration = Duration::from_secs(3);

    /// The hooks [`after_box_init`] and [`no_activation`] of the calling thread (no other
    /// thread is hooked), until dropped. With `report`, each message box laid out on the
    /// thread is reported there and stays open, off every monitor and without a taskbar
    /// button; without, it ends before Windows shows it. Panics, before any box is shown, when
    /// a hook cannot be set.
    struct BoxHooks(Vec<HHOOK>);

    impl BoxHooks {
        fn install(report: Option<mpsc::Sender<isize>>) -> Self {
            REPORT.with(|slot| *slot.borrow_mut() = report);
            // SAFETY: no preconditions.
            let thread = unsafe { GetCurrentThreadId() };
            // SAFETY: hook procedures of this process for this thread only, so no module
            // handle is needed; they are removed on drop, on this thread.
            let hooks = unsafe {
                [
                    SetWindowsHookExW(WH_CALLWNDPROCRET, Some(after_box_init), None, thread),
                    SetWindowsHookExW(WH_CBT, Some(no_activation), None, thread),
                ]
            };
            Self(hooks.into_iter().map(Result::unwrap).collect())
        }
    }

    impl Drop for BoxHooks {
        fn drop(&mut self) {
            for hook in self.0.drain(..) {
                // SAFETY: removes a hook that `install` set on this thread.
                let _ = unsafe { UnhookWindowsHookEx(hook) };
            }
            REPORT.with(|slot| slot.borrow_mut().take());
        }
    }

    /// Closes the box `hwnd` that a dismiss left open, as the user would (Esc, or "No" of a
    /// question), so that a failing test leaves no box behind.
    fn close_by_hand(hwnd: isize) {
        let hwnd = HWND(hwnd as *mut c_void);
        for command in [2_usize, 7] {
            // SAFETY: posts plain values to a window of this process.
            let _ = unsafe { PostMessageW(Some(hwnd), WM_COMMAND, WPARAM(command), LPARAM(0)) };
        }
    }

    /// A `WH_CALLWNDPROCRET` hook of the box's thread: once a message box answered
    /// `WM_INITDIALOG`, its layout is done and it is not shown yet. Reads its text control,
    /// moves the box off every monitor and drops its taskbar button (`WS_EX_TOOLWINDOW`,
    /// `WS_EX_NOACTIVATE`), then reports it ([`BoxHooks`]) or posts `WM_CLOSE` (the button of a
    /// box with OK only is `IDCANCEL`, so `IDOK` would not close it), which ends the box before
    /// Windows first shows it. Notes in [`SHOWN`] when a window of the thread is shown.
    unsafe extern "system" fn after_box_init(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        guard_value((), || {
            if code < 0 {
                return;
            }
            // SAFETY: for a hook code of 0 or more, `lparam` points to the `CWPRETSTRUCT`
            // of the message the window just answered.
            let call = unsafe { &*(lparam.0 as *const CWPRETSTRUCT) };
            let shows = match call.message {
                WM_SHOWWINDOW => call.wParam.0 != 0,
                WM_WINDOWPOSCHANGED => {
                    // SAFETY: for WM_WINDOWPOSCHANGED, `lParam` points to the window's
                    // `WINDOWPOS`.
                    let position = unsafe { &*(call.lParam.0 as *const WINDOWPOS) };
                    position.flags & SWP_SHOWWINDOW == SWP_SHOWWINDOW
                }
                _ => false,
            };
            // SAFETY: only reads the style of a window of this thread.
            let child = unsafe { GetWindowLongW(call.hwnd, GWL_STYLE) } as u32 & WS_CHILD.0 != 0;
            // The controls show themselves inside the hidden box.
            if shows && !child {
                SHOWN.with(|shown| shown.set(true));
            }
            if call.message == WM_INITDIALOG {
                let seen = read_box(call.hwnd);
                SEEN.with(|slot| *slot.borrow_mut() = seen);
                // SAFETY: a window of this thread; no pointers.
                unsafe {
                    let ex_style = GetWindowLongW(call.hwnd, GWL_EXSTYLE) as u32
                        | WS_EX_TOOLWINDOW.0
                        | WS_EX_NOACTIVATE.0;
                    SetWindowLongW(call.hwnd, GWL_EXSTYLE, ex_style as i32);
                    let _ = SetWindowPos(
                        call.hwnd,
                        None,
                        -32_000,
                        -32_000,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
                let reported = REPORT.with(|slot| {
                    slot.borrow()
                        .as_ref()
                        .map(|report| report.send(call.hwnd.0 as isize))
                });
                if reported.is_none() {
                    // SAFETY: posts plain values to a window of this thread.
                    let _ =
                        unsafe { PostMessageW(Some(call.hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
                }
            }
        });
        // SAFETY: forwards the unchanged arguments to the next hook.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// A `WH_CBT` hook of the box's thread that refuses to activate any window, so the box
    /// never takes the foreground (`MB_SETFOREGROUND`).
    unsafe extern "system" fn no_activation(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HCBT_ACTIVATE as i32 {
            // Nonzero: the window is not activated.
            return LRESULT(1);
        }
        // SAFETY: forwards the unchanged arguments to the next hook.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// The text of the box's text control and how many lines it shows: `DrawTextW` lays the
    /// text out in the control's client width with its font and the flags a label with
    /// `SS_EDITCONTROL` and `SS_NOPREFIX` uses.
    fn read_box(dialog: HWND) -> Option<Seen> {
        // SAFETY: only reads the dialog's list of children.
        let control = unsafe { GetDlgItem(Some(dialog), BOX_TEXT) }.ok()?;
        let mut client = RECT::default();
        // SAFETY: `client` is a valid out buffer.
        unsafe { GetClientRect(control, &mut client) }.ok()?;
        let mut buffer = [0_u16; 4096];
        // SAFETY: the buffer is writable for its length.
        let length = unsafe { GetWindowTextW(control, &mut buffer) };
        let text = String::from_utf16_lossy(&buffer[..usize::try_from(length).ok()?]);
        // SAFETY: a control of this thread; no pointers.
        let font = unsafe { SendMessageW(control, WM_GETFONT, None, None) }.0;
        // SAFETY: a device context of a window of this thread, released below; the font is
        // the control's own and is deselected before the release. `DrawTextW` only reads the
        // text (no DT_MODIFYSTRING) and writes the rectangles.
        unsafe {
            let dc = GetDC(Some(control));
            let previous = SelectObject(dc, HGDIOBJ(font as *mut c_void));
            let mut metrics = TEXTMETRICW::default();
            let _ = GetTextMetricsW(dc, &mut metrics);
            let flags = DT_WORDBREAK | DT_EDITCONTROL | DT_EXPANDTABS | DT_NOPREFIX;
            let mut units: Vec<u16> = text.encode_utf16().collect();
            let mut laid_out = RECT {
                right: client.right,
                ..RECT::default()
            };
            DrawTextW(dc, &mut units, &mut laid_out, flags | DT_CALCRECT);
            let line_widths = text
                .split('\n')
                .map(|line| {
                    let mut units: Vec<u16> = line.encode_utf16().collect();
                    if units.is_empty() {
                        // An empty slice lends a dangling pointer, which DrawTextW reads.
                        return 0;
                    }
                    let mut single = RECT::default();
                    let single_line = DT_SINGLELINE | DT_EXPANDTABS | DT_NOPREFIX | DT_CALCRECT;
                    DrawTextW(dc, &mut units, &mut single, single_line);
                    single.right
                })
                .collect();
            SelectObject(dc, previous);
            ReleaseDC(Some(control), dc);
            Some(Seen {
                text,
                shown_lines: laid_out.bottom / metrics.tmHeight.max(1),
                line_widths,
                text_width: client.right,
            })
        }
    }

    /// Shows `text` in a real message box through `message_box` on a fresh thread whose hooks
    /// end the box before Windows shows it ([`after_box_init`], [`no_activation`]), and returns
    /// what the box laid out.
    fn shown_in_a_box(text: &'static str) -> Seen {
        let (sender, seen) = mpsc::channel();
        std::thread::spawn(move || {
            let hooks = BoxHooks::install(None);
            let answer = message_box(None, text, "Raumakey", MessageKind::Information);
            drop(hooks);
            let laid_out = SEEN.with(|slot| slot.borrow_mut().take());
            let _ = sender.send((answer.ok(), laid_out, SHOWN.with(Cell::get)));
        });
        let (answer, laid_out, shown) =
            seen.recv_timeout(WAIT).expect("the box was closed in time");
        assert_eq!(answer, Some(MessageResponse::Ok));
        assert!(!shown, "the box ended before Windows showed it");
        laid_out.expect("the box's text was read")
    }

    /// A long Vietnamese text in a real message box (ended before it is shown): `message_box`
    /// wraps it at spaces to lines the box keeps as they are, so the box shows exactly those
    /// lines, each whole words, and cuts no word.
    #[test]
    fn a_message_box_shows_the_text_wrapped_at_spaces_only() {
        const TEXT: &str = "Raumakey đang chạy. Biểu tượng V/E ở khay hệ thống cho biết chế độ \
                            gõ: nhấp vào biểu tượng để chuyển giữa tiếng Việt và tiếng Anh, nhấp \
                            chuột phải để mở menu.\n\nTệp cấu hình: C:\\Users\\Nguyen Van Anh\\\
                            AppData\\Local\\Raumakey\\raumakey.toml";
        let seen = shown_in_a_box(TEXT);
        assert_eq!(seen.text, wrap_for_message_box(TEXT));
        let lines = seen.text.split('\n').count();
        assert!(lines > TEXT.split('\n').count(), "{:?}", seen.text);
        assert_eq!(
            seen.shown_lines,
            i32::try_from(lines).unwrap(),
            "{:?} in {} pixels: {:?}",
            seen.text,
            seen.text_width,
            seen.line_widths
        );
        let words: Vec<&str> = TEXT.split_whitespace().collect();
        assert_eq!(seen.text.split_whitespace().collect::<Vec<_>>(), words);
    }

    /// A box of each kind shown through `spawn_message_box` (hooked: off every monitor,
    /// without a taskbar button, never activated) closes once it is dismissed, and its callback
    /// gets the dismiss answer: OK for a box with OK only, "No" for a question.
    #[test]
    fn a_detached_box_of_each_kind_closes_when_it_is_dismissed() {
        let mut found = Vec::new();
        for kind in KINDS {
            let (up_sender, up) = mpsc::channel();
            let (answer_sender, answers) = mpsc::channel();
            let shown = spawn_message_box_with(
                "Hộp thử nghiệm.",
                "Raumakey",
                kind,
                move || BoxHooks::install(Some(up_sender)),
                move |answer| {
                    let _ = answer_sender.send(answer);
                },
            )
            .unwrap();
            let hwnd = up.recv_timeout(WAIT).expect("the box was laid out");
            shown.dismiss();
            match answers.recv_timeout(CLOSING) {
                Ok(answer) => {
                    if answer != Some(kind.dismiss_answer()) {
                        found.push(format!("{kind:?}: answered {answer:?}"));
                    }
                    assert!(shown.is_closed(), "{kind:?}");
                }
                Err(_) => {
                    found.push(format!("{kind:?}: still open"));
                    close_by_hand(hwnd);
                    answers.recv_timeout(WAIT).expect("closed by hand");
                }
            }
        }
        assert_eq!(found, Vec::<String>::new());
    }

    /// The private message that has the test handler show its box, after `init`.
    fn ask() -> AppMessage {
        AppMessage::new(1).unwrap()
    }

    /// A box of each kind that the handler of a detached dialog shows through
    /// `Dialog::message_box` (hooked as above; the dialog is off every monitor and never
    /// activated) closes when the dialog is dismissed: `message_box` fails with `E_ABORT`, and
    /// the dialog ends.
    #[test]
    fn a_box_of_each_kind_closes_with_the_dismissed_dialog_that_shows_it() {
        struct Asking {
            kind: MessageKind,
            up: Option<mpsc::Sender<isize>>,
            answers: mpsc::Sender<Result<MessageResponse, WinErrorKind>>,
        }
        impl DialogHandler for Asking {
            fn init(&mut self, dialog: &Dialog) {
                dialog.move_to(-32_000, -32_000).unwrap();
                // Asked once the window is published, so that a dismiss reaches it.
                assert!(dialog.post(ask(), 0, 0));
            }

            fn app_message(&mut self, dialog: &Dialog, _: AppMessage, _: usize, _: isize) {
                let Some(up) = self.up.take() else {
                    return;
                };
                let hooks = BoxHooks::install(Some(up));
                let answer = dialog.message_box("Hộp thử nghiệm.", self.kind);
                drop(hooks);
                let _ = self.answers.send(answer.map_err(|error| error.kind()));
            }
        }
        let template = DialogTemplate::new("Raumakey test", 120, 40)
            .at(-30_000, -30_000)
            .with_ex_style(styles::WS_EX_NOACTIVATE)
            .with_control(Control::label(
                100,
                "Hộp thoại thử nghiệm.",
                DialogRect::new(5, 5, 110, 10),
            ));
        let mut found = Vec::new();
        for kind in KINDS {
            let (up_sender, up) = mpsc::channel();
            let (answer_sender, answers) = mpsc::channel();
            let (closed_sender, closed) = mpsc::channel();
            let handler = Asking {
                kind,
                up: Some(up_sender),
                answers: answer_sender,
            };
            let running = spawn_dialog(template.build().unwrap(), handler, move |result| {
                let _ = closed_sender.send(result);
            })
            .unwrap();
            let hwnd = up.recv_timeout(WAIT).expect("the box was laid out");
            running.dismiss();
            match answers.recv_timeout(CLOSING) {
                Ok(Err(WinErrorKind::Cancelled)) => {}
                Ok(other) => found.push(format!("{kind:?}: answered {other:?}")),
                Err(_) => {
                    found.push(format!("{kind:?}: still open"));
                    close_by_hand(hwnd);
                    let _ = answers.recv_timeout(WAIT).expect("closed by hand");
                }
            }
            closed.recv_timeout(WAIT).expect("the dialog closed");
            assert!(running.is_closed());
        }
        assert_eq!(found, Vec::<String>::new());
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
