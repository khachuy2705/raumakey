//! The tray window glue (`desktop::create_tray_window`, `desktop::TrayWindow`) on a real
//! hidden window, message loop, and window timer, with a fake shell in place of the
//! notification icon, popup menu, and message boxes, and fake services in place of the typing
//! hook and the settings worker, so nothing is shown, no hook is installed, and no file is
//! touched.
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

use raumakey_config::{ConfigError, StartupMode};
use raumakey_controller::desktop::{ModeIcons, TrayServices, TrayShell, create_tray_window};
use raumakey_controller::notice::{Notice, Question, save_error_notice};
use raumakey_controller::settings::{Change, DocumentStatus, Location, Status};
use raumakey_controller::tray::{
    Event, IconState, InputMode, LOOP_EXIT_CODE, POLL_INTERVAL_MS, TRAY_CALLBACK, TRAY_ICON_ID,
    Tray, TrayCommand, TrayStart, WAKE,
};
use raumakey_controller::worker::{Answer, Outcome, WorkerCommand};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::options::InputMethod;
use raumakey_windows::icon::{IconKind, icon_size};
use raumakey_windows::menu::MenuItem;
use raumakey_windows::module::Module;
use raumakey_windows::shell::{NIN_SELECT, WM_CONTEXTMENU, taskbar_created_message};
use raumakey_windows::window::{Window, WindowClass, post_quit, run_message_loop};
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

/// A poll interval long enough that no tick comes during a test that does not wait for one.
const NO_TICKS: u32 = 600_000;

/// What the glue asked of the shell and the services, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Show(IconState),
    Update(IconState),
    Menu {
        x: i32,
        y: i32,
        items: Vec<MenuItem<'static>>,
    },
    Notify(Notice),
    Remove,
    HookMode(bool),
    HookRuntime(Box<RuntimeSnapshot>),
    StopTyping,
    KeepTypingAlive,
    Send(u64, WorkerCommand),
    FinishWorker,
}

type Log = Rc<RefCell<Vec<Call>>>;

/// Records every call; the menu returns the queued choices in turn (then "dismissed") and runs
/// `during_menu` with its owner window while it is "open"; message boxes answer the queued
/// answers in turn (then "no").
struct FakeShell {
    log: Log,
    choices: VecDeque<Option<u16>>,
    answers: VecDeque<bool>,
    during_menu: Option<Box<dyn FnMut(HWND)>>,
}

impl FakeShell {
    fn new(log: &Log) -> Self {
        Self {
            log: Rc::clone(log),
            choices: VecDeque::new(),
            answers: VecDeque::new(),
            during_menu: None,
        }
    }

    fn choosing(mut self, choices: &[Option<u16>]) -> Self {
        self.choices = choices.iter().copied().collect();
        self
    }

    fn answering(mut self, answers: &[bool]) -> Self {
        self.answers = answers.iter().copied().collect();
        self
    }

    fn during_menu(mut self, action: impl FnMut(HWND) + 'static) -> Self {
        self.during_menu = Some(Box::new(action));
        self
    }
}

impl TrayShell for FakeShell {
    fn show(&mut self, _owner: HWND, icon: IconState) {
        self.log.borrow_mut().push(Call::Show(icon));
    }

    fn update(&mut self, icon: IconState) {
        self.log.borrow_mut().push(Call::Update(icon));
    }

    fn open_menu(
        &mut self,
        owner: HWND,
        x: i32,
        y: i32,
        items: &[MenuItem<'static>],
    ) -> Option<u16> {
        self.log.borrow_mut().push(Call::Menu {
            x,
            y,
            items: items.to_vec(),
        });
        if let Some(action) = &mut self.during_menu {
            action(owner);
        }
        self.choices.pop_front().flatten()
    }

    fn notify(&mut self, notice: &Notice) -> bool {
        self.log.borrow_mut().push(Call::Notify(notice.clone()));
        self.answers.pop_front().unwrap_or(false)
    }

    fn remove(&mut self) {
        self.log.borrow_mut().push(Call::Remove);
    }
}

/// What the fake worker does with a command it is sent: it may answer into the inbox.
type OnSend = Box<dyn FnMut(u64, WorkerCommand, &mpsc::Sender<Event>)>;

/// Records every call; the inbox is a channel that other threads may fill; `on_send` plays
/// the worker and answers the commands it is sent.
struct FakeServices {
    log: Log,
    inbox: mpsc::Receiver<Event>,
    /// Feeds the inbox from this thread (for the answers of `on_send`).
    feed: mpsc::Sender<Event>,
    on_send: Option<OnSend>,
}

impl FakeServices {
    fn new(log: &Log) -> (Self, mpsc::Sender<Event>) {
        let (feed, inbox) = mpsc::channel();
        let services = Self {
            log: Rc::clone(log),
            inbox,
            feed: feed.clone(),
            on_send: None,
        };
        (services, feed)
    }

