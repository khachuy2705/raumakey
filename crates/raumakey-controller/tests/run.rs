//! The run mode (`run::run_controller`): `raumakey.exe` without arguments, against a fake
//! desktop. The single instance (a second start exits at once with 0), the tray in the
//! initial mode while the instance is held, and errors shown to the user, who has no console.

mod support;

use raumakey_controller::cli::{EXIT_FAILED, EXIT_OK};
use raumakey_controller::run::{run_controller, start_error_text};
use raumakey_controller::tray::initial_mode;
use raumakey_windows::error::{E_ACCESSDENIED, E_FAIL, WinError, win32};

use support::{DesktopCall, FakeDesktop};

#[test]
fn the_first_instance_runs_the_tray_in_the_initial_mode_and_then_releases_the_instance() {
    let mut desktop = FakeDesktop::default();
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    assert_eq!(
        desktop.calls,
        [
            DesktopCall::AcquireInstance,
            DesktopCall::RunTray(initial_mode()),
            DesktopCall::ReleaseInstance,
        ]
    );
}

#[test]
fn a_second_instance_exits_quietly_with_0() {
    let mut desktop = FakeDesktop {
        instance: Ok(false),
        ..FakeDesktop::default()
    };
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    assert_eq!(desktop.calls, [DesktopCall::AcquireInstance]);
}

/// An elevated controller of the same user holds the mutex with a higher integrity label, so
/// a non-elevated start may not open it (`raumakey_windows::single_instance`): a controller
/// runs, and this start ends quietly as well.
#[test]
fn an_instance_that_this_process_may_not_open_counts_as_running() {
    for error in [
        WinError::new(E_ACCESSDENIED, "CreateMutexW"),
        WinError::from_win32(win32::ERROR_ACCESS_DENIED, "CreateMutexW"),
    ] {
        let mut desktop = FakeDesktop {
            instance: Err(error),
            ..FakeDesktop::default()
        };
        assert_eq!(run_controller(&mut desktop), EXIT_OK, "{error}");
        assert_eq!(desktop.calls, [DesktopCall::AcquireInstance], "{error}");
    }
}

#[test]
fn a_failed_instance_check_is_reported_and_nothing_runs() {
    let error = WinError::new(E_FAIL, "CreateMutexW");
    let mut desktop = FakeDesktop {
        instance: Err(error),
        ..FakeDesktop::default()
    };
    assert_eq!(run_controller(&mut desktop), EXIT_FAILED);
    assert_eq!(
        desktop.calls,
        [
            DesktopCall::AcquireInstance,
            DesktopCall::ReportError(start_error_text(&error)),
        ]
    );
}

/// The instance is given back before the error box waits for the user, so a new start is
/// not taken for a second instance meanwhile.
#[test]
fn a_tray_that_cannot_start_releases_the_instance_and_is_reported() {
    let error = WinError::new(E_FAIL, "LoadImageW");
    let mut desktop = FakeDesktop {
        tray: Err(error),
        ..FakeDesktop::default()
    };
    assert_eq!(run_controller(&mut desktop), EXIT_FAILED);
    assert_eq!(
        desktop.calls,
        [
            DesktopCall::AcquireInstance,
            DesktopCall::RunTray(initial_mode()),
            DesktopCall::ReleaseInstance,
            DesktopCall::ReportError(start_error_text(&error)),
        ]
    );
}

#[test]
fn the_exit_code_of_the_message_loop_is_passed_on() {
    for (code, expected) in [
        (0, EXIT_OK),
        (1, 1),
        (255, 255),
        (256, EXIT_FAILED),
        (-1, EXIT_FAILED),
    ] {
        let mut desktop = FakeDesktop {
            tray: Ok(code),
            ..FakeDesktop::default()
        };
        assert_eq!(run_controller(&mut desktop), expected, "{code}");
        assert_eq!(
            desktop.calls.last(),
            Some(&DesktopCall::ReleaseInstance),
            "{code}"
        );
    }
}

#[test]
fn start_errors_name_raumakey_and_the_failed_operation_in_vietnamese() {
    let text = start_error_text(&WinError::new(E_ACCESSDENIED, "CreateMutexW"));
    assert!(text.contains("Không khởi động được Raumakey"), "{text}");
    assert!(text.contains("0x80070005"), "{text}");
    assert!(text.contains("CreateMutexW"), "{text}");
    assert!(!text.contains('\0'), "{text}");
}
