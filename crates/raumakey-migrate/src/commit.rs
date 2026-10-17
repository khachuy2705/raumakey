//! Preview and atomic commit of a parsed migration (plan §9, design §9).
//!
//! [`commit`] merges a [`ParsedMigration`] into the Raumakey documents:
//!
//! 1. **Idempotence.** When `raumakey-migration.json` says [`MigrationStatus::Completed`] for
//!    exactly the same source fingerprints (in any order), nothing is written and the outcome
//!    is [`MigrationOutcome::NoChange`].
//! 2. **Snapshot.** Every document is loaded with
//!    [`AtomicStore::load_or_default`]; a document that cannot be loaded (corrupt, newer
//!    version, unreadable) aborts the migration before anything is written. The loaded
//!    documents are the snapshot that a rollback restores.
//! 3. **Merge.** Macros are appended with fresh identifiers above every identifier in use
//!    (also those of entries the document keeps verbatim); an entry whose NFC trigger already
//!    exists, or appeared earlier in the import, is skipped as a conflict. Named sets are
//!    appended unless a set with the same name exists (a conflict). `raumakey.toml` changes
//!    only through [`FIELD_MAP`](crate::FIELD_MAP). A document is written only when it changes, and
//!    every changed document is encoded before the first write, so a value its format cannot
//!    hold or a document over the store's size limit also aborts before anything is written.
//! 4. **Record.** `raumakey-migration.json` is written with status
//!    [`MigrationStatus::InProgress`], the source fingerprints, the quarantined fields, and the
//!    preserved raw values.
//! 5. **Commit and verify.** The changed documents are committed in the order `raumakey.toml`,
//!    `raumakey-macros.toml`, `raumakey-app-rules.toml`, each with the revision it was loaded
//!    at (so a concurrent change is a [`ConfigError::Conflict`], never overwritten). Each is
//!    then read back from disk: its revision and the SHA-256 of its bytes must match what was
//!    committed.
//! 6. **Complete.** Only then is the record written as [`MigrationStatus::Completed`] with the
//!    imported counts and the [`CommitResult`] (revisions and checksums).
//!
//! Any failure after step 4 restores the snapshot of every document already committed (in
//! reverse order, each with the revision the migration gave it) and records
//! [`MigrationStatus::RolledBack`], or [`MigrationStatus::Failed`] when a document could not be
//! restored (the error is then [`MigrateError::RollbackFailed`]). The original error is
//! returned. A document that did not exist before is restored as its default document, which
//! is what readers saw before; revisions only grow. A restore writes the document as it was
//! loaded, so a field that had been reported as invalid on load keeps its default.
//!
//! Sources are never touched: this module has no access to them. An interrupted migration
//! (status `in-progress`) runs again in full; because the merge skips what already exists, the
//! rerun converges.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::Read as _;
use std::path::Path;

use raumakey_config::{
    AppRulesDocument, AtomicStore, CommitResult, ConfigDocument, ConfigError, ImportedCounts,
    MacroDocument, MigrationRecord, MigrationStatus, SourceFingerprint, StoreDocument, sha256_hex,
};
use raumakey_macro::normalize_trigger;
use raumakey_model::macros::{MacroEntry, MacroId};
use raumakey_model::policy::NamedSet;

use crate::discover::SourceKind;
use crate::error::{DocumentKind, MigrateError};
use crate::parse::ParsedMigration;

/// Largest macro identifier `raumakey-macros.toml` can hold (TOML integers are signed 64-bit).
const MAX_MACRO_ID: u64 = 0x7FFF_FFFF_FFFF_FFFF;

/// The stores of the Raumakey data root that a migration reads and writes.
///
/// Pass the controller's own store instances so that their subscribers see the commits.
#[derive(Clone, Copy)]
pub struct MigrationStores<'a> {
    /// `raumakey.toml`.
    pub config: &'a AtomicStore<ConfigDocument>,
    /// `raumakey-macros.toml`.
    pub macros: &'a AtomicStore<MacroDocument>,
    /// `raumakey-app-rules.toml`.
    pub app_rules: &'a AtomicStore<AppRulesDocument>,
    /// `raumakey-migration.json`.
    pub record: &'a AtomicStore<MigrationRecord>,
}

impl MigrationStores<'_> {
    /// Loads every document (a missing file as its default) without writing anything; input
    /// for [`preview`].
    pub fn load_current(&self) -> Result<CurrentDocuments, MigrateError> {
        Ok(CurrentDocuments {
            record: load(self.record, DocumentKind::Record)?,
            config: load(self.config, DocumentKind::Config)?,
            macros: load(self.macros, DocumentKind::Macros)?,
            app_rules: load(self.app_rules, DocumentKind::AppRules)?,
        })
    }
}

