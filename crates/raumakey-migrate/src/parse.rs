//! Field-level parsing of discovered sources (plan §9, D18; matrix #44, #45).
//!
//! [`parse`] turns a [`DiscoveryReport`] into a [`ParsedMigration`] without any I/O. Every
//! field of every source ends in exactly one [`Disposition`], reported as a [`FieldReport`]:
//!
//! - **Imported** into `raumakey.toml`, only through [`FIELD_MAP`] (which ships empty, plan D18).
//! - **NamedSet**: a string value of the settings key or of `setting.ini` that lists executable
//!   names ([`exe_list`](crate::exe_list)) becomes the named set `evkey-import-<n>` with no
//!   action (matrix #28: the user decides what the set does). `n` counts from 1 in parse
//!   order: registry values first, then `setting.ini` entries in line order.
//! - **Preserved**: the raw value is kept in [`ParsedMigration::preserved`].
//! - **Quarantined**: the field is reported with a [`QuarantineReason`] and its value is kept
//!   nowhere. Reports name fields by registry value path or by line number, never by content.
//!
//! The raw value of every imported, named-set, or preserved field is kept in
//! [`ParsedMigration::preserved`] as evidence (an `invalid-value` field keeps its raw value
//! too); quarantined values never are. The TSF cache is preserved only: it holds compatibility
//! hints, not settings, so it is never imported and never becomes a named set. `setting.tree`
//! is never parsed. Macros are read with
//! [`parse_legacy_text`] and normalized with
//! [`normalize_entry`]; lines that cannot be imported are
//! quarantined by line number. Macro text is not copied into the record: the source file stays
//! unchanged and is identified by its fingerprint.
//!
//! # Precedence (matrix #44)
//!
//! When the registry and `setting.ini` both exist, both are kept, separately, under their own
//! keys of the preserved object; neither overwrites the other. For a setting named by a
//! [`FieldMapping`], the registry value wins: the first value that the mapping accepts, in the
//! order registry, `setting.ini`, is imported; a rejected value is quarantined as
//! `invalid-value`, and values after the imported one are preserved only.
//!
//! # Preserved layout
//!
//! ```text
//! {
//!   "registry":      { "location": "HKCU\\Software\\MyQ\\EVKey",
//!                      "values":  { "<name>": <value> },
//!                      "subkeys": { "<name>": { "values": {...}, "subkeys": {...} } } },
//!   "tsf-cache":     { same layout as "registry" },
//!   "settings-ini":  { "location": "<path>", "encoding": "utf-8" | "utf-8-bom" | "utf-16le" | "utf-16be",
//!                      "sections": { "<section>": { "<key>": "<raw value>" } } },
//!   "settings-tree": { "location": "<path>", "size": <bytes>, "sha256": "<hex>", "base64": "<data>" }
//! }
//! <value> = {"type": "string" | "expand-string", "data": "<text>"}
//!         | {"type": "multi-string", "data": ["<text>", ...]}
//!         | {"type": "dword" | "qword", "data": <number>}
//!         | {"type": "binary", "hex": "<hex>"}
//!         | {"type": "other", "kind": <registry type>, "hex": "<hex>"}
//! ```
//!
//! The root section of `setting.ini` (keys before the first header) is the empty name.
//!
//! # Limits
//!
//! A value larger than [`MAX_FIELD_BYTES`] is quarantined as `too-large`. Preserved data is
//! bounded by [`MAX_PRESERVED_BYTES`] (estimated as pretty-printed JSON, including the section
//! and subkey objects); a field that would exceed the budget is quarantined as
//! `limit-exceeded` (an imported or named-set field keeps its disposition but not its raw
//! value). At most [`MAX_FIELDS_PER_SOURCE`] fields are reported per source; the rest of the
//! source is covered by one `limit-exceeded` report on [`FieldRef::Content`]. The record lists
//! at most [`MAX_RECORDED_QUARANTINE`] quarantined fields with names of at most 128
//! characters. Together these keep the migration record well below the 16 MiB limit of its
//! store whatever the sources hold.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};

use raumakey_config::{ConfigDocument, QuarantinedField, SourceFingerprint};
use raumakey_macro::{LegacyIssueReason, normalize_entry, parse_legacy_text};
use raumakey_model::host::ExeName;
use raumakey_model::macros::MacroEntry;
use raumakey_model::policy::{NamedSet, RuleAction};
use serde_json::{Map, Value, json};

