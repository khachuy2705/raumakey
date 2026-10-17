//! `raumakey-test-runner` (plan §12, design §15.3): command line, environment probe, and JSON
//! report.
//!
//! ```text
//! raumakey-test-runner vectors [--json <path>]
//! raumakey-test-runner live …            not implemented yet
//! raumakey-test-runner --help | --version
//! ```
//!
//! `vectors` runs every embedded vector ([`EMBEDDED`]) in-process and prints a one-line summary;
//! with `--json` it also writes a [`Report`]. The exit code is [`EXIT_PASS`] when every vector
//! passed, [`EXIT_FAIL`] when one failed or the report could not be written, and
//! [`EXIT_USAGE`] for a usage error and for `live`, which drives a host application through the
//! installed TIP and does not exist yet. A vector whose run panics fails with a `panic` failure
//! and the run goes on, so the report is still written.
//!
//! The report holds vector data and the environment only: the runner's name and version; the OS
//! name and, on Windows, its build (`CurrentBuild`, `UBR`, and `DisplayVersion` of
//! `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion`, read from the 64-bit registry view
//! through `raumakey-windows`); the architectures of the process and of the OS; the
//! configuration revision; the SHA-256 of each fixture file; and one [`VectorResult`] per
//! vector, each carrying the OS and build as well (matrix pass criteria). It never holds user
//! input, the command line, paths, or user and machine names.
//!
//! An in-process run is evidence about the engine only. It does not exercise TSF, the
//! controller, or a Windows application, so it supports no runtime claim (design §19).

use core::fmt;
use std::ffi::OsString;
use std::panic::Location;
use std::path::PathBuf;

use raumakey_config::sha256_hex;
use serde::Serialize;

use crate::vectors::{
    ConfigRevision, EMBEDDED, EmbeddedFixture, LoadError, OsInfo, PROCESS_ARCHITECTURE, Status,
    VectorResult, load_embedded, run_vector,
};

/// Name of the runner binary.
pub const RUNNER_NAME: &str = "raumakey-test-runner";

/// Version of the runner (the workspace version).
pub const RUNNER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Version of the report format.
pub const REPORT_SCHEMA_VERSION: u32 = 1;

/// Exit code: every vector passed.
pub const EXIT_PASS: u8 = 0;
/// Exit code: a vector failed, or the report could not be written.
pub const EXIT_FAIL: u8 = 1;
/// Exit code: usage error, or a mode that is not implemented.
pub const EXIT_USAGE: u8 = 2;

/// Command-line help.
pub const USAGE: &str = "\
Usage:
  raumakey-test-runner vectors [--json <path>]
      Runs every embedded compatibility vector in-process and prints a one-line summary.
      --json <path>  also writes the JSON report to <path>; the folder must exist.
  raumakey-test-runner live ...
      Not implemented yet.
  raumakey-test-runner --help | --version

Exit codes: 0 every vector passed; 1 a vector failed or the report could not be written;
2 usage error, or live.
";

/// The `--json` option.
const JSON_OPTION: &str = "--json";

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// `vectors [--json <path>]`.
    Vectors {
        /// Where to write the JSON report.
        json: Option<PathBuf>,
    },
    /// `live …`: not implemented yet.
    Live,
    /// `help`, `--help`, `-h`.
    Help,
    /// `--version`, `-V`.
    Version,
}

/// A command line the runner does not accept. Variants carry positions, never argument text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UsageError {
    /// No command.
    NoCommand,
    /// The first argument is not a command.
    UnknownCommand,
    /// An option without its value.
    MissingValue {
        /// The option.
        option: &'static str,
    },
    /// An option given twice.
    DuplicateOption {
        /// The option.
        option: &'static str,
    },
    /// An argument the command does not take (1-based position after the program name).
    UnexpectedArgument {
        /// Position of the argument.
        position: usize,
    },
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCommand => f.write_str("no command given"),
            Self::UnknownCommand => {
                f.write_str("the first argument must be vectors, live, help, or --version")
            }
            Self::MissingValue { option } => write!(f, "{option} needs a value"),
            Self::DuplicateOption { option } => write!(f, "{option} is given twice"),
            Self::UnexpectedArgument { position } => {
                write!(f, "argument {position} is not accepted here")
            }
        }
    }
}

impl std::error::Error for UsageError {}

/// Parses the arguments that follow the program name.
pub fn parse_args<I, S>(args: I) -> Result<Command, UsageError>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut args = args.into_iter().map(Into::into);
    let Some(command) = args.next() else {
        return Err(UsageError::NoCommand);
    };
    let command = match command.to_str() {
        Some("vectors") => return parse_vectors(args),
        // `live` is recognized whatever follows, so that it can say it is not implemented.
        Some("live") => return Ok(Command::Live),
        Some("help" | "--help" | "-h") => Command::Help,
        Some("--version" | "-V") => Command::Version,
        _ => return Err(UsageError::UnknownCommand),
    };
    match args.next() {
        Some(_) => Err(UsageError::UnexpectedArgument { position: 2 }),
        None => Ok(command),
    }
}

