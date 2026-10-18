//! The macro list editor (D44, design §22.2): the "Danh sách gõ tắt" dialog over a
//! [`MacroList`], which reads and writes `raumakey-macros.toml` on the dialog's own thread.
//!
//! The layout, the texts, and every decision are pure and tested on every host:
//!
//! - [`template`]: Raumakey's own layout. A virtual report list with the columns
//!   [`COLUMNS`] ("Gõ tắt", "Thay bằng", "Trạng thái"), the row count ([`count_text`]) and a
//!   status line ([`status_text`]) under it, the [`HINT`], the trigger in a single-line edit
//!   and the replacement in a multi-line one (replacements may hold line breaks; Enter there
//!   is a line break, while Enter in the trigger presses "Thêm / Sửa", the default button),
//!   and the buttons "Thêm / Sửa", "Xoá", "Lưu", and "Đóng" (`IDCANCEL`, as Esc and the close
//!   box). [`Placement::OffScreen`] is the variant the tests show.
//! - What the list and the edits show: [`row_cells`] (the trigger, the replacement, and
//!   [`RowStatus::label`](crate::macro_list::RowStatus::label)), [`cell_text`] (line breaks as
//!   [`LINE_BREAK_MARK`], control characters replaced, long texts cut to [`CELL_SCALARS`]: for
//!   display only), [`edit_text`] and [`entered_text`] (the edits hold line breaks as `\r\n`,
//!   the model as `\n`), and the edits' limits ([`TRIGGER_LIMIT`], [`REPLACEMENT_LIMIT`]),
//!   which every valid trigger and replacement fits, typed decomposed or not. So a `\r\n` or a
//!   lone `\r` that the file holds (written by hand) comes back as `\n` once its row is put
//!   again with "Thêm / Sửa", by design: the edits cannot tell the line breaks apart.
//! - What the buttons do, over a [`MacroList`] and an `ask` callback that shows a [`Prompt`]
//!   and returns whether the user answered "yes" (the dialog shows them in message boxes it
//!   owns): [`open_list`] (a load error is shown and the editor does not open; values the
//!   store could not read are asked about first), [`put_row`] ("Thêm / Sửa"), [`save_list`]
//!   ("Lưu", with the conflict and loss questions), and [`may_close`] ("Đóng", Esc, the close
//!   box: unsaved changes are asked about). "Xoá" is [`MacroList::remove`].
//!
//! On Windows, `MacroListDialog` is the dialog's handler and `spawn_macro_list` shows it on a
//! thread of its own (`raumakey_windows::dialog_box::spawn_dialog`), so the caller (the tray)
//! never waits for it:
//!
//! - **Opening.** `init` sets the icon of `raumakey.exe` ([`APP_ICON_ID`]; a failure is
//!   ignored), the columns, the [`HINT`] wrapped at spaces to its label's width (Windows' own
//!   word wrap would cut a Vietnamese word; `Dialog::set_text_wrapped`), and the edits'
//!   limits, and loads the list: file I/O on the dialog's own thread, as design §22.2 has it
//!   (see `MacroListDialog` for how long the editor may then stop answering; nothing else
//!   waits for that thread). When the load failed or left values it could not read, `init`
//!   posts itself a private message, which arrives before the dialog is first shown (a dialog
//!   box shows itself once its queue first runs empty): the box is shown then, owned by the
//!   hidden dialog, so that dismissing the dialog closes it, and on an error or a "no" the
//!   dialog ends without ever being shown.
//! - **Rows.** The list view is virtual (`LVS_OWNERDATA`): after every change the handler
//!   hands all rows to the dialog (`Dialog::list_set_virtual_rows`), which only stores them
//!   and sets the count, so 20,000 rows open and update at once, and the cells are drawn also
//!   while a message box is open. Selecting a row (or double-clicking it) fills the two edits
//!   with it and enables "Xoá"; "Thêm / Sửa" selects the row it added or edited; "Xoá" removes
//!   the selected row and leaves its texts in the edits, so that "Thêm / Sửa" puts it back
//!   (as a new row, with the default settings). Only a selection that the user changes fills
//!   the edits: those the handler makes itself do not reach it. Typing into the list selects
//!   the next row whose trigger starts with the letters typed, ignoring case (the dialog
//!   answers the list view's search, `LVN_ODFINDITEMW`, from the rows it keeps), and so fills
//!   the edits as a click on that row would; letters that lead to no other row keep the
//!   selection, and so what the user typed into the edits.
//! - **Saving.** After each write ("Lưu", or "yes" to writing over a changed file),
//!   `on_saved` runs on the dialog's thread (the tray then asks the worker to poll at once,
//!   so the typing hook gets the new macros). After every message box, the controls are
//!   filled again from the model.
//! - **Closing.** "Đóng", Esc, and the close box ask [`DISCARD_QUESTION`] when rows are not
//!   saved; `DetachedDialog::dismiss` (the tray's exit) closes the dialog and its box without
//!   asking and without saving. `on_closed` runs exactly once when the dialog ends, also when
//!   it never opened.
//!
//! The dialog's questions go through a `Prompter`: `MessageBoxes` (boxes owned by the dialog,
//! `Dialog::message_box`) is the real one; the tests answer from a script without showing
//! anything.
//!
//! Nothing here logs. The texts of the prompts are built from paths, schema paths, limits, and
//! error kinds ([`crate::macro_list`]), never from macro text; the `Debug` output of
//! [`Prompt`] shows its kind and length only, and that of the handler no macro text.
//!
//! # Manual check list
//!
//! A successful build and the tests are no evidence that the real dialog behaves; check on a
//! test desktop with `raumakey.exe` (or the ignored test `manual_macro_list_editor` of
//! `tests/macro_dialog.rs`, with `RAUMAKEY_MACROS_DIR` naming a folder):
//!
//! 1. The editor opens centered with Raumakey's icon in its title bar and taskbar button, the
//!    title "Raumakey – Danh sách gõ tắt", and the rows of `raumakey-macros.toml`; disabled
//!    and unusable rows say so in "Trạng thái"; a replacement with line breaks shows `↵`.
//! 2. Click a row: both edits show it, line breaks as lines; "Xoá" is enabled. Edit the
//!    replacement, add a line with Enter, press "Thêm / Sửa": the row changes in place.
//!    Type a new trigger and press Enter in it: a row is added at the end and selected.
//!    Change the replacement without pressing "Thêm / Sửa", click into the list, and type a
//!    letter that no trigger starts with: the selection and the edits stay as they were (the
//!    list never jumps to its first row). Type the first letter of another row's trigger
//!    (in either case): that row is selected and shown in the edits.
//! 3. A trigger with a space, or empty edits: a warning box owned by the editor, in
//!    Vietnamese; nothing changes.
//! 4. "Lưu": "Đã lưu."; within a moment the new macro expands in Notepad. "Lưu" again:
//!    "Không có thay đổi nào để lưu."
//! 5. Edit the file in Notepad and save it while the editor has unsaved rows, then "Lưu": the
//!    conflict question; "Không" keeps the file, "Có" writes the editor's rows over it.
//! 6. "Đóng" (and Esc, and the close box) with unsaved rows: the discard question; "Không"
//!    keeps the editor open.
//! 7. Break the TOML of `raumakey-macros.toml` and open the editor: only an error box, no
//!    editor, and the file is unchanged. Give an entry an `enabled` that is no boolean and
//!    open the editor: first the question about the values it cannot read; "Không" opens
//!    nothing and leaves the file as it is. Both boxes are owned by the editor, which is not
//!    shown yet and has no taskbar button, so neither has one either: click another window,
//!    then bring the box back with Alt+Tab, or with "Danh sách gõ tắt…" of the panel (which
//!    brings the editor that is opening, and so its box, to the front).
//! 8. With 20,000 macros the editor opens within about a second and scrolls smoothly; adding
//!    and removing stay quick, and "Lưu" leaves the editor unresponsive for at most a second
//!    or two.
//! 9. With a box of the editor open, choose "Thoát" in the tray menu: the box and the editor
//!    close, and nothing is saved. The same while the question about unreadable values is
//!    open, before the editor was ever shown: the question goes, and no editor appears.

