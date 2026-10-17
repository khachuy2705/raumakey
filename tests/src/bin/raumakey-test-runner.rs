//! Raumakey deterministic acceptance vector runner (plan §12, design §15.3).
//!
//! `vectors [--json <path>]` runs every embedded compatibility vector in-process, prints a
//! one-line summary, and optionally writes the JSON report; `live` is not implemented yet. The
//! command line, report, and exit codes are documented in `raumakey_tests::runner`. Standard
//! error names failing vectors by id only; their details are in the report. A vector whose run
//! panics fails with a `panic` failure, and standard error shows only where the panic happened.

use std::fs;
use std::io::{self, Write as _};
use std::panic;
use std::path::Path;
use std::process::ExitCode;

use raumakey_tests::runner::{
    self, Command, EXIT_FAIL, EXIT_USAGE, RUNNER_NAME, RUNNER_VERSION, USAGE,
};

fn main() -> ExitCode {
    // A vector whose run panics is recorded as a failure and the run goes on; standard error
    // then shows where the panic happened, not its message.
    panic::set_hook(Box::new(|info| {
        let _ = writeln!(io::stderr(), "{}", runner::panic_line(info.location()));
    }));
    let command = match runner::parse_args(std::env::args_os().skip(1)) {
        Ok(command) => command,
        Err(error) => {
            eprint!("{RUNNER_NAME}: {error}\n\n{USAGE}");
            return ExitCode::from(EXIT_USAGE);
        }
    };
    match command {
        Command::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Command::Version => {
            println!("{RUNNER_NAME} {RUNNER_VERSION}");
            ExitCode::SUCCESS
        }
        Command::Live => {
            eprintln!(
                "{RUNNER_NAME}: live is not implemented yet: it needs the installed Raumakey \
                 TIP, which does not exist yet. No result was recorded."
            );
            ExitCode::from(EXIT_USAGE)
        }
        Command::Vectors { json } => vectors(json.as_deref()),
    }
}

/// Runs the embedded vectors, writes the report to `json` when given, and prints the summary.
fn vectors(json: Option<&Path>) -> ExitCode {
    let report = match runner::run_vectors(runner::probe_environment()) {
        Ok(report) => report,
        Err(error) => {
            eprintln!("{RUNNER_NAME}: the embedded fixtures do not load: {error}");
            return ExitCode::from(EXIT_FAIL);
        }
    };
    let mut code = report.exit_code();
    if let Some(path) = json {
        let written = report
            .to_json()
            .map_err(io::Error::from)
            .and_then(|text| fs::write(path, text));
        if let Err(error) = written {
            eprintln!("{RUNNER_NAME}: the report could not be written: {error}");
            code = EXIT_FAIL;
        }
    }
    for id in report.failed_ids() {
        eprintln!("{RUNNER_NAME}: FAIL {id}");
    }
    println!("{}", report.summary_line());
    ExitCode::from(code)
}
