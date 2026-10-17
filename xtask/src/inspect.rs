//! `cargo xtask inspect`: release checks on built PE artifacts.

use std::path::Path;

use raumakey_model::identity::{Guid, TIP_CLSID, TIP_PROFILE_GUID};

use crate::checksum::sha256_hex;
use crate::pe::{self, PeInfo, ResourceId};
use crate::targets::{Artifact, Profile, Target};

/// The exports `raumakey_tip.dll` must expose, and nothing else.
pub const TIP_EXPORTS: [&str; 4] = [
    "DllGetClassObject",
    "DllCanUnloadNow",
    "DllRegisterServer",
    "DllUnregisterServer",
];

/// Result of one check on one artifact.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    /// Stable check identifier, e.g. `machine`.
    pub name: &'static str,
    /// Whether the check passed.
    pub passed: bool,
    /// Human-readable evidence or failure reason.
    pub detail: String,
}

/// One row of the inspection table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    /// Target triple.
    pub target: &'static str,
    /// Artifact file name.
    pub artifact: &'static str,
    /// The check result.
    pub check: Check,
}

/// Runs every check that applies to `artifact` built for `target` with `profile`.
pub fn check_image(
    artifact: Artifact,
    target: Target,
    profile: Profile,
    bytes: &[u8],
) -> Vec<Check> {
    let info = match pe::parse(bytes) {
        Ok(info) => info,
        Err(error) => return vec![fail("pe-format", error.to_string())],
    };
    let mut checks = vec![machine(&info, target), image_type(&info, artifact)];
    match artifact {
        Artifact::Controller => {
            checks.push(subsystem(&info));
            checks.push(manifest(&info));
            checks.push(version_resource(&info));
            checks.push(icon_resource(&info));
        }
        Artifact::Tip => {
            checks.push(tip_exports(&info));
            checks.push(version_resource(&info));
            checks.push(icon_resource(&info));
            checks.push(guid_present("tip-clsid", bytes, TIP_CLSID));
            checks.push(guid_present("tip-profile-guid", bytes, TIP_PROFILE_GUID));
            checks.push(legacy_guids(bytes));
        }
        Artifact::TestRunner => {}
    }
    if profile.is_release() {
        checks.push(static_crt(&info));
    }
    checks
}

/// Reads the artifact at `path` and returns its table rows: a `present` row (size and digest
/// prefix, or why it cannot be read) followed by the [`check_image`] results.
pub fn inspect_file(path: &Path, artifact: Artifact, target: Target, profile: Profile) -> Vec<Row> {
    let row = |check| Row {
        target: target.triple(),
        artifact: artifact.file_name(),
        check,
    };
    match std::fs::read(path) {
        Err(error) => vec![row(fail(
            "present",
            format!(
                "cannot read {}: {error} (run `{}` first)",
                path.display(),
                profile.cross_command()
            ),
        ))],
        Ok(bytes) => {
            let digest = sha256_hex(&bytes);
            let mut rows = vec![row(pass(
                "present",
                format!("{} bytes, sha256 {}...", bytes.len(), &digest[..16]),
            ))];
            rows.extend(
                check_image(artifact, target, profile, &bytes)
                    .into_iter()
                    .map(row),
            );
            rows
        }
    }
}

/// Whether `dll` is a dynamically linked Visual C++ runtime library
/// (`vcruntime140*.dll` or `msvcp*.dll`).
pub fn is_dynamic_crt(dll: &str) -> bool {
    let lower = dll.to_ascii_lowercase();
    (lower.starts_with("vcruntime140") || lower.starts_with("msvcp")) && lower.ends_with(".dll")
}

/// Formats rows as an aligned text table.
pub fn format_table(rows: &[Row]) -> String {
    const HEADER: [&str; 5] = ["TARGET", "ARTIFACT", "CHECK", "RESULT", "DETAIL"];
    let cells: Vec<[&str; 5]> = rows
        .iter()
        .map(|row| {
            [
                row.target,
                row.artifact,
                row.check.name,
                if row.check.passed { "PASS" } else { "FAIL" },
                row.check.detail.as_str(),
            ]
        })
        .collect();
    let mut widths = HEADER.map(str::len);
    for line in &cells {
        for (width, cell) in widths.iter_mut().zip(line) {
            *width = (*width).max(cell.len());
        }
    }
    let mut table = String::new();
    for line in std::iter::once(&HEADER).chain(&cells) {
        let mut text = String::new();
        for (index, cell) in line.iter().enumerate() {
            if index + 1 == line.len() {
                text.push_str(cell);
            } else {
                text.push_str(&format!("{cell:<width$}  ", width = widths[index]));
            }
        }
        table.push_str(text.trim_end());
        table.push('\n');
    }
    table
}

