//! `startup <none|shortcut|elevated-task>` (design §13, matrix #56) against the in-memory fake:
//! the Startup-folder shortcut is the normal way to start at logon, the elevated logon task
//! exists only after the explicit `elevated-task` choice, and choosing a mode removes the other
//! one idempotently. Each account has its own task; an elevated restart changes only that task,
//! and only for the account that asked for it.

mod support;

use raumakey_config::StartupMode::{self, ElevatedTask, Shortcut};
use raumakey_controller::cli::{EXIT_FAILED, EXIT_INCOMPLETE, EXIT_OK};
use raumakey_controller::startup::startup;
use raumakey_windows::error::{E_FAIL, WinError};
use raumakey_windows::names::SidHash;

use support::{
    Call, Captured, FakeSystem, cancelled, capture, denied, me, someone_else, timed_out,
};

const MODES: [StartupMode; 3] = [StartupMode::None, Shortcut, ElevatedTask];

/// The user runs `raumakey startup <mode>`.
fn run(system: &mut FakeSystem, mode: StartupMode) -> Captured {
    capture(|output| startup(system, mode, None, output))
}

/// The elevated restart that `user` asked for: `startup <mode> --for-user <user> --elevated`.
fn run_restarted(system: &mut FakeSystem, mode: StartupMode, user: &SidHash) -> Captured {
    capture(|output| startup(system, mode, Some(user), output))
}

/// The elevated restart of `mode` for the account of this process.
fn elevated_restart(mode: StartupMode) -> Call {
    Call::RunElevated(vec![
        "startup".to_owned(),
        mode.as_str().to_owned(),
        "--for-user".to_owned(),
        me().as_str().to_owned(),
        "--elevated".to_owned(),
    ])
}

/// A machine on which this process is not elevated.
fn standard() -> FakeSystem {
    FakeSystem {
        elevated: Ok(false),
        ..FakeSystem::default()
    }
}

/// A fake machine on which the elevated task of this process's account exists.
fn with_my_task(mut system: FakeSystem) -> FakeSystem {
    system.tasks.insert(me());
    system
}

fn restarted(system: &FakeSystem) -> bool {
    system
        .calls
        .iter()
        .any(|call| matches!(call, Call::RunElevated(_)))
}

// ---------------------------------------------------------------------------------------------
// The modes

#[test]
fn the_shortcut_needs_no_administrator_rights() {
    let mut system = standard();
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.shortcut);
    assert!(system.tasks.is_empty());
    assert_eq!(
        system.calls,
        vec![Call::TaskExists(me()), Call::CreateShortcut]
    );
}

/// The shortcut lives in the Startup folder of the account that asked, so the process the user
/// started makes it; the elevated restart, which may run as another account
/// (over-the-shoulder elevation), only deletes the task.
#[test]
fn switching_from_the_task_to_the_shortcut_restarts_elevated_only_for_the_task() {
    let mut system = with_my_task(standard());
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(
        system.calls,
        vec![
            Call::TaskExists(me()),
            Call::CreateShortcut,
            Call::IsElevated,
            elevated_restart(Shortcut),
        ]
    );
    assert!(system.shortcut);
}

#[test]
fn the_shortcut_is_created_before_the_task_is_deleted() {
    let mut system = with_my_task(FakeSystem::default());
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.shortcut && !system.my_task());
    assert_eq!(
        system.calls,
        vec![
            Call::TaskExists(me()),
            Call::CreateShortcut,
            Call::IsElevated,
            Call::DeleteTask(me()),
        ]
    );
}

