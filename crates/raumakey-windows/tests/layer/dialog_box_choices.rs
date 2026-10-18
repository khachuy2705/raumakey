//! `dialog_box`: the helpers that the exclusions dialog of the controller (D48) adds, on real
//! dialog boxes: an editable drop-down combo box (`CBS_DROPDOWN`: the text of its edit, its
//! items and the text of each, a choice from its list, what the user types into it) and the
//! Open dialog owned by the dialog (`Dialog::choose_file`).
//!
//! The dialogs stay out of the way as in `dialog_box`: `WS_EX_NOACTIVATE`, an explicit
//! position, and a move to (-32000, -32000) in `init`, before Windows shows them. They are
//! driven from the test thread with `SendMessageTimeoutW`, and every wait has a timeout. No
//! test shows an Open dialog: the ones that ask for one ask after a dismiss, or with a
//! `WH_CBT` hook of the dialog's thread that refuses to create any window; the one that shows
//! it is `#[ignore]`d and manual.

use core::cell::Cell;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use raumakey_windows::dialog_box::{
    DetachedDialog, Dialog, DialogHandler, notifications, spawn_dialog,
};
use raumakey_windows::dialog_template::{
    Control, DialogRect, DialogTemplate, IDCANCEL, TemplateBytes, styles,
};
use raumakey_windows::error::WinErrorKind;
use raumakey_windows::file_dialog::FileFilter;
use raumakey_windows::panic::guard_value;
use raumakey_windows::window::AppMessage;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_CLICK, CB_GETCOUNT, CB_SETCURSEL, CallNextHookEx, GW_CHILD, GetDlgItem, GetWindow,
    HCBT_CREATEWND, HHOOK, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
    SendMessageTimeoutW, SetWindowPos, SetWindowsHookExW, UnhookWindowsHookEx, WH_CBT, WM_CHAR,
    WM_COMMAND, WM_GETTEXT, WM_GETTEXTLENGTH, WM_NULL,
};

const WAIT: Duration = Duration::from_secs(10);
/// Milliseconds a sent message may take before the test fails.
const SEND_TIMEOUT_MS: u32 = 10_000;

const ID_COMBO: u16 = 101;
const ID_BUTTON: u16 = 102;

/// Marks the end of what a test waits for (see `Running::settle`).
const BARRIER: u32 = 2;

const ITEMS: [&str; 2] = ["mstsc.exe – Remote Desktop Connection", "notepad.exe"];

/// The filter of the Open dialog of the tests.
const PROGRAMS: [FileFilter<'static>; 1] = [FileFilter {
    label: "Chương trình (*.exe)",
    patterns: "*.exe",
}];

fn app(offset: u32) -> AppMessage {
    AppMessage::new(offset).unwrap()
}

/// What the test handler saw, sent from the dialog's thread.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Init {
        hwnd: isize,
    },
    App(u32),
    Command {
        id: u16,
        code: u16,
    },
    /// The text of the combo box's edit, as the handler read it.
    Text(String),
    /// The selected item, as the handler read it.
    Selected(Option<usize>),
    /// The text of an item, or why it could not be read.
    Item(usize, Result<String, WinErrorKind>),
    /// What `Dialog::choose_file` returned, and how many windows a veto refused meanwhile.
    Chosen(Result<Option<PathBuf>, WinErrorKind>, u32),
}

