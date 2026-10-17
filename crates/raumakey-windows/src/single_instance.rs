//! Single-instance guard: a named mutex in the user's session namespace.

use std::os::windows::io::OwnedHandle;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, SetLastError, WIN32_ERROR};
use windows::Win32::System::Threading::CreateMutexW;

use crate::error::{E_INVALIDARG, ResultExt, WinError, WinResult};
use crate::handle;
use crate::names::{is_valid_component, object_name};
use crate::security::UserOnlySecurity;
use crate::sid::current_user_sid_hash;
use crate::strings::WideString;

/// Holds `Local\Raumakey.<sidhash>.<component>` for as long as it is alive.
///
/// The mutex is created with user-only security. Its existence, not its ownership, marks the
/// running instance, so the guard can be dropped on any thread.
#[derive(Debug)]
pub struct SingleInstance {
    _mutex: OwnedHandle,
    name: String,
}

impl SingleInstance {
    /// Creates the mutex for `component` (`[A-Za-z0-9_-]{1,64}`). Returns `Ok(None)` when it
    /// already exists, that is, another instance of this user in this session holds it.
    ///
    /// Fails with `AccessDenied` when an object of that name exists but is not accessible
    /// (for example created by another account); callers must treat that as "not acquired".
    pub fn acquire(component: &str) -> WinResult<Option<Self>> {
        if !is_valid_component(component) {
            return Err(WinError::new(E_INVALIDARG, "single-instance name"));
        }
        let hash = current_user_sid_hash()?;
        let name = object_name(&hash, component)
            .map_err(|_| WinError::new(E_INVALIDARG, "single-instance name"))?;
        let security = UserOnlySecurity::for_current_user()?;
        let wide = WideString::new(&name)?;
        // SAFETY: resets the calling thread's last-error value; no other effect.
        unsafe { SetLastError(WIN32_ERROR(0)) };
        // SAFETY: the security attributes and the name outlive the call.
        let created = unsafe { CreateMutexW(Some(security.as_ptr()), false, wide.as_pcwstr()) };
        // SAFETY: reads the calling thread's last-error value right after the call.
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let mutex = created.context("CreateMutexW")?;
        // SAFETY: CreateMutexW returned a new handle that nothing else owns.
        let mutex = unsafe { handle::take(mutex) };
        if existed {
            return Ok(None);
        }
        Ok(Some(Self {
            _mutex: mutex,
            name,
        }))
    }

    /// The kernel object name.
    pub fn object_name(&self) -> &str {
        &self.name
    }
}
