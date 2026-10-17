//! `install` and `uninstall` (plan §10.2, §10.4; design §12, §13) against the in-memory fake
//! of `support`: dry runs, elevation, registration with the regsvr32 of each TIP's bitness,
//! verification, the installation record, and rollback.
//!
//! The file is not called `install.rs`: Windows treats a 32-bit executable without a manifest
//! whose name contains "install", "setup", "update", or "patch" as an installer and refuses to
//! start it without elevation (UAC installer detection), which would break the i686 test run.
//! Test files of this package avoid those words.

mod support;

use std::path::{Path, PathBuf};

use raumakey_controller::cli::{EXIT_FAILED, EXIT_INCOMPLETE, EXIT_OK, EXIT_USAGE};
use raumakey_controller::install::{
    INSTALL_RECORD_KEY, InstallRecord, PACKAGE_ARCHS, TIP_FILE_NAME, default_tip_path, install,
    print_install_plan, print_uninstall_plan, uninstall,
};
use raumakey_model::identity::{Guid, TIP_CLSID, TIP_PROFILE_GUID};
use raumakey_windows::error::{E_FAIL, WinError};
use raumakey_windows::tsf_registration::{TIP_CATEGORIES, TipArch, TipBinary};

use support::{
    Call, Captured, FakeSystem, Outcome, ViewState, cancelled, capture, denied, guids_in,
    timed_out, tip,
};

const X64: &str = "C:\\pkg\\x64\\raumakey_tip.dll";
const X86: &str = "C:\\pkg\\x86\\raumakey_tip.dll";
const OLD_X64: &str = "C:\\old\\x64\\raumakey_tip.dll";
const OLD_X86: &str = "C:\\old\\x86\\raumakey_tip.dll";

fn tips() -> Vec<TipBinary> {
    vec![tip(TipArch::X64, X64), tip(TipArch::X86, X86)]
}

/// An elevated machine with both package TIPs.
fn machine() -> FakeSystem {
    FakeSystem::with_files(&[Path::new(X64), Path::new(X86)])
}

fn run_install(system: &mut FakeSystem) -> Captured {
    capture(|output| install(system, &tips(), false, output))
}

fn run(arch: TipArch, path: &str, unregister: bool) -> (TipArch, PathBuf, bool) {
    (arch, PathBuf::from(path), unregister)
}

/// Raumakey's GUIDs and the Windows SDK category GUIDs of plan §10.4.
fn allowed_guids() -> Vec<Guid> {
    let mut allowed = vec![TIP_CLSID, TIP_PROFILE_GUID];
    allowed.extend(TIP_CATEGORIES.iter().map(|category| category.guid));
    allowed
}

