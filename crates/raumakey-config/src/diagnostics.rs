//! Opt-in diagnostics sink (design §14, plan §5).
//!
//! Lines come only from [`DiagEvent::to_log_line`], which has no free-form text, so keystrokes,
//! document text, clipboard data, and macro content cannot reach the log.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use raumakey_model::diag::DiagEvent;

use crate::error::ConfigError;

const FILE_PREFIX: &str = "raumakey-";
const FILE_SUFFIX: &str = ".log";
const MS_PER_DAY: u64 = 86_400_000;

/// Writes diagnostics to `raumakey-<yyyymmdd>.log` files (UTC date) when enabled.
///
/// A file that would grow past the size limit continues in `raumakey-<yyyymmdd>.<n>.log`, and
/// only the newest files are kept. Old files are deleted on a best-effort basis whenever a new
/// file is started; a failure to list or delete them never fails the event that was written.
///
/// The log remembers the file it writes to, so an event costs one open, one size query, and
/// one append; the folder is listed only on a new day, on a rollover, or when the remembered
/// file disappeared. Several logs (one per process) may share a folder: every line is one
/// append, so lines are never torn, and a file can exceed the size limit by at most one line
/// per concurrent writer.
#[derive(Debug)]
pub struct DiagnosticsLog {
    dir: PathBuf,
    enabled: AtomicBool,
    max_file_bytes: u64,
    max_files: usize,
    /// The file events go to; the lock also serializes the writes of this instance.
    cursor: Mutex<Option<Cursor>>,
}

/// The segment that events of one UTC day currently go to.
#[derive(Debug)]
struct Cursor {
    date: String,
    segment: u32,
}

impl DiagnosticsLog {
    /// Size limit of one file.
    pub const MAX_FILE_BYTES: u64 = 1024 * 1024;
    /// Number of files kept.
    pub const MAX_FILES: usize = 5;

    /// A log writing into `dir`, which is created on first write.
    pub fn new(dir: impl Into<PathBuf>, enabled: bool) -> Self {
        Self {
            dir: dir.into(),
            enabled: AtomicBool::new(enabled),
            max_file_bytes: Self::MAX_FILE_BYTES,
            max_files: Self::MAX_FILES,
            cursor: Mutex::new(None),
        }
    }

    /// Replaces the size and retention limits (at least one file is kept).
    #[must_use]
    pub fn with_limits(mut self, max_file_bytes: u64, max_files: usize) -> Self {
        self.max_file_bytes = max_file_bytes;
        self.max_files = max_files.max(1);
        self
    }

