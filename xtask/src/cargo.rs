//! Cargo command lines for `ci` and `cross`, and `cargo metadata` parsing.

use std::path::PathBuf;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::runner::CommandSpec;
use crate::targets::{Artifact, Profile, Target};

/// Features that enable the legacy EVKey codec in every crate that has it.
pub const EVKEY_COMPAT_FEATURES: &str = "raumakey-ipc/evkey-compat,raumakey-tests/evkey-compat";

/// Package and test target of the deterministic fuzz smoke suite.
pub const FUZZ_SMOKE: (&str, &str) = ("raumakey-tests", "fuzz_smoke");

/// Operating system of the machine running xtask.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Host {
    /// Windows: plain `cargo build` with the machine-local MSVC linker configuration.
    Windows,
    /// Any other host: `cargo xwin build`.
    Other,
}

impl Host {
    /// The host xtask runs on.
    pub const fn current() -> Self {
        if cfg!(windows) {
            Host::Windows
        } else {
            Host::Other
        }
    }
}

/// The `ci` gate commands in order. The fuzz smoke command is included when `fuzz_smoke` is set.
pub fn ci_commands(fuzz_smoke: bool) -> Vec<CommandSpec> {
    let mut commands = vec![
        CommandSpec::cargo(["fmt", "--all", "--check"]),
        CommandSpec::cargo([
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ]),
        CommandSpec::cargo([
            "clippy",
            "--workspace",
            "--all-targets",
            "--features",
            EVKEY_COMPAT_FEATURES,
            "--",
            "-D",
            "warnings",
        ]),
        CommandSpec::cargo(["test", "--workspace"]),
        CommandSpec::cargo(["test", "--workspace", "--features", EVKEY_COMPAT_FEATURES]),
    ];
    if fuzz_smoke {
        let (package, test) = FUZZ_SMOKE;
        commands.push(CommandSpec::cargo(["test", "-p", package, "--test", test]));
    }
    commands
}

/// `--config` value that links the CRT statically for `target` (D23). It is appended to the
/// machine-local `target.<triple>.rustflags`, so the library search paths are kept.
pub fn static_crt_config(target: Target) -> String {
    format!(
        "target.{}.rustflags=['-Ctarget-feature=+crt-static','-Clink-arg=/ignore:4099']",
        target.triple()
    )
}

/// The build command for one target: `cargo build` on Windows, `cargo xwin build` elsewhere.
pub fn cross_command(target: Target, profile: Profile, host: Host) -> CommandSpec {
    let mut args: Vec<String> = Vec::new();
    if host == Host::Other {
        args.push("xwin".to_owned());
    }
    args.extend(["build", "--target", target.triple()].map(str::to_owned));
    if profile.is_release() {
        args.push("--release".to_owned());
    }
    for artifact in Artifact::ALL {
        args.push("-p".to_owned());
        args.push(artifact.package_name().to_owned());
    }
    if profile.is_release() {
        args.push("--config".to_owned());
        args.push(static_crt_config(target));
    }
    CommandSpec::cargo(args)
}

/// Fails when `RUSTFLAGS` or `CARGO_ENCODED_RUSTFLAGS` is set: either replaces
/// `target.<triple>.rustflags`, which would silently drop `+crt-static` from release builds.
pub fn check_rustflags_env(lookup: impl Fn(&str) -> Option<String>) -> Result<()> {
    for name in ["CARGO_ENCODED_RUSTFLAGS", "RUSTFLAGS"] {
        if lookup(name).is_some_and(|value| !value.is_empty()) {
            return Err(Error::new(format!(
                "{name} is set; it replaces target.<triple>.rustflags and would drop \
                 +crt-static (D23) from release builds. Unset {name} and retry."
            )));
        }
    }
    Ok(())
}

/// `cargo metadata --format-version 1`, optionally without dependencies or for one platform.
pub fn metadata_command(no_deps: bool, platform: Option<Target>) -> CommandSpec {
    let mut args = vec![
        "metadata".to_owned(),
        "--format-version".to_owned(),
        "1".to_owned(),
    ];
    if no_deps {
        args.push("--no-deps".to_owned());
    }
    if let Some(target) = platform {
        args.push("--filter-platform".to_owned());
        args.push(target.triple().to_owned());
    }
    CommandSpec::cargo(args)
}

/// The parts of `cargo metadata` output that xtask uses.
#[derive(Clone, Debug, Deserialize)]
pub struct Metadata {
    /// Packages in the (possibly filtered) graph.
    pub packages: Vec<MetaPackage>,
    /// Package ids of the workspace members.
    pub workspace_members: Vec<String>,
    /// Cargo target directory.
    pub target_directory: PathBuf,
    /// Resolved dependency graph (absent with `--no-deps`).
    #[serde(default)]
    pub resolve: Option<Resolve>,
}