impl fmt::Debug for MigrationStores<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MigrationStores").finish_non_exhaustive()
    }
}

/// The current Raumakey documents.
///
/// The [`fmt::Debug`] output shows revisions and counts, never macro text.
#[derive(Clone, PartialEq)]
pub struct CurrentDocuments {
    /// `raumakey.toml`.
    pub config: ConfigDocument,
    /// `raumakey-macros.toml`.
    pub macros: MacroDocument,
    /// `raumakey-app-rules.toml`.
    pub app_rules: AppRulesDocument,
    /// `raumakey-migration.json`.
    pub record: MigrationRecord,
}

impl fmt::Debug for CurrentDocuments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CurrentDocuments")
            .field("config_revision", &self.config.revision)
            .field("macros_revision", &self.macros.revision)
            .field("macros", &self.macros.entries.len())
            .field("app_rules_revision", &self.app_rules.revision)
            .field("named_sets", &self.app_rules.compatibility.named_sets.len())
            .field("record_revision", &self.record.revision)
            .field("record_status", &self.record.status)
            .finish()
    }
}

/// What [`commit`] would do with the current documents.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MigrationPreview {
    /// The same sources were already imported; [`commit`] would change nothing, so every count
    /// of changes below is zero.
    pub up_to_date: bool,
    /// Settings that would be stored through [`FIELD_MAP`](crate::FIELD_MAP).
    pub settings: usize,
    /// Macros that would be added.
    pub macro_count: usize,
    /// Macros that would be skipped because their trigger exists.
    pub macro_conflicts: usize,
    /// Named sets that would be added.
    pub named_sets: usize,
    /// Named sets that would be skipped because their name exists.
    pub named_set_conflicts: usize,
    /// Quarantined source fields.
    pub quarantined: usize,
    /// Source fields kept only as raw values, by source
    /// ([`ParsedMigration::preserved_counts`]).
    pub preserved_counts: BTreeMap<SourceKind, usize>,
}

/// Computes what [`commit`] would do, without writing anything.
pub fn preview(parsed: &ParsedMigration, current: &CurrentDocuments) -> MigrationPreview {
    let base = MigrationPreview {
        up_to_date: is_up_to_date(&current.record, parsed),
        quarantined: parsed.quarantined_count(),
        preserved_counts: parsed.preserved_counts(),
        ..MigrationPreview::default()
    };
    if base.up_to_date {
        return base;
    }
    let plan = plan(parsed, current);
    MigrationPreview {
        settings: plan.settings,
        macro_count: plan.macros.len(),
        macro_conflicts: plan.macro_conflicts,
        named_sets: plan.named_sets.len(),
        named_set_conflicts: plan.named_set_conflicts,
        ..base
    }
}

/// Whether [`commit`] imported anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MigrationOutcome {
    /// The migration ran and was recorded as completed.
    Imported,
    /// The same sources were already imported; nothing was written.
    NoChange,
}

/// The result of a successful [`commit`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MigrationReport {
    /// Whether anything was imported.
    pub outcome: MigrationOutcome,
    /// What was added (all zero for [`MigrationOutcome::NoChange`]).
    pub imported: ImportedCounts,
    /// Macros skipped because their trigger existed.
    pub macro_conflicts: u64,
    /// Named sets skipped because their name existed.
    pub named_set_conflicts: u64,
    /// Quarantined source fields.
    pub quarantined: u64,
    /// Revision of `raumakey-migration.json` after the call.
    pub record_revision: u64,
    /// Revisions and checksums of the committed documents; `None` for
    /// [`MigrationOutcome::NoChange`].
    pub result: Option<CommitResult>,
}

