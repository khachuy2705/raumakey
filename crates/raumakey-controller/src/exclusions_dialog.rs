//! The exclusions dialog (D48): "Raumakey – Ứng dụng loại trừ", which manages the applications
//! of `[exclusions] apps` (D47) in a window of its own, opened from the settings panel.
//!
//! The layout, the texts, and every decision are pure and tested on every host:
//!
//! - [`template`]: Raumakey's own layout. The [`INTRO`] wrapped on two lines; a virtual report
//!   list with the columns [`COLUMNS`] ("Tệp chương trình", "Mô tả"); [`APP_LABEL`] and an
//!   editable drop-down combo box whose list holds the applications with a window now, with
//!   "Làm mới" beside it; "Duyệt…" and "Thêm" (the default button) under it; the note of a
//!   run without a folder ([`NOT_SAVED`]); and "Xoá", "Mặc định", and "Đóng" (`IDCANCEL`, as
//!   Esc and the close box). [`Placement::OffScreen`] is the variant the tests show.
//!
//!   ```text
//!   Trong các ứng dụng dưới đây, Raumakey luôn gõ tiếng Anh và không mở rộng gõ tắt.
//!   Mọi thay đổi có hiệu lực và được lưu ngay.
//!   +- Tệp chương trình ---+- Mô tả ----------------------------+
//!   | mstsc.exe            | Remote Desktop Connection          |
//!   | msrdc.exe            | Remote Desktop / Windows App       |
//!   +----------------------+------------------------------------+
//!   Ứng dụng: [notepad.exe                            v] [Làm mới]
//!             [Duyệt…] [Thêm]
//!   (Không có thư mục để lưu cấu hình, …: without a folder only)
//!   [Xoá] [Mặc định]                                       [Đóng]
//!   ```
//!
//! - What a row shows: [`row_cells`], the executable and its description, which is the
//!   `FileDescription` of a running program of that name when one runs, else the product name
//!   of the four defaults ([`builtin_description`], in English: these are product names),
//!   else nothing. An entry of the list of running applications is [`program_entry`]
//!   (`mstsc.exe – Remote Desktop Connection`).
//! - What "Thêm" does with the text of the edit ([`entered`]): it is normalized exactly as
//!   `raumakey.toml` is read (`raumakey_config::normalize_excluded_app`: a name without
//!   `.exe`, a path pasted from Explorer, quoted or not, any case), and an entry of the list of
//!   running applications stands for its executable; a text that names no program is
//!   explained ([`invalid_name_prompt`], a warning), a listed application is selected, and
//!   only a new one is sent to the tray. "Thêm" is enabled while the edit holds text
//!   ([`can_add`]). "Mặc định" asks first ([`defaults_prompt`]). After a refresh the
//!   application just added is selected once it is listed, else the one selected before
//!   ([`selection_after`]).
//!
//! # Data flow
//!
//! The tray is the only source of the dialog's data, as for the settings panel
//! (`crate::exclusions`): every edit ("Thêm", "Xoá", "Mặc định") goes to the tray as an
//! `ExclusionsRequest` and takes effect, and is saved, at once; the dialog changes no row
//! itself and shows the list that the tray sends back ([`EXCLUSIONS_REFRESH`]). There is no
//! "Lưu". Without a folder the edits take effect until Raumakey exits, and the note says so.
//!
//! # Windows
//!
//! `ExclusionsDialog` is the dialog's handler and `spawn_exclusions` shows it on a thread of
//! its own (`raumakey_windows::dialog_box::spawn_dialog`), so the tray never waits for it. It
//! reaches the tray through an `ExclusionsLink` (the shared state to show, the requests, and
//! the moments it was reached and agreed to close: `crate::desktop` implements it over the
//! tray's inbox), asks through the `Prompter` of the macro list editor
//! (`crate::macro_dialog`: message boxes and the Open dialog owned by the dialog for real,
//! a script in the tests), and reads the running programs through `Programs`
//! (`RunningPrograms`, over `raumakey_windows::programs`, for real):
//!
//! - **Opening.** `init` sets the icon of `raumakey.exe`, the columns, the [`INTRO`] wrapped
//!   at spaces to its label's width (Windows' own word wrap would cut a Vietnamese word;
//!   `Dialog::set_text_wrapped`), fills the list of running applications, shows the shared
//!   state, moves the focus to the edit, and posts itself [`EXCLUSIONS_REFRESH`], so that a
//!   refresh refused while the dialog was starting is made up once it is up.
//! - **Descriptions.** The dialog looks up the description of each application it has not
//!   looked up yet (one pass over the running processes and the version resources of their
//!   programs) and keeps what it found; "Làm mới" fills the list of running applications
//!   again and looks up once more what was not found. This reads the window manager's lists,
//!   process image names, and version resources on the dialog's own thread: bounded work
//!   that may take a moment at the opening and at "Làm mới", for the dialog alone.
//! - **The edit.** Choosing an entry of the list puts its executable into the edit: the combo
//!   box shows the entry's own text only after it reported the choice, so the handler posts
//!   itself a private message and replaces the text then. The user may also type or paste a
//!   name or a path, or choose a program with "Duyệt…" (the Open dialog, programs only, owned
//!   by the dialog), whose path goes into the edit; "Thêm" normalizes it.
//! - **Boxes.** After every message box and the Open dialog, whose modal loops run inside the
//!   handler, the dialog shows the shared state again: a refresh posted meanwhile did not
//!   reach it.
//! - **Closing.** "Đóng", Esc, and the close box close the dialog without asking: every edit
//!   took effect already. `DetachedDialog::dismiss` (the tray's exit) closes it and its box.
//!
//! Strings shown to the user are new Raumakey strings in Vietnamese (design §22; not taken
//! from any other input method; the descriptions of the defaults are product names). Nothing
//! here logs; the texts hold executable names, descriptions of programs, and the path the user
//! chose, never keystrokes, document text, or macro text.
//!
//! # Manual check list
//!
//! A successful build and the tests are no evidence that the real dialog behaves; check on a
//! test desktop with `raumakey.exe` (the list of `crate::desktop`, items 37–45, opens it from
//! the panel):
//!
//! 1. The dialog opens centered with Raumakey's icon in its title bar and taskbar button, the
//!    title "Raumakey – Ứng dụng loại trừ", the two lines of its introduction whole, and the
//!    four defaults with their descriptions ("Remote Desktop Connection" and so on, or the
//!    description of the running program).
//! 2. Open the list of the combo box: the applications with a window, each once, with its
//!    description, Raumakey not among them; choose Notepad: the edit holds `notepad.exe`
//!    only. Type `mspaint` and press Enter: a row `mspaint.exe` is added and selected, and the
//!    edit is empty. Start another program and press "Làm mới": it is in the list.
//! 3. Paste `"C:\Windows\System32\notepad.exe"` (Explorer's "Copy as path") and press "Thêm":
//!    the row of `notepad.exe` is selected, nothing is added. Type `C:\` and press "Thêm": a
//!    warning box owned by the dialog, in Vietnamese; nothing changes.
//! 4. "Duyệt…": the Open dialog lists folders and programs only ("Chương trình (*.exe)");
//!    choose one: its path is in the edit; "Thêm" adds its file name. Cancel it: nothing
//!    changes.
//! 5. Select a row: "Xoá" is enabled; press it: the row goes. "Mặc định": the question names
//!    the four defaults; "Không" changes nothing, "Có" puts them back.
//! 6. Esc, "Đóng", and the close box close it without asking.
//! 7. At 150 % (and dragged between monitors of different scale) the texts are sharp and
//!    whole, the introduction on its two lines.

