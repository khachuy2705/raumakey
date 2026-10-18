//! The macro list editor dialog (`raumakey_controller::macro_dialog`, D44): its template, the
//! texts of its cells, edits, and labels, and what its buttons do with a `MacroList` and the
//! user's answers (host-independent, each test in its own temporary folder); on Windows, the
//! real dialog on a thread of its own.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use raumakey_config::{ConfigError, FieldIssue};
use raumakey_controller::macro_dialog::{
    APP_ICON_ID, CELL_SCALARS, COLUMNS, DIALOG_SIZE, HINT, ID_COUNT, ID_HINT, ID_LIST, ID_PUT,
    ID_REMOVE, ID_REPLACEMENT, ID_SAVE, ID_STATUS, ID_TRIGGER, LINE_BREAK_MARK, Placement, Prompt,
    PromptKind, REPLACEMENT_LIMIT, SaveOutcome, Status, TITLE, TRIGGER_LIMIT, cell_text,
    count_text, edit_text, entered_text, may_close, open_list, put_row, row_cells, save_list,
    status_text, template,
};
use raumakey_controller::macro_list::{
    DISCARD_QUESTION, MacroList, RowError, RowStatus, SaveError, conflict_question,
    load_error_text, open_loss_question, overwrite_loss_question, row_error_text, save_error_text,
};
use raumakey_macro::{EntryError, MAX_TRIGGER_SCALARS, normalize_replacement, normalize_trigger};
use raumakey_windows::dialog::MessageKind;
use raumakey_windows::dialog_template::{ControlClass, IDCANCEL, styles};

/// Common Controls 6 for the editor, as `raumakey.exe`'s manifest asks for it; shared with the
/// tests of `raumakey-windows`.
#[cfg(windows)]
#[path = "../../raumakey-windows/tests/layer/common_controls_v6.rs"]
mod common_controls_v6;

fn macros_path(dir: &Path) -> PathBuf {
    dir.join("raumakey-macros.toml")
}

/// A `raumakey-macros.toml` at revision 3 with `entries` (id, trigger, replacement, enabled).
fn macros_file(entries: &[(u64, &str, &str, bool)]) -> String {
    let mut text = String::from("version = 1\nrevision = 3\n");
    for (id, trigger, replacement, enabled) in entries {
        write!(
            text,
            "[[macro]]\nid = {id}\ntrigger = \"{trigger}\"\nreplacement = \"{replacement}\"\n\
             enabled = {enabled}\n"
        )
        .unwrap();
    }
    text
}

/// A file whose only entry has an `enabled` the store cannot read: a write would reset it.
const LOSSY_FILE: &str = "version = 1\nrevision = 3\n\
    [[macro]]\nid = 1\ntrigger = \"ko\"\nreplacement = \"không\"\nenabled = \"có\"\n";

/// Answers questions in turn (then "no") and records every prompt.
struct Script {
    answers: VecDeque<bool>,
    asked: RefCell<Vec<Prompt>>,
}

impl Script {
    fn answering(answers: &[bool]) -> Self {
        Self {
            answers: answers.iter().copied().collect(),
            asked: RefCell::new(Vec::new()),
        }
    }

    fn ask(&mut self, prompt: &Prompt) -> bool {
        self.asked.borrow_mut().push(prompt.clone());
        prompt.kind() == PromptKind::Question && self.answers.pop_front().unwrap_or(false)
    }

    fn asked(&self) -> Vec<(PromptKind, String)> {
        self.asked
            .borrow()
            .iter()
            .map(|prompt| (prompt.kind(), prompt.text().to_owned()))
            .collect()
    }
}

/// An asker that fails the test when asked.
fn never(prompt: &Prompt) -> bool {
    panic!(
        "nothing to ask, but asked {:?}: {}",
        prompt.kind(),
        prompt.text()
    )
}

fn issues_of(list: &MacroList) -> Vec<FieldIssue> {
    list.lossy_issues().to_vec()
}

// ---- The template ----

#[test]
fn the_template_lays_out_the_editor_with_its_controls_and_texts() {
    let template = template(Placement::Centered);
    assert_eq!(TITLE, "Raumakey – Danh sách gõ tắt");
    assert_eq!(template.title(), TITLE);
    assert_ne!(template.style() & styles::DS_CENTER, 0);
    assert_eq!(template.ex_style(), 0);
    template.build().unwrap();

    let controls = template.controls();
    let find = |id: u16| {
        controls
            .iter()
            .find(|control| control.id() == id)
            .unwrap_or_else(|| panic!("control {id}"))
    };
    let list = find(ID_LIST);
    assert_eq!(list.class(), ControlClass::ListView);
    assert_ne!(list.style() & styles::LVS_OWNERDATA, 0, "a virtual list");
    assert_ne!(list.style() & styles::LVS_SINGLESEL, 0);

    let trigger = find(ID_TRIGGER);
    assert_eq!(trigger.class(), ControlClass::Edit);
    assert_eq!(trigger.style() & styles::ES_MULTILINE, 0);
    let replacement = find(ID_REPLACEMENT);
    assert_eq!(replacement.class(), ControlClass::Edit);
    let multi = styles::ES_MULTILINE | styles::ES_WANTRETURN | styles::ES_AUTOVSCROLL;
    assert_eq!(replacement.style() & multi, multi);
    assert_ne!(replacement.style() & styles::WS_VSCROLL, 0);

    for (id, text) in [
        (ID_PUT, "Thêm / Sửa"),
        (ID_REMOVE, "Xoá"),
        (ID_SAVE, "Lưu"),
        (IDCANCEL, "Đóng"),
    ] {
        let button = find(id);
        assert_eq!(button.class(), ControlClass::Button, "{text}");
        assert_eq!(button.text(), text);
    }
    // Enter in the trigger edit adds or edits the row; Enter in the replacement is a line
    // break.
    assert_ne!(find(ID_PUT).style() & styles::BS_DEFPUSHBUTTON, 0);
    for id in [ID_COUNT, ID_STATUS] {
        assert_eq!(find(id).class(), ControlClass::Static);
    }
    let texts: Vec<&str> = controls.iter().map(|control| control.text()).collect();
    for text in ["Gõ tắt:", "Thay bằng:", HINT] {
        assert!(texts.contains(&text), "{text}");
    }
    // The hint has an id of its own: the editor sets it again, wrapped at spaces.
    assert_eq!(find(ID_HINT).class(), ControlClass::Static);
    assert_eq!(find(ID_HINT).text(), HINT);
    assert_eq!(
        HINT,
        "Chọn một dòng để sửa. Mục mới được bật sẵn; xoá rồi thêm lại một mục sẽ đặt lại các \
         tuỳ chọn của nó."
    );
    assert_eq!(
        COLUMNS.map(|(text, _)| text),
        ["Gõ tắt", "Thay bằng", "Trạng thái"]
    );

    // Tab order: the list, the two edits, then the buttons.
    let stops: Vec<u16> = controls
        .iter()
        .filter(|control| control.style() & styles::WS_TABSTOP != 0)
        .map(|control| control.id())
        .collect();
    assert_eq!(
        stops,
        [
            ID_LIST,
            ID_TRIGGER,
            ID_REPLACEMENT,
            ID_PUT,
            ID_REMOVE,
            ID_SAVE,
            IDCANCEL
        ]
    );
    // Every control lies inside the dialog.
    let (width, height) = DIALOG_SIZE;
    for control in controls {
        let rect = control.rect();
        assert!(rect.x >= 0 && rect.y >= 0, "{control:?}");
        assert!(rect.x + rect.cx <= width, "{control:?}");
        assert!(rect.y + rect.cy <= height, "{control:?}");
    }
}

