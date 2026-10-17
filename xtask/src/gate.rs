//! `cargo xtask release-gate`: gate order and the installer dry-run contract.

use std::path::Path;

use raumakey_model::identity::{TIP_CLSID, TIP_PROFILE_GUID};
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::pe::{LEGACY_EVKEY_GUIDS, contains_guid_text};
use crate::runner::CommandSpec;

/// Gate steps in execution order.
pub const STEPS: [&str; 7] = [
    "ci",
    "audit",
    "cross",
    "inspect",
    "whole-branch-review",
    "package",
    "installer-dry-run",
];

/// Reviewer-supplied attestation; xtask validates its scope and revision, not the review itself.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReviewRecord {
    schema_version: u32,
    scope: String,
    source_revision: String,
    reviewer: String,
    approved: bool,
    unresolved_findings: u32,
}

/// Reject missing, stale, incomplete, or unapproved whole-branch review evidence.
pub fn validate_review(bytes: &[u8], source_revision: Option<&str>) -> Result<()> {
    let revision = source_revision
        .filter(|revision| {
            matches!(revision.len(), 40 | 64) && revision.bytes().all(|b| b.is_ascii_hexdigit())
        })
        .ok_or_else(|| Error::new("whole-branch review requires a known, clean Git revision"))?;
    let review: ReviewRecord = serde_json::from_slice(bytes)
        .map_err(|error| Error::new(format!("invalid whole-branch review record: {error}")))?;
    if review.schema_version != 1
        || review.scope != "whole-branch"
        || review.source_revision != revision
        || review.reviewer.trim().is_empty()
        || !review.approved
        || review.unresolved_findings != 0
    {
        return Err(Error::new(
            "whole-branch review must approve the current revision, name an independent reviewer, \
             use schema version 1 and scope whole-branch, and have no unresolved findings",
        ));
    }
    Ok(())
}

/// The `install --dry-run` and `uninstall --dry-run` invocations of the packaged x64 controller.
pub fn dry_run_commands(package_dir: &Path) -> [CommandSpec; 2] {
    let controller = package_dir.join("raumakey.exe");
    let tip = |arch: &str| {
        package_dir
            .join(arch)
            .join("raumakey_tip.dll")
            .display()
            .to_string()
    };
    [
        CommandSpec::new(
            &controller,
            [
                "install".to_owned(),
                "--dry-run".to_owned(),
                "--tip-x64".to_owned(),
                tip("x64"),
                "--tip-x86".to_owned(),
                tip("x86"),
            ],
        ),
        CommandSpec::new(&controller, ["uninstall", "--dry-run"]),
    ]
}

/// Every run of the installer dry-run step, in order: the controller's own dry runs, then the
/// packaged `install.ps1 -DryRun` and `uninstall.ps1 -DryRun`, run with Windows PowerShell 5.1
/// the way the package README tells users to run them. The scripts' output must meet the same
/// contract as the controller's, so a script that loses the controller's output or exit code
/// fails the gate. Windows PowerShell is taken from `system_root` (`%SystemRoot%`), never from
/// the search path, which starts with the directory of `xtask.exe`.
pub fn installer_dry_run_commands(package_dir: &Path, system_root: &Path) -> [CommandSpec; 4] {
    let powershell = system_root
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe");
    let script = |name: &str| {
        CommandSpec::new(
            &powershell,
            [
                "-NoProfile".to_owned(),
                "-ExecutionPolicy".to_owned(),
                "Bypass".to_owned(),
                "-File".to_owned(),
                package_dir.join(name).display().to_string(),
                "-DryRun".to_owned(),
            ],
        )
    };
    let [install, uninstall] = dry_run_commands(package_dir);
    [
        install,
        uninstall,
        script("install.ps1"),
        script("uninstall.ps1"),
    ]
}

