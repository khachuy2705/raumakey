//! Security descriptors of Raumakey's pipes and named kernel objects (design §14, plan D27).
//!
//! Both descriptors set the owner to the user (also when the creator runs elevated, where the
//! default owner would be the Administrators group) and use a protected DACL, so nothing is
//! inherited and no other account (including other users of the same session) is granted
//! access.
//!
//! - **User-only** ([`user_only_sddl`], `UserOnlySecurity`): `O:<user>D:P(A;;GA;;;<user>)`, a
//!   single ACE granting the user full access. Every named kernel object uses it: the
//!   controller's single-instance mutex, which no TIP opens, and the `evkey-compat` legacy
//!   mapping, events, and mutex, which carry per-key text and stay out of reach of sandboxed
//!   processes (see `legacy`).
//! - **AppContainer clients** ([`app_container_client_sddl`], `AppContainerClientSecurity`),
//!   for the control pipe only: `O:<user>D:P(A;;GA;;;<user>)(A;;0x12008b;;;AC)`, with no
//!   mandatory label. A TIP loaded in an AppContainer host of the same user can connect and
//!   receive the same snapshots as any other TIP. `ALL APPLICATION PACKAGES` (`AC`) gets
//!   [`PIPE_CLIENT_ACCESS`] only, so an AppContainer can neither add pipe instances nor change
//!   the pipe's security.
//!
//! Neither descriptor carries a mandatory label, so both objects count as medium with
//! `NO_WRITE_UP`: a low-integrity process of the user that is not an AppContainer gets none of
//! the write-class rights the user's ACE grants (no `FILE_WRITE_DATA`, no
//! `FILE_CREATE_PIPE_INSTANCE`, no `WRITE_DAC`, no `WRITE_OWNER`). It therefore cannot connect
//! to the pipe, add instances, or rewrite its security; such hosts stay on defaults v1 (plan
//! D27). A low label would have let it do all three, so it could have posed as the controller.
//! An AppContainer token is low as well, yet Windows lets it use the rights the `AC` ACE grants
//! on an object without a label (the layer tests check both). Conversely, a pipe that a
//! low-integrity process creates always carries a low label, so clients refuse any pipe labeled
//! below medium ([`is_label_at_least_medium`], applied in `pipe`).
//!
//! Accepted risk (D27): any AppContainer process of the user can connect, read the runtime and
//! macro snapshots (configuration and macros), and send client commands. Restricted tokens and
//! less-privileged AppContainers (`ALL RESTRICTED APPLICATION PACKAGES`) are not granted.

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

/// `ALL APPLICATION PACKAGES`, the group of every AppContainer token (SDDL alias `AC`).
pub const ALL_APPLICATION_PACKAGES_SID: &str = "S-1-15-2-1";

/// The access a control-pipe client requests, and all that the `ALL APPLICATION PACKAGES` ACE
/// of [`app_container_client_sddl`] grants: `FILE_GENERIC_READ | FILE_WRITE_DATA`.
///
/// That is `READ_CONTROL`, `SYNCHRONIZE`, `FILE_READ_DATA`, `FILE_READ_EA`,
/// `FILE_READ_ATTRIBUTES`, and `FILE_WRITE_DATA`: enough to read and write the pipe and to read
/// its owner and label. It leaves out the rest of `GENERIC_WRITE`, above all
/// `FILE_APPEND_DATA`, which on a pipe is `FILE_CREATE_PIPE_INSTANCE` (the right to add server
/// instances; the pipe file system checks it when an instance is created and ignores it on
/// client opens), and `WRITE_DAC`, `WRITE_OWNER`, and `DELETE`. Without
/// `FILE_WRITE_ATTRIBUTES`, a client handle cannot call `SetNamedPipeHandleState`; nothing
/// does (the pipe stays in byte mode).
pub const PIPE_CLIENT_ACCESS: u32 = 0x0012_008B;

/// RID of the medium mandatory level (`S-1-16-8192`).
pub const MEDIUM_MANDATORY_RID: u32 = 0x2000;