fn assert_only_raumakey_guids(text: &str) {
    let allowed = allowed_guids();
    for guid in guids_in(text) {
        assert!(
            allowed.contains(&guid),
            "unexpected GUID {guid} in:\n{text}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Dry runs

#[test]
fn the_install_dry_run_prints_the_plan_with_the_raumakey_identities() {
    let result = capture(|output| print_install_plan(&tips(), output));
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(result.err.is_empty(), "{}", result.err);
    let plan = &result.out;
    assert!(plan.contains(&TIP_CLSID.to_string()), "{plan}");
    assert!(plan.contains(&TIP_PROFILE_GUID.to_string()), "{plan}");
    for category in &TIP_CATEGORIES {
        assert!(plan.contains(category.name), "{plan}");
    }
    assert!(plan.contains(X64) && plan.contains(X86), "{plan}");
    assert!(plan.contains("64-bit view") && plan.contains("32-bit view"));
    assert!(plan.contains(INSTALL_RECORD_KEY), "{plan}");
    assert!(plan.to_lowercase().contains("dry run"), "{plan}");
    assert_only_raumakey_guids(plan);
}

#[test]
fn the_install_dry_run_rejects_paths_that_cannot_be_registered() {
    for tips in [
        vec![
            tip(TipArch::X64, "C:\\pkg\\x64\\raumakey_tip.txt"),
            tip(TipArch::X86, X86),
        ],
        vec![
            tip(TipArch::X64, "x64\\raumakey_tip.dll"),
            tip(TipArch::X86, X86),
        ],
    ] {
        let result = capture(|output| print_install_plan(&tips, output));
        assert_eq!(result.code, EXIT_USAGE, "{result:?}");
        assert!(result.out.is_empty(), "{}", result.out);
        assert!(result.err.contains("x64"), "{}", result.err);
    }
}

#[test]
fn the_uninstall_dry_run_prints_the_removal_plan_of_both_architectures() {
    let result = capture(print_uninstall_plan);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(result.err.is_empty(), "{}", result.err);
    let plan = &result.out;
    assert!(plan.contains(&TIP_CLSID.to_string()), "{plan}");
    assert!(plan.contains(&TIP_PROFILE_GUID.to_string()), "{plan}");
    assert!(plan.contains("[x64]") && plan.contains("[x86]"), "{plan}");
    assert!(plan.contains(INSTALL_RECORD_KEY), "{plan}");
    assert!(plan.to_lowercase().contains("dry run"), "{plan}");
    assert_only_raumakey_guids(plan);
}

#[test]
fn the_package_layout_holds_an_x64_and_an_x86_tip() {
    assert_eq!(PACKAGE_ARCHS, [TipArch::X64, TipArch::X86]);
    assert_eq!(TIP_FILE_NAME, "raumakey_tip.dll");
    let dir = Path::new("C:\\pkg");
    assert_eq!(
        default_tip_path(dir, TipArch::X64),
        dir.join("x64").join("raumakey_tip.dll")
    );
    assert_eq!(
        default_tip_path(dir, TipArch::X86),
        dir.join("x86").join("raumakey_tip.dll")
    );
}

// ---------------------------------------------------------------------------------------------
// The installation record

#[test]
fn the_record_holds_the_version_and_one_path_per_architecture() {
    let record = InstallRecord::new(&tips());
    assert_eq!(record.version, env!("CARGO_PKG_VERSION"));
    assert_eq!(record.tip_path(TipArch::X64), Some(Path::new(X64)));
    assert_eq!(record.tip_path(TipArch::X86), Some(Path::new(X86)));
    assert_eq!(record.tip_path(TipArch::Arm64), None);
    let values = record.to_values();
    assert_eq!(
        values,
        vec![
            ("Version", env!("CARGO_PKG_VERSION").to_owned()),
            ("TipX64", X64.to_owned()),
            ("TipX86", X86.to_owned()),
        ]
    );
    let read = InstallRecord::from_values(|name| {
        values
            .iter()
            .find(|(value, _)| *value == name)
            .map(|(_, text)| text.clone())
    });
    assert_eq!(read, record);
    assert_eq!(
        InstallRecord::from_values(|_| None),
        InstallRecord {
            version: String::new(),
            tips: Vec::new()
        }
    );
}

// ---------------------------------------------------------------------------------------------
// install

#[test]
fn install_registers_both_tips_verifies_them_and_records_the_installation() {
    let mut system = machine();
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![run(TipArch::X64, X64, false), run(TipArch::X86, X86, false)]
    );
    assert_eq!(system.view64, ViewState::registered(Path::new(X64)));
    assert_eq!(system.view32, ViewState::registered(Path::new(X86)));
    assert_eq!(system.record, Some(InstallRecord::new(&tips())));
    // The record is written last, after the registrations were read back.
    assert!(matches!(system.calls.last(), Some(Call::WriteRecord(_))));
    assert!(result.err.is_empty(), "{}", result.err);
    // The verification report: one line per check, all passed.
    assert!(
        result.out.contains("[x86] language profile"),
        "{}",
        result.out
    );
    assert!(!result.out.contains("FAILED"), "{}", result.out);
    assert!(result.out.contains(INSTALL_RECORD_KEY), "{}", result.out);
}

#[test]
fn installing_again_is_idempotent() {
    let mut system = machine();
    assert_eq!(run_install(&mut system).code, EXIT_OK);
    let views = (system.view64.clone(), system.view32.clone());
    let record = system.record.clone();
    assert_eq!(run_install(&mut system).code, EXIT_OK);
    assert_eq!((system.view64.clone(), system.view32.clone()), views);
    assert_eq!(system.record, record);
}

#[test]
fn without_administrator_rights_install_restarts_itself_elevated() {
    for (child, expected) in [
        (0, EXIT_OK),
        (1, EXIT_FAILED),
        (3, EXIT_INCOMPLETE),
        (0xC000_0005, EXIT_FAILED),
    ] {
        let mut system = machine();
        system.elevated = Ok(false);
        system.elevated_run = Ok(child);
        let result = run_install(&mut system);
        assert_eq!(result.code, expected, "{child:#x}: {result:?}");
        assert_eq!(
            system.calls,
            vec![
                Call::IsElevated,
                Call::RunElevated(
                    ["install", "--tip-x64", X64, "--tip-x86", X86, "--elevated"]
                        .map(str::to_owned)
                        .to_vec()
                ),
            ]
        );
        assert!(
            result.out.to_lowercase().contains("administrator"),
            "{}",
            result.out
        );
    }
}

#[test]
fn a_declined_elevation_changes_nothing() {
    let mut system = machine();
    system.elevated = Ok(false);
    system.elevated_run = Err(cancelled());
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.changed_anything());
    assert!(result.err.contains("not granted"), "{}", result.err);
}

/// The elevated process keeps running after the wait ends, and may still change the machine:
/// the outcome is unknown, which is not "changed nothing" (exit code 1).
#[test]
fn an_elevated_restart_that_did_not_finish_leaves_the_outcome_unknown() {
    let mut system = machine();
    system.elevated = Ok(false);
    system.elevated_run = Err(timed_out());
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert!(
        result.err.contains("may still be running"),
        "{}",
        result.err
    );
    assert!(
        !result.err.contains("nothing was changed"),
        "{}",
        result.err
    );
    assert!(!system.changed_anything());

    let mut system = installed_from_old_folder();
    system.elevated = Ok(false);
    system.elevated_run = Err(timed_out());
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert!(
        result.err.contains("may still be running"),
        "{}",
        result.err
    );
}

#[test]
fn an_elevated_restart_that_cannot_start_changes_nothing() {
    let mut system = machine();
    system.elevated = Ok(false);
    system.elevated_run = Err(WinError::new(E_FAIL, "fake ShellExecuteExW"));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(
        result.err.contains("could not be started as administrator"),
        "{}",
        result.err
    );
    assert!(result.err.contains("nothing was changed"), "{}", result.err);
    assert!(!system.changed_anything());
}

#[test]
fn an_elevated_restart_that_is_still_not_elevated_never_restarts_again() {
    let mut system = machine();
    system.elevated = Ok(false);
    let result = capture(|output| install(&mut system, &tips(), true, output));
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(system.calls, vec![Call::IsElevated]);
    assert!(
        result.err.to_lowercase().contains("administrator"),
        "{}",
        result.err
    );
}

#[test]
fn a_failed_elevation_check_changes_nothing() {
    let mut system = machine();
    system.elevated = Err(denied("fake token query"));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(system.calls, vec![Call::IsElevated]);
}

#[test]
fn invalid_tip_paths_are_usage_errors_before_anything_else() {
    let mut system = machine();
    let tips = vec![
        tip(TipArch::X64, X64),
        tip(TipArch::X86, "C:\\pkg\\x86\\raumakey_tip.exe"),
    ];
    let result = capture(|output| install(&mut system, &tips, false, output));
    assert_eq!(result.code, EXIT_USAGE, "{result:?}");
    assert!(system.calls.is_empty(), "{:?}", system.calls);
    assert!(result.err.contains("x86"), "{}", result.err);
}

#[test]
fn a_missing_tip_file_fails_before_any_change() {
    let mut system = FakeSystem::with_files(&[Path::new(X64)]);
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(!system.changed_anything(), "{:?}", system.calls);
    assert!(result.err.contains(X86), "{}", result.err);
}

/// A mistyped path or an incomplete package costs no UAC prompt: checking that a file exists
/// needs no administrator rights. (The elevated restart runs `install` again and checks again.)
#[test]
fn a_missing_tip_file_is_found_before_asking_for_administrator_rights() {
    let mut system = FakeSystem::with_files(&[Path::new(X64)]);
    system.elevated = Ok(false);
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(system.calls, vec![], "no elevation check, no restart");
    assert!(result.err.contains(X86), "{}", result.err);
    assert!(result.err.contains("nothing was changed"), "{}", result.err);
}

// ---------------------------------------------------------------------------------------------
// TIPs outside the folders that only administrators can change

/// Every application loads the TIP, also those that run as administrator: a TIP that the
/// user's programs can replace is a way into them. Installing goes on, with a warning.
#[test]
fn install_warns_about_a_tip_outside_the_protected_folders() {
    let mut system = machine();
    system.unprotected.insert(PathBuf::from(X86));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(result.err.contains("warning"), "{}", result.err);
    assert!(result.err.contains(X86), "{}", result.err);
    assert!(!result.err.contains(X64), "{}", result.err);
    assert!(result.err.contains("Program Files"), "{}", result.err);
    assert_eq!(system.view32, ViewState::registered(Path::new(X86)));
}

/// The warning appears before the UAC prompt, where the user sees it; the elevated restart,
/// whose output may not reach the console, does not repeat it.
#[test]
fn the_location_warning_comes_before_the_elevated_restart_and_only_once() {
    let mut system = machine();
    system.elevated = Ok(false);
    system.unprotected.insert(PathBuf::from(X64));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(result.err.contains("warning"), "{}", result.err);
    assert!(result.err.contains(X64), "{}", result.err);
    assert!(
        matches!(system.calls.last(), Some(Call::RunElevated(_))),
        "{:?}",
        system.calls
    );

    let mut system = machine();
    system.unprotected.insert(PathBuf::from(X64));
    let result = capture(|output| install(&mut system, &tips(), true, output));
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(!result.err.contains("warning"), "{}", result.err);
}

#[test]
fn unreadable_state_fails_before_any_change() {
    let mut system = machine();
    system.record_read_fails = true;
    assert_eq!(run_install(&mut system).code, EXIT_FAILED);
    assert!(!system.changed_anything(), "{:?}", system.calls);

    let mut system = machine();
    system.registration_read_fails = true;
    assert_eq!(run_install(&mut system).code, EXIT_FAILED);
    assert!(!system.changed_anything(), "{:?}", system.calls);
}

#[test]
fn a_failed_registration_undoes_the_registrations_made_before_it() {
    let mut system = machine();
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X64, X64, true),
        ]
    );
    assert!(system.view64.is_empty() && system.view32.is_empty());
    assert_eq!(system.record, None);
    assert!(
        !system
            .calls
            .iter()
            .any(|call| matches!(call, Call::WriteRecord(_)))
    );
    assert!(result.err.contains("x86"), "{}", result.err);
    assert!(result.err.contains('5'), "{}", result.err);
}