#[test]
fn the_elevated_task_is_created_only_when_chosen_and_needs_administrator_rights() {
    let mut system = FakeSystem {
        shortcut: true,
        ..standard()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    // The restarted process creates the task; then this one removes the shortcut.
    assert_eq!(
        system.calls,
        vec![
            Call::IsElevated,
            elevated_restart(ElevatedTask),
            Call::RemoveShortcut,
        ]
    );
    assert!(!system.shortcut);
    assert!(system.tasks.is_empty(), "the restarted process creates it");
}

#[test]
fn the_elevated_task_replaces_the_shortcut() {
    let mut system = FakeSystem {
        shortcut: true,
        ..FakeSystem::default()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.my_task() && !system.shortcut);
    assert_eq!(
        system.calls,
        vec![
            Call::IsElevated,
            Call::CreateTask(me()),
            Call::RemoveShortcut
        ]
    );
}

#[test]
fn none_removes_both_and_needs_administrator_rights_only_for_the_task() {
    let mut system = with_my_task(FakeSystem {
        shortcut: true,
        ..FakeSystem::default()
    });
    let result = run(&mut system, StartupMode::None);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(!system.shortcut && !system.my_task());
    assert!(
        result.out.contains("does not start at logon"),
        "{}",
        result.out
    );

    let mut system = FakeSystem {
        shortcut: true,
        ..standard()
    };
    let result = run(&mut system, StartupMode::None);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(!system.shortcut);
    assert!(!system.calls.contains(&Call::IsElevated));
}

#[test]
fn every_mode_is_idempotent() {
    for mode in MODES {
        let mut system = FakeSystem::default();
        assert_eq!(run(&mut system, mode).code, EXIT_OK, "{mode:?}");
        let state = (system.shortcut, system.my_task());
        let result = run(&mut system, mode);
        assert_eq!(result.code, EXIT_OK, "{mode:?}: {result:?}");
        assert_eq!((system.shortcut, system.my_task()), state, "{mode:?}");
        assert_eq!(state, (mode == Shortcut, mode == ElevatedTask));
    }
}

// ---------------------------------------------------------------------------------------------
// One task per account

/// Each account has its own elevated task, removed only when that account disables it
/// (matrix #56): no mode looks up, replaces, or deletes the task of another account.
#[test]
fn the_task_of_another_account_is_never_touched() {
    for mode in MODES {
        let mut system = FakeSystem::default();
        system.tasks.insert(someone_else());
        let result = run(&mut system, mode);
        assert_eq!(result.code, EXIT_OK, "{mode:?}: {result:?}");
        assert!(system.tasks.contains(&someone_else()), "{mode:?}");
        assert_eq!(system.my_task(), mode == ElevatedTask, "{mode:?}");
        for call in &system.calls {
            if let Call::TaskExists(user) | Call::CreateTask(user) | Call::DeleteTask(user) = call {
                assert_eq!(*user, me(), "{mode:?}: {:?}", system.calls);
            }
        }
    }
}

/// The restarted process changes the task and nothing else: the shortcut was handled by the
/// process the user started.
#[test]
fn the_elevated_restart_changes_only_the_task_of_the_account_that_asked() {
    let mut system = FakeSystem {
        shortcut: true,
        ..FakeSystem::default()
    };
    let result = run_restarted(&mut system, ElevatedTask, &me());
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(system.calls, vec![Call::IsElevated, Call::CreateTask(me())]);
    assert!(system.shortcut, "the process that asked removes it");

    for mode in [Shortcut, StartupMode::None] {
        let mut system = with_my_task(FakeSystem::default());
        let result = run_restarted(&mut system, mode, &me());
        assert_eq!(result.code, EXIT_OK, "{mode:?}: {result:?}");
        assert_eq!(
            system.calls,
            vec![Call::IsElevated, Call::DeleteTask(me())],
            "{mode:?}"
        );
    }
}

/// Over-the-shoulder elevation: a standard user types the credentials of an administrator, and
/// the restarted process runs as that administrator. It must not set up or remove the
/// administrator's elevated startup in place of the user's.
#[test]
fn an_elevated_restart_as_another_account_changes_nothing() {
    for mode in MODES {
        let mut system = FakeSystem {
            user: Ok(someone_else()),
            ..FakeSystem::default()
        };
        system.tasks.insert(someone_else());
        let result = run_restarted(&mut system, mode, &me());
        assert_eq!(result.code, EXIT_FAILED, "{mode:?}: {result:?}");
        assert!(!system.changed_anything(), "{mode:?}: {:?}", system.calls);
        assert!(system.tasks.contains(&someone_else()));
        assert!(
            result.err.contains("another account"),
            "{mode:?}: {}",
            result.err
        );
    }
}

#[test]
fn an_elevated_restart_that_cannot_identify_its_account_changes_nothing() {
    let mut system = FakeSystem {
        user: Err(denied("fake token query")),
        ..FakeSystem::default()
    };
    let result = run_restarted(&mut system, ElevatedTask, &me());
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.changed_anything());
}

#[test]
fn an_unidentified_account_still_sets_up_the_shortcut_but_never_the_task() {
    let mut system = FakeSystem {
        user: Err(denied("fake token query")),
        ..FakeSystem::default()
    };
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.shortcut);
    assert!(result.err.contains("task"), "{}", result.err);

    let mut system = FakeSystem {
        user: Err(denied("fake token query")),
        shortcut: true,
        ..standard()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.changed_anything());
    assert!(!restarted(&system));
}

