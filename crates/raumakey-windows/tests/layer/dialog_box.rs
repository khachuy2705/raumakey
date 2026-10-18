//! `dialog_box`: the reserved private messages and the notification codes (pure); on Windows,
//! real modal dialog boxes built from in-memory templates.
//!
//! Every dialog runs on a thread of its own (`spawn_dialog`, or `run_dialog` on a test-made
//! thread) and is driven from the test thread through its window handle, the way Windows
//! drives it: `BM_CLICK` on buttons, `CB_SETCURSEL` followed by the `CBN_SELCHANGE` that a user
//! selection sends, `WM_SETTEXT` on an edit, `LVM_SETITEMSTATE` on a list view. Messages are
//! sent with a timeout and every wait has one, so a broken dialog fails its test instead of
//! hanging it.
//!
//! The dialogs stay out of the developer's way: their templates have `WS_EX_NOACTIVATE` (which
//! also drops the taskbar button) and an explicit position (Windows would move a dialog that
//! starts off-screen back onto it), and the test handler moves each dialog to (-32000, -32000)
//! in `init`, before Windows shows it. A dialog becomes the active window of its own thread
//! (the dialog manager focuses a control), which takes the foreground only in a process that
//! may set the foreground window; a test process started from a console may not. No test
//! brings a dialog to the front or shows a message box (the one that does is `#[ignore]`d and
//! manual; the one that asks for a box that must not appear refuses every window of its
//! dialog's thread with a `WH_CBT` hook of that thread), and no test depends on what else the
//! desktop shows.

use raumakey_windows::dialog_box::{RESERVED_FIRST, is_reserved, notifications};
use raumakey_windows::window::{WM_APP, WM_APP_LAST};

#[test]
fn the_top_of_the_wm_app_range_is_reserved_for_the_dialog_procedure() {
    assert_eq!(RESERVED_FIRST, 0xBFF0);
    for reserved in [RESERVED_FIRST, 0xBFFE, WM_APP_LAST] {
        assert!(is_reserved(reserved), "{reserved:#x}");
    }
    for free in [WM_APP, WM_APP + 1, RESERVED_FIRST - 1, 0xC000, 0x0010] {
        assert!(!is_reserved(free), "{free:#x}");
    }
}

#[test]
fn notification_codes_have_their_win32_values() {
    assert_eq!(notifications::BN_CLICKED, 0);
    assert_eq!(notifications::CBN_SELCHANGE, 1);
    assert_eq!(notifications::CBN_EDITCHANGE, 5);
    assert_eq!(notifications::EN_CHANGE, 0x0300);
    assert_eq!(notifications::LVN_ITEMCHANGED, -101);
    assert_eq!(notifications::NM_DBLCLK, -3);
}

#[cfg(windows)]
mod win {
    use core::cell::Cell;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::{Duration, Instant};

    use raumakey_windows::dialog::{MessageKind, MessageResponse};
    use raumakey_windows::dialog_box::{
        DetachedDialog, Dialog, DialogHandler, RESERVED_FIRST, notifications, run_dialog,
        spawn_dialog,
    };
    use raumakey_windows::dialog_template::{
        Control, DialogRect, DialogTemplate, IDCANCEL, IDOK, TemplateBytes, styles,
    };
    use raumakey_windows::error::{WinError, WinErrorKind};
    use raumakey_windows::module::Module;
    use raumakey_windows::panic::guard_value;
    use raumakey_windows::window::{AppMessage, WM_APP_LAST};
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::Controls::{
        LVIS_SELECTED, LVITEMW, LVM_GETITEMCOUNT, LVM_GETITEMTEXTW, LVM_GETNEXTITEM,
        LVM_SETITEMSTATE, LVNI_SELECTED, NMHDR,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_CLICK, BM_GETCHECK, CB_GETCOUNT, CB_GETCURSEL, CB_GETLBTEXT, CB_GETLBTEXTLEN,
        CB_SETCURSEL, CallNextHookEx, GetDlgItem, GetWindowRect, HCBT_CREATEWND, HHOOK, ICON_BIG,
        ICON_SMALL, IsWindow, IsWindowVisible, PostMessageW, SMTO_NORMAL, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER, SendMessageTimeoutW, SetWindowPos, SetWindowsHookExW,
        UnhookWindowsHookEx, WH_CBT, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_GETICON, WM_GETTEXT,
        WM_GETTEXTLENGTH, WM_INITDIALOG, WM_KEYDOWN, WM_NOTIFY, WM_NULL, WM_SETTEXT,
    };

    use crate::support::{ResourceFile, group_icon_ids};

    const WAIT: Duration = Duration::from_secs(10);
    /// Milliseconds a sent message may take before the test fails.
    const SEND_TIMEOUT_MS: u32 = 10_000;

    const ID_CHECK: u16 = 101;
    const ID_BUTTON: u16 = 102;
    const ID_COMBO: u16 = 103;
    const ID_EDIT: u16 = 104;
    const ID_LIST: u16 = 105;
    const ID_LABEL: u16 = 106;
    const ID_NESTED: u16 = 107;

    /// `EN_KILLFOCUS`: the edit lost the focus.
    const EN_KILLFOCUS: u16 = 0x0200;
    /// `EN_UPDATE`: the edit is about to show changed text.
    const EN_UPDATE: u16 = 0x0400;
    /// `LVN_DELETEITEM`: the list view deletes an item.
    const LVN_DELETEITEM: i32 = -103;
    /// `LVN_DELETEALLITEMS`: the list view deletes all its items (also when it is destroyed).
    const LVN_DELETEALLITEMS: i32 = -104;
    /// The reserved message that dismisses a dialog (`DetachedDialog::dismiss` posts it).
    const DISMISS: u32 = WM_APP_LAST;

    const METHODS: [&str; 3] = ["Telex", "VNI", "VIQR"];
    const EDIT_TEXT: &str = "Kiểm tra chính tả";
    const LABEL_TEXT: &str = "Đường dẫn: D:\\Công cụ\\raumakey.toml";
    const COLUMNS: [(&str, i16); 2] = [("Từ gõ tắt", 60), ("Thay bằng", 120)];
    const ROWS: [[&str; 2]; 3] = [
        ["vn", "Việt Nam"],
        ["hcm", "Thành phố Hồ Chí Minh"],
        ["ko", "không"],
    ];

    /// Private messages of the test handler, by offset from `WM_APP`.
    const RECORD: u32 = 1;
    /// Ends the dialog with `wParam` as its result.
    const END: u32 = 2;
    /// Marks the end of what a test waits for (see `Running::settle`).
    const BARRIER: u32 = 3;
    /// Selects item `wParam` of the drop-down list and of the list view.
    const SELECT: u32 = 4;

    fn app(offset: u32) -> AppMessage {
        AppMessage::new(offset).unwrap()
    }