#[test]
fn a_regsvr32_that_cannot_run_is_rolled_back_cleanly() {
    let mut system = machine();
    system.regsvr32.insert(
        (TipArch::X86, false),
        Outcome::Error(denied("fake CreateProcessW")),
    );
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.view64.is_empty() && system.view32.is_empty());
    // The view of the TIP that failed was read back and found unchanged: nothing to undo.
    assert!(
        !system
            .regsvr32_runs()
            .contains(&run(TipArch::X86, X86, true)),
        "{:?}",
        system.calls
    );
}

/// regsvr32 may have registered the TIP before waiting for it or reading its exit code
/// failed, so its view is read back and restored like that of a failed registration.
#[test]
fn a_registration_whose_exit_code_was_lost_is_read_back_and_undone() {
    let mut system = machine();
    system.regsvr32.insert(
        (TipArch::X86, false),
        Outcome::Unread(denied("fake GetExitCodeProcess")),
    );
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X86, X86, true),
            run(TipArch::X64, X64, true),
        ]
    );
    assert!(system.view64.is_empty() && system.view32.is_empty());
}

#[test]
fn a_failed_verification_undoes_every_registration_in_reverse_order() {
    let mut system = machine();
    system.without_profile.insert(TipArch::X86);
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X86, X86, true),
            run(TipArch::X64, X64, true),
        ]
    );
    assert!(system.view64.is_empty() && system.view32.is_empty());
    assert_eq!(system.record, None);
    let report = format!("{}{}", result.out, result.err);
    assert!(report.contains("FAILED"), "{report}");
}

