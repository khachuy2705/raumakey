//! `startup <none|shortcut|elevated-task>` (design §13, matrix #56).
//!
//! - `shortcut` is the normal way to start at logon: a shortcut to `raumakey.exe` in the user's
//!   Startup folder, which needs no administrator rights.
//! - `elevated-task` exists only as this explicit choice: a scheduled task that starts
//!   `raumakey.exe` with the highest run level at the logon of the account that chose it.
//!   Creating it needs administrator rights, and `raumakey.exe` must lie in a folder that only
//!   administrators can change, such as Program Files
//!   ([`StartupSystem::program_is_protected`]): the task runs it with administrator rights at
//!   every logon, so a `raumakey.exe` that the account's programs could replace would hand them
//!   those rights. Elsewhere the command refuses and changes nothing.
//! - Choosing a mode removes the other one; `none` removes both. Every removal is idempotent.
//!
//! Each account has its own task, named after the account's SID hash
//! (`raumakey_windows::task_scheduler::ElevatedLogonTask::for_user`): the command looks up,
//! creates, and deletes only the task of the account it runs for, never another account's.
//!
//! The new start-up entry is created before the old one is removed, so a failure never leaves
//! the user without the start-up they had: when the new entry cannot be created, nothing is
//! changed. When the old entry cannot be removed, the new one stays and the command fails.
//!
//! # Administrator rights
//!
//! Only the task needs them: for `elevated-task`, and for the other modes when the account's
//! task exists and must be deleted, or when only administrators may read it (the lookup is
//! refused). The shortcut lives in the Startup folder of the account, so the process that the
//! user started always handles it. Without administrator rights, that process restarts
//! `raumakey.exe` elevated ([`crate::elevation`]) for the task alone, with `--elevated
//! --for-user <SID hash of this account>`. The restarted process changes only that task, and
//! only when it runs as that account: with over-the-shoulder elevation (a standard user types
//! the credentials of an administrator) it runs as the administrator, and then refuses and
//! changes nothing, so it never sets up or removes another account's start-up.
//!
//! When the task cannot be looked up for another reason (the Task Scheduler is not running,
//! for example), the shortcut part still runs and the command reports the failure; it then
//! does not claim that Raumakey no longer starts at logon. An elevated restart that does not
//! finish in time leaves the task unknown ([`EXIT_INCOMPLETE`](crate::cli::EXIT_INCOMPLETE)):
//! `elevated-task` then keeps the shortcut.
//!
//! Raumakey never writes a `Run` registry value, so it has none to clean up. Once the command
//! succeeded, the dispatch (`crate::commands`) saves the chosen mode in `raumakey.toml`
//! ([`crate::settings::Change::Startup`]), as the tray does for its own choice, so the tray
//! shows it and the run mode repairs the shortcut only for `shortcut` (D41); the elevated
//! restart saves nothing.
//!
//! # Manual checks (Windows, on a test machine)
//!
//! The shortcut and task side effects (`crate::system`) only run on a real desktop. The task
//! needs a `raumakey.exe` inside Program Files (copy the package there first); the ignored test
//! `manual_startup_elevated_task` (`tests/manual.rs`) covers checks 2 and 3.
//!
//! 1. `raumakey.exe startup shortcut` creates `Raumakey.lnk` in `shell:startup`, pointing at
//!    `raumakey.exe` without arguments.
//! 2. `raumakey.exe startup elevated-task` shows one UAC prompt, creates the task `Raumakey
//!    Elevated Startup <SID hash>` (logon trigger of the current account, highest run level)
//!    and removes the shortcut. The same command from the build folder or `Downloads` refuses
//!    without a UAC prompt.
//! 3. `raumakey.exe startup none` removes both; running it again changes nothing and exits
//!    with 0.
//! 4. Two administrator accounts A and B: A runs `startup elevated-task`, then B runs
//!    `startup elevated-task` and later `startup none`. A's task is still there with A as its
//!    principal, and B never saw a UAC prompt for A's task.
//! 5. A standard account runs `startup elevated-task` and types an administrator's
//!    credentials at the prompt: the command fails with "another account", and neither account
//!    has a new task or lost its shortcut.

use raumakey_config::StartupMode;
use raumakey_windows::error::{WinError, WinErrorKind, WinResult};
use raumakey_windows::names::SidHash;

use crate::cli::{EXIT_FAILED, EXIT_OK, FOR_USER_OPTION};
use crate::elevation::{Elevation, NOTHING_CHANGED, Privilege, ensure_elevated};
use crate::output::Output;

/// What the elevated restart that deletes the task leaves unchanged when it cannot run.
const TASK_UNCHANGED: &str = "the elevated startup task was not changed";

