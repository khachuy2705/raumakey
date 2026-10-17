//! `migration.json`: the record of an EVKey import (plan §9).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::document::{
    IssueReason, Loaded, MAX_JSON_REVISION, StoreDocument, ensure_current, read_revision,
    read_version, strip_bom,
};
use crate::entries::{keep_records, match_in_order};
use crate::error::ConfigError;
use crate::fields::{Extra, Fields, Node, integer_in, owned_text};

type JsonMap = serde_json::Map<String, serde_json::Value>;
type JsonFields<'i> = Fields<'i, serde_json::Value>;

const ROOT_KEYS: &[&str] = &[
    "version",
    "revision",
    "status",
    "started_at_ms",
    "completed_at_ms",
    SOURCES_KEY,
    IMPORTED_KEY,
    QUARANTINED_KEY,
    "preserved",
    RESULT_KEY,
];
const SOURCE_KEYS: &[&str] = &["kind", "location", "sha256", "size"];
const IMPORTED_KEYS: &[&str] = &["settings", "macros", "named_sets", "probes"];
const QUARANTINED_KEYS: &[&str] = &["source", "field", "reason_code"];
const RESULT_KEYS: &[&str] = &[
    "config_revision",
    "macros_revision",
    "app_rules_revision",
    CHECKSUMS_KEY,
];

const SOURCE_IDENTITY: [&str; 2] = ["kind", "location"];
const QUARANTINED_IDENTITY: [&str; 2] = ["source", "field"];

const SOURCES_KEY: &str = "sources";
const IMPORTED_KEY: &str = "imported";
const QUARANTINED_KEY: &str = "quarantined";
const RESULT_KEY: &str = "result";
const CHECKSUMS_KEY: &str = "checksums";

/// Progress of a migration.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MigrationStatus {
    /// Nothing was attempted.
    #[default]
    NotStarted,
    /// A migration started and has not finished (or was interrupted).
    InProgress,
    /// Documents were committed and verified.
    Completed,
    /// A failure restored the previous documents.
    RolledBack,
    /// The migration failed before anything was committed.
    Failed,
}

impl MigrationStatus {
    /// Stable name used in `migration.json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not-started",
            Self::InProgress => "in-progress",
            Self::Completed => "completed",
            Self::RolledBack => "rolled-back",
            Self::Failed => "failed",
        }
    }
}

/// Identity of one read-only migration source.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFingerprint {
    /// Source kind chosen by the importer, such as `registry`, `settings-ini`,
    /// `settings-tree`, or `macro-text`.
    pub kind: String,
    /// Registry path or file path of the source.
    pub location: String,
    /// Lowercase hexadecimal SHA-256 of the source bytes (see [`crate::sha256_hex`]).
    pub sha256: String,
    /// Source size in bytes.
    pub size: u64,
}

/// How many items each target received.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ImportedCounts {
    /// Settings fields mapped into `config.toml`.
    #[serde(default)]
    pub settings: u64,
    /// Macro entries written to `macros.toml`.
    #[serde(default)]
    pub macros: u64,
    /// Named sets written to `app_rules.toml`.
    #[serde(default)]
    pub named_sets: u64,
    /// Probe records written to `app_rules.toml`.
    #[serde(default)]
    pub probes: u64,
}

/// A source field that was not imported.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct QuarantinedField {
    /// Source kind or location the field came from.
    pub source: String,
    /// Field name or line reference inside the source; never the field's value.
    pub field: String,
    /// Stable reason code chosen by the importer.
    pub reason_code: String,
}

/// What a completed migration committed.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommitResult {
    /// Revision of `config.toml` after the commit.
    pub config_revision: u64,
    /// Revision of `macros.toml` after the commit.
    pub macros_revision: u64,
    /// Revision of `app_rules.toml` after the commit.
    pub app_rules_revision: u64,
    /// Lowercase hexadecimal SHA-256 of each committed file, keyed by file name.
    #[serde(default)]
    pub checksums: BTreeMap<String, String>,
}

