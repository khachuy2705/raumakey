//! `task_scheduler`: only the existence query of a task that does not exist runs here.

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::names::{SidHash, sid_hash16};
use raumakey_windows::task_scheduler::ElevatedLogonTask;

use crate::support::unique_suffix;

#[test]
fn the_product_task_has_a_fixed_name() {
    assert_eq!(ElevatedLogonTask::NAME, "Raumakey Elevated Startup");
    assert_eq!(ElevatedLogonTask::new().name(), "Raumakey Elevated Startup");
}

/// The task starts the controller at the logon of one account, so each account has its own,
/// named after its SID hash: the `startup` of one account never replaces or deletes the task
/// of another (matrix #56).
#[test]
fn each_account_has_its_own_task() {
    let first = SidHash::parse("0123456789abcdef").unwrap();
    let second = SidHash::parse("fedcba9876543210").unwrap();
    let task = ElevatedLogonTask::for_user(&first);
    assert_eq!(task.name(), "Raumakey Elevated Startup 0123456789abcdef");
    assert_eq!(task, ElevatedLogonTask::for_user(&first));
    assert_ne!(task, ElevatedLogonTask::for_user(&second));
    assert_ne!(task, ElevatedLogonTask::new());
    // A name that `with_name` accepts as well.
    assert_eq!(ElevatedLogonTask::with_name(task.name()).unwrap(), task);
}

#[test]
fn the_task_of_an_account_without_one_does_not_exist() {
    let account = sid_hash16(&format!("S-1-5-21-0-0-0-{}", unique_suffix()));
    assert!(!ElevatedLogonTask::for_user(&account).exists().unwrap());
}

#[test]
fn task_names_are_validated() {
    for bad in [
        "", "a\\b", "a/b", "a:b", "a*b", "a?b", "a\"b", "a<b", "a>b", "a|b", "a\0b",
    ] {
        assert_eq!(
            ElevatedLogonTask::with_name(bad).unwrap_err().kind(),
            WinErrorKind::InvalidInput,
            "{bad:?}"
        );
    }
    assert!(ElevatedLogonTask::with_name("Raumakey Test").is_ok());
}

#[test]
fn a_missing_task_does_not_exist() {
    let task = ElevatedLogonTask::with_name(&format!("Raumakey Test Missing {}", unique_suffix()))
        .unwrap();
    assert!(!task.exists().unwrap());
}

/// Creates and deletes a scheduled task (needs elevation). Manual test only.
#[test]
#[ignore = "manual: creates a scheduled task; run elevated"]
fn manual_create_and_delete() {
    let task = ElevatedLogonTask::with_name("Raumakey Manual Test Task").unwrap();
    let exe = std::env::current_exe().unwrap();
    task.create(&exe, "--startup").unwrap();
    assert!(task.exists().unwrap());
    assert!(task.delete().unwrap());
    assert!(!task.delete().unwrap());
}