#[test]
fn a_failed_record_write_undoes_the_registrations() {
    let mut system = machine();
    system.record_write_failures = 1;
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.view64.is_empty() && system.view32.is_empty());
    assert_eq!(system.record, None);
    assert!(result.err.contains(INSTALL_RECORD_KEY), "{}", result.err);
}

#[test]
fn a_failed_upgrade_restores_the_previous_installation() {
    let mut system = FakeSystem::with_files(&[
        Path::new(X64),
        Path::new(X86),
        Path::new(OLD_X64),
        Path::new(OLD_X86),
    ]);
    system.view64 = ViewState::registered(Path::new(OLD_X64));
    system.view32 = ViewState::registered(Path::new(OLD_X86));
    let previous = InstallRecord {
        version: "0.0.9".to_owned(),
        tips: vec![tip(TipArch::X64, OLD_X64), tip(TipArch::X86, OLD_X86)],
    };
    system.record = Some(previous.clone());
    system.record_write_failures = 1;
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    // Each architecture is registered again from the DLL it used before.
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X86, OLD_X86, false),
            run(TipArch::X64, OLD_X64, false),
        ]
    );
    assert_eq!(system.view64, ViewState::registered(Path::new(OLD_X64)));
    assert_eq!(system.view32, ViewState::registered(Path::new(OLD_X86)));
    assert_eq!(system.record, Some(previous));
}

