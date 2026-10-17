//! `window`: private messages and class names (pure); on Windows, window classes, hidden and
//! message-only windows, message routing, and the message loop.
//!
//! Every window is created on a fresh thread and destroyed before that thread ends. Nothing is
//! shown and nothing is broadcast: `TaskbarCreated` is posted to the test window only. Two tests
//! hook window creation with a `WH_CBT` hook of their own thread, which sees only the windows
//! that thread creates.
//!
//! A class of this executable takes no server lock, so the lock that a DLL's class holds until
//! it is unregistered is tested by unit tests in `src/window.rs`.

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::window::{
    AppMessage, REGISTERED_MESSAGE_FIRST, REGISTERED_MESSAGE_LAST, WM_APP, WM_APP_LAST, class_name,
    is_postable,
};

#[test]
fn only_private_and_registered_messages_are_postable() {
    assert_eq!(REGISTERED_MESSAGE_FIRST, 0xC000);
    assert_eq!(REGISTERED_MESSAGE_LAST, 0xFFFF);
    for allowed in [WM_APP, WM_APP + 1, WM_APP_LAST, 0xC000, 0xC123, 0xFFFF] {
        assert!(is_postable(allowed), "{allowed:#x}");
    }
    // WM_NULL, WM_DESTROY, WM_CLOSE, WM_QUIT, WM_NCDESTROY, the WM_USER range (private to each
    // window class), and values above 16 bits.
    for rejected in [
        0x0000,
        0x0002,
        0x0010,
        0x0012,
        0x0082,
        0x0400,
        0x7FFF,
        0x1_0000,
        u32::MAX,
    ] {
        assert!(!is_postable(rejected), "{rejected:#x}");
    }
}

#[test]
fn app_messages_stay_in_the_wm_app_range() {
    assert_eq!(WM_APP, 0x8000);
    assert_eq!(WM_APP_LAST, 0xBFFF);
    let first = AppMessage::new(0).unwrap();
    assert_eq!(first.id(), 0x8000);
    assert_eq!(first.offset(), 0);
    let last = AppMessage::new(0x3FFF).unwrap();
    assert_eq!(last.id(), 0xBFFF);
    assert_eq!(last.offset(), 0x3FFF);
    assert_eq!(AppMessage::new(0x4000), None);
    assert_eq!(AppMessage::new(u32::MAX), None);
    assert_eq!(AppMessage::from_id(0x8001), AppMessage::new(1));
    assert_eq!(AppMessage::from_id(0xBFFF), Some(last));
    for outside in [0, 0x0400, 0x7FFF, 0xC000, 0xFFFF, 0x1_0000, u32::MAX] {
        assert_eq!(AppMessage::from_id(outside), None, "{outside:#x}");
    }
}

#[test]
fn class_names_carry_the_product_prefix_a_label_and_a_serial() {
    assert_eq!(
        class_name("controller", 0).unwrap(),
        "Raumakey.controller.0"
    );
    assert_eq!(
        class_name("tip-ime_2", 17).unwrap(),
        "Raumakey.tip-ime_2.17"
    );
    let too_long = "a".repeat(65);
    for bad in ["", "a.b", "a b", "x\0", "ế", too_long.as_str()] {
        let error = class_name(bad, 0).unwrap_err();
        assert_eq!(error.kind(), WinErrorKind::InvalidInput, "{bad:?}");
    }
}

#[cfg(windows)]
mod win {
    use std::cell::{Cell, RefCell};
    use std::ffi::c_void;
    use std::rc::Rc;
    use std::sync::mpsc;
    use std::thread;