use core::fmt;

use raumakey_macro::{EntryError, MAX_TRIGGER_SCALARS};
use raumakey_model::transaction::MAX_INSERT_UNITS;
use raumakey_windows::dialog::MessageKind;
use raumakey_windows::dialog_template::{
    Control, DialogRect, DialogTemplate, IDCANCEL, STATIC_ID, styles,
};

use crate::macro_list::{
    DISCARD_QUESTION, LoadError, MacroList, Put, Row, RowError, SaveError, conflict_question,
    grouped, load_error_text, open_loss_question, overwrite_loss_question, row_error_text,
    save_error_text,
};
use crate::worker::Saved;

/// The dialog's title.
pub const TITLE: &str = "Raumakey – Danh sách gõ tắt";
/// The dialog's width and height in dialog units.
pub const DIALOG_SIZE: (i16, i16) = (320, 262);

/// The list of rows (a virtual list view).
pub const ID_LIST: u16 = 101;
/// The edit of the trigger ("Gõ tắt").
pub const ID_TRIGGER: u16 = 102;
/// The multi-line edit of the replacement ("Thay bằng").
pub const ID_REPLACEMENT: u16 = 103;
/// "Thêm / Sửa", the default button.
pub const ID_PUT: u16 = 104;
/// "Xoá".
pub const ID_REMOVE: u16 = 105;
/// "Lưu".
pub const ID_SAVE: u16 = 106;
/// The label with the number of rows ([`count_text`]).
pub const ID_COUNT: u16 = 107;
/// The status line ([`status_text`]).
pub const ID_STATUS: u16 = 108;
/// The label with the [`HINT`], on two lines.
pub const ID_HINT: u16 = 109;

