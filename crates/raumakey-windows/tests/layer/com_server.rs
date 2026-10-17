//! `com_server`: object and lock counts (pure) and, on Windows, the generic class factory and
//! the bodies of `DllGetClassObject` and `DllCanUnloadNow`, exercised in-process with a test
//! class. Nothing is registered: the factory and the objects are plain in-process COM objects.
//!
//! The counts are process-wide, so every test here holds [`serial`] and leaves them at zero.

use std::sync::{Mutex, MutexGuard, PoisonError};

use raumakey_windows::com_server::{ObjectGuard, ServerLock, can_unload, lock_count, object_count};

static SERIAL: Mutex<()> = Mutex::new(());

/// Serializes the tests that change or read the process-wide counts (tests of other modules
/// that read them hold it too).
pub(crate) fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

#[test]
fn object_guards_count_live_objects() {
    let _serial = serial();
    assert_eq!(object_count(), 0);
    assert!(can_unload());
    let first = ObjectGuard::new();
    let second = ObjectGuard::default();
    assert_eq!(object_count(), 2);
    assert!(!can_unload());
    drop(first);
    assert_eq!(object_count(), 1);
    drop(second);
    assert_eq!(object_count(), 0);
    assert!(can_unload());
}

#[test]
fn server_locks_keep_the_server_loaded_from_any_thread() {
    let _serial = serial();
    let lock = ServerLock::new();
    assert_eq!(lock_count(), 1);
    assert_eq!(object_count(), 0);
    assert!(!can_unload());
    // A worker thread that runs code of the DLL holds a lock for its lifetime.
    let after = std::thread::spawn(move || {
        drop(lock);
        lock_count()
    })
    .join()
    .unwrap();
    assert_eq!(after, 0);
    assert!(can_unload());
}

