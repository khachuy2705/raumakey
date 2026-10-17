//! Release-gate evidence document written to
//! `<target-dir>/release-evidence/release-gate-<unix-seconds>.json`.

use raumakey_model::identity::PRODUCT_NAME;
use serde::Serialize;

use crate::error::Result;
use crate::runner::CommandRecord;

/// Verification level reported when every gate passed (design §19).
pub const CROSS_BUILD_VERIFIED: &str = "cross-build verified";
/// Verification level reported when any gate failed or did not run.
pub const NOT_VERIFIED: &str = "not verified";

/// Status of one gate step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StepStatus {
    /// The step ran and passed.
    Passed,
    /// The step ran and failed.
    Failed,
    /// The step did not run because an earlier step failed.
    NotRun,
}

/// Evidence of one gate step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StepEvidence {
    /// Step name, e.g. `cross`.
    pub name: &'static str,
    /// Outcome.
    pub status: StepStatus,
    /// Wall-clock duration in milliseconds.
    pub duration_ms: u64,
    /// Failure reason.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Commands the step ran, with exit status and duration.
    pub commands: Vec<CommandRecord>,
    /// Notices such as skipped optional work.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// A hashed artifact produced by a passing step.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ArtifactEvidence {
    /// Path relative to the Cargo target directory, with `/` separators.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// Lower-case hexadecimal SHA-256.
    pub sha256: String,
}

/// Host that ran the gate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct HostInfo {
    /// `std::env::consts::OS`.
    pub os: &'static str,
    /// `std::env::consts::ARCH`.
    pub arch: &'static str,
}

/// The release-gate evidence document.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Evidence {
    /// Document schema version.
    pub schema_version: u32,
    /// Product name.
    pub product: &'static str,
    /// Product version.
    pub version: String,
    /// Creation time (seconds since the Unix epoch).
    pub generated_unix_seconds: u64,
    /// Host that ran the gate.
    pub host: HostInfo,
    /// `git rev-parse HEAD` (with `-dirty` when the tree had changes), when available.
    pub source_revision: Option<String>,
    /// Whether every step passed.
    pub passed: bool,
    /// `cross-build verified` or `not verified`; never a runtime claim.
    pub verification_level: &'static str,
    /// Always false: runtime verification needs the Windows acceptance harness.
    pub runtime_verified: bool,
    /// Always false until a signing certificate exists.
    pub signed: bool,
    /// Steps in execution order.
    pub steps: Vec<StepEvidence>,
    /// Artifacts produced by passing steps.
    pub artifacts: Vec<ArtifactEvidence>,
}

impl Evidence {
    /// Builds the document; `passed` and `verification_level` follow from `steps`.
    pub fn new(
        version: String,
        generated_unix_seconds: u64,
        source_revision: Option<String>,
        steps: Vec<StepEvidence>,
        artifacts: Vec<ArtifactEvidence>,
    ) -> Self {
        let passed =
            !steps.is_empty() && steps.iter().all(|step| step.status == StepStatus::Passed);
        Self {
            schema_version: 1,
            product: PRODUCT_NAME,
            version,
            generated_unix_seconds,
            host: HostInfo {
                os: std::env::consts::OS,
                arch: std::env::consts::ARCH,
            },
            source_revision,
            passed,
            verification_level: if passed {
                CROSS_BUILD_VERIFIED
            } else {
                NOT_VERIFIED
            },
            runtime_verified: false,
            signed: false,
            steps,
            artifacts,
        }
    }

    /// Pretty-printed JSON with a trailing newline.
    pub fn to_json(&self) -> Result<String> {
        let mut text = serde_json::to_string_pretty(self)?;
        text.push('\n');
        Ok(text)
    }
}