/// `migration.json`, schema v1.
#[derive(Clone, Debug, PartialEq)]
pub struct MigrationRecord {
    /// Schema version.
    pub version: u32,
    /// Store revision.
    pub revision: u64,
    /// Migration progress.
    pub status: MigrationStatus,
    /// Start time in milliseconds since the Unix epoch; 0 when not started.
    pub started_at_ms: u64,
    /// Completion time in milliseconds since the Unix epoch; 0 when not finished.
    pub completed_at_ms: u64,
    /// Fingerprints of the sources that were read.
    pub sources: Vec<SourceFingerprint>,
    /// Imported item counts.
    pub imported: ImportedCounts,
    /// Fields that were not imported.
    pub quarantined: Vec<QuarantinedField>,
    /// Raw values preserved for evidence and rollback (unmapped fields).
    pub preserved: serde_json::Value,
    /// What was committed, once the migration completed.
    pub result: Option<CommitResult>,
    /// Unknown keys, re-emitted on save.
    ///
    /// Holds the unknown top-level keys and, under the name of the known key they belong to,
    /// the unknown keys of the nested objects (whose types have no extension map):
    ///
    /// - `imported` and `result`: an object with the unknown keys of that object.
    /// - `sources` and `quarantined`: an array of records, each holding the identity of one
    ///   entry (`kind` and `location` for a source, `source` and `field` for a quarantined
    ///   field) and that entry's unknown keys. Records follow their entry by identity.
    pub extensions: JsonMap,
}

impl Default for MigrationRecord {
    fn default() -> Self {
        Self {
            version: <Self as StoreDocument>::CURRENT_VERSION,
            revision: 0,
            status: MigrationStatus::NotStarted,
            started_at_ms: 0,
            completed_at_ms: 0,
            sources: Vec::new(),
            imported: ImportedCounts::default(),
            quarantined: Vec::new(),
            preserved: serde_json::Value::Object(JsonMap::new()),
            result: None,
            extensions: JsonMap::new(),
        }
    }
}

/// Reads an unsigned integer (consuming form).
fn count(value: serde_json::Value) -> Result<u64, IssueReason> {
    integer_in(&value, 0..=u64::MAX)
}

/// The records kept under `sources` or `quarantined` in [`MigrationRecord::extensions`]: the
/// unknown keys of each entry plus its identity fields (`names`), so that they can follow the
/// entry (see [`keep_records`] for entries without unknown keys).
fn identity_records(
    records: Vec<((String, String), JsonMap)>,
    names: [&str; 2],
) -> Vec<serde_json::Value> {
    keep_records(records, JsonMap::is_empty)
        .into_iter()
        .map(|((first, second), mut unknown)| {
            for (name, value) in names.into_iter().zip([first, second]) {
                unknown.insert(name.to_owned(), serde_json::Value::String(value));
            }
            serde_json::Value::Object(unknown)
        })
        .collect()
}

fn decode_sources(root: &mut JsonFields<'_>) -> (Vec<SourceFingerprint>, Vec<serde_json::Value>) {
    let mut sources = Vec::new();
    let mut records = Vec::new();
    root.each_table(SOURCES_KEY, |root, path, map| {
        let mut fields = root.at(map, path);
        let source = SourceFingerprint {
            kind: fields.needed("kind", String::new(), owned_text),
            location: fields.needed("location", String::new(), owned_text),
            sha256: fields.needed("sha256", String::new(), owned_text),
            size: fields.needed("size", 0, count),
        };
        let identity = (source.kind.clone(), source.location.clone());
        records.push((identity, fields.into_rest()));
        sources.push(source);
    });
    (sources, identity_records(records, SOURCE_IDENTITY))
}

fn decode_quarantined(
    root: &mut JsonFields<'_>,
) -> (Vec<QuarantinedField>, Vec<serde_json::Value>) {
    let mut quarantined = Vec::new();
    let mut records = Vec::new();
    root.each_table(QUARANTINED_KEY, |root, path, map| {
        let mut fields = root.at(map, path);
        let entry = QuarantinedField {
            source: fields.needed("source", String::new(), owned_text),
            field: fields.needed("field", String::new(), owned_text),
            reason_code: fields.needed("reason_code", String::new(), owned_text),
        };
        let identity = (entry.source.clone(), entry.field.clone());
        records.push((identity, fields.into_rest()));
        quarantined.push(entry);
    });
    (quarantined, identity_records(records, QUARANTINED_IDENTITY))
}

fn decode_imported(root: &mut JsonFields<'_>) -> (ImportedCounts, JsonMap) {
    let map = root.table(IMPORTED_KEY).unwrap_or_default();
    let mut fields = root.child(map, IMPORTED_KEY);
    let counts = ImportedCounts {
        settings: fields.u64("settings", 0),
        macros: fields.u64("macros", 0),
        named_sets: fields.u64("named_sets", 0),
        probes: fields.u64("probes", 0),
    };
    (counts, fields.into_rest())
}