use raumakey_config::normalize_excluded_app;
use raumakey_model::host::ExeName;
use raumakey_windows::dialog_template::{
    Control, DialogRect, DialogTemplate, IDCANCEL, STATIC_ID, styles,
};
use raumakey_windows::file_dialog::FileFilter;
use raumakey_windows::window::AppMessage;

use crate::macro_dialog::{Placement, Prompt, PromptKind};

/// The dialog's title.
pub const TITLE: &str = "Raumakey \u{2013} Ứng dụng loại trừ";
/// The dialog's width and height in dialog units.
pub const DIALOG_SIZE: (i16, i16) = (300, 218);

/// The introduction (label, two lines).
pub const ID_INTRO: u16 = 101;
/// The list of excluded applications (a virtual list view).
pub const ID_LIST: u16 = 102;
/// The application to add: an editable combo box whose list holds the running applications.
pub const ID_APP: u16 = 103;
/// "Làm mới": fills the list of running applications again.
pub const ID_REFRESH: u16 = 104;
/// "Duyệt…": the Open dialog for a program.
pub const ID_BROWSE: u16 = 105;
/// "Thêm", the default button.
pub const ID_ADD: u16 = 106;
/// The note of a run without a folder (label, two lines; empty otherwise).
pub const ID_NOT_SAVED: u16 = 107;
/// "Xoá".
pub const ID_REMOVE: u16 = 108;
/// "Mặc định".
pub const ID_DEFAULTS: u16 = 109;