#[test]
fn a_registration_from_the_same_dll_is_kept_by_the_rollback() {
    let mut system = machine();
    system.view64 = ViewState::registered(Path::new(X64));
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![run(TipArch::X64, X64, false), run(TipArch::X86, X86, false)]
    );
    assert_eq!(system.view64, ViewState::registered(Path::new(X64)));
    assert!(system.view32.is_empty());
}

#[test]
fn a_previous_dll_that_is_gone_leaves_the_rollback_incomplete() {
    let mut system = machine();
    system.view64 = ViewState::registered(Path::new(OLD_X64));
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    // The new registration stays: it works, and the previous DLL cannot be registered again.
    assert_eq!(system.view64, ViewState::registered(Path::new(X64)));
    assert!(result.err.contains(OLD_X64), "{}", result.err);
}

#[test]
fn a_previous_registration_that_cannot_be_restored_leaves_the_rollback_incomplete() {
    for outcome in [
        Outcome::Exit(5),
        Outcome::Error(denied("fake CreateProcessW")),
    ] {
        let mut system =
            FakeSystem::with_files(&[Path::new(X64), Path::new(X86), Path::new(OLD_X64)]);
        system.view64 = ViewState::registered(Path::new(OLD_X64));
        system
            .regsvr32
            .insert((TipArch::X86, false), Outcome::Exit(5));
        system
            .regsvr32_dll
            .insert((PathBuf::from(OLD_X64), false), outcome);
        let result = run_install(&mut system);
        assert_eq!(result.code, EXIT_INCOMPLETE, "{outcome:?}: {result:?}");
        assert!(
            system
                .regsvr32_runs()
                .contains(&run(TipArch::X64, OLD_X64, false)),
            "{outcome:?}: {:?}",
            system.calls
        );
        assert!(
            result.err.contains("[x64] could not be restored"),
            "{outcome:?}: {}",
            result.err
        );
    }
}

