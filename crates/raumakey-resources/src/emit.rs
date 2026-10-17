//! Build-script entry point: one `.res` file per Raumakey artifact (see the crate docs for the
//! build-script recipe).

use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::error::ResourceError;
use crate::icon::{IconGlyph, render_icon_set};
use crate::manifest::Manifest;
use crate::res::ResWriter;
use crate::version::{FileType, VersionInfo, version_quad};

/// Version shared by every Raumakey binary (the workspace package version).
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Icon group of the Raumakey application mark. As the lowest icon group ordinal it is the
/// icon Explorer shows for the file and icon index 0 for TSF.
pub const ICON_APP: u16 = 1;
/// Icon group of the Vietnamese-mode glyph ("V" on teal).
pub const ICON_VIETNAMESE: u16 = 2;
/// Icon group of the English-mode glyph ("E" on slate).
pub const ICON_ENGLISH: u16 = 3;
/// `uIconIndex` for `ITfInputProcessorProfileMgr::RegisterProfile`: the app mark is the first
/// icon group of `raumakey_tip.dll`.
pub const TIP_PROFILE_ICON_INDEX: u32 = 0;

const ALL_ICONS: [(u16, IconGlyph); 3] = [
    (ICON_APP, IconGlyph::AppMark),
    (ICON_VIETNAMESE, IconGlyph::Vietnamese),
    (ICON_ENGLISH, IconGlyph::English),
];
const APP_ICON_ONLY: [(u16, IconGlyph); 1] = [(ICON_APP, IconGlyph::AppMark)];

const TEST_RUNNER_IDENTITY: &str = "Raumakey.TestRunner";

/// A Raumakey binary that carries Windows resources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Artifact {
    /// `raumakey.exe`: manifest, version information, app/V/E icons.
    Controller,
    /// `raumakey_tip.dll`: version information, app/V/E icons, no manifest.
    Tip,
    /// `raumakey-test-runner.exe`: manifest, version information, app icon.
    TestRunner,
}

impl Artifact {
    /// File name without extension; also the `.res` file name and `InternalName`.
    pub const fn file_stem(self) -> &'static str {
        match self {
            Self::Controller => "raumakey",
            Self::Tip => "raumakey_tip",
            Self::TestRunner => "raumakey-test-runner",
        }
    }

    /// File name of the linked binary (`OriginalFilename`).
    pub const fn original_filename(self) -> &'static str {
        match self {
            Self::Controller => "raumakey.exe",
            Self::Tip => "raumakey_tip.dll",
            Self::TestRunner => "raumakey-test-runner.exe",
        }
    }

    /// `FileDescription` of the binary.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Controller => "Raumakey Vietnamese input method",
            Self::Tip => "Raumakey text input processor",
            Self::TestRunner => "Raumakey acceptance test runner",
        }
    }

    /// Kind of binary.
    pub const fn file_type(self) -> FileType {
        match self {
            Self::Controller | Self::TestRunner => FileType::App,
            Self::Tip => FileType::Dll,
        }
    }

    /// Icon groups embedded in the binary, by group ordinal.
    pub const fn icon_groups(self) -> &'static [(u16, IconGlyph)] {
        match self {
            Self::Controller | Self::Tip => &ALL_ICONS,
            Self::TestRunner => &APP_ICON_ONLY,
        }
    }

    /// The executable manifest, or `None` for the DLL.
    fn manifest(self, version: [u16; 4]) -> Result<Option<Manifest>, ResourceError> {
        match self {
            Self::Controller => Ok(Some(Manifest::controller(version))),
            Self::Tip => Ok(None),
            Self::TestRunner => {
                Manifest::executable(TEST_RUNNER_IDENTITY, self.description(), version).map(Some)
            }
        }
    }

    /// Builds every resource of the artifact for `version` (see [`version_quad`]).
    pub fn resources(self, version: &str) -> Result<ResWriter, ResourceError> {
        let quad = version_quad(version)?;
        let mut writer = ResWriter::new();
        if let Some(manifest) = self.manifest(quad)? {
            writer.add_manifest(&manifest)?;
        }
        writer.add_version_info(&VersionInfo::raumakey(
            self.file_type(),
            version,
            self.description(),
            self.file_stem(),
            self.original_filename(),
        )?)?;
        for &(group, glyph) in self.icon_groups() {
            writer.add_icon_group(group, &render_icon_set(glyph)?)?;
        }
        Ok(writer)
    }

    /// The Cargo build-script directive that passes `res` to the linker of this artifact only.
    pub fn link_directive(self, res: &Path) -> String {
        match self {
            Self::Controller | Self::TestRunner => format!(
                "cargo:rustc-link-arg-bin={}={}",
                self.file_stem(),
                res.display()
            ),
            Self::Tip => format!("cargo:rustc-link-arg-cdylib={}", res.display()),
        }
    }
}

/// Whether a target's linker takes `.res` files directly: Windows with the MSVC toolchain
/// (`link.exe`, or `rust-lld` in `lld-link` mode). Takes `CARGO_CFG_TARGET_OS` and
/// `CARGO_CFG_TARGET_ENV`.
pub fn links_res_files(target_os: &str, target_env: &str) -> bool {
    target_os == "windows" && target_env == "msvc"
}

/// Writes `<out_dir>/<file stem>.res` for `artifact` with [`PRODUCT_VERSION`] and returns its
/// path. Creates `out_dir` when needed; an existing file with identical content is left alone.
pub fn emit(out_dir: impl AsRef<Path>, artifact: Artifact) -> io::Result<PathBuf> {
    emit_with_version(out_dir, artifact, PRODUCT_VERSION)
}

