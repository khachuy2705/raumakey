//! `raumakey.toml`: user settings (plan §5.1).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use raumakey_model::hotkey::{Hotkey, HotkeyConfig};
use raumakey_model::keymap::{KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};

use crate::document::{
    FieldIssue, IssueReason, Loaded, MAX_TOML_REVISION, StoreDocument, encode_toml, ensure_current,
    parse_toml, read_revision, read_version, toml_head_revision,
};
use crate::entries::{EntriesWire, EntryExtras, decode_entries};
use crate::error::ConfigError;
use crate::fields::{Extra, Fields, Node, Peek, integer_in, integer_u32, text};

type TomlFields<'i> = Fields<'i, toml::Value>;

const ROOT_KEYS: &[&str] = &[
    "version",
    "revision",
    "input",
    "macros",
    HOTKEYS_KEY,
    "system",
    "conversion",
    "keymaps",
];
const INPUT_KEYS: &[&str] = &[
    "enabled",
    "method",
    "user_keymap",
    "normalization",
    "modern_tone",
    "free_marking",
    "spell_check",
    "auto_restore_invalid",
    "context_limit",
];
const MACRO_KEYS: &[&str] = &["enabled", "when_disabled", "auto_case"];
const HOTKEY_KEYS: &[&str] = &["toggle_mode", "hard_reset"];
const SYSTEM_KEYS: &[&str] = &["startup", "show_panel_on_start", "diagnostics"];
const CONVERSION_KEYS: &[&str] = &["source", "target", "case", "remove_tones"];
const KEYMAP_KEYS: &[&str] = &["id", "version", "name", BINDINGS_KEY];
const BINDING_KEYS: &[&str] = &["key", "actions"];

/// Name of the `bindings` array of a `[[keymaps]]` entry. In the unknown-key record of a key
/// map ([`ConfigDocument::keymap_extras`]) this key holds the unknown keys of its bindings, in
/// a table keyed by the binding's key character.
pub const BINDINGS_KEY: &str = "bindings";

/// Longest key map name, in characters (as checked by [`KeyMap::validate`]).
const MAX_KEYMAP_NAME_CHARS: usize = 128;

/// Name of the `[hotkeys]` table. Its unknown keys are kept in [`ConfigDocument::extensions`]
/// under this key, because [`HotkeyConfig`] is a shared model type without an extension map.
pub const HOTKEYS_KEY: &str = "hotkeys";

/// How Raumakey starts with Windows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupMode {
    /// No automatic start.
    #[default]
    None,
    /// Startup-folder shortcut, normal privileges.
    Shortcut,
    /// Elevated scheduled task, created only after an explicit user choice.
    ElevatedTask,
}

impl StartupMode {
    /// Stable name used in `raumakey.toml`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Shortcut => "shortcut",
            Self::ElevatedTask => "elevated-task",
        }
    }
}

/// Case change applied by text conversion.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CaseTransform {
    /// Keep the case.
    #[default]
    None,
    /// UPPERCASE.
    Upper,
    /// lowercase.
    Lower,
    /// Title Case.
    Title,
}

impl CaseTransform {
    /// Stable name used in `raumakey.toml`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Title => "title",
        }
    }
}

/// `[input]`: typing behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct InputSection {
    /// Vietnamese mode on.
    pub enabled: bool,
    /// Input method as configured. The engine uses the default method instead while
    /// [`InputMethod::UserDefined`] has no usable key map (see
    /// [`ConfigDocument::engine_options`]).
    pub method: InputMethod,
    /// Id of the key map (in [`ConfigDocument::keymaps`]) used by
    /// [`InputMethod::UserDefined`].
    pub user_keymap: Option<String>,
    /// Output normalization.
    pub normalization: Normalization,
    /// New tone placement.
    pub modern_tone: bool,
    /// Marks may be typed anywhere after their vowel.
    pub free_marking: bool,
    /// Gate transformations on Vietnamese spelling.
    pub spell_check: bool,
    /// Restore invalid transformed words at a boundary.
    pub auto_restore_invalid: bool,
    /// Engine context limit in code points
    /// ([`EngineOptions::MIN_CONTEXT`]..=[`EngineOptions::MAX_CONTEXT`]).
    pub context_limit: u32,
    /// Unknown keys of `[input]`, re-emitted on save.
    pub extensions: toml::Table,
}