/// The introduction: "In the applications below, Raumakey always types English and expands no
/// macro. Every change takes effect and is saved at once."
pub const INTRO: &str = "Trong các ứng dụng dưới đây, Raumakey luôn gõ tiếng Anh và không mở \
                         rộng gõ tắt. Mọi thay đổi có hiệu lực và được lưu ngay.";
/// The columns of the list: header and width in dialog units.
pub const COLUMNS: [(&str, i16); 2] = [("Tệp chương trình", 100), ("Mô tả", 166)];
/// The label of the edit: "Application:".
pub const APP_LABEL: &str = "Ứng dụng:";
/// The warning for a text that names no program: "Invalid application name. Type the name of
/// the program file, for example mstsc.exe, or choose an application in the list."
pub const INVALID_NAME: &str = "Tên ứng dụng không hợp lệ. Hãy nhập tên tệp chương trình, ví dụ \
                                mstsc.exe, hoặc chọn một ứng dụng trong danh sách.";
/// The question of "Mặc định": "Reset the list to the default remote-control applications
/// (…)?"
pub const DEFAULTS_QUESTION: &str = "Đặt lại danh sách về các ứng dụng điều khiển máy từ xa mặc \
                                     định (mstsc.exe, msrdc.exe, rdcman.exe, vmconnect.exe)?";
/// The note without a folder: "No folder to save the configuration, so changes last only until
/// Raumakey exits."
pub const NOT_SAVED: &str = "Không có thư mục để lưu cấu hình, nên thay đổi chỉ có hiệu lực tới \
                             khi Raumakey thoát.";
/// The file types of "Duyệt…": programs only.
pub const PROGRAMS: [FileFilter<'static>; 1] = [FileFilter {
    label: "Chương trình (*.exe)",
    patterns: "*.exe",
}];

/// The private message that asks the open dialog to show the shared state again (`WM_APP + 1`
/// of the dialog's own window; below the range the dialog procedure keeps for itself).
pub const EXCLUSIONS_REFRESH: AppMessage = match AppMessage::new(1) {
    Some(message) => message,
    None => panic!("offset outside the WM_APP range"),
};

/// The built-in descriptions of the defaults: product names, in English.
const BUILTIN_DESCRIPTIONS: [(&str, &str); 4] = [
    ("mstsc.exe", "Remote Desktop Connection"),
    ("msrdc.exe", "Remote Desktop / Windows App"),
    ("rdcman.exe", "Remote Desktop Connection Manager"),
    ("vmconnect.exe", "Hyper-V VM Connection"),
];

/// The description of a default application (`[exclusions]`'s defaults, the Remote Desktop and
/// Hyper-V clients): its product name; `None` for any other.
pub fn builtin_description(exe: &ExeName) -> Option<&'static str> {
    BUILTIN_DESCRIPTIONS
        .iter()
        .find(|(name, _)| *name == exe.as_str())
        .map(|(_, description)| *description)
}

/// `text` as a cell shows it: every control character as a space.
fn shown(text: &str) -> String {
    text.chars()
        .map(|char| if char.is_control() { ' ' } else { char })
        .collect()
}

