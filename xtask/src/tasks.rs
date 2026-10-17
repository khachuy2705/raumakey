//! Command orchestration: runs the gates through [`Runner`] and prints reports.

use std::collections::BTreeSet;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::cargo::{
    FUZZ_SMOKE, Host, Metadata, check_rustflags_env, ci_commands, cross_command, metadata_command,
};
use crate::checksum::{sha256_file, sha256_hex};
use crate::cli::{BuildSelection, CiOptions, Command, USAGE};
use crate::error::{Error, Result};
use crate::evidence::{
    ArtifactEvidence, CROSS_BUILD_VERIFIED, Evidence, StepEvidence, StepStatus, evidence_file_name,
};
use crate::gate::{STEPS, dry_run_problems, installer_dry_run_commands, validate_review};
use crate::inspect::{format_table, inspect_file};
use crate::notices;
use crate::package::{
    PackageInputs, PackageKind, assemble, missing_inputs, missing_inputs_error, package_dir_name,
};
use crate::runner::{CommandSpec, Runner, elapsed_ms};
use crate::targets::{Artifact, Profile, Target};
use crate::wix::{msi_file_name, wix_build_args, wix_source, wxs_file_name};

/// Time limit for one `raumakey.exe ... --dry-run` invocation.
const DRY_RUN_TIMEOUT: Duration = Duration::from_secs(120);

/// What a task produced, for the release evidence.
#[derive(Debug, Default)]
pub struct TaskOutput {
    /// Files produced by the task.
    pub artifacts: Vec<PathBuf>,
    /// Notices such as skipped optional work.
    pub notes: Vec<String>,
}

/// Shared state of one xtask invocation.
#[derive(Debug)]
pub struct Context {
    runner: Runner,
    host: Host,
    workspace: Option<Metadata>,
}

impl Context {
    /// A context rooted at the workspace that contains xtask, running `$CARGO`.
    pub fn new() -> Self {
        let cargo = env::var_os("CARGO").unwrap_or_else(|| OsString::from("cargo"));
        Self {
            runner: Runner::new(workspace_root(), cargo),
            host: Host::current(),
            workspace: None,
        }
    }

    fn workspace(&mut self) -> Result<&Metadata> {
        if self.workspace.is_none() {
            let json = self.runner.capture(&metadata_command(true, None))?;
            self.workspace = Some(Metadata::parse(&json)?);
        }
        self.workspace
            .as_ref()
            .ok_or_else(|| Error::new("`cargo metadata` produced no workspace"))
    }

    fn target_dir(&mut self) -> Result<PathBuf> {
        // Re-collecting the components normalizes separators (`E:/x` becomes `E:\x` on Windows).
        Ok(self.workspace()?.target_directory.components().collect())
    }

    fn version(&mut self) -> Result<String> {
        self.workspace()?.product_version()
    }
}

/// Root of the workspace that contains this xtask crate.
pub fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.parent().unwrap_or(manifest_dir).to_path_buf()
}

/// Target directory used when `cargo metadata` is unavailable: `CARGO_TARGET_DIR` (relative to
/// the workspace root) or `<root>/target`.
pub fn fallback_target_dir(root: &Path, cargo_target_dir: Option<OsString>) -> PathBuf {
    match cargo_target_dir {
        Some(dir) if !dir.is_empty() => root.join(dir),
        _ => root.join("target"),
    }
}

/// `path` relative to `base` with `/` separators, or the full path when it is outside `base`.
pub fn relative_display(base: &Path, path: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(relative) => relative
            .components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        Err(_) => path.display().to_string(),
    }
}

/// Sizes and digests of existing artifacts, without duplicates, paths relative to `target_dir`.
pub fn artifact_evidence(target_dir: &Path, paths: &[PathBuf]) -> Vec<ArtifactEvidence> {
    let mut seen = BTreeSet::new();
    paths
        .iter()
        .filter(|path| seen.insert(path.as_path()))
        .filter_map(|path| {
            let size = fs::metadata(path).ok()?.len();
            let sha256 = sha256_file(path).ok()?;
            Some(ArtifactEvidence {
                path: relative_display(target_dir, path),
                size,
                sha256,
            })
        })
        .collect()
}