use crate::discover::{Content, DiscoveredSource, DiscoveryReport, SourceKind};
use crate::exe_list::exe_list;
use crate::field_map::{FIELD_MAP, FieldMapping, InvalidSetting, SettingValue};
use crate::ini::{IniEntry, IniIssue, IniIssueKind, parse_ini};
use crate::ports::{LegacyValue, REG_EXPAND_SZ, REG_MULTI_SZ, REG_SZ};
use crate::registry::{RegistryNode, SkipReason};

/// Largest registry or `setting.ini` value that is read (64 KiB, UTF-8 bytes for text).
pub const MAX_FIELD_BYTES: usize = 64 * 1024;

/// Budget for preserved raw data (8 MiB, estimated as pretty-printed JSON).
pub const MAX_PRESERVED_BYTES: usize = 8 * 1024 * 1024;

/// Most fields reported per source.
pub const MAX_FIELDS_PER_SOURCE: usize = 65_536;

/// Most quarantined fields listed in the migration record; see
/// [`ParsedMigration::quarantined`].
pub const MAX_RECORDED_QUARANTINE: usize = 2_000;

/// Name prefix of the named sets created from executable lists.
pub const NAMED_SET_PREFIX: &str = "evkey-import-";

/// Longest field name written to the migration record, in characters.
const MAX_RECORD_NAME_CHARS: usize = 128;

/// Bytes charged per JSON line for indentation, punctuation, and line breaks.
const LINE_OVERHEAD: usize = 32;

/// Why a field was quarantined. Codes are stable and never describe the value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuarantineReason {
    /// The value is not valid text in its encoding.
    Undecodable,
    /// The value is larger than [`MAX_FIELD_BYTES`], or the file larger than its reader
    /// accepts.
    TooLarge,
    /// The line is not a comment, section header, or entry of a valid section.
    Malformed,
    /// A later line sets the same key, or a registry key repeats a value name.
    DuplicateKey,
    /// A registry subkey name cannot be addressed (empty or containing `\`).
    InvalidName,
    /// A [`FieldMapping`] rejected the value (its raw value is still preserved).
    InvalidValue,
    /// A size or count limit was reached.
    LimitExceeded,
    /// A macro line could not be imported.
    Macro(LegacyIssueReason),
}

impl QuarantineReason {
    /// Stable kebab-case reason code, recorded as
    /// [`QuarantinedField::reason_code`].
    pub const fn code(self) -> &'static str {
        match self {
            Self::Undecodable => "undecodable",
            Self::TooLarge => "too-large",
            Self::Malformed => "malformed",
            Self::DuplicateKey => "duplicate-key",
            Self::InvalidName => "invalid-name",
            Self::InvalidValue => "invalid-value",
            Self::LimitExceeded => "limit-exceeded",
            Self::Macro(reason) => reason.code(),
        }
    }
}

impl fmt::Display for QuarantineReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Which field of a source a report is about. Never holds a value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FieldRef {
    /// A registry value, by its path below the source key (`Name` or `Subkey\Name`).
    Registry(String),
    /// A registry subkey that was not read, by its path below the source key.
    RegistryKey(String),
    /// A `setting.ini` entry.
    Ini {
        /// Section name (empty for the root section).
        section: String,
        /// Key.
        key: String,
        /// One-based line number.
        line: usize,
    },
    /// A line of `setting.ini` or of the macro file that holds no usable entry.
    Line(usize),
    /// A macro entry that failed normalization, by its one-based position among the imported
    /// entries.
    MacroEntry(u64),
    /// The whole source.
    Content,
}

impl FieldRef {
    /// The field name written to the migration record: the registry path (control characters
    /// replaced, at most 128 characters; a subkey ends in `\`), `line <n>`, `entry <n>`, or
    /// `content`. `setting.ini` fields are named by line only.
    pub fn record_name(&self) -> String {
        match self {
            Self::Registry(path) => record_safe(path),
            Self::RegistryKey(path) => {
                let mut name = record_safe(path);
                name.push('\\');
                name
            }
            Self::Ini { line, .. } | Self::Line(line) => format!("line {line}"),
            Self::MacroEntry(position) => format!("entry {position}"),
            Self::Content => "content".to_owned(),
        }
    }
}

