//! The model of the macro list editor (D44, design §22.2): the rows of `raumakey-macros.toml`
//! that the "Danh sách gõ tắt" dialog shows and edits, their checks, the save, and the
//! dialog's Vietnamese texts. The dialog owns one [`MacroList`] on its own thread and adds
//! only the Win32 calls. The list reads and writes the file through an
//! [`AtomicStore`] of its own (atomic, locked, revision-checked commits), so
//! [`Settings`](crate::settings::Settings) still never writes it.
//!
//! - **Loading.** [`MacroList::load`] reads `raumakey-macros.toml` from the data folder. A
//!   missing file is an empty list at revision 0, and the first save creates it. Any other
//!   failure ([`ConfigError::Corrupt`], [`ConfigError::UnsupportedVersion`],
//!   [`ConfigError::PermissionDenied`], [`ConfigError::IoFailure`]) is a [`LoadError`]: the
//!   editor does not open ([`load_error_text`]), and nothing is written over a damaged file.
//!   Values that the store could read only in part are [`MacroList::lossy_issues`] (see
//!   below): the dialog asks [`open_loss_question`] before it opens.
//! - **Rows.** One [`Row`] per decoded entry, in file order, with the trigger and the
//!   replacement as written, including entries that are invalid now (a trigger of 40 scalars
//!   or with a space written by hand, a repeated id): they are shown, and written back
//!   untouched unless the user edits or removes them. [`Row::status`] says whether the typing
//!   hook uses a row once the list is saved ([`RowStatus`], the third column). Entries the
//!   store set aside because they could not be decoded are not rows; they are kept (see
//!   below).
//! - **Edits.** [`MacroList::put`] checks the trigger and the replacement with
//!   [`normalize_trigger`] and [`normalize_replacement`] (D29, D30: 1 to 32 scalars after NFC
//!   and no whitespace or control character in a trigger; a non-empty replacement without NUL
//!   that fits one engine insert) and stores their NFC forms. A row whose trigger has the same
//!   NFC form takes the replacement ([`Put::Updated`]; its trigger becomes the NFC form): the
//!   one the hook uses for that trigger ([`RowStatus::Active`]), else the first. Otherwise a
//!   row is appended ([`Put::Added`]), up to [`MAX_MACROS`] rows. [`MacroList::remove`]
//!   removes a row.
//! - **Saving.** [`MacroList::save`] writes every row in one commit based on the revision the
//!   list was loaded at, and only when a row changed ([`Saved::Unchanged`] otherwise). When
//!   the file changed since then (another program, a hand edit even without a new revision,
//!   another editor), the commit fails with [`SaveError::Conflict`] and the rows stay; the
//!   dialog asks ([`conflict_question`]), and "yes" calls [`MacroList::overwrite`]. A
//!   document larger than the store takes (16 MiB) is [`SaveError::TooLarge`], before the
//!   commit.
//!
//! After a successful save or overwrite, the written document is the new baseline: its entries
//! are the rows, [`MacroList::is_modified`] is `false`, and [`MacroList::revision`] is the new
//! revision. A failed one leaves the rows and the baseline as they were. The typing hook gets
//! the new macros through [`Settings::poll`](crate::settings::Settings::poll), which notices
//! the change like a hand edit; the tray asks the worker to poll at once after a save.
//!
//! # What a save keeps
//!
//! The document written by [`MacroList::save`] is the loaded one with the rows as its entries,
//! in row order:
//!
//! - its `version`, its unknown top-level keys, and the entries set aside (matrix #45) are
//!   written back unchanged;
//! - a row from the file keeps its entry's id, `enabled`, `boundary`, and `case`, and its
//!   unknown keys ([`EntryExtras`]), also when an edit recomposed its trigger to NFC. They
//!   follow the row itself, not its identity (id and trigger), so entries that share an
//!   identity keep their own, and a new row never takes those of a removed entry;
//! - an edited row whose id an earlier row has gets a new id (with its switches and unknown
//!   keys): `MacroStore::from_entries_lenient` skips a repeated id, so the edit would never
//!   take effect. A row that was not edited keeps its id, repeated or not;
//! - a new row is enabled, with the default `boundary` and `case`, and gets a new id the way
//!   `MacroStore::next_id` assigns one: one above the largest id still present (`1` when there
//!   is none), so the id of a removed entry with the largest id is free again. Present are the
//!   ids of the rows from the file, valid or not (`MacroStore` counts the entries it skips as
//!   invalid as well), and the ids that the entries set aside carry, which a fix by hand
//!   brings back. New ids are given in row order.
//!   The ids end at `i64::MAX`, the largest integer of TOML: beyond it, [`RowError::NoIdLeft`]
//!   or [`SaveError::NoIdLeft`].
//!
//! # Values the store could not read
//!
//! The store reads a file leniently (matrix #45), and a write of what it read loses what it
//! could not: a `macro` that is not an array of tables, an element of that array that is not a
//! table (both dropped), and an `enabled`, `boundary`, or `case` that cannot be read (reset to
//! its default). [`MacroList::lossy_issues`] lists them by schema path, never with values;
//! entries set aside because their id, trigger, or replacement cannot be read are written back
//! verbatim and are not listed. While the list is not empty, [`MacroList::save`] and
//! [`MacroList::overwrite`] refuse with [`SaveError::Lossy`] until the user accepts the loss
//! ([`MacroList::accept_loss`]): when the list opens ([`open_loss_question`]), or when an
//! overwrite finds such values in the current file ([`overwrite_loss_question`]). A file is
//! thus never written over silently with less than it holds.
//!
//! # Writing over a changed file
//!
//! [`MacroList::overwrite`] loads the file as it is now (a missing file is empty, at revision
//! 0) and commits the rows over it, against its revision:
//!
//! - The rows replace its entries. Its `version`, unknown top-level keys, and entries set aside
//!   are kept; those of the file the list was loaded from are not (the other writer removed
//!   them).
//! - A row from the loaded file is matched, by the identity it was loaded with (in order, for
//!   repeated identities), with an entry of the current file. A matched row takes that entry's
//!   switches and unknown keys (the latest state of what the editor does not show); an
//!   unmatched one, whose entry the other writer removed, keeps those it was loaded with.
//! - The ids of rows from the loaded file are kept by the rules of a save. Nothing can collide
//!   with them: the current file's entries are all replaced, so their ids leave with them; its
//!   entries set aside are not macros, and new ids lie above the ids they carry; unknown keys
//!   follow only a row with the same id and trigger. Among themselves the rows keep the ids of
//!   the loaded file, since the editor never makes a repeated id. (An entry set aside that
//!   carries the id of a row becomes a repeated id once it is fixed by hand, which
//!   `MacroStore::from_entries_lenient` skips and reports, as for any hand edit.)
//! - A file that cannot be read now (it became corrupt, ...) is the error, and nothing is
//!   written. A file with values the overwrite would lose is [`SaveError::Lossy`] unless the
//!   user accepted losing exactly those. When the commit itself fails, the list remembers the
//!   file it read: the next [`MacroList::save`] writes over that file (the one the user agreed
//!   to write over), and is a conflict again when the file changed once more.
//!
//! # Texts
//!
//! [`row_error_text`], [`load_error_text`], [`save_error_text`], [`conflict_question`],
//! [`open_loss_question`], [`overwrite_loss_question`], [`DISCARD_QUESTION`], and
//! [`RowStatus::label`] are new Raumakey strings in Vietnamese, built from paths, schema
//! paths, limits, and error kinds, never from macro text.
//!
//! Nothing here logs. The [`Debug`](fmt::Debug) output of [`MacroList`] and [`Row`] shows the
//! path, the revision, counts, and the modified flag, never macro text.

