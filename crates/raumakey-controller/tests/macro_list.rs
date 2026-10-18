//! The macro list editor's model (`raumakey_controller::macro_list`, D44): loading
//! `raumakey-macros.toml` from the data folder, the rows the dialog shows, adding, editing,
//! and removing rows under the rules of `raumakey-macro` (D29, D30), the save, which keeps
//! what the dialog does not show, conflicts with other writers and the overwrite, damaged
//! files, and the Vietnamese texts. Host-independent: every test works in its own temporary
//! folder.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use raumakey_config::{
    AtomicStore, ConfigError, FieldIssue, IssueReason, MacroDocument, StdReplacer, StoreDocument,
};
use raumakey_controller::macro_list::{
    DISCARD_QUESTION, MacroList, Put, Row, RowError, RowStatus, SaveError, conflict_question,
    load_error_text, open_loss_question, overwrite_loss_question, row_error_text, save_error_text,
};
use raumakey_controller::settings::{DocumentStatus, Settings};
use raumakey_controller::worker::Saved;
use raumakey_ipc::message::MAX_MACROS;
use raumakey_macro::{EntryError, MacroStore, MacroTable};
use raumakey_model::macros::{
    Boundary, BoundaryPolicy, CaseMode, CasePolicy, MacroEntry, MacroId, MacroLookup,
};

/// Macro text that must never appear in formatted output.
const SECRET_TRIGGER: &str = "zqxjtrigger";
const SECRET_REPLACEMENT: &str = "Bí mật thay thế";

/// "việt" with its marks decomposed (NFD).
const VIET_NFD: &str = "vie\u{0302}\u{0323}t";
/// "việt" composed (NFC).
const VIET_NFC: &str = "vi\u{1EC7}t";

fn macros_path(dir: &Path) -> PathBuf {
    dir.join("raumakey-macros.toml")
}

/// A `raumakey-macros.toml` at revision 3 with `entries` (id, trigger, replacement).
fn macros_file(entries: &[(u64, &str, &str)]) -> String {
    let mut text = String::from("version = 1\nrevision = 3\n");
    for (id, trigger, replacement) in entries {
        write!(
            text,
            "[[macro]]\nid = {id}\ntrigger = \"{trigger}\"\nreplacement = \"{replacement}\"\n"
        )
        .unwrap();
    }
    text
}

/// What an independent store reads from `dir`.
fn stored(dir: &Path) -> MacroDocument {
    AtomicStore::<MacroDocument>::new(dir, Arc::new(StdReplacer))
        .load()
        .unwrap()
        .document
}

fn entry(id: u64, trigger: &str, replacement: &str) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::default(),
        case_policy: CasePolicy::default(),
    }
}

/// The rows as (trigger, replacement) pairs.
fn rows(list: &MacroList) -> Vec<(String, String)> {
    list.rows()
        .iter()
        .map(|row| (row.trigger().to_owned(), row.replacement().to_owned()))
        .collect()
}

fn pairs(expected: &[(&str, &str)]) -> Vec<(String, String)> {
    expected
        .iter()
        .map(|(trigger, replacement)| ((*trigger).to_owned(), (*replacement).to_owned()))
        .collect()
}

#[test]
fn a_missing_file_is_an_empty_list_and_the_first_save_creates_it() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(list.is_empty());
    assert_eq!(list.len(), 0);
    assert_eq!(list.revision(), 0);
    assert!(!list.is_modified());
    assert_eq!(list.path(), macros_path(dir.path()));
    // Loading creates nothing.
    assert!(!macros_path(dir.path()).exists());
    // Nothing to save yet: nothing is written.
    assert_eq!(list.save(), Ok(Saved::Unchanged));
    assert!(!macros_path(dir.path()).exists());

    assert_eq!(list.put("ko", "không"), Ok(Put::Added(0)));
    assert_eq!(list.put("vn", "Việt Nam"), Ok(Put::Added(1)));
    assert!(list.is_modified());
    assert_eq!(list.save(), Ok(Saved::Written));
    assert!(!list.is_modified());
    assert_eq!(list.revision(), 1);

    let document = stored(dir.path());
    assert_eq!(document.revision, 1);
    assert_eq!(
        document.entries,
        vec![entry(1, "ko", "không"), entry(2, "vn", "Việt Nam")]
    );
    assert!(document.entry_extras.is_empty());
    assert!(document.extensions.is_empty());

    // An unchanged list writes nothing.
    assert_eq!(list.save(), Ok(Saved::Unchanged));
    assert_eq!(stored(dir.path()).revision, 1);
}

#[test]
fn rows_show_every_entry_in_file_order_even_the_ones_that_are_invalid_now() {
    let dir = tempfile::tempdir().unwrap();
    let long_trigger = "t".repeat(33);
    let text = macros_file(&[
        (4, "b", "second"),
        (2, &long_trigger, "too long a trigger"),
        (9, "a b", "whitespace"),
        (1, VIET_NFD, "decomposed"),
    ]);
    fs::write(macros_path(dir.path()), &text).unwrap();
    let list = MacroList::load(dir.path()).unwrap();
    assert_eq!(list.revision(), 3);
    assert_eq!(list.len(), 4);
    assert!(!list.is_modified());
    assert_eq!(
        rows(&list),
        pairs(&[
            ("b", "second"),
            (&long_trigger, "too long a trigger"),
            ("a b", "whitespace"),
            (VIET_NFD, "decomposed"),
        ])
    );
    // Loading writes nothing.
    assert_eq!(fs::read_to_string(macros_path(dir.path())).unwrap(), text);
}

#[test]
fn put_adds_a_new_trigger_and_updates_the_row_with_the_same_trigger_after_nfc() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(
        list.put(VIET_NFD, "Vie\u{0302}\u{0323}t Nam"),
        Ok(Put::Added(0))
    );
    assert_eq!(list.put("ko", "không"), Ok(Put::Added(1)));
    // The NFC forms are stored.
    assert_eq!(list.rows()[0].trigger(), VIET_NFC);
    assert_eq!(list.rows()[0].replacement(), "Việt Nam");
    // The composed trigger is the same trigger.
    assert_eq!(list.put(VIET_NFC, "tiếng Việt"), Ok(Put::Updated(0)));
    assert_eq!(list.len(), 2);
    assert_eq!(
        rows(&list),
        pairs(&[(VIET_NFC, "tiếng Việt"), ("ko", "không")])
    );
    // Triggers differ by case.
    assert_eq!(list.put("KO", "KHÔNG"), Ok(Put::Added(2)));
}