// ---------------------------------------------------------------------------------------------
// Where raumakey.exe lives

/// The task starts raumakey.exe with administrator rights at every logon: a raumakey.exe that
/// the account's programs can replace would hand them those rights.
#[test]
fn the_elevated_task_needs_raumakey_in_a_folder_only_administrators_can_change() {
    let mut system = FakeSystem {
        shortcut: true,
        program_protected: false,
        ..standard()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(system.calls, vec![], "no UAC prompt for nothing");
    assert!(system.shortcut);
    assert!(result.err.contains("Program Files"), "{}", result.err);

    // The restarted process checks again.
    let mut system = FakeSystem {
        program_protected: false,
        ..FakeSystem::default()
    };
    let result = run_restarted(&mut system, ElevatedTask, &me());
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.changed_anything());

    // Removing the task needs no such folder.
    let mut system = with_my_task(FakeSystem {
        program_protected: false,
        ..FakeSystem::default()
    });
    assert_eq!(run(&mut system, StartupMode::None).code, EXIT_OK);
    assert!(!system.my_task());
}

// ---------------------------------------------------------------------------------------------
// Failures

#[test]
fn a_failed_task_creation_keeps_the_shortcut() {
    let mut system = FakeSystem {
        shortcut: true,
        task_create_fails: true,
        ..FakeSystem::default()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.shortcut);
    assert!(!system.calls.contains(&Call::RemoveShortcut));
    assert!(!result.err.is_empty());
}

#[test]
fn a_failed_shortcut_creation_keeps_the_task() {
    let mut system = with_my_task(FakeSystem {
        shortcut_create_fails: true,
        ..FakeSystem::default()
    });
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.my_task());
    assert!(!system.calls.contains(&Call::DeleteTask(me())));
}

#[test]
fn a_failed_removal_is_reported_and_the_other_removal_still_runs() {
    let mut system = with_my_task(FakeSystem {
        shortcut: true,
        shortcut_remove_fails: true,
        ..FakeSystem::default()
    });
    let result = run(&mut system, StartupMode::None);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.my_task());
    assert!(
        !result.out.contains("does not start at logon"),
        "{}",
        result.out
    );

    let mut system = with_my_task(FakeSystem {
        task_delete_fails: true,
        ..FakeSystem::default()
    });
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.shortcut, "the new shortcut is kept");
    assert!(system.my_task());
}

#[test]
fn none_reports_a_task_that_cannot_be_deleted() {
    let mut system = with_my_task(FakeSystem {
        shortcut: true,
        task_delete_fails: true,
        ..FakeSystem::default()
    });
    let result = run(&mut system, StartupMode::None);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.shortcut);
    assert!(system.my_task());
    assert!(
        result.err.contains("could not be deleted"),
        "{}",
        result.err
    );
    assert!(
        !result.out.contains("does not start at logon"),
        "{}",
        result.out
    );
}

#[test]
fn a_shortcut_that_cannot_be_removed_after_the_task_was_created_is_reported() {
    let mut system = FakeSystem {
        shortcut: true,
        shortcut_remove_fails: true,
        ..FakeSystem::default()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.my_task());
    assert!(system.shortcut);
    assert!(
        result.err.contains("could not be removed"),
        "{}",
        result.err
    );
}

#[test]
fn a_failed_task_check_still_sets_up_the_shortcut() {
    // The Task Scheduler cannot be reached: administrator rights would not help.
    let mut system = FakeSystem {
        task_check_error: Some(WinError::new(E_FAIL, "fake task query")),
        ..standard()
    };
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.shortcut);
    assert!(!system.calls.contains(&Call::IsElevated));
    assert!(result.err.contains("task"), "{}", result.err);
}

/// While the task is unknown, `none` cannot say that Raumakey no longer starts at logon.
#[test]
fn none_does_not_claim_success_when_the_task_could_not_be_looked_up() {
    for system in [
        FakeSystem {
            task_check_error: Some(WinError::new(E_FAIL, "fake task query")),
            ..FakeSystem::default()
        },
        FakeSystem {
            user: Err(denied("fake token query")),
            ..FakeSystem::default()
        },
    ] {
        let mut system = FakeSystem {
            shortcut: true,
            ..system
        };
        let result = run(&mut system, StartupMode::None);
        assert_eq!(result.code, EXIT_FAILED, "{result:?}");
        assert!(!system.shortcut);
        assert!(
            !result.out.contains("does not start at logon"),
            "{}",
            result.out
        );
        assert!(
            result.err.contains("could not be looked up"),
            "{}",
            result.err
        );
    }
}

