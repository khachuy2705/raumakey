//! The tray window glue (`desktop::create_tray_window`, `desktop::TrayWindow`) on a real
//! hidden window and message loop, with a fake shell in place of the notification icon and
//! the popup menu, so nothing is shown.
//!
//! Each test runs on a fresh thread, which owns the window and destroys it before it ends.
//! Messages are posted or sent to the test window only: `TaskbarCreated` is never broadcast.
//! A watchdog ends the message loop with [`WATCHDOG`] if the code under test never does.

#![cfg(windows)]

use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use raumakey_controller::desktop::{ModeIcons, TrayShell, create_tray_window};
use raumakey_controller::tray::{
    InputMode, LOOP_EXIT_CODE, TRAY_CALLBACK, TRAY_ICON_ID, Tray, TrayCommand, menu_items,
};
use raumakey_windows::icon::{IconKind, icon_size};
use raumakey_windows::menu::MenuItem;
use raumakey_windows::module::Module;
use raumakey_windows::shell::{NIN_SELECT, WM_CONTEXTMENU, taskbar_created_message};
use raumakey_windows::window::{Window, WindowClass, run_message_loop};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CHANGEFILTERSTRUCT, ChangeWindowMessageFilterEx, GA_PARENT, GetAncestor, GetDesktopWindow,
    IsWindowVisible, MSGFLT_ALLOW, MSGFLTINFO_ALREADYALLOWED_FORWND, MSGFLTINFO_NONE,
    MSGFLTINFO_STATUS, PostMessageW, PostThreadMessageW, SendMessageW, WM_CLOSE, WM_ENDSESSION,
    WM_QUIT,
};

use InputMode::{English, Vietnamese};

/// Exit code of a message loop that the watchdog ended.
const WATCHDOG: i32 = -77;

/// What the tray asked of the shell.
#[derive(Clone, Debug, PartialEq, Eq)]
enum ShellCall {
    Show(InputMode),
    Update(InputMode),
    Menu {
        x: i32,
        y: i32,
        items: Vec<MenuItem<'static>>,
    },
    Remove,
}

type Log = Rc<RefCell<Vec<ShellCall>>>;

/// Records every call; the menu returns the queued choices in turn (then "dismissed") and
/// runs `during_menu` with its owner window while it is "open".
struct FakeShell {
    log: Log,
    choices: VecDeque<Option<u16>>,
    during_menu: Option<Box<dyn FnMut(HWND)>>,
}

impl FakeShell {
    fn new(log: &Log) -> Self {
        Self {
            log: Rc::clone(log),
            choices: VecDeque::new(),
            during_menu: None,
        }
    }

    fn choosing(mut self, choices: &[Option<u16>]) -> Self {
        self.choices = choices.iter().copied().collect();
        self
    }

    fn during_menu(mut self, action: impl FnMut(HWND) + 'static) -> Self {
        self.during_menu = Some(Box::new(action));
        self
    }
}

impl TrayShell for FakeShell {
    fn show(&mut self, _owner: HWND, mode: InputMode) {
        self.log.borrow_mut().push(ShellCall::Show(mode));
    }

    fn update(&mut self, mode: InputMode) {
        self.log.borrow_mut().push(ShellCall::Update(mode));
    }

