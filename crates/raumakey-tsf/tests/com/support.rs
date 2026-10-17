//! Helpers shared by the COM tests.

use core::ffi::c_void;
use std::path::PathBuf;
use std::ptr;
use std::sync::{Mutex, MutexGuard, PoisonError};

use raumakey_model::identity::TIP_CLSID;
use raumakey_tip::DllGetClassObject;
use raumakey_windows::com::ComApartment;
use windows::Win32::Foundation::S_OK;
use windows::Win32::System::Com::IClassFactory;
use windows::Win32::UI::TextServices::ITfTextInputProcessorEx;
use windows::core::{GUID, IUnknown, Interface};

/// The CLSID of the text service as a COM `GUID`.
pub const TIP: GUID = GUID::from_u128(TIP_CLSID.to_u128());

static SERIAL: Mutex<()> = Mutex::new(());

/// Serializes the tests that create objects of this server or read its process-wide object
/// and lock counts (`raumakey_windows::com_server`); every test of this binary that touches
/// the server holds it.
pub fn serial() -> MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Runs `body` on a new thread inside a single-threaded apartment, as TSF calls a text
/// service, and returns its result. The apartment is left (`CoUninitialize`) after `body`
/// returned, so every COM object `body` created must be released by then. A panic (a failed
/// assertion) fails the calling test.
pub fn on_sta<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    let joined = std::thread::spawn(move || {
        let apartment = ComApartment::sta().expect("the thread enters an STA");
        let value = body();
        drop(apartment);
        value
    })
    .join();
    match joined {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

/// The class factory of the text service, from `DllGetClassObject`.
pub fn class_factory() -> IClassFactory {
    let mut object: *mut c_void = ptr::null_mut();
    // SAFETY: valid GUID pointers and a writable result pointer.
    let code = unsafe { DllGetClassObject(&TIP, &IClassFactory::IID, &mut object) };
    assert_eq!(code, S_OK);
    assert!(!object.is_null());
    // SAFETY: on success the result holds a new IClassFactory reference, which this value
    // takes over.
    unsafe { IClassFactory::from_raw(object) }
}

/// A new text service, created through the class factory of `DllGetClassObject`.
pub fn text_service() -> ITfTextInputProcessorEx {
    let factory = class_factory();
    // SAFETY: an in-process class factory; no aggregation.
    unsafe { factory.CreateInstance(None::<&IUnknown>) }.expect("the text service is created")
}

/// The reference count of the object behind `object`, read with a balanced `AddRef` and
/// `Release` (exact for the objects of this server, which count with `windows-core`).
pub fn references(object: &impl Interface) -> u32 {
    let raw = object.as_raw();
    // SAFETY: every COM interface pointer starts with the IUnknown methods, and `raw` is live
    // while `object` is.
    let unknown = unsafe { IUnknown::from_raw_borrowed(&raw) }.expect("a live interface");
    // SAFETY: the AddRef is balanced by the Release, whose result is the count before the
    // AddRef.
    unsafe {
        (unknown.vtable().AddRef)(unknown.as_raw());
        (unknown.vtable().Release)(unknown.as_raw())
    }
}

/// `raumakey_tip.dll` as Cargo built it for this test run: the library is compiled once for
/// both of its crate types, so the DLL lies in `deps` beside this test binary.
pub fn built_dll() -> PathBuf {
    let exe = std::env::current_exe().expect("the test binary has a path");
    let dll = exe
        .parent()
        .expect("the test binary lies in a directory")
        .join("raumakey_tip.dll");
    assert!(dll.is_file(), "{} was not built", dll.display());
    dll
}