/// What happened to a field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Disposition {
    /// Imported into `raumakey.toml` through [`FIELD_MAP`].
    Imported,
    /// Imported as the named set with this name.
    NamedSet(String),
    /// Kept only as a raw value in [`ParsedMigration::preserved`].
    Preserved,
    /// Not imported and not kept.
    Quarantined(QuarantineReason),
}

/// The disposition of one source field.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldReport {
    /// Source of the field.
    pub source: SourceKind,
    /// The field.
    pub field: FieldRef,
    /// What happened to it.
    pub disposition: Disposition,
}

/// A raw value accepted by a [`FieldMapping`].
#[derive(Clone)]
pub(crate) enum RawSetting {
    Registry(LegacyValue),
    Ini(String),
}

impl RawSetting {
    fn as_value(&self) -> SettingValue<'_> {
        match self {
            Self::Registry(value) => SettingValue::Registry(value),
            Self::Ini(text) => SettingValue::Ini(text),
        }
    }
}

/// A setting to store in `raumakey.toml`.
#[derive(Clone)]
pub(crate) struct MappedSetting {
    apply: fn(&mut ConfigDocument, SettingValue<'_>) -> Result<(), InvalidSetting>,
    value: RawSetting,
}

impl MappedSetting {
    /// Stores the setting in `document`; false when the mapping rejected it.
    pub(crate) fn apply_to(&self, document: &mut ConfigDocument) -> bool {
        (self.apply)(document, self.value.as_value()).is_ok()
    }
}

/// The parsed sources: what an import would add, keep, and quarantine.
///
/// The [`fmt::Debug`] output shows counts only, never macro text or setting values.
#[derive(Clone)]
pub struct ParsedMigration {
    sources: Vec<SourceFingerprint>,
    macros: Vec<MacroEntry>,
    named_sets: Vec<NamedSet>,
    fields: Vec<FieldReport>,
    preserved: Value,
    settings: Vec<MappedSetting>,
}

impl ParsedMigration {
    /// Fingerprints of the parsed sources, in discovery order.
    pub fn sources(&self) -> &[SourceFingerprint] {
        &self.sources
    }

    /// Macro entries of the macro file, NFC, in file order, with identifiers 1, 2, 3, … (the
    /// commit assigns fresh identifiers).
    pub fn macros(&self) -> &[MacroEntry] {
        &self.macros
    }

    /// Named sets created from executable lists, in creation order.
    pub fn named_sets(&self) -> &[NamedSet] {
        &self.named_sets
    }

    /// Every field report, by source in discovery order and in source order within a source.
    pub fn fields(&self) -> &[FieldReport] {
        &self.fields
    }

    /// Raw source values (see the [module documentation](self) for the layout).
    pub fn preserved(&self) -> &Value {
        &self.preserved
    }

    /// Number of settings imported through [`FIELD_MAP`].
    pub fn settings_count(&self) -> usize {
        self.settings.len()
    }

    pub(crate) fn settings(&self) -> &[MappedSetting] {
        &self.settings
    }

    /// The quarantined fields as recorded in `raumakey-migration.json`, in report order.
    ///
    /// At most [`MAX_RECORDED_QUARANTINE`] fields are listed; when there are more, one final
    /// entry with source `*`, field `+<count of the others>`, and reason `limit-exceeded`
    /// stands for them.
    pub fn quarantined(&self) -> Vec<QuarantinedField> {
        let mut list = Vec::new();
        let mut omitted = 0usize;
        for report in &self.fields {
            let Disposition::Quarantined(reason) = &report.disposition else {
                continue;
            };
            if list.len() < MAX_RECORDED_QUARANTINE {
                list.push(QuarantinedField {
                    source: report.source.as_str().to_owned(),
                    field: report.field.record_name(),
                    reason_code: reason.code().to_owned(),
                });
            } else {
                omitted = omitted.saturating_add(1);
            }
        }
        if omitted > 0 {
            list.push(QuarantinedField {
                source: "*".to_owned(),
                field: format!("+{omitted}"),
                reason_code: QuarantineReason::LimitExceeded.code().to_owned(),
            });
        }
        list
    }

    /// Number of quarantined fields (all of them, also beyond the recorded list).
    pub fn quarantined_count(&self) -> usize {
        self.fields
            .iter()
            .filter(|report| matches!(report.disposition, Disposition::Quarantined(_)))
            .count()
    }

