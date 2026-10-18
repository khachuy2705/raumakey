//! Window classes, hidden and message-only windows, message routing, and the message loop
//! (plan §10.2: the controller's hidden window, its message loop, and results posted back from
//! the worker thread).
//!
//! [`AppMessage`] (private message numbers in the `WM_APP` range), [`class_name`], and the
//! message numbers below are pure. On Windows:
//!
//! - [`WindowClass`] registers a class (`RegisterClassExW`) under a name that is unique to that
//!   class ([`class_name`]: `Raumakey.<label>.<serial>`) and unregisters it once the last clone
//!   of it and the last window of the class are gone. Classes are registered for the module
//!   that links this crate ([`Module::containing_this_crate`](crate::module::Module)), so a DLL
//!   registers classes of its own. Windows does not unregister a DLL's classes when the DLL is
//!   unloaded, and their window procedure is code of the DLL, so a class registered by a DLL
//!   holds a server lock ([`com_server`](crate::com_server)) while it is registered:
//!   `DllCanUnloadNow` keeps the DLL loaded while any class or window of it exists.
//! - [`Window::create_hidden`] creates a top-level window that is never shown (`WS_POPUP` with
//!   `WS_EX_TOOLWINDOW`: no taskbar button, not in Alt+Tab). [`Window::create_message_only`]
//!   creates a message-only window (parent `HWND_MESSAGE`): it is not enumerated, never
//!   visible, and receives only messages sent or posted to it.
//! - Each window routes its messages to its own [`WindowHandler`], a trait object stored per
//!   window: it is handed to `CreateWindowExW` through `CREATESTRUCTW::lpCreateParams`, installed
//!   in the window's extra bytes at `WM_NCCREATE`, and freed at `WM_NCDESTROY`. Only windows
//!   created through [`Window`] get a handler: the window procedure never reads the creation
//!   parameters of a window of the class created any other way.
//! - [`run_message_loop`] runs `GetMessageW`/`TranslateMessage`/`DispatchMessageW` until
//!   `WM_QUIT` and returns its exit code; [`post_quit`] ends it. A [`WindowHandle`] lets other
//!   threads post private and registered messages to a window ([`is_postable`]).
//! - [`register_message`] registers a message by name (`RegisterWindowMessageW`), and
//!   [`register_user_message`] one named for the current user
//!   ([`message_name`](crate::names::message_name): `Raumakey.<sidhash>.<component>`), such as
//!   the request of a second controller start to the running one.
//!   [`post_to_top_level_windows`] posts a registered message to every top-level window of the
//!   desktop (`HWND_BROADCAST`; [`is_broadcastable`]), and [`allow_any_foreground`]
//!   (`AllowSetForegroundWindow(ASFW_ANY)`) first passes the caller's right to set the
//!   foreground window on, so that the window that answers may come to the front.
//!
//! **Message-only windows receive no broadcast messages.** Explorer announces a new taskbar by
//! broadcasting the registered `TaskbarCreated` message
//! ([`taskbar_created_message`](crate::shell::taskbar_created_message)) to top-level windows
//! only, so a message-only window would never learn that its notification icon must be added
//! again (matrix #58). The owner of the notification icon must therefore be a hidden top-level
//! window created with [`Window::create_hidden`]; message-only windows suit purely internal
//! traffic. An elevated process (the elevated startup task) receives the broadcast from a
//! non-elevated Explorer only if it lets that message through User Interface Privilege
//! Isolation: [`Window::allow_message_from_lower_integrity`] (`ChangeWindowMessageFilterEx`)
//! does so for one private or registered message of one window.
//!
//! [`WM_CLOSE`] and [`WM_ENDSESSION`] are pure numbers for code that handles them on every
//! host (the controller's tray state machine).
//!
//! **Threads.** A [`Window`] belongs to the thread that created it: it is neither `Send` nor
//! `Sync`, its handler runs only on that thread (inside that thread's message loop or a
//! `SendMessageW` from that thread), and only that thread destroys it. Another thread may only
//! post messages to it, through a [`WindowHandle`].
//!
//! **Re-entrancy.** While the handler handles a message, messages for the same window that
//! arrive re-entrantly (sent by the handler itself, or dispatched by a modal loop the handler
//! runs, such as a message box or a tracked popup menu) do not reach the handler: they get the
//! default processing of `DefWindowProcW`. A posted private message delivered during such a
//! modal loop is therefore not seen by the handler; data should travel through a channel that
//! the next notification drains. A window destroyed while its handler runs (for example by
//! `DestroyWindow` inside the handler) is freed when the handler returns.
//!
//! **Forged messages.** `WM_DESTROY` and `WM_NCDESTROY` carry no pointers, so any process that
//! may post to a window can post them, although the system only ever sends them. A
//! [`WindowHandle`] posts no system messages, and [`run_message_loop`] drops queued
//! `WM_DESTROY` and `WM_NCDESTROY` messages. The message loop of another component (such as the
//! host application of the TIP) dispatches them, and the window procedure cannot tell a posted
//! `WM_NCDESTROY` from the real one: the handler is then freed while the window lives on with
//! default processing only. Dropping the [`Window`] still destroys such a window, so neither
//! the window nor its class leaks.
//!
//! **Panics.** The window procedure runs inside [`guard_lresult`](crate::panic::guard_lresult);
//! a panicking handler leaves the message to `DefWindowProcW`, and no unwind reaches Windows.