impl Default for InputSection {
    fn default() -> Self {
        let options = EngineOptions::defaults_v1();
        Self {
            enabled: options.enabled,
            method: options.input_method,
            user_keymap: options.user_keymap_id,
            normalization: options.normalization,
            modern_tone: options.modern_tone,
            free_marking: options.free_marking,
            spell_check: options.spell_check,
            auto_restore_invalid: options.auto_restore_invalid,
            context_limit: options.context_limit_codepoints,
            extensions: toml::Table::new(),
        }
    }
}

/// `[macros]`: macro expansion switches.
#[derive(Clone, Debug, PartialEq)]
pub struct MacroSection {
    /// Expand macros.
    pub enabled: bool,
    /// Expand macros while Vietnamese mode is off.
    pub when_disabled: bool,
    /// Adapt replacement case to the typed trigger.
    pub auto_case: bool,
    /// Unknown keys of `[macros]`, re-emitted on save.
    pub extensions: toml::Table,
}

impl Default for MacroSection {
    fn default() -> Self {
        let options = EngineOptions::defaults_v1();
        Self {
            enabled: options.macros_enabled,
            when_disabled: options.macros_when_disabled,
            auto_case: options.auto_case_macro,
            extensions: toml::Table::new(),
        }
    }
}

/// `[system]`: process-level behavior.
#[derive(Clone, Debug, PartialEq)]
pub struct SystemSection {
    /// Automatic start mode.
    pub startup: StartupMode,
    /// Show the compact panel when the controller starts.
    pub show_panel_on_start: bool,
    /// Opt-in diagnostics log.
    pub diagnostics: bool,
    /// Unknown keys of `[system]`, re-emitted on save.
    pub extensions: toml::Table,
}

impl Default for SystemSection {
    fn default() -> Self {
        Self {
            startup: StartupMode::None,
            show_panel_on_start: true,
            diagnostics: false,
            extensions: toml::Table::new(),
        }
    }
}

/// `[conversion]`: defaults of the text converter.
///
/// Charset names are stored as written; the converter validates them when it runs.
#[derive(Clone, Debug, PartialEq)]
pub struct ConversionSection {
    /// Source charset name.
    pub source: String,
    /// Target charset name.
    pub target: String,
    /// Case change.
    pub case: CaseTransform,
    /// Strip tones.
    pub remove_tones: bool,
    /// Unknown keys of `[conversion]`, re-emitted on save.
    pub extensions: toml::Table,
}

impl ConversionSection {
    /// Charset name used when none is configured.
    pub const DEFAULT_CHARSET: &'static str = "unicode";
}

impl Default for ConversionSection {
    fn default() -> Self {
        Self {
            source: Self::DEFAULT_CHARSET.to_owned(),
            target: Self::DEFAULT_CHARSET.to_owned(),
            case: CaseTransform::None,
            remove_tones: false,
            extensions: toml::Table::new(),
        }
    }
}