/// Runs a parsed command.
pub fn run(command: Command) -> Result<()> {
    let mut ctx = Context::new();
    let mut output = TaskOutput::default();
    match command {
        Command::Help => {
            print!("{USAGE}");
            Ok(())
        }
        Command::Ci(options) => ci(&mut ctx, options, &mut output),
        Command::Cross(selection) => cross(&mut ctx, &selection, &mut output),
        Command::Inspect(selection) => inspect(&mut ctx, &selection),
        Command::Package { profile } => package(&mut ctx, profile, &mut output),
        Command::Audit => audit(&mut ctx),
        Command::Notices => notices(&mut ctx),
        Command::ReleaseGate => release_gate(&mut ctx),
    }
}

/// Tasks record notes and produced files in `output` as they go, so a failing step still
/// reports its notes in the release evidence.
fn ci(ctx: &mut Context, options: CiOptions, output: &mut TaskOutput) -> Result<()> {
    let (package, test) = FUZZ_SMOKE;
    let fuzz = if !options.fuzz {
        output
            .notes
            .push("fuzz smoke suite skipped (--no-fuzz)".to_owned());
        false
    } else if ctx.workspace()?.has_test_target(package, test) {
        true
    } else {
        return Err(Error::new(format!(
            "required fuzz smoke suite missing: package `{package}` has no `{test}` test target; \
             implement the suite before running CI or the release gate. \
             Use `ci --no-fuzz` only for partial local checks."
        )));
    };
    for note in &output.notes {
        eprintln!("notice: {note}");
    }
    for spec in ci_commands(fuzz) {
        ctx.runner.run(&spec)?;
    }
    println!("ci: passed");
    Ok(())
}

fn cross(ctx: &mut Context, selection: &BuildSelection, output: &mut TaskOutput) -> Result<()> {
    if selection.profile.is_release() {
        check_rustflags_env(|name| env::var(name).ok())?;
    }
    let target_dir = ctx.target_dir()?;
    let mut outcomes = Vec::new();
    for &target in &selection.targets {
        let spec = cross_command(target, selection.profile, ctx.host);
        outcomes.push((target, ctx.runner.run(&spec)));
    }

    println!("\ncross ({}):", selection.profile.dir_name());
    let mut failed = Vec::new();
    for (target, outcome) in outcomes {
        if let Err(error) = outcome {
            println!("  {}: FAILED ({error})", target.triple());
            failed.push(target.triple());
            continue;
        }
        let paths: Vec<PathBuf> = Artifact::ALL
            .iter()
            .map(|artifact| artifact.path(&target_dir, target, selection.profile))
            .collect();
        let complete = paths.iter().all(|path| path.is_file());
        if complete {
            println!("  {}: OK", target.triple());
        } else {
            println!(
                "  {}: FAILED (build succeeded but artifacts are missing)",
                target.triple()
            );
            failed.push(target.triple());
        }
        for path in paths {
            if path.is_file() {
                println!("    {}", path.display());
                output.artifacts.push(path);
            } else {
                println!("    {} (missing)", path.display());
            }
        }
    }
    if failed.is_empty() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "cross build failed for {}",
            failed.join(", ")
        )))
    }
}

fn inspect(ctx: &mut Context, selection: &BuildSelection) -> Result<()> {
    let target_dir = ctx.target_dir()?;
    let mut rows = Vec::new();
    for &target in &selection.targets {
        for artifact in Artifact::ALL {
            let path = artifact.path(&target_dir, target, selection.profile);
            rows.extend(inspect_file(&path, artifact, target, selection.profile));
        }
    }
    print!("{}", format_table(&rows));
    let failures = rows.iter().filter(|row| !row.check.passed).count();
    if failures == 0 {
        println!("inspect: all {} checks passed", rows.len());
        Ok(())
    } else {
        Err(Error::new(format!(
            "inspect: {failures} of {} checks failed",
            rows.len()
        )))
    }
}

