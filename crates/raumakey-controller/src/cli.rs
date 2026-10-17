//! Command line of `raumakey.exe` (plan §10.2).
//!
//! ```text
//! raumakey                                   run Raumakey (keyboard hook and tray icon)
//! raumakey install [--dry-run] [--tip-x64 <path>] [--tip-x86 <path>]   TSF text service
//! raumakey uninstall [--dry-run]                                         TSF text service
//! raumakey startup <none|shortcut|elevated-task>
//! raumakey convert --from <charset> --to <charset>
//!                  (--in <file> --out <file> [--overwrite] | --clipboard)
//! raumakey migrate [--from <folder>] [--dry-run]   not available yet
//! raumakey update
//! raumakey --version | help
//! ```
//!
//! The portable release (D38) is `raumakey.exe` alone: started without arguments it types
//! through a keyboard hook (`crate::run`). `install` and `uninstall` register and remove the
//! TSF text service (TIP), which stays in the repository but is not shipped; the commands are
//! kept for it (design §21.1).
//!
//! [`parse_args`] is pure. The command comes first; its options follow in any order (so
//! `install --dry-run --tip-x64 <path>` and `install --tip-x64 <path> --dry-run` are the
//! same). An option may be given once. Its value is the next argument, which must be
//! non-empty and must not start with `--` (so `--tip-x64 --dry-run` is a missing value, not a
//! path). Charset and startup-mode names ignore ASCII case.
//!
//! `install`, `uninstall`, and `startup` also accept `--elevated`. Raumakey adds it when it
//! restarts itself with administrator rights; a process that has it and is still not elevated
//! fails instead of asking again, so a restart can never repeat itself. `startup` restarts only
//! to change the elevated logon task of the account that asked, so it adds `--for-user <id>`,
//! that account's SID hash, as well: the two come together or not at all.
//!
//! A [`UsageError`] names options and positions only, never the text of an argument.

use core::fmt;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

use raumakey_config::StartupMode;
use raumakey_converter::Charset;
use raumakey_windows::names::SidHash;

/// Exit code: the command succeeded.
pub const EXIT_OK: u8 = 0;
/// Exit code: the command failed; it changed nothing, or it undid its changes.
pub const EXIT_FAILED: u8 = 1;
/// Exit code: usage error, or a command that is not available in this build (`migrate`).
pub const EXIT_USAGE: u8 = 2;
/// Exit code: the command failed and could not undo all of its changes, or its outcome is
/// unknown (an elevated restart that did not finish in time and may still change the machine).
pub const EXIT_INCOMPLETE: u8 = 3;

/// Command-line help.
pub const USAGE: &str = "\
Usage:
  raumakey
      Runs Raumakey: it types Vietnamese in every application through a keyboard hook,
      with a notification-area icon that shows V (Vietnamese) or E (English); click it or
      press Ctrl+Shift to switch. The settings are in the tray menu (right-click the icon)
      and in raumakey.toml, kept next to raumakey.exe (or in %LOCALAPPDATA%\\Raumakey when
      that folder cannot be used); a hand edit is picked up within a few seconds. A second
      start exits at once while it runs.
  raumakey install [--dry-run] [--tip-x64 <path>] [--tip-x86 <path>]
      For the TSF text service (TIP), which is not shipped with the portable raumakey.exe:
      registers the Raumakey text input processor for 64-bit and 32-bit applications and
      records the installation. Asks for administrator rights.
      --dry-run         prints the registration plan and changes nothing.
      --tip-x64 <path>  the x64 TIP (default: x64\\raumakey_tip.dll next to raumakey.exe).
      --tip-x86 <path>  the x86 TIP (default: x86\\raumakey_tip.dll next to raumakey.exe).
  raumakey uninstall [--dry-run]
      For the TSF text service (TIP), which is not shipped with the portable raumakey.exe:
      removes the Raumakey registration entries and the installation record. Asks for
      administrator rights. --dry-run prints the removal plan and changes nothing.
  raumakey startup <none|shortcut|elevated-task>
      How Raumakey starts at logon: none (it does not), shortcut (a shortcut in your
      Startup folder), or elevated-task (a scheduled task that starts it with administrator
      rights at your logon; asks for administrator rights, and raumakey.exe must be in a
      folder that only administrators can change, such as Program Files). The choice is
      also saved in raumakey.toml, where the tray menu shows it.
  raumakey convert --from <charset> --to <charset> (--in <file> --out <file> [--overwrite] | --clipboard)
      Converts Vietnamese text from one charset to another: a file into a new file
      (--overwrite replaces an existing one), or the text on the clipboard.
      Charsets: unicode, unicode-nfd, viqr, tcvn3, vni-windows, viscii, cp1258,
      ncr-decimal, ncr-hex, utf8-literal.
  raumakey migrate [--from <folder>] [--dry-run]
      Imports EVKey settings and macros. Not available in this version yet.
  raumakey update
      Reports that automatic updates are not available.
  raumakey --version | help