use core::fmt;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use raumakey_config::{
    AtomicStore, ConfigError, EntryExtras, FieldIssue, FileReplacer, MacroDocument, MacroIdentity,
    StoreDocument,
};
use raumakey_ipc::message::MAX_MACROS;
use raumakey_macro::{
    EntryError, MAX_REPLACEMENT_SCALARS, MAX_TRIGGER_SCALARS, normalize_replacement,
    normalize_trigger,
};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};

use crate::settings::replacer;
use crate::worker::Saved;

/// How long a save waits for the store lock, as the settings worker does.
const LOCK_TIMEOUT: Duration = Duration::from_secs(2);

/// The largest id `raumakey-macros.toml` can hold: TOML integers are signed 64-bit.
const MAX_FILE_ID: u64 = i64::MAX.unsigned_abs();

/// One row of the list: a trigger and its replacement, as the dialog shows them (as written in
/// the file, or in NFC after an edit). Its `Debug` output shows no text.
#[derive(Clone, PartialEq, Eq)]
pub struct Row {
    trigger: String,
    replacement: String,
    /// The NFC form of the trigger when it is valid (D29): what [`MacroList::put`] matches.
    key: Option<String>,
    /// Whether the replacement is valid (D30).
    replacement_ok: bool,
    /// The entry of the loaded document the row came from; `None` for a row added since.
    origin: Option<usize>,
    /// Whether [`MacroList::put`] changed the row since the load.
    edited: bool,
    status: RowStatus,
}

/// Whether the typing hook uses a row once the list is saved, as `Settings` builds its macro
/// table (`MacroStore::from_entries_lenient`, then `MacroTable`), for the third column of the
/// dialog ([`RowStatus::label`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowStatus {
    /// The hook expands the trigger to this row's replacement.
    Active,
    /// The row is switched off (`enabled = false`, set in the file).
    Disabled,
    /// The hook skips the row: its trigger or replacement is invalid now (D29, D30), an earlier
    /// row has its id, or another row with the same trigger (after NFC) and a lower id is used
    /// instead.
    Unused,
}

impl RowStatus {
    /// The text of the status column: nothing for a row in use.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Active => "",
            Self::Disabled => "đang tắt",
            Self::Unused => "không dùng",
        }
    }
}

impl Row {
    fn loaded(index: usize, entry: &MacroEntry) -> Self {
        Self {
            trigger: entry.trigger.clone(),
            replacement: entry.replacement.clone(),
            key: normalize_trigger(&entry.trigger).ok(),
            replacement_ok: normalize_replacement(&entry.replacement).is_ok(),
            origin: Some(index),
            edited: false,
            status: RowStatus::Active,
        }
    }

    /// Whether the typing hook uses the row once the list is saved.
    pub fn status(&self) -> RowStatus {
        self.status
    }

    /// The trigger ("Gõ tắt").
    pub fn trigger(&self) -> &str {
        &self.trigger
    }

    /// The replacement ("Thay bằng").
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

impl fmt::Debug for Row {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Row")
            .field("origin", &self.origin)
            .finish_non_exhaustive()
    }
}

/// What [`MacroList::put`] did, with the index of the row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Put {
    /// A new row was appended.
    Added(usize),
    /// The row with the same trigger after NFC (the one the hook uses, else the first) got the
    /// replacement.
    Updated(usize),
}

/// Why [`MacroList::put`] refused a row; nothing was changed ([`row_error_text`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RowError {
    /// The trigger or the replacement breaks a rule of `raumakey-macro` (D29, D30).
    Invalid(EntryError),
    /// The list holds [`MAX_MACROS`] rows already, the most one macro snapshot carries.
    TooMany,
    /// No id is left for a new row between the largest one in use and `i64::MAX`.
    NoIdLeft,
}

/// Why [`MacroList::load`] did not open the list: the path of `raumakey-macros.toml` and
/// [`ConfigError::Corrupt`], [`ConfigError::UnsupportedVersion`],
/// [`ConfigError::PermissionDenied`], or [`ConfigError::IoFailure`] ([`load_error_text`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    path: PathBuf,
    error: ConfigError,
}

