//! Supported Windows targets, build profiles, and the artifacts `cross` produces.

use std::path::{Path, PathBuf};

/// A Windows MSVC target that Raumakey cross-builds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// `x86_64-pc-windows-msvc` (runtime acceptance target).
    X86_64,
    /// `i686-pc-windows-msvc` (32-bit TIP for WOW64 applications).
    I686,
    /// `aarch64-pc-windows-msvc` (compile-gated only).
    Aarch64,
}

impl Target {
    /// Every supported target in canonical order.
    pub const ALL: [Target; 3] = [Target::X86_64, Target::I686, Target::Aarch64];

    /// Rust target triple.
    pub const fn triple(self) -> &'static str {
        match self {
            Target::X86_64 => "x86_64-pc-windows-msvc",
            Target::I686 => "i686-pc-windows-msvc",
            Target::Aarch64 => "aarch64-pc-windows-msvc",
        }
    }

    /// Parses a supported target triple.
    pub fn from_triple(triple: &str) -> Option<Self> {
        Target::ALL
            .into_iter()
            .find(|target| target.triple() == triple)
    }

    /// Short architecture name used in package layouts (`x64`, `x86`, `arm64`).
    pub const fn arch(self) -> &'static str {
        match self {
            Target::X86_64 => "x64",
            Target::I686 => "x86",
            Target::Aarch64 => "arm64",
        }
    }

    /// Expected COFF machine value of images built for this target.
    pub const fn machine(self) -> u16 {
        match self {
            Target::X86_64 => 0x8664,
            Target::I686 => 0x014C,
            Target::Aarch64 => 0xAA64,
        }
    }

    /// Whether images for this target use the PE32+ (64-bit) optional header.
    pub const fn is_pe32_plus(self) -> bool {
        !matches!(self, Target::I686)
    }
}

/// Cargo profile of the artifacts being built, inspected, or packaged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// `dev` profile, output in `target/<triple>/debug`.
    Debug,
    /// `release` profile, output in `target/<triple>/release` (static CRT, D23).
    Release,
}

impl Profile {
    /// Name of the output directory under `target/<triple>/`.
    pub const fn dir_name(self) -> &'static str {
        match self {
            Profile::Debug => "debug",
            Profile::Release => "release",
        }
    }

    /// Whether this is the release profile.
    pub const fn is_release(self) -> bool {
        matches!(self, Profile::Release)
    }

    /// The `cross` invocation that produces artifacts for this profile.
    pub const fn cross_command(self) -> &'static str {
        match self {
            Profile::Debug => "cargo xtask cross",
            Profile::Release => "cargo xtask cross --release",
        }
    }
}

/// A binary produced by `cargo xtask cross` for every target.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Artifact {
    /// `raumakey.exe` from package `raumakey-controller`.
    Controller,
    /// `raumakey_tip.dll` from package `raumakey-tsf`.
    Tip,
    /// `raumakey-test-runner.exe` from package `raumakey-tests`.
    TestRunner,
}

impl Artifact {
    /// Every artifact in canonical order.
    pub const ALL: [Artifact; 3] = [Artifact::Controller, Artifact::Tip, Artifact::TestRunner];

    /// File name of the artifact in the Cargo output directory.
    pub const fn file_name(self) -> &'static str {
        match self {
            Artifact::Controller => "raumakey.exe",
            Artifact::Tip => "raumakey_tip.dll",
            Artifact::TestRunner => "raumakey-test-runner.exe",
        }
    }

    /// Cargo package that produces the artifact.
    pub const fn package_name(self) -> &'static str {
        match self {
            Artifact::Controller => "raumakey-controller",
            Artifact::Tip => "raumakey-tsf",
            Artifact::TestRunner => "raumakey-tests",
        }
    }

    /// Location of the artifact inside the Cargo target directory.
    pub fn path(self, target_dir: &Path, target: Target, profile: Profile) -> PathBuf {
        target_dir
            .join(target.triple())
            .join(profile.dir_name())
            .join(self.file_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triples_round_trip() {
        assert_eq!(Target::X86_64.triple(), "x86_64-pc-windows-msvc");
        assert_eq!(Target::I686.triple(), "i686-pc-windows-msvc");
        assert_eq!(Target::Aarch64.triple(), "aarch64-pc-windows-msvc");
        for target in Target::ALL {
            assert_eq!(Target::from_triple(target.triple()), Some(target));
        }
        assert_eq!(Target::from_triple("x86_64-unknown-linux-gnu"), None);
        assert_eq!(Target::from_triple(""), None);
    }

    #[test]
    fn machines_and_arch_names() {
        assert_eq!(Target::X86_64.machine(), 0x8664);
        assert_eq!(Target::I686.machine(), 0x014C);
        assert_eq!(Target::Aarch64.machine(), 0xAA64);
        assert_eq!(Target::X86_64.arch(), "x64");
        assert_eq!(Target::I686.arch(), "x86");
        assert_eq!(Target::Aarch64.arch(), "arm64");
        assert!(Target::X86_64.is_pe32_plus());
        assert!(!Target::I686.is_pe32_plus());
        assert!(Target::Aarch64.is_pe32_plus());
    }

    #[test]
    fn profiles() {
        assert_eq!(Profile::Debug.dir_name(), "debug");
        assert_eq!(Profile::Release.dir_name(), "release");
        assert!(Profile::Release.is_release());
        assert!(!Profile::Debug.is_release());
        assert_eq!(Profile::Debug.cross_command(), "cargo xtask cross");
        assert_eq!(
            Profile::Release.cross_command(),
            "cargo xtask cross --release"
        );
    }

    #[test]
    fn artifact_names_and_paths() {
        assert_eq!(Artifact::Controller.file_name(), "raumakey.exe");
        assert_eq!(Artifact::Tip.file_name(), "raumakey_tip.dll");
        assert_eq!(Artifact::TestRunner.file_name(), "raumakey-test-runner.exe");
        assert_eq!(Artifact::Controller.package_name(), "raumakey-controller");
        assert_eq!(Artifact::Tip.package_name(), "raumakey-tsf");
        assert_eq!(Artifact::TestRunner.package_name(), "raumakey-tests");
        let path = Artifact::Tip.path(Path::new("out"), Target::I686, Profile::Release);
        assert_eq!(
            path,
            Path::new("out")
                .join("i686-pc-windows-msvc")
                .join("release")
                .join("raumakey_tip.dll")
        );
    }
}
