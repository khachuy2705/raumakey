//! Module (EXE or DLL) handles and module file paths.
//!
//! A [`Module`] is a non-owning module handle: it never changes the module's reference count,
//! so it does not keep the module loaded. The safe constructors only produce handles of modules
//! that stay loaded while code of this crate can run:
//!
//! - [`Module::containing_this_crate`] is the module that statically links this crate: the
//!   controller's `raumakey.exe`, or `raumakey_tip.dll` inside a host process. A DLL finds
//!   itself this way (`GetModuleHandleExW` with `FROM_ADDRESS | UNCHANGED_REFCOUNT` on an
//!   address of this crate's code), for example to pass its own path to TSF registration in
//!   `DllRegisterServer`, or to load icons from its own resources.
//! - [`Module::executable`] is the executable of the process, which is never unloaded.
//!
//! [`Module::containing`] and [`Module::from_raw`] accept other modules; the caller promises
//! that the module stays loaded while the handle is in use.
//!
//! [`Module::file_path`] reads the full path with `GetModuleFileNameW` and a buffer that grows
//! until the path fits ([`read_growing`], tested on every host), so paths longer than
//! `MAX_PATH` are returned complete. A path may name the user's profile folder: never log it.

use core::ffi::c_void;
use core::fmt;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::Foundation::{HINSTANCE, HMODULE};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS, GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    GetModuleFileNameW, GetModuleHandleExW, GetModuleHandleW,
};
use windows::core::PCWSTR;

use crate::error::{E_INVALIDARG, ResultExt, WinError, WinResult};
use crate::strings::read_growing;

/// First buffer size of a module path in UTF-16 units (`MAX_PATH`).
const PATH_UNITS: usize = 260;
/// Largest buffer tried, in UTF-16 units: the longest path Windows supports (32,767 units)
/// plus the NUL.
const MAX_PATH_UNITS: usize = 32_768;

/// A module handle that does not own a reference to the module. It is a plain value: copying
/// it or sending it to another thread does not affect the module.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Module(usize);

impl fmt::Debug for Module {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Module({:#x})", self.0)
    }
}

impl Module {
    /// Wraps a module handle; fails with `E_INVALIDARG` for a null handle.
    ///
    /// # Safety
    ///
    /// `handle` must be the handle of a loaded module (or of a module loaded as a data file with
    /// `LoadLibraryExW`), and the module must stay loaded while the result is in use.
    pub unsafe fn from_raw(handle: HMODULE) -> WinResult<Self> {
        if handle.is_invalid() {
            return Err(WinError::new(E_INVALIDARG, "module handle"));
        }
        Ok(Self(handle.0 as usize))
    }

    /// The module whose image contains `address` (`GetModuleHandleExW` with `FROM_ADDRESS` and
    /// `UNCHANGED_REFCOUNT`). Fails with `NotFound` for an address outside every module.
    ///
    /// # Safety
    ///
    /// The module that contains `address` must stay loaded while the result is in use. The
    /// address itself is only looked up, never dereferenced.
    pub unsafe fn containing(address: *const c_void) -> WinResult<Self> {
        let mut handle = HMODULE::default();
        // SAFETY: with FROM_ADDRESS the name argument is an address that is only looked up;
        // `handle` is a valid out pointer.
        unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                PCWSTR(address.cast()),
                &mut handle,
            )
        }
        .context("GetModuleHandleExW")?;
        // SAFETY: the loader returned the handle of a loaded module; keeping it loaded is the
        // caller's promise.
        unsafe { Self::from_raw(handle) }
    }

    /// The module that contains this crate's code: the EXE or DLL that links it.
    pub fn containing_this_crate() -> WinResult<Self> {
        let function = Self::containing_this_crate as *const c_void;
        // SAFETY: the address is this function's code, so its module stays loaded for as long
        // as code of this crate can run.
        unsafe { Self::containing(function) }
    }

    /// The executable of the current process (`GetModuleHandleW(NULL)`).
    pub fn executable() -> WinResult<Self> {
        // SAFETY: a null name asks for the executable; no reference count is taken.
        let handle = unsafe { GetModuleHandleW(PCWSTR::null()) }.context("GetModuleHandleW")?;
        // SAFETY: the executable is never unloaded.
        unsafe { Self::from_raw(handle) }
    }

    /// The handle as the `windows` crate type.
    pub fn handle(self) -> HMODULE {
        HMODULE(self.0 as *mut c_void)
    }

    /// The handle as an instance handle (the same value), as window classes and resource
    /// functions expect it.
    pub fn instance(self) -> HINSTANCE {
        HINSTANCE(self.0 as *mut c_void)
    }

    /// Full path of the module file (`GetModuleFileNameW`). Fails for modules loaded as data
    /// files, which the loader does not list.
    pub fn file_path(self) -> WinResult<PathBuf> {
        let units = read_growing(PATH_UNITS, MAX_PATH_UNITS, "GetModuleFileNameW", |buffer| {
            // SAFETY: the buffer is writable for its whole length.
            let written = unsafe { GetModuleFileNameW(Some(self.handle()), buffer) };
            if written == 0 {
                Err(WinError::last("GetModuleFileNameW"))
            } else {
                Ok(written as usize)
            }
        })?;
        Ok(PathBuf::from(OsString::from_wide(&units)))
    }
}
