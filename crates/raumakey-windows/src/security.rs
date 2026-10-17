//! Security descriptors that grant access only to the current user (design §14).
//!
//! Raumakey's pipes, mutexes, events, and file mappings are created with
//! `O:<user>D:P(A;;GA;;;<user>)`: the owner is the user (also when the creator runs elevated,
//! where the default owner would be the Administrators group) and the protected DACL has a single
//! ACE granting the user full access, so nothing is inherited and no other account (including
//! other users of the same session) is granted access.

use core::fmt;

/// Why an SDDL string could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SddlError {
    /// The SID is not in canonical `S-1-<authority>-<subauthority>…` form.
    InvalidSid,
}

impl fmt::Display for SddlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SID is not in canonical S-1-… form")
    }
}

impl std::error::Error for SddlError {}

/// True for a canonical SID string with a decimal authority and 1..=15 decimal subauthorities
/// (`S-1-5-18`, `S-1-5-21-…-1001`, `S-1-12-1-…`). Anything else, including SDDL syntax, is
/// rejected, so the result can be embedded in SDDL safely.
pub fn is_valid_sid_string(sid: &str) -> bool {
    let Some(rest) = sid.strip_prefix("S-1-") else {
        return false;
    };
    let parts: Vec<&str> = rest.split('-').collect();
    let Some((authority, subauthorities)) = parts.split_first() else {
        return false;
    };
    let decimal = |part: &str, max: u64| {
        !part.is_empty()
            && part.len() <= 15
            && part.bytes().all(|byte| byte.is_ascii_digit())
            && part.parse::<u64>().is_ok_and(|value| value <= max)
    };
    (1..=15).contains(&subauthorities.len())
        && decimal(authority, (1_u64 << 48) - 1)
        && subauthorities
            .iter()
            .all(|part| decimal(part, u64::from(u32::MAX)))
}