    /// Number of fields kept only as raw values, by source; sources without such a field are
    /// left out.
    pub fn preserved_counts(&self) -> BTreeMap<SourceKind, usize> {
        let mut counts = BTreeMap::new();
        for report in &self.fields {
            if report.disposition == Disposition::Preserved {
                let count: &mut usize = counts.entry(report.source).or_default();
                *count = count.saturating_add(1);
            }
        }
        counts
    }
}

impl fmt::Debug for ParsedMigration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ParsedMigration")
            .field("sources", &self.sources.len())
            .field("macros", &self.macros.len())
            .field("named_sets", &self.named_sets.len())
            .field("fields", &self.fields.len())
            .field("quarantined", &self.quarantined_count())
            .field("settings", &self.settings.len())
            .finish()
    }
}

/// Parses discovered sources with [`FIELD_MAP`] (see the [module documentation](self)).
///
/// Pure: reads only the data held by `report`. Never fails and never panics.
pub fn parse(report: &DiscoveryReport) -> ParsedMigration {
    parse_with_field_map(report, FIELD_MAP)
}

/// Like [`parse`], with a caller-supplied mapping table instead of [`FIELD_MAP`].
///
/// Meant for developing and testing new mappings; production imports use [`parse`].
pub fn parse_with_field_map(
    report: &DiscoveryReport,
    field_map: &[FieldMapping],
) -> ParsedMigration {
    let mut builder = Builder::new(field_map);
    let mut seen = BTreeSet::new();
    for source in &report.sources {
        if !seen.insert(source.kind) {
            continue;
        }
        builder.sources.push(source.fingerprint());
        match (&source.content, source.kind) {
            (Content::TooLarge, kind) => builder.too_large(kind),
            (Content::Registry(node), SourceKind::Registry | SourceKind::TsfCache) => {
                builder.registry(source, node);
            }
            (Content::File(bytes), SourceKind::SettingsIni) => builder.ini(source, bytes),
            (Content::File(bytes), SourceKind::SettingsTree) => builder.tree(source, bytes),
            (Content::File(bytes), SourceKind::MacroText) => builder.macros(bytes),
            // Discovery never pairs other content with these kinds.
            _ => {}
        }
    }
    builder.finish()
}

/// A field that a [`FieldMapping`] names.
struct Candidate {
    mapping: usize,
    kind: SourceKind,
    report: usize,
    value: RawSetting,
}

/// Text form of a registry value for the executable-list heuristic.
enum RegistryText {
    /// Not a string type.
    NotText,
    /// The text (list items joined with line breaks).
    Text(String),
    /// String data that is not valid UTF-16.
    Undecodable,
}

struct Builder<'m> {
    field_map: &'m [FieldMapping],
    sources: Vec<SourceFingerprint>,
    macros: Vec<MacroEntry>,
    named_sets: Vec<NamedSet>,
    fields: Vec<FieldReport>,
    preserved: Map<String, Value>,
    reported: BTreeMap<SourceKind, usize>,
    capped: BTreeSet<SourceKind>,
    budget: usize,
    candidates: Vec<Candidate>,
}

impl<'m> Builder<'m> {
    fn new(field_map: &'m [FieldMapping]) -> Self {
        Self {
            field_map,
            sources: Vec::new(),
            macros: Vec::new(),
            named_sets: Vec::new(),
            fields: Vec::new(),
            preserved: Map::new(),
            reported: BTreeMap::new(),
            capped: BTreeSet::new(),
            budget: MAX_PRESERVED_BYTES,
            candidates: Vec::new(),
        }
    }

    /// Whether another field of `kind` may be reported. The first refusal adds the
    /// `limit-exceeded` report for the rest of the source.
    fn room(&mut self, kind: SourceKind) -> bool {
        if self.reported.get(&kind).copied().unwrap_or(0) < MAX_FIELDS_PER_SOURCE {
            return true;
        }
        if self.capped.insert(kind) {
            self.fields.push(FieldReport {
                source: kind,
                field: FieldRef::Content,
                disposition: Disposition::Quarantined(QuarantineReason::LimitExceeded),
            });
        }
        false
    }

    /// Adds a report and returns its index.
    fn report(&mut self, source: SourceKind, field: FieldRef, disposition: Disposition) -> usize {
        let count = self.reported.entry(source).or_default();
        *count = count.saturating_add(1);
        self.fields.push(FieldReport {
            source,
            field,
            disposition,
        });
        self.fields.len().saturating_sub(1)
    }