    /// The log folder.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Whether events are written.
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    /// Turns logging on or off.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Relaxed);
    }

    /// File name for the UTC day containing `unix_ms`, such as `raumakey-20261016.log`.
    pub fn file_name_for(unix_ms: u64) -> String {
        format!("{FILE_PREFIX}{}{FILE_SUFFIX}", utc_date_stamp(unix_ms))
    }

    /// Appends `event` to the file of the current UTC day; does nothing when disabled.
    pub fn record(&self, event: &DiagEvent) -> Result<(), ConfigError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| {
                u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
            });
        self.record_at(event, now)
    }

    /// Appends `event` to the file of the UTC day containing `now_unix_ms`; does nothing when
    /// disabled.
    ///
    /// Once the line is written the call succeeds, so an error means the line was not written
    /// (or not in full) and a caller may retry without duplicating it.
    pub fn record_at(&self, event: &DiagEvent, now_unix_ms: u64) -> Result<(), ConfigError> {
        if !self.is_enabled() {
            return Ok(());
        }
        let mut line = event.to_log_line();
        line.push('\n');
        let line_len = u64::try_from(line.len()).unwrap_or(u64::MAX);
        let mut cursor = self.cursor.lock().unwrap_or_else(PoisonError::into_inner);
        let date = utc_date_stamp(now_unix_ms);
        // The cursor is restored only after a successful write, so any failure leads to a
        // fresh look at the folder next time.
        let (mut segment, mut scanned) = match cursor.take() {
            Some(current) if current.date == date => (current.segment, false),
            _ => (self.newest_segment(&date)?, true),
        };
        loop {
            let path = self.dir.join(segment_name(&date, segment));
            match OpenOptions::new().append(true).open(&path) {
                Ok(mut file) => {
                    let len = file.metadata()?.len();
                    if len > 0 && len.saturating_add(line_len) > self.max_file_bytes {
                        // Full: continue in the next segment, or in a newer one that another
                        // writer already started.
                        let next = segment.checked_add(1).ok_or(ConfigError::IoFailure)?;
                        segment = next.max(self.newest_segment(&date)?);
                        scanned = true;
                        continue;
                    }
                    file.write_all(line.as_bytes())?;
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    if !scanned {
                        // The remembered file (or the folder) is gone: look again.
                        scanned = true;
                        let newest = self.newest_segment(&date)?;
                        if newest != segment {
                            segment = newest;
                            continue;
                        }
                    }
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&path)?
                        .write_all(line.as_bytes())?;
                    // The line is written: from here on nothing may fail the event.
                    self.prune(&path);
                }
                Err(error) => return Err(error.into()),
            }
            *cursor = Some(Cursor { date, segment });
            return Ok(());
        }
    }

    /// Creates the folder if needed and returns the highest segment present for `date` (0 when
    /// there is none).
    fn newest_segment(&self, date: &str) -> Result<u32, ConfigError> {
        fs::create_dir_all(&self.dir)?;
        self.last_segment(date)
    }

    /// Highest segment number already present for `date` (0 when none).
    fn last_segment(&self, date: &str) -> Result<u32, ConfigError> {
        Ok(self
            .log_files()?
            .into_iter()
            .filter(|file| file.date == date)
            .map(|file| file.segment)
            .max()
            .unwrap_or(0))
    }

    /// Deletes the oldest log files beyond the retention limit, never `active`.
    ///
    /// Best effort: a folder that cannot be listed, or a file that cannot be deleted, is left
    /// for the next time a file is started.
    fn prune(&self, active: &Path) {
        let Ok(mut files) = self.log_files() else {
            return;
        };
        files.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
        let mut excess = files.len().saturating_sub(self.max_files);
        for file in files {
            if excess == 0 {
                break;
            }
            if file.path != active {
                let _ = fs::remove_file(&file.path);
                excess -= 1;
            }
        }
    }

    fn log_files(&self) -> Result<Vec<LogFile>, ConfigError> {
        #[cfg(test)]
        tests::listing_fault()?;
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str()
                && let Some((date, segment)) = parse_log_name(name)
            {
                files.push(LogFile {
                    date: date.to_owned(),
                    segment,
                    path: entry.path(),
                });
            }
        }
        Ok(files)
    }
}

struct LogFile {
    date: String,
    segment: u32,
    path: PathBuf,
}

impl LogFile {
    /// Orders by date (longer stamps are later years), then segment.
    fn sort_key(&self) -> (usize, &str, u32) {
        (self.date.len(), &self.date, self.segment)
    }
}

fn segment_name(date: &str, segment: u32) -> String {
    if segment == 0 {
        format!("{FILE_PREFIX}{date}{FILE_SUFFIX}")
    } else {
        format!("{FILE_PREFIX}{date}.{segment}{FILE_SUFFIX}")
    }
}

/// Splits `raumakey-<date>[.<n>].log` into its date stamp and segment number.
fn parse_log_name(name: &str) -> Option<(&str, u32)> {
    let stem = name.strip_prefix(FILE_PREFIX)?.strip_suffix(FILE_SUFFIX)?;
    let (date, segment) = match stem.split_once('.') {
        Some((date, segment)) => (date, segment),
        None => (stem, "0"),
    };
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
    if date.len() < 8 || !digits(date) || !digits(segment) {
        return None;
    }
    let segment: u32 = segment.parse().ok()?;
    if segment == 0 && stem.contains('.') {
        return None;
    }
    Some((date, segment))
}

/// `yyyymmdd` of the UTC day containing `unix_ms` (proleptic Gregorian calendar).
fn utc_date_stamp(unix_ms: u64) -> String {
    let (year, month, day) = civil_from_days(unix_ms / MS_PER_DAY);
    format!("{year:04}{month:02}{day:02}")
}

