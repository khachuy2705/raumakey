//! Where the data root lies (D40): the folder of `raumakey.exe` when it holds `raumakey.toml`
//! already, or when it can be written and is not protected; else `<local_app_data>/Raumakey`.

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raumakey_config::{
    AppRulesDocument, ConfigDocument, ConfigError, DataRoot, MacroDocument, MigrationRecord,
    StdReplacer, is_writable_dir,
};

fn writable(_: &Path) -> bool {
    true
}

fn unwritable(_: &Path) -> bool {
    false
}

fn unprotected(_: &Path) -> bool {
    false
}

fn protected(_: &Path) -> bool {
    true
}

/// Every name in `dir`, sorted.
fn listing(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_writable_executable_folder_is_the_root() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let root = DataRoot::resolve_with(exe_dir.path(), Some(local.path()), &writable, &unprotected)
        .unwrap();
    assert!(root.is_exe_dir());
    assert_eq!(root.path(), exe_dir.path());
    // The local application data folder is not needed then.
    let without_local =
        DataRoot::resolve_with(exe_dir.path(), None, &writable, &unprotected).unwrap();
    assert_eq!(without_local, root);
}

#[test]
fn an_unwritable_executable_folder_falls_back_to_local_app_data() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let root = DataRoot::resolve_with(
        exe_dir.path(),
        Some(local.path()),
        &unwritable,
        &unprotected,
    )
    .unwrap();
    assert!(!root.is_exe_dir());
    assert_eq!(root.path(), local.path().join("Raumakey"));
}

#[test]
fn only_the_executable_folder_is_probed() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let probed: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
    let probe = |dir: &Path| {
        probed.borrow_mut().push(dir.to_path_buf());
        false
    };
    DataRoot::resolve_with(exe_dir.path(), Some(local.path()), &probe, &unprotected).unwrap();
    assert_eq!(probed.into_inner(), vec![exe_dir.path().to_path_buf()]);
}

/// A protected folder (Program Files, the Windows folder, the temp folder: the caller decides)
/// is not used even when it can be written, and it is not probed: elevated and normal runs of
/// a copy there share one file under the local application data folder.
#[test]
fn a_protected_executable_folder_falls_back_without_a_probe() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let probed = RefCell::new(0u32);
    let probe = |_: &Path| {
        *probed.borrow_mut() += 1;
        true
    };
    let asked: RefCell<Vec<PathBuf>> = RefCell::new(Vec::new());
    let is_protected = |dir: &Path| {
        asked.borrow_mut().push(dir.to_path_buf());
        true
    };
    let root =
        DataRoot::resolve_with(exe_dir.path(), Some(local.path()), &probe, &is_protected).unwrap();
    assert!(!root.is_exe_dir());
    assert_eq!(root.path(), local.path().join("Raumakey"));
    assert_eq!(probed.into_inner(), 0);
    assert_eq!(asked.into_inner(), vec![exe_dir.path().to_path_buf()]);
    assert_eq!(
        DataRoot::resolve_with(exe_dir.path(), None, &writable, &protected).unwrap_err(),
        ConfigError::NotFound
    );
}

/// An existing `raumakey.toml` decides first, even in a protected folder.
#[test]
fn an_existing_configuration_in_a_protected_folder_is_used() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::write(exe_dir.path().join("raumakey.toml"), "version = 1\n").unwrap();
    let asked = RefCell::new(0u32);
    let is_protected = |_: &Path| {
        *asked.borrow_mut() += 1;
        true
    };
    let root = DataRoot::resolve_with(
        exe_dir.path(),
        Some(local.path()),
        &unwritable,
        &is_protected,
    )
    .unwrap();
    assert!(root.is_exe_dir());
    assert_eq!(root.path(), exe_dir.path());
    assert_eq!(asked.into_inner(), 0);
}

/// A relative executable folder is not asked about either.
#[test]
fn a_relative_executable_folder_is_not_checked_for_protection() {
    let local = tempfile::tempdir().unwrap();
    let asked = RefCell::new(0u32);
    let is_protected = |_: &Path| {
        *asked.borrow_mut() += 1;
        false
    };
    let root = DataRoot::resolve_with(
        Path::new("bin"),
        Some(local.path()),
        &writable,
        &is_protected,
    )
    .unwrap();
    assert_eq!(root.path(), local.path().join("Raumakey"));
    assert_eq!(asked.into_inner(), 0);
}