    fn on_send(
        mut self,
        action: impl FnMut(u64, WorkerCommand, &mpsc::Sender<Event>) + 'static,
    ) -> Self {
        self.on_send = Some(Box::new(action));
        self
    }
}

impl TrayServices for FakeServices {
    fn hook_mode(&mut self, vietnamese: bool) {
        self.log.borrow_mut().push(Call::HookMode(vietnamese));
    }

    fn hook_runtime(&mut self, runtime: RuntimeSnapshot) {
        self.log
            .borrow_mut()
            .push(Call::HookRuntime(Box::new(runtime)));
    }

    fn stop_typing(&mut self) {
        self.log.borrow_mut().push(Call::StopTyping);
    }

    fn keep_typing_alive(&mut self) {
        self.log.borrow_mut().push(Call::KeepTypingAlive);
    }

    fn send(&mut self, seq: u64, command: WorkerCommand) {
        self.log.borrow_mut().push(Call::Send(seq, command));
        if let Some(action) = &mut self.on_send {
            action(seq, command, &self.feed);
        }
    }

    fn finish_worker(&mut self) {
        self.log.borrow_mut().push(Call::FinishWorker);
    }

    fn next_event(&mut self) -> Option<Event> {
        self.inbox.try_recv().ok()
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

fn runtime(mode: InputMode) -> RuntimeSnapshot {
    let mut runtime = RuntimeSnapshot::defaults_v1();
    runtime.config_revision = 1;
    runtime.vietnamese_mode = mode.is_vietnamese();
    runtime.options.enabled = mode.is_vietnamese();
    runtime
}

fn tray(mode: InputMode) -> Tray {
    let start = TrayStart {
        runtime: runtime(mode),
        startup: StartupMode::None,
        location: Location::ExeDir,
        config_path: Some(r"C:\Raumakey\raumakey.toml".into()),
        status: Status {
            config: DocumentStatus::Loaded { issues: Vec::new() },
            macros: DocumentStatus::Missing,
            app_rules: DocumentStatus::Missing,
            skipped_macros: Vec::new(),
        },
        notices: Vec::new(),
    };
    Tray::new(start, taskbar_created())
}

fn icon(mode: InputMode, method: InputMethod) -> IconState {
    IconState { mode, method }
}

/// The calls of an exit, in order.
fn exit_calls() -> [Call; 3] {
    [Call::StopTyping, Call::Remove, Call::FinishWorker]
}

/// The tray window in `mode` with `shell` and `services`, created the way the controller
/// creates it.
fn tray_window(
    class_label: &str,
    mode: InputMode,
    shell: FakeShell,
    services: FakeServices,
    poll_interval_ms: u32,
) -> Window {
    let class = WindowClass::register(class_label).unwrap();
    create_tray_window(&class, tray(mode), shell, services, false, poll_interval_ms).unwrap()
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

/// Exit from the menu stops the hook, removes the icon, lets the worker finish, and ends the
/// loop with 0, in that order.
#[test]
fn start_shows_the_icon_a_click_switches_it_and_exit_from_the_menu_ends_the_loop_with_0() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).choosing(&[Some(TrayCommand::Exit.id())]);
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window("test-tray-exit", Vietnamese, shell, services, NO_TICKS);
        post_icon(&window, NIN_SELECT, 30, 1050);
        post_icon(&window, WM_CONTEXTMENU, -25, 1040);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![
            Call::Show(icon(Vietnamese, InputMethod::Telex)),
            Call::Update(icon(English, InputMethod::Telex)),
            Call::HookMode(false),
            Call::Send(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
            Call::Menu {
                x: -25,
                y: 1040,
                items: {
                    let mut english = tray(English);
                    english.on_message(raumakey_controller::tray::START.id(), 0, 0);
                    english.menu()
                },
            },
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
        assert!(
            window.is_alive(),
            "the window is left to its owner, who destroys it after the loop"
        );
        drop(window);
        assert_eq!(log.take(), [], "nothing is removed twice");
    });
}

#[test]
fn matrix_58_taskbar_created_adds_the_icon_again_with_the_same_state() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window(
            "test-tray-taskbar",
            Vietnamese,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        post_icon(&window, NIN_SELECT, 0, 0);
        // Posted to this window only; Explorer broadcasts it after it restarts.
        post(&window, taskbar_created(), 0, 0);
        post(&window, taskbar_created(), 0, 0);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![
            Call::Show(icon(Vietnamese, InputMethod::Telex)),
            Call::Update(icon(English, InputMethod::Telex)),
            Call::HookMode(false),
            Call::Send(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
            Call::Show(icon(English, InputMethod::Telex)),
            Call::Show(icon(English, InputMethod::Telex)),
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
    });
}

#[test]
fn wm_close_ends_the_loop_like_exit_and_does_not_destroy_the_window() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window(
            "test-tray-close",
            English,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        post(&window, WM_CLOSE, 0, 0);
        // Queued behind the exit: it must not add the removed icon again.
        post(&window, taskbar_created(), 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![Call::Show(icon(English, InputMethod::Telex))];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
        assert!(
            window.is_alive(),
            "WM_CLOSE was answered, not default-processed"
        );
    });
}

/// After `WM_ENDSESSION` the process may end at any time: the hook is stopped and the worker
/// finished inside the message.
#[test]
fn the_end_of_the_session_stops_the_hook_and_ends_the_loop_like_exit() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window(
            "test-tray-session",
            Vietnamese,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        // A cancelled end changes nothing; a real one exits.
        post(&window, WM_ENDSESSION, 0, 0);
        post(&window, WM_ENDSESSION, 1, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![Call::Show(icon(Vietnamese, InputMethod::Telex))];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
    });
}

