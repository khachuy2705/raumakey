//! `raumakey-app-rules.toml`: per-application compatibility policy data (plan §5.1).

use serde::Serialize;

use raumakey_model::host::ExeName;
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, NamedSet, ProbeRecord, RuleAction, SecureFieldMode, Transport,
};

use crate::document::{
    IssueReason, Loaded, MAX_TOML_REVISION, StoreDocument, encode_toml, ensure_current, parse_toml,
    read_revision, read_version, toml_head_revision,
};
use crate::entries::{EntriesWire, EntryExtras, decode_entries, match_in_order};
use crate::error::ConfigError;
use crate::fields::{Extra, Fields, Peek, boolean, integer_u32, keyword, text};

type TomlFields<'i> = Fields<'i, toml::Value>;

const ROOT_KEYS: &[&str] = &[
    "version",
    "revision",
    "clipboard_enabled",
    "default_reset_on_mouse",
    "secure_field_mode",
    "default_max_delete_before",
    RULE_ARRAY_KEY,
    NAMED_SET_ARRAY_KEY,
    PROBE_ARRAY_KEY,
];
const ACTION_KEYS: &[&str] = &[
    "transport",
    "reset_on_mouse",
    "macros",
    "vietnamese",
    "max_delete_before",
];
const RULE_KEYS: &[&str] = &[
    "exe",
    "window_class",
    "transport",
    "reset_on_mouse",
    "macros",
    "vietnamese",
    "max_delete_before",
];
const NAMED_SET_KEYS: &[&str] = &["name", "members", ACTION_KEY];
const PROBE_KEYS: &[&str] = &["exe", "probe_version", "tsf_range_edit"];

const RULE_ARRAY_KEY: &str = "rule";
const NAMED_SET_ARRAY_KEY: &str = "named_set";
const PROBE_ARRAY_KEY: &str = "probe";

/// Name of the `action` table of a `[[named_set]]` entry. In the unknown-key record of a named
/// set ([`AppRulesDocument::named_set_extras`]) this key holds the unknown keys of its action.
pub const ACTION_KEY: &str = "action";

/// Identity of a `[[rule]]` entry: its executable and optional window class.
pub type RuleIdentity = (ExeName, Option<String>);

/// `raumakey-app-rules.toml`: a [`CompatibilityConfig`] whose fields sit at the top level of
/// the file next to `version` and `revision`.
///
/// Missing scalar fields take the model's defaults and missing arrays are empty; the D13 named
/// sets come from [`CompatibilityConfig::defaults_v1`] only when the file does not exist.
/// Entries are decoded field by field (matrix #45):
///
/// - `[[rule]]`: `exe` and `window_class` decide which windows the rule covers, so a rule whose
///   executable is missing or not a valid basename, or whose window class is not a string, is
///   set aside verbatim in [`AppRulesDocument::rule_extras`] (dropping only the window class
///   would widen the rule). A bad action field (`transport`, `reset_on_mouse`, `macros`,
///   `vietnamese`, `max_delete_before`) is reported and inherits.
/// - `[[named_set]]`: an entry without a string `name` is set aside. A bad member is dropped on
///   its own (`named_set[0].members[2]`); a bad action field inherits.
/// - `[[probe]]`: a probe is one measurement, so a record whose `exe`, `probe_version`, or
///   `tsf_range_edit` is missing or invalid is set aside rather than guessed.
///
/// Unknown keys of every entry (and of a named set's `action`) are kept and written back with
/// their entry.
#[derive(Clone, Debug, PartialEq)]
pub struct AppRulesDocument {
    /// Schema version.
    pub version: u32,
    /// Store revision.
    pub revision: u64,
    /// Compatibility data.
    pub compatibility: CompatibilityConfig,
    /// Unknown keys of `[[rule]]` entries (by executable and window class) and the rules that
    /// could not be decoded.
    pub rule_extras: EntryExtras<RuleIdentity>,
    /// Unknown keys of `[[named_set]]` entries (by name; the unknown keys of their action are
    /// nested under [`ACTION_KEY`]) and the named sets that could not be decoded.
    pub named_set_extras: EntryExtras<String>,
    /// Unknown keys of `[[probe]]` records (by executable) and the records that could not be
    /// decoded.
    pub probe_extras: EntryExtras<ExeName>,
    /// Unknown top-level keys, re-emitted on save.
    pub extensions: toml::Table,
}

impl Default for AppRulesDocument {
    fn default() -> Self {
        Self {
            version: <Self as StoreDocument>::CURRENT_VERSION,
            revision: 0,
            compatibility: CompatibilityConfig::defaults_v1(),
            rule_extras: EntryExtras::default(),
            named_set_extras: EntryExtras::default(),
            probe_extras: EntryExtras::default(),
            extensions: toml::Table::new(),
        }
    }
}

/// Reads an executable basename (lowercased, as [`ExeName::new`]).
fn exe_name(value: &toml::Value) -> Result<ExeName, IssueReason> {
    ExeName::new(text(value)?).map_err(|_| IssueReason::InvalidValue)
}

