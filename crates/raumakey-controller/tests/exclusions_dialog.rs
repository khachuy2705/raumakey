//! The exclusions dialog (`exclusions_dialog`, D48): its template, texts, the descriptions and
//! cells of its rows, the entries of its list of running applications, what "Thêm" makes of
//! the text typed, pasted, or chosen, and the row selected after a refresh
//! (host-independent); on Windows, the real dialog on a thread of its own, off every monitor.

use raumakey_controller::exclusions::{ExclusionsState, default_apps};
use raumakey_controller::exclusions_dialog::{
    APP_LABEL, COLUMNS, DEFAULTS_QUESTION, DIALOG_SIZE, EXCLUSIONS_REFRESH, Entered, ID_ADD,
    ID_APP, ID_BROWSE, ID_DEFAULTS, ID_INTRO, ID_LIST, ID_NOT_SAVED, ID_REFRESH, ID_REMOVE, INTRO,
    INVALID_NAME, NOT_SAVED, PROGRAMS, Program, TITLE, builtin_description, can_add,
    defaults_prompt, entered, invalid_name_prompt, not_saved_text, program_entry, row_cells,
    selection_after, template,
};
use raumakey_controller::macro_dialog::{Placement, PromptKind};
use raumakey_model::host::ExeName;
use raumakey_windows::dialog_box::is_reserved;
use raumakey_windows::dialog_template::{
    Control, ControlClass, DROPPED_LIST_HEIGHT, DialogRect, IDCANCEL, STATIC_ID, styles,
};
use raumakey_windows::window::is_postable;

/// Common Controls 6 for the dialog, as `raumakey.exe`'s manifest asks for it; shared with the
/// tests of `raumakey-windows`.
#[cfg(windows)]
#[path = "../../raumakey-windows/tests/layer/common_controls_v6.rs"]
mod common_controls_v6;

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn find(controls: &[Control], id: u16) -> &Control {
    controls
        .iter()
        .find(|control| control.id() == id)
        .unwrap_or_else(|| panic!("control {id}"))
}

// ---- Texts ----

#[test]
fn the_texts_are_raumakeys_own_vietnamese() {
    assert_eq!(TITLE, "Raumakey \u{2013} Ứng dụng loại trừ");
    assert_eq!(
        INTRO,
        "Trong các ứng dụng dưới đây, Raumakey luôn gõ tiếng Anh và không mở rộng gõ tắt. Mọi \
         thay đổi có hiệu lực và được lưu ngay."
    );
    assert_eq!(COLUMNS.map(|(text, _)| text), ["Tệp chương trình", "Mô tả"]);
    assert_eq!(APP_LABEL, "Ứng dụng:");
    assert_eq!(
        INVALID_NAME,
        "Tên ứng dụng không hợp lệ. Hãy nhập tên tệp chương trình, ví dụ mstsc.exe, hoặc chọn \
         một ứng dụng trong danh sách."
    );
    assert_eq!(
        DEFAULTS_QUESTION,
        "Đặt lại danh sách về các ứng dụng điều khiển máy từ xa mặc định (mstsc.exe, msrdc.exe, \
         rdcman.exe, vmconnect.exe)?"
    );
    assert_eq!(
        NOT_SAVED,
        "Không có thư mục để lưu cấu hình, nên thay đổi chỉ có hiệu lực tới khi Raumakey thoát."
    );
    assert_eq!(PROGRAMS.len(), 1);
    assert_eq!(PROGRAMS[0].label, "Chương trình (*.exe)");
    assert_eq!(PROGRAMS[0].patterns, "*.exe");
    // The question names every default, in the order of the schema.
    let names: Vec<String> = default_apps()
        .iter()
        .map(|app| app.as_str().to_owned())
        .collect();
    assert!(DEFAULTS_QUESTION.contains(&names.join(", ")));
}

#[test]
fn the_prompts_are_a_warning_and_a_question() {
    let warning = invalid_name_prompt();
    assert_eq!(warning.kind(), PromptKind::Warning);
    assert_eq!(warning.text(), INVALID_NAME);
    let question = defaults_prompt();
    assert_eq!(question.kind(), PromptKind::Question);
    assert_eq!(question.text(), DEFAULTS_QUESTION);
}

/// The line under "Thêm" says that nothing is saved without a folder, and nothing otherwise.
#[test]
fn the_note_says_when_nothing_is_saved() {
    assert_eq!(not_saved_text(false), NOT_SAVED);
    assert_eq!(not_saved_text(true), "");
}

/// The private message that asks the open dialog to show the shared state again.
#[test]
fn the_refresh_message_is_private_and_not_reserved_by_the_dialog() {
    assert!(is_postable(EXCLUSIONS_REFRESH.id()));
    assert!(!is_reserved(EXCLUSIONS_REFRESH.id()));
}

// ---- Rows and the list of running applications ----