    use raumakey_windows::com_server::lock_count;
    use raumakey_windows::error::{WinErrorKind, WinResult};
    use raumakey_windows::module::Module;
    use raumakey_windows::panic::guard_value;
    use raumakey_windows::shell::taskbar_created_message;
    use raumakey_windows::strings::WideString;
    use raumakey_windows::window::{
        AppMessage, Window, WindowClass, WindowHandle, WindowHandler, post_quit, run_message_loop,
    };
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CBT_CREATEWNDW, CREATESTRUCTW, CallNextHookEx, CreateWindowExW, DestroyWindow,
        DispatchMessageW, FindWindowExW, GA_PARENT, GWL_EXSTYLE, GWL_STYLE, GetAncestor,
        GetClassInfoExW, GetClassNameW, GetDesktopWindow, GetWindowLongW, HCBT_CREATEWND, HHOOK,
        HWND_MESSAGE, IsWindow, IsWindowVisible, MSG, PM_REMOVE, PeekMessageW, PostMessageW,
        SendMessageW, SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, WH_CBT,
        WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_GETTEXTLENGTH,
        WM_NCCREATE, WM_NCDESTROY, WM_NULL, WM_QUIT, WM_USER, WNDCLASSEXW, WS_EX_TOOLWINDOW,
        WS_POPUP, WS_VISIBLE,
    };
    use windows::core::w;

    use crate::support::on_fresh_thread;

    const PING: AppMessage = match AppMessage::new(1) {
        Some(message) => message,
        None => panic!("offset outside the WM_APP range"),
    };
    const TITLE: &str = "Raumakey test";

    fn title_length() -> isize {
        TITLE.encode_utf16().count() as isize
    }

    /// A handler that never handles anything.
    fn ignore_all() -> Box<dyn WindowHandler> {
        Box::new(|_: HWND, _: u32, _: WPARAM, _: LPARAM| -> Option<LRESULT> { None })
    }

    /// Sends `message` to a window of the calling thread (the window procedure runs directly).
    fn send(hwnd: HWND, message: u32) -> isize {
        // SAFETY: the window belongs to the calling thread; no pointer arguments.
        unsafe { SendMessageW(hwnd, message, None, None) }.0
    }

    /// Whether `hwnd` still names a window.
    fn is_window(hwnd: HWND) -> bool {
        // SAFETY: only queries whether the handle names a window.
        unsafe { IsWindow(Some(hwnd)) }.as_bool()
    }

    /// Posts `message` with `PostMessageW` itself, as another process could.
    fn post_raw(hwnd: HWND, message: u32) {
        // SAFETY: posts plain values (no pointers) to a window of this thread.
        unsafe { PostMessageW(Some(hwnd), message, WPARAM(0), LPARAM(0)) }.unwrap();
    }

    /// Dispatches every queued message of this thread the way a host application's own message
    /// loop does (the TIP's windows live in such loops), without `run_message_loop`.
    fn dispatch_queued_messages() {
        let mut message = MSG::default();
        // SAFETY: `message` is a valid out buffer; the filter accepts every message of the
        // thread.
        while unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool() {
            // SAFETY: `message` was filled by PeekMessageW.
            unsafe {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }

    fn class_registered(name: &str) -> bool {
        let module = Module::containing_this_crate().unwrap();
        let name = WideString::new(name).unwrap();
        let mut info = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            ..Default::default()
        };
        // SAFETY: valid instance, NUL-terminated name, size-initialized out structure.
        unsafe { GetClassInfoExW(Some(module.instance()), name.as_pcwstr(), &mut info) }.is_ok()
    }

    /// Sets a flag when the window state that owns it is freed.
    struct DropFlag(Rc<Cell<bool>>);

    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }

    /// Counts how often the window state that owns it is freed.
    struct DropCount(Rc<Cell<u32>>);

    impl Drop for DropCount {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }

    type Constructor = fn(&WindowClass, &str, Box<dyn WindowHandler>) -> WinResult<Window>;

    const CONSTRUCTORS: [(&str, Constructor); 2] = [
        ("hidden", Window::create_hidden),
        ("message-only", Window::create_message_only),
    ];

    #[test]
    fn the_handler_is_installed_at_wm_nccreate_and_sees_wm_create_with_the_title() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-create").unwrap();
            for (kind, create) in CONSTRUCTORS {
                let seen = Rc::new(RefCell::new(Vec::new()));
                let title = Rc::new(RefCell::new(None));
                let (record, title_in) = (Rc::clone(&seen), Rc::clone(&title));
                let window = create(
                    &class,
                    TITLE,
                    Box::new(move |_: HWND, message: u32, _: WPARAM, lparam: LPARAM| {
                        record.borrow_mut().push(message);
                        if message == WM_CREATE {
                            // SAFETY: for WM_CREATE, `lparam` points to the CREATESTRUCTW
                            // of the creating call, whose title lives during the call.
                            let name = unsafe {
                                let create = &*(lparam.0 as *const CREATESTRUCTW);
                                create.lpszName.to_string()
                            };
                            *title_in.borrow_mut() = Some(name.unwrap());
                        }
                        None
                    }),
                )
                .unwrap();
                let seen = seen.take();
                assert_eq!(seen.first(), Some(&WM_NCCREATE), "{kind}: {seen:x?}");
                assert!(seen.contains(&WM_CREATE), "{kind}: {seen:x?}");
                assert_eq!(title.take().as_deref(), Some(TITLE), "{kind}");
                drop(window);
            }
        });
    }

    /// Create parameters that are not this module's: the first word is an invalid pointer, so
    /// a window procedure that took them for its own state would crash.
    const FOREIGN_PARAMETERS: [usize; 4] = [0x10, 0, 0, 0];

    #[test]
    fn windows_of_the_class_created_by_name_get_no_handler() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-foreign").unwrap();
            let name = WideString::new(class.name()).unwrap();
            let foreign = FOREIGN_PARAMETERS;
            // SAFETY: a class registered for this instance, static strings, and a
            // message-only parent; the parameter is only passed along.
            let hwnd = unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE(0),
                    name.as_pcwstr(),
                    w!("foreign"),
                    WINDOW_STYLE(0),
                    0,
                    0,
                    0,
                    0,
                    Some(HWND_MESSAGE),
                    None,
                    Some(class.module().instance()),
                    Some(foreign.as_ptr().cast()),
                )
            }
            .unwrap();
            // The default procedure answers every message of such a window.
            assert_eq!(send(hwnd, WM_GETTEXTLENGTH), "foreign".len() as isize);
            assert_eq!(send(hwnd, PING.id()), 0);
            // SAFETY: the window belongs to this thread and was created above.
            unsafe { DestroyWindow(hwnd) }.unwrap();
            assert_eq!(
                foreign, FOREIGN_PARAMETERS,
                "the parameters were not touched"
            );
        });
    }

    /// A `WM_NCCREATE` sent to a live window outside any creation, whose parameters cannot be a
    /// pending state. One that carries the pending state of a running creation is covered by
    /// `a_wm_nccreate_forwarded_to_a_live_window_during_a_creation_leaves_both_handlers`.
    #[test]
    fn a_wm_nccreate_sent_to_a_live_window_outside_a_creation_keeps_its_handler() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-second-nccreate").unwrap();
            let window = Window::create_message_only(
                &class,
                TITLE,
                Box::new(|_: HWND, message: u32, _: WPARAM, _: LPARAM| {
                    (message == PING.id()).then_some(LRESULT(7))
                }),
            )
            .unwrap();
            let foreign = FOREIGN_PARAMETERS;
            let create = CREATESTRUCTW {
                lpCreateParams: foreign.as_ptr().cast_mut().cast(),
                ..Default::default()
            };
            // SAFETY: a creation structure that outlives the call, sent to a window of this
            // thread.
            unsafe {
                SendMessageW(
                    window.hwnd(),
                    WM_NCCREATE,
                    None,
                    Some(LPARAM(&create as *const CREATESTRUCTW as isize)),
                )
            };
            assert_eq!(
                send(window.hwnd(), PING.id()),
                7,
                "the handler still answers"
            );
            assert_eq!(
                foreign, FOREIGN_PARAMETERS,
                "the parameters were not touched"
            );
        });
    }

    /// A handler that answers `PING` with `value` and leaves every other message to the
    /// default procedure.
    fn answer_ping(value: isize) -> Box<dyn WindowHandler> {
        Box::new(move |_: HWND, message: u32, _: WPARAM, _: LPARAM| {
            (message == PING.id()).then_some(LRESULT(value))
        })
    }

    /// What a [`CreationHook`] does inside `CreateWindowExW`: it gets the new window and its
    /// creation structure (whose `lpCreateParams` is the pending state of `Window`).
    type OnCreate = Box<dyn FnOnce(HWND, *const CREATESTRUCTW)>;

    thread_local! {
        /// The class name a [`CreationHook`] of this thread waits for, and its action.
        static ON_CREATE: RefCell<Option<(Vec<u16>, OnCreate)>> = const { RefCell::new(None) };
    }

    /// A `WH_CBT` hook of the calling thread only (no other thread is hooked). It runs its action
    /// once, when the next window of its class is about to be created: inside
    /// `CreateWindowExW`, after `Window` marked its pending state and before the new window's
    /// `WM_NCCREATE`. Dropping it removes the hook.
    struct CreationHook(HHOOK);

    impl CreationHook {
        fn install(class: &WindowClass, on_create: OnCreate) -> Self {
            let name = class.name().encode_utf16().collect();
            ON_CREATE.with(|slot| *slot.borrow_mut() = Some((name, on_create)));
            // SAFETY: a hook procedure of this process for the calling thread only, so no
            // module handle is needed.
            let hook = unsafe {
                SetWindowsHookExW(WH_CBT, Some(creation_hook), None, GetCurrentThreadId())
            };
            Self(hook.unwrap())
        }
    }

    impl Drop for CreationHook {
        fn drop(&mut self) {
            // SAFETY: removes the hook that `install` set on this thread.
            let _ = unsafe { UnhookWindowsHookEx(self.0) };
            ON_CREATE.with(|slot| slot.borrow_mut().take());
        }
    }

    unsafe extern "system" fn creation_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        // No unwind may reach the system: actions record what they see and assert nothing.
        guard_value((), || {
            if code != HCBT_CREATEWND as i32 {
                return;
            }
            let hwnd = HWND(wparam.0 as *mut c_void);
            let mut buffer = [0_u16; 257];
            // SAFETY: for HCBT_CREATEWND, `wparam` is the window being created, which exists.
            let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
            let class = &buffer[..usize::try_from(length).unwrap_or(0)];
            let action = ON_CREATE.with(|slot| {
                let mut slot = slot.borrow_mut();
                match slot.as_ref() {
                    Some((name, _)) if name.as_slice() == class => slot.take(),
                    _ => None,
                }
            });
            if let Some((_, action)) = action {
                // SAFETY: for HCBT_CREATEWND, `lparam` points to a CBT_CREATEWNDW whose
                // creation structure lives during the call.
                let create = unsafe { (*(lparam.0 as *const CBT_CREATEWNDW)).lpcs };
                action(hwnd, create.cast_const());
            }
        });
        // SAFETY: forwards the unchanged arguments to the next hook.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    /// A `WM_NCCREATE` that carries the creation structure of the window being created (so its
    /// parameters are the marked pending state) and is sent meanwhile to a live window of the
    /// class neither replaces that window's handler nor takes the new window's: the pending
    /// state goes only into an empty slot.
    #[test]
    fn a_wm_nccreate_forwarded_to_a_live_window_during_a_creation_leaves_both_handlers() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-forwarded-nccreate").unwrap();
            let live = Window::create_message_only(&class, TITLE, answer_ping(7)).unwrap();
            let live_hwnd = live.hwnd();
            let forwarded = Rc::new(Cell::new(false));
            let forwarded_in = Rc::clone(&forwarded);
            let hook = CreationHook::install(
                &class,
                Box::new(move |_: HWND, create: *const CREATESTRUCTW| {
                    // SAFETY: the creation structure lives during this creation; the window
                    // belongs to this thread.
                    unsafe {
                        SendMessageW(live_hwnd, WM_NCCREATE, None, Some(LPARAM(create as isize)))
                    };
                    forwarded_in.set(true);
                }),
            );
            let created = Window::create_message_only(&class, TITLE, answer_ping(8)).unwrap();
            drop(hook);
            assert!(forwarded.get(), "the hook ran during the creation");
            assert_eq!(
                send(live.hwnd(), PING.id()),
                7,
                "the live window kept its handler"
            );
            assert_eq!(
                send(created.hwnd(), PING.id()),
                8,
                "the new window got its own"
            );
        });
    }

    /// Code that runs on the thread inside `CreateWindowExW` before the new window's
    /// `WM_NCCREATE` (here a `WH_CBT` hook) may create a `Window` too: both windows get their
    /// handlers, because the inner creation restores the outer creation's mark.
    #[test]
    fn a_window_created_while_another_is_being_created_leaves_both_handlers() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-nested-create").unwrap();
            let inner = Rc::new(RefCell::new(None));
            let (inner_in, class_in) = (Rc::clone(&inner), class.clone());
            let hook = CreationHook::install(
                &class,
                Box::new(move |_: HWND, _: *const CREATESTRUCTW| {
                    let created = Window::create_message_only(&class_in, TITLE, answer_ping(9));
                    *inner_in.borrow_mut() = Some(created);
                }),
            );
            let outer = Window::create_message_only(&class, TITLE, answer_ping(10)).unwrap();
            drop(hook);
            let inner = inner.take().expect("the hook ran").unwrap();
            assert_eq!(
                send(inner.hwnd(), PING.id()),
                9,
                "the window created inside the hook got its handler"
            );
            assert_eq!(
                send(outer.hwnd(), PING.id()),
                10,
                "the window being created still got its handler"
            );
        });
    }

    #[test]
    fn a_handler_that_rejects_creation_fails_it_and_is_dropped_once() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-reject").unwrap();
            let name = class.name().to_owned();
            for (kind, create) in CONSTRUCTORS {
                // FALSE from WM_NCCREATE and -1 from WM_CREATE both refuse the window.
                for (refused, answer) in [(WM_NCCREATE, LRESULT(0)), (WM_CREATE, LRESULT(-1))] {
                    let drops = Rc::new(Cell::new(0_u32));
                    let count = DropCount(Rc::clone(&drops));
                    let error = create(
                        &class,
                        TITLE,
                        Box::new(move |_: HWND, message: u32, _: WPARAM, _: LPARAM| {
                            let _keep = &count;
                            (message == refused).then_some(answer)
                        }),
                    )
                    .unwrap_err();
                    let case = format!("{kind}, {refused:#x}");
                    assert_eq!(error.kind(), WinErrorKind::Cancelled, "{case}: {error}");
                    assert_eq!(error.context(), "CreateWindowExW", "{case}");
                    assert_eq!(
                        drops.get(),
                        1,
                        "{case}: the handler is dropped exactly once"
                    );
                }
            }
            // No window of the class is left behind.
            drop(class);
            assert!(!class_registered(&name));
        });
    }

    #[test]
    fn a_message_posted_from_another_thread_reaches_the_handler_and_quit_ends_the_loop() {
        let (handle_tx, handle_rx) = mpsc::channel::<WindowHandle>();
        let ui = thread::spawn(move || {
            let class = WindowClass::register("test-post").unwrap();
            let seen = Rc::new(RefCell::new(Vec::new()));
            let record = Rc::clone(&seen);
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(
                    move |_: HWND, message: u32, wparam: WPARAM, lparam: LPARAM| {
                        if message != PING.id() {
                            return None;
                        }
                        record.borrow_mut().push((wparam.0, lparam.0));
                        post_quit(7);
                        Some(LRESULT(0))
                    },
                ),
            )
            .unwrap();
            handle_tx.send(window.handle()).unwrap();
            let code = run_message_loop().unwrap();
            drop(window);
            (code, seen.take())
        });
        let handle = handle_rx.recv().unwrap();
        thread::spawn(move || handle.post(PING.id(), WPARAM(42), LPARAM(-5)).unwrap())
            .join()
            .unwrap();
        let (code, seen) = ui.join().unwrap();
        assert_eq!(code, 7);
        assert_eq!(seen, [(42, -5)]);
    }

    #[test]
    fn the_handler_receives_taskbar_created_posted_to_the_hidden_window() {
        let taskbar_created = taskbar_created_message().unwrap();
        let (code, seen) = on_fresh_thread(move || {
            let class = WindowClass::register("test-taskbar").unwrap();
            let seen = Rc::new(Cell::new(0_u32));
            let count = Rc::clone(&seen);
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(move |_: HWND, message: u32, _: WPARAM, _: LPARAM| {
                    if message != taskbar_created {
                        return None;
                    }
                    count.set(count.get() + 1);
                    post_quit(-2);
                    Some(LRESULT(0))
                }),
            )
            .unwrap();
            // Posted to this window only: never broadcast in tests.
            window
                .handle()
                .post(taskbar_created, WPARAM(0), LPARAM(0))
                .unwrap();
            let code = run_message_loop().unwrap();
            (code, seen.get())
        });
        assert_eq!(code, -2);
        assert_eq!(seen, 1);
    }

    #[test]
    fn a_panicking_handler_is_contained_and_the_default_procedure_answers() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-panic").unwrap();
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(|_: HWND, message: u32, _: WPARAM, _: LPARAM| {
                    // The default hook's message for this deliberate panic is captured by the
                    // test harness.
                    assert_ne!(message, WM_GETTEXTLENGTH, "handler bug");
                    (message == PING.id()).then_some(LRESULT(4))
                }),
            )
            .unwrap();
            assert_eq!(send(window.hwnd(), WM_GETTEXTLENGTH), title_length());
            // The handler is still installed and usable after the contained panic.
            assert_eq!(send(window.hwnd(), PING.id()), 4);
            assert!(window.is_alive());
        });
    }

    #[test]
    fn reentrant_messages_get_the_default_procedure_while_the_handler_is_busy() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-reentry").unwrap();
            let calls = Rc::new(Cell::new(0_u32));
            let nested = Rc::new(Cell::new(None));
            let (calls_in, nested_in) = (Rc::clone(&calls), Rc::clone(&nested));
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(move |hwnd: HWND, message: u32, _: WPARAM, _: LPARAM| {
                    if message == PING.id() {
                        calls_in.set(calls_in.get() + 1);
                        nested_in.set(Some(send(hwnd, WM_GETTEXTLENGTH)));
                        return Some(LRESULT(1));
                    }
                    if message == WM_GETTEXTLENGTH {
                        calls_in.set(calls_in.get() + 1);
                        return Some(LRESULT(99));
                    }
                    None
                }),
            )
            .unwrap();
            assert_eq!(send(window.hwnd(), PING.id()), 1);
            assert_eq!(
                nested.get(),
                Some(title_length()),
                "DefWindowProcW answered"
            );
            assert_eq!(
                calls.get(),
                1,
                "the nested message did not reach the handler"
            );
            // Outside a handler call the same message reaches the handler.
            assert_eq!(send(window.hwnd(), WM_GETTEXTLENGTH), 99);
            assert_eq!(calls.get(), 2);
        });
    }

    struct SelfDestruct {
        freed: DropFlag,
        saw_destroy: Rc<Cell<bool>>,
        touched_after_destroy: Rc<Cell<bool>>,
    }

    impl WindowHandler for SelfDestruct {
        fn on_message(
            &mut self,
            hwnd: HWND,
            message: u32,
            _wparam: WPARAM,
            _lparam: LPARAM,
        ) -> Option<LRESULT> {
            if message == WM_DESTROY {
                self.saw_destroy.set(true);
            }
            if message != PING.id() {
                return None;
            }
            // SAFETY: the window belongs to this thread.
            unsafe { DestroyWindow(hwnd) }.unwrap();
            // The window is gone, but this handler must still be alive.
            assert!(!self.freed.0.get());
            self.touched_after_destroy.set(true);
            Some(LRESULT(5))
        }
    }

    #[test]
    fn a_window_destroyed_by_its_own_handler_is_freed_after_the_handler_returns() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-self-destroy").unwrap();
            let freed = Rc::new(Cell::new(false));
            let saw_destroy = Rc::new(Cell::new(false));
            let touched = Rc::new(Cell::new(false));
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(SelfDestruct {
                    freed: DropFlag(Rc::clone(&freed)),
                    saw_destroy: Rc::clone(&saw_destroy),
                    touched_after_destroy: Rc::clone(&touched),
                }),
            )
            .unwrap();
            assert_eq!(send(window.hwnd(), PING.id()), 5);
            assert!(touched.get());
            assert!(!window.is_alive());
            assert!(freed.get(), "the state is freed once the handler returned");
            // WM_DESTROY arrived while the handler was busy, so it went to DefWindowProcW.
            assert!(!saw_destroy.get());
            drop(window);
        });
    }

    #[test]
    fn dropping_a_window_destroys_it_and_frees_its_handler() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-drop").unwrap();
            let freed = Rc::new(Cell::new(false));
            let saw_destroy = Rc::new(Cell::new(false));
            let flag = DropFlag(Rc::clone(&freed));
            let destroyed = Rc::clone(&saw_destroy);
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(move |_: HWND, message: u32, _: WPARAM, _: LPARAM| {
                    let _keep = &flag;
                    if message == WM_DESTROY {
                        destroyed.set(true);
                    }
                    None
                }),
            )
            .unwrap();
            let hwnd = window.hwnd();
            assert!(window.is_alive());
            assert!(!freed.get());
            drop(window);
            assert!(saw_destroy.get(), "the handler saw WM_DESTROY");
            assert!(freed.get(), "the handler was freed at WM_NCDESTROY");
            // SAFETY: only queries whether the handle still names a window.
            let still_a_window = unsafe { IsWindow(Some(hwnd)) }.as_bool();
            assert!(!still_a_window);
        });
    }

    #[test]
    fn the_class_is_unregistered_after_its_last_window_and_handle_are_dropped() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-unregister").unwrap();
            let name = class.name().to_owned();
            assert!(name.starts_with("Raumakey.test-unregister."), "{name}");
            assert!(class_registered(&name));
            let window = Window::create_message_only(&class, "", ignore_all()).unwrap();
            let copy = class.clone();
            drop(class);
            drop(copy);
            assert!(class_registered(&name), "a live window keeps its class");
            drop(window);
            assert!(!class_registered(&name));
        });
    }

    /// In a DLL (the TIP), a registered class holds a server lock, because its window procedure
    /// is code of the DLL. An executable is never unloaded, so its classes hold none. The
    /// decision itself is tested in `com_server`, and when a DLL's class gives its lock back in
    /// unit tests in `src/window.rs`.
    #[test]
    fn classes_and_windows_of_an_executable_hold_no_server_lock() {
        // The counts are process-wide: hold the lock of the tests that change them.
        let _serial = crate::com_server::serial();
        assert_eq!(
            Module::containing_this_crate().unwrap(),
            Module::executable().unwrap()
        );
        on_fresh_thread(|| {
            let class = WindowClass::register("test-no-lock").unwrap();
            let window = Window::create_message_only(&class, "", ignore_all()).unwrap();
            assert_eq!(lock_count(), 0);
            drop((window, class));
        });
        assert_eq!(lock_count(), 0);
    }

    #[test]
    fn every_class_gets_a_unique_name() {
        let first = WindowClass::register("test-unique").unwrap();
        let second = WindowClass::register("test-unique").unwrap();
        assert_ne!(first.name(), second.name());
        assert!(class_registered(first.name()));
        assert!(class_registered(second.name()));
        assert_eq!(first.module(), Module::containing_this_crate().unwrap());
    }

    #[test]
    fn invalid_labels_and_titles_are_rejected() {
        assert_eq!(
            WindowClass::register("no spaces").unwrap_err().kind(),
            WinErrorKind::InvalidInput
        );
        on_fresh_thread(|| {
            let class = WindowClass::register("test-title").unwrap();
            let error = Window::create_hidden(&class, "a\0b", ignore_all()).unwrap_err();
            assert_eq!(error.kind(), WinErrorKind::InvalidInput);
        });
    }

    #[test]
    fn hidden_windows_are_invisible_tool_popups_at_top_level() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-hidden").unwrap();
            let window = Window::create_hidden(&class, TITLE, ignore_all()).unwrap();
            let hwnd = window.hwnd();
            // SAFETY: read-only queries on this thread's window.
            unsafe {
                assert!(!IsWindowVisible(hwnd).as_bool());
                let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
                assert_ne!(style & WS_POPUP.0, 0, "{style:#x}");
                assert_eq!(style & WS_VISIBLE.0, 0, "{style:#x}");
                let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                assert_ne!(ex_style & WS_EX_TOOLWINDOW.0, 0, "{ex_style:#x}");
                assert_eq!(GetAncestor(hwnd, GA_PARENT), GetDesktopWindow());
            }
            assert_eq!(send(hwnd, WM_GETTEXTLENGTH), title_length());
        });
    }

    #[test]
    fn message_only_windows_live_under_the_message_parent() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-message-only").unwrap();
            let window = Window::create_message_only(&class, TITLE, ignore_all()).unwrap();
            let name = WideString::new(class.name()).unwrap();
            // SAFETY: NUL-terminated class name; read-only lookups.
            unsafe {
                let found = FindWindowExW(Some(HWND_MESSAGE), None, name.as_pcwstr(), None);
                assert_eq!(found.unwrap(), window.hwnd());
                assert_ne!(GetAncestor(window.hwnd(), GA_PARENT), GetDesktopWindow());
            }
            // Posted messages reach it like any other window.
            let handle = window.handle();
            handle.post(PING.id(), WPARAM(0), LPARAM(0)).unwrap();
            drop(window);
            assert!(handle.post(PING.id(), WPARAM(0), LPARAM(0)).is_err());
        });
    }

    #[test]
    fn posting_to_a_destroyed_window_fails() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-gone").unwrap();
            let window = Window::create_hidden(&class, TITLE, ignore_all()).unwrap();
            let handle = window.handle();
            drop(window);
            let error = handle.post(PING.id(), WPARAM(0), LPARAM(0)).unwrap_err();
            assert_eq!(error.context(), "PostMessageW");
        });
    }

    #[test]
    fn window_handles_post_only_private_and_registered_messages() {
        let taskbar_created = taskbar_created_message().unwrap();
        let seen = on_fresh_thread(move || {
            let class = WindowClass::register("test-post-filter").unwrap();
            let seen = Rc::new(RefCell::new(Vec::new()));
            let record = Rc::clone(&seen);
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(move |_: HWND, message: u32, _: WPARAM, _: LPARAM| {
                    if message == taskbar_created || message == PING.id() {
                        record.borrow_mut().push(message);
                    }
                    None
                }),
            )
            .unwrap();
            let handle = window.handle();
            for system in [
                WM_NULL,
                WM_CLOSE,
                WM_DESTROY,
                WM_NCDESTROY,
                WM_QUIT,
                WM_USER,
                0x7FFF,
            ] {
                let error = handle.post(system, WPARAM(0), LPARAM(0)).unwrap_err();
                assert_eq!(error.kind(), WinErrorKind::InvalidInput, "{system:#x}");
            }
            handle.post(taskbar_created, WPARAM(0), LPARAM(0)).unwrap();
            handle.post(PING.id(), WPARAM(0), LPARAM(0)).unwrap();
            // WM_QUIT is retrieved only once no posted message is left.
            post_quit(0);
            assert_eq!(run_message_loop().unwrap(), 0);
            assert!(window.is_alive());
            seen.take()
        });
        assert_eq!(seen, [taskbar_created, PING.id()]);
    }

    #[test]
    fn the_message_loop_drops_posted_destroy_messages() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-forged-destroy").unwrap();
            let freed = Rc::new(Cell::new(false));
            let destroys = Rc::new(Cell::new(0_u32));
            let pings = Rc::new(Cell::new(0_u32));
            let flag = DropFlag(Rc::clone(&freed));
            let (destroys_in, pings_in) = (Rc::clone(&destroys), Rc::clone(&pings));
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(move |_: HWND, message: u32, _: WPARAM, _: LPARAM| {
                    let _keep = &flag;
                    if message == WM_DESTROY {
                        destroys_in.set(destroys_in.get() + 1);
                    }
                    if message == PING.id() {
                        pings_in.set(pings_in.get() + 1);
                    }
                    None
                }),
            )
            .unwrap();
            // Any process may post these (they carry no pointers); the system only sends them.
            post_raw(window.hwnd(), WM_DESTROY);
            post_raw(window.hwnd(), WM_NCDESTROY);
            window
                .handle()
                .post(PING.id(), WPARAM(0), LPARAM(0))
                .unwrap();
            post_quit(0);
            assert_eq!(run_message_loop().unwrap(), 0);
            assert_eq!(destroys.get(), 0, "the posted WM_DESTROY was dropped");
            assert_eq!(pings.get(), 1, "the handler is still installed");
            assert!(window.is_alive());
            assert!(!freed.get());
            let hwnd = window.hwnd();
            drop(window);
            assert_eq!(destroys.get(), 1, "the real WM_DESTROY reached the handler");
            assert!(freed.get());
            assert!(!is_window(hwnd));
        });
    }

    #[test]
    fn a_window_cut_off_from_its_handler_by_a_posted_destroy_is_still_destroyed_on_drop() {
        on_fresh_thread(|| {
            let class = WindowClass::register("test-forged-host").unwrap();
            let name = class.name().to_owned();
            let freed = Rc::new(Cell::new(false));
            let flag = DropFlag(Rc::clone(&freed));
            let window = Window::create_hidden(
                &class,
                TITLE,
                Box::new(
                    move |_: HWND, _: u32, _: WPARAM, _: LPARAM| -> Option<LRESULT> {
                        let _keep = &flag;
                        None
                    },
                ),
            )
            .unwrap();
            let hwnd = window.hwnd();
            post_raw(hwnd, WM_NCDESTROY);
            // A host's own message loop cannot tell the posted message from the real one.
            dispatch_queued_messages();
            assert!(
                freed.get(),
                "the handler was freed at the posted WM_NCDESTROY"
            );
            assert!(!window.is_alive());
            assert!(is_window(hwnd), "the window itself lives on");
            drop(window);
            assert!(!is_window(hwnd), "dropping the Window destroyed the window");
            drop(class);
            assert!(
                !class_registered(&name),
                "and its class could be unregistered"
            );
        });
    }

    #[test]
    fn handles_and_classes_may_cross_threads() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<WindowHandle>();
        assert_send_sync::<WindowClass>();
        // That `Window` is neither Send nor Sync is checked by `compile_fail` doctests on it.
    }
}