impl LoadError {
    /// The path of `raumakey-macros.toml`.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Why it could not be read.
    pub fn error(&self) -> ConfigError {
        self.error
    }
}

/// Why [`MacroList::save`] or [`MacroList::overwrite`] wrote nothing; the rows stay
/// ([`save_error_text`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveError {
    /// The file changed since it was loaded: the dialog asks [`conflict_question`].
    Conflict,
    /// The file the write is based on holds values that the write would drop or reset
    /// ([`MacroList::lossy_issues`]), and the user has not accepted that yet
    /// ([`MacroList::accept_loss`]).
    Lossy,
    /// No id is left for the new rows below `i64::MAX`.
    NoIdLeft,
    /// The file would be larger than the store's limit
    /// ([`AtomicStore::MAX_FILE_BYTES`], 16 MiB): rows must be removed or shortened.
    TooLarge,
    /// The store could not read or write the file (never [`ConfigError::Conflict`]).
    Store(ConfigError),
}

impl From<ConfigError> for SaveError {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::Conflict { .. } => Self::Conflict,
            other => Self::Store(other),
        }
    }
}

/// The rows of `raumakey-macros.toml` that the editor works on (see the [module
/// documentation](self)).
///
/// `Send`: the dialog loads, edits, and saves it on its own thread. [`MacroList::load`],
/// [`MacroList::save`], and [`MacroList::overwrite`] do file I/O and may wait up to two
/// seconds for the store lock; the other methods do none.
pub struct MacroList {
    store: AtomicStore<MacroDocument>,
    /// The document as loaded or last saved.
    base: MacroDocument,
    /// For each entry of `base`, its record of unknown keys ([`records_of`]).
    base_records: Vec<Option<usize>>,
    /// The file as [`MacroList::overwrite`] last read it, while the write over it has not
    /// succeeded: the user agreed to write over this content, and the store checks the next
    /// commit against it.
    target: Option<MacroDocument>,
    /// What the store could not take from the file that the next write is based on (`base`,
    /// or `target`), and that the write would therefore drop or reset ([`is_lossy`]).
    lossy: Vec<FieldIssue>,
    /// Whether the user accepted losing `lossy`.
    loss_accepted: bool,
    rows: Vec<Row>,
    modified: bool,
}

impl fmt::Debug for MacroList {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroList")
            .field("path", &self.store.path())
            .field("revision", &self.base.revision)
            .field("rows", &self.rows.len())
            .field("modified", &self.modified)
            .finish_non_exhaustive()
    }
}

impl MacroList {
    /// Loads `raumakey-macros.toml` from the data folder `dir` (the folder of `raumakey.toml`,
    /// [`Settings::config_path`](crate::settings::Settings::config_path)). A missing file is
    /// an empty list at revision 0; any other failure is a [`LoadError`], and nothing is
    /// written.
    pub fn load(dir: &Path) -> Result<Self, LoadError> {
        Self::load_with(dir, replacer(), LOCK_TIMEOUT)
    }

    /// [`MacroList::load`] with the store's file replacer and lock timeout.
    fn load_with(
        dir: &Path,
        replacer: Arc<dyn FileReplacer>,
        lock_timeout: Duration,
    ) -> Result<Self, LoadError> {
        let store = AtomicStore::new(dir, replacer).with_lock_timeout(lock_timeout);
        let loaded = store.load_or_default().map_err(|error| LoadError {
            path: store.path().to_path_buf(),
            error,
        })?;
        let mut list = Self {
            store,
            base: MacroDocument::default(),
            base_records: Vec::new(),
            target: None,
            lossy: Vec::new(),
            loss_accepted: false,
            rows: Vec::new(),
            modified: false,
        };
        list.adopt(loaded.document);
        list.lossy = lossy_issues(loaded.issues);
        Ok(list)
    }

    /// The path of `raumakey-macros.toml`.
    pub fn path(&self) -> &Path {
        self.store.path()
    }

    /// The revision of the file as loaded or last saved; 0 when it did not exist.
    pub fn revision(&self) -> u64 {
        self.base.revision
    }

    /// The rows, in order: the entries of the file, then the rows added since, as edited.
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    /// The number of rows.
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether there is no row.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Whether a row was added, changed, or removed since the list was loaded or last saved
    /// (the dialog asks [`DISCARD_QUESTION`] before it closes then).
    pub fn is_modified(&self) -> bool {
        self.modified
    }

    /// Sets `replacement` for `trigger` ("Thêm / Sửa").
    ///
    /// Both are checked and stored in NFC (D29, D30); a trigger error is reported before a
    /// replacement error. A row whose trigger has the same NFC form takes the replacement and
    /// the NFC trigger ([`Put::Updated`]; nothing changes when it holds them already): the row
    /// the hook uses for that trigger ([`RowStatus::Active`]), so that the edit takes effect,
    /// else the first such row. Otherwise the row is appended ([`Put::Added`]). Triggers that
    /// differ in case are different rows. Fails, with nothing changed, on an invalid row
    /// ([`RowError::Invalid`]), and for a new row when the list holds [`MAX_MACROS`] rows
    /// ([`RowError::TooMany`]) or no id is left for it ([`RowError::NoIdLeft`]).
    pub fn put(&mut self, trigger: &str, replacement: &str) -> Result<Put, RowError> {
        let trigger = normalize_trigger(trigger).map_err(RowError::Invalid)?;
        let replacement = normalize_replacement(replacement).map_err(RowError::Invalid)?;
        // The row the hook uses for the trigger, else the first with it.
        let with_trigger = |row: &Row| row.key.as_deref() == Some(trigger.as_str());
        let same = self
            .rows
            .iter()
            .position(|row| with_trigger(row) && row.status == RowStatus::Active)
            .or_else(|| self.rows.iter().position(with_trigger));
        if let Some((index, row)) = same.and_then(|index| Some((index, self.rows.get_mut(index)?)))
        {
            if row.trigger != trigger || row.replacement != replacement {
                row.trigger = trigger;
                row.replacement = replacement;
                row.replacement_ok = true;
                row.edited = true;
                self.modified = true;
                self.refresh();
            }
            return Ok(Put::Updated(index));
        }
        if self.rows.len() >= MAX_MACROS {
            return Err(RowError::TooMany);
        }
        let new_ids = self.new_ids().saturating_add(1);
        if first_new_id(self.present_max(&self.base), new_ids).is_none() {
            return Err(RowError::NoIdLeft);
        }
        let index = self.rows.len();
        self.rows.push(Row {
            key: Some(trigger.clone()),
            trigger,
            replacement,
            replacement_ok: true,
            origin: None,
            edited: true,
            status: RowStatus::Active,
        });
        self.modified = true;
        self.refresh();
        Ok(Put::Added(index))
    }

