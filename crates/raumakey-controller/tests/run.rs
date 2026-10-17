//! The run mode (`run::run_controller`): `raumakey.exe` without arguments, against a fake
//! desktop. The single instance first (a second start exits at once with 0); then the
//! settings, the typing hook, the Startup-shortcut repair (D41), and the tray in the saved
//! mode with the first-run and problem messages; the hook stops on every way out; errors are
//! shown to the user, who has no console.

mod support;

use std::path::Path;

use raumakey_config::{ConfigError, StartupMode};
use raumakey_controller::cli::{EXIT_FAILED, EXIT_OK};
use raumakey_controller::notice::{Question, heal_notice, start_notice};
use raumakey_controller::run::{
    ShortcutState, TypingStartError, needs_repair, run_controller, shortcut_points_to,
    start_error_text, typing_error_text,
};
use raumakey_controller::settings::DocumentStatus;
use raumakey_controller::tray::TrayStart;
use raumakey_windows::error::{E_ACCESSDENIED, E_FAIL, WinError, win32};

use support::{DesktopCall, FakeDesktop, opened_settings};

/// The tray start that the run mode builds from the fake desktop's settings.
fn tray_start(desktop: &FakeDesktop) -> TrayStart {
    let opened = &desktop.opened;
    TrayStart {
        runtime: opened.runtime.clone(),
        startup: opened.startup,
        location: opened.report.location,
        config_path: opened.report.config_path.clone(),
        status: opened.report.status.clone(),
        notices: start_notice(&opened.report).into_iter().collect(),
    }
}

/// The tray the run mode started, from the calls.
fn started(desktop: &FakeDesktop) -> &TrayStart {
    desktop
        .calls
        .iter()
        .find_map(|call| match call {
            DesktopCall::RunTray(start) => Some(&**start),
            _ => None,
        })
        .expect("the tray ran")
}

#[test]
fn the_first_instance_opens_the_settings_starts_typing_then_runs_the_tray() {
    let mut desktop = FakeDesktop::default();
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    let start = tray_start(&desktop);
    assert!(start.notices.is_empty(), "a second run without problems");
    assert_eq!(
        desktop.calls,
        [
            DesktopCall::AcquireInstance,
            DesktopCall::OpenSettings,
            DesktopCall::StartTyping,
            DesktopCall::RunTray(Box::new(start)),
            DesktopCall::StopTyping,
            DesktopCall::ReleaseInstance,
        ]
    );
}

/// The icon starts in the mode saved in `raumakey.toml`, with its method and options.
#[test]
fn the_tray_starts_in_the_saved_mode() {
    let mut desktop = FakeDesktop::default();
    desktop.opened.runtime.vietnamese_mode = false;
    desktop.opened.runtime.options.enabled = false;
    desktop.opened.runtime.options.modern_tone = true;
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    let start = started(&desktop);
    assert!(!start.runtime.vietnamese_mode);
    assert!(start.runtime.options.modern_tone);
    assert_eq!(start.runtime, desktop.opened.runtime);
}