#[test]
fn put_updates_a_decomposed_trigger_of_the_file_and_stores_it_composed() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        macros_file(&[
            (1, "ko", "không"),
            (2, VIET_NFD, "old"),
            (3, VIET_NFC, "repeat"),
        ]),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    // The first row with the same trigger is updated.
    assert_eq!(list.put(VIET_NFC, "Việt Nam"), Ok(Put::Updated(1)));
    assert!(list.is_modified());
    assert_eq!(
        rows(&list),
        pairs(&[
            ("ko", "không"),
            (VIET_NFC, "Việt Nam"),
            (VIET_NFC, "repeat")
        ])
    );
    assert_eq!(list.save(), Ok(Saved::Written));
    let document = stored(dir.path());
    assert_eq!(
        document.entries,
        vec![
            entry(1, "ko", "không"),
            entry(2, VIET_NFC, "Việt Nam"),
            entry(3, VIET_NFC, "repeat"),
        ]
    );
}

#[test]
fn putting_what_a_row_holds_already_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), macros_file(&[(1, "ko", "không")])).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(list.put("ko", "không"), Ok(Put::Updated(0)));
    assert!(!list.is_modified());
}

#[test]
fn remove_takes_a_row_out() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        macros_file(&[(1, "a", "x"), (2, "b", "y"), (3, "c", "z")]),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(!list.remove(3));
    assert!(!list.is_modified());
    assert!(list.remove(1));
    assert!(list.is_modified());
    assert_eq!(rows(&list), pairs(&[("a", "x"), ("c", "z")]));
    assert_eq!(list.save(), Ok(Saved::Written));
    assert_eq!(
        stored(dir.path()).entries,
        vec![entry(1, "a", "x"), entry(3, "c", "z")]
    );
}

#[test]
fn removing_every_row_saves_an_empty_list() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), macros_file(&[(1, "a", "x")])).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(list.remove(0));
    assert_eq!(list.save(), Ok(Saved::Written));
    let document = stored(dir.path());
    assert!(document.entries.is_empty());
    assert_eq!(document.revision, 4);
}

#[test]
fn an_invalid_row_is_refused_with_its_reason_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    let longest = "t".repeat(32);
    let too_long = "t".repeat(33);
    // "ệ" is three UTF-16 units in NFD: 2,731 of them do not fit one insert (D30).
    let too_long_replacement = "ệ".repeat(2731);
    let cases: [(&str, &str, EntryError); 7] = [
        ("", "x", EntryError::EmptyTrigger),
        (&too_long, "x", EntryError::TriggerTooLong),
        ("k o", "x", EntryError::TriggerWhitespace),
        ("ko\u{7}", "x", EntryError::TriggerControl),
        ("ko", "", EntryError::EmptyReplacement),
        ("ko", &too_long_replacement, EntryError::ReplacementTooLong),
        ("ko", "a\0b", EntryError::ReplacementNul),
    ];
    for (trigger, replacement, reason) in cases {
        assert_eq!(
            list.put(trigger, replacement),
            Err(RowError::Invalid(reason)),
            "{reason:?}"
        );
        assert!(list.is_empty());
        assert!(!list.is_modified());
    }
    // Trigger errors come first.
    assert_eq!(
        list.put("", ""),
        Err(RowError::Invalid(EntryError::EmptyTrigger))
    );
    // The limits themselves are accepted, and a replacement may span lines.
    assert_eq!(
        list.put(&longest, "dòng một\r\ndòng hai"),
        Ok(Put::Added(0))
    );
    // 32 composed scalars, typed decomposed.
    let decomposed = "e\u{0302}\u{0323}".repeat(32);
    assert_eq!(list.put(&decomposed, "x"), Ok(Put::Added(1)));
    // A row that does not fit is not updated either.
    assert_eq!(
        list.put(&longest, ""),
        Err(RowError::Invalid(EntryError::EmptyReplacement))
    );
    assert_eq!(list.rows()[0].replacement(), "dòng một\r\ndòng hai");
}

#[test]
fn no_row_is_added_beyond_the_limit_of_the_macro_table() {
    let dir = tempfile::tempdir().unwrap();
    let mut text = String::from("version = 1\nrevision = 1\n");
    for id in 1..=MAX_MACROS {
        write!(
            text,
            "[[macro]]\nid = {id}\ntrigger = \"k{id}\"\nreplacement = \"x\"\n"
        )
        .unwrap();
    }
    fs::write(macros_path(dir.path()), text).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(list.len(), MAX_MACROS);
    assert_eq!(list.put("new", "x"), Err(RowError::TooMany));
    assert!(!list.is_modified());
    // Rows can still be edited.
    assert_eq!(list.put("k1", "y"), Ok(Put::Updated(0)));
    assert!(list.remove(MAX_MACROS - 1));
    assert_eq!(list.put("new", "x"), Ok(Put::Added(MAX_MACROS - 1)));
    assert_eq!(list.put("newer", "x"), Err(RowError::TooMany));
}

#[test]
fn debug_output_shows_no_macro_text() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        macros_file(&[(1, SECRET_TRIGGER, SECRET_REPLACEMENT)]),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    list.put("zqxjother", "Nội dung bí mật khác").unwrap();
    let texts = [
        format!("{list:?}"),
        format!("{:?}", list.rows()),
        format!("{list:#?}"),
    ];
    for text in texts {
        for secret in [
            SECRET_TRIGGER,
            SECRET_REPLACEMENT,
            "zqxjother",
            "Nội dung bí mật khác",
        ] {
            assert!(!text.contains(secret), "{text}");
        }
    }
    let text = format!("{list:?}");
    assert!(text.contains("revision: 3"), "{text}");
    assert!(text.contains("rows: 2"), "{text}");
    assert!(text.contains("modified: true"), "{text}");
}

#[test]
fn the_list_can_move_to_the_dialog_thread() {
    fn send<T: Send>() {}
    send::<MacroList>();
}

/// A file with what the dialog does not show: switches, unknown keys of entries and of the
/// document, and two entries set aside (no id; no trigger, with id 20).
const RICH_FILE: &str = r#"version = 1
revision = 7
future = "kept"

[future_table]
answer = 42

