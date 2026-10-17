//! `DllRegisterServer` and `DllUnregisterServer` of the built DLL.
//!
//! Both write `HKEY_LOCAL_MACHINE` (the COM class, the TSF language profile, and its
//! categories), so the test is `#[ignore]`d. Run it by hand from an elevated prompt, with a
//! test binary of the bitness to check (x86_64, or i686 under WOW64), for example
//! `cargo test --locked -p raumakey-tsf --test com registration -- --ignored`, and compare
//! with the check list in the documentation of `raumakey_tip::server`. Should it fail after
//! registering, remove the registration with `regsvr32 /u` of the same bitness on the DLL
//! that `built_dll` names.

use raumakey_windows::tsf_registration::{TipArch, TipBinary, verify_installation, verify_removal};
use windows::Win32::Foundation::S_OK;

use crate::artifact::LoadedDll;
use crate::support::{built_dll, on_sta};

#[test]
#[ignore = "manual: registers raumakey_tip.dll machine-wide (HKLM, TSF profile); run elevated"]
fn the_built_dll_registers_and_unregisters_itself() {
    on_sta(|| {
        let path = built_dll();
        let dll = LoadedDll::load(&path);
        let register = dll
            .no_arguments(c"DllRegisterServer")
            .expect("DllRegisterServer is exported");
        let unregister = dll
            .no_arguments(c"DllUnregisterServer")
            .expect("DllUnregisterServer is exported");
        let arch = TipArch::current();

        // SAFETY: the export takes no arguments.
        assert_eq!(unsafe { register() }, S_OK, "DllRegisterServer");
        let report = verify_installation(&[TipBinary {
            path: path.clone(),
            arch,
        }]);
        assert!(report.all_passed(), "{:#?}", report.describe());

        // SAFETY: as above.
        assert_eq!(unsafe { unregister() }, S_OK, "DllUnregisterServer");
        let report = verify_removal(&[arch]);
        assert!(report.all_passed(), "{:#?}", report.describe());
    });
}
