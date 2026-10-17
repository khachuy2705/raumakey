//! The command line of `raumakey.exe` (plan §10.2): parsing only, no side effects.

use std::ffi::OsString;
use std::path::PathBuf;

use raumakey_config::StartupMode;
use raumakey_controller::cli::{
    Command, ConvertArgs, ConvertTarget, EXIT_FAILED, EXIT_INCOMPLETE, EXIT_OK, EXIT_USAGE,
    InstallArgs, MigrateArgs, USAGE, UsageError, parse_args,
};
use raumakey_converter::Charset;
use raumakey_windows::names::SidHash;

/// A SID hash, as `--for-user` takes it.
const ACCOUNT: &str = "0123456789abcdef";

fn account() -> SidHash {
    SidHash::parse(ACCOUNT).unwrap()
}

fn parse(args: &[&str]) -> Result<Command, UsageError> {
    parse_args(args.iter().map(OsString::from))
}

fn install(dry_run: bool, tip_x64: Option<&str>, tip_x86: Option<&str>) -> Command {
    Command::Install(InstallArgs {
        dry_run,
        tip_x64: tip_x64.map(PathBuf::from),
        tip_x86: tip_x86.map(PathBuf::from),
        elevated: false,
    })
}

#[test]
fn no_arguments_run_the_controller() {
    assert_eq!(parse(&[]), Ok(Command::Run));
}

#[test]
fn version_help_and_update_take_no_arguments() {
    for version in ["--version", "-V"] {
        assert_eq!(parse(&[version]), Ok(Command::Version), "{version}");
    }
    for help in ["help", "--help", "-h"] {
        assert_eq!(parse(&[help]), Ok(Command::Help), "{help}");
    }
    assert_eq!(parse(&["update"]), Ok(Command::Update));
    for command in ["--version", "help", "update"] {
        assert_eq!(
            parse(&[command, "extra"]),
            Err(UsageError::UnexpectedArgument { position: 2 }),
            "{command}"
        );
    }
}

#[test]
fn install_options_default_to_the_package_layout() {
    assert_eq!(parse(&["install"]), Ok(install(false, None, None)));
    assert_eq!(
        parse(&["install", "--dry-run"]),
        Ok(install(true, None, None))
    );
    assert_eq!(
        parse(&["install", "--tip-x86", "b.dll"]),
        Ok(install(false, None, Some("b.dll")))
    );
}

/// The options of `install` (the TSF text service, not shipped) come in any order: `--dry-run`
/// before the TIP paths, after them, or between them.
#[test]
fn install_options_are_accepted_in_any_order() {
    let expected = Ok(install(
        true,
        Some("C:\\p\\x64\\t.dll"),
        Some("C:\\p\\x86\\t.dll"),
    ));
    for args in [
        [
            "install",
            "--dry-run",
            "--tip-x64",
            "C:\\p\\x64\\t.dll",
            "--tip-x86",
            "C:\\p\\x86\\t.dll",
        ],
        [
            "install",
            "--tip-x64",
            "C:\\p\\x64\\t.dll",
            "--tip-x86",
            "C:\\p\\x86\\t.dll",
            "--dry-run",
        ],
        [
            "install",
            "--tip-x86",
            "C:\\p\\x86\\t.dll",
            "--dry-run",
            "--tip-x64",
            "C:\\p\\x64\\t.dll",
        ],
    ] {
        assert_eq!(parse(&args), expected, "{args:?}");
    }
}

#[test]
fn the_elevated_marker_is_an_option_of_the_privileged_commands() {
    assert_eq!(
        parse(&["install", "--elevated", "--tip-x64", "a.dll"]),
        Ok(Command::Install(InstallArgs {
            dry_run: false,
            tip_x64: Some(PathBuf::from("a.dll")),
            tip_x86: None,
            elevated: true,
        }))
    );
    assert_eq!(
        parse(&["uninstall", "--elevated"]),
        Ok(Command::Uninstall {
            dry_run: false,
            elevated: true
        })
    );
    assert_eq!(
        parse(&["startup", "--elevated", "shortcut", "--for-user", ACCOUNT]),
        Ok(Command::Startup {
            mode: StartupMode::Shortcut,
            for_user: Some(account()),
        })
    );
    assert_eq!(
        parse(&["startup", "--for-user", ACCOUNT, "none", "--elevated"]),
        Ok(Command::Startup {
            mode: StartupMode::None,
            for_user: Some(account()),
        })
    );
    assert_eq!(
        parse(&["convert", "--elevated"]),
        Err(UsageError::UnexpectedArgument { position: 2 })
    );
}