#[test]
fn the_off_screen_variant_differs_only_in_position_and_activation() {
    let centered = template(Placement::Centered);
    let hidden = template(Placement::OffScreen);
    assert_eq!(hidden.style() & styles::DS_CENTER, 0);
    assert_eq!(hidden.ex_style(), styles::WS_EX_NOACTIVATE);
    assert_eq!(hidden.controls(), centered.controls());
    assert_eq!(hidden.title(), centered.title());
}

#[test]
fn the_icon_is_the_application_mark_of_raumakey_exe() {
    assert_eq!(APP_ICON_ID, raumakey_resources::ICON_APP);
}

// ---- Texts ----

#[test]
fn the_count_groups_thousands_with_dots() {
    assert_eq!(count_text(0), "0 mục");
    assert_eq!(count_text(1), "1 mục");
    assert_eq!(count_text(999), "999 mục");
    assert_eq!(count_text(20_000), "20.000 mục");
    assert_eq!(count_text(1_234_567), "1.234.567 mục");
}

#[test]
fn cells_show_line_breaks_as_a_mark_and_cut_long_texts() {
    assert_eq!(LINE_BREAK_MARK, '↵');
    assert_eq!(cell_text("Việt Nam"), "Việt Nam");
    assert_eq!(cell_text("một\nhai"), "một↵hai");
    assert_eq!(cell_text("một\r\nhai\rba"), "một↵hai↵ba");
    assert_eq!(cell_text("a\tb"), "a b");
    // Control characters, NUL among them, would garble or cut the cell.
    assert_eq!(cell_text("x\0y\u{7}z"), "x\u{FFFD}y\u{FFFD}z");
    let fits = "ệ".repeat(CELL_SCALARS);
    assert_eq!(cell_text(&fits), fits);
    let long = "ệ".repeat(CELL_SCALARS + 50);
    let cut = cell_text(&long);
    assert_eq!(cut.chars().count(), CELL_SCALARS + 1);
    assert!(cut.starts_with(&"ệ".repeat(CELL_SCALARS)));
    assert!(cut.ends_with('…'));
    // A line break counts as one character.
    let breaks = "\r\n".repeat(CELL_SCALARS);
    assert_eq!(cell_text(&breaks), "↵".repeat(CELL_SCALARS));
}

#[test]
fn rows_show_their_trigger_replacement_and_status() {
    let dir = tempfile::tempdir().unwrap();
    let text = macros_file(&[
        (1, "ko", "không\\nnhé", true),
        (2, "tat", "đang tắt", false),
        (3, "ko", "trùng", true),
    ]);
    fs::write(macros_path(dir.path()), text).unwrap();
    let list = MacroList::load(dir.path()).unwrap();
    let cells: Vec<Vec<String>> = list.rows().iter().map(row_cells).collect();
    assert_eq!(
        cells,
        [
            ["ko", "không↵nhé", RowStatus::Active.label()],
            ["tat", "đang tắt", RowStatus::Disabled.label()],
            ["ko", "trùng", RowStatus::Unused.label()],
        ]
        .map(|row| row.map(str::to_owned).to_vec())
    );
}

#[test]
fn edits_show_line_breaks_as_crlf_and_entered_texts_use_lf() {
    assert_eq!(edit_text("a\nb\r\nc\rd"), "a\r\nb\r\nc\r\nd");
    assert_eq!(edit_text("x\0y"), "x\u{FFFD}y");
    assert_eq!(entered_text("a\r\nb\rc\nd"), "a\nb\nc\nd");
    for text in ["", "không", "dòng một\ndòng hai\n", "\n\n", "Việt\tNam"] {
        assert_eq!(entered_text(&edit_text(text)), text, "{text:?}");
    }
    // By design, a `\r\n` or a lone `\r` of the file comes back as `\n` once its row is put
    // again: the edit holds every line break as `\r\n`.
    assert_eq!(entered_text(&edit_text("a\r\nb\rc")), "a\nb\nc");
}