/// Commits `parsed` into the stores (see the [module documentation](self)).
///
/// `now_ms` (milliseconds since the Unix epoch) is recorded as start and completion time.
/// Fails with [`MigrateError::NoSources`], without writing, when `parsed` has no source.
pub fn commit(
    parsed: &ParsedMigration,
    stores: &MigrationStores<'_>,
    now_ms: u64,
) -> Result<MigrationReport, MigrateError> {
    if parsed.sources().is_empty() {
        return Err(MigrateError::NoSources);
    }
    let record = load(stores.record, DocumentKind::Record)?;
    if is_up_to_date(&record, parsed) {
        return Ok(MigrationReport {
            outcome: MigrationOutcome::NoChange,
            imported: ImportedCounts::default(),
            macro_conflicts: 0,
            named_set_conflicts: 0,
            quarantined: 0,
            record_revision: record.revision,
            result: None,
        });
    }
    let current = CurrentDocuments {
        record,
        config: load(stores.config, DocumentKind::Config)?,
        macros: load(stores.macros, DocumentKind::Macros)?,
        app_rules: load(stores.app_rules, DocumentKind::AppRules)?,
    };
    let plan = plan(parsed, &current);
    let imported = ImportedCounts {
        settings: count(plan.settings),
        macros: count(plan.macros.len()),
        named_sets: count(plan.named_sets.len()),
        probes: 0,
    };
    let changes = Changes::prepare(&current, plan.config, plan.macros, plan.named_sets)?;

    let mut record = current.record.clone();
    record.status = MigrationStatus::InProgress;
    record.started_at_ms = now_ms;
    record.completed_at_ms = 0;
    record.sources = parsed.sources().to_vec();
    record.imported = ImportedCounts::default();
    record.quarantined = parsed.quarantined();
    record.preserved = parsed.preserved().clone();
    record.result = None;
    let in_progress = stores
        .record
        .commit(current.record.revision, &record)
        .map_err(|error| store_error(DocumentKind::Record, error))?;

    let mut written = Vec::new();
    let failure = match changes.apply(stores, &current, &mut written) {
        Ok(result) => {
            record.status = MigrationStatus::Completed;
            record.completed_at_ms = now_ms;
            record.imported = imported;
            record.result = Some(result.clone());
            match stores.record.commit(in_progress, &record) {
                Ok(record_revision) => {
                    return Ok(MigrationReport {
                        outcome: MigrationOutcome::Imported,
                        imported,
                        macro_conflicts: count(plan.macro_conflicts),
                        named_set_conflicts: count(plan.named_set_conflicts),
                        quarantined: count(parsed.quarantined_count()),
                        record_revision,
                        result: Some(result),
                    });
                }
                Err(error) => store_error(DocumentKind::Record, error),
            }
        }
        Err(error) => error,
    };
    Err(roll_back(
        failure,
        stores,
        &current,
        &written,
        record,
        in_progress,
        now_ms,
    ))
}

/// The merge of a parsed migration into the current documents.
struct MergePlan {
    /// The changed configuration, if it changed.
    config: Option<ConfigDocument>,
    settings: usize,
    /// Macros to append (identifiers are assigned by [`Changes::prepare`]).
    macros: Vec<MacroEntry>,
    macro_conflicts: usize,
    named_sets: Vec<NamedSet>,
    named_set_conflicts: usize,
}

fn plan(parsed: &ParsedMigration, current: &CurrentDocuments) -> MergePlan {
    let mut config = current.config.clone();
    let settings = parsed
        .settings()
        .iter()
        .filter(|setting| setting.apply_to(&mut config))
        .count();
    let config = (config != current.config).then_some(config);

    let mut triggers = existing_triggers(&current.macros);
    let mut macros = Vec::new();
    let mut macro_conflicts = 0usize;
    for entry in parsed.macros() {
        if triggers.insert(entry.trigger.clone()) {
            macros.push(entry.clone());
        } else {
            macro_conflicts = macro_conflicts.saturating_add(1);
        }
    }

    let mut names: BTreeSet<&str> = current
        .app_rules
        .compatibility
        .named_sets
        .iter()
        .map(|set| set.name.as_str())
        .collect();
    let mut named_sets = Vec::new();
    let mut named_set_conflicts = 0usize;
    for set in parsed.named_sets() {
        if names.insert(set.name.as_str()) {
            named_sets.push(set.clone());
        } else {
            named_set_conflicts = named_set_conflicts.saturating_add(1);
        }
    }

    MergePlan {
        config,
        settings,
        macros,
        macro_conflicts,
        named_sets,
        named_set_conflicts,
    }
}

/// NFC triggers in use: of the decoded entries and of the entries kept verbatim.
fn existing_triggers(document: &MacroDocument) -> BTreeSet<String> {
    let nfc = |trigger: &str| normalize_trigger(trigger).unwrap_or_else(|_| trigger.to_owned());
    let mut triggers: BTreeSet<String> = document
        .entries
        .iter()
        .map(|entry| nfc(&entry.trigger))
        .collect();
    for table in &document.entry_extras.rejected {
        if let Some(trigger) = table.get("trigger").and_then(|value| value.as_str()) {
            triggers.insert(nfc(trigger));
        }
    }
    triggers
}