/// Converts days since 1970-01-01 to `(year, month, day)`.
///
/// This is the well-known era-based algorithm: days are shifted to start on 0000-03-01 so
/// that leap days fall at the end of each 400-year era. All intermediate values are
/// non-negative and far below `u64::MAX` for any `u64` millisecond input.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    let z = days + 719_468;
    let era = z / 146_097;
    let day_of_era = z % 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;
    use std::fs;

    use raumakey_model::diag::{Component, DiagCode, DiagEvent};

    use super::{DiagnosticsLog, civil_from_days, parse_log_name};
    use crate::error::ConfigError;

    thread_local! {
        /// Folder listings this thread may still make before the next one fails; `None` means
        /// no limit. Lets a test fail a listing that no outside condition fails reliably.
        static LISTINGS_LEFT: Cell<Option<u32>> = const { Cell::new(None) };
    }

    /// Fault point at the start of every folder listing.
    pub(super) fn listing_fault() -> Result<(), ConfigError> {
        LISTINGS_LEFT.with(|left| match left.get() {
            None => Ok(()),
            Some(0) => Err(ConfigError::IoFailure),
            Some(remaining) => {
                left.set(Some(remaining - 1));
                Ok(())
            }
        })
    }

    fn allow_listings(count: Option<u32>) {
        LISTINGS_LEFT.with(|left| left.set(count));
    }

    /// 2026-10-16T00:00:00Z.
    const OCT_16_2026: u64 = 1_792_108_800_000;

    /// Finding: once its line is written, an event never fails. Pruning old files after a new
    /// file was started is best effort, so a failing folder listing at that point neither
    /// fails the event (a caller that retries would write the line twice) nor makes the log
    /// forget the file it writes to.
    #[test]
    fn a_failed_prune_after_the_write_does_not_fail_the_event() {
        let dir = tempfile::tempdir().unwrap();
        let log = DiagnosticsLog::new(dir.path(), true);
        let event = DiagEvent {
            timestamp_ms: OCT_16_2026,
            component: Component::Config,
            code: DiagCode(7),
            revision: None,
            host_exe: None,
            vector_id: None,
        };
        // The listing that finds the newest segment succeeds; the one that prunes fails.
        allow_listings(Some(1));
        let first = log.record_at(&event, OCT_16_2026);
        // The log still knows its file, so the next event needs no listing at all.
        allow_listings(Some(0));
        let second = log.record_at(&event, OCT_16_2026);
        allow_listings(None);
        assert_eq!(first, Ok(()));
        assert_eq!(second, Ok(()));
        let text = fs::read_to_string(dir.path().join("raumakey-20261016.log")).unwrap();
        assert_eq!(text, format!("{0}\n{0}\n", event.to_log_line()));
    }

    /// A listing that fails before anything was written still fails the event.
    #[test]
    fn a_failed_listing_before_the_write_fails_the_event() {
        let dir = tempfile::tempdir().unwrap();
        let log = DiagnosticsLog::new(dir.path(), true);
        let event = DiagEvent {
            timestamp_ms: OCT_16_2026,
            component: Component::Config,
            code: DiagCode(8),
            revision: None,
            host_exe: None,
            vector_id: None,
        };
        allow_listings(Some(0));
        let outcome = log.record_at(&event, OCT_16_2026);
        allow_listings(None);
        assert_eq!(outcome, Err(ConfigError::IoFailure));
        assert!(!dir.path().join("raumakey-20261016.log").exists());
        assert_eq!(log.record_at(&event, OCT_16_2026), Ok(()));
    }

    #[test]
    fn civil_dates_match_known_days() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(59), (1970, 3, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(11_017), (2000, 3, 1));
        assert_eq!(civil_from_days(20_742), (2026, 10, 16));
        assert_eq!(civil_from_days(47_540), (2100, 2, 28));
        assert_eq!(civil_from_days(47_541), (2100, 3, 1));
    }

    #[test]
    fn day_numbers_round_trip_through_a_reference_count() {
        let mut days = 0u64;
        for year in 1970u64..2400 {
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            let lengths = [
                31,
                if leap { 29 } else { 28 },
                31,
                30,
                31,
                30,
                31,
                31,
                30,
                31,
                30,
                31,
            ];
            for (month, length) in (1u64..).zip(lengths) {
                for day in 1..=length {
                    assert_eq!(civil_from_days(days), (year, month, day));
                    days += 1;
                }
            }
        }
    }

    #[test]
    fn log_names_parse_strictly() {
        assert_eq!(
            parse_log_name("raumakey-20261016.log"),
            Some(("20261016", 0))
        );
        assert_eq!(
            parse_log_name("raumakey-20261016.3.log"),
            Some(("20261016", 3))
        );
        for bad in [
            "raumakey-2026101.log",
            "raumakey-20261016.0.log",
            "raumakey-20261016.+1.log",
            "raumakey-20261016..log",
            "raumakey-2026x016.log",
            "raumakey-20261016.log.bak",
            "other-20261016.log",
        ] {
            assert_eq!(parse_log_name(bad), None, "{bad}");
        }
    }
}