    /// What the test handler saw, sent from the dialog's thread.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Seen {
        Init {
            hwnd: isize,
            thread: u32,
        },
        Command {
            id: u16,
            code: u16,
        },
        Notify {
            id: u16,
            code: i32,
        },
        App {
            offset: u32,
            wparam: usize,
            lparam: isize,
            thread: u32,
        },
        CloseRequested,
        /// The edit's text, read by the handler after `EN_CHANGE`.
        Text(String),
        /// The drop-down list's selection, read by the handler after `CBN_SELCHANGE`.
        ComboSelected(Option<usize>),
        /// The list view's selection, read by the handler after `LVN_ITEMCHANGED`.
        ListSelected(Option<usize>),
        /// The result of a nested dialog the handler ran.
        Nested(Result<isize, WinErrorKind>),
        /// What `combo_select` and `list_select` returned for the index of `SELECT`.
        Selected {
            combo: Result<(), WinErrorKind>,
            list: Result<(), WinErrorKind>,
        },
        /// A helper failed.
        Failed(&'static str, WinErrorKind),
        /// `on_close` ran with this result.
        Closed(Option<isize>),
    }

    /// The test handler: forwards what it sees, fills the controls at `init`, and changes them
    /// programmatically when the push button is clicked.
    struct Probe {
        seen: Sender<Seen>,
        /// The answer of `close_requested`.
        allow_close: bool,
        /// `init` waits for this before it does anything else (after moving off-screen).
        gate: Option<Receiver<()>>,
        /// Panics in this callback ("init" or "command" for the push button).
        panic_in: Option<&'static str>,
        /// Icons to set at `init`.
        icons: Option<(Module, u16)>,
        /// The handler of the nested dialog that the "Mở" button runs.
        nested: Option<Box<Probe>>,
        /// Counts the drops of this handler.
        drops: Option<Arc<AtomicUsize>>,
        /// Dropping this handler panics.
        panic_on_drop: bool,
        /// `init` posts `WM_DESTROY` to the dialog, as another process could.
        forge_destroy: bool,
        /// `init` moves the dialog here with `Dialog::move_to` (after moving it off-screen).
        place: Option<(i32, i32)>,
    }

    impl Probe {
        fn new(seen: Sender<Seen>) -> Self {
            Self {
                seen,
                allow_close: true,
                gate: None,
                panic_in: None,
                icons: None,
                nested: None,
                drops: None,
                panic_on_drop: false,
                forge_destroy: false,
                place: None,
            }
        }

        fn send(&self, seen: Seen) {
            let _ = self.seen.send(seen);
        }

        fn check<T>(&self, what: &'static str, result: Result<T, WinError>) {
            if let Err(error) = result {
                self.send(Seen::Failed(what, error.kind()));
            }
        }

        fn fill(&self, dialog: &Dialog) {
            dialog.set_checked(ID_CHECK, true);
            self.check(
                "combo_set_items",
                dialog.combo_set_items(ID_COMBO, &METHODS),
            );
            self.check("combo_select", dialog.combo_select(ID_COMBO, Some(1)));
            self.check("set_text", dialog.set_text(ID_EDIT, EDIT_TEXT));
            self.check(
                "list_set_columns",
                dialog.list_set_columns(ID_LIST, &COLUMNS),
            );
            let rows: Vec<Vec<String>> = ROWS
                .iter()
                .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
                .collect();
            self.check("list_set_rows", dialog.list_set_rows(ID_LIST, &rows));
            self.check("list_select", dialog.list_select(ID_LIST, Some(2)));
            self.check("set_text label", dialog.set_text(ID_LABEL, LABEL_TEXT));
            dialog.set_enabled(ID_BUTTON, true);
            dialog.focus(ID_EDIT);
        }
    }

    impl Drop for Probe {
        fn drop(&mut self) {
            if let Some(drops) = &self.drops {
                drops.fetch_add(1, Ordering::SeqCst);
            }
            assert!(!self.panic_on_drop, "deliberate panic in drop");
        }
    }

    /// Moves the dialog far off-screen before Windows shows it, without activating it.
    fn move_off_screen(hwnd: HWND) {
        // SAFETY: a window of the calling thread; no pointers.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                None,
                -32_000,
                -32_000,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
    }

    fn current_thread() -> u32 {
        // SAFETY: no preconditions.
        unsafe { GetCurrentThreadId() }
    }

    impl DialogHandler for Probe {
        fn init(&mut self, dialog: &Dialog) {
            move_off_screen(dialog.hwnd());
            if let Some(gate) = &self.gate {
                let _ = gate.recv_timeout(WAIT);
            }
            self.send(Seen::Init {
                hwnd: dialog.hwnd().0 as isize,
                thread: current_thread(),
            });
            assert_ne!(self.panic_in, Some("init"), "deliberate panic in init");
            self.fill(dialog);
            if let Some((module, id)) = self.icons {
                self.check("set_icons", dialog.set_icons(module, id));
            }
            if self.forge_destroy {
                // SAFETY: posts plain values to the dialog.
                let posted =
                    unsafe { PostMessageW(Some(dialog.hwnd()), WM_DESTROY, WPARAM(0), LPARAM(0)) };
                self.check("PostMessageW", posted.map_err(WinError::from));
            }
            if let Some((x, y)) = self.place {
                self.check("move_to", dialog.move_to(x, y));
            }
        }

        fn command(&mut self, dialog: &Dialog, id: u16, code: u16) {
            self.send(Seen::Command { id, code });
            match (id, code) {
                (ID_BUTTON, notifications::BN_CLICKED) => {
                    assert_ne!(
                        self.panic_in,
                        Some("command"),
                        "deliberate panic in command"
                    );
                    // Each of these makes a control notify the dialog; none of it reaches
                    // this handler, which is busy.
                    dialog.set_checked(ID_CHECK, false);
                    self.check("set_text", dialog.set_text(ID_EDIT, "đã đổi"));
                    self.check("combo_select", dialog.combo_select(ID_COMBO, Some(2)));
                    self.check("list_select", dialog.list_select(ID_LIST, Some(0)));
                    dialog.set_enabled(ID_NESTED, false);
                }
                (ID_EDIT, notifications::EN_CHANGE) => match dialog.text(ID_EDIT) {
                    Ok(text) => self.send(Seen::Text(text)),
                    Err(error) => self.send(Seen::Failed("text", error.kind())),
                },
                (ID_COMBO, notifications::CBN_SELCHANGE) => {
                    self.send(Seen::ComboSelected(dialog.combo_selected(ID_COMBO)));
                }
                (IDOK, notifications::BN_CLICKED) => dialog.end(IDOK as isize),
                (ID_NESTED, notifications::BN_CLICKED) => {
                    if let Some(nested) = self.nested.take() {
                        let result = run_dialog(&template(), Some(dialog.hwnd()), nested);
                        self.send(Seen::Nested(result.map_err(|error| error.kind())));
                    }
                }
                _ => {}
            }
        }

        fn notify(&mut self, dialog: &Dialog, id: u16, code: i32) {
            self.send(Seen::Notify { id, code });
            if (id, code) == (ID_LIST, notifications::LVN_ITEMCHANGED) {
                self.send(Seen::ListSelected(dialog.list_selected(ID_LIST)));
            }
        }

        fn app_message(
            &mut self,
            dialog: &Dialog,
            message: AppMessage,
            wparam: usize,
            lparam: isize,
        ) {
            self.send(Seen::App {
                offset: message.offset(),
                wparam,
                lparam,
                thread: current_thread(),
            });
            match message.offset() {
                END => dialog.end(wparam as isize),
                SELECT => self.send(Seen::Selected {
                    combo: dialog
                        .combo_select(ID_COMBO, Some(wparam))
                        .map_err(|error| error.kind()),
                    list: dialog
                        .list_select(ID_LIST, Some(wparam))
                        .map_err(|error| error.kind()),
                }),
                _ => {}
            }
        }