[[macro]]
id = 3
trigger = "ko"
replacement = "không"
enabled = false
boundary = "whitespace"
case = "exact"
note = "first"

[[macro]]
id = 9
trigger = "vn"
replacement = "Việt Nam"
color = "blue"

[[macro]]
trigger = "lost"
replacement = "no id"
extra = 1

[[macro]]
id = 20
replacement = "no trigger"

[[macro]]
id = 5
trigger = "dc"
replacement = "được"
"#;

fn identity(id: u64, trigger: &str) -> (MacroId, String) {
    (MacroId(id), trigger.to_owned())
}

/// The string value of the unknown key `key` of the entry `(id, trigger)` in `document`.
fn unknown<'a>(document: &'a MacroDocument, id: u64, trigger: &str, key: &str) -> Option<&'a str> {
    document
        .entry_extras
        .unknown_keys_of(&identity(id, trigger))?
        .get(key)?
        .as_str()
}

#[test]
fn ids_and_switches_of_the_entries_are_kept_and_new_rows_get_new_ids_with_the_defaults() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), RICH_FILE).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(
        rows(&list),
        pairs(&[("ko", "không"), ("vn", "Việt Nam"), ("dc", "được")])
    );
    assert_eq!(list.put("ko", "không bao giờ"), Ok(Put::Updated(0)));
    assert_eq!(list.put("new", "mới"), Ok(Put::Added(3)));
    assert_eq!(list.put("newer", "mới hơn"), Ok(Put::Added(4)));
    assert_eq!(list.save(), Ok(Saved::Written));
    assert_eq!(list.revision(), 8);
    let document = stored(dir.path());
    assert_eq!(document.revision, 8);
    // New ids lie above every id still in the file, including the one set aside (20).
    assert_eq!(
        document.entries,
        vec![
            MacroEntry {
                enabled: false,
                boundary: BoundaryPolicy::Whitespace,
                case_policy: CasePolicy::Exact,
                ..entry(3, "ko", "không bao giờ")
            },
            entry(9, "vn", "Việt Nam"),
            entry(5, "dc", "được"),
            entry(21, "new", "mới"),
            entry(22, "newer", "mới hơn"),
        ]
    );
}

#[test]
fn unknown_keys_and_entries_set_aside_survive_a_save() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), RICH_FILE).unwrap();
    let original = stored(dir.path());
    assert_eq!(original.entry_extras.rejected.len(), 2);

    // A save that changes no entry writes the same document.
    let mut list = MacroList::load(dir.path()).unwrap();
    list.put("tmp", "x").unwrap();
    assert!(list.remove(3));
    assert_eq!(list.save(), Ok(Saved::Written));
    let saved = stored(dir.path());
    assert_eq!(
        saved,
        MacroDocument {
            revision: 8,
            ..original.clone()
        }
    );

    // Edits keep the unknown keys of the entries that stay.
    assert_eq!(list.put("ko", "không bao giờ"), Ok(Put::Updated(0)));
    assert!(list.remove(1));
    assert_eq!(list.put("vn", "nước Việt Nam"), Ok(Put::Added(2)));
    assert_eq!(list.save(), Ok(Saved::Written));
    let saved = stored(dir.path());
    assert_eq!(unknown(&saved, 3, "ko", "note"), Some("first"));
    // The removed entry's keys are gone; the new row with its trigger has none.
    assert_eq!(saved.entries[2], entry(21, "vn", "nước Việt Nam"));
    assert!(
        saved
            .entry_extras
            .unknown_keys_of(&identity(9, "vn"))
            .is_none()
    );
    assert!(
        saved
            .entry_extras
            .unknown_keys_of(&identity(21, "vn"))
            .is_none()
    );
    assert_eq!(saved.entry_extras.unknown_keys.len(), 1);
    assert_eq!(saved.entry_extras.rejected, original.entry_extras.rejected);
    assert_eq!(saved.extensions, original.extensions);
    assert_eq!(
        saved.extensions.get("future").unwrap().as_str(),
        Some("kept")
    );
}

#[test]
fn a_new_row_never_takes_the_unknown_keys_of_a_removed_entry() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        "version = 1\nrevision = 1\n[[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\n\
         [[macro]]\nid = 2\ntrigger = \"b\"\nreplacement = \"y\"\nnote = \"of b\"\n",
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(list.remove(1));
    // Like `MacroStore::next_id`, the id of the removed largest entry is free again; the new
    // row has the identity of the removed entry, but not its keys.
    assert_eq!(list.put("b", "again"), Ok(Put::Added(1)));
    assert_eq!(list.save(), Ok(Saved::Written));
    let saved = stored(dir.path());
    assert_eq!(
        saved.entries,
        vec![entry(1, "a", "x"), entry(2, "b", "again")]
    );
    assert!(saved.entry_extras.is_empty());
}

#[test]
fn a_trigger_composed_by_an_edit_keeps_its_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        format!(
            "version = 1\nrevision = 1\n[[macro]]\nid = 4\ntrigger = \"{VIET_NFD}\"\n\
             replacement = \"x\"\nenabled = false\nnote = \"kept\"\n"
        ),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(list.put(VIET_NFC, "Việt"), Ok(Put::Updated(0)));
    assert_eq!(list.save(), Ok(Saved::Written));
    let saved = stored(dir.path());
    assert_eq!(
        saved.entries,
        vec![MacroEntry {
            enabled: false,
            ..entry(4, VIET_NFC, "Việt")
        }]
    );
    assert_eq!(unknown(&saved, 4, VIET_NFC, "note"), Some("kept"));
}

#[test]
fn entries_with_the_same_identity_keep_their_own_unknown_keys() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        "version = 1\nrevision = 1\n\
         [[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\nnote = \"one\"\n\
         [[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"y\"\nenabled = false\n\
         [[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"z\"\nnote = \"three\"\n",
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(list.remove(0));
    assert_eq!(list.save(), Ok(Saved::Written));
    let saved = stored(dir.path());
    assert_eq!(
        saved.entries,
        vec![
            MacroEntry {
                enabled: false,
                ..entry(1, "a", "y")
            },
            entry(1, "a", "z"),
        ]
    );
    let notes: Vec<Option<&str>> = saved
        .entry_extras
        .unknown_keys
        .iter()
        .map(|(_, table)| table.get("note").and_then(|value| value.as_str()))
        .collect();
    assert_eq!(notes, vec![None, Some("three")]);
}