/// The edits take every valid trigger and replacement, also typed decomposed or full of line
/// breaks (which the multi-line edit holds as `\r\n`).
#[test]
fn the_edit_limits_fit_every_valid_trigger_and_replacement() {
    let decomposed = "e\u{0323}\u{0302}".repeat(MAX_TRIGGER_SCALARS);
    assert!(normalize_trigger(&decomposed).is_ok());
    assert!(decomposed.encode_utf16().count() <= TRIGGER_LIMIT);
    for replacement in [
        "\n".repeat(4096),
        "😀".repeat(4095),
        "e\u{0323}\u{0302}".repeat(2730),
    ] {
        assert!(normalize_replacement(&replacement).is_ok());
        let shown = edit_text(&replacement);
        assert!(shown.encode_utf16().count() <= REPLACEMENT_LIMIT);
    }
}

#[test]
fn the_status_line_says_what_happened_to_the_rows() {
    assert_eq!(status_text(Status::Loaded), "");
    assert_eq!(status_text(Status::Modified), "Có thay đổi chưa lưu.");
    assert_eq!(status_text(Status::Saved), "Đã lưu.");
    assert_eq!(
        status_text(Status::NothingToSave),
        "Không có thay đổi nào để lưu."
    );
}

#[test]
fn prompts_map_to_message_boxes_and_debug_shows_no_text() {
    assert_eq!(PromptKind::Warning.message_kind(), MessageKind::Warning);
    assert_eq!(PromptKind::Error.message_kind(), MessageKind::Error);
    assert_eq!(PromptKind::Question.message_kind(), MessageKind::Question);
    let dir = tempfile::tempdir().unwrap();
    fs::write(macros_path(dir.path()), "không phải TOML [").unwrap();
    let mut script = Script::answering(&[]);
    assert!(
        open_list(MacroList::load(dir.path()), &mut |prompt| script
            .ask(prompt))
        .is_none()
    );
    let prompt = script.asked.borrow()[0].clone();
    let shown = format!("{prompt:?}");
    assert!(!shown.contains("raumakey-macros"), "{shown}");
    assert!(shown.contains("Error"), "{shown}");
}

// ---- Opening ----

#[test]
fn a_missing_file_opens_an_empty_list_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let list = open_list(MacroList::load(dir.path()), &mut never).unwrap();
    assert!(list.is_empty());
    assert!(!macros_path(dir.path()).exists());
}

#[test]
fn a_damaged_file_is_explained_and_the_editor_does_not_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = macros_path(dir.path());
    fs::write(&path, "không phải TOML [").unwrap();
    let mut script = Script::answering(&[true]);
    let opened = open_list(MacroList::load(dir.path()), &mut |prompt| {
        script.ask(prompt)
    });
    assert!(opened.is_none());
    assert_eq!(
        script.asked(),
        [(
            PromptKind::Error,
            load_error_text(&path, ConfigError::Corrupt)
        )]
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "không phải TOML [");
}

#[test]
fn values_the_store_cannot_read_are_asked_about_before_the_editor_opens() {
    let dir = tempfile::tempdir().unwrap();
    let path = macros_path(dir.path());
    fs::write(&path, LOSSY_FILE).unwrap();
    let issues = issues_of(&MacroList::load(dir.path()).unwrap());
    assert!(!issues.is_empty());
    let question = (PromptKind::Question, open_loss_question(&path, &issues));

    let mut script = Script::answering(&[false]);
    assert!(
        open_list(MacroList::load(dir.path()), &mut |prompt| script
            .ask(prompt))
        .is_none()
    );
    assert_eq!(script.asked(), std::slice::from_ref(&question));
    assert_eq!(fs::read_to_string(&path).unwrap(), LOSSY_FILE);

    let mut script = Script::answering(&[true]);
    let mut list = open_list(MacroList::load(dir.path()), &mut |prompt| {
        script.ask(prompt)
    })
    .unwrap();
    assert_eq!(script.asked(), [question]);
    // The loss is accepted: a save writes.
    assert_eq!(put_row(&mut list, "vn", "Việt Nam", &mut never), Ok(1));
    assert_eq!(save_list(&mut list, &mut never), SaveOutcome::Written);
}

// ---- "Thêm / Sửa" ----

#[test]
fn putting_a_row_returns_the_row_to_select() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(put_row(&mut list, "ko", "không", &mut never), Ok(0));
    assert_eq!(put_row(&mut list, "vn", "Việt\nNam", &mut never), Ok(1));
    // The same trigger edits its row.
    assert_eq!(put_row(&mut list, "ko", "không có", &mut never), Ok(0));
    assert_eq!(list.len(), 2);
    assert_eq!(list.rows()[0].replacement(), "không có");
    assert_eq!(list.rows()[1].replacement(), "Việt\nNam");
}

#[test]
fn a_refused_row_is_explained_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    for (trigger, replacement, error) in [
        ("", "x", EntryError::EmptyTrigger),
        ("a b", "x", EntryError::TriggerWhitespace),
        ("ab", "", EntryError::EmptyReplacement),
    ] {
        let mut script = Script::answering(&[true]);
        assert_eq!(
            put_row(&mut list, trigger, replacement, &mut |prompt| script
                .ask(prompt)),
            Err(RowError::Invalid(error))
        );
        assert_eq!(
            script.asked(),
            [(
                PromptKind::Warning,
                row_error_text(RowError::Invalid(error))
            )]
        );
    }
    assert!(list.is_empty());
    assert!(!list.is_modified());
}

// ---- "Lưu" ----

#[test]
fn saving_writes_the_rows_once_and_then_has_nothing_to_write() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert_eq!(save_list(&mut list, &mut never), SaveOutcome::Unchanged);
    put_row(&mut list, "ko", "không\nnhé", &mut never).unwrap();
    assert_eq!(save_list(&mut list, &mut never), SaveOutcome::Written);
    assert_eq!(save_list(&mut list, &mut never), SaveOutcome::Unchanged);
    let saved = MacroList::load(dir.path()).unwrap();
    assert_eq!(saved.rows()[0].replacement(), "không\nnhé");
}