/// Contract violations of one dry run: it must exit with 0, mention the Raumakey CLSID and
/// profile GUID, and never mention a legacy EVKey GUID.
pub fn dry_run_problems(exit_code: Option<i32>, output: &str) -> Vec<String> {
    let mut problems = Vec::new();
    match exit_code {
        Some(0) => {}
        Some(code) => problems.push(format!("exit code {code} (expected 0)")),
        None => problems.push("process did not exit normally".to_owned()),
    }
    for (label, guid) in [
        ("Raumakey CLSID", TIP_CLSID),
        ("Raumakey profile GUID", TIP_PROFILE_GUID),
    ] {
        if !contains_guid_text(output.as_bytes(), guid) {
            problems.push(format!("plan does not mention the {label} {guid}"));
        }
    }
    for (label, guid) in LEGACY_EVKEY_GUIDS {
        if contains_guid_text(output.as_bytes(), guid) {
            problems.push(format!("plan mentions the {label} {guid}"));
        }
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::{LEGACY_EVKEY_GUIDS, guid_text};
    use raumakey_model::identity::{TIP_CLSID, TIP_PROFILE_GUID};

    fn review_record() -> serde_json::Value {
        serde_json::json!({
            "schema_version": 1,
            "scope": "whole-branch",
            "source_revision": "a".repeat(40),
            "reviewer": "independent reviewer",
            "approved": true,
            "unresolved_findings": 0
        })
    }

    #[test]
    fn review_requires_approval_of_the_current_clean_revision() {
        let json = serde_json::to_vec(&review_record()).unwrap();
        let revision = "a".repeat(40);
        assert!(validate_review(&json, Some(&revision)).is_ok());
        assert!(validate_review(&json, None).is_err());
        assert!(validate_review(&json, Some(&"b".repeat(40))).is_err());
        assert!(validate_review(&json, Some(&format!("{revision}-dirty"))).is_err());
        assert!(validate_review(b"invalid json", Some(&revision)).is_err());
    }

    #[test]
    fn incomplete_or_unapproved_reviews_are_rejected() {
        let revision = "a".repeat(40);
        for (field, bad_value) in [
            ("schema_version", serde_json::json!(2)),
            ("scope", serde_json::json!("one-file")),
            ("reviewer", serde_json::json!("  ")),
            ("approved", serde_json::json!(false)),
            ("unresolved_findings", serde_json::json!(1)),
        ] {
            let mut record = review_record();
            record[field] = bad_value;
            assert!(
                validate_review(&serde_json::to_vec(&record).unwrap(), Some(&revision)).is_err(),
                "{field}"
            );
        }
        for field in review_record().as_object().unwrap().keys() {
            let mut record = review_record();
            record.as_object_mut().unwrap().remove(field);
            assert!(
                validate_review(&serde_json::to_vec(&record).unwrap(), Some(&revision)).is_err(),
                "{field}"
            );
        }
    }

    #[test]
    fn steps_follow_the_documented_order() {
        assert_eq!(
            STEPS.as_slice(),
            &[
                "ci",
                "audit",
                "cross",
                "inspect",
                "whole-branch-review",
                "package",
                "installer-dry-run"
            ]
        );
    }

    #[test]
    fn dry_runs_use_the_packaged_controller_and_tips() {
        let dir = Path::new("pkg");
        let [install, uninstall] = dry_run_commands(dir);
        assert_eq!(
            install,
            CommandSpec::new(
                dir.join("raumakey.exe"),
                [
                    "install".to_owned(),
                    "--dry-run".to_owned(),
                    "--tip-x64".to_owned(),
                    dir.join("x64")
                        .join("raumakey_tip.dll")
                        .display()
                        .to_string(),
                    "--tip-x86".to_owned(),
                    dir.join("x86")
                        .join("raumakey_tip.dll")
                        .display()
                        .to_string(),
                ]
            )
        );
        assert_eq!(
            uninstall,
            CommandSpec::new(dir.join("raumakey.exe"), ["uninstall", "--dry-run"])
        );
    }

    #[test]
    fn the_installer_dry_run_runs_the_controller_then_both_scripts_with_windows_powershell() {
        let dir = Path::new("pkg");
        let system_root = Path::new("C:\\Windows");
        let powershell = system_root
            .join("System32")
            .join("WindowsPowerShell")
            .join("v1.0")
            .join("powershell.exe");
        let script = |name: &str| {
            CommandSpec::new(
                &powershell,
                [
                    "-NoProfile".to_owned(),
                    "-ExecutionPolicy".to_owned(),
                    "Bypass".to_owned(),
                    "-File".to_owned(),
                    dir.join(name).display().to_string(),
                    "-DryRun".to_owned(),
                ],
            )
        };
        let [install, uninstall] = dry_run_commands(dir);
        assert_eq!(
            installer_dry_run_commands(dir, system_root),
            [
                install,
                uninstall,
                script("install.ps1"),
                script("uninstall.ps1")
            ]
        );
    }

    fn plan() -> String {
        format!(
            "register CLSID {TIP_CLSID} (64-bit view)\nregister profile {TIP_PROFILE_GUID} \
             LANGID 0x042A\n"
        )
    }

    #[test]
    fn a_plan_naming_both_raumakey_identities_passes() {
        assert!(dry_run_problems(Some(0), &plan()).is_empty());
        let lower = plan().to_ascii_lowercase();
        assert!(
            dry_run_problems(Some(0), &lower).is_empty(),
            "case-insensitive"
        );
    }

    #[test]
    fn exit_status_is_part_of_the_contract() {
        let problems = dry_run_problems(Some(3), &plan());
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains('3'), "{problems:?}");
        let problems = dry_run_problems(None, &plan());
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn missing_identities_are_reported() {
        let problems = dry_run_problems(Some(0), "nothing to do\n");
        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].contains(&guid_text(TIP_CLSID)), "{problems:?}");
        assert!(
            problems[1].contains(&guid_text(TIP_PROFILE_GUID)),
            "{problems:?}"
        );
    }

    #[test]
    fn legacy_identities_are_rejected() {
        for (label, guid) in LEGACY_EVKEY_GUIDS {
            let output = format!("{}\nskip {guid}\n", plan());
            let problems = dry_run_problems(Some(0), &output);
            assert_eq!(problems.len(), 1, "{problems:?}");
            assert!(problems[0].contains(label), "{problems:?}");
        }
    }
}