/// A method chosen in the menu reaches the icon, the hook (a snapshot), and the worker; a
/// dismissed menu changes nothing.
#[test]
fn choosing_an_input_method_in_the_menu_reaches_the_icon_the_hook_and_the_worker() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).choosing(&[Some(TrayCommand::Vni.id()), None]);
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window("test-tray-menu", Vietnamese, shell, services, NO_TICKS);
        post_icon(&window, WM_CONTEXTMENU, 10, 20);
        post_icon(&window, WM_CONTEXTMENU, 10, 20);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let calls = log.take();
        let mut vni = runtime(Vietnamese);
        vni.options.input_method = InputMethod::Vni;
        assert!(
            matches!(calls[1], Call::Menu { x: 10, y: 20, .. }),
            "{calls:?}"
        );
        assert_eq!(
            calls[2..5],
            [
                Call::Update(icon(Vietnamese, InputMethod::Vni)),
                Call::HookRuntime(Box::new(vni)),
                Call::Send(
                    1,
                    WorkerCommand::Apply(Change::InputMethod(InputMethod::Vni))
                ),
            ]
        );
        assert!(matches!(calls[5], Call::Menu { .. }), "{calls:?}");
        assert_eq!(calls[6..], exit_calls(), "dismissed: nothing changes");
    });
}

/// While the menu's modal loop runs inside the handler, the window's messages go to
/// `DefWindowProcW`, which answers a `WM_CLOSE` (here sent by the fake menu, as `taskkill`
/// would post one) by destroying the window. The tray still stops the hook, removes its
/// icon, finishes the worker, and ends the loop once the handler returns and is freed.
#[test]
fn a_window_destroyed_while_the_menu_is_open_still_stops_everything_and_ends_the_loop() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).during_menu(|owner| {
            // SAFETY: a window of this thread; no pointer arguments.
            unsafe { SendMessageW(owner, WM_CLOSE, None, None) };
        });
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window("test-tray-gone", Vietnamese, shell, services, NO_TICKS);
        post_icon(&window, WM_CONTEXTMENU, 1, 2);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let calls = log.take();
        assert_eq!(calls[0], Call::Show(icon(Vietnamese, InputMethod::Telex)));
        assert!(
            matches!(calls[1], Call::Menu { x: 1, y: 2, .. }),
            "{calls:?}"
        );
        assert_eq!(calls[2..], exit_calls());
        assert!(!window.is_alive());
    });
}

