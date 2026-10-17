//! Support for an in-process COM server: the TIP (plan §10.3, design §12: "DLL unload
//! eligibility reflects outstanding COM object and lock counts").
//!
//! The counts are pure and process-wide. They belong to the module that links this crate, so
//! `raumakey_tip.dll` has counts of its own:
//!
//! - an [`ObjectGuard`] counts one live COM object for as long as it exists; every COM object
//!   the server hands out holds one (the class factories of [`class_factory`] do);
//! - a [`ServerLock`] holds one server lock for as long as it exists, for in-process code that
//!   must keep the DLL loaded without holding an object (a worker thread running DLL code);
//!   `IClassFactory::LockServer` adds and removes locks on the same count;
//! - [`can_unload`] is true when both counts are zero.
//!
//! On Windows:
//!
//! - [`ServerLock::for_module`] takes a lock only for code of a DLL. Every window class
//!   registered by the DLL holds one ([`WindowClass`](crate::window::WindowClass)), because
//!   the system calls its window procedure for as long as the class or a window of it exists.
//! - [`class_factory`] builds a generic `IClassFactory` (with the `windows-core` `implement`
//!   macro) from a [`Constructor`]. `CreateInstance` rejects aggregation with
//!   `CLASS_E_NOAGGREGATION`, creates the object, and queries it for the requested interface;
//!   `LockServer` adds or removes a server lock.
//! - [`get_class_object`] is the body of `DllGetClassObject`, and [`can_unload_now`] the
//!   result of `DllCanUnloadNow` (`S_OK` only when both counts are zero, `S_FALSE` otherwise).
//!
//! Every COM method and both entry points contain panics: a panic becomes `E_UNEXPECTED`, and no
//! unwind crosses the COM boundary.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Live COM objects of this server.
static OBJECTS: AtomicUsize = AtomicUsize::new(0);
/// Server locks of this server.
static LOCKS: AtomicUsize = AtomicUsize::new(0);

/// Number of live COM objects (live [`ObjectGuard`]s).
pub fn object_count() -> usize {
    OBJECTS.load(Ordering::SeqCst)
}

/// Number of server locks ([`ServerLock`]s and `IClassFactory::LockServer(TRUE)` calls not yet
/// undone).
pub fn lock_count() -> usize {
    LOCKS.load(Ordering::SeqCst)
}

/// True when no object is alive and no lock is held: the DLL may be unloaded.
pub fn can_unload() -> bool {
    object_count() == 0 && lock_count() == 0
}

fn acquire(counter: &AtomicUsize) {
    counter.fetch_add(1, Ordering::SeqCst);
}

/// Removes one count; a count that is already zero stays zero.
fn release(counter: &AtomicUsize) {
    let _ = counter.try_update(Ordering::SeqCst, Ordering::SeqCst, |count| {
        count.checked_sub(1)
    });
}

/// Counts one live COM object of this server for as long as it exists. Embed one in every COM
/// object the server creates.
///
/// A guard that is dropped at once counts nothing, so the compiler rejects an unused one:
///
/// ```compile_fail
/// #![deny(unused_must_use)]
/// raumakey_windows::com_server::ObjectGuard::new();
/// ```
#[derive(Debug)]
#[must_use = "the object is counted only while the guard lives"]
pub struct ObjectGuard {
    _private: (),
}

impl ObjectGuard {
    /// Counts a new object.
    pub fn new() -> Self {
        acquire(&OBJECTS);
        Self { _private: () }
    }
}

impl Default for ObjectGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ObjectGuard {
    fn drop(&mut self) {
        release(&OBJECTS);
    }
}

/// Holds one server lock for as long as it exists. It may move to (and be dropped on) any
/// thread.
///
/// A lock that is dropped at once keeps nothing loaded, so the compiler rejects an unused one:
///
/// ```compile_fail
/// #![deny(unused_must_use)]
/// raumakey_windows::com_server::ServerLock::new();
/// ```
#[derive(Debug)]
#[must_use = "the lock is released when it is dropped"]
pub struct ServerLock {
    _private: (),
}

impl ServerLock {
    /// Takes a server lock.
    pub fn new() -> Self {
        acquire(&LOCKS);
        Self { _private: () }
    }
}

impl Default for ServerLock {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ServerLock {
    fn drop(&mut self) {
        release(&LOCKS);
    }
}

#[cfg(windows)]
pub use win::{Constructor, can_unload_now, class_factory, get_class_object};

#[cfg(windows)]
mod win {
    use core::ffi::c_void;
    use core::ptr;

    use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
    use windows::core::{BOOL, Error, GUID, HRESULT, IUnknown, Interface, Ref, implement};

    use super::{LOCKS, ObjectGuard, ServerLock, acquire, release};
    use crate::error::{
        CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_POINTER, E_UNEXPECTED, S_FALSE, S_OK,
        WinResult,
    };
    use crate::module::Module;
    use crate::panic::{guard_hresult_raw, guard_value};

