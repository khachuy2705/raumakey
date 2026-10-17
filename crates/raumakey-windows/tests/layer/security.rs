//! `security`: user-only and AppContainer-client security descriptors.

use raumakey_windows::security::{
    ALL_APPLICATION_PACKAGES_SID, MEDIUM_MANDATORY_RID, PIPE_CLIENT_ACCESS, SddlError,
    app_container_client_sddl, is_label_at_least_medium, is_valid_sid_string, user_only_sddl,
};

#[test]
fn sid_strings_are_validated_strictly() {
    for good in [
        "S-1-5-18",
        "S-1-5-21-1004336348-1177238915-682003330-1001",
        "S-1-12-1-1-2-3-4",
    ] {
        assert!(is_valid_sid_string(good), "{good}");
    }
    for bad in [
        "",
        "S-1",
        "S-1-",
        "S-1-5-",
        "s-1-5-18",
        "S-1-5--18",
        "S-1-5-18)(A;;GA;;;WD",
        "S-1-5-18 ",
        "WD",
        "S-1-5-1x",
    ] {
        assert!(!is_valid_sid_string(bad), "{bad:?}");
    }
}

#[test]
fn user_only_sddl_grants_exactly_one_user_with_a_protected_dacl() {
    let sid = "S-1-5-21-1004336348-1177238915-682003330-1001";
    assert_eq!(
        user_only_sddl(sid).unwrap(),
        format!("O:{sid}D:P(A;;GA;;;{sid})")
    );
    assert_eq!(
        user_only_sddl("S-1-5-18)(A;;GA;;;WD"),
        Err(SddlError::InvalidSid)
    );
}

#[test]
fn app_container_client_sddl_adds_client_access_for_app_packages_and_no_label() {
    let sid = "S-1-5-21-1004336348-1177238915-682003330-1001";
    assert_eq!(
        app_container_client_sddl(sid).unwrap(),
        format!("O:{sid}D:P(A;;GA;;;{sid})(A;;0x12008b;;;AC)")
    );
    assert_eq!(
        app_container_client_sddl("S-1-5-18)(A;;GA;;;WD"),
        Err(SddlError::InvalidSid)
    );
    assert_eq!(ALL_APPLICATION_PACKAGES_SID, "S-1-15-2-1");
}

#[test]
fn only_pipes_labeled_medium_or_above_can_belong_to_the_controller() {
    assert_eq!(MEDIUM_MANDATORY_RID, 0x2000);
    // No label counts as medium.
    assert!(is_label_at_least_medium(None));
    for rid in [0x2000, 0x2100, 0x3000, 0x4000] {
        assert!(is_label_at_least_medium(Some(rid)), "{rid:#x}");
    }
    // Untrusted, low, and anything else below medium.
    for rid in [0, 0x1000, 0x1fff] {
        assert!(!is_label_at_least_medium(Some(rid)), "{rid:#x}");
    }
}

#[test]
fn pipe_clients_get_read_access_and_write_data_only() {
    // FILE_GENERIC_READ (READ_CONTROL, SYNCHRONIZE, FILE_READ_DATA, FILE_READ_EA,
    // FILE_READ_ATTRIBUTES) | FILE_WRITE_DATA.
    assert_eq!(PIPE_CLIENT_ACCESS, 0x0012_008B);
    for (right, name) in [
        (0x0000_0004, "FILE_CREATE_PIPE_INSTANCE (FILE_APPEND_DATA)"),
        (0x0000_0010, "FILE_WRITE_EA"),
        (0x0000_0100, "FILE_WRITE_ATTRIBUTES"),
        (0x0001_0000, "DELETE"),
        (0x0004_0000, "WRITE_DAC"),
        (0x0008_0000, "WRITE_OWNER"),
        (0x1000_0000, "GENERIC_ALL"),
        (0x4000_0000, "GENERIC_WRITE"),
    ] {
        assert_eq!(PIPE_CLIENT_ACCESS & right, 0, "{name}");
    }
}

#[cfg(windows)]
mod win {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::security::{
        ALL_APPLICATION_PACKAGES_SID, AppContainerClientSecurity, PIPE_CLIENT_ACCESS,
        UserOnlySecurity, kernel_object_owner_sid, kernel_object_sddl,
    };
    use raumakey_windows::sid::current_user_sid_string;
    use raumakey_windows::strings::WideString;
    use windows::Win32::Foundation::{GENERIC_ALL, HANDLE};
    use windows::Win32::Security::SECURITY_ATTRIBUTES;
    use windows::Win32::Storage::FileSystem::{
        FILE_CREATE_PIPE_INSTANCE, FILE_GENERIC_READ, FILE_WRITE_DATA,
    };
    use windows::Win32::System::SystemServices::ACCESS_ALLOWED_ACE_TYPE;
    use windows::Win32::System::Threading::CreateEventW;

    use crate::support::{DescriptorParts, unique_suffix};