    /// Takes `cost` bytes from the preserved budget, if available.
    fn spend(&mut self, cost: usize) -> bool {
        match self.budget.checked_sub(cost) {
            Some(rest) => {
                self.budget = rest;
                true
            }
            None => false,
        }
    }

    /// Stores a raw value under `name` when the budget allows; a field that would only be
    /// preserved is quarantined otherwise.
    fn keep(
        &mut self,
        name: &str,
        value: Value,
        map: &mut Map<String, Value>,
        disposition: Disposition,
    ) -> Disposition {
        if self.spend(entry_cost(name, &value)) {
            map.insert(name.to_owned(), value);
            disposition
        } else {
            unkept(disposition)
        }
    }

    fn named_set(&mut self, members: Vec<ExeName>) -> Disposition {
        let name = format!(
            "{NAMED_SET_PREFIX}{}",
            self.named_sets.len().saturating_add(1)
        );
        self.named_sets.push(NamedSet {
            name: name.clone(),
            members,
            action: RuleAction::default(),
        });
        Disposition::NamedSet(name)
    }

    fn too_large(&mut self, kind: SourceKind) {
        if self.room(kind) {
            self.report(
                kind,
                FieldRef::Content,
                Disposition::Quarantined(QuarantineReason::TooLarge),
            );
        }
    }

    fn registry(&mut self, source: &DiscoveredSource, node: &RegistryNode) {
        let settings = source.kind == SourceKind::Registry;
        let mut object = Map::new();
        object.insert(
            "location".to_owned(),
            Value::String(source.location.clone()),
        );
        object.extend(self.registry_node(source.kind, settings, node, ""));
        self.preserved
            .insert(source.kind.as_str().to_owned(), Value::Object(object));
    }

    /// Classifies a key and returns its `values` and `subkeys` objects.
    fn registry_node(
        &mut self,
        kind: SourceKind,
        settings: bool,
        node: &RegistryNode,
        prefix: &str,
    ) -> Map<String, Value> {
        let mut values = Map::new();
        let mut seen = BTreeSet::new();
        for (name, value) in &node.values {
            if !self.room(kind) {
                break;
            }
            let path = join(prefix, name);
            if !seen.insert(name.to_lowercase()) {
                self.report(
                    kind,
                    FieldRef::Registry(path),
                    Disposition::Quarantined(QuarantineReason::DuplicateKey),
                );
                continue;
            }
            self.registry_value(kind, settings, path, name, value, &mut values);
        }
        let mut subkeys = Map::new();
        for (name, child) in &node.subkeys {
            let structure = escaped_len(name).saturating_add(LINE_OVERHEAD.saturating_mul(6));
            // The subkey's fields are reported either way; its object is kept only when it
            // fits the budget (its values then could not be kept either).
            let fits = self.spend(structure);
            let child = self.registry_node(kind, settings, child, &join(prefix, name));
            if fits {
                subkeys.insert(name.clone(), Value::Object(child));
            }
        }
        for (name, reason) in &node.skipped {
            if !self.room(kind) {
                break;
            }
            let reason = match reason {
                SkipReason::InvalidName => QuarantineReason::InvalidName,
                SkipReason::LimitExceeded => QuarantineReason::LimitExceeded,
            };
            self.report(
                kind,
                FieldRef::RegistryKey(join(prefix, name)),
                Disposition::Quarantined(reason),
            );
        }
        let mut map = Map::new();
        map.insert("values".to_owned(), Value::Object(values));
        map.insert("subkeys".to_owned(), Value::Object(subkeys));
        map
    }

