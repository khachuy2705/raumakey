//! The excluded applications as pure data (`exclusions`, D47, D48): the edits of the
//! exclusions dialog and what they do to the list of `[exclusions] apps`, and the state the
//! dialog shows. Pure: these tests run on every host.

use std::path::{Path, PathBuf};

use raumakey_config::ExclusionsSection;
use raumakey_controller::exclusions::{ExclusionsRequest, ExclusionsState, default_apps};
use raumakey_controller::settings::Location;
use raumakey_model::host::ExeName;

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn apps(names: &[&str]) -> Vec<ExeName> {
    names.iter().map(|name| exe(name)).collect()
}

/// The defaults are the Remote Desktop and Hyper-V clients of the schema, in its order.
#[test]
fn the_defaults_are_the_remote_control_applications_of_the_schema() {
    assert_eq!(
        default_apps(),
        apps(&["mstsc.exe", "msrdc.exe", "rdcman.exe", "vmconnect.exe"])
    );
    assert_eq!(default_apps(), ExclusionsSection::default().apps);
}

/// "Thêm" appends the application at the end; one that is listed already stays where it is,
/// and the request holds already.
#[test]
fn add_appends_an_application_once() {
    let mut list = apps(&["mstsc.exe", "putty.exe"]);
    let add = ExclusionsRequest::Add(exe("notepad.exe"));
    assert!(!add.holds_in(&list));
    add.apply_to(&mut list);
    assert_eq!(list, apps(&["mstsc.exe", "putty.exe", "notepad.exe"]));
    assert!(add.holds_in(&list));
    let again = ExclusionsRequest::Add(exe("putty.exe"));
    assert!(again.holds_in(&list));
    again.apply_to(&mut list);
    assert_eq!(list, apps(&["mstsc.exe", "putty.exe", "notepad.exe"]));
}

/// "Xoá" removes the application; one that is not listed holds already.
#[test]
fn remove_takes_the_application_out() {
    let mut list = apps(&["mstsc.exe", "putty.exe", "notepad.exe"]);
    let remove = ExclusionsRequest::Remove(exe("putty.exe"));
    assert!(!remove.holds_in(&list));
    remove.apply_to(&mut list);
    assert_eq!(list, apps(&["mstsc.exe", "notepad.exe"]));
    assert!(remove.holds_in(&list));
    remove.apply_to(&mut list);
    assert_eq!(list, apps(&["mstsc.exe", "notepad.exe"]));
    let mut empty = Vec::new();
    ExclusionsRequest::Remove(exe("mstsc.exe")).apply_to(&mut empty);
    assert!(empty.is_empty());
}

/// "Mặc định" puts the list back to the defaults, in their order; it holds only for exactly
/// that list.
#[test]
fn defaults_restore_the_list_of_the_schema() {
    let mut list = apps(&["putty.exe"]);
    assert!(!ExclusionsRequest::Defaults.holds_in(&list));
    ExclusionsRequest::Defaults.apply_to(&mut list);
    assert_eq!(list, default_apps());
    assert!(ExclusionsRequest::Defaults.holds_in(&list));
    let mut reordered = default_apps();
    reordered.reverse();
    assert!(!ExclusionsRequest::Defaults.holds_in(&reordered));
    assert!(!ExclusionsRequest::Defaults.holds_in(&[]));
}

/// The dialog shows the list in effect and whether its edits are saved: not without a folder.
#[test]
fn the_state_says_whether_the_edits_are_saved() {
    let path = PathBuf::from(r"D:\Tools\Raumakey\raumakey.toml");
    let list = apps(&["mstsc.exe"]);
    let state = ExclusionsState::new(&list, Location::ExeDir, Some(&path));
    assert_eq!(
        state,
        ExclusionsState {
            apps: list.clone(),
            can_save: true
        }
    );
    assert!(ExclusionsState::new(&list, Location::LocalAppData, Some(&path)).can_save);
    assert!(!ExclusionsState::new(&list, Location::Unavailable, None).can_save);
    assert!(!ExclusionsState::new(&list, Location::ExeDir, None::<&Path>).can_save);
}
