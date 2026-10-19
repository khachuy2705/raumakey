//! `dialog_box`: the window picker that the exclusions dialog of the controller uses (D55). A
//! picture control registered with `Dialog::enable_window_picker` starts a drag when the mouse
//! button goes down on it; while the button is held the handler learns the process of the
//! top-level window under the pointer (`WindowPick::Over`, once per window, never the picking
//! dialog itself), the release ends the drag with that process (`WindowPick::Dropped`), and a
//! lost capture cancels it (`WindowPick::Cancelled`).
//!
//! The dialog and the target window stay out of the way: both lie far off every monitor and
//! are never activated. The dialog has a place of its own (the dialogs of the other tests lie
//! at -32000), so that the pointer over it finds it and no other. The mouse messages are sent from the test thread with coordinates of
//! the test's choosing, so the pointer of the desktop never moves.

use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;

use raumakey_windows::dialog_box::{
    DetachedDialog, Dialog, DialogHandler, WindowPick, spawn_dialog,
};
use raumakey_windows::dialog_template::{
    Control, DialogRect, DialogTemplate, IDCANCEL, TemplateBytes, styles,
};
use raumakey_windows::window::AppMessage;
use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::ScreenToClient;
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, GetDlgItem, GetWindowRect, SMTO_NORMAL, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SendMessageTimeoutW, SetWindowPos, WM_CAPTURECHANGED, WM_LBUTTONDOWN,
    WM_LBUTTONUP, WM_MOUSEMOVE, WM_NULL, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
};
use windows::core::w;

const WAIT: Duration = Duration::from_secs(10);
const SEND_TIMEOUT_MS: u32 = 10_000;

const ID_PICKER: u16 = 101;
const ID_LABEL: u16 = 102;
/// Marks the end of what a test waits for.
const BARRIER: u32 = 2;
/// `MK_LBUTTON`.
const MK_LBUTTON: usize = 0x0001;
/// Where each test puts its dialog and its target window: far off every monitor, apart from
/// each other and from the dialogs of the other tests (at -32000), as the tests run at once.
const PLACES: [((i32, i32), (i32, i32)); 3] = [
    ((-26_000, -26_000), (-20_000, -20_000)),
    ((-24_000, -24_000), (-18_000, -18_000)),
    ((-22_000, -22_000), (-16_000, -16_000)),
];

#[derive(Clone, Debug, PartialEq, Eq)]
enum Seen {
    Init { hwnd: isize },
    App(u32),
    Pick { id: u16, pick: WindowPick },
}

struct Driver {
    seen: Sender<Seen>,
    /// Where the dialog goes.
    at: (i32, i32),
    /// Release the mouse capture while handling the start of a drag, as a message box shown
    /// by the handler takes it.
    release_on_start: bool,
}

impl DialogHandler for Driver {
    fn init(&mut self, dialog: &Dialog) {
        // SAFETY: a window of the calling thread; no pointers.
        let _ = unsafe {
            SetWindowPos(
                dialog.hwnd(),
                None,
                self.at.0,
                self.at.1,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        dialog.enable_window_picker(ID_PICKER).unwrap();
        let _ = self.seen.send(Seen::Init {
            hwnd: dialog.hwnd().0 as isize,
        });
    }

    fn window_pick(&mut self, _dialog: &Dialog, id: u16, pick: WindowPick) {
        let _ = self.seen.send(Seen::Pick { id, pick });
        if self.release_on_start && pick == WindowPick::Started {
            // SAFETY: no preconditions; the capture is this thread's.
            let _ = unsafe { ReleaseCapture() };
        }
    }

    fn app_message(&mut self, _dialog: &Dialog, message: AppMessage, _: usize, _: isize) {
        let _ = self.seen.send(Seen::App(message.offset()));
    }
}

fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
    DialogRect::new(x, y, cx, cy)
}

fn template() -> TemplateBytes {
    DialogTemplate::new("Raumakey thử nghiệm", 160, 60)
        .at(-30_000, -30_000)
        .with_ex_style(styles::WS_EX_NOACTIVATE)
        .with_control(Control::window_picker(ID_PICKER, rect(7, 7, 20, 20)))
        .with_control(Control::label(
            ID_LABEL,
            "Kéo vào một cửa sổ",
            rect(32, 12, 120, 8),
        ))
        .with_control(Control::push_button(
            IDCANCEL,
            "Đóng",
            rect(103, 39, 50, 14),
        ))
        .build()
        .unwrap()
}

fn next(seen: &Receiver<Seen>) -> Seen {
    seen.recv_timeout(WAIT)
        .expect("the dialog's thread reported in time")
}

fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    let mut result = 0_usize;
    // SAFETY: no pointers are passed; `result` is a valid out pointer.
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

struct Running {
    dialog: DetachedDialog,
    seen: Receiver<Seen>,
    hwnd: HWND,
}

impl Running {
    fn settle(&self) -> Vec<Seen> {
        assert!(
            self.dialog.post(AppMessage::new(BARRIER).unwrap(), 0, 0),
            "the dialog is up"
        );
        let mut seen = Vec::new();
        loop {
            match next(&self.seen) {
                Seen::App(BARRIER) => return seen,
                other => seen.push(other),
            }
        }
    }