/// A package entry of `cargo metadata`.
#[derive(Clone, Debug, Deserialize)]
pub struct MetaPackage {
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Package id.
    pub id: String,
    /// SPDX license expression.
    #[serde(default)]
    pub license: Option<String>,
    /// License file path, when the package has no expression.
    #[serde(default)]
    pub license_file: Option<String>,
    /// Build targets.
    #[serde(default)]
    pub targets: Vec<MetaTarget>,
}

/// A build target of a package.
#[derive(Clone, Debug, Deserialize)]
pub struct MetaTarget {
    /// Target name.
    pub name: String,
    /// Target kinds, e.g. `["test"]`.
    pub kind: Vec<String>,
}

/// The resolved dependency graph.
#[derive(Clone, Debug, Deserialize)]
pub struct Resolve {
    /// Graph nodes.
    pub nodes: Vec<ResolveNode>,
}

/// A node of the resolved graph.
#[derive(Clone, Debug, Deserialize)]
pub struct ResolveNode {
    /// Package id.
    pub id: String,
}

impl Metadata {
    /// Parses `cargo metadata --format-version 1` output.
    pub fn parse(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|error| Error::new(format!("cannot parse `cargo metadata` output: {error}")))
    }

    /// Whether workspace package `package` has a test target named `test`.
    pub fn has_test_target(&self, package: &str, test: &str) -> bool {
        self.workspace_packages()
            .filter(|candidate| candidate.name == package)
            .flat_map(|candidate| &candidate.targets)
            .any(|target| target.name == test && target.kind.iter().any(|kind| kind == "test"))
    }

    /// Product version: the version of the controller package.
    pub fn product_version(&self) -> Result<String> {
        let controller = Artifact::Controller.package_name();
        self.workspace_packages()
            .find(|package| package.name == controller)
            .map(|package| package.version.clone())
            .ok_or_else(|| Error::new(format!("workspace has no `{controller}` package")))
    }

    /// Whether `id` is a workspace member.
    pub fn is_workspace_member(&self, id: &str) -> bool {
        self.workspace_members.iter().any(|member| member == id)
    }

    fn workspace_packages(&self) -> impl Iterator<Item = &MetaPackage> {
        self.packages
            .iter()
            .filter(|package| self.is_workspace_member(&package.id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn displays(specs: &[CommandSpec]) -> Vec<String> {
        specs.iter().map(CommandSpec::display).collect()
    }

    #[test]
    fn ci_runs_fmt_clippy_and_tests_with_and_without_compat_features() {
        let expected = [
            "cargo fmt --all --check",
            "cargo clippy --workspace --all-targets -- -D warnings",
            "cargo clippy --workspace --all-targets --features \
             raumakey-ipc/evkey-compat,raumakey-tests/evkey-compat -- -D warnings",
            "cargo test --workspace",
            "cargo test --workspace --features \
             raumakey-ipc/evkey-compat,raumakey-tests/evkey-compat",
        ];
        assert_eq!(displays(&ci_commands(false)), expected);
        let with_fuzz = displays(&ci_commands(true));
        assert_eq!(with_fuzz[..5], expected);
        assert_eq!(
            with_fuzz[5],
            "cargo test -p raumakey-tests --test fuzz_smoke"
        );
        assert_eq!(with_fuzz.len(), 6);
    }

    #[test]
    fn cross_builds_the_three_packages_natively_on_windows() {
        assert_eq!(
            cross_command(Target::I686, Profile::Debug, Host::Windows).display(),
            "cargo build --target i686-pc-windows-msvc -p raumakey-controller -p raumakey-tsf \
             -p raumakey-tests"
        );
        assert_eq!(
            cross_command(Target::X86_64, Profile::Release, Host::Windows).display(),
            "cargo build --target x86_64-pc-windows-msvc --release -p raumakey-controller \
             -p raumakey-tsf -p raumakey-tests --config \
             \"target.x86_64-pc-windows-msvc.rustflags=['-Ctarget-feature=+crt-static',\
             '-Clink-arg=/ignore:4099']\""
        );
    }

    #[test]
    fn cross_uses_cargo_xwin_on_other_hosts() {
        let spec = cross_command(Target::Aarch64, Profile::Release, Host::Other);
        assert_eq!(
            &spec.args[..4],
            ["xwin", "build", "--target", "aarch64-pc-windows-msvc"]
        );
        assert_eq!(
            spec.args.last().map(String::as_str),
            Some(static_crt_config(Target::Aarch64).as_str())
        );
    }

    #[test]
    fn static_crt_config_targets_one_triple() {
        assert_eq!(
            static_crt_config(Target::Aarch64),
            "target.aarch64-pc-windows-msvc.rustflags=['-Ctarget-feature=+crt-static',\
             '-Clink-arg=/ignore:4099']"
        );
    }

    #[test]
    fn rustflags_overrides_are_rejected() {
        assert!(check_rustflags_env(|_| None).is_ok());
        assert!(check_rustflags_env(|name| (name == "RUSTFLAGS").then(String::new)).is_ok());
        let error = check_rustflags_env(|name| {
            (name == "RUSTFLAGS").then(|| "-Ctarget-cpu=native".to_owned())
        })
        .expect_err("RUSTFLAGS overrides target rustflags");
        assert!(error.to_string().contains("RUSTFLAGS"), "{error}");
        assert!(error.to_string().contains("crt-static"), "{error}");
        let error = check_rustflags_env(|name| {
            (name == "CARGO_ENCODED_RUSTFLAGS").then(|| "-Cdebuginfo=0".to_owned())
        })
        .expect_err("CARGO_ENCODED_RUSTFLAGS overrides target rustflags");
        assert!(
            error.to_string().contains("CARGO_ENCODED_RUSTFLAGS"),
            "{error}"
        );
    }

    #[test]
    fn metadata_commands() {
        assert_eq!(
            metadata_command(true, None).display(),
            "cargo metadata --format-version 1 --no-deps"
        );
        assert_eq!(
            metadata_command(false, Some(Target::I686)).display(),
            "cargo metadata --format-version 1 --filter-platform i686-pc-windows-msvc"
        );
    }

    const METADATA: &str = r#"{
        "packages": [
            {"name": "raumakey-controller", "version": "0.3.1", "id": "path+file:///w/crates/raumakey-controller#0.3.1",
             "license": "GPL-3.0-or-later", "license_file": null, "source": null,
             "targets": [{"name": "raumakey", "kind": ["bin"], "src_path": "main.rs"}]},
            {"name": "raumakey-tests", "version": "0.3.1", "id": "path+file:///w/tests#0.3.1",
             "license": "GPL-3.0-or-later", "source": null,
             "targets": [
                {"name": "raumakey_tests", "kind": ["lib"]},
                {"name": "raumakey-test-runner", "kind": ["bin"]},
                {"name": "fuzz_smoke", "kind": ["test"]}
             ]},
            {"name": "serde", "version": "1.0.229", "id": "registry+https://github.com/rust-lang/crates.io-index#serde@1.0.229",
             "license": "MIT OR Apache-2.0", "source": "registry+https://github.com/rust-lang/crates.io-index", "targets": []}
        ],
        "workspace_members": ["path+file:///w/crates/raumakey-controller#0.3.1", "path+file:///w/tests#0.3.1"],
        "target_directory": "E:/w/target",
        "workspace_root": "E:/w",
        "resolve": {"nodes": [{"id": "path+file:///w/tests#0.3.1", "deps": []}], "root": null},
        "version": 1
    }"#;

    #[test]
    fn metadata_fields() {
        let metadata = Metadata::parse(METADATA).expect("parses");
        assert_eq!(metadata.target_directory, PathBuf::from("E:/w/target"));
        assert_eq!(metadata.product_version().expect("version"), "0.3.1");
        assert!(metadata.has_test_target("raumakey-tests", "fuzz_smoke"));
        assert!(!metadata.has_test_target("raumakey-tests", "raumakey-test-runner"));
        assert!(!metadata.has_test_target("raumakey-controller", "fuzz_smoke"));
        assert_eq!(
            metadata.packages[2].license.as_deref(),
            Some("MIT OR Apache-2.0")
        );
        assert_eq!(metadata.resolve.as_ref().map(|r| r.nodes.len()), Some(1));
    }

    #[test]
    fn metadata_errors() {
        assert!(Metadata::parse("not json").is_err());
        let without_controller = METADATA.replace("raumakey-controller", "other");
        let metadata = Metadata::parse(&without_controller).expect("parses");
        assert!(metadata.product_version().is_err());
    }

    #[test]
    fn every_artifact_package_is_built() {
        let spec = cross_command(Target::X86_64, Profile::Debug, Host::Windows);
        for artifact in Artifact::ALL {
            assert!(
                spec.args
                    .windows(2)
                    .any(|pair| pair[0] == "-p" && pair[1] == artifact.package_name()),
                "{artifact:?}"
            );
        }
    }
}