        fn close_requested(&mut self, _dialog: &Dialog) -> bool {
            self.send(Seen::CloseRequested);
            self.allow_close
        }
    }

    fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
        DialogRect::new(x, y, cx, cy)
    }

    /// The test dialog: never activated, no taskbar button, at an explicit position.
    fn template() -> TemplateBytes {
        DialogTemplate::new("Raumakey thử nghiệm", 220, 160)
            .at(-30_000, -30_000)
            .with_ex_style(styles::WS_EX_NOACTIVATE)
            .with_control(Control::check_box(
                ID_CHECK,
                "Kiểm tra chính tả",
                rect(7, 7, 120, 10),
            ))
            .with_control(Control::push_button(ID_BUTTON, "Đổi", rect(130, 7, 50, 14)))
            .with_control(Control::drop_down_list(ID_COMBO, rect(7, 24, 100, 12)))
            .with_control(Control::edit(ID_EDIT, rect(110, 24, 100, 12)))
            .with_control(Control::list_view(ID_LIST, rect(7, 40, 206, 70)))
            .with_control(Control::label(ID_LABEL, "", rect(7, 114, 206, 8)))
            .with_control(Control::push_button(ID_NESTED, "Mở", rect(7, 140, 50, 14)))
            .with_control(Control::default_push_button(
                IDOK,
                "Đóng",
                rect(110, 140, 50, 14),
            ))
            .with_control(Control::push_button(
                IDCANCEL,
                "Hủy",
                rect(163, 140, 50, 14),
            ))
            .build()
            .unwrap()
    }

    fn next(seen: &Receiver<Seen>) -> Seen {
        seen.recv_timeout(WAIT)
            .expect("the dialog's thread reported in time")
    }

    /// Sends `message` to a window of the dialog's thread and returns the answer; fails the
    /// test when the thread does not answer in time.
    fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
        let mut result = 0_usize;
        // SAFETY: the pointers passed in `lparam` by the callers point to data of this thread
        // that outlives the call (the receiver is in this process); `result` is a valid out
        // pointer.
        let answered = unsafe {
            SendMessageTimeoutW(
                hwnd,
                message,
                WPARAM(wparam),
                LPARAM(lparam),
                SMTO_NORMAL,
                SEND_TIMEOUT_MS,
                Some(&mut result),
            )
        };
        assert_ne!(answered.0, 0, "message {message:#x} was answered in time");
        result as isize
    }

    fn item(dialog: HWND, id: u16) -> HWND {
        // SAFETY: only reads the child window list.
        unsafe { GetDlgItem(Some(dialog), i32::from(id)) }.unwrap()
    }

    fn is_window(hwnd: HWND) -> bool {
        // SAFETY: only queries whether the handle names a window.
        unsafe { IsWindow(Some(hwnd)) }.as_bool()
    }

    /// Waits until Windows showed the dialog: its modal loop shows it the first time its queue
    /// is empty after `init`, which the messages of a test may precede.
    fn wait_until_shown(hwnd: HWND) {
        let deadline = Instant::now() + WAIT;
        // SAFETY: only reads the window's state.
        while !unsafe { IsWindowVisible(hwnd) }.as_bool() {
            assert!(Instant::now() < deadline, "the dialog was shown in time");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn window_text(hwnd: HWND) -> String {
        let length = send(hwnd, WM_GETTEXTLENGTH, 0, 0) as usize;
        let mut buffer = vec![0_u16; length + 1];
        let copied = send(hwnd, WM_GETTEXT, buffer.len(), buffer.as_mut_ptr() as isize) as usize;
        String::from_utf16(&buffer[..copied]).unwrap()
    }

    fn combo_item(combo: HWND, index: usize) -> String {
        let length = send(combo, CB_GETLBTEXTLEN, index, 0) as usize;
        let mut buffer = vec![0_u16; length + 1];
        let copied = send(combo, CB_GETLBTEXT, index, buffer.as_mut_ptr() as isize) as usize;
        String::from_utf16(&buffer[..copied]).unwrap()
    }

    fn list_cell(list: HWND, row: usize, column: i32) -> String {
        let mut buffer = [0_u16; 128];
        let mut cell = LVITEMW {
            iSubItem: column,
            pszText: windows::core::PWSTR(buffer.as_mut_ptr()),
            cchTextMax: buffer.len() as i32,
            ..Default::default()
        };
        let copied = send(list, LVM_GETITEMTEXTW, row, &raw mut cell as isize) as usize;
        String::from_utf16(&buffer[..copied]).unwrap()
    }

    fn list_selected(list: HWND) -> isize {
        send(list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize)
    }

    /// Selects row `row` of a list view the way a click does (`LVM_SETITEMSTATE`), so that the
    /// list view notifies the dialog.
    fn select_row(list: HWND, row: usize) {
        let mut state = LVITEMW {
            state: LVIS_SELECTED,
            stateMask: LVIS_SELECTED,
            ..Default::default()
        };
        send(list, LVM_SETITEMSTATE, row, &raw mut state as isize);
    }

    fn command(dialog: HWND, id: u16, code: u16, control: HWND) {
        let wparam = usize::from(id) | (usize::from(code) << 16);
        send(dialog, WM_COMMAND, wparam, control.0 as isize);
    }

    /// A dialog shown by `spawn_dialog`, with the handle its handler reported.
    struct Running {
        dialog: DetachedDialog,
        seen: Receiver<Seen>,
        hwnd: HWND,
        thread: u32,
    }

    impl Running {
        fn start(probe: Probe, seen: Receiver<Seen>) -> Self {
            let report = probe.seen.clone();
            let dialog = spawn_dialog(template(), probe, move |result| {
                let _ = report.send(Seen::Closed(result));
            })
            .unwrap();
            let Seen::Init { hwnd, thread } = next(&seen) else {
                panic!("the dialog started with init");
            };
            let hwnd = HWND(hwnd as *mut core::ffi::c_void);
            // The handle is published once `init` returned; a message sent now is answered by
            // the dialog's modal loop, after that.
            send(hwnd, WM_NULL, 0, 0);
            Self {
                dialog,
                seen,
                hwnd,
                thread,
            }
        }

        /// Everything the handler saw until a barrier posted now: the messages sent before
        /// were all handled before `send` returned, and posted ones are handled in order.
        fn settle(&self) -> Vec<Seen> {
            assert!(self.dialog.post(app(BARRIER), 0, 0), "the dialog is up");
            let mut seen = Vec::new();
            loop {
                match next(&self.seen) {
                    Seen::App {
                        offset: BARRIER, ..
                    } => return seen,
                    other => seen.push(other),
                }
            }
        }

        /// Waits for `on_close` and returns its result.
        fn closed(&self) -> Option<isize> {
            self.closing().1
        }

        /// Waits for `on_close`: everything the handler saw until then, and the result.
        fn closing(&self) -> (Vec<Seen>, Option<isize>) {
            let mut seen = Vec::new();
            loop {
                match next(&self.seen) {
                    Seen::Closed(result) => return (seen, result),
                    other => seen.push(other),
                }
            }
        }
    }

    impl Drop for Running {
        fn drop(&mut self) {
            // A failed test leaves no dialog behind.
            self.dialog.dismiss();
        }
    }

    fn started(configure: impl FnOnce(&mut Probe)) -> Running {
        let (sender, seen) = mpsc::channel();
        let mut probe = Probe::new(sender);
        configure(&mut probe);
        Running::start(probe, seen)
    }

    fn failures(seen: &[Seen]) -> Vec<&Seen> {
        seen.iter()
            .filter(|seen| matches!(seen, Seen::Failed(..)))
            .collect()
    }

    #[test]
    fn detached_dialogs_are_shared_between_threads() {
        fn assert_send_sync<T: Send + Sync + Clone>() {}
        assert_send_sync::<DetachedDialog>();
    }

    #[test]
    fn init_fills_the_controls_without_notifying_the_handler() {
        let running = started(|_| {});
        let seen = running.settle();
        assert_eq!(failures(&seen), Vec::<&Seen>::new());
        // The programmatic changes of `init` (EN_CHANGE and EN_UPDATE of the edit,
        // LVN_ITEMCHANGED of the list view) did not reach the handler, which was busy. Other
        // notifications may: the shown dialog's own, such as NM_CUSTOMDRAW when the list view
        // paints, or NM_SETFOCUS.
        for seen in &seen {
            assert!(
                !matches!(
                    seen,
                    Seen::Command {
                        code: notifications::BN_CLICKED
                            | notifications::CBN_SELCHANGE
                            | notifications::EN_CHANGE
                            | EN_UPDATE,
                        ..
                    } | Seen::Notify {
                        code: notifications::LVN_ITEMCHANGED,
                        ..
                    } | Seen::Text(_)
                        | Seen::ComboSelected(_)
                        | Seen::ListSelected(_)
                ),
                "{seen:?}"
            );
        }
        let hwnd = running.hwnd;
        assert_eq!(send(item(hwnd, ID_CHECK), BM_GETCHECK, 0, 0), 1);
        let combo = item(hwnd, ID_COMBO);
        assert_eq!(send(combo, CB_GETCOUNT, 0, 0), 3);
        assert_eq!(send(combo, CB_GETCURSEL, 0, 0), 1);
        let items: Vec<String> = (0..3).map(|index| combo_item(combo, index)).collect();
        assert_eq!(items, METHODS);
        assert_eq!(window_text(item(hwnd, ID_EDIT)), EDIT_TEXT);
        assert_eq!(window_text(item(hwnd, ID_LABEL)), LABEL_TEXT);
        let list = item(hwnd, ID_LIST);
        assert_eq!(send(list, LVM_GETITEMCOUNT, 0, 0), 3);
        for (row, cells) in ROWS.iter().enumerate() {
            assert_eq!(list_cell(list, row, 0), cells[0]);
            assert_eq!(list_cell(list, row, 1), cells[1]);
        }
        assert_eq!(list_selected(list), 2);
        // The dialog stays where `init` moved it: off the screen.
        let mut frame = RECT::default();
        // SAFETY: `frame` is a valid out buffer.
        unsafe { GetWindowRect(hwnd, &mut frame) }.unwrap();
        assert!(frame.right < 0 && frame.bottom < 0, "{frame:?}");
        assert_ne!(running.thread, current_thread());
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
    }

    #[test]
    fn commands_and_notifications_reach_the_handler_with_their_ids_and_codes() {
        let running = started(|_| {});
        let hwnd = running.hwnd;
        // A click on the check box toggles it and reports BN_CLICKED.
        send(item(hwnd, ID_CHECK), BM_CLICK, 0, 0);
        assert_eq!(send(item(hwnd, ID_CHECK), BM_GETCHECK, 0, 0), 0);
        // A selection in the drop-down list: the list moves, then tells the dialog.
        let combo = item(hwnd, ID_COMBO);
        send(combo, CB_SETCURSEL, 2, 0);
        command(hwnd, ID_COMBO, notifications::CBN_SELCHANGE, combo);
        // Typing in the edit: WM_SETTEXT makes it report EN_CHANGE.
        let edit = item(hwnd, ID_EDIT);
        let typed: Vec<u16> = "Gõ tắt (macro)".encode_utf16().chain([0]).collect();
        send(edit, WM_SETTEXT, 0, typed.as_ptr() as isize);
        // A click on a row of the list view.
        let list = item(hwnd, ID_LIST);
        select_row(list, 1);
        // A double click, as the list view reports it.
        let mut header = NMHDR {
            hwndFrom: list,
            idFrom: usize::from(ID_LIST),
            code: notifications::NM_DBLCLK as u32,
        };
        send(
            hwnd,
            WM_NOTIFY,
            usize::from(ID_LIST),
            &raw mut header as isize,
        );
        let seen = running.settle();
        assert_eq!(failures(&seen), Vec::<&Seen>::new());
        let expected = [
            Seen::Command {
                id: ID_CHECK,
                code: notifications::BN_CLICKED,
            },
            Seen::Command {
                id: ID_COMBO,
                code: notifications::CBN_SELCHANGE,
            },
            Seen::ComboSelected(Some(2)),
            Seen::Command {
                id: ID_EDIT,
                code: notifications::EN_CHANGE,
            },
            Seen::Text("Gõ tắt (macro)".to_owned()),
            Seen::Notify {
                id: ID_LIST,
                code: notifications::LVN_ITEMCHANGED,
            },
            Seen::Notify {
                id: ID_LIST,
                code: notifications::NM_DBLCLK,
            },
        ];
        let mut rest = seen.iter();
        for wanted in &expected {
            assert!(
                rest.any(|seen| seen == wanted),
                "{wanted:?} in order in {seen:?}"
            );
        }
        // The rows' selection changes are reported, and the handler reads the new selection.
        assert!(seen.contains(&Seen::ListSelected(Some(1))), "{seen:?}");
        // Focus changes and the like are reported too, but no close request.
        assert!(!seen.contains(&Seen::CloseRequested), "{seen:?}");
    }

    #[test]
    fn programmatic_changes_made_by_the_handler_do_not_reenter_it() {
        let running = started(|_| {});
        let hwnd = running.hwnd;
        send(item(hwnd, ID_BUTTON), BM_CLICK, 0, 0);
        let seen = running.settle();
        assert_eq!(failures(&seen), Vec::<&Seen>::new());
        // The click itself is reported...
        assert!(
            seen.contains(&Seen::Command {
                id: ID_BUTTON,
                code: notifications::BN_CLICKED
            }),
            "{seen:?}"
        );
        // ...and so is the focus the click took from the edit (EN_KILLFOCUS)...
        assert!(
            seen.contains(&Seen::Command {
                id: ID_EDIT,
                code: EN_KILLFOCUS
            }),
            "{seen:?}"
        );
        // ...but not what the handler's changes made the controls report (the shown dialog's
        // own notifications, such as NM_CUSTOMDRAW when the list view repaints its new
        // selection, may arrive).
        for seen in &seen {
            assert!(
                !matches!(
                    seen,
                    Seen::Command {
                        id: ID_EDIT,
                        code: notifications::EN_CHANGE | EN_UPDATE
                    } | Seen::Command {
                        id: ID_CHECK,
                        code: notifications::BN_CLICKED
                    } | Seen::Command {
                        id: ID_COMBO,
                        code: notifications::CBN_SELCHANGE
                    } | Seen::Notify {
                        code: notifications::LVN_ITEMCHANGED,
                        ..
                    } | Seen::Text(_)
                        | Seen::ComboSelected(_)
                        | Seen::ListSelected(_)
                ),
                "{seen:?}"
            );
        }
        // The changes were made.
        assert_eq!(send(item(hwnd, ID_CHECK), BM_GETCHECK, 0, 0), 0);
        assert_eq!(window_text(item(hwnd, ID_EDIT)), "đã đổi");
        assert_eq!(send(item(hwnd, ID_COMBO), CB_GETCURSEL, 0, 0), 2);
        assert_eq!(list_selected(item(hwnd, ID_LIST)), 0);
        // SAFETY: only reads the window state.
        let enabled = unsafe { IsWindowEnabled(item(hwnd, ID_NESTED)) }.as_bool();
        assert!(!enabled);
    }

    #[test]
    fn a_posted_app_message_runs_on_the_dialogs_thread() {
        let running = started(|_| {});
        assert!(running.dialog.post(app(RECORD), 42, -7));
        let seen = running.settle();
        assert!(
            seen.contains(&Seen::App {
                offset: RECORD,
                wparam: 42,
                lparam: -7,
                thread: running.thread,
            }),
            "{seen:?}"
        );
        assert_ne!(running.thread, current_thread());
    }

    /// `Dialog::move_to` places the dialog in screen coordinates; from `init` that happens
    /// before Windows shows it (the controller's off-screen test panel relies on it).
    #[test]
    fn move_to_places_the_dialog_in_screen_coordinates() {
        let running = started(|probe| probe.place = Some((-31_500, -31_400)));
        let seen = running.settle();
        assert_eq!(failures(&seen), Vec::<&Seen>::new());
        let mut frame = RECT::default();
        // SAFETY: `frame` is a valid out buffer.
        unsafe { GetWindowRect(running.hwnd, &mut frame) }.unwrap();
        assert_eq!((frame.left, frame.top), (-31_500, -31_400), "{frame:?}");
    }

    /// The reserved top of the `WM_APP` range is refused by `post`, and the handler never sees
    /// those messages even when they are posted to the window directly.
    #[test]
    fn reserved_messages_are_neither_posted_nor_handed_to_the_handler() {
        let running = started(|_| {});
        for reserved in [RESERVED_FIRST, RESERVED_FIRST + 5] {
            let message = AppMessage::from_id(reserved).unwrap();
            assert!(!running.dialog.post(message, 0, 0), "{reserved:#x}");
            // SAFETY: posts plain values to the dialog.
            unsafe { PostMessageW(Some(running.hwnd), reserved, WPARAM(0), LPARAM(0)) }.unwrap();
        }
        let seen = running.settle();
        assert!(
            !seen.iter().any(|seen| matches!(seen, Seen::App { .. })),
            "{seen:?}"
        );
        assert!(is_window(running.hwnd));
        assert!(!running.dialog.is_closed());
    }

    #[test]
    fn a_handler_that_refuses_to_close_keeps_the_dialog_until_it_is_dismissed() {
        let running = started(|probe| probe.allow_close = false);
        let hwnd = running.hwnd;
        // The Cancel button, the close box (WM_CLOSE), and Esc all ask the handler.
        send(item(hwnd, IDCANCEL), BM_CLICK, 0, 0);
        send(hwnd, WM_CLOSE, 0, 0);
        let edit = item(hwnd, ID_EDIT);
        // SAFETY: posts plain values (VK_ESCAPE) to a control of the dialog.
        unsafe { PostMessageW(Some(edit), WM_KEYDOWN, WPARAM(0x1B), LPARAM(0)) }.unwrap();
        let seen = running.settle();
        let asked = seen
            .iter()
            .filter(|seen| **seen == Seen::CloseRequested)
            .count();
        assert_eq!(asked, 3, "{seen:?}");
        assert!(is_window(hwnd));
        assert!(!running.dialog.is_closed());
        // Dismissing does not ask.
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
        assert!(running.dialog.is_closed());
        assert!(!is_window(hwnd));
        // A closed dialog takes no more messages.
        assert!(!running.dialog.post(app(RECORD), 0, 0));
        assert!(!running.dialog.bring_to_front());
    }

    #[test]
    fn a_handler_that_agrees_to_close_ends_the_dialog_with_idcancel() {
        let running = started(|_| {});
        command(
            running.hwnd,
            IDCANCEL,
            notifications::BN_CLICKED,
            item(running.hwnd, IDCANCEL),
        );
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
        assert!(running.dialog.is_closed());
    }

    #[test]
    fn on_close_gets_the_result_the_handler_ended_with() {
        let running = started(|_| {});
        assert!(running.dialog.post(app(END), 77, 0));
        assert_eq!(running.closed(), Some(77));
        assert!(running.dialog.is_closed());
    }

    /// The dialog is dismissed while its handler's `init` runs, before its handle is published:
    /// it ends without being shown (or, had its thread not started yet, never starts), and
    /// `on_close` gets `None` either way.
    #[test]
    fn a_dialog_dismissed_before_it_is_shown_is_never_shown() {
        let (gate, gated) = mpsc::channel();
        let (sender, seen) = mpsc::channel();
        let mut probe = Probe::new(sender.clone());
        probe.gate = Some(gated);
        let dialog = spawn_dialog(template(), probe, move |result| {
            let _ = sender.send(Seen::Closed(result));
        })
        .unwrap();
        // Nothing is published before `init` returns.
        assert!(!dialog.post(app(RECORD), 0, 0));
        assert!(!dialog.bring_to_front());
        dialog.dismiss();
        let _ = gate.send(());
        loop {
            match next(&seen) {
                Seen::Init { .. } | Seen::Failed(..) => {}
                Seen::Closed(result) => {
                    assert_eq!(result, None);
                    break;
                }
                other => panic!("{other:?}"),
            }
        }
        assert!(dialog.is_closed());
    }

    #[test]
    fn a_panic_in_init_is_contained_and_the_dialog_can_still_be_dismissed() {
        let running = started(|probe| probe.panic_in = Some("init"));
        assert!(is_window(running.hwnd));
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
    }

    #[test]
    fn a_panic_in_a_callback_is_contained_and_the_handler_keeps_working() {
        let running = started(|probe| probe.panic_in = Some("command"));
        let hwnd = running.hwnd;
        send(item(hwnd, ID_BUTTON), BM_CLICK, 0, 0);
        send(item(hwnd, ID_CHECK), BM_CLICK, 0, 0);
        let seen = running.settle();
        assert!(
            seen.contains(&Seen::Command {
                id: ID_CHECK,
                code: notifications::BN_CLICKED
            }),
            "the handler is not left borrowed: {seen:?}"
        );
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
    }

    /// A `WM_INITDIALOG` sent to a live dialog (it carries a pointer that a sender outside
    /// `run_dialog` cannot know) neither runs `init` again nor replaces the handler.
    #[test]
    fn a_forged_wm_initdialog_is_ignored() {
        let running = started(|_| {});
        send(running.hwnd, WM_INITDIALOG, 0, 0x10);
        let seen = running.settle();
        assert!(
            !seen.iter().any(|seen| matches!(seen, Seen::Init { .. })),
            "{seen:?}"
        );
        assert!(running.dialog.post(app(RECORD), 1, 2));
        assert!(running.settle().contains(&Seen::App {
            offset: RECORD,
            wparam: 1,
            lparam: 2,
            thread: running.thread,
        }));
    }

    /// `WM_NOTIFY` carries a pointer, yet Windows lets any process post it (and send it from
    /// another process, where the pointer means nothing here). A pointer to memory that cannot
    /// be read, and a readable header that does not name a control of the dialog, are ignored;
    /// the dialog keeps working.
    #[test]
    fn a_forged_wm_notify_is_ignored() {
        let running = started(|_| {});
        let hwnd = running.hwnd;
        // SAFETY: posts plain values; the dialog must not trust the pointer value.
        unsafe { PostMessageW(Some(hwnd), WM_NOTIFY, WPARAM(0), LPARAM(0x10)) }.unwrap();
        send(hwnd, WM_NOTIFY, 0, 0x20);
        let list = item(hwnd, ID_LIST);
        for (from, id) in [(hwnd, ID_LIST), (HWND::default(), ID_LIST), (list, ID_EDIT)] {
            let mut header = NMHDR {
                hwndFrom: from,
                idFrom: usize::from(id),
                code: notifications::NM_DBLCLK as u32,
            };
            send(hwnd, WM_NOTIFY, usize::from(id), &raw mut header as isize);
        }
        let seen = running.settle();
        assert!(
            !seen.iter().any(|seen| matches!(seen, Seen::Notify { .. })),
            "{seen:?}"
        );
        assert!(is_window(hwnd));
        // A genuine one still arrives.
        select_row(list, 0);
        assert!(running.settle().contains(&Seen::Notify {
            id: ID_LIST,
            code: notifications::LVN_ITEMCHANGED
        }));
    }

    /// `WM_DESTROY` carries nothing, so any process may post or send it, although only
    /// `DestroyWindow` sends the genuine one, after hiding the window. A forged one that reaches
    /// the shown dialog changes nothing: `post` still reaches the handler, and the Cancel button
    /// still asks it and ends the dialog.
    #[test]
    fn a_forged_wm_destroy_leaves_the_dialog_working() {
        let running = started(|_| {});
        let hwnd = running.hwnd;
        wait_until_shown(hwnd);
        // SAFETY: posts plain values to the dialog.
        unsafe { PostMessageW(Some(hwnd), WM_DESTROY, WPARAM(0), LPARAM(0)) }.unwrap();
        send(hwnd, WM_DESTROY, 0, 0);
        assert!(
            running.dialog.post(app(RECORD), 3, 4),
            "the dialog is still published"
        );
        let seen = running.settle();
        assert!(
            seen.contains(&Seen::App {
                offset: RECORD,
                wparam: 3,
                lparam: 4,
                thread: running.thread,
            }),
            "{seen:?}"
        );
        send(item(hwnd, IDCANCEL), BM_CLICK, 0, 0);
        let (seen, result) = running.closing();
        assert!(seen.contains(&Seen::CloseRequested), "{seen:?}");
        assert_eq!(result, Some(IDCANCEL as isize));
        assert!(running.dialog.is_closed());
        assert!(!is_window(hwnd));
    }

    /// After a forged `WM_DESTROY`, a handler that refuses to close is still asked, and
    /// `dismiss` still closes the dialog.
    #[test]
    fn a_dialog_that_got_a_forged_wm_destroy_can_still_be_dismissed() {
        let running = started(|probe| probe.allow_close = false);
        let hwnd = running.hwnd;
        wait_until_shown(hwnd);
        send(hwnd, WM_DESTROY, 0, 0);
        send(hwnd, WM_CLOSE, 0, 0);
        assert!(is_window(hwnd));
        assert!(!running.dialog.is_closed());
        running.dialog.dismiss();
        let (seen, result) = running.closing();
        assert!(seen.contains(&Seen::CloseRequested), "{seen:?}");
        assert_eq!(result, Some(IDCANCEL as isize));
        assert!(running.dialog.is_closed());
        assert!(!is_window(hwnd));
    }

    /// A forged `WM_DESTROY` sent to a nested dialog does not hide it from its parent:
    /// dismissing the parent still closes both.
    #[test]
    fn a_nested_dialog_that_got_a_forged_wm_destroy_is_dismissed_with_its_parent() {
        let (nested_sender, nested_seen) = mpsc::channel();
        let nested = Probe::new(nested_sender);
        let running = started(move |probe| probe.nested = Some(Box::new(nested)));
        let wparam = usize::from(ID_NESTED);
        // SAFETY: posts plain values (a button's command) to the dialog.
        unsafe { PostMessageW(Some(running.hwnd), WM_COMMAND, WPARAM(wparam), LPARAM(0)) }.unwrap();
        let Seen::Init {
            hwnd: nested_hwnd, ..
        } = next(&nested_seen)
        else {
            panic!("the nested dialog started with init");
        };
        let nested_hwnd = HWND(nested_hwnd as *mut core::ffi::c_void);
        wait_until_shown(nested_hwnd);
        send(nested_hwnd, WM_DESTROY, 0, 0);
        assert!(is_window(nested_hwnd));
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
        assert!(!is_window(nested_hwnd));
        assert!(!is_window(running.hwnd));
    }

    /// A forged `WM_DESTROY` that reaches the dialog before Windows first shows it (here posted
    /// by `init`, so it is handled before the modal loop shows the dialog) cannot be told from
    /// the genuine one, which also finds the dialog hidden: the handler hears nothing until the
    /// dialog is shown, and everything again from then on.
    #[test]
    fn a_forged_wm_destroy_before_the_dialog_is_shown_mutes_the_handler_only_until_then() {
        let running = started(|probe| probe.forge_destroy = true);
        let hwnd = running.hwnd;
        wait_until_shown(hwnd);
        send(item(hwnd, IDCANCEL), BM_CLICK, 0, 0);
        let (seen, result) = running.closing();
        assert!(seen.contains(&Seen::CloseRequested), "{seen:?}");
        assert_eq!(result, Some(IDCANCEL as isize));
    }

    /// The genuine `WM_DESTROY` comes from `DestroyWindow`, which hides the shown dialog first,
    /// on every way a dialog ends (the handler's `end`, a close request it agreed to,
    /// `dismiss`): from then on the handler hears nothing, such as the `LVN_DELETEALLITEMS` or
    /// `LVN_DELETEITEM` that the list view sends while it is destroyed with the dialog.
    #[test]
    fn the_handler_hears_nothing_once_the_dialog_is_destroyed() {
        for how in ["end", "cancel", "dismiss"] {
            let running = started(|_| {});
            wait_until_shown(running.hwnd);
            let _ = running.settle();
            match how {
                "end" => assert!(running.dialog.post(app(END), 5, 0)),
                "cancel" => command(
                    running.hwnd,
                    IDCANCEL,
                    notifications::BN_CLICKED,
                    item(running.hwnd, IDCANCEL),
                ),
                _ => running.dialog.dismiss(),
            }
            let (seen, result) = running.closing();
            assert!(result.is_some(), "{how}");
            assert!(
                !seen.iter().any(|seen| matches!(
                    seen,
                    Seen::Notify {
                        code: LVN_DELETEALLITEMS | LVN_DELETEITEM,
                        ..
                    }
                )),
                "{how}: {seen:?}"
            );
        }
    }

    /// The dialog procedure honours its reserved messages only with the dialog's own cookie in
    /// `wParam`, which only its `DetachedDialog` (and the dialog that runs it) knows: a dismiss
    /// posted or sent by anyone else is ignored.
    #[test]
    fn a_reserved_message_without_the_dialogs_cookie_is_ignored() {
        let running = started(|_| {});
        let hwnd = running.hwnd;
        for wparam in [0, 1, 0x1234_5678, usize::MAX] {
            // SAFETY: posts plain values to the dialog.
            unsafe { PostMessageW(Some(hwnd), DISMISS, WPARAM(wparam), LPARAM(0)) }.unwrap();
            send(hwnd, DISMISS, wparam, 0);
        }
        let seen = running.settle();
        assert!(
            !seen.iter().any(|seen| matches!(seen, Seen::App { .. })),
            "{seen:?}"
        );
        assert!(is_window(hwnd));
        assert!(!running.dialog.is_closed());
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
        assert!(!is_window(hwnd));
    }

    /// USER reads the index of `CB_SETCURSEL` and `LVM_SETITEMSTATE` as an `int`: an index
    /// beyond `i32::MAX` is past the last item, so it fails and selects nothing instead of
    /// selecting whatever its low 32 bits name.
    #[test]
    fn an_index_beyond_i32_max_fails_and_selects_nothing() {
        let running = started(|_| {});
        let hwnd = running.hwnd;
        let (combo, list) = (item(hwnd, ID_COMBO), item(hwnd, ID_LIST));
        let mut indexes = vec![i32::MAX as usize + 1, usize::MAX - 1];
        // On 64-bit Windows: an index whose low 32 bits name item 1.
        indexes.extend(usize::try_from((1_u64 << 32) | 1).ok());
        for index in indexes {
            assert!(running.dialog.post(app(SELECT), index, 0));
            let seen = running.settle();
            let failed = Err(WinErrorKind::InvalidInput);
            assert!(
                seen.contains(&Seen::Selected {
                    combo: failed,
                    list: failed
                }),
                "{index:#x}: {seen:?}"
            );
            assert_eq!(send(combo, CB_GETCURSEL, 0, 0), -1, "{index:#x}");
            assert_eq!(list_selected(list), -1, "{index:#x}");
        }
    }

    /// A panic while the handler is dropped, after the dialog closed, is contained: `on_close`
    /// still gets the result the dialog ended with, and `run_dialog` returns it.
    #[test]
    fn the_result_survives_a_panic_while_the_handler_is_dropped() {
        let running = started(|probe| probe.panic_on_drop = true);
        assert!(running.dialog.post(app(END), 77, 0));
        assert_eq!(running.closed(), Some(77));

        let (sender, seen) = mpsc::channel();
        let mut probe = Probe::new(sender);
        probe.panic_on_drop = true;
        let (result_sender, result) = mpsc::channel();
        std::thread::spawn(move || {
            let result = run_dialog(&template(), None, Box::new(probe));
            let _ = result_sender.send(result.map_err(|error| error.kind()));
        });
        let Seen::Init { hwnd, .. } = next(&seen) else {
            panic!("the dialog started with init");
        };
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        send(hwnd, WM_NULL, 0, 0);
        // SAFETY: posts plain values to the dialog: END with 78.
        unsafe { PostMessageW(Some(hwnd), app(END).id(), WPARAM(78), LPARAM(0)) }.unwrap();
        let result = result.recv_timeout(WAIT).expect("run_dialog returned");
        assert_eq!(result, Ok(78));
    }

    thread_local! {
        /// The windows that a `WindowVeto` of this thread refused to create.
        static VETOED: Cell<u32> = const { Cell::new(0) };
    }

    /// A `WH_CBT` hook of the calling thread only (no other thread is hooked) that refuses to
    /// create any window, so that a message box shown by mistake fails, and is counted, instead
    /// of appearing. Dropping it removes the hook.
    struct WindowVeto(HHOOK);

    impl WindowVeto {
        fn install() -> Self {
            VETOED.with(|vetoed| vetoed.set(0));
            // SAFETY: a hook procedure of this process for the calling thread only, so no
            // module handle is needed.
            let hook =
                unsafe { SetWindowsHookExW(WH_CBT, Some(veto_creation), None, current_thread()) };
            Self(hook.unwrap())
        }

        /// The number of windows refused on this thread since `install`.
        fn vetoed() -> u32 {
            VETOED.with(Cell::get)
        }
    }

    impl Drop for WindowVeto {
        fn drop(&mut self) {
            // SAFETY: removes the hook that `install` set on this thread.
            let _ = unsafe { UnhookWindowsHookEx(self.0) };
        }
    }

    unsafe extern "system" fn veto_creation(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let vetoed = guard_value(false, || {
            let creating = code == HCBT_CREATEWND as i32;
            if creating {
                VETOED.with(|vetoed| vetoed.set(vetoed.get().saturating_add(1)));
            }
            creating
        });
        if vetoed {
            // Nonzero: the window is not created.
            return LRESULT(1);
        }
        // SAFETY: forwards the unchanged arguments to the next hook.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// `Dialog::message_box` asked in `init` of a dialog that was dismissed before its handle
    /// was published fails with `E_ABORT` without creating a box (a `WindowVeto` would refuse
    /// it), and the dialog ends unshown.
    #[test]
    fn a_message_box_asked_in_init_of_a_dismissed_dialog_fails_without_showing() {
        type Answer = (Result<MessageResponse, WinErrorKind>, u32);
        struct AskInInit {
            started: Sender<()>,
            gate: Receiver<()>,
            answers: Sender<Answer>,
        }
        impl DialogHandler for AskInInit {
            fn init(&mut self, dialog: &Dialog) {
                move_off_screen(dialog.hwnd());
                let veto = WindowVeto::install();
                let _ = self.started.send(());
                let _ = self.gate.recv_timeout(WAIT);
                let answer =
                    dialog.message_box("Lưu các thay đổi? (thử nghiệm)", MessageKind::Question);
                let vetoed = WindowVeto::vetoed();
                drop(veto);
                let _ = self
                    .answers
                    .send((answer.map_err(|error| error.kind()), vetoed));
            }
        }
        let (started, has_started) = mpsc::channel();
        let (open, gate) = mpsc::channel();
        let (answer_sender, answers) = mpsc::channel();
        let (close_sender, closed) = mpsc::channel();
        let handler = AskInInit {
            started,
            gate,
            answers: answer_sender,
        };
        let dialog = spawn_dialog(template(), handler, move |result| {
            let _ = close_sender.send(result);
        })
        .unwrap();
        has_started.recv_timeout(WAIT).expect("init started");
        dialog.dismiss();
        let _ = open.send(());
        let (answer, vetoed) = answers.recv_timeout(WAIT).expect("init asked");
        assert_eq!(answer, Err(WinErrorKind::Cancelled));
        assert_eq!(vetoed, 0, "no window was created for the box");
        assert_eq!(closed.recv_timeout(WAIT).expect("on_close ran"), None);
        assert!(dialog.is_closed());
    }

    #[test]
    fn icons_are_loaded_for_the_title_bar_and_the_taskbar() {
        let file = ResourceFile::system("shell32.dll");
        let first = *group_icon_ids(&file)
            .first()
            .expect("shell32 has icon groups");
        let module = file.module();
        let running = started(|probe| probe.icons = Some((module, first)));
        let seen = running.settle();
        assert_eq!(failures(&seen), Vec::<&Seen>::new());
        assert_ne!(send(running.hwnd, WM_GETICON, ICON_BIG as usize, 0), 0);
        assert_ne!(send(running.hwnd, WM_GETICON, ICON_SMALL as usize, 0), 0);
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
        drop(file);
    }

    /// `run_dialog` on a thread of the caller's: it returns the result the handler ended with
    /// (here -1, which `DialogBoxIndirectParamW` also returns when it fails), and the handler
    /// is dropped once, after the dialog is gone.
    #[test]
    fn run_dialog_returns_the_result_and_drops_the_handler_once() {
        let (sender, seen) = mpsc::channel();
        let drops = Arc::new(AtomicUsize::new(0));
        let mut probe = Probe::new(sender);
        probe.drops = Some(Arc::clone(&drops));
        let (result_sender, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = result_sender.send(run_dialog(&template(), None, Box::new(probe)));
        });
        let Seen::Init { hwnd, .. } = next(&seen) else {
            panic!("the dialog started with init");
        };
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        send(hwnd, WM_NULL, 0, 0);
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        // SAFETY: posts plain values to the dialog: END with -1.
        unsafe { PostMessageW(Some(hwnd), app(END).id(), WPARAM(usize::MAX), LPARAM(0)) }.unwrap();
        let result = result.recv_timeout(WAIT).expect("run_dialog returned");
        assert_eq!(result.unwrap(), -1);
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(!is_window(hwnd));
    }

    /// The OK button ends the dialog through the handler (`Dialog::end`).
    #[test]
    fn the_handler_ends_the_dialog_from_its_ok_button() {
        let (sender, seen) = mpsc::channel();
        let probe = Probe::new(sender);
        let (result_sender, result) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = result_sender.send(run_dialog(&template(), None, Box::new(probe)));
        });
        let Seen::Init { hwnd, .. } = next(&seen) else {
            panic!("the dialog started with init");
        };
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        send(item(hwnd, IDOK), BM_CLICK, 0, 0);
        let result = result.recv_timeout(WAIT).expect("run_dialog returned");
        assert_eq!(result.unwrap(), IDOK as isize);
    }

    /// A dialog that cannot be created (here: its owner is no window) fails, and its handler
    /// is dropped once without `init`.
    #[test]
    fn a_dialog_that_cannot_be_created_fails_and_drops_its_handler() {
        let (sender, seen) = mpsc::channel();
        let drops = Arc::new(AtomicUsize::new(0));
        let mut probe = Probe::new(sender);
        probe.drops = Some(Arc::clone(&drops));
        let result = std::thread::spawn(move || {
            let not_a_window = HWND(0x0BAD_0000_usize as *mut core::ffi::c_void);
            run_dialog(&template(), Some(not_a_window), Box::new(probe)).map_err(|e| e.context())
        })
        .join()
        .unwrap();
        assert_eq!(result, Err("DialogBoxIndirectParamW"));
        assert_eq!(drops.load(Ordering::SeqCst), 1);
        assert!(seen.try_recv().is_err(), "init never ran");
    }

    /// A dialog that the handler runs with the dialog as its owner (a nested dialog) closes
    /// with its parent when the parent is dismissed.
    #[test]
    fn dismissing_a_dialog_also_dismisses_the_dialog_it_runs() {
        let (nested_sender, nested_seen) = mpsc::channel();
        let nested = Probe::new(nested_sender);
        let running = started(move |probe| probe.nested = Some(Box::new(nested)));
        // Posted: the click runs the nested dialog's modal loop inside the handler, and the
        // test must go on meanwhile.
        let wparam = usize::from(ID_NESTED);
        // SAFETY: posts plain values (a button's command) to the dialog.
        unsafe { PostMessageW(Some(running.hwnd), WM_COMMAND, WPARAM(wparam), LPARAM(0)) }.unwrap();
        let Seen::Init {
            hwnd: nested_hwnd, ..
        } = next(&nested_seen)
        else {
            panic!("the nested dialog started with init");
        };
        let nested_hwnd = HWND(nested_hwnd as *mut core::ffi::c_void);
        send(nested_hwnd, WM_NULL, 0, 0);
        assert!(is_window(nested_hwnd));
        running.dialog.dismiss();
        assert_eq!(running.closed(), Some(IDCANCEL as isize));
        assert!(!is_window(nested_hwnd));
        assert!(!is_window(running.hwnd));
    }

    /// The nested dialog's own result reaches the handler that ran it.
    #[test]
    fn a_nested_dialog_returns_its_result_to_the_handler_that_ran_it() {
        let (nested_sender, nested_seen) = mpsc::channel();
        let nested = Probe::new(nested_sender);
        let running = started(move |probe| probe.nested = Some(Box::new(nested)));
        let wparam = usize::from(ID_NESTED);
        // SAFETY: posts plain values (a button's command) to the dialog.
        unsafe { PostMessageW(Some(running.hwnd), WM_COMMAND, WPARAM(wparam), LPARAM(0)) }.unwrap();
        let Seen::Init {
            hwnd: nested_hwnd, ..
        } = next(&nested_seen)
        else {
            panic!("the nested dialog started with init");
        };
        let nested_hwnd = HWND(nested_hwnd as *mut core::ffi::c_void);
        send(item(nested_hwnd, IDOK), BM_CLICK, 0, 0);
        let seen = running.settle();
        assert!(seen.contains(&Seen::Nested(Ok(IDOK as isize))), "{seen:?}");
        assert!(is_window(running.hwnd));
        assert!(!running.dialog.is_closed());
    }

    /// Shows a dialog whose handler asks a question in a message box, then dismisses the
    /// dialog after a second: the box closes with it, and the question fails as cancelled.
    /// Manual test.
    #[test]
    #[ignore = "manual: shows a dialog and a message box for about a second"]
    fn manual_dismissing_a_dialog_closes_its_message_box() {
        struct Asker(Sender<Result<MessageResponse, WinErrorKind>>);
        impl DialogHandler for Asker {
            fn app_message(&mut self, dialog: &Dialog, _: AppMessage, _: usize, _: isize) {
                let answer = dialog.message_box(
                    "Hộp này tự đóng sau một giây (thử nghiệm).",
                    MessageKind::Question,
                );
                let _ = self.0.send(answer.map_err(|error| error.kind()));
            }
        }
        let (sender, answers) = mpsc::channel();
        let template = DialogTemplate::new("Raumakey thử nghiệm", 200, 80)
            .build()
            .unwrap();
        let dialog = spawn_dialog(template, Asker(sender), |_| {}).unwrap();
        let deadline = std::time::Instant::now() + WAIT;
        while !dialog.post(app(RECORD), 0, 0) {
            assert!(std::time::Instant::now() < deadline, "the dialog came up");
            std::thread::sleep(Duration::from_millis(10));
        }
        std::thread::sleep(Duration::from_secs(1));
        dialog.dismiss();
        let answer = answers.recv_timeout(WAIT).unwrap();
        assert_eq!(answer, Err(WinErrorKind::Cancelled));
    }
}