/// The restarted `startup` changes only the elevated task of the account that asked, so it must
/// name that account: `--elevated` and `--for-user` come together, and the account is a SID
/// hash (16 lowercase hexadecimal digits).
#[test]
fn the_restarted_startup_names_the_account_that_asked() {
    assert_eq!(
        parse(&["startup", "none", "--elevated"]),
        Err(UsageError::Missing { what: "--for-user" })
    );
    assert_eq!(
        parse(&["startup", "none", "--for-user", ACCOUNT]),
        Err(UsageError::Missing { what: "--elevated" })
    );
    for bad in [
        "0123456789ABCDEF",
        "0123",
        "0123456789abcdeg",
        "S-1-5-21-1-2-3-1001",
    ] {
        assert_eq!(
            parse(&["startup", "none", "--elevated", "--for-user", bad]),
            Err(UsageError::InvalidValue {
                option: "--for-user"
            }),
            "{bad}"
        );
    }
    assert_eq!(
        parse(&["startup", "none", "--elevated", "--for-user"]),
        Err(UsageError::MissingValue {
            option: "--for-user"
        })
    );
    assert_eq!(
        parse(&[
            "startup",
            "none",
            "--elevated",
            "--for-user",
            ACCOUNT,
            "--for-user",
            ACCOUNT
        ]),
        Err(UsageError::DuplicateOption {
            option: "--for-user"
        })
    );
    // Only `startup` restarts for one account.
    assert_eq!(
        parse(&["install", "--for-user", ACCOUNT]),
        Err(UsageError::UnexpectedArgument { position: 2 })
    );
}

#[test]
fn repeated_options_are_usage_errors() {
    assert_eq!(
        parse(&["install", "--dry-run", "--dry-run"]),
        Err(UsageError::DuplicateOption {
            option: "--dry-run"
        })
    );
    assert_eq!(
        parse(&["install", "--tip-x64", "a.dll", "--tip-x64", "b.dll"]),
        Err(UsageError::DuplicateOption {
            option: "--tip-x64"
        })
    );
    assert_eq!(
        parse(&["uninstall", "--dry-run", "--dry-run"]),
        Err(UsageError::DuplicateOption {
            option: "--dry-run"
        })
    );
    assert_eq!(
        parse(&["startup", "none", "--elevated", "--elevated"]),
        Err(UsageError::DuplicateOption {
            option: "--elevated"
        })
    );
}

#[test]
fn missing_values_are_usage_errors() {
    for args in [
        &["install", "--tip-x64"][..],
        &["install", "--tip-x64", ""],
        // A value never starts with `--`: the next option is not taken as a path.
        &["install", "--tip-x64", "--dry-run"],
    ] {
        assert_eq!(
            parse(args),
            Err(UsageError::MissingValue {
                option: "--tip-x64"
            }),
            "{args:?}"
        );
    }
    assert_eq!(
        parse(&["migrate", "--from"]),
        Err(UsageError::MissingValue { option: "--from" })
    );
}

#[test]
fn unknown_options_and_stray_arguments_are_usage_errors() {
    assert_eq!(
        parse(&["install", "--bogus"]),
        Err(UsageError::UnexpectedArgument { position: 2 })
    );
    assert_eq!(
        parse(&["install", "--dry-run", "x.dll"]),
        Err(UsageError::UnexpectedArgument { position: 3 })
    );
    assert_eq!(
        parse(&["uninstall", "--tip-x64", "a.dll"]),
        Err(UsageError::UnexpectedArgument { position: 2 })
    );
    assert_eq!(
        parse(&["migrate", "--from", "dir", "--overwrite"]),
        Err(UsageError::UnexpectedArgument { position: 4 })
    );
}

#[test]
fn the_command_comes_first() {
    assert_eq!(parse(&["bogus"]), Err(UsageError::UnknownCommand));
    assert_eq!(
        parse(&["--dry-run", "install"]),
        Err(UsageError::UnknownCommand)
    );
    assert_eq!(parse(&["Install"]), Err(UsageError::UnknownCommand));
}

