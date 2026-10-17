//! Decode-side wire form of the compatibility configuration.
//!
//! The model's `AppRule` flattens its `RuleAction` overrides into the rule object. serde reads
//! a struct with a flattened field by buffering every key the struct does not name itself (the
//! overrides and any unknown key) in its generic content tree. That costs a few dozen times
//! the bytes buffered, so one padded rule in an 8 MiB snapshot would cost about 200 MB.
//!
//! [`compatibility`] therefore reads `RuntimeSnapshot::compatibility` through the types
//! below. They accept the same JSON objects as the model types: the same keys, the same
//! defaults for missing ones, and the same validation of each value. Unknown keys are skipped
//! without being buffered. Like every other derived struct in a snapshot, and unlike the
//! flattened model type, a rule may also arrive as an array of its fields in declaration
//! order; the encoder never writes that form, and its values are validated the same way.
//!
//! The result is converted to the model types with struct literals, so a field added to one
//! of them fails to compile here until the wire form handles it as well.

use raumakey_model::host::ExeName;
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, NamedSet, ProbeRecord, RuleAction, SecureFieldMode, Transport,
};
use serde::{Deserialize, Deserializer};

/// `deserialize_with` target for `RuntimeSnapshot::compatibility`.
pub(crate) fn compatibility<'de, D>(deserializer: D) -> Result<CompatibilityConfig, D::Error>
where
    D: Deserializer<'de>,
{
    WireCompatibility::deserialize(deserializer).map(WireCompatibility::into_model)
}

/// `CompatibilityConfig` with the defaults of its serde attributes.
#[derive(Deserialize)]
struct WireCompatibility {
    #[serde(default)]
    clipboard_enabled: bool,
    #[serde(default = "default_reset_on_mouse")]
    default_reset_on_mouse: bool,
    #[serde(default = "default_secure_field_mode")]
    secure_field_mode: SecureFieldMode,
    #[serde(default = "default_max_delete_before")]
    default_max_delete_before: u32,
    #[serde(default, rename = "rule")]
    rules: Vec<WireAppRule>,
    #[serde(default, rename = "named_set")]
    named_sets: Vec<NamedSet>,
    #[serde(default, rename = "probe")]
    probes: Vec<ProbeRecord>,
}

/// `AppRule` with its flattened `RuleAction` fields listed explicitly.
#[derive(Deserialize)]
struct WireAppRule {
    exe: ExeName,
    #[serde(default)]
    window_class: Option<String>,
    #[serde(default)]
    transport: Option<Transport>,
    #[serde(default)]
    reset_on_mouse: Option<bool>,
    #[serde(default)]
    macros: Option<bool>,
    #[serde(default)]
    vietnamese: Option<bool>,
    #[serde(default)]
    max_delete_before: Option<u32>,
}

impl WireCompatibility {
    fn into_model(self) -> CompatibilityConfig {
        CompatibilityConfig {
            clipboard_enabled: self.clipboard_enabled,
            default_reset_on_mouse: self.default_reset_on_mouse,
            secure_field_mode: self.secure_field_mode,
            default_max_delete_before: self.default_max_delete_before,
            rules: self
                .rules
                .into_iter()
                .map(WireAppRule::into_model)
                .collect(),
            named_sets: self.named_sets,
            probes: self.probes,
        }
    }
}

impl WireAppRule {
    fn into_model(self) -> AppRule {
        AppRule {
            exe: self.exe,
            window_class: self.window_class,
            action: RuleAction {
                transport: self.transport,
                reset_on_mouse: self.reset_on_mouse,
                macros: self.macros,
                vietnamese: self.vietnamese,
                max_delete_before: self.max_delete_before,
            },
        }
    }
}

// The defaults of `CompatibilityConfig`'s serde attributes. A test in `tests/messages.rs`
// compares this decoder with the model's own one on missing, null, present, repeated, and
// unknown keys.

const fn default_reset_on_mouse() -> bool {
    true
}

const fn default_secure_field_mode() -> SecureFieldMode {
    SecureFieldMode::Bypass
}

const fn default_max_delete_before() -> u32 {
    64
}
