use std::fs;
use std::path::Path;

use raumakey_config::DiagnosticsLog;
use raumakey_model::diag::{Component, DiagCode, DiagEvent, VectorId};
use raumakey_model::host::ExeName;

const DAY_MS: u64 = 86_400_000;
/// 2026-10-16T00:00:00Z.
const OCT_16_2026: u64 = 1_792_108_800_000;

fn event(code: u32) -> DiagEvent {
    DiagEvent {
        timestamp_ms: OCT_16_2026 + 1234,
        component: Component::Config,
        code: DiagCode(code),
        revision: Some(3),
        host_exe: Some(ExeName::new("notepad.exe").unwrap()),
        vector_id: Some(VectorId::new("A1").unwrap()),
    }
}

fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
        .collect();
    names.sort();
    names
}

#[test]
fn file_names_use_the_utc_date() {
    let cases = [
        (0, "raumakey-19700101.log"),
        (DAY_MS - 1, "raumakey-19700101.log"),
        (DAY_MS, "raumakey-19700102.log"),
        (951_782_400_000, "raumakey-20000229.log"),
        (1_700_000_000_000, "raumakey-20231114.log"),
        (1_709_164_799_999, "raumakey-20240228.log"),
        (1_709_164_800_000, "raumakey-20240229.log"),
        (OCT_16_2026, "raumakey-20261016.log"),
        (4_107_542_399_999, "raumakey-21000228.log"),
        (4_107_542_400_000, "raumakey-21000301.log"),
        (253_402_300_799_999, "raumakey-99991231.log"),
    ];
    for (unix_ms, name) in cases {
        assert_eq!(DiagnosticsLog::file_name_for(unix_ms), name, "{unix_ms}");
    }
    // The largest timestamp still yields a name instead of overflowing.
    assert!(DiagnosticsLog::file_name_for(u64::MAX).starts_with("raumakey-"));
}

#[test]
fn the_limits_are_one_mebibyte_and_five_files() {
    assert_eq!(DiagnosticsLog::MAX_FILE_BYTES, 1024 * 1024);
    assert_eq!(DiagnosticsLog::MAX_FILES, 5);
}

#[test]
fn a_disabled_log_writes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    let log = DiagnosticsLog::new(&logs, false);
    assert!(!log.is_enabled());
    log.record_at(&event(1), OCT_16_2026).unwrap();
    log.record(&event(1)).unwrap();
    assert!(!logs.exists());
}

#[test]
fn an_enabled_log_appends_typed_lines_to_the_dated_file() {
    let dir = tempfile::tempdir().unwrap();
    let log = DiagnosticsLog::new(dir.path(), true);
    log.record_at(&event(1), OCT_16_2026).unwrap();
    log.record_at(&event(2), OCT_16_2026 + 5_000).unwrap();
    let text = fs::read_to_string(dir.path().join("raumakey-20261016.log")).unwrap();
    assert_eq!(
        text,
        format!("{}\n{}\n", event(1).to_log_line(), event(2).to_log_line())
    );
    log.record_at(&event(3), OCT_16_2026 + DAY_MS).unwrap();
    assert_eq!(
        names(dir.path()),
        ["raumakey-20261016.log", "raumakey-20261017.log"]
    );
}

#[test]
fn set_enabled_switches_logging_at_runtime() {
    let dir = tempfile::tempdir().unwrap();
    let log = DiagnosticsLog::new(dir.path(), false);
    log.set_enabled(true);
    log.record_at(&event(1), OCT_16_2026).unwrap();
    log.set_enabled(false);
    log.record_at(&event(2), OCT_16_2026).unwrap();
    let text = fs::read_to_string(dir.path().join("raumakey-20261016.log")).unwrap();
    assert_eq!(text.lines().count(), 1);
}

#[test]
fn record_uses_the_current_date() {
    let dir = tempfile::tempdir().unwrap();
    let log = DiagnosticsLog::new(dir.path(), true);
    log.record(&event(1)).unwrap();
    let files = names(dir.path());
    assert_eq!(files.len(), 1);
    assert!(files[0].starts_with("raumakey-20") && files[0].ends_with(".log"));
}

#[test]
fn a_full_file_rolls_over_to_a_numbered_segment() {
    let dir = tempfile::tempdir().unwrap();
    let line_len = event(1).to_log_line().len() as u64 + 1;
    let log = DiagnosticsLog::new(dir.path(), true).with_limits(line_len * 2, 5);
    for code in 0..5 {
        log.record_at(&event(code), OCT_16_2026).unwrap();
    }
    assert_eq!(
        names(dir.path()),
        [
            "raumakey-20261016.1.log",
            "raumakey-20261016.2.log",
            "raumakey-20261016.log",
        ]
    );
    for name in names(dir.path()) {
        let len = fs::metadata(dir.path().join(&name)).unwrap().len();
        assert!(len <= line_len * 2, "{name}: {len}");
    }
    let last = fs::read_to_string(dir.path().join("raumakey-20261016.2.log")).unwrap();
    assert_eq!(last, format!("{}\n", event(4).to_log_line()));
}

#[test]
fn a_file_at_the_real_limit_rolls_over() {
    let dir = tempfile::tempdir().unwrap();
    let full = dir.path().join("raumakey-20261016.log");
    fs::File::create(&full)
        .unwrap()
        .set_len(DiagnosticsLog::MAX_FILE_BYTES - 10)
        .unwrap();
    let log = DiagnosticsLog::new(dir.path(), true);
    log.record_at(&event(1), OCT_16_2026).unwrap();
    assert_eq!(
        fs::metadata(&full).unwrap().len(),
        DiagnosticsLog::MAX_FILE_BYTES - 10
    );
    let next = fs::read_to_string(dir.path().join("raumakey-20261016.1.log")).unwrap();
    assert_eq!(next, format!("{}\n", event(1).to_log_line()));
}