fn package(ctx: &mut Context, profile: Profile, output: &mut TaskOutput) -> Result<()> {
    let target_dir = ctx.target_dir()?;
    let version = ctx.version()?;
    let root = ctx.runner.root().to_path_buf();
    let inputs = PackageInputs {
        repo_root: &root,
        target_dir: &target_dir,
        version: &version,
        profile,
    };
    let mut missing: Vec<PathBuf> = PackageKind::ALL
        .iter()
        .flat_map(|kind| missing_inputs(*kind, &inputs))
        .collect();
    missing.sort();
    missing.dedup();
    if !missing.is_empty() {
        return Err(missing_inputs_error(profile, &missing));
    }

    let out_dir = target_dir.join("package");
    for kind in PackageKind::ALL {
        let package = assemble(kind, &inputs, &out_dir)?;
        println!(
            "package: {} ({} files{})",
            package.dir.display(),
            package.files.len(),
            if kind.compile_gated() {
                ", compile-gated"
            } else {
                ""
            }
        );
        output.artifacts.extend(package.files);
    }

    let x64_dir = out_dir.join(package_dir_name(PackageKind::X64, &version, profile));
    let wxs = out_dir.join(wxs_file_name(profile));
    fs::write(&wxs, wix_source(&version, &x64_dir))
        .map_err(|error| Error::io("write", &wxs, error))?;
    println!("WiX source: {}", wxs.display());
    output.artifacts.push(wxs.clone());
    let msi = out_dir.join(msi_file_name(&version, profile));
    if msi.exists() {
        fs::remove_file(&msi).map_err(|error| Error::io("remove", &msi, error))?;
    }
    if ctx.runner.probe(&CommandSpec::new("wix", ["--version"])) {
        ctx.runner
            .run(&CommandSpec::new("wix", wix_build_args(&wxs, &msi)))?;
        println!("MSI: {}", msi.display());
        output.artifacts.push(msi);
    } else {
        println!("MSI skipped: wix not found");
        output.notes.push("MSI skipped: wix not found".to_owned());
    }
    println!("packages are unsigned and not runtime verified");
    Ok(())
}

fn audit(ctx: &mut Context) -> Result<()> {
    if !ctx
        .runner
        .probe(&CommandSpec::new("cargo-deny", ["--version"]))
    {
        return Err(Error::new(
            "cargo-deny not found on PATH; install it with `cargo install cargo-deny --locked`",
        ));
    }
    ctx.runner.run(&CommandSpec::cargo(["deny", "check"]))?;
    println!("audit: passed");
    Ok(())
}

fn notices(ctx: &mut Context) -> Result<()> {
    let mut per_target = Vec::new();
    for target in Target::ALL {
        let json = ctx.runner.capture(&metadata_command(false, Some(target)))?;
        per_target.push((target, Metadata::parse(&json)?));
    }
    let list = notices::collect(&per_target);
    let dir = ctx.target_dir()?.join("notices");
    fs::create_dir_all(&dir).map_err(|error| Error::io("create", &dir, error))?;
    let path = dir.join("third-party-licenses.md");
    fs::write(&path, notices::render_markdown(&list))
        .map_err(|error| Error::io("write", &path, error))?;
    println!("{}", notices::summary(&list));
    println!("wrote {}", path.display());
    let missing = list.iter().filter(|notice| !notice.has_license()).count();
    if missing == 0 {
        Ok(())
    } else {
        Err(Error::new(format!(
            "{missing} third-party packages declare no license expression"
        )))
    }
}

