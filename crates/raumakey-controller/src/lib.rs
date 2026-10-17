//! Raumakey controller (`raumakey.exe`, plan §10.2; design §11–§13).
//!
//! `raumakey.exe` is a Windows GUI application. Started without arguments, it runs the
//! controller: a single instance per user and session with one notification-area icon that
//! shows and switches the input mode (V or E) and offers a menu with Exit. With arguments, it
//! runs one command of its command line. The modules:
//!
//! - [`cli`]: the pure parser, usage text, and exit codes;
//! - [`commands`]: dispatch and the commands that only print (`--version`, `help`, `update`,
//!   and `migrate`, which is not available yet);
//! - [`run`]: the run mode (no arguments): single instance, tray, and error reports, over the
//!   [`run::Desktop`] port;
//! - [`tray`]: the tray as a pure state machine (mode, icon, tooltip, menu, and the answer to
//!   each window message, including `TaskbarCreated`, matrix #58);
//! - `desktop` (Windows): the desktop of the run mode and the thin tray glue (hidden window,
//!   notification icon, popup menu, message loop), with its manual check list;
//! - [`install`]: `install` and `uninstall`, with dry runs, verification, the installation
//!   record, and rollback;
//! - [`startup`]: `startup <none|shortcut|elevated-task>`;
//! - [`convert`]: `convert` of files and of the clipboard;
//! - [`elevation`]: the elevated restart of the privileged commands;
//! - [`output`]: where commands print;
//! - [`system`]: the real side effects (Windows), a thin layer over `raumakey-windows`.
//!
//! Every side effect is behind the traits of [`commands::System`] (and the tray's shell
//! behind `desktop::TrayShell`), so the commands and the run mode are tested with in-memory
//! fakes; only the thin glue in [`system`] and `desktop` needs a real machine.
//!
//! Not done yet in the run mode: the panel and its tabs, the IPC server hub and its
//! snapshots (so the TIP does not follow the controller's mode yet), the worker thread, the
//! hook thread (D24 is an open question, design §20.8 item 2), the fallback transport, and the
//! configuration lifecycle (the mode is neither read from nor saved to `config.toml`).
//!
//! [`run_process`] is the body of `main`: it parses the command line, attaches to the console
//! of the caller for a subcommand (a GUI-subsystem process has none of its own), runs the
//! command inside a panic guard ([`run_guarded`]), and returns the exit code. A panic is
//! reported by its source location only; the quiet panic hook that `main` installs first
//! prints nothing. The run mode attaches to no console: it shows its errors in a message box,
//! a panic in its window handler is contained by the window procedure's guard, and any other
//! panic ends it with exit code 1 (its location then goes to a standard error that a started
//! GUI program usually does not have).
//!
//! An interactive `cmd.exe` or PowerShell does not wait for a GUI-subsystem program. Callers
//! that need the exit code wait for it: `start /wait raumakey.exe …`, `Start-Process -Wait
//! -PassThru`, or capturing the output (`$out = & raumakey.exe …`, as the packaged
//! `install.ps1` does). Redirected output is never replaced by the console.
//!
//! Nothing here logs. Commands never print keystrokes, document or clipboard text, or macro
//! text; they print paths the user passed and Raumakey's own registry keys and files.

#![warn(missing_docs)]

pub mod cli;
pub mod commands;
pub mod convert;
#[cfg(windows)]
pub mod desktop;
pub mod elevation;
pub mod install;
pub mod output;
pub mod run;
pub mod startup;
pub mod system;
pub mod tray;

use std::ffi::OsString;
use std::io;
use std::path::Path;

use raumakey_windows::panic::{guard_value, thread_last_panic_location};

use crate::cli::{Command, EXIT_FAILED, UsageError};
use crate::commands::Context;
use crate::output::Output;

/// Runs `raumakey.exe` with `args` (the arguments after the program name) and returns the exit
/// code of the process.
pub fn run_process<I>(args: I) -> u8
where
    I: IntoIterator,
    I::Item: Into<OsString>,
{
    let parsed = cli::parse_args(args);
    if !matches!(parsed, Ok(Command::Run)) {
        system::attach_console();
    }
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let mut output = Output::new(&mut stdout, &mut stderr);
    let code = run_guarded(&mut output, |output| run_parsed(parsed, output));
    output.flush();
    code
}

/// Runs `body` and returns its exit code. A panic in `body` is contained: it becomes
/// [`EXIT_FAILED`] and an error line that names where it happened (as recorded by the quiet
/// panic hook), never the panic message, which may have been formatted from user content.
pub fn run_guarded<'a>(output: &mut Output<'a>, body: impl FnOnce(&mut Output<'a>) -> u8) -> u8 {
    match guard_value(None, || Some(body(output))) {
        Some(code) => code,
        None => {
            match thread_last_panic_location() {
                Some(location) => output.error(format_args!(
                    "internal error at {location}; the command did not finish"
                )),
                None => output.error("internal error; the command did not finish"),
            }
            EXIT_FAILED
        }
    }
}

fn run_parsed(parsed: Result<Command, UsageError>, output: &mut Output<'_>) -> u8 {
    let command = match parsed {
        Ok(command) => command,
        Err(error) => return commands::print_usage_error(&error, output),
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            output.error(format_args!(
                "the location of raumakey.exe cannot be found ({error})"
            ));
            return EXIT_FAILED;
        }
    };
    let context = Context {
        exe_dir: exe.parent().map(Path::to_path_buf).unwrap_or_default(),
    };
    let mut system = system::platform(exe);
    commands::execute(&command, &context, &mut system, output)
}