#[test]
fn uninstall_takes_only_the_dry_run_and_elevated_options() {
    assert_eq!(
        parse(&["uninstall"]),
        Ok(Command::Uninstall {
            dry_run: false,
            elevated: false
        })
    );
    assert_eq!(
        parse(&["uninstall", "--dry-run"]),
        Ok(Command::Uninstall {
            dry_run: true,
            elevated: false
        })
    );
}

#[test]
fn startup_takes_exactly_one_mode() {
    for (name, mode) in [
        ("none", StartupMode::None),
        ("shortcut", StartupMode::Shortcut),
        ("elevated-task", StartupMode::ElevatedTask),
        ("Elevated-Task", StartupMode::ElevatedTask),
    ] {
        assert_eq!(
            parse(&["startup", name]),
            Ok(Command::Startup {
                mode,
                for_user: None
            }),
            "{name}"
        );
    }
    assert_eq!(
        parse(&["startup"]),
        Err(UsageError::Missing {
            what: "a startup mode"
        })
    );
    assert_eq!(
        parse(&["startup", "always"]),
        Err(UsageError::InvalidValue { option: "startup" })
    );
    assert_eq!(
        parse(&["startup", "none", "shortcut"]),
        Err(UsageError::UnexpectedArgument { position: 3 })
    );
    assert_eq!(
        parse(&["startup", "--bogus"]),
        Err(UsageError::UnexpectedArgument { position: 2 })
    );
}

fn files(input: &str, output: &str, overwrite: bool) -> ConvertTarget {
    ConvertTarget::Files {
        input: PathBuf::from(input),
        output: PathBuf::from(output),
        overwrite,
    }
}

#[test]
fn convert_reads_files_or_the_clipboard_with_options_in_any_order() {
    let expected = Ok(Command::Convert(ConvertArgs {
        from: Charset::Tcvn3,
        to: Charset::Unicode,
        target: files("in.txt", "out.txt", false),
    }));
    assert_eq!(
        parse(&[
            "convert", "--from", "tcvn3", "--to", "unicode", "--in", "in.txt", "--out", "out.txt"
        ]),
        expected
    );
    assert_eq!(
        parse(&[
            "convert", "--out", "out.txt", "--in", "in.txt", "--to", "UNICODE", "--from", "TCVN3"
        ]),
        expected
    );
    assert_eq!(
        parse(&[
            "convert",
            "--overwrite",
            "--from",
            "vni-windows",
            "--in",
            "a",
            "--to",
            "viqr",
            "--out",
            "b"
        ]),
        Ok(Command::Convert(ConvertArgs {
            from: Charset::VniWindows,
            to: Charset::Viqr,
            target: files("a", "b", true),
        }))
    );
    assert_eq!(
        parse(&[
            "convert",
            "--clipboard",
            "--to",
            "unicode",
            "--from",
            "viscii"
        ]),
        Ok(Command::Convert(ConvertArgs {
            from: Charset::Viscii,
            to: Charset::Unicode,
            target: ConvertTarget::Clipboard,
        }))
    );
}

#[test]
fn every_charset_name_is_accepted() {
    for charset in Charset::ALL {
        assert_eq!(
            parse(&[
                "convert",
                "--from",
                charset.name(),
                "--to",
                "unicode",
                "--clipboard"
            ]),
            Ok(Command::Convert(ConvertArgs {
                from: charset,
                to: Charset::Unicode,
                target: ConvertTarget::Clipboard,
            })),
            "{charset}"
        );
    }
}

