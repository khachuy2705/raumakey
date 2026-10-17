//! `file_replace`: the `ReplaceFileW` implementation of `raumakey_config::FileReplacer`.

use std::fs;
use std::io;
use std::sync::Arc;

use raumakey_config::{AtomicStore, ConfigDocument, FileReplacer, StoreDocument};
use raumakey_model::options::InputMethod;
use raumakey_windows::file_replace::ReplaceFileReplacer;

#[test]
fn an_existing_target_is_replaced_and_the_replacement_disappears() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("config.toml");
    let replacement = dir.path().join("config.toml.tmp-1");
    fs::write(&target, b"old").unwrap();
    fs::write(&replacement, b"new").unwrap();
    ReplaceFileReplacer.replace(&replacement, &target).unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert!(!replacement.exists());
    let leftovers: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 1, "no backup or temporary file is left");
}

#[test]
fn a_missing_target_is_created_by_moving_the_replacement() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("macros.toml");
    let replacement = dir.path().join("macros.toml.tmp-2");
    fs::write(&replacement, "ế".as_bytes()).unwrap();
    ReplaceFileReplacer.replace(&replacement, &target).unwrap();
    assert_eq!(fs::read_to_string(&target).unwrap(), "ế");
    assert!(!replacement.exists());
}

#[test]
fn a_missing_replacement_fails_and_keeps_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("app_rules.toml");
    fs::write(&target, b"keep").unwrap();
    let error = ReplaceFileReplacer
        .replace(&dir.path().join("missing.tmp"), &target)
        .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(fs::read(&target).unwrap(), b"keep");
}

#[test]
fn atomic_store_commits_through_replace_file() {
    let dir = tempfile::tempdir().unwrap();
    let store: AtomicStore<ConfigDocument> =
        AtomicStore::new(dir.path(), Arc::new(ReplaceFileReplacer));
    let mut document = ConfigDocument::defaults();
    assert_eq!(store.commit(0, &document).unwrap(), 1);
    document.input.method = InputMethod::Vni;
    assert_eq!(store.commit(1, &document).unwrap(), 2);
    let loaded = store.load().unwrap();
    assert_eq!(loaded.document.revision(), 2);
    assert_eq!(loaded.document.input.method, InputMethod::Vni);
    assert!(
        store.commit(1, &document).is_err(),
        "stale revision conflicts"
    );
    let temps = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.contains(".tmp-"))
        .count();
    assert_eq!(temps, 0);
}