/// The marker-based portable mode of plan D9 is gone: neither its marker file nor its `data`
/// folder changes where the root lies.
#[test]
fn the_old_portable_marker_and_data_folder_mean_nothing() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::write(exe_dir.path().join("raumakey.portable"), "").unwrap();
    fs::create_dir(exe_dir.path().join("data")).unwrap();
    let root = DataRoot::resolve_with(exe_dir.path(), Some(local.path()), &writable, &unprotected)
        .unwrap();
    assert_eq!(root.path(), exe_dir.path());
    let root = DataRoot::resolve_with(
        exe_dir.path(),
        Some(local.path()),
        &unwritable,
        &unprotected,
    )
    .unwrap();
    assert_eq!(root.path(), local.path().join("Raumakey"));
}

#[test]
fn an_unwritable_executable_folder_without_usable_local_app_data_is_an_error() {
    let exe_dir = tempfile::tempdir().unwrap();
    for local in [
        None,
        Some(Path::new("")),
        Some(Path::new("relative/appdata")),
    ] {
        assert_eq!(
            DataRoot::resolve_with(exe_dir.path(), local, &unwritable, &unprotected).unwrap_err(),
            ConfigError::NotFound,
            "{local:?}"
        );
    }
}

/// Finding: a relative executable folder (for example `""` when the caller could not find the
/// executable) would resolve the root against the current directory. It is never looked at
/// or probed: the root is the local application data folder, and only without one is there no
/// root.
#[test]
fn a_relative_executable_folder_falls_back_without_a_probe() {
    let local = tempfile::tempdir().unwrap();
    let probed = RefCell::new(0u32);
    let probe = |_: &Path| {
        *probed.borrow_mut() += 1;
        true
    };
    for exe_dir in [
        "",
        ".",
        "relative",
        "relative/bin",
        "C:relative",
        "\\rooted",
    ] {
        let root =
            DataRoot::resolve_with(Path::new(exe_dir), Some(local.path()), &probe, &unprotected)
                .unwrap();
        assert!(!root.is_exe_dir(), "{exe_dir:?}");
        assert_eq!(root.path(), local.path().join("Raumakey"), "{exe_dir:?}");
        assert_eq!(
            DataRoot::resolve_with(Path::new(exe_dir), None, &probe, &unprotected).unwrap_err(),
            ConfigError::NotFound,
            "{exe_dir:?}"
        );
    }
    assert_eq!(probed.into_inner(), 0);
}

/// A `raumakey.toml` beside the executable is used even when the folder cannot be written
/// (saving then fails, and the tray reports it); the folder is not probed then.
#[test]
fn an_existing_configuration_beside_the_executable_is_used_without_a_probe() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::write(exe_dir.path().join("raumakey.toml"), "version = 1\n").unwrap();
    let probed = RefCell::new(0u32);
    let probe = |_: &Path| {
        *probed.borrow_mut() += 1;
        false
    };
    let root =
        DataRoot::resolve_with(exe_dir.path(), Some(local.path()), &probe, &unprotected).unwrap();
    assert!(root.is_exe_dir());
    assert_eq!(root.path(), exe_dir.path());
    let without_local = DataRoot::resolve_with(exe_dir.path(), None, &probe, &unprotected).unwrap();
    assert_eq!(without_local, root);
    assert_eq!(probed.into_inner(), 0);
}

/// Only a file counts as an existing configuration.
#[test]
fn a_folder_named_like_the_configuration_does_not_count() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    fs::create_dir(exe_dir.path().join("raumakey.toml")).unwrap();
    let root = DataRoot::resolve_with(
        exe_dir.path(),
        Some(local.path()),
        &unwritable,
        &unprotected,
    )
    .unwrap();
    assert_eq!(root.path(), local.path().join("Raumakey"));
}

#[test]
fn ensure_dirs_creates_the_root_and_logs_idempotently() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let root = DataRoot::resolve_with(
        exe_dir.path(),
        Some(local.path()),
        &unwritable,
        &unprotected,
    )
    .unwrap();
    assert!(!root.path().exists());
    root.ensure_dirs().unwrap();
    assert!(root.path().is_dir());
    assert_eq!(root.logs_dir(), root.path().join("raumakey-logs"));
    assert!(root.logs_dir().is_dir());
    root.ensure_dirs().unwrap();
}