#[test]
fn no_row_is_added_when_no_id_is_left_for_it() {
    let dir = tempfile::tempdir().unwrap();
    let largest = i64::MAX.unsigned_abs();
    fs::write(
        macros_path(dir.path()),
        macros_file(&[(largest - 1, "a", "x")]),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(list.put("b", "y"), Ok(Put::Added(1)));
    assert_eq!(list.put("c", "z"), Err(RowError::NoIdLeft));
    assert_eq!(list.save(), Ok(Saved::Written));
    assert_eq!(
        stored(dir.path()).entries,
        vec![entry(largest - 1, "a", "x"), entry(largest, "b", "y")]
    );
    // An entry set aside holds the largest id.
    fs::write(
        macros_path(dir.path()),
        format!("version = 1\nrevision = 1\n[[macro]]\nid = {largest}\nreplacement = \"x\"\n"),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(list.is_empty());
    assert_eq!(list.put("b", "y"), Err(RowError::NoIdLeft));
}

/// Another writer's version of [`RICH_FILE`] at revision 8: a new top-level key, entry 3
/// switched on with another note, entry 9 removed, entry 30 added, and an entry set aside with
/// id 40 instead of the two of the original.
const THEIR_FILE: &str = r#"version = 1
revision = 7
future = "theirs"
added = true

[[macro]]
id = 3
trigger = "ko"
replacement = "không có"
note = "theirs"

[[macro]]
id = 5
trigger = "dc"
replacement = "được"

[[macro]]
id = 30
trigger = "theirs"
replacement = "của họ"

[[macro]]
id = 40
replacement = "set aside by them"
"#;

/// Commits [`THEIR_FILE`] over revision 7 through another store, as another editor does.
fn commit_theirs(dir: &Path) -> MacroDocument {
    let other = AtomicStore::<MacroDocument>::new(dir, Arc::new(StdReplacer));
    let document = MacroDocument::decode(THEIR_FILE).unwrap().document;
    assert_eq!(other.commit(7, &document), Ok(8));
    stored(dir)
}

#[test]
fn a_save_after_another_writer_committed_is_a_conflict_that_keeps_the_edits() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), RICH_FILE).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(list.put("dc", "được rồi"), Ok(Put::Updated(2)));
    assert_eq!(list.put("new", "mới"), Ok(Put::Added(3)));
    let edited = rows(&list);
    let theirs = commit_theirs(dir.path());

    assert_eq!(list.save(), Err(SaveError::Conflict));
    assert!(list.is_modified());
    assert_eq!(rows(&list), edited);
    assert_eq!(list.revision(), 7);
    assert_eq!(stored(dir.path()), theirs);
    // Saving again does not write over it either.
    assert_eq!(list.save(), Err(SaveError::Conflict));
    assert_eq!(stored(dir.path()), theirs);

    // The user chose to write the list over the file.
    assert_eq!(list.overwrite(), Ok(()));
    assert!(!list.is_modified());
    assert_eq!(list.revision(), 9);
    assert_eq!(rows(&list), edited);
    let saved = stored(dir.path());
    assert_eq!(saved.revision, 9);
    // The rows replace the entries; ids are kept, the new row's id lies above every id still
    // present (40, set aside by the other writer). Entry 3 takes the switches and keys of the
    // current file, entry 9 (which the other writer removed) the ones it was loaded with.
    assert_eq!(
        saved.entries,
        vec![
            entry(3, "ko", "không"),
            entry(9, "vn", "Việt Nam"),
            entry(5, "dc", "được rồi"),
            entry(41, "new", "mới"),
        ]
    );
    assert_eq!(unknown(&saved, 3, "ko", "note"), Some("theirs"));
    assert_eq!(unknown(&saved, 9, "vn", "color"), Some("blue"));
    assert_eq!(saved.entry_extras.unknown_keys.len(), 2);
    assert_eq!(saved.entry_extras.rejected, theirs.entry_extras.rejected);
    assert_eq!(saved.extensions, theirs.extensions);
    assert_eq!(
        saved.extensions.get("future").unwrap().as_str(),
        Some("theirs")
    );
    assert_eq!(saved.extensions.get("added").unwrap().as_bool(), Some(true));

    // The list goes on from the written file.
    assert_eq!(list.put("x", "y"), Ok(Put::Added(4)));
    assert_eq!(list.save(), Ok(Saved::Written));
    assert_eq!(stored(dir.path()).entries[4], entry(42, "x", "y"));
}

#[test]
fn a_hand_edit_that_keeps_the_revision_is_a_conflict_too() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), macros_file(&[(1, "a", "x")])).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    list.put("b", "y").unwrap();
    // The same revision, an edited replacement, and a new top-level key.
    let edit = "version = 1\nrevision = 3\nfuture = 1\n\
                [[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"edited\"\n";
    fs::write(macros_path(dir.path()), edit).unwrap();
    assert_eq!(list.save(), Err(SaveError::Conflict));
    assert_eq!(fs::read_to_string(macros_path(dir.path())).unwrap(), edit);
    assert_eq!(list.overwrite(), Ok(()));
    let saved = stored(dir.path());
    assert_eq!(saved.revision, 4);
    assert_eq!(saved.entries, vec![entry(1, "a", "x"), entry(2, "b", "y")]);
    assert_eq!(
        saved.extensions.get("future").unwrap().as_integer(),
        Some(1)
    );
}

#[test]
fn a_file_created_after_an_empty_load_is_a_conflict() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    list.put("a", "x").unwrap();
    fs::write(macros_path(dir.path()), macros_file(&[(7, "b", "y")])).unwrap();
    assert_eq!(list.save(), Err(SaveError::Conflict));
    assert_eq!(list.overwrite(), Ok(()));
    assert_eq!(stored(dir.path()).entries, vec![entry(1, "a", "x")]);
}