    /// Removes the row at `index` ("Xoá"); `false`, with nothing changed, when there is none.
    /// The rows after it move up by one.
    pub fn remove(&mut self, index: usize) -> bool {
        if index >= self.rows.len() {
            return false;
        }
        self.rows.remove(index);
        self.modified = true;
        self.refresh();
        true
    }

    /// Writes the rows to the file in one commit ("Lưu"; see the [module
    /// documentation](self) for what it keeps): [`Saved::Written`], or [`Saved::Unchanged`]
    /// when no row changed and nothing was written. Errors, with the rows kept:
    ///
    /// - [`SaveError::Lossy`]: the write would lose [`MacroList::lossy_issues`], which the user
    ///   has not accepted ([`MacroList::accept_loss`]); checked first.
    /// - [`SaveError::Conflict`]: the file changed since it was loaded (or since the file that
    ///   a failed [`MacroList::overwrite`] read); ask [`conflict_question`] and call
    ///   [`MacroList::overwrite`] on "yes".
    /// - [`SaveError::TooLarge`]: the encoded document exceeds the store's limit (16 MiB).
    /// - [`SaveError::Store`]: [`ConfigError::Corrupt`] or [`ConfigError::UnsupportedVersion`]
    ///   when the file on disk became damaged or of a newer version (it is never written
    ///   over), [`ConfigError::PermissionDenied`] or [`ConfigError::IoFailure`] when it could
    ///   not be written (or the lock was not free within two seconds).
    /// - [`SaveError::NoIdLeft`]: no id is left for the new rows.
    pub fn save(&mut self) -> Result<Saved, SaveError> {
        if !self.modified && self.target.is_none() {
            return Ok(Saved::Unchanged);
        }
        self.write()
    }

    /// Writes the rows over the file as it is now, after the user answered "yes" to
    /// [`conflict_question`] (see the [module documentation](self) for what it keeps). The
    /// errors are those of [`MacroList::save`], and a file that cannot be read now is
    /// [`SaveError::Store`] with nothing written; [`SaveError::Conflict`] means the file
    /// changed again meanwhile. [`SaveError::Lossy`] means the file as it is now holds values
    /// the overwrite would lose, other than those the user accepted: ask
    /// [`overwrite_loss_question`] with [`MacroList::lossy_issues`], and on "yes" call
    /// [`MacroList::accept_loss`] and this again.
    pub fn overwrite(&mut self) -> Result<(), SaveError> {
        let current = self.store.load_or_default()?;
        let lossy = lossy_issues(current.issues);
        if lossy != self.lossy {
            // What the user accepted to lose is not what this file would lose.
            self.lossy = lossy;
            self.loss_accepted = false;
        }
        self.target = Some(current.document);
        self.write().map(|_| ())
    }

    /// What the store could not take from the file that the next write is based on, and that
    /// the write would therefore drop (a `macro` that is not an array of tables, an element of
    /// it that is not a table) or reset to its default (an `enabled`, `boundary`, or `case`
    /// that cannot be read): by schema path and reason, never with values. Empty when nothing
    /// would be lost; entries set aside because their id, trigger, or replacement cannot be
    /// read are written back verbatim and are not listed.
    ///
    /// The list is that of the loaded file, and after [`MacroList::overwrite`] that of the file
    /// it read. While it is not empty, [`MacroList::save`] and [`MacroList::overwrite`] refuse
    /// with [`SaveError::Lossy`] until [`MacroList::accept_loss`]: the dialog asks
    /// [`open_loss_question`] when the list opens (and [`overwrite_loss_question`] after an
    /// overwrite refused so).
    pub fn lossy_issues(&self) -> &[FieldIssue] {
        &self.lossy
    }

    /// Accepts losing [`MacroList::lossy_issues`]: the next write drops or resets them. An
    /// overwrite that finds other values it would lose asks again.
    pub fn accept_loss(&mut self) {
        self.loss_accepted = true;
    }

    /// Commits the rows over the baseline, or over `target` when the user chose to write over
    /// the file as [`MacroList::overwrite`] last read it.
    fn write(&mut self) -> Result<Saved, SaveError> {
        if !self.lossy.is_empty() && !self.loss_accepted {
            return Err(SaveError::Lossy);
        }
        let (mut document, expected) = match &self.target {
            None => {
                let own: Vec<Option<usize>> = self.rows.iter().map(|row| row.origin).collect();
                let document = self.document_over(&self.base, &self.base_records, &own)?;
                (document, self.base.revision)
            }
            Some(file) => {
                let records = records_of(file);
                let matches = self.matches_in(file);
                let document = self.document_over(file, &records, &matches)?;
                (document, file.revision)
            }
        };
        // The store refuses a larger file as an I/O failure, which retrying never mends.
        document.revision = expected.saturating_add(1);
        let size = document.encode()?.len();
        if u64::try_from(size).map_or(true, |size| {
            size > AtomicStore::<MacroDocument>::MAX_FILE_BYTES
        }) {
            return Err(SaveError::TooLarge);
        }
        document.revision = self.store.commit(expected, &document)?;
        self.adopt(document);
        Ok(Saved::Written)
    }