/// `raumakey.toml`, schema v1 (plan §5.1).
#[derive(Clone, Debug, PartialEq)]
pub struct ConfigDocument {
    /// Schema version.
    pub version: u32,
    /// Store revision.
    pub revision: u64,
    /// `[input]`.
    pub input: InputSection,
    /// `[macros]`.
    pub macros: MacroSection,
    /// `[hotkeys]`.
    pub hotkeys: HotkeyConfig,
    /// `[system]`.
    pub system: SystemSection,
    /// `[conversion]`.
    pub conversion: ConversionSection,
    /// `[[keymaps]]`: user-defined key maps.
    ///
    /// Every key map produced by [`StoreDocument::decode`] passes [`KeyMap::validate`] and has
    /// a unique id. Problems are isolated per element (matrix #45):
    ///
    /// - `id` (missing, malformed, or repeated) and a `version` other than
    ///   [`KEYMAP_SCHEMA_VERSION`] identify the key map: such an entry is set aside verbatim in
    ///   [`ConfigDocument::keymap_extras`] and written back.
    /// - A missing `version` is read as [`KEYMAP_SCHEMA_VERSION`]; a missing or invalid `name`
    ///   is read as empty; a missing or invalid `bindings` list is read as empty. Each is
    ///   reported.
    /// - A binding whose `key` is missing, not one printable ASCII character, or repeated
    ///   (letters compare case-insensitively) is dropped with its unknown keys, as is a binding
    ///   left without a usable action. An action that is not recognised, or beyond
    ///   [`KeyMap::MAX_ACTIONS`], is dropped on its own. Dropped elements are not written back.
    pub keymaps: Vec<KeyMap>,
    /// Unknown keys of `[[keymaps]]` entries (by key map id; the unknown keys of their bindings
    /// are nested under [`BINDINGS_KEY`]) and the key maps that could not be decoded.
    pub keymap_extras: EntryExtras<String>,
    /// Unknown top-level keys, plus the unknown keys of `[hotkeys]` under [`HOTKEYS_KEY`];
    /// re-emitted on save.
    pub extensions: toml::Table,
}

impl ConfigDocument {
    /// Schema v1 defaults: [`EngineOptions::defaults_v1`] and [`HotkeyConfig::default`].
    pub fn defaults() -> Self {
        Self {
            version: <Self as StoreDocument>::CURRENT_VERSION,
            revision: 0,
            input: InputSection::default(),
            macros: MacroSection::default(),
            hotkeys: HotkeyConfig::default(),
            system: SystemSection::default(),
            conversion: ConversionSection::default(),
            keymaps: Vec::new(),
            keymap_extras: EntryExtras::default(),
            extensions: toml::Table::new(),
        }
    }

    /// The engine options described by `[input]` and `[macros]`: the options the engine runs
    /// with.
    ///
    /// [`InputMethod::UserDefined`] needs the key map that `input.user_keymap` names among
    /// [`ConfigDocument::keymaps`]. While there is none (the id is absent, or names a key map
    /// that is missing or was set aside), the options carry the default method of
    /// [`EngineOptions::defaults_v1`] instead, so the engine never rejects them. The document
    /// keeps `method = "user"`: a commit writes it back, the problem is reported on
    /// `input.user_keymap` on every load, and the user method takes effect again as soon as
    /// the key map can be used (matrix #45: a valid field is never reset because of another
    /// one).
    ///
    /// For a document produced by [`StoreDocument::decode`] the options always pass
    /// [`EngineOptions::validate`], and [`InputMethod::UserDefined`] in them always names a key
    /// map of [`ConfigDocument::keymaps`].
    pub fn engine_options(&self) -> EngineOptions {
        let input_method =
            if self.input.method == InputMethod::UserDefined && !self.has_user_keymap() {
                EngineOptions::defaults_v1().input_method
            } else {
                self.input.method
            };
        EngineOptions {
            enabled: self.input.enabled,
            input_method,
            normalization: self.input.normalization,
            modern_tone: self.input.modern_tone,
            free_marking: self.input.free_marking,
            spell_check: self.input.spell_check,
            auto_restore_invalid: self.input.auto_restore_invalid,
            macros_enabled: self.macros.enabled,
            macros_when_disabled: self.macros.when_disabled,
            auto_case_macro: self.macros.auto_case,
            user_keymap_id: self.input.user_keymap.clone(),
            context_limit_codepoints: self.input.context_limit,
        }
    }

    /// Whether `input.user_keymap` names one of [`ConfigDocument::keymaps`].
    fn has_user_keymap(&self) -> bool {
        self.input
            .user_keymap
            .as_deref()
            .is_some_and(|id| self.keymaps.iter().any(|map| map.id == id))
    }