/// The cells of the row of `exe`: its name, and the description of a running program of that
/// name (`running`), else its [`builtin_description`], else nothing.
pub fn row_cells(exe: &ExeName, running: Option<&str>) -> Vec<String> {
    let description = running.or_else(|| builtin_description(exe)).unwrap_or("");
    vec![exe.as_str().to_owned(), shown(description)]
}

/// A program with a window now, as the list of running applications shows it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Program {
    /// Its executable.
    pub exe: ExeName,
    /// The description of its program (`FileDescription`), if it has one.
    pub description: Option<String>,
}

/// The entry of `program` in the list of running applications: its executable, and its
/// description after an en dash when it has one (`mstsc.exe – Remote Desktop Connection`).
pub fn program_entry(program: &Program) -> String {
    match &program.description {
        Some(description) => format!("{} \u{2013} {}", program.exe, shown(description)),
        None => program.exe.as_str().to_owned(),
    }
}

/// What "Thêm" makes of the text of the edit ([`entered`]).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Entered {
    /// Nothing but spaces: nothing to do.
    Empty,
    /// The text names no program: explained ([`invalid_name_prompt`]), nothing changes.
    Invalid,
    /// The application is listed already, at this row: the row is selected.
    Listed(usize),
    /// A new application: sent to the tray to be added.
    New(ExeName),
}

/// What "Thêm" makes of `text` with `apps` listed and `running` in the list of running
/// applications: an entry of that list ([`program_entry`]) stands for its executable, and any
/// other text is normalized as `raumakey.toml` is read
/// (`raumakey_config::normalize_excluded_app`).
pub fn entered(text: &str, apps: &[ExeName], running: &[Program]) -> Entered {
    if !can_add(text) {
        return Entered::Empty;
    }
    let chosen = running
        .iter()
        .find(|program| program_entry(program) == text)
        .map(|program| program.exe.clone());
    let Some(exe) = chosen.or_else(|| normalize_excluded_app(text)) else {
        return Entered::Invalid;
    };
    match apps.iter().position(|listed| *listed == exe) {
        Some(index) => Entered::Listed(index),
        None => Entered::New(exe),
    }
}

/// Whether "Thêm" has something to add: `text` holds more than spaces.
pub fn can_add(text: &str) -> bool {
    !text.trim().is_empty()
}

/// The note under "Thêm": [`NOT_SAVED`] when nothing can be saved, else nothing.
pub const fn not_saved_text(can_save: bool) -> &'static str {
    if can_save { "" } else { NOT_SAVED }
}

/// The warning of a text that names no program ([`INVALID_NAME`]).
pub fn invalid_name_prompt() -> Prompt {
    Prompt::new(PromptKind::Warning, INVALID_NAME.to_owned())
}

/// The question of "Mặc định" ([`DEFAULTS_QUESTION`]).
pub fn defaults_prompt() -> Prompt {
    Prompt::new(PromptKind::Question, DEFAULTS_QUESTION.to_owned())
}

/// The row to select once `apps` are shown: that of `added` (the application just added) when
/// it is listed, else that of `selected` (the one selected before), else none.
pub fn selection_after(
    selected: Option<&ExeName>,
    added: Option<&ExeName>,
    apps: &[ExeName],
) -> Option<usize> {
    let row = |exe: &ExeName| apps.iter().position(|listed| listed == exe);
    added.and_then(row).or_else(|| selected.and_then(row))
}

fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
    DialogRect::new(x, y, cx, cy)
}

