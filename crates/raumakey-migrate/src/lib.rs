//! Raumakey read-only EVKey settings and macro importer (plan §9, design §9).
//!
//! An import runs in four steps, each usable on its own:
//!
//! 1. [`discover`](fn@discover) reads the EVKey sources once, through the read-only ports [`LegacyRegistry`]
//!    and [`SourceFs`], and fingerprints them (registry `HKCU\Software\MyQ\EVKey` and
//!    `HKCU\Software\EVKey\TsfCache`; `setting.ini`, `setting.tree`, and `evkmacro.txt` in the
//!    first candidate folder that holds any of them).
//! 2. [`parse`](fn@parse) classifies every field without I/O: imported into `config.toml` through
//!    [`FIELD_MAP`] (which ships empty, plan D18), turned into an `evkey-import-<n>` named set
//!    when it lists executables ([`exe_list`]), preserved raw in the migration record, or
//!    quarantined with a reason code. Macros go through `raumakey-macro`'s legacy import.
//! 3. [`preview`] computes what a commit would change, without writing anything.
//! 4. [`commit`](fn@commit) merges into `config.toml`, `macros.toml`, and `app_rules.toml` through
//!    [`AtomicStore`](raumakey_config::AtomicStore), reads every written document back to verify
//!    its revision and SHA-256, and records the outcome in `migration.json`. Any failure after
//!    the first write restores the previous documents (see [`commit`](mod@commit)).
//!
//! # Guarantees
//!
//! - **Read-only sources** (design §9, §18): the ports have no method that writes, renames,
//!   deletes, unregisters, or changes permissions, and nothing else in the crate touches a
//!   source.
//! - **Field independence** (matrix #45): a corrupt, undecodable, or oversized field is
//!   quarantined on its own; every other field is still imported or preserved.
//! - **Precedence** (matrix #44): registry and `setting.ini` values are both preserved,
//!   separately; for a mapped setting the registry value wins.
//! - **Idempotence:** a completed import of the same source fingerprints is not repeated; a
//!   changed source is imported again, skipping what already exists.
//! - **Privacy:** errors, reports, and [`Debug`](core::fmt::Debug) output carry codes, counts,
//!   field names, and line numbers only, never setting values, macro text, or paths, and the
//!   crate never logs ([`MigrateError::diag_code`] feeds the typed diagnostics log). The
//!   migration record keeps raw setting values as evidence, never macro text.
//! - **Robustness:** no platform code, no `unsafe`, and no panic on any input; every size and
//!   count taken from a source is bounded.
//!
//! # Example
//!
//! ```
//! use std::sync::Arc;
//!
//! use raumakey_config::{AtomicStore, StdReplacer};
//! use raumakey_migrate::{
//!     FakeFs, FakeRegistry, MigrationOutcome, MigrationStores, commit, discover, parse, preview,
//! };
//!
//! let mut fs = FakeFs::new();
//! fs.add_file("EVKey/evkmacro.txt", "ko:không\n".as_bytes().to_vec());
//! let report = discover(&FakeRegistry::new(), &fs, ["EVKey"])?;
//! let parsed = parse(&report);
//!
//! let dir = tempfile::tempdir()?;
//! let replacer = Arc::new(StdReplacer);
//! let config = AtomicStore::new(dir.path(), replacer.clone());
//! let macros = AtomicStore::new(dir.path(), replacer.clone());
//! let app_rules = AtomicStore::new(dir.path(), replacer.clone());
//! let record = AtomicStore::new(dir.path(), replacer);
//! let stores = MigrationStores {
//!     config: &config,
//!     macros: &macros,
//!     app_rules: &app_rules,
//!     record: &record,
//! };
//!
//! assert_eq!(preview(&parsed, &stores.load_current()?).macro_count, 1);
//! let first = commit(&parsed, &stores, 1_700_000_000_000)?;
//! assert_eq!(first.imported.macros, 1);
//! let again = commit(&parsed, &stores, 1_700_000_000_001)?;
//! assert_eq!(again.outcome, MigrationOutcome::NoChange);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod commit;
pub mod discover;
pub mod error;
mod exe_list;
pub mod fake;
pub mod field_map;
pub mod ini;
pub mod parse;
pub mod ports;
mod registry;

pub use commit::{
    CurrentDocuments, MigrationOutcome, MigrationPreview, MigrationReport, MigrationStores, commit,
    preview,
};
pub use discover::{
    DiscoveredSource, DiscoveryReport, MACRO_TEXT_FILE, SETTINGS_INI_FILE, SETTINGS_REGISTRY_PATH,
    SETTINGS_TREE_FILE, SourceKind, TSF_CACHE_REGISTRY_PATH, discover,
};
pub use error::{DocumentKind, MigrateError};
pub use exe_list::exe_list;
pub use fake::{FakeFs, FakeRegistry};
pub use field_map::{FIELD_MAP, FieldMapping, InvalidSetting, SettingValue};
pub use ini::{
    IniDocument, IniEncoding, IniEntry, IniIssue, IniIssueKind, MAX_INI_ITEMS, parse_ini,
};
pub use parse::{
    Disposition, FieldRef, FieldReport, MAX_FIELD_BYTES, MAX_FIELDS_PER_SOURCE,
    MAX_PRESERVED_BYTES, MAX_RECORDED_QUARANTINE, NAMED_SET_PREFIX, ParsedMigration,
    QuarantineReason, parse, parse_with_field_map,
};
pub use ports::{
    LegacyRegistry, LegacyValue, MAX_SOURCE_BYTES, REG_BINARY, REG_DWORD, REG_EXPAND_SZ,
    REG_MULTI_SZ, REG_QWORD, REG_SZ, SourceFs, StdSourceFs,
};
