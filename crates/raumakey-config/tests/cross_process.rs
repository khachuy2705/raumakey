//! Matrix #46 and the shared diagnostics folder across real OS processes.
//!
//! The parent tests start copies of this test binary that run only `child_worker`, selected by
//! environment variables; without them `child_worker` does nothing.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use raumakey_config::{AtomicStore, ConfigDocument, ConfigError, DiagnosticsLog, StdReplacer};
use raumakey_model::diag::{Component, DiagCode, DiagEvent};

const ROLE: &str = "RAUMAKEY_CONFIG_TEST_ROLE";
const DIR: &str = "RAUMAKEY_CONFIG_TEST_DIR";
const ARG: &str = "RAUMAKEY_CONFIG_TEST_ARG";
const GO: &str = "go";
/// 2026-10-16T00:00:00Z.
const DAY: u64 = 1_792_108_800_000;

fn wait_for(path: &Path) {
    let deadline = Instant::now() + Duration::from_secs(60);
    while !path.exists() {
        assert!(Instant::now() < deadline, "timed out waiting for {path:?}");
        thread::sleep(Duration::from_millis(1));
    }
}

fn event(code: u32) -> DiagEvent {
    DiagEvent {
        timestamp_ms: DAY + 1,
        component: Component::Config,
        code: DiagCode(code),
        revision: None,
        host_exe: None,
        vector_id: None,
    }
}

/// Runs only inside a child process started by the tests below.
#[test]
fn child_worker() {
    let Ok(role) = env::var(ROLE) else {
        return;
    };
    let dir = PathBuf::from(env::var(DIR).unwrap());
    let arg = env::var(ARG).unwrap();
    match role.as_str() {
        "commit" => child_commit(&dir, &arg),
        "log" => child_log(&dir, &arg),
        other => panic!("unknown role {other}"),
    }
}

/// `arg` is `<name> <expected revision>`: commits once the parent says go and writes the
/// outcome to `<name>.result`.
fn child_commit(dir: &Path, arg: &str) {
    let (name, expected) = arg.split_once(' ').unwrap();
    let expected: u64 = expected.parse().unwrap();
    let store: AtomicStore<ConfigDocument> = AtomicStore::new(dir, Arc::new(StdReplacer));
    let mut doc = ConfigDocument::defaults();
    doc.conversion.source = name.to_owned();
    fs::write(dir.join(format!("{name}.ready")), "").unwrap();
    wait_for(&dir.join(GO));
    let outcome = match store.commit(expected, &doc) {
        Ok(revision) => format!("ok {revision}"),
        Err(ConfigError::Conflict { expected, actual }) => format!("conflict {expected} {actual}"),
        Err(other) => format!("error {other:?}"),
    };
    fs::write(dir.join(format!("{name}.result")), outcome).unwrap();
}

/// `arg` is `<writer> <count> <file limit>`: appends `count` events once the parent says go.
fn child_log(dir: &Path, arg: &str) {
    let mut parts = arg.split(' ').map(|part| part.parse::<u64>().unwrap());
    let (writer, count, limit) = (
        parts.next().unwrap(),
        parts.next().unwrap(),
        parts.next().unwrap(),
    );
    let log = DiagnosticsLog::new(dir.join("logs"), true).with_limits(limit, 100_000);
    fs::write(dir.join(format!("{writer}.ready")), "").unwrap();
    wait_for(&dir.join(GO));
    for n in 0..count {
        let code = u32::try_from(writer * 100_000 + n).unwrap();
        log.record_at(&event(code), DAY).unwrap();
    }
}

fn spawn_child(role: &str, dir: &Path, arg: &str) -> Child {
    Command::new(env::current_exe().unwrap())
        .args(["--exact", "child_worker", "--test-threads=1", "--nocapture"])
        .env(ROLE, role)
        .env(DIR, dir)
        .env(ARG, arg)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap()
}

fn finish(child: Child) {
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "child failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Matrix #46 between two OS processes: both commit with the same expected revision; exactly
/// one wins and the other receives `Conflict`. Repeated over many rounds.
#[test]
fn matrix_46_concurrent_commits_from_two_processes_have_one_winner() {
    let dir = tempfile::tempdir().unwrap();
    let names = ["first", "second"];
    for round in 0..16u64 {
        let _ = fs::remove_file(dir.path().join(GO));
        for name in names {
            let _ = fs::remove_file(dir.path().join(format!("{name}.ready")));
            let _ = fs::remove_file(dir.path().join(format!("{name}.result")));
        }
        let children: Vec<Child> = names
            .iter()
            .map(|name| spawn_child("commit", dir.path(), &format!("{name} {round}")))
            .collect();
        for name in names {
            wait_for(&dir.path().join(format!("{name}.ready")));
        }
        fs::write(dir.path().join(GO), "").unwrap();
        for child in children {
            finish(child);
        }
        let results: Vec<String> = names
            .iter()
            .map(|name| fs::read_to_string(dir.path().join(format!("{name}.result"))).unwrap())
            .collect();
        let win = format!("ok {}", round + 1);
        let lose = format!("conflict {round} {}", round + 1);
        let winners: Vec<usize> = (0..2).filter(|&i| results[i] == win).collect();
        assert_eq!(winners.len(), 1, "round {round}: {results:?}");
        let winner = winners[0];
        assert_eq!(results[1 - winner], lose, "round {round}: {results:?}");
        let store: AtomicStore<ConfigDocument> =
            AtomicStore::new(dir.path(), Arc::new(StdReplacer));
        let on_disk = store.load().unwrap().document;
        assert_eq!(on_disk.revision, round + 1);
        assert_eq!(on_disk.conversion.source, names[winner]);
    }
    let leftovers: Vec<String> = fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .filter(|name| name.contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

/// Coverage gap: several processes (TIP instances) appending to one log folder never tear or
/// lose a line, and a file exceeds the limit by at most one line per concurrent writer.
#[test]
fn processes_sharing_the_log_folder_never_tear_or_lose_lines() {
    let dir = tempfile::tempdir().unwrap();
    let writers = 3u64;
    let count = 200u64;
    let line_len = u64::try_from(event(100_000).to_log_line().len()).unwrap() + 1;
    let limit = line_len * 16;
    let children: Vec<Child> = (0..writers)
        .map(|writer| spawn_child("log", dir.path(), &format!("{writer} {count} {limit}")))
        .collect();
    for writer in 0..writers {
        wait_for(&dir.path().join(format!("{writer}.ready")));
    }
    fs::write(dir.path().join(GO), "").unwrap();
    for child in children {
        finish(child);
    }
    let logs = dir.path().join("logs");
    let mut codes = Vec::new();
    for entry in fs::read_dir(&logs).unwrap() {
        let path = entry.unwrap().path();
        let len = fs::metadata(&path).unwrap().len();
        assert!(len <= limit + line_len * (writers - 1), "{path:?}: {len}");
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'), "{path:?}");
        for line in text.lines() {
            let code: u32 = line.split(' ').nth(2).unwrap().parse().unwrap();
            assert_eq!(line, event(code).to_log_line(), "torn line in {path:?}");
            codes.push(code);
        }
    }
    codes.sort_unstable();
    let mut expected: Vec<u32> = (0..writers)
        .flat_map(|writer| (0..count).map(move |n| u32::try_from(writer * 100_000 + n).unwrap()))
        .collect();
    expected.sort_unstable();
    assert_eq!(codes, expected);
}
