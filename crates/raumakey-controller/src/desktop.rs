//! The run mode on Windows: the [`Desktop`] of [`crate::run`] and the glue that carries out
//! the [`Effect`]s of the tray state machine ([`crate::tray`]) on the real notification icon
//! and popup menu (plan §10.2; design §11; matrix #58).
//!
//! This module is thin: every decision is made in `crate::run` and `crate::tray`, which are
//! tested on every host, and every Win32 call is a `raumakey-windows` wrapper.
//!
//! - [`WinDesktop`] holds the single-instance mutex (`raumakey_windows::single_instance`,
//!   component [`CONTROLLER_INSTANCE`]), runs [`run_tray`], and shows errors in a message box.
//! - [`run_tray`] loads the V and E icons from `raumakey.exe` itself at the small icon size
//!   ([`ModeIcons`]), registers the window class, creates the tray window
//!   ([`create_tray_window`]), and runs the message loop on the calling thread. The window is
//!   destroyed after the loop has ended.
//! - [`create_tray_window`] creates a hidden top-level window: a message-only window would
//!   never receive the `TaskbarCreated` broadcast. When the controller runs elevated (the
//!   elevated logon task), it lets the shell's two messages, `TaskbarCreated` and the icon's
//!   callback, through User Interface Privilege Isolation (`ChangeWindowMessageFilterEx`,
//!   through `Window::allow_message_from_lower_integrity`). It then posts [`START`].
//! - [`TrayWindow`] is the window's handler: it passes each message to [`Tray::on_message`]
//!   and carries out the effect on a [`TrayShell`]. [`ShellTray`] is the real shell: the
//!   notification icon (`raumakey_windows::shell::NotifyIcon`, `NOTIFYICON_VERSION_4`, with
//!   the tooltip of the mode) and the popup menu (`raumakey_windows::menu::PopupMenu`, whose
//!   `track` makes the window the foreground window first and posts it a `WM_NULL` afterwards,
//!   as Microsoft documents for notification-icon menus). Tests use a fake shell on a real
//!   window and message loop.
//!
//! # Threads and failures
//!
//! The tray runs on the thread that calls [`run_tray`] (the main thread); no other thread
//! exists yet. Its window handler does no file, network, engine, or IPC work. A panic in the
//! handler is contained by the window procedure's guard (`raumakey_windows::window`).
//!
//! If the icon cannot be added at the start (Explorer has not created the taskbar yet, as
//! may happen to the elevated logon task), the controller keeps running and adds it when
//! `TaskbarCreated` arrives. If `TaskbarCreated` itself cannot be registered
//! (`RegisterWindowMessageW` fails), the controller runs without it: then the icon does not
//! come back after an Explorer restart. A `TaskbarCreated` without an Explorer restart finds the icon in
//! place: the icon is updated and adding it again fails harmlessly. If the window is
//! destroyed while the menu is open (a `WM_CLOSE` reaches `DefWindowProcW` during the menu's
//! modal loop), the handler removes the icon and ends the loop when it is freed. A
//! `TaskbarCreated` or `WM_ENDSESSION` that arrives while the menu is open gets the default
//! processing and is lost; the menu is open only while the user holds it open.
//!
//! The glue tests (`tests/desktop.rs`) run a real hidden window and message loop with a fake
//! shell. They also read back that the tray window is top-level (its parent is the desktop)
//! and, through `ChangeWindowMessageFilterEx`, that the window of an elevated controller lets
//! the shell's two messages through while that of a non-elevated one does not change its
//! filter. A successful build and these tests are no evidence that the real icon, menu, and
//! broadcasts behave on Windows.
//!
//! # Manual check list
//!
//! Run on a desktop session with the package's (or the build's) `raumakey.exe`, for example
//! with the ignored test `manual_tray_icon` (`tests/manual.rs`):
//!
//! 1. Start `raumakey.exe` without arguments: the V icon appears in the notification area,
//!    and its tooltip reads "Raumakey: đang gõ tiếng Việt".
//! 2. Click the icon: it shows E and the tooltip "Raumakey: đang gõ tiếng Anh"; click again:
//!    V. Select the icon with the keyboard (Win+B, arrow keys) and press Space, then Enter:
//!    each switches the mode once (the shell's `NIN_KEYSELECT` for Enter is unverified). A
//!    double click is two clicks and is expected to switch twice; record what it does.
//! 3. Right-click the icon: the menu opens at the icon with "Gõ tiếng Việt" checked in V
//!    mode and unchecked in E mode, and "Thoát". Choosing "Gõ tiếng Việt" switches the mode;
//!    clicking elsewhere or pressing Esc closes the menu and changes nothing; Shift+F10 on the
//!    selected icon opens the menu as well.
//! 4. Start `raumakey.exe` again while it runs: no second icon appears, and the second process
//!    ends at once with exit code 0 (`start /wait raumakey.exe` then `echo %errorlevel%`).
//! 5. Switch to E, then restart Windows Explorer (Task Manager, "Windows Explorer",
//!    Restart): the icon comes back as E, with the E tooltip (matrix #58).
//! 6. Choose "Thoát": the icon disappears and the process ends with exit code 0.
//! 7. `taskkill /IM raumakey.exe` (without `/F`), once with the menu closed and once with the
//!    menu open: the icon disappears and the process ends.
//! 8. Sign out while it runs: no error box, no hang; after signing in again nothing is left.
//! 9. Elevated: start it from the elevated logon task (`raumakey startup elevated-task` with a
//!    `raumakey.exe` inside Program Files, then sign out and in) or from an elevated prompt,
//!    and repeat 2, 3, and 5.

