//! Low-level keyboard and mouse hooks (plan §10.2: a dedicated hook thread).
//!
//! The decode functions turn `KBDLLHOOKSTRUCT`/`MSLLHOOKSTRUCT` field values into
//! [`KeyboardEvent`]/[`MouseEvent`] and are pure. [`HookThread`] (Windows) owns a thread that
//! installs `WH_KEYBOARD_LL` and `WH_MOUSE_LL`, runs a message loop, and uninstalls both hooks
//! when stopped.
//!
//! **Handlers must return quickly.** Windows calls low-level hooks synchronously for every key
//! and click in the session and gives up on a hook that exceeds `LowLevelHooksTimeout` (about
//! one second; repeated timeouts make Windows remove the hook silently). Handlers must not block
//! on I/O, locks held by other threads, or IPC; heavy work belongs on another thread.
//!
//! A panicking handler never unwinds into Windows: the event is passed through.

use crate::panic::guard_value;
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;

/// Window message numbers seen by low-level hooks.
pub mod messages {
    /// `WM_KEYDOWN`.
    pub const WM_KEYDOWN: u32 = 0x0100;
    /// `WM_KEYUP`.
    pub const WM_KEYUP: u32 = 0x0101;
    /// `WM_SYSKEYDOWN`.
    pub const WM_SYSKEYDOWN: u32 = 0x0104;
    /// `WM_SYSKEYUP`.
    pub const WM_SYSKEYUP: u32 = 0x0105;
    /// `WM_MOUSEMOVE`.
    pub const WM_MOUSEMOVE: u32 = 0x0200;
    /// `WM_LBUTTONDOWN`.
    pub const WM_LBUTTONDOWN: u32 = 0x0201;
    /// `WM_LBUTTONUP`.
    pub const WM_LBUTTONUP: u32 = 0x0202;
    /// `WM_RBUTTONDOWN`.
    pub const WM_RBUTTONDOWN: u32 = 0x0204;
    /// `WM_RBUTTONUP`.
    pub const WM_RBUTTONUP: u32 = 0x0205;
    /// `WM_MBUTTONDOWN`.
    pub const WM_MBUTTONDOWN: u32 = 0x0207;
    /// `WM_MOUSEWHEEL`.
    pub const WM_MOUSEWHEEL: u32 = 0x020A;
    /// `WM_XBUTTONDOWN`.
    pub const WM_XBUTTONDOWN: u32 = 0x020B;
    /// `WM_MOUSEHWHEEL`.
    pub const WM_MOUSEHWHEEL: u32 = 0x020E;
}

use messages::{
    WM_KEYUP, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
};

const LLKHF_EXTENDED: u32 = 0x01;
const LLKHF_LOWER_IL_INJECTED: u32 = 0x02;
const LLKHF_INJECTED: u32 = 0x10;
const LLKHF_ALTDOWN: u32 = 0x20;
const LLKHF_UP: u32 = 0x80;
const LLMHF_INJECTED: u32 = 0x01;
const LLMHF_LOWER_IL_INJECTED: u32 = 0x02;

/// A decoded `KBDLLHOOKSTRUCT`. It contains a key code, so it must never be logged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyboardEvent {
    /// Virtual key.
    pub vk: u32,
    /// Scan code (without the `E0` prefix; see `extended`).
    pub scan: u32,
    /// `LLKHF_EXTENDED`.
    pub extended: bool,
    /// `LLKHF_INJECTED`: the event came from `SendInput` or similar.
    pub injected: bool,
    /// `LLKHF_LOWER_IL_INJECTED`: injected by a lower-integrity process.
    pub injected_lower_il: bool,
    /// `LLKHF_ALTDOWN`.
    pub alt_down: bool,
    /// Key release.
    pub key_up: bool,
    /// Event time stamp (milliseconds, `GetTickCount` base).
    pub time: u32,
    /// `dwExtraInfo`.
    pub extra_info: usize,
}

impl KeyboardEvent {
    /// True when Raumakey itself injected the key (matrix #39): such keys bypass the engine.
    pub const fn is_raumakey_injected(&self) -> bool {
        self.extra_info as u64 == RAUMAKEY_INJECTED_TAG
    }
}

/// Mouse buttons whose presses reset the typing context.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left button.
    Left,
    /// Right button.
    Right,
    /// Middle button.
    Middle,
    /// First extra button.
    X1,
    /// Second extra button.
    X2,
}