#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    use std::ptr::{self, NonNull};

    use raumakey_windows::com::ComApartment;
    use raumakey_windows::com_server::{
        ObjectGuard, ServerLock, can_unload_now, class_factory, get_class_object, lock_count,
        object_count,
    };
    use raumakey_windows::module::Module;
    use windows::Win32::Foundation::{
        CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_NOINTERFACE, E_OUTOFMEMORY, E_POINTER,
        E_UNEXPECTED, S_FALSE, S_OK,
    };
    use windows::Win32::System::Com::{
        GetErrorInfo, IClassFactory, IErrorInfo, IPersist, IPersist_Impl, SetErrorInfo,
    };
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::System::Ole::CreateErrorInfo;
    use windows::core::{BOOL, GUID, IUnknown, Interface, implement, w};

    use super::serial;
    use crate::support::on_fresh_thread;

    /// The class the test server serves (a test value, not a Raumakey identity).
    const SERVED: GUID = GUID::from_u128(0x6d1a_3c52_8f0e_4b7d_9c21_5e4f_3a2b_1c0d);
    /// Another class.
    const OTHER: GUID = GUID::from_u128(0x2f4e_6a81_0c3d_4e5f_8a9b_7c6d_5e4f_3a2b);

    #[implement(IPersist)]
    struct TestObject {
        _counted: ObjectGuard,
    }

    impl IPersist_Impl for TestObject_Impl {
        fn GetClassID(&self) -> windows::core::Result<GUID> {
            Ok(SERVED)
        }
    }

    fn create_test_object() -> windows::core::Result<IUnknown> {
        Ok(TestObject {
            _counted: ObjectGuard::new(),
        }
        .into())
    }

    fn failing_constructor() -> windows::core::Result<IUnknown> {
        Err(windows::core::Error::from_hresult(E_OUTOFMEMORY))
    }

    fn panicking_constructor() -> windows::core::Result<IUnknown> {
        // The default hook's message for this deliberate panic is captured by the harness.
        panic!("constructor bug")
    }

    /// A non-null out pointer value, to check that the callee clears it.
    fn sentinel() -> *mut c_void {
        NonNull::<c_void>::dangling().as_ptr()
    }

    #[test]
    fn created_objects_are_reachable_through_iunknown_and_counted() {
        let _serial = serial();
        let factory = class_factory(create_test_object);
        assert_eq!(object_count(), 1, "the factory itself counts as an object");
        // SAFETY: in-process COM object; no aggregation.
        let unknown: IUnknown = unsafe { factory.CreateInstance(None::<&IUnknown>) }.unwrap();
        assert_eq!(object_count(), 2);
        let persist: IPersist = unknown.cast().unwrap();
        // SAFETY: a live in-process object.
        assert_eq!(unsafe { persist.GetClassID() }.unwrap(), SERVED);
        // SAFETY: as above; the requested interface is queried directly.
        let direct: IPersist = unsafe { factory.CreateInstance(None::<&IUnknown>) }.unwrap();
        assert_eq!(object_count(), 3);
        assert_eq!(can_unload_now(), S_FALSE);
        drop(unknown);
        assert_eq!(
            object_count(),
            3,
            "`persist` still references the first object"
        );
        drop(persist);
        drop(direct);
        assert_eq!(object_count(), 1);
        drop(factory);
        assert_eq!(object_count(), 0);
        assert_eq!(can_unload_now(), S_OK);
    }

    #[test]
    fn aggregation_is_rejected() {
        let _serial = serial();
        let factory = class_factory(create_test_object);
        let outer: IUnknown = factory.cast().unwrap();
        // SAFETY: in-process COM object; `outer` is a live IUnknown.
        let result: windows::core::Result<IUnknown> = unsafe { factory.CreateInstance(&outer) };
        assert_eq!(result.unwrap_err().code(), CLASS_E_NOAGGREGATION);
        assert_eq!(object_count(), 1, "no object was created");
        drop(outer);
        drop(factory);
        assert_eq!(object_count(), 0);
    }

    #[test]
    fn unsupported_interfaces_failures_and_panics_become_hresults() {
        let _serial = serial();
        let factory = class_factory(create_test_object);
        // SAFETY: in-process COM object; the test object does not implement IClassFactory.
        let result: windows::core::Result<IClassFactory> =
            unsafe { factory.CreateInstance(None::<&IUnknown>) };
        assert_eq!(result.unwrap_err().code(), E_NOINTERFACE);
        assert_eq!(
            object_count(),
            1,
            "the object of the failed query was released"
        );

        let failing = class_factory(failing_constructor);
        // SAFETY: as above.
        let result: windows::core::Result<IUnknown> =
            unsafe { failing.CreateInstance(None::<&IUnknown>) };
        assert_eq!(result.unwrap_err().code(), E_OUTOFMEMORY);

        let panicking = class_factory(panicking_constructor);
        // SAFETY: as above.
        let result: windows::core::Result<IUnknown> =
            unsafe { panicking.CreateInstance(None::<&IUnknown>) };
        assert_eq!(result.unwrap_err().code(), E_UNEXPECTED);
        drop((factory, failing, panicking));
        assert_eq!(object_count(), 0);
    }

    #[test]
    fn create_instance_checks_its_pointers_and_clears_the_result() {
        let _serial = serial();
        let factory = class_factory(create_test_object);
        let vtable = factory.vtable();
        // SAFETY: a raw call through the factory's own vtable with a null result pointer,
        // which the method must reject before writing anything.
        let code = unsafe {
            (vtable.CreateInstance)(
                factory.as_raw(),
                ptr::null_mut(),
                &IUnknown::IID,
                ptr::null_mut(),
            )
        };
        assert_eq!(code, E_POINTER);
        let mut object = sentinel();
        // SAFETY: as above with a null IID; `object` is writable.
        let code = unsafe {
            (vtable.CreateInstance)(factory.as_raw(), ptr::null_mut(), ptr::null(), &mut object)
        };
        assert_eq!(code, E_POINTER);
        assert!(object.is_null(), "the result is cleared first");
        drop(factory);
        assert_eq!(object_count(), 0);
    }

    #[test]
    fn factory_methods_leave_the_callers_error_object_alone() {
        let _serial = serial();
        on_fresh_thread(|| {
            let _apartment = ComApartment::sta().unwrap();
            // SAFETY: creates and fills a standalone error object.
            let created = unsafe { CreateErrorInfo() }.unwrap();
            // SAFETY: a NUL-terminated static string.
            unsafe { created.SetDescription(w!("an earlier error of the caller")) }.unwrap();
            let info: IErrorInfo = created.cast().unwrap();
            // SAFETY: sets the error object of this thread.
            unsafe { SetErrorInfo(0, &info) }.unwrap();

            let factory = class_factory(create_test_object);
            let vtable = factory.vtable();
            // SAFETY: raw calls through the factory's own vtable with valid arguments; raw
            // calls avoid the wrappers' own error-object handling.
            unsafe {
                assert_eq!(
                    (vtable.LockServer)(factory.as_raw(), BOOL::from(true)),
                    S_OK
                );
                assert_eq!(
                    (vtable.LockServer)(factory.as_raw(), BOOL::from(false)),
                    S_OK
                );
                let mut object = ptr::null_mut();
                let code = (vtable.CreateInstance)(
                    factory.as_raw(),
                    ptr::null_mut(),
                    &IUnknown::IID,
                    &mut object,
                );
                assert_eq!(code, S_OK);
                drop(IUnknown::from_raw(object));
            }
            // SAFETY: takes this thread's error object.
            let kept = unsafe { GetErrorInfo(0) }.expect("the error object is still set");
            assert_eq!(kept, info);
            drop(factory);
        });
        assert_eq!(object_count(), 0);
        assert_eq!(lock_count(), 0);
    }

    #[test]
    fn lock_server_updates_the_lock_count() {
        let _serial = serial();
        let factory = class_factory(create_test_object);
        // SAFETY: in-process COM object.
        unsafe { factory.LockServer(true) }.unwrap();
        assert_eq!(lock_count(), 1);
        drop(factory);
        assert_eq!(object_count(), 0);
        assert_eq!(can_unload_now(), S_FALSE, "a lock keeps the server loaded");
        let factory = class_factory(create_test_object);
        // SAFETY: as above.
        unsafe { factory.LockServer(false) }.unwrap();
        assert_eq!(lock_count(), 0);
        // An unbalanced unlock does not wrap the count around.
        // SAFETY: as above.
        unsafe { factory.LockServer(false) }.unwrap();
        assert_eq!(lock_count(), 0);
        drop(factory);
        assert_eq!(can_unload_now(), S_OK);
    }

    #[test]
    fn only_code_of_a_dll_takes_a_server_lock() {
        let _serial = serial();
        // The process executable is never unloaded.
        let executable = Module::executable().unwrap();
        assert!(ServerLock::for_module(executable).unwrap().is_none());
        assert_eq!(lock_count(), 0);
        assert_eq!(can_unload_now(), S_OK);
        // SAFETY: kernel32 is loaded in every process and never unloaded.
        let handle = unsafe { GetModuleHandleW(w!("kernel32.dll")) }.unwrap();
        // SAFETY: a module handle the loader returned for a module that stays loaded.
        let dll = unsafe { Module::from_raw(handle) }.unwrap();
        let lock = ServerLock::for_module(dll).unwrap();
        assert!(lock.is_some(), "a DLL is kept loaded");
        assert_eq!(lock_count(), 1);
        assert_eq!(can_unload_now(), S_FALSE);
        drop(lock);
        assert_eq!(lock_count(), 0);
        assert_eq!(can_unload_now(), S_OK);
    }

    #[test]
    fn can_unload_now_follows_objects_and_locks() {
        let _serial = serial();
        assert_eq!(can_unload_now(), S_OK);
        let object = ObjectGuard::new();
        assert_eq!(can_unload_now(), S_FALSE);
        let lock = ServerLock::new();
        drop(object);
        assert_eq!(can_unload_now(), S_FALSE);
        drop(lock);
        assert_eq!(can_unload_now(), S_OK);
    }

    #[test]
    fn get_class_object_serves_only_its_class() {
        let _serial = serial();
        let mut object = sentinel();
        // SAFETY: valid GUID pointers and a writable result.
        let code = unsafe {
            get_class_object(
                &OTHER,
                &IClassFactory::IID,
                &mut object,
                &SERVED,
                create_test_object,
            )
        };
        assert_eq!(code, CLASS_E_CLASSNOTAVAILABLE);
        assert!(object.is_null(), "the result is cleared first");
        assert_eq!(object_count(), 0);

        // SAFETY: as above.
        let code = unsafe {
            get_class_object(
                &SERVED,
                &IClassFactory::IID,
                &mut object,
                &SERVED,
                create_test_object,
            )
        };
        assert_eq!(code, S_OK);
        assert!(!object.is_null());
        // SAFETY: S_OK returned an owned IClassFactory reference in `object`.
        let factory = unsafe { IClassFactory::from_raw(object) };
        assert_eq!(object_count(), 1);
        // SAFETY: in-process COM object.
        let persist: IPersist = unsafe { factory.CreateInstance(None::<&IUnknown>) }.unwrap();
        // SAFETY: a live in-process object.
        assert_eq!(unsafe { persist.GetClassID() }.unwrap(), SERVED);
        drop((persist, factory));
        assert_eq!(object_count(), 0);

        // The factory can be requested as IUnknown, but not as an interface it lacks.
        let mut unknown = sentinel();
        // SAFETY: as above.
        let code = unsafe {
            get_class_object(
                &SERVED,
                &IUnknown::IID,
                &mut unknown,
                &SERVED,
                create_test_object,
            )
        };
        assert_eq!(code, S_OK);
        // SAFETY: S_OK returned an owned IUnknown reference.
        drop(unsafe { IUnknown::from_raw(unknown) });
        let mut missing = sentinel();
        // SAFETY: as above.
        let code = unsafe {
            get_class_object(
                &SERVED,
                &IPersist::IID,
                &mut missing,
                &SERVED,
                create_test_object,
            )
        };
        assert_eq!(code, E_NOINTERFACE);
        assert!(missing.is_null());
        assert_eq!(object_count(), 0);
    }

    #[test]
    fn get_class_object_rejects_null_pointers() {
        let _serial = serial();
        // SAFETY: null pointers are what is being tested; nothing is written through them.
        let code = unsafe {
            get_class_object(
                &SERVED,
                &IClassFactory::IID,
                ptr::null_mut(),
                &SERVED,
                create_test_object,
            )
        };
        assert_eq!(code, E_POINTER);
        let mut object = sentinel();
        // SAFETY: a null CLSID with a writable result.
        let code = unsafe {
            get_class_object(
                ptr::null(),
                &IClassFactory::IID,
                &mut object,
                &SERVED,
                create_test_object,
            )
        };
        assert_eq!(code, E_POINTER);
        assert!(object.is_null());
        let mut object = sentinel();
        // SAFETY: a null IID with a writable result.
        let code = unsafe {
            get_class_object(
                &SERVED,
                ptr::null(),
                &mut object,
                &SERVED,
                create_test_object,
            )
        };
        assert_eq!(code, E_POINTER);
        assert!(object.is_null());
        assert_eq!(object_count(), 0);
    }
}
