//! `security`: user-only security descriptors.

use raumakey_windows::security::{SddlError, is_valid_sid_string, user_only_sddl};

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

#[cfg(windows)]
mod win {
    use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};

    use raumakey_windows::security::{
        UserOnlySecurity, kernel_object_owner_sid, kernel_object_sddl,
    };
    use raumakey_windows::sid::current_user_sid_string;
    use raumakey_windows::strings::WideString;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Security::SECURITY_ATTRIBUTES;
    use windows::Win32::System::Threading::CreateEventW;

    use crate::support::unique_suffix;

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
