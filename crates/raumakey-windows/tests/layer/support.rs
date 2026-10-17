//! Helpers shared by the integration tests.

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// `<pid>-<n>`, unique within this test process and across concurrently running test processes.
pub fn unique_suffix() -> String {
    let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{n}", std::process::id())
}

/// Runs `body` on a new thread and returns its result; a panic (a failed assertion) fails the
/// calling test. Windows, menus, message queues, and per-thread message state created by `body`
/// stay on that thread, so the harness threads stay clean.
pub fn on_fresh_thread<T: Send + 'static>(body: impl FnOnce() -> T + Send + 'static) -> T {
    match std::thread::spawn(body).join() {
        Ok(value) => value,
        Err(payload) => std::panic::resume_unwind(payload),
    }
}

#[cfg(windows)]
pub use win::{DescriptorParts, ResourceFile, group_icon_ids};

#[cfg(windows)]
mod win {
    use std::mem::offset_of;

    use raumakey_windows::known_folder::system_dir;
    use raumakey_windows::module::Module;
    use raumakey_windows::panic::guard_bool;
    use raumakey_windows::strings::WideString;
    use windows::Win32::Foundation::{FreeLibrary, HLOCAL, HMODULE, LocalFree};
    use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
    use windows::Win32::Security::{
        ACCESS_ALLOWED_ACE, ACE_HEADER, ACL, GetAce, GetSecurityDescriptorControl,
        GetSecurityDescriptorDacl, GetSecurityDescriptorOwner, GetSecurityDescriptorSacl,
        PSECURITY_DESCRIPTOR, PSID, SE_DACL_PROTECTED,
    };
    use windows::Win32::System::LibraryLoader::{
        EnumResourceNamesW, LOAD_LIBRARY_AS_DATAFILE, LOAD_LIBRARY_AS_IMAGE_RESOURCE,
        LoadLibraryExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::RT_GROUP_ICON;
    use windows::core::{BOOL, PCWSTR, PWSTR};

    /// The owner, DACL, and SACL of a security descriptor, read with `GetAce`.
    #[derive(Debug)]
    pub struct DescriptorParts {
        /// Owner SID.
        pub owner: String,
        /// `SE_DACL_PROTECTED` is set.
        pub dacl_protected: bool,
        /// `(ACE type, access mask, SID)` of each DACL ACE, in order.
        pub dacl: Vec<(u8, u32, String)>,
        /// `(ACE type, mask or label policy, SID)` of each SACL ACE, in order.
        pub sacl: Vec<(u8, u32, String)>,
    }

    impl DescriptorParts {
        /// Reads a valid security descriptor.
        pub fn read(descriptor: PSECURITY_DESCRIPTOR) -> Self {
            let mut owner = PSID::default();
            let mut defaulted = BOOL::default();
            // SAFETY: a valid descriptor and valid out pointers; `owner` points into it.
            unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted) }.unwrap();
            let mut control = 0_u16;
            let mut revision = 0_u32;
            // SAFETY: a valid descriptor and valid out pointers.
            unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) }
                .unwrap();
            let mut present = BOOL::default();
            let mut dacl: *mut ACL = std::ptr::null_mut();
            // SAFETY: a valid descriptor and valid out pointers; `dacl` points into it.
            unsafe {
                GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted)
            }
            .unwrap();
            let dacl = if present.as_bool() {
                aces(dacl)
            } else {
                Vec::new()
            };
            let mut sacl: *mut ACL = std::ptr::null_mut();
            // SAFETY: as above; a descriptor without a SACL reports `present` false.
            unsafe {
                GetSecurityDescriptorSacl(descriptor, &mut present, &mut sacl, &mut defaulted)
            }
            .unwrap();
            let sacl = if present.as_bool() {
                aces(sacl)
            } else {
                Vec::new()
            };
            Self {
                owner: sid_string(owner),
                dacl_protected: control & SE_DACL_PROTECTED.0 != 0,
                dacl,
                sacl,
            }
        }
    }

    /// `(type, mask, SID)` of every ACE of `acl` (null: none).
    fn aces(acl: *const ACL) -> Vec<(u8, u32, String)> {
        if acl.is_null() {
            return Vec::new();
        }
        // SAFETY: `acl` points to a valid ACL inside a live descriptor.
        let count = unsafe { (*acl).AceCount };
        (0..u32::from(count))
            .map(|index| {
                let mut ace = std::ptr::null_mut();
                // SAFETY: `index` is below the ACE count; `ace` is a valid out pointer.
                unsafe { GetAce(acl, index, &mut ace) }.unwrap();
                let ace = ace.cast::<u8>();
                // SAFETY: every ACE starts with an ACE_HEADER; the read makes no alignment
                // assumption.
                let header = unsafe { ace.cast::<ACE_HEADER>().read_unaligned() };
                // Access-allowed and mandatory-label ACEs share this layout (header, mask, SID);
                // the tests only meet those two types.
                let mask_at = offset_of!(ACCESS_ALLOWED_ACE, Mask);
                let sid_at = offset_of!(ACCESS_ALLOWED_ACE, SidStart);
                assert!(usize::from(header.AceSize) >= sid_at + 8, "ACE too short");
                // SAFETY: the mask lies inside the ACE (checked above); unaligned read.
                let mask = unsafe { ace.add(mask_at).cast::<u32>().read_unaligned() };
                // The SID pointer derives from the ACE pointer, which covers the whole ACE
                // (`AceSize` bytes, SID included).
                // SAFETY: `sid_at` lies inside the ACE (checked above).
                let sid = PSID(unsafe { ace.add(sid_at) }.cast());
                (header.AceType, mask, sid_string(sid))
            })
            .collect()
    }

    fn sid_string(sid: PSID) -> String {
        let mut text = PWSTR::null();
        // SAFETY: `sid` is a valid SID; `text` is a valid out pointer, freed below.
        unsafe { ConvertSidToStringSidW(sid, &mut text) }.unwrap();
        // SAFETY: on success `text` is a NUL-terminated string.
        let string = unsafe { text.to_string() }.unwrap();
        // SAFETY: frees the LocalAlloc'd string once.
        unsafe { LocalFree(Some(HLOCAL(text.0.cast()))) };
        string
    }

    /// A system DLL loaded as an image resource only (no code runs); freed on drop.
    pub struct ResourceFile(HMODULE);

    impl ResourceFile {
        /// Loads `name` from the system directory.
        pub fn system(name: &str) -> Self {
            let path = system_dir().unwrap().join(name);
            let path = WideString::from_os(path.as_os_str()).unwrap();
            // SAFETY: NUL-terminated path; the flags map the file without running any code.
            let module = unsafe {
                LoadLibraryExW(
                    path.as_pcwstr(),
                    None,
                    LOAD_LIBRARY_AS_IMAGE_RESOURCE | LOAD_LIBRARY_AS_DATAFILE,
                )
            }
            .unwrap();
            Self(module)
        }

        /// The module, valid while `self` lives.
        pub fn module(&self) -> Module {
            // SAFETY: the file stays loaded while `self` lives, and the tests use the result
            // only within that time.
            unsafe { Module::from_raw(self.0) }.unwrap()
        }
    }

    impl Drop for ResourceFile {
        fn drop(&mut self) {
            // SAFETY: frees the load done in `system`, once.
            let _ = unsafe { FreeLibrary(self.0) };
        }
    }

    unsafe extern "system" fn collect_integer_ids(
        _module: HMODULE,
        _kind: PCWSTR,
        name: PCWSTR,
        ids: isize,
    ) -> BOOL {
        guard_bool(|| {
            // Integer resource names are values below 0x10000 in place of a pointer.
            let value = name.0 as usize;
            if let Ok(id) = u16::try_from(value) {
                // SAFETY: `ids` is the `&mut Vec<u16>` passed to EnumResourceNamesW below,
                // valid for the whole enumeration.
                unsafe { &mut *(ids as *mut Vec<u16>) }.push(id);
            }
            true
        })
    }

    /// The integer ids of the `RT_GROUP_ICON` resources of `file`, in enumeration order.
    pub fn group_icon_ids(file: &ResourceFile) -> Vec<u16> {
        let mut ids: Vec<u16> = Vec::new();
        // SAFETY: a loaded resource module; the callback only appends to `ids`, which outlives
        // the call.
        let _ = unsafe {
            EnumResourceNamesW(
                Some(file.0),
                RT_GROUP_ICON,
                Some(collect_integer_ids),
                &mut ids as *mut Vec<u16> as isize,
            )
        };
        ids
    }
}

