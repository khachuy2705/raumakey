//! Dispatch of parsed commands (`commands::execute`) against the in-memory fake: messages,
//! exit codes, and the defaults that come from the location of `raumakey.exe`.

mod support;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use raumakey_controller::cli::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, USAGE, UsageError, parse_args};
use raumakey_controller::commands::{Context, execute, print_usage_error};
use raumakey_controller::install::default_tip_path;
use raumakey_controller::run::start_error_text;
use raumakey_controller::tray::initial_mode;
use raumakey_windows::error::{E_FAIL, WinError};
use raumakey_windows::tsf_registration::TipArch;

use support::{Captured, DesktopCall, FakeClipboard, FakeSystem, ViewState, capture, me};

fn exe_dir() -> PathBuf {
    PathBuf::from("C:\\pkg")
}

fn exec(args: &[&str], system: &mut FakeSystem) -> Captured {
    let command = match parse_args(args.iter().map(OsString::from)) {
        Ok(command) => command,
        Err(error) => panic!("{args:?} must parse: {error}"),
    };
    let context = Context { exe_dir: exe_dir() };
    capture(|output| execute(&command, &context, system, output))
}

#[test]
fn version_prints_the_product_name_and_version() {
    let mut system = FakeSystem::default();
    let result = exec(&["--version"], &mut system);
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(
        result.out,
        format!("Raumakey {}\n", env!("CARGO_PKG_VERSION"))
    );
    assert!(result.err.is_empty());
    assert!(system.calls.is_empty());
}

#[test]
fn help_prints_the_usage() {
    let result = exec(&["help"], &mut FakeSystem::default());
    assert_eq!(result.code, EXIT_OK);
    assert_eq!(result.out, USAGE);
}

/// Matrix #57: there is no updater; `update` says so and never touches the network.
#[test]
fn update_reports_that_updates_are_not_available() {
    let mut system = FakeSystem::default();
    let result = exec(&["update"], &mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(result.out.contains("not available"), "{}", result.out);
    assert!(result.out.contains("Raumakey"), "{}", result.out);
    assert!(result.err.is_empty());
    assert!(system.calls.is_empty());
}

#[test]
fn migrate_is_not_available_yet() {
    for args in [
        &["migrate"][..],
        &["migrate", "--from", "C:\\EVKey", "--dry-run"],
    ] {
        let mut system = FakeSystem::default();
        let result = exec(args, &mut system);
        assert_eq!(result.code, EXIT_USAGE, "{args:?}: {result:?}");
        assert!(result.out.is_empty(), "{}", result.out);
        assert!(result.err.contains("not available"), "{}", result.err);
        assert!(result.err.contains("Nothing was read"), "{}", result.err);
        assert!(system.calls.is_empty());
    }
}

/// Without arguments `raumakey.exe` runs the controller: the tray on the desktop, with the
/// exit code of its message loop. It prints nothing (it has no console).
#[test]
fn no_arguments_run_the_controller_on_the_desktop() {
    let mut system = FakeSystem::default();
    let result = exec(&[], &mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(result.out.is_empty(), "{}", result.out);
    assert!(result.err.is_empty(), "{}", result.err);
    assert_eq!(
        system.desktop.calls,
        [
            DesktopCall::AcquireInstance,
            DesktopCall::RunTray(initial_mode()),
            DesktopCall::ReleaseInstance,
        ]
    );
    assert!(system.calls.is_empty());
}

/// A failure is shown on the desktop (a message box), not printed.
#[test]
fn a_controller_that_cannot_start_reports_on_the_desktop_and_exits_with_1() {
    let mut system = FakeSystem::default();
    let error = WinError::new(E_FAIL, "RegisterClassExW");
    system.desktop.tray = Err(error);
    let result = exec(&[], &mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(result.out.is_empty() && result.err.is_empty(), "{result:?}");
    assert_eq!(
        system.desktop.calls.last(),
        Some(&DesktopCall::ReportError(start_error_text(&error)))
    );
}

#[test]
fn usage_errors_print_the_error_and_the_usage() {
    let result = capture(|output| {
        print_usage_error(
            &UsageError::DuplicateOption {
                option: "--dry-run",
            },
            output,
        )
    });
    assert_eq!(result.code, EXIT_USAGE);
    assert!(result.out.is_empty());
    assert!(
        result
            .err
            .starts_with("raumakey: --dry-run is given more than once"),
        "{}",
        result.err
    );
    assert!(result.err.ends_with(USAGE), "{}", result.err);
}

#[test]
fn install_defaults_to_the_tips_next_to_raumakey() {
    let mut system = FakeSystem::default();
    let result = exec(&["install", "--dry-run"], &mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    for arch in [TipArch::X64, TipArch::X86] {
        let path = default_tip_path(&exe_dir(), arch);
        assert!(
            result.out.contains(&path.display().to_string()),
            "{}",
            result.out
        );
    }
    assert!(system.calls.is_empty(), "a dry run touches nothing");
}

#[test]
fn relative_tip_paths_are_resolved_against_the_current_directory() {
    let mut system = FakeSystem::default();
    let result = exec(
        &[
            "install",
            "--dry-run",
            "--tip-x86",
            "build\\x86\\raumakey_tip.dll",
        ],
        &mut system,
    );
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    let absolute = std::path::absolute(Path::new("build\\x86\\raumakey_tip.dll")).unwrap();
    assert!(absolute.is_absolute());
    assert!(
        result.out.contains(&absolute.display().to_string()),
        "{}",
        result.out
    );
}

#[test]
fn uninstall_dry_run_touches_nothing() {
    let mut system = FakeSystem::default();
    let result = exec(&["uninstall", "--dry-run"], &mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.calls.is_empty());
}

#[test]
fn install_and_uninstall_run_with_the_package_tips() {
    let x64 = default_tip_path(&exe_dir(), TipArch::X64);
    let x86 = default_tip_path(&exe_dir(), TipArch::X86);
    let mut system = FakeSystem::with_files(&[&x64, &x86]);
    let result = exec(&["install"], &mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(system.view64, ViewState::registered(&x64));
    assert_eq!(system.view32, ViewState::registered(&x86));

    let result = exec(&["uninstall"], &mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.view64.is_empty() && system.view32.is_empty());
    assert_eq!(system.record, None);
}

#[test]
fn startup_and_clipboard_conversion_are_dispatched() {
    let mut system = FakeSystem::default();
    let result = exec(&["startup", "shortcut"], &mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.shortcut);

    // The elevated restart of `startup` changes only the task of the account that asked.
    let account = me();
    let result = exec(
        &[
            "startup",
            "elevated-task",
            "--for-user",
            account.as_str(),
            "--elevated",
        ],
        &mut system,
    );
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.my_task());
    assert!(system.shortcut, "the process that asked removes it");

    system.clipboard = FakeClipboard::with_text("TiÕng ViÖt");
    let result = exec(
        &[
            "convert",
            "--from",
            "tcvn3",
            "--to",
            "unicode",
            "--clipboard",
        ],
        &mut system,
    );
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(system.clipboard.text().as_deref(), Some("Tiếng Việt"));
}

#[test]
fn a_failed_command_returns_its_exit_code() {
    // A clipboard without text: the conversion fails.
    let mut system = FakeSystem {
        clipboard: FakeClipboard::default(),
        ..FakeSystem::default()
    };
    let result = exec(
        &[
            "convert",
            "--from",
            "tcvn3",
            "--to",
            "unicode",
            "--clipboard",
        ],
        &mut system,
    );
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
}