fn installer_dry_run(ctx: &mut Context) -> Result<()> {
    if ctx.host != Host::Windows {
        return Err(Error::new(
            "the installer dry-run contract check executes raumakey.exe and needs a Windows host",
        ));
    }
    let target_dir = ctx.target_dir()?;
    let version = ctx.version()?;
    let package_dir = target_dir.join("package").join(package_dir_name(
        PackageKind::X64,
        &version,
        Profile::Release,
    ));
    let system_root = env::var_os("SystemRoot")
        .or_else(|| env::var_os("windir"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            Error::new("neither SystemRoot nor windir is set; Windows PowerShell cannot be found")
        })?;
    let mut problems = Vec::new();
    for spec in installer_dry_run_commands(&package_dir, &system_root) {
        let captured = ctx.runner.capture_with_timeout(&spec, DRY_RUN_TIMEOUT)?;
        let output = format!("{}{}", captured.stdout, captured.stderr);
        for line in output.lines() {
            println!("    {line}");
        }
        problems.extend(
            dry_run_problems(captured.exit_code, &output)
                .into_iter()
                .map(|problem| format!("`{}`: {problem}", spec.display())),
        );
    }
    if problems.is_empty() {
        println!("installer dry-run contract: passed");
        Ok(())
    } else {
        Err(Error::new(problems.join("\n")))
    }
}

fn source_revision(ctx: &mut Context) -> Option<String> {
    let head = ctx
        .runner
        .capture(&CommandSpec::new("git", ["rev-parse", "HEAD"]))
        .ok()?;
    let dirty = ctx
        .runner
        .capture(&CommandSpec::new("git", ["status", "--porcelain"]))
        .ok()?;
    Some(format!(
        "{}{}",
        head.trim(),
        if dirty.trim().is_empty() {
            ""
        } else {
            "-dirty"
        }
    ))
}

fn whole_branch_review(ctx: &mut Context, output: &mut TaskOutput) -> Result<()> {
    let path = ctx
        .target_dir()?
        .join("release-evidence/whole-branch-review.json");
    let bytes = fs::read(&path).map_err(|error| Error::io("read required review", &path, error))?;
    validate_review(&bytes, source_revision(ctx).as_deref())?;
    output.notes.push(format!(
        "whole-branch-review.json SHA-256: {}",
        sha256_hex(&bytes)
    ));
    Ok(())
}