/// The four defaults have descriptions of their own, the product names; other programs none.
#[test]
fn the_defaults_have_built_in_descriptions() {
    let described: Vec<(String, Option<&str>)> = default_apps()
        .iter()
        .map(|app| (app.as_str().to_owned(), builtin_description(app)))
        .collect();
    assert_eq!(
        described,
        [
            ("mstsc.exe".to_owned(), Some("Remote Desktop Connection")),
            ("msrdc.exe".to_owned(), Some("Remote Desktop / Windows App")),
            (
                "rdcman.exe".to_owned(),
                Some("Remote Desktop Connection Manager")
            ),
            ("vmconnect.exe".to_owned(), Some("Hyper-V VM Connection")),
        ]
    );
    assert_eq!(builtin_description(&exe("putty.exe")), None);
}

/// A row shows the executable and the description of a running program of that name, else the
/// built-in one, else nothing; a control character of a description is shown as a space.
#[test]
fn a_row_shows_the_executable_and_its_description() {
    assert_eq!(
        row_cells(&exe("mstsc.exe"), Some("Kết nối Máy tính Từ xa")),
        ["mstsc.exe", "Kết nối Máy tính Từ xa"]
    );
    assert_eq!(
        row_cells(&exe("mstsc.exe"), None),
        ["mstsc.exe", "Remote Desktop Connection"]
    );
    assert_eq!(row_cells(&exe("putty.exe"), None), ["putty.exe", ""]);
    assert_eq!(
        row_cells(&exe("putty.exe"), Some("PuTTY\tSSH\u{7}client")),
        ["putty.exe", "PuTTY SSH client"]
    );
}

/// An entry of the list of running applications names the executable, and its description
/// after an en dash when it has one.
#[test]
fn an_entry_of_the_running_applications_names_the_executable() {
    let described = Program {
        exe: exe("mstsc.exe"),
        description: Some("Remote Desktop Connection".to_owned()),
    };
    assert_eq!(
        program_entry(&described),
        "mstsc.exe \u{2013} Remote Desktop Connection"
    );
    let bare = Program {
        exe: exe("putty.exe"),
        description: None,
    };
    assert_eq!(program_entry(&bare), "putty.exe");
}

// ---- "Thêm" ----

fn running() -> Vec<Program> {
    vec![
        Program {
            exe: exe("mstsc.exe"),
            description: Some("Remote Desktop Connection".to_owned()),
        },
        Program {
            exe: exe("notepad++.exe"),
            description: Some("Notepad++ : a free (GNU) source code editor".to_owned()),
        },
    ]
}

/// "Thêm" is enabled while the edit holds something besides spaces.
#[test]
fn add_needs_a_text() {
    assert!(can_add("mstsc"));
    assert!(can_add("  a "));
    assert!(!can_add(""));
    assert!(!can_add(" \t "));
}

