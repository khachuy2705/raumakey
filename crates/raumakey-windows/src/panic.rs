//! Panic containment for FFI, COM, Win32 callback, and thread entry points.
//!
//! No unwind may cross an ABI boundary (plan §0.3). Every exported function, COM method, window
//! procedure, hook procedure, and thread entry point runs its body inside one of the guards
//! below; a panic becomes a fallback value (`E_UNEXPECTED` for HRESULT entry points).
//!
//! [`install_quiet_panic_hook`] replaces the default hook, which prints the panic message to
//! stderr. The message may have been formatted from user content, so the quiet hook prints
//! nothing and records only the source location (`file:line`) and a counter.

use core::fmt;
use std::cell::RefCell;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, Once};

/// Source location of a panic. It never contains the panic message.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PanicLocation {
    /// Source file as recorded by the compiler.
    pub file: String,
    /// Line number.
    pub line: u32,
}

impl fmt::Display for PanicLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.file, self.line)
    }
}

static INSTALL: Once = Once::new();
static PANICS: AtomicU64 = AtomicU64::new(0);
static LAST: Mutex<Option<PanicLocation>> = Mutex::new(None);

thread_local! {
    static THREAD_LAST: RefCell<Option<PanicLocation>> = const { RefCell::new(None) };
}

/// Installs (once per process) a panic hook that prints nothing and records only the location
/// and a counter. Safe to call from every entry point; later calls do nothing.
pub fn install_quiet_panic_hook() {
    INSTALL.call_once(|| {
        panic::set_hook(Box::new(|info| {
            PANICS.fetch_add(1, Ordering::Relaxed);
            let Some(location) = info.location() else {
                return;
            };
            let location = PanicLocation {
                file: location.file().to_owned(),
                line: location.line(),
            };
            // Never block or panic inside the hook: skip the record if it is busy.
            if let Ok(mut last) = LAST.try_lock() {
                *last = Some(location.clone());
            }
            let _ = THREAD_LAST.try_with(|cell| {
                if let Ok(mut slot) = cell.try_borrow_mut() {
                    *slot = Some(location);
                }
            });
        }));
    });
}

/// Location of the most recent panic in the process seen by the quiet hook.
pub fn last_panic_location() -> Option<PanicLocation> {
    match LAST.lock() {
        Ok(last) => last.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

/// Location of the most recent panic on the calling thread seen by the quiet hook.
pub fn thread_last_panic_location() -> Option<PanicLocation> {
    THREAD_LAST
        .try_with(|cell| cell.try_borrow().ok().and_then(|slot| slot.clone()))
        .ok()
        .flatten()
}

/// Panics seen by the quiet hook since it was installed.
pub fn panic_count() -> u64 {
    PANICS.load(Ordering::Relaxed)
}

/// Runs `body`; returns `fallback` if it panics. The panic payload is dropped without being
/// inspected or printed.
pub fn guard_value<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    match panic::catch_unwind(AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(payload) => {
            // Dropping a payload may itself panic; contain that too.
            let _ = panic::catch_unwind(AssertUnwindSafe(move || drop(payload)));
            fallback
        }
    }
}

/// HRESULT entry points: `Ok` → `S_OK`, `Err(e)` → its code (never a success code), panic →
/// `E_UNEXPECTED`. Rich error information carried by `e` is moved to the thread's error object.
#[cfg(windows)]
pub fn guard_hresult(body: impl FnOnce() -> windows::core::Result<()>) -> windows::core::HRESULT {
    use windows::core::HRESULT;
    guard_value(HRESULT(crate::error::E_UNEXPECTED), || match body() {
        Ok(()) => HRESULT(crate::error::S_OK),
        Err(error) => {
            let code: HRESULT = error.into();
            if code.is_ok() {
                HRESULT(crate::error::E_FAIL)
            } else {
                code
            }
        }
    })
}

/// HRESULT entry points that compute their own code (such as `S_FALSE`); panic →
/// `E_UNEXPECTED`.
#[cfg(windows)]
pub fn guard_hresult_raw(body: impl FnOnce() -> windows::core::HRESULT) -> windows::core::HRESULT {
    guard_value(windows::core::HRESULT(crate::error::E_UNEXPECTED), body)
}

/// Window and hook procedures: panic → `fallback`.
#[cfg(windows)]
pub fn guard_lresult(
    fallback: windows::Win32::Foundation::LRESULT,
    body: impl FnOnce() -> windows::Win32::Foundation::LRESULT,
) -> windows::Win32::Foundation::LRESULT {
    guard_value(fallback, body)
}

/// `BOOL` entry points (such as `DllMain`): panic → `FALSE`.
#[cfg(windows)]
pub fn guard_bool(body: impl FnOnce() -> bool) -> windows::core::BOOL {
    windows::core::BOOL::from(guard_value(false, body))
}
