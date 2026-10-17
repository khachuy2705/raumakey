//! Application manifest content and storage.

use crate::support::{XmlElement, find, parse_res, parse_xml};
use raumakey_resources::{
    ExecutionLevel, LANG_VIETNAMESE, MANIFEST_ID, Manifest, RT_MANIFEST, ResWriter, ResourceError,
};

const ASM_V1: &str = "urn:schemas-microsoft-com:asm.v1";
const ASM_V3: &str = "urn:schemas-microsoft-com:asm.v3";
const COMPATIBILITY_V1: &str = "urn:schemas-microsoft-com:compatibility.v1";
const WINDOWS_SETTINGS_2005: &str = "http://schemas.microsoft.com/SMI/2005/WindowsSettings";
const WINDOWS_SETTINGS_2016: &str = "http://schemas.microsoft.com/SMI/2016/WindowsSettings";
const WINDOWS_SETTINGS_2019: &str = "http://schemas.microsoft.com/SMI/2019/WindowsSettings";
const WINDOWS_10_AND_11: &str = "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}";

/// Asserts the settings shared by every Raumakey executable manifest.
fn assert_common_settings(root: &XmlElement) {
    assert_eq!(root.name, "assembly");
    assert_eq!(root.attr("xmlns"), Some(ASM_V1));
    assert_eq!(root.attr("manifestVersion"), Some("1.0"));

    let trust = root.find(&["trustInfo"]);
    assert_eq!(trust.attr("xmlns"), Some(ASM_V3));
    let level = root.find(&[
        "trustInfo",
        "security",
        "requestedPrivileges",
        "requestedExecutionLevel",
    ]);
    assert_eq!(level.attr("uiAccess"), Some("false"));

    let common_controls = root.find(&["dependency", "dependentAssembly", "assemblyIdentity"]);
    assert_eq!(common_controls.attr("type"), Some("win32"));
    assert_eq!(
        common_controls.attr("name"),
        Some("Microsoft.Windows.Common-Controls")
    );
    assert_eq!(common_controls.attr("version"), Some("6.0.0.0"));
    assert_eq!(common_controls.attr("processorArchitecture"), Some("*"));
    assert_eq!(
        common_controls.attr("publicKeyToken"),
        Some("6595b64144ccf1df")
    );
    assert_eq!(common_controls.attr("language"), Some("*"));

    let compatibility = root.find(&["compatibility"]);
    assert_eq!(compatibility.attr("xmlns"), Some(COMPATIBILITY_V1));
    let supported = root.find(&["compatibility", "application", "supportedOS"]);
    assert_eq!(supported.attr("Id"), Some(WINDOWS_10_AND_11));

    let settings_parent = root
        .find_all(&["application"])
        .into_iter()
        .find(|element| element.attr("xmlns") == Some(ASM_V3))
        .expect("asm.v3 application element");
    let settings = settings_parent.find(&["windowsSettings"]);
    let dpi_aware = settings.find(&["dpiAware"]);
    assert_eq!(dpi_aware.attr("xmlns"), Some(WINDOWS_SETTINGS_2005));
    assert_eq!(dpi_aware.text, "true/pm");
    let dpi_awareness = settings.find(&["dpiAwareness"]);
    assert_eq!(dpi_awareness.attr("xmlns"), Some(WINDOWS_SETTINGS_2016));
    assert_eq!(dpi_awareness.text, "PerMonitorV2, PerMonitor");
    let code_page = settings.find(&["activeCodePage"]);
    assert_eq!(code_page.attr("xmlns"), Some(WINDOWS_SETTINGS_2019));
    assert_eq!(code_page.text, "UTF-8");
}

