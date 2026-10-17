//! Process execution from the workspace root, with a record of every command for the release
//! evidence.

use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::error::{Error, Result};

/// Program a command runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Program {
    /// The Cargo that runs xtask (`$CARGO`), displayed as `cargo`.
    Cargo,
    /// An executable name looked up on `PATH`, or a path.
    Path(PathBuf),
}

/// A command line run from the workspace root.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandSpec {
    /// Program to run.
    pub program: Program,
    /// Arguments.
    pub args: Vec<String>,
}

impl CommandSpec {
    /// A `cargo` command.
    pub fn cargo<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            program: Program::Cargo,
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// A command running `program`.
    pub fn new<I, S>(program: impl Into<PathBuf>, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            program: Program::Path(program.into()),
            args: args.into_iter().map(Into::into).collect(),
        }
    }

    /// Shell-like rendering used in logs and evidence; arguments with special characters are
    /// double-quoted.
    pub fn display(&self) -> String {
        let program = match &self.program {
            Program::Cargo => "cargo".to_owned(),
            Program::Path(path) => quote(&path.display().to_string()),
        };
        std::iter::once(program)
            .chain(self.args.iter().map(|arg| quote(arg)))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

fn quote(arg: &str) -> String {
    let plain = !arg.is_empty()
        && arg
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_./\\:=,+@%".contains(c));
    if plain {
        arg.to_owned()
    } else {
        format!("\"{}\"", arg.replace('"', "\\\""))
    }
}

/// Record of one executed command.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommandRecord {
    /// Rendered command line.
    pub command: String,
    /// Process exit code; `None` when it could not be started, timed out, or was killed.
    pub exit_code: Option<i32>,
    /// Whether the process exited successfully.
    pub success: bool,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
}

/// Output of a command run with [`Runner::capture_with_timeout`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    /// Process exit code.
    pub exit_code: Option<i32>,
    /// Whether the process exited successfully.
    pub success: bool,
    /// Standard output (lossy UTF-8).
    pub stdout: String,
    /// Standard error (lossy UTF-8).
    pub stderr: String,
}

/// Runs commands from the workspace root and records each one.
#[derive(Debug)]
pub struct Runner {
    root: PathBuf,
    cargo: OsString,
    records: Vec<CommandRecord>,
}

impl Runner {
    /// A runner working in `root` that executes [`Program::Cargo`] commands with the `cargo`
    /// executable (normally `$CARGO`, the Cargo running xtask).
    pub fn new(root: PathBuf, cargo: OsString) -> Self {
        Self {
            root,
            cargo,
            records: Vec::new(),
        }
    }

    /// Workspace root used as the working directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Every command run so far.
    pub fn records(&self) -> &[CommandRecord] {
        &self.records
    }

    /// Runs `spec` with inherited standard streams; a non-zero exit is an error.
    pub fn run(&mut self, spec: &CommandSpec) -> Result<()> {
        eprintln!("==> {}", spec.display());
        let start = Instant::now();
        let status = self.command(spec).status();
        match status {
            Ok(status) => {
                self.record(spec, start, status.code(), status.success());
                if status.success() {
                    Ok(())
                } else {
                    Err(Error::new(format!(
                        "`{}` failed ({status})",
                        spec.display()
                    )))
                }
            }
            Err(error) => {
                self.record(spec, start, None, false);
                Err(spawn_error(spec, &error))
            }
        }
    }

    /// Runs `spec` with standard output captured; a non-zero exit is an error.
    pub fn capture(&mut self, spec: &CommandSpec) -> Result<String> {
        let start = Instant::now();
        let output = self
            .command(spec)
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .output();
        match output {
            Ok(output) => {
                self.record(spec, start, output.status.code(), output.status.success());
                if output.status.success() {
                    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
                } else {
                    Err(Error::new(format!(
                        "`{}` failed ({})",
                        spec.display(),
                        output.status
                    )))
                }
            }
            Err(error) => {
                self.record(spec, start, None, false);
                Err(spawn_error(spec, &error))
            }
        }
    }

    /// Runs `spec` with both output streams captured and a time limit. The exit status is
    /// returned rather than judged; failing to start or timing out is an error.
    pub fn capture_with_timeout(
        &mut self,
        spec: &CommandSpec,
        timeout: Duration,
    ) -> Result<Captured> {
        eprintln!("==> {}", spec.display());
        let start = Instant::now();
        let spawned = self
            .command(spec)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn();
        let mut child = match spawned {
            Ok(child) => child,
            Err(error) => {
                self.record(spec, start, None, false);
                return Err(spawn_error(spec, &error));
            }
        };
        let stdout = child.stdout.take().map(read_in_background);
        let stderr = child.stderr.take().map(read_in_background);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if start.elapsed() < timeout => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break Err(Error::new(format!(
                        "`{}` timed out after {} s",
                        spec.display(),
                        timeout.as_secs()
                    )));
                }
                Err(error) => {
                    let _ = child.kill();
                    break Err(Error::new(format!(
                        "cannot wait for `{}`: {error}",
                        spec.display()
                    )));
                }
            }
        };
        match status {
            Ok(status) => {
                self.record(spec, start, status.code(), status.success());
                Ok(Captured {
                    exit_code: status.code(),
                    success: status.success(),
                    stdout: join_reader(stdout),
                    stderr: join_reader(stderr),
                })
            }
            Err(error) => {
                // Reader threads are left detached: a surviving grandchild could keep the pipes
                // open indefinitely.
                self.record(spec, start, None, false);
                Err(error)
            }
        }
    }

    /// Whether `spec` can be started (its exit status is ignored).
    pub fn probe(&mut self, spec: &CommandSpec) -> bool {
        let start = Instant::now();
        let output = self
            .command(spec)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        match output {
            Ok(status) => {
                self.record(spec, start, status.code(), status.success());
                true
            }
            Err(_) => {
                self.record(spec, start, None, false);
                false
            }
        }
    }

    fn command(&self, spec: &CommandSpec) -> Command {
        let program = match &spec.program {
            Program::Cargo => self.cargo.as_os_str(),
            Program::Path(path) => path.as_os_str(),
        };
        let mut command = Command::new(program);
        command.args(&spec.args).current_dir(&self.root);
        command
    }

    fn record(
        &mut self,
        spec: &CommandSpec,
        start: Instant,
        exit_code: Option<i32>,
        success: bool,
    ) {
        self.records.push(CommandRecord {
            command: spec.display(),
            exit_code,
            success,
            duration_ms: elapsed_ms(start),
        });
    }
}

