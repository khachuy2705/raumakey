//! `cargo xtask release-gate`: gate order, the whole-branch review record, and the contract of
//! the portable package check.

use std::fs;
use std::io::Read;
use std::path::Path;

use raumakey_model::identity::PRODUCT_NAME;
use serde::Deserialize;

use crate::error::{Error, Result};
use crate::package::CONTROLLER;
use crate::runner::CommandSpec;

/// Gate steps in execution order.
pub const STEPS: [&str; 7] = [
    "ci",
    "audit",
    "cross",
    "inspect",
    "whole-branch-review",
    "package",
    "portable-check",
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

/// One entry of a package directory, as found on disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageEntry {
    /// Path inside the package, with `/` separators.
    pub path: String,
    /// Whether the entry is a directory.
    pub directory: bool,
    /// Whether the file is executable code: a PE image, or a file with an extension that Windows
    /// runs or loads as code.
    pub executable: bool,
}

/// Extensions of files that Windows runs or loads as code (compared without case).
const EXECUTABLE_EXTENSIONS: [&str; 22] = [
    "bat", "cmd", "com", "cpl", "dll", "exe", "hta", "js", "jse", "lnk", "msc", "msi", "msp",
    "ocx", "ps1", "psm1", "scr", "sys", "vbe", "vbs", "wsf", "wsh",
];

