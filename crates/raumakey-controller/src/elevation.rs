//! Administrator rights for the commands that change machine-wide state (design §13).
//!
//! `install` and `uninstall` write `HKEY_LOCAL_MACHINE`; `startup` creates or deletes the
//! elevated logon task. Each checks its token first ([`ensure_elevated`]). A process without an
//! elevated token restarts `raumakey.exe` through the `runas` verb (the UAC prompt) with the
//! command line its caller gives plus [`ELEVATED_OPTION`], waits for it, and exits with its exit
//! code. A process that was started with [`ELEVATED_OPTION`] and is still not elevated (UAC
//! turned off for a standard user, for example) fails instead of asking again, so a restart
//! never repeats itself.
//!
//! When the restart does not finish within its time limit, the elevated process keeps running
//! and may still change the machine, so the outcome is unknown: [`EXIT_INCOMPLETE`], never the
//! "changed nothing" of [`EXIT_FAILED`].
//!
//! The elevated process has no standard handles of its own; its output reaches the caller only
//! when Windows lets it attach to the console of its parent. The process that asked for the
//! restart therefore prints one line before and one line after it, and the exit code always
//! reaches the caller.
//!
//! With over-the-shoulder elevation (a standard user types the credentials of another account),
//! the elevated process runs as that other account. `install` and `uninstall` are
//! machine-wide, so this does not matter for them. `startup` restarts only for the elevated
//! logon task, which belongs to one account: it passes the account that asked, and the
//! restarted process refuses to work for any other (see [`crate::startup`]).

use raumakey_windows::error::{WinErrorKind, WinResult};

use crate::cli::{ELEVATED_OPTION, EXIT_FAILED, EXIT_INCOMPLETE};
use crate::output::Output;

/// What a privileged command that changed nothing yet says when it cannot get administrator
/// rights.
pub const NOTHING_CHANGED: &str = "nothing was changed";

/// The elevation side effects.
pub trait Elevation {
    /// Whether this process runs with an elevated token.
    fn is_elevated(&mut self) -> WinResult<bool>;
    /// Runs `raumakey.exe` again with administrator rights and `args` (the UAC prompt asks the
    /// user), waits for it, and returns its exit code. A declined prompt fails with
    /// [`WinErrorKind::Cancelled`], a restart that did not finish in time with
    /// [`WinErrorKind::TimedOut`] (it keeps running).
    fn run_elevated(&mut self, args: &[String]) -> WinResult<u32>;
}

/// What a privileged command does next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Privilege {
    /// This process is elevated: go on.
    Granted,
    /// Stop and exit with this code: the elevated restart ran (its exit code), administrator
    /// rights could not be obtained ([`EXIT_FAILED`]), or the restart did not finish in time
    /// ([`EXIT_INCOMPLETE`]).
    Done(u8),
}

/// Makes sure the command runs elevated. `args` is the command line of the elevated restart
/// without [`ELEVATED_OPTION`]; `already_restarted` tells whether this process has it.
/// `unchanged` completes the error messages when administrator rights cannot be obtained: what
/// the command left unchanged ([`NOTHING_CHANGED`] when it has not changed anything yet).
pub fn ensure_elevated<E: Elevation + ?Sized>(
    system: &mut E,
    args: &[String],
    already_restarted: bool,
    unchanged: &str,
    output: &mut Output<'_>,
) -> Privilege {
    match system.is_elevated() {
        Ok(true) => Privilege::Granted,
        Ok(false) if already_restarted => {
            output.error(format_args!(
                "administrator rights are required, but this restarted raumakey.exe is still not \
                 elevated; {unchanged}"
            ));
            Privilege::Done(EXIT_FAILED)
        }
        Ok(false) => restart_elevated(system, args, unchanged, output),
        Err(error) => {
            output.error(format_args!(
                "could not check for administrator rights ({error}); {unchanged}"
            ));
            Privilege::Done(EXIT_FAILED)
        }
    }
}

fn restart_elevated<E: Elevation + ?Sized>(
    system: &mut E,
    args: &[String],
    unchanged: &str,
    output: &mut Output<'_>,
) -> Privilege {
    let mut args = args.to_vec();
    args.push(ELEVATED_OPTION.to_owned());
    output.line(
        "Administrator rights are required: starting raumakey.exe as administrator (Windows asks \
         for permission).",
    );
    output.flush();
    match system.run_elevated(&args) {
        Ok(code) => {
            output.line(format_args!(
                "The elevated raumakey.exe finished with exit code {code}."
            ));
            Privilege::Done(exit_code(code))
        }
        Err(error) if error.kind() == WinErrorKind::Cancelled => {
            output.error(format_args!(
                "administrator rights were not granted; {unchanged}"
            ));
            Privilege::Done(EXIT_FAILED)
        }
        Err(error) if error.kind() == WinErrorKind::TimedOut => {
            output.error(format_args!(
                "the elevated raumakey.exe did not finish in time ({error}); it may still be \
                 running, so what it changes is unknown"
            ));
            Privilege::Done(EXIT_INCOMPLETE)
        }
        Err(error) => {
            output.error(format_args!(
                "raumakey.exe could not be started as administrator ({error}); {unchanged}"
            ));
            Privilege::Done(EXIT_FAILED)
        }
    }
}

/// The exit code of the elevated process as the exit code of this one: 0 to 255 pass through;
/// anything else (the NTSTATUS of a crash, for example) becomes [`EXIT_FAILED`].
fn exit_code(code: u32) -> u8 {
    u8::try_from(code).unwrap_or(EXIT_FAILED)
}