/// The overrides of a rule or named set; a bad field is reported and inherits (`None`).
fn decode_action(fields: &mut TomlFields<'_>) -> RuleAction {
    RuleAction {
        transport: fields.optional("transport", keyword::<toml::Value, Transport>),
        reset_on_mouse: fields.optional("reset_on_mouse", |value| boolean(&value)),
        macros: fields.optional("macros", |value| boolean(&value)),
        vietnamese: fields.optional("vietnamese", |value| boolean(&value)),
        max_delete_before: fields.optional("max_delete_before", |value| integer_u32(&value)),
    }
}

fn identify_rule(peek: &mut Peek<'_, '_, toml::Value>) -> Option<RuleIdentity> {
    let exe = peek.require("exe", exe_name);
    let window_class = match peek.get("window_class") {
        None => Some(None),
        Some(value) => match text(value) {
            Ok(class) => Some(Some(class.to_owned())),
            Err(reason) => {
                peek.report("window_class", reason);
                None
            }
        },
    };
    Some((exe?, window_class?))
}

fn build_rule(
    fields: &mut TomlFields<'_>,
    (exe, window_class): RuleIdentity,
) -> (AppRule, RuleIdentity) {
    fields.drop_keys(&["exe", "window_class"]);
    let rule = AppRule {
        exe: exe.clone(),
        window_class: window_class.clone(),
        action: decode_action(fields),
    };
    (rule, (exe, window_class))
}

fn identify_named_set(peek: &mut Peek<'_, '_, toml::Value>) -> Option<String> {
    peek.require("name", |value| text(value).map(str::to_owned))
}

fn build_named_set(fields: &mut TomlFields<'_>, name: String) -> (NamedSet, String) {
    fields.drop_keys(&["name"]);
    let members = fields.string_list("members", Vec::new(), |member| ExeName::new(member).ok());
    let action = match fields.table(ACTION_KEY) {
        None => RuleAction::default(),
        Some(table) => {
            let mut action_fields = fields.child(table, ACTION_KEY);
            let action = decode_action(&mut action_fields);
            let unknown = action_fields.into_rest();
            if !unknown.is_empty() {
                fields.keep(ACTION_KEY, toml::Value::Table(unknown));
            }
            action
        }
    };
    let set = NamedSet {
        name: name.clone(),
        members,
        action,
    };
    (set, name)
}

fn identify_probe(peek: &mut Peek<'_, '_, toml::Value>) -> Option<(ExeName, u32, bool)> {
    let exe = peek.require("exe", exe_name);
    let probe_version = peek.require("probe_version", integer_u32);
    let tsf_range_edit = peek.require("tsf_range_edit", boolean);
    Some((exe?, probe_version?, tsf_range_edit?))
}

fn build_probe(
    fields: &mut TomlFields<'_>,
    (exe, probe_version, tsf_range_edit): (ExeName, u32, bool),
) -> (ProbeRecord, ExeName) {
    fields.drop_keys(PROBE_KEYS);
    let probe = ProbeRecord {
        exe: exe.clone(),
        probe_version,
        tsf_range_edit,
    };
    (probe, exe)
}

impl StoreDocument for AppRulesDocument {
    const FILE_NAME: &'static str = "raumakey-app-rules.toml";
    const CURRENT_VERSION: u32 = 1;

    fn revision(&self) -> u64 {
        self.revision
    }

    fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }

    fn decode(text: &str) -> Result<Loaded<Self>, ConfigError> {
        let table = parse_toml(text)?;
        let mut issues = Vec::new();
        let mut root = TomlFields::new(table, "", &mut issues);
        let version = read_version(&mut root, Self::CURRENT_VERSION)?;
        let revision = read_revision(&mut root, MAX_TOML_REVISION);
        let defaults = CompatibilityConfig::defaults_v1();
        let clipboard_enabled = root.bool("clipboard_enabled", defaults.clipboard_enabled);
        let default_reset_on_mouse =
            root.bool("default_reset_on_mouse", defaults.default_reset_on_mouse);
        let secure_field_mode = root.keyword("secure_field_mode", defaults.secure_field_mode);
        let default_max_delete_before = root.u32_in(
            "default_max_delete_before",
            defaults.default_max_delete_before,
            0..=u32::MAX,
        );
        let (rules, rule_extras) =
            decode_entries(&mut root, RULE_ARRAY_KEY, identify_rule, build_rule);
        let (named_sets, named_set_extras) = decode_entries(
            &mut root,
            NAMED_SET_ARRAY_KEY,
            identify_named_set,
            build_named_set,
        );
        let (probes, probe_extras) =
            decode_entries(&mut root, PROBE_ARRAY_KEY, identify_probe, build_probe);
        let extensions = root.into_rest();
        let document = Self {
            version,
            revision,
            compatibility: CompatibilityConfig {
                clipboard_enabled,
                default_reset_on_mouse,
                secure_field_mode,
                default_max_delete_before,
                rules,
                named_sets,
                probes,
            },
            rule_extras,
            named_set_extras,
            probe_extras,
            extensions,
        };
        Ok(Loaded { document, issues })
    }

    fn encode(&self) -> Result<String, ConfigError> {
        ensure_current(self.version, Self::CURRENT_VERSION)?;
        let c = &self.compatibility;
        encode_toml(&AppRulesWire {
            version: self.version,
            revision: self.revision,
            clipboard_enabled: c.clipboard_enabled,
            default_reset_on_mouse: c.default_reset_on_mouse,
            secure_field_mode: c.secure_field_mode,
            default_max_delete_before: c.default_max_delete_before,
            rules: EntriesWire::new(self.rule_wires(), &self.rule_extras.rejected),
            named_sets: EntriesWire::new(self.named_set_wires(), &self.named_set_extras.rejected),
            probes: EntriesWire::new(self.probe_wires(), &self.probe_extras.rejected),
            extensions: Extra::new(Some(&self.extensions), ROOT_KEYS),
        })
    }

    fn default_document() -> Self {
        Self::default()
    }

    fn revision_from_head(head: &[u8]) -> Option<u64> {
        toml_head_revision(head, Self::CURRENT_VERSION)
    }
}