#[test]
fn controller_manifest_declares_required_settings() {
    let manifest = Manifest::controller([0, 1, 0, 0]);
    assert_eq!(manifest.identity_name(), "Raumakey.Controller");
    assert_eq!(manifest.execution_level(), ExecutionLevel::AsInvoker);
    let xml = manifest.to_xml();
    assert!(xml.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>"));
    let root = parse_xml(&xml);
    assert_common_settings(&root);

    let identity = root.find(&["assemblyIdentity"]);
    assert_eq!(identity.attr("type"), Some("win32"));
    assert_eq!(identity.attr("name"), Some("Raumakey.Controller"));
    assert_eq!(identity.attr("version"), Some("0.1.0.0"));
    assert_eq!(
        root.find(&["description"]).text,
        "Raumakey Vietnamese input method"
    );
    let level = root.find(&[
        "trustInfo",
        "security",
        "requestedPrivileges",
        "requestedExecutionLevel",
    ]);
    assert_eq!(level.attr("level"), Some("asInvoker"));
}

#[test]
fn executable_manifest_uses_the_given_identity() {
    let manifest = Manifest::executable(
        "Raumakey.TestRunner",
        "Raumakey acceptance test runner",
        [1, 2, 3, 4],
    )
    .unwrap();
    let root = parse_xml(&manifest.to_xml());
    assert_common_settings(&root);
    let identity = root.find(&["assemblyIdentity"]);
    assert_eq!(identity.attr("name"), Some("Raumakey.TestRunner"));
    assert_eq!(identity.attr("version"), Some("1.2.3.4"));
    assert_eq!(
        root.find(&["description"]).text,
        "Raumakey acceptance test runner"
    );
}

#[test]
fn execution_level_is_configurable_but_ui_access_stays_false() {
    for (level, text) in [
        (ExecutionLevel::AsInvoker, "asInvoker"),
        (ExecutionLevel::HighestAvailable, "highestAvailable"),
        (ExecutionLevel::RequireAdministrator, "requireAdministrator"),
    ] {
        let manifest = Manifest::controller([1, 0, 0, 0]).with_execution_level(level);
        assert_eq!(manifest.execution_level(), level);
        let root = parse_xml(&manifest.to_xml());
        let element = root.find(&[
            "trustInfo",
            "security",
            "requestedPrivileges",
            "requestedExecutionLevel",
        ]);
        assert_eq!(element.attr("level"), Some(text));
        assert_eq!(element.attr("uiAccess"), Some("false"));
    }
}

#[test]
fn description_is_escaped() {
    let manifest = Manifest::executable(
        "Raumakey.Tool",
        "Tiếng Việt <&> \"quoted\" 'single'",
        [1, 0, 0, 0],
    )
    .unwrap();
    let xml = manifest.to_xml();
    assert!(!xml.contains("<&>"));
    let root = parse_xml(&xml);
    assert_eq!(
        root.find(&["description"]).text,
        "Tiếng Việt <&> \"quoted\" 'single'"
    );
}

#[test]
fn rejects_invalid_identity_names_and_descriptions() {
    for name in ["", "has space", "a<b", "quote\"", "Tiếng", "x/y"] {
        assert_eq!(
            Manifest::executable(name, "ok", [1, 0, 0, 0]),
            Err(ResourceError::InvalidText),
            "{name:?}"
        );
    }
    let too_long = "a".repeat(257);
    assert_eq!(
        Manifest::executable(&too_long, "ok", [1, 0, 0, 0]),
        Err(ResourceError::InvalidText)
    );
    for description in ["nul\0", "bell\u{7}", "escape\u{1b}"] {
        assert_eq!(
            Manifest::executable("Raumakey.Tool", description, [1, 0, 0, 0]),
            Err(ResourceError::InvalidText)
        );
    }
    assert!(Manifest::executable("Raumakey.Tool-2_x", "multi\nline\ttext", [1, 0, 0, 0]).is_ok());
    assert!(Manifest::executable("Raumakey.Tool", "carriage\r\nreturn", [1, 0, 0, 0]).is_ok());
}

#[test]
fn writer_stores_the_manifest_as_rt_manifest_one() {
    let manifest = Manifest::controller([0, 1, 0, 0]);
    let mut writer = ResWriter::with_language(LANG_VIETNAMESE);
    writer.add_manifest(&manifest).unwrap();
    assert_eq!(
        writer.add_manifest(&manifest),
        Err(ResourceError::Duplicate)
    );
    let entries = parse_res(&writer.to_bytes());
    assert_eq!(MANIFEST_ID, 1);
    let entry = find(&entries, RT_MANIFEST, MANIFEST_ID);
    assert_eq!(entry.language, LANG_VIETNAMESE);
    assert_eq!(entry.memory_flags, 0x0030, "MOVEABLE | PURE");
    assert!(entry.data.starts_with(b"<?xml"), "UTF-8 without a BOM");
    let xml = String::from_utf8(entry.data.clone()).unwrap();
    assert_eq!(xml, manifest.to_xml());
    assert_common_settings(&parse_xml(&xml));
}

#[test]
fn rejects_descriptions_with_noncharacters_or_c1_controls() {
    for description in [
        "\u{FFFE}",
        "end\u{FFFF}",
        "\u{7F}",
        "\u{80}",
        "next line\u{85}",
        "\u{9F}",
    ] {
        assert_eq!(
            Manifest::executable("Raumakey.Tool", description, [1, 0, 0, 0]),
            Err(ResourceError::InvalidText),
            "{description:?}"
        );
    }
    // Printable neighbours of those ranges are fine: no-break space, the replacement
    // character, and a character outside the BMP.
    let description = "a\u{A0}b\u{FFFD}\u{1D519}";
    let manifest = Manifest::executable("Raumakey.Tool", description, [1, 0, 0, 0]).unwrap();
    assert_eq!(
        parse_xml(&manifest.to_xml()).find(&["description"]).text,
        description
    );
}
