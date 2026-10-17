//! Build-script entry point: one `.res` file per Raumakey artifact.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::support::{
    Entry, find, of_type, parse_res, parse_version_node, parse_xml, u16_at, u32_at,
};
use raumakey_resources::{
    Artifact, FileType, ICON_APP, ICON_ENGLISH, ICON_VIETNAMESE, IconGlyph, IconImage,
    PRODUCT_VERSION, RT_GROUP_ICON, RT_ICON, RT_MANIFEST, RT_VERSION, TIP_PROFILE_ICON_INDEX,
    build_script_for, emit, emit_with_version, links_res_files, render_icon_set, version_quad,
};

/// A fresh directory under Cargo's per-target temporary directory.
fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("raumakey-resources-emit")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// The `RT_ICON` images referenced by an icon group, in directory order.
fn group_images(entries: &[Entry], group: u16) -> Vec<Vec<u8>> {
    let directory = &find(entries, RT_GROUP_ICON, group).data;
    let count = usize::from(u16_at(directory, 4));
    (0..count)
        .map(|index| {
            let id = u16_at(directory, 6 + 14 * index + 12);
            find(entries, RT_ICON, id).data.clone()
        })
        .collect()
}

fn expected_images(glyph: IconGlyph) -> Vec<Vec<u8>> {
    render_icon_set(glyph)
        .unwrap()
        .iter()
        .map(IconImage::to_dib)
        .collect()
}

fn dotted_version() -> String {
    let [a, b, c, d] = version_quad(PRODUCT_VERSION).unwrap();
    format!("{a}.{b}.{c}.{d}")
}

fn read_entries(path: &Path) -> Vec<Entry> {
    parse_res(&fs::read(path).unwrap())
}

#[test]
fn icon_ids_and_profile_index_are_stable() {
    assert_eq!(ICON_APP, 1);
    assert_eq!(ICON_VIETNAMESE, 2);
    assert_eq!(ICON_ENGLISH, 3);
    assert_eq!(TIP_PROFILE_ICON_INDEX, 0);
    assert_eq!(PRODUCT_VERSION, env!("CARGO_PKG_VERSION"));
}

#[test]
fn artifacts_use_raumakey_file_names() {
    let expected = [
        (
            Artifact::Controller,
            "raumakey",
            "raumakey.exe",
            FileType::App,
        ),
        (
            Artifact::Tip,
            "raumakey_tip",
            "raumakey_tip.dll",
            FileType::Dll,
        ),
        (
            Artifact::TestRunner,
            "raumakey-test-runner",
            "raumakey-test-runner.exe",
            FileType::App,
        ),
    ];
    for (artifact, stem, file_name, file_type) in expected {
        assert_eq!(artifact.file_stem(), stem);
        assert_eq!(artifact.original_filename(), file_name);
        assert_eq!(artifact.file_type(), file_type);
        assert!(artifact.description().starts_with("Raumakey "));
    }
}

#[test]
fn controller_res_has_manifest_version_and_three_icon_groups() {
    let dir = scratch_dir("controller");
    let path = emit(&dir, Artifact::Controller).unwrap();
    assert_eq!(path, dir.join("raumakey.res"));
    let entries = read_entries(&path);

    let manifest = find(&entries, RT_MANIFEST, 1);
    let xml = parse_xml(std::str::from_utf8(&manifest.data).unwrap());
    assert_eq!(
        xml.find(&["assemblyIdentity"]).attr("name"),
        Some("Raumakey.Controller")
    );
    assert_eq!(
        xml.find(&["assemblyIdentity"]).attr("version"),
        Some(dotted_version().as_str())
    );
    let level = xml.find(&[
        "trustInfo",
        "security",
        "requestedPrivileges",
        "requestedExecutionLevel",
    ]);
    assert_eq!(level.attr("level"), Some("asInvoker"));
    assert_eq!(level.attr("uiAccess"), Some("false"));

    let version = find(&entries, RT_VERSION, 1);
    let root = parse_version_node(&version.data, 0);
    assert_eq!(usize::from(root.length), version.data.len());
    assert_eq!(u32_at(&root.value, 0), 0xFEEF_04BD);
    assert_eq!(u32_at(&root.value, 36), 1, "VFT_APP");
    let table = root.child("StringFileInfo").child("040904B0");
    assert_eq!(table.child("CompanyName").text(), "Raumakey contributors");
    assert_eq!(table.child("ProductName").text(), "Raumakey");
    assert_eq!(table.child("InternalName").text(), "raumakey");
    assert_eq!(table.child("OriginalFilename").text(), "raumakey.exe");
    assert_eq!(table.child("FileVersion").text(), dotted_version());
    assert_eq!(table.child("ProductVersion").text(), PRODUCT_VERSION);
    assert_eq!(
        table.child("FileDescription").text(),
        Artifact::Controller.description()
    );

    assert_eq!(of_type(&entries, RT_GROUP_ICON).len(), 3);
    assert_eq!(of_type(&entries, RT_ICON).len(), 15);
    assert_eq!(
        group_images(&entries, ICON_APP),
        expected_images(IconGlyph::AppMark)
    );
    assert_eq!(
        group_images(&entries, ICON_VIETNAMESE),
        expected_images(IconGlyph::Vietnamese)
    );
    assert_eq!(
        group_images(&entries, ICON_ENGLISH),
        expected_images(IconGlyph::English)
    );
}