#[test]
fn only_the_newest_files_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "not a log").unwrap();
    fs::write(dir.path().join("raumakey-bogus.log"), "not ours").unwrap();
    let log = DiagnosticsLog::new(dir.path(), true);
    for day in 0..8 {
        log.record_at(&event(1), OCT_16_2026 + day * DAY_MS)
            .unwrap();
    }
    assert_eq!(
        names(dir.path()),
        [
            "notes.txt",
            "raumakey-20261019.log",
            "raumakey-20261020.log",
            "raumakey-20261021.log",
            "raumakey-20261022.log",
            "raumakey-20261023.log",
            "raumakey-bogus.log",
        ]
    );
}

#[test]
fn retention_counts_segments_and_never_deletes_the_active_file() {
    let dir = tempfile::tempdir().unwrap();
    let line_len = event(1).to_log_line().len() as u64 + 1;
    let log = DiagnosticsLog::new(dir.path(), true).with_limits(line_len, 3);
    // Files from a later date (clock moved backwards) must not evict the file in use.
    for day in 1..=3 {
        log.record_at(&event(1), OCT_16_2026 + day * DAY_MS)
            .unwrap();
    }
    for code in 0..4 {
        log.record_at(&event(code), OCT_16_2026).unwrap();
    }
    let files = names(dir.path());
    assert_eq!(files.len(), 3, "{files:?}");
    assert!(
        files.contains(&"raumakey-20261016.3.log".to_owned()),
        "{files:?}"
    );
}

/// The log remembers the file it writes to: it lists the folder again only on a new day, a
/// rollover, or when that file disappeared (finding: no folder listing per event).
#[test]
fn the_current_segment_is_remembered_between_events() {
    let dir = tempfile::tempdir().unwrap();
    let log = DiagnosticsLog::new(dir.path(), true);
    log.record_at(&event(1), OCT_16_2026).unwrap();
    // A higher segment appears behind the log's back (another process, or a stray file).
    fs::write(dir.path().join("raumakey-20261016.5.log"), "").unwrap();
    log.record_at(&event(2), OCT_16_2026).unwrap();
    let first = fs::read_to_string(dir.path().join("raumakey-20261016.log")).unwrap();
    assert_eq!(
        first,
        format!("{}\n{}\n", event(1).to_log_line(), event(2).to_log_line())
    );
    // When its file disappears, the log looks again and continues in the newest segment.
    fs::remove_file(dir.path().join("raumakey-20261016.log")).unwrap();
    log.record_at(&event(3), OCT_16_2026).unwrap();
    let fifth = fs::read_to_string(dir.path().join("raumakey-20261016.5.log")).unwrap();
    assert_eq!(fifth, format!("{}\n", event(3).to_log_line()));
    assert!(!dir.path().join("raumakey-20261016.log").exists());
    // A new day starts a new file.
    log.record_at(&event(4), OCT_16_2026 + DAY_MS).unwrap();
    let next_day = fs::read_to_string(dir.path().join("raumakey-20261017.log")).unwrap();
    assert_eq!(next_day, format!("{}\n", event(4).to_log_line()));
}

/// A deleted log folder is recreated on the next event.
#[test]
fn a_deleted_log_folder_is_recreated() {
    let dir = tempfile::tempdir().unwrap();
    let logs = dir.path().join("logs");
    let log = DiagnosticsLog::new(&logs, true);
    log.record_at(&event(1), OCT_16_2026).unwrap();
    fs::remove_dir_all(&logs).unwrap();
    log.record_at(&event(2), OCT_16_2026).unwrap();
    let text = fs::read_to_string(logs.join("raumakey-20261016.log")).unwrap();
    assert_eq!(text, format!("{}\n", event(2).to_log_line()));
}

/// Coverage gap: several writers (TIP instances in different processes each own a log; here
/// two instances with separate locks) share one folder. No line is torn or lost, and a file
/// exceeds the limit by at most one line per concurrent writer.
#[test]
fn concurrent_writers_never_tear_or_lose_lines() {
    let dir = tempfile::tempdir().unwrap();
    let line_len = event(1).to_log_line().len() as u64 + 1;
    let writers = 2u32;
    let per_writer = 300u32;
    let limit = line_len * 10;
    let handles: Vec<_> = (0..writers)
        .map(|writer| {
            let path = dir.path().to_owned();
            std::thread::spawn(move || {
                let log = DiagnosticsLog::new(path, true).with_limits(limit, 10_000);
                for n in 0..per_writer {
                    log.record_at(&event(writer * 100_000 + n), OCT_16_2026)
                        .unwrap();
                }
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let mut codes = Vec::new();
    for name in names(dir.path()) {
        let path = dir.path().join(&name);
        let len = fs::metadata(&path).unwrap().len();
        assert!(
            len <= limit + line_len * u64::from(writers - 1),
            "{name}: {len}"
        );
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.ends_with('\n'), "{name}");
        for line in text.lines() {
            let code: u32 = line.split(' ').nth(2).unwrap().parse().unwrap();
            assert_eq!(line, event(code).to_log_line(), "torn line in {name}");
            codes.push(code);
        }
    }
    codes.sort_unstable();
    let mut expected: Vec<u32> = (0..writers)
        .flat_map(|writer| (0..per_writer).map(move |n| writer * 100_000 + n))
        .collect();
    expected.sort_unstable();
    assert_eq!(codes, expected);
}