#[test]
fn convert_usage_errors() {
    let cases: [(&[&str], UsageError); 12] = [
        (
            &["convert", "--to", "unicode", "--clipboard"],
            UsageError::Missing { what: "--from" },
        ),
        (
            &["convert", "--from", "unicode", "--clipboard"],
            UsageError::Missing { what: "--to" },
        ),
        (
            &[
                "convert",
                "--from",
                "klingon",
                "--to",
                "unicode",
                "--clipboard",
            ],
            UsageError::InvalidValue { option: "--from" },
        ),
        (
            &[
                "convert",
                "--from",
                "unicode",
                "--to",
                "utf-7",
                "--clipboard",
            ],
            UsageError::InvalidValue { option: "--to" },
        ),
        (
            &["convert", "--from", "unicode", "--to", "viqr"],
            UsageError::Missing {
                what: "--in and --out, or --clipboard",
            },
        ),
        (
            &["convert", "--from", "unicode", "--to", "viqr", "--in", "a"],
            UsageError::Missing { what: "--out" },
        ),
        (
            &["convert", "--from", "unicode", "--to", "viqr", "--out", "b"],
            UsageError::Missing { what: "--in" },
        ),
        (
            &[
                "convert",
                "--from",
                "unicode",
                "--to",
                "viqr",
                "--overwrite",
            ],
            UsageError::Missing {
                what: "--in and --out, or --clipboard",
            },
        ),
        (
            &[
                "convert",
                "--from",
                "unicode",
                "--to",
                "viqr",
                "--clipboard",
                "--in",
                "a",
            ],
            UsageError::Conflict {
                first: "--clipboard",
                second: "--in",
            },
        ),
        (
            &[
                "convert",
                "--from",
                "unicode",
                "--to",
                "viqr",
                "--out",
                "b",
                "--clipboard",
            ],
            UsageError::Conflict {
                first: "--clipboard",
                second: "--out",
            },
        ),
        (
            &[
                "convert",
                "--overwrite",
                "--from",
                "unicode",
                "--to",
                "viqr",
                "--clipboard",
            ],
            UsageError::Conflict {
                first: "--clipboard",
                second: "--overwrite",
            },
        ),
        (
            &[
                "convert",
                "--from",
                "unicode",
                "--from",
                "viqr",
                "--to",
                "viqr",
                "--clipboard",
            ],
            UsageError::DuplicateOption { option: "--from" },
        ),
    ];
    for (args, error) in cases {
        assert_eq!(parse(args), Err(error), "{args:?}");
    }
}

#[test]
fn migrate_options_are_parsed_in_any_order() {
    assert_eq!(
        parse(&["migrate"]),
        Ok(Command::Migrate(MigrateArgs {
            from: None,
            dry_run: false
        }))
    );
    let expected = Ok(Command::Migrate(MigrateArgs {
        from: Some(PathBuf::from("C:\\EVKey")),
        dry_run: true,
    }));
    assert_eq!(
        parse(&["migrate", "--from", "C:\\EVKey", "--dry-run"]),
        expected
    );
    assert_eq!(
        parse(&["migrate", "--dry-run", "--from", "C:\\EVKey"]),
        expected
    );
}

#[cfg(windows)]
#[test]
fn paths_may_be_any_os_string() {
    use std::os::windows::ffi::OsStringExt;
    // An unpaired surrogate: valid in a Windows path, not UTF-8.
    let path = OsString::from_wide(&[0xD800, u16::from(b'a')]);
    let args = vec![
        OsString::from("install"),
        OsString::from("--tip-x64"),
        path.clone(),
    ];
    assert_eq!(
        parse_args(args),
        Ok(Command::Install(InstallArgs {
            dry_run: false,
            tip_x64: Some(PathBuf::from(path)),
            tip_x86: None,
            elevated: false,
        }))
    );
    // A charset name must be text.
    let name = OsString::from_wide(&[0xD800]);
    let args = vec![
        OsString::from("convert"),
        OsString::from("--from"),
        name,
        OsString::from("--to"),
        OsString::from("unicode"),
        OsString::from("--clipboard"),
    ];
    assert_eq!(
        parse_args(args),
        Err(UsageError::InvalidValue { option: "--from" })
    );
}

/// Errors name options and positions, never the text of an argument.
#[test]
fn usage_errors_never_echo_arguments() {
    let secret = "Tiếng-Việt-secret";
    for args in [
        &["install", secret][..],
        &[secret],
        &[
            "convert",
            "--from",
            secret,
            "--to",
            "unicode",
            "--clipboard",
        ],
        &["startup", secret],
        &["startup", "none", "--elevated", "--for-user", secret],
    ] {
        let error = parse(args).unwrap_err();
        let text = error.to_string();
        assert!(!text.is_empty(), "{args:?}");
        assert!(!text.contains(secret), "{text}");
    }
}