/// The window timer asks the worker to poll; the next tick polls again once the answer came.
/// Each tick also checks the typing hook first: Windows removes a slow low-level hook without
/// telling it, and `TypingHook::ensure_alive` installs it again (design §21.2).
#[test]
fn the_poll_timer_asks_the_worker_to_poll_again_and_again() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let services = services.on_send(|seq, _command, feed| {
            feed.send(Event::Worker(Answer {
                seq,
                outcome: Outcome::Polled(None),
                state: None,
            }))
            .unwrap();
            if seq == 3 {
                post_quit(5);
            }
        });
        let _window = tray_window(
            "test-tray-poll",
            Vietnamese,
            FakeShell::new(&log),
            services,
            10,
        );
        assert_eq!(run_loop_with_watchdog(), 5);
        assert_eq!(
            log.take(),
            [
                Call::Show(icon(Vietnamese, InputMethod::Telex)),
                Call::KeepTypingAlive,
                Call::Send(1, WorkerCommand::Poll),
                Call::KeepTypingAlive,
                Call::Send(2, WorkerCommand::Poll),
                Call::KeepTypingAlive,
                Call::Send(3, WorkerCommand::Poll),
            ]
        );
        assert_eq!(POLL_INTERVAL_MS, 2_000);
    });
}

/// The hook observer and the worker leave events in the inbox from their threads and post
/// `WAKE`: the glue drains the inbox and carries out the effects.
#[test]
fn events_from_other_threads_are_drained_after_wake() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, feed) = FakeServices::new(&log);
        let window = tray_window(
            "test-tray-wake",
            Vietnamese,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        let handle = window.handle();
        std::thread::spawn(move || {
            feed.send(Event::Hotkey(false)).unwrap();
            handle.post(WAKE.id(), WPARAM(0), LPARAM(0)).unwrap();
        })
        .join()
        .unwrap();
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![
            Call::Show(icon(Vietnamese, InputMethod::Telex)),
            Call::Update(icon(English, InputMethod::Telex)),
            Call::HookMode(false),
            Call::Send(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
    });
}

/// A save error is shown in a message box; "yes" to its recovery question goes to the
/// worker.
#[test]
fn a_notice_is_shown_and_yes_to_its_question_reaches_the_worker() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).answering(&[true]);
        let (services, _feed) = FakeServices::new(&log);
        // The worker cannot save over the damaged file.
        let services = services.on_send(|seq, command, feed| {
            if let WorkerCommand::Apply(change) = command {
                let outcome = Outcome::Applied {
                    change,
                    result: Err(ConfigError::Corrupt),
                };
                let answer = Answer {
                    seq,
                    outcome,
                    state: None,
                };
                feed.send(Event::Worker(answer)).unwrap();
            }
        });
        let window = tray_window("test-tray-notice", Vietnamese, shell, services, NO_TICKS);
        post_icon(&window, NIN_SELECT, 0, 0);
        let change = Change::VietnameseMode(false);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let notice = save_error_notice(
            Some(r"C:\Raumakey\raumakey.toml".as_ref()),
            ConfigError::Corrupt,
        );
        assert_eq!(notice.question, Some(Question::Recover));
        let mut expected = vec![
            Call::Show(icon(Vietnamese, InputMethod::Telex)),
            Call::Update(icon(English, InputMethod::Telex)),
            Call::HookMode(false),
            Call::Send(1, WorkerCommand::Apply(change)),
            Call::Notify(notice),
            Call::Send(2, WorkerCommand::Recover),
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
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
        let (services, _feed) = FakeServices::new(&log);
        let elevated = create_tray_window(
            &class,
            tray(Vietnamese),
            FakeShell::new(&log),
            services,
            true,
            NO_TICKS,
        )
        .unwrap();
        let (services, _feed) = FakeServices::new(&log);
        let not_elevated = create_tray_window(
            &class,
            tray(Vietnamese),
            FakeShell::new(&log),
            services,
            false,
            NO_TICKS,
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
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window(
            "test-tray-top-level",
            Vietnamese,
            FakeShell::new(&log),
            services,
            NO_TICKS,
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