#[test]
fn a_file_removed_before_the_overwrite_is_written_again() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), RICH_FILE).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    list.put("new", "mới").unwrap();
    fs::remove_file(macros_path(dir.path())).unwrap();
    assert_eq!(list.save(), Err(SaveError::Conflict));
    assert!(!macros_path(dir.path()).exists());
    assert_eq!(list.overwrite(), Ok(()));
    let saved = stored(dir.path());
    assert_eq!(saved.revision, 1);
    assert_eq!(
        saved.entries,
        vec![
            MacroEntry {
                enabled: false,
                boundary: BoundaryPolicy::Whitespace,
                case_policy: CasePolicy::Exact,
                ..entry(3, "ko", "không")
            },
            entry(9, "vn", "Việt Nam"),
            entry(5, "dc", "được"),
            entry(10, "new", "mới"),
        ]
    );
    // The rows keep the keys they were loaded with; what belonged to the file is gone with it.
    assert_eq!(unknown(&saved, 3, "ko", "note"), Some("first"));
    assert!(saved.entry_extras.rejected.is_empty());
    assert!(saved.extensions.is_empty());
}

#[test]
fn a_damaged_file_is_a_load_error_and_is_left_as_it_is() {
    let dir = tempfile::tempdir().unwrap();
    for (text, error) in [
        ("[[macro]\n", ConfigError::Corrupt),
        (
            "version = 99\nrevision = 1\n",
            ConfigError::UnsupportedVersion,
        ),
    ] {
        fs::write(macros_path(dir.path()), text).unwrap();
        let failure = MacroList::load(dir.path()).unwrap_err();
        assert_eq!(failure.error(), error);
        assert_eq!(failure.path(), macros_path(dir.path()));
        assert_eq!(fs::read_to_string(macros_path(dir.path())).unwrap(), text);
    }
}

#[test]
fn a_file_damaged_before_the_save_or_the_overwrite_is_not_written() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), RICH_FILE).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    list.put("new", "mới").unwrap();
    commit_theirs(dir.path());
    assert_eq!(list.save(), Err(SaveError::Conflict));
    let damaged = "version = 1\nrevision = 8\n[[macro]\n";
    fs::write(macros_path(dir.path()), damaged).unwrap();
    assert_eq!(
        list.overwrite(),
        Err(SaveError::Store(ConfigError::Corrupt))
    );
    assert_eq!(list.save(), Err(SaveError::Store(ConfigError::Corrupt)));
    assert_eq!(
        fs::read_to_string(macros_path(dir.path())).unwrap(),
        damaged
    );
    assert!(list.is_modified());
    assert_eq!(list.len(), 4);
    assert_eq!(list.revision(), 7);

    // Damaged while the list was open, without a conflict before.
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), RICH_FILE).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    list.put("new", "mới").unwrap();
    fs::write(macros_path(dir.path()), damaged).unwrap();
    assert_eq!(list.save(), Err(SaveError::Store(ConfigError::Corrupt)));
    assert_eq!(
        list.overwrite(),
        Err(SaveError::Store(ConfigError::Corrupt))
    );
    assert_eq!(
        fs::read_to_string(macros_path(dir.path())).unwrap(),
        damaged
    );
}

/// The macro table the hook builds from the file in `dir`, as `Settings` builds it.
fn engine_table(dir: &Path) -> (MacroTable, usize) {
    let document = stored(dir);
    let (store, skipped) = MacroStore::from_entries_lenient(document.revision, document.entries);
    (MacroTable::new(store.entries()).unwrap(), skipped.len())
}

/// What the hook inserts for `typed` at a space.
fn expands(table: &MacroTable, typed: &str) -> Option<String> {
    table
        .lookup(typed, Boundary::Whitespace, CaseMode::Adaptive)
        .map(|hit| hit.replacement)
}

fn statuses(list: &MacroList) -> Vec<RowStatus> {
    list.rows().iter().map(Row::status).collect()
}

#[test]
fn rows_with_a_repeated_id_are_shown_and_written_back_untouched() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        macros_file(&[(1, "a", "x"), (1, "b", "y"), (2, "c", "z")]),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(rows(&list), pairs(&[("a", "x"), ("b", "y"), ("c", "z")]));
    assert_eq!(list.put("d", "w"), Ok(Put::Added(3)));
    assert_eq!(list.save(), Ok(Saved::Written));
    assert_eq!(
        stored(dir.path()).entries,
        vec![
            entry(1, "a", "x"),
            entry(1, "b", "y"),
            entry(2, "c", "z"),
            entry(3, "d", "w"),
        ]
    );
}

#[test]
fn an_edited_row_whose_id_an_earlier_row_has_gets_a_new_id_so_the_hook_uses_it() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        "version = 1\nrevision = 1\n\
         [[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\n\
         [[macro]]\nid = 1\ntrigger = \"b\"\nreplacement = \"y\"\ncase = \"exact\"\nnote = \"b\"\n\
         [[macro]]\nid = 1\ntrigger = \"c\"\nreplacement = \"z\"\n",
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(
        statuses(&list),
        vec![RowStatus::Active, RowStatus::Unused, RowStatus::Unused]
    );
    assert_eq!(list.put("b", "edited"), Ok(Put::Updated(1)));
    assert_eq!(
        statuses(&list),
        vec![RowStatus::Active, RowStatus::Active, RowStatus::Unused]
    );
    assert_eq!(list.save(), Ok(Saved::Written));
    let saved = stored(dir.path());
    // The edited row keeps its switches and unknown keys under its new id; the row that was
    // not edited keeps its id.
    assert_eq!(
        saved.entries,
        vec![
            entry(1, "a", "x"),
            MacroEntry {
                case_policy: CasePolicy::Exact,
                ..entry(2, "b", "edited")
            },
            entry(1, "c", "z"),
        ]
    );
    assert_eq!(unknown(&saved, 2, "b", "note"), Some("b"));
    let (table, skipped) = engine_table(dir.path());
    assert_eq!(skipped, 1);
    assert_eq!(expands(&table, "b").as_deref(), Some("edited"));
    assert_eq!(
        statuses(&list),
        vec![RowStatus::Active, RowStatus::Active, RowStatus::Unused]
    );
}

#[test]
fn put_edits_the_row_the_hook_uses_for_a_repeated_trigger() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        // `\u0000` is TOML for NUL.
        macros_file(&[(3, "a", "x"), (2, "a", "y"), (1, "a", "nul\\u0000inside")]),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    // Id 1 is invalid (NUL), so the hook uses id 2, not the first row or the lowest id.
    assert_eq!(
        statuses(&list),
        vec![RowStatus::Unused, RowStatus::Active, RowStatus::Unused]
    );
    assert_eq!(list.put("a", "edited"), Ok(Put::Updated(1)));
    assert_eq!(list.save(), Ok(Saved::Written));
    let (table, _) = engine_table(dir.path());
    assert_eq!(expands(&table, "a").as_deref(), Some("edited"));
}