/// The columns of the list: header and width in dialog units.
pub const COLUMNS: [(&str, i16); 3] = [("Gõ tắt", 70), ("Thay bằng", 166), ("Trạng thái", 56)];
/// The hint under the list.
pub const HINT: &str = "Chọn một dòng để sửa. Mục mới được bật sẵn; xoá rồi thêm lại một mục sẽ \
                        đặt lại các tuỳ chọn của\u{a0}nó.";

/// The icon group of the Raumakey application mark in the resources of `raumakey.exe`
/// (`raumakey_resources::ICON_APP`, which the tests compare).
pub const APP_ICON_ID: u16 = 1;

/// The most UTF-16 units the trigger edit takes: every valid trigger fits
/// ([`MAX_TRIGGER_SCALARS`] scalars after NFC), also typed decomposed (a Vietnamese letter
/// decomposes into at most three scalars) or with characters outside the BMP.
pub const TRIGGER_LIMIT: usize = 4 * MAX_TRIGGER_SCALARS;
/// The most UTF-16 units the replacement edit takes: every valid replacement fits (it fits one
/// engine insert, [`MAX_INSERT_UNITS`], in NFC and NFD), also when each of its characters is a
/// line break, which the edit holds as `\r\n`.
pub const REPLACEMENT_LIMIT: usize = 2 * MAX_INSERT_UNITS;

/// What a cell shows for a line break of the replacement (display only).
pub const LINE_BREAK_MARK: char = '↵';
/// The most characters a cell shows; a longer text is cut and ends with `…` (display only).
pub const CELL_SCALARS: usize = 100;

/// Where the dialog appears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Placement {
    /// Centered on its monitor, as the user sees it.
    Centered,
    /// Far off-screen and never activated (`WS_EX_NOACTIVATE`, which also drops the taskbar
    /// button), for tests; Windows moves a dialog that starts off-screen back onto the screen,
    /// so the tests move it again before it is shown.
    OffScreen,
}

fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
    DialogRect::new(x, y, cx, cy)
}

/// The dialog's template (see the [module documentation](self)).
pub fn template(placement: Placement) -> DialogTemplate {
    let (width, height) = DIALOG_SIZE;
    let template = DialogTemplate::new(TITLE, width, height);
    let template = match placement {
        Placement::Centered => template,
        Placement::OffScreen => template
            .at(-30_000, -30_000)
            .with_ex_style(styles::WS_EX_NOACTIVATE),
    };
    template
        .with_control(Control::virtual_list_view(ID_LIST, rect(7, 7, 306, 128)))
        .with_control(
            Control::label(ID_COUNT, "", rect(7, 139, 100, 8)).with_style(styles::SS_NOPREFIX),
        )
        .with_control(
            Control::label(ID_STATUS, "", rect(110, 139, 203, 8)).with_style(styles::SS_NOPREFIX),
        )
        .with_control(
            Control::label(ID_HINT, HINT, rect(7, 151, 306, 16)).with_style(styles::SS_NOPREFIX),
        )
        .with_control(Control::label(STATIC_ID, "Gõ tắt:", rect(7, 173, 44, 8)))
        .with_control(Control::edit(ID_TRIGGER, rect(54, 171, 120, 12)))
        .with_control(Control::label(STATIC_ID, "Thay bằng:", rect(7, 189, 44, 8)))
        .with_control(Control::multiline_edit(
            ID_REPLACEMENT,
            rect(54, 187, 259, 46),
        ))
        .with_control(Control::default_push_button(
            ID_PUT,
            "Thêm / Sửa",
            rect(54, 241, 58, 14),
        ))
        .with_control(Control::push_button(
            ID_REMOVE,
            "Xoá",
            rect(116, 241, 50, 14),
        ))
        .with_control(Control::push_button(ID_SAVE, "Lưu", rect(209, 241, 50, 14)))
        .with_control(Control::push_button(
            IDCANCEL,
            "Đóng",
            rect(263, 241, 50, 14),
        ))
}

/// `20.000 mục`: the number of rows.
pub fn count_text(rows: usize) -> String {
    format!("{} mục", grouped(rows))
}

/// What happened to the rows last, for the status line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    /// As loaded or last saved, and not changed since: nothing to say.
    Loaded,
    /// A row was added, changed, or removed since.
    Modified,
    /// "Lưu" wrote the rows.
    Saved,
    /// "Lưu" found nothing to write.
    NothingToSave,
}

/// The text of the status line.
pub const fn status_text(status: Status) -> &'static str {
    match status {
        Status::Loaded => "",
        Status::Modified => "Có thay đổi chưa lưu.",
        Status::Saved => "Đã lưu.",
        Status::NothingToSave => "Không có thay đổi nào để lưu.",
    }
}