    /// Reports a user method that cannot take effect (see [`ConfigDocument::engine_options`])
    /// on `input.user_keymap`: [`IssueReason::Missing`] without an id,
    /// [`IssueReason::Unresolved`] for an id that names no key map of the document. Nothing is
    /// changed. An id that was itself rejected is already reported on that path.
    fn report_unusable_user_keymap(&self, issues: &mut Vec<FieldIssue>) {
        const PATH: &str = "input.user_keymap";
        if self.input.method != InputMethod::UserDefined || self.has_user_keymap() {
            return;
        }
        let reason = if self.input.user_keymap.is_some() {
            IssueReason::Unresolved
        } else {
            IssueReason::Missing
        };
        if !issues.iter().any(|issue| issue.path == PATH) {
            issues.push(FieldIssue {
                path: PATH.to_owned(),
                reason,
            });
        }
    }

    /// Stores `options` in `[input]` and `[macros]` as given; other fields are left unchanged.
    ///
    /// While the user method falls back (see [`ConfigDocument::engine_options`]), the options
    /// that `engine_options` returns carry the default method, and applying them stores that
    /// method. To change other options without replacing the configured method, set the
    /// section fields directly.
    pub fn apply_engine_options(&mut self, options: &EngineOptions) {
        self.input.enabled = options.enabled;
        self.input.method = options.input_method;
        self.input.normalization = options.normalization;
        self.input.modern_tone = options.modern_tone;
        self.input.free_marking = options.free_marking;
        self.input.spell_check = options.spell_check;
        self.input.auto_restore_invalid = options.auto_restore_invalid;
        self.input.user_keymap.clone_from(&options.user_keymap_id);
        self.input.context_limit = options.context_limit_codepoints;
        self.macros.enabled = options.macros_enabled;
        self.macros.when_disabled = options.macros_when_disabled;
        self.macros.auto_case = options.auto_case_macro;
    }
}

impl Default for ConfigDocument {
    fn default() -> Self {
        Self::defaults()
    }
}

/// Whether `id` is a valid key map id (`[A-Za-z0-9._-]{1,64}`, as [`KeyMap::validate`]).
fn is_keymap_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

fn decode_input(root: &mut TomlFields<'_>) -> InputSection {
    let defaults = InputSection::default();
    let table = root.table("input").unwrap_or_default();
    let mut f = root.child(table, "input");
    InputSection {
        enabled: f.bool("enabled", defaults.enabled),
        method: f.keyword("method", defaults.method),
        user_keymap: f.optional_string("user_keymap", is_keymap_id),
        normalization: f.keyword("normalization", defaults.normalization),
        modern_tone: f.bool("modern_tone", defaults.modern_tone),
        free_marking: f.bool("free_marking", defaults.free_marking),
        spell_check: f.bool("spell_check", defaults.spell_check),
        auto_restore_invalid: f.bool("auto_restore_invalid", defaults.auto_restore_invalid),
        context_limit: f.u32_in(
            "context_limit",
            defaults.context_limit,
            EngineOptions::MIN_CONTEXT..=EngineOptions::MAX_CONTEXT,
        ),
        extensions: f.into_rest(),
    }
}

fn decode_macros(root: &mut TomlFields<'_>) -> MacroSection {
    let defaults = MacroSection::default();
    let table = root.table("macros").unwrap_or_default();
    let mut f = root.child(table, "macros");
    MacroSection {
        enabled: f.bool("enabled", defaults.enabled),
        when_disabled: f.bool("when_disabled", defaults.when_disabled),
        auto_case: f.bool("auto_case", defaults.auto_case),
        extensions: f.into_rest(),
    }
}

fn decode_hotkeys(root: &mut TomlFields<'_>) -> (HotkeyConfig, toml::Table) {
    let defaults = HotkeyConfig::default();
    let table = root.table(HOTKEYS_KEY).unwrap_or_default();
    let mut f = root.child(table, HOTKEYS_KEY);
    let parse = |text: &str| text.parse::<Hotkey>().ok();
    let hotkeys = HotkeyConfig {
        toggle_mode: f.string_list("toggle_mode", defaults.toggle_mode, parse),
        hard_reset: f.string_list("hard_reset", defaults.hard_reset, parse),
    };
    (hotkeys, f.into_rest())
}