fn pass(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        passed: true,
        detail: detail.into(),
    }
}

fn fail(name: &'static str, detail: impl Into<String>) -> Check {
    Check {
        name,
        passed: false,
        detail: detail.into(),
    }
}

fn machine_name(machine: u16) -> String {
    let name = match machine {
        0x8664 => "AMD64",
        0x014C => "I386",
        0xAA64 => "ARM64",
        _ => "unknown machine",
    };
    format!("{name} (0x{machine:04X})")
}

fn format_name(pe32_plus: bool) -> &'static str {
    if pe32_plus { "PE32+" } else { "PE32" }
}

fn machine(info: &PeInfo<'_>, target: Target) -> Check {
    let found = format!(
        "{}, {}",
        machine_name(info.machine),
        format_name(info.pe32_plus)
    );
    if info.machine == target.machine() && info.pe32_plus == target.is_pe32_plus() {
        pass("machine", found)
    } else {
        fail(
            "machine",
            format!(
                "expected {}, {}; found {found}",
                machine_name(target.machine()),
                format_name(target.is_pe32_plus())
            ),
        )
    }
}

fn image_type(info: &PeInfo<'_>, artifact: Artifact) -> Check {
    let expected_dll = artifact == Artifact::Tip;
    let found = if info.is_dll { "DLL" } else { "EXE" };
    if info.is_dll == expected_dll {
        pass("image-type", found)
    } else {
        fail(
            "image-type",
            format!(
                "expected {}, found {found}",
                if expected_dll { "DLL" } else { "EXE" }
            ),
        )
    }
}

fn subsystem(info: &PeInfo<'_>) -> Check {
    if info.subsystem == pe::SUBSYSTEM_WINDOWS_GUI {
        pass("subsystem", "Windows GUI")
    } else {
        fail(
            "subsystem",
            format!("expected Windows GUI (2), found {}", info.subsystem),
        )
    }
}

fn manifest(info: &PeInfo<'_>) -> Check {
    let manifests: Vec<_> = info.resources_of(pe::RT_MANIFEST).collect();
    let Some(resource) = manifests
        .iter()
        .find(|resource| resource.name == ResourceId::Id(1))
    else {
        return fail(
            "manifest",
            if manifests.is_empty() {
                "no RT_MANIFEST resource".to_owned()
            } else {
                "RT_MANIFEST is not ID 1 (CREATEPROCESS_MANIFEST_RESOURCE_ID)".to_owned()
            },
        );
    };
    let text = pe::decode_manifest(resource.data);
    let level = match pe::requested_execution_level(&text) {
        Ok(level) => level,
        Err(error) => return fail("manifest", error),
    };
    let mut problems = Vec::new();
    match level.level.as_deref() {
        Some("asInvoker") => {}
        Some(other) => problems.push(format!("level=\"{other}\" (expected asInvoker)")),
        None => problems.push("level attribute missing (expected asInvoker)".to_owned()),
    }
    match level.ui_access.as_deref() {
        Some("false") => {}
        Some(other) => problems.push(format!("uiAccess=\"{other}\" (expected false)")),
        None => problems.push("uiAccess attribute missing (expected false)".to_owned()),
    }
    if problems.is_empty() {
        pass("manifest", "ID 1, level=\"asInvoker\", uiAccess=\"false\"")
    } else {
        fail("manifest", problems.join("; "))
    }
}

fn version_resource(info: &PeInfo<'_>) -> Check {
    match info.resources_of(pe::RT_VERSION).next() {
        Some(resource) => pass(
            "version-resource",
            format!("RT_VERSION, {} bytes", resource.data.len()),
        ),
        None => fail("version-resource", "no RT_VERSION resource"),
    }
}