/// Largest identifier in use, also by entries kept verbatim.
fn max_macro_id(document: &MacroDocument) -> Option<u64> {
    let decoded = document.entries.iter().map(|entry| entry.id.0);
    let kept = document
        .entry_extras
        .rejected
        .iter()
        .filter_map(|table| table.get("id").and_then(|value| value.as_integer()))
        .filter_map(|id| u64::try_from(id).ok());
    decoded.chain(kept).max()
}

/// `current` with `entries` appended under fresh identifiers.
fn with_macros(
    current: &MacroDocument,
    entries: Vec<MacroEntry>,
) -> Result<MacroDocument, MigrateError> {
    let mut document = current.clone();
    let mut next = match max_macro_id(current) {
        None => 1,
        Some(max) => max.checked_add(1).ok_or(MigrateError::MacroIdsExhausted)?,
    };
    for entry in entries {
        if next > MAX_MACRO_ID {
            return Err(MigrateError::MacroIdsExhausted);
        }
        document.entries.push(MacroEntry {
            id: MacroId(next),
            ..entry
        });
        next = next.saturating_add(1);
    }
    Ok(document)
}

/// A document to commit, encoded once beforehand.
struct Change<D> {
    kind: DocumentKind,
    /// Revision the document was loaded at.
    expected_revision: u64,
    /// The document with the revision the commit gives it.
    document: D,
    /// SHA-256 of its encoding.
    sha256: String,
}

impl<D: StoreDocument> Change<D> {
    fn prepare(kind: DocumentKind, current: &D, mut document: D) -> Result<Self, MigrateError> {
        let expected_revision = current.revision();
        let next = expected_revision
            .checked_add(1)
            .ok_or(store_error(kind, ConfigError::IoFailure))?;
        document.set_revision(next);
        let text = document
            .encode()
            .map_err(|error| store_error(kind, error))?;
        let too_large =
            u64::try_from(text.len()).map_or(true, |len| len > AtomicStore::<D>::MAX_FILE_BYTES);
        if too_large {
            return Err(store_error(kind, ConfigError::IoFailure));
        }
        Ok(Self {
            kind,
            expected_revision,
            document,
            sha256: sha256_hex(text.as_bytes()),
        })
    }

    /// Commits the document and records that it was written.
    fn write(
        &self,
        store: &AtomicStore<D>,
        written: &mut Vec<(DocumentKind, u64)>,
    ) -> Result<u64, MigrateError> {
        let revision = store
            .commit(self.expected_revision, &self.document)
            .map_err(|error| store_error(self.kind, error))?;
        written.push((self.kind, revision));
        if revision != self.document.revision() {
            return Err(MigrateError::Verification {
                document: self.kind,
            });
        }
        Ok(revision)
    }

    /// Reads the committed file back and checks its revision and checksum.
    fn verify(&self, store: &AtomicStore<D>) -> Result<String, MigrateError> {
        let failure = || MigrateError::Verification {
            document: self.kind,
        };
        let bytes = read_back::<D>(store.path()).ok_or_else(failure)?;
        let text = core::str::from_utf8(&bytes).map_err(|_| failure())?;
        let loaded = D::decode(text).map_err(|_| failure())?;
        let sha256 = sha256_hex(&bytes);
        if loaded.document.revision() != self.document.revision() || sha256 != self.sha256 {
            return Err(failure());
        }
        Ok(sha256)
    }
}

/// The documents a migration changes.
struct Changes {
    config: Option<Change<ConfigDocument>>,
    macros: Option<Change<MacroDocument>>,
    app_rules: Option<Change<AppRulesDocument>>,
}

impl Changes {
    fn prepare(
        current: &CurrentDocuments,
        config: Option<ConfigDocument>,
        macros: Vec<MacroEntry>,
        named_sets: Vec<NamedSet>,
    ) -> Result<Self, MigrateError> {
        let config = config
            .map(|document| Change::prepare(DocumentKind::Config, &current.config, document))
            .transpose()?;
        let macros = if macros.is_empty() {
            None
        } else {
            let document = with_macros(&current.macros, macros)?;
            Some(Change::prepare(
                DocumentKind::Macros,
                &current.macros,
                document,
            )?)
        };
        let app_rules = if named_sets.is_empty() {
            None
        } else {
            let mut document = current.app_rules.clone();
            document.compatibility.named_sets.extend(named_sets);
            Some(Change::prepare(
                DocumentKind::AppRules,
                &current.app_rules,
                document,
            )?)
        };
        Ok(Self {
            config,
            macros,
            app_rules,
        })
    }