#[test]
fn a_second_instance_exits_quietly_with_0_and_opens_nothing() {
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

/// Without the hook Raumakey cannot type: the start ends with an error and the failure code,
/// and the instance is given back before the box waits for the user.
#[test]
fn a_hook_that_cannot_start_is_reported_and_the_controller_exits_with_1() {
    let error = TypingStartError::Hook(WinError::new(E_ACCESSDENIED, "SetWindowsHookExW"));
    let mut desktop = FakeDesktop {
        typing: Err(error),
        ..FakeDesktop::default()
    };
    assert_eq!(run_controller(&mut desktop), EXIT_FAILED);
    assert_eq!(
        desktop.calls,
        [
            DesktopCall::AcquireInstance,
            DesktopCall::OpenSettings,
            DesktopCall::StartTyping,
            DesktopCall::ReleaseInstance,
            DesktopCall::ReportError(typing_error_text(&error)),
        ]
    );
}

#[test]
fn hook_errors_say_that_raumakey_cannot_type_in_vietnamese() {
    let text = typing_error_text(&TypingStartError::Hook(WinError::new(
        E_ACCESSDENIED,
        "SetWindowsHookExW",
    )));
    assert!(text.contains("Không khởi động được"), "{text}");
    assert!(text.contains("hook bàn phím"), "{text}");
    assert!(text.contains("0x80070005"), "{text}");
    assert!(text.contains("SetWindowsHookExW"), "{text}");
    let refused = typing_error_text(&TypingStartError::Refused);
    assert!(refused.contains("Không khởi động được"), "{refused}");
    for text in [text, refused] {
        assert!(!text.contains('\0'));
    }
}

/// The hook stops on this way out as well, and the instance is given back before the error
/// box waits for the user, so a new start is not taken for a second instance meanwhile.
#[test]
fn a_tray_that_cannot_start_stops_the_hook_releases_the_instance_and_is_reported() {
    let error = WinError::new(E_FAIL, "LoadImageW");
    let mut desktop = FakeDesktop {
        tray: Err(error),
        ..FakeDesktop::default()
    };
    assert_eq!(run_controller(&mut desktop), EXIT_FAILED);
    let start = tray_start(&desktop);
    assert_eq!(
        desktop.calls,
        [
            DesktopCall::AcquireInstance,
            DesktopCall::OpenSettings,
            DesktopCall::StartTyping,
            DesktopCall::RunTray(Box::new(start)),
            DesktopCall::StopTyping,
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

/// The first run tells the user once where `raumakey.toml` was created: the tray shows it
/// once its icon is there.
#[test]
fn the_first_run_message_goes_to_the_tray() {
    let mut desktop = FakeDesktop::default();
    desktop.opened.report.first_run = true;
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    let notice = start_notice(&desktop.opened.report).unwrap();
    assert_eq!(started(&desktop).notices, [notice]);
}

/// A damaged file at the start is told once, with the offer to move it aside.
#[test]
fn a_problem_at_the_start_goes_to_the_tray() {
    let mut desktop = FakeDesktop::default();
    desktop.opened.report.status.config = DocumentStatus::Unreadable(ConfigError::Corrupt);
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    let notices = &started(&desktop).notices;
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].question, Some(Question::Recover));
    // The tray knows what was told, so a reload does not tell it again.
    assert_eq!(
        started(&desktop).status.config,
        DocumentStatus::Unreadable(ConfigError::Corrupt)
    );
}

/// D41: with `startup = "shortcut"` saved, the start makes sure the shortcut exists and
/// starts this `raumakey.exe` (the exe may have moved).
#[test]
fn a_saved_shortcut_that_is_missing_or_points_elsewhere_is_recreated() {
    for shortcut in [
        Ok(ShortcutState::Missing),
        Ok(ShortcutState::PointsElsewhere),
        Err(WinError::new(E_FAIL, "IPersistFile::Load")),
    ] {
        let mut desktop = FakeDesktop {
            shortcut,
            ..FakeDesktop::default()
        };
        desktop.opened.startup = StartupMode::Shortcut;
        assert_eq!(run_controller(&mut desktop), EXIT_OK, "{shortcut:?}");
        let mut start = tray_start(&desktop);
        start.startup = StartupMode::Shortcut;
        assert_eq!(
            desktop.calls,
            [
                DesktopCall::AcquireInstance,
                DesktopCall::OpenSettings,
                DesktopCall::StartTyping,
                DesktopCall::StartupShortcut,
                DesktopCall::CreateStartupShortcut,
                DesktopCall::RunTray(Box::new(start)),
                DesktopCall::StopTyping,
                DesktopCall::ReleaseInstance,
            ],
            "{shortcut:?}"
        );
    }
}

#[test]
fn a_saved_shortcut_that_starts_this_exe_is_left_alone() {
    let mut desktop = FakeDesktop {
        shortcut: Ok(ShortcutState::PointsHere),
        ..FakeDesktop::default()
    };
    desktop.opened.startup = StartupMode::Shortcut;
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    assert!(desktop.calls.contains(&DesktopCall::StartupShortcut));
    assert!(!desktop.calls.contains(&DesktopCall::CreateStartupShortcut));
}

/// Without the shortcut mode saved, the start does not look at the Startup folder at all.
#[test]
fn other_startup_modes_leave_the_startup_folder_alone() {
    for mode in [StartupMode::None, StartupMode::ElevatedTask] {
        let mut desktop = FakeDesktop::default();
        desktop.opened.startup = mode;
        assert_eq!(run_controller(&mut desktop), EXIT_OK);
        assert!(
            !desktop.calls.contains(&DesktopCall::StartupShortcut),
            "{mode:?}"
        );
        assert!(
            !desktop.calls.contains(&DesktopCall::CreateStartupShortcut),
            "{mode:?}"
        );
        assert_eq!(started(&desktop).startup, mode);
    }
}

/// A shortcut that cannot be repaired does not stop the controller; the tray tells the user.
#[test]
fn a_shortcut_that_cannot_be_repaired_is_told_by_the_tray() {
    let error = WinError::new(E_ACCESSDENIED, "IPersistFile::Save");
    let mut desktop = FakeDesktop {
        create_shortcut: Err(error),
        ..FakeDesktop::default()
    };
    desktop.opened.startup = StartupMode::Shortcut;
    desktop.opened.report.first_run = true;
    assert_eq!(run_controller(&mut desktop), EXIT_OK);
    assert_eq!(
        started(&desktop).notices,
        [
            start_notice(&desktop.opened.report).unwrap(),
            heal_notice(&error)
        ]
    );
}

#[test]
fn only_a_missing_or_foreign_shortcut_of_the_shortcut_mode_is_repaired() {
    use ShortcutState::{Missing, PointsElsewhere, PointsHere};
    let failed = Err(WinError::new(E_FAIL, "IPersistFile::Load"));
    assert!(needs_repair(StartupMode::Shortcut, &Ok(Missing)));
    assert!(needs_repair(StartupMode::Shortcut, &Ok(PointsElsewhere)));
    assert!(needs_repair(StartupMode::Shortcut, &failed));
    assert!(!needs_repair(StartupMode::Shortcut, &Ok(PointsHere)));
    for mode in [StartupMode::None, StartupMode::ElevatedTask] {
        for state in [Ok(Missing), Ok(PointsElsewhere), Ok(PointsHere), failed] {
            assert!(!needs_repair(mode, &state), "{mode:?} {state:?}");
        }
    }
}

/// The shortcut starts this exe when its target is this exe, as Windows compares paths
/// (ASCII case and `/` do not matter), without arguments.
#[test]
fn a_shortcut_points_here_when_it_starts_this_exe_without_arguments() {
    let exe = Path::new(r"D:\Tools\Raumakey\raumakey.exe");
    assert!(shortcut_points_to(exe, "", exe));
    assert!(shortcut_points_to(
        Path::new(r"d:\tools\RAUMAKEY\Raumakey.EXE"),
        "",
        exe
    ));
    assert!(shortcut_points_to(
        Path::new("D:/Tools/Raumakey/raumakey.exe"),
        "",
        exe
    ));
    assert!(!shortcut_points_to(
        Path::new(r"D:\Old\raumakey.exe"),
        "",
        exe
    ));
    assert!(!shortcut_points_to(exe, "--elevated", exe));
    assert!(!shortcut_points_to(Path::new(""), "", exe));
}

#[test]
fn start_errors_name_raumakey_and_the_failed_operation_in_vietnamese() {
    let text = start_error_text(&WinError::new(E_ACCESSDENIED, "CreateMutexW"));
    assert!(text.contains("Không khởi động được Raumakey"), "{text}");
    assert!(text.contains("0x80070005"), "{text}");
    assert!(text.contains("CreateMutexW"), "{text}");
    assert!(!text.contains('\0'), "{text}");
}

#[test]
fn the_fake_settings_are_a_clean_second_run() {
    let opened = opened_settings();
    assert!(!opened.report.first_run);
    assert_eq!(start_notice(&opened.report), None);
}
