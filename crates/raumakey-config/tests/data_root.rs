use std::fs;
use std::path::Path;
use std::sync::Arc;

use raumakey_config::{ConfigDocument, ConfigError, DataRoot, MacroDocument, StdReplacer};

#[test]
fn the_portable_marker_selects_the_data_folder_beside_the_executable() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::write(exe_dir.path().join("raumakey.portable"), "").unwrap();
    let root = DataRoot::resolve(exe_dir.path(), Some(local.path())).unwrap();
    assert!(root.is_portable());
    assert_eq!(root.path(), exe_dir.path().join("data"));
    let without_local = DataRoot::resolve(exe_dir.path(), None).unwrap();
    assert_eq!(without_local, root);
}

#[test]
fn without_the_marker_the_root_is_under_local_app_data() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    // A writable executable folder alone never means portable (plan D9).
    fs::write(exe_dir.path().join("raumakey.portable.txt"), "").unwrap();
    fs::create_dir(exe_dir.path().join("data")).unwrap();
    let root = DataRoot::resolve(exe_dir.path(), Some(local.path())).unwrap();
    assert!(!root.is_portable());
    assert_eq!(root.path(), local.path().join("Raumakey"));
}

#[test]
fn no_marker_and_no_usable_local_app_data_is_an_error() {
    let exe_dir = tempfile::tempdir().unwrap();
    assert_eq!(
        DataRoot::resolve(exe_dir.path(), None).unwrap_err(),
        ConfigError::NotFound
    );
    assert_eq!(
        DataRoot::resolve(exe_dir.path(), Some(Path::new(""))).unwrap_err(),
        ConfigError::NotFound
    );
    assert_eq!(
        DataRoot::resolve(exe_dir.path(), Some(Path::new("relative/appdata"))).unwrap_err(),
        ConfigError::NotFound
    );
}

/// Finding: a relative executable folder (for example `""` when the caller could not find the
/// executable) would resolve both the marker and the data root against the current directory.
#[test]
fn a_relative_executable_folder_is_refused() {
    let local = tempfile::tempdir().unwrap();
    for exe_dir in [
        "",
        ".",
        "relative",
        "relative/bin",
        "C:relative",
        "\\rooted",
    ] {
        assert_eq!(
            DataRoot::resolve(Path::new(exe_dir), Some(local.path())).unwrap_err(),
            ConfigError::NotFound,
            "{exe_dir:?}"
        );
    }
}

/// Finding: portable mode needs a marker *file* (design §8, plan D9); a folder of that name
/// does not enable it.
#[test]
fn a_folder_named_like_the_marker_does_not_enable_portable_mode() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::create_dir(exe_dir.path().join("raumakey.portable")).unwrap();
    let root = DataRoot::resolve(exe_dir.path(), Some(local.path())).unwrap();
    assert!(!root.is_portable());
    assert_eq!(root.path(), local.path().join("Raumakey"));
    assert_eq!(
        DataRoot::resolve(exe_dir.path(), None).unwrap_err(),
        ConfigError::NotFound
    );
}

/// An executable folder that does not exist simply has no marker.
#[test]
fn a_missing_executable_folder_has_no_marker() {
    let base = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let exe_dir = base.path().join("not-there");
    let root = DataRoot::resolve(&exe_dir, Some(local.path())).unwrap();
    assert!(!root.is_portable());
    assert_eq!(root.path(), local.path().join("Raumakey"));
}

#[test]
fn ensure_dirs_creates_the_root_and_logs_idempotently() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let root = DataRoot::resolve(exe_dir.path(), Some(local.path())).unwrap();
    assert!(!root.path().exists());
    root.ensure_dirs().unwrap();
    assert!(root.path().is_dir());
    assert_eq!(root.logs_dir(), root.path().join("logs"));
    assert!(root.logs_dir().is_dir());
    root.ensure_dirs().unwrap();
}

#[test]
fn stores_and_the_log_live_in_the_root() {
    let exe_dir = tempfile::tempdir().unwrap();
    fs::write(exe_dir.path().join("raumakey.portable"), "").unwrap();
    let root = DataRoot::resolve(exe_dir.path(), None).unwrap();
    let config = root.store::<ConfigDocument>(Arc::new(StdReplacer));
    assert_eq!(config.path(), root.path().join("config.toml"));
    let macros = root.store::<MacroDocument>(Arc::new(StdReplacer));
    assert_eq!(macros.path(), root.path().join("macros.toml"));
    assert_eq!(config.commit(0, &ConfigDocument::defaults()).unwrap(), 1);
    let log = root.diagnostics_log(false);
    assert_eq!(log.dir(), root.logs_dir());
    assert!(!log.is_enabled());
}
