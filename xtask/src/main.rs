//! Raumakey developer tasks: `cargo xtask <command>`.
//!
//! Run `cargo xtask help` for the command list. Passing the release gate means "cross-build
//! verified"; it is never evidence of Windows runtime behavior.

#![forbid(unsafe_code)]

mod cargo;
mod checksum;
mod cli;
mod error;
mod evidence;
mod gate;
mod inspect;
mod notices;
mod package;
mod pe;
mod runner;
mod targets;
mod tasks;

use std::ffi::OsString;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = match std::env::args_os()
        .skip(1)
        .map(OsString::into_string)
        .collect()
    {
        Ok(args) => args,
        Err(arg) => {
            eprintln!(
                "error: argument `{}` is not valid Unicode",
                arg.to_string_lossy()
            );
            return ExitCode::from(2);
        }
    };
    let command = match cli::parse(&args) {
        Ok(command) => command,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };
    match tasks::run(command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