Options may be given in any order. When install, uninstall, or startup restarts
raumakey.exe with administrator rights, it adds --elevated; startup also adds
--for-user <id> (the account that asked), and the restarted process changes only the
elevated startup task of that account.

Exit codes: 0 success; 1 the command failed; 2 usage error, or a command that is not
available yet; 3 the command failed and could not undo all of its changes, or its
outcome is unknown (the elevated raumakey.exe did not finish in time).
";

/// `--dry-run`.
const DRY_RUN: &str = "--dry-run";
/// `--elevated`: added by a privileged command when it restarts `raumakey.exe` with
/// administrator rights (see [`crate::elevation`]).
pub const ELEVATED_OPTION: &str = "--elevated";
/// Short name used inside this module.
const ELEVATED: &str = ELEVATED_OPTION;
/// `--for-user <id>`: added with [`ELEVATED_OPTION`] when `startup` restarts `raumakey.exe`
/// with administrator rights: the SID hash of the account that asked (see
/// [`crate::startup`]).
pub const FOR_USER_OPTION: &str = "--for-user";
/// `--tip-x64`.
const TIP_X64: &str = "--tip-x64";
/// `--tip-x86`.
const TIP_X86: &str = "--tip-x86";
/// `--from`.
const FROM: &str = "--from";
/// `--to`.
const TO: &str = "--to";
/// `--in`.
const IN: &str = "--in";
/// `--out`.
const OUT: &str = "--out";
/// `--overwrite`.
const OVERWRITE: &str = "--overwrite";
/// `--clipboard`.
const CLIPBOARD: &str = "--clipboard";
/// Name of the `startup` command, used for its mode in [`UsageError::InvalidValue`].
const STARTUP: &str = "startup";

/// `install` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallArgs {
    /// `--dry-run`: print the registration plan and change nothing.
    pub dry_run: bool,
    /// `--tip-x64`: the x64 TIP (default: `x64\raumakey_tip.dll` next to `raumakey.exe`).
    pub tip_x64: Option<PathBuf>,
    /// `--tip-x86`: the x86 TIP (default: `x86\raumakey_tip.dll` next to `raumakey.exe`).
    pub tip_x86: Option<PathBuf>,
    /// `--elevated`: this process is the elevated restart.
    pub elevated: bool,
}

/// What `convert` converts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConvertTarget {
    /// `--in <file> --out <file> [--overwrite]`: a file into another file.
    Files {
        /// Source file.
        input: PathBuf,
        /// Destination file.
        output: PathBuf,
        /// Replace an existing destination.
        overwrite: bool,
    },
    /// `--clipboard`: the clipboard text, in place.
    Clipboard,
}

/// `convert` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConvertArgs {
    /// `--from`: the charset of the source text.
    pub from: Charset,
    /// `--to`: the charset of the result.
    pub to: Charset,
    /// What to convert.
    pub target: ConvertTarget,
}

/// `migrate` options.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrateArgs {
    /// `--from`: the EVKey folder.
    pub from: Option<PathBuf>,
    /// `--dry-run`.
    pub dry_run: bool,
}