/// A regsvr32 exit code is never proof (clean-room spec §6.1): the rollback reads every
/// restored view back.
#[test]
fn an_unregistration_that_changed_nothing_leaves_the_rollback_incomplete() {
    let mut system = machine();
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(5));
    system
        .regsvr32
        .insert((TipArch::X64, true), Outcome::Exit(0));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert_eq!(system.view64, ViewState::registered(Path::new(X64)));
    assert!(result.err.contains("[x64]"), "{}", result.err);
}

#[test]
fn a_registration_again_that_changed_nothing_leaves_the_rollback_incomplete() {
    let mut system = FakeSystem::with_files(&[Path::new(X64), Path::new(X86), Path::new(OLD_X64)]);
    system.view64 = ViewState::registered(Path::new(OLD_X64));
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(5));
    system
        .regsvr32_dll
        .insert((PathBuf::from(OLD_X64), false), Outcome::Exit(0));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    // The run's own registration is still there: the previous one was not restored.
    assert_eq!(system.view64, ViewState::registered(Path::new(X64)));
    assert!(result.err.contains("[x64]"), "{}", result.err);
}

#[test]
fn a_failed_unregistration_leaves_the_rollback_incomplete() {
    let mut system = machine();
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(5));
    system
        .regsvr32
        .insert((TipArch::X64, true), Outcome::Exit(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert_eq!(system.view64, ViewState::registered(Path::new(X64)));
}

#[test]
fn a_registration_that_did_not_finish_leaves_the_rollback_incomplete() {
    let mut system = machine();
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Error(timed_out()));
    let result = run_install(&mut system);
    // The x64 registration is undone, but the state of the x86 one is unknown.
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert!(system.view64.is_empty());
}

/// The TIP undoes a failed registration itself, but it ignores errors of that undo; what is
/// left in a view that had no Raumakey registration before is removed as well.
#[test]
fn entries_left_by_a_failed_registration_are_removed() {
    let mut system = machine();
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Partial(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X86, X86, true),
            run(TipArch::X64, X64, true),
        ]
    );
    assert!(system.view64.is_empty() && system.view32.is_empty());
}

#[test]
fn entries_left_by_a_failed_registration_that_stay_make_the_rollback_incomplete() {
    let mut system = machine();
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Partial(5));
    system
        .regsvr32
        .insert((TipArch::X86, true), Outcome::Exit(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert!(system.view64.is_empty());
    assert!(!system.view32.is_empty());
}

#[test]
fn a_failed_registration_that_changed_nothing_needs_no_cleanup() {
    // A DLL that cannot be loaded (regsvr32 exit code 3) changes nothing.
    let mut system = machine();
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(3));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(
        !system
            .regsvr32_runs()
            .contains(&run(TipArch::X86, X86, true)),
        "{:?}",
        system.calls
    );
}

#[test]
fn a_failed_registration_that_left_the_previous_one_intact_needs_no_cleanup() {
    let mut system = FakeSystem::with_files(&[Path::new(X64), Path::new(X86), Path::new(OLD_X86)]);
    system.view32 = ViewState::registered(Path::new(OLD_X86));
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Exit(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X64, X64, true),
        ]
    );
    assert_eq!(system.view32, ViewState::registered(Path::new(OLD_X86)));
    assert!(system.view64.is_empty());
}