type Action = Box<dyn FnMut(&Dialog<'_>, &Sender<Seen>) + Send>;

/// The test handler: reports what it sees, runs `on_init` at `init` and `on_click` when the
/// push button is clicked.
struct Driver {
    seen: Sender<Seen>,
    on_init: Option<Action>,
    on_click: Option<Action>,
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

impl DialogHandler for Driver {
    fn init(&mut self, dialog: &Dialog) {
        move_off_screen(dialog.hwnd());
        let _ = self.seen.send(Seen::Init {
            hwnd: dialog.hwnd().0 as isize,
        });
        if let Some(action) = &mut self.on_init {
            action(dialog, &self.seen);
        }
    }

    fn command(&mut self, dialog: &Dialog, id: u16, code: u16) {
        let _ = self.seen.send(Seen::Command { id, code });
        match (id, code) {
            (ID_BUTTON, notifications::BN_CLICKED) => {
                if let Some(action) = &mut self.on_click {
                    action(dialog, &self.seen);
                }
            }
            (ID_COMBO, notifications::CBN_SELCHANGE) => {
                let selected = dialog.combo_selected(ID_COMBO);
                let _ = self.seen.send(Seen::Selected(selected));
                if let Some(index) = selected {
                    let item = dialog
                        .combo_item_text(ID_COMBO, index)
                        .map_err(|error| error.kind());
                    let _ = self.seen.send(Seen::Item(index, item));
                }
            }
            (ID_COMBO, notifications::CBN_EDITCHANGE) => {
                let text = dialog.text(ID_COMBO).unwrap_or_default();
                let _ = self.seen.send(Seen::Text(text));
            }
            _ => {}
        }
    }

    fn app_message(&mut self, _dialog: &Dialog, message: AppMessage, _: usize, _: isize) {
        let _ = self.seen.send(Seen::App(message.offset()));
    }
}

fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
    DialogRect::new(x, y, cx, cy)
}

/// The test dialog: never activated, no taskbar button, at an explicit position.
fn template() -> TemplateBytes {
    DialogTemplate::new("Raumakey thử nghiệm", 220, 80)
        .at(-30_000, -30_000)
        .with_ex_style(styles::WS_EX_NOACTIVATE)
        .with_control(Control::drop_down_combo(ID_COMBO, rect(7, 7, 206, 12)))
        .with_control(Control::push_button(ID_BUTTON, "Đổi", rect(7, 59, 50, 14)))
        .with_control(Control::push_button(
            IDCANCEL,
            "Đóng",
            rect(163, 59, 50, 14),
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

fn window_text(hwnd: HWND) -> String {
    let length = usize::try_from(send(hwnd, WM_GETTEXTLENGTH, 0, 0)).unwrap();
    let mut buffer = vec![0_u16; length + 1];
    let copied = send(hwnd, WM_GETTEXT, buffer.len(), buffer.as_mut_ptr() as isize);
    String::from_utf16(&buffer[..usize::try_from(copied).unwrap()]).unwrap()
}

/// The edit of the combo box: its only child.
fn combo_edit(combo: HWND) -> HWND {
    // SAFETY: only reads the window list.
    unsafe { GetWindow(combo, GW_CHILD) }.unwrap()
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
                Seen::App(BARRIER) => return seen,
                other => seen.push(other),
            }
        }
    }

    fn combo(&self) -> HWND {
        item(self.hwnd, ID_COMBO)
    }

    fn click(&self) {
        send(item(self.hwnd, ID_BUTTON), BM_CLICK, 0, 0);
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

/// The edit of a `CBS_DROPDOWN` combo box holds the text the handler sets and reads, whatever
/// its items; each item's text is read by its index (one past the last fails); a choice from
/// the list names its item; what the user types reaches the handler as `CBN_EDITCHANGE`; and
/// replacing the items empties the edit (`CB_RESETCONTENT`).
#[test]
fn an_editable_combo_box_holds_typed_text_and_its_items() {
    let running = started(
        Some(Box::new(|dialog, seen| {
            dialog.combo_set_items(ID_COMBO, &ITEMS).unwrap();
            dialog.set_text(ID_COMBO, "putty").unwrap();
            let _ = seen.send(Seen::Text(dialog.text(ID_COMBO).unwrap()));
        })),
        Some(Box::new(|dialog, seen| {
            for index in [0, 1, 2, usize::MAX] {
                let item = dialog
                    .combo_item_text(ID_COMBO, index)
                    .map_err(|error| error.kind());
                let _ = seen.send(Seen::Item(index, item));
            }
            dialog.combo_set_items(ID_COMBO, &["notepad.exe"]).unwrap();
            let _ = seen.send(Seen::Text(dialog.text(ID_COMBO).unwrap()));
            let _ = seen.send(Seen::Selected(dialog.combo_selected(ID_COMBO)));
        })),
    );
    let combo = running.combo();
    let seen = running.settle();
    // The focus notifications of the combo box reach the handler too; its own changes do not.
    assert!(seen.contains(&Seen::Text("putty".to_owned())), "{seen:?}");
    assert!(
        !seen.contains(&Seen::Command {
            id: ID_COMBO,
            code: notifications::CBN_EDITCHANGE
        }),
        "{seen:?}"
    );
    assert_eq!(window_text(combo), "putty");
    assert_eq!(send(combo, CB_GETCOUNT, 0, 0), 2);

    // A choice from the list, as the user makes it: the selection, then CBN_SELCHANGE.
    send(combo, CB_SETCURSEL, 1, 0);
    let wparam = usize::from(ID_COMBO) | (usize::from(notifications::CBN_SELCHANGE) << 16);
    send(running.hwnd, WM_COMMAND, wparam, combo.0 as isize);
    let seen = running.settle();
    assert!(seen.contains(&Seen::Selected(Some(1))), "{seen:?}");
    assert!(
        seen.contains(&Seen::Item(1, Ok("notepad.exe".to_owned()))),
        "{seen:?}"
    );

    // The user types into the edit.
    send(combo, CB_SETCURSEL, usize::MAX, 0);
    for unit in "x".encode_utf16() {
        send(combo_edit(combo), WM_CHAR, usize::from(unit), 1);
    }
    let seen = running.settle();
    let typed = window_text(combo);
    assert!(typed.ends_with('x'), "{typed:?}");
    assert!(
        seen.contains(&Seen::Command {
            id: ID_COMBO,
            code: notifications::CBN_EDITCHANGE
        }),
        "{seen:?}"
    );
    assert!(seen.contains(&Seen::Text(typed)), "{seen:?}");

    running.click();
    let seen = running.settle();
    let items: Vec<&Seen> = seen
        .iter()
        .filter(|seen| matches!(seen, Seen::Item(..)))
        .collect();
    assert_eq!(
        items,
        [
            &Seen::Item(0, Ok(ITEMS[0].to_owned())),
            &Seen::Item(1, Ok(ITEMS[1].to_owned())),
            &Seen::Item(2, Err(WinErrorKind::InvalidInput)),
            &Seen::Item(usize::MAX, Err(WinErrorKind::InvalidInput)),
        ]
    );
    assert!(seen.contains(&Seen::Text(String::new())), "{seen:?}");
    assert!(seen.contains(&Seen::Selected(None)), "{seen:?}");
    assert_eq!(window_text(combo), "");
    assert_eq!(send(combo, CB_GETCOUNT, 0, 0), 1);
}

thread_local! {
    /// The windows that a `WindowVeto` of this thread refused to create.
    static VETOED: Cell<u32> = const { Cell::new(0) };
}

/// A `WH_CBT` hook of the calling thread only (no other thread is hooked) that refuses to
/// create any window, so that an Open dialog fails, and is counted, instead of appearing.
/// Dropping it removes the hook.
struct WindowVeto(HHOOK);

impl WindowVeto {
    fn install() -> Self {
        VETOED.with(|vetoed| vetoed.set(0));
        // SAFETY: GetCurrentThreadId has no preconditions.
        let thread = unsafe { GetCurrentThreadId() };
        // SAFETY: a hook procedure of this process for the calling thread only, so no module
        // handle is needed.
        let hook = unsafe { SetWindowsHookExW(WH_CBT, Some(veto_creation), None, thread) };
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

/// `Dialog::choose_file` with a veto on the dialog's thread: `choice` reports the result and
/// the windows refused.
fn choose_under_veto(dialog: &Dialog<'_>, seen: &Sender<Seen>) {
    let veto = WindowVeto::install();
    let chosen = dialog.choose_file(&PROGRAMS).map_err(|error| error.kind());
    let vetoed = WindowVeto::vetoed();
    drop(veto);
    let _ = seen.send(Seen::Chosen(chosen, vetoed));
}

/// The Open dialog asked in `init` of a dialog that was dismissed before its handle was
/// published fails with `E_ABORT` without creating a window, as a message box does.
#[test]
fn the_open_dialog_of_a_dismissed_dialog_fails_without_showing() {
    let (started_sender, has_started) = mpsc::channel();
    let (open, gate) = mpsc::channel::<()>();
    let (sender, seen) = mpsc::channel();
    let (close_sender, closed) = mpsc::channel();
    let gate = std::sync::Mutex::new(gate);
    let driver = Driver {
        seen: sender,
        on_init: Some(Box::new(move |dialog, seen| {
            let _ = started_sender.send(());
            let _ = gate.lock().unwrap().recv_timeout(WAIT);
            choose_under_veto(dialog, seen);
        })),
        on_click: None,
    };
    let dialog = spawn_dialog(template(), driver, move |result| {
        let _ = close_sender.send(result);
    })
    .unwrap();
    assert!(matches!(next(&seen), Seen::Init { .. }));
    has_started.recv_timeout(WAIT).expect("init started");
    dialog.dismiss();
    let _ = open.send(());
    assert_eq!(next(&seen), Seen::Chosen(Err(WinErrorKind::Cancelled), 0));
    assert_eq!(closed.recv_timeout(WAIT).expect("on_close ran"), None);
    assert!(dialog.is_closed());
}

/// An Open dialog that cannot be created (every window of the dialog's thread is refused) is
/// an error, not a choice the user cancelled; nothing was shown, and the dialog goes on.
#[test]
fn an_open_dialog_that_cannot_be_created_fails() {
    let running = started(None, Some(Box::new(choose_under_veto)));
    running.click();
    let seen = running.settle();
    let chosen = seen
        .iter()
        .find_map(|seen| match seen {
            Seen::Chosen(result, vetoed) => Some((result.clone(), *vetoed)),
            _ => None,
        })
        .expect("the handler asked");
    assert!(chosen.0.is_err(), "{chosen:?}");
    assert!(chosen.1 > 0, "the dialog's window was refused: {chosen:?}");
    // The dialog still answers.
    send(running.hwnd, WM_NULL, 0, 0);
}

/// Shows a dialog on the screen whose button opens the Open dialog for programs; the path
/// chosen is printed. Close the dialog to end. Manual test.
#[test]
#[ignore = "manual: shows a dialog and the Open dialog until you close them"]
fn manual_the_open_dialog_returns_the_chosen_program() {
    struct Shown(Sender<Option<PathBuf>>);
    impl DialogHandler for Shown {
        fn command(&mut self, dialog: &Dialog, id: u16, code: u16) {
            if (id, code) == (ID_BUTTON, notifications::BN_CLICKED) {
                let chosen = dialog.choose_file(&PROGRAMS);
                eprintln!("chosen: {chosen:?}");
                let _ = self.0.send(chosen.ok().flatten());
            }
        }
    }
    let template = DialogTemplate::new("Raumakey – Duyệt (thủ công)", 220, 80)
        .with_control(Control::drop_down_combo(ID_COMBO, rect(7, 7, 206, 12)))
        .with_control(Control::default_push_button(
            ID_BUTTON,
            "Duyệt…",
            rect(7, 59, 50, 14),
        ))
        .with_control(Control::push_button(
            IDCANCEL,
            "Đóng",
            rect(163, 59, 50, 14),
        ))
        .build()
        .unwrap();
    let (sender, chosen) = mpsc::channel();
    let (closed_sender, closed) = mpsc::channel();
    let _dialog = spawn_dialog(template, Shown(sender), move |_| {
        let _ = closed_sender.send(());
    })
    .unwrap();
    closed.recv().unwrap();
    for path in chosen.try_iter() {
        eprintln!("{path:?}");
    }
}
