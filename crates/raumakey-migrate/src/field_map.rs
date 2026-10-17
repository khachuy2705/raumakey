//! The table of EVKey settings that are imported into `raumakey.toml` (plan D18).

use core::fmt;

use raumakey_config::ConfigDocument;

use crate::ports::LegacyValue;

/// A raw setting value handed to a [`FieldMapping`].
///
/// The [`fmt::Debug`] output shows where the value came from, never the value.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SettingValue<'a> {
    /// A value of the settings registry key.
    Registry(&'a LegacyValue),
    /// The raw (trimmed) value of a `setting.ini` entry.
    Ini(&'a str),
}

impl fmt::Debug for SettingValue<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Registry(value) => f.debug_tuple("Registry").field(value).finish(),
            Self::Ini(text) => f.debug_struct("Ini").field("bytes", &text.len()).finish(),
        }
    }
}

/// A [`FieldMapping`] rejected a value; the field is then quarantined as `invalid-value`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InvalidSetting;

/// One EVKey setting and how it is stored in `raumakey.toml`.
///
/// A mapping names the setting in the registry and/or in `setting.ini` (both compare ignoring
/// case) and converts a raw value with `apply`, which must be a pure function of the value: it
/// either changes the document or returns [`InvalidSetting`] without changing it, and it must
/// not panic. When both sources hold the setting the registry value wins (matrix #44); a value
/// that `apply` rejects is quarantined and the next source is tried.
#[derive(Clone, Copy, Debug)]
pub struct FieldMapping {
    /// Value path below [`SETTINGS_REGISTRY_PATH`](crate::SETTINGS_REGISTRY_PATH): a value
    /// name, or `Subkey\Name` for a value of a subkey.
    pub registry: Option<&'static str>,
    /// Section and key in `setting.ini`.
    pub ini: Option<(&'static str, &'static str)>,
    /// Stores the value in the document.
    pub apply: fn(&mut ConfigDocument, SettingValue<'_>) -> Result<(), InvalidSetting>,
}

/// The settings imported into `raumakey.toml` (plan D18).
///
/// **The table is empty on purpose.** No source that Raumakey may use documents EVKey's
/// setting names or value encodings, so mapping any of them would be a guess. Every settings
/// field is therefore preserved raw in the migration record (or quarantined when unreadable),
/// lists of executables become named sets without an action, and `raumakey.toml` keeps its
/// current values. A mapping is added here only once a permitted source documents the setting.
pub const FIELD_MAP: &[FieldMapping] = &[];