#[test]
fn tip_res_has_version_and_icons_but_no_manifest() {
    let dir = scratch_dir("tip");
    let path = emit(&dir, Artifact::Tip).unwrap();
    assert_eq!(path, dir.join("raumakey_tip.res"));
    let entries = read_entries(&path);

    assert!(
        of_type(&entries, RT_MANIFEST).is_empty(),
        "DLLs get no manifest"
    );
    let root = parse_version_node(&find(&entries, RT_VERSION, 1).data, 0);
    assert_eq!(u32_at(&root.value, 36), 2, "VFT_DLL");
    let table = root.child("StringFileInfo").child("040904B0");
    assert_eq!(table.child("OriginalFilename").text(), "raumakey_tip.dll");
    assert_eq!(table.child("InternalName").text(), "raumakey_tip");

    // TSF registers the profile icon by index; index 0 is the lowest icon group ordinal.
    let groups = of_type(&entries, RT_GROUP_ICON);
    assert_eq!(groups.len(), 3);
    assert_eq!(
        group_images(&entries, ICON_APP),
        expected_images(IconGlyph::AppMark)
    );
    assert_eq!(
        group_images(&entries, ICON_VIETNAMESE),
        expected_images(IconGlyph::Vietnamese)
    );
    assert_eq!(
        group_images(&entries, ICON_ENGLISH),
        expected_images(IconGlyph::English)
    );
}

#[test]
fn test_runner_res_has_manifest_version_and_app_icon() {
    let dir = scratch_dir("runner");
    let path = emit(&dir, Artifact::TestRunner).unwrap();
    assert_eq!(path, dir.join("raumakey-test-runner.res"));
    let entries = read_entries(&path);

    let xml = parse_xml(std::str::from_utf8(&find(&entries, RT_MANIFEST, 1).data).unwrap());
    assert_eq!(
        xml.find(&["assemblyIdentity"]).attr("name"),
        Some("Raumakey.TestRunner")
    );
    let root = parse_version_node(&find(&entries, RT_VERSION, 1).data, 0);
    assert_eq!(u32_at(&root.value, 36), 1, "VFT_APP");
    let table = root.child("StringFileInfo").child("040904B0");
    assert_eq!(
        table.child("OriginalFilename").text(),
        "raumakey-test-runner.exe"
    );
    assert_eq!(of_type(&entries, RT_GROUP_ICON).len(), 1);
    assert_eq!(
        group_images(&entries, ICON_APP),
        expected_images(IconGlyph::AppMark)
    );
}

#[test]
fn emit_is_deterministic_and_leaves_no_temporary_files() {
    let dir = scratch_dir("idempotent");
    let first = emit(&dir, Artifact::Tip).unwrap();
    let bytes = fs::read(&first).unwrap();
    let second = emit(&dir, Artifact::Tip).unwrap();
    assert_eq!(first, second);
    assert_eq!(fs::read(&second).unwrap(), bytes);
    fs::write(&first, b"stale").unwrap();
    emit(&dir, Artifact::Tip).unwrap();
    assert_eq!(
        fs::read(&first).unwrap(),
        bytes,
        "stale content is replaced"
    );
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["raumakey_tip.res"]);
}

#[test]
fn emit_creates_missing_output_directories() {
    let dir = scratch_dir("nested").join("a").join("b");
    let path = emit(&dir, Artifact::Controller).unwrap();
    assert!(path.is_file());
}

#[test]
fn emit_with_version_uses_the_given_version() {
    let dir = scratch_dir("versioned");
    let path = emit_with_version(&dir, Artifact::Controller, "2.3.4-rc.1").unwrap();
    let entries = read_entries(&path);
    let root = parse_version_node(&find(&entries, RT_VERSION, 1).data, 0);
    assert_eq!(u32_at(&root.value, 8), 0x0002_0003, "dwFileVersionMS");
    assert_eq!(u32_at(&root.value, 12), 0x0004_0000, "dwFileVersionLS");
    assert_eq!(u32_at(&root.value, 28), 0x02, "VS_FF_PRERELEASE");
    let table = root.child("StringFileInfo").child("040904B0");
    assert_eq!(table.child("ProductVersion").text(), "2.3.4-rc.1");
    let xml = parse_xml(std::str::from_utf8(&find(&entries, RT_MANIFEST, 1).data).unwrap());
    assert_eq!(
        xml.find(&["assemblyIdentity"]).attr("version"),
        Some("2.3.4.0")
    );
}