    fn open_menu(
        &mut self,
        owner: HWND,
        x: i32,
        y: i32,
        items: &[MenuItem<'static>],
    ) -> Option<u16> {
        self.log.borrow_mut().push(ShellCall::Menu {
            x,
            y,
            items: items.to_vec(),
        });
        if let Some(action) = &mut self.during_menu {
            action(owner);
        }
        self.choices.pop_front().flatten()
    }

    fn remove(&mut self) {
        self.log.borrow_mut().push(ShellCall::Remove);
    }
}

/// Runs `body` on a new thread, which owns the windows it creates; a panic fails the test.
fn on_fresh_thread<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    match std::thread::spawn(body).join() {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

/// The registered `TaskbarCreated` message.
fn taskbar_created() -> u32 {
    taskbar_created_message().unwrap()
}

/// The tray window in `mode` with `shell`, created the way the controller creates it.
fn tray_window(class_label: &str, mode: InputMode, shell: FakeShell) -> Window {
    let class = WindowClass::register(class_label).unwrap();
    create_tray_window(&class, Tray::new(mode, taskbar_created()), shell, false).unwrap()
}

/// Posts `message` to `window` with `PostMessageW` itself, as the shell or another process
/// does (a `WindowHandle` posts no system messages).
fn post(window: &Window, message: u32, wparam: usize, lparam: isize) {
    // SAFETY: posts plain values (no pointers) to a window of this thread.
    unsafe { PostMessageW(Some(window.hwnd()), message, WPARAM(wparam), LPARAM(lparam)) }.unwrap();
}

/// Posts the tray icon's callback message (`NOTIFYICON_VERSION_4`) for `code` at (x, y).
fn post_icon(window: &Window, code: u32, x: i16, y: i16) {
    let wparam = usize::from(x as u16) | (usize::from(y as u16) << 16);
    let lparam = ((u32::from(TRAY_ICON_ID) << 16) | code) as i32 as isize;
    post(window, TRAY_CALLBACK.id(), wparam, lparam);
}

/// Runs this thread's message loop until it ends; a watchdog ends it with [`WATCHDOG`] after
/// 20 s if nothing else does.
fn run_loop_with_watchdog() -> i32 {
    // SAFETY: no preconditions.
    let thread = unsafe { GetCurrentThreadId() };
    let (finished, wait) = mpsc::channel::<()>();
    let watchdog = std::thread::spawn(move || {
        if wait.recv_timeout(Duration::from_secs(20)) == Err(mpsc::RecvTimeoutError::Timeout) {
            // SAFETY: posts plain values to a thread that is still in its message loop.
            let _ = unsafe {
                PostThreadMessageW(thread, WM_QUIT, WPARAM(WATCHDOG as usize), LPARAM(0))
            };
        }
    });
    let code = run_message_loop().unwrap();
    drop(finished);
    watchdog.join().unwrap();
    code
}

#[test]
fn start_shows_the_icon_a_click_switches_it_and_exit_from_the_menu_ends_the_loop_with_0() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).choosing(&[Some(TrayCommand::Exit.id())]);
        let window = tray_window("test-tray-exit", Vietnamese, shell);
        post_icon(&window, NIN_SELECT, 30, 1050);
        post_icon(&window, WM_CONTEXTMENU, -25, 1040);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        assert_eq!(
            log.take(),
            [
                ShellCall::Show(Vietnamese),
                ShellCall::Update(English),
                ShellCall::Menu {
                    x: -25,
                    y: 1040,
                    items: menu_items(English).to_vec()
                },
                ShellCall::Remove,
            ]
        );
        assert!(
            window.is_alive(),
            "the window is left to its owner, who destroys it after the loop"
        );
        drop(window);
        assert_eq!(log.take(), [], "nothing is removed twice");
    });
}

#[test]
fn matrix_58_taskbar_created_adds_the_icon_again_with_the_same_mode() {
    on_fresh_thread(|| {
        let log = Log::default();
        let window = tray_window("test-tray-taskbar", Vietnamese, FakeShell::new(&log));
        post_icon(&window, NIN_SELECT, 0, 0);
        // Posted to this window only; Explorer broadcasts it after it restarts.
        post(&window, taskbar_created(), 0, 0);
        post(&window, taskbar_created(), 0, 0);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        assert_eq!(
            log.take(),
            [
                ShellCall::Show(Vietnamese),
                ShellCall::Update(English),
                ShellCall::Show(English),
                ShellCall::Show(English),
                ShellCall::Remove,
            ]
        );
    });
}

#[test]
fn wm_close_ends_the_loop_like_exit_and_does_not_destroy_the_window() {
    on_fresh_thread(|| {
        let log = Log::default();
        let window = tray_window("test-tray-close", English, FakeShell::new(&log));
        post(&window, WM_CLOSE, 0, 0);
        // Queued behind the exit: it must not add the removed icon again.
        post(&window, taskbar_created(), 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        assert_eq!(log.take(), [ShellCall::Show(English), ShellCall::Remove]);
        assert!(
            window.is_alive(),
            "WM_CLOSE was answered, not default-processed"
        );
    });
}

#[test]
fn the_end_of_the_session_ends_the_loop_like_exit() {
    on_fresh_thread(|| {
        let log = Log::default();
        let window = tray_window("test-tray-session", Vietnamese, FakeShell::new(&log));
        // A cancelled end changes nothing; a real one exits.
        post(&window, WM_ENDSESSION, 0, 0);
        post(&window, WM_ENDSESSION, 1, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        assert_eq!(log.take(), [ShellCall::Show(Vietnamese), ShellCall::Remove]);
    });
}

#[test]
fn choosing_vietnamese_mode_in_the_menu_switches_the_icon() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).choosing(&[Some(TrayCommand::VietnameseMode.id()), None]);
        let window = tray_window("test-tray-menu", Vietnamese, shell);
        post_icon(&window, WM_CONTEXTMENU, 10, 20);
        post_icon(&window, WM_CONTEXTMENU, 10, 20);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        assert_eq!(
            log.take(),
            [
                ShellCall::Show(Vietnamese),
                ShellCall::Menu {
                    x: 10,
                    y: 20,
                    items: menu_items(Vietnamese).to_vec()
                },
                ShellCall::Update(English),
                // Dismissed: nothing changes.
                ShellCall::Menu {
                    x: 10,
                    y: 20,
                    items: menu_items(English).to_vec()
                },
                ShellCall::Remove,
            ]
        );
    });
}