/// A parsed command line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// No arguments: run the controller.
    Run,
    /// `install [--dry-run] [--tip-x64 <path>] [--tip-x86 <path>]`.
    Install(InstallArgs),
    /// `uninstall [--dry-run]`.
    Uninstall {
        /// `--dry-run`: print the removal plan and change nothing.
        dry_run: bool,
        /// `--elevated`: this process is the elevated restart.
        elevated: bool,
    },
    /// `startup <none|shortcut|elevated-task>`.
    Startup {
        /// How Raumakey starts at logon.
        mode: StartupMode,
        /// `--elevated --for-user <id>`: this process is the elevated restart that the account
        /// with this SID hash asked for; it changes only that account's elevated logon task.
        for_user: Option<SidHash>,
    },
    /// `convert --from <charset> --to <charset> (--in <file> --out <file> [--overwrite] |
    /// --clipboard)`.
    Convert(ConvertArgs),
    /// `migrate [--from <folder>] [--dry-run]` (not available yet).
    Migrate(MigrateArgs),
    /// `update`.
    Update,
    /// `--version`, `-V`.
    Version,
    /// `help`, `--help`, `-h`.
    Help,
}

/// A command line `raumakey.exe` does not accept. Variants carry option names and positions,
/// never argument text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UsageError {
    /// The first argument is not a command.
    UnknownCommand,
    /// An argument the command does not take (1-based position after the program name).
    UnexpectedArgument {
        /// Position of the argument.
        position: usize,
    },
    /// An option given twice.
    DuplicateOption {
        /// The option.
        option: &'static str,
    },
    /// An option without its value.
    MissingValue {
        /// The option.
        option: &'static str,
    },
    /// A required option or argument is absent.
    Missing {
        /// What is missing.
        what: &'static str,
    },
    /// A value that is not accepted: an unknown charset (`--from`, `--to`) or startup mode
    /// (`startup`).
    InvalidValue {
        /// The option or command whose value it is.
        option: &'static str,
    },
    /// Two options that cannot be combined.
    Conflict {
        /// The first option.
        first: &'static str,
        /// The second option.
        second: &'static str,
    },
}

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownCommand => f.write_str(
                "the first argument must be a command: install, uninstall, startup, convert, \
                 migrate, update, help, or --version",
            ),
            Self::UnexpectedArgument { position } => {
                write!(f, "argument {position} is not accepted here")
            }
            Self::DuplicateOption { option } => write!(f, "{option} is given more than once"),
            Self::MissingValue { option } => write!(f, "{option} needs a value"),
            Self::Missing { what } => write!(f, "missing {what}"),
            Self::InvalidValue { option } if *option == STARTUP => {
                f.write_str("the startup mode must be none, shortcut, or elevated-task")
            }
            Self::InvalidValue { option } if *option == FROM || *option == TO => {
                let names: Vec<&str> = Charset::ALL.iter().map(|charset| charset.name()).collect();
                match names.split_last() {
                    Some((last, others)) => write!(
                        f,
                        "{option} must name a charset: {}, or {last}",
                        others.join(", ")
                    ),
                    None => write!(f, "{option} must name a charset"),
                }
            }
            Self::InvalidValue { option } => write!(f, "the value of {option} is not accepted"),
            Self::Conflict { first, second } => {
                write!(f, "{first} cannot be combined with {second}")
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
    let args: Vec<OsString> = args.into_iter().map(Into::into).collect();
    let Some((command, rest)) = args.split_first() else {
        return Ok(Command::Run);
    };
    let mut cursor = Cursor {
        args: rest,
        next: 0,
    };
    match command.to_str() {
        Some("install") => parse_install(&mut cursor),
        Some("uninstall") => parse_uninstall(&mut cursor),
        Some("startup") => parse_startup(&mut cursor),
        Some("convert") => parse_convert(&mut cursor),
        Some("migrate") => parse_migrate(&mut cursor),
        Some("update") => no_arguments(&mut cursor, Command::Update),
        Some("--version" | "-V") => no_arguments(&mut cursor, Command::Version),
        Some("help" | "--help" | "-h") => no_arguments(&mut cursor, Command::Help),
        _ => Err(UsageError::UnknownCommand),
    }
}

/// The arguments after the command.
struct Cursor<'a> {
    args: &'a [OsString],
    next: usize,
}