#[test]
fn put_falls_back_to_the_first_row_when_the_hook_uses_none_with_the_trigger() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        "version = 1\nrevision = 1\n\
         [[macro]]\nid = 4\ntrigger = \"a\"\nreplacement = \"x\"\nenabled = false\n\
         [[macro]]\nid = 2\ntrigger = \"a\"\nreplacement = \"y\"\nenabled = false\n",
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(
        statuses(&list),
        vec![RowStatus::Disabled, RowStatus::Disabled]
    );
    assert_eq!(list.put("a", "edited"), Ok(Put::Updated(0)));
}

#[test]
fn each_row_says_whether_the_hook_uses_it() {
    let dir = tempfile::tempdir().unwrap();
    let long_trigger = "t".repeat(33);
    fs::write(
        macros_path(dir.path()),
        format!(
            "version = 1\nrevision = 1\n\
             [[macro]]\nid = 1\ntrigger = \"ko\"\nreplacement = \"không\"\n\
             [[macro]]\nid = 2\ntrigger = \"off\"\nreplacement = \"tắt\"\nenabled = false\n\
             [[macro]]\nid = 3\ntrigger = \"{long_trigger}\"\nreplacement = \"x\"\n\
             [[macro]]\nid = 1\ntrigger = \"dup\"\nreplacement = \"x\"\n\
             [[macro]]\nid = 5\ntrigger = \"ko\"\nreplacement = \"shadowed\"\n\
             [[macro]]\nid = 6\ntrigger = \"z\"\nreplacement = \"off\"\nenabled = false\n\
             [[macro]]\nid = 7\ntrigger = \"z\"\nreplacement = \"on\"\n\
             [[macro]]\nid = 8\ntrigger = \"KO\"\nreplacement = \"KHÔNG\"\n"
        ),
    )
    .unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    let expected = vec![
        RowStatus::Active,
        RowStatus::Disabled,
        RowStatus::Unused,
        RowStatus::Unused,
        RowStatus::Unused,
        RowStatus::Disabled,
        RowStatus::Active,
        RowStatus::Active,
    ];
    assert_eq!(statuses(&list), expected);
    // A new row is used; removing the row that shadows another brings that one into use.
    assert_eq!(list.put("new", "mới"), Ok(Put::Added(8)));
    assert_eq!(list.rows()[8].status(), RowStatus::Active);
    assert!(list.remove(0));
    assert_eq!(list.rows()[3].status(), RowStatus::Active);
    // The statuses agree with the table the hook builds.
    assert_eq!(list.save(), Ok(Saved::Written));
    let (table, skipped) = engine_table(dir.path());
    // The trigger of 33 scalars; id 1 is no longer repeated.
    assert_eq!(skipped, 1);
    assert_eq!(list.rows()[2].status(), RowStatus::Active);
    assert_eq!(expands(&table, "dup").as_deref(), Some("x"));
    assert_eq!(expands(&table, "ko").as_deref(), Some("shadowed"));
    assert_eq!(expands(&table, "z").as_deref(), Some("on"));
    assert_eq!(expands(&table, "off"), None);

    assert_eq!(RowStatus::Active.label(), "");
    assert_eq!(RowStatus::Disabled.label(), "đang tắt");
    assert_eq!(RowStatus::Unused.label(), "không dùng");
}

#[test]
fn a_list_too_large_for_the_store_is_refused_before_the_commit() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    // 4,096 CJK scalars fit one insert and take 12 KiB in UTF-8: 1,400 of them exceed 16 MiB.
    let long = "中".repeat(4096);
    for n in 0..1400 {
        list.put(&format!("k{n}"), &long).unwrap();
    }
    assert_eq!(list.save(), Err(SaveError::TooLarge));
    assert!(!macros_path(dir.path()).exists());
    assert!(list.is_modified());
    // Shorter, it fits.
    for n in 700..1400 {
        assert_eq!(list.put(&format!("k{n}"), "x"), Ok(Put::Updated(n)));
    }
    assert_eq!(list.save(), Ok(Saved::Written));
    assert_eq!(stored(dir.path()).entries.len(), 1400);
}

fn issue(path: &str, reason: IssueReason) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason,
    }
}

/// Loads `text`, checks the values a save would lose, and that nothing is written over them
/// until the loss is accepted.
fn assert_loss_needs_consent(text: &str, lossy: &[FieldIssue]) -> MacroDocument {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), text).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(list.lossy_issues(), lossy);
    // Nothing changed: nothing to write, nothing to refuse.
    assert_eq!(list.save(), Ok(Saved::Unchanged));
    list.put("new", "mới").unwrap();
    assert_eq!(list.save(), Err(SaveError::Lossy));
    assert_eq!(list.overwrite(), Err(SaveError::Lossy));
    assert_eq!(fs::read_to_string(macros_path(dir.path())).unwrap(), text);
    assert!(list.is_modified());
    list.accept_loss();
    assert_eq!(list.save(), Ok(Saved::Written));
    assert!(list.lossy_issues().is_empty());
    stored(dir.path())
}

#[test]
fn a_single_macro_table_is_not_written_over_without_consent() {
    let saved = assert_loss_needs_consent(
        "version = 1\nrevision = 1\n[macro]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\n",
        &[issue("macro", IssueReason::InvalidType)],
    );
    assert_eq!(saved.entries, vec![entry(1, "new", "mới")]);
}

#[test]
fn an_element_of_the_macro_array_that_is_no_table_is_not_dropped_without_consent() {
    let saved = assert_loss_needs_consent(
        "version = 1\nrevision = 1\n\
         macro = [{ id = 1, trigger = \"a\", replacement = \"x\" }, \"oops\"]\n",
        &[issue("macro[1]", IssueReason::InvalidType)],
    );
    assert_eq!(
        saved.entries,
        vec![entry(1, "a", "x"), entry(2, "new", "mới")]
    );
}

#[test]
fn switches_that_cannot_be_read_are_not_reset_without_consent() {
    let saved = assert_loss_needs_consent(
        "version = 1\nrevision = 1\n[[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\n\
         enabled = \"no\"\nboundary = \"whitespce\"\ncase = 3\n",
        &[
            issue("macro[0].enabled", IssueReason::InvalidType),
            issue("macro[0].boundary", IssueReason::InvalidValue),
            issue("macro[0].case", IssueReason::InvalidType),
        ],
    );
    // Accepted: the defaults are written.
    assert_eq!(
        saved.entries,
        vec![entry(1, "a", "x"), entry(2, "new", "mới")]
    );
}

