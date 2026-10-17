//! Host policy types (clean-room spec §4). Resolution lives in `raumakey-policy`.

use serde::{Deserialize, Serialize};

use crate::host::ExeName;

/// Version of the TSF capability probe; records with another version are ignored.
pub const PROBE_VERSION: u32 = 1;

/// How edits reach a host. Exactly one transport handles a given host.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Transport {
    /// In-process TSF text input processor.
    Tsf,
    /// Controller hook with synthetic key injection.
    SyntheticKey,
    /// Controller hook with clipboard paste (opt-in, never in secure fields).
    Clipboard,
}

/// Behavior in password/secure fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SecureFieldMode {
    /// No transformation at all.
    Bypass,
    /// Transformation allowed, but no macros, clipboard, or content diagnostics.
    Restricted,
}

/// Resolved policy for one host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostPolicy {
    /// Selected transport.
    pub transport: Transport,
    /// Reset engine state on mouse clicks.
    pub reset_on_mouse: bool,
    /// Allow replaying the original key after an edit.
    pub permit_replay: bool,
    /// Largest `delete_before` the adapter may apply.
    pub max_delete_before: u32,
    /// Macro expansion allowed.
    pub macro_allowed: bool,
    /// Secure field behavior.
    pub secure_field_mode: SecureFieldMode,
    /// Vietnamese transformation allowed in this host.
    pub vietnamese_allowed: bool,
}

impl HostPolicy {
    /// The generic policy: TSF with conservative defaults.
    pub fn default_tsf() -> Self {
        Self {
            transport: Transport::Tsf,
            reset_on_mouse: true,
            permit_replay: true,
            max_delete_before: 64,
            macro_allowed: true,
            secure_field_mode: SecureFieldMode::Bypass,
            vietnamese_allowed: true,
        }
    }
}

/// Policy overrides; `None` inherits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleAction {
    /// Transport override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transport: Option<Transport>,
    /// Mouse reset override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reset_on_mouse: Option<bool>,
    /// Macro override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macros: Option<bool>,
    /// Vietnamese allowed override (false forces English in the host).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vietnamese: Option<bool>,
    /// Deletion bound override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_delete_before: Option<u32>,
}

/// A rule for one executable, optionally narrowed to a window class.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    /// Executable basename.
    pub exe: ExeName,
    /// Window class, when the rule is narrower than the executable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_class: Option<String>,
    /// Overrides.
    #[serde(flatten)]
    pub action: RuleAction,
}

/// A named group of executables that shares one configurable action.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NamedSet {
    /// Set name.
    pub name: String,
    /// Members.
    #[serde(default)]
    pub members: Vec<ExeName>,
    /// Overrides applied to members.
    #[serde(default)]
    pub action: RuleAction,
}

/// Persisted outcome of a TSF capability probe.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProbeRecord {
    /// Executable basename.
    pub exe: ExeName,
    /// Probe version that produced the record.
    pub probe_version: u32,
    /// Whether TSF range edits worked.
    pub tsf_range_edit: bool,
}

/// Probe outcome reported by an adapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ProbeResult {
    /// TSF range edits worked.
    TsfRangeEditOk,
    /// TSF range edits failed.
    TsfRangeEditFailed,
}

/// Compatibility configuration persisted in `app_rules.toml`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityConfig {
    /// Clipboard transport may be selected by rules.
    #[serde(default)]
    pub clipboard_enabled: bool,
    /// Default mouse reset.
    #[serde(default = "default_true")]
    pub default_reset_on_mouse: bool,
    /// Secure field behavior.
    #[serde(default = "default_secure_mode")]
    pub secure_field_mode: SecureFieldMode,
    /// Default deletion bound.
    #[serde(default = "default_max_delete")]
    pub default_max_delete_before: u32,
    /// Per-application rules.
    #[serde(default, rename = "rule")]
    pub rules: Vec<AppRule>,
    /// Named sets, evaluated in declared order.
    #[serde(default, rename = "named_set")]
    pub named_sets: Vec<NamedSet>,
    /// Probe cache.
    #[serde(default, rename = "probe")]
    pub probes: Vec<ProbeRecord>,
}

fn default_true() -> bool {
    true
}

fn default_secure_mode() -> SecureFieldMode {
    SecureFieldMode::Bypass
}

fn default_max_delete() -> u32 {
    64
}

fn exe_list(names: &[&str]) -> Vec<ExeName> {
    names.iter().filter_map(|n| ExeName::new(n).ok()).collect()
}

impl CompatibilityConfig {
    /// Raumakey default compatibility data (plan D13).
    pub fn defaults_v1() -> Self {
        let mouse_reset = RuleAction {
            reset_on_mouse: Some(true),
            ..RuleAction::default()
        };
        Self {
            clipboard_enabled: false,
            default_reset_on_mouse: true,
            secure_field_mode: SecureFieldMode::Bypass,
            default_max_delete_before: default_max_delete(),
            rules: Vec::new(),
            named_sets: vec![
                NamedSet {
                    name: "chromium-family".to_owned(),
                    members: exe_list(&[
                        "msedge.exe",
                        "chrome.exe",
                        "brave.exe",
                        "vivaldi.exe",
                        "opera.exe",
                        "ms-teams.exe",
                        "teams.exe",
                        "slack.exe",
                        "discord.exe",
                    ]),
                    action: mouse_reset.clone(),
                },
                NamedSet {
                    name: "terminals".to_owned(),
                    members: exe_list(&[
                        "windowsterminal.exe",
                        "wt.exe",
                        "openconsole.exe",
                        "conhost.exe",
                    ]),
                    action: mouse_reset,
                },
                NamedSet {
                    name: "games".to_owned(),
                    members: Vec::new(),
                    action: RuleAction::default(),
                },
            ],
            probes: Vec::new(),
        }
    }
}

impl Default for CompatibilityConfig {
    fn default() -> Self {
        Self::defaults_v1()
    }
}
