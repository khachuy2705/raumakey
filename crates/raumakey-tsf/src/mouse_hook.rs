//! The `WH_MOUSE` hook of the TSF thread (plan §10.3, matrix #19).
//!
//! The text service installs the hook when TSF activates it and removes it when TSF
//! deactivates it. It is a thread hook (`SetWindowsHookExW(WH_MOUSE, …, NULL,
//! GetCurrentThreadId())`), never a global one: Windows calls [`procedure`] on this thread for
//! the mouse messages that this thread retrieves, the clicks in the host's own windows. The
//! procedure reports every button press ([`is_button_press`])
//! to the handler of the thread and passes every message on unchanged; it never swallows a
//! click. Everything it does before it calls the next hook (the check of the message, the
//! handler, and the release of the handler, which drops a handler that removed the hook)
//! runs inside a `raumakey_windows::panic` guard: a panic there is dropped, and the message
//! still reaches the next hook.
//!
//! One hook per thread: TSF activates one instance of a text service per thread, and a
//! second installation on the same thread is refused while the first one is in place.

use core::cell::RefCell;
use std::rc::Rc;

use raumakey_windows::panic::guard_value;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE,
};

use crate::adapter::mouse::is_button_press;

/// Why the hook was not installed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InstallError {
    /// A hook is installed on this thread already.
    Busy,
    /// `SetWindowsHookExW` failed with this `HRESULT`.
    Failed(i32),
}

/// The hook of this thread.
struct Installed {
    hook: HHOOK,
    /// Identifies the text service that installed it.
    owner: usize,
    /// Called for every button press.
    on_press: Rc<dyn Fn()>,
}

thread_local! {
    static HOOK: RefCell<Option<Installed>> = const { RefCell::new(None) };
}

/// Installs the hook of the calling thread for `owner`; `on_press` runs for every mouse
/// button press that the thread retrieves.
pub(crate) fn install(owner: usize, on_press: Rc<dyn Fn()>) -> Result<(), InstallError> {
    HOOK.try_with(|slot| {
        let mut slot = slot.try_borrow_mut().map_err(|_| InstallError::Busy)?;
        if slot.is_some() {
            return Err(InstallError::Busy);
        }
        // SAFETY: no preconditions.
        let thread = unsafe { GetCurrentThreadId() };
        // SAFETY: a hook procedure of this module for the calling thread of this process; with
        // a thread of this process, the module handle must be null. The hook is removed
        // before the text service, which keeps this DLL loaded, is released.
        let hook = unsafe { SetWindowsHookExW(WH_MOUSE, Some(procedure), None, thread) }
            .map_err(|error| InstallError::Failed(error.code().0))?;
        *slot = Some(Installed {
            hook,
            owner,
            on_press,
        });
        Ok(())
    })
    .unwrap_or(Err(InstallError::Busy))
}

/// Removes the hook of the calling thread when `owner` installed it, and says whether it did.
pub(crate) fn remove(owner: usize) -> bool {
    let removed = HOOK
        .try_with(|slot| {
            let mut slot = slot.try_borrow_mut().ok()?;
            slot.take_if(|installed| installed.owner == owner)
        })
        .ok()
        .flatten();
    match removed {
        Some(installed) => {
            // SAFETY: the hook this module installed on this thread, removed once. A failure
            // means that Windows removed it already.
            let _ = unsafe { UnhookWindowsHookEx(installed.hook) };
            true
        }
        None => false,
    }
}

/// Whether a text service installed the hook of the calling thread.
pub fn is_installed() -> bool {
    HOOK.try_with(|slot| slot.try_borrow().is_ok_and(|slot| slot.is_some()))
        .unwrap_or(false)
}

/// The handler of the calling thread, taken out of its slot so that it may remove the hook.
fn handler() -> Option<Rc<dyn Fn()>> {
    HOOK.try_with(|slot| {
        let slot = slot.try_borrow().ok()?;
        slot.as_ref()
            .map(|installed| Rc::clone(&installed.on_press))
    })
    .ok()
    .flatten()
}