    /// Creates one object of the served class and returns its `IUnknown`. The object must
    /// hold an [`ObjectGuard`](super::ObjectGuard) for its lifetime.
    pub type Constructor = fn() -> windows::core::Result<IUnknown>;

    /// The generic class factory. Counted as an object, so a client holding a factory keeps
    /// the DLL loaded.
    #[implement(IClassFactory)]
    struct ClassFactory {
        constructor: Constructor,
        _counted: ObjectGuard,
    }

    /// An error carrying only `code`. Unlike `From<HRESULT>`, it does not take (and so clear)
    /// the thread's current COM error object.
    fn error(code: i32) -> Error {
        Error::from_hresult(HRESULT(code))
    }

    impl IClassFactory_Impl for ClassFactory_Impl {
        fn CreateInstance(
            &self,
            outer: Ref<IUnknown>,
            iid: *const GUID,
            object: *mut *mut c_void,
        ) -> windows::core::Result<()> {
            let constructor = self.constructor;
            let aggregated = !outer.is_null();
            guard_value(Err(error(E_UNEXPECTED)), || {
                // SAFETY: COM passes `iid` and `object` as the caller of CreateInstance gave
                // them: each null or valid.
                unsafe { create_instance(constructor, aggregated, iid, object) }
            })
        }

        fn LockServer(&self, lock: BOOL) -> windows::core::Result<()> {
            guard_value(Err(error(E_UNEXPECTED)), || {
                if lock.as_bool() {
                    acquire(&LOCKS);
                } else {
                    release(&LOCKS);
                }
                Ok(())
            })
        }
    }

    /// The body of `IClassFactory::CreateInstance`.
    ///
    /// # Safety
    ///
    /// `iid` must be null or point to a GUID; `object` must be null or writable.
    unsafe fn create_instance(
        constructor: Constructor,
        aggregated: bool,
        iid: *const GUID,
        object: *mut *mut c_void,
    ) -> windows::core::Result<()> {
        if object.is_null() {
            return Err(error(E_POINTER));
        }
        // SAFETY: `object` is writable per the contract.
        unsafe { object.write(ptr::null_mut()) };
        if iid.is_null() {
            return Err(error(E_POINTER));
        }
        if aggregated {
            return Err(error(CLASS_E_NOAGGREGATION));
        }
        let instance = constructor()?;
        // SAFETY: `iid` points to a GUID and `object` is writable; on success the result holds
        // a new reference.
        unsafe { instance.query(iid, object) }.ok()
    }

    /// A class factory whose `CreateInstance` builds objects with `constructor`.
    pub fn class_factory(constructor: Constructor) -> IClassFactory {
        ClassFactory {
            constructor,
            _counted: ObjectGuard::new(),
        }
        .into()
    }

    impl ServerLock {
        /// A server lock for code of `module` that the system may still call, such as the
        /// window procedure of a registered window class: `Some` when `module` is a DLL, which
        /// must not be unloaded meanwhile, and `None` for the process executable, which is
        /// never unloaded.
        pub fn for_module(module: Module) -> WinResult<Option<Self>> {
            Ok((module != Module::executable()?).then(Self::new))
        }
    }

    /// The `DllCanUnloadNow` result: `S_OK` when no object is alive and no lock is held,
    /// `S_FALSE` otherwise.
    pub fn can_unload_now() -> HRESULT {
        if super::can_unload() {
            HRESULT(S_OK)
        } else {
            HRESULT(S_FALSE)
        }
    }

    /// The body of `DllGetClassObject` for a server of the single class `served`, whose
    /// objects `constructor` creates.
    ///
    /// Returns `E_POINTER` for a null `ppv`, clears `*ppv`, returns `E_POINTER` for a null
    /// `rclsid` or `riid` and `CLASS_E_CLASSNOTAVAILABLE` for any other class, and otherwise
    /// queries a new [`class_factory`] for `riid` (usually `IClassFactory`). A panic becomes
    /// `E_UNEXPECTED`.
    ///
    /// # Safety
    ///
    /// `rclsid` and `riid` must each be null or point to a GUID; `ppv` must be null or point
    /// to writable storage for an interface pointer.
    pub unsafe fn get_class_object(
        rclsid: *const GUID,
        riid: *const GUID,
        ppv: *mut *mut c_void,
        served: &GUID,
        constructor: Constructor,
    ) -> HRESULT {
        guard_hresult_raw(|| {
            if ppv.is_null() {
                return HRESULT(E_POINTER);
            }
            // SAFETY: `ppv` is writable per the contract.
            unsafe { ppv.write(ptr::null_mut()) };
            if rclsid.is_null() || riid.is_null() {
                return HRESULT(E_POINTER);
            }
            // SAFETY: `rclsid` points to a GUID per the contract.
            if unsafe { rclsid.read() } != *served {
                return HRESULT(CLASS_E_CLASSNOTAVAILABLE);
            }
            let factory = class_factory(constructor);
            // SAFETY: `riid` points to a GUID and `ppv` is writable; on success `*ppv` holds a
            // new reference and `factory` releases its own.
            unsafe { factory.query(riid, ppv) }
        })
    }
}