#[test]
fn entries_set_aside_and_a_missing_version_lose_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let text = "revision = 1\n[[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\n\
                [[macro]]\nid = 2\nreplacement = \"no trigger\"\n\
                [[macro]]\nid = -1\ntrigger = \"b\"\nreplacement = \"bad id\"\n\
                [[macro]]\nid = 3\ntrigger = \"c\"\nreplacement = 4\n";
    fs::write(macros_path(dir.path()), text).unwrap();
    let original = stored(dir.path());
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(list.lossy_issues().is_empty());
    list.put("new", "mới").unwrap();
    assert_eq!(list.save(), Ok(Saved::Written));
    let saved = stored(dir.path());
    assert_eq!(saved.entry_extras.rejected, original.entry_extras.rejected);
    assert_eq!(saved.entry_extras.rejected.len(), 3);
}

#[test]
fn an_overwrite_asks_again_for_what_the_current_file_would_lose() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), macros_file(&[(1, "a", "x")])).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(list.lossy_issues().is_empty());
    list.put("b", "y").unwrap();
    // Another writer leaves a switch this build cannot read.
    let theirs = "version = 1\nrevision = 4\n[[macro]]\nid = 1\ntrigger = \"a\"\n\
                  replacement = \"x\"\nenabled = \"later\"\n";
    fs::write(macros_path(dir.path()), theirs).unwrap();
    assert_eq!(list.save(), Err(SaveError::Conflict));
    assert_eq!(list.overwrite(), Err(SaveError::Lossy));
    assert_eq!(
        list.lossy_issues(),
        &[issue("macro[0].enabled", IssueReason::InvalidType)]
    );
    assert_eq!(fs::read_to_string(macros_path(dir.path())).unwrap(), theirs);
    list.accept_loss();
    assert_eq!(list.overwrite(), Ok(()));
    assert_eq!(
        stored(dir.path()).entries,
        vec![entry(1, "a", "x"), entry(2, "b", "y")]
    );
}

#[test]
fn a_loss_accepted_at_the_load_is_not_asked_again_for_the_same_values() {
    let dir = tempfile::tempdir().unwrap();
    let bad = "version = 1\nrevision = 1\n[[macro]]\nid = 1\ntrigger = \"a\"\nreplacement = \"x\"\n\
               case = \"never\"\n";
    fs::write(macros_path(dir.path()), bad).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(
        list.lossy_issues(),
        &[issue("macro[0].case", IssueReason::InvalidValue)]
    );
    list.accept_loss();
    list.put("b", "y").unwrap();
    // A hand edit elsewhere that keeps the unreadable value.
    fs::write(
        macros_path(dir.path()),
        format!("{bad}[[macro]]\nid = 7\ntrigger = \"z\"\nreplacement = \"theirs\"\n"),
    )
    .unwrap();
    assert_eq!(list.save(), Err(SaveError::Conflict));
    assert_eq!(list.overwrite(), Ok(()));
    assert!(list.lossy_issues().is_empty());
}

/// Every text is shown in a message box or beside the edit fields: not empty, no NUL.
fn shown(text: &str) -> &str {
    assert!(!text.is_empty());
    assert!(!text.contains('\0'), "{text}");
    text
}

#[test]
fn every_row_error_has_its_own_vietnamese_message() {
    let errors = [
        RowError::Invalid(EntryError::EmptyTrigger),
        RowError::Invalid(EntryError::TriggerTooLong),
        RowError::Invalid(EntryError::TriggerWhitespace),
        RowError::Invalid(EntryError::TriggerControl),
        RowError::Invalid(EntryError::EmptyReplacement),
        RowError::Invalid(EntryError::ReplacementTooLong),
        RowError::Invalid(EntryError::ReplacementNul),
        RowError::TooMany,
        RowError::NoIdLeft,
    ];
    let texts: Vec<String> = errors.iter().map(|&error| row_error_text(error)).collect();
    for (index, text) in texts.iter().enumerate() {
        shown(text);
        assert!(!texts[..index].contains(text), "{text}");
    }
    assert_eq!(texts[0], "Hãy nhập từ gõ tắt.");
    assert!(texts[1].contains("32 ký tự"), "{}", texts[1]);
    assert!(texts[2].contains("khoảng trắng"), "{}", texts[2]);
    assert!(texts[3].contains("ký tự điều khiển"), "{}", texts[3]);
    assert_eq!(texts[4], "Hãy nhập nội dung thay thế.");
    assert!(
        texts[5].contains("nên chỉ được dài tối đa 4.096 ký tự (ít hơn nếu có nhiều chữ có dấu)"),
        "{}",
        texts[5]
    );
    assert!(texts[6].contains("NUL"), "{}", texts[6]);
    // Also right when a file written by hand holds more.
    assert!(
        texts[7].starts_with("Danh sách đã đạt số mục tối đa (20.000)"),
        "{}",
        texts[7]
    );
}

#[test]
fn load_errors_name_the_file_and_say_what_to_do() {
    let path = PathBuf::from(r"D:\Tools\Raumakey\raumakey-macros.toml");
    let corrupt = shown(&load_error_text(&path, ConfigError::Corrupt)).to_owned();
    assert_eq!(
        corrupt,
        "Không đọc được tệp gõ tắt D:\\Tools\\Raumakey\\raumakey-macros.toml: tệp bị hỏng hoặc \
         sai cú pháp TOML. Raumakey không mở danh sách để không ghi đè tệp này. Hãy sửa tệp bằng \
         tay rồi mở lại danh sách."
    );
    let newer = load_error_text(&path, ConfigError::UnsupportedVersion);
    assert!(newer.contains("phiên bản Raumakey mới hơn"), "{newer}");
    for error in [
        ConfigError::PermissionDenied,
        ConfigError::IoFailure,
        ConfigError::NotFound,
    ] {
        let text = load_error_text(&path, error);
        assert!(
            shown(&text).contains(r"D:\Tools\Raumakey\raumakey-macros.toml"),
            "{text}"
        );
        assert!(text.contains("Hãy thử mở lại danh sách sau."), "{text}");
    }
    // A real failure carries what the text needs.
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), "[[macro]\n").unwrap();
    let failure = MacroList::load(dir.path()).unwrap_err();
    let text = load_error_text(failure.path(), failure.error());
    assert!(
        text.contains(&macros_path(dir.path()).display().to_string()),
        "{text}"
    );
    assert!(text.contains("sửa tệp bằng tay"), "{text}");
}

