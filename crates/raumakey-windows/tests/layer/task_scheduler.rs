//! `task_scheduler`: only the existence query of a task that does not exist runs here.

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::task_scheduler::ElevatedLogonTask;

use crate::support::unique_suffix;

#[test]
fn the_product_task_has_a_fixed_name() {
    assert_eq!(ElevatedLogonTask::NAME, "Raumakey Elevated Startup");
    assert_eq!(ElevatedLogonTask::new().name(), "Raumakey Elevated Startup");
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