/// The dialog's template (see the [module documentation](self)). Each group (`WS_GROUP`)
/// holds the list alone, the edit alone, push buttons only, or texts only, as the panel's do.
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
        .with_control(
            Control::label(ID_INTRO, INTRO, rect(7, 7, 286, 16))
                .with_style(styles::SS_NOPREFIX)
                .group(),
        )
        .with_control(Control::virtual_list_view(ID_LIST, rect(7, 27, 286, 104)).group())
        .with_control(Control::label(STATIC_ID, APP_LABEL, rect(7, 140, 38, 8)).group())
        .with_control(Control::drop_down_combo(ID_APP, rect(47, 138, 186, 12)).group())
        .with_control(Control::push_button(ID_REFRESH, "Làm mới", rect(237, 137, 56, 14)).group())
        .with_control(Control::push_button(
            ID_BROWSE,
            "Duyệt\u{2026}",
            rect(47, 155, 56, 14),
        ))
        .with_control(Control::default_push_button(
            ID_ADD,
            "Thêm",
            rect(107, 155, 56, 14),
        ))
        .with_control(
            Control::label(ID_NOT_SAVED, "", rect(7, 175, 286, 16))
                .with_style(styles::SS_NOPREFIX)
                .group(),
        )
        .with_control(Control::push_button(ID_REMOVE, "Xoá", rect(7, 197, 56, 14)).group())
        .with_control(Control::push_button(
            ID_DEFAULTS,
            "Mặc định",
            rect(67, 197, 56, 14),
        ))
        .with_control(Control::push_button(
            IDCANCEL,
            "Đóng",
            rect(237, 197, 56, 14),
        ))
}

#[cfg(windows)]
pub use win::{
    ExclusionsDialog, ExclusionsLink, Programs, RunningPrograms, spawn_exclusions,
    spawn_exclusions_with,
};

#[cfg(windows)]
mod win {
    use core::fmt;
    use std::collections::BTreeMap;

    use raumakey_model::host::ExeName;
    use raumakey_windows::dialog_box::{
        DetachedDialog, Dialog, DialogHandler, notifications, spawn_dialog,
    };
    use raumakey_windows::error::WinResult;
    use raumakey_windows::module::Module;
    use raumakey_windows::programs::{
        process_description, running_applications, running_processes,
    };
    use raumakey_windows::window::AppMessage;

    use super::{
        COLUMNS, EXCLUSIONS_REFRESH, Entered, ID_ADD, ID_APP, ID_BROWSE, ID_DEFAULTS, ID_INTRO,
        ID_LIST, ID_NOT_SAVED, ID_REFRESH, ID_REMOVE, INTRO, PROGRAMS, Placement, Program, can_add,
        defaults_prompt, entered, invalid_name_prompt, not_saved_text, program_entry, row_cells,
        selection_after, template,
    };
    use crate::exclusions::{ExclusionsRequest, ExclusionsState};
    use crate::macro_dialog::{APP_ICON_ID, MessageBoxes, Prompter};

    /// The descriptions the dialog looked up, by executable (`None`: none found).
    type Descriptions = BTreeMap<ExeName, Option<String>>;

    /// The offset of the private message that the handler posts to its own dialog after a
    /// choice from the list of running applications, to put the choice's executable into the
    /// edit once the combo box has shown the entry's text.
    const PICKED: u32 = 2;

    /// How the dialog reaches the tray; `crate::desktop` implements it over the tray's inbox
    /// and the state the tray's thread shares with the dialog. Its methods run on the dialog's
    /// thread, inside the handler, and return at once.
    pub trait ExclusionsLink: Send {
        /// What the dialog shows now: the state the tray sent last.
        fn state(&mut self) -> ExclusionsState;

        /// Sends `request` to the tray.
        fn request(&mut self, request: ExclusionsRequest);

        /// The dialog's window took a message after its `init` (its handle was published by
        /// then). Nothing by default.
        fn reached(&mut self) {}

        /// The handler agreed to close: the dialog ends once it returns. Nothing by default.
        fn closing(&mut self) {}
    }

    /// The running programs, as the dialog lists and describes them; [`RunningPrograms`] is
    /// the real one. Its methods run on the dialog's thread and may take a moment.
    pub trait Programs: Send {
        /// The applications with a window now, one per executable, sorted, not Raumakey
        /// itself, each with its description.
        fn running(&mut self) -> Vec<Program>;

        /// For each of `apps`, the description of a running program of that name, if one
        /// runs and has one.
        fn descriptions(&mut self, apps: &[ExeName]) -> Vec<Option<String>>;
    }

    /// The real [`Programs`]: `raumakey_windows::programs` (the applications with an
    /// unowned, visible window that is no tool window, other than this process; the
    /// `FileDescription` of the program of a running process with that image name).
    #[derive(Clone, Copy, Debug, Default)]
    pub struct RunningPrograms;