/// A decoded mouse button press.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MouseEvent {
    /// The button pressed.
    pub button: MouseButton,
    /// `LLMHF_INJECTED`.
    pub injected: bool,
    /// `LLMHF_LOWER_IL_INJECTED`.
    pub injected_lower_il: bool,
    /// Event time stamp.
    pub time: u32,
    /// `dwExtraInfo`.
    pub extra_info: usize,
}

/// Decodes the fields of a `KBDLLHOOKSTRUCT` delivered with `message`.
pub const fn decode_keyboard(
    message: u32,
    vk: u32,
    scan: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
) -> KeyboardEvent {
    KeyboardEvent {
        vk,
        scan,
        extended: flags & LLKHF_EXTENDED != 0,
        injected: flags & LLKHF_INJECTED != 0,
        injected_lower_il: flags & LLKHF_LOWER_IL_INJECTED != 0,
        alt_down: flags & LLKHF_ALTDOWN != 0,
        key_up: flags & LLKHF_UP != 0 || message == WM_KEYUP || message == WM_SYSKEYUP,
        time,
        extra_info,
    }
}

/// Decodes the fields of an `MSLLHOOKSTRUCT`; only button presses produce an event (moves,
/// releases, and wheel turns do not change the caret).
pub const fn decode_mouse(
    message: u32,
    mouse_data: u32,
    flags: u32,
    time: u32,
    extra_info: usize,
) -> Option<MouseEvent> {
    let button = match message {
        WM_LBUTTONDOWN => MouseButton::Left,
        WM_RBUTTONDOWN => MouseButton::Right,
        WM_MBUTTONDOWN => MouseButton::Middle,
        WM_XBUTTONDOWN => match mouse_data >> 16 {
            1 => MouseButton::X1,
            2 => MouseButton::X2,
            _ => return None,
        },
        _ => return None,
    };
    Some(MouseEvent {
        button,
        injected: flags & LLMHF_INJECTED != 0,
        injected_lower_il: flags & LLMHF_LOWER_IL_INJECTED != 0,
        time,
        extra_info,
    })
}

/// What the keyboard hook does with a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HookDecision {
    /// Let the key reach the application.
    PassThrough,
    /// Consume the key (the application never sees it).
    Swallow,
}

/// Receives hook events on the hook thread. Implementations must return quickly (see the module
/// documentation).
pub trait HookHandler {
    /// A key press or release.
    fn on_key(&mut self, event: &KeyboardEvent) -> HookDecision;
    /// A mouse button press.
    fn on_mouse(&mut self, event: &MouseEvent);
}

/// Calls `handler.on_key`; a panic passes the key through.
pub fn dispatch_key(handler: &mut dyn HookHandler, event: &KeyboardEvent) -> HookDecision {
    guard_value(HookDecision::PassThrough, || handler.on_key(event))
}

/// Calls `handler.on_mouse`; a panic is contained.
pub fn dispatch_mouse(handler: &mut dyn HookHandler, event: &MouseEvent) {
    guard_value((), || handler.on_mouse(event));
}

#[cfg(windows)]
pub use win::HookThread;