fn release_gate(ctx: &mut Context) -> Result<()> {
    let generated = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or_default();
    let release = BuildSelection {
        profile: Profile::Release,
        targets: Target::ALL.to_vec(),
    };
    let mut steps = Vec::new();
    let mut produced = Vec::new();
    let mut failed_step = None;
    for name in STEPS {
        if failed_step.is_some() {
            steps.push(StepEvidence {
                name,
                status: StepStatus::NotRun,
                duration_ms: 0,
                error: None,
                commands: Vec::new(),
                notes: Vec::new(),
            });
            continue;
        }
        eprintln!("\n=== release-gate: {name} ===");
        let first_command = ctx.runner.records().len();
        let start = Instant::now();
        let mut output = TaskOutput::default();
        let outcome = match name {
            "ci" => ci(ctx, CiOptions { fuzz: true }, &mut output),
            "audit" => audit(ctx),
            "cross" => cross(ctx, &release, &mut output),
            "inspect" => inspect(ctx, &release),
            "whole-branch-review" => whole_branch_review(ctx, &mut output),
            "package" => package(ctx, Profile::Release, &mut output),
            "installer-dry-run" => installer_dry_run(ctx),
            other => Err(Error::new(format!("unknown gate step `{other}`"))),
        };
        let duration_ms = elapsed_ms(start);
        let commands = ctx.runner.records()[first_command..].to_vec();
        let (status, error) = match outcome {
            Ok(()) => {
                // Only files from passing steps count as release artifacts.
                produced.extend(output.artifacts);
                (StepStatus::Passed, None)
            }
            Err(error) => {
                eprintln!("release-gate: step `{name}` failed: {error}");
                failed_step = Some(name);
                (StepStatus::Failed, Some(error.to_string()))
            }
        };
        steps.push(StepEvidence {
            name,
            status,
            duration_ms,
            error,
            commands,
            notes: output.notes,
        });
    }

    let target_dir = ctx.target_dir().unwrap_or_else(|_| {
        fallback_target_dir(ctx.runner.root(), env::var_os("CARGO_TARGET_DIR"))
    });
    let version = ctx
        .version()
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").to_owned());
    let revision = source_revision(ctx);
    let evidence = Evidence::new(
        version,
        generated,
        revision,
        steps,
        artifact_evidence(&target_dir, &produced),
    );
    let directory = target_dir.join("release-evidence");
    fs::create_dir_all(&directory).map_err(|error| Error::io("create", &directory, error))?;
    let path = directory.join(evidence_file_name(generated));
    fs::write(&path, evidence.to_json()?).map_err(|error| Error::io("write", &path, error))?;
    println!("\nrelease evidence: {}", path.display());

    match failed_step {
        None => {
            println!(
                "release-gate: PASSED. Result: {CROSS_BUILD_VERIFIED}. This is NOT runtime \
                 verification; runtime x86_64 verification requires the Windows acceptance \
                 harness and the manual matrix."
            );
            Ok(())
        }
        Some(step) => Err(Error::new(format!(
            "release-gate: FAILED at step `{step}`; result: not verified (not cross-build \
             verified, not runtime verified). Evidence: {}",
            path.display()
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fuzz_suite_fails_before_starting_ci_commands() {
        let mut ctx = Context {
            runner: Runner::new(workspace_root(), OsString::from("raumakey-no-such-cargo")),
            host: Host::current(),
            workspace: Some(Metadata {
                packages: Vec::new(),
                workspace_members: Vec::new(),
                target_directory: PathBuf::from("target"),
                resolve: None,
            }),
        };
        let mut output = TaskOutput::default();
        let error = ci(&mut ctx, CiOptions { fuzz: true }, &mut output).expect_err("missing suite");
        assert!(error.to_string().contains("fuzz_smoke"), "{error}");
        assert!(ctx.runner.records().is_empty());
        assert!(
            output.notes.is_empty(),
            "required tests must not become a skip note"
        );
    }

    #[test]
    fn workspace_root_contains_the_workspace_manifest() {
        let root = workspace_root();
        let manifest = fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
        assert!(manifest.contains("[workspace]"));
        assert!(root.join("xtask").join("Cargo.toml").is_file());
    }

    #[test]
    fn fallback_target_dir_honours_cargo_target_dir() {
        let root = Path::new("repo");
        assert_eq!(fallback_target_dir(root, None), root.join("target"));
        assert_eq!(
            fallback_target_dir(root, Some(OsString::from("out/build"))),
            root.join("out/build")
        );
        let absolute = env::temp_dir().join("raumakey-target");
        assert_eq!(
            fallback_target_dir(root, Some(absolute.clone().into_os_string())),
            absolute
        );
        assert_eq!(
            fallback_target_dir(root, Some(OsString::new())),
            root.join("target")
        );
    }

    #[test]
    fn relative_paths_use_forward_slashes() {
        let base = Path::new("target");
        assert_eq!(
            relative_display(base, &base.join("package").join("raumakey.exe")),
            "package/raumakey.exe"
        );
        let outside = Path::new("elsewhere").join("file.txt");
        assert_eq!(
            relative_display(base, &outside),
            outside.display().to_string()
        );
    }

    #[test]
    fn artifact_evidence_hashes_existing_files_once() {
        let dir = env::temp_dir().join(format!("raumakey-xtask-evidence-{}", std::process::id()));
        fs::create_dir_all(dir.join("x86_64-pc-windows-msvc")).expect("temp dir");
        let exe = dir.join("x86_64-pc-windows-msvc").join("raumakey.exe");
        fs::write(&exe, b"abc").expect("write");
        let evidence =
            artifact_evidence(&dir, &[exe.clone(), dir.join("missing.dll"), exe.clone()]);
        fs::remove_dir_all(&dir).expect("cleanup");
        assert_eq!(
            evidence,
            [ArtifactEvidence {
                path: "x86_64-pc-windows-msvc/raumakey.exe".to_owned(),
                size: 3,
                sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
                    .to_owned(),
            }]
        );
    }
}