/// Like [`emit`], with an explicit version string. An invalid version fails with
/// [`io::ErrorKind::InvalidInput`] before anything is written.
pub fn emit_with_version(
    out_dir: impl AsRef<Path>,
    artifact: Artifact,
    version: &str,
) -> io::Result<PathBuf> {
    let bytes = artifact
        .resources(version)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?
        .to_bytes();
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;
    let path = out_dir.join(format!("{}.res", artifact.file_stem()));
    if fs::read(&path).is_ok_and(|existing| existing == bytes) {
        return Ok(path);
    }
    let temporary = out_dir.join(format!("{}.res.tmp", artifact.file_stem()));
    replace_file_with(&path, &temporary, &bytes, |temporary, bytes| {
        fs::write(temporary, bytes)
    })?;
    Ok(path)
}

/// Writes `bytes` to `temporary` with `write`, then renames it over `path`, so `path` never
/// holds a partial file. On failure the temporary file is removed.
fn replace_file_with(
    path: &Path,
    temporary: &Path,
    bytes: &[u8],
    write: impl FnOnce(&Path, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    let result = write(temporary, bytes).and_then(|()| fs::rename(temporary, path));
    if result.is_err() {
        // Best effort: the write or rename error is the one worth reporting.
        let _ = fs::remove_file(temporary);
    }
    result
}

/// Resource step of a build script. On Windows MSVC targets it writes the artifact's `.res`
/// into `OUT_DIR`, prints [`Artifact::link_directive`] to standard output, and returns the
/// path; on other targets it does nothing and returns `None`. The caller still prints its own
/// `rerun-if-changed`.
///
/// Reads `CARGO_CFG_TARGET_OS`, `CARGO_CFG_TARGET_ENV`, and `OUT_DIR`, then calls
/// [`build_script_for`].
pub fn build_script(artifact: Artifact) -> io::Result<Option<PathBuf>> {
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    let out_dir = env::var_os("OUT_DIR").map(PathBuf::from);
    build_script_for(
        artifact,
        &target_os,
        &target_env,
        out_dir.as_deref(),
        &mut io::stdout().lock(),
    )
}

/// [`build_script`] with explicit inputs: the target OS and environment (the values of
/// `CARGO_CFG_TARGET_OS` and `CARGO_CFG_TARGET_ENV`), the output directory (`OUT_DIR`), and
/// where the link directive is printed.
///
/// When [`links_res_files`] is false it writes nothing, prints nothing, and returns `None`
/// (`out_dir` may then be absent). Otherwise it [`emit`]s the artifact into `out_dir`, prints
/// one line with [`Artifact::link_directive`], and returns the path. A missing `out_dir` fails
/// with [`io::ErrorKind::NotFound`]; a failure to print is returned, never a panic.
pub fn build_script_for(
    artifact: Artifact,
    target_os: &str,
    target_env: &str,
    out_dir: Option<&Path>,
    directives: &mut impl Write,
) -> io::Result<Option<PathBuf>> {
    if !links_res_files(target_os, target_env) {
        return Ok(None);
    }
    let out_dir =
        out_dir.ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "OUT_DIR is not set"))?;
    let path = emit(out_dir, artifact)?;
    writeln!(directives, "{}", artifact.link_directive(&path))?;
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::replace_file_with;
    use std::fs;
    use std::io::{self, Write};
    use std::path::PathBuf;

    /// A fresh directory for one unit test, inside the target directory beside the test binary.
    fn scratch_dir(name: &str) -> PathBuf {
        let exe = std::env::current_exe().unwrap();
        let dir = exe
            .parent()
            .unwrap()
            .join("raumakey-resources-unit-tmp")
            .join(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_failed_write_removes_the_partial_temporary_file() {
        let dir = scratch_dir("failed-write");
        let path = dir.join("artifact.res");
        let temporary = dir.join("artifact.res.tmp");
        let error = replace_file_with(&path, &temporary, &[1, 2, 3, 4], |temporary, bytes| {
            // Simulates running out of space halfway through the write.
            let mut file = fs::File::create(temporary)?;
            file.write_all(&bytes[..2])?;
            Err(io::Error::new(io::ErrorKind::StorageFull, "disk full"))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::StorageFull);
        assert!(!temporary.exists(), "partial temporary file left behind");
        assert!(!path.exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_rename_removes_the_temporary_file() {
        let dir = scratch_dir("failed-rename");
        // A non-empty directory where the file should go cannot be replaced by a rename.
        let path = dir.join("artifact.res");
        fs::create_dir_all(path.join("occupied")).unwrap();
        let temporary = dir.join("artifact.res.tmp");
        replace_file_with(&path, &temporary, &[1, 2], |temporary, bytes| {
            fs::write(temporary, bytes)
        })
        .unwrap_err();
        assert!(!temporary.exists(), "temporary file left behind");
        assert!(path.is_dir());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_successful_write_replaces_the_target() {
        let dir = scratch_dir("replace");
        let path = dir.join("artifact.res");
        let temporary = dir.join("artifact.res.tmp");
        fs::write(&path, b"stale").unwrap();
        replace_file_with(&path, &temporary, &[5, 6], |temporary, bytes| {
            fs::write(temporary, bytes)
        })
        .unwrap();
        assert_eq!(fs::read(&path).unwrap(), [5, 6]);
        assert!(!temporary.exists());
        fs::remove_dir_all(&dir).unwrap();
    }
}