#[test]
fn a_task_that_only_administrators_can_read_is_handled_elevated() {
    for mode in [Shortcut, StartupMode::None] {
        let mut system = FakeSystem {
            shortcut: true,
            task_check_error: Some(denied("fake task query")),
            ..standard()
        };
        let result = run(&mut system, mode);
        assert_eq!(result.code, EXIT_OK, "{mode:?}: {result:?}");
        let shortcut = if mode == Shortcut {
            Call::CreateShortcut
        } else {
            Call::RemoveShortcut
        };
        assert_eq!(
            system.calls,
            vec![
                Call::TaskExists(me()),
                shortcut,
                Call::IsElevated,
                elevated_restart(mode),
            ],
            "{mode:?}"
        );
        assert_eq!(system.shortcut, mode == Shortcut, "{mode:?}");
    }
    // Elevated, it deletes the task, which is idempotent.
    let mut system = with_my_task(FakeSystem {
        task_check_error: Some(denied("fake task query")),
        ..FakeSystem::default()
    });
    let result = run(&mut system, StartupMode::None);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(!system.my_task());
}

// ---------------------------------------------------------------------------------------------
// The elevated restart

#[test]
fn a_declined_elevation_changes_nothing() {
    let mut system = FakeSystem {
        elevated_run: Err(cancelled()),
        shortcut: true,
        ..standard()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.changed_anything());
    assert!(system.shortcut);
    assert!(result.err.contains("nothing was changed"), "{}", result.err);
}

/// Declining the prompt for the task keeps the new shortcut, and the command says that the
/// task may still start Raumakey instead of "nothing was changed".
#[test]
fn a_declined_elevation_for_the_task_keeps_the_new_shortcut() {
    let mut system = with_my_task(FakeSystem {
        elevated_run: Err(cancelled()),
        ..standard()
    });
    let result = run(&mut system, Shortcut);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.shortcut);
    assert!(system.my_task());
    assert!(result.err.contains("not granted"), "{}", result.err);
    assert!(
        result.err.contains("may still start Raumakey at logon"),
        "{}",
        result.err
    );
    assert!(
        !result.err.contains("nothing was changed"),
        "{}",
        result.err
    );
}

/// The restarted process keeps running after the wait ends: what it changes is unknown.
#[test]
fn an_elevated_restart_that_did_not_finish_leaves_the_outcome_unknown() {
    // elevated-task: the task may or may not exist, so the shortcut stays.
    let mut system = FakeSystem {
        elevated_run: Err(timed_out()),
        shortcut: true,
        ..standard()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert!(system.shortcut);
    assert!(!system.calls.contains(&Call::RemoveShortcut));
    assert!(
        result.err.contains("may still be running"),
        "{}",
        result.err
    );

    // none: the shortcut is gone; the task may still exist.
    let mut system = with_my_task(FakeSystem {
        elevated_run: Err(timed_out()),
        shortcut: true,
        ..standard()
    });
    let result = run(&mut system, StartupMode::None);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert!(!system.shortcut);
    assert!(
        !result.out.contains("does not start at logon"),
        "{}",
        result.out
    );
}

#[test]
fn an_elevated_restart_that_cannot_start_changes_nothing() {
    let mut system = FakeSystem {
        elevated_run: Err(WinError::new(E_FAIL, "fake ShellExecuteExW")),
        shortcut: true,
        ..standard()
    };
    let result = run(&mut system, ElevatedTask);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.changed_anything());
    assert!(
        result.err.contains("could not be started as administrator"),
        "{}",
        result.err
    );
}

/// A restarted process that failed, or whose outcome is unknown, created no task that this
/// process knows of, so the shortcut stays.
#[test]
fn the_exit_code_of_the_elevated_restart_is_passed_on() {
    for (child, expected) in [(1, EXIT_FAILED), (3, EXIT_INCOMPLETE)] {
        let mut system = FakeSystem {
            elevated_run: Ok(child),
            shortcut: true,
            ..standard()
        };
        let result = run(&mut system, ElevatedTask);
        assert_eq!(result.code, expected, "{child}: {result:?}");
        assert!(system.shortcut, "{child}");
        assert!(!system.calls.contains(&Call::RemoveShortcut), "{child}");
    }
}

#[test]
fn an_elevated_restart_that_is_still_not_elevated_never_restarts_again() {
    let mut system = standard();
    let result = run_restarted(&mut system, ElevatedTask, &me());
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!restarted(&system));
    assert!(!system.changed_anything());
}
