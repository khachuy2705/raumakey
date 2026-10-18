//! `dialog_box`: the helpers that the macro list editor adds to S1's dialogs, on real dialog
//! boxes: a private message that the handler posts to its own dialog (from `init`, it arrives
//! before the dialog is shown), the text limit of an edit, multi-line edits, and virtual list
//! views (`LVS_OWNERDATA`) whose cells the dialog keeps and hands to the list view itself
//! (`Dialog::list_set_virtual_rows`), also while the handler is busy, and searches when the
//! user types into the list view.
//!
//! The dialogs stay out of the way as in `dialog_box`: `WS_EX_NOACTIVATE`, an explicit
//! position, and a move to (-32000, -32000) in `init`, before Windows shows them. They are
//! driven from the test thread with `SendMessageTimeoutW`, and every wait has a timeout. The
//! tests of typing run their dialogs under Common Controls 6, as `raumakey.exe` does
//! (`CommonControls6`). Forged notifications point into pages of this process's memory that
//! the tests allocate with the protection they need (`VirtualAlloc`) and release.

use core::ffi::c_void;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use raumakey_windows::dialog_box::{
    DetachedDialog, Dialog, DialogHandler, RESERVED_FIRST, notifications, spawn_dialog,
};
use raumakey_windows::dialog_template::{
    Control, DialogRect, DialogTemplate, IDCANCEL, TemplateBytes, styles,
};
use raumakey_windows::error::WinErrorKind;
use raumakey_windows::strings::until_nul;
use raumakey_windows::window::AppMessage;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ,
    PAGE_NOACCESS, PAGE_PROTECTION_FLAGS, PAGE_READONLY, PAGE_READWRITE, VirtualAlloc, VirtualFree,
    VirtualProtect, VirtualQuery,
};
use windows::Win32::System::SystemInformation::{GetSystemInfo, SYSTEM_INFO};
use windows::Win32::UI::Controls::{
    EM_GETLIMITTEXT, LVFI_NEARESTXY, LVFI_PARAM, LVFI_PARTIAL, LVFI_STRING, LVFI_SUBSTRING,
    LVFI_WRAP, LVFINDINFOW, LVFINDINFOW_FLAGS, LVIF_TEXT, LVIS_SELECTED, LVITEMW, LVM_GETITEMCOUNT,
    LVM_GETITEMTEXTW, LVM_GETNEXTITEM, LVM_SETITEMSTATE, LVN_GETDISPINFOW, LVN_ODFINDITEMW,
    LVNI_SELECTED, NMHDR, NMLVDISPINFOW, NMLVFINDITEMW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BM_CLICK, GWL_STYLE, GetDlgItem, GetWindowLongW, IsWindowVisible, SMTO_NORMAL, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SendMessageTimeoutW, SendMessageW, SetWindowPos, WM_CHAR, WM_NOTIFY,
    WM_NULL, WM_SETTEXT,
};
use windows::core::{PCWSTR, PWSTR};

use crate::common_controls_v6::{CCM_GETVERSION, CommonControls6};

const WAIT: Duration = Duration::from_secs(10);
/// Milliseconds a sent message may take before the test fails.
const SEND_TIMEOUT_MS: u32 = 10_000;

const ID_EDIT: u16 = 101;
const ID_MULTI: u16 = 102;
const ID_LIST: u16 = 103;
const ID_PLAIN_LIST: u16 = 104;
const ID_BUTTON: u16 = 105;
/// An id that names no control.
const ID_NONE: u16 = 999;

/// The private message the handler posts to its own dialog.
const STARTED: u32 = 1;
/// Marks the end of what a test waits for (see `Running::settle`).
const BARRIER: u32 = 2;

const COLUMNS: [(&str, i16); 3] = [("Gõ tắt", 60), ("Thay bằng", 120), ("Trạng thái", 50)];

fn app(offset: u32) -> AppMessage {
    AppMessage::new(offset).unwrap()
}