/// `text` as a cell shows it: each line break (`\r\n`, `\r`, `\n`) as [`LINE_BREAK_MARK`], a
/// tab as a space, any other control character (NUL among them, which would cut the cell) as
/// U+FFFD, and cut after [`CELL_SCALARS`] characters with `…`. For display only: the rows keep
/// their texts.
pub fn cell_text(text: &str) -> String {
    let mut shown = String::with_capacity(text.len().min(CELL_SCALARS * 4));
    let mut count = 0_usize;
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        if count == CELL_SCALARS {
            shown.push('…');
            break;
        }
        let char = match char {
            '\r' => {
                chars.next_if_eq(&'\n');
                LINE_BREAK_MARK
            }
            '\n' => LINE_BREAK_MARK,
            '\t' => ' ',
            control if control.is_control() => '\u{FFFD}',
            other => other,
        };
        shown.push(char);
        count += 1;
    }
    shown
}

/// The cells of `row`: its trigger, its replacement ([`cell_text`]), and its status
/// ([`RowStatus::label`](crate::macro_list::RowStatus::label)).
pub fn row_cells(row: &Row) -> Vec<String> {
    vec![
        cell_text(row.trigger()),
        cell_text(row.replacement()),
        row.status().label().to_owned(),
    ]
}

/// `text` as an edit shows it: each line break (`\r\n`, `\r`, `\n`) as `\r\n`, the line break
/// of an edit, and NUL (which an edit cannot hold) as U+FFFD.
pub fn edit_text(text: &str) -> String {
    let mut shown = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        match char {
            '\r' => {
                chars.next_if_eq(&'\n');
                shown.push_str("\r\n");
            }
            '\n' => shown.push_str("\r\n"),
            '\0' => shown.push('\u{FFFD}'),
            other => shown.push(other),
        }
    }
    shown
}

/// The text of an edit as the model takes it: each line break (`\r\n`, or a lone `\r`) as
/// `\n`. With [`edit_text`], a replacement whose file holds `\r\n` or a lone `\r` comes back
/// with `\n` once its row is put again, by design.
pub fn entered_text(text: &str) -> String {
    let mut entered = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(char) = chars.next() {
        if char == '\r' {
            chars.next_if_eq(&'\n');
            entered.push('\n');
        } else {
            entered.push(char);
        }
    }
    entered
}

/// How a [`Prompt`] is shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PromptKind {
    /// A row was refused: the user fixes the edits ("OK").
    Warning,
    /// The list could not be opened or saved ("OK").
    Error,
    /// A question: "yes" or "no".
    Question,
}

impl PromptKind {
    /// The message box that shows it.
    pub const fn message_kind(self) -> MessageKind {
        match self {
            Self::Warning => MessageKind::Warning,
            Self::Error => MessageKind::Error,
            Self::Question => MessageKind::Question,
        }
    }
}

/// What the editor tells or asks the user: a text of [`crate::macro_list`] and how to show
/// it. Its `Debug` output shows the kind and the length of the text only (it names paths).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Prompt {
    kind: PromptKind,
    text: String,
}

impl fmt::Debug for Prompt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Prompt")
            .field("kind", &self.kind)
            .field("text_len", &self.text.len())
            .finish()
    }
}

impl Prompt {
    /// A prompt of `kind` with `text` (the exclusions dialog, `crate::exclusions_dialog`,
    /// asks through the same `Prompter`).
    pub(crate) fn new(kind: PromptKind, text: String) -> Self {
        Self { kind, text }
    }

    /// How it is shown.
    pub fn kind(&self) -> PromptKind {
        self.kind
    }