use std::rc::Rc;

use raumakey_model::identity::PRODUCT_NAME;
use raumakey_windows::dialog::{MessageKind, message_box};
use raumakey_windows::error::WinResult;
use raumakey_windows::icon::{Icon, IconKind, IconSize, icon_size};
use raumakey_windows::menu::{MenuItem, PopupMenu};
use raumakey_windows::module::Module;
use raumakey_windows::names::CONTROLLER_INSTANCE;
use raumakey_windows::shell::{NotifyIcon, taskbar_created_message};
use raumakey_windows::sid::current_process_is_elevated;
use raumakey_windows::single_instance::SingleInstance;
use raumakey_windows::window::{Window, WindowClass, WindowHandler, post_quit, run_message_loop};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};

use crate::run::Desktop;
use crate::tray::{Effect, InputMode, LOOP_EXIT_CODE, START, TRAY_CALLBACK, TRAY_ICON_ID, Tray};

/// Label of the tray window's class (`Raumakey.controller.<serial>`).
pub const WINDOW_CLASS_LABEL: &str = "controller";
/// Title of the hidden tray window (never shown; it names the window in window lists).
pub const WINDOW_TITLE: &str = PRODUCT_NAME;

/// The desktop of this Windows session.
#[derive(Debug, Default)]
pub struct WinDesktop {
    /// The single-instance mutex, while this process holds it.
    instance: Option<SingleInstance>,
}

impl WinDesktop {
    /// The desktop; nothing is acquired yet.
    pub fn new() -> Self {
        Self::default()
    }
}

impl Desktop for WinDesktop {
    fn acquire_instance(&mut self) -> WinResult<bool> {
        let instance = SingleInstance::acquire(CONTROLLER_INSTANCE)?;
        let acquired = instance.is_some();
        if acquired {
            self.instance = instance;
        }
        Ok(acquired)
    }

    fn run_tray(&mut self, mode: InputMode) -> WinResult<i32> {
        run_tray(mode)
    }

    fn release_instance(&mut self) {
        self.instance = None;
    }

    fn report_error(&mut self, text: &str) {
        // Nothing is left to try when the box itself cannot be shown.
        let _ = message_box(None, text, PRODUCT_NAME, MessageKind::Error);
    }
}

