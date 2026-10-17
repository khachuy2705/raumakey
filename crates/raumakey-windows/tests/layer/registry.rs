//! `registry` (Win32 part). Writes happen only under `HKCU\Software\Raumakey\Tests-<pid>-<n>`,
//! which every test deletes again.

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::registry::{
    OwnedRegKey, RegKey, RegRoot, RegValue, RegView, delete_empty_key, delete_tree, key_exists,
};

use crate::support::unique_suffix;

/// A test key that is deleted on drop.
struct TestKey {
    path: String,
}

impl TestKey {
    fn new() -> Self {
        Self {
            path: format!("Software\\Raumakey\\Tests-{}", unique_suffix()),
        }
    }
}

impl Drop for TestKey {
    fn drop(&mut self) {
        let _ = delete_tree(RegRoot::CurrentUser, RegView::Default, &self.path);
        // The product root may have been created as an intermediate key; remove it again when
        // nothing else lives there.
        let _ = delete_empty_key(RegRoot::CurrentUser, RegView::Default, "Software\\Raumakey");
    }
}

#[test]
fn values_round_trip_through_an_owned_key() {
    let test = TestKey::new();
    let key = OwnedRegKey::create(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap();
    assert_eq!(key.path(), test.path);
    key.set_string("", "Raumakey").unwrap();
    key.set_string("text", "Tiếng Việt 😀").unwrap();
    key.set_expand_string("expand", "%LOCALAPPDATA%\\Raumakey")
        .unwrap();
    key.set_dword("dword", 42).unwrap();
    key.set_qword("qword", 1 << 40).unwrap();
    key.set_multi_string("multi", &["a", "ế"]).unwrap();
    key.set_binary("binary", &[1, 2, 3]).unwrap();

    let read = RegKey::open_read(RegRoot::CurrentUser, RegView::Default, &test.path)
        .unwrap()
        .expect("the key exists");
    assert_eq!(read.get_string("").unwrap().as_deref(), Some("Raumakey"));
    assert_eq!(
        read.get_string("text").unwrap().as_deref(),
        Some("Tiếng Việt 😀")
    );
    assert_eq!(
        read.get_value("expand").unwrap(),
        Some(RegValue::ExpandString(
            "%LOCALAPPDATA%\\Raumakey".to_owned()
        ))
    );
    assert_eq!(read.get_dword("dword").unwrap(), Some(42));
    assert_eq!(read.get_qword("qword").unwrap(), Some(1 << 40));
    assert_eq!(
        read.get_multi_string("multi").unwrap(),
        Some(vec!["a".to_owned(), "ế".to_owned()])
    );
    assert_eq!(read.get_binary("binary").unwrap(), Some(vec![1, 2, 3]));
    assert_eq!(read.get_value("missing").unwrap(), None);
    assert_eq!(read.get_dword("missing").unwrap(), None);
    // A type mismatch is an error, not a silent None.
    assert_eq!(
        read.get_dword("text").unwrap_err().kind(),
        WinErrorKind::InvalidInput
    );

    let mut values = read.enum_values().unwrap();
    values.sort_by(|a, b| a.0.cmp(&b.0));
    let names: Vec<&str> = values.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(
        names,
        ["", "binary", "dword", "expand", "multi", "qword", "text"]
    );
    assert!(values.contains(&("dword".to_owned(), RegValue::Dword(42))));

    assert!(key.delete_value("dword").unwrap());
    assert!(!key.delete_value("dword").unwrap());
    assert_eq!(read.get_dword("dword").unwrap(), None);
}

#[test]
fn subkeys_are_enumerated_and_trees_deleted() {
    let test = TestKey::new();
    let key = OwnedRegKey::create(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap();
    let child = key.create_subkey("Child\\Grandchild").unwrap();
    child.set_dword("x", 1).unwrap();
    key.create_subkey("Other").unwrap();
    let read = RegKey::open_read(RegRoot::CurrentUser, RegView::Default, &test.path)
        .unwrap()
        .unwrap();
    let mut subkeys = read.enum_subkeys().unwrap();
    subkeys.sort();
    assert_eq!(subkeys, ["Child", "Other"]);
    let nested = read.open_subkey("Child\\Grandchild").unwrap().unwrap();
    assert_eq!(nested.get_dword("x").unwrap(), Some(1));
    assert!(read.open_subkey("Missing").unwrap().is_none());
    drop((key, child, read, nested));

    assert!(key_exists(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    assert!(delete_tree(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    assert!(!key_exists(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    assert!(
        RegKey::open_read(RegRoot::CurrentUser, RegView::Default, &test.path)
            .unwrap()
            .is_none()
    );
    assert!(!delete_tree(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
}

#[test]
fn the_write_guard_refuses_foreign_paths_without_touching_them() {
    let foreign = format!("Software\\Raumakey-Guard-{}", unique_suffix());
    let error = OwnedRegKey::create(RegRoot::CurrentUser, RegView::Default, &foreign).unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::AccessDenied);
    assert!(!key_exists(RegRoot::CurrentUser, RegView::Default, &foreign).unwrap());
    assert_eq!(
        delete_tree(
            RegRoot::CurrentUser,
            RegView::Default,
            "Software\\Microsoft"
        )
        .unwrap_err()
        .kind(),
        WinErrorKind::AccessDenied
    );
    assert_eq!(
        OwnedRegKey::open(
            RegRoot::CurrentUser,
            RegView::Default,
            "Software\\Microsoft"
        )
        .unwrap_err()
        .kind(),
        WinErrorKind::AccessDenied
    );

    let test = TestKey::new();
    let key = OwnedRegKey::create(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap();
    for escape in ["..\\..\\Escape", "a\\..\\..", "", "\\a", "a\\"] {
        assert_eq!(
            key.create_subkey(escape).unwrap_err().kind(),
            WinErrorKind::AccessDenied,
            "{escape:?}"
        );
    }
    assert!(!key_exists(RegRoot::CurrentUser, RegView::Default, "Software\\Escape").unwrap());
}

#[test]
fn opening_an_existing_owned_key_and_a_missing_one() {
    let test = TestKey::new();
    assert!(
        OwnedRegKey::open(RegRoot::CurrentUser, RegView::Default, &test.path)
            .unwrap()
            .is_none()
    );
    OwnedRegKey::create(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap();
    let key = OwnedRegKey::open(RegRoot::CurrentUser, RegView::Default, &test.path)
        .unwrap()
        .unwrap();
    key.set_string("v", "x").unwrap();
    assert_eq!(key.get_string("v").unwrap().as_deref(), Some("x"));
}

#[test]
fn foreign_roots_can_be_read_in_both_views() {
    for view in [RegView::Default, RegView::Wow64_64, RegView::Wow64_32] {
        let key = RegKey::open_read(
            RegRoot::LocalMachine,
            view,
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
        )
        .unwrap()
        .expect("CurrentVersion exists in every view");
        assert!(key.get_string("CurrentBuild").unwrap().is_some());
    }
    assert!(
        RegKey::open_read(
            RegRoot::LocalMachine,
            RegView::Default,
            "SOFTWARE\\Raumakey-Test-Missing-Key"
        )
        .unwrap()
        .is_none()
    );
}

#[test]
fn delete_empty_key_removes_only_empty_owned_keys() {
    let test = TestKey::new();
    let key = OwnedRegKey::create(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap();
    key.create_subkey("Child").unwrap();
    key.set_dword("v", 1).unwrap();
    drop(key);
    let child = format!("{}\\Child", test.path);
    // Not empty: a value, then a subkey, keep it alive.
    assert!(!delete_empty_key(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    assert!(key_exists(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    assert!(delete_empty_key(RegRoot::CurrentUser, RegView::Default, &child).unwrap());
    let key = OwnedRegKey::open(RegRoot::CurrentUser, RegView::Default, &test.path)
        .unwrap()
        .unwrap();
    assert!(!delete_empty_key(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    key.delete_value("v").unwrap();
    drop(key);
    assert!(delete_empty_key(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    assert!(!key_exists(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    // Missing keys and foreign paths.
    assert!(!delete_empty_key(RegRoot::CurrentUser, RegView::Default, &test.path).unwrap());
    assert_eq!(
        delete_empty_key(
            RegRoot::CurrentUser,
            RegView::Default,
            "Software\\Microsoft"
        )
        .unwrap_err()
        .kind(),
        WinErrorKind::AccessDenied
    );
}