    /// Commits every change, then verifies each one.
    fn apply(
        &self,
        stores: &MigrationStores<'_>,
        current: &CurrentDocuments,
        written: &mut Vec<(DocumentKind, u64)>,
    ) -> Result<CommitResult, MigrateError> {
        let mut result = CommitResult {
            config_revision: current.config.revision,
            macros_revision: current.macros.revision,
            app_rules_revision: current.app_rules.revision,
            checksums: BTreeMap::new(),
        };
        if let Some(change) = &self.config {
            result.config_revision = change.write(stores.config, written)?;
        }
        if let Some(change) = &self.macros {
            result.macros_revision = change.write(stores.macros, written)?;
        }
        if let Some(change) = &self.app_rules {
            result.app_rules_revision = change.write(stores.app_rules, written)?;
        }
        if let Some(change) = &self.config {
            let checksum = change.verify(stores.config)?;
            result
                .checksums
                .insert(change.kind.file_name().to_owned(), checksum);
        }
        if let Some(change) = &self.macros {
            let checksum = change.verify(stores.macros)?;
            result
                .checksums
                .insert(change.kind.file_name().to_owned(), checksum);
        }
        if let Some(change) = &self.app_rules {
            let checksum = change.verify(stores.app_rules)?;
            result
                .checksums
                .insert(change.kind.file_name().to_owned(), checksum);
        }
        Ok(result)
    }
}

/// Restores the snapshot of every written document and records the outcome; returns the error
/// to report.
fn roll_back(
    cause: MigrateError,
    stores: &MigrationStores<'_>,
    current: &CurrentDocuments,
    written: &[(DocumentKind, u64)],
    mut record: MigrationRecord,
    in_progress: u64,
    now_ms: u64,
) -> MigrateError {
    let mut unrestored = None;
    for &(kind, revision) in written.iter().rev() {
        let restored = match kind {
            DocumentKind::Config => stores.config.commit(revision, &current.config).is_ok(),
            DocumentKind::Macros => stores.macros.commit(revision, &current.macros).is_ok(),
            DocumentKind::AppRules => stores
                .app_rules
                .commit(revision, &current.app_rules)
                .is_ok(),
            DocumentKind::Record => true,
        };
        if !restored && unrestored.is_none() {
            unrestored = Some(kind);
        }
    }
    record.status = if unrestored.is_some() {
        MigrationStatus::Failed
    } else {
        MigrationStatus::RolledBack
    };
    record.completed_at_ms = now_ms;
    record.imported = ImportedCounts::default();
    record.result = None;
    // Best effort: the error that caused the rollback is what the caller must see. When the
    // record cannot be written it stays `in-progress`, so the next run imports again.
    let _ = stores.record.commit(in_progress, &record);
    match unrestored {
        Some(document) => MigrateError::RollbackFailed {
            cause: Box::new(cause),
            document,
        },
        None => cause,
    }
}

fn is_up_to_date(record: &MigrationRecord, parsed: &ParsedMigration) -> bool {
    record.status == MigrationStatus::Completed && same_sources(&record.sources, parsed.sources())
}

/// Whether two fingerprint lists hold the same fingerprints, in any order.
fn same_sources(recorded: &[SourceFingerprint], parsed: &[SourceFingerprint]) -> bool {
    sorted_keys(recorded) == sorted_keys(parsed)
}

/// The fields of each fingerprint, sorted.
fn sorted_keys(sources: &[SourceFingerprint]) -> Vec<(&str, &str, &str, u64)> {
    let mut keys: Vec<(&str, &str, &str, u64)> = sources
        .iter()
        .map(|source| {
            (
                source.kind.as_str(),
                source.location.as_str(),
                source.sha256.as_str(),
                source.size,
            )
        })
        .collect();
    keys.sort_unstable();
    keys
}

fn load<D: StoreDocument>(
    store: &AtomicStore<D>,
    document: DocumentKind,
) -> Result<D, MigrateError> {
    store
        .load_or_default()
        .map(|loaded| loaded.document)
        .map_err(|error| store_error(document, error))
}

fn store_error(document: DocumentKind, error: ConfigError) -> MigrateError {
    MigrateError::Store { document, error }
}

/// The bytes of a store file, at most the store's size limit.
fn read_back<D: StoreDocument>(path: &Path) -> Option<Vec<u8>> {
    let limit = AtomicStore::<D>::MAX_FILE_BYTES;
    let file = File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .ok()?;
    let size = u64::try_from(bytes.len()).ok()?;
    (size <= limit).then_some(bytes)
}

fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
