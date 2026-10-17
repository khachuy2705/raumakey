//! `cargo xtask release-gate`: gate order and the installer dry-run contract.

use std::path::Path;

use raumakey_model::identity::{TIP_CLSID, TIP_PROFILE_GUID};

use crate::pe::{LEGACY_EVKEY_GUIDS, contains_guid_text};
use crate::runner::CommandSpec;

/// Gate steps in execution order.
pub const STEPS: [&str; 6] = [
    "ci",
    "audit",
    "cross",
    "inspect",
    "package",
    "installer-dry-run",
];

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

    #[test]
    fn steps_follow_the_documented_order() {
        assert_eq!(
            STEPS,
            [
                "ci",
                "audit",
                "cross",
                "inspect",
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