/// File name of the evidence written at `unix_seconds`.
pub fn evidence_file_name(unix_seconds: u64) -> String {
    format!("release-gate-{unix_seconds}.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(command: &str, exit_code: Option<i32>) -> CommandRecord {
        CommandRecord {
            command: command.to_owned(),
            exit_code,
            success: exit_code == Some(0),
            duration_ms: 1234,
        }
    }

    fn step(name: &'static str, status: StepStatus, commands: Vec<CommandRecord>) -> StepEvidence {
        StepEvidence {
            name,
            status,
            duration_ms: 99,
            error: (status == StepStatus::Failed).then(|| format!("{name} failed")),
            commands,
            notes: Vec::new(),
        }
    }

    #[test]
    fn step_notes_are_recorded_when_present() {
        let mut ci = step("ci", StepStatus::Passed, Vec::new());
        ci.notes
            .push("fuzz smoke suite skipped: no fuzz_smoke test target".to_owned());
        let evidence = Evidence::new(
            "0.1.0".to_owned(),
            1,
            None,
            vec![ci, step("audit", StepStatus::Passed, Vec::new())],
            Vec::new(),
        );
        let value: serde_json::Value =
            serde_json::from_str(&evidence.to_json().expect("serializes")).expect("JSON");
        assert_eq!(
            value["steps"][0]["notes"][0],
            "fuzz smoke suite skipped: no fuzz_smoke test target"
        );
        assert!(value["steps"][1].get("notes").is_none());
    }

    fn artifact() -> ArtifactEvidence {
        ArtifactEvidence {
            path: "x86_64-pc-windows-msvc/release/raumakey.exe".to_owned(),
            size: 4096,
            sha256: "ef".repeat(32),
        }
    }

    #[test]
    fn passing_gate_is_cross_build_verified_only() {
        let evidence = Evidence::new(
            "0.1.0".to_owned(),
            1_792_000_000,
            Some("abc123".to_owned()),
            vec![
                step(
                    "ci",
                    StepStatus::Passed,
                    vec![record("cargo fmt --all --check", Some(0))],
                ),
                step(
                    "audit",
                    StepStatus::Passed,
                    vec![record("cargo deny check", Some(0))],
                ),
            ],
            vec![artifact()],
        );
        assert!(evidence.passed);
        assert_eq!(evidence.verification_level, CROSS_BUILD_VERIFIED);
        assert!(!evidence.runtime_verified);
        assert!(!evidence.signed);
        assert_eq!(evidence.schema_version, 1);
        assert_eq!(evidence.product, "Raumakey");
        assert_eq!(evidence.host.os, std::env::consts::OS);
        assert_eq!(evidence.host.arch, std::env::consts::ARCH);
    }

    #[test]
    fn failed_or_missing_steps_are_not_verified() {
        let failed = Evidence::new(
            "0.1.0".to_owned(),
            1,
            None,
            vec![
                step("ci", StepStatus::Passed, Vec::new()),
                step(
                    "audit",
                    StepStatus::Failed,
                    vec![record("cargo deny check", Some(1))],
                ),
                step("cross", StepStatus::NotRun, Vec::new()),
            ],
            Vec::new(),
        );
        assert!(!failed.passed);
        assert_eq!(failed.verification_level, NOT_VERIFIED);
        let empty = Evidence::new("0.1.0".to_owned(), 1, None, Vec::new(), Vec::new());
        assert!(!empty.passed);
        assert_eq!(empty.verification_level, NOT_VERIFIED);
    }

    #[test]
    fn json_records_commands_statuses_durations_and_hashes() {
        let evidence = Evidence::new(
            "0.1.0".to_owned(),
            1_792_000_000,
            None,
            vec![
                step(
                    "cross",
                    StepStatus::Failed,
                    vec![record(
                        "cargo build --target x86_64-pc-windows-msvc",
                        Some(101),
                    )],
                ),
                step("inspect", StepStatus::NotRun, Vec::new()),
            ],
            vec![artifact()],
        );
        let text = evidence.to_json().expect("serializes");
        assert!(text.ends_with('\n'));
        let value: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(value["version"], "0.1.0");
        assert_eq!(value["generated_unix_seconds"], 1_792_000_000u64);
        assert_eq!(value["passed"], false);
        assert_eq!(value["runtime_verified"], false);
        assert_eq!(value["source_revision"], serde_json::Value::Null);
        let cross = &value["steps"][0];
        assert_eq!(cross["name"], "cross");
        assert_eq!(cross["status"], "failed");
        assert_eq!(cross["error"], "cross failed");
        assert_eq!(cross["duration_ms"], 99);
        assert_eq!(
            cross["commands"][0]["command"],
            "cargo build --target x86_64-pc-windows-msvc"
        );
        assert_eq!(cross["commands"][0]["exit_code"], 101);
        assert_eq!(cross["commands"][0]["success"], false);
        assert_eq!(cross["commands"][0]["duration_ms"], 1234);
        assert_eq!(value["steps"][1]["status"], "not-run");
        assert!(value["steps"][1].get("error").is_none());
        assert_eq!(
            value["artifacts"][0]["path"],
            "x86_64-pc-windows-msvc/release/raumakey.exe"
        );
        assert_eq!(value["artifacts"][0]["sha256"], "ef".repeat(32));
        assert_eq!(value["artifacts"][0]["size"], 4096);
    }

    #[test]
    fn file_name_uses_unix_seconds() {
        assert_eq!(
            evidence_file_name(1_792_000_000),
            "release-gate-1792000000.json"
        );
    }
}
