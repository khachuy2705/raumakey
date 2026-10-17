//! The built `raumakey_tip.dll` itself: loaded with `LoadLibraryW` and called through the
//! exports that `GetProcAddress` finds by name, as COM does. The DLL has its own object and
//! lock counts, apart from those of this test binary, so these tests do not take
//! [`serial`](crate::support::serial).

use core::ffi::{CStr, c_void};
use std::path::Path;
use std::ptr;

use raumakey_windows::strings::WideString;
use windows::Win32::Foundation::{FreeLibrary, HMODULE, S_FALSE, S_OK};
use windows::Win32::System::Com::IClassFactory;
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
use windows::Win32::UI::TextServices::ITfTextInputProcessorEx;
use windows::core::{GUID, HRESULT, IUnknown, Interface, PCSTR};

use crate::support::{TIP, built_dll, on_sta};

/// `DllGetClassObject`.
type GetClassObject =
    unsafe extern "system" fn(*const GUID, *const GUID, *mut *mut c_void) -> HRESULT;
/// `DllCanUnloadNow`, `DllRegisterServer`, and `DllUnregisterServer`.
type NoArguments = unsafe extern "system" fn() -> HRESULT;

/// A loaded copy of the DLL, freed on drop.
pub struct LoadedDll(HMODULE);

impl LoadedDll {
    /// Loads the DLL at `path`; its code runs in this process.
    pub fn load(path: &Path) -> Self {
        let path = WideString::from_os(path.as_os_str()).expect("a path without NUL");
        // SAFETY: a NUL-terminated path of a DLL built from this crate.
        let module = unsafe { LoadLibraryW(path.as_pcwstr()) }.expect("the DLL loads");
        Self(module)
    }

    /// The export `name` with no arguments, or `None` when the DLL does not export it.
    pub fn no_arguments(&self, name: &CStr) -> Option<NoArguments> {
        // SAFETY: a loaded module and a NUL-terminated name.
        let address = unsafe { GetProcAddress(self.0, PCSTR(name.as_ptr().cast())) }?;
        // SAFETY: the four exports of the TIP that take no arguments have this signature.
        Some(unsafe {
            core::mem::transmute::<unsafe extern "system" fn() -> isize, NoArguments>(address)
        })
    }

    /// `DllGetClassObject`, or `None` when the DLL does not export it.
    pub fn get_class_object(&self) -> Option<GetClassObject> {
        // SAFETY: a loaded module and a NUL-terminated name.
        let address =
            unsafe { GetProcAddress(self.0, PCSTR(c"DllGetClassObject".as_ptr().cast())) }?;
        // SAFETY: `DllGetClassObject` has this signature.
        Some(unsafe {
            core::mem::transmute::<unsafe extern "system" fn() -> isize, GetClassObject>(address)
        })
    }
}

impl Drop for LoadedDll {
    fn drop(&mut self) {
        // SAFETY: frees the load of `load`, once; no object of the DLL is alive by then.
        let _ = unsafe { FreeLibrary(self.0) };
    }
}

#[test]
fn the_built_dll_exports_its_entry_points_by_name_and_serves_the_text_service() {
    on_sta(|| {
        let dll = LoadedDll::load(&built_dll());
        for name in [
            c"DllCanUnloadNow",
            c"DllRegisterServer",
            c"DllUnregisterServer",
        ] {
            assert!(dll.no_arguments(name).is_some(), "{name:?} is not exported");
        }
        let get_class_object = dll
            .get_class_object()
            .expect("DllGetClassObject is exported");
        let can_unload_now = dll
            .no_arguments(c"DllCanUnloadNow")
            .expect("DllCanUnloadNow is exported");

        let mut object = ptr::null_mut();
        // SAFETY: valid GUID pointers and a writable result pointer.
        let code = unsafe { get_class_object(&TIP, &IClassFactory::IID, &mut object) };
        assert_eq!(code, S_OK);
        // SAFETY: on success the result holds a new IClassFactory reference.
        let factory = unsafe { IClassFactory::from_raw(object) };
        // SAFETY: an in-process class factory; no aggregation.
        let service: ITfTextInputProcessorEx =
            unsafe { factory.CreateInstance(None::<&IUnknown>) }.expect("a text service");
        drop(factory);
        // SAFETY: the export takes no arguments.
        assert_eq!(unsafe { can_unload_now() }, S_FALSE, "a live text service");
        assert!(service.cast::<IUnknown>().is_ok());
        drop(service);
        // SAFETY: as above.
        assert_eq!(unsafe { can_unload_now() }, S_OK);
    });
}
