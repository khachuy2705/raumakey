//! `DllGetClassObject` and `DllCanUnloadNow`, called in-process through the library.

use core::ffi::c_void;
use std::ptr::{self, NonNull};

use raumakey_tip::{DllCanUnloadNow, DllGetClassObject};
use raumakey_windows::com_server::{lock_count, object_count};
use windows::Win32::Foundation::{
    CLASS_E_CLASSNOTAVAILABLE, E_NOINTERFACE, E_POINTER, S_FALSE, S_OK,
};
use windows::Win32::System::Com::IClassFactory;
use windows::Win32::UI::TextServices::{ITfTextInputProcessor, ITfTextInputProcessorEx};
use windows::core::{GUID, IUnknown, Interface};

use crate::support::{TIP, class_factory, on_sta, serial, text_service};

/// A class that this server does not serve (a test value, not a Raumakey identity).
const OTHER: GUID = GUID::from_u128(0x5d8e_41a7_93c2_4f06_b1e5_2a7c_9d30_6f18);

/// A non-null result pointer, to check that the callee clears it.
fn sentinel() -> *mut c_void {
    NonNull::<c_void>::dangling().as_ptr()
}

#[test]
fn the_tip_clsid_is_served_by_a_class_factory() {
    let _serial = serial();
    on_sta(|| {
        let factory = class_factory();
        assert_eq!(object_count(), 1, "the factory counts as an object");
        let mut object = sentinel();
        // SAFETY: valid GUID pointers and a writable result pointer.
        let code = unsafe { DllGetClassObject(&TIP, &IUnknown::IID, &mut object) };
        assert_eq!(code, S_OK);
        // SAFETY: on success the result holds a new reference, which this value takes over.
        let unknown = unsafe { IUnknown::from_raw(object) };
        assert!(unknown.cast::<IClassFactory>().is_ok());
        drop((factory, unknown));
        assert_eq!(object_count(), 0);
    });
}

#[test]
fn other_classes_are_not_available_and_the_result_is_cleared() {
    let _serial = serial();
    on_sta(|| {
        let mut object = sentinel();
        // SAFETY: valid GUID pointers and a writable result pointer.
        let code = unsafe { DllGetClassObject(&OTHER, &IClassFactory::IID, &mut object) };
        assert_eq!(code, CLASS_E_CLASSNOTAVAILABLE);
        assert!(object.is_null());
        assert_eq!(object_count(), 0);
    });
}

#[test]
fn null_pointers_are_rejected() {
    let _serial = serial();
    on_sta(|| {
        // SAFETY: a null result pointer, which the export must reject before writing.
        let code = unsafe { DllGetClassObject(&TIP, &IClassFactory::IID, ptr::null_mut()) };
        assert_eq!(code, E_POINTER);
        let mut object = sentinel();
        // SAFETY: a null CLSID and a writable result pointer.
        let code = unsafe { DllGetClassObject(ptr::null(), &IClassFactory::IID, &mut object) };
        assert_eq!(code, E_POINTER);
        assert!(object.is_null(), "the result is cleared first");
        let mut object = sentinel();
        // SAFETY: a null IID and a writable result pointer.
        let code = unsafe { DllGetClassObject(&TIP, ptr::null(), &mut object) };
        assert_eq!(code, E_POINTER);
        assert!(object.is_null());
        assert_eq!(object_count(), 0);
    });
}

#[test]
fn the_class_object_answers_only_as_a_class_factory() {
    let _serial = serial();
    on_sta(|| {
        for iid in [ITfTextInputProcessorEx::IID, ITfTextInputProcessor::IID] {
            let mut object = sentinel();
            // SAFETY: valid GUID pointers and a writable result pointer.
            let code = unsafe { DllGetClassObject(&TIP, &iid, &mut object) };
            assert_eq!(code, E_NOINTERFACE);
            assert!(object.is_null());
        }
        assert_eq!(object_count(), 0, "the refused factory was released");
    });
}

#[test]
fn dll_can_unload_now_waits_for_every_object_and_server_lock() {
    let _serial = serial();
    on_sta(|| {
        // SAFETY: the export takes no arguments.
        let can_unload = || unsafe { DllCanUnloadNow() };
        assert_eq!(can_unload(), S_OK);

        let factory = class_factory();
        assert_eq!(can_unload(), S_FALSE, "a live class factory");
        let service = text_service();
        drop(factory);
        assert_eq!(can_unload(), S_FALSE, "a live text service");
        drop(service);
        assert_eq!(can_unload(), S_OK);

        let factory = class_factory();
        // SAFETY: an in-process class factory.
        unsafe { factory.LockServer(true) }.expect("the server is locked");
        drop(factory);
        assert_eq!(object_count(), 0);
        assert_eq!(lock_count(), 1);
        assert_eq!(can_unload(), S_FALSE, "a server lock without objects");
        let factory = class_factory();
        // SAFETY: as above.
        unsafe { factory.LockServer(false) }.expect("the server is unlocked");
        drop(factory);
        assert_eq!(can_unload(), S_OK);
    });
}