    /// Takes `document` as the baseline: its entries become the rows.
    fn adopt(&mut self, document: MacroDocument) {
        self.rows = document
            .entries
            .iter()
            .enumerate()
            .map(|(index, entry)| Row::loaded(index, entry))
            .collect();
        self.base_records = records_of(&document);
        self.base = document;
        self.target = None;
        // A written document holds nothing the store cannot read back.
        self.lossy.clear();
        self.loss_accepted = false;
        self.modified = false;
        self.refresh();
    }

    /// The number of rows that get a new id ([`MacroList::written_ids`]).
    fn new_ids(&self) -> usize {
        self.written_ids().iter().filter(|id| id.is_none()).count()
    }

    /// For each row, the id it is written with: its own, or `None` when it gets a new one: a
    /// row added since the load, and an edited row whose id an earlier row has (the hook would
    /// skip it as a repeated id, so the edit would never take effect). A row that was not
    /// edited keeps its id, repeated or not.
    fn written_ids(&self) -> Vec<Option<MacroId>> {
        let mut earlier = BTreeSet::new();
        self.rows
            .iter()
            .map(|row| {
                let id = self.origin(row)?.id;
                let repeated = !earlier.insert(id);
                (!(repeated && row.edited)).then_some(id)
            })
            .collect()
    }

    /// Computes [`Row::status`] of every row as `Settings` builds the hook's macro table from
    /// the saved file: `MacroStore::from_entries_lenient` keeps the valid rows whose id no
    /// earlier valid row has, and `MacroTable` indexes the enabled ones, the lowest id winning
    /// for a trigger. New ids lie above every kept one, in row order.
    fn refresh(&mut self) {
        /// The order of the ids rows are written with: kept ids, then new ones in row order.
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
        enum Written {
            Kept(u64),
            New(usize),
        }
        let ids: Vec<Written> = self
            .written_ids()
            .into_iter()
            .enumerate()
            .map(|(index, id)| id.map_or(Written::New(index), |id| Written::Kept(id.0)))
            .collect();
        let enabled: Vec<bool> = self
            .rows
            .iter()
            .map(|row| self.origin(row).is_none_or(|entry| entry.enabled))
            .collect();
        let mut taken = BTreeSet::new();
        let mut used: BTreeMap<&str, (Written, usize)> = BTreeMap::new();
        let mut statuses = vec![RowStatus::Unused; self.rows.len()];
        for (index, row) in self.rows.iter().enumerate() {
            let (Some(key), Some(&id)) = (row.key.as_deref(), ids.get(index)) else {
                continue;
            };
            if !row.replacement_ok || !taken.insert(id) {
                continue;
            }
            if !enabled.get(index).copied().unwrap_or(true) {
                if let Some(status) = statuses.get_mut(index) {
                    *status = RowStatus::Disabled;
                }
                continue;
            }
            let lowest = used.entry(key).or_insert((id, index));
            if id < lowest.0 {
                *lowest = (id, index);
            }
        }
        for &(_, index) in used.values() {
            if let Some(status) = statuses.get_mut(index) {
                *status = RowStatus::Active;
            }
        }
        for (row, status) in self.rows.iter_mut().zip(statuses) {
            row.status = status;
        }
    }

    /// The entry of the baseline that `row` came from.
    fn origin(&self, row: &Row) -> Option<&MacroEntry> {
        row.origin.and_then(|index| self.base.entries.get(index))
    }

    /// The largest id still present when the rows are written over `file`: that of a row from
    /// the loaded file, or of an entry `file` set aside.
    fn present_max(&self, file: &MacroDocument) -> Option<u64> {
        let rows = self
            .rows
            .iter()
            .filter_map(|row| self.origin(row))
            .map(|entry| entry.id.0);
        let set_aside = file
            .entry_extras
            .rejected
            .iter()
            .filter_map(|table| table.get("id")?.as_integer())
            .filter_map(|id| u64::try_from(id).ok());
        rows.chain(set_aside).max()
    }

    /// For each row, the entry of `file` that holds the same macro: the first entry with the
    /// identity (id and trigger) that the row was loaded with that no earlier row took; `None`
    /// for a new row and for one whose entry `file` does not hold.
    fn matches_in(&self, file: &MacroDocument) -> Vec<Option<usize>> {
        let mut queues: BTreeMap<(MacroId, &str), VecDeque<usize>> = BTreeMap::new();
        for (index, entry) in file.entries.iter().enumerate() {
            queues
                .entry((entry.id, entry.trigger.as_str()))
                .or_default()
                .push_back(index);
        }
        self.rows
            .iter()
            .map(|row| {
                let entry = self.origin(row)?;
                queues
                    .get_mut(&(entry.id, entry.trigger.as_str()))?
                    .pop_front()
            })
            .collect()
    }

