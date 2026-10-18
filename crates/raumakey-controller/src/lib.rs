//! Raumakey controller (`raumakey.exe`, plan §10.2 and §16; design §21).
//!
//! Raumakey ships as one portable `raumakey.exe` (D38), a Windows GUI application. Started
//! without arguments, it runs the controller: a single instance per user and session that
//! types Vietnamese through a global keyboard hook (D39), keeps its configuration in
//! `raumakey.toml` next to the exe (D40), and shows one notification-area icon that switches
//! the input mode (V or E) and whose menu holds the settings (D42) and the start with Windows
//! (D41). With arguments, it runs one command of its command line. The modules:
//!
//! - [`cli`]: the pure parser, usage text, and exit codes;
//! - [`commands`]: dispatch and the commands that only print (`--version`, `help`, `update`,
//!   and `migrate`, which is not available yet);
//! - [`run`]: the run mode (no arguments): single instance, settings, typing hook, the
//!   Startup-shortcut repair, tray, and error reports, over the [`run::Desktop`] port;
//! - [`tray`]: the tray as a pure state machine (settings shown, icon, tooltip, menu, and the
//!   answer to each window message, menu choice, hotkey change, and worker answer, including
//!   `TaskbarCreated`, matrix #58);
//! - [`worker`]: the settings worker thread, which owns [`settings::Settings`] and does the
//!   tray's file work (commits, polls, recovery, the shortcut, Notepad);
//! - [`notice`]: what the run mode tells the user, in Vietnamese message boxes;
//! - [`typing`]: typing through the global keyboard hook and `SendInput` (plan D39): the pure
//!   handler over a platform port, and on Windows `typing::TypingHook`, which the run mode
//!   starts, with its manual check list;
//! - `desktop` (Windows): the desktop of the run mode and the thin glue (hidden window,
//!   notification icon, popup menu, message boxes, poll timer, message loop, hook observer,
//!   worker thread), with the manual check list of the portable run;
//! - [`install`]: `install` and `uninstall` of the TSF text service, which stays in the
//!   repository but is not shipped (design §21.1), with dry runs, verification, the
//!   installation record, and rollback;
//! - [`settings`]: the configuration lifecycle of the portable controller (D40): where
//!   `raumakey.toml` lives, the first run, changes from the tray, hand edits, and the
//!   snapshots for the session core;
//! - [`macro_list`]: the model of the macro list editor (D44): the rows of
//!   `raumakey-macros.toml`, their checks (D29, D30), and the save that keeps what the editor
//!   does not show, with its Vietnamese messages;
//! - [`macro_dialog`]: the macro list editor itself: its template, cells, and the decisions
//!   of its buttons (pure), and on Windows the dialog on a thread of its own
//!   (`spawn_macro_list`), with its manual check list;
//! - [`startup`]: `startup <none|shortcut|elevated-task>`;
//! - [`convert`]: `convert` of files and of the clipboard;
//! - [`elevation`]: the elevated restart of the privileged commands;
//! - [`output`]: where commands print;
//! - [`system`]: the real side effects (Windows), a thin layer over `raumakey-windows`.
//!
//! Every side effect is behind the traits of [`commands::System`] (and the tray's shell and
//! services behind `desktop::TrayShell` and `desktop::TrayServices`, the worker's behind
//! [`worker::WorkerPorts`]), so the commands and the run mode are tested with in-memory fakes;
//! only the thin glue in [`system`] and `desktop` needs a real machine.
//!
//! Not in the portable release (design §21.6): the settings panel and its tabs, the IPC
//! server hub (there is no TIP to serve), and `migrate`.
//!
//! [`run_process`] is the body of `main`: it parses the command line, attaches to the console
//! of the caller for a subcommand (a GUI-subsystem process has none of its own), runs the
//! command inside a panic guard ([`run_guarded`]), and returns the exit code. A panic is
//! reported by its source location only; the quiet panic hook that `main` installs first
//! prints nothing. The run mode attaches to no console: it shows its errors in a message box,
//! a panic in its window handler is contained by the window procedure's guard, one in the
//! hook callback by the hook handler's guard, one in a settings command by the worker, and any
//! other panic ends it with exit code 1 (its location then goes to a standard error that a
//! started GUI program usually does not have).
//!
//! An interactive `cmd.exe` or PowerShell does not wait for a GUI-subsystem program, and the
//! call operator of Windows PowerShell 5.1 sets no exit code for it, even when the output is
//! captured. Callers that need the exit code wait for it: `start /wait raumakey.exe …`,
//! `Start-Process -Wait -PassThru`, or a process started with redirected output and waited
//! for (`System.Diagnostics.Process` and `WaitForExit`; the release gate runs
//! `raumakey.exe --version` that way). Redirected output is never replaced by the console.
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
pub mod macro_dialog;
pub mod macro_list;
pub mod notice;
pub mod output;
pub mod run;
pub mod settings;
pub mod startup;
pub mod system;
pub mod tray;
pub mod typing;
pub mod worker;

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