fn decode_result(root: &mut JsonFields<'_>) -> (Option<CommitResult>, JsonMap) {
    let Some(value) = root.take(RESULT_KEY) else {
        return (None, JsonMap::new());
    };
    if value.is_null() {
        return (None, JsonMap::new());
    }
    let Ok(map) = value.into_map() else {
        root.report(RESULT_KEY, IssueReason::InvalidType);
        return (None, JsonMap::new());
    };
    let mut fields = root.child(map, RESULT_KEY);
    let result = CommitResult {
        config_revision: fields.needed("config_revision", 0, count),
        macros_revision: fields.needed("macros_revision", 0, count),
        app_rules_revision: fields.needed("app_rules_revision", 0, count),
        checksums: decode_checksums(&mut fields),
    };
    (Some(result), fields.into_rest())
}

/// `checksums`: file name to checksum. An entry whose checksum is not a string is dropped, and
/// the field is reported once (file names are not echoed into issue paths).
fn decode_checksums(fields: &mut JsonFields<'_>) -> BTreeMap<String, String> {
    let Some(map) = fields.table(CHECKSUMS_KEY) else {
        return BTreeMap::new();
    };
    let mut checksums = BTreeMap::new();
    let mut dropped = false;
    for (name, value) in map {
        match value.into_string() {
            Some(checksum) => {
                checksums.insert(name, checksum);
            }
            None => dropped = true,
        }
    }
    if dropped {
        fields.report(CHECKSUMS_KEY, IssueReason::InvalidType);
    }
    checksums
}

impl StoreDocument for MigrationRecord {
    const FILE_NAME: &'static str = "migration.json";
    const CURRENT_VERSION: u32 = 1;

    fn revision(&self) -> u64 {
        self.revision
    }

    fn set_revision(&mut self, revision: u64) {
        self.revision = revision;
    }

    fn decode(text: &str) -> Result<Loaded<Self>, ConfigError> {
        let value: serde_json::Value =
            serde_json::from_str(strip_bom(text)).map_err(|_| ConfigError::Corrupt)?;
        let serde_json::Value::Object(map) = value else {
            return Err(ConfigError::Corrupt);
        };
        let mut issues = Vec::new();
        let mut root = JsonFields::new(map, "", &mut issues);
        let version = read_version(&mut root, Self::CURRENT_VERSION)?;
        let revision = read_revision(&mut root, MAX_JSON_REVISION);
        let status = root.keyword("status", MigrationStatus::NotStarted);
        let started_at_ms = root.u64("started_at_ms", 0);
        let completed_at_ms = root.u64("completed_at_ms", 0);
        let (sources, source_records) = decode_sources(&mut root);
        let (imported, imported_unknown) = decode_imported(&mut root);
        let (quarantined, quarantined_records) = decode_quarantined(&mut root);
        let preserved = root
            .take("preserved")
            .unwrap_or_else(|| serde_json::Value::Object(JsonMap::new()));
        let (result, result_unknown) = decode_result(&mut root);
        let mut extensions = root.into_rest();
        if !source_records.is_empty() {
            extensions.insert(
                SOURCES_KEY.to_owned(),
                serde_json::Value::Array(source_records),
            );
        }
        if !imported_unknown.is_empty() {
            extensions.insert(
                IMPORTED_KEY.to_owned(),
                serde_json::Value::Object(imported_unknown),
            );
        }
        if !quarantined_records.is_empty() {
            extensions.insert(
                QUARANTINED_KEY.to_owned(),
                serde_json::Value::Array(quarantined_records),
            );
        }
        if !result_unknown.is_empty() {
            extensions.insert(
                RESULT_KEY.to_owned(),
                serde_json::Value::Object(result_unknown),
            );
        }
        let document = Self {
            version,
            revision,
            status,
            started_at_ms,
            completed_at_ms,
            sources,
            imported,
            quarantined,
            preserved,
            result,
            extensions,
        };
        Ok(Loaded { document, issues })
    }

