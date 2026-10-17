//! The persisted-document contract shared by every store file.

use core::fmt;

use crate::error::ConfigError;
use crate::fields::{Fields, Node};

/// Why a single field was not taken from a file.
///
/// Reasons are stable codes; they never describe the rejected value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IssueReason {
    /// A field the schema requires is absent.
    Missing,
    /// The value has the wrong type (for example a string where a boolean belongs).
    InvalidType,
    /// The value has the right type but is not an accepted value.
    InvalidValue,
    /// The number (or count) is outside the accepted range.
    OutOfRange,
    /// The entry repeats an identifier that an earlier entry already uses.
    DuplicateEntry,
    /// The value is kept as written, but it names something the file does not provide in
    /// usable form (such as `input.user_keymap` naming a key map that is absent or was set
    /// aside), so what depends on it falls back until the name can be resolved.
    Unresolved,
}

impl IssueReason {
    /// Stable kebab-case reason code.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::InvalidType => "invalid-type",
            Self::InvalidValue => "invalid-value",
            Self::OutOfRange => "out-of-range",
            Self::DuplicateEntry => "duplicate-entry",
            Self::Unresolved => "unresolved",
        }
    }
}

impl fmt::Display for IssueReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// A field that was defaulted, or an entry or list element that was set aside, while loading
/// a file (matrix #45).
///
/// `path` is built only from schema field names and array indices, such as
/// `input.context_limit`, `macro[3].boundary`, or `named_set[0].members[2]`; it never contains
/// user content.
///
/// - A path that ends in an array index (such as `named_set[0].members[2]` or
///   `keymaps[0].bindings[10]`) means that element was dropped.
/// - A path that names a field an element cannot exist without (each document lists them,
///   such as `macro[1].trigger`, `rule[0].exe`, or a binding's `key`) means the element was
///   dropped; an entry of a top-level array (`[[macro]]`, `[[keymaps]]`, `[[rule]]`, ...) is
///   instead set aside verbatim (see [`EntryExtras`](crate::EntryExtras)) and written back
///   unchanged.
/// - An [`IssueReason::Unresolved`] issue names a field that was kept as written; only what
///   depends on it falls back (see
///   [`ConfigDocument::engine_options`](crate::ConfigDocument::engine_options)).
/// - Any other path names a field that took its default.
///
/// A commit writes what the document holds, so a field that took its default is written with
/// that default and is not reported again. Issues about entries that were set aside, and about
/// a user input method without a usable key map (`input.user_keymap`), are reported on every
/// load until the file is fixed.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FieldIssue {
    /// Schema path of the field or entry.
    pub path: String,
    /// Why it was rejected.
    pub reason: IssueReason,
}

impl fmt::Display for FieldIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.reason)
    }
}

/// A decoded document plus the fields that had to be defaulted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded<D> {
    /// The document; rejected fields hold their defaults.
    pub document: D,
    /// Rejected fields, in the order the decoder visits the schema (not file order, which a
    /// parsed table does not keep); cross-field checks come last.
    pub issues: Vec<FieldIssue>,
}

impl<D> Loaded<D> {
    /// Wraps a document that needed no repair.
    pub fn clean(document: D) -> Self {
        Self {
            document,
            issues: Vec::new(),
        }
    }

    /// Whether every field was taken from the file.
    pub fn is_clean(&self) -> bool {
        self.issues.is_empty()
    }
}

/// A versioned document stored in one file of the data root.
///
/// Every document carries a schema `version` and a monotonically increasing `revision`, and
/// keeps unknown keys in extension maps so that they survive a rewrite. Values that a known
/// field does not accept do not survive: they are replaced by the field's default (see the
/// [crate documentation](crate)).
pub trait StoreDocument: Clone + Send + Sync + Sized + 'static {
    /// File name inside the data root, such as `config.toml`.
    const FILE_NAME: &'static str;
    /// Schema version written by this build.
    const CURRENT_VERSION: u32;

    /// Revision of this document (0 for a document that was never committed).
    fn revision(&self) -> u64;

    /// Replaces the revision.
    fn set_revision(&mut self, revision: u64);

    /// Decodes file text with field-level leniency.
    ///
    /// Syntax errors yield [`ConfigError::Corrupt`]. A `version` that is not an integer this
    /// build reads yields [`ConfigError::UnsupportedVersion`]; an absent `version` is read as
    /// the current one and reported as [`IssueReason::Missing`], so an empty file never loads
    /// silently. Otherwise every known field is decoded on its own: a bad field takes its
    /// default and is reported in [`Loaded::issues`], and unknown keys are kept in the
    /// document's extension maps.
    ///
    /// A revision that could not be incremented and written back is reported as
    /// [`IssueReason::OutOfRange`] and read as 0, so a store can always commit again.
    fn decode(text: &str) -> Result<Loaded<Self>, ConfigError>;

    /// Encodes the document in its file format.
    ///
    /// Fails with [`ConfigError::UnsupportedVersion`] when the document does not carry
    /// [`StoreDocument::CURRENT_VERSION`], and with [`ConfigError::Corrupt`] when a value
    /// cannot be represented in the file format.
    fn encode(&self) -> Result<String, ConfigError>;

    /// The document used when the file does not exist.
    fn default_document() -> Self;

    /// The revision of a file that starts with `head`, when the head alone determines it.
    ///
    /// [`AtomicStore::poll_revision`](crate::AtomicStore::poll_revision) reads only the first
    /// bytes of a file and asks this first, so that polling a large file stays cheap; it
    /// decodes the whole file only when this returns `None`. An implementation may return
    /// `Some` only when every file that starts with `head` and that [`StoreDocument::decode`]
    /// accepts decodes to that revision. The default recognises nothing.
    fn revision_from_head(_head: &[u8]) -> Option<u64> {
        None
    }
}