/// While the menu's modal loop runs inside the handler, the window's messages go to
/// `DefWindowProcW`, which answers a `WM_CLOSE` (here sent by the fake menu, as `taskkill`
/// would post one) by destroying the window. The tray still removes its icon and ends the
/// loop once the handler returns and is freed.
#[test]
fn a_window_destroyed_while_the_menu_is_open_still_removes_the_icon_and_ends_the_loop() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).during_menu(|owner| {
            // SAFETY: a window of this thread; no pointer arguments.
            unsafe { SendMessageW(owner, WM_CLOSE, None, None) };
        });
        let window = tray_window("test-tray-gone", Vietnamese, shell);
        post_icon(&window, WM_CONTEXTMENU, 1, 2);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        assert_eq!(
            log.take(),
            [
                ShellCall::Show(Vietnamese),
                ShellCall::Menu {
                    x: 1,
                    y: 2,
                    items: menu_items(Vietnamese).to_vec()
                },
                ShellCall::Remove,
            ]
        );
        assert!(!window.is_alive());
    });
}

/// What User Interface Privilege Isolation lets through to `window` for `message`, as
/// `ChangeWindowMessageFilterEx` reports it before it allows the message itself (on this test
/// window only, so call it once per window and message).
fn filter_status(window: &Window, message: u32) -> MSGFLTINFO_STATUS {
    let mut change = CHANGEFILTERSTRUCT {
        cbSize: size_of::<CHANGEFILTERSTRUCT>() as u32,
        ExtStatus: MSGFLTINFO_NONE,
    };
    // SAFETY: a window of this thread; `change` is a CHANGEFILTERSTRUCT of the size it states
    // and outlives the call.
    unsafe { ChangeWindowMessageFilterEx(window.hwnd(), message, MSGFLT_ALLOW, Some(&mut change)) }
        .unwrap();
    change.ExtStatus
}

/// An elevated controller lets `TaskbarCreated` (matrix #58) and the icon's callback through
/// UIPI, which a non-elevated Explorer needs to reach it; a non-elevated controller leaves its
/// filter alone. This non-elevated test reads the filter of each test window back.
#[test]
fn an_elevated_tray_window_lets_the_shell_messages_through() {
    on_fresh_thread(|| {
        let log = Log::default();
        let class = WindowClass::register("test-tray-elevated").unwrap();
        let elevated = create_tray_window(
            &class,
            Tray::new(Vietnamese, taskbar_created()),
            FakeShell::new(&log),
            true,
        )
        .unwrap();
        let not_elevated = create_tray_window(
            &class,
            Tray::new(Vietnamese, taskbar_created()),
            FakeShell::new(&log),
            false,
        )
        .unwrap();
        for message in [taskbar_created(), TRAY_CALLBACK.id()] {
            assert_eq!(
                filter_status(&elevated, message),
                MSGFLTINFO_ALREADYALLOWED_FORWND,
                "{message:#x}"
            );
            assert_eq!(
                filter_status(&not_elevated, message),
                MSGFLTINFO_NONE,
                "{message:#x}"
            );
        }
        // No message loop ran: the windows go before their tray started.
        drop((elevated, not_elevated));
        assert_eq!(log.take(), []);
    });
}

/// A message-only window never receives the `TaskbarCreated` broadcast (matrix #58), so the
/// tray window is a hidden top-level window: the desktop is its parent.
#[test]
fn the_tray_window_is_a_hidden_top_level_window() {
    on_fresh_thread(|| {
        let window = tray_window(
            "test-tray-top-level",
            Vietnamese,
            FakeShell::new(&Log::default()),
        );
        // SAFETY: read-only queries on a window of this thread.
        unsafe {
            assert_eq!(GetAncestor(window.hwnd(), GA_PARENT), GetDesktopWindow());
            assert!(!IsWindowVisible(window.hwnd()).as_bool());
        }
    });
}

/// Only `raumakey.exe` carries the V and E icons (`build.rs`); this test executable has
/// none, so loading them fails with the loader's error (the binary's own unit test loads
/// them).
#[test]
fn the_mode_icons_are_not_in_a_test_executable() {
    let size = icon_size(IconKind::Small).unwrap();
    let error = ModeIcons::load(Module::executable().unwrap(), size).unwrap_err();
    assert_eq!(error.context(), "LoadImageW");
}