/// Whether a pipe whose mandatory label has this RID (`None`: no label) can belong to the
/// controller: no label, which counts as medium, or a label at medium or above.
///
/// An object created by a process below medium always carries that process's level, and no
/// process can label an object above its own level, so a pipe labeled below medium was created
/// by a lower-integrity process (for example one that took the controller's pipe name before
/// the controller started).
pub fn is_label_at_least_medium(label_rid: Option<u32>) -> bool {
    label_rid.is_none_or(|rid| rid >= MEDIUM_MANDATORY_RID)
}

/// `O:<sid>D:P(A;;GA;;;<sid>)`.
pub fn user_only_sddl(sid: &str) -> Result<String, SddlError> {
    if !is_valid_sid_string(sid) {
        return Err(SddlError::InvalidSid);
    }
    Ok(format!("O:{sid}D:P(A;;GA;;;{sid})"))
}

/// `O:<sid>D:P(A;;GA;;;<sid>)(A;;0x12008b;;;AC)` (plan D27).
///
/// The user keeps full access. `ALL APPLICATION PACKAGES` gets [`PIPE_CLIENT_ACCESS`] only:
/// an AppContainer token passes an access check only for rights that both its user and groups
/// and its package (or `ALL APPLICATION PACKAGES`) are granted, so an AppContainer process of
/// the user can open the object as a client but cannot add pipe instances or change the DACL.
///
/// There is deliberately no mandatory label: the object counts as medium with
/// `SYSTEM_MANDATORY_LABEL_NO_WRITE_UP` (a principal below medium gets none of the write rights
/// of the object's generic mapping), which keeps low-integrity processes that are not
/// AppContainers from writing to the pipe, adding instances, or changing its DACL or owner.
pub fn app_container_client_sddl(sid: &str) -> Result<String, SddlError> {
    let user_only = user_only_sddl(sid)?;
    Ok(format!("{user_only}(A;;{PIPE_CLIENT_ACCESS:#x};;;AC)"))
}

#[cfg(windows)]
pub use win::*;

#[cfg(windows)]
mod win {
    use core::mem::offset_of;

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::Authorization::{
        ConvertSecurityDescriptorToStringSecurityDescriptorW,
        ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
        SE_FILE_OBJECT, SE_KERNEL_OBJECT, SE_OBJECT_TYPE,
    };
    use windows::Win32::Security::{
        ACE_HEADER, ACL, DACL_SECURITY_INFORMATION, GetAce, GetSidSubAuthority, IsValidSid,
        LABEL_SECURITY_INFORMATION, OBJECT_SECURITY_INFORMATION, OWNER_SECURITY_INFORMATION,
        PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, SYSTEM_MANDATORY_LABEL_ACE,
    };
    use windows::Win32::System::SystemServices::SYSTEM_MANDATORY_LABEL_ACE_TYPE;
    use windows::core::{PCWSTR, PWSTR};

    use super::{SddlError, app_container_client_sddl, user_only_sddl};
    use crate::error::{E_FAIL, E_INVALIDARG, ResultExt, WinError, WinResult, check_win32};
    use crate::handle::LocalMemory;
    use crate::sid::{current_user_sid_string, sid_to_string};
    use crate::strings::{WideString, read_pwstr};

    /// A self-relative security descriptor built from SDDL, plus the non-inheritable
    /// `SECURITY_ATTRIBUTES` that point to it.
    struct Descriptor {
        memory: LocalMemory,
        attributes: SECURITY_ATTRIBUTES,
        sid: String,
    }

    // SAFETY: the descriptor is an immutable heap allocation owned by this value and freed only
    // on drop; the attributes only point to it. Sharing or sending it across threads is sound.
    unsafe impl Send for Descriptor {}
    // SAFETY: see `Send`; no method mutates the descriptor.
    unsafe impl Sync for Descriptor {}

    impl Descriptor {
        /// Builds the descriptor `sddl(sid)` describes; `context` names it in errors.
        fn build(
            sid: &str,
            sddl: fn(&str) -> Result<String, SddlError>,
            context: &'static str,
        ) -> WinResult<Self> {
            let sddl = sddl(sid).map_err(|_| WinError::new(E_INVALIDARG, context))?;
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
            let memory = LocalMemory(descriptor.0);
            let attributes = SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: memory.0,
                bInheritHandle: false.into(),
            };
            Ok(Self {
                memory,
                attributes,
                sid: sid.to_owned(),
            })
        }