    /// The document that writes the rows over `file`, whose records of unknown keys are
    /// `file_records` ([`records_of`]); `matches` names, for each row, the entry of `file`
    /// that holds its macro.
    fn document_over(
        &self,
        file: &MacroDocument,
        file_records: &[Option<usize>],
        matches: &[Option<usize>],
    ) -> Result<MacroDocument, SaveError> {
        let written = self.written_ids();
        let new_ids = written.iter().filter(|id| id.is_none()).count();
        let mut next = if new_ids == 0 {
            0
        } else {
            first_new_id(self.present_max(file), new_ids).ok_or(SaveError::NoIdLeft)?
        };
        let mut fresh = || {
            let id = MacroId(next);
            next = next.saturating_add(1);
            id
        };
        let mut entries = Vec::with_capacity(self.rows.len());
        let mut tables = Vec::with_capacity(self.rows.len());
        for ((row, matched), written) in self.rows.iter().zip(matches).zip(written) {
            let loaded = row
                .origin
                .and_then(|index| Some((index, self.base.entries.get(index)?)));
            let Some((origin, own)) = loaded else {
                entries.push(MacroEntry {
                    id: fresh(),
                    trigger: row.trigger.clone(),
                    replacement: row.replacement.clone(),
                    enabled: true,
                    boundary: BoundaryPolicy::default(),
                    case_policy: CasePolicy::default(),
                });
                tables.push(None);
                continue;
            };
            // The switches and the unknown keys come from the entry of `file` that holds the
            // macro, else from the entry the row was loaded with.
            let (document, records, index) = match *matched {
                Some(index) => (file, file_records, index),
                None => (&self.base, self.base_records.as_slice(), origin),
            };
            let kept = document.entries.get(index).unwrap_or(own);
            let table = records
                .get(index)
                .copied()
                .flatten()
                .and_then(|record| document.entry_extras.unknown_keys.get(record))
                .map(|(_, table)| table);
            entries.push(MacroEntry {
                id: written.unwrap_or_else(&mut fresh),
                trigger: row.trigger.clone(),
                replacement: row.replacement.clone(),
                enabled: kept.enabled,
                boundary: kept.boundary,
                case_policy: kept.case_policy,
            });
            tables.push(table);
        }
        let unknown_keys = records_to_write(&entries, &tables, |table| table.is_empty());
        Ok(MacroDocument {
            version: file.version,
            revision: file.revision,
            entries,
            entry_extras: EntryExtras {
                unknown_keys,
                rejected: file.entry_extras.rejected.clone(),
            },
            extensions: file.extensions.clone(),
        })
    }
}

/// The issues of a load whose values a write of the loaded document would lose ([`is_lossy`]).
fn lossy_issues(issues: Vec<FieldIssue>) -> Vec<FieldIssue> {
    issues.into_iter().filter(is_lossy).collect()
}

/// Whether a write of the loaded document loses what `issue` reports. Everything does but a
/// missing `version` (the current one is written), a `revision` that cannot be read (the
/// commit writes a new one), and the identity fields of an entry set aside
/// (`macro[<n>].id`, `.trigger`, `.replacement`), which the store writes back verbatim. So a
/// `macro` that is not an array (dropped), an element of it that is not a table (`macro[<n>]`,
/// dropped), and `macro[<n>].enabled`, `.boundary`, or `.case` (reset to the default) are
/// lossy, and so is any issue this list does not know.
fn is_lossy(issue: &FieldIssue) -> bool {
    if issue.path == "version" || issue.path == "revision" {
        return false;
    }
    let field = issue
        .path
        .strip_prefix("macro[")
        .and_then(|rest| rest.split_once(']'))
        .filter(|(index, _)| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
        .map(|(_, field)| field);
    !matches!(field, Some(".id" | ".trigger" | ".replacement"))
}

/// For each entry of `document`, the index of its record in `entry_extras.unknown_keys`: the
/// first record of its identity that no earlier entry took, as the store's encoder pairs them.
fn records_of(document: &MacroDocument) -> Vec<Option<usize>> {
    let mut queues: BTreeMap<(MacroId, &str), VecDeque<usize>> = BTreeMap::new();
    for (index, ((id, trigger), _)) in document.entry_extras.unknown_keys.iter().enumerate() {
        queues
            .entry((*id, trigger.as_str()))
            .or_default()
            .push_back(index);
    }
    document
        .entries
        .iter()
        .map(|entry| {
            queues
                .get_mut(&(entry.id, entry.trigger.as_str()))
                .and_then(VecDeque::pop_front)
        })
        .collect()
}

/// The unknown-key records to write with `entries`, whose unknown keys are `tables` (one per
/// entry), as a load of the written file reads them back (matrix #45): one for each entry
/// with unknown keys, and one for every entry whose identity repeats (empty when it has none),
/// so that the encoder, which pairs records in order, gives each entry its own.
fn records_to_write<T: Clone + Default>(
    entries: &[MacroEntry],
    tables: &[Option<&T>],
    is_empty: impl Fn(&T) -> bool,
) -> Vec<(MacroIdentity, T)> {
    let mut counts: BTreeMap<(MacroId, &str), usize> = BTreeMap::new();
    for entry in entries {
        let count = counts
            .entry((entry.id, entry.trigger.as_str()))
            .or_default();
        *count = count.saturating_add(1);
    }
    entries
        .iter()
        .zip(tables)
        .filter_map(|(entry, table)| {
            let repeats = counts
                .get(&(entry.id, entry.trigger.as_str()))
                .is_some_and(|&count| count > 1);
            let table = match table {
                Some(table) if repeats || !is_empty(table) => (*table).clone(),
                None if repeats => T::default(),
                _ => return None,
            };
            Some(((entry.id, entry.trigger.clone()), table))
        })
        .collect()
}

/// The first of `count` (at least one) consecutive new ids above `present_max`, the
/// largest id still present (`1` when there is none), as `MacroStore::next_id` assigns them;
/// `None` when the last would lie above [`MAX_FILE_ID`].
fn first_new_id(present_max: Option<u64>, count: usize) -> Option<u64> {
    let first = match present_max {
        None => 1,
        Some(max) => max.checked_add(1)?,
    };
    let last = first.checked_add(u64::try_from(count.saturating_sub(1)).ok()?)?;
    (last <= MAX_FILE_ID).then_some(first)
}

/// The question before the dialog closes with rows that were not saved.
pub const DISCARD_QUESTION: &str = "Danh sách gõ tắt có thay đổi chưa được lưu. Đóng danh sách và \
bỏ các thay đổi này không?";

/// `20.000`: `number` with its digits grouped by dots, as Vietnamese writes numbers.
pub(crate) fn grouped(number: usize) -> String {
    let digits = number.to_string();
    let mut text = String::with_capacity(digits.len().saturating_mul(4) / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            text.push('.');
        }
        text.push(digit);
    }
    text
}