/// Every entry below `dir`, sorted by path.
pub fn scan_package(dir: &Path) -> Result<Vec<PackageEntry>> {
    let mut entries = Vec::new();
    scan_into(dir, "", &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn scan_into(dir: &Path, prefix: &str, entries: &mut Vec<PackageEntry>) -> Result<()> {
    let listing = fs::read_dir(dir).map_err(|error| Error::io("list", dir, error))?;
    for entry in listing {
        let entry = entry.map_err(|error| Error::io("list", dir, error))?;
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let relative = format!("{prefix}{name}");
        let file_type = entry
            .file_type()
            .map_err(|error| Error::io("stat", &path, error))?;
        if file_type.is_dir() {
            scan_into(&path, &format!("{relative}/"), entries)?;
            entries.push(PackageEntry {
                path: relative,
                directory: true,
                executable: false,
            });
        } else {
            entries.push(PackageEntry {
                executable: has_executable_extension(&name) || is_pe_image(&path)?,
                path: relative,
                directory: false,
            });
        }
    }
    Ok(())
}

fn has_executable_extension(name: &str) -> bool {
    name.rsplit_once('.').is_some_and(|(_, extension)| {
        EXECUTABLE_EXTENSIONS
            .iter()
            .any(|known| known.eq_ignore_ascii_case(extension))
    })
}

/// Whether the file starts with the `MZ` signature of a PE image.
fn is_pe_image(path: &Path) -> Result<bool> {
    let mut signature = [0u8; 2];
    let mut file = fs::File::open(path).map_err(|error| Error::io("open", path, error))?;
    let mut read = 0;
    while read < signature.len() {
        match file.read(&mut signature[read..]) {
            Ok(0) => break,
            Ok(count) => read += count,
            Err(error) => return Err(Error::io("read", path, error)),
        }
    }
    Ok(read == signature.len() && signature == *b"MZ")
}

/// Contract violations of a portable package layout: it holds exactly the `expected` files (and
/// the directories they need), and [`CONTROLLER`] is its only executable.
pub fn layout_problems(entries: &[PackageEntry], expected: &[&str]) -> Vec<String> {
    let mut problems = Vec::new();
    let files: Vec<&str> = entries
        .iter()
        .filter(|entry| !entry.directory)
        .map(|entry| entry.path.as_str())
        .collect();
    for path in expected {
        if !files.contains(path) {
            problems.push(format!("missing {path}"));
        }
    }
    for entry in entries {
        if entry.directory {
            let prefix = format!("{}/", entry.path);
            if !expected.iter().any(|path| path.starts_with(&prefix)) {
                problems.push(format!("unexpected directory {prefix}"));
            }
        } else if !expected.contains(&entry.path.as_str()) {
            problems.push(format!("unexpected file {}", entry.path));
        }
    }
    for entry in entries {
        if entry.executable && entry.path != CONTROLLER {
            problems.push(format!(
                "{} is executable code; {CONTROLLER} must be the only executable",
                entry.path
            ));
        }
    }
    problems
}

/// The `--version` invocation of the controller in `package_dir`.
pub fn version_command(package_dir: &Path) -> CommandSpec {
    CommandSpec::new(package_dir.join(CONTROLLER), ["--version"])
}

/// Contract violations of `raumakey.exe --version`: it exits with 0 and prints a line
/// `Raumakey <version>` (optionally followed by more words).
pub fn version_problems(exit_code: Option<i32>, output: &str, version: &str) -> Vec<String> {
    let mut problems = Vec::new();
    match exit_code {
        Some(0) => {}
        Some(code) => problems.push(format!("exit code {code} (expected 0)")),
        None => problems.push("process did not exit normally".to_owned()),
    }
    let expected = format!("{PRODUCT_NAME} {version}");
    let found = output.lines().any(|line| {
        line.trim()
            .strip_prefix(&expected)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    });
    if !found {
        problems.push(format!("output does not contain the line `{expected}`"));
    }
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

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
                "portable-check"
            ]
        );
    }

    #[test]
    fn the_version_check_runs_the_packaged_controller() {
        let dir = Path::new("pkg");
        assert_eq!(
            version_command(dir),
            CommandSpec::new(dir.join("raumakey.exe"), ["--version"])
        );
    }

    #[test]
    fn the_version_line_and_exit_code_zero_pass() {
        for output in [
            "Raumakey 0.1.0\n",
            "Raumakey 0.1.0\r\n",
            "Raumakey 0.1.0",
            "\r\n  Raumakey 0.1.0 (x64)\r\n",
        ] {
            assert!(
                version_problems(Some(0), output, "0.1.0").is_empty(),
                "{output:?}"
            );
        }
    }

    #[test]
    fn exit_status_is_part_of_the_version_contract() {
        let problems = version_problems(Some(2), "Raumakey 0.1.0\n", "0.1.0");
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("exit code 2"), "{problems:?}");
        let problems = version_problems(None, "Raumakey 0.1.0\n", "0.1.0");
        assert_eq!(problems.len(), 1, "{problems:?}");
    }

    #[test]
    fn another_version_or_no_version_line_is_reported() {
        for output in [
            "",
            "raumakey: unknown command\n",
            "Raumakey 0.2.0\n",
            "Raumakey 0.1.01\n",
            "Raumakey 0.1.0-rc.1\n",
            "Raumakey0.1.0\n",
            "raumakey 0.1.0\n",
            "EVKey 0.1.0\n",
        ] {
            let problems = version_problems(Some(0), output, "0.1.0");
            assert_eq!(problems.len(), 1, "{output:?}: {problems:?}");
            assert!(problems[0].contains("Raumakey 0.1.0"), "{problems:?}");
        }
    }

    fn file(path: &str, executable: bool) -> PackageEntry {
        PackageEntry {
            path: path.to_owned(),
            directory: false,
            executable,
        }
    }

    fn directory(path: &str) -> PackageEntry {
        PackageEntry {
            path: path.to_owned(),
            directory: true,
            executable: false,
        }
    }

    const EXPECTED: [&str; 6] = [
        "raumakey.exe",
        "LICENSE",
        "THIRD_PARTY.md",
        "README.txt",
        "manifest.json",
        "SHA256SUMS",
    ];

    fn portable() -> Vec<PackageEntry> {
        EXPECTED
            .iter()
            .map(|path| file(path, *path == "raumakey.exe"))
            .collect()
    }

    #[test]
    fn the_expected_files_with_one_executable_pass() {
        assert!(layout_problems(&portable(), &EXPECTED).is_empty());
    }

    #[test]
    fn missing_and_unexpected_files_are_reported() {
        let mut entries = portable();
        entries.retain(|entry| entry.path != "SHA256SUMS");
        entries.push(file("notes.txt", false));
        assert_eq!(
            layout_problems(&entries, &EXPECTED),
            ["missing SHA256SUMS", "unexpected file notes.txt"]
        );
    }

    #[test]
    fn subdirectories_are_reported_unless_they_hold_an_expected_file() {
        let mut entries = portable();
        entries.push(directory("x64"));
        assert_eq!(
            layout_problems(&entries, &EXPECTED),
            ["unexpected directory x64/"]
        );
        let nested = [directory("docs"), file("docs/README.txt", false)];
        assert!(layout_problems(&nested, &["docs/README.txt"]).is_empty());
    }

    #[test]
    fn raumakey_exe_must_be_the_only_executable() {
        // The old layout's TIP is both unexpected and a second executable.
        let mut entries = portable();
        entries.push(directory("x64"));
        entries.push(file("x64/raumakey_tip.dll", true));
        let problems = layout_problems(&entries, &EXPECTED);
        assert_eq!(problems.len(), 3, "{problems:?}");
        assert!(
            problems.iter().any(|problem| {
                problem.contains("x64/raumakey_tip.dll") && problem.contains("only executable")
            }),
            "{problems:?}"
        );
        // An expected document that holds executable code is reported as well.
        let mut entries = portable();
        entries[3].executable = true;
        let problems = layout_problems(&entries, &EXPECTED);
        assert_eq!(problems.len(), 1, "{problems:?}");
        assert!(problems[0].contains("README.txt"), "{problems:?}");
    }

    #[test]
    fn scan_lists_every_entry_and_flags_executable_code() {
        let dir = std::env::temp_dir().join(format!("raumakey-xtask-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("tools")).expect("temp dir");
        std::fs::write(dir.join("raumakey.exe"), b"MZ\x90\x00").expect("exe");
        std::fs::write(dir.join("README.txt"), "\u{feff}Raumakey").expect("readme");
        std::fs::write(dir.join("LICENSE"), "").expect("empty file");
        // A PE image is executable code whatever its name.
        std::fs::write(dir.join("notes"), b"MZ disguised").expect("pe without extension");
        // A script is executable code by its extension, in any case.
        std::fs::write(dir.join("tools").join("Setup.PS1"), "Write-Host").expect("script");
        let entries = scan_package(&dir);
        std::fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(
            entries.expect("scan"),
            [
                file("LICENSE", false),
                file("README.txt", false),
                file("notes", true),
                file("raumakey.exe", true),
                directory("tools"),
                file("tools/Setup.PS1", true),
            ]
        );
        let error = scan_package(&dir).expect_err("a missing package is an error");
        assert!(error.to_string().contains("raumakey-xtask-scan"), "{error}");
    }
}