        fn descriptor(&self) -> PSECURITY_DESCRIPTOR {
            PSECURITY_DESCRIPTOR(self.memory.0)
        }
    }

    /// A self-relative security descriptor granting only one user ([`user_only_sddl`]), plus
    /// the `SECURITY_ATTRIBUTES` (non-inheritable) that point to it.
    ///
    /// Pass [`UserOnlySecurity::as_ptr`] to `CreateMutexW`, `CreateEventW`, or
    /// `CreateFileMappingW`; the pointer is valid while `self` is alive and not moved. The
    /// control pipe uses [`AppContainerClientSecurity`] instead.
    pub struct UserOnlySecurity {
        inner: Descriptor,
    }

    impl UserOnlySecurity {
        /// A descriptor for the user running this process.
        pub fn for_current_user() -> WinResult<Self> {
            Self::for_sid(&current_user_sid_string()?)
        }

        /// A descriptor for `sid` (a canonical SID string).
        pub fn for_sid(sid: &str) -> WinResult<Self> {
            Descriptor::build(sid, user_only_sddl, "user-only security descriptor")
                .map(|inner| Self { inner })
        }

        /// The security attributes (non-inheritable, pointing to the user-only descriptor).
        pub fn as_attributes(&self) -> &SECURITY_ATTRIBUTES {
            &self.inner.attributes
        }

        /// Pointer to the security attributes for Win32 calls.
        pub fn as_ptr(&self) -> *const SECURITY_ATTRIBUTES {
            &raw const self.inner.attributes
        }

        /// The security descriptor.
        pub fn descriptor(&self) -> PSECURITY_DESCRIPTOR {
            self.inner.descriptor()
        }

        /// The SID the descriptor grants.
        pub fn sid(&self) -> &str {
            &self.inner.sid
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
                .field("sid", &self.inner.sid)
                .finish_non_exhaustive()
        }
    }

    /// The control pipe's descriptor ([`app_container_client_sddl`], plan D27): full access for
    /// one user and [`super::PIPE_CLIENT_ACCESS`] for `ALL APPLICATION PACKAGES`, with no
    /// mandatory label, so that AppContainer processes of that user can connect as clients while
    /// low-integrity processes that are not AppContainers cannot. Plus the
    /// `SECURITY_ATTRIBUTES` (non-inheritable) that point to it.
    ///
    /// Use it only for objects that AppContainer clients must open; every other object keeps
    /// [`UserOnlySecurity`]. Pass [`AppContainerClientSecurity::as_ptr`] to `CreateNamedPipeW`;
    /// the pointer is valid while `self` is alive and not moved.
    pub struct AppContainerClientSecurity {
        inner: Descriptor,
    }

    impl AppContainerClientSecurity {
        /// A descriptor for the user running this process.
        pub fn for_current_user() -> WinResult<Self> {
            Self::for_sid(&current_user_sid_string()?)
        }

        /// A descriptor for `sid` (a canonical SID string).
        pub fn for_sid(sid: &str) -> WinResult<Self> {
            Descriptor::build(
                sid,
                app_container_client_sddl,
                "AppContainer-client security descriptor",
            )
            .map(|inner| Self { inner })
        }

        /// The security attributes (non-inheritable, pointing to the descriptor).
        pub fn as_attributes(&self) -> &SECURITY_ATTRIBUTES {
            &self.inner.attributes
        }

        /// Pointer to the security attributes for Win32 calls.
        pub fn as_ptr(&self) -> *const SECURITY_ATTRIBUTES {
            &raw const self.inner.attributes
        }

        /// The security descriptor.
        pub fn descriptor(&self) -> PSECURITY_DESCRIPTOR {
            self.inner.descriptor()
        }

        /// The SID that gets full access.
        pub fn sid(&self) -> &str {
            &self.inner.sid
        }

        /// The owner, DACL, and mandatory label (there is none) of the descriptor in SDDL form.
        pub fn to_sddl(&self) -> WinResult<String> {
            descriptor_to_sddl(
                self.descriptor(),
                OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION,
            )
        }
    }

    impl core::fmt::Debug for AppContainerClientSecurity {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("AppContainerClientSecurity")
                .field("sid", &self.inner.sid)
                .finish_non_exhaustive()
        }
    }

    /// Owner and DACL of a kernel object (event, mutex, mapping) in SDDL form.
    pub fn kernel_object_sddl(handle: HANDLE) -> WinResult<String> {
        handle_sddl(
            handle,
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
        )
    }

    /// Owner, DACL, and mandatory label of a file-system object such as a pipe, in SDDL form.
    pub(crate) fn file_object_sddl(handle: HANDLE) -> WinResult<String> {
        handle_sddl(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | LABEL_SECURITY_INFORMATION,
        )
    }

    /// RID of the mandatory label of a file-system object such as a pipe (`None`: no label).
    pub(crate) fn file_object_label_rid(handle: HANDLE) -> WinResult<Option<u32>> {
        let mut sacl: *mut ACL = core::ptr::null_mut();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `handle` is a valid handle with READ_CONTROL (all a label query needs); both
        // out pointers are valid. `sacl` points into the descriptor, which `memory` frees after
        // the label was read.
        let status = unsafe {
            GetSecurityInfo(
                handle,
                SE_FILE_OBJECT,
                LABEL_SECURITY_INFORMATION,
                None,
                None,
                None,
                Some(&mut sacl),
                Some(&mut descriptor),
            )
        };
        check_win32(status, "GetSecurityInfo(label)")?;
        let memory = LocalMemory(descriptor.0);
        let rid = label_rid(sacl);
        drop(memory);
        rid
    }

    /// RID of the first mandatory-label ACE of `sacl` (`None`: no SACL or no label ACE).
    fn label_rid(sacl: *const ACL) -> WinResult<Option<u32>> {
        let malformed = || WinError::new(E_FAIL, "mandatory label");
        if sacl.is_null() {
            return Ok(None);
        }
        // SAFETY: `sacl` points to a valid ACL inside a live descriptor.
        let count = unsafe { (*sacl).AceCount };
        for index in 0..u32::from(count) {
            let mut ace = core::ptr::null_mut();
            // SAFETY: `index` is below the ACE count; `ace` is a valid out pointer.
            unsafe { GetAce(sacl, index, &mut ace) }.context("GetAce")?;
            // SAFETY: every ACE starts with an ACE_HEADER; the read makes no alignment
            // assumption.
            let header = unsafe { ace.cast::<ACE_HEADER>().read_unaligned() };
            if u32::from(header.AceType) != SYSTEM_MANDATORY_LABEL_ACE_TYPE {
                continue;
            }
            // A SID is 8 bytes (revision, subauthority count, 6-byte authority) plus 4 bytes per
            // subauthority; all of it must lie within the ACE's `AceSize` bytes.
            let start = offset_of!(SYSTEM_MANDATORY_LABEL_ACE, SidStart);
            let size = usize::from(header.AceSize);
            if size < start + 8 {
                return Err(malformed());
            }
            let ace = ace.cast::<u8>();
            // SAFETY: the subauthority count is byte 1 of the SID, inside the ACE (checked
            // above); a byte read needs no alignment. The pointer derives from the ACE pointer.
            let subauthorities = unsafe { ace.add(start + 1).read() };
            if start + 8 + 4 * usize::from(subauthorities) > size {
                return Err(malformed());
            }
            // SAFETY: the SID starts at `start`, inside the ACE.
            let sid = PSID(unsafe { ace.add(start) }.cast());
            // SAFETY: every byte of the SID, as its own count describes it, lies within the ACE
            // (checked above), so IsValidSid reads only inside it.
            if !unsafe { IsValidSid(sid) }.as_bool() {
                return Err(malformed());
            }
            let Some(last) = u32::from(subauthorities).checked_sub(1) else {
                return Err(malformed());
            };
            // SAFETY: `sid` is a valid SID that lies within the ACE, and `last` indexes one of
            // its subauthorities.
            return Ok(Some(unsafe { *GetSidSubAuthority(sid, last) }));
        }
        Ok(None)
    }

    fn handle_sddl(
        handle: HANDLE,
        kind: SE_OBJECT_TYPE,
        information: OBJECT_SECURITY_INFORMATION,
    ) -> WinResult<String> {
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `handle` is a valid handle with READ_CONTROL (all that owner, DACL, and label
        // need); the out pointer is valid and the returned descriptor is freed by `memory`.
        let status = unsafe {
            GetSecurityInfo(
                handle,
                kind,
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