fn icon_resource(info: &PeInfo<'_>) -> Check {
    let groups = info.resources_of(pe::RT_GROUP_ICON).count();
    let icons = info.resources_of(pe::RT_ICON).count();
    if groups > 0 && icons > 0 {
        pass(
            "icon-resource",
            format!("{groups} RT_GROUP_ICON, {icons} RT_ICON"),
        )
    } else {
        fail(
            "icon-resource",
            format!("expected RT_GROUP_ICON and RT_ICON, found {groups} and {icons}"),
        )
    }
}

fn tip_exports(info: &PeInfo<'_>) -> Check {
    let missing: Vec<&str> = TIP_EXPORTS
        .iter()
        .copied()
        .filter(|wanted| !info.exports.iter().any(|name| name == wanted))
        .collect();
    let extra: Vec<&str> = info
        .exports
        .iter()
        .map(String::as_str)
        .filter(|name| !TIP_EXPORTS.contains(name))
        .collect();
    let mut problems = Vec::new();
    if !missing.is_empty() {
        problems.push(format!("missing {}", missing.join(", ")));
    }
    if !extra.is_empty() {
        problems.push(format!("unexpected {}", extra.join(", ")));
    }
    if info.unnamed_exports > 0 {
        problems.push(format!("{} ordinal-only exports", info.unnamed_exports));
    }
    if problems.is_empty() && info.exports.len() == TIP_EXPORTS.len() {
        pass("exports", TIP_EXPORTS.join(", "))
    } else {
        if problems.is_empty() {
            problems.push("duplicate export names".to_owned());
        }
        fail("exports", problems.join("; "))
    }
}

fn guid_present(name: &'static str, bytes: &[u8], guid: Guid) -> Check {
    if pe::contains_guid_binary(bytes, guid) {
        pass(name, format!("{guid} present as GUID struct"))
    } else {
        fail(name, format!("{guid} not found as GUID struct"))
    }
}

fn legacy_guids(bytes: &[u8]) -> Check {
    let mut found = Vec::new();
    for (label, guid) in pe::LEGACY_EVKEY_GUIDS {
        if pe::contains_guid_binary(bytes, guid) {
            found.push(format!("{label} {guid} (binary)"));
        }
        if pe::contains_guid_text(bytes, guid) {
            found.push(format!("{label} {guid} (text)"));
        }
    }
    if found.is_empty() {
        pass("legacy-guids", "no legacy EVKey CLSID or profile GUID")
    } else {
        fail("legacy-guids", format!("found {}", found.join(", ")))
    }
}

