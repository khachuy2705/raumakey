//! `module`: module handles and file paths, checked against the test executable itself (which
//! statically links this crate, as the controller and the TIP do), against `kernel32.dll`, and
//! against a uniquely named copy of a data-only system DLL.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::known_folder::system_dir;
use raumakey_windows::module::Module;
use raumakey_windows::strings::WideString;
use windows::Win32::Foundation::{FreeLibrary, HMODULE};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, LoadLibraryW};
use windows::core::w;

use crate::support::unique_suffix;

fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|error| panic!("canonicalize {}: {error}", path.display()))
}

#[test]
fn the_module_containing_a_function_of_this_crate_is_the_test_executable() {
    let function = raumakey_windows::strings::to_wide as *const c_void;
    // SAFETY: the function is part of this test executable, which stays loaded.
    let module = unsafe { Module::containing(function) }.unwrap();
    let path = module.file_path().unwrap();
    assert!(path.is_absolute(), "{}", path.display());
    assert_eq!(
        canonical(&path),
        canonical(&std::env::current_exe().unwrap())
    );
}

#[test]
fn this_crate_and_the_executable_are_one_module_in_a_test_binary() {
    let own = Module::containing_this_crate().unwrap();
    let exe = Module::executable().unwrap();
    assert_eq!(own, exe);
    let function = raumakey_windows::strings::to_wide as *const c_void;
    // SAFETY: as above.
    assert_eq!(unsafe { Module::containing(function) }.unwrap(), own);
    assert_eq!(own.instance().0, own.handle().0);
    assert_eq!(
        canonical(&exe.file_path().unwrap()),
        canonical(&std::env::current_exe().unwrap())
    );
}

#[test]
fn the_path_of_a_loaded_system_dll_comes_from_the_loader() {
    // SAFETY: kernel32 is loaded in every process and is never unloaded.
    let handle = unsafe { GetModuleHandleW(w!("kernel32.dll")) }.unwrap();
    // SAFETY: a module handle returned by the loader for a module that stays loaded.
    let module = unsafe { Module::from_raw(handle) }.unwrap();
    let path = module.file_path().unwrap();
    assert!(path.is_absolute(), "{}", path.display());
    assert!(
        path.file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("kernel32.dll")),
        "{}",
        path.display()
    );
    assert!(path.is_file(), "{}", path.display());
}

/// The module of an address is looked up without a reference (`UNCHANGED_REFCOUNT`), so a DLL
/// that finds itself this way can still be unloaded.
///
/// The DLL is a copy of `kbdus.dll` under a name unique to this test, so no other code of the
/// process loads it meanwhile. Keyboard layout DLLs hold only data (no entry point, no imports),
/// so loading the copy runs no code.
#[test]
fn finding_the_module_of_an_address_does_not_keep_it_loaded() {
    let dir = tempfile::tempdir().unwrap();
    let name = format!("raumakey-module-{}.dll", unique_suffix());
    let path = dir.path().join(&name);
    std::fs::copy(system_dir().unwrap().join("kbdus.dll"), &path).unwrap();
    let base_name = WideString::new(&name).unwrap();
    // SAFETY: a NUL-terminated name; the lookup takes no reference.
    let loaded = || unsafe { GetModuleHandleW(base_name.as_pcwstr()) }.ok();
    assert_eq!(loaded(), None);

    let full_path = WideString::from_os(path.as_os_str()).unwrap();
    // SAFETY: a NUL-terminated path of a data-only DLL; loading it runs no code.
    let handle = unsafe { LoadLibraryW(full_path.as_pcwstr()) }.unwrap();
    assert_eq!(loaded(), Some(handle));
    // The image base is an address inside the module.
    let address = handle.0.cast_const();
    for _ in 0..3 {
        // SAFETY: the module stays loaded until the FreeLibrary below.
        let module = unsafe { Module::containing(address) }.unwrap();
        assert_eq!(module.handle(), handle);
    }
    // SAFETY: undoes the LoadLibraryW above, once.
    unsafe { FreeLibrary(handle) }.unwrap();
    let still_loaded = loaded().is_some();
    // Drop any reference a lookup took, so that the temporary directory can be deleted.
    for _ in 0..8 {
        let Some(extra) = loaded() else { break };
        // SAFETY: releases a reference the lookups above added to this module.
        let _ = unsafe { FreeLibrary(extra) };
    }
    assert!(!still_loaded, "a lookup kept the module loaded");
}

#[test]
fn addresses_outside_every_module_are_not_found() {
    let heap = Box::new(0_u64);
    let address = (&*heap as *const u64).cast::<c_void>();
    // SAFETY: the lookup fails; no module handle is used.
    let error = unsafe { Module::containing(address) }.unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::NotFound, "{error}");
}

#[test]
fn a_null_module_handle_is_rejected() {
    // SAFETY: a null handle is rejected before any use.
    let error = unsafe { Module::from_raw(HMODULE::default()) }.unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::InvalidInput);
}

#[test]
fn module_handles_are_plain_values_shared_between_threads() {
    fn assert_send_sync<T: Send + Sync + Copy>() {}
    assert_send_sync::<Module>();
    let own = Module::containing_this_crate().unwrap();
    let other = std::thread::spawn(move || own.file_path().unwrap())
        .join()
        .unwrap();
    assert_eq!(other, own.file_path().unwrap());
}