/// What the test handler saw, sent from the dialog's thread.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Init {
        hwnd: isize,
    },
    /// A private message arrived, while the dialog was (not) visible.
    App {
        offset: u32,
        wparam: usize,
        visible: bool,
    },
    Notify {
        id: u16,
        code: i32,
    },
    /// A text the handler read.
    Text(String),
    /// The list view's selection, read by the handler after `LVN_ITEMCHANGED`.
    Selected(Option<usize>),
    /// What a helper returned.
    Result(&'static str, Result<(), WinErrorKind>),
    /// What `Dialog::post` returned.
    Posted(&'static str, bool),
    /// How long a helper took.
    Elapsed(Duration),
}

type Action = Box<dyn FnMut(&Dialog<'_>, &Sender<Seen>) + Send>;

/// The test handler: reports what it sees, runs `on_init` at `init` and `on_click` when the
/// push button is clicked.
struct Driver {
    seen: Sender<Seen>,
    on_init: Option<Action>,
    on_click: Option<Action>,
}

fn report(seen: &Sender<Seen>, what: &'static str, result: Result<(), WinErrorKind>) {
    let _ = seen.send(Seen::Result(what, result));
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

fn is_visible(hwnd: HWND) -> bool {
    // SAFETY: only reads the window state.
    unsafe { IsWindowVisible(hwnd) }.as_bool()
}

impl DialogHandler for Driver {
    fn init(&mut self, dialog: &Dialog) {
        move_off_screen(dialog.hwnd());
        let _ = self.seen.send(Seen::Init {
            hwnd: dialog.hwnd().0 as isize,
        });
        report(
            &self.seen,
            "list_set_columns",
            dialog
                .list_set_columns(ID_LIST, &COLUMNS)
                .map_err(|error| error.kind()),
        );
        if let Some(action) = &mut self.on_init {
            action(dialog, &self.seen);
        }
    }

    fn command(&mut self, dialog: &Dialog, id: u16, code: u16) {
        if (id, code) == (ID_BUTTON, notifications::BN_CLICKED)
            && let Some(action) = &mut self.on_click
        {
            action(dialog, &self.seen);
        }
    }

    fn notify(&mut self, dialog: &Dialog, id: u16, code: i32) {
        let _ = self.seen.send(Seen::Notify { id, code });
        if (id, code) == (ID_LIST, notifications::LVN_ITEMCHANGED) {
            let _ = self
                .seen
                .send(Seen::Selected(dialog.list_selected(ID_LIST)));
        }
    }

    fn app_message(&mut self, dialog: &Dialog, message: AppMessage, wparam: usize, _: isize) {
        let _ = self.seen.send(Seen::App {
            offset: message.offset(),
            wparam,
            visible: is_visible(dialog.hwnd()),
        });
    }
}

fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
    DialogRect::new(x, y, cx, cy)
}

/// The test dialog: never activated, no taskbar button, at an explicit position.
fn template() -> TemplateBytes {
    DialogTemplate::new("Raumakey thử nghiệm", 260, 200)
        .at(-30_000, -30_000)
        .with_ex_style(styles::WS_EX_NOACTIVATE)
        .with_control(Control::virtual_list_view(ID_LIST, rect(7, 7, 246, 80)))
        .with_control(Control::edit(ID_EDIT, rect(7, 92, 100, 12)))
        .with_control(Control::multiline_edit(ID_MULTI, rect(7, 108, 246, 40)))
        .with_control(Control::list_view(ID_PLAIN_LIST, rect(7, 152, 120, 20)))
        .with_control(Control::push_button(
            ID_BUTTON,
            "Đổi",
            rect(140, 178, 50, 14),
        ))
        .with_control(Control::push_button(
            IDCANCEL,
            "Đóng",
            rect(203, 178, 50, 14),
        ))
        .build()
        .unwrap()
}

fn next(seen: &Receiver<Seen>) -> Seen {
    seen.recv_timeout(WAIT)
        .expect("the dialog's thread reported in time")
}

/// Sends `message` to a window of the dialog's thread and returns the answer; fails the test
/// when the thread does not answer in time.
fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    let mut result = 0_usize;
    // SAFETY: the pointers passed in `lparam` by the callers point to data of this thread that
    // outlives the call (the receiver is in this process); `result` is a valid out pointer.
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

/// The text of a cell, as the list view copies it into a buffer of 128 units
/// (`LVM_GETITEMTEXTW`): a longer cell is cut to 127. The answer is not the length copied:
/// Common Controls 6 answers the length of the whole cell (and also hands over the text lent
/// to it, which this never reads), so the copy ends at its NUL.
fn list_cell(list: HWND, row: usize, column: i32) -> String {
    let mut buffer = [0_u16; 128];
    let mut cell = LVITEMW {
        iSubItem: column,
        pszText: PWSTR(buffer.as_mut_ptr()),
        cchTextMax: buffer.len() as i32,
        ..Default::default()
    };
    send(list, LVM_GETITEMTEXTW, row, &raw mut cell as isize);
    String::from_utf16(until_nul(&buffer)).unwrap()
}

/// The selected row of a list view, or -1 (`LVM_GETNEXTITEM`).
fn selection(list: HWND) -> isize {
    send(list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize)
}

/// Types `typed` into a window of the dialog's thread as the keyboard would (`WM_CHAR`).
fn type_into(window: HWND, typed: &str) {
    for unit in typed.encode_utf16() {
        send(window, WM_CHAR, usize::from(unit), 1);
    }
}

/// Selects row `row` of a list view the way a click does, so that it notifies the dialog.
fn select_row(list: HWND, row: usize) {
    let mut state = LVITEMW {
        state: LVIS_SELECTED,
        stateMask: LVIS_SELECTED,
        ..Default::default()
    };
    send(list, LVM_SETITEMSTATE, row, &raw mut state as isize);
}

fn rows(cells: &[&[&str]]) -> Vec<Vec<String>> {
    cells
        .iter()
        .map(|row| row.iter().map(|cell| (*cell).to_owned()).collect())
        .collect()
}

/// Three rows, the second without its third cell.
fn three_rows() -> Vec<Vec<String>> {
    rows(&[
        &["vn", "Việt Nam", ""],
        &["hcm", "Thành phố Hồ Chí Minh"],
        &["ko", "không", "đang tắt"],
    ])
}

/// A dialog shown by `spawn_dialog`, with the handle its handler reported.
struct Running {
    dialog: DetachedDialog,
    seen: Receiver<Seen>,
    hwnd: HWND,
}

impl Running {
    /// Everything the handler saw until a barrier posted now.
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

    fn list(&self) -> HWND {
        item(self.hwnd, ID_LIST)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        // A failed test leaves no dialog behind.
        self.dialog.dismiss();
    }
}

fn started(on_init: Option<Action>, on_click: Option<Action>) -> Running {
    let (sender, seen) = mpsc::channel();
    let driver = Driver {
        seen: sender,
        on_init,
        on_click,
    };
    let dialog = spawn_dialog(template(), driver, |_| {}).unwrap();
    let Seen::Init { hwnd } = next(&seen) else {
        panic!("the dialog started with init");
    };
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    // Answered by the dialog's modal loop, once `init` returned and the handle is published.
    send(hwnd, WM_NULL, 0, 0);
    Running { dialog, seen, hwnd }
}

fn results(seen: &[Seen]) -> Vec<(&'static str, Result<(), WinErrorKind>)> {
    seen.iter()
        .filter_map(|seen| match seen {
            Seen::Result(what, result) => Some((*what, *result)),
            _ => None,
        })
        .collect()
}

/// `init` posts a message to its own dialog: it arrives before the dialog is shown (the dialog
/// box shows itself once its queue first runs empty). Reserved messages are refused.
#[test]
fn a_message_the_handler_posts_from_init_arrives_before_the_dialog_is_shown() {
    let running = started(
        Some(Box::new(|dialog, seen| {
            let _ = seen.send(Seen::Posted("own", dialog.post(app(STARTED), 7, -1)));
            let reserved = AppMessage::from_id(RESERVED_FIRST).unwrap();
            let _ = seen.send(Seen::Posted("reserved", dialog.post(reserved, 0, 0)));
        })),
        None,
    );
    let seen = running.settle();
    assert!(seen.contains(&Seen::Posted("own", true)), "{seen:?}");
    assert!(seen.contains(&Seen::Posted("reserved", false)), "{seen:?}");
    assert!(
        seen.contains(&Seen::App {
            offset: STARTED,
            wparam: 7,
            visible: false
        }),
        "{seen:?}"
    );
    // The dialog shows itself afterwards (far off-screen).
    let deadline = Instant::now() + WAIT;
    while !is_visible(running.hwnd) {
        assert!(Instant::now() < deadline, "the dialog was shown");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn edits_take_a_text_limit_and_multi_line_edits_keep_their_line_breaks() {
    let running = started(
        Some(Box::new(|dialog, _| {
            dialog.set_text_limit(ID_EDIT, 64);
            dialog.set_text_limit(ID_MULTI, 16_384);
            // No such control: nothing happens.
            dialog.set_text_limit(ID_NONE, 5);
        })),
        // A multi-line edit does not report EN_CHANGE for WM_SETTEXT: read it on a click.
        Some(Box::new(|dialog, seen| {
            if let Ok(text) = dialog.text(ID_MULTI) {
                let _ = seen.send(Seen::Text(text));
            }
        })),
    );
    assert_eq!(send(item(running.hwnd, ID_EDIT), EM_GETLIMITTEXT, 0, 0), 64);
    let multi = item(running.hwnd, ID_MULTI);
    assert_eq!(send(multi, EM_GETLIMITTEXT, 0, 0), 16_384);
    // SAFETY: only reads the window's style.
    let style = unsafe { GetWindowLongW(multi, GWL_STYLE) } as u32;
    let wanted = styles::ES_MULTILINE | styles::ES_WANTRETURN | styles::ES_AUTOVSCROLL;
    assert_eq!(style & wanted, wanted, "{style:#x}");
    let typed: Vec<u16> = "dòng một\r\ndòng hai".encode_utf16().chain([0]).collect();
    send(multi, WM_SETTEXT, 0, typed.as_ptr() as isize);
    send(item(running.hwnd, ID_BUTTON), BM_CLICK, 0, 0);
    let seen = running.settle();
    assert!(
        seen.contains(&Seen::Text("dòng một\r\ndòng hai".to_owned())),
        "{seen:?}"
    );
}

/// The rows of a virtual list view live in the dialog, which hands their cells to the list
/// view (`LVN_GETDISPINFOW`) without involving the handler; selecting rows works as in any
/// list view.
#[test]
fn a_virtual_list_view_shows_the_rows_the_dialog_keeps() {
    let running = started(
        Some(Box::new(|dialog, seen| {
            report(
                seen,
                "list_set_virtual_rows",
                dialog
                    .list_set_virtual_rows(ID_LIST, &three_rows())
                    .map_err(|error| error.kind()),
            );
            report(
                seen,
                "list_select",
                dialog
                    .list_select(ID_LIST, Some(2))
                    .map_err(|error| error.kind()),
            );
        })),
        None,
    );
    let list = running.list();
    assert_eq!(send(list, LVM_GETITEMCOUNT, 0, 0), 3);
    for (row, cells) in three_rows().iter().enumerate() {
        for column in 0..3 {
            let expected = cells.get(column).map_or("", String::as_str);
            assert_eq!(
                list_cell(list, row, column as i32),
                expected,
                "{row}, {column}"
            );
        }
    }
    assert_eq!(
        send(list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize),
        2
    );
    select_row(list, 0);
    let seen = running.settle();
    assert_eq!(
        results(&seen),
        [
            ("list_set_columns", Ok(())),
            ("list_set_virtual_rows", Ok(())),
            ("list_select", Ok(()))
        ]
    );
    assert!(seen.contains(&Seen::Selected(Some(0))), "{seen:?}");
    // The cells were asked for, but never through the handler.
    assert!(
        !seen.iter().any(|seen| matches!(
            seen,
            Seen::Notify { code, .. } if *code == LVN_GETDISPINFOW as i32
        )),
        "{seen:?}"
    );
}

/// Replacing the rows only stores them and sets the count, so even 20,000 rows take little
/// time; nothing is selected afterwards.
#[test]
fn replacing_the_rows_is_quick_for_20000_rows_and_clears_the_selection() {
    let running = started(
        Some(Box::new(|dialog, seen| {
            report(
                seen,
                "first rows",
                dialog
                    .list_set_virtual_rows(ID_LIST, &three_rows())
                    .map_err(|error| error.kind()),
            );
        })),
        Some(Box::new(|dialog, seen| {
            let many: Vec<Vec<String>> = (0..20_000)
                .map(|row| {
                    vec![
                        format!("t{row}"),
                        format!("Thay thế số {row}"),
                        String::new(),
                    ]
                })
                .collect();
            let start = Instant::now();
            let result = dialog.list_set_virtual_rows(ID_LIST, &many);
            let _ = seen.send(Seen::Elapsed(start.elapsed()));
            report(seen, "many rows", result.map_err(|error| error.kind()));
        })),
    );
    let list = running.list();
    select_row(list, 1);
    send(item(running.hwnd, ID_BUTTON), BM_CLICK, 0, 0);
    let seen = running.settle();
    assert_eq!(
        results(&seen),
        [
            ("list_set_columns", Ok(())),
            ("first rows", Ok(())),
            ("many rows", Ok(()))
        ]
    );
    let elapsed = seen
        .iter()
        .find_map(|seen| match seen {
            Seen::Elapsed(elapsed) => Some(*elapsed),
            _ => None,
        })
        .unwrap();
    eprintln!("list_set_virtual_rows of 20,000 rows took {elapsed:?}");
    assert!(elapsed < Duration::from_secs(2), "{elapsed:?}");
    assert_eq!(send(list, LVM_GETITEMCOUNT, 0, 0), 20_000);
    assert_eq!(list_cell(list, 19_999, 0), "t19999");
    assert_eq!(list_cell(list, 19_999, 1), "Thay thế số 19999");
    assert_eq!(
        send(list, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize),
        -1
    );
}

/// The handler reads a cell of its own list view during a callback: the dialog answers the
/// list view although the handler is busy.
#[test]
fn the_dialog_hands_out_the_cells_while_its_handler_is_busy() {
    let running = started(
        Some(Box::new(|dialog, _| {
            let _ = dialog.list_set_virtual_rows(ID_LIST, &three_rows());
        })),
        Some(Box::new(|dialog, seen| {
            let list = dialog.item(ID_LIST).unwrap();
            let mut buffer = [0_u16; 64];
            let mut cell = LVITEMW {
                iSubItem: 1,
                pszText: PWSTR(buffer.as_mut_ptr()),
                cchTextMax: buffer.len() as i32,
                ..Default::default()
            };
            // SAFETY: the item and its buffer outlive the call, which the list view (a window
            // of this thread) answers before it returns. The answer is not the length copied
            // (see `list_cell`).
            unsafe {
                SendMessageW(
                    list,
                    LVM_GETITEMTEXTW,
                    Some(WPARAM(2)),
                    Some(LPARAM(&raw mut cell as isize)),
                )
            };
            let _ = seen.send(Seen::Text(String::from_utf16_lossy(until_nul(&buffer))));
        })),
    );
    send(item(running.hwnd, ID_BUTTON), BM_CLICK, 0, 0);
    let seen = running.settle();
    assert!(seen.contains(&Seen::Text("không".to_owned())), "{seen:?}");
}

#[test]
fn rows_with_a_nul_or_for_a_list_without_owner_data_change_nothing() {
    let running = started(
        Some(Box::new(|dialog, seen| {
            let attempts: [(&'static str, u16, Vec<Vec<String>>); 4] = [
                ("rows", ID_LIST, three_rows()),
                ("nul", ID_LIST, rows(&[&["a", "b\0c"]])),
                ("plain list", ID_PLAIN_LIST, three_rows()),
                ("no control", ID_NONE, three_rows()),
            ];
            for (what, id, cells) in attempts {
                report(
                    seen,
                    what,
                    dialog
                        .list_set_virtual_rows(id, &cells)
                        .map_err(|error| error.kind()),
                );
            }
        })),
        None,
    );
    let seen = running.settle();
    let results = results(&seen);
    assert_eq!(results[1], ("rows", Ok(())));
    assert_eq!(results[2], ("nul", Err(WinErrorKind::InvalidInput)));
    assert_eq!(results[3], ("plain list", Err(WinErrorKind::InvalidInput)));
    assert!(results[4].1.is_err(), "{results:?}");
    let list = running.list();
    assert_eq!(send(list, LVM_GETITEMCOUNT, 0, 0), 3);
    assert_eq!(list_cell(list, 2, 1), "không");
    assert_eq!(
        send(item(running.hwnd, ID_PLAIN_LIST), LVM_GETITEMCOUNT, 0, 0),
        0
    );
}

/// A request for a cell is answered only when it names the virtual list view by its handle
/// and id; the answer lends the dialog's own text (`pszText` points to it), so nothing is
/// written through a pointer of the request.
#[test]
fn a_display_info_request_is_answered_only_for_the_list_view_it_names() {
    let running = started(
        Some(Box::new(|dialog, _| {
            let _ = dialog.list_set_virtual_rows(ID_LIST, &three_rows());
        })),
        None,
    );
    let list = running.list();
    let mut buffer = [0x55_u16; 8];
    let request =
        |from: HWND, id: u16, row: i32, column: i32, buffer: &mut [u16; 8]| NMLVDISPINFOW {
            hdr: NMHDR {
                hwndFrom: from,
                idFrom: usize::from(id),
                code: LVN_GETDISPINFOW,
            },
            item: LVITEMW {
                mask: LVIF_TEXT,
                iItem: row,
                iSubItem: column,
                pszText: PWSTR(buffer.as_mut_ptr()),
                cchTextMax: buffer.len() as i32,
                ..Default::default()
            },
        };
    for (from, id) in [
        (running.hwnd, ID_LIST),
        (list, ID_EDIT),
        (HWND::default(), ID_LIST),
    ] {
        let mut info = request(from, id, 0, 0, &mut buffer);
        let lent = info.item.pszText;
        send(
            running.hwnd,
            WM_NOTIFY,
            usize::from(id),
            &raw mut info as isize,
        );
        assert_eq!(info.item.pszText, lent, "{id}");
    }
    assert_eq!(buffer, [0x55; 8]);
    let mut info = request(list, ID_LIST, 2, 1, &mut buffer);
    send(
        running.hwnd,
        WM_NOTIFY,
        usize::from(ID_LIST),
        &raw mut info as isize,
    );
    assert_ne!(info.item.pszText.0, buffer.as_mut_ptr());
    assert_eq!(buffer, [0x55; 8]);
    // The lent text is the dialog's, which it may drop as soon as it lent a few more (the list
    // view asks for cells whenever it paints): it is never read here. The list view copies the
    // same cell into a buffer of the caller's.
    assert_eq!(list_cell(list, 2, 1), "không");
}

/// Pages of this process's memory for forged notifications, committed read-write until a test
/// changes their protection, and released when dropped.
struct Pages {
    base: *mut u8,
    page: usize,
    count: usize,
}

impl Pages {
    fn new(count: usize) -> Self {
        let mut info = SYSTEM_INFO::default();
        // SAFETY: a valid out pointer.
        unsafe { GetSystemInfo(&mut info) };
        let page = info.dwPageSize as usize;
        // SAFETY: reserves and commits new pages; no pointers.
        let base =
            unsafe { VirtualAlloc(None, page * count, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE) };
        assert!(!base.is_null(), "VirtualAlloc");
        Self {
            base: base.cast(),
            page,
            count,
        }
    }

    /// The address `offset` bytes into the pages.
    fn at(&self, offset: usize) -> *mut u8 {
        assert!(offset < self.page * self.count);
        self.base.wrapping_add(offset)
    }

    /// Copies the bytes of `value` to `offset` (any alignment) and returns their address; the
    /// pages there must still be writable.
    fn write<T: Copy>(&self, offset: usize, value: &T) -> *mut u8 {
        let at = self.at(offset);
        assert!(offset + size_of::<T>() <= self.page * self.count);
        // SAFETY: the pages are this test's own, committed and writable over the whole range
        // (checked above), and `value` is plain data.
        unsafe {
            core::ptr::copy_nonoverlapping((&raw const *value).cast::<u8>(), at, size_of::<T>())
        };
        at
    }

    /// Reads a `T` from `offset` (any alignment); the pages there must be readable.
    fn read<T: Copy>(&self, offset: usize) -> T {
        assert!(offset + size_of::<T>() <= self.page * self.count);
        // SAFETY: the pages are this test's own and readable over the whole range, and `T` is
        // plain data that was written there.
        unsafe { core::ptr::read_unaligned(self.at(offset).cast::<T>()) }
    }

    fn protect(&self, page: usize, protection: PAGE_PROTECTION_FLAGS) {
        let mut previous = PAGE_PROTECTION_FLAGS::default();
        // SAFETY: changes the protection of one of this test's own pages; a valid out pointer.
        unsafe {
            VirtualProtect(
                self.at(page * self.page).cast::<c_void>(),
                self.page,
                protection,
                &mut previous,
            )
        }
        .unwrap();
    }

    fn protection(&self, page: usize) -> PAGE_PROTECTION_FLAGS {
        let mut info = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: queries this test's own page into a valid out buffer of the size passed.
        let size = unsafe {
            VirtualQuery(
                Some(self.at(page * self.page).cast::<c_void>().cast_const()),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        assert_ne!(size, 0, "VirtualQuery");
        info.Protect
    }
}

impl Drop for Pages {
    fn drop(&mut self) {
        // SAFETY: releases the whole reservation made in `new`; nothing points into it any
        // more (the dialog answered every message sent with it before `send` returned).
        let _ = unsafe { VirtualFree(self.base.cast(), 0, MEM_RELEASE) };
    }
}

/// The dialog writes its answer to a cell request only into committed read-write memory: a
/// request in a page that is read-only, or executable and read-only (which
/// `WriteProcessMemory` would make writable for the write, and which a forged request could
/// otherwise use to patch code), stays as it was, and so does the page's protection.
#[test]
fn a_cell_request_in_memory_that_is_not_writable_stays_as_it_was() {
    let running = started(
        Some(Box::new(|dialog, _| {
            let _ = dialog.list_set_virtual_rows(ID_LIST, &three_rows());
        })),
        None,
    );
    let list = running.list();
    let mut buffer = [0x55_u16; 8];
    let request = NMLVDISPINFOW {
        hdr: NMHDR {
            hwndFrom: list,
            idFrom: usize::from(ID_LIST),
            code: LVN_GETDISPINFOW,
        },
        item: LVITEMW {
            mask: LVIF_TEXT,
            iItem: 2,
            iSubItem: 1,
            pszText: PWSTR(buffer.as_mut_ptr()),
            cchTextMax: buffer.len() as i32,
            ..Default::default()
        },
    };
    for (protection, answered) in [
        (PAGE_READWRITE, true),
        (PAGE_READONLY, false),
        (PAGE_EXECUTE_READ, false),
    ] {
        let pages = Pages::new(1);
        let at = pages.write(0, &request);
        pages.protect(0, protection);
        send(running.hwnd, WM_NOTIFY, usize::from(ID_LIST), at as isize);
        assert_eq!(pages.protection(0), protection, "{protection:?}");
        let after: NMLVDISPINFOW = pages.read(0);
        assert_eq!(
            after.item.pszText.0 != buffer.as_mut_ptr(),
            answered,
            "{protection:?}"
        );
    }
    assert_eq!(buffer, [0x55; 8]);
}

/// Rows whose first cells start alike, to see where a search lands.
fn search_rows() -> Vec<Vec<String>> {
    rows(&[
        &["ab", "một"],
        &["hcm", "Hồ Chí Minh"],
        &["ko", "không"],
        &["vn", "Việt Nam"],
        &["Kx", "khác"],
    ])
}

/// `text` and a NUL.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// A search request (`LVN_ODFINDITEMW`, which the list view sends for `LVM_FINDITEMW` and when
/// the user types into it) for a virtual list view is answered by the dialog, without the
/// handler, from the rows it keeps: a case-insensitive search of the first cells, from row
/// `iStart` on, for a cell equal to the text or, with `LVFI_PARTIAL` (or `LVFI_SUBSTRING`),
/// starting with it, going on from the first row with `LVFI_WRAP`. The request and its text
/// are read without trusting their pointers: a text that cannot be read, or that has no NUL
/// within the bound, finds nothing (-1), as do searches by `lParam` or by position, which the
/// dialog cannot answer. A request for another control reaches the handler and gets the
/// default answer, 0.
#[test]
fn a_search_request_is_answered_from_the_rows_without_trusting_its_pointer() {
    let running = started(
        Some(Box::new(|dialog, _| {
            let _ = dialog.list_set_virtual_rows(ID_LIST, &search_rows());
        })),
        None,
    );
    let list = running.list();
    let search = |from: HWND, id: u16, flags: LVFINDINFOW_FLAGS, start: i32, text: *const u8| {
        let request = NMLVFINDITEMW {
            hdr: NMHDR {
                hwndFrom: from,
                idFrom: usize::from(id),
                code: LVN_ODFINDITEMW,
            },
            iStart: start,
            lvfi: LVFINDINFOW {
                flags,
                psz: PCWSTR(text.cast()),
                ..Default::default()
            },
        };
        send(
            running.hwnd,
            WM_NOTIFY,
            usize::from(id),
            &raw const request as isize,
        )
    };
    let find = |flags: LVFINDINFOW_FLAGS, start: i32, text: &str| {
        let text = wide(text);
        search(list, ID_LIST, flags, start, text.as_ptr().cast())
    };
    let prefix = LVFI_STRING | LVFI_PARTIAL;
    assert_eq!(find(prefix, 0, "K"), 2);
    assert_eq!(find(prefix, 2, "k"), 2);
    assert_eq!(find(prefix, 3, "k"), 4);
    assert_eq!(find(LVFI_PARTIAL, 3, "K"), 4);
    assert_eq!(find(LVFI_SUBSTRING, 0, "Hc"), 1);
    assert_eq!(find(prefix, 0, "việt"), -1);
    // Past the last match: nothing, unless the search wraps.
    assert_eq!(find(prefix, 1, "a"), -1);
    assert_eq!(find(prefix | LVFI_WRAP, 1, "a"), 0);
    assert_eq!(find(prefix | LVFI_WRAP, 3, "z"), -1);
    // Without LVFI_PARTIAL the whole first cell must match (LVFI_WRAP alone is such a search).
    assert_eq!(find(LVFI_STRING, 0, "k"), -1);
    assert_eq!(find(LVFI_STRING, 0, "kX"), 4);
    assert_eq!(find(LVFI_WRAP, 3, "KO"), 2);
    // A negative start searches from the first row; one past the last row (as when the last
    // row has the focus) only when the search wraps.
    assert_eq!(find(prefix, -1, "k"), 2);
    assert_eq!(find(prefix, 5, "k"), -1);
    assert_eq!(find(prefix | LVFI_WRAP, 5, "k"), 2);
    assert_eq!(find(prefix | LVFI_WRAP, i32::MAX, "k"), 2);
    // What Common Controls 6 sends as the user types.
    assert_eq!(find(LVFI_STRING | LVFI_SUBSTRING | LVFI_WRAP, 3, "k"), 4);
    // Searches the dialog cannot answer.
    assert_eq!(find(LVFI_PARAM, 0, "k"), -1);
    assert_eq!(find(LVFI_NEARESTXY | prefix, 0, "k"), -1);
    assert_eq!(find(LVFINDINFOW_FLAGS(0), 0, "k"), -1);

    // Texts that cannot be read, or that never end.
    assert_eq!(search(list, ID_LIST, prefix, 0, core::ptr::null()), -1);
    assert_eq!(search(list, ID_LIST, prefix, 0, 0x10 as *const u8), -1);
    let endless = vec![u16::from(b'k'); 100_000];
    assert_eq!(
        search(list, ID_LIST, prefix, 0, endless.as_ptr().cast()),
        -1
    );
    // Each case ends just before a page that cannot be read.
    let pages = Pages::new(8);
    let page = pages.page;
    let k = u16::from(b'k');
    let at_end = pages.write(page - 4, &[k, 0]);
    let odd = pages.write(3 * page - 7, &[k, u16::from(b'O'), 0]);
    let unended = pages.write(5 * page - 8, &[k; 4]);
    let header = NMHDR {
        hwndFrom: list,
        idFrom: usize::from(ID_LIST),
        code: LVN_ODFINDITEMW,
    };
    let cut = pages.write(
        7 * page - size_of::<NMHDR>(),
        &NMLVFINDITEMW {
            hdr: header,
            iStart: 0,
            lvfi: LVFINDINFOW {
                flags: prefix,
                psz: PCWSTR(at_end.cast()),
                ..Default::default()
            },
        },
    );
    for unreadable in [1, 3, 5, 7] {
        pages.protect(unreadable, PAGE_NOACCESS);
    }
    assert_eq!(search(list, ID_LIST, prefix, 0, at_end), 2);
    assert_eq!(search(list, ID_LIST, LVFI_STRING, 0, odd), 2);
    // The letters run into the page that cannot be read.
    assert_eq!(search(list, ID_LIST, prefix, 0, unended), -1);
    // So does the request after its header.
    assert_eq!(
        send(running.hwnd, WM_NOTIFY, usize::from(ID_LIST), cut as isize),
        -1
    );

    // Another control's request reaches the handler, which cannot answer it.
    let plain = item(running.hwnd, ID_PLAIN_LIST);
    let text = wide("k");
    assert_eq!(
        search(plain, ID_PLAIN_LIST, prefix, 0, text.as_ptr().cast()),
        0
    );
    let seen = running.settle();
    let searches: Vec<u16> = seen
        .iter()
        .filter_map(|seen| match seen {
            Seen::Notify { id, code } if *code == LVN_ODFINDITEMW as i32 => Some(*id),
            _ => None,
        })
        .collect();
    assert_eq!(searches, [ID_PLAIN_LIST]);
}

/// Opens the test dialog under Common Controls 6, as `raumakey.exe` runs it, with `rows` in
/// the virtual list view, row `selected` selected, and the keyboard focus in the list view.
fn started_v6(rows: Vec<Vec<String>>, selected: Option<usize>) -> Running {
    // The dialog's thread inherits the context; the guard may go once it started.
    let _v6 = CommonControls6::activate();
    let running = started(
        Some(Box::new(move |dialog, seen| {
            report(
                seen,
                "rows",
                dialog
                    .list_set_virtual_rows(ID_LIST, &rows)
                    .map_err(|error| error.kind()),
            );
            report(
                seen,
                "select",
                dialog
                    .list_select(ID_LIST, selected)
                    .map_err(|error| error.kind()),
            );
            dialog.focus(ID_LIST);
        })),
        None,
    );
    assert_eq!(
        send(running.list(), CCM_GETVERSION, 0, 0),
        6,
        "Common Controls 6"
    );
    running
}

/// Typing into a virtual list view selects the next row (after the selected one, else from the
/// first) whose first cell starts with the letters typed, in any case, going on from the first
/// row; when no row does, the selection stays. Never the first row merely because the search
/// went unanswered.
#[test]
fn typing_into_a_virtual_list_selects_the_next_row_that_starts_with_the_letters() {
    for (selected, typed, expected) in [
        (None, "k", 2),
        (None, "K", 2),
        (None, "hc", 1),
        (None, "z", -1),
        (Some(2), "k", 4),
        (Some(4), "k", 2),
        (Some(3), "z", 3),
    ] {
        let running = started_v6(search_rows(), selected);
        let list = running.list();
        type_into(list, typed);
        assert_eq!(selection(list), expected, "{selected:?} {typed:?}");
        let seen = running.settle();
        assert_eq!(
            results(&seen),
            [
                ("list_set_columns", Ok(())),
                ("rows", Ok(())),
                ("select", Ok(()))
            ]
        );
        // The dialog answered the search itself.
        assert!(
            !seen.iter().any(|seen| matches!(
                seen,
                Seen::Notify { code, .. } if *code == LVN_ODFINDITEMW as i32
            )),
            "{seen:?}"
        );
    }
}

/// Common Controls 6 answers `LVM_GETITEMTEXTW` with the length of the whole cell, which may
/// exceed the caller's buffer, and copies into the buffer what fits.
#[test]
fn a_long_cell_is_cut_to_the_callers_buffer_under_common_controls_6() {
    let long = "ệ".repeat(300);
    let running = started_v6(vec![vec!["dài".to_owned(), long]], None);
    let list = running.list();
    assert_eq!(list_cell(list, 0, 0), "dài");
    assert_eq!(list_cell(list, 0, 1), "ệ".repeat(127));
}