/// Why a store operation failed, as a clause.
fn reason(error: ConfigError) -> &'static str {
    match error {
        ConfigError::NotFound => "không tìm thấy tệp hoặc thư mục chứa nó",
        ConfigError::Corrupt => "tệp bị hỏng hoặc sai cú pháp TOML",
        ConfigError::UnsupportedVersion => "tệp thuộc một phiên bản Raumakey mới hơn",
        ConfigError::PermissionDenied => "Windows từ chối quyền truy cập",
        ConfigError::Conflict { .. } => "tệp vừa bị thay đổi ở nơi khác",
        ConfigError::IoFailure => "lỗi đọc hoặc ghi tệp, có thể tệp đang bị chương trình khác mở",
    }
}

/// What the dialog says when [`MacroList::put`] refuses a row.
pub fn row_error_text(error: RowError) -> String {
    match error {
        RowError::Invalid(EntryError::EmptyTrigger) => "Hãy nhập từ gõ tắt.".to_owned(),
        RowError::Invalid(EntryError::TriggerTooLong) => {
            format!("Từ gõ tắt chỉ được dài tối đa {MAX_TRIGGER_SCALARS} ký tự.")
        }
        RowError::Invalid(EntryError::TriggerWhitespace) => {
            "Từ gõ tắt không được có khoảng trắng.".to_owned()
        }
        RowError::Invalid(EntryError::TriggerControl) => {
            "Từ gõ tắt không được có ký tự điều khiển.".to_owned()
        }
        RowError::Invalid(EntryError::EmptyReplacement) => "Hãy nhập nội dung thay thế.".to_owned(),
        RowError::Invalid(EntryError::ReplacementTooLong) => format!(
            "Nội dung thay thế quá dài: Raumakey phải chèn được nó trong một lần, nên chỉ được \
             dài tối đa {} ký tự (ít hơn nếu có nhiều chữ có dấu).",
            grouped(MAX_REPLACEMENT_SCALARS)
        ),
        RowError::Invalid(EntryError::ReplacementNul) => {
            "Nội dung thay thế không được có ký tự NUL.".to_owned()
        }
        RowError::TooMany => format!(
            "Danh sách đã đạt số mục tối đa ({}). Hãy xoá bớt mục rồi thêm lại.",
            grouped(MAX_MACROS)
        ),
        RowError::NoIdLeft => "Không thêm được mục mới: tệp gõ tắt đã dùng tới số hiệu (id) lớn \
                               nhất. Hãy sửa các id trong tệp bằng tay."
            .to_owned(),
    }
}

/// Why the list of `path` did not open ([`MacroList::load`]). A damaged file is to be fixed by
/// hand: the editor never writes over it.
pub fn load_error_text(path: &Path, error: ConfigError) -> String {
    let path = path.display();
    match error {
        ConfigError::Corrupt => format!(
            "Không đọc được tệp gõ tắt {path}: tệp bị hỏng hoặc sai cú pháp TOML. Raumakey không \
             mở danh sách để không ghi đè tệp này. Hãy sửa tệp bằng tay rồi mở lại danh sách."
        ),
        ConfigError::UnsupportedVersion => format!(
            "Tệp gõ tắt {path} thuộc một phiên bản Raumakey mới hơn. Raumakey không mở danh sách \
             để không ghi đè tệp này."
        ),
        other => format!(
            "Không đọc được tệp gõ tắt {path}: {}. Hãy thử mở lại danh sách sau.",
            reason(other)
        ),
    }
}

/// Schema paths listed in a question before the rest is counted.
const LISTED_PATHS: usize = 6;

/// `a, b, c`: the distinct paths of `issues`, the first [`LISTED_PATHS`] and how many more.
fn paths(issues: &[FieldIssue]) -> String {
    let mut distinct: Vec<&str> = Vec::new();
    for issue in issues {
        if !distinct.contains(&issue.path.as_str()) {
            distinct.push(&issue.path);
        }
    }
    let mut text = distinct
        .iter()
        .take(LISTED_PATHS)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if distinct.len() > LISTED_PATHS {
        text.push_str(&format!(
            " và {} mục khác",
            distinct.len().saturating_sub(LISTED_PATHS)
        ));
    }
    text
}

/// The question when the list opens with [`MacroList::lossy_issues`] (the schema paths of
/// `issues`, never values): "yes" calls [`MacroList::accept_loss`] and shows the list, "no"
/// closes it without writing.
pub fn open_loss_question(path: &Path, issues: &[FieldIssue]) -> String {
    format!(
        "Một số giá trị trong {} không đọc được ({}). Nếu lưu, các giá trị này sẽ bị bỏ hoặc thay \
         bằng giá trị mặc định. Bạn vẫn muốn mở danh sách gõ tắt không?",
        path.display(),
        paths(issues)
    )
}

/// The question after [`MacroList::overwrite`] refused with [`SaveError::Lossy`]: the file it
/// read holds `issues` ([`MacroList::lossy_issues`]). "Yes" calls [`MacroList::accept_loss`]
/// and [`MacroList::overwrite`] again.
pub fn overwrite_loss_question(path: &Path, issues: &[FieldIssue]) -> String {
    format!(
        "Tệp gõ tắt {} hiện có một số giá trị không đọc được ({}). Nếu ghi đè, các giá trị này sẽ \
         bị bỏ hoặc thay bằng giá trị mặc định. Bạn vẫn muốn ghi danh sách đang sửa đè lên tệp \
         đó không?",
        path.display(),
        paths(issues)
    )
}

/// The question after [`SaveError::Conflict`]: "yes" calls [`MacroList::overwrite`].
pub fn conflict_question(path: &Path) -> String {
    format!(
        "Tệp gõ tắt {} vừa được thay đổi ở nơi khác (do sửa bằng tay hoặc do một chương trình \
         khác) sau khi danh sách được mở.\n\nBạn có muốn ghi danh sách đang sửa đè lên tệp đó \
         không? Nếu ghi đè, các mục gõ tắt trong tệp sẽ được thay bằng danh sách này. Nếu không, \
         tệp được giữ nguyên và danh sách đang sửa vẫn còn trong hộp thoại.",
        path.display()
    )
}