    fn encode(&self) -> Result<String, ConfigError> {
        ensure_current(self.version, Self::CURRENT_VERSION)?;
        let nested_object = |key: &str| {
            self.extensions
                .get(key)
                .and_then(serde_json::Value::as_object)
        };
        let source_unknown = self.records(
            SOURCES_KEY,
            SOURCE_IDENTITY,
            self.sources
                .iter()
                .map(|source| (source.kind.as_str(), source.location.as_str())),
        );
        let quarantined_unknown = self.records(
            QUARANTINED_KEY,
            QUARANTINED_IDENTITY,
            self.quarantined
                .iter()
                .map(|entry| (entry.source.as_str(), entry.field.as_str())),
        );
        let wire = MigrationWire {
            version: self.version,
            revision: self.revision,
            status: self.status,
            started_at_ms: self.started_at_ms,
            completed_at_ms: self.completed_at_ms,
            sources: self
                .sources
                .iter()
                .zip(source_unknown)
                .map(|(source, unknown)| SourceWire {
                    kind: &source.kind,
                    location: &source.location,
                    sha256: &source.sha256,
                    size: source.size,
                    extensions: Extra::new(unknown, SOURCE_KEYS),
                })
                .collect(),
            imported: ImportedWire {
                settings: self.imported.settings,
                macros: self.imported.macros,
                named_sets: self.imported.named_sets,
                probes: self.imported.probes,
                extensions: Extra::new(nested_object(IMPORTED_KEY), IMPORTED_KEYS),
            },
            quarantined: self
                .quarantined
                .iter()
                .zip(quarantined_unknown)
                .map(|(entry, unknown)| QuarantinedWire {
                    source: &entry.source,
                    field: &entry.field,
                    reason_code: &entry.reason_code,
                    extensions: Extra::new(unknown, QUARANTINED_KEYS),
                })
                .collect(),
            preserved: &self.preserved,
            result: self.result.as_ref().map(|result| ResultWire {
                config_revision: result.config_revision,
                macros_revision: result.macros_revision,
                app_rules_revision: result.app_rules_revision,
                checksums: &result.checksums,
                extensions: Extra::new(nested_object(RESULT_KEY), RESULT_KEYS),
            }),
            extensions: Extra::new(Some(&self.extensions), ROOT_KEYS),
        };
        let mut text = serde_json::to_string_pretty(&wire).map_err(|_| ConfigError::Corrupt)?;
        text.push('\n');
        Ok(text)
    }

    fn default_document() -> Self {
        Self::default()
    }
}

impl MigrationRecord {
    /// The unknown keys recorded under `key` for each entry identity of `ids`.
    fn records<'s, 'k>(
        &'s self,
        key: &str,
        [first, second]: [&str; 2],
        ids: impl Iterator<Item = (&'k str, &'k str)>,
    ) -> Vec<Option<&'s JsonMap>>
    where
        's: 'k,
    {
        let records = self
            .extensions
            .get(key)
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_object)
            .filter_map(|record| {
                let identity = (record.get(first)?.as_str()?, record.get(second)?.as_str()?);
                Some((identity, record))
            });
        match_in_order(records, ids)
    }
}

#[derive(Serialize)]
struct MigrationWire<'a> {
    version: u32,
    revision: u64,
    status: MigrationStatus,
    started_at_ms: u64,
    completed_at_ms: u64,
    sources: Vec<SourceWire<'a>>,
    imported: ImportedWire<'a>,
    quarantined: Vec<QuarantinedWire<'a>>,
    preserved: &'a serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<ResultWire<'a>>,
    #[serde(flatten)]
    extensions: Extra<'a, JsonMap>,
}

#[derive(Serialize)]
struct SourceWire<'a> {
    kind: &'a str,
    location: &'a str,
    sha256: &'a str,
    size: u64,
    #[serde(flatten)]
    extensions: Extra<'a, JsonMap>,
}

#[derive(Serialize)]
struct ImportedWire<'a> {
    settings: u64,
    macros: u64,
    named_sets: u64,
    probes: u64,
    #[serde(flatten)]
    extensions: Extra<'a, JsonMap>,
}

#[derive(Serialize)]
struct QuarantinedWire<'a> {
    source: &'a str,
    field: &'a str,
    reason_code: &'a str,
    #[serde(flatten)]
    extensions: Extra<'a, JsonMap>,
}

#[derive(Serialize)]
struct ResultWire<'a> {
    config_revision: u64,
    macros_revision: u64,
    app_rules_revision: u64,
    checksums: &'a BTreeMap<String, String>,
    #[serde(flatten)]
    extensions: Extra<'a, JsonMap>,
}