    fn registry_value(
        &mut self,
        kind: SourceKind,
        settings: bool,
        path: String,
        name: &str,
        value: &LegacyValue,
        values: &mut Map<String, Value>,
    ) {
        if value.data_len() > MAX_FIELD_BYTES {
            self.report(
                kind,
                FieldRef::Registry(path),
                Disposition::Quarantined(QuarantineReason::TooLarge),
            );
            return;
        }
        let text = match registry_text(value) {
            RegistryText::Undecodable => {
                self.report(
                    kind,
                    FieldRef::Registry(path),
                    Disposition::Quarantined(QuarantineReason::Undecodable),
                );
                return;
            }
            RegistryText::NotText => None,
            RegistryText::Text(text) => Some(text),
        };
        let mut mapping = None;
        let disposition = if !settings {
            Disposition::Preserved
        } else if let Some(index) = self.registry_mapping(&path) {
            mapping = Some(index);
            Disposition::Preserved
        } else if let Some(members) = text.as_deref().and_then(exe_list) {
            self.named_set(members)
        } else {
            Disposition::Preserved
        };
        let disposition = self.keep(name, registry_json(value), values, disposition);
        let report = self.report(kind, FieldRef::Registry(path), disposition);
        if let Some(mapping) = mapping {
            self.candidates.push(Candidate {
                mapping,
                kind,
                report,
                value: RawSetting::Registry(value.clone()),
            });
        }
    }

    fn ini(&mut self, source: &DiscoveredSource, bytes: &[u8]) {
        let document = parse_ini(bytes);
        let mut sections = Map::new();
        let mut issues = document.issues.iter().peekable();
        for entry in &document.entries {
            while let Some(issue) = issues.next_if(|issue| issue.line < entry.line) {
                self.ini_issue(issue);
            }
            self.ini_entry(entry, &mut sections);
        }
        for issue in issues {
            self.ini_issue(issue);
        }
        let mut object = Map::new();
        object.insert(
            "location".to_owned(),
            Value::String(source.location.clone()),
        );
        object.insert(
            "encoding".to_owned(),
            Value::String(document.encoding.as_str().to_owned()),
        );
        object.insert("sections".to_owned(), Value::Object(sections));
        self.preserved.insert(
            SourceKind::SettingsIni.as_str().to_owned(),
            Value::Object(object),
        );
    }

    fn ini_issue(&mut self, issue: &IniIssue) {
        let kind = SourceKind::SettingsIni;
        if !self.room(kind) {
            return;
        }
        let reason = match issue.kind {
            IniIssueKind::Undecodable => QuarantineReason::Undecodable,
            IniIssueKind::Malformed => QuarantineReason::Malformed,
            IniIssueKind::DuplicateKey => QuarantineReason::DuplicateKey,
            IniIssueKind::LimitExceeded => QuarantineReason::LimitExceeded,
        };
        self.report(
            kind,
            FieldRef::Line(issue.line),
            Disposition::Quarantined(reason),
        );
    }

    fn ini_entry(&mut self, entry: &IniEntry, sections: &mut Map<String, Value>) {
        let kind = SourceKind::SettingsIni;
        if !self.room(kind) {
            return;
        }
        let field = FieldRef::Ini {
            section: entry.section.clone(),
            key: entry.key.clone(),
            line: entry.line,
        };
        if entry.value.len() > MAX_FIELD_BYTES {
            self.report(
                kind,
                field,
                Disposition::Quarantined(QuarantineReason::TooLarge),
            );
            return;
        }
        let mut mapping = None;
        let disposition = if let Some(index) = self.ini_mapping(&entry.section, &entry.key) {
            mapping = Some(index);
            Disposition::Preserved
        } else if let Some(members) = exe_list(&entry.value) {
            self.named_set(members)
        } else {
            Disposition::Preserved
        };
        if !sections.contains_key(&entry.section) {
            let structure =
                escaped_len(&entry.section).saturating_add(LINE_OVERHEAD.saturating_mul(2));
            if self.spend(structure) {
                sections.insert(entry.section.clone(), Value::Object(Map::new()));
            }
        }
        let disposition = match sections
            .get_mut(&entry.section)
            .and_then(Value::as_object_mut)
        {
            Some(section) => self.keep(
                &entry.key,
                Value::String(entry.value.clone()),
                section,
                disposition,
            ),
            // The section itself did not fit the budget.
            None => unkept(disposition),
        };
        let report = self.report(kind, field, disposition);
        if let Some(mapping) = mapping {
            self.candidates.push(Candidate {
                mapping,
                kind,
                report,
                value: RawSetting::Ini(entry.value.clone()),
            });
        }
    }

