//! The tray window glue (`desktop::create_tray_window`, `desktop::TrayWindow`,
//! `desktop::ShellTray`) on a real hidden window, message loop, and window timer, with fakes
//! in place of the notification icon, popup menu, message boxes, typing hook, and settings
//! worker, so nothing is shown, no hook is installed, and no file is touched.
//!
//! Notices never block the tray's thread (message boxes run on threads of their own), so the
//! fake shell keeps them "open" for as long as a test wants, and the window goes on handling
//! `WM_CLOSE`, `WM_ENDSESSION`, `TaskbarCreated`, clicks, and the timer meanwhile.
//!
//! Each test runs on a fresh thread, which owns the window and destroys it before it ends.
//! Messages are posted or sent to the test window only: `TaskbarCreated` is never broadcast,
//! and neither is the show-panel request of a second start (the tests register it under a
//! test name, so a Raumakey running on this desktop never sees it). A watchdog ends the
//! message loop with [`WATCHDOG`] if the code under test never does.
//!
//! Some tests run the real settings panel and the real macro list editor (`ShellTray` with its
//! off-screen variant), one test at a time ([`real_shell`]): they stay off every monitor and
//! are never activated or brought to the front, and they are driven with `SendMessageW` from
//! the test thread; no test clicks "Mặc định", whose question would show a message box. Every
//! wait has a timeout.

#![cfg(windows)]

/// Common Controls 6 for the real panel, as `raumakey.exe`'s manifest asks for it; shared with
/// the tests of `raumakey-windows`.
#[path = "../../raumakey-windows/tests/layer/common_controls_v6.rs"]
mod common_controls_v6;

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use raumakey_config::{ConfigError, StartupMode};
use raumakey_controller::desktop::{
    ModeIcons, NotifyArea, ShellTray, TrayServices, TrayShell, WakeTarget, create_tray_window,
};
use raumakey_controller::exclusions::{ExclusionsRequest, ExclusionsState, default_apps};
use raumakey_controller::exclusions_dialog;
use raumakey_controller::macro_dialog;
use raumakey_controller::notice::{
    Notice, NoticeKind, Question, save_error_notice, temporary_exe_notice,
};
use raumakey_controller::panel::{
    ID_AUTO_RESTORE, ID_CLOSE, ID_CONFIG_FILE, ID_CONFIG_LOCATION, ID_EXCLUSIONS, ID_FREE_MARKING,
    ID_HINT, ID_MACRO_AUTO_CASE, ID_MACRO_LIST, ID_MACROS, ID_MACROS_WHEN_ENGLISH, ID_METHOD,
    ID_MODERN_TONE, ID_OPEN_CONFIG, ID_SHOW_ON_START, ID_SPELL_CHECK, ID_STARTUP, ID_VIETNAMESE,
    PanelRequest, PanelState, panel_title,
};
use raumakey_controller::settings::{Change, DocumentStatus, Location, Status};
use raumakey_controller::tray::{
    DEFAULT_DOUBLE_CLICK_MS, Event, FOREGROUND, ForegroundApp, IconState, InputMode,
    LOOP_EXIT_CODE, POLL_INTERVAL_MS, RegisteredMessages, START, TRAY_CALLBACK, TRAY_ICON_ID, Tray,
    TrayCommand, TrayStart, WAKE,
};
use raumakey_controller::worker::{Answer, Outcome, PollReport, WorkerCommand};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::host::ExeName;
use raumakey_model::options::InputMethod;
use raumakey_model::policy::{AppRule, RuleAction};
use raumakey_windows::dialog_box::notifications::{BN_CLICKED, CBN_SELCHANGE};
use raumakey_windows::dialog_template::{IDCANCEL, styles};
use raumakey_windows::foreground::window_class_name;
use raumakey_windows::icon::{IconKind, icon_size};
use raumakey_windows::input::double_click_time;
use raumakey_windows::menu::MenuItem;
use raumakey_windows::module::Module;
use raumakey_windows::shell::{
    NIN_SELECT, WM_CONTEXTMENU, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP,
    taskbar_created_message,
};
use raumakey_windows::window::{
    Window, WindowClass, post_quit, register_user_message, run_message_loop,
};
use raumakey_windows::word_wrap::line_width;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    DT_CALCRECT, DT_MODIFYSTRING, DT_NOPREFIX, DT_PATH_ELLIPSIS, DT_SINGLELINE, DT_WORDBREAK,
    GetDC, HDC, HGDIOBJ, ReleaseDC, SelectObject,
};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Controls::LVM_GETITEMCOUNT;
use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
use windows::Win32::UI::WindowsAndMessaging::{
    BM_CLICK, BM_GETCHECK, CB_GETCOUNT, CB_GETCURSEL, CB_SETCURSEL, CHANGEFILTERSTRUCT,
    ChangeWindowMessageFilterEx, EnumChildWindows, EnumWindows, GA_PARENT, GW_CHILD, GWL_STYLE,
    GetAncestor, GetClassNameW, GetClientRect, GetDesktopWindow, GetDlgCtrlID, GetDlgItem,
    GetNextDlgGroupItem, GetWindow, GetWindowLongW, GetWindowRect, GetWindowThreadProcessId,
    InternalGetWindowText, IsWindow, IsWindowVisible, MSGFLT_ALLOW,
    MSGFLTINFO_ALREADYALLOWED_FORWND, MSGFLTINFO_NONE, MSGFLTINFO_STATUS, PostMessageW,
    PostThreadMessageW, SMTO_NORMAL, SendMessageTimeoutW, SendMessageW, WM_CHAR, WM_CLOSE,
    WM_COMMAND, WM_ENDSESSION, WM_GETFONT, WM_GETTEXT, WM_GETTEXTLENGTH, WM_NULL, WM_QUIT,
    WM_SETTEXT,
};
use windows::core::BOOL;

use InputMode::{English, Vietnamese};
use common_controls_v6::CommonControls6;

/// Exit code of a message loop that the watchdog ended.
const WATCHDOG: i32 = -77;

/// A poll interval long enough that no tick comes during a test that does not wait for one.
const NO_TICKS: u32 = 600_000;

/// What the glue asked of the shell and the services, in order.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Call {
    Show(IconState),
    Update(IconState),
    Ensure(IconState),
    Menu {
        x: i32,
        y: i32,
        items: Vec<MenuItem<'static>>,
    },
    Notify(Notice),
    Dismiss,
    Remove,
    OpenPanel(u64, Box<PanelState>),
    RefreshPanel(Box<PanelState>),
    ClosePanel,
    MacroList(PathBuf),
    CloseMacroList,
    OpenExclusions(u64, ExclusionsState),
    RefreshExclusions(ExclusionsState),
    CloseExclusions,
    HookMode(bool),
    HookRuntime(Box<RuntimeSnapshot>),
    StopTyping,
    KeepTypingAlive,
    /// The glue read the user's double-click time.
    DoubleClickTime,
    Send(u64, WorkerCommand),
    FinishWorker,
}

type Log = Rc<RefCell<Vec<Call>>>;

/// The windows whose application the glue read (`None`: the one in front), in order.
type Reads = Rc<RefCell<Vec<Option<isize>>>>;

/// Records every call; the menu returns the queued choices in turn (then "dismissed") and runs
/// `during_menu` with its owner window while it is "open". Notices stay "open" and never
/// block; a queued "yes" is answered into the inbox as `Event::Answered`, as the box's thread
/// does. The first panel opened sends the queued panel events into the inbox, as the panel's
/// thread does. Reads of the application in front answer the queued applications in turn
/// (then "unknown") and are recorded apart from the calls. The double-click time is the
/// default of Windows.
struct FakeShell {
    log: Log,
    choices: VecDeque<Option<u16>>,
    answers: VecDeque<bool>,
    inbox: Option<mpsc::Sender<Event>>,
    during_menu: Option<Box<dyn FnMut(HWND)>>,
    panel_events: Vec<Event>,
    exclusions_events: Vec<Event>,
    foreground: VecDeque<Option<ForegroundApp>>,
    reads: Reads,
}

impl FakeShell {
    fn new(log: &Log) -> Self {
        Self {
            log: Rc::clone(log),
            choices: VecDeque::new(),
            answers: VecDeque::new(),
            inbox: None,
            during_menu: None,
            panel_events: Vec::new(),
            exclusions_events: Vec::new(),
            foreground: VecDeque::new(),
            reads: Reads::default(),
        }
    }

    /// The first exclusions dialog opened sends `events` into `inbox`.
    fn exclusions_sending(mut self, events: Vec<Event>, inbox: &mpsc::Sender<Event>) -> Self {
        self.exclusions_events = events;
        self.inbox = Some(inbox.clone());
        self
    }

    /// Answers the next reads of the application in front with `apps`, and records the reads
    /// in `reads`.
    fn reading_foreground(mut self, apps: Vec<Option<ForegroundApp>>, reads: &Reads) -> Self {
        self.foreground = apps.into();
        self.reads = Rc::clone(reads);
        self
    }

    /// The first panel opened sends `events` into `inbox`.
    fn panel_sending(mut self, events: Vec<Event>, inbox: &mpsc::Sender<Event>) -> Self {
        self.panel_events = events;
        self.inbox = Some(inbox.clone());
        self
    }