/// The side effects of `startup`.
pub trait StartupSystem: Elevation {
    /// The SID hash (`raumakey_windows::names::sid_hash16`) of the account this process runs
    /// as.
    fn current_user(&mut self) -> WinResult<SidHash>;
    /// Creates (or replaces) the shortcut to `raumakey.exe` in the Startup folder of this
    /// process's account.
    fn create_shortcut(&mut self) -> WinResult<()>;
    /// Removes that shortcut; returns whether it existed.
    fn remove_shortcut(&mut self) -> WinResult<bool>;
    /// Whether the elevated logon task of the account `user` exists (read-only).
    fn task_exists(&mut self, user: &SidHash) -> WinResult<bool>;
    /// Creates (or updates) the elevated logon task of `user`, which must be the account of
    /// this process (the task runs as that account). Needs administrator rights.
    fn create_task(&mut self, user: &SidHash) -> WinResult<()>;
    /// Deletes the elevated logon task of `user`; returns whether it existed. Needs
    /// administrator rights.
    fn delete_task(&mut self, user: &SidHash) -> WinResult<bool>;
    /// Whether `raumakey.exe`, which the task starts with administrator rights, lies in a
    /// folder that only administrators can change, such as Program Files.
    fn program_is_protected(&mut self) -> bool;
}

/// `startup <mode>` (see the module documentation). `for_user` is `Some` in the elevated
/// restart (`--elevated --for-user <id>`): the account that asked for it.
pub fn startup<S: StartupSystem + ?Sized>(
    system: &mut S,
    mode: StartupMode,
    for_user: Option<&SidHash>,
    output: &mut Output<'_>,
) -> u8 {
    match for_user {
        Some(user) => task_for(system, mode, user, output),
        None => match mode {
            StartupMode::Shortcut => use_shortcut(system, output),
            StartupMode::ElevatedTask => use_task(system, output),
            StartupMode::None => use_none(system, output),
        },
    }
}

/// What the process that the user started knows about the account's task.
enum Task {
    /// It does not exist.
    Absent,
    /// It exists, or only administrators may read it: deleting it needs administrator rights.
    Present(SidHash),
    /// The account or its task could not be looked up.
    Unknown(WinError),
}

fn look_up_task<S: StartupSystem + ?Sized>(system: &mut S) -> Task {
    let user = match system.current_user() {
        Ok(user) => user,
        Err(error) => return Task::Unknown(error),
    };
    match system.task_exists(&user) {
        Ok(false) => Task::Absent,
        Ok(true) => Task::Present(user),
        // Administrator rights can read (and delete) it: the elevated restart does.
        Err(error) if error.kind() == WinErrorKind::AccessDenied => Task::Present(user),
        Err(error) => Task::Unknown(error),
    }
}

/// The command line of the elevated restart that changes the task of `user` (without
/// `--elevated`, which [`ensure_elevated`] adds).
fn restart_args(mode: StartupMode, user: &SidHash) -> Vec<String> {
    vec![
        "startup".to_owned(),
        mode.as_str().to_owned(),
        FOR_USER_OPTION.to_owned(),
        user.as_str().to_owned(),
    ]
}

fn use_shortcut<S: StartupSystem + ?Sized>(system: &mut S, output: &mut Output<'_>) -> u8 {
    let task = look_up_task(system);
    if let Err(error) = system.create_shortcut() {
        output.error(format_args!(
            "startup: the shortcut in the Startup folder could not be created ({error}); nothing \
             was changed"
        ));
        return EXIT_FAILED;
    }
    output.line("Raumakey starts at logon through a shortcut in your Startup folder.");
    match task {
        Task::Absent => EXIT_OK,
        Task::Present(user) => remove_task(system, StartupMode::Shortcut, &user, output),
        Task::Unknown(error) => {
            report_unknown_task(&error, output);
            EXIT_FAILED
        }
    }
}

fn use_task<S: StartupSystem + ?Sized>(system: &mut S, output: &mut Output<'_>) -> u8 {
    if let Some(code) = refuse_unprotected_program(system, output) {
        return code;
    }
    let user = match system.current_user() {
        Ok(user) => user,
        Err(error) => {
            output.error(format_args!(
                "startup: the account of this process could not be identified ({error}); \
                 nothing was changed"
            ));
            return EXIT_FAILED;
        }
    };
    let args = restart_args(StartupMode::ElevatedTask, &user);
    let created = match ensure_elevated(system, &args, false, NOTHING_CHANGED, output) {
        Privilege::Granted => create_task(system, &user, output),
        // The exit code of the restarted process (0: it created the task), or why it did not
        // run or finish.
        Privilege::Done(code) => code,
    };
    if created != EXIT_OK {
        // The task was not created, or whether it was is unknown: the shortcut stays.
        return created;
    }
    match system.remove_shortcut() {
        Ok(true) => {
            output.line("Removed the shortcut in the Startup folder.");
            EXIT_OK
        }
        Ok(false) => EXIT_OK,
        Err(error) => {
            output.error(format_args!(
                "startup: the shortcut in the Startup folder could not be removed ({error}); it \
                 still starts Raumakey at logon as well"
            ));
            EXIT_FAILED
        }
    }
}