    fn tree(&mut self, source: &DiscoveredSource, bytes: &[u8]) {
        let kind = SourceKind::SettingsTree;
        let mut object = Map::new();
        object.insert(
            "location".to_owned(),
            Value::String(source.location.clone()),
        );
        object.insert("size".to_owned(), json!(source.size));
        object.insert("sha256".to_owned(), Value::String(source.sha256.clone()));
        let cost = bytes
            .len()
            .div_ceil(3)
            .saturating_mul(4)
            .saturating_add(LINE_OVERHEAD.saturating_mul(4));
        let disposition = if self.spend(cost) {
            object.insert("base64".to_owned(), Value::String(base64(bytes)));
            Disposition::Preserved
        } else {
            Disposition::Quarantined(QuarantineReason::LimitExceeded)
        };
        if self.room(kind) {
            self.report(kind, FieldRef::Content, disposition);
        }
        self.preserved
            .insert(kind.as_str().to_owned(), Value::Object(object));
    }

    fn macros(&mut self, bytes: &[u8]) {
        let kind = SourceKind::MacroText;
        let import = parse_legacy_text(bytes);
        for issue in &import.issues {
            if !self.room(kind) {
                break;
            }
            let field = if issue.line == 0 {
                FieldRef::Content
            } else {
                FieldRef::Line(issue.line)
            };
            self.report(
                kind,
                field,
                Disposition::Quarantined(QuarantineReason::Macro(issue.reason)),
            );
        }
        for (index, entry) in import.entries.into_iter().enumerate() {
            match normalize_entry(entry) {
                Ok(entry) => self.macros.push(entry),
                Err(error) => {
                    if self.room(kind) {
                        let position =
                            u64::try_from(index).map_or(u64::MAX, |index| index.saturating_add(1));
                        self.report(
                            kind,
                            FieldRef::MacroEntry(position),
                            Disposition::Quarantined(QuarantineReason::Macro(error.into())),
                        );
                    }
                }
            }
        }
    }

    fn registry_mapping(&self, path: &str) -> Option<usize> {
        let path = path.to_lowercase();
        self.field_map.iter().position(|mapping| {
            mapping
                .registry
                .is_some_and(|name| name.to_lowercase() == path)
        })
    }

    fn ini_mapping(&self, section: &str, key: &str) -> Option<usize> {
        let (section, key) = (section.to_lowercase(), key.to_lowercase());
        self.field_map.iter().position(|mapping| {
            mapping.ini.is_some_and(|(mapped_section, mapped_key)| {
                mapped_section.to_lowercase() == section && mapped_key.to_lowercase() == key
            })
        })
    }

    /// Applies the precedence of matrix #44 to the fields named by the field map.
    fn resolve_settings(&mut self) -> Vec<MappedSetting> {
        let mut settings = Vec::new();
        for (index, mapping) in self.field_map.iter().enumerate() {
            let mut found: Vec<&Candidate> = self
                .candidates
                .iter()
                .filter(|candidate| candidate.mapping == index)
                .collect();
            found.sort_by_key(|candidate| candidate.kind);
            for candidate in found {
                let mut scratch = ConfigDocument::defaults();
                let accepted = (mapping.apply)(&mut scratch, candidate.value.as_value()).is_ok();
                let disposition = if accepted {
                    settings.push(MappedSetting {
                        apply: mapping.apply,
                        value: candidate.value.clone(),
                    });
                    Disposition::Imported
                } else {
                    Disposition::Quarantined(QuarantineReason::InvalidValue)
                };
                if let Some(report) = self.fields.get_mut(candidate.report) {
                    report.disposition = disposition;
                }
                if accepted {
                    break;
                }
            }
        }
        settings
    }

    fn finish(mut self) -> ParsedMigration {
        let settings = self.resolve_settings();
        ParsedMigration {
            sources: self.sources,
            macros: self.macros,
            named_sets: self.named_sets,
            fields: self.fields,
            preserved: Value::Object(self.preserved),
            settings,
        }
    }
}

/// `prefix\name`, or `name` at the source key.
fn join(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_owned()
    } else {
        format!("{prefix}\\{name}")
    }
}

/// A name that is safe to record: control characters replaced, at most 128 characters.
fn record_safe(name: &str) -> String {
    name.chars()
        .take(MAX_RECORD_NAME_CHARS)
        .map(|c| if c.is_control() { '\u{FFFD}' } else { c })
        .collect()
}