#[cfg(windows)]
pub use sandbox::{TestToken, as_anonymous};

/// In-process stand-ins for sandboxed clients: a thread of this process impersonates a token
/// derived from the process token, so every access check it triggers (opening a pipe, adding a
/// pipe instance, opening a process or token) runs against that token.
///
/// Nothing outside the test process changes: the AppContainer token is a "lowbox" token made by
/// `NtCreateLowBoxToken` for a made-up package SID. It needs no AppContainer profile (none is
/// created), no capabilities, and no named-object directories, and it disappears with its
/// handles. The process itself stays a normal process, so a peer that opens *this process* (for
/// example a pipe server reading its client's process token) still sees a normal process.
#[cfg(windows)]
mod sandbox {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::ConvertStringSidToSidW;
    use windows::Win32::Security::{
        DuplicateTokenEx, GetLengthSid, GetTokenInformation, ImpersonateAnonymousToken, PSID,
        RevertToSelf, SECURITY_IMPERSONATION_LEVEL, SID_AND_ATTRIBUTES, SecurityImpersonation,
        SetTokenInformation, TOKEN_ACCESS_MASK, TOKEN_ADJUST_DEFAULT, TOKEN_ALL_ACCESS,
        TOKEN_DUPLICATE, TOKEN_IMPERSONATE, TOKEN_MANDATORY_LABEL, TOKEN_QUERY, TokenImpersonation,
        TokenImpersonationLevel, TokenIntegrityLevel,
    };
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::Win32::System::SystemServices::SE_GROUP_INTEGRITY;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, GetCurrentThread, OpenProcessToken, OpenThreadToken, SetThreadToken,
    };
    use windows::core::{PCWSTR, s, w};

    /// A made-up AppContainer package SID (`S-1-15-2-` plus seven RIDs). No package or profile
    /// with this SID exists.
    const TEST_PACKAGE_SID: PCWSTR =
        w!("S-1-15-2-3524604950-1158209347-3010532185-1306467402-2541658385-2232617311-4161216301");
    /// The low mandatory level.
    const LOW_INTEGRITY_SID: PCWSTR = w!("S-1-16-4096");

    /// `NtCreateLowBoxToken` (ntdll).
    type CreateLowBoxToken = unsafe extern "system" fn(
        token: *mut HANDLE,
        existing: HANDLE,
        access: u32,
        attributes: *const core::ffi::c_void,
        package_sid: PSID,
        capability_count: u32,
        capabilities: *const SID_AND_ATTRIBUTES,
        handle_count: u32,
        handles: *const HANDLE,
    ) -> i32;

    /// A SID parsed from its string form, freed on drop.
    struct Sid(PSID);

    impl Sid {
        fn parse(text: PCWSTR) -> Self {
            let mut sid = PSID::default();
            // SAFETY: `text` is a NUL-terminated literal; `sid` is a valid out pointer.
            unsafe { ConvertStringSidToSidW(text, &mut sid) }.unwrap();
            Self(sid)
        }
    }

    impl Drop for Sid {
        fn drop(&mut self) {
            // SAFETY: ConvertStringSidToSidW allocated the SID with LocalAlloc; freed once.
            unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
        }
    }

    /// Reverts the calling thread's impersonation when dropped.
    struct Revert;

    impl Drop for Revert {
        fn drop(&mut self) {
            // SAFETY: no preconditions; the thread ends right after anyway.
            let _ = unsafe { RevertToSelf() };
        }
    }

    fn own(handle: HANDLE) -> OwnedHandle {
        // SAFETY: `handle` was just returned by a successful call and is owned by nobody else.
        unsafe { OwnedHandle::from_raw_handle(handle.0) }
    }

    fn raw(handle: &OwnedHandle) -> HANDLE {
        HANDLE(handle.as_raw_handle())
    }

    fn process_token(access: TOKEN_ACCESS_MASK) -> OwnedHandle {
        let mut token = HANDLE::default();
        // SAFETY: the pseudo handle of this process; `token` is a valid out pointer.
        unsafe { OpenProcessToken(GetCurrentProcess(), access, &mut token) }.unwrap();
        own(token)
    }

    fn impersonation_copy(token: &OwnedHandle) -> OwnedHandle {
        let mut copy = HANDLE::default();
        // SAFETY: `token` is valid with TOKEN_DUPLICATE; `copy` is a valid out pointer.
        unsafe {
            DuplicateTokenEx(
                raw(token),
                TOKEN_QUERY | TOKEN_IMPERSONATE | TOKEN_ADJUST_DEFAULT | TOKEN_DUPLICATE,
                None,
                SecurityImpersonation,
                TokenImpersonation,
                &mut copy,
            )
        }
        .unwrap();
        own(copy)
    }

    /// An impersonation token derived from this process's token.
    #[derive(Debug)]
    pub struct TestToken(OwnedHandle);

    impl TestToken {
        /// This process's token lowered to the low integrity level (not an AppContainer).
        pub fn low_integrity() -> Self {
            let token = impersonation_copy(&process_token(TOKEN_DUPLICATE | TOKEN_QUERY));
            let low = Sid::parse(LOW_INTEGRITY_SID);
            let label = TOKEN_MANDATORY_LABEL {
                Label: SID_AND_ATTRIBUTES {
                    Sid: low.0,
                    Attributes: SE_GROUP_INTEGRITY as u32,
                },
            };
            // SAFETY: `low.0` is a valid SID.
            let length = size_of::<TOKEN_MANDATORY_LABEL>() as u32 + unsafe { GetLengthSid(low.0) };
            // SAFETY: the token has TOKEN_ADJUST_DEFAULT; `label` and the SID it points to
            // outlive the call. Lowering the level needs no privilege.
            unsafe {
                SetTokenInformation(
                    raw(&token),
                    TokenIntegrityLevel,
                    (&raw const label).cast(),
                    length,
                )
            }
            .unwrap();
            Self(token)
        }

        /// An AppContainer (lowbox) token of this process's user for [`TEST_PACKAGE_SID`].
        pub fn app_container() -> Self {
            // SAFETY: a literal module name; ntdll is loaded in every process.
            let ntdll = unsafe { GetModuleHandleW(w!("ntdll.dll")) }.unwrap();
            // SAFETY: a literal export name of a loaded module.
            let address = unsafe { GetProcAddress(ntdll, s!("NtCreateLowBoxToken")) }
                .expect("ntdll exports NtCreateLowBoxToken");
            // SAFETY: the export has the signature of `CreateLowBoxToken` (ntseapi.h).
            let create: CreateLowBoxToken = unsafe { core::mem::transmute(address) };
            let base = process_token(TOKEN_DUPLICATE | TOKEN_QUERY);
            let package = Sid::parse(TEST_PACKAGE_SID);
            let mut lowbox = HANDLE::default();
            // SAFETY: valid out pointer and base token (with TOKEN_DUPLICATE); no object
            // attributes, capabilities, or handles.
            let status = unsafe {
                create(
                    &mut lowbox,
                    raw(&base),
                    TOKEN_ALL_ACCESS.0,
                    core::ptr::null(),
                    package.0,
                    0,
                    core::ptr::null(),
                    0,
                    core::ptr::null(),
                )
            };
            assert_eq!(status, 0, "NtCreateLowBoxToken failed with {status:#010x}");
            Self(impersonation_copy(&own(lowbox)))
        }

        /// Runs `body` on a new thread that impersonates this token (at impersonation level,
        /// checked) and returns its result; a panic in `body` fails the calling test.
        pub fn impersonate<T: Send>(&self, body: impl FnOnce() -> T + Send) -> T {
            let token = &self.0;
            on_impersonating_thread(
                || {
                    // SAFETY: an impersonation token with TOKEN_IMPERSONATE, for this thread.
                    unsafe { SetThreadToken(None, Some(raw(token))) }.unwrap();
                    assert_eq!(
                        thread_impersonation_level(),
                        SecurityImpersonation,
                        "the impersonation was downgraded"
                    );
                },
                body,
            )
        }
    }

    /// Runs `body` on a new thread that impersonates the anonymous logon (another account).
    pub fn as_anonymous<T: Send>(body: impl FnOnce() -> T + Send) -> T {
        on_impersonating_thread(
            || {
                // SAFETY: the pseudo handle of the calling thread.
                unsafe { ImpersonateAnonymousToken(GetCurrentThread()) }.unwrap();
            },
            body,
        )
    }

    fn on_impersonating_thread<T: Send>(
        impersonate: impl FnOnce() + Send,
        body: impl FnOnce() -> T + Send,
    ) -> T {
        std::thread::scope(|scope| {
            let worker = scope.spawn(|| {
                let _revert = Revert;
                impersonate();
                body()
            });
            match worker.join() {
                Ok(value) => value,
                Err(payload) => std::panic::resume_unwind(payload),
            }
        })
    }

    fn thread_impersonation_level() -> SECURITY_IMPERSONATION_LEVEL {
        let mut token = HANDLE::default();
        // SAFETY: the pseudo handle of the calling thread; opened as self so that the check
        // does not depend on the impersonated token.
        unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut token) }.unwrap();
        let token = own(token);
        let mut level = SECURITY_IMPERSONATION_LEVEL::default();
        let mut returned = 0_u32;
        // SAFETY: `level` is a buffer of the size passed.
        unsafe {
            GetTokenInformation(
                raw(&token),
                TokenImpersonationLevel,
                Some((&raw mut level).cast()),
                size_of::<SECURITY_IMPERSONATION_LEVEL>() as u32,
                &mut returned,
            )
        }
        .unwrap();
        level
    }
}
