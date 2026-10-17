//! RAII ownership of kernel handles and `LocalAlloc` memory.

use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};

/// Takes ownership of `handle`; it is closed with `CloseHandle` when the result is dropped.
///
/// # Safety
///
/// `handle` must be an open kernel handle returned by a successful call, owned by nobody else,
/// and needing no cleanup other than `CloseHandle` (not a registry key, not a pseudo handle).
pub(crate) unsafe fn take(handle: HANDLE) -> OwnedHandle {
    // SAFETY: forwarded from the caller's contract.
    unsafe { OwnedHandle::from_raw_handle(handle.0) }
}

/// Borrows an owned handle as the `windows` crate type.
pub(crate) fn raw(owned: &OwnedHandle) -> HANDLE {
    HANDLE(owned.as_raw_handle())
}

/// Memory returned by an API that must be released with `LocalFree`.
pub(crate) struct LocalMemory(pub(crate) *mut core::ffi::c_void);

impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: the pointer came from an API documented to allocate with LocalAlloc and
            // is freed exactly once here.
            unsafe {
                let _ = LocalFree(Some(HLOCAL(self.0)));
            }
        }
    }
}