fn decode_system(root: &mut TomlFields<'_>) -> SystemSection {
    let defaults = SystemSection::default();
    let table = root.table("system").unwrap_or_default();
    let mut f = root.child(table, "system");
    SystemSection {
        startup: f.keyword("startup", defaults.startup),
        show_panel_on_start: f.bool("show_panel_on_start", defaults.show_panel_on_start),
        diagnostics: f.bool("diagnostics", defaults.diagnostics),
        extensions: f.into_rest(),
    }
}

fn decode_conversion(root: &mut TomlFields<'_>) -> ConversionSection {
    let defaults = ConversionSection::default();
    let table = root.table("conversion").unwrap_or_default();
    let mut f = root.child(table, "conversion");
    ConversionSection {
        source: f.string("source", &defaults.source),
        target: f.string("target", &defaults.target),
        case: f.keyword("case", defaults.case),
        remove_tones: f.bool("remove_tones", defaults.remove_tones),
        extensions: f.into_rest(),
    }
}

fn decode_keymaps(root: &mut TomlFields<'_>) -> (Vec<KeyMap>, EntryExtras<String>) {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    decode_entries(
        root,
        "keymaps",
        |peek| identify_keymap(peek, &mut seen),
        build_keymap,
    )
}

/// Checks the identity of a `[[keymaps]]` table (its id, and its schema version when present)
/// without consuming anything; accepted ids are recorded in `seen`.
fn identify_keymap(
    peek: &mut Peek<'_, '_, toml::Value>,
    seen: &mut BTreeSet<String>,
) -> Option<String> {
    let id = peek.require("id", |value| {
        let id = text(value)?;
        if is_keymap_id(id) {
            Ok(id.to_owned())
        } else {
            Err(IssueReason::InvalidValue)
        }
    });
    let version_known = match peek.get("version") {
        None => true,
        Some(value) => {
            let version = integer_in(value, 0..=u64::MAX).and_then(|version| {
                if version == u64::from(KEYMAP_SCHEMA_VERSION) {
                    Ok(version)
                } else {
                    Err(IssueReason::OutOfRange)
                }
            });
            match version {
                Ok(_) => true,
                Err(reason) => {
                    peek.report("version", reason);
                    false
                }
            }
        }
    };
    let id = id?;
    if !version_known {
        return None;
    }
    if seen.contains(&id) {
        peek.report("id", IssueReason::DuplicateEntry);
        return None;
    }
    seen.insert(id.clone());
    Some(id)
}

fn build_keymap(fields: &mut TomlFields<'_>, id: String) -> (KeyMap, String) {
    fields.drop_keys(&["id"]);
    let version = fields.needed("version", KEYMAP_SCHEMA_VERSION, |value| {
        integer_u32(&value)
    });
    let name = fields.needed("name", String::new(), |value| {
        let name = value.into_string().ok_or(IssueReason::InvalidType)?;
        if name.chars().count() > MAX_KEYMAP_NAME_CHARS {
            Err(IssueReason::OutOfRange)
        } else {
            Ok(name)
        }
    });
    let bindings = decode_bindings(fields);
    let map = KeyMap {
        id: id.clone(),
        version,
        name,
        bindings,
    };
    (map, id)
}

/// Decodes `bindings` element by element; the unknown keys of the kept bindings are put back
/// under [`BINDINGS_KEY`], keyed by binding key, for the key map's unknown-key record.
fn decode_bindings(fields: &mut TomlFields<'_>) -> Vec<KeyBinding> {
    if fields.peek(BINDINGS_KEY).is_none() {
        fields.report(BINDINGS_KEY, IssueReason::Missing);
        return Vec::new();
    }
    let mut bindings = Vec::new();
    let mut folded_keys: Vec<char> = Vec::new();
    let mut unknown = toml::Table::new();
    fields.each_table(BINDINGS_KEY, |fields, path, table| {
        let mut binding = fields.at(table, path);
        let key = decode_binding_key(&mut binding);
        let actions = decode_actions(&mut binding);
        let (Some(key), Some(actions)) = (key, actions) else {
            return;
        };
        let folded = key.to_ascii_lowercase();
        if folded_keys.contains(&folded) {
            binding.report("key", IssueReason::DuplicateEntry);
            return;
        }
        folded_keys.push(folded);
        let rest = binding.into_rest();
        if !rest.is_empty() {
            unknown.insert(key.to_string(), toml::Value::Table(rest));
        }
        bindings.push(KeyBinding { key, actions });
    });
    if !unknown.is_empty() {
        fields.keep(BINDINGS_KEY, toml::Value::Table(unknown));
    }
    bindings
}