    /// The text, in Vietnamese.
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Shows a [`Prompt`] and returns whether the user answered "yes" (`false` for a warning or an
/// error, and when no answer came: the box could not be shown, or the dialog was dismissed).
pub type Ask<'a> = dyn FnMut(&Prompt) -> bool + 'a;

/// The editor opens on `loaded` (the result of [`MacroList::load`]): `None`, after showing
/// [`load_error_text`], when the file could not be read; values the store could not read
/// ([`MacroList::lossy_issues`]) are asked about first ([`open_loss_question`]), and only a
/// "yes" opens the editor, accepting their loss ([`confirm_open`]).
pub fn open_list(loaded: Result<MacroList, LoadError>, ask: &mut Ask<'_>) -> Option<MacroList> {
    match loaded {
        Err(error) => {
            ask(&load_error_prompt(&error));
            None
        }
        Ok(mut list) => confirm_open(&mut list, ask).then_some(list),
    }
}

/// The error shown when the list could not be loaded ([`load_error_text`]).
pub fn load_error_prompt(error: &LoadError) -> Prompt {
    Prompt::new(
        PromptKind::Error,
        load_error_text(error.path(), error.error()),
    )
}

/// Whether the editor opens on the loaded `list`: at once when the store read everything;
/// otherwise after "yes" to [`open_loss_question`], which accepts the loss
/// ([`MacroList::accept_loss`]).
pub fn confirm_open(list: &mut MacroList, ask: &mut Ask<'_>) -> bool {
    if list.lossy_issues().is_empty() {
        return true;
    }
    let question = Prompt::new(
        PromptKind::Question,
        open_loss_question(list.path(), list.lossy_issues()),
    );
    let yes = ask(&question);
    if yes {
        list.accept_loss();
    }
    yes
}

/// "Thêm / Sửa": puts `trigger` and `replacement` (the [`entered_text`] of the two edits) with
/// [`MacroList::put`], and returns the row to select (the one added or edited). A refused row
/// is explained ([`row_error_text`], a warning) and nothing changes.
pub fn put_row(
    list: &mut MacroList,
    trigger: &str,
    replacement: &str,
    ask: &mut Ask<'_>,
) -> Result<usize, RowError> {
    match list.put(trigger, replacement) {
        Ok(Put::Added(index) | Put::Updated(index)) => Ok(index),
        Err(error) => {
            ask(&Prompt::new(PromptKind::Warning, row_error_text(error)));
            Err(error)
        }
    }
}

/// Whether the refused row is to be fixed in the trigger edit (else in the replacement, or in
/// neither for [`RowError::TooMany`] and [`RowError::NoIdLeft`]).
pub fn is_trigger_error(error: RowError) -> bool {
    matches!(
        error,
        RowError::Invalid(
            EntryError::EmptyTrigger
                | EntryError::TriggerTooLong
                | EntryError::TriggerWhitespace
                | EntryError::TriggerControl
        )
    )
}

/// Whether the refused row is to be fixed in the replacement edit.
pub fn is_replacement_error(error: RowError) -> bool {
    matches!(error, RowError::Invalid(_)) && !is_trigger_error(error)
}

/// What "Lưu" did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SaveOutcome {
    /// The file now holds the rows.
    Written,
    /// No row changed since the load or the last save: nothing was written.
    Unchanged,
    /// Nothing was written: the user declined to write over the file, or the write failed
    /// (the error was shown). The rows stay.
    NotWritten,
}

/// The step that [`save_list`] tries next.
#[derive(Clone, Copy)]
enum Step {
    Save,
    Overwrite,
}

/// "Lưu": [`MacroList::save`], and what its errors ask:
///
/// - [`SaveError::Conflict`] (the file changed elsewhere): [`conflict_question`]; "yes"
///   writes over it ([`MacroList::overwrite`]), which may find it changed once more (asked
///   again);
/// - [`SaveError::Lossy`] (the file to write over holds values the store could not read,
///   whose loss the user has not accepted): [`overwrite_loss_question`]; "yes" accepts the
///   loss and tries the same step again;
/// - any other error is shown ([`save_error_text`]).
///
/// A "no" ends with [`SaveOutcome::NotWritten`]; the rows stay in every case but a write.
pub fn save_list(list: &mut MacroList, ask: &mut Ask<'_>) -> SaveOutcome {
    let mut step = Step::Save;
    loop {
        let result = match step {
            Step::Save => list.save(),
            Step::Overwrite => list.overwrite().map(|()| Saved::Written),
        };
        let (question, lossy) = match result {
            Ok(Saved::Written) => return SaveOutcome::Written,
            Ok(Saved::Unchanged) => return SaveOutcome::Unchanged,
            Err(SaveError::Conflict) => {
                step = Step::Overwrite;
                (conflict_question(list.path()), false)
            }
            Err(SaveError::Lossy) => (
                overwrite_loss_question(list.path(), list.lossy_issues()),
                true,
            ),
            Err(error) => {
                ask(&Prompt::new(
                    PromptKind::Error,
                    save_error_text(list.path(), error),
                ));
                return SaveOutcome::NotWritten;
            }
        };
        if !ask(&Prompt::new(PromptKind::Question, question)) {
            return SaveOutcome::NotWritten;
        }
        if lossy {
            list.accept_loss();
        }
    }
}

#[cfg(windows)]
pub use win::{MacroListDialog, MessageBoxes, Prompter, spawn_macro_list, spawn_macro_list_with};

#[cfg(windows)]
mod win {
    use core::fmt;
    use std::path::PathBuf;

    use raumakey_windows::dialog::MessageResponse;
    use raumakey_windows::dialog_box::{
        DetachedDialog, Dialog, DialogHandler, notifications, spawn_dialog,
    };
    use raumakey_windows::dialog_template::IDCANCEL;
    use raumakey_windows::error::WinResult;
    use raumakey_windows::file_dialog::FileFilter;
    use raumakey_windows::module::Module;
    use raumakey_windows::panic::guard_value;
    use raumakey_windows::window::AppMessage;

    use super::{
        APP_ICON_ID, COLUMNS, HINT, ID_COUNT, ID_HINT, ID_LIST, ID_PUT, ID_REMOVE, ID_REPLACEMENT,
        ID_SAVE, ID_STATUS, ID_TRIGGER, Placement, Prompt, REPLACEMENT_LIMIT, SaveOutcome, Status,
        TRIGGER_LIMIT, confirm_open, count_text, edit_text, entered_text, is_replacement_error,
        is_trigger_error, load_error_prompt, may_close, put_row, row_cells, save_list, status_text,
        template,
    };
    use crate::macro_list::{LoadError, MacroList};