#[test]
fn a_save_that_fails_is_explained_and_the_rows_stay() {
    let dir = tempfile::tempdir().unwrap();
    let path = macros_path(dir.path());
    fs::write(&path, macros_file(&[(1, "ko", "không", true)])).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    put_row(&mut list, "vn", "Việt Nam", &mut never).unwrap();
    // The file became damaged meanwhile: it is never written over.
    fs::write(&path, "hỏng [").unwrap();
    let mut script = Script::answering(&[true]);
    assert_eq!(
        save_list(&mut list, &mut |prompt| script.ask(prompt)),
        SaveOutcome::NotWritten
    );
    assert_eq!(
        script.asked(),
        [(
            PromptKind::Error,
            save_error_text(&path, SaveError::Store(ConfigError::Corrupt))
        )]
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "hỏng [");
    assert_eq!(list.len(), 2);
    assert!(list.is_modified());
}

#[test]
fn a_file_changed_elsewhere_is_written_over_only_after_yes() {
    let dir = tempfile::tempdir().unwrap();
    let path = macros_path(dir.path());
    fs::write(&path, macros_file(&[(1, "ko", "không", true)])).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    put_row(&mut list, "vn", "Việt Nam", &mut never).unwrap();
    // Another editor saves first.
    let mut other = MacroList::load(dir.path()).unwrap();
    put_row(&mut other, "hcm", "Hồ Chí Minh", &mut never).unwrap();
    assert_eq!(save_list(&mut other, &mut never), SaveOutcome::Written);
    let theirs = fs::read_to_string(&path).unwrap();

    let question = (PromptKind::Question, conflict_question(&path));
    let mut script = Script::answering(&[false]);
    assert_eq!(
        save_list(&mut list, &mut |prompt| script.ask(prompt)),
        SaveOutcome::NotWritten
    );
    assert_eq!(script.asked(), std::slice::from_ref(&question));
    assert_eq!(fs::read_to_string(&path).unwrap(), theirs);
    assert!(list.is_modified());

    let mut script = Script::answering(&[true]);
    assert_eq!(
        save_list(&mut list, &mut |prompt| script.ask(prompt)),
        SaveOutcome::Written
    );
    assert_eq!(script.asked(), [question]);
    let saved = MacroList::load(dir.path()).unwrap();
    let triggers: Vec<&str> = saved.rows().iter().map(|row| row.trigger()).collect();
    assert_eq!(triggers, ["ko", "vn"]);
}

/// The file written elsewhere holds values the store cannot read: writing over it asks about
/// losing them, and a "no" is asked again at the next save (the user agreed to write over that
/// file already).
#[test]
fn writing_over_unreadable_values_asks_about_losing_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = macros_path(dir.path());
    fs::write(&path, macros_file(&[(1, "ko", "không", true)])).unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    put_row(&mut list, "vn", "Việt Nam", &mut never).unwrap();
    let lossy = LOSSY_FILE.replace("revision = 3", "revision = 4");
    fs::write(&path, &lossy).unwrap();

    let mut script = Script::answering(&[true, false]);
    assert_eq!(
        save_list(&mut list, &mut |prompt| script.ask(prompt)),
        SaveOutcome::NotWritten
    );
    let issues = issues_of(&list);
    assert!(!issues.is_empty());
    let loss = (
        PromptKind::Question,
        overwrite_loss_question(&path, &issues),
    );
    assert_eq!(
        script.asked(),
        [
            (PromptKind::Question, conflict_question(&path)),
            loss.clone()
        ]
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), lossy);

    let mut script = Script::answering(&[true]);
    assert_eq!(
        save_list(&mut list, &mut |prompt| script.ask(prompt)),
        SaveOutcome::Written
    );
    assert_eq!(script.asked(), [loss]);
    assert_eq!(MacroList::load(dir.path()).unwrap().len(), 2);
}

// ---- "Đóng" ----

#[test]
fn closing_asks_only_about_unsaved_changes() {
    let dir = tempfile::tempdir().unwrap();
    let mut list = MacroList::load(dir.path()).unwrap();
    assert!(may_close(Some(&list), &mut never));
    assert!(may_close(None, &mut never));
    put_row(&mut list, "ko", "không", &mut never).unwrap();
    let question = (PromptKind::Question, DISCARD_QUESTION.to_owned());
    let mut script = Script::answering(&[false]);
    assert!(!may_close(Some(&list), &mut |prompt| script.ask(prompt)));
    assert_eq!(script.asked(), std::slice::from_ref(&question));
    let mut script = Script::answering(&[true]);
    assert!(may_close(Some(&list), &mut |prompt| script.ask(prompt)));
    assert_eq!(script.asked(), [question]);
}

/// The real editor on a thread of its own, in a temporary data folder. It is placed far
/// off-screen and never activated ([`Placement::OffScreen`], and the fake prompter moves it
/// again before Windows shows it), and its questions are answered from a script by the fake
/// prompter, so no message box is ever shown. It is driven from the test thread with
/// `SendMessageTimeoutW` (`WM_SETTEXT` on the edits, `BM_CLICK` on the buttons,
/// `LVM_SETITEMSTATE` on the list), which returns once the handler is done; every wait has a
/// timeout.
#[cfg(windows)]
mod win {
    use std::collections::VecDeque;
    use std::fs;
    use std::path::Path;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use raumakey_config::ConfigError;
    use raumakey_controller::macro_dialog::{
        ID_COUNT, ID_LIST, ID_PUT, ID_REMOVE, ID_REPLACEMENT, ID_SAVE, ID_STATUS, ID_TRIGGER,
        Placement, Prompt, PromptKind, Prompter, TITLE, spawn_macro_list_with,
    };
    use raumakey_controller::macro_list::{
        DISCARD_QUESTION, MacroList, RowError, conflict_question, load_error_text,
        open_loss_question, row_error_text,
    };
    use raumakey_macro::EntryError;
    use raumakey_windows::dialog_box::{DetachedDialog, Dialog};
    use raumakey_windows::dialog_template::IDCANCEL;
    use raumakey_windows::strings::until_nul;
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::Controls::{
        LVIS_SELECTED, LVITEMW, LVM_GETITEMCOUNT, LVM_GETITEMTEXTW, LVM_GETNEXTITEM,
        LVM_SETITEMSTATE, LVNI_SELECTED,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_CLICK, GetDlgItem, IsWindow, IsWindowVisible, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOSIZE,
        SWP_NOZORDER, SendMessageTimeoutW, SetWindowPos, WM_CHAR, WM_GETTEXT, WM_GETTEXTLENGTH,
        WM_SETTEXT,
    };
    use windows::core::PWSTR;