/// The `key` of a binding: one printable ASCII character (as [`KeyMap::validate`]); `None`
/// (reported) otherwise.
fn decode_binding_key(binding: &mut TomlFields<'_>) -> Option<char> {
    let Some(value) = binding.take("key") else {
        binding.report("key", IssueReason::Missing);
        return None;
    };
    let outcome = text(&value).and_then(|key| {
        let mut chars = key.chars();
        match (chars.next(), chars.next()) {
            (Some(key), None) if matches!(key, '\u{21}'..='\u{7E}') => Ok(key),
            _ => Err(IssueReason::InvalidValue),
        }
    });
    match outcome {
        Ok(key) => Some(key),
        Err(reason) => {
            binding.report("key", reason);
            None
        }
    }
}

/// The `actions` of a binding; `None` (reported) when there is no usable action.
fn decode_actions(binding: &mut TomlFields<'_>) -> Option<Vec<KeyAction>> {
    let Some(value) = binding.take("actions") else {
        binding.report("actions", IssueReason::Missing);
        return None;
    };
    let Ok(items) = value.into_array() else {
        binding.report("actions", IssueReason::InvalidType);
        return None;
    };
    let mut actions = Vec::with_capacity(items.len().min(KeyMap::MAX_ACTIONS));
    for (index, item) in items.iter().enumerate() {
        let outcome = text(item).and_then(|name| {
            name.parse::<KeyAction>()
                .map_err(|_| IssueReason::InvalidValue)
        });
        let reason = match outcome {
            Ok(action) if actions.len() < KeyMap::MAX_ACTIONS => {
                actions.push(action);
                continue;
            }
            Ok(_) => IssueReason::OutOfRange,
            Err(reason) => reason,
        };
        let path = binding.element_path("actions", index);
        binding.report_path(path, reason);
    }
    if actions.is_empty() {
        binding.report("actions", IssueReason::OutOfRange);
        return None;
    }
    Some(actions)
}

impl StoreDocument for ConfigDocument {
    const FILE_NAME: &'static str = "raumakey.toml";
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
        let input = decode_input(&mut root);
        let macros = decode_macros(&mut root);
        let (hotkeys, hotkey_extensions) = decode_hotkeys(&mut root);
        let system = decode_system(&mut root);
        let conversion = decode_conversion(&mut root);
        let (keymaps, keymap_extras) = decode_keymaps(&mut root);
        let mut extensions = root.into_rest();
        if !hotkey_extensions.is_empty() {
            extensions.insert(
                HOTKEYS_KEY.to_owned(),
                toml::Value::Table(hotkey_extensions),
            );
        }
        let document = Self {
            version,
            revision,
            input,
            macros,
            hotkeys,
            system,
            conversion,
            keymaps,
            keymap_extras,
            extensions,
        };
        document.report_unusable_user_keymap(&mut issues);
        Ok(Loaded { document, issues })
    }

    fn encode(&self) -> Result<String, ConfigError> {
        ensure_current(self.version, Self::CURRENT_VERSION)?;
        let hotkey_extensions = self
            .extensions
            .get(HOTKEYS_KEY)
            .and_then(toml::Value::as_table);
        let wire = ConfigWire {
            version: self.version,
            revision: self.revision,
            input: InputWire {
                enabled: self.input.enabled,
                method: self.input.method,
                user_keymap: self.input.user_keymap.as_deref(),
                normalization: self.input.normalization,
                modern_tone: self.input.modern_tone,
                free_marking: self.input.free_marking,
                spell_check: self.input.spell_check,
                auto_restore_invalid: self.input.auto_restore_invalid,
                context_limit: self.input.context_limit,
                extensions: Extra::new(Some(&self.input.extensions), INPUT_KEYS),
            },
            macros: MacrosWire {
                enabled: self.macros.enabled,
                when_disabled: self.macros.when_disabled,
                auto_case: self.macros.auto_case,
                extensions: Extra::new(Some(&self.macros.extensions), MACRO_KEYS),
            },
            hotkeys: HotkeysWire {
                toggle_mode: &self.hotkeys.toggle_mode,
                hard_reset: &self.hotkeys.hard_reset,
                extensions: Extra::new(hotkey_extensions, HOTKEY_KEYS),
            },
            system: SystemWire {
                startup: self.system.startup,
                show_panel_on_start: self.system.show_panel_on_start,
                diagnostics: self.system.diagnostics,
                extensions: Extra::new(Some(&self.system.extensions), SYSTEM_KEYS),
            },
            conversion: ConversionWire {
                source: &self.conversion.source,
                target: &self.conversion.target,
                case: self.conversion.case,
                remove_tones: self.conversion.remove_tones,
                extensions: Extra::new(Some(&self.conversion.extensions), CONVERSION_KEYS),
            },
            keymaps: EntriesWire::new(self.keymap_wires(), &self.keymap_extras.rejected),
            extensions: Extra::new(Some(&self.extensions), ROOT_KEYS),
        };
        encode_toml(&wire)
    }

    fn default_document() -> Self {
        Self::defaults()
    }

    fn revision_from_head(head: &[u8]) -> Option<u64> {
        toml_head_revision(head, Self::CURRENT_VERSION)
    }
}