/// Runs the tray in `mode` on the calling thread until its message loop ends, and returns
/// the loop's exit code (see the module documentation).
pub fn run_tray(mode: InputMode) -> WinResult<i32> {
    let icons = ModeIcons::load(Module::executable()?, icon_size(IconKind::Small)?)?;
    // Without the registered message (0) the tray still runs; only an Explorer restart does
    // not bring the icon back.
    let tray = Tray::new(mode, taskbar_created_message().unwrap_or(0));
    let class = WindowClass::register(WINDOW_CLASS_LABEL)?;
    // An elevation check that fails counts as not elevated: the filter stays closed.
    let elevated = current_process_is_elevated().unwrap_or(false);
    let window = create_tray_window(&class, tray, ShellTray::new(icons), elevated)?;
    let code = run_message_loop();
    drop(window);
    code
}

/// Creates the hidden top-level tray window of `class` whose handler runs `tray` on `shell`,
/// lets the shell's messages through UIPI when `elevated`, and posts [`START`], so that the
/// icon is added once the message loop runs.
pub fn create_tray_window<S: TrayShell + 'static>(
    class: &WindowClass,
    tray: Tray,
    shell: S,
    elevated: bool,
) -> WinResult<Window> {
    let shell_messages = tray.shell_messages();
    let window =
        Window::create_hidden(class, WINDOW_TITLE, Box::new(TrayWindow::new(tray, shell)))?;
    if elevated {
        for message in shell_messages {
            // Without it the controller still runs; only the shell's message is lost (the
            // icon would not come back after an Explorer restart, or would not answer).
            let _ = window.allow_message_from_lower_integrity(message);
        }
    }
    window.handle().post(START.id(), WPARAM(0), LPARAM(0))?;
    Ok(window)
}

/// What the tray asks of the shell; [`ShellTray`] is the real one. Every method runs on the
/// thread of the tray window, inside its handler.
pub trait TrayShell {
    /// Adds the icon showing `mode` (its image and tooltip) for window `owner`; if it was
    /// added before, adds it again (`TaskbarCreated`) or, when it still exists, updates it.
    fn show(&mut self, owner: HWND, mode: InputMode);

    /// Shows `mode` on the icon, if the icon exists.
    fn update(&mut self, mode: InputMode);

    /// Shows the popup menu of `items` for window `owner` at (`x`, `y`) in screen
    /// coordinates, and returns the chosen command id, or `None` when it was dismissed.
    fn open_menu(
        &mut self,
        owner: HWND,
        x: i32,
        y: i32,
        items: &[MenuItem<'static>],
    ) -> Option<u16>;

    /// Removes the icon, if it exists.
    fn remove(&mut self);
}

/// The handler of the tray window: passes each message to the [`Tray`] and carries out its
/// effect on the shell.
///
/// When it is freed while the tray runs (the window was destroyed under it, see the module
/// documentation), it removes the icon and ends the message loop with [`LOOP_EXIT_CODE`], so
/// the process never lingers without its icon while it holds the single-instance mutex.
pub struct TrayWindow<S: TrayShell> {
    tray: Tray,
    shell: S,
}

impl<S: TrayShell> core::fmt::Debug for TrayWindow<S> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("TrayWindow")
            .field("tray", &self.tray)
            .finish_non_exhaustive()
    }
}

impl<S: TrayShell> TrayWindow<S> {
    /// The handler for `tray` on `shell`.
    pub fn new(tray: Tray, shell: S) -> Self {
        Self { tray, shell }
    }