fn use_none<S: StartupSystem + ?Sized>(system: &mut S, output: &mut Output<'_>) -> u8 {
    let task = look_up_task(system);
    let mut code = EXIT_OK;
    match system.remove_shortcut() {
        Ok(true) => output.line("Removed the shortcut in the Startup folder."),
        Ok(false) => {}
        Err(error) => {
            output.error(format_args!(
                "startup: the shortcut in the Startup folder could not be removed ({error}); it \
                 still starts Raumakey at logon"
            ));
            code = EXIT_FAILED;
        }
    }
    let task_code = match task {
        Task::Absent => EXIT_OK,
        Task::Present(user) => remove_task(system, StartupMode::None, &user, output),
        Task::Unknown(error) => {
            report_unknown_task(&error, output);
            EXIT_FAILED
        }
    };
    // The worse of the two: an unknown outcome (3) over a failure (1) over success (0).
    let code = code.max(task_code);
    if code == EXIT_OK {
        output.line("Raumakey does not start at logon.");
    }
    code
}

/// Deletes the task of `user` (it exists, or only administrators may read it) in an elevated
/// process: this one, or an elevated restart that does only that.
fn remove_task<S: StartupSystem + ?Sized>(
    system: &mut S,
    mode: StartupMode,
    user: &SidHash,
    output: &mut Output<'_>,
) -> u8 {
    let args = restart_args(mode, user);
    match ensure_elevated(system, &args, false, TASK_UNCHANGED, output) {
        Privilege::Granted => delete_task(system, user, output),
        Privilege::Done(EXIT_OK) => EXIT_OK,
        Privilege::Done(code) => {
            output.error("startup: the elevated startup task may still start Raumakey at logon");
            code
        }
    }
}

/// The elevated restart (`--elevated --for-user <user>`): changes only the task of `user`, and
/// only when this process runs as that account.
fn task_for<S: StartupSystem + ?Sized>(
    system: &mut S,
    mode: StartupMode,
    user: &SidHash,
    output: &mut Output<'_>,
) -> u8 {
    // Already restarted: a process that is still not elevated fails instead of asking again,
    // so no restart command line is needed.
    if let Privilege::Done(code) = ensure_elevated(system, &[], true, NOTHING_CHANGED, output) {
        return code;
    }
    match system.current_user() {
        Ok(current) if current == *user => {}
        Ok(_) => {
            output.error(
                "startup: administrator rights were given to another account, and the elevated \
                 startup task can only be changed for the account that asked; nothing was \
                 changed. Sign in to an administrator account to use elevated-task, or choose \
                 shortcut",
            );
            return EXIT_FAILED;
        }
        Err(error) => {
            output.error(format_args!(
                "startup: the account of this process could not be identified ({error}); \
                 nothing was changed"
            ));
            return EXIT_FAILED;
        }
    }
    match mode {
        StartupMode::ElevatedTask => match refuse_unprotected_program(system, output) {
            Some(code) => code,
            None => create_task(system, user, output),
        },
        StartupMode::Shortcut | StartupMode::None => delete_task(system, user, output),
    }
}

fn create_task<S: StartupSystem + ?Sized>(
    system: &mut S,
    user: &SidHash,
    output: &mut Output<'_>,
) -> u8 {
    match system.create_task(user) {
        Ok(()) => {
            output.line(
                "Raumakey starts at logon with administrator rights through an elevated \
                 scheduled task.",
            );
            EXIT_OK
        }
        Err(error) => {
            output.error(format_args!(
                "startup: the elevated startup task could not be created ({error}); nothing was \
                 changed"
            ));
            EXIT_FAILED
        }
    }
}

fn delete_task<S: StartupSystem + ?Sized>(
    system: &mut S,
    user: &SidHash,
    output: &mut Output<'_>,
) -> u8 {
    match system.delete_task(user) {
        Ok(true) => {
            output.line("Removed the elevated startup task.");
            EXIT_OK
        }
        Ok(false) => EXIT_OK,
        Err(error) => {
            output.error(format_args!(
                "startup: the elevated startup task could not be deleted ({error}); it still \
                 starts Raumakey at logon"
            ));
            EXIT_FAILED
        }
    }
}

/// Refuses `elevated-task` for a `raumakey.exe` that the account's programs might replace.
fn refuse_unprotected_program<S: StartupSystem + ?Sized>(
    system: &mut S,
    output: &mut Output<'_>,
) -> Option<u8> {
    if system.program_is_protected() {
        return None;
    }
    output.error(
        "startup: elevated-task needs raumakey.exe in a folder that only administrators can \
         change, such as Program Files: the task starts it with administrator rights at every \
         logon, so a raumakey.exe that other programs can replace would hand them those rights. \
         Nothing was changed",
    );
    Some(EXIT_FAILED)
}

fn report_unknown_task(error: &WinError, output: &mut Output<'_>) {
    output.error(format_args!(
        "startup: the elevated startup task could not be looked up ({error}); if it exists, it \
         still starts Raumakey at logon"
    ));
}