impl ConfigDocument {
    /// The key maps on the wire, each with its unknown keys and those of its bindings.
    fn keymap_wires(&self) -> Vec<KeyMapWire<'_>> {
        let unknown = self
            .keymap_extras
            .assign(self.keymaps.iter().map(|map| &map.id));
        self.keymaps
            .iter()
            .zip(unknown)
            .map(|(map, unknown)| {
                let binding_unknown = unknown
                    .and_then(|table| table.get(BINDINGS_KEY))
                    .and_then(toml::Value::as_table);
                let bindings = map
                    .bindings
                    .iter()
                    .map(|binding| {
                        let mut buffer = [0u8; 4];
                        let key: &str = binding.key.encode_utf8(&mut buffer);
                        let unknown = binding_unknown
                            .and_then(|table| table.get(key))
                            .and_then(toml::Value::as_table);
                        BindingWire {
                            key: binding.key,
                            actions: &binding.actions,
                            extensions: Extra::new(unknown, BINDING_KEYS),
                        }
                    })
                    .collect();
                KeyMapWire {
                    id: &map.id,
                    version: map.version,
                    name: &map.name,
                    bindings,
                    extensions: Extra::new(unknown, KEYMAP_KEYS),
                }
            })
            .collect()
    }
}

#[derive(Serialize)]
struct ConfigWire<'a> {
    version: u32,
    revision: u64,
    input: InputWire<'a>,
    macros: MacrosWire<'a>,
    hotkeys: HotkeysWire<'a>,
    system: SystemWire<'a>,
    conversion: ConversionWire<'a>,
    #[serde(skip_serializing_if = "EntriesWire::is_empty")]
    keymaps: EntriesWire<'a, KeyMapWire<'a>>,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct KeyMapWire<'a> {
    id: &'a str,
    version: u32,
    name: &'a str,
    bindings: Vec<BindingWire<'a>>,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct BindingWire<'a> {
    key: char,
    actions: &'a [KeyAction],
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct InputWire<'a> {
    enabled: bool,
    method: InputMethod,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_keymap: Option<&'a str>,
    normalization: Normalization,
    modern_tone: bool,
    free_marking: bool,
    spell_check: bool,
    auto_restore_invalid: bool,
    context_limit: u32,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct MacrosWire<'a> {
    enabled: bool,
    when_disabled: bool,
    auto_case: bool,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct HotkeysWire<'a> {
    toggle_mode: &'a [Hotkey],
    hard_reset: &'a [Hotkey],
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct SystemWire<'a> {
    startup: StartupMode,
    show_panel_on_start: bool,
    diagnostics: bool,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct ConversionWire<'a> {
    source: &'a str,
    target: &'a str,
    case: CaseTransform,
    remove_tones: bool,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}