    use super::common_controls_v6::{CCM_GETVERSION, CommonControls6};
    use super::{LOSSY_FILE, macros_file, macros_path};

    const WAIT: Duration = Duration::from_secs(30);
    /// Milliseconds a sent message may take before the test fails.
    const SEND_TIMEOUT_MS: u32 = 30_000;

    /// What the editor did, reported from its thread.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        /// The dialog exists, filled and not shown yet.
        Created(isize),
        /// A prompt, and whether the editor was visible then.
        Asked {
            kind: PromptKind,
            text: String,
            visible: bool,
        },
        Saved,
        Closed,
    }

    type Answers = Arc<Mutex<VecDeque<bool>>>;

    /// Moves the dialog off-screen before it is shown, and answers questions from `answers`
    /// (then "no") without showing anything.
    struct FakePrompter {
        events: Sender<Event>,
        answers: Answers,
    }

    fn is_visible(hwnd: HWND) -> bool {
        // SAFETY: only reads the window state.
        unsafe { IsWindowVisible(hwnd) }.as_bool()
    }

    impl Prompter for FakePrompter {
        fn created(&mut self, dialog: &Dialog<'_>) {
            // SAFETY: a window of the calling thread; no pointers.
            let _ = unsafe {
                SetWindowPos(
                    dialog.hwnd(),
                    None,
                    -32_000,
                    -32_000,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            };
            let _ = self.events.send(Event::Created(dialog.hwnd().0 as isize));
        }

        fn ask(&mut self, dialog: &Dialog<'_>, prompt: &Prompt) -> bool {
            let _ = self.events.send(Event::Asked {
                kind: prompt.kind(),
                text: prompt.text().to_owned(),
                visible: is_visible(dialog.hwnd()),
            });
            prompt.kind() == PromptKind::Question
                && self.answers.lock().unwrap().pop_front().unwrap_or(false)
        }
    }

    /// The editor of a folder, as the test sees it.
    struct Editor {
        dialog: DetachedDialog,
        events: Receiver<Event>,
        answers: Answers,
        closings: Arc<AtomicUsize>,
    }

    impl Drop for Editor {
        fn drop(&mut self) {
            // A failed test leaves no dialog behind.
            self.dialog.dismiss();
        }
    }

    /// Opens the editor on `dir`, answering its questions with `answers` in turn.
    fn spawn(dir: &Path, answers: &[bool]) -> Editor {
        let (sender, events) = mpsc::channel();
        let answers: Answers = Arc::new(Mutex::new(answers.iter().copied().collect()));
        let closings = Arc::new(AtomicUsize::new(0));
        let prompter = FakePrompter {
            events: sender.clone(),
            answers: Arc::clone(&answers),
        };
        let saved = sender.clone();
        let closed = Arc::clone(&closings);
        let dialog = spawn_macro_list_with(
            Placement::OffScreen,
            prompter,
            dir.to_path_buf(),
            move || {
                let _ = saved.send(Event::Saved);
            },
            move || {
                closed.fetch_add(1, Ordering::SeqCst);
                let _ = sender.send(Event::Closed);
            },
        )
        .unwrap();
        Editor {
            dialog,
            events,
            answers,
            closings,
        }
    }

    impl Editor {
        fn next(&self) -> Event {
            self.events
                .recv_timeout(WAIT)
                .expect("the editor reported in time")
        }

        /// Waits for the dialog to be shown (far off-screen).
        fn shown(&self) -> HWND {
            let Event::Created(hwnd) = self.next() else {
                panic!("the editor started with its dialog");
            };
            let hwnd = HWND(hwnd as *mut core::ffi::c_void);
            let deadline = Instant::now() + WAIT;
            while !is_visible(hwnd) {
                assert!(Instant::now() < deadline, "the editor was shown");
                std::thread::sleep(Duration::from_millis(5));
            }
            hwnd
        }

        /// The events that are waiting (the handler is done with what was sent).
        fn waiting(&self) -> Vec<Event> {
            self.events.try_iter().collect()
        }

        /// Waits for `on_closed` and returns the events before it.
        fn closed(&self) -> Vec<Event> {
            let mut before = Vec::new();
            loop {
                match self.next() {
                    Event::Closed => break,
                    other => before.push(other),
                }
            }
            assert_eq!(self.closings.load(Ordering::SeqCst), 1);
            assert!(self.dialog.is_closed());
            before
        }

        fn answer(&self, answers: &[bool]) {
            self.answers.lock().unwrap().extend(answers);
        }
    }

    /// Sends `message` and returns the answer; fails the test when it is not answered in time.
    fn send(hwnd: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
        let mut result = 0_usize;
        // SAFETY: the pointers passed in `lparam` by the callers point to data of this thread
        // that outlives the call (the receiver is in this process); `result` is a valid out
        // pointer.
        let answered = unsafe {
            SendMessageTimeoutW(
                hwnd,
                message,
                WPARAM(wparam),
                LPARAM(lparam),
                SMTO_NORMAL,
                SEND_TIMEOUT_MS,
                Some(&mut result),
            )
        };
        assert_ne!(answered.0, 0, "message {message:#x} was answered in time");
        result as isize
    }

    fn item(dialog: HWND, id: u16) -> HWND {
        // SAFETY: only reads the child window list.
        unsafe { GetDlgItem(Some(dialog), i32::from(id)) }.unwrap()
    }

    fn text(hwnd: HWND) -> String {
        let length = send(hwnd, WM_GETTEXTLENGTH, 0, 0) as usize;
        let mut buffer = vec![0_u16; length + 1];
        let copied = send(hwnd, WM_GETTEXT, buffer.len(), buffer.as_mut_ptr() as isize) as usize;
        String::from_utf16(&buffer[..copied]).unwrap()
    }

    fn label(dialog: HWND, id: u16) -> String {
        text(item(dialog, id))
    }

    fn type_into(dialog: HWND, id: u16, typed: &str) {
        let units: Vec<u16> = typed.encode_utf16().chain([0]).collect();
        send(item(dialog, id), WM_SETTEXT, 0, units.as_ptr() as isize);
    }

    fn click(dialog: HWND, id: u16) {
        send(item(dialog, id), BM_CLICK, 0, 0);
    }

    fn is_enabled(dialog: HWND, id: u16) -> bool {
        // SAFETY: only reads the window state.
        unsafe { IsWindowEnabled(item(dialog, id)) }.as_bool()
    }

    fn row_count(dialog: HWND) -> isize {
        send(item(dialog, ID_LIST), LVM_GETITEMCOUNT, 0, 0)
    }

    /// The text of a cell, as the list view copies it into a buffer of 256 units
    /// (`LVM_GETITEMTEXTW`; every cell fits, see `CELL_SCALARS`). The answer is not the length
    /// copied: Common Controls 6 answers the length of the whole cell, so the copy ends at its
    /// NUL.
    fn cell(dialog: HWND, row: usize, column: i32) -> String {
        let mut buffer = [0_u16; 256];
        let mut cell = LVITEMW {
            iSubItem: column,
            pszText: PWSTR(buffer.as_mut_ptr()),
            cchTextMax: buffer.len() as i32,
            ..Default::default()
        };
        let list = item(dialog, ID_LIST);
        send(list, LVM_GETITEMTEXTW, row, &raw mut cell as isize);
        String::from_utf16(until_nul(&buffer)).unwrap()
    }

    fn selected(dialog: HWND) -> isize {
        send(
            item(dialog, ID_LIST),
            LVM_GETNEXTITEM,
            usize::MAX,
            LVNI_SELECTED as isize,
        )
    }

    /// Selects `row` the way a click does, so that the list view notifies the dialog.
    fn select(dialog: HWND, row: usize) {
        let mut state = LVITEMW {
            state: LVIS_SELECTED,
            stateMask: LVIS_SELECTED,
            ..Default::default()
        };
        send(
            item(dialog, ID_LIST),
            LVM_SETITEMSTATE,
            row,
            &raw mut state as isize,
        );
    }

    /// Types `typed` into the list as the keyboard would (`WM_CHAR`).
    fn type_into_list(dialog: HWND, typed: &str) {
        for unit in typed.encode_utf16() {
            send(item(dialog, ID_LIST), WM_CHAR, usize::from(unit), 1);
        }
    }

    /// Opens the editor on `dir` under Common Controls 6, as `raumakey.exe` shows it (only
    /// version 6 searches the list as the user types into it), and waits until it is shown.
    fn spawn_v6(dir: &Path) -> (Editor, HWND) {
        let editor = {
            // The editor's thread inherits the context; the guard may go once it started.
            let _v6 = CommonControls6::activate();
            spawn(dir, &[])
        };
        let hwnd = editor.shown();
        assert_eq!(
            send(item(hwnd, ID_LIST), CCM_GETVERSION, 0, 0),
            6,
            "Common Controls 6"
        );
        (editor, hwnd)
    }

    fn asked(kind: PromptKind, text: &str, visible: bool) -> Event {
        Event::Asked {
            kind,
            text: text.to_owned(),
            visible,
        }
    }

    #[test]
    fn the_editor_shows_the_rows_of_the_file_and_closes_without_asking() {
        let dir = tempfile::tempdir().unwrap();
        let file = macros_file(&[
            (1, "ko", "không\\nnhé", true),
            (2, "tat", "đang tắt", false),
            (3, "ko", "trùng", true),
        ]);
        fs::write(macros_path(dir.path()), &file).unwrap();
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        assert_eq!(text(hwnd), TITLE);
        assert_eq!(row_count(hwnd), 3);
        let rows: Vec<[String; 3]> = (0..3)
            .map(|row| [0, 1, 2].map(|column| cell(hwnd, row, column)))
            .collect();
        assert_eq!(
            rows,
            [
                ["ko", "không↵nhé", ""],
                ["tat", "đang tắt", "đang tắt"],
                ["ko", "trùng", "không dùng"],
            ]
            .map(|row| row.map(str::to_owned))
        );
        assert_eq!(label(hwnd, ID_COUNT), "3 mục");
        assert_eq!(label(hwnd, ID_STATUS), "");
        assert_eq!(selected(hwnd), -1);
        assert!(!is_enabled(hwnd, ID_REMOVE));
        click(hwnd, IDCANCEL);
        assert_eq!(editor.closed(), []);
        assert_eq!(fs::read_to_string(macros_path(dir.path())).unwrap(), file);
    }

    #[test]
    fn a_row_added_and_saved_reaches_the_file_and_on_saved() {
        let dir = tempfile::tempdir().unwrap();
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        assert_eq!(row_count(hwnd), 0);
        assert_eq!(label(hwnd, ID_COUNT), "0 mục");
        type_into(hwnd, ID_TRIGGER, "ko");
        type_into(hwnd, ID_REPLACEMENT, "không\r\nnhé");
        click(hwnd, ID_PUT);
        assert_eq!(row_count(hwnd), 1);
        assert_eq!(cell(hwnd, 0, 0), "ko");
        assert_eq!(cell(hwnd, 0, 1), "không↵nhé");
        assert_eq!(selected(hwnd), 0);
        assert!(is_enabled(hwnd, ID_REMOVE));
        assert_eq!(label(hwnd, ID_COUNT), "1 mục");
        assert_eq!(label(hwnd, ID_STATUS), "Có thay đổi chưa lưu.");
        assert!(!macros_path(dir.path()).exists());

        click(hwnd, ID_SAVE);
        assert_eq!(editor.waiting(), [Event::Saved]);
        assert_eq!(label(hwnd, ID_STATUS), "Đã lưu.");
        let saved = MacroList::load(dir.path()).unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved.rows()[0].trigger(), "ko");
        assert_eq!(saved.rows()[0].replacement(), "không\nnhé");
        // The edits keep what was typed.
        assert_eq!(label(hwnd, ID_REPLACEMENT), "không\r\nnhé");

        click(hwnd, ID_SAVE);
        assert_eq!(editor.waiting(), []);
        assert_eq!(label(hwnd, ID_STATUS), "Không có thay đổi nào để lưu.");
        // Nothing unsaved: closing asks nothing.
        click(hwnd, IDCANCEL);
        assert_eq!(editor.closed(), []);
    }

    #[test]
    fn selecting_a_row_fills_the_edits_and_xoa_removes_it() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            macros_path(dir.path()),
            macros_file(&[(1, "ko", "một\\ndòng", true), (2, "vn", "Việt Nam", true)]),
        )
        .unwrap();
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        select(hwnd, 0);
        assert_eq!(label(hwnd, ID_TRIGGER), "ko");
        assert_eq!(label(hwnd, ID_REPLACEMENT), "một\r\ndòng");
        assert!(is_enabled(hwnd, ID_REMOVE));
        select(hwnd, 1);
        assert_eq!(label(hwnd, ID_TRIGGER), "vn");
        assert_eq!(label(hwnd, ID_REPLACEMENT), "Việt Nam");

        click(hwnd, ID_REMOVE);
        assert_eq!(row_count(hwnd), 1);
        assert_eq!(cell(hwnd, 0, 0), "ko");
        assert_eq!(selected(hwnd), -1);
        assert!(!is_enabled(hwnd, ID_REMOVE));
        assert_eq!(label(hwnd, ID_COUNT), "1 mục");
        assert_eq!(label(hwnd, ID_STATUS), "Có thay đổi chưa lưu.");
        // The removed row stays in the edits: "Thêm / Sửa" puts it back, as a new row.
        assert_eq!(label(hwnd, ID_TRIGGER), "vn");
        click(hwnd, ID_PUT);
        assert_eq!(row_count(hwnd), 2);
        assert_eq!(cell(hwnd, 1, 1), "Việt Nam");
        assert_eq!(selected(hwnd), 1);
        assert_eq!(editor.waiting(), []);
    }

    /// Typing into the list selects the next row whose trigger starts with the letters typed,
    /// which fills the edits as a click on it would; letters that lead to no other row change
    /// neither the selection nor what the user typed into the edits. Never the first row by
    /// accident.
    #[test]
    fn typing_into_the_list_selects_the_matching_row_and_keeps_the_edits_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            macros_path(dir.path()),
            macros_file(&[
                (1, "ab", "một", true),
                (2, "vn", "Việt Nam", true),
                (3, "ko", "không", true),
            ]),
        )
        .unwrap();

        let (editor, hwnd) = spawn_v6(dir.path());
        assert_eq!(selected(hwnd), -1);
        type_into_list(hwnd, "k");
        assert_eq!(selected(hwnd), 2);
        assert_eq!(label(hwnd, ID_TRIGGER), "ko");
        assert_eq!(label(hwnd, ID_REPLACEMENT), "không");
        assert!(is_enabled(hwnd, ID_REMOVE));
        assert_eq!(editor.waiting(), []);
        drop(editor);

        let (editor, hwnd) = spawn_v6(dir.path());
        select(hwnd, 1);
        type_into(hwnd, ID_REPLACEMENT, "Việt Nam đang gõ dở");
        // "v" finds the selected row again, and "vq" (typed quickly) no row.
        for typed in ["v", "q"] {
            type_into_list(hwnd, typed);
            assert_eq!(selected(hwnd), 1, "{typed}");
            assert_eq!(label(hwnd, ID_TRIGGER), "vn");
            assert_eq!(label(hwnd, ID_REPLACEMENT), "Việt Nam đang gõ dở");
        }
        assert_eq!(row_count(hwnd), 3);
        assert_eq!(editor.waiting(), []);
    }

    #[test]
    fn a_refused_row_is_explained_and_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        type_into(hwnd, ID_TRIGGER, "a b");
        type_into(hwnd, ID_REPLACEMENT, "x");
        click(hwnd, ID_PUT);
        let error = row_error_text(RowError::Invalid(EntryError::TriggerWhitespace));
        assert_eq!(editor.waiting(), [asked(PromptKind::Warning, &error, true)]);
        assert_eq!(row_count(hwnd), 0);
        assert_eq!(label(hwnd, ID_STATUS), "");
        assert_eq!(label(hwnd, ID_TRIGGER), "a b");
    }

    #[test]
    fn a_file_changed_elsewhere_is_asked_about_and_yes_writes_over_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = macros_path(dir.path());
        fs::write(&path, macros_file(&[(1, "ko", "không", true)])).unwrap();
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        type_into(hwnd, ID_TRIGGER, "vn");
        type_into(hwnd, ID_REPLACEMENT, "Việt Nam");
        click(hwnd, ID_PUT);
        // Another editor saves first.
        let mut other = MacroList::load(dir.path()).unwrap();
        other.put("hcm", "Hồ Chí Minh").unwrap();
        other.save().unwrap();

        editor.answer(&[true]);
        click(hwnd, ID_SAVE);
        assert_eq!(
            editor.waiting(),
            [
                asked(PromptKind::Question, &conflict_question(&path), true),
                Event::Saved
            ]
        );
        let saved = MacroList::load(dir.path()).unwrap();
        let triggers: Vec<&str> = saved.rows().iter().map(|row| row.trigger()).collect();
        assert_eq!(triggers, ["ko", "vn"]);
        assert_eq!(label(hwnd, ID_STATUS), "Đã lưu.");
        assert_eq!(row_count(hwnd), 2);
    }

    #[test]
    fn closing_with_unsaved_rows_asks_and_dismissing_does_not() {
        let dir = tempfile::tempdir().unwrap();
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        type_into(hwnd, ID_TRIGGER, "ko");
        type_into(hwnd, ID_REPLACEMENT, "không");
        click(hwnd, ID_PUT);
        click(hwnd, IDCANCEL);
        let question = asked(PromptKind::Question, DISCARD_QUESTION, true);
        assert_eq!(editor.waiting(), std::slice::from_ref(&question));
        // SAFETY: only queries whether the handle names a window.
        assert!(unsafe { IsWindow(Some(hwnd)) }.as_bool(), "still open");
        editor.answer(&[true]);
        click(hwnd, IDCANCEL);
        assert_eq!(editor.closed(), [question]);
        assert!(!macros_path(dir.path()).exists());

        // The tray's exit dismisses the editor: no question, nothing saved.
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        type_into(hwnd, ID_TRIGGER, "ko");
        type_into(hwnd, ID_REPLACEMENT, "không");
        click(hwnd, ID_PUT);
        editor.dialog.dismiss();
        assert_eq!(editor.closed(), []);
        assert!(!macros_path(dir.path()).exists());
    }

    #[test]
    fn a_damaged_file_is_explained_and_the_editor_never_shows() {
        let dir = tempfile::tempdir().unwrap();
        let path = macros_path(dir.path());
        fs::write(&path, "không phải TOML [").unwrap();
        let editor = spawn(dir.path(), &[true]);
        let events = editor.closed();
        assert!(matches!(events[0], Event::Created(_)), "{events:?}");
        assert_eq!(
            events[1..],
            [asked(
                PromptKind::Error,
                &load_error_text(&path, ConfigError::Corrupt),
                false
            )]
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), "không phải TOML [");
    }

    #[test]
    fn unreadable_values_are_asked_about_before_the_editor_shows() {
        let dir = tempfile::tempdir().unwrap();
        let path = macros_path(dir.path());
        fs::write(&path, LOSSY_FILE).unwrap();
        let issues = MacroList::load(dir.path()).unwrap().lossy_issues().to_vec();
        let question = asked(
            PromptKind::Question,
            &open_loss_question(&path, &issues),
            false,
        );

        let editor = spawn(dir.path(), &[false]);
        let events = editor.closed();
        assert_eq!(&events[1..], std::slice::from_ref(&question), "{events:?}");
        assert_eq!(fs::read_to_string(&path).unwrap(), LOSSY_FILE);

        let editor = spawn(dir.path(), &[true]);
        let hwnd = editor.shown();
        assert_eq!(editor.waiting(), [question]);
        assert_eq!(row_count(hwnd), 1);
        type_into(hwnd, ID_TRIGGER, "vn");
        type_into(hwnd, ID_REPLACEMENT, "Việt Nam");
        click(hwnd, ID_PUT);
        click(hwnd, ID_SAVE);
        assert_eq!(editor.waiting(), [Event::Saved]);
        assert_eq!(MacroList::load(dir.path()).unwrap().len(), 2);
    }

    /// 20,000 rows, the most a list holds: the editor opens and changes quickly (the times
    /// are printed; the bound only catches a list view filled row by row).
    #[test]
    fn twenty_thousand_rows_open_and_change_quickly() {
        let dir = tempfile::tempdir().unwrap();
        let mut file = String::from("version = 1\nrevision = 3\n");
        for id in 1..=20_000 {
            file.push_str(&format!(
                "[[macro]]\nid = {id}\ntrigger = \"t{id}\"\nreplacement = \"Thay thế số {id}\"\n"
            ));
        }
        fs::write(macros_path(dir.path()), file).unwrap();
        let start = Instant::now();
        let editor = spawn(dir.path(), &[]);
        let hwnd = editor.shown();
        let opened = start.elapsed();
        assert_eq!(row_count(hwnd), 20_000);
        assert_eq!(label(hwnd, ID_COUNT), "20.000 mục");
        assert_eq!(cell(hwnd, 19_999, 1), "Thay thế số 20000");

        let start = Instant::now();
        select(hwnd, 19_999);
        type_into(hwnd, ID_REPLACEMENT, "đã sửa");
        click(hwnd, ID_PUT);
        let edited = start.elapsed();
        assert_eq!(cell(hwnd, 19_999, 1), "đã sửa");
        assert_eq!(selected(hwnd), 19_999);

        let start = Instant::now();
        select(hwnd, 0);
        click(hwnd, ID_REMOVE);
        let removed = start.elapsed();
        assert_eq!(row_count(hwnd), 19_999);

        let start = Instant::now();
        click(hwnd, ID_SAVE);
        let saved = start.elapsed();
        assert_eq!(editor.waiting(), [Event::Saved]);
        eprintln!(
            "20,000 rows: opened in {opened:?}, edited in {edited:?}, removed in {removed:?}, \
             saved in {saved:?}"
        );
        for elapsed in [opened, edited, removed, saved] {
            assert!(elapsed < Duration::from_secs(20), "{elapsed:?}");
        }
    }

    /// Opens the real editor with real message boxes on `RAUMAKEY_MACROS_DIR` (or a new
    /// temporary folder) and waits until it is closed. Manual test.
    #[test]
    #[ignore = "manual: shows the macro list editor and its message boxes until you close it"]
    fn manual_macro_list_editor() {
        let temporary = tempfile::tempdir().unwrap();
        let dir = std::env::var_os("RAUMAKEY_MACROS_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| temporary.path().to_path_buf());
        let (sender, closed) = mpsc::channel();
        let dialog = raumakey_controller::macro_dialog::spawn_macro_list(
            dir,
            || eprintln!("on_saved"),
            move || {
                let _ = sender.send(());
            },
        )
        .unwrap();
        closed.recv().unwrap();
        assert!(dialog.is_closed());
    }
}