fn static_crt(info: &PeInfo<'_>) -> Check {
    let dynamic: Vec<&str> = info
        .imports
        .iter()
        .map(String::as_str)
        .filter(|dll| is_dynamic_crt(dll))
        .collect();
    if dynamic.is_empty() {
        pass("static-crt", "no vcruntime140*/msvcp* imports")
    } else {
        fail("static-crt", format!("imports {}", dynamic.join(", ")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pe::synthetic::PeBuilder;
    use crate::pe::{
        LEGACY_EVKEY_GUIDS, RT_GROUP_ICON, RT_ICON, RT_MANIFEST, RT_VERSION, guid_le_bytes,
    };
    use raumakey_model::identity::{TIP_CLSID, TIP_PROFILE_GUID};

    const GOOD_MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges>
    <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
  </requestedPrivileges></security></trustInfo>
</assembly>"#;

    fn controller() -> PeBuilder {
        let mut builder = PeBuilder::new(0x8664, true, false);
        builder.imports = vec!["KERNEL32.dll".to_owned(), "USER32.dll".to_owned()];
        builder.resources = vec![
            (RT_ICON, 1, vec![1; 40]),
            (RT_GROUP_ICON, 1, vec![2; 20]),
            (RT_VERSION, 1, vec![3; 60]),
            (RT_MANIFEST, 1, GOOD_MANIFEST.as_bytes().to_vec()),
        ];
        builder
    }

    fn tip(machine: u16, pe32_plus: bool) -> PeBuilder {
        let mut builder = PeBuilder::new(machine, pe32_plus, true);
        builder.exports = TIP_EXPORTS.iter().map(|name| (*name).to_owned()).collect();
        builder.imports = vec!["KERNEL32.dll".to_owned(), "ole32.dll".to_owned()];
        builder.resources = vec![
            (RT_ICON, 1, vec![1; 40]),
            (RT_GROUP_ICON, 101, vec![2; 20]),
            (RT_VERSION, 1, vec![3; 60]),
        ];
        builder.payload.extend_from_slice(&guid_le_bytes(TIP_CLSID));
        builder.payload.extend_from_slice(b"Raumakey");
        builder
            .payload
            .extend_from_slice(&guid_le_bytes(TIP_PROFILE_GUID));
        builder
    }

    fn names(checks: &[Check]) -> Vec<&'static str> {
        checks.iter().map(|check| check.name).collect()
    }

    fn find<'a>(checks: &'a [Check], name: &str) -> &'a Check {
        checks
            .iter()
            .find(|check| check.name == name)
            .unwrap_or_else(|| panic!("no `{name}` check in {checks:#?}"))
    }

    fn assert_all_pass(checks: &[Check]) {
        for check in checks {
            assert!(check.passed, "{check:#?}");
        }
    }

    #[test]
    fn good_release_controller_passes_every_check() {
        let image = controller().build();
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Release,
            &image,
        );
        assert_eq!(
            names(&checks),
            [
                "machine",
                "image-type",
                "subsystem",
                "manifest",
                "version-resource",
                "icon-resource",
                "static-crt",
            ]
        );
        assert_all_pass(&checks);
        assert!(find(&checks, "machine").detail.contains("AMD64"));
    }

    #[test]
    fn debug_builds_skip_the_static_crt_check() {
        let mut builder = controller();
        builder.imports.push("VCRUNTIME140.dll".to_owned());
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Debug,
            &builder.build(),
        );
        assert!(!names(&checks).contains(&"static-crt"));
        assert_all_pass(&checks);
    }

    #[test]
    fn controller_manifest_failures() {
        let cases = [
            (
                GOOD_MANIFEST.replace("uiAccess=\"false\"", "uiAccess=\"true\""),
                "uiAccess",
            ),
            (
                GOOD_MANIFEST.replace("asInvoker", "requireAdministrator"),
                "requireAdministrator",
            ),
            (GOOD_MANIFEST.replace(" uiAccess=\"false\"", ""), "uiAccess"),
            ("<assembly/>".to_owned(), "requestedExecutionLevel"),
        ];
        for (manifest, expected) in cases {
            let mut builder = controller();
            builder.resources[3].2 = manifest.into_bytes();
            let checks = check_image(
                Artifact::Controller,
                Target::X86_64,
                Profile::Release,
                &builder.build(),
            );
            let check = find(&checks, "manifest");
            assert!(!check.passed, "{check:#?}");
            assert!(check.detail.contains(expected), "{check:#?}");
        }
    }

    #[test]
    fn controller_manifest_must_use_resource_id_1() {
        let mut builder = controller();
        builder.resources[3].1 = 2;
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Release,
            &builder.build(),
        );
        let check = find(&checks, "manifest");
        assert!(!check.passed);
        assert!(check.detail.contains("ID 1"), "{check:#?}");

        builder.resources.remove(3);
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Release,
            &builder.build(),
        );
        assert!(!find(&checks, "manifest").passed);
    }

    #[test]
    fn controller_type_subsystem_and_resource_failures() {
        let mut console = controller();
        console.subsystem = 3;
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Debug,
            &console.build(),
        );
        assert!(!find(&checks, "subsystem").passed);

        let mut dll = controller();
        dll.dll = true;
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Debug,
            &dll.build(),
        );
        assert!(!find(&checks, "image-type").passed);

        let mut bare = controller();
        bare.resources.retain(|(kind, _, _)| *kind == RT_MANIFEST);
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Debug,
            &bare.build(),
        );
        assert!(!find(&checks, "version-resource").passed);
        assert!(!find(&checks, "icon-resource").passed);

        let mut group_only = controller();
        group_only.resources.retain(|(kind, _, _)| *kind != RT_ICON);
        let checks = check_image(
            Artifact::Controller,
            Target::X86_64,
            Profile::Debug,
            &group_only.build(),
        );
        assert!(!find(&checks, "icon-resource").passed);
    }

    #[test]
    fn good_release_tip_passes_every_check() {
        let image = tip(0x014C, false).build();
        let checks = check_image(Artifact::Tip, Target::I686, Profile::Release, &image);
        assert_eq!(
            names(&checks),
            [
                "machine",
                "image-type",
                "exports",
                "version-resource",
                "icon-resource",
                "tip-clsid",
                "tip-profile-guid",
                "legacy-guids",
                "static-crt",
            ]
        );
        assert_all_pass(&checks);
    }

    #[test]
    fn tip_export_failures_name_the_offending_symbols() {
        let mut extra = tip(0x8664, true);
        extra.exports.push("DllMain".to_owned());
        let checks = check_image(
            Artifact::Tip,
            Target::X86_64,
            Profile::Debug,
            &extra.build(),
        );
        let check = find(&checks, "exports");
        assert!(!check.passed);
        assert!(check.detail.contains("DllMain"), "{check:#?}");

        let mut missing = tip(0x8664, true);
        missing.exports.retain(|name| name != "DllRegisterServer");
        let checks = check_image(
            Artifact::Tip,
            Target::X86_64,
            Profile::Debug,
            &missing.build(),
        );
        let check = find(&checks, "exports");
        assert!(!check.passed);
        assert!(check.detail.contains("DllRegisterServer"), "{check:#?}");

        let mut ordinal = tip(0x8664, true);
        ordinal.ordinal_only_exports = 1;
        let checks = check_image(
            Artifact::Tip,
            Target::X86_64,
            Profile::Debug,
            &ordinal.build(),
        );
        assert!(!find(&checks, "exports").passed);
    }

    #[test]
    fn tip_identity_failures() {
        let mut no_profile = tip(0x8664, true);
        no_profile.payload = guid_le_bytes(TIP_CLSID).to_vec();
        let checks = check_image(
            Artifact::Tip,
            Target::X86_64,
            Profile::Debug,
            &no_profile.build(),
        );
        assert!(find(&checks, "tip-clsid").passed);
        assert!(!find(&checks, "tip-profile-guid").passed);

        for (label, guid) in LEGACY_EVKEY_GUIDS {
            let mut binary = tip(0x8664, true);
            binary.payload.extend_from_slice(&guid_le_bytes(guid));
            let checks = check_image(
                Artifact::Tip,
                Target::X86_64,
                Profile::Debug,
                &binary.build(),
            );
            let check = find(&checks, "legacy-guids");
            assert!(!check.passed, "{label}");
            assert!(check.detail.contains(label), "{check:#?}");

            let mut text = tip(0x8664, true);
            let braced = format!("{{{}}}", pe::guid_text(guid).to_ascii_lowercase());
            text.payload
                .extend(braced.encode_utf16().flat_map(u16::to_le_bytes));
            let checks = check_image(Artifact::Tip, Target::X86_64, Profile::Debug, &text.build());
            assert!(!find(&checks, "legacy-guids").passed, "{label} as text");
        }
    }

    #[test]
    fn release_artifacts_must_not_import_the_vc_runtime() {
        let mut builder = tip(0xAA64, true);
        builder.delay_imports = vec!["VCRUNTIME140.dll".to_owned()];
        let checks = check_image(
            Artifact::Tip,
            Target::Aarch64,
            Profile::Release,
            &builder.build(),
        );
        let check = find(&checks, "static-crt");
        assert!(!check.passed);
        assert!(check.detail.contains("VCRUNTIME140.dll"), "{check:#?}");
    }

    #[test]
    fn dynamic_crt_names() {
        for dll in [
            "VCRUNTIME140.dll",
            "vcruntime140_1.dll",
            "vcruntime140d.dll",
            "MSVCP140.dll",
            "msvcp140_atomic_wait.dll",
        ] {
            assert!(is_dynamic_crt(dll), "{dll}");
        }
        for dll in [
            "KERNEL32.dll",
            "ucrtbase.dll",
            "api-ms-win-crt-runtime-l1-1-0.dll",
            "msvcrt.dll",
            "vcruntime140.exe",
        ] {
            assert!(!is_dynamic_crt(dll), "{dll}");
        }
    }

    #[test]
    fn machine_and_format_must_match_the_target() {
        let x64 = tip(0x8664, true).build();
        let checks = check_image(Artifact::Tip, Target::Aarch64, Profile::Debug, &x64);
        let check = find(&checks, "machine");
        assert!(!check.passed);
        assert!(check.detail.contains("ARM64"), "{check:#?}");
        assert!(check.detail.contains("AMD64"), "{check:#?}");

        let pe32_amd64 = tip(0x8664, false).build();
        let checks = check_image(Artifact::Tip, Target::X86_64, Profile::Debug, &pe32_amd64);
        assert!(!find(&checks, "machine").passed);
    }

    #[test]
    fn test_runner_checks() {
        let mut runner = PeBuilder::new(0x8664, true, false);
        runner.subsystem = 3;
        runner.imports = vec!["KERNEL32.dll".to_owned()];
        let checks = check_image(
            Artifact::TestRunner,
            Target::X86_64,
            Profile::Release,
            &runner.build(),
        );
        assert_eq!(names(&checks), ["machine", "image-type", "static-crt"]);
        assert_all_pass(&checks);
    }

    #[test]
    fn unparsable_images_fail_one_check() {
        let checks = check_image(
            Artifact::Tip,
            Target::X86_64,
            Profile::Release,
            b"MZ but not really",
        );
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].name, "pe-format");
        assert!(!checks[0].passed);
    }

    #[test]
    fn inspect_file_reports_presence_then_checks() {
        let dir =
            std::env::temp_dir().join(format!("raumakey-xtask-inspect-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("raumakey_tip.dll");
        let image = tip(0x8664, true).build();
        std::fs::write(&path, &image).expect("write image");
        let rows = inspect_file(&path, Artifact::Tip, Target::X86_64, Profile::Debug);
        let missing = inspect_file(
            &dir.join("absent.dll"),
            Artifact::Tip,
            Target::X86_64,
            Profile::Release,
        );
        std::fs::remove_dir_all(&dir).expect("cleanup");

        assert_eq!(rows[0].check.name, "present");
        assert!(rows[0].check.passed);
        assert!(
            rows[0]
                .check
                .detail
                .starts_with(&format!("{} bytes, sha256 ", image.len())),
            "{:#?}",
            rows[0]
        );
        assert!(
            rows.iter()
                .all(|row| row.target == "x86_64-pc-windows-msvc")
        );
        assert!(rows.iter().all(|row| row.artifact == "raumakey_tip.dll"));
        let checks: Vec<Check> = rows[1..].iter().map(|row| row.check.clone()).collect();
        assert_eq!(
            checks,
            check_image(Artifact::Tip, Target::X86_64, Profile::Debug, &image)
        );

        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].check.name, "present");
        assert!(!missing[0].check.passed);
        assert!(
            missing[0]
                .check
                .detail
                .contains("cargo xtask cross --release"),
            "{:#?}",
            missing[0]
        );
    }

    #[test]
    fn table_is_aligned_with_pass_and_fail_markers() {
        let rows = [
            Row {
                target: "x86_64-pc-windows-msvc",
                artifact: "raumakey.exe",
                check: Check {
                    name: "machine",
                    passed: true,
                    detail: "AMD64 (0x8664), PE32+".to_owned(),
                },
            },
            Row {
                target: "i686-pc-windows-msvc",
                artifact: "raumakey_tip.dll",
                check: Check {
                    name: "legacy-guids",
                    passed: false,
                    detail: "found legacy EVKey CLSID".to_owned(),
                },
            },
        ];
        let table = format_table(&rows);
        let lines: Vec<&str> = table.lines().collect();
        assert_eq!(lines.len(), 3, "{table}");
        assert!(lines[0].starts_with("TARGET"));
        let result_column = lines[0].find("RESULT").expect("header has RESULT");
        assert_eq!(&lines[1][result_column..result_column + 4], "PASS");
        assert_eq!(&lines[2][result_column..result_column + 4], "FAIL");
        let detail_column = lines[0].find("DETAIL").expect("header has DETAIL");
        assert!(lines[2][detail_column..].starts_with("found legacy EVKey CLSID"));
        assert!(table.ends_with('\n'));
    }
}