/// Largest revision a TOML document reads: one below the largest TOML integer, so that the
/// next commit can still write `revision + 1`.
pub(crate) const MAX_TOML_REVISION: u64 = 0x7FFF_FFFF_FFFF_FFFE;

/// Largest revision a JSON document reads, for the same reason.
pub(crate) const MAX_JSON_REVISION: u64 = u64::MAX - 1;

/// Strips a UTF-8 byte order mark, which some editors add.
pub(crate) fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Parses TOML text into a table; any syntax error is [`ConfigError::Corrupt`].
pub(crate) fn parse_toml(text: &str) -> Result<toml::Table, ConfigError> {
    strip_bom(text)
        .parse::<toml::Table>()
        .map_err(|_| ConfigError::Corrupt)
}

/// Encodes a TOML wire view; values TOML cannot hold yield [`ConfigError::Corrupt`].
///
/// The serializer accepts some values the TOML grammar rejects (integers above `i64::MAX`), so
/// the output is parsed again: a file that could not be read back is never produced.
pub(crate) fn encode_toml<T: serde::Serialize>(wire: &T) -> Result<String, ConfigError> {
    let text = toml::to_string(wire).map_err(|_| ConfigError::Corrupt)?;
    parse_toml(&text)?;
    Ok(text)
}

/// Rejects documents that do not carry the schema version this build writes.
pub(crate) fn ensure_current(version: u32, current: u32) -> Result<(), ConfigError> {
    if version == current {
        Ok(())
    } else {
        Err(ConfigError::UnsupportedVersion)
    }
}

/// Reads the top-level `version` field.
///
/// `version` decides how every other field is read, so it fails closed: anything but an
/// integer in `1..=current` is [`ConfigError::UnsupportedVersion`] (and a store never
/// overwrites such a file). An absent version is read as the current one and reported as
/// [`IssueReason::Missing`]. Version 1 is the first schema, so there is nothing to migrate
/// yet: every supported version decodes to the current one.
pub(crate) fn read_version<N: Node>(
    fields: &mut Fields<'_, N>,
    current: u32,
) -> Result<u32, ConfigError> {
    let Some(value) = fields.take("version") else {
        fields.report("version", IssueReason::Missing);
        return Ok(current);
    };
    match value.as_u64() {
        Some(found) if value.is_integer() && (1..=u64::from(current)).contains(&found) => {
            Ok(current)
        }
        _ => Err(ConfigError::UnsupportedVersion),
    }
}

/// Reads the top-level `revision` field (0 when absent), limited to `0..=max`.
pub(crate) fn read_revision<N: Node>(fields: &mut Fields<'_, N>, max: u64) -> u64 {
    fields.u64_in("revision", 0, 0..=max)
}

/// [`StoreDocument::revision_from_head`] for the TOML documents.
///
/// Their encoders always begin a file with `version = <v>\nrevision = <n>\n`. Those first two
/// lines are top-level keys and TOML forbids redefining a key, so when a file starts with them
/// and is valid at all, its revision is `n`. Anything else (comments first, CRLF, a BOM, a
/// version this build does not read, a revision that decode would reject) is left to a full
/// decode.
pub(crate) fn toml_head_revision(head: &[u8], current: u32) -> Option<u64> {
    let rest = head.strip_prefix(b"version = ")?;
    let (version, rest) = canonical_decimal(rest)?;
    let rest = rest.strip_prefix(b"\nrevision = ")?;
    let (revision, rest) = canonical_decimal(rest)?;
    let complete = rest.first() == Some(&b'\n');
    let supported = (1..=u64::from(current)).contains(&version);
    (complete && supported && revision <= MAX_TOML_REVISION).then_some(revision)
}

/// Splits a leading decimal integer as the TOML encoder writes it (no sign, no `_`, no leading
/// zero) off `bytes`.
fn canonical_decimal(bytes: &[u8]) -> Option<(u64, &[u8])> {
    let digits = bytes
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let (number, rest) = bytes.split_at_checked(digits)?;
    if number.is_empty() || (number.len() > 1 && number.first() == Some(&b'0')) {
        return None;
    }
    let value = core::str::from_utf8(number).ok()?.parse::<u64>().ok()?;
    Some((value, rest))
}