    /// `screen` in the dialog's client coordinates, as a mouse message's `lParam`.
    fn at(&self, screen: (i32, i32)) -> isize {
        let mut point = POINT {
            x: screen.0,
            y: screen.1,
        };
        // SAFETY: `point` is a valid in/out pointer; the window is the dialog's.
        assert!(unsafe { ScreenToClient(self.hwnd, &mut point) }.as_bool());
        let x = (point.x as i16 as u16) as isize;
        let y = (point.y as i16 as u16) as isize;
        x | (y << 16)
    }

    /// The center of the picture control, on the screen.
    fn picker_center(&self) -> (i32, i32) {
        // SAFETY: reads the child list and the rectangle of a window.
        let picker = unsafe { GetDlgItem(Some(self.hwnd), i32::from(ID_PICKER)) }.unwrap();
        let mut frame = RECT::default();
        // SAFETY: `frame` is a valid out pointer.
        unsafe { GetWindowRect(picker, &mut frame) }.unwrap();
        (
            (frame.left + frame.right) / 2,
            (frame.top + frame.bottom) / 2,
        )
    }

    fn mouse(&self, message: u32, screen: (i32, i32)) {
        let keys = if message == WM_LBUTTONUP {
            0
        } else {
            MK_LBUTTON
        };
        send(self.hwnd, message, keys, self.at(screen));
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.dialog.dismiss();
    }
}

fn started(at: (i32, i32)) -> Running {
    started_with(at, false)
}

fn started_with(at: (i32, i32), release_on_start: bool) -> Running {
    let (sender, seen) = mpsc::channel();
    let driver = Driver {
        seen: sender,
        at,
        release_on_start,
    };
    let dialog = spawn_dialog(template(), driver, |_| {}).unwrap();
    let Seen::Init { hwnd } = next(&seen) else {
        panic!("the dialog started with init");
    };
    let hwnd = HWND(hwnd as *mut core::ffi::c_void);
    send(hwnd, WM_NULL, 0, 0);
    Running { dialog, seen, hwnd }
}

/// A plain top-level window of this process, visible but off every monitor and never
/// activated; a button, so that hit testing finds it (a static control is transparent).
struct Target(HWND, (i32, i32));

impl Target {
    fn new(at: (i32, i32)) -> Self {
        // SAFETY: creates a window of a system class with no extra data; it is destroyed on drop.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                w!("BUTTON"),
                w!("Raumakey target"),
                WS_POPUP | WS_VISIBLE,
                at.0,
                at.1,
                200,
                200,
                None,
                None,
                None,
                None,
            )
        }
        .unwrap();
        Self(hwnd, at)
    }

    fn inside(&self) -> (i32, i32) {
        (self.1.0 + 100, self.1.1 + 100)
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        // SAFETY: a window this thread created.
        let _ = unsafe { DestroyWindow(self.0) };
    }
}

fn pick(pick: WindowPick) -> Seen {
    Seen::Pick {
        id: ID_PICKER,
        pick,
    }
}

#[test]
fn a_drag_from_the_picker_reports_the_process_under_the_pointer_and_where_it_dropped() {
    let (dialog_at, target_at) = PLACES[0];
    let target = Target::new(target_at);
    let running = started(dialog_at);
    // SAFETY: no preconditions.
    let own = unsafe { GetCurrentProcessId() };
    let picker = running.picker_center();

    running.mouse(WM_LBUTTONDOWN, picker);
    // Over the target: its process, once while the pointer stays over it.
    running.mouse(WM_MOUSEMOVE, target.inside());
    running.mouse(WM_MOUSEMOVE, (target.inside().0 + 5, target.inside().1));
    // Over the picking dialog itself: no window.
    running.mouse(WM_MOUSEMOVE, picker);
    running.mouse(WM_MOUSEMOVE, target.inside());
    running.mouse(WM_LBUTTONUP, target.inside());
    // After the drop the pointer reports nothing.
    running.mouse(WM_MOUSEMOVE, picker);
    assert_eq!(
        running.settle(),
        [
            pick(WindowPick::Started),
            pick(WindowPick::Over(Some(own))),
            pick(WindowPick::Over(None)),
            pick(WindowPick::Over(Some(own))),
            pick(WindowPick::Dropped(Some(own))),
        ]
    );

    // A release over the dialog drops nothing.
    running.mouse(WM_LBUTTONDOWN, picker);
    running.mouse(WM_LBUTTONUP, picker);
    assert_eq!(
        running.settle(),
        [pick(WindowPick::Started), pick(WindowPick::Dropped(None))]
    );
}

