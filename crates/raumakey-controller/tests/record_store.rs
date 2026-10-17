//! The registry-backed installation record (`system::RegistryRecordStore`, Windows only).
//!
//! Production uses `HKLM\Software\Raumakey\Install`; these tests use a key of their own,
//! `HKCU\Software\Raumakey\Tests-controller-<pid>-<n>\Install`, which each test deletes again,
//! with `HKCU\Software\Raumakey` when nothing else is left in it. Nothing else is written.
//!
//! The tests that write run one at a time ([`REGISTRY`]): a test that deletes the empty
//! `Software\Raumakey` must never do so while another one creates its key below it, which then
//! fails with `ERROR_KEY_DELETED` or `ERROR_FILE_NOT_FOUND`.

#![cfg(windows)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use raumakey_controller::install::{INSTALL_RECORD_KEY, InstallRecord};
use raumakey_controller::system::RegistryRecordStore;
use raumakey_windows::error::WinErrorKind;
use raumakey_windows::registry::{
    OwnedRegKey, RegKey, RegRoot, RegValue, RegView, delete_empty_key, delete_tree,
};
use raumakey_windows::tsf_registration::{TipArch, TipBinary};

static SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Held by each [`TestKey`] while it exists, so the tests below `HKCU\Software\Raumakey` run
/// one at a time. A test that failed while holding it leaves it poisoned, which is ignored.
static REGISTRY: Mutex<()> = Mutex::new(());

/// `Software\Raumakey\Tests-controller-<pid>-<n>`, deleted on drop (while [`REGISTRY`] is still
/// held: fields are dropped after `Drop::drop`).
struct TestKey {
    parent: String,
    _serial: MutexGuard<'static, ()>,
}

impl TestKey {
    fn new() -> Self {
        let serial = REGISTRY.lock().unwrap_or_else(PoisonError::into_inner);
        let n = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        Self {
            parent: format!(
                "Software\\Raumakey\\Tests-controller-{}-{n}",
                std::process::id()
            ),
            _serial: serial,
        }
    }

    fn record_path(&self) -> String {
        format!("{}\\Install", self.parent)
    }

    fn store(&self) -> RegistryRecordStore {
        RegistryRecordStore::at(RegRoot::CurrentUser, RegView::Default, &self.record_path())
    }
}

impl Drop for TestKey {
    fn drop(&mut self) {
        let _ = delete_tree(RegRoot::CurrentUser, RegView::Default, &self.parent);
        let _ = delete_empty_key(RegRoot::CurrentUser, RegView::Default, "Software\\Raumakey");
    }
}

fn record() -> InstallRecord {
    InstallRecord {
        version: "0.1.0".to_owned(),
        tips: vec![
            TipBinary {
                path: "C:\\Program Files\\Raumakey\\x64\\raumakey_tip.dll".into(),
                arch: TipArch::X64,
            },
            TipBinary {
                path: "C:\\Program Files\\Raumakey\\x86\\raumakey_tip.dll".into(),
                arch: TipArch::X86,
            },
        ],
    }
}

#[test]
fn the_production_record_lives_in_the_64_bit_view_of_hklm() {
    assert_eq!(
        RegistryRecordStore::machine(),
        RegistryRecordStore::at(RegRoot::LocalMachine, RegView::Wow64_64, INSTALL_RECORD_KEY)
    );
}

#[test]
fn a_missing_record_reads_as_none() {
    let key = TestKey::new();
    assert_eq!(key.store().read().unwrap(), None);
    assert!(!key.store().remove().unwrap());
}

#[test]
fn a_record_round_trips_as_string_values() {
    let key = TestKey::new();
    let store = key.store();
    store.write(&record()).unwrap();
    assert_eq!(store.read().unwrap(), Some(record()));
    let raw = RegKey::open_read(RegRoot::CurrentUser, RegView::Default, &key.record_path())
        .unwrap()
        .expect("the record key exists");
    assert_eq!(raw.get_string("Version").unwrap().as_deref(), Some("0.1.0"));
    assert_eq!(
        raw.get_string("TipX86").unwrap().as_deref(),
        Some("C:\\Program Files\\Raumakey\\x86\\raumakey_tip.dll")
    );
}

#[test]
fn writing_replaces_the_values_of_architectures_the_new_record_lacks() {
    let key = TestKey::new();
    let store = key.store();
    let mut old = record();
    old.tips.push(TipBinary {
        path: "C:\\old\\arm64\\raumakey_tip.dll".into(),
        arch: TipArch::Arm64,
    });
    store.write(&old).unwrap();
    store.write(&record()).unwrap();
    assert_eq!(store.read().unwrap(), Some(record()));
}

#[test]
fn values_of_another_type_are_ignored_when_reading() {
    let key = TestKey::new();
    let raw =
        OwnedRegKey::create(RegRoot::CurrentUser, RegView::Default, &key.record_path()).unwrap();
    raw.set_dword("TipX64", 7).unwrap();
    raw.set_string("TipX86", "C:\\x86\\raumakey_tip.dll")
        .unwrap();
    let read = key.store().read().unwrap().expect("the record key exists");
    assert_eq!(read.version, "");
    assert_eq!(
        read.tips,
        vec![TipBinary {
            path: "C:\\x86\\raumakey_tip.dll".into(),
            arch: TipArch::X86,
        }]
    );
    assert_eq!(raw.get_value("TipX64").unwrap(), Some(RegValue::Dword(7)));
}

#[test]
fn removing_deletes_the_record_and_its_empty_parent_only() {
    let key = TestKey::new();
    let store = key.store();
    store.write(&record()).unwrap();
    assert!(store.remove().unwrap());
    assert_eq!(store.read().unwrap(), None);
    // The parent was created for the record and is empty now: it goes too.
    assert!(
        RegKey::open_read(RegRoot::CurrentUser, RegView::Default, &key.parent)
            .unwrap()
            .is_none()
    );

    // A parent that holds anything else stays.
    let sibling = OwnedRegKey::create(
        RegRoot::CurrentUser,
        RegView::Default,
        &format!("{}\\Other", key.parent),
    )
    .unwrap();
    drop(sibling);
    store.write(&record()).unwrap();
    assert!(store.remove().unwrap());
    assert!(
        RegKey::open_read(RegRoot::CurrentUser, RegView::Default, &key.parent)
            .unwrap()
            .is_some()
    );
}

#[test]
fn a_record_outside_the_owned_registry_paths_is_refused() {
    let store = RegistryRecordStore::at(
        RegRoot::CurrentUser,
        RegView::Default,
        "Software\\NotRaumakey\\Install",
    );
    assert_eq!(
        store.write(&record()).unwrap_err().kind(),
        WinErrorKind::AccessDenied
    );
    assert_eq!(
        store.remove().unwrap_err().kind(),
        WinErrorKind::AccessDenied
    );
}

/// Writes `HKLM\Software\Raumakey\Install` (needs elevation). Manual test only.
#[test]
#[ignore = "manual: writes and deletes HKLM\\Software\\Raumakey\\Install; run elevated"]
fn manual_machine_record_round_trip() {
    let store = RegistryRecordStore::machine();
    let before = store.read().unwrap();
    store.write(&record()).unwrap();
    assert_eq!(store.read().unwrap(), Some(record()));
    match before {
        Some(previous) => store.write(&previous).unwrap(),
        None => {
            store.remove().unwrap();
        }
    }
}
