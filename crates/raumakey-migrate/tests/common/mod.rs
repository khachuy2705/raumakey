//! Shared invented fixtures. None of the setting names or values is EVKey data.

#![allow(dead_code)]

use raumakey_config::ConfigDocument;
use raumakey_migrate::{
    FakeFs, FakeRegistry, FieldMapping, InvalidSetting, LegacyValue, SETTINGS_REGISTRY_PATH,
    SettingValue, TSF_CACHE_REGISTRY_PATH,
};

/// Folder that holds the invented EVKey files in [`evkey_fs`].
pub const FOLDER: &str = "evkey";

pub const SETTING_INI: &[u8] = include_bytes!("../fixtures/evkey-basic/setting.ini");
pub const SETTING_TREE: &[u8] = include_bytes!("../fixtures/evkey-basic/setting.tree");
pub const MACRO_TEXT: &[u8] = include_bytes!("../fixtures/evkey-basic/evkmacro.txt");

/// Settings below [`SETTINGS_REGISTRY_PATH`] and two TSF cache hints.
pub fn evkey_registry() -> FakeRegistry {
    let mut registry = FakeRegistry::new();
    registry
        .set_value(SETTINGS_REGISTRY_PATH, "SampleMode", LegacyValue::Dword(1))
        .set_value(SETTINGS_REGISTRY_PATH, "SampleTone", LegacyValue::Dword(1))
        .set_value(
            SETTINGS_REGISTRY_PATH,
            "SampleApps",
            LegacyValue::MultiString(vec!["chat.exe".into(), "Mail.EXE".into()]),
        )
        .set_value(
            SETTINGS_REGISTRY_PATH,
            "SampleTitle",
            LegacyValue::String("Bộ gõ".into()),
        )
        .set_value(
            &format!("{SETTINGS_REGISTRY_PATH}\\Hotkeys"),
            "SampleToggle",
            LegacyValue::Dword(0x11),
        )
        .set_value(
            &format!("{TSF_CACHE_REGISTRY_PATH}\\v1"),
            "notepad.exe",
            LegacyValue::Dword(1),
        )
        .set_value(
            &format!("{TSF_CACHE_REGISTRY_PATH}\\v1"),
            "winword.exe",
            LegacyValue::Dword(0),
        );
    registry
}

/// The three fixture files in [`FOLDER`].
pub fn evkey_fs() -> FakeFs {
    let mut fs = FakeFs::new();
    fs.add_file(format!("{FOLDER}/setting.ini"), SETTING_INI.to_vec())
        .add_file(format!("{FOLDER}/setting.tree"), SETTING_TREE.to_vec())
        .add_file(format!("{FOLDER}/evkmacro.txt"), MACRO_TEXT.to_vec());
    fs
}

/// A file system with only the folder and the given files.
pub fn fs_with(files: &[(&str, &[u8])]) -> FakeFs {
    let mut fs = FakeFs::new();
    fs.add_dir(FOLDER);
    for (name, bytes) in files {
        fs.add_file(format!("{FOLDER}/{name}"), bytes.to_vec());
    }
    fs
}

/// UTF-16 LE text with a byte order mark.
pub fn utf16le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

/// Invented mapping used to exercise FIELD_MAP handling: a 0/1 flag stored as a DWORD in the
/// registry or as `0`/`1` in `setting.ini`, imported as `input.modern_tone`.
pub fn apply_modern_tone(
    document: &mut ConfigDocument,
    value: SettingValue<'_>,
) -> Result<(), InvalidSetting> {
    let flag = match value {
        SettingValue::Registry(LegacyValue::Dword(0)) | SettingValue::Ini("0") => false,
        SettingValue::Registry(LegacyValue::Dword(1)) | SettingValue::Ini("1") => true,
        _ => return Err(InvalidSetting),
    };
    document.input.modern_tone = flag;
    Ok(())
}

/// A field map with the single invented `SampleTone` mapping.
pub const TONE_MAP: &[FieldMapping] = &[FieldMapping {
    registry: Some("SampleTone"),
    ini: Some(("Options", "SampleTone")),
    apply: apply_modern_tone,
}];