    /// Carries out `effect`, and the effect of a command chosen in the menu.
    fn apply(&mut self, owner: HWND, effect: Effect) {
        let mut next = Some(effect);
        while let Some(effect) = next.take() {
            match effect {
                Effect::ShowIcon(mode) => self.shell.show(owner, mode),
                Effect::UpdateIcon(mode) => self.shell.update(mode),
                Effect::OpenMenu { x, y } => {
                    let items = self.tray.menu();
                    let chosen = self.shell.open_menu(owner, x, y, &items);
                    next = self.tray.menu_choice(chosen);
                }
                Effect::Exit => {
                    self.shell.remove();
                    post_quit(LOOP_EXIT_CODE);
                }
            }
        }
    }
}

impl<S: TrayShell> WindowHandler for TrayWindow<S> {
    fn on_message(
        &mut self,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Option<LRESULT> {
        let reply = self.tray.on_message(message, wparam.0, lparam.0);
        if let Some(effect) = reply.effect {
            self.apply(hwnd, effect);
        }
        reply.result.map(LRESULT)
    }
}

impl<S: TrayShell> Drop for TrayWindow<S> {
    fn drop(&mut self) {
        if self.tray.ends_loop_when_dropped() {
            self.shell.remove();
            post_quit(LOOP_EXIT_CODE);
        }
    }
}

/// The V and E icons, loaded once at one size and shared with the notification icon.
#[derive(Debug)]
pub struct ModeIcons {
    vietnamese: Rc<Icon>,
    english: Rc<Icon>,
}

impl ModeIcons {
    /// Loads the icon of each mode ([`InputMode::icon_id`]) from `module` at `size`. Only
    /// `raumakey.exe` carries them (its `build.rs`).
    pub fn load(module: Module, size: IconSize) -> WinResult<Self> {
        Ok(Self {
            vietnamese: Rc::new(Icon::load(module, InputMode::Vietnamese.icon_id(), size)?),
            english: Rc::new(Icon::load(module, InputMode::English.icon_id(), size)?),
        })
    }

    /// The icon of `mode`.
    pub fn icon(&self, mode: InputMode) -> Rc<Icon> {
        Rc::clone(match mode {
            InputMode::Vietnamese => &self.vietnamese,
            InputMode::English => &self.english,
        })
    }
}

/// The real shell of the tray: the notification icon and the popup menu.
#[derive(Debug)]
pub struct ShellTray {
    icons: ModeIcons,
    /// The icon, once the shell accepted it.
    notify: Option<NotifyIcon>,
}

impl ShellTray {
    /// A shell showing `icons`; the icon is added by the first [`TrayShell::show`].
    pub fn new(icons: ModeIcons) -> Self {
        Self {
            icons,
            notify: None,
        }
    }
}

impl TrayShell for ShellTray {
    fn show(&mut self, owner: HWND, mode: InputMode) {
        match &mut self.notify {
            Some(notify) => {
                // The data first: an icon that still exists is updated here (and adding it
                // again fails); after an Explorer restart these fail and the icon is added
                // with the current mode.
                let _ = notify.set_icon(self.icons.icon(mode));
                let _ = notify.set_tooltip(mode.tooltip());
                let _ = notify.readd();
            }
            None => {
                // Without a taskbar yet, the next TaskbarCreated adds the icon.
                self.notify = NotifyIcon::add(
                    owner,
                    u32::from(TRAY_ICON_ID),
                    TRAY_CALLBACK.id(),
                    self.icons.icon(mode),
                    mode.tooltip(),
                )
                .ok();
            }
        }
    }

    fn update(&mut self, mode: InputMode) {
        if let Some(notify) = &mut self.notify {
            let _ = notify.set_icon(self.icons.icon(mode));
            let _ = notify.set_tooltip(mode.tooltip());
        }
    }

    fn open_menu(
        &mut self,
        owner: HWND,
        x: i32,
        y: i32,
        items: &[MenuItem<'static>],
    ) -> Option<u16> {
        let mut menu = PopupMenu::new().ok()?;
        for item in items {
            menu.append(*item).ok()?;
        }
        menu.track(owner, POINT { x, y })
    }

    fn remove(&mut self) {
        if let Some(notify) = self.notify.take() {
            let _ = notify.delete();
        }
    }
}