#[cfg(windows)]
mod win {
    use std::cell::RefCell;
    use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
    use std::thread::{self, JoinHandle};
    use std::time::Duration;

    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, MSG,
        MSLLHOOKSTRUCT, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, SetWindowsHookExW,
        TranslateMessage, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_QUIT, WM_USER,
    };
    use windows::core::PCWSTR;

    use super::{
        HookDecision, HookHandler, decode_keyboard, decode_mouse, dispatch_key, dispatch_mouse,
    };
    use crate::error::{E_UNEXPECTED, ResultExt, WinError, WinResult, win32};
    use crate::panic::guard_value;

    /// How long [`HookThread::stop`] waits for the thread to finish.
    const STOP_TIMEOUT: Duration = Duration::from_secs(2);

    type Handler = Box<dyn HookHandler + Send>;

    /// A worker thread that reported a start value.
    #[derive(Debug)]
    struct Started<T> {
        value: T,
        join: JoinHandle<()>,
        done: Receiver<()>,
    }

    /// Runs `body` on a new thread named `name` and waits until `body` reports through its
    /// callback or ends without reporting.
    ///
    /// There is deliberately no start timeout: giving up early would leave a thread that may
    /// still install global hooks, with nobody left to stop it. An `Err` report and an end
    /// without a report (including a contained panic) join the thread before returning. The
    /// callback returns whether the report was delivered. `done` receives one message when the
    /// thread is about to exit.
    fn start_reporting<T: Send + 'static>(
        name: &str,
        body: impl FnOnce(&dyn Fn(WinResult<T>) -> bool) + Send + 'static,
    ) -> WinResult<Started<T>> {
        let (ready_tx, ready_rx) = mpsc::sync_channel::<WinResult<T>>(1);
        let (done_tx, done_rx) = mpsc::channel::<()>();
        let join = thread::Builder::new()
            .name(name.to_owned())
            .spawn(move || {
                let report = |result: WinResult<T>| ready_tx.send(result).is_ok();
                guard_value((), || body(&report));
                let _ = done_tx.send(());
                // `ready_tx` is dropped here, which ends a wait for a report that never came.
            })
            .map_err(|error| WinError::from_io(&error, "spawn hook thread"))?;
        match ready_rx.recv() {
            Ok(Ok(value)) => Ok(Started {
                value,
                join,
                done: done_rx,
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(WinError::new(E_UNEXPECTED, "hook thread start"))
            }
        }
    }

    thread_local! {
        /// The handler of the hook thread; hook procedures run on the installing thread.
        static HANDLER: RefCell<Option<Handler>> = const { RefCell::new(None) };
    }

    /// A thread that owns the low-level keyboard and mouse hooks and their message loop.
    ///
    /// Dropping it stops the thread like [`HookThread::stop`] (ignoring the result).
    #[derive(Debug)]
    pub struct HookThread {
        thread_id: u32,
        join: Option<JoinHandle<()>>,
        done: Receiver<()>,
    }

    impl HookThread {
        /// Starts the thread, installs both hooks, and returns once they are installed, or with
        /// the installation error once the thread has exited. It never returns while the thread
        /// could still install hooks on its own.
        pub fn spawn(handler: Handler) -> WinResult<Self> {
            let started = start_reporting("raumakey-hooks", move |ready| run(handler, ready))?;
            Ok(Self {
                thread_id: started.value,
                join: Some(started.join),
                done: started.done,
            })
        }

        /// Id of the hook thread.
        pub const fn thread_id(&self) -> u32 {
            self.thread_id
        }

        /// Posts `WM_QUIT` and waits at most 2 s for the thread to uninstall the hooks and
        /// exit. On timeout the thread is detached and `TimedOut` is returned.
        pub fn stop(mut self) -> WinResult<()> {
            self.stop_inner()
        }

        fn stop_inner(&mut self) -> WinResult<()> {
            let Some(join) = self.join.take() else {
                return Ok(());
            };
            // SAFETY: posting to a thread id has no memory-safety preconditions; a thread that
            // already exited makes the call fail, which the wait below handles.
            let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
            match self.done.recv_timeout(STOP_TIMEOUT) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                    let _ = join.join();
                    Ok(())
                }
                Err(RecvTimeoutError::Timeout) => Err(WinError::from_win32(
                    win32::ERROR_TIMEOUT,
                    "hook thread stop",
                )),
            }
        }
    }

    impl Drop for HookThread {
        fn drop(&mut self) {
            let _ = self.stop_inner();
        }
    }

    /// Body of the hook thread.
    fn run(handler: Handler, ready: &dyn Fn(WinResult<u32>) -> bool) {
        let mut message = MSG::default();
        // Create the thread's message queue before reporting the id, so WM_QUIT can be posted.
        // SAFETY: `message` is a valid out buffer; PM_NOREMOVE leaves the queue unchanged.
        let _ = unsafe { PeekMessageW(&mut message, None, WM_USER, WM_USER, PM_NOREMOVE) };
        HANDLER.with(|slot| *slot.borrow_mut() = Some(handler));
        let hooks = match install() {
            Ok(hooks) => hooks,
            Err(error) => {
                HANDLER.with(|slot| slot.borrow_mut().take());
                let _ = ready(Err(error));
                return;
            }
        };
        // SAFETY: no preconditions.
        let delivered = ready(Ok(unsafe { GetCurrentThreadId() }));
        // Without a delivered id nobody could stop this thread, so the hooks are removed
        // right away instead of running the loop.
        if delivered {
            loop {
                // SAFETY: `message` is a valid out buffer for this thread's queue.
                let result = unsafe { GetMessageW(&mut message, None, 0, 0) };
                if result.0 == 0 || result.0 == -1 {
                    break;
                }
                // SAFETY: `message` was filled by GetMessageW.
                unsafe {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
        }
        for hook in [hooks.1, hooks.0] {
            // SAFETY: both hooks were installed by this thread and are removed once.
            let _ = unsafe { UnhookWindowsHookEx(hook) };
        }
        HANDLER.with(|slot| slot.borrow_mut().take());
    }

    fn install() -> WinResult<(HHOOK, HHOOK)> {
        // SAFETY: a null name returns the module of the executable without a reference count.
        let module = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("GetModuleHandleW")?;
        let instance = HINSTANCE(module.0);
        // SAFETY: the procedure is a valid `extern "system"` hook procedure for this module.
        let keyboard =
            unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(instance), 0) }
                .context("SetWindowsHookExW(WH_KEYBOARD_LL)")?;
        // SAFETY: as above.
        let mouse =
            match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(instance), 0) } {
                Ok(mouse) => mouse,
                Err(error) => {
                    // SAFETY: removes the keyboard hook installed just above.
                    let _ = unsafe { UnhookWindowsHookEx(keyboard) };
                    return Err(WinError::new(
                        error.code().0,
                        "SetWindowsHookExW(WH_MOUSE_LL)",
                    ));
                }
            };
        Ok((keyboard, mouse))
    }

    /// `WH_KEYBOARD_LL` procedure. Never unwinds; a missing or busy handler passes through.
    unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && lparam.0 != 0 {
            let swallow = guard_value(false, || {
                // SAFETY: for HC_ACTION, lparam points to a KBDLLHOOKSTRUCT that is valid for
                // the duration of this call.
                let record = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
                let event = decode_keyboard(
                    wparam.0 as u32,
                    record.vkCode,
                    record.scanCode,
                    record.flags.0,
                    record.time,
                    record.dwExtraInfo,
                );
                with_handler(|handler| dispatch_key(handler, &event) == HookDecision::Swallow)
                    .unwrap_or(false)
            });
            if swallow {
                return LRESULT(1);
            }
        }
        // SAFETY: forwards the unchanged arguments to the next hook.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// `WH_MOUSE_LL` procedure. Never unwinds and never swallows clicks.
    unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 && lparam.0 != 0 {
            guard_value((), || {
                // SAFETY: for HC_ACTION, lparam points to an MSLLHOOKSTRUCT that is valid for
                // the duration of this call.
                let record = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
                if let Some(event) = decode_mouse(
                    wparam.0 as u32,
                    record.mouseData,
                    record.flags,
                    record.time,
                    record.dwExtraInfo,
                ) {
                    let _ = with_handler(|handler| dispatch_mouse(handler, &event));
                }
            });
        }
        // SAFETY: forwards the unchanged arguments to the next hook.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// Runs `body` with the thread's handler; `None` when there is none or it is already in
    /// use (re-entrant hook call).
    fn with_handler<T>(body: impl FnOnce(&mut dyn HookHandler) -> T) -> Option<T> {
        HANDLER
            .try_with(|slot| {
                let mut slot = slot.try_borrow_mut().ok()?;
                let handler = slot.as_mut()?;
                Some(body(handler.as_mut()))
            })
            .ok()
            .flatten()
    }

    #[cfg(test)]
    mod tests {
        //! The start handshake of the hook thread, with bodies that install no hooks.

        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::thread;
        use std::time::{Duration, Instant};

        use super::start_reporting;
        use crate::error::{E_ACCESSDENIED, WinError, WinErrorKind};

        #[test]
        fn start_waits_for_a_late_report_instead_of_abandoning_the_thread() {
            let started = Instant::now();
            let worker = start_reporting("raumakey-test-late", |ready| {
                thread::sleep(Duration::from_millis(150));
                assert!(ready(Ok(7_u32)));
            })
            .unwrap();
            assert_eq!(worker.value, 7);
            assert!(started.elapsed() >= Duration::from_millis(100));
            worker.join.join().unwrap();
            assert!(worker.done.recv_timeout(Duration::from_secs(2)).is_ok());
        }

        #[test]
        fn a_reported_error_returns_after_the_thread_ended() {
            let finished = Arc::new(AtomicBool::new(false));
            let flag = Arc::clone(&finished);
            let error = start_reporting::<u32>("raumakey-test-error", move |ready| {
                assert!(ready(Err(WinError::new(E_ACCESSDENIED, "install"))));
                flag.store(true, Ordering::SeqCst);
            })
            .unwrap_err();
            assert_eq!(error.kind(), WinErrorKind::AccessDenied);
            assert!(finished.load(Ordering::SeqCst), "joined before returning");
        }

        #[test]
        fn a_body_that_ends_without_reporting_is_an_error() {
            // The default hook's message for the deliberate panic is captured by the harness.
            let error =
                start_reporting::<u32>("raumakey-test-panic", |_| panic!("bug")).unwrap_err();
            assert_eq!(error.context(), "hook thread start");
            let error = start_reporting::<u32>("raumakey-test-silent", |_| {}).unwrap_err();
            assert_eq!(error.context(), "hook thread start");
        }
    }
}