#[test]
fn save_errors_name_the_file_and_a_conflict_asks_before_writing_over_it() {
    let path = PathBuf::from(r"D:\Tools\Raumakey\raumakey-macros.toml");
    let question = conflict_question(&path);
    assert!(
        shown(&question).starts_with(
            "Tệp gõ tắt D:\\Tools\\Raumakey\\raumakey-macros.toml vừa được thay đổi ở nơi khác"
        ),
        "{question}"
    );
    assert!(
        question.contains("(do sửa bằng tay hoặc do một chương trình khác)"),
        "{question}"
    );
    assert!(
        question.contains("Bạn có muốn ghi danh sách đang sửa đè lên tệp đó không?"),
        "{question}"
    );
    // The answers explain themselves: the labels of the buttons depend on the language of
    // Windows.
    assert!(!question.contains("Chọn"), "{question}");
    assert_eq!(save_error_text(&path, SaveError::Conflict), question);
    let corrupt = save_error_text(&path, SaveError::Store(ConfigError::Corrupt));
    assert!(corrupt.contains("Raumakey không ghi đè nó"), "{corrupt}");
    let too_large = save_error_text(&path, SaveError::TooLarge);
    assert!(too_large.contains("danh sách quá lớn"), "{too_large}");
    let errors = [
        SaveError::Lossy,
        SaveError::TooLarge,
        SaveError::NoIdLeft,
        SaveError::Store(ConfigError::Corrupt),
        SaveError::Store(ConfigError::UnsupportedVersion),
        SaveError::Store(ConfigError::PermissionDenied),
        SaveError::Store(ConfigError::IoFailure),
        SaveError::Store(ConfigError::NotFound),
    ];
    let texts: Vec<String> = errors
        .iter()
        .map(|&error| save_error_text(&path, error))
        .collect();
    for (index, text) in texts.iter().enumerate() {
        assert!(
            shown(text).starts_with(
                "Không lưu được danh sách gõ tắt vào D:\\Tools\\Raumakey\\raumakey-macros.toml"
            ),
            "{text}"
        );
        assert!(text.contains("vẫn còn trong hộp thoại"), "{text}");
        assert!(!texts[..index].contains(text), "{text}");
    }
    assert_eq!(
        shown(DISCARD_QUESTION),
        "Danh sách gõ tắt có thay đổi chưa được lưu. Đóng danh sách và bỏ các thay đổi này không?"
    );
}

#[test]
fn the_questions_about_values_that_would_be_lost_list_their_paths() {
    let path = PathBuf::from(r"D:\Tools\Raumakey\raumakey-macros.toml");
    let issues = [
        issue("macro", IssueReason::InvalidType),
        issue("macro[4].enabled", IssueReason::InvalidType),
        issue("macro[4].enabled", IssueReason::InvalidType),
        issue("macro[7].case", IssueReason::InvalidValue),
    ];
    assert_eq!(
        shown(&open_loss_question(&path, &issues)),
        "Một số giá trị trong D:\\Tools\\Raumakey\\raumakey-macros.toml không đọc được (macro, \
         macro[4].enabled, macro[7].case). Nếu lưu, các giá trị này sẽ bị bỏ hoặc thay bằng giá \
         trị mặc định. Bạn vẫn muốn mở danh sách gõ tắt không?"
    );
    assert_eq!(
        shown(&overwrite_loss_question(&path, &issues)),
        "Tệp gõ tắt D:\\Tools\\Raumakey\\raumakey-macros.toml hiện có một số giá trị không đọc \
         được (macro, macro[4].enabled, macro[7].case). Nếu ghi đè, các giá trị này sẽ bị bỏ \
         hoặc thay bằng giá trị mặc định. Bạn vẫn muốn ghi danh sách đang sửa đè lên tệp đó \
         không?"
    );
    // A long list is cut.
    let many: Vec<FieldIssue> = (0..9)
        .map(|n| issue(&format!("macro[{n}].case"), IssueReason::InvalidValue))
        .collect();
    let text = open_loss_question(&path, &many);
    assert!(
        text.contains(
            "(macro[0].case, macro[1].case, macro[2].case, macro[3].case, \
                       macro[4].case, macro[5].case và 3 mục khác)"
        ),
        "{text}"
    );
    // A real file: the paths name fields, never values.
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        macros_path(dir.path()),
        format!(
            "version = 1\nrevision = 1\n[[macro]]\nid = 1\ntrigger = \"{SECRET_TRIGGER}\"\n\
             replacement = \"{SECRET_REPLACEMENT}\"\nboundary = \"{SECRET_TRIGGER}\"\n"
        ),
    )
    .unwrap();
    let list = MacroList::load(dir.path()).unwrap();
    let text = open_loss_question(list.path(), list.lossy_issues());
    assert!(text.contains("(macro[0].boundary)"), "{text}");
    assert!(!text.contains(SECRET_TRIGGER), "{text}");
    assert!(!text.contains(SECRET_REPLACEMENT), "{text}");
}

#[test]
fn settings_take_the_saved_macros_at_their_next_poll() {
    let exe = tempfile::tempdir().unwrap();
    let local = tempfile::tempdir().unwrap();
    let (mut settings, _) = Settings::open(exe.path(), Some(local.path()), &|_| true, &|_| false);
    assert!(settings.macro_snapshot().entries.is_empty());
    let dir = settings
        .config_path()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    let mut list = MacroList::load(&dir).unwrap();
    list.put("ko", "không").unwrap();
    list.put(VIET_NFD, "Việt Nam").unwrap();
    assert_eq!(list.save(), Ok(Saved::Written));

    let reloaded = settings.poll().unwrap();
    let macros = reloaded.macros.unwrap();
    assert_eq!(macros.macro_revision, 1);
    assert_eq!(
        macros.entries,
        vec![entry(1, "ko", "không"), entry(2, VIET_NFC, "Việt Nam")]
    );
    assert_eq!(
        settings.status().macros,
        DocumentStatus::Loaded { issues: Vec::new() }
    );
    assert!(settings.status().skipped_macros.is_empty());
}
