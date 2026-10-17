//! `macros.toml`: persisted macro entries.

use serde::Serialize;

use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};

use crate::document::{
    Loaded, MAX_TOML_REVISION, StoreDocument, encode_toml, ensure_current, parse_toml,
    read_revision, read_version, toml_head_revision,
};
use crate::entries::{EntriesWire, EntryExtras, decode_entries, match_in_order};
use crate::error::ConfigError;
use crate::fields::{Extra, Fields, Peek, integer_in, text};

type TomlFields<'i> = Fields<'i, toml::Value>;

const ROOT_KEYS: &[&str] = &["version", "revision", MACRO_ARRAY_KEY];
const ENTRY_KEYS: &[&str] = &[
    "id",
    "trigger",
    "replacement",
    "enabled",
    "boundary",
    "case",
];
const IDENTITY_KEYS: &[&str] = &["id", "trigger", "replacement"];

/// Name of the `[[macro]]` array of tables.
pub const MACRO_ARRAY_KEY: &str = "macro";

/// Identity of a `[[macro]]` entry for its unknown keys: its id and trigger.
///
/// The id alone is not enough: `raumakey-macro` gives a new macro `max(id) + 1`, so after the
/// macro with the largest id is removed, the next new macro reuses that id. Triggers compare
/// exactly as written; one that `raumakey-macro` normalizes to NFC counts as a new trigger.
pub type MacroIdentity = (MacroId, String);

/// `macros.toml`: the macro table as an array of `[[macro]]` tables.
///
/// Decoding is structural only; semantic checks (NFC, lengths, duplicate ids) happen when
/// `raumakey-macro` builds its table. Each entry is decoded field by field (matrix #45):
///
/// - `id`, `trigger`, and `replacement` identify the macro. An entry where one of them is
///   missing or has the wrong type (or a negative id) is set aside verbatim in
///   [`MacroDocument::entry_extras`] and written back on save; the issue names that field.
/// - `enabled`, `boundary`, and `case` take their defaults when absent, and when invalid they
///   take their defaults and are reported.
/// - Unknown keys of an entry are kept by [`MacroIdentity`] (id and trigger) and written back
///   with the macro that has that id and trigger. They follow edits of the replacement and of
///   the switches. A macro whose trigger is changed has none, and neither has a new macro that
///   reuses the id of a removed one under another trigger, so unknown keys never move to a
///   macro with another trigger.
#[derive(Clone, Debug, PartialEq)]
pub struct MacroDocument {
    /// Schema version.
    pub version: u32,
    /// Store revision.
    pub revision: u64,
    /// Macro entries in file order.
    pub entries: Vec<MacroEntry>,
    /// Unknown keys of `[[macro]]` entries (by [`MacroIdentity`]) and the entries that could
    /// not be decoded.
    pub entry_extras: EntryExtras<MacroIdentity>,
    /// Unknown top-level keys, re-emitted on save.
    pub extensions: toml::Table,
}

impl Default for MacroDocument {
    fn default() -> Self {
        Self {
            version: <Self as StoreDocument>::CURRENT_VERSION,
            revision: 0,
            entries: Vec::new(),
            entry_extras: EntryExtras::default(),
            extensions: toml::Table::new(),
        }
    }
}

/// Checks the identity fields of one `[[macro]]` table without consuming them.
fn identify(peek: &mut Peek<'_, '_, toml::Value>) -> Option<(MacroId, String, String)> {
    let id = peek.require("id", |value| integer_in(value, 0..=u64::MAX).map(MacroId));
    let trigger = peek.require("trigger", |value| text(value).map(str::to_owned));
    let replacement = peek.require("replacement", |value| text(value).map(str::to_owned));
    Some((id?, trigger?, replacement?))
}

fn build(
    fields: &mut TomlFields<'_>,
    (id, trigger, replacement): (MacroId, String, String),
) -> (MacroEntry, MacroIdentity) {
    fields.drop_keys(IDENTITY_KEYS);
    let identity = (id, trigger.clone());
    let entry = MacroEntry {
        id,
        trigger,
        replacement,
        enabled: fields.bool("enabled", true),
        boundary: fields.keyword("boundary", BoundaryPolicy::default()),
        case_policy: fields.keyword("case", CasePolicy::default()),
    };
    (entry, identity)
}

impl StoreDocument for MacroDocument {
    const FILE_NAME: &'static str = "macros.toml";
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
        let (entries, entry_extras) = decode_entries(&mut root, MACRO_ARRAY_KEY, identify, build);
        let extensions = root.into_rest();
        let document = Self {
            version,
            revision,
            entries,
            entry_extras,
            extensions,
        };
        Ok(Loaded { document, issues })
    }

    fn encode(&self) -> Result<String, ConfigError> {
        ensure_current(self.version, Self::CURRENT_VERSION)?;
        let unknown = match_in_order(
            self.entry_extras
                .unknown_keys
                .iter()
                .map(|((id, trigger), table)| ((*id, trigger.as_str()), table)),
            self.entries
                .iter()
                .map(|entry| (entry.id, entry.trigger.as_str())),
        );
        let entries = self
            .entries
            .iter()
            .zip(unknown)
            .map(|(entry, unknown)| MacroEntryWire {
                id: entry.id.0,
                trigger: &entry.trigger,
                replacement: &entry.replacement,
                enabled: entry.enabled,
                boundary: entry.boundary,
                case: entry.case_policy,
                extensions: Extra::new(unknown, ENTRY_KEYS),
            })
            .collect();
        encode_toml(&MacroWire {
            version: self.version,
            revision: self.revision,
            entries: EntriesWire::new(entries, &self.entry_extras.rejected),
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

#[derive(Serialize)]
struct MacroWire<'a> {
    version: u32,
    revision: u64,
    #[serde(rename = "macro", skip_serializing_if = "EntriesWire::is_empty")]
    entries: EntriesWire<'a, MacroEntryWire<'a>>,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}

#[derive(Serialize)]
struct MacroEntryWire<'a> {
    id: u64,
    trigger: &'a str,
    replacement: &'a str,
    enabled: bool,
    boundary: BoundaryPolicy,
    case: CasePolicy,
    #[serde(flatten)]
    extensions: Extra<'a, toml::Table>,
}