use crate::error::{E_INVALIDARG, WinError, WinResult};
use crate::names::is_valid_component;

/// `WM_APP`: the first message number an application may use for private messages.
pub const WM_APP: u32 = 0x8000;
/// The last message number of the `WM_APP` range.
pub const WM_APP_LAST: u32 = 0xBFFF;
/// The first message number `RegisterWindowMessageW` hands out (such as `TaskbarCreated`).
pub const REGISTERED_MESSAGE_FIRST: u32 = 0xC000;
/// The last message number `RegisterWindowMessageW` hands out.
pub const REGISTERED_MESSAGE_LAST: u32 = 0xFFFF;
/// Prefix of the name of every window class registered by this crate.
pub const CLASS_NAME_PREFIX: &str = "Raumakey.";
/// `WM_CLOSE`: a request to close the window (its close command, or `taskkill` without `/F`).
/// `DefWindowProcW` answers it by destroying the window.
pub const WM_CLOSE: u32 = 0x0010;
/// `WM_ENDSESSION`: the answer to `WM_QUERYENDSESSION`. A non-zero `wParam` means that the
/// session ends (sign-out, shutdown, or the Restart Manager closing the application); the
/// process may end any time after the window procedure returns.
pub const WM_ENDSESSION: u32 = 0x0016;
/// `WM_TIMER`: a timer of the window (`Window::set_timer`, on Windows) elapsed;
/// `wParam` is the timer's id. Windows posts it only when the thread's queue is otherwise
/// empty, and never more than one per timer at a time.
pub const WM_TIMER: u32 = 0x0113;

/// True for the messages a `WindowHandle` posts: private messages
/// ([`WM_APP`]`..=`[`WM_APP_LAST`], see [`AppMessage`]) and registered messages
/// ([`REGISTERED_MESSAGE_FIRST`]`..=`[`REGISTERED_MESSAGE_LAST`]). System messages (below
/// `WM_USER`) come from the system only, and the `WM_USER` range belongs to each window class.
pub const fn is_postable(message: u32) -> bool {
    matches!(
        message,
        WM_APP..=WM_APP_LAST | REGISTERED_MESSAGE_FIRST..=REGISTERED_MESSAGE_LAST
    )
}

/// True for the messages [`post_to_top_level_windows`] posts: registered messages
/// ([`REGISTERED_MESSAGE_FIRST`]`..=`[`REGISTERED_MESSAGE_LAST`]), whose meaning every
/// receiver agrees on by name. A private message means something else to each application, and
/// system messages come from the system only.
pub const fn is_broadcastable(message: u32) -> bool {
    matches!(message, REGISTERED_MESSAGE_FIRST..=REGISTERED_MESSAGE_LAST)
}

/// A private message of the application: `WM_APP + offset`, within
/// [`WM_APP`]`..=`[`WM_APP_LAST`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AppMessage(u32);

impl AppMessage {
    /// `WM_APP + offset`; `None` when that would leave the range (`offset > 0x3FFF`).
    pub const fn new(offset: u32) -> Option<Self> {
        if offset <= WM_APP_LAST - WM_APP {
            Some(Self(WM_APP + offset))
        } else {
            None
        }
    }

    /// The private message numbered `id`; `None` outside the `WM_APP` range.
    pub const fn from_id(id: u32) -> Option<Self> {
        if matches!(id, WM_APP..=WM_APP_LAST) {
            Some(Self(id))
        } else {
            None
        }
    }

    /// The message number.
    pub const fn id(self) -> u32 {
        self.0
    }

    /// The offset from `WM_APP`.
    pub const fn offset(self) -> u32 {
        self.0 - WM_APP
    }
}

/// The window class name `Raumakey.<label>.<serial>`. `label` must match
/// `[A-Za-z0-9_-]{1,64}`; otherwise the result is an `E_INVALIDARG` error.
pub fn class_name(label: &str, serial: u64) -> WinResult<String> {
    if !is_valid_component(label) {
        return Err(WinError::new(E_INVALIDARG, "window class label"));
    }
    Ok(format!("{CLASS_NAME_PREFIX}{label}.{serial}"))
}

#[cfg(windows)]
pub use win::{
    Window, WindowClass, WindowHandle, WindowHandler, allow_any_foreground, post_quit,
    post_to_top_level_windows, register_message, register_user_message, run_message_loop,
};