#[test]
fn a_lost_capture_cancels_the_drag_and_a_press_elsewhere_starts_none() {
    let (dialog_at, target_at) = PLACES[1];
    let target = Target::new(target_at);
    let running = started(dialog_at);
    let picker = running.picker_center();

    running.mouse(WM_LBUTTONDOWN, picker);
    running.mouse(WM_MOUSEMOVE, target.inside());
    send(running.hwnd, WM_CAPTURECHANGED, 0, 0);
    // The drag is over: neither a move nor a release reports anything.
    running.mouse(WM_MOUSEMOVE, picker);
    running.mouse(WM_LBUTTONUP, target.inside());
    let seen = running.settle();
    assert_eq!(seen.first(), Some(&pick(WindowPick::Started)));
    assert_eq!(seen.last(), Some(&pick(WindowPick::Cancelled)));
    assert_eq!(seen.len(), 3, "{seen:?}");

    // The dialog still holds the capture (the cancel above was sent, not caused by a release):
    // capturing again at the next press, which tells the dialog itself, cancels nothing.
    running.mouse(WM_LBUTTONDOWN, picker);
    running.mouse(WM_MOUSEMOVE, target.inside());
    running.mouse(WM_LBUTTONUP, target.inside());
    let seen = running.settle();
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert_eq!(seen.first(), Some(&pick(WindowPick::Started)));
    assert!(
        matches!(
            seen.last(),
            Some(Seen::Pick {
                pick: WindowPick::Dropped(Some(_)),
                ..
            })
        ),
        "{seen:?}"
    );

    // A press beside the picker (on the label) starts no drag.
    // SAFETY: reads the child list and the rectangle of a window.
    let label = unsafe { GetDlgItem(Some(running.hwnd), i32::from(ID_LABEL)) }.unwrap();
    let mut frame = RECT::default();
    // SAFETY: `frame` is a valid out pointer.
    unsafe { GetWindowRect(label, &mut frame) }.unwrap();
    running.mouse(WM_LBUTTONDOWN, (frame.left + 2, frame.top + 2));
    running.mouse(WM_MOUSEMOVE, target.inside());
    running.mouse(WM_LBUTTONUP, target.inside());
    assert_eq!(running.settle(), []);
}

/// The picture control starts with the cross of the pointer as its image, and only a
/// registered picture control picks.
#[test]
fn only_a_registered_picture_control_picks() {
    let template = DialogTemplate::new("Raumakey thử nghiệm", 160, 60)
        .with_control(Control::window_picker(ID_PICKER, rect(7, 7, 20, 20)));
    let control = &template.controls()[0];
    assert_eq!(
        control.style() & 0x0F,
        styles::SS_ICON,
        "a picture of an icon"
    );
    assert_eq!(
        control.style() & styles::SS_NOTIFY,
        0,
        "transparent to the mouse, so that the dialog gets the press"
    );
    assert_ne!(
        control.style() & styles::SS_CENTERIMAGE,
        0,
        "keeps its size"
    );
}

/// A capture lost while the handler is busy (a message box it shows takes the capture) still
/// ends the drag: the cancel arrives once the handler is free, and nothing after it is
/// reported.
#[test]
fn a_capture_lost_while_the_handler_is_busy_cancels_the_drag() {
    let (dialog_at, target_at) = PLACES[2];
    let target = Target::new(target_at);
    let running = started_with(dialog_at, true);
    let picker = running.picker_center();

    running.mouse(WM_LBUTTONDOWN, picker);
    running.mouse(WM_MOUSEMOVE, target.inside());
    running.mouse(WM_LBUTTONUP, target.inside());
    assert_eq!(
        running.settle(),
        [pick(WindowPick::Started), pick(WindowPick::Cancelled)]
    );
}
