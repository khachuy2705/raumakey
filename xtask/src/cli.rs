//! Command-line parsing for `cargo xtask` (std only).

use std::fmt;

use crate::targets::{Profile, Target};

/// Usage text printed by `cargo xtask help`.
pub const USAGE: &str = "Raumakey developer tasks

Usage: cargo xtask <command> [options]

Commands:
  ci [--no-fuzz]
      cargo fmt --check; clippy -D warnings with default and evkey-compat features;
      workspace tests with both feature sets; the fuzz smoke suite (required unless --no-fuzz)
  cross [--release] [--target <TRIPLE>]...
      build raumakey.exe, raumakey_tip.dll and raumakey-test-runner.exe;
      --release links the CRT statically (D23)
  inspect [--release] [--target <TRIPLE>]...
      validate PE machine, subsystem, exports, manifest, resources, COM identities
      and CRT imports of the built artifacts
  package [--release]
      assemble the portable x64 and ARM64 (compile-gated) packages; each holds
      raumakey.exe, README.txt, LICENSE, THIRD_PARTY.md, SHA256SUMS and manifest.json
  audit
      cargo deny check
  notices
      list third-party crates and licenses for the Windows targets
  release-gate
      ci, audit, cross --release, inspect --release, whole-branch review,
      package --release and portable-check (the x64 package holds exactly its files with
      raumakey.exe as the only executable, and raumakey.exe --version names this version);
      writes release evidence JSON
      requires release-evidence/whole-branch-review.json under the Cargo target directory,
      approving the current clean Git revision with no unresolved findings
  help
      show this help

Targets (default: all):
  x86_64-pc-windows-msvc, i686-pc-windows-msvc, aarch64-pc-windows-msvc

Outputs are written below the Cargo target directory (package/, notices/, release-evidence/).
Passing the release gate means \"cross-build verified\", not runtime verified: runtime x86_64
verification requires the Windows acceptance harness and the manual matrix.
";

/// Options of `cargo xtask ci`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CiOptions {
    /// Run the deterministic fuzz smoke suite.
    pub fuzz: bool,
}

/// Profile and targets selected for `cross` and `inspect`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildSelection {
    /// Build profile.
    pub profile: Profile,
    /// Targets in canonical order, without duplicates.
    pub targets: Vec<Target>,
}

/// A parsed `cargo xtask` invocation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Formatting, lints, tests, and fuzz smoke.
    Ci(CiOptions),
    /// Cross-build the shipped binaries.
    Cross(BuildSelection),
    /// Validate built PE artifacts.
    Inspect(BuildSelection),
    /// Assemble distributable packages.
    Package {
        /// Profile of the packaged artifacts.
        profile: Profile,
    },
    /// `cargo deny check`.
    Audit,
    /// Third-party license list.
    Notices,
    /// Every release gate in order, with evidence.
    ReleaseGate,
    /// Print usage.
    Help,
}

/// A command-line usage error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CliError(pub String);

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Parses the arguments that follow `cargo xtask`.
pub fn parse(args: &[String]) -> Result<Command, CliError> {
    let Some((command, options)) = args.split_first() else {
        return Ok(Command::Help);
    };
    match command.as_str() {
        "help" | "--help" | "-h" => {
            no_options(command, options)?;
            Ok(Command::Help)
        }
        "ci" => {
            let mut fuzz = true;
            for option in options {
                match option.as_str() {
                    "--no-fuzz" => fuzz = false,
                    other => return Err(unknown_option(command, other)),
                }
            }
            Ok(Command::Ci(CiOptions { fuzz }))
        }
        "cross" => parse_selection(command, options).map(Command::Cross),
        "inspect" => parse_selection(command, options).map(Command::Inspect),
        "package" => {
            let mut profile = Profile::Debug;
            for option in options {
                match option.as_str() {
                    "--release" => profile = Profile::Release,
                    other => return Err(unknown_option(command, other)),
                }
            }
            Ok(Command::Package { profile })
        }
        "audit" => no_options(command, options).map(|()| Command::Audit),
        "notices" => no_options(command, options).map(|()| Command::Notices),
        "release-gate" => no_options(command, options).map(|()| Command::ReleaseGate),
        other => Err(CliError(format!(
            "unknown command `{other}`; run `cargo xtask help`"
        ))),
    }
}

fn parse_selection(command: &str, options: &[String]) -> Result<BuildSelection, CliError> {
    let mut profile = Profile::Debug;
    let mut requested = Vec::new();
    let mut iter = options.iter();
    while let Some(option) = iter.next() {
        if option == "--release" {
            profile = Profile::Release;
        } else if option == "--target" {
            let value = iter.next().ok_or_else(|| {
                CliError(format!("`{command} --target` requires a target triple"))
            })?;
            requested.push(parse_target(value)?);
        } else if let Some(value) = option.strip_prefix("--target=") {
            requested.push(parse_target(value)?);
        } else {
            return Err(unknown_option(command, option));
        }
    }
    let targets = if requested.is_empty() {
        Target::ALL.to_vec()
    } else {
        Target::ALL
            .into_iter()
            .filter(|target| requested.contains(target))
            .collect()
    };
    Ok(BuildSelection { profile, targets })
}

fn parse_target(value: &str) -> Result<Target, CliError> {
    Target::from_triple(value).ok_or_else(|| {
        let supported: Vec<&str> = Target::ALL.iter().map(|target| target.triple()).collect();
        CliError(format!(
            "unsupported target `{value}`; supported targets: {}",
            supported.join(", ")
        ))
    })
}