/// The options of `vectors`; `args` starts at position 2.
fn parse_vectors(mut args: impl Iterator<Item = OsString>) -> Result<Command, UsageError> {
    let mut json = None;
    let mut position = 1;
    while let Some(arg) = args.next() {
        position += 1;
        if arg != JSON_OPTION {
            return Err(UsageError::UnexpectedArgument { position });
        }
        if json.is_some() {
            return Err(UsageError::DuplicateOption {
                option: JSON_OPTION,
            });
        }
        position += 1;
        match args.next() {
            Some(path) if !path.is_empty() => json = Some(PathBuf::from(path)),
            _ => {
                return Err(UsageError::MissingValue {
                    option: JSON_OPTION,
                });
            }
        }
    }
    Ok(Command::Vectors { json })
}

/// Where the vectors ran.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Environment {
    /// Operating system name and build.
    pub os: OsInfo,
    /// Architecture of the runner process: `x86_64`, `x86`, or `aarch64`.
    pub process_architecture: String,
    /// Native architecture of the operating system, when known (Windows only).
    pub os_architecture: Option<String>,
}

/// Probes the environment of this process. A value that cannot be read is left unset.
pub fn probe_environment() -> Environment {
    Environment {
        os: probe_os(),
        process_architecture: PROCESS_ARCHITECTURE.to_owned(),
        os_architecture: probe_os_architecture(),
    }
}

/// The registry key holding the Windows build.
#[cfg(windows)]
const CURRENT_VERSION_KEY: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

#[cfg(windows)]
fn probe_os() -> OsInfo {
    use raumakey_windows::registry::{RegKey, RegRoot, RegView};
    // The 64-bit view: in a 32-bit process on 64-bit Windows the default view of
    // HKLM\SOFTWARE is WOW6432Node.
    let key = RegKey::open_read(
        RegRoot::LocalMachine,
        RegView::Wow64_64,
        CURRENT_VERSION_KEY,
    )
    .ok()
    .flatten();
    let string = |name: &str| {
        key.as_ref()
            .and_then(|key| key.get_string(name).ok().flatten())
    };
    OsInfo {
        name: std::env::consts::OS.to_owned(),
        build: string("CurrentBuild").filter(|build| is_build_number(build)),
        ubr: key
            .as_ref()
            .and_then(|key| key.get_dword("UBR").ok().flatten()),
        display_version: string("DisplayVersion").filter(|version| is_display_version(version)),
    }
}

#[cfg(not(windows))]
fn probe_os() -> OsInfo {
    OsInfo {
        name: std::env::consts::OS.to_owned(),
        ..OsInfo::default()
    }
}

#[cfg(windows)]
fn probe_os_architecture() -> Option<String> {
    use raumakey_windows::process::{Machine, current_machines};
    let (native, _process) = current_machines().ok()?;
    let name = match native {
        Machine::X64 => "x86_64",
        Machine::X86 => "x86",
        Machine::Arm64 => "aarch64",
    };
    Some(name.to_owned())
}

#[cfg(not(windows))]
fn probe_os_architecture() -> Option<String> {
    None
}

/// A Windows `CurrentBuild` value: 1 to 10 ASCII digits.
fn is_build_number(text: &str) -> bool {
    (1..=10).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// A Windows `DisplayVersion` value such as `24H2`: 1 to 16 ASCII letters and digits.
fn is_display_version(text: &str) -> bool {
    (1..=16).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Name and version of the runner.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RunnerInfo {
    /// [`RUNNER_NAME`].
    pub name: String,
    /// [`RUNNER_VERSION`].
    pub version: String,
}

/// A fixture file the vectors came from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct FixtureDigest {
    /// File name.
    pub file: String,
    /// Lowercase hexadecimal SHA-256 of the embedded file.
    pub sha256: String,
}

/// Counts of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Vectors run.
    pub total: usize,
    /// Vectors that passed.
    pub passed: usize,
    /// Vectors that failed.
    pub failed: usize,
}

/// The JSON report of `vectors --json`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Report {
    /// [`REPORT_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Runner name and version.
    pub runner: RunnerInfo,
    /// `vectors`.
    pub mode: String,
    /// Where the vectors ran.
    pub environment: Environment,
    /// The configuration revision every in-process vector starts from.
    pub config_revision: ConfigRevision,
    /// The fixture files.
    pub fixtures: Vec<FixtureDigest>,
    /// Counts.
    pub summary: Summary,
    /// One record per vector, each with the OS and build of `environment`.
    pub results: Vec<VectorResult>,
}