/// The view was registered from the very DLL that just failed, and the failure removed that
/// registration: running the failing DLL again cannot restore it.
#[test]
fn a_failed_registration_that_removed_the_same_dlls_registration_leaves_the_rollback_incomplete() {
    let mut system = machine();
    system.view32 = ViewState::registered(Path::new(X86));
    system
        .regsvr32
        .insert((TipArch::X86, false), Outcome::Removed(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X64, X64, true),
        ]
    );
    assert!(system.view32.is_empty());
    assert!(system.view64.is_empty());
}

#[test]
fn a_failed_registration_over_another_dll_registers_that_dll_again() {
    let mut system = FakeSystem::with_files(&[Path::new(X64), Path::new(X86), Path::new(OLD_X86)]);
    system.view32 = ViewState::registered(Path::new(OLD_X86));
    system
        .regsvr32_dll
        .insert((PathBuf::from(X86), false), Outcome::Partial(5));
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, X64, false),
            run(TipArch::X86, X86, false),
            run(TipArch::X86, OLD_X86, false),
            run(TipArch::X64, X64, true),
        ]
    );
    assert_eq!(system.view32, ViewState::registered(Path::new(OLD_X86)));
    assert!(system.view64.is_empty());
}

#[test]
fn a_record_that_cannot_be_restored_leaves_the_rollback_incomplete() {
    let mut system = machine();
    system.record = Some(InstallRecord {
        version: "0.0.9".to_owned(),
        tips: tips(),
    });
    system.record_write_failures = 2;
    let result = run_install(&mut system);
    assert_eq!(result.code, EXIT_INCOMPLETE, "{result:?}");
}

// ---------------------------------------------------------------------------------------------
// uninstall

fn exe_dir() -> &'static Path {
    Path::new("C:\\pkg")
}

fn run_uninstall(system: &mut FakeSystem) -> Captured {
    capture(|output| uninstall(system, exe_dir(), false, output))
}

/// A machine where the old installation is registered and recorded.
fn installed_from_old_folder() -> FakeSystem {
    let mut system = FakeSystem::with_files(&[Path::new(OLD_X64), Path::new(OLD_X86)]);
    system.view64 = ViewState::registered(Path::new(OLD_X64));
    system.view32 = ViewState::registered(Path::new(OLD_X86));
    system.record = Some(InstallRecord {
        version: "0.0.9".to_owned(),
        tips: vec![tip(TipArch::X64, OLD_X64), tip(TipArch::X86, OLD_X86)],
    });
    system
}

#[test]
fn uninstall_unregisters_the_recorded_tips_and_removes_the_record() {
    let mut system = installed_from_old_folder();
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![
            run(TipArch::X64, OLD_X64, true),
            run(TipArch::X86, OLD_X86, true)
        ]
    );
    assert!(system.view64.is_empty() && system.view32.is_empty());
    assert_eq!(system.record, None);
    assert!(matches!(system.calls.last(), Some(Call::RemoveRecord)));
    assert!(result.err.is_empty(), "{}", result.err);
}

#[test]
fn uninstall_uses_the_tips_next_to_raumakey_when_the_recorded_ones_are_gone() {
    let x64 = default_tip_path(exe_dir(), TipArch::X64);
    let x86 = default_tip_path(exe_dir(), TipArch::X86);
    let mut system = FakeSystem::with_files(&[&x64, &x86]);
    system.view64 = ViewState::registered(Path::new(OLD_X64));
    system.view32 = ViewState::registered(Path::new(OLD_X86));
    system.record = Some(InstallRecord {
        version: "0.0.9".to_owned(),
        tips: vec![tip(TipArch::X64, OLD_X64)],
    });
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![(TipArch::X64, x64, true), (TipArch::X86, x86, true)]
    );
    assert!(system.view64.is_empty() && system.view32.is_empty());
}