/// The hook procedure: reports a button press (`code` is `HC_ACTION`, `wparam` the mouse
/// message) to the handler of the thread, then calls the next hook.
///
/// It is public so that tests can deliver a mouse message the way Windows does; a test
/// thread has no mouse input of its own.
///
/// # Safety
///
/// The arguments are passed on to the next hook of the thread: `lparam` must be what Windows
/// passes with `code`, a `MOUSEHOOKSTRUCT` that lives for the call when `code` is
/// `HC_ACTION`.
pub unsafe extern "system" fn procedure(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // Everything but the call of the next hook runs in the guard, the release of the
    // handler too: when the handler removed the hook, that release drops it.
    guard_value((), || {
        // Only the message number is read; `lparam` is never dereferenced here.
        if code == HC_ACTION as i32
            && u32::try_from(wparam.0).is_ok_and(is_button_press)
            && let Some(on_press) = handler()
        {
            on_press();
        }
    });
    // SAFETY: the caller's arguments are passed on unchanged, under the same contract.
    unsafe { CallNextHookEx(None::<HHOOK>, code, wparam, lparam) }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::rc::Rc;

    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        HC_ACTION, HC_NOREM, MOUSEHOOKSTRUCT, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE,
        WM_NCRBUTTONDOWN,
    };

    use super::{InstallError, install, is_installed, procedure, remove};

    /// Delivers `message` with `code` to the hook procedure, as Windows does.
    fn deliver(code: u32, message: u32) {
        let mut info = MOUSEHOOKSTRUCT::default();
        // SAFETY: the procedure with a hook code, a mouse message, and a record that lives
        // for the call.
        unsafe {
            procedure(
                code as i32,
                WPARAM(message as usize),
                LPARAM(&raw mut info as isize),
            )
        };
    }

    /// A handler that counts the presses it gets.
    fn counter() -> (Rc<Cell<u32>>, Rc<dyn Fn()>) {
        let presses = Rc::new(Cell::new(0));
        let count = Rc::clone(&presses);
        (presses, Rc::new(move || count.set(count.get() + 1)))
    }

    /// Runs `body` on a thread of its own, so that no other test shares its hook.
    fn on_thread(body: impl FnOnce() + Send + 'static) {
        if let Err(payload) = std::thread::spawn(body).join() {
            std::panic::resume_unwind(payload);
        }
    }

    #[test]
    fn every_button_press_reaches_the_handler_once() {
        on_thread(|| {
            let (presses, handler) = counter();
            install(1, handler).expect("installed");
            assert!(is_installed());
            deliver(HC_ACTION, WM_LBUTTONDOWN);
            deliver(HC_ACTION, WM_NCRBUTTONDOWN);
            assert_eq!(presses.get(), 2);
            // A message only peeked at comes again with HC_ACTION when it is retrieved.
            deliver(HC_NOREM, WM_LBUTTONDOWN);
            deliver(HC_ACTION, WM_MOUSEMOVE);
            deliver(HC_ACTION, WM_LBUTTONUP);
            assert_eq!(presses.get(), 2);
            assert!(remove(1));
            assert!(!is_installed());
            deliver(HC_ACTION, WM_LBUTTONDOWN);
            assert_eq!(presses.get(), 2, "a removed hook reports nothing");
        });
    }

    #[test]
    fn the_hook_belongs_to_the_thread_that_installed_it() {
        on_thread(|| {
            let (presses, handler) = counter();
            install(1, handler).expect("installed");
            let elsewhere = std::thread::spawn(|| {
                deliver(HC_ACTION, WM_LBUTTONDOWN);
                is_installed()
            })
            .join()
            .expect("the thread ran");
            assert!(!elsewhere);
            assert_eq!(presses.get(), 0);
            assert!(remove(1));
        });
    }

    #[test]
    fn one_hook_per_thread_and_only_its_owner_removes_it() {
        on_thread(|| {
            let (first, handler) = counter();
            install(1, handler).expect("installed");
            let (second, other) = counter();
            assert_eq!(install(2, other), Err(InstallError::Busy));
            assert!(!remove(2), "not the owner");
            assert!(is_installed());
            deliver(HC_ACTION, WM_LBUTTONDOWN);
            assert_eq!((first.get(), second.get()), (1, 0));
            assert!(remove(1));
            assert!(!remove(1), "removed already");
            let (third, handler) = counter();
            install(2, handler).expect("installed again");
            deliver(HC_ACTION, WM_LBUTTONDOWN);
            assert_eq!(third.get(), 1);
            assert!(remove(2));
        });
    }

    #[test]
    fn a_panicking_handler_never_unwinds_into_windows() {
        on_thread(|| {
            install(1, Rc::new(|| panic!("handler bug"))).expect("installed");
            deliver(HC_ACTION, WM_LBUTTONDOWN);
            assert!(is_installed(), "the hook stays");
            assert!(remove(1));
        });
    }

    #[test]
    fn a_handler_may_remove_the_hook() {
        on_thread(|| {
            install(1, Rc::new(|| assert!(remove(1)))).expect("installed");
            deliver(HC_ACTION, WM_LBUTTONDOWN);
            assert!(!is_installed());
        });
    }

    /// A value whose drop panics.
    struct PanicsWhenDropped;

    impl Drop for PanicsWhenDropped {
        fn drop(&mut self) {
            panic!("dropped");
        }
    }

    #[test]
    fn dropping_a_handler_after_its_call_never_unwinds_into_windows() {
        on_thread(|| {
            let value = PanicsWhenDropped;
            // The handler removes the hook, so the procedure holds the last reference to it
            // and drops it, and `value` with it, after the call.
            let handler = Rc::new(move || {
                let _held = &value;
                assert!(remove(1));
            });
            install(1, handler).expect("installed");
            deliver(HC_ACTION, WM_LBUTTONDOWN);
            assert!(!is_installed());
        });
    }

    #[test]
    fn without_a_hook_the_procedure_only_passes_the_message_on() {
        on_thread(|| {
            assert!(!is_installed());
            deliver(HC_ACTION, WM_LBUTTONDOWN);
        });
    }
}