/// Why the list was not saved to `path`; for [`SaveError::Conflict`], the
/// [`conflict_question`]. The rows stay in the dialog.
pub fn save_error_text(path: &Path, error: SaveError) -> String {
    let stays = "Danh sách đang sửa vẫn còn trong hộp thoại";
    let failed = format!("Không lưu được danh sách gõ tắt vào {}", path.display());
    match error {
        SaveError::Conflict => conflict_question(path),
        SaveError::Lossy => format!(
            "{failed}: tệp có giá trị không đọc được, sẽ bị bỏ hoặc thay bằng giá trị mặc định \
             nếu lưu, và bạn chưa đồng ý bỏ chúng. {stays}."
        ),
        SaveError::TooLarge => format!(
            "{failed}: danh sách quá lớn, tệp sẽ vượt quá giới hạn 16 MiB. Hãy xoá bớt hoặc rút \
             ngắn các mục rồi lưu lại. {stays}."
        ),
        SaveError::NoIdLeft => format!(
            "{failed}: không còn số hiệu (id) trống cho các mục mới. Hãy sửa các id trong tệp \
             bằng tay. {stays}."
        ),
        SaveError::Store(ConfigError::Corrupt) => format!(
            "{failed}: tệp hiện có bị hỏng hoặc sai cú pháp TOML, và Raumakey không ghi đè nó. \
             Hãy sửa tệp bằng tay rồi lưu lại. {stays}."
        ),
        SaveError::Store(ConfigError::UnsupportedVersion) => format!(
            "{failed}: tệp hiện có thuộc một phiên bản Raumakey mới hơn, và Raumakey không ghi đè \
             nó. {stays}."
        ),
        SaveError::Store(other) => {
            format!("{failed}: {}. {stays}; hãy thử lưu lại.", reason(other))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::sync::atomic::{AtomicBool, Ordering};

    use raumakey_config::StoreDocument;

    use super::*;

    /// A replacer whose swaps fail while `failing` is set.
    struct Switch {
        failing: AtomicBool,
    }

    impl FileReplacer for Switch {
        fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
            if self.failing.load(Ordering::SeqCst) {
                return Err(io::Error::from(io::ErrorKind::PermissionDenied));
            }
            fs::rename(replacement, target)
        }
    }

    /// Another writer holds the store lock longer than the list waits: the save fails as an
    /// I/O failure, writes nothing, keeps the rows, and works once the lock is free.
    #[test]
    fn a_save_that_cannot_take_the_lock_writes_nothing_and_can_be_retried() {
        let dir = tempfile::tempdir().unwrap();
        let replacer: Arc<dyn FileReplacer> = Arc::new(raumakey_config::StdReplacer);
        let mut list =
            MacroList::load_with(dir.path(), replacer, Duration::from_millis(50)).unwrap();
        list.put("a", "x").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(list.store.lock_path())
            .unwrap();
        lock.lock().unwrap();
        assert_eq!(list.save(), Err(SaveError::Store(ConfigError::IoFailure)));
        assert!(!list.path().exists());
        assert!(list.is_modified());
        assert_eq!(list.len(), 1);
        lock.unlock().unwrap();
        assert_eq!(list.save(), Ok(Saved::Written));
        assert!(list.path().exists());
    }

    /// The user chose to write over a file changed by hand, and that write failed: the next
    /// save still writes over the file the user agreed to, keeping its top-level keys, and
    /// still refuses one that changed again.
    #[test]
    fn a_save_after_a_failed_overwrite_writes_over_the_file_the_user_agreed_to() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(MacroDocument::FILE_NAME);
        fs::write(
            &path,
            "version = 1\nrevision = 3\n[[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\n",
        )
        .unwrap();
        let switch = Arc::new(Switch {
            failing: AtomicBool::new(false),
        });
        let replacer: Arc<dyn FileReplacer> = Arc::clone(&switch) as _;
        let mut list = MacroList::load_with(dir.path(), replacer, LOCK_TIMEOUT).unwrap();
        list.put("b", "y").unwrap();
        // A hand edit that keeps the revision.
        let edit = "version = 1\nrevision = 3\ntheirs = 1\n\
                    [[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"edited\"\n";
        fs::write(&path, edit).unwrap();
        assert_eq!(list.save(), Err(SaveError::Conflict));

        switch.failing.store(true, Ordering::SeqCst);
        assert_eq!(
            list.overwrite(),
            Err(SaveError::Store(ConfigError::PermissionDenied))
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), edit);
        assert!(list.is_modified());
        assert_eq!(list.revision(), 3);

        switch.failing.store(false, Ordering::SeqCst);
        assert_eq!(list.save(), Ok(Saved::Written));
        let saved = MacroDocument::decode(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .document;
        assert_eq!(saved.revision, 4);
        assert_eq!(saved.entries.len(), 2);
        assert_eq!(
            saved.extensions.get("theirs").unwrap().as_integer(),
            Some(1)
        );

        // Once more, and the file changes again before the retry: that is a new conflict.
        list.put("c", "z").unwrap();
        let edit = fs::read_to_string(&path)
            .unwrap()
            .replace("theirs = 1", "theirs = 2");
        fs::write(&path, &edit).unwrap();
        assert_eq!(list.save(), Err(SaveError::Conflict));
        switch.failing.store(true, Ordering::SeqCst);
        assert!(list.overwrite().is_err());
        switch.failing.store(false, Ordering::SeqCst);
        let edit = edit.replace("theirs = 2", "theirs = 3");
        fs::write(&path, &edit).unwrap();
        assert_eq!(list.save(), Err(SaveError::Conflict));
        assert_eq!(fs::read_to_string(&path).unwrap(), edit);
    }
}
