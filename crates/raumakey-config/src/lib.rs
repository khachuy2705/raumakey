//! Raumakey versioned configuration documents and atomic file storage.
//!
//! The data root ([`DataRoot`], decision D40) is the folder of `raumakey.exe` when it holds
//! `raumakey.toml` already, or when it can be written and the caller does not protect it;
//! otherwise `%LOCALAPPDATA%\Raumakey` (see [`data_root`]). It holds `raumakey.toml`,
//! `raumakey-macros.toml`, `raumakey-app-rules.toml`, `raumakey-migration.json`, the
//! `<file>.lock` file of each store, and `raumakey-logs/`: names that say they belong to
//! Raumakey, because the executable's folder usually holds the user's other files too. Each
//! document carries a schema `version` and a monotonically increasing `revision`.
//!
//! Loading is lenient per field (matrix #45): a bad field takes its default and is reported, a
//! bad list element is dropped and reported, and an array entry whose identity cannot be read
//! is set aside verbatim and reported on every load ([`FieldIssue`]).
//!
//! What a commit keeps of a file that a newer build wrote, or that was edited by hand:
//!
//! - **Kept:** unknown keys of every decoded table, including inside array entries
//!   ([`EntryExtras`]), and whole entries that were set aside. They are written back unchanged.
//! - **Not kept:** values this build does not accept in the fields it knows, such as a newer
//!   keyword for `startup`, `method`, `case`, `boundary`, or `transport`, or a value of
//!   another type. Such a field holds its default, is reported, and the next commit writes the
//!   default. List elements this build does not accept (hotkeys, key actions, excluded
//!   applications, named-set members) are not written back either, nor are key-map bindings
//!   dropped for a bad `key` or `actions`, together with their unknown keys.
//!
//! A caller should therefore show the load issues before it commits over them. Writing is
//! atomic, with optimistic revision checks (matrix #46) that also catch a writer that changed
//! the file without a new revision ([`AtomicStore`]).
//!
//! This crate depends only on `raumakey-model`. It never logs or returns user content: errors
//! and load issues carry codes and schema paths only.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod app_rules;
pub mod config;
pub mod data_root;
pub mod diagnostics;
pub mod document;
mod entries;
pub mod error;
mod fields;
pub mod macros;
pub mod migration;
pub mod store;

pub use app_rules::{ACTION_KEY, AppRulesDocument, RuleIdentity};
pub use config::{
    BINDINGS_KEY, CaseTransform, ConfigDocument, ConversionSection, ExclusionsSection, HOTKEYS_KEY,
    InputSection, MacroSection, StartupMode, SystemSection, normalize_excluded_app,
};
pub use data_root::{DataRoot, WRITE_PROBE_FILE, is_writable_dir};
pub use diagnostics::DiagnosticsLog;
pub use document::{FieldIssue, IssueReason, Loaded, StoreDocument};
pub use entries::EntryExtras;
pub use error::ConfigError;
pub use macros::{MACRO_ARRAY_KEY, MacroDocument, MacroIdentity};
pub use migration::{
    CommitResult, ImportedCounts, MigrationRecord, MigrationStatus, QuarantinedField,
    SourceFingerprint,
};
pub use store::{AtomicStore, FileReplacer, StdReplacer, sha256_hex};