fn no_options(command: &str, options: &[String]) -> Result<(), CliError> {
    match options.first() {
        None => Ok(()),
        Some(option) => Err(unknown_option(command, option)),
    }
}

fn unknown_option(command: &str, option: &str) -> CliError {
    CliError(format!(
        "unexpected argument `{option}` for `{command}`; run `cargo xtask help`"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, CliError> {
        let owned: Vec<String> = args.iter().map(|arg| (*arg).to_owned()).collect();
        parse(&owned)
    }

    #[test]
    fn no_arguments_and_help_flags_print_usage() {
        assert_eq!(parse_strs(&[]), Ok(Command::Help));
        assert_eq!(parse_strs(&["help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
    }

    #[test]
    fn ci_runs_fuzz_unless_disabled() {
        assert_eq!(
            parse_strs(&["ci"]),
            Ok(Command::Ci(CiOptions { fuzz: true }))
        );
        assert_eq!(
            parse_strs(&["ci", "--no-fuzz"]),
            Ok(Command::Ci(CiOptions { fuzz: false }))
        );
    }

    #[test]
    fn cross_defaults_to_debug_and_every_target() {
        assert_eq!(
            parse_strs(&["cross"]),
            Ok(Command::Cross(BuildSelection {
                profile: Profile::Debug,
                targets: Target::ALL.to_vec(),
            }))
        );
    }

    #[test]
    fn cross_accepts_release_and_targets_in_both_spellings() {
        assert_eq!(
            parse_strs(&[
                "cross",
                "--target=aarch64-pc-windows-msvc",
                "--release",
                "--target",
                "x86_64-pc-windows-msvc",
                "--target",
                "aarch64-pc-windows-msvc",
            ]),
            Ok(Command::Cross(BuildSelection {
                profile: Profile::Release,
                targets: vec![Target::X86_64, Target::Aarch64],
            }))
        );
    }

    #[test]
    fn inspect_takes_the_same_options_as_cross() {
        assert_eq!(
            parse_strs(&["inspect", "--release", "--target", "i686-pc-windows-msvc"]),
            Ok(Command::Inspect(BuildSelection {
                profile: Profile::Release,
                targets: vec![Target::I686],
            }))
        );
    }

    #[test]
    fn target_errors_are_reported() {
        let missing = parse_strs(&["cross", "--target"]).unwrap_err();
        assert!(missing.0.contains("--target"), "{missing}");
        let unsupported =
            parse_strs(&["inspect", "--target", "x86_64-unknown-linux-gnu"]).unwrap_err();
        assert!(
            unsupported.0.contains("x86_64-unknown-linux-gnu"),
            "{unsupported}"
        );
        assert!(
            unsupported.0.contains("x86_64-pc-windows-msvc"),
            "error lists the supported triples: {unsupported}"
        );
    }

    #[test]
    fn package_audit_notices_release_gate() {
        assert_eq!(
            parse_strs(&["package"]),
            Ok(Command::Package {
                profile: Profile::Debug
            })
        );
        assert_eq!(
            parse_strs(&["package", "--release"]),
            Ok(Command::Package {
                profile: Profile::Release
            })
        );
        assert_eq!(parse_strs(&["audit"]), Ok(Command::Audit));
        assert_eq!(parse_strs(&["notices"]), Ok(Command::Notices));
        assert_eq!(parse_strs(&["release-gate"]), Ok(Command::ReleaseGate));
    }

    #[test]
    fn unknown_commands_and_options_are_rejected() {
        let unknown = parse_strs(&["frobnicate"]).unwrap_err();
        assert!(unknown.0.contains("frobnicate"), "{unknown}");
        for args in [
            &["ci", "--release"][..],
            &["package", "--target", "x86_64-pc-windows-msvc"][..],
            &["audit", "--fix"][..],
            &["notices", "extra"][..],
            &["release-gate", "--no-fuzz"][..],
            &["help", "ci"][..],
        ] {
            let error = parse_strs(args).unwrap_err();
            assert!(error.0.contains(args[1]), "{args:?}: {error}");
        }
    }

    #[test]
    fn usage_mentions_every_command_and_the_verification_wording() {
        for command in [
            "ci",
            "cross",
            "inspect",
            "package",
            "audit",
            "notices",
            "release-gate",
            "help",
        ] {
            assert!(USAGE.contains(command), "usage lacks {command}");
        }
        assert!(USAGE.contains("--no-fuzz"));
        assert!(USAGE.contains("--release"));
        assert!(USAGE.contains("--target"));
        assert!(USAGE.contains("cross-build verified"));
    }

    /// The packages are portable (D38): `package` lists what a package holds, and the gate's
    /// last step checks the packaged `raumakey.exe`. Nothing installs, so no installer, script,
    /// WiX source, or MSI is mentioned.
    #[test]
    fn usage_describes_the_portable_package_and_its_check() {
        for needle in [
            "portable",
            "raumakey.exe, README.txt, LICENSE, THIRD_PARTY.md, SHA256SUMS and manifest.json",
            "portable-check",
            "raumakey.exe --version",
        ] {
            assert!(USAGE.contains(needle), "usage lacks {needle}\n{USAGE}");
        }
        for stale in ["install", "script", "WiX", "wix", "MSI", "dry-run"] {
            assert!(
                !USAGE.contains(stale),
                "usage still mentions {stale}\n{USAGE}"
            );
        }
    }
}