/// Milliseconds elapsed since `start`.
pub fn elapsed_ms(start: Instant) -> u64 {
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

fn spawn_error(spec: &CommandSpec, error: &std::io::Error) -> Error {
    let program = match &spec.program {
        Program::Cargo => "cargo".to_owned(),
        Program::Path(path) => path.display().to_string(),
    };
    Error::new(format!(
        "cannot start `{program}` ({}): {error}",
        spec.display()
    ))
}

fn read_in_background(mut pipe: impl Read + Send + 'static) -> JoinHandle<String> {
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        String::from_utf8_lossy(&bytes).into_owned()
    })
}

fn join_reader(reader: Option<JoinHandle<String>>) -> String {
    reader
        .and_then(|handle| handle.join().ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runner() -> Runner {
        Runner::new(
            PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            OsString::from(env!("CARGO")),
        )
    }

    #[test]
    fn display_quotes_only_when_needed() {
        let spec = CommandSpec::cargo([
            "build",
            "--target",
            "x86_64-pc-windows-msvc",
            "--config",
            "target.x86_64-pc-windows-msvc.rustflags=['-Ctarget-feature=+crt-static']",
        ]);
        assert_eq!(
            spec.display(),
            "cargo build --target x86_64-pc-windows-msvc --config \
             \"target.x86_64-pc-windows-msvc.rustflags=['-Ctarget-feature=+crt-static']\""
        );
        let spec = CommandSpec::new(
            r"C:\Program Files\raumakey\raumakey.exe",
            [
                "install",
                "--tip-x64",
                r"C:\pkg\x64\raumakey_tip.dll",
                "say \"hi\"",
                "",
            ],
        );
        assert_eq!(
            spec.display(),
            r#""C:\Program Files\raumakey\raumakey.exe" install --tip-x64 C:\pkg\x64\raumakey_tip.dll "say \"hi\"" """#
        );
    }

    #[test]
    fn run_records_success_and_failure() {
        let mut runner = runner();
        runner
            .run(&CommandSpec::cargo(["--version"]))
            .expect("cargo --version succeeds");
        let error = runner
            .run(&CommandSpec::cargo(["--raumakey-no-such-flag"]))
            .expect_err("unknown flag fails");
        assert!(
            error.to_string().contains("--raumakey-no-such-flag"),
            "{error}"
        );
        let records = runner.records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].command, "cargo --version");
        assert!(records[0].success);
        assert_eq!(records[0].exit_code, Some(0));
        assert!(!records[1].success);
        assert_ne!(records[1].exit_code, Some(0));
    }

    #[test]
    fn capture_returns_stdout() {
        let mut runner = runner();
        let stdout = runner
            .capture(&CommandSpec::cargo(["--version"]))
            .expect("capture");
        assert!(stdout.starts_with("cargo "), "{stdout}");
        assert!(
            runner
                .capture(&CommandSpec::cargo(["--raumakey-no-such-flag"]))
                .is_err()
        );
        assert_eq!(runner.records().len(), 2);
    }

    #[test]
    fn capture_with_timeout_reports_status_and_both_streams() {
        let mut runner = runner();
        let ok = runner
            .capture_with_timeout(&CommandSpec::cargo(["--version"]), Duration::from_secs(60))
            .expect("runs");
        assert!(ok.success);
        assert_eq!(ok.exit_code, Some(0));
        assert!(ok.stdout.starts_with("cargo "), "{ok:?}");
        let failed = runner
            .capture_with_timeout(
                &CommandSpec::cargo(["--raumakey-no-such-flag"]),
                Duration::from_secs(60),
            )
            .expect("runs");
        assert!(!failed.success);
        assert!(
            failed.stderr.contains("raumakey-no-such-flag"),
            "{failed:?}"
        );
        assert_eq!(runner.records().len(), 2);
    }

    #[test]
    fn missing_programs_are_reported_and_recorded() {
        let mut runner = runner();
        let missing = CommandSpec::new("raumakey-no-such-program", ["--version"]);
        let error = runner.run(&missing).expect_err("missing program");
        assert!(
            error.to_string().contains("raumakey-no-such-program"),
            "{error}"
        );
        assert!(runner.capture(&missing).is_err());
        assert!(
            runner
                .capture_with_timeout(&missing, Duration::from_secs(5))
                .is_err()
        );
        assert!(!runner.probe(&missing));
        assert!(runner.probe(&CommandSpec::cargo(["--version"])));
        assert_eq!(runner.records().len(), 5);
        assert!(
            runner.records()[..4]
                .iter()
                .all(|record| record.exit_code.is_none())
        );
    }
}