    /// The offset of the private message that `init` posts to its own dialog when the loaded
    /// list must be explained or asked about before the dialog is shown.
    const OPENING: u32 = 1;

    /// How the editor and the exclusions dialog (`crate::exclusions_dialog`) tell and ask the
    /// user: [`MessageBoxes`] for real; the tests answer from a script. Its methods run on the
    /// dialog's thread, inside the handler.
    pub trait Prompter: Send {
        /// The dialog was created and filled, and is not shown yet. Nothing by default; the
        /// tests move it off-screen here.
        fn created(&mut self, dialog: &Dialog<'_>) {
            let _ = dialog;
        }

        /// Shows `prompt` owned by `dialog` and returns whether the user answered "yes":
        /// `false` for a warning or an error, and when no answer came (the box could not be
        /// shown, or the dialog was dismissed while it was open).
        fn ask(&mut self, dialog: &Dialog<'_>, prompt: &Prompt) -> bool;

        /// Shows the Open dialog for one existing file of the types `filters`, owned by
        /// `dialog`, and returns the path chosen, or `None` when the user cancelled. Fails
        /// when the Open dialog could not be shown, and with `E_ABORT`
        /// (`WinErrorKind::Cancelled`) when `dialog` was dismissed meanwhile
        /// ([`Dialog::choose_file`]). `Ok(None)` by default, without showing anything (the
        /// tests answer from a script).
        fn choose_file(
            &mut self,
            dialog: &Dialog<'_>,
            filters: &[FileFilter<'_>],
        ) -> WinResult<Option<PathBuf>> {
            let _ = (dialog, filters);
            Ok(None)
        }
    }

    /// The real [`Prompter`]: a message box owned by the dialog
    /// ([`Dialog::message_box`]), and the Open dialog owned by it ([`Dialog::choose_file`]),
    /// whose modal loops run inside the handler; dismissing the dialog closes them.
    #[derive(Clone, Copy, Debug, Default)]
    pub struct MessageBoxes;

    impl Prompter for MessageBoxes {
        fn ask(&mut self, dialog: &Dialog<'_>, prompt: &Prompt) -> bool {
            matches!(
                dialog.message_box(prompt.text(), prompt.kind().message_kind()),
                Ok(MessageResponse::Yes)
            )
        }

        fn choose_file(
            &mut self,
            dialog: &Dialog<'_>,
            filters: &[FileFilter<'_>],
        ) -> WinResult<Option<PathBuf>> {
            dialog.choose_file(filters)
        }
    }

    /// The handler of the editor (see the [module documentation](super)): it owns the
    /// [`MacroList`] of the data folder, loaded at `init`, and fills the controls from it
    /// after every change and every prompt.
    ///
    /// It reads and writes `raumakey-macros.toml` on the dialog's own UI thread, inside its
    /// callbacks (the load in `init`; "Lưu", and "yes" to writing over a changed file), which
    /// design §22.2 accepts: the editor stops answering meanwhile, for the store's wait for
    /// its lock (up to two seconds while another writer commits) plus the read or the write (a
    /// save of 20,000 rows takes about 1.5–1.7 s in a debug build). Only the editor waits:
    /// nothing else waits for its thread, and the tray only posts to it.
    pub struct MacroListDialog<P: Prompter> {
        folder: PathBuf,
        /// The list, once loaded; `None` when it could not be, or the user declined to open
        /// it.
        list: Option<MacroList>,
        /// Why the list could not be loaded, until it was shown.
        load_error: Option<LoadError>,
        /// `init` left something to explain or ask before the dialog is shown.
        opening: bool,
        prompter: P,
        on_saved: Box<dyn FnMut() + Send>,
        /// The selected row, as the handler last saw or set it.
        selected: Option<usize>,
        status: Status,
    }

    impl<P: Prompter> fmt::Debug for MacroListDialog<P> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            // No macro text: the list shows its path, revision, and counts only.
            f.debug_struct("MacroListDialog")
                .field("list", &self.list)
                .field(
                    "load_error",
                    &self.load_error.as_ref().map(LoadError::error),
                )
                .field("selected", &self.selected)
                .field("status", &self.status)
                .finish_non_exhaustive()
        }
    }

    impl<P: Prompter> MacroListDialog<P> {
        /// The editor of the macros in the data folder `folder` (where `raumakey-macros.toml`
        /// is, beside `raumakey.toml`), asking through `prompter`; `on_saved` runs on the
        /// dialog's thread after each write of the file. Nothing is loaded before `init`.
        pub fn new(folder: PathBuf, prompter: P, on_saved: impl FnMut() + Send + 'static) -> Self {
            Self {
                folder,
                list: None,
                load_error: None,
                opening: false,
                prompter,
                on_saved: Box::new(on_saved),
                selected: None,
                status: Status::Loaded,
            }
        }