/// `O:<sid>D:P(A;;GA;;;<sid>)`.
pub fn user_only_sddl(sid: &str) -> Result<String, SddlError> {
    if !is_valid_sid_string(sid) {
        return Err(SddlError::InvalidSid);
    }
    Ok(format!("O:{sid}D:P(A;;GA;;;{sid})"))
}

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_FILE_OBJECT, SE_KERNEL_OBJECT, SE_OBJECT_TYPE,
    };
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES,
    };
    use windows::core::{PCWSTR, PWSTR};

    use super::user_only_sddl;
    use crate::error::{E_FAIL, E_INVALIDARG, ResultExt, WinError, WinResult, check_win32};
    use crate::handle::LocalMemory;
    use crate::sid::{current_user_sid_string, sid_to_string};
    use crate::strings::{WideString, read_pwstr};

    /// A self-relative security descriptor granting only one user, plus the
    /// `SECURITY_ATTRIBUTES` (non-inheritable) that point to it.
    ///
    /// Pass [`UserOnlySecurity::as_ptr`] to `CreateNamedPipeW`, `CreateMutexW`, `CreateEventW`,
    /// or `CreateFileMappingW`; the pointer is valid while `self` is alive and not moved.
    pub struct UserOnlySecurity {
        descriptor: LocalMemory,
        attributes: SECURITY_ATTRIBUTES,
        sid: String,
    }

    // SAFETY: the descriptor is an immutable heap allocation owned by this value and freed only
    // on drop; the attributes only point to it. Sharing or sending it across threads is sound.
    unsafe impl Send for UserOnlySecurity {}
    // SAFETY: see `Send`; no method mutates the descriptor.
    unsafe impl Sync for UserOnlySecurity {}

    impl UserOnlySecurity {
        /// A descriptor for the user running this process.
        pub fn for_current_user() -> WinResult<Self> {
            Self::for_sid(&current_user_sid_string()?)
        }

        /// A descriptor for `sid` (a canonical SID string).
        pub fn for_sid(sid: &str) -> WinResult<Self> {
            let sddl = user_only_sddl(sid)
                .map_err(|_| WinError::new(E_INVALIDARG, "user-only security descriptor"))?;
            let text = WideString::new(&sddl)?;
            let mut descriptor = PSECURITY_DESCRIPTOR::default();
            // SAFETY: `text` is NUL-terminated and outlives the call; `descriptor` is a valid
            // out pointer. On success the descriptor is LocalAlloc'd and owned below.
            unsafe {
                ConvertStringSecurityDescriptorToSecurityDescriptorW(
                    text.as_pcwstr(),
                    SDDL_REVISION_1,
                    &mut descriptor,
                    None,
                )
            }
            .context("ConvertStringSecurityDescriptorToSecurityDescriptorW")?;
            let descriptor = LocalMemory(descriptor.0);
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            };
            Ok(Self {
                descriptor,
                attributes,
                sid: sid.to_owned(),
            })
        }

        /// The security attributes (non-inheritable, pointing to the user-only descriptor).
        pub fn as_attributes(&self) -> &SECURITY_ATTRIBUTES {
            &self.attributes
        }

        /// Pointer to the security attributes for Win32 calls.
        pub fn as_ptr(&self) -> *const SECURITY_ATTRIBUTES {
            &raw const self.attributes
        }

        /// The security descriptor.
        pub fn descriptor(&self) -> PSECURITY_DESCRIPTOR {
            PSECURITY_DESCRIPTOR(self.descriptor.0)
        }

        /// The SID the descriptor grants.
        pub fn sid(&self) -> &str {
            &self.sid
        }

        /// The owner and DACL of the descriptor in SDDL form.
        pub fn to_sddl(&self) -> WinResult<String> {
            descriptor_to_sddl(
                self.descriptor(),
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            )
        }
    }

    impl core::fmt::Debug for UserOnlySecurity {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("UserOnlySecurity")
                .field("sid", &self.sid)
                .finish_non_exhaustive()
        }
    }

    /// Owner and DACL of a kernel object (event, mutex, mapping) in SDDL form.
    pub fn kernel_object_sddl(handle: HANDLE) -> WinResult<String> {
        let information = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `handle` is a valid handle with READ_CONTROL; the out pointer is valid and the
        // returned descriptor is freed by `memory`.
        let status = unsafe {
            GetSecurityInfo(
                handle,
                SE_KERNEL_OBJECT,
                information,
                None,
                None,
                None,
                None,
                Some(&mut descriptor),
            )
        };
        check_win32(status, "GetSecurityInfo")?;
        let memory = LocalMemory(descriptor.0);
        let sddl = descriptor_to_sddl(PSECURITY_DESCRIPTOR(memory.0), information);
        drop(memory);
        sddl
    }

    /// Owner SID of a kernel object.
    pub fn kernel_object_owner_sid(handle: HANDLE) -> WinResult<String> {
        handle_owner_sid(handle, SE_KERNEL_OBJECT)
    }

    /// Owner SID of a file-system object such as a pipe instance.
    pub(crate) fn file_object_owner_sid(handle: HANDLE) -> WinResult<String> {
        handle_owner_sid(handle, SE_FILE_OBJECT)
    }

    fn handle_owner_sid(handle: HANDLE, kind: SE_OBJECT_TYPE) -> WinResult<String> {
        let mut owner = PSID::default();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `handle` is valid with READ_CONTROL; both out pointers are valid. `owner`
        // points into the descriptor, which `memory` frees after the SID was converted.
        let status = unsafe {
            GetSecurityInfo(
                handle,
                kind,
                OWNER_SECURITY_INFORMATION,
                Some(&mut owner),
                None,
                None,
                None,
                Some(&mut descriptor),
            )
        };
        check_win32(status, "GetSecurityInfo(owner)")?;
        let memory = LocalMemory(descriptor.0);
        if owner.is_invalid() {
            return Err(WinError::new(E_FAIL, "GetSecurityInfo(owner)"));
        }
        let sid = sid_to_string(owner);
        drop(memory);
        sid
    }

    fn descriptor_to_sddl(
        descriptor: PSECURITY_DESCRIPTOR,
        information: OBJECT_SECURITY_INFORMATION,
    ) -> WinResult<String> {
        let mut text = PWSTR::null();
        // SAFETY: `descriptor` is a valid descriptor; `text` is a valid out pointer and the
        // LocalAlloc'd result is freed by `memory`.
        unsafe {
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                descriptor,
                SDDL_REVISION_1,
                information,
                &mut text,
                None,
            )
        }
        .context("ConvertSecurityDescriptorToStringSecurityDescriptorW")?;
        let memory = LocalMemory(text.0.cast());
        // SAFETY: on success `text` is a NUL-terminated string that `memory` still owns.
        let units = unsafe { read_pwstr(PCWSTR(text.0)) };
        drop(memory);
        String::from_utf16(&units).map_err(|_| WinError::new(E_FAIL, "security descriptor text"))
    }
}
