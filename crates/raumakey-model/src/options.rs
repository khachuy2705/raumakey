//! Engine options (clean-room spec §3.2) and versioned defaults (plan D4).

use core::fmt;

use serde::{Deserialize, Serialize};

/// Version of the product defaults, [`EngineOptions::defaults_v2`] (D51). Version 1
/// ([`EngineOptions::defaults_v1`], plan D4) stays available: the engine's vectors state it.
pub const DEFAULTS_VERSION: u32 = 2;

/// Supported input methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InputMethod {
    /// Telex.
    Telex,
    /// Simple Telex (D53): Telex without the standalone `w`, which changes only a vowel
    /// before it (`aw`, `ow`, `uw`).
    #[serde(rename = "simple-telex")]
    SimpleTelex,
    /// VNI.
    Vni,
    /// VIQR.
    Viqr,
    /// A user-defined semantic key map.
    #[serde(rename = "user")]
    UserDefined,
}

/// Unicode normalization of engine output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Normalization {
    /// Precomposed (NFC).
    Nfc,
    /// Decomposed (NFD).
    Nfd,
    /// Emit text as produced, without normalizing.
    Preserve,
}

/// The charset the keyboard hook types in (D54). The engine works in Unicode; the hook
/// encodes what it deletes and inserts. The names are those of `raumakey-converter`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OutputCharset {
    /// Precomposed Unicode.
    #[default]
    Unicode,
    /// TCVN3 (ABC), for the `.Vn` fonts.
    Tcvn3,
    /// VNI Windows, for the VNI fonts.
    VniWindows,
    /// Decomposed Unicode ("Unicode tổ hợp").
    UnicodeNfd,
    /// Windows-1258 ("Vietnamese locale CP 1258").
    Cp1258,
}

impl OutputCharset {
    /// Every charset, in the order of the settings panel.
    pub const ALL: [Self; 5] = [
        Self::Unicode,
        Self::Tcvn3,
        Self::VniWindows,
        Self::UnicodeNfd,
        Self::Cp1258,
    ];

    /// The stable name, as in `raumakey.toml` and `raumakey-converter`.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::Tcvn3 => "tcvn3",
            Self::VniWindows => "vni-windows",
            Self::UnicodeNfd => "unicode-nfd",
            Self::Cp1258 => "cp1258",
        }
    }
}

/// Why options were rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionsError {
    /// `context_limit_codepoints` outside `MIN_CONTEXT..=MAX_CONTEXT`.
    ContextLimitOutOfRange,
    /// `UserDefined` without a key map id.
    MissingUserKeymap,
}

impl fmt::Display for OptionsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ContextLimitOutOfRange => "context limit out of range",
            Self::MissingUserKeymap => "user-defined input method needs a key map",
        })
    }
}

impl std::error::Error for OptionsError {}

/// Options that shape engine behavior. Immutable for the duration of one request.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineOptions {
    /// Vietnamese mode on.
    pub enabled: bool,
    /// Active input method.
    pub input_method: InputMethod,
    /// Output normalization.
    pub normalization: Normalization,
    /// New tone placement (`hoà`, `thuý`) instead of the old one (`hòa`, `thúy`).
    pub modern_tone: bool,
    /// Allow marks to be typed anywhere after their target vowel.
    pub free_marking: bool,
    /// Gate transformations on Vietnamese spelling.
    pub spell_check: bool,
    /// Restore an invalid transformed word to its raw keys at a boundary.
    pub auto_restore_invalid: bool,
    /// Expand macros.
    pub macros_enabled: bool,
    /// Expand macros even while Vietnamese mode is off.
    pub macros_when_disabled: bool,
    /// Adapt macro replacement case to the typed trigger.
    pub auto_case_macro: bool,
    /// Key map id for [`InputMethod::UserDefined`].
    pub user_keymap_id: Option<String>,
    /// Maximum keys tracked for one word before ownership is dropped.
    pub context_limit_codepoints: u32,
    /// The charset the keyboard hook types in (D54); the engine ignores it.
    #[serde(default)]
    pub charset: OutputCharset,
}

impl EngineOptions {
    /// Smallest accepted context limit.
    pub const MIN_CONTEXT: u32 = 8;
    /// Largest accepted context limit.
    pub const MAX_CONTEXT: u32 = 128;

    /// Raumakey defaults, version 2 ([`DEFAULTS_VERSION`], D51): version 1 with spell checking
    /// off. The engine's structure check keeps words that cannot be Vietnamese (`EVKey`)
    /// untouched without it.
    pub fn defaults_v2() -> Self {
        Self {
            spell_check: false,
            ..Self::defaults_v1()
        }
    }

    /// Raumakey defaults, version 1 (plan D4).
    pub fn defaults_v1() -> Self {
        Self {
            enabled: true,
            input_method: InputMethod::Telex,
            normalization: Normalization::Nfc,
            modern_tone: false,
            free_marking: true,
            spell_check: true,
            auto_restore_invalid: true,
            macros_enabled: true,
            macros_when_disabled: false,
            auto_case_macro: true,
            user_keymap_id: None,
            context_limit_codepoints: 32,
            charset: OutputCharset::Unicode,
        }
    }

    /// Validates option invariants.
    pub fn validate(&self) -> Result<(), OptionsError> {
        if !(Self::MIN_CONTEXT..=Self::MAX_CONTEXT).contains(&self.context_limit_codepoints) {
            return Err(OptionsError::ContextLimitOutOfRange);
        }
        if self.input_method == InputMethod::UserDefined && self.user_keymap_id.is_none() {
            return Err(OptionsError::MissingUserKeymap);
        }
        Ok(())
    }
}

impl Default for EngineOptions {
    fn default() -> Self {
        Self::defaults_v1()
    }
}