/// What "Thêm" makes of the text: the name normalized as the file is read (a name without
/// `.exe`, a path pasted from Explorer, in quotes or not, any case), the row of a listed one,
/// or a warning for what names no program; an entry of the running applications stands for
/// its executable.
#[test]
fn the_text_of_the_edit_is_normalized_as_the_file_is_read() {
    let apps = vec![exe("mstsc.exe"), exe("putty.exe")];
    let cases = [
        ("mstsc", Entered::Listed(0)),
        ("MSTSC.EXE", Entered::Listed(0)),
        (r"C:\Windows\System32\mstsc.exe", Entered::Listed(0)),
        (r#""C:\Program Files\PuTTY\putty.exe""#, Entered::Listed(1)),
        ("  putty  ", Entered::Listed(1)),
        ("notepad", Entered::New(exe("notepad.exe"))),
        (
            r#""C:\Program Files\Notepad++\notepad++.exe""#,
            Entered::New(exe("notepad++.exe")),
        ),
        ("VirtualBoxVM.exe", Entered::New(exe("virtualboxvm.exe"))),
        (
            "notepad++.exe \u{2013} Notepad++ : a free (GNU) source code editor",
            Entered::New(exe("notepad++.exe")),
        ),
        (
            "mstsc.exe \u{2013} Remote Desktop Connection",
            Entered::Listed(0),
        ),
        ("", Entered::Empty),
        ("   ", Entered::Empty),
        (r"C:\Tools\", Entered::Invalid),
        ("...", Entered::Invalid),
        ("c:", Entered::Invalid),
    ];
    for (text, expected) in cases {
        assert_eq!(entered(text, &apps, &running()), expected, "{text:?}");
    }
    // A name of an empty list is always new.
    assert_eq!(
        entered("mstsc", &[], &running()),
        Entered::New(exe("mstsc.exe"))
    );
}

// ---- Selection after a refresh ----

/// After a refresh the row of the application just added is selected once it is listed;
/// otherwise the application selected before stays selected where it is now, or nothing is.
#[test]
fn the_selection_follows_the_application_across_a_refresh() {
    let apps = vec![exe("msrdc.exe"), exe("putty.exe"), exe("notepad.exe")];
    assert_eq!(
        selection_after(Some(&exe("putty.exe")), Some(&exe("notepad.exe")), &apps),
        Some(2)
    );
    assert_eq!(
        selection_after(Some(&exe("putty.exe")), Some(&exe("gone.exe")), &apps),
        Some(1)
    );
    assert_eq!(
        selection_after(Some(&exe("putty.exe")), None, &apps),
        Some(1)
    );
    assert_eq!(selection_after(Some(&exe("mstsc.exe")), None, &apps), None);
    assert_eq!(selection_after(None, None, &apps), None);
}

// ---- The template ----

#[test]
fn the_template_lays_out_the_dialog_with_its_controls_and_texts() {
    let template = template(Placement::Centered);
    assert_eq!(template.title(), TITLE);
    assert_ne!(template.style() & styles::DS_CENTER, 0);
    assert_eq!(template.ex_style(), 0);
    template.build().unwrap();
    let controls = template.controls();

    let intro = find(controls, ID_INTRO);
    assert_eq!(intro.class(), ControlClass::Static);
    assert_eq!(intro.text(), INTRO);
    assert_ne!(intro.style() & styles::SS_NOPREFIX, 0);
    assert!(intro.rect().cy >= 16, "two lines");
    let note = find(controls, ID_NOT_SAVED);
    assert_eq!(note.class(), ControlClass::Static);
    assert_eq!(note.text(), "");
    assert!(note.rect().cy >= 16, "two lines");

    let list = find(controls, ID_LIST);
    assert_eq!(list.class(), ControlClass::ListView);
    assert_ne!(list.style() & styles::LVS_OWNERDATA, 0, "a virtual list");
    assert_ne!(list.style() & styles::LVS_SINGLESEL, 0);

    let app = find(controls, ID_APP);
    assert_eq!(app.class(), ControlClass::ComboBox);
    assert_eq!(
        app.style() & 0x03,
        styles::CBS_DROPDOWN,
        "an editable combo box"
    );
    assert_ne!(app.style() & styles::CBS_AUTOHSCROLL, 0);
    let label = controls
        .iter()
        .find(|control| control.text() == APP_LABEL)
        .unwrap();
    assert_eq!(label.id(), STATIC_ID);
    assert_eq!(label.class(), ControlClass::Static);

    for (id, text) in [
        (ID_REFRESH, "Làm mới"),
        (ID_BROWSE, "Duyệt\u{2026}"),
        (ID_ADD, "Thêm"),
        (ID_REMOVE, "Xoá"),
        (ID_DEFAULTS, "Mặc định"),
        (IDCANCEL, "Đóng"),
    ] {
        let button = find(controls, id);
        assert_eq!(button.class(), ControlClass::Button, "{text}");
        assert_eq!(button.text(), text);
    }
    // Enter adds what the edit holds; "Đóng" is the Cancel button (Esc, the close box).
    let defaults: Vec<u16> = controls
        .iter()
        .filter(|control| {
            control.class() == ControlClass::Button
                && control.style() & 0x0F == styles::BS_DEFPUSHBUTTON
        })
        .map(Control::id)
        .collect();
    assert_eq!(defaults, [ID_ADD]);

    // Tab order: the list, the edit, its buttons, then the buttons of the list.
    let stops: Vec<u16> = controls
        .iter()
        .filter(|control| control.style() & styles::WS_TABSTOP != 0)
        .map(Control::id)
        .collect();
    assert_eq!(
        stops,
        [
            ID_LIST,
            ID_APP,
            ID_REFRESH,
            ID_BROWSE,
            ID_ADD,
            ID_REMOVE,
            ID_DEFAULTS,
            IDCANCEL
        ]
    );
}

/// The rectangle a control takes on the dialog: a combo box without its list.
fn shown_rect(control: &Control) -> DialogRect {
    let mut rect = control.rect();
    if control.class() == ControlClass::ComboBox {
        rect.cy -= DROPPED_LIST_HEIGHT;
    }
    rect
}

fn overlap(a: DialogRect, b: DialogRect) -> bool {
    a.x < b.x + b.cx && b.x < a.x + a.cx && a.y < b.y + b.cy && b.y < a.y + a.cy
}

/// Every control lies inside the dialog with a margin, and no two overlap.
#[test]
fn the_layout_keeps_every_control_inside_without_overlap() {
    let template = template(Placement::Centered);
    let (width, height) = DIALOG_SIZE;
    let controls = template.controls();
    for control in controls {
        let rect = shown_rect(control);
        assert!(rect.x >= 7 && rect.y >= 7, "{control:?}");
        assert!(rect.x + rect.cx <= width - 7, "{control:?}");
        assert!(rect.y + rect.cy <= height - 7, "{control:?}");
    }
    for (index, a) in controls.iter().enumerate() {
        for b in &controls[index + 1..] {
            assert!(
                !overlap(shown_rect(a), shown_rect(b)),
                "{a:?} overlaps {b:?}"
            );
        }
    }
    // The columns fit the list, with room for its vertical scroll bar.
    let list = find(controls, ID_LIST).rect();
    let columns: i16 = COLUMNS.iter().map(|(_, width)| width).sum();
    assert!(columns + 14 <= list.cx, "{columns} in {list:?}");
}

/// A conservative estimate of a text's width in dialog units: four units per character.
fn estimated_width(text: &str) -> i16 {
    i16::try_from(text.chars().count() * 4).unwrap()
}

/// Buttons and the one-line label fit their texts, with a margin.
#[test]
fn the_labels_fit_their_controls() {
    let template = template(Placement::Centered);
    for control in template.controls() {
        let rect = control.rect();
        match control.class() {
            ControlClass::Button => {
                assert!(rect.cy >= 14, "{}", control.text());
                assert!(
                    estimated_width(control.text()) + 8 <= rect.cx,
                    "{}: {rect:?}",
                    control.text()
                );
            }
            ControlClass::Static if rect.cy < 16 => {
                assert!(estimated_width(control.text()) <= rect.cx, "{rect:?}");
            }
            _ => {}
        }
    }
}

/// The arrow keys move within a group (`WS_GROUP` starts one), which Windows makes of labels
/// too: each group holds the list alone, the edit alone, push buttons only, or texts only.
#[test]
fn every_group_holds_one_kind_of_control() {
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    enum Kind {
        List,
        Edit,
        Push,
        Text,
    }
    let kind_of = |control: &Control| match control.class() {
        ControlClass::ListView => Kind::List,
        ControlClass::ComboBox => Kind::Edit,
        ControlClass::Button => Kind::Push,
        _ => Kind::Text,
    };
    let template = template(Placement::Centered);
    let mut groups: Vec<Vec<&Control>> = Vec::new();
    for control in template.controls() {
        if control.style() & styles::WS_GROUP != 0 || groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(control);
    }
    for group in &groups {
        let kinds: Vec<Kind> = group.iter().map(|control| kind_of(control)).collect();
        assert!(kinds.iter().all(|kind| *kind == kinds[0]), "{kinds:?}");
        if matches!(kinds[0], Kind::List | Kind::Edit) {
            assert_eq!(group.len(), 1, "{kinds:?} alone");
        }
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

/// The state the dialog is opened with, for the tests of the real dialog.
#[allow(dead_code)]
fn state(apps: &[&str], can_save: bool) -> ExclusionsState {
    ExclusionsState {
        apps: apps.iter().map(|name| exe(name)).collect(),
        can_save,
    }
}

/// The real dialog on a thread of its own (`spawn_exclusions_with`, off every monitor, never
/// activated), driven from the test thread with `SendMessageW` as Windows drives it; its
/// questions are answered from a script, the Open dialog answers a scripted path, the running
/// programs are made up, and its link to the tray is a shared state and a channel. Every wait
/// has a timeout.
#[cfg(windows)]
mod win {
    use std::collections::{BTreeMap, VecDeque};
    use std::path::PathBuf;
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use raumakey_controller::exclusions::{ExclusionsRequest, ExclusionsState};
    use raumakey_controller::exclusions_dialog::{
        DEFAULTS_QUESTION, EXCLUSIONS_REFRESH, ExclusionsLink, ID_ADD, ID_APP, ID_BROWSE,
        ID_DEFAULTS, ID_INTRO, ID_LIST, ID_NOT_SAVED, ID_REFRESH, ID_REMOVE, INTRO, INVALID_NAME,
        NOT_SAVED, Program, Programs, TITLE, program_entry, spawn_exclusions_with,
    };
    use raumakey_controller::macro_dialog::{Placement, Prompt, PromptKind, Prompter};
    use raumakey_model::host::ExeName;
    use raumakey_windows::dialog_box::notifications::CBN_SELCHANGE;
    use raumakey_windows::dialog_box::{DetachedDialog, Dialog};
    use raumakey_windows::dialog_template::IDCANCEL;
    use raumakey_windows::file_dialog::FileFilter;
    use raumakey_windows::strings::until_nul;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::UI::Controls::{
        LVIS_SELECTED, LVITEMW, LVM_GETITEMCOUNT, LVM_GETITEMTEXTW, LVM_GETNEXTITEM,
        LVM_SETITEMSTATE, LVNI_SELECTED,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_CLICK, CB_GETCOUNT, CB_GETLBTEXT, CB_GETLBTEXTLEN, CB_SETCURSEL, GW_CHILD, GetDlgItem,
        GetWindow, GetWindowRect, IsWindow, IsWindowVisible, SMTO_NORMAL, SWP_NOACTIVATE,
        SWP_NOSIZE, SWP_NOZORDER, SendMessageTimeoutW, SetWindowPos, WM_CHAR, WM_COMMAND,
        WM_GETTEXT, WM_GETTEXTLENGTH, WM_NULL,
    };
    use windows::core::PWSTR;

    use super::common_controls_v6::{CCM_GETVERSION, CommonControls6};
    use super::{exe, state};

    const WAIT: Duration = Duration::from_secs(30);
    /// Milliseconds a sent message may take before the test fails.
    const SEND_TIMEOUT_MS: u32 = 30_000;

    /// What the dialog did, reported from its thread.
    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Event {
        /// The dialog exists, filled and not shown yet.
        Created(isize),
        /// A prompt, and whether the dialog was visible then.
        Asked {
            kind: PromptKind,
            text: String,
            visible: bool,
        },
        /// The Open dialog was asked for, with the patterns of its filters.
        Browsed(Vec<String>),
        Request(ExclusionsRequest),
        Closing,
        Closed,
    }

    type Answers = Arc<Mutex<VecDeque<bool>>>;
    type Shared = Arc<Mutex<ExclusionsState>>;
    type Running = Arc<Mutex<Vec<Program>>>;

    fn is_visible(hwnd: HWND) -> bool {
        // SAFETY: only reads the window state.
        unsafe { IsWindowVisible(hwnd) }.as_bool()
    }

    /// Moves the dialog off-screen before it is shown, answers questions from `answers` (then
    /// "no") and the Open dialog with `chosen`, without showing anything.
    struct FakePrompter {
        events: Sender<Event>,
        answers: Answers,
        chosen: Option<PathBuf>,
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

        fn choose_file(
            &mut self,
            _dialog: &Dialog<'_>,
            filters: &[FileFilter<'_>],
        ) -> Option<PathBuf> {
            let patterns = filters
                .iter()
                .map(|filter| format!("{} | {}", filter.label, filter.patterns))
                .collect();
            let _ = self.events.send(Event::Browsed(patterns));
            self.chosen.clone()
        }
    }

    /// Made-up running programs, and descriptions by executable.
    struct FakePrograms {
        running: Running,
        descriptions: BTreeMap<String, String>,
    }

    impl Programs for FakePrograms {
        fn running(&mut self) -> Vec<Program> {
            self.running.lock().unwrap().clone()
        }

        fn descriptions(&mut self, apps: &[ExeName]) -> Vec<Option<String>> {
            apps.iter()
                .map(|app| self.descriptions.get(app.as_str()).cloned())
                .collect()
        }
    }

    /// The tray as the dialog sees it: the shared state, and the requests into a channel.
    struct FakeLink {
        state: Shared,
        events: Sender<Event>,
    }

    impl ExclusionsLink for FakeLink {
        fn state(&mut self) -> ExclusionsState {
            self.state.lock().unwrap().clone()
        }

        fn request(&mut self, request: ExclusionsRequest) {
            let _ = self.events.send(Event::Request(request));
        }

        fn closing(&mut self) {
            let _ = self.events.send(Event::Closing);
        }
    }

    /// The dialog as the test sees it.
    struct Opened {
        dialog: DetachedDialog,
        events: Receiver<Event>,
        answers: Answers,
        shared: Shared,
        running: Running,
        hwnd: HWND,
    }

    impl Drop for Opened {
        fn drop(&mut self) {
            // A failed test leaves no dialog behind.
            self.dialog.dismiss();
        }
    }

    fn program(name: &str, description: Option<&str>) -> Program {
        Program {
            exe: exe(name),
            description: description.map(str::to_owned),
        }
    }

    /// The running applications of the tests.
    fn running_programs() -> Vec<Program> {
        vec![
            program("notepad.exe", Some("Notepad")),
            program("winword.exe", Some("Microsoft Word")),
        ]
    }

    /// Opens the dialog on `initial`, the Open dialog answering `chosen`, and waits until it
    /// is shown (far off-screen).
    fn open(initial: ExclusionsState, chosen: Option<PathBuf>) -> Opened {
        let (sender, events) = mpsc::channel();
        let answers: Answers = Arc::default();
        let shared: Shared = Arc::new(Mutex::new(initial));
        let running: Running = Arc::new(Mutex::new(running_programs()));
        let prompter = FakePrompter {
            events: sender.clone(),
            answers: Arc::clone(&answers),
            chosen,
        };
        let programs = FakePrograms {
            running: Arc::clone(&running),
            descriptions: BTreeMap::from([(
                "putty.exe".to_owned(),
                "PuTTY: a free SSH client".to_owned(),
            )]),
        };
        let link = FakeLink {
            state: Arc::clone(&shared),
            events: sender.clone(),
        };
        let closed = sender;
        let dialog = {
            // Under Common Controls 6, as `raumakey.exe` shows the dialog: its thread inherits
            // the context, and the guard may go once it started.
            let _v6 = CommonControls6::activate();
            spawn_exclusions_with(Placement::OffScreen, prompter, programs, link, move || {
                let _ = closed.send(Event::Closed);
            })
            .unwrap()
        };
        let Event::Created(hwnd) = events.recv_timeout(WAIT).expect("the dialog was created")
        else {
            panic!("the dialog started with its creation");
        };
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        let deadline = Instant::now() + WAIT;
        while !is_visible(hwnd) {
            assert!(Instant::now() < deadline, "the dialog was shown");
            std::thread::sleep(Duration::from_millis(5));
        }
        send(hwnd, WM_NULL, 0, 0);
        assert_eq!(
            send(item(hwnd, ID_LIST), CCM_GETVERSION, 0, 0),
            6,
            "Common Controls 6"
        );
        Opened {
            dialog,
            events,
            answers,
            shared,
            running,
            hwnd,
        }
    }

    impl Opened {
        /// The events that are waiting (the handler is done with what was sent).
        fn waiting(&self) -> Vec<Event> {
            self.events.try_iter().collect()
        }

        /// The tray sends `state`: the shared state, then the refresh message.
        fn refresh(&self, state: ExclusionsState) {
            *self.shared.lock().unwrap() = state;
            assert!(self.dialog.post(EXCLUSIONS_REFRESH, 0, 0));
        }

        /// Waits for the close and returns the events before it.
        fn closed(&self) -> Vec<Event> {
            let mut before = Vec::new();
            loop {
                match self.events.recv_timeout(WAIT).expect("the dialog closed") {
                    Event::Closed => break,
                    other => before.push(other),
                }
            }
            assert!(self.dialog.is_closed());
            before
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

    /// Types `typed` at the end of the empty edit of the combo box (its only child) as the
    /// keyboard does (`WM_CHAR`): the edit reports each change to the combo box, which reports
    /// `CBN_EDITCHANGE` to the dialog. (A `WM_SETTEXT`, to the combo box or its edit, reports
    /// nothing, as the handler's own changes do not.)
    fn type_into_edit(dialog: HWND, typed: &str) {
        // SAFETY: only reads the window list.
        let edit = unsafe { GetWindow(item(dialog, ID_APP), GW_CHILD) }.unwrap();
        for unit in typed.encode_utf16() {
            send(edit, WM_CHAR, usize::from(unit), 1);
        }
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

    /// The text of a cell (`LVM_GETITEMTEXTW` into a buffer of 256 units).
    fn cell(dialog: HWND, row: usize, column: i32) -> String {
        let mut buffer = [0_u16; 256];
        let mut cell = LVITEMW {
            iSubItem: column,
            pszText: PWSTR(buffer.as_mut_ptr()),
            cchTextMax: buffer.len() as i32,
            ..Default::default()
        };
        send(
            item(dialog, ID_LIST),
            LVM_GETITEMTEXTW,
            row,
            &raw mut cell as isize,
        );
        String::from_utf16(until_nul(&buffer)).unwrap()
    }

    fn rows(dialog: HWND) -> Vec<[String; 2]> {
        (0..usize::try_from(row_count(dialog)).unwrap())
            .map(|row| [cell(dialog, row, 0), cell(dialog, row, 1)])
            .collect()
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

    /// The items of the list of running applications.
    fn combo_items(dialog: HWND) -> Vec<String> {
        let combo = item(dialog, ID_APP);
        (0..usize::try_from(send(combo, CB_GETCOUNT, 0, 0)).unwrap())
            .map(|index| {
                let length = usize::try_from(send(combo, CB_GETLBTEXTLEN, index, 0)).unwrap();
                let mut buffer = vec![0_u16; length + 1];
                send(combo, CB_GETLBTEXT, index, buffer.as_mut_ptr() as isize);
                String::from_utf16(until_nul(&buffer)).unwrap()
            })
            .collect()
    }

    /// Waits until `done` holds.
    fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + WAIT;
        while !done() {
            assert!(Instant::now() < deadline, "{what} in time");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn asked(kind: PromptKind, text: &str) -> Event {
        Event::Asked {
            kind,
            text: text.to_owned(),
            visible: true,
        }
    }

    fn defaults() -> ExclusionsState {
        state(
            &["mstsc.exe", "msrdc.exe", "rdcman.exe", "vmconnect.exe"],
            true,
        )
    }

    #[test]
    fn the_dialog_shows_the_list_with_its_descriptions_and_closes_without_asking() {
        let opened = open(
            state(&["mstsc.exe", "putty.exe", "notepad.exe"], true),
            None,
        );
        let hwnd = opened.hwnd;
        assert_eq!(text(hwnd), TITLE);
        assert_eq!(
            rows(hwnd),
            [
                ["mstsc.exe", "Remote Desktop Connection"],
                ["putty.exe", "PuTTY: a free SSH client"],
                ["notepad.exe", "Notepad"],
            ]
            .map(|row| row.map(str::to_owned))
        );
        assert_eq!(selected(hwnd), -1);
        assert!(!is_enabled(hwnd, ID_REMOVE), "nothing selected");
        assert!(!is_enabled(hwnd, ID_ADD), "nothing typed");
        assert!(is_enabled(hwnd, ID_DEFAULTS));
        assert_eq!(
            combo_items(hwnd),
            running_programs()
                .iter()
                .map(program_entry)
                .collect::<Vec<_>>()
        );
        assert_eq!(label(hwnd, ID_APP), "");
        assert_eq!(label(hwnd, ID_INTRO).replace('\n', " "), INTRO);
        assert_eq!(label(hwnd, ID_NOT_SAVED), "");
        // Off every monitor.
        let mut frame = RECT::default();
        // SAFETY: `frame` is a valid out buffer.
        unsafe { GetWindowRect(hwnd, &mut frame) }.unwrap();
        assert!(frame.right < 0 && frame.bottom < 0, "{frame:?}");
        click(hwnd, IDCANCEL);
        assert_eq!(opened.closed(), [Event::Closing]);
    }

    /// A name typed (here without `.exe`, in capitals) and "Thêm": one request to add the
    /// executable, the edit emptied; the refresh of the tray lists it, and it is selected.
    #[test]
    fn a_new_name_is_sent_and_selected_once_the_tray_lists_it() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, "PuTTY");
        assert!(is_enabled(hwnd, ID_ADD));
        click(hwnd, ID_ADD);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Add(exe("putty.exe")))]
        );
        assert_eq!(label(hwnd, ID_APP), "");
        assert!(!is_enabled(hwnd, ID_ADD));
        assert_eq!(row_count(hwnd), 4, "the dialog changes no row itself");
        let mut added = defaults();
        added.apps.push(exe("putty.exe"));
        opened.refresh(added);
        wait_until("the refresh was shown", || row_count(hwnd) == 5);
        assert_eq!(cell(hwnd, 4, 0), "putty.exe");
        assert_eq!(cell(hwnd, 4, 1), "PuTTY: a free SSH client");
        assert_eq!(selected(hwnd), 4);
        assert!(is_enabled(hwnd, ID_REMOVE));
        assert_eq!(opened.waiting(), []);
    }

    /// A path of a listed application selects its row and sends nothing.
    #[test]
    fn a_listed_application_is_selected_and_nothing_is_sent() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, r#""C:\Windows\System32\msrdc.exe""#);
        click(hwnd, ID_ADD);
        assert_eq!(opened.waiting(), []);
        assert_eq!(selected(hwnd), 1);
        assert!(is_enabled(hwnd, ID_REMOVE));
    }

    /// A text that names no program is explained in a warning owned by the dialog, and nothing
    /// changes: no request, and the edit keeps the text to fix.
    #[test]
    fn an_invalid_name_is_explained_and_nothing_changes() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, r"C:\Tools\");
        click(hwnd, ID_ADD);
        assert_eq!(opened.waiting(), [asked(PromptKind::Warning, INVALID_NAME)]);
        assert_eq!(label(hwnd, ID_APP), r"C:\Tools\");
        assert_eq!(row_count(hwnd), 4);
    }

    /// Choosing an entry of the list of running applications puts its executable into the
    /// edit (not the entry with its description); "Thêm" then adds it.
    #[test]
    fn a_running_application_chosen_from_the_list_puts_its_executable_into_the_edit() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        let combo = item(hwnd, ID_APP);
        send(combo, CB_SETCURSEL, 1, 0);
        let wparam = usize::from(ID_APP) | (usize::from(CBN_SELCHANGE) << 16);
        send(hwnd, WM_COMMAND, wparam, combo.0 as isize);
        wait_until("the choice was put into the edit", || {
            label(hwnd, ID_APP) == "winword.exe"
        });
        assert!(is_enabled(hwnd, ID_ADD));
        click(hwnd, ID_ADD);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Add(exe("winword.exe")))]
        );
    }

    /// "Xoá" is enabled with a selection and asks to remove the selected application; a
    /// refresh without it leaves nothing selected.
    #[test]
    fn remove_sends_the_selected_application() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        select(hwnd, 2);
        assert!(is_enabled(hwnd, ID_REMOVE));
        click(hwnd, ID_REMOVE);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Remove(exe("rdcman.exe")))]
        );
        opened.refresh(state(&["mstsc.exe", "msrdc.exe", "vmconnect.exe"], true));
        wait_until("the refresh was shown", || row_count(hwnd) == 3);
        assert_eq!(selected(hwnd), -1);
        assert!(!is_enabled(hwnd, ID_REMOVE));
    }

    /// "Mặc định" asks first: "no" sends nothing, "yes" asks for the defaults.
    #[test]
    fn defaults_are_asked_for_after_yes_only() {
        let opened = open(state(&["putty.exe"], true), None);
        let hwnd = opened.hwnd;
        click(hwnd, ID_DEFAULTS);
        assert_eq!(
            opened.waiting(),
            [asked(PromptKind::Question, DEFAULTS_QUESTION)]
        );
        opened.answers.lock().unwrap().push_back(true);
        click(hwnd, ID_DEFAULTS);
        assert_eq!(
            opened.waiting(),
            [
                asked(PromptKind::Question, DEFAULTS_QUESTION),
                Event::Request(ExclusionsRequest::Defaults)
            ]
        );
    }

    /// "Duyệt…" asks for a program; the path chosen goes into the edit, and "Thêm" adds its
    /// executable. A cancelled choice changes nothing.
    #[test]
    fn browse_puts_the_chosen_path_into_the_edit() {
        let chosen = PathBuf::from(r"D:\Công cụ\PuTTY\putty.exe");
        let opened = open(defaults(), Some(chosen.clone()));
        let hwnd = opened.hwnd;
        click(hwnd, ID_BROWSE);
        assert_eq!(
            opened.waiting(),
            [Event::Browsed(vec![
                "Chương trình (*.exe) | *.exe".to_owned()
            ])]
        );
        assert_eq!(label(hwnd, ID_APP), chosen.to_str().unwrap());
        assert!(is_enabled(hwnd, ID_ADD));
        click(hwnd, ID_ADD);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Add(exe("putty.exe")))]
        );
        drop(opened);

        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, "đang gõ");
        click(hwnd, ID_BROWSE);
        assert_eq!(opened.waiting().len(), 1);
        assert_eq!(label(hwnd, ID_APP), "đang gõ");
    }

    /// A refresh shows the list the tray sends, and, without a folder, the note that nothing
    /// is saved.
    #[test]
    fn a_refresh_shows_the_new_list_and_the_note_without_a_folder() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        opened.refresh(state(&[], false));
        wait_until("the refresh was shown", || row_count(hwnd) == 0);
        assert_eq!(label(hwnd, ID_NOT_SAVED).replace('\n', " "), NOT_SAVED);
        opened.refresh(state(&["putty.exe"], true));
        wait_until("the second refresh was shown", || row_count(hwnd) == 1);
        assert_eq!(label(hwnd, ID_NOT_SAVED), "");
    }

    /// "Làm mới" fills the list of running applications again and keeps what the edit holds.
    #[test]
    fn refresh_lists_the_running_applications_again() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, "đang gõ");
        opened
            .running
            .lock()
            .unwrap()
            .push(program("putty.exe", None));
        click(hwnd, ID_REFRESH);
        assert_eq!(combo_items(hwnd).len(), 3);
        assert_eq!(combo_items(hwnd)[2], "putty.exe");
        assert_eq!(label(hwnd, ID_APP), "đang gõ");
        assert_eq!(opened.waiting(), []);
    }

    /// The tray's exit dismisses the dialog: it closes without a request.
    #[test]
    fn dismissing_closes_the_dialog_without_a_request() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, "putty");
        opened.dialog.dismiss();
        assert_eq!(opened.closed(), []);
        wait_until("the dialog was destroyed", || {
            // SAFETY: only queries whether the handle names a window.
            !unsafe { IsWindow(Some(hwnd)) }.as_bool()
        });
    }

    /// Opens the real dialog with real message boxes, the real Open dialog, and the running
    /// programs of this desktop; each request is applied to its list, which the dialog is
    /// shown again. Waits until it is closed. Manual test.
    #[test]
    #[ignore = "manual: shows the exclusions dialog and its boxes until you close it"]
    fn manual_exclusions_dialog() {
        struct Link {
            state: Shared,
            requests: Sender<ExclusionsRequest>,
        }
        impl ExclusionsLink for Link {
            fn state(&mut self) -> ExclusionsState {
                self.state.lock().unwrap().clone()
            }
            fn request(&mut self, request: ExclusionsRequest) {
                eprintln!("{request:?}");
                let _ = self.requests.send(request);
            }
        }
        let shared: Shared = Arc::new(Mutex::new(defaults()));
        let (requests, received) = mpsc::channel();
        let (closed_sender, closed) = mpsc::channel();
        let dialog = raumakey_controller::exclusions_dialog::spawn_exclusions(
            Link {
                state: Arc::clone(&shared),
                requests,
            },
            move || {
                let _ = closed_sender.send(());
            },
        )
        .unwrap();
        loop {
            if closed.try_recv().is_ok() {
                break;
            }
            if let Ok(request) = received.recv_timeout(Duration::from_millis(50)) {
                request.apply_to(&mut shared.lock().unwrap().apps);
                let _ = dialog.post(EXCLUSIONS_REFRESH, 0, 0);
            }
        }
    }
}