        /// Explains or asks about the loaded list ([`load_error_prompt`], [`confirm_open`]),
        /// once; ends the dialog, never shown, unless the list opens.
        fn open(&mut self, dialog: &Dialog<'_>) {
            if !core::mem::take(&mut self.opening) {
                return;
            }
            let Self {
                prompter,
                list,
                load_error,
                ..
            } = self;
            let mut ask = |prompt: &Prompt| prompter.ask(dialog, prompt);
            let opens = match (load_error.take(), list.as_mut()) {
                (Some(error), _) => {
                    ask(&load_error_prompt(&error));
                    false
                }
                (None, Some(list)) => confirm_open(list, &mut ask),
                (None, None) => false,
            };
            if opens {
                self.refresh(dialog);
            } else {
                self.list = None;
                dialog.end(IDCANCEL as isize);
            }
        }

        /// "Thêm / Sửa": puts the texts of the edits; selects the row added or edited and
        /// shows it in the edits, or moves the focus to the edit to fix.
        fn put(&mut self, dialog: &Dialog<'_>) {
            let (Ok(trigger), Ok(replacement)) =
                (dialog.text(ID_TRIGGER), dialog.text(ID_REPLACEMENT))
            else {
                return;
            };
            let Self { prompter, list, .. } = self;
            let Some(list) = list.as_mut() else {
                return;
            };
            let put = put_row(
                list,
                &entered_text(&trigger),
                &entered_text(&replacement),
                &mut |prompt| prompter.ask(dialog, prompt),
            );
            match put {
                Ok(index) => {
                    self.selected = Some(index);
                    self.note_change();
                    self.refresh(dialog);
                    self.fill_edits(dialog, index);
                }
                Err(error) => {
                    self.refresh(dialog);
                    if is_trigger_error(error) {
                        dialog.focus(ID_TRIGGER);
                    } else if is_replacement_error(error) {
                        dialog.focus(ID_REPLACEMENT);
                    }
                }
            }
        }

        /// "Xoá": removes the selected row; its texts stay in the edits.
        fn remove(&mut self, dialog: &Dialog<'_>) {
            let (Some(list), Some(index)) = (self.list.as_mut(), dialog.list_selected(ID_LIST))
            else {
                return;
            };
            if list.remove(index) {
                self.selected = None;
                self.note_change();
                self.refresh(dialog);
            }
        }

        /// "Lưu" ([`save_list`]); `on_saved` runs after a write.
        fn save(&mut self, dialog: &Dialog<'_>) {
            let Self { prompter, list, .. } = self;
            let Some(list) = list.as_mut() else {
                return;
            };
            let outcome = save_list(list, &mut |prompt| prompter.ask(dialog, prompt));
            match outcome {
                SaveOutcome::Written => {
                    self.status = Status::Saved;
                    // A panic of the caller's callback must not leave the controls stale.
                    guard_value((), &mut self.on_saved);
                }
                SaveOutcome::Unchanged => self.status = Status::NothingToSave,
                SaveOutcome::NotWritten => self.note_change(),
            }
            self.refresh(dialog);
        }

        /// The status after a change of the rows: "not saved" while the list differs from
        /// the file.
        fn note_change(&mut self) {
            if self.list.as_ref().is_some_and(MacroList::is_modified) {
                self.status = Status::Modified;
            }
        }

        /// Shows row `index` in the two edits.
        fn fill_edits(&self, dialog: &Dialog<'_>, index: usize) {
            let Some(row) = self.list.as_ref().and_then(|list| list.rows().get(index)) else {
                return;
            };
            // `edit_text` leaves no NUL, so neither can fail.
            let _ = dialog.set_text(ID_TRIGGER, &edit_text(row.trigger()));
            let _ = dialog.set_text(ID_REPLACEMENT, &edit_text(row.replacement()));
        }

        /// Fills the list, the selection, the labels, and "Xoá" from the model.
        fn refresh(&mut self, dialog: &Dialog<'_>) {
            let rows: Vec<Vec<String>> = self
                .list
                .as_ref()
                .map(|list| list.rows().iter().map(row_cells).collect())
                .unwrap_or_default();
            // The cells hold no NUL (`cell_text`); a failure leaves the rows shown before.
            let _ = dialog.list_set_virtual_rows(ID_LIST, &rows);
            self.selected = self.selected.filter(|&index| index < rows.len());
            let _ = dialog.list_select(ID_LIST, self.selected);
            let _ = dialog.set_text(ID_COUNT, &count_text(rows.len()));
            let _ = dialog.set_text(ID_STATUS, status_text(self.status));
            dialog.set_enabled(ID_REMOVE, self.selected.is_some());
        }
    }