    impl Programs for RunningPrograms {
        fn running(&mut self) -> Vec<Program> {
            running_applications()
                .into_iter()
                .map(|program| Program {
                    description: process_description(program.process_id),
                    exe: program.exe,
                })
                .collect()
        }

        fn descriptions(&mut self, apps: &[ExeName]) -> Vec<Option<String>> {
            let processes = running_processes();
            apps.iter()
                .map(|app| {
                    processes
                        .iter()
                        .filter(|process| process.exe == *app)
                        .find_map(|process| process_description(process.process_id))
                })
                .collect()
        }
    }

    /// The handler of the exclusions dialog (see the [module documentation](super)): it shows
    /// the state of its [`ExclusionsLink`], sends the user's edits through it, and asks through
    /// its [`Prompter`].
    pub struct ExclusionsDialog<P: Prompter, R: Programs, L: ExclusionsLink> {
        prompter: P,
        programs: R,
        link: L,
        /// The applications the list shows now, in its order.
        apps: Vec<ExeName>,
        /// The descriptions of running programs looked up so far.
        descriptions: Descriptions,
        /// The running applications in the combo box's list, in its order.
        running: Vec<Program>,
        /// The application just added: selected once a refresh lists it.
        added: Option<ExeName>,
    }

    impl<P: Prompter, R: Programs, L: ExclusionsLink> fmt::Debug for ExclusionsDialog<P, R, L> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("ExclusionsDialog")
                .field("apps", &self.apps.len())
                .field("descriptions", &self.descriptions.len())
                .field("running", &self.running.len())
                .finish_non_exhaustive()
        }
    }

    impl<P: Prompter, R: Programs, L: ExclusionsLink> ExclusionsDialog<P, R, L> {
        /// The dialog that shows the state of `link`, asks through `prompter`, and lists and
        /// describes the running programs through `programs`. Nothing is read before `init`.
        pub fn new(prompter: P, programs: R, link: L) -> Self {
            Self {
                prompter,
                programs,
                link,
                apps: Vec::new(),
                descriptions: Descriptions::new(),
                running: Vec::new(),
                added: None,
            }
        }

        /// Shows the shared state: the rows (with the descriptions looked up for the
        /// applications not looked up yet), the selection ([`selection_after`]), the note, and
        /// the buttons that need a selection or a text.
        fn show(&mut self, dialog: &Dialog<'_>) {
            let state = self.link.state();
            let selected = dialog
                .list_selected(ID_LIST)
                .and_then(|row| self.apps.get(row))
                .cloned();
            self.look_up(&state.apps);
            let rows: Vec<Vec<String>> = state
                .apps
                .iter()
                .map(|exe| row_cells(exe, self.description(exe)))
                .collect();
            // Names and descriptions hold no NUL; a failure leaves the rows shown before.
            let _ = dialog.list_set_virtual_rows(ID_LIST, &rows);
            let row = selection_after(selected.as_ref(), self.added.as_ref(), &state.apps);
            if self
                .added
                .as_ref()
                .is_some_and(|added| state.apps.contains(added))
            {
                self.added = None;
            }
            let _ = dialog.list_select(ID_LIST, row);
            dialog.set_enabled(ID_REMOVE, row.is_some());
            // Wrapped at spaces, since Windows would cut a Vietnamese word.
            let _ = dialog.set_text_wrapped(ID_NOT_SAVED, not_saved_text(state.can_save));
            self.apps = state.apps;
            self.follow_edit(dialog);
        }

        /// Looks up the descriptions of the applications of `apps` not looked up yet.
        fn look_up(&mut self, apps: &[ExeName]) {
            let missing: Vec<ExeName> = apps
                .iter()
                .filter(|app| !self.descriptions.contains_key(app))
                .cloned()
                .collect();
            if missing.is_empty() {
                return;
            }
            let found = self.programs.descriptions(&missing);
            for (index, app) in missing.into_iter().enumerate() {
                let description = found.get(index).cloned().flatten();
                self.descriptions.insert(app, description);
            }
        }

        fn description(&self, exe: &ExeName) -> Option<&str> {
            self.descriptions.get(exe).and_then(Option::as_deref)
        }

        /// Fills the list of running applications again, keeping what the edit holds (the
        /// combo box empties its edit with its items); their descriptions are kept for the
        /// rows of the same executables.
        fn fill_running(&mut self, dialog: &Dialog<'_>) {
            let running = self.programs.running();
            for program in &running {
                if program.description.is_some() {
                    self.descriptions
                        .insert(program.exe.clone(), program.description.clone());
                }
            }
            let typed = dialog.text(ID_APP).unwrap_or_default();
            let entries: Vec<String> = running.iter().map(program_entry).collect();
            let entries: Vec<&str> = entries.iter().map(String::as_str).collect();
            // The entries hold no NUL; a list that cannot be filled stays as it was.
            let _ = dialog.combo_set_items(ID_APP, &entries);
            let _ = dialog.set_text(ID_APP, &typed);
            self.running = running;
        }

        /// "Thêm" is enabled while the edit holds text.
        fn follow_edit(&self, dialog: &Dialog<'_>) {
            let text = dialog.text(ID_APP).unwrap_or_default();
            dialog.set_enabled(ID_ADD, can_add(&text));
        }

        /// "Thêm": the text of the edit ([`entered`]).
        fn add(&mut self, dialog: &Dialog<'_>) {
            let text = dialog.text(ID_APP).unwrap_or_default();
            match entered(&text, &self.apps, &self.running) {
                Entered::Empty => {}
                Entered::Invalid => {
                    self.prompter.ask(dialog, &invalid_name_prompt());
                    // A refresh posted while the box was open did not reach the handler.
                    self.show(dialog);
                    dialog.focus(ID_APP);
                }
                Entered::Listed(row) => {
                    let _ = dialog.list_select(ID_LIST, Some(row));
                    dialog.set_enabled(ID_REMOVE, true);
                }
                Entered::New(exe) => {
                    self.link.request(ExclusionsRequest::Add(exe.clone()));
                    self.added = Some(exe);
                    let _ = dialog.set_text(ID_APP, "");
                    self.follow_edit(dialog);
                }
            }
        }

        /// "Xoá": the selected application.
        fn remove(&mut self, dialog: &Dialog<'_>) {
            let selected = dialog
                .list_selected(ID_LIST)
                .and_then(|row| self.apps.get(row))
                .cloned();
            if let Some(exe) = selected {
                self.link.request(ExclusionsRequest::Remove(exe));
            }
        }

        /// "Mặc định", after "yes" to [`defaults_prompt`].
        fn defaults(&mut self, dialog: &Dialog<'_>) {
            let yes = self.prompter.ask(dialog, &defaults_prompt());
            self.show(dialog);
            if yes {
                self.link.request(ExclusionsRequest::Defaults);
            }
        }

        /// "Duyệt…": the path chosen goes into the edit.
        fn browse(&mut self, dialog: &Dialog<'_>) {
            let chosen = self.prompter.choose_file(dialog, &PROGRAMS);
            // A refresh posted while the Open dialog was open did not reach the handler.
            self.show(dialog);
            if let Some(path) = chosen {
                let _ = dialog.set_text(ID_APP, &path.to_string_lossy());
                self.follow_edit(dialog);
                dialog.focus(ID_APP);
            }
        }

        /// A choice from the list of running applications: its executable goes into the edit.
        fn picked(&self, dialog: &Dialog<'_>) {
            let program = dialog
                .combo_selected(ID_APP)
                .and_then(|index| self.running.get(index));
            if let Some(program) = program {
                let _ = dialog.set_text(ID_APP, program.exe.as_str());
            }
            self.follow_edit(dialog);
        }
    }

    impl<P: Prompter, R: Programs, L: ExclusionsLink> DialogHandler for ExclusionsDialog<P, R, L> {
        fn init(&mut self, dialog: &Dialog<'_>) {
            // The icon of `raumakey.exe`; another executable (a test) has none, and the
            // dialog works without it.
            if let Ok(module) = Module::executable() {
                let _ = dialog.set_icons(module, APP_ICON_ID);
            }
            let _ = dialog.list_set_columns(ID_LIST, &COLUMNS);
            // Wrapped at spaces, since Windows would cut a Vietnamese word; a text that
            // cannot be measured stays as the template has it.
            let _ = dialog.set_text_wrapped(ID_INTRO, INTRO);
            self.fill_running(dialog);
            self.show(dialog);
            dialog.focus(ID_APP);
            self.prompter.created(dialog);
            // A refresh posted before this returns is refused (the window is published only
            // then), so the dialog looks at the shared state once more when it is up.
            let _ = dialog.post(EXCLUSIONS_REFRESH, 0, 0);
        }

        fn command(&mut self, dialog: &Dialog<'_>, id: u16, code: u16) {
            self.link.reached();
            match (id, code) {
                (ID_ADD, notifications::BN_CLICKED) => self.add(dialog),
                (ID_REMOVE, notifications::BN_CLICKED) => self.remove(dialog),
                (ID_DEFAULTS, notifications::BN_CLICKED) => self.defaults(dialog),
                (ID_BROWSE, notifications::BN_CLICKED) => self.browse(dialog),
                (ID_REFRESH, notifications::BN_CLICKED) => {
                    // What was not found is looked up again.
                    self.descriptions
                        .retain(|_, description| description.is_some());
                    self.fill_running(dialog);
                    self.show(dialog);
                }
                (ID_APP, notifications::CBN_SELCHANGE) => {
                    // The combo box shows the entry's text after this returns.
                    let posted =
                        AppMessage::new(PICKED).is_some_and(|picked| dialog.post(picked, 0, 0));
                    if !posted {
                        self.picked(dialog);
                    }
                }
                (ID_APP, notifications::CBN_EDITCHANGE) => self.follow_edit(dialog),
                _ => {}
            }
        }

        fn notify(&mut self, dialog: &Dialog<'_>, id: u16, code: i32) {
            if (id, code) == (ID_LIST, notifications::LVN_ITEMCHANGED) {
                dialog.set_enabled(ID_REMOVE, dialog.list_selected(ID_LIST).is_some());
            }
        }

        fn app_message(&mut self, dialog: &Dialog<'_>, message: AppMessage, _: usize, _: isize) {
            // The refresh that `init` posted arrives first.
            self.link.reached();
            if message == EXCLUSIONS_REFRESH {
                self.show(dialog);
            } else if message.offset() == PICKED {
                self.picked(dialog);
            }
        }

        fn close_requested(&mut self, _dialog: &Dialog<'_>) -> bool {
            // Every edit took effect already: nothing to ask. The dialog ends once this
            // returns.
            self.link.closing();
            true
        }
    }

    /// Shows the exclusions dialog of `link` on a thread of its own, centered, asking in
    /// message boxes and the Open dialog owned by it ([`MessageBoxes`]) and reading the
    /// running programs ([`RunningPrograms`]), and returns at once (see the [module
    /// documentation](super)). `on_closed` runs on that thread exactly once when the dialog
    /// ends: closed by the user, dismissed (`DetachedDialog::dismiss`), or never shown. Fails
    /// when the thread cannot start; `on_closed` does not run then.
    pub fn spawn_exclusions(
        link: impl ExclusionsLink + 'static,
        on_closed: impl FnOnce() + Send + 'static,
    ) -> WinResult<DetachedDialog> {
        spawn_exclusions_with(
            Placement::Centered,
            MessageBoxes,
            RunningPrograms,
            link,
            on_closed,
        )
    }

    /// [`spawn_exclusions`] at `placement`, asking through `prompter` and reading the running
    /// programs through `programs`: the tests show the dialog off-screen
    /// ([`Placement::OffScreen`], moved off every monitor by the prompter's `created`) and
    /// answer from a script.
    pub fn spawn_exclusions_with<
        P: Prompter + 'static,
        R: Programs + 'static,
        L: ExclusionsLink + 'static,
    >(
        placement: Placement,
        prompter: P,
        programs: R,
        link: L,
        on_closed: impl FnOnce() + Send + 'static,
    ) -> WinResult<DetachedDialog> {
        let template = template(placement).build()?;
        let handler = ExclusionsDialog::new(prompter, programs, link);
        spawn_dialog(template, handler, move |_| on_closed())
    }
}