fn registry_text(value: &LegacyValue) -> RegistryText {
    match value {
        LegacyValue::String(text) | LegacyValue::ExpandString(text) => {
            RegistryText::Text(text.clone())
        }
        LegacyValue::MultiString(items) => RegistryText::Text(items.join("\n")),
        LegacyValue::Other { kind, bytes } if *kind == REG_SZ || *kind == REG_EXPAND_SZ => {
            match decode_utf16le(bytes) {
                Some(text) => RegistryText::Text(text.trim_end_matches('\0').to_owned()),
                None => RegistryText::Undecodable,
            }
        }
        LegacyValue::Other { kind, bytes } if *kind == REG_MULTI_SZ => {
            match decode_utf16le(bytes) {
                Some(text) => RegistryText::Text(
                    text.split('\0')
                        .filter(|item| !item.is_empty())
                        .collect::<Vec<_>>()
                        .join("\n"),
                ),
                None => RegistryText::Undecodable,
            }
        }
        _ => RegistryText::NotText,
    }
}

/// Decodes UTF-16 LE bytes; `None` for an odd length or an unpaired surrogate.
fn decode_utf16le(bytes: &[u8]) -> Option<String> {
    let (units, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() {
        return None;
    }
    let units: Vec<u16> = units.iter().map(|unit| u16::from_le_bytes(*unit)).collect();
    String::from_utf16(&units).ok()
}

fn registry_json(value: &LegacyValue) -> Value {
    match value {
        LegacyValue::String(text) => json!({"type": "string", "data": text}),
        LegacyValue::ExpandString(text) => json!({"type": "expand-string", "data": text}),
        LegacyValue::MultiString(items) => json!({"type": "multi-string", "data": items}),
        LegacyValue::Dword(number) => json!({"type": "dword", "data": number}),
        LegacyValue::Qword(number) => json!({"type": "qword", "data": number}),
        LegacyValue::Binary(bytes) => json!({"type": "binary", "hex": hex(bytes)}),
        LegacyValue::Other { kind, bytes } => {
            json!({"type": "other", "kind": kind, "hex": hex(bytes)})
        }
    }
}

/// Estimated pretty-printed size of `"name": value`.
fn entry_cost(name: &str, value: &Value) -> usize {
    escaped_len(name)
        .saturating_add(4)
        .saturating_add(json_cost(value))
        .saturating_add(LINE_OVERHEAD)
}

fn json_cost(value: &Value) -> usize {
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => 24,
        Value::String(text) => escaped_len(text).saturating_add(2),
        Value::Array(items) => items.iter().fold(2usize, |total, item| {
            total
                .saturating_add(json_cost(item))
                .saturating_add(LINE_OVERHEAD)
        }),
        Value::Object(map) => map.iter().fold(2usize, |total, (key, item)| {
            total.saturating_add(entry_cost(key, item))
        }),
    }
}

/// Length of `text` as a JSON string body (serde_json escapes `"`, `\`, and control
/// characters).
fn escaped_len(text: &str) -> usize {
    text.chars().fold(0usize, |total, c| {
        let len = match c {
            '"' | '\\' | '\n' | '\r' | '\t' | '\u{8}' | '\u{c}' => 2,
            c if u32::from(c) < 0x20 => 6,
            c => c.len_utf8(),
        };
        total.saturating_add(len)
    })
}

/// Lowercase hexadecimal.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let digit = |nibble: u8| char::from(DIGITS.get(usize::from(nibble)).copied().unwrap_or(b'0'));
    let mut text = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        text.push(digit(byte >> 4));
        text.push(digit(byte & 0x0F));
    }
    text
}

/// Standard base64 with padding (RFC 4648 §4).
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut text = String::with_capacity(bytes.len().div_ceil(3).saturating_mul(4));
    for chunk in bytes.chunks(3) {
        let byte = |index: usize| u32::from(chunk.get(index).copied().unwrap_or(0));
        let group = (byte(0) << 16) | (byte(1) << 8) | byte(2);
        for (position, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if position <= chunk.len() {
                let index = usize::try_from((group >> shift) & 0x3F).unwrap_or(0);
                text.push(char::from(ALPHABET.get(index).copied().unwrap_or(b'A')));
            } else {
                text.push('=');
            }
        }
    }
    text
}

/// The disposition of a field whose raw value did not fit the preserved budget: a field that
/// would only have been preserved is quarantined; an imported field keeps its disposition.
fn unkept(disposition: Disposition) -> Disposition {
    if disposition == Disposition::Preserved {
        Disposition::Quarantined(QuarantineReason::LimitExceeded)
    } else {
        disposition
    }
}