/// The root may be a folder full of the user's other files, so every name says that it
/// belongs to Raumakey.
#[test]
fn stores_and_the_log_live_in_the_root_under_raumakey_names() {
    let exe_dir = tempfile::tempdir().unwrap();
    let root = DataRoot::resolve_with(exe_dir.path(), None, &writable, &unprotected).unwrap();
    let config = root.store::<ConfigDocument>(Arc::new(StdReplacer));
    assert_eq!(config.path(), exe_dir.path().join("raumakey.toml"));
    assert_eq!(
        config.lock_path(),
        exe_dir.path().join("raumakey.toml.lock")
    );
    let macros = root.store::<MacroDocument>(Arc::new(StdReplacer));
    assert_eq!(macros.path(), exe_dir.path().join("raumakey-macros.toml"));
    let rules = root.store::<AppRulesDocument>(Arc::new(StdReplacer));
    assert_eq!(rules.path(), exe_dir.path().join("raumakey-app-rules.toml"));
    let record = root.store::<MigrationRecord>(Arc::new(StdReplacer));
    assert_eq!(
        record.path(),
        exe_dir.path().join("raumakey-migration.json")
    );
    assert_eq!(config.commit(0, &ConfigDocument::defaults()).unwrap(), 1);
    let log = root.diagnostics_log(false);
    assert_eq!(log.dir(), exe_dir.path().join("raumakey-logs"));
    assert!(!log.is_enabled());
}

/// The real probe creates a file of its own and deletes it again: the folder holds exactly
/// what it held before, and an existing configuration file keeps its bytes.
#[test]
fn the_write_probe_leaves_the_folder_as_it_was() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("raumakey.toml");
    fs::write(&config, "version = 1\nrevision = 7\n").unwrap();
    fs::write(dir.path().join("notes.txt"), "user file").unwrap();
    let before = listing(dir.path());
    for _ in 0..3 {
        assert!(is_writable_dir(dir.path()));
    }
    assert_eq!(listing(dir.path()), before);
    assert_eq!(fs::read(&config).unwrap(), b"version = 1\nrevision = 7\n");
}

/// The probe always uses the same name, so a probe file that an earlier launch could not
/// delete is replaced rather than joined by another one; it is removed first.
#[test]
fn a_stale_probe_file_is_removed_and_none_piles_up() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "user file").unwrap();
    fs::write(
        dir.path().join("raumakey-write-probe.tmp"),
        "left by an earlier launch",
    )
    .unwrap();
    assert!(is_writable_dir(dir.path()));
    assert_eq!(listing(dir.path()), vec!["notes.txt".to_owned()]);
    assert!(is_writable_dir(dir.path()));
    assert_eq!(listing(dir.path()), vec!["notes.txt".to_owned()]);
}

#[test]
fn the_write_probe_refuses_a_missing_folder_and_a_file() {
    let dir = tempfile::tempdir().unwrap();
    assert!(!is_writable_dir(&dir.path().join("not-there")));
    let file = dir.path().join("raumakey.exe");
    fs::write(&file, "").unwrap();
    assert!(!is_writable_dir(&file));
    assert_eq!(listing(dir.path()), vec!["raumakey.exe".to_owned()]);
    assert!(!is_writable_dir(Path::new("")));
}

/// [`DataRoot::resolve`] uses the real probe, and only for a folder that is not protected.
#[test]
fn resolve_probes_the_real_folder() {
    let exe_dir = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let root = DataRoot::resolve(exe_dir.path(), Some(local.path()), &unprotected).unwrap();
    assert!(root.is_exe_dir());
    assert_eq!(root.path(), exe_dir.path());
    assert!(listing(exe_dir.path()).is_empty());
    // A folder that does not exist cannot be written.
    let missing = exe_dir.path().join("not-there");
    let root = DataRoot::resolve(&missing, Some(local.path()), &unprotected).unwrap();
    assert!(!root.is_exe_dir());
    assert_eq!(root.path(), local.path().join("Raumakey"));
    // A protected folder is left alone.
    let root = DataRoot::resolve(exe_dir.path(), Some(local.path()), &protected).unwrap();
    assert!(!root.is_exe_dir());
    assert!(listing(exe_dir.path()).is_empty());
}