impl AppRulesDocument {
    fn rule_wires(&self) -> Vec<RuleWire<'_>> {
        let rules = &self.compatibility.rules;
        let unknown = match_in_order(
            self.rule_extras
                .unknown_keys
                .iter()
                .map(|((exe, class), table)| ((exe, class.as_deref()), table)),
            rules
                .iter()
                .map(|rule| (&rule.exe, rule.window_class.as_deref())),
        );
        rules
            .iter()
            .zip(unknown)
            .map(|(rule, unknown)| RuleWire {
                exe: rule.exe.as_str(),
                window_class: rule.window_class.as_deref(),
                action: ActionWire::new(&rule.action, None),
                extensions: Extra::new(unknown, RULE_KEYS),
            })
            .collect()
    }

    fn named_set_wires(&self) -> Vec<NamedSetWire<'_>> {
        let sets = &self.compatibility.named_sets;
        let unknown = self
            .named_set_extras
            .assign(sets.iter().map(|set| &set.name));
        sets.iter()
            .zip(unknown)
            .map(|(set, unknown)| {
                let action_unknown = unknown
                    .and_then(|table| table.get(ACTION_KEY))
                    .and_then(toml::Value::as_table);
                NamedSetWire {
                    name: &set.name,
                    members: set.members.iter().map(ExeName::as_str).collect(),
                    action: ActionWire::new(&set.action, action_unknown),
                    extensions: Extra::new(unknown, NAMED_SET_KEYS),
                }
            })
            .collect()
    }

    fn probe_wires(&self) -> Vec<ProbeWire<'_>> {
        let probes = &self.compatibility.probes;
        let unknown = self
            .probe_extras
            .assign(probes.iter().map(|probe| &probe.exe));
        probes
            .iter()
            .zip(unknown)
            .map(|(probe, unknown)| ProbeWire {
                exe: probe.exe.as_str(),
                probe_version: probe.probe_version,
                tsf_range_edit: probe.tsf_range_edit,
                extensions: Extra::new(unknown, PROBE_KEYS),
            })
            .collect()
    }
}

#[derive(Serialize)]
struct AppRulesWire<'a> {
    version: u32,
    revision: u64,
    clipboard_enabled: bool,
    default_reset_on_mouse: bool,
    secure_field_mode: SecureFieldMode,
    default_max_delete_before: u32,
    #[serde(rename = "rule", skip_serializing_if = "EntriesWire::is_empty")]
    rules: EntriesWire<'a, RuleWire<'a>>,
    #[serde(rename = "named_set", skip_serializing_if = "EntriesWire::is_empty")]
    named_sets: EntriesWire<'a, NamedSetWire<'a>>,
    #[serde(rename = "probe", skip_serializing_if = "EntriesWire::is_empty")]
    probes: EntriesWire<'a, ProbeWire<'a>>,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct ActionWire<'a> {
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: Option<Transport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reset_on_mouse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    macros: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    vietnamese: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_delete_before: Option<u32>,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

impl<'a> ActionWire<'a> {
    fn new(action: &RuleAction, unknown: Option<&'a toml::Table>) -> Self {
        Self {
            transport: action.transport,
            reset_on_mouse: action.reset_on_mouse,
            macros: action.macros,
            vietnamese: action.vietnamese,
            max_delete_before: action.max_delete_before,
            extensions: Extra::new(unknown, ACTION_KEYS),
        }
    }
}

#[derive(Serialize)]
struct RuleWire<'a> {
    exe: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    window_class: Option<&'a str>,
    #[serde(flatten)]
    action: ActionWire<'a>,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct NamedSetWire<'a> {
    name: &'a str,
    members: Vec<&'a str>,
    action: ActionWire<'a>,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct ProbeWire<'a> {
    exe: &'a str,
    probe_version: u32,
    tsf_range_edit: bool,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}