    impl<P: Prompter> DialogHandler for MacroListDialog<P> {
        fn init(&mut self, dialog: &Dialog<'_>) {
            // The icon of `raumakey.exe`; another executable (a test) has none, and the
            // editor works without it.
            if let Ok(module) = Module::executable() {
                let _ = dialog.set_icons(module, APP_ICON_ID);
            }
            let _ = dialog.list_set_columns(ID_LIST, &COLUMNS);
            // Wrapped at spaces, since Windows would cut a Vietnamese word; a hint that
            // cannot be measured stays as the template has it.
            let _ = dialog.set_text_wrapped(ID_HINT, HINT);
            dialog.set_text_limit(ID_TRIGGER, TRIGGER_LIMIT);
            dialog.set_text_limit(ID_REPLACEMENT, REPLACEMENT_LIMIT);
            match MacroList::load(&self.folder) {
                Ok(list) => {
                    self.opening = !list.lossy_issues().is_empty();
                    self.list = Some(list);
                }
                Err(error) => {
                    self.load_error = Some(error);
                    self.opening = true;
                }
            }
            self.refresh(dialog);
            if self.list.as_ref().is_some_and(MacroList::is_empty) {
                dialog.focus(ID_TRIGGER);
            }
            self.prompter.created(dialog);
            if self.opening {
                let posted =
                    AppMessage::new(OPENING).is_some_and(|opening| dialog.post(opening, 0, 0));
                if !posted {
                    // Asked now, before the window is published: a dismiss waits for it.
                    self.open(dialog);
                }
            }
        }

        fn command(&mut self, dialog: &Dialog<'_>, id: u16, code: u16) {
            if code != notifications::BN_CLICKED {
                return;
            }
            match id {
                ID_PUT => self.put(dialog),
                ID_REMOVE => self.remove(dialog),
                ID_SAVE => self.save(dialog),
                _ => {}
            }
        }

        fn notify(&mut self, dialog: &Dialog<'_>, id: u16, code: i32) {
            if id != ID_LIST {
                return;
            }
            match code {
                notifications::LVN_ITEMCHANGED => {
                    // Reported for each row that changes state; only a new selection fills
                    // the edits, so that typing is never overwritten by a repaint.
                    let selected = dialog.list_selected(ID_LIST);
                    if selected != self.selected {
                        self.selected = selected;
                        if let Some(index) = selected {
                            self.fill_edits(dialog, index);
                        }
                        dialog.set_enabled(ID_REMOVE, selected.is_some());
                    }
                }
                notifications::NM_DBLCLK => {
                    if let Some(index) = dialog.list_selected(ID_LIST) {
                        self.fill_edits(dialog, index);
                        dialog.focus(ID_REPLACEMENT);
                    }
                }
                _ => {}
            }
        }

        fn app_message(
            &mut self,
            dialog: &Dialog<'_>,
            message: AppMessage,
            _wparam: usize,
            _lparam: isize,
        ) {
            if message.offset() == OPENING {
                self.open(dialog);
            }
        }

        fn close_requested(&mut self, dialog: &Dialog<'_>) -> bool {
            let Self { prompter, list, .. } = self;
            may_close(list.as_ref(), &mut |prompt| prompter.ask(dialog, prompt))
        }
    }

    /// Shows the macro list editor of the data folder `folder` (the folder of `raumakey.toml`,
    /// where `raumakey-macros.toml` is or will be) on a thread of its own and returns at once
    /// (see the [module documentation](super)). `on_saved` runs on that thread after each
    /// write of the file; `on_closed` runs there exactly once when the editor ends: closed by
    /// the user, dismissed ([`DetachedDialog::dismiss`]), or never opened (a load error, a
    /// "no" to the question about unreadable values, or a dialog that could not be created).
    /// Fails when the thread cannot start; neither callback runs then.
    pub fn spawn_macro_list(
        folder: PathBuf,
        on_saved: impl FnMut() + Send + 'static,
        on_closed: impl FnOnce() + Send + 'static,
    ) -> WinResult<DetachedDialog> {
        spawn_macro_list_with(
            Placement::Centered,
            MessageBoxes,
            folder,
            on_saved,
            on_closed,
        )
    }

    /// [`spawn_macro_list`] at `placement`, asking through `prompter`: the tests show the
    /// editor off-screen ([`Placement::OffScreen`]) and answer from a script.
    pub fn spawn_macro_list_with<P: Prompter + 'static>(
        placement: Placement,
        prompter: P,
        folder: PathBuf,
        on_saved: impl FnMut() + Send + 'static,
        on_closed: impl FnOnce() + Send + 'static,
    ) -> WinResult<DetachedDialog> {
        let template = template(placement).build()?;
        let handler = MacroListDialog::new(folder, prompter, on_saved);
        spawn_dialog(template, handler, move |_| on_closed())
    }
}

/// "Đóng", Esc, or the close box: whether the editor may close. Without unsaved rows (or
/// without a list) at once; otherwise after "yes" to [`DISCARD_QUESTION`].
pub fn may_close(list: Option<&MacroList>, ask: &mut Ask<'_>) -> bool {
    match list {
        Some(list) if list.is_modified() => ask(&Prompt::new(
            PromptKind::Question,
            DISCARD_QUESTION.to_owned(),
        )),
        _ => true,
    }
}