#[test]
fn uninstall_leaves_alone_what_is_not_registered() {
    let mut system = installed_from_old_folder();
    system.view32 = ViewState::default();
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(
        system.regsvr32_runs(),
        vec![run(TipArch::X64, OLD_X64, true)]
    );
    assert_eq!(system.record, None);
}

#[test]
fn uninstalling_twice_is_idempotent() {
    let mut system = installed_from_old_folder();
    assert_eq!(run_uninstall(&mut system).code, EXIT_OK);
    let runs = system.regsvr32_runs().len();
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(system.regsvr32_runs().len(), runs, "nothing left to remove");
    assert!(result.out.contains("not registered"), "{}", result.out);
}

#[test]
fn uninstall_without_a_tip_dll_fails_and_keeps_the_record() {
    let mut system = installed_from_old_folder();
    system.files.clear();
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.regsvr32_runs().is_empty());
    assert!(system.record.is_some());
    assert!(!system.calls.contains(&Call::RemoveRecord));
}

#[test]
fn a_failed_unregistration_keeps_the_record_for_the_next_attempt() {
    let mut system = installed_from_old_folder();
    system
        .regsvr32
        .insert((TipArch::X86, true), Outcome::Exit(5));
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.view64.is_empty());
    assert!(!system.view32.is_empty());
    assert!(system.record.is_some());
    let report = format!("{}{}", result.out, result.err);
    assert!(report.contains("still present"), "{report}");
}

#[test]
fn an_unregistration_that_cannot_run_keeps_the_record() {
    let mut system = installed_from_old_folder();
    system.regsvr32.insert(
        (TipArch::X86, true),
        Outcome::Error(denied("fake CreateProcessW")),
    );
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.view64.is_empty());
    assert!(!system.view32.is_empty());
    assert!(system.record.is_some());
    assert!(!system.calls.contains(&Call::RemoveRecord));
    assert!(
        result.err.contains("could not unregister the x86 TIP"),
        "{}",
        result.err
    );
}

#[test]
fn uninstall_says_whether_there_was_an_installation_record() {
    let result = run_uninstall(&mut FakeSystem::default());
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(
        result.out.contains("No installation record"),
        "{}",
        result.out
    );
    assert!(!result.out.contains("Removed the installation record"));

    let result = run_uninstall(&mut installed_from_old_folder());
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(
        result.out.contains("Removed the installation record"),
        "{}",
        result.out
    );
    assert!(!result.out.contains("No installation record"));
}

#[test]
fn without_administrator_rights_uninstall_restarts_itself_elevated() {
    let mut system = installed_from_old_folder();
    system.elevated = Ok(false);
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert_eq!(
        system.calls,
        vec![
            Call::IsElevated,
            Call::RunElevated(vec!["uninstall".to_owned(), "--elevated".to_owned()]),
        ]
    );
}

#[test]
fn an_unreadable_record_does_not_block_uninstall() {
    let x64 = default_tip_path(exe_dir(), TipArch::X64);
    let x86 = default_tip_path(exe_dir(), TipArch::X86);
    let mut system = FakeSystem::with_files(&[&x64, &x86]);
    system.view64 = ViewState::registered(&x64);
    system.view32 = ViewState::registered(&x86);
    system.record_read_fails = true;
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_OK, "{result:?}");
    assert!(system.view64.is_empty() && system.view32.is_empty());
    assert!(system.calls.contains(&Call::RemoveRecord));
    assert!(result.err.contains("record"), "{}", result.err);
}

#[test]
fn a_record_that_cannot_be_removed_fails_the_uninstall() {
    let mut system = installed_from_old_folder();
    system.record_remove_fails = true;
    let result = run_uninstall(&mut system);
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert!(system.view64.is_empty() && system.view32.is_empty());
    assert!(result.err.contains(INSTALL_RECORD_KEY), "{}", result.err);
}