impl<'a> Cursor<'a> {
    /// The next argument and its 1-based position after the program name (the command is 1).
    fn next(&mut self) -> Option<(usize, &'a OsStr)> {
        let arg = self.args.get(self.next)?;
        self.next += 1;
        Some((self.next + 1, arg.as_os_str()))
    }

    /// The value of `option`: the next argument, non-empty and not starting with `--`.
    fn value(&mut self, option: &'static str) -> Result<&'a OsStr, UsageError> {
        match self.args.get(self.next) {
            Some(value) if !value.is_empty() && !starts_with(value, "--") => {
                self.next += 1;
                Ok(value.as_os_str())
            }
            _ => Err(UsageError::MissingValue { option }),
        }
    }
}

fn starts_with(arg: &OsStr, prefix: &str) -> bool {
    arg.as_encoded_bytes().starts_with(prefix.as_bytes())
}

/// Sets a flag that may be given once.
fn set_flag(flag: &mut bool, option: &'static str) -> Result<(), UsageError> {
    if *flag {
        return Err(UsageError::DuplicateOption { option });
    }
    *flag = true;
    Ok(())
}

/// Reads the value of an option that may be given once.
fn set_value<T>(
    slot: &mut Option<T>,
    cursor: &mut Cursor<'_>,
    option: &'static str,
    convert: impl FnOnce(&OsStr) -> Result<T, UsageError>,
) -> Result<(), UsageError> {
    if slot.is_some() {
        return Err(UsageError::DuplicateOption { option });
    }
    *slot = Some(convert(cursor.value(option)?)?);
    Ok(())
}

fn path(value: &OsStr) -> Result<PathBuf, UsageError> {
    Ok(PathBuf::from(value))
}

fn charset(option: &'static str) -> impl FnOnce(&OsStr) -> Result<Charset, UsageError> {
    move |value| {
        value
            .to_str()
            .and_then(|name| name.parse().ok())
            .ok_or(UsageError::InvalidValue { option })
    }
}

fn sid_hash(value: &OsStr) -> Result<SidHash, UsageError> {
    value
        .to_str()
        .and_then(|text| SidHash::parse(text).ok())
        .ok_or(UsageError::InvalidValue {
            option: FOR_USER_OPTION,
        })
}

fn startup_mode(value: &OsStr) -> Result<StartupMode, UsageError> {
    let modes = [
        StartupMode::None,
        StartupMode::Shortcut,
        StartupMode::ElevatedTask,
    ];
    value
        .to_str()
        .and_then(|name| {
            modes
                .into_iter()
                .find(|mode| mode.as_str().eq_ignore_ascii_case(name))
        })
        .ok_or(UsageError::InvalidValue { option: STARTUP })
}

fn no_arguments(cursor: &mut Cursor<'_>, command: Command) -> Result<Command, UsageError> {
    match cursor.next() {
        Some((position, _)) => Err(UsageError::UnexpectedArgument { position }),
        None => Ok(command),
    }
}

fn parse_install(cursor: &mut Cursor<'_>) -> Result<Command, UsageError> {
    let mut args = InstallArgs {
        dry_run: false,
        tip_x64: None,
        tip_x86: None,
        elevated: false,
    };
    while let Some((position, arg)) = cursor.next() {
        match arg.to_str() {
            Some(DRY_RUN) => set_flag(&mut args.dry_run, DRY_RUN)?,
            Some(ELEVATED) => set_flag(&mut args.elevated, ELEVATED)?,
            Some(TIP_X64) => set_value(&mut args.tip_x64, cursor, TIP_X64, path)?,
            Some(TIP_X86) => set_value(&mut args.tip_x86, cursor, TIP_X86, path)?,
            _ => return Err(UsageError::UnexpectedArgument { position }),
        }
    }
    Ok(Command::Install(args))
}

fn parse_uninstall(cursor: &mut Cursor<'_>) -> Result<Command, UsageError> {
    let mut dry_run = false;
    let mut elevated = false;
    while let Some((position, arg)) = cursor.next() {
        match arg.to_str() {
            Some(DRY_RUN) => set_flag(&mut dry_run, DRY_RUN)?,
            Some(ELEVATED) => set_flag(&mut elevated, ELEVATED)?,
            _ => return Err(UsageError::UnexpectedArgument { position }),
        }
    }
    Ok(Command::Uninstall { dry_run, elevated })
}

fn parse_startup(cursor: &mut Cursor<'_>) -> Result<Command, UsageError> {
    let mut mode = None;
    let mut elevated = false;
    let mut for_user = None;
    while let Some((position, arg)) = cursor.next() {
        if arg == ELEVATED {
            set_flag(&mut elevated, ELEVATED)?;
            continue;
        }
        if arg == FOR_USER_OPTION {
            set_value(&mut for_user, cursor, FOR_USER_OPTION, sid_hash)?;
            continue;
        }
        // An option-like argument is not a misspelled mode.
        if mode.is_some() || starts_with(arg, "-") {
            return Err(UsageError::UnexpectedArgument { position });
        }
        mode = Some(startup_mode(arg)?);
    }
    let mode = mode.ok_or(UsageError::Missing {
        what: "a startup mode",
    })?;
    // The restart names the account it works for; nothing else names one.
    match (elevated, for_user) {
        (false, None) => Ok(Command::Startup {
            mode,
            for_user: None,
        }),
        (true, Some(user)) => Ok(Command::Startup {
            mode,
            for_user: Some(user),
        }),
        (true, None) => Err(UsageError::Missing {
            what: FOR_USER_OPTION,
        }),
        (false, Some(_)) => Err(UsageError::Missing { what: ELEVATED }),
    }
}

fn parse_convert(cursor: &mut Cursor<'_>) -> Result<Command, UsageError> {
    let mut from = None;
    let mut to = None;
    let mut input = None;
    let mut output = None;
    let mut overwrite = false;
    let mut clipboard = false;
    while let Some((position, arg)) = cursor.next() {
        match arg.to_str() {
            Some(FROM) => set_value(&mut from, cursor, FROM, charset(FROM))?,
            Some(TO) => set_value(&mut to, cursor, TO, charset(TO))?,
            Some(IN) => set_value(&mut input, cursor, IN, path)?,
            Some(OUT) => set_value(&mut output, cursor, OUT, path)?,
            Some(OVERWRITE) => set_flag(&mut overwrite, OVERWRITE)?,
            Some(CLIPBOARD) => set_flag(&mut clipboard, CLIPBOARD)?,
            _ => return Err(UsageError::UnexpectedArgument { position }),
        }
    }
    let from = from.ok_or(UsageError::Missing { what: FROM })?;
    let to = to.ok_or(UsageError::Missing { what: TO })?;
    let target = if clipboard {
        let conflict = [
            (input.is_some(), IN),
            (output.is_some(), OUT),
            (overwrite, OVERWRITE),
        ]
        .into_iter()
        .find_map(|(given, option)| given.then_some(option));
        if let Some(second) = conflict {
            return Err(UsageError::Conflict {
                first: CLIPBOARD,
                second,
            });
        }
        ConvertTarget::Clipboard
    } else {
        match (input, output) {
            (Some(input), Some(output)) => ConvertTarget::Files {
                input,
                output,
                overwrite,
            },
            (Some(_), None) => return Err(UsageError::Missing { what: OUT }),
            (None, Some(_)) => return Err(UsageError::Missing { what: IN }),
            (None, None) => {
                return Err(UsageError::Missing {
                    what: "--in and --out, or --clipboard",
                });
            }
        }
    };
    Ok(Command::Convert(ConvertArgs { from, to, target }))
}

fn parse_migrate(cursor: &mut Cursor<'_>) -> Result<Command, UsageError> {
    let mut args = MigrateArgs {
        from: None,
        dry_run: false,
    };
    while let Some((position, arg)) = cursor.next() {
        match arg.to_str() {
            Some(FROM) => set_value(&mut args.from, cursor, FROM, path)?,
            Some(DRY_RUN) => set_flag(&mut args.dry_run, DRY_RUN)?,
            _ => return Err(UsageError::UnexpectedArgument { position }),
        }
    }
    Ok(Command::Migrate(args))
}