#[test]
fn emit_reports_invalid_versions_and_io_failures() {
    let dir = scratch_dir("errors");
    let error = emit_with_version(&dir, Artifact::Controller, "2.3").unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    assert!(!dir.join("raumakey.res").exists());

    let blocker = dir.join("not-a-directory");
    fs::write(&blocker, b"x").unwrap();
    assert!(emit(&blocker, Artifact::Tip).is_err());
}

#[test]
fn link_directives_target_each_artifact_kind() {
    let res = Path::new("C:/out/x.res");
    assert_eq!(
        Artifact::Controller.link_directive(res),
        "cargo:rustc-link-arg-bin=raumakey=C:/out/x.res"
    );
    assert_eq!(
        Artifact::TestRunner.link_directive(res),
        "cargo:rustc-link-arg-bin=raumakey-test-runner=C:/out/x.res"
    );
    assert_eq!(
        Artifact::Tip.link_directive(res),
        "cargo:rustc-link-arg-cdylib=C:/out/x.res"
    );
}

#[test]
fn only_windows_msvc_targets_link_res_files() {
    assert!(links_res_files("windows", "msvc"));
    assert!(!links_res_files("windows", "gnu"));
    assert!(!links_res_files("linux", "gnu"));
    assert!(!links_res_files("macos", ""));
}

#[test]
fn build_script_writes_the_res_and_prints_the_link_directive_on_windows_msvc() {
    for (artifact, directive) in [
        (Artifact::Controller, "cargo:rustc-link-arg-bin=raumakey="),
        (
            Artifact::TestRunner,
            "cargo:rustc-link-arg-bin=raumakey-test-runner=",
        ),
        (Artifact::Tip, "cargo:rustc-link-arg-cdylib="),
    ] {
        let dir = scratch_dir(&format!("build-script-{}", artifact.file_stem()));
        let mut printed = Vec::new();
        let path = build_script_for(
            artifact,
            "windows",
            "msvc",
            Some(dir.as_path()),
            &mut printed,
        )
        .unwrap()
        .expect("windows-msvc links .res files");
        assert_eq!(path, dir.join(format!("{}.res", artifact.file_stem())));
        assert_eq!(
            fs::read(&path).unwrap(),
            artifact.resources(PRODUCT_VERSION).unwrap().to_bytes()
        );
        assert_eq!(
            String::from_utf8(printed).unwrap(),
            format!("{directive}{}\n", path.display())
        );
    }
}

#[test]
fn build_script_does_nothing_for_targets_without_res_support() {
    let dir = scratch_dir("build-script-other-targets");
    for (os, env) in [
        ("windows", "gnu"),
        ("windows", "gnullvm"),
        ("linux", "gnu"),
        ("macos", ""),
        ("", ""),
    ] {
        for out_dir in [Some(dir.as_path()), None] {
            let mut printed = Vec::new();
            let result = build_script_for(Artifact::Controller, os, env, out_dir, &mut printed);
            assert_eq!(result.unwrap(), None, "{os}/{env}");
            assert!(printed.is_empty(), "{os}/{env} printed a directive");
        }
    }
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 0, "nothing is written");
}

#[test]
fn build_script_requires_out_dir_on_windows_msvc() {
    let mut printed = Vec::new();
    let error =
        build_script_for(Artifact::Controller, "windows", "msvc", None, &mut printed).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(printed.is_empty());
}

/// Standard output of a build script whose reader went away.
struct ClosedPipe;

impl io::Write for ClosedPipe {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::ErrorKind::BrokenPipe.into())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn build_script_reports_directive_write_failures() {
    let dir = scratch_dir("build-script-closed-pipe");
    let error = build_script_for(
        Artifact::Tip,
        "windows",
        "msvc",
        Some(dir.as_path()),
        &mut ClosedPipe,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
}

/// Puts a file's original permissions back when dropped, even when the test fails.
struct RestorePermissions {
    path: PathBuf,
    original: fs::Permissions,
}

impl Drop for RestorePermissions {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, self.original.clone());
    }
}

#[test]
fn emit_leaves_an_identical_file_untouched() {
    let dir = scratch_dir("untouched");
    let path = emit(&dir, Artifact::Tip).unwrap();
    // A distinctive old time stamp: any rewrite would replace it with the current time.
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let original = fs::metadata(&path).unwrap().permissions();
    let mut read_only = original.clone();
    read_only.set_readonly(true);
    fs::set_permissions(&path, read_only).unwrap();
    let _restore = RestorePermissions {
        path: path.clone(),
        original,
    };

    // On Windows a rewrite could not even rename a new file over the read-only one.
    assert_eq!(emit(&dir, Artifact::Tip).unwrap(), path);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), old);
    let names: Vec<String> = fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(names, ["raumakey_tip.res"]);
}
