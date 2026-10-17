//! Dispatch of parsed commands, and the commands that only print a message.
//!
//! Every side effect goes through [`System`], so the whole command line is tested with an
//! in-memory fake; `crate::system` holds the real one.
//!
//! - Without arguments, `raumakey.exe` runs the controller on the [`System::desktop`]
//!   ([`crate::run`]): a single instance that types through the keyboard hook, with its tray
//!   icon and the settings of `raumakey.toml`. It prints nothing.
//! - `startup <mode>` ([`crate::startup`]) saves the mode it set in `raumakey.toml`
//!   ([`System::save_startup_mode`]) once it succeeded, so the tray shows it; a mode that
//!   cannot be saved is reported on standard error, and the exit code stays 0.
//! - `--version` prints the product name and version; `help` the usage.
//! - `update` (matrix #57): Raumakey has no updater. The command says so, exits with 0, and
//!   never touches the network (design §13: a future updater must verify an authenticated
//!   manifest, the package hash, and its signature).
//! - `migrate` is not available in this version: importing EVKey data needs a read-only Windows
//!   registry adapter for `raumakey_migrate::LegacyRegistry` and the open decisions of design
//!   §20.8 items 3 and 4. The command line is parsed, then the command says so and exits with
//!   [`EXIT_USAGE`] without reading anything.

use std::io;
use std::path::{Path, PathBuf};

use raumakey_config::{ConfigError, StartupMode};
use raumakey_converter::ClipboardPort;
use raumakey_model::identity::PRODUCT_NAME;
use raumakey_windows::tsf_registration::{TipArch, TipBinary};

use crate::cli::{Command, EXIT_OK, EXIT_USAGE, InstallArgs, USAGE, UsageError};
use crate::convert::convert;
use crate::install::{
    InstallSystem, PRODUCT_VERSION, default_tip_path, install, print_install_plan,
    print_uninstall_plan, uninstall,
};
use crate::output::Output;
use crate::run::{Desktop, run_controller};
use crate::startup::{StartupSystem, startup};

/// What `update` prints (matrix #57).
pub const UPDATE_UNAVAILABLE: &str = "Raumakey does not update itself: automatic updates are not \
available. Raumakey never connects to the network to look for or download updates. To update, \
install a newer Raumakey package after checking its files against the package's SHA256SUMS.";

/// What `migrate` prints.
pub const MIGRATE_UNAVAILABLE: &str = "migrate is not available in this version: importing \
EVKey settings and macros is not implemented yet. Nothing was read or changed.";

/// Every side effect of the commands.
pub trait System: InstallSystem + StartupSystem {
    /// The clipboard, for `convert --clipboard`.
    fn clipboard(&mut self) -> &mut dyn ClipboardPort;

    /// The desktop of the run mode (no arguments): the single-instance mutex, the tray, and
    /// error reports.
    fn desktop(&mut self) -> &mut dyn Desktop;

    /// Saves `[system] startup` in the `raumakey.toml` of this `raumakey.exe`
    /// (`Settings::open_for_current_exe` and `Change::Startup`), after `startup <mode>` set it.
    fn save_startup_mode(&mut self, mode: StartupMode) -> Result<(), ConfigError>;
}

/// Where `raumakey.exe` runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    /// The directory of `raumakey.exe`; in the layout of a TSF build (not shipped) it holds
    /// the TIPs.
    pub exe_dir: PathBuf,
}

/// The product name and version (`--version`).
pub fn version_line() -> String {
    format!("{PRODUCT_NAME} {PRODUCT_VERSION}")
}

/// The TIPs of `install`: the given paths made absolute against the current directory (an
/// elevated restart runs in another directory), or else the TSF build layout next to
/// `raumakey.exe` (`x64\raumakey_tip.dll`, `x86\raumakey_tip.dll`).
pub fn resolve_tips(args: &InstallArgs, exe_dir: &Path) -> io::Result<Vec<TipBinary>> {
    let resolve = |given: &Option<PathBuf>, arch: TipArch| -> io::Result<TipBinary> {
        let path = match given {
            Some(path) => std::path::absolute(path)?,
            None => default_tip_path(exe_dir, arch),
        };
        Ok(TipBinary { path, arch })
    };
    Ok(vec![
        resolve(&args.tip_x64, TipArch::X64)?,
        resolve(&args.tip_x86, TipArch::X86)?,
    ])
}

/// Runs `command` and returns its exit code.
pub fn execute(
    command: &Command,
    context: &Context,
    system: &mut dyn System,
    output: &mut Output<'_>,
) -> u8 {
    match command {
        Command::Run => run_controller(system.desktop()),
        Command::Install(args) => {
            let tips = match resolve_tips(args, &context.exe_dir) {
                Ok(tips) => tips,
                Err(error) => {
                    output.error(format_args!(
                        "install: a TIP path cannot be made absolute ({error})"
                    ));
                    return EXIT_USAGE;
                }
            };
            if args.dry_run {
                print_install_plan(&tips, output)
            } else {
                install(system, &tips, args.elevated, output)
            }
        }
        Command::Uninstall { dry_run, elevated } => {
            if *dry_run {
                print_uninstall_plan(output)
            } else {
                uninstall(system, &context.exe_dir, *elevated, output)
            }
        }
        Command::Startup { mode, for_user } => {
            let code = startup(system, *mode, for_user.as_ref(), output);
            // The process the user started saves the choice; the elevated restart, which may
            // run as another account, does not.
            if code == EXIT_OK
                && for_user.is_none()
                && let Err(error) = system.save_startup_mode(*mode)
            {
                output.error(format_args!(
                    "startup: the choice could not be saved in raumakey.toml ({error}); \
                     Raumakey's menu may show the previous choice"
                ));
            }
            code
        }
        Command::Convert(args) => convert(args, system.clipboard(), output),
        Command::Migrate(_) => {
            output.error(MIGRATE_UNAVAILABLE);
            EXIT_USAGE
        }
        Command::Update => {
            output.line(UPDATE_UNAVAILABLE);
            EXIT_OK
        }
        Command::Version => {
            output.line(version_line());
            EXIT_OK
        }
        Command::Help => {
            output.text(USAGE);
            EXIT_OK
        }
    }
}

/// Prints a usage error and the usage on standard error; returns [`EXIT_USAGE`].
pub fn print_usage_error(error: &UsageError, output: &mut Output<'_>) -> u8 {
    output.error(error);
    output.error_text("\n");
    output.error_text(USAGE);
    EXIT_USAGE
}