    fn choosing(mut self, choices: &[Option<u16>]) -> Self {
        self.choices = choices.iter().copied().collect();
        self
    }

    /// Answers the questions of the next notices, into `inbox`.
    fn answering(mut self, answers: &[bool], inbox: &mpsc::Sender<Event>) -> Self {
        self.answers = answers.iter().copied().collect();
        self.inbox = Some(inbox.clone());
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

    fn ensure(&mut self, _owner: HWND, icon: IconState) {
        self.log.borrow_mut().push(Call::Ensure(icon));
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

    fn notify(&mut self, notice: Notice) {
        let yes = self.answers.pop_front().unwrap_or(false);
        if yes && let (Some(question), Some(inbox)) = (notice.question, &self.inbox) {
            inbox.send(Event::Answered(question)).unwrap();
        }
        self.log.borrow_mut().push(Call::Notify(notice));
    }

    fn dismiss_notices(&mut self) {
        self.log.borrow_mut().push(Call::Dismiss);
    }

    fn remove(&mut self) {
        self.log.borrow_mut().push(Call::Remove);
    }

    fn open_panel(&mut self, serial: u64, state: PanelState) {
        self.log
            .borrow_mut()
            .push(Call::OpenPanel(serial, Box::new(state)));
        if let Some(inbox) = &self.inbox {
            for event in self.panel_events.drain(..) {
                inbox.send(event).unwrap();
            }
        }
    }

    fn refresh_panel(&mut self, state: PanelState) {
        self.log
            .borrow_mut()
            .push(Call::RefreshPanel(Box::new(state)));
    }

    fn close_panel(&mut self) {
        self.log.borrow_mut().push(Call::ClosePanel);
    }

    fn open_macro_list(&mut self, folder: PathBuf) {
        self.log.borrow_mut().push(Call::MacroList(folder));
    }

    fn close_macro_list(&mut self) {
        self.log.borrow_mut().push(Call::CloseMacroList);
    }

    fn open_exclusions(&mut self, serial: u64, state: ExclusionsState) {
        self.log
            .borrow_mut()
            .push(Call::OpenExclusions(serial, state));
        if let Some(inbox) = &self.inbox {
            for event in self.exclusions_events.drain(..) {
                inbox.send(event).unwrap();
            }
        }
    }

    fn refresh_exclusions(&mut self, state: ExclusionsState) {
        self.log.borrow_mut().push(Call::RefreshExclusions(state));
    }

    fn close_exclusions(&mut self) {
        self.log.borrow_mut().push(Call::CloseExclusions);
    }

    fn foreground_app(&mut self, window: Option<isize>) -> Option<ForegroundApp> {
        self.reads.borrow_mut().push(window);
        self.foreground.pop_front().flatten()
    }

    fn double_click_ms(&mut self) -> u32 {
        self.log.borrow_mut().push(Call::DoubleClickTime);
        DEFAULT_DOUBLE_CLICK_MS
    }
}

/// What the fake worker does with a command it is sent: it may answer into the inbox.
type OnSend = Box<dyn FnMut(u64, WorkerCommand, &mpsc::Sender<Event>)>;

/// Records every call; the inbox is a channel that other threads may fill; `on_send` plays
/// the worker and answers the commands it is sent; `on_stop` plays the hook's last events.
struct FakeServices {
    log: Log,
    inbox: mpsc::Receiver<Event>,
    /// Feeds the inbox from this thread (for the answers of `on_send`).
    feed: mpsc::Sender<Event>,
    on_send: Option<OnSend>,
    /// Left in the inbox when the hook stops (hotkey changes it made just before).
    on_stop: Vec<Event>,
}

impl FakeServices {
    fn new(log: &Log) -> (Self, mpsc::Sender<Event>) {
        let (feed, inbox) = mpsc::channel();
        let services = Self {
            log: Rc::clone(log),
            inbox,
            feed: feed.clone(),
            on_send: None,
            on_stop: Vec::new(),
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

    fn on_stop(mut self, events: Vec<Event>) -> Self {
        self.on_stop = events;
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
        for event in self.on_stop.drain(..) {
            self.feed.send(event).unwrap();
        }
    }

    fn keep_typing_alive(&mut self) {
        self.log.borrow_mut().push(Call::KeepTypingAlive);
    }

    fn send(&mut self, seq: u64, command: WorkerCommand) {
        self.log.borrow_mut().push(Call::Send(seq, command.clone()));
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

/// What a fake notification area was asked.
#[derive(Clone, Debug, PartialEq, Eq)]
enum AreaCall {
    Add(IconState),
    Modify(IconState),
    Delete,
}

type AreaLog = Rc<RefCell<Vec<AreaCall>>>;

/// A notification area whose adds and modifies answer the queued results (then "it worked").
struct FakeArea {
    log: AreaLog,
    adds: VecDeque<bool>,
    modifies: VecDeque<bool>,
}

impl NotifyArea for FakeArea {
    fn add(&mut self, _owner: HWND, icon: IconState) -> bool {
        self.log.borrow_mut().push(AreaCall::Add(icon));
        self.adds.pop_front().unwrap_or(true)
    }

    fn modify(&mut self, icon: IconState) -> bool {
        self.log.borrow_mut().push(AreaCall::Modify(icon));
        self.modifies.pop_front().unwrap_or(true)
    }

    fn delete(&mut self) {
        self.log.borrow_mut().push(AreaCall::Delete);
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

/// The show-panel request of the tests: registered for this user like the real one, but under
/// a test name, so that no running Raumakey ever takes it for its own.
fn test_show_panel() -> u32 {
    register_user_message("test-show-panel").unwrap()
}

fn messages() -> RegisteredMessages {
    RegisteredMessages {
        taskbar_created: taskbar_created(),
        show_panel: test_show_panel(),
    }
}

fn runtime(mode: InputMode) -> RuntimeSnapshot {
    let mut runtime = RuntimeSnapshot::defaults_v1();
    runtime.config_revision = 1;
    runtime.vietnamese_mode = mode.is_vietnamese();
    runtime.options.enabled = mode.is_vietnamese();
    runtime
}

fn start(mode: InputMode) -> TrayStart {
    TrayStart {
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
        temporary_exe: false,
        show_panel: false,
        exclusions: default_apps(),
    }
}

fn tray(mode: InputMode) -> Tray {
    Tray::new(start(mode), messages())
}

fn icon(mode: InputMode, method: InputMethod) -> IconState {
    IconState::new(mode, method)
}

/// The calls of an exit, in order: the hook stops, open notices, the panel, the macro list
/// editor, and the exclusions dialog close, the worker finishes, and only then the icon goes.
fn exit_calls() -> [Call; 7] {
    [
        Call::StopTyping,
        Call::Dismiss,
        Call::ClosePanel,
        Call::CloseMacroList,
        Call::CloseExclusions,
        Call::FinishWorker,
        Call::Remove,
    ]
}

/// The tray window created the way the controller creates it.
fn window_for(
    class_label: &str,
    tray: Tray,
    shell: impl TrayShell + 'static,
    services: FakeServices,
    poll_interval_ms: u32,
) -> Window {
    let class = WindowClass::register(class_label).unwrap();
    create_tray_window(&class, tray, shell, services, false, poll_interval_ms).unwrap()
}

/// The tray window in `mode` with `shell` and `services`.
fn tray_window(
    class_label: &str,
    mode: InputMode,
    shell: FakeShell,
    services: FakeServices,
    poll_interval_ms: u32,
) -> Window {
    window_for(class_label, tray(mode), shell, services, poll_interval_ms)
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

/// The menu of a running tray in `mode`.
fn menu_of(mode: InputMode) -> Vec<MenuItem<'static>> {
    let mut tray = tray(mode);
    tray.on_message_at(START.id(), 0, 0, 0);
    tray.menu()
}

/// Exit from the menu stops the hook, closes the notices, lets the worker finish, removes
/// the icon, and ends the loop with 0, in that order.
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
                items: menu_of(English),
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

/// The glue hands the tray the clock with the icon's messages: a double-click as the shell
/// reports it (press, release, select, double-click, release, select) opens the panel and
/// leaves the mode as it was; the select of the second click is swallowed (D46).
#[test]
fn a_double_click_on_the_icon_opens_the_panel_and_keeps_the_mode() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        // A double-click time no busy test machine can overrun.
        let tray = tray(Vietnamese).with_double_click_ms(60_000);
        let window = window_for(
            "test-tray-double-click",
            tray,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        post_icon(&window, WM_LBUTTONDOWN, 0, 0);
        post_icon(&window, WM_LBUTTONUP, 0, 0);
        post_icon(&window, NIN_SELECT, 0, 0);
        post_icon(&window, WM_LBUTTONDBLCLK, 0, 0);
        post_icon(&window, WM_LBUTTONUP, 0, 0);
        post_icon(&window, NIN_SELECT, 0, 0);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![
            Call::Show(icon(Vietnamese, InputMethod::Telex)),
            Call::Update(icon(English, InputMethod::Telex)),
            Call::HookMode(false),
            Call::Send(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
            Call::Update(icon(Vietnamese, InputMethod::Telex)),
            Call::HookMode(true),
            Call::Send(2, WorkerCommand::Apply(Change::VietnameseMode(true))),
            Call::OpenPanel(1, Box::new(panel_state(Vietnamese))),
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
    });
}

/// At the start and at each message of the foreground watcher the glue reads the application
/// in front (the window the message names) and hands it to the tray: an excluded one shows E
/// and its name, the next one the mode again (D47).
#[test]
fn the_icon_follows_the_application_in_front() {
    on_fresh_thread(|| {
        let log = Log::default();
        let reads = Reads::default();
        let mstsc = ExeName::new("mstsc.exe").unwrap();
        let apps = vec![
            Some(ForegroundApp {
                exe: mstsc.clone(),
                window_class: Some("TscShellContainerClass".to_owned()),
            }),
            Some(ForegroundApp {
                exe: ExeName::new("notepad.exe").unwrap(),
                window_class: Some("Notepad".to_owned()),
            }),
        ];
        let shell = FakeShell::new(&log).reading_foreground(apps, &reads);
        let (services, _feed) = FakeServices::new(&log);
        let mut start = start(Vietnamese);
        start.runtime.compatibility.rules.push(AppRule {
            exe: mstsc.clone(),
            window_class: None,
            action: RuleAction {
                vietnamese: Some(false),
                macros: Some(false),
                ..RuleAction::default()
            },
        });
        let tray = Tray::new(start, messages());
        let window = window_for("test-tray-foreground", tray, shell, services, NO_TICKS);
        post(&window, FOREGROUND.id(), 0x1234, 0);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![
            Call::Show(icon(Vietnamese, InputMethod::Telex)),
            Call::Update(IconState::excluded(mstsc, InputMethod::Telex)),
            Call::Update(icon(Vietnamese, InputMethod::Telex)),
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
        assert_eq!(*reads.borrow(), [None, Some(0x1234)]);
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

/// A notice is open (the welcome box of the first run): the tray thread still handles
/// `TaskbarCreated`, a click, and `WM_ENDSESSION` or `WM_CLOSE`; the exit closes the open
/// notice and never waits for it (the smoke test of the first run, `taskkill` while the box
/// was open).
#[test]
fn messages_arriving_while_a_notice_is_open_are_handled_and_the_exit_closes_it() {
    for (label, last) in [
        ("test-tray-notice-session", (WM_ENDSESSION, 1)),
        ("test-tray-notice-close", (WM_CLOSE, 0)),
    ] {
        on_fresh_thread(move || {
            let log = Log::default();
            let welcome = Notice {
                kind: NoticeKind::Information,
                text: "chào".to_owned(),
                question: None,
            };
            let mut start = start(Vietnamese);
            start.notices = vec![welcome.clone()];
            let (services, _feed) = FakeServices::new(&log);
            let window = window_for(
                label,
                Tray::new(start, messages()),
                FakeShell::new(&log),
                services,
                NO_TICKS,
            );
            post(&window, taskbar_created(), 0, 0);
            post_icon(&window, NIN_SELECT, 0, 0);
            post(&window, last.0, last.1, 0);
            assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE, "{label}");
            let mut expected = vec![
                Call::Show(icon(Vietnamese, InputMethod::Telex)),
                Call::Notify(welcome),
                Call::Show(icon(Vietnamese, InputMethod::Telex)),
                Call::Update(icon(English, InputMethod::Telex)),
                Call::HookMode(false),
                Call::Send(1, WorkerCommand::Apply(Change::VietnameseMode(false))),
            ];
            expected.extend(exit_calls());
            assert_eq!(log.take(), expected, "{label}");
        });
    }
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

/// From the temporary folder, "Khởi động cùng Windows" explains and sends nothing.
#[test]
fn startup_from_the_temporary_folder_is_refused_in_the_menu() {
    on_fresh_thread(|| {
        let log = Log::default();
        let shell = FakeShell::new(&log).choosing(&[Some(TrayCommand::Startup.id())]);
        let (services, _feed) = FakeServices::new(&log);
        let mut start = start(Vietnamese);
        start.temporary_exe = true;
        let window = window_for(
            "test-tray-temporary",
            Tray::new(start, messages()),
            shell,
            services,
            NO_TICKS,
        );
        post_icon(&window, WM_CONTEXTMENU, 1, 1);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let calls = log.take();
        assert_eq!(calls[2], Call::Notify(temporary_exe_notice()), "{calls:?}");
        assert!(
            !calls.iter().any(|call| matches!(call, Call::Send(..))),
            "{calls:?}"
        );
    });
}

/// While the menu's modal loop runs inside the handler, the window's messages go to
/// `DefWindowProcW`, which answers a `WM_CLOSE` (here sent by the fake menu, as `taskkill`
/// would post one) by destroying the window. The tray still stops the hook, closes the
/// notices, finishes the worker, removes its icon, and ends the loop once the handler
/// returns and is freed.
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
/// Each tick also checks the typing hook first (Windows removes a slow low-level hook without
/// telling it, and `TypingHook::ensure_alive` installs it again, design §21.2), gives the tray
/// the user's double-click time again (D46), and makes sure the icon is shown.
#[test]
fn the_poll_timer_asks_the_worker_to_poll_again_and_again() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let services = services.on_send(|seq, _command, feed| {
            feed.send(Event::Worker(Answer {
                seq,
                outcome: Outcome::Polled(PollReport::default()),
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
        let shown = Call::Ensure(icon(Vietnamese, InputMethod::Telex));
        assert_eq!(
            log.take(),
            [
                Call::Show(icon(Vietnamese, InputMethod::Telex)),
                Call::KeepTypingAlive,
                Call::DoubleClickTime,
                shown.clone(),
                Call::Send(1, WorkerCommand::Poll),
                Call::KeepTypingAlive,
                Call::DoubleClickTime,
                shown.clone(),
                Call::Send(2, WorkerCommand::Poll),
                Call::KeepTypingAlive,
                Call::DoubleClickTime,
                shown,
                Call::Send(3, WorkerCommand::Poll),
            ]
        );
        assert_eq!(POLL_INTERVAL_MS, 2_000);
    });
}

/// The real shell reads the application of a window: the executable of its process, and the
/// class it is known by (`ForegroundApp::new`: the focus in the window's thread, else its own;
/// this thread never took the focus, and nothing here takes it); nothing for a window that is
/// gone or for none. Its double-click time is the user's.
#[test]
fn the_shell_reads_the_application_of_a_window() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let window = tray_window(
            "test-tray-read-app",
            Vietnamese,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        let hwnd = window.hwnd().0 as isize;
        let area = FakeArea {
            log: AreaLog::default(),
            adds: VecDeque::new(),
            modifies: VecDeque::new(),
        };
        let (inbox, _events) = mpsc::channel();
        let mut shell = ShellTray::with_area(area, inbox, WakeTarget::default());
        let test_exe = std::env::current_exe().unwrap();
        let test_exe = test_exe
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_lowercase();
        let app = shell.foreground_app(Some(hwnd)).unwrap();
        assert_eq!(app.exe.as_str(), test_exe);
        assert!(app.window_class.is_some());
        assert_eq!(app.window_class, window_class_name(hwnd));
        drop(window);
        assert_eq!(
            shell.foreground_app(Some(hwnd)),
            None,
            "a window that is gone"
        );
        assert_eq!(shell.foreground_app(Some(0)), None);
        assert_eq!(shell.double_click_ms(), double_click_time());
    });
}

/// The shell of the tray over a notification area that refuses the first add (no taskbar
/// yet, as for the elevated logon task): the next tick adds the icon again, and only once.
#[test]
fn an_icon_that_could_not_be_added_is_added_at_the_next_tick() {
    on_fresh_thread(|| {
        let area_log = AreaLog::default();
        let area = FakeArea {
            log: Rc::clone(&area_log),
            adds: VecDeque::from([false]),
            modifies: VecDeque::new(),
        };
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let services = services.on_send(|seq, _command, feed| {
            feed.send(Event::Worker(Answer {
                seq,
                outcome: Outcome::Polled(PollReport::default()),
                state: None,
            }))
            .unwrap();
            if seq == 3 {
                post_quit(5);
            }
        });
        let (inbox, _answers) = mpsc::channel();
        let shell = ShellTray::with_area(area, inbox, Default::default());
        let _window = window_for(
            "test-tray-icon-retry",
            tray(Vietnamese),
            shell,
            services,
            10,
        );
        assert_eq!(run_loop_with_watchdog(), 5);
        let state = icon(Vietnamese, InputMethod::Telex);
        assert_eq!(
            area_log.take(),
            [AreaCall::Add(state.clone()), AreaCall::Add(state)],
            "added at START (refused), added again at the first tick, then left alone"
        );
    });
}

/// A change of the icon that the shell refused (Explorer went away without
/// `TaskbarCreated`): the next tick adds the icon again with the current state.
#[test]
fn an_icon_whose_change_failed_is_added_again_at_the_next_tick() {
    on_fresh_thread(|| {
        let area_log = AreaLog::default();
        let area = FakeArea {
            log: Rc::clone(&area_log),
            adds: VecDeque::new(),
            modifies: VecDeque::from([false]),
        };
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let services = services.on_send(|seq, command, feed| {
            if command == WorkerCommand::Poll {
                feed.send(Event::Worker(Answer {
                    seq,
                    outcome: Outcome::Polled(PollReport::default()),
                    state: None,
                }))
                .unwrap();
                post_quit(6);
            }
        });
        let (inbox, _answers) = mpsc::channel();
        let shell = ShellTray::with_area(area, inbox, Default::default());
        let window = window_for(
            "test-tray-icon-modify",
            tray(Vietnamese),
            shell,
            services,
            50,
        );
        post_icon(&window, NIN_SELECT, 0, 0);
        assert_eq!(run_loop_with_watchdog(), 6);
        let english = icon(English, InputMethod::Telex);
        assert_eq!(
            area_log.take(),
            [
                AreaCall::Add(icon(Vietnamese, InputMethod::Telex)),
                AreaCall::Modify(english.clone()),
                AreaCall::Add(english),
            ]
        );
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

/// Hotkey changes waiting together in the inbox count as the last one: one icon change, one
/// mode for the hook, one save.
#[test]
fn hotkey_changes_waiting_together_are_coalesced() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, feed) = FakeServices::new(&log);
        for vietnamese in [false, true, false] {
            feed.send(Event::Hotkey(vietnamese)).unwrap();
        }
        let window = tray_window(
            "test-tray-coalesce",
            Vietnamese,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
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

/// Hotkey changes the hook made just before it stopped are saved at the exit, before the
/// worker finishes.
#[test]
fn hotkey_changes_left_at_the_exit_are_saved_before_the_worker_finishes() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let services = services.on_stop(vec![Event::Hotkey(false), Event::Hotkey(true)]);
        let window = window_for(
            "test-tray-flush",
            tray(English),
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        assert_eq!(
            log.take(),
            [
                Call::Show(icon(English, InputMethod::Telex)),
                Call::StopTyping,
                Call::Dismiss,
                Call::ClosePanel,
                Call::CloseMacroList,
                Call::CloseExclusions,
                Call::Send(1, WorkerCommand::Apply(Change::VietnameseMode(true))),
                Call::FinishWorker,
                Call::Remove,
            ]
        );
    });
}

/// A save error is shown in a message box; the "yes" to its recovery question comes back
/// through the inbox and goes to the worker.
#[test]
fn a_notice_is_shown_and_yes_to_its_question_reaches_the_worker() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, feed) = FakeServices::new(&log);
        let shell = FakeShell::new(&log).answering(&[true], &feed);
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

/// An elevated controller lets `TaskbarCreated` (matrix #58), the icon's callback, and the
/// show-panel request of a second start (D45) through UIPI, which a non-elevated Explorer and
/// a start from a normal prompt need to reach it; a non-elevated controller leaves its filter
/// alone. This non-elevated test reads the filter of each test window back.
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
        for message in [taskbar_created(), test_show_panel(), TRAY_CALLBACK.id()] {
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

// --- The settings panel (D43, D45) ---------------------------------------------------------

/// The panel state of a tray built by [`start`] in `mode`.
fn panel_state(mode: InputMode) -> PanelState {
    tray(mode).panel_state()
}

/// "Bảng cấu hình…" opens the panel; its requests come through the inbox, go the way of the
/// menu, and refresh it; its close stops the refreshes; the show-panel request of a second
/// start (posted to this window only) opens it again; the exit closes it.
#[test]
fn the_panel_opens_from_the_menu_and_a_second_start_and_its_requests_act() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, feed) = FakeServices::new(&log);
        let shell = FakeShell::new(&log)
            .choosing(&[Some(TrayCommand::Panel.id())])
            .panel_sending(
                vec![
                    Event::Panel(PanelRequest::Change(Change::SpellCheck(false))),
                    Event::Panel(PanelRequest::MacroList),
                    Event::PanelClosed(1),
                ],
                &feed,
            );
        let window = tray_window("test-tray-panel", Vietnamese, shell, services, NO_TICKS);
        post_icon(&window, WM_CONTEXTMENU, 5, 6);
        post_icon(&window, NIN_SELECT, 0, 0);
        post(&window, test_show_panel(), 0, 0);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let calls = log.take();
        assert!(
            matches!(calls[1], Call::Menu { x: 5, y: 6, .. }),
            "{calls:?}"
        );
        let opened = panel_state(Vietnamese);
        let mut no_spelling = runtime(Vietnamese);
        no_spelling.options.spell_check = false;
        let mut refreshed = opened.clone();
        refreshed.spell_check = false;
        let mut english = refreshed.clone();
        english.vietnamese = false;
        let mut expected = vec![
            Call::OpenPanel(1, Box::new(opened)),
            Call::HookRuntime(Box::new(no_spelling)),
            Call::Send(1, WorkerCommand::Apply(Change::SpellCheck(false))),
            Call::RefreshPanel(Box::new(refreshed.clone())),
            Call::MacroList(PathBuf::from(r"C:\Raumakey")),
            Call::RefreshPanel(Box::new(refreshed)),
            // Closed: the click is not shown to it.
            Call::Update(icon(English, InputMethod::Telex)),
            Call::HookMode(false),
            Call::Send(2, WorkerCommand::Apply(Change::VietnameseMode(false))),
            Call::OpenPanel(2, Box::new(english)),
        ];
        expected.extend(exit_calls());
        assert_eq!(calls[2..], expected);
    });
}

/// With `show_panel_on_start` the panel opens once the icon is added.
#[test]
fn the_panel_opens_at_start_after_the_icon() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, _feed) = FakeServices::new(&log);
        let mut start = start(English);
        start.show_panel = true;
        let state = Tray::new(start.clone(), messages()).panel_state();
        assert!(state.show_on_start);
        let window = window_for(
            "test-tray-panel-start",
            Tray::new(start, messages()),
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![
            Call::Show(icon(English, InputMethod::Telex)),
            Call::OpenPanel(1, Box::new(state)),
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
    });
}

/// The ids of the panel's check boxes and what a state shows in each.
fn checks_of(state: &PanelState) -> Vec<(u16, bool)> {
    state.view().checks
}

const CHECK_BOX_IDS: [u16; 10] = [
    ID_VIETNAMESE,
    ID_MODERN_TONE,
    ID_FREE_MARKING,
    ID_SPELL_CHECK,
    ID_AUTO_RESTORE,
    ID_MACROS,
    ID_MACROS_WHEN_ENGLISH,
    ID_MACRO_AUTO_CASE,
    ID_STARTUP,
    ID_SHOW_ON_START,
];

/// How long a test waits for the panel's thread.
const PANEL_WAIT: Duration = Duration::from_secs(10);

/// Sends `message` to a window of another thread and returns the answer; fails the test when
/// the thread does not answer in time.
fn send_to(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    let mut result = 0_usize;
    // SAFETY: the pointers some callers pass in `lparam` point to data of this thread that
    // outlives the call (the receiver is in this process); `result` is a valid out pointer.
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

fn item(dialog: HWND, id: u16) -> HWND {
    // SAFETY: only reads the dialog's list of children.
    unsafe { GetDlgItem(Some(dialog), i32::from(id)) }.unwrap()
}

fn window_text(hwnd: HWND) -> String {
    let length = usize::try_from(send_to(hwnd, WM_GETTEXTLENGTH, 0, 0)).unwrap();
    let mut buffer = vec![0_u16; length + 1];
    let copied = send_to(hwnd, WM_GETTEXT, buffer.len(), buffer.as_mut_ptr() as isize);
    String::from_utf16(&buffer[..usize::try_from(copied).unwrap()]).unwrap()
}

fn is_enabled(hwnd: HWND) -> bool {
    // SAFETY: only reads the window's state.
    unsafe { IsWindowEnabled(hwnd) }.as_bool()
}

fn is_window(hwnd: HWND) -> bool {
    // SAFETY: only queries whether the handle names a window.
    unsafe { IsWindow(Some(hwnd)) }.as_bool()
}

/// What the panel's check boxes show, in the order of [`CHECK_BOX_IDS`].
fn shown_checks(panel: HWND) -> Vec<(u16, bool)> {
    CHECK_BOX_IDS
        .iter()
        .map(|&id| (id, send_to(item(panel, id), BM_GETCHECK, 0, 0) == 1))
        .collect()
}

/// What the enumeration of [`dialog_windows`] collects.
struct Found {
    process: u32,
    title: Vec<u16>,
    windows: Vec<HWND>,
}

/// The top-level dialog windows of this process with the panel's title (the tests that show a
/// real panel take turns, [`real_shell`]).
fn panel_windows() -> Vec<HWND> {
    dialog_windows(&panel_title())
}

/// The top-level dialog windows of this process titled `title`. Neither call sends a message
/// to the window, so a dialog whose `init` still runs is found as well.
fn dialog_windows(title: &str) -> Vec<HWND> {
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the `Found` of the enumeration below, which outlives it.
        let found = unsafe { &mut *(lparam.0 as *mut Found) };
        let mut process = 0_u32;
        // SAFETY: only reads the window's process id into a valid out pointer.
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut process)) };
        let mut class = [0_u16; 16];
        // SAFETY: the buffer is writable for its length.
        let class_len = unsafe { GetClassNameW(hwnd, &mut class) };
        let mut title = [0_u16; 128];
        // SAFETY: the buffer is writable for its length; no message is sent.
        let title_len = unsafe { InternalGetWindowText(hwnd, &mut title) };
        let class = String::from_utf16_lossy(&class[..usize::try_from(class_len).unwrap()]);
        if process == found.process
            && class == "#32770"
            && title[..usize::try_from(title_len).unwrap()] == found.title[..]
        {
            found.windows.push(hwnd);
        }
        BOOL::from(true)
    }
    let mut found = Found {
        // SAFETY: no preconditions.
        process: unsafe { GetCurrentProcessId() },
        title: title.encode_utf16().collect(),
        windows: Vec::new(),
    };
    // SAFETY: the callback runs during the call only and gets a pointer to `found`, which
    // outlives the call.
    unsafe { EnumWindows(Some(each), LPARAM(&raw mut found as isize)) }.unwrap();
    found.windows
}

/// Waits until `done` holds; fails the test after [`PANEL_WAIT`].
fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + PANEL_WAIT;
    while !done() {
        assert!(Instant::now() < deadline, "{what} in time");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Waits for the one panel window of this process and for its `init` to have returned (the
/// dialog's modal loop answers the sent `WM_NULL` only then).
fn wait_for_panel() -> HWND {
    wait_for_dialog(&panel_title())
}

/// Waits for the one dialog of this process titled `title` and for its `init` to have
/// returned.
fn wait_for_dialog(title: &str) -> HWND {
    let mut dialog = None;
    wait_until("the dialog came up", || {
        dialog = dialog_windows(title).first().copied();
        dialog.is_some()
    });
    let dialog = dialog.unwrap();
    send_to(dialog, WM_NULL, 0, 0);
    dialog
}

/// The next event of the panel, in time.
fn next_event(events: &mpsc::Receiver<Event>) -> Event {
    events.recv_timeout(PANEL_WAIT).expect("an event in time")
}

/// The tests that show real dialogs take turns: [`panel_windows`] and [`dialog_windows`] find
/// them by their titles among all windows of this process.
static REAL_DIALOGS: Mutex<()> = Mutex::new(());

/// A real shell with the test dialogs (off every monitor, never activated), for one test at a
/// time. Dropped, also when the test failed, it closes its panel and its editor and waits (for
/// a bounded time) until they are gone, so the next test finds no window of this one.
struct RealShell {
    shell: ShellTray<FakeArea>,
    _turn: MutexGuard<'static, ()>,
}

impl core::ops::Deref for RealShell {
    type Target = ShellTray<FakeArea>;

    fn deref(&self) -> &Self::Target {
        &self.shell
    }
}

impl core::ops::DerefMut for RealShell {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shell
    }
}

impl Drop for RealShell {
    fn drop(&mut self) {
        self.shell.close_panel();
        self.shell.close_macro_list();
        self.shell.close_exclusions();
        let deadline = Instant::now() + PANEL_WAIT;
        while (!panel_windows().is_empty()
            || !dialog_windows(macro_dialog::TITLE).is_empty()
            || !dialog_windows(exclusions_dialog::TITLE).is_empty())
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

/// The real shell of a test, once it is this test's turn, answering into `inbox`.
fn real_shell(inbox: mpsc::Sender<Event>) -> RealShell {
    let turn = REAL_DIALOGS.lock().unwrap_or_else(PoisonError::into_inner);
    let area = FakeArea {
        log: AreaLog::default(),
        adds: VecDeque::new(),
        modifies: VecDeque::new(),
    };
    RealShell {
        shell: ShellTray::with_area(area, inbox, WakeTarget::default()).with_off_screen_dialogs(),
        _turn: turn,
    }
}

// `DrawTextW` with an explicit buffer: the `windows` crate passes the length of a slice as the
// text's length, while `DT_MODIFYSTRING` needs four more units of room after the text.
#[link(name = "user32")]
unsafe extern "system" {
    #[link_name = "DrawTextW"]
    fn draw_text(dc: HDC, text: *mut u16, length: i32, rect: *mut RECT, format: u32) -> i32;
}

/// Runs `body` with a device context of `control` that has the control's font selected.
fn with_font_of<T>(control: HWND, body: impl FnOnce(HDC) -> T) -> T {
    let font = send_to(control, WM_GETFONT, 0, 0);
    // SAFETY: a device context of a window of this process, released below; the font is the
    // control's own, which outlives the call, and it is deselected before the release.
    unsafe {
        let dc = GetDC(Some(control));
        let previous = SelectObject(dc, HGDIOBJ(font as *mut core::ffi::c_void));
        let result = body(dc);
        SelectObject(dc, previous);
        ReleaseDC(Some(control), dc);
        result
    }
}

/// The text of the label `control` as it draws it with `SS_PATHELLIPSIS`: one line in its
/// client width, a path that does not fit shortened in its middle (`DT_PATH_ELLIPSIS`).
fn path_as_shown(control: HWND) -> String {
    let text = window_text(control);
    let mut client = RECT::default();
    // SAFETY: `client` is a valid out buffer.
    unsafe { GetClientRect(control, &mut client) }.unwrap();
    with_font_of(control, |dc| {
        let mut buffer: Vec<u16> = text.encode_utf16().collect();
        let length = i32::try_from(buffer.len()).unwrap();
        // Room for the four units `DT_MODIFYSTRING` may add, and the NUL after them.
        buffer.extend([0; 5]);
        let mut rect = client;
        let format = DT_SINGLELINE | DT_PATH_ELLIPSIS | DT_MODIFYSTRING | DT_NOPREFIX;
        // SAFETY: the buffer holds `length` units of text and five more, as the call may write
        // four units more and a NUL; `rect` is a valid in/out rectangle.
        unsafe { draw_text(dc, buffer.as_mut_ptr(), length, &mut rect, format.0) };
        let end = buffer.iter().position(|&unit| unit == 0).unwrap();
        String::from_utf16(&buffer[..end]).unwrap()
    })
}

/// How many pixels the label `control` needs for its text, wrapped at its client width, and
/// how many it has.
fn wrapped_height(control: HWND) -> (i32, i32) {
    let mut text: Vec<u16> = window_text(control).encode_utf16().collect();
    let mut client = RECT::default();
    // SAFETY: `client` is a valid out buffer.
    unsafe { GetClientRect(control, &mut client) }.unwrap();
    let needed = with_font_of(control, |dc| {
        let mut rect = RECT {
            right: client.right,
            ..RECT::default()
        };
        let length = i32::try_from(text.len()).unwrap();
        let format = DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX;
        // SAFETY: the buffer holds `length` units, which the call only reads; `rect` is a
        // valid in/out rectangle.
        unsafe { draw_text(dc, text.as_mut_ptr(), length, &mut rect, format.0) };
        rect.bottom
    });
    (needed, client.bottom)
}

/// The ids of the controls that the arrow keys reach from control `id` of `dialog`, in both
/// directions (`GetNextDlgGroupItem`, which `IsDialogMessageW` follows), without `id` itself.
fn arrow_targets(dialog: HWND, id: u16) -> Vec<i32> {
    let from = item(dialog, id);
    let mut targets = Vec::new();
    for previous in [false, true] {
        let mut at = from;
        for _ in 0..32 {
            // SAFETY: only reads the dialog's list of children.
            at = unsafe { GetNextDlgGroupItem(dialog, Some(at), previous) }.unwrap();
            if at == from {
                break;
            }
            // SAFETY: only reads the control's id.
            let target = unsafe { GetDlgCtrlID(at) };
            if !targets.contains(&target) {
                targets.push(target);
            }
        }
    }
    targets.sort_unstable();
    targets
}

/// The real panel, through the real shell (off every monitor, never activated): it shows the
/// state it was opened with; clicks and a new selection become requests in the inbox; a
/// refresh puts every control to the new state (a refused click is put back, "Kiểu gõ riêng"
/// appears, the footer and the disabled buttons of a run without a folder); opening it again
/// shows the new state in the same window; "Đóng" closes it with its latest number; the exit
/// closes a panel without waiting.
#[test]
fn the_real_panel_shows_the_state_sends_requests_and_closes() {
    let (inbox, events) = mpsc::channel();
    let mut shell = real_shell(inbox);
    let state = panel_state(Vietnamese);
    shell.open_panel(1, state.clone());
    let panel = wait_for_panel();
    // The controls show the state.
    assert_eq!(shown_checks(panel), checks_of(&state));
    let methods = item(panel, ID_METHOD);
    assert_eq!(send_to(methods, CB_GETCOUNT, 0, 0), 3);
    assert_eq!(send_to(methods, CB_GETCURSEL, 0, 0), 0);
    assert_eq!(window_text(item(panel, ID_HINT)), state.hint());
    assert_eq!(
        window_text(item(panel, ID_CONFIG_FILE)),
        state.config_file()
    );
    assert_eq!(
        window_text(item(panel, ID_CONFIG_LOCATION)),
        state.config_location()
    );
    assert!(is_enabled(item(panel, ID_OPEN_CONFIG)));
    assert!(is_enabled(item(panel, ID_MACRO_LIST)));
    // Off every monitor.
    let mut frame = RECT::default();
    // SAFETY: `frame` is a valid out buffer.
    unsafe { GetWindowRect(panel, &mut frame) }.unwrap();
    assert!(frame.right < 0 && frame.bottom < 0, "{frame:?}");

    // Clicks and a selection become requests with the new values.
    send_to(item(panel, ID_SPELL_CHECK), BM_CLICK, 0, 0);
    assert_eq!(
        next_event(&events),
        Event::Panel(PanelRequest::Change(Change::SpellCheck(false)))
    );
    send_to(item(panel, ID_STARTUP), BM_CLICK, 0, 0);
    assert_eq!(
        next_event(&events),
        Event::Panel(PanelRequest::Startup(true))
    );
    send_to(methods, CB_SETCURSEL, 2, 0);
    let wparam = usize::from(ID_METHOD) | (usize::from(CBN_SELCHANGE) << 16);
    send_to(panel, WM_COMMAND, wparam, methods.0 as isize);
    assert_eq!(
        next_event(&events),
        Event::Panel(PanelRequest::Change(Change::InputMethod(InputMethod::Viqr)))
    );
    send_to(item(panel, ID_OPEN_CONFIG), BM_CLICK, 0, 0);
    assert_eq!(next_event(&events), Event::Panel(PanelRequest::OpenConfig));
    send_to(item(panel, ID_MACRO_LIST), BM_CLICK, 0, 0);
    assert_eq!(next_event(&events), Event::Panel(PanelRequest::MacroList));
    assert!(is_enabled(item(panel, ID_EXCLUSIONS)));
    send_to(item(panel, ID_EXCLUSIONS), BM_CLICK, 0, 0);
    assert_eq!(next_event(&events), Event::Panel(PanelRequest::Exclusions));
    // A command of a label asks nothing.
    let wparam = usize::from(ID_HINT) | (usize::from(BN_CLICKED) << 16);
    send_to(panel, WM_COMMAND, wparam, 0);
    assert!(events.try_recv().is_err());

    // A refresh: the start with Windows was refused (put back), the spell check was taken,
    // a user method with its key map, no hotkey, and no folder.
    let mut refreshed = state.clone();
    refreshed.spell_check = false;
    refreshed.method = InputMethod::UserDefined;
    refreshed.user_defined_listed = true;
    refreshed.toggle_hotkey = None;
    refreshed.location = Location::Unavailable;
    refreshed.config_path = None;
    shell.refresh_panel(refreshed.clone());
    wait_until("the refresh was shown", || {
        send_to(methods, CB_GETCOUNT, 0, 0) == 4
    });
    assert_eq!(shown_checks(panel), checks_of(&refreshed));
    assert_eq!(send_to(item(panel, ID_STARTUP), BM_GETCHECK, 0, 0), 0);
    assert_eq!(send_to(methods, CB_GETCURSEL, 0, 0), 3);
    assert_eq!(
        window_text(item(panel, ID_HINT)),
        "Bật/tắt nhanh: nhấp biểu tượng V/E"
    );
    assert_eq!(
        window_text(item(panel, ID_CONFIG_FILE)),
        "Tệp cấu hình: không có, nên các thay đổi không được lưu."
    );
    assert_eq!(window_text(item(panel, ID_CONFIG_LOCATION)), "");
    assert!(!is_enabled(item(panel, ID_OPEN_CONFIG)));
    assert!(!is_enabled(item(panel, ID_MACRO_LIST)));
    // Without a folder the edits of the exclusions dialog take effect in memory.
    assert!(is_enabled(item(panel, ID_EXCLUSIONS)));

    // Opened again: the same window shows the new state (an off-screen panel is never
    // brought to the front).
    let mut again = refreshed.clone();
    again.vietnamese = false;
    again.method = InputMethod::Vni;
    again.user_defined_listed = false;
    shell.open_panel(2, again.clone());
    wait_until("the second opening was shown", || {
        send_to(methods, CB_GETCOUNT, 0, 0) == 3
    });
    assert_eq!(shown_checks(panel), checks_of(&again));
    assert_eq!(send_to(methods, CB_GETCURSEL, 0, 0), 1);
    assert_eq!(panel_windows(), [panel]);

    // "Đóng" closes it; the close names the latest opening.
    send_to(item(panel, ID_CLOSE), BM_CLICK, 0, 0);
    assert_eq!(next_event(&events), Event::PanelClosed(2));
    wait_until("the panel was destroyed", || !is_window(panel));
    // Refreshing or closing a panel that is gone does nothing.
    shell.refresh_panel(state.clone());
    shell.close_panel();
    assert!(events.try_recv().is_err());

    // A new opening shows a new panel; the exit closes it without waiting.
    shell.open_panel(3, state.clone());
    let panel = wait_for_panel();
    assert_eq!(shown_checks(panel), checks_of(&state));
    shell.close_panel();
    assert_eq!(next_event(&events), Event::PanelClosed(3));
    wait_until("the panel was destroyed", || !is_window(panel));
    assert_eq!(panel_windows(), []);
}

/// The real panel at its font, under Common Controls 6 as `raumakey.exe` shows it: the path
/// label keeps the file name when it shortens a long path, also for `%LOCALAPPDATA%`; the
/// location note (with its reason) fits its label; and the arrow keys move from a check box
/// only to the check boxes of its own group, never to the method list or a button.
#[test]
fn the_real_panel_keeps_the_file_name_and_the_arrow_keys_in_their_groups() {
    let (inbox, events) = mpsc::channel();
    let mut shell = real_shell(inbox);
    let mut state = panel_state(Vietnamese);
    state.location = Location::LocalAppData;
    state.config_path = Some(PathBuf::from(
        r"C:\Users\Nguyen Van Anh\AppData\Local\Raumakey\raumakey.toml",
    ));
    {
        // The panel's thread inherits the context; the guard may go once it started.
        let _v6 = CommonControls6::activate();
        shell.open_panel(1, state.clone());
    }
    let panel = wait_for_panel();

    let groups: [&[u16]; 4] = [
        &[ID_VIETNAMESE],
        &[
            ID_MODERN_TONE,
            ID_FREE_MARKING,
            ID_SPELL_CHECK,
            ID_AUTO_RESTORE,
        ],
        &[ID_MACROS, ID_MACROS_WHEN_ENGLISH, ID_MACRO_AUTO_CASE],
        &[ID_STARTUP, ID_SHOW_ON_START],
    ];
    for group in groups {
        for &id in group {
            let mut expected: Vec<i32> = group
                .iter()
                .filter(|&&other| other != id)
                .map(|&other| i32::from(other))
                .collect();
            expected.sort_unstable();
            assert_eq!(arrow_targets(panel, id), expected, "from {id}");
        }
    }

    let footer = item(panel, ID_CONFIG_FILE);
    for (location, path, shortened) in [
        (
            Location::LocalAppData,
            r"C:\Users\Nguyen Van Anh\AppData\Local\Raumakey\raumakey.toml",
            false,
        ),
        (
            Location::LocalAppData,
            concat!(
                r"C:\Users\Nguyen Van Anh Tuan.CONTOSO-VIETNAM-HANOI",
                r"\AppData\Local\Raumakey\raumakey.toml"
            ),
            true,
        ),
        (
            Location::ExeDir,
            concat!(
                r"C:\Users\Nguyen Van Anh\Downloads",
                r"\raumakey-0.1.0-x86_64-pc-windows-msvc\raumakey-0.1.0\raumakey.toml"
            ),
            true,
        ),
    ] {
        state.location = location;
        state.config_path = Some(PathBuf::from(path));
        shell.refresh_panel(state.clone());
        wait_until("the refresh was shown", || {
            window_text(footer) == state.config_file()
        });
        let shown = path_as_shown(footer);
        assert!(shown.ends_with(r"\raumakey.toml"), "{shown}");
        assert_eq!(shown.contains("..."), shortened, "{shown}");
        let note = item(panel, ID_CONFIG_LOCATION);
        // Wrapped at spaces by the panel.
        assert_eq!(
            window_text(note).replace('\n', " "),
            state.config_location()
        );
        let (needed, height) = wrapped_height(note);
        assert!(needed <= height, "{location:?}: {needed} > {height}");
    }

    shell.close_panel();
    assert_eq!(next_event(&events), Event::PanelClosed(1));
    wait_until("the panel was destroyed", || !is_window(panel));
}

/// The child windows of `dialog`.
fn children(dialog: HWND) -> Vec<HWND> {
    unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
        // SAFETY: `lparam` is the vector of the enumeration below, which outlives it.
        let found = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
        found.push(hwnd);
        BOOL::from(true)
    }
    let mut found: Vec<HWND> = Vec::new();
    // SAFETY: the callback runs during the call only and gets a pointer to `found`, which
    // outlives the call.
    let _ = unsafe { EnumChildWindows(Some(dialog), Some(each), LPARAM(&raw mut found as isize)) };
    found
}

/// The width of `line` drawn on one line with the font of `control`.
fn single_line_width(control: HWND, line: &str) -> i32 {
    let mut units: Vec<u16> = line.encode_utf16().collect();
    if units.is_empty() {
        return 0;
    }
    with_font_of(control, |dc| {
        let mut rect = RECT::default();
        let length = i32::try_from(units.len()).unwrap();
        let format = DT_CALCRECT | DT_SINGLELINE | DT_NOPREFIX;
        // SAFETY: the buffer holds `length` units, which the call only reads; `rect` is a
        // valid in/out rectangle.
        unsafe { draw_text(dc, units.as_mut_ptr(), length, &mut rect, format.0) };
        rect.right
    })
}

/// The pixels a push button keeps clear around its text, on each side: its edge and the
/// focus rectangle inside it.
const BUTTON_TEXT_MARGIN: i32 = 4;

/// What is wrong with the text of `control` when it is a push button, a check box, or a label
/// that wraps (not one that shortens its text on one line): for a push button, a text that
/// does not fit on its one line inside the button's edges; for the others, each line that
/// does not fit the control's line width (`line_width`: the client width, less a check box's
/// box) on one line, which Windows would wrap, possibly inside a word, and lines that need
/// more height than the control has. `None` for other controls.
fn text_overflow(control: HWND) -> Option<Vec<String>> {
    let mut class = [0_u16; 16];
    // SAFETY: the buffer is writable for its length.
    let length = unsafe { GetClassNameW(control, &mut class) };
    let class = String::from_utf16_lossy(&class[..usize::try_from(length).unwrap()]);
    // SAFETY: only reads the window's style.
    let style = unsafe { GetWindowLongW(control, GWL_STYLE) } as u32;
    let push_button = class == "Button" && matches!(style & 0x0F, 0 | 1);
    let check_box = class == "Button" && matches!(style & 0x0F, 2..=6 | 9);
    let shortens = style & (styles::SS_PATHELLIPSIS | styles::SS_ENDELLIPSIS) != 0;
    // SAFETY: only reads the control's id.
    let id = unsafe { GetDlgCtrlID(control) };
    if push_button {
        let text = window_text(control);
        let mut client = RECT::default();
        // SAFETY: `client` is a valid out buffer.
        unsafe { GetClientRect(control, &mut client) }.unwrap();
        let width = single_line_width(control, &text);
        let room = client.right - 2 * BUTTON_TEXT_MARGIN;
        let problems = if text.contains('\n') || width > room {
            vec![format!(
                "{id}: button {text:?} takes {width} pixels of {room}"
            )]
        } else {
            Vec::new()
        };
        return Some(problems);
    }
    if !check_box && !(class == "Static" && !shortens) {
        return None;
    }
    let limit = line_width(control).unwrap();
    let text = window_text(control);
    let mut problems = Vec::new();
    for line in text.split('\n') {
        let width = single_line_width(control, line);
        if width > limit {
            problems.push(format!("{id}: {line:?} takes {width} pixels of {limit}"));
        }
    }
    let mut client = RECT::default();
    // SAFETY: `client` is a valid out buffer.
    unsafe { GetClientRect(control, &mut client) }.unwrap();
    let mut units: Vec<u16> = text.encode_utf16().collect();
    if !units.is_empty() {
        let needed = with_font_of(control, |dc| {
            let mut rect = RECT {
                right: limit,
                ..RECT::default()
            };
            let length = i32::try_from(units.len()).unwrap();
            let format = DT_CALCRECT | DT_WORDBREAK | DT_NOPREFIX;
            // SAFETY: as in `single_line_width`.
            unsafe { draw_text(dc, units.as_mut_ptr(), length, &mut rect, format.0) };
            rect.bottom
        });
        if needed > client.bottom {
            problems.push(format!(
                "{id}: {text:?} needs {needed} pixels of height, has {}",
                client.bottom
            ));
        }
    }
    Some(problems)
}

/// The real panel, opened with the long note of `%LOCALAPPDATA%`, the real macro list editor,
/// and the real exclusions dialog, without a folder (its note shown), under Common Controls 6
/// as `raumakey.exe` runs them: every check box and every label that wraps shows lines that
/// each fit its line width on one line, so Windows has nothing left to wrap and cuts no
/// Vietnamese word ("…không phải ti" / "ếng Việt"), and is tall enough for them; and the text
/// of every push button, "Ứng dụng loại trừ…" of the panel among them, fits on its one line.
#[test]
fn the_real_panel_and_editor_leave_windows_nothing_to_wrap() {
    let folder = tempfile::tempdir().unwrap();
    let (inbox, events) = mpsc::channel();
    let mut shell = real_shell(inbox);
    let mut state = panel_state(Vietnamese);
    state.location = Location::LocalAppData;
    state.config_path = Some(PathBuf::from(
        r"C:\Users\Nguyen Van Anh\AppData\Local\Raumakey\raumakey.toml",
    ));
    let exclusions = ExclusionsState {
        apps: default_apps(),
        can_save: false,
    };
    {
        // The dialogs' threads inherit the context; the guard may go once they started.
        let _v6 = CommonControls6::activate();
        shell.open_panel(1, state.clone());
        shell.open_macro_list(folder.path().to_path_buf());
        shell.open_exclusions(1, exclusions);
    }
    let panel = wait_for_panel();
    let editor = wait_for_dialog(macro_dialog::TITLE);
    let excluded = wait_for_dialog(exclusions_dialog::TITLE);
    assert_eq!(
        window_text(item(panel, ID_CONFIG_LOCATION)).replace('\n', " "),
        state.config_location()
    );
    assert_eq!(
        window_text(item(excluded, exclusions_dialog::ID_NOT_SAVED)).replace('\n', " "),
        exclusions_dialog::NOT_SAVED
    );
    let mut checked = 0;
    let mut found = Vec::new();
    for dialog in [panel, editor, excluded] {
        for control in children(dialog) {
            if let Some(problems) = text_overflow(control) {
                checked += 1;
                found.extend(problems);
            }
        }
    }
    assert_eq!(found, Vec::<String>::new());
    // The panel's ten check boxes, three labels, and five push buttons; the editor's five
    // labels and four push buttons; the exclusions dialog's three labels and six push buttons.
    assert_eq!(checked, 18 + 5 + 4 + 3 + 6);

    shell.close_panel();
    assert_eq!(next_event(&events), Event::PanelClosed(1));
    shell.close_exclusions();
    assert_eq!(next_event(&events), Event::ExclusionsClosed(1));
    shell.close_macro_list();
    wait_until("the dialogs were destroyed", || {
        !is_window(panel) && !is_window(editor) && !is_window(excluded)
    });
}

/// Opened again at once after the user closed it (its dialog has ended, but its thread has
/// not marked it closed yet): a new panel opens, and only the close of the old opening is
/// reported, so the tray keeps the new one.
#[test]
fn a_panel_opened_again_while_it_closes_opens_a_new_one() {
    let (inbox, events) = mpsc::channel();
    let mut shell = real_shell(inbox);
    let state = panel_state(Vietnamese);
    shell.open_panel(1, state.clone());
    let panel = wait_for_panel();
    // The close box; the handler has agreed once this returns.
    send_to(panel, WM_CLOSE, 0, 0);
    shell.open_panel(2, state.clone());
    assert_eq!(next_event(&events), Event::PanelClosed(1));
    let mut again = None;
    wait_until("a new panel came up", || {
        again = panel_windows().into_iter().find(|&window| window != panel);
        again.is_some()
    });
    let again = again.unwrap();
    send_to(again, WM_NULL, 0, 0);
    assert_eq!(shown_checks(again), checks_of(&state));
    assert!(events.try_recv().is_err(), "the new panel stays open");
    shell.close_panel();
    assert_eq!(next_event(&events), Event::PanelClosed(2));
    wait_until("the panels were destroyed", || {
        !is_window(panel) && !is_window(again)
    });
    assert_eq!(panel_windows(), []);
}

/// The editor saved `raumakey-macros.toml` (its event comes through the inbox): the worker
/// polls at once.
#[test]
fn a_save_of_the_macro_list_makes_the_worker_poll() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, feed) = FakeServices::new(&log);
        feed.send(Event::MacrosSaved).unwrap();
        let window = tray_window(
            "test-tray-macros-saved",
            Vietnamese,
            FakeShell::new(&log),
            services,
            NO_TICKS,
        );
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let mut expected = vec![
            Call::Show(icon(Vietnamese, InputMethod::Telex)),
            Call::Send(1, WorkerCommand::Poll),
        ];
        expected.extend(exit_calls());
        assert_eq!(log.take(), expected);
    });
}

/// Types `text` into the edit `edit` as the user would (`WM_SETTEXT`).
fn type_into(edit: HWND, text: &str) {
    let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
    send_to(edit, WM_SETTEXT, 0, units.as_ptr() as isize);
}

/// The real macro list editor through the real shell (off every monitor, never activated, its
/// questions answered "no" without a box): one editor at a time, a save reaches the inbox as
/// `MacrosSaved`, the exit closes it without waiting, and once it is closed (by the exit or by
/// "Đóng"), the next opening shows a new one, also the moment its window is gone.
#[test]
fn the_real_macro_list_editor_opens_once_reports_saves_and_closes() {
    let folder = tempfile::tempdir().unwrap();
    let (inbox, events) = mpsc::channel();
    let mut shell = real_shell(inbox);
    let title = macro_dialog::TITLE;
    shell.open_macro_list(folder.path().to_path_buf());
    let editor = wait_for_dialog(title);
    let mut frame = RECT::default();
    // SAFETY: `frame` is a valid out buffer.
    unsafe { GetWindowRect(editor, &mut frame) }.unwrap();
    assert!(frame.right < 0 && frame.bottom < 0, "{frame:?}");
    // Opened again while it is open: the same editor (an off-screen one is never brought to
    // the front).
    shell.open_macro_list(folder.path().to_path_buf());
    assert_eq!(dialog_windows(title), [editor]);

    // A new row, then "Lưu": the file is written and the save reported.
    type_into(item(editor, macro_dialog::ID_TRIGGER), "rk");
    type_into(item(editor, macro_dialog::ID_REPLACEMENT), "Raumakey");
    send_to(item(editor, macro_dialog::ID_PUT), BM_CLICK, 0, 0);
    send_to(item(editor, macro_dialog::ID_SAVE), BM_CLICK, 0, 0);
    assert_eq!(next_event(&events), Event::MacrosSaved);
    assert!(folder.path().join("raumakey-macros.toml").is_file());

    // The exit closes it without waiting; its close is nothing the tray must hear of.
    shell.close_macro_list();
    wait_until("the editor was destroyed", || !is_window(editor));
    // A new opening shows a new editor; "Đóng" closes it (nothing unsaved, nothing asked).
    shell.open_macro_list(folder.path().to_path_buf());
    let editor = wait_for_dialog(title);
    send_to(item(editor, IDCANCEL), BM_CLICK, 0, 0);
    // Opened again the moment its window is gone, also before its thread has marked it
    // closed: a new editor opens.
    let deadline = Instant::now() + PANEL_WAIT;
    while is_window(editor) {
        assert!(Instant::now() < deadline, "the editor was closed in time");
        std::thread::yield_now();
    }
    shell.open_macro_list(folder.path().to_path_buf());
    let editor = wait_for_dialog(title);
    shell.close_macro_list();
    wait_until("the editor was destroyed", || !is_window(editor));
    assert_eq!(dialog_windows(title), []);
    // Nothing open: closing does nothing.
    shell.close_macro_list();
    assert!(events.try_recv().is_err());
}

// --- The exclusions dialog (D48) ---------------------------------------------------------------

/// The exclusions state of a tray built by [`start`].
fn exclusions_state() -> ExclusionsState {
    tray(Vietnamese).exclusions_state()
}

/// "Ứng dụng loại trừ…" of the panel opens the exclusions dialog; its edits come through the
/// inbox, go to the worker as they are, and the dialog is shown the list again; its close
/// stops the refreshes; the exit closes it after the panel and the macro list editor.
#[test]
fn the_exclusions_dialog_opens_from_the_panel_and_its_edits_reach_the_worker() {
    on_fresh_thread(|| {
        let log = Log::default();
        let (services, feed) = FakeServices::new(&log);
        let add = ExclusionsRequest::Add(ExeName::new("notepad.exe").unwrap());
        let shell = FakeShell::new(&log)
            .choosing(&[Some(TrayCommand::Panel.id())])
            .panel_sending(vec![Event::Panel(PanelRequest::Exclusions)], &feed)
            .exclusions_sending(
                vec![
                    Event::Exclusions(add.clone()),
                    Event::ExclusionsClosed(1),
                    Event::Exclusions(ExclusionsRequest::Defaults),
                ],
                &feed,
            );
        let window = tray_window(
            "test-tray-exclusions",
            Vietnamese,
            shell,
            services,
            NO_TICKS,
        );
        post_icon(&window, WM_CONTEXTMENU, 5, 6);
        post(&window, WM_CLOSE, 0, 0);
        assert_eq!(run_loop_with_watchdog(), LOOP_EXIT_CODE);
        let calls = log.take();
        let opened = panel_state(Vietnamese);
        let state = exclusions_state();
        let mut expected = vec![
            Call::OpenPanel(1, Box::new(opened.clone())),
            Call::OpenExclusions(1, state.clone()),
            Call::RefreshPanel(Box::new(opened)),
            Call::Send(1, WorkerCommand::Exclusions(add)),
            Call::RefreshExclusions(state),
            // Closed: nothing is shown to it any more, and its edits still go to the worker.
            Call::Send(2, WorkerCommand::Exclusions(ExclusionsRequest::Defaults)),
        ];
        expected.extend(exit_calls());
        assert_eq!(calls[2..], expected);
    });
}

/// Types `typed` into the edit of the exclusions dialog's combo box as the keyboard does.
fn type_into_combo(dialog: HWND, typed: &str) {
    // SAFETY: only reads the window list.
    let edit = unsafe { GetWindow(item(dialog, exclusions_dialog::ID_APP), GW_CHILD) }.unwrap();
    for unit in typed.encode_utf16() {
        send_to(edit, WM_CHAR, usize::from(unit), 1);
    }
}

fn exclusion_rows(dialog: HWND) -> isize {
    send_to(
        item(dialog, exclusions_dialog::ID_LIST),
        LVM_GETITEMCOUNT,
        0,
        0,
    )
}

/// The real exclusions dialog through the real shell (off every monitor, never activated):
/// it shows the state it was opened with; "Thêm" becomes a request in the inbox; a refresh
/// shows the new list; opening it again shows the new state in the same window; "Đóng" closes
/// it with its latest number; a new opening shows a new dialog, which the exit closes without
/// waiting.
#[test]
fn the_real_exclusions_dialog_shows_the_state_sends_requests_and_closes() {
    let (inbox, events) = mpsc::channel();
    let mut shell = real_shell(inbox);
    let title = exclusions_dialog::TITLE;
    let state = exclusions_state();
    shell.open_exclusions(1, state.clone());
    let dialog = wait_for_dialog(title);
    assert_eq!(exclusion_rows(dialog), 4);
    let mut frame = RECT::default();
    // SAFETY: `frame` is a valid out buffer.
    unsafe { GetWindowRect(dialog, &mut frame) }.unwrap();
    assert!(frame.right < 0 && frame.bottom < 0, "{frame:?}");

    type_into_combo(dialog, "notepad");
    send_to(item(dialog, exclusions_dialog::ID_ADD), BM_CLICK, 0, 0);
    assert_eq!(
        next_event(&events),
        Event::Exclusions(ExclusionsRequest::Add(ExeName::new("notepad.exe").unwrap()))
    );

    let mut refreshed = state.clone();
    refreshed.apps.push(ExeName::new("notepad.exe").unwrap());
    shell.refresh_exclusions(refreshed.clone());
    wait_until("the refresh was shown", || exclusion_rows(dialog) == 5);

    // Opened again: the same window shows the new state.
    let mut again = refreshed.clone();
    again.apps.truncate(1);
    shell.open_exclusions(2, again);
    wait_until("the second opening was shown", || {
        exclusion_rows(dialog) == 1
    });
    assert_eq!(dialog_windows(title), [dialog]);

    // "Đóng" closes it; the close names the latest opening.
    send_to(item(dialog, IDCANCEL), BM_CLICK, 0, 0);
    assert_eq!(next_event(&events), Event::ExclusionsClosed(2));
    wait_until("the dialog was destroyed", || !is_window(dialog));
    shell.refresh_exclusions(state.clone());
    shell.close_exclusions();
    assert!(events.try_recv().is_err());

    // A new opening shows a new dialog; the exit closes it without waiting.
    shell.open_exclusions(3, state);
    let dialog = wait_for_dialog(title);
    assert_eq!(exclusion_rows(dialog), 4);
    shell.close_exclusions();
    assert_eq!(next_event(&events), Event::ExclusionsClosed(3));
    wait_until("the dialog was destroyed", || !is_window(dialog));
    assert_eq!(dialog_windows(title), []);
}

/// Opened again at once after the user closed it: a new dialog opens, and only the close of
/// the old opening is reported, so the tray keeps the new one.
#[test]
fn an_exclusions_dialog_opened_again_while_it_closes_opens_a_new_one() {
    let (inbox, events) = mpsc::channel();
    let mut shell = real_shell(inbox);
    let title = exclusions_dialog::TITLE;
    let state = exclusions_state();
    shell.open_exclusions(1, state.clone());
    let dialog = wait_for_dialog(title);
    // The close box; the handler has agreed once this returns.
    send_to(dialog, WM_CLOSE, 0, 0);
    shell.open_exclusions(2, state);
    assert_eq!(next_event(&events), Event::ExclusionsClosed(1));
    let mut again = None;
    wait_until("a new dialog came up", || {
        again = dialog_windows(title)
            .into_iter()
            .find(|&window| window != dialog);
        again.is_some()
    });
    let again = again.unwrap();
    send_to(again, WM_NULL, 0, 0);
    assert_eq!(exclusion_rows(again), 4);
    assert!(events.try_recv().is_err(), "the new dialog stays open");
    shell.close_exclusions();
    assert_eq!(next_event(&events), Event::ExclusionsClosed(2));
    wait_until("the dialogs were destroyed", || {
        !is_window(dialog) && !is_window(again)
    });
    assert_eq!(dialog_windows(title), []);
}

/// The check boxes the real-panel test reads back are the panel's, in the order of its view.
#[test]
fn the_check_box_ids_of_the_test_are_the_panels() {
    assert_eq!(
        checks_of(&panel_state(Vietnamese))
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        CHECK_BOX_IDS
    );
}