#[cfg(windows)]
mod win {
    use core::cell::{Cell, RefCell};
    use core::ffi::c_void;
    use core::fmt;
    use std::rc::Rc;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        ASFW_ANY, AllowSetForegroundWindow, CREATESTRUCTW, ChangeWindowMessageFilterEx,
        CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GCW_ATOM, GetClassWord,
        GetMessageW, GetWindowThreadProcessId, HWND_BROADCAST, HWND_MESSAGE, KillTimer, MSG,
        MSGFLT_ALLOW, PostMessageW, PostQuitMessage, RegisterClassExW, RegisterWindowMessageW,
        SetTimer, TranslateMessage, UnregisterClassW, WINDOW_EX_STYLE, WINDOW_LONG_PTR_INDEX,
        WINDOW_STYLE, WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WNDCLASSEXW, WS_EX_TOOLWINDOW,
        WS_POPUP,
    };

    use super::{class_name, is_broadcastable, is_postable};
    use crate::com_server::ServerLock;
    use crate::error::{E_ABORT, E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::module::Module;
    use crate::names::message_name;
    use crate::panic::{guard_lresult, guard_value};
    use crate::sid::current_user_sid_hash;
    use crate::strings::WideString;

    /// Serial number of the next class name.
    static NEXT_CLASS: AtomicU64 = AtomicU64::new(0);

    /// Offset of the state pointer in the window's extra bytes.
    const STATE_SLOT: WINDOW_LONG_PTR_INDEX = WINDOW_LONG_PTR_INDEX(0);
    /// Extra bytes of every window of these classes: one pointer.
    const EXTRA_BYTES: i32 = size_of::<isize>() as i32;

    /// Receives the messages of one window, on the window's thread.
    ///
    /// The window's thread is a UI thread: a handler must return quickly and must not block on
    /// files, the network, or IPC (design §11); slow work belongs to a worker thread that posts
    /// its result back through a [`WindowHandle`].
    ///
    /// Closures `FnMut(HWND, u32, WPARAM, LPARAM) -> Option<LRESULT>` are handlers too.
    pub trait WindowHandler {
        /// Handles `message` for window `hwnd`. `Some(result)` answers it; `None` leaves it to
        /// `DefWindowProcW`.
        fn on_message(
            &mut self,
            hwnd: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> Option<LRESULT>;
    }

    impl<F> WindowHandler for F
    where
        F: FnMut(HWND, u32, WPARAM, LPARAM) -> Option<LRESULT>,
    {
        fn on_message(
            &mut self,
            hwnd: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> Option<LRESULT> {
            self(hwnd, message, wparam, lparam)
        }
    }

    /// A registered window class. Clones share the registration; every [`Window`] of the
    /// class holds a clone too. The class is unregistered (`UnregisterClassW`) when the last
    /// of them is dropped. It may be shared between threads.
    ///
    /// Windows of the class must be created through [`Window`]. A window created any other
    /// way, such as `CreateWindowExW` with the class [`name`](Self::name) or a dialog template,
    /// gets no handler: the window procedure leaves all its messages to `DefWindowProcW`.
    ///
    /// A class registered by a DLL holds a server lock
    /// ([`ServerLock::for_module`](crate::com_server::ServerLock::for_module)) until it is
    /// unregistered, because its window procedure is code of the DLL: `DllCanUnloadNow`
    /// answers `S_FALSE` while a class or window of the DLL exists. If unregistering fails (a
    /// window of the class that was not created through [`Window`] still exists), the lock is
    /// kept for the rest of the process and the DLL stays loaded.
    #[derive(Clone)]
    pub struct WindowClass {
        inner: Arc<ClassInner>,
    }

    struct ClassInner {
        name: String,
        wide: WideString,
        module: Module,
        /// The class atom `RegisterClassExW` returned (never 0).
        atom: u16,
        /// Keeps a DLL loaded while the class is registered; `None` in an executable.
        unload_lock: Option<ServerLock>,
    }

    impl Drop for ClassInner {
        fn drop(&mut self) {
            // SAFETY: unregisters the class `register` registered under this name and
            // instance; the call fails while a window of the class exists.
            let unregistered =
                unsafe { UnregisterClassW(self.wide.as_pcwstr(), Some(self.module.instance())) }
                    .is_ok();
            if !unregistered {
                // The window procedure of the remaining windows is code of this module.
                if let Some(lock) = self.unload_lock.take() {
                    core::mem::forget(lock);
                }
            }
        }
    }

    impl fmt::Debug for WindowClass {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("WindowClass")
                .field("name", &self.inner.name)
                .finish_non_exhaustive()
        }
    }

    impl WindowClass {
        /// Registers a new class named `Raumakey.<label>.<serial>` for the module that links
        /// this crate. `label` must match `[A-Za-z0-9_-]{1,64}`.
        pub fn register(label: &str) -> WinResult<Self> {
            let module = Module::containing_this_crate()?;
            Self::register_with(label, module, ServerLock::for_module(module)?)
        }

        /// Registers the class for `module`, holding `unload_lock` (if any) until the class is
        /// unregistered (see the `Drop` of `ClassInner`).
        fn register_with(
            label: &str,
            module: Module,
            unload_lock: Option<ServerLock>,
        ) -> WinResult<Self> {
            let serial = NEXT_CLASS.fetch_add(1, Ordering::Relaxed);
            let name = class_name(label, serial)?;
            let wide = WideString::new(&name)?;
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                cbWndExtra: EXTRA_BYTES,
                hInstance: module.instance(),
                lpszClassName: wide.as_pcwstr(),
                ..Default::default()
            };
            // SAFETY: `class` is fully initialized and its name outlives the call (the system
            // copies it).
            let atom = unsafe { RegisterClassExW(&class) };
            if atom == 0 {
                return Err(WinError::last("RegisterClassExW"));
            }
            Ok(Self {
                inner: Arc::new(ClassInner {
                    name,
                    wide,
                    module,
                    atom,
                    unload_lock,
                }),
            })
        }

        /// The class name, for lookups such as `FindWindowExW`. Windows of the class must be
        /// created through [`Window`] (see the type documentation).
        pub fn name(&self) -> &str {
            &self.inner.name
        }

        /// The module the class is registered for.
        pub fn module(&self) -> Module {
            self.inner.module
        }
    }

    /// A window created by this module. Dropping it destroys the window (`DestroyWindow`)
    /// unless the window is already gone. It belongs to the creating thread (see the module
    /// documentation), so it is neither `Send`:
    ///
    /// ```compile_fail
    /// fn assert_send<T: Send>() {}
    /// assert_send::<raumakey_windows::window::Window>();
    /// ```
    ///
    /// nor `Sync`:
    ///
    /// ```compile_fail
    /// fn assert_sync<T: Sync>() {}
    /// assert_sync::<raumakey_windows::window::Window>();
    /// ```
    pub struct Window {
        hwnd: HWND,
        alive: Rc<Cell<bool>>,
        class: WindowClass,
    }

    impl fmt::Debug for Window {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Window")
                .field("hwnd", &self.hwnd.0)
                .field("alive", &self.alive.get())
                .finish_non_exhaustive()
        }
    }

    impl Window {
        /// Creates a top-level window of `class` that is never shown (`WS_POPUP`,
        /// `WS_EX_TOOLWINDOW`). It receives broadcast messages such as `TaskbarCreated`, so it
        /// is the owner for the notification icon.
        ///
        /// The handler sees the window's messages from `WM_NCCREATE` on, so it can set the
        /// window up at `WM_CREATE`. A handler that refuses the window (`FALSE` for
        /// `WM_NCCREATE`, -1 for `WM_CREATE`) makes creation fail with `E_ABORT`
        /// ([`WinErrorKind::Cancelled`](crate::error::WinErrorKind)), and is dropped.
        pub fn create_hidden(
            class: &WindowClass,
            title: &str,
            handler: Box<dyn WindowHandler>,
        ) -> WinResult<Self> {
            Self::create(class, title, handler, WS_EX_TOOLWINDOW, WS_POPUP, None)
        }

        /// Creates a message-only window of `class` (parent `HWND_MESSAGE`). It receives no
        /// broadcast messages. Creation and its failures work as for
        /// [`create_hidden`](Self::create_hidden).
        pub fn create_message_only(
            class: &WindowClass,
            title: &str,
            handler: Box<dyn WindowHandler>,
        ) -> WinResult<Self> {
            Self::create(
                class,
                title,
                handler,
                WINDOW_EX_STYLE(0),
                WINDOW_STYLE(0),
                Some(HWND_MESSAGE),
            )
        }

        fn create(
            class: &WindowClass,
            title: &str,
            handler: Box<dyn WindowHandler>,
            ex_style: WINDOW_EX_STYLE,
            style: WINDOW_STYLE,
            parent: Option<HWND>,
        ) -> WinResult<Self> {
            let title = WideString::new(title)?;
            let alive = Rc::new(Cell::new(true));
            let pending = PendingState(Cell::new(Some(Box::new(WindowState {
                handler: RefCell::new(handler),
                alive: Rc::clone(&alive),
                frames: Cell::new(0),
                destroyed: Cell::new(false),
            }))));
            let params: *const PendingState = &pending;
            let created = {
                let _creating = Creating::mark(params);
                // SAFETY: the class was registered for this instance; both strings and
                // `pending` outlive the call, and `window_proc` reads `pending` only during
                // `WM_NCCREATE` of this call, on this thread (see `Creating`).
                unsafe {
                    CreateWindowExW(
                        ex_style,
                        class.inner.wide.as_pcwstr(),
                        title.as_pcwstr(),
                        style,
                        0,
                        0,
                        0,
                        0,
                        parent,
                        None,
                        Some(class.module().instance()),
                        Some(params.cast::<c_void>()),
                    )
                }
            };
            // When creation failed before `WM_NCCREATE`, the state is still pending and is
            // dropped with `pending`. Otherwise the window existed and `WM_NCDESTROY` frees the
            // state; if creation failed after all, the window procedure refused the window,
            // which `GetLastError` does not describe.
            let hwnd = match created {
                Ok(hwnd) => hwnd,
                Err(_) if pending.was_installed() => {
                    return Err(WinError::new(E_ABORT, "CreateWindowExW"));
                }
                Err(error) => return Err(WinError::new(error.code().0, "CreateWindowExW")),
            };
            Ok(Self {
                hwnd,
                alive,
                class: class.clone(),
            })
        }

        /// The window handle, for calls on this thread (such as adding a notification icon).
        pub fn hwnd(&self) -> HWND {
            self.hwnd
        }

        /// A handle that other threads may use to post messages to this window.
        pub fn handle(&self) -> WindowHandle {
            WindowHandle(self.hwnd.0 as isize)
        }

        /// False once the window received `WM_NCDESTROY`, which normally means that it was
        /// destroyed (see "Forged messages" in the module documentation).
        pub fn is_alive(&self) -> bool {
            self.alive.get()
        }

        /// Lets `message` reach this window from processes of lower integrity
        /// (`ChangeWindowMessageFilterEx` with `MSGFLT_ALLOW`), for this window only.
        ///
        /// User Interface Privilege Isolation (UIPI) drops most messages that a process of
        /// lower integrity sends or posts to the windows of an elevated process. An elevated
        /// controller needs two of them from the (non-elevated) shell: the `TaskbarCreated`
        /// broadcast (matrix #58) and the callback message of its notification icon. Every
        /// process of the user's desktop can then send that message to the window, as it can
        /// to a window of a non-elevated process, so only private ([`super::AppMessage`]) and
        /// registered messages are accepted ([`super::is_postable`]). Any other message fails
        /// with `E_INVALIDARG` before the call: system messages such as `WM_CLOSE` or
        /// `WM_COPYDATA` and the class-private `WM_USER` range stay filtered.
        ///
        /// In a process that is not elevated the call succeeds and only widens the filter of
        /// this window towards low-integrity processes, so callers allow messages only when
        /// they run elevated. Allowing a message twice is harmless; the call fails once the
        /// window is destroyed.
        pub fn allow_message_from_lower_integrity(&self, message: u32) -> WinResult<()> {
            if !is_postable(message) {
                return Err(WinError::new(
                    E_INVALIDARG,
                    "message let through from lower integrity",
                ));
            }
            // SAFETY: USER validates the handle, which names a window of this process while
            // it lives; no filter-change structure is passed, so nothing is written back.
            unsafe { ChangeWindowMessageFilterEx(self.hwnd, message, MSGFLT_ALLOW, None) }
                .context("ChangeWindowMessageFilterEx")
        }

        /// Starts timer `id` of this window (`SetTimer`), or restarts it with the new interval
        /// when it runs already: every `interval_ms` milliseconds (Windows raises intervals
        /// below 10 ms to 10 ms) the window's handler receives a `WM_TIMER`
        /// ([`super::WM_TIMER`]) whose `wParam` is `id`. The timer ends with
        /// [`kill_timer`](Self::kill_timer) or with the window. Id 0 fails with
        /// `E_INVALIDARG`.
        ///
        /// While a modal loop runs inside the handler (a menu, a message box), the timer's
        /// messages get the default processing like any other (see "Re-entrancy" in the module
        /// documentation), so a tick can be missed but never queued twice.
        pub fn set_timer(&self, id: usize, interval_ms: u32) -> WinResult<()> {
            if id == 0 {
                return Err(WinError::new(E_INVALIDARG, "timer id"));
            }
            // SAFETY: USER validates the handle; no timer procedure is passed, so the timer
            // posts WM_TIMER to the window and calls no code of ours.
            let set = unsafe { SetTimer(Some(self.hwnd), id, interval_ms, None) };
            if set == 0 {
                return Err(WinError::last("SetTimer"));
            }
            Ok(())
        }

        /// Stops timer `id` of this window (`KillTimer`); a `WM_TIMER` already queued is
        /// removed with it.
        pub fn kill_timer(&self, id: usize) -> WinResult<()> {
            // SAFETY: USER validates the handle and the id.
            unsafe { KillTimer(Some(self.hwnd), id) }.context("KillTimer")
        }

        /// True when the handle still names a window of this thread and of this window's class
        /// that has no handler: the window outlived a `WM_NCDESTROY` that was posted to it
        /// rather than sent by `DestroyWindow`. (A reused handle value of another [`Window`]
        /// of the class has a handler.)
        fn outlived_a_forged_destroy(&self) -> bool {
            // SAFETY: queries about a window handle, which USER validates; a handle that names
            // no window yields 0, which is neither a thread id nor a class atom.
            let same_thread_and_class = unsafe {
                GetWindowThreadProcessId(self.hwnd, None) == GetCurrentThreadId()
                    && GetClassWord(self.hwnd, GCW_ATOM.0) == self.class.inner.atom
            };
            same_thread_and_class && read_slot(self.hwnd) == 0
        }
    }

    impl Drop for Window {
        fn drop(&mut self) {
            if self.alive.get() || self.outlived_a_forged_destroy() {
                // SAFETY: this thread created the window (`Window` is not `Send`) and the
                // window still exists: `alive` is cleared only at `WM_NCDESTROY`, and otherwise
                // the handle was just checked to name a window of this thread and class.
                let _ = unsafe { DestroyWindow(self.hwnd) };
            }
        }
    }

    /// The handle value of a window, for any thread. Its only operation is posting a message
    /// (`PostMessageW`); everything else must happen on the window's thread through
    /// [`Window`].
    ///
    /// Only private messages ([`super::AppMessage`]) and registered messages are posted (see
    /// [`super::is_postable`]), with plain values, never pointers. After the window is
    /// destroyed, posting fails; Windows may eventually reuse the handle value for another
    /// window, so a handle should not outlive its window by long.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct WindowHandle(isize);

    impl WindowHandle {
        /// Posts `message` to the window's queue. A message that is not private or registered
        /// ([`super::is_postable`]) fails with `E_INVALIDARG` and is not posted: system
        /// messages must come from the system (a posted `WM_NCDESTROY`, for example, would cut
        /// the window off from its handler).
        pub fn post(self, message: u32, wparam: WPARAM, lparam: LPARAM) -> WinResult<()> {
            if !is_postable(message) {
                return Err(WinError::new(E_INVALIDARG, "posted message"));
            }
            // SAFETY: PostMessageW only queues the values; a handle that no longer names a
            // window makes the call fail.
            unsafe { PostMessageW(Some(HWND(self.0 as *mut c_void)), message, wparam, lparam) }
                .context("PostMessageW")
        }
    }

    /// True for a queued message that the system only ever sends: `WM_DESTROY` and
    /// `WM_NCDESTROY` come from `DestroyWindow`, so a queued one was posted by someone else
    /// (they carry no pointers, so any process may post them). `WM_NCCREATE` and `WM_CREATE`
    /// carry pointers, so they cannot be posted at all.
    fn is_forged(message: &MSG) -> bool {
        matches!(message.message, WM_DESTROY | WM_NCDESTROY)
    }

    /// Runs the calling thread's message loop (`GetMessageW`, `TranslateMessage`,
    /// `DispatchMessageW`) until `WM_QUIT`, and returns the exit code given to [`post_quit`].
    /// Posted `WM_DESTROY` and `WM_NCDESTROY` messages are dropped (see "Forged messages" in
    /// the module documentation).
    pub fn run_message_loop() -> WinResult<i32> {
        let mut message = MSG::default();
        loop {
            // SAFETY: `message` is a valid out buffer; the filter accepts every window of the
            // thread and thread messages.
            let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
            match result.0 {
                0 => return Ok(message.wParam.0 as i32),
                -1 => return Err(WinError::last("GetMessageW")),
                _ if is_forged(&message) => {}
                _ => {
                    // SAFETY: `message` was filled by GetMessageW.
                    unsafe {
                        let _ = TranslateMessage(&message);
                        DispatchMessageW(&message);
                    }
                }
            }
        }
    }

    /// Ends the calling thread's message loop with `exit_code` (`PostQuitMessage`).
    pub fn post_quit(exit_code: i32) {
        // SAFETY: no preconditions; posts `WM_QUIT` to the calling thread.
        unsafe { PostQuitMessage(exit_code) };
    }

    /// The message number registered for `name` (`RegisterWindowMessageW`), one of the
    /// registered messages ([`super::is_broadcastable`]). Every process of the session that
    /// registers the same name gets the same number until the session ends. An empty name or
    /// one with a NUL fails with `E_INVALIDARG`.
    pub fn register_message(name: &str) -> WinResult<u32> {
        if name.is_empty() {
            return Err(WinError::new(E_INVALIDARG, "registered message name"));
        }
        let wide =
            WideString::new(name).map_err(|error| error.with_context("registered message name"))?;
        // SAFETY: the name is NUL-terminated and outlives the call (the system copies it).
        let message = unsafe { RegisterWindowMessageW(wide.as_pcwstr()) };
        if message == 0 {
            return Err(WinError::last("RegisterWindowMessageW"));
        }
        Ok(message)
    }

    /// The message registered for `Raumakey.<sidhash>.<component>` of the current user
    /// ([`message_name`]). `component` must match `[A-Za-z0-9_-]{1,64}`; otherwise the result
    /// is an `E_INVALIDARG` error.
    pub fn register_user_message(component: &str) -> WinResult<u32> {
        let hash = current_user_sid_hash()?;
        let name = message_name(&hash, component)
            .map_err(|_| WinError::new(E_INVALIDARG, "registered message component"))?;
        register_message(&name)
    }

    /// Posts the registered `message` with zero parameters to every top-level window of the
    /// desktop (`PostMessageW(HWND_BROADCAST)`), without waiting. Message-only windows receive
    /// nothing, and User Interface Privilege Isolation drops the message for windows of
    /// processes of higher integrity unless they let it through
    /// ([`Window::allow_message_from_lower_integrity`]). Any message that is not registered
    /// ([`super::is_broadcastable`]) fails with `E_INVALIDARG` before anything is posted.
    pub fn post_to_top_level_windows(message: u32) -> WinResult<()> {
        if !is_broadcastable(message) {
            return Err(WinError::new(E_INVALIDARG, "broadcast message"));
        }
        // SAFETY: posts plain values (no pointers) to every top-level window; the call never
        // waits for a receiver.
        unsafe { PostMessageW(Some(HWND_BROADCAST), message, WPARAM(0), LPARAM(0)) }
            .context("PostMessageW(HWND_BROADCAST)")
    }

    /// Lets any process set the foreground window (`AllowSetForegroundWindow(ASFW_ANY)`), for
    /// as long as this process could itself: the process that the user just started (or
    /// clicked) asks another one to bring its window to the front. Fails with
    /// `ERROR_ACCESS_DENIED` ([`WinErrorKind::AccessDenied`](crate::error::WinErrorKind)) when
    /// this process may not set the foreground window either.
    pub fn allow_any_foreground() -> WinResult<()> {
        // SAFETY: no pointers; the call only changes the foreground rights of the session.
        unsafe { AllowSetForegroundWindow(ASFW_ANY) }.context("AllowSetForegroundWindow")
    }

    /// What the window procedure keeps per window (its pointer is in the extra bytes).
    struct WindowState {
        handler: RefCell<Box<dyn WindowHandler>>,
        /// Shared with [`Window`]; cleared at `WM_NCDESTROY`.
        alive: Rc<Cell<bool>>,
        /// Window procedure frames currently using this state.
        frames: Cell<u32>,
        /// `WM_NCDESTROY` was seen: the last frame frees the state.
        destroyed: Cell<bool>,
    }

    /// The state handed to `CreateWindowExW`; the window procedure takes it at `WM_NCCREATE`.
    struct PendingState(Cell<Option<Box<WindowState>>>);

    impl PendingState {
        /// True once the window procedure took the state.
        fn was_installed(&self) -> bool {
            let state = self.0.take();
            let installed = state.is_none();
            self.0.set(state);
            installed
        }
    }

    thread_local! {
        /// The pending state of the `CreateWindowExW` call that `Window::create` is running on
        /// this thread, or null.
        static CREATING: Cell<*const PendingState> = const { Cell::new(core::ptr::null()) };
    }

    /// Marks a pending state as the one `install` may take, for the duration of one
    /// `CreateWindowExW` call. Creations nest: code that runs on this thread inside that call,
    /// before the new window's `WM_NCCREATE` (such as a `WH_CBT` hook) or after it (a handler),
    /// may create windows through [`Window`] too, so dropping the mark restores the previous
    /// one.
    struct Creating {
        previous: *const PendingState,
    }

    impl Creating {
        fn mark(pending: *const PendingState) -> Self {
            Self {
                previous: CREATING.with(|creating| creating.replace(pending)),
            }
        }
    }

    impl Drop for Creating {
        fn drop(&mut self) {
            CREATING.with(|creating| creating.set(self.previous));
        }
    }

    /// The window procedure of every class of this module.
    unsafe extern "system" fn window_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        guard_lresult(LRESULT(0), || {
            if message == WM_NCCREATE {
                // SAFETY: for WM_NCCREATE, `lparam` points to the CREATESTRUCTW of the
                // creating call.
                unsafe { install(hwnd, lparam) };
            }
            let state = read_slot(hwnd) as *mut WindowState;
            if state.is_null() {
                // SAFETY: default processing of a message of this thread's window.
                return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
            }
            // SAFETY: the slot holds a state installed by `install`, which stays allocated
            // until no frame uses it (see `dispatch`).
            unsafe { dispatch(state, hwnd, message, wparam, lparam) }
        })
    }

    /// Calls the handler in `state` (or `DefWindowProcW` when it is busy or declines) and frees
    /// the state after `WM_NCDESTROY` once no frame uses it any more.
    ///
    /// # Safety
    ///
    /// `state` must come from `Box::into_raw` in `install` and must not have been freed.
    unsafe fn dispatch(
        state: *mut WindowState,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let (last_use, result) = {
            // SAFETY: valid per the contract; only shared references are created, and the
            // mutable parts sit behind `Cell`/`RefCell`. The reference ends with this block.
            let shared = unsafe { &*state };
            shared.frames.set(shared.frames.get().saturating_add(1));
            if message == WM_NCDESTROY {
                // No later frame can reach the state once the slot is cleared.
                write_slot(hwnd, 0);
                shared.destroyed.set(true);
                shared.alive.set(false);
            }
            // A busy handler (re-entrant message) and a panicking one both leave the message
            // to DefWindowProcW. The borrow is released before the default processing runs.
            let answer = guard_value(None, || match shared.handler.try_borrow_mut() {
                Ok(mut handler) => handler.on_message(hwnd, message, wparam, lparam),
                Err(_) => None,
            });
            let result = match answer {
                Some(result) => result,
                // SAFETY: default processing of a message of this thread's window.
                None => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
            };
            shared.frames.set(shared.frames.get().saturating_sub(1));
            (shared.frames.get() == 0 && shared.destroyed.get(), result)
        };
        if last_use {
            // SAFETY: the slot was cleared at WM_NCDESTROY and no frame uses the state, so
            // this is the last reference; it came from Box::into_raw.
            drop(unsafe { Box::from_raw(state) });
        }
        result
    }

    /// Moves the pending state of a window being created into its extra bytes.
    ///
    /// Only the state that `Window::create` marked for its running `CreateWindowExW` call is
    /// taken, and only into an empty slot. Parameters of any other creation (a window of the
    /// class created by name, a dialog template) and a `WM_NCCREATE` sent to a live window are
    /// never read as a state: such windows get no handler, or keep the one they have.
    ///
    /// # Safety
    ///
    /// `lparam` must be the `WM_NCCREATE` argument: a pointer to a `CREATESTRUCTW`.
    unsafe fn install(hwnd: HWND, lparam: LPARAM) {
        let create = lparam.0 as *const CREATESTRUCTW;
        if create.is_null() || read_slot(hwnd) != 0 {
            return;
        }
        // SAFETY: per the contract, `create` points to the creation structure.
        let params = unsafe { (*create).lpCreateParams }
            .cast::<PendingState>()
            .cast_const();
        if params.is_null() || params != CREATING.with(Cell::get) {
            return;
        }
        // SAFETY: `params` is the pending state that `Window::create` marked on this thread
        // and still holds while its `CreateWindowExW` call (which sent this message) runs.
        let Some(state) = (unsafe { &*params }).0.take() else {
            return;
        };
        write_slot(hwnd, Box::into_raw(state) as isize);
    }

    #[cfg(target_pointer_width = "64")]
    fn read_slot(hwnd: HWND) -> isize {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW;
        // SAFETY: reads the extra bytes of a window of these classes (`EXTRA_BYTES` covers
        // the slot); a stale handle reads 0.
        unsafe { GetWindowLongPtrW(hwnd, STATE_SLOT) }
    }

    #[cfg(target_pointer_width = "32")]
    fn read_slot(hwnd: HWND) -> isize {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowLongW;
        // SAFETY: as above; on 32-bit Windows the slot is a 32-bit value.
        (unsafe { GetWindowLongW(hwnd, STATE_SLOT) }) as isize
    }

    #[cfg(target_pointer_width = "64")]
    fn write_slot(hwnd: HWND, value: isize) {
        use windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW;
        // SAFETY: writes the extra bytes of a window of these classes on its own thread.
        unsafe { SetWindowLongPtrW(hwnd, STATE_SLOT, value) };
    }

    #[cfg(target_pointer_width = "32")]
    fn write_slot(hwnd: HWND, value: isize) {
        use windows::Win32::UI::WindowsAndMessaging::SetWindowLongW;
        // SAFETY: as above; on 32-bit Windows `isize` is 32 bits wide.
        unsafe { SetWindowLongW(hwnd, STATE_SLOT, value as i32) };
    }

    #[cfg(test)]
    mod tests {
        //! The server lock of a class registered by a DLL: released once the class is
        //! unregistered, kept while a window of the class prevents that. A class of the test
        //! executable takes no lock ([`ServerLock::for_module`]), so these tests hand one to
        //! `register_with`, as `register` does in a DLL.

        use std::sync::{Mutex, MutexGuard, PoisonError};

        use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetClassInfoExW, HWND_MESSAGE, UnregisterClassW,
            WINDOW_EX_STYLE, WINDOW_STYLE, WNDCLASSEXW,
        };
        use windows::core::w;

        use super::{Window, WindowClass};
        use crate::com_server::{ServerLock, lock_count};
        use crate::module::Module;
        use crate::strings::WideString;

        /// Serializes the tests that read the process-wide lock count.
        static SERIAL: Mutex<()> = Mutex::new(());

        fn serial() -> MutexGuard<'static, ()> {
            SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
        }

        /// Runs `body` on a new thread, which owns the windows it creates; a panic (a failed
        /// assertion) fails the calling test.
        fn on_fresh_thread(body: impl FnOnce() + Send + 'static) {
            if let Err(payload) = std::thread::spawn(body).join() {
                std::panic::resume_unwind(payload);
            }
        }

        /// A class registered the way `register` registers one in a DLL: holding a lock.
        fn register_locked(label: &str) -> WindowClass {
            let module = Module::containing_this_crate().unwrap();
            WindowClass::register_with(label, module, Some(ServerLock::new())).unwrap()
        }

        fn is_registered(name: &WideString, module: Module) -> bool {
            let mut info = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                ..Default::default()
            };
            // SAFETY: valid instance, NUL-terminated name, size-initialized out structure.
            unsafe { GetClassInfoExW(Some(module.instance()), name.as_pcwstr(), &mut info) }.is_ok()
        }

        #[test]
        fn a_dll_class_keeps_its_lock_until_its_last_window_is_gone_and_it_is_unregistered() {
            let _serial = serial();
            on_fresh_thread(|| {
                let before = lock_count();
                let class = register_locked("unit-lock-released");
                assert_eq!(lock_count(), before + 1);
                let name = WideString::new(class.name()).unwrap();
                let module = class.module();
                let window = Window::create_message_only(
                    &class,
                    "",
                    Box::new(|_: HWND, _: u32, _: WPARAM, _: LPARAM| -> Option<LRESULT> { None }),
                )
                .unwrap();
                drop(class);
                assert_eq!(
                    lock_count(),
                    before + 1,
                    "a window that outlives every class handle keeps the lock"
                );
                drop(window);
                assert!(!is_registered(&name, module), "the class was unregistered");
                assert_eq!(lock_count(), before, "and its lock released");
            });
        }

        #[test]
        fn a_dll_class_that_cannot_be_unregistered_keeps_its_lock() {
            let _serial = serial();
            on_fresh_thread(|| {
                let before = lock_count();
                let class = register_locked("unit-lock-kept");
                let name = WideString::new(class.name()).unwrap();
                let module = class.module();
                // A window of the class created by name (it gets no handler) makes
                // `UnregisterClassW` fail.
                // SAFETY: a class registered for this instance, NUL-terminated strings, and a
                // message-only parent; no creation parameters.
                let hwnd = unsafe {
                    CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        name.as_pcwstr(),
                        w!("by name"),
                        WINDOW_STYLE(0),
                        0,
                        0,
                        0,
                        0,
                        Some(HWND_MESSAGE),
                        None,
                        Some(module.instance()),
                        None,
                    )
                }
                .unwrap();
                drop(class);
                assert!(
                    is_registered(&name, module),
                    "the window keeps the class registered"
                );
                assert_eq!(lock_count(), before + 1, "and the class keeps its lock");
                // SAFETY: the window belongs to this thread and was created above.
                unsafe { DestroyWindow(hwnd) }.unwrap();
                // SAFETY: the class was registered above for this instance and has no window
                // left.
                unsafe { UnregisterClassW(name.as_pcwstr(), Some(module.instance())) }.unwrap();
                // Nothing gives the lock back: it stays counted for the rest of the process
                // (here the unit-test process), as it does in the DLL.
                assert_eq!(lock_count(), before + 1);
            });
        }
    }
}