impl Report {
    /// Assembles a report and fills the OS and build into every result.
    pub fn new(
        environment: Environment,
        fixtures: &[EmbeddedFixture],
        mut results: Vec<VectorResult>,
    ) -> Self {
        for result in &mut results {
            result.os = Some(environment.os.clone());
        }
        let passed = results
            .iter()
            .filter(|result| result.status == Status::Pass)
            .count();
        Self {
            schema_version: REPORT_SCHEMA_VERSION,
            runner: RunnerInfo {
                name: RUNNER_NAME.to_owned(),
                version: RUNNER_VERSION.to_owned(),
            },
            mode: "vectors".to_owned(),
            environment,
            config_revision: ConfigRevision::IN_PROCESS,
            fixtures: fixtures
                .iter()
                .map(|fixture| FixtureDigest {
                    file: fixture.name.to_owned(),
                    sha256: sha256_hex(fixture.text.as_bytes()),
                })
                .collect(),
            summary: Summary {
                total: results.len(),
                passed,
                failed: results.len() - passed,
            },
            results,
        }
    }

    /// [`EXIT_PASS`] when at least one vector ran and every vector passed, else
    /// [`EXIT_FAIL`].
    pub fn exit_code(&self) -> u8 {
        if self.summary.total > 0 && self.summary.failed == 0 {
            EXIT_PASS
        } else {
            EXIT_FAIL
        }
    }

    /// The ids of the vectors that failed.
    pub fn failed_ids(&self) -> impl Iterator<Item = &str> {
        self.results
            .iter()
            .filter(|result| result.status != Status::Pass)
            .map(|result| result.id.as_str())
    }

    /// The one-line summary printed to standard output: counts and environment only.
    pub fn summary_line(&self) -> String {
        let Summary {
            total,
            passed,
            failed,
        } = self.summary;
        let environment = &self.environment;
        let mut os = environment.os.name.clone();
        if let Some(build) = &environment.os.build {
            os.push_str(" build ");
            os.push_str(build);
            if let Some(ubr) = environment.os.ubr {
                os.push_str(&format!(".{ubr}"));
            }
        }
        if let Some(version) = &environment.os.display_version {
            os.push(' ');
            os.push_str(version);
        }
        let os_architecture = environment.os_architecture.as_deref().unwrap_or("unknown");
        format!(
            "{RUNNER_NAME} {RUNNER_VERSION} vectors: {passed} passed, {failed} failed of {total} \
             (process {}; OS {os}, {os_architecture})",
            environment.process_architecture
        )
    }

    /// The report as pretty-printed JSON with a final newline.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self).map(|mut json| {
            json.push('\n');
            json
        })
    }
}

/// The line the runner prints on standard error for a panic: where it happened, never its
/// message, which may quote what the panicking code saw. A panic inside a vector run is also
/// recorded as that vector's `panic` failure ([`run_vector`]).
pub fn panic_line(location: Option<&Location<'_>>) -> String {
    match location {
        Some(location) => format!(
            "{RUNNER_NAME}: a panic at {}:{} (message withheld)",
            location.file(),
            location.line()
        ),
        None => format!("{RUNNER_NAME}: a panic (message withheld)"),
    }
}

/// Loads and runs every embedded vector in `environment`.
pub fn run_vectors(environment: Environment) -> Result<Report, LoadError> {
    let results = load_embedded()?.iter().map(run_vector).collect();
    Ok(Report::new(environment, EMBEDDED, results))
}

#[cfg(test)]
mod tests {
    use std::panic::Location;

    use super::{is_build_number, is_display_version, panic_line};

    #[test]
    fn a_panic_is_reported_by_its_location_only() {
        let line = panic_line(Some(Location::caller()));
        assert!(
            line.starts_with("raumakey-test-runner: a panic at "),
            "{line}"
        );
        assert!(line.contains("runner.rs:"), "{line}");
        assert!(line.ends_with("(message withheld)"), "{line}");
        assert_eq!(
            panic_line(None),
            "raumakey-test-runner: a panic (message withheld)"
        );
    }

    #[test]
    fn registry_values_are_kept_only_in_their_documented_form() {
        for build in ["19045", "22631", "26100", "1"] {
            assert!(is_build_number(build), "{build}");
        }
        for build in ["", "26100.1", " 26100", "2610O", "12345678901"] {
            assert!(!is_build_number(build), "{build:?}");
        }
        for version in ["22H2", "24H2", "21H1", "2004"] {
            assert!(is_display_version(version), "{version}");
        }
        for version in ["", "24H2 ", "24-H2", "ＡＢ", "12345678901234567"] {
            assert!(!is_display_version(version), "{version:?}");
        }
    }
}