    /// `(ACE type, mask, SID)` of an access-allowed ACE.
    fn allowed(mask: u32, sid: &str) -> (u8, u32, String) {
        (ACCESS_ALLOWED_ACE_TYPE as u8, mask, sid.to_owned())
    }

    #[test]
    fn pipe_client_access_is_file_generic_read_plus_file_write_data() {
        assert_eq!(PIPE_CLIENT_ACCESS, (FILE_GENERIC_READ | FILE_WRITE_DATA).0);
        assert_eq!(PIPE_CLIENT_ACCESS & FILE_CREATE_PIPE_INSTANCE.0, 0);
    }

    #[test]
    fn the_app_container_client_descriptor_grants_the_user_and_app_packages_without_a_label() {
        let security = AppContainerClientSecurity::for_current_user().unwrap();
        let sid = current_user_sid_string().unwrap();
        assert_eq!(security.sid(), sid);
        let attributes = security.as_attributes();
        assert_eq!(security.as_ptr(), std::ptr::from_ref(attributes));
        assert_eq!(
            attributes.nLength as usize,
            size_of::<SECURITY_ATTRIBUTES>()
        );
        assert!(!attributes.bInheritHandle.as_bool());
        assert_eq!(attributes.lpSecurityDescriptor, security.descriptor().0);
        assert_eq!(
            security.to_sddl().unwrap(),
            format!("O:{sid}D:P(A;;GA;;;{sid})(A;;0x12008b;;;AC)")
        );

        // Exactly these two ACEs: no Everyone, Authenticated Users, or other account.
        let parts = DescriptorParts::read(security.descriptor());
        assert_eq!(parts.owner, sid);
        assert!(parts.dacl_protected);
        assert_eq!(
            parts.dacl,
            vec![
                allowed(GENERIC_ALL.0, &sid),
                allowed(PIPE_CLIENT_ACCESS, ALL_APPLICATION_PACKAGES_SID),
            ]
        );
        // No mandatory label: the object counts as medium with NO_WRITE_UP, which keeps
        // low-integrity processes that are not AppContainers from writing to it.
        assert!(parts.sacl.is_empty());
    }

    #[test]
    fn the_app_container_client_descriptor_rejects_invalid_sids() {
        let error = AppContainerClientSecurity::for_sid("S-1-5-18)(A;;GA;;;WD").unwrap_err();
        assert_eq!(error.kind(), WinErrorKind::InvalidInput);
    }

    #[test]
    fn the_user_only_descriptor_has_no_app_package_ace_and_no_label() {
        let security = UserOnlySecurity::for_current_user().unwrap();
        let sid = current_user_sid_string().unwrap();
        let parts = DescriptorParts::read(security.descriptor());
        assert_eq!(parts.owner, sid);
        assert!(parts.dacl_protected);
        assert_eq!(parts.dacl, vec![allowed(GENERIC_ALL.0, &sid)]);
        assert!(parts.sacl.is_empty());
    }

    #[test]
    fn attributes_describe_a_non_inheritable_user_only_descriptor() {
        let security = UserOnlySecurity::for_current_user().unwrap();
        let attributes = security.as_attributes();
        assert_eq!(security.as_ptr(), std::ptr::from_ref(attributes));
        assert_eq!(
            attributes.nLength as usize,
            size_of::<SECURITY_ATTRIBUTES>()
        );
        assert!(!attributes.bInheritHandle.as_bool());
        assert!(!attributes.lpSecurityDescriptor.is_null());
        let sid = current_user_sid_string().unwrap();
        assert_eq!(security.sid(), sid);
        let sddl = security.to_sddl().unwrap();
        assert!(sddl.contains("D:P"), "{sddl}");
        assert!(sddl.contains(&sid), "{sddl}");
    }

    #[test]
    fn objects_created_with_it_get_the_user_only_dacl_and_owner() {
        let security = UserOnlySecurity::for_current_user().unwrap();
        let name = WideString::new(&format!(
            "Local\\Raumakey-Test-{}-security",
            unique_suffix()
        ))
        .unwrap();
        // SAFETY: the attributes and the name outlive the call; the handle is owned below.
        let handle =
            unsafe { CreateEventW(Some(security.as_ptr()), true, false, name.as_pcwstr()) }
                .unwrap();
        // SAFETY: `handle` was just returned by CreateEventW and is owned by nobody else.
        let owned = unsafe { OwnedHandle::from_raw_handle(handle.0) };
        let raw = HANDLE(owned.as_raw_handle());
        let sid = current_user_sid_string().unwrap();
        let sddl = kernel_object_sddl(raw).unwrap();
        assert!(sddl.contains("D:P(A;;"), "{sddl}");
        assert_eq!(sddl.matches("(A;").count(), 1, "{sddl}");
        assert!(sddl.contains(&sid), "{sddl}");
        assert_eq!(kernel_object_owner_sid(raw).unwrap(), sid);
    }
}