#[test]
fn every_usage_error_has_a_message() {
    for error in [
        UsageError::UnknownCommand,
        UsageError::UnexpectedArgument { position: 2 },
        UsageError::DuplicateOption {
            option: "--dry-run",
        },
        UsageError::MissingValue { option: "--from" },
        UsageError::Missing { what: "--to" },
        UsageError::InvalidValue { option: "--from" },
        UsageError::InvalidValue { option: "startup" },
        UsageError::InvalidValue {
            option: "--for-user",
        },
        UsageError::Conflict {
            first: "--clipboard",
            second: "--in",
        },
    ] {
        assert!(!error.to_string().is_empty(), "{error:?}");
    }
    // The charset error lists the accepted names.
    let text = UsageError::InvalidValue { option: "--to" }.to_string();
    for charset in Charset::ALL {
        assert!(text.contains(charset.name()), "{text}");
    }
    let text = UsageError::InvalidValue { option: "startup" }.to_string();
    for mode in ["none", "shortcut", "elevated-task"] {
        assert!(text.contains(mode), "{text}");
    }
}

#[test]
fn the_usage_text_names_every_command() {
    for command in [
        "install",
        "uninstall",
        "startup",
        "convert",
        "migrate",
        "update",
        "--version",
        "help",
        "--dry-run",
        "--tip-x64",
        "--tip-x86",
        "--clipboard",
        "--overwrite",
        "elevated-task",
        "--elevated",
        "--for-user",
        "Program Files",
    ] {
        assert!(USAGE.contains(command), "{command}");
    }
    for charset in Charset::ALL {
        assert!(USAGE.contains(charset.name()), "{charset}");
    }
}

/// The indented description below the usage line that starts with `usage`, as one line.
fn description_of(usage: &str) -> String {
    let mut lines = USAGE.lines();
    assert!(
        lines.any(|line| line.trim().starts_with(usage)),
        "{usage}: {USAGE}"
    );
    let description: Vec<&str> = lines
        .take_while(|line| line.starts_with("      "))
        .map(str::trim)
        .collect();
    description.join(" ")
}

/// Without arguments `raumakey.exe` runs the portable controller (D38–D42): it types
/// Vietnamese through a keyboard hook, keeps `raumakey.toml` next to the exe, and has its
/// settings in the tray menu; `migrate` is the only command that is not available.
#[test]
fn the_usage_describes_the_portable_controller_without_arguments() {
    assert!(
        USAGE.lines().any(|line| line.trim() == "raumakey"),
        "{USAGE}"
    );
    let description = description_of("raumakey");
    for words in [
        "keyboard hook",
        "notification-area icon",
        "raumakey.toml",
        "next to raumakey.exe",
        "%LOCALAPPDATA%\\Raumakey",
        "tray menu",
        "Ctrl+Shift",
    ] {
        assert!(description.contains(words), "{words}: {description}");
    }
    assert!(
        description.contains("exits"),
        "a second start: {description}"
    );
    assert!(!description.contains("Not available"), "{description}");
    assert_eq!(USAGE.matches("Not available").count(), 1, "{USAGE}");
}

/// `install` and `uninstall` stay for the TSF text service, which the portable release does
/// not ship (design §21.1).
#[test]
fn install_and_uninstall_are_marked_for_the_tsf_text_service_that_is_not_shipped() {
    for command in ["raumakey install", "raumakey uninstall"] {
        let description = description_of(command);
        assert!(
            description.contains("TSF text service"),
            "{command}: {description}"
        );
        assert!(
            description.contains("not shipped"),
            "{command}: {description}"
        );
    }
}

#[test]
fn exit_codes_are_distinct() {
    let codes = [EXIT_OK, EXIT_FAILED, EXIT_USAGE, EXIT_INCOMPLETE];
    assert_eq!(codes, [0, 1, 2, 3]);
}

/// Exit code 3 also stands for an elevated restart that did not finish in time: it may still
/// change the machine, so callers must not read it as "nothing changed" (1).
#[test]
fn the_usage_explains_an_unknown_outcome() {
    let (_, codes) = USAGE.split_once("Exit codes:").expect("exit codes");
    let codes = codes.split_whitespace().collect::<Vec<_>>().join(" ");
    let (_, three) = codes.split_once("3 ").expect("exit code 3");
    assert!(three.contains("unknown"), "{codes}");
    assert!(three.contains("did not finish in time"), "{codes}");
}
