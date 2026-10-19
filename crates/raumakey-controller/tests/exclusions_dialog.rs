//! The exclusions dialog (`exclusions_dialog`, D48): its template, texts, the descriptions and
//! cells of its rows, the entries of its list of running applications and what a choice of one
//! puts into the edit, what "Thêm" makes of the text typed, pasted, or chosen, and the row
//! selected after a refresh (host-independent); on Windows, the real dialog on a thread of its
//! own, off every monitor, never activated.

use raumakey_controller::exclusions::{ExclusionsState, default_apps};
use raumakey_controller::exclusions_dialog::{
    APP_LABEL, BROWSE_FAILED, COLUMNS, DEFAULTS_QUESTION, DIALOG_SIZE, EXCLUSIONS_REFRESH, Entered,
    ID_ADD, ID_APP, ID_BROWSE, ID_DEFAULTS, ID_INTRO, ID_LIST, ID_NOT_SAVED, ID_PICKER,
    ID_PICKER_HINT, ID_REFRESH, ID_REMOVE, INTRO, INVALID_NAME, NOT_SAVED, PICKER_HINT, PROGRAMS,
    Program, TITLE, browse_error_prompt, builtin_description, can_add, defaults_prompt, entered,
    invalid_name_prompt, neighbour, not_saved_text, picked_text, program_entry, row_cells,
    selection_after, template,
};
use raumakey_controller::macro_dialog::{Placement, PromptKind};
use raumakey_model::host::ExeName;
use raumakey_windows::dialog_box::is_reserved;
use raumakey_windows::dialog_template::{
    Control, ControlClass, DROPPED_LIST_HEIGHT, DialogRect, IDCANCEL, STATIC_ID, styles,
};
use raumakey_windows::error::WinError;
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
    assert_eq!(BROWSE_FAILED, "Không mở được hộp thoại chọn tệp");
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
    // The Open dialog could not be shown: why, as the notices tell an error of Windows.
    let failed = browse_error_prompt(&WinError::new(0x8004_0002_u32 as i32, "GetOpenFileNameW"));
    assert_eq!(failed.kind(), PromptKind::Warning);
    assert_eq!(
        failed.text(),
        "Không mở được hộp thoại chọn tệp (lỗi 0x80040002 tại GetOpenFileNameW)."
    );
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

fn program(name: &str, description: Option<&str>) -> Program {
    Program {
        exe: exe(name),
        description: description.map(str::to_owned),
    }
}

/// An entry of the running applications stands for its executable as the file reads it: a
/// process image without an extension (`tool`) is added as `tool.exe`, and the edit shows it
/// so once it is chosen; an image whose name the file cannot hold is put into the edit as it
/// is, and "Thêm" explains it.
#[test]
fn a_running_application_is_taken_as_the_file_reads_its_name() {
    let running = vec![
        program("tool", None),
        program("helper", Some("Trợ giúp")),
        program("...", None),
    ];
    assert_eq!(
        entered("tool", &[], &running),
        Entered::New(exe("tool.exe"))
    );
    assert_eq!(
        entered("helper \u{2013} Trợ giúp", &[], &running),
        Entered::New(exe("helper.exe"))
    );
    assert_eq!(
        entered("helper \u{2013} Trợ giúp", &[exe("helper.exe")], &running),
        Entered::Listed(0)
    );
    assert_eq!(entered("...", &[], &running), Entered::Invalid);
    assert_eq!(picked_text(&running[0]), "tool.exe");
    assert_eq!(picked_text(&running[1]), "helper.exe");
    assert_eq!(picked_text(&running[2]), "...");
    assert_eq!(
        picked_text(&program("mstsc.exe", Some("Remote Desktop Connection"))),
        "mstsc.exe"
    );
}

// ---- Selection after a refresh ----

/// After a refresh the row of the application just added is selected once it is listed;
/// otherwise the application selected before stays selected where it is now; otherwise the
/// neighbour of the application just removed, which took its place; or nothing.
#[test]
fn the_selection_follows_the_application_across_a_refresh() {
    let apps = vec![exe("msrdc.exe"), exe("putty.exe"), exe("notepad.exe")];
    let (putty, notepad, gone) = (exe("putty.exe"), exe("notepad.exe"), exe("gone.exe"));
    assert_eq!(
        selection_after(Some(&putty), Some(&notepad), None, &apps),
        Some(2)
    );
    assert_eq!(
        selection_after(Some(&putty), Some(&gone), None, &apps),
        Some(1)
    );
    assert_eq!(selection_after(Some(&putty), None, None, &apps), Some(1));
    assert_eq!(
        selection_after(Some(&exe("mstsc.exe")), None, None, &apps),
        None
    );
    assert_eq!(selection_after(None, None, None, &apps), None);
    // The application removed is gone: its neighbour is selected.
    assert_eq!(
        selection_after(Some(&gone), None, Some(&notepad), &apps),
        Some(2)
    );
    // Not yet gone, or the user chose another row meanwhile: that row stays.
    assert_eq!(
        selection_after(Some(&putty), None, Some(&notepad), &apps),
        Some(1)
    );
    assert_eq!(selection_after(None, None, Some(&gone), &apps), None);
}

/// The row that takes the place of a removed one: the next, else the one before it (the last
/// row was removed), else none (it was the only one).
#[test]
fn the_neighbour_of_a_removed_row_takes_its_place() {
    let apps = vec![exe("a.exe"), exe("b.exe"), exe("c.exe")];
    assert_eq!(neighbour(&apps, 0), Some(&apps[1]));
    assert_eq!(neighbour(&apps, 1), Some(&apps[2]));
    assert_eq!(neighbour(&apps, 2), Some(&apps[1]));
    assert_eq!(neighbour(&apps, 3), None, "no such row");
    assert_eq!(neighbour(&apps[..1], 0), None);
    assert_eq!(neighbour(&[], 0), None);
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

    // D55: the window picker, and its hint on two lines.
    let picker = find(controls, ID_PICKER);
    assert_eq!(picker.class(), ControlClass::Static);
    assert_eq!(picker.style() & 0x0F, styles::SS_ICON);
    assert_eq!(
        picker.style() & styles::SS_NOTIFY,
        0,
        "the dialog gets the press"
    );
    let hint = find(controls, ID_PICKER_HINT);
    assert_eq!(hint.class(), ControlClass::Static);
    assert_eq!(hint.text(), PICKER_HINT);
    assert!(hint.rect().cy >= 16, "two lines");
    assert!(
        estimated_width(PICKER_HINT) <= hint.rect().cx * 2,
        "the hint fits its two lines"
    );

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
/// questions are answered from a script, the Open dialog answers a scripted path (or error),
/// the running programs are made up, and its link to the tray is a shared state and a
/// channel. Every wait has a timeout.
#[cfg(windows)]
mod win {
    use std::collections::{BTreeMap, VecDeque};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use raumakey_controller::exclusions::{ExclusionsRequest, ExclusionsState};
    use raumakey_controller::exclusions_dialog::{
        DEFAULTS_QUESTION, EXCLUSIONS_REFRESH, ExclusionsLink, ID_ADD, ID_APP, ID_BROWSE,
        ID_DEFAULTS, ID_INTRO, ID_LIST, ID_NOT_SAVED, ID_PICKER, ID_REFRESH, ID_REMOVE, INTRO,
        INVALID_NAME, NOT_SAVED, Program, Programs, TITLE, browse_error_prompt, program_entry,
        spawn_exclusions_with,
    };
    use raumakey_controller::macro_dialog::{Placement, Prompt, PromptKind, Prompter};
    use raumakey_model::host::ExeName;
    use raumakey_windows::dialog_box::{DetachedDialog, Dialog};
    use raumakey_windows::dialog_template::IDCANCEL;
    use raumakey_windows::error::{E_ABORT, WinError, WinResult};
    use raumakey_windows::file_dialog::FileFilter;
    use raumakey_windows::strings::until_nul;
    use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::ScreenToClient;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::Controls::{
        LVIS_SELECTED, LVITEMW, LVM_GETITEMCOUNT, LVM_GETITEMTEXTW, LVM_GETNEXTITEM,
        LVM_SETITEMSTATE, LVNI_SELECTED,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled;
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_CLICK, CB_GETCOUNT, CB_GETLBTEXT, CB_GETLBTEXTLEN, GUITHREADINFO, GW_CHILD, GetDlgItem,
        GetGUIThreadInfo, GetWindow, GetWindowRect, GetWindowThreadProcessId, IsWindow,
        IsWindowVisible, SMTO_NORMAL, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
        SendMessageTimeoutW, SetWindowPos, WM_CHAR, WM_GETTEXT, WM_GETTEXTLENGTH, WM_KEYDOWN,
        WM_NEXTDLGCTL, WM_NULL,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, WM_CAPTURECHANGED, WM_LBUTTONDOWN, WM_LBUTTONUP,
        WM_MOUSEMOVE, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP, WS_VISIBLE,
    };
    use windows::core::PWSTR;
    use windows::core::w;

    use super::common_controls_v6::{CCM_GETVERSION, CommonControls6};
    use super::{exe, program, state};

    const WAIT: Duration = Duration::from_secs(30);
    /// Milliseconds a sent message may take before the test fails.
    const SEND_TIMEOUT_MS: u32 = 30_000;

    /// `VK_DOWN`.
    const VK_DOWN: u16 = 0x28;
    /// `VK_DELETE`.
    const VK_DELETE: u16 = 0x2E;
    /// The scan codes of the arrow down and Delete keys of the navigation block, in the
    /// `lParam` of their `WM_KEYDOWN` (extended keys).
    const SCAN_DOWN: isize = 0x0150_0001;
    const SCAN_DELETE: isize = 0x0153_0001;

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
    /// "no") and the Open dialog with `chosen`, without showing anything. While the Open
    /// dialog is "open", the tray sends `meanwhile`: the shared state changes, and the refresh
    /// posted then is lost (as one posted during a modal loop is).
    struct FakePrompter {
        events: Sender<Event>,
        answers: Answers,
        chosen: WinResult<Option<PathBuf>>,
        meanwhile: Option<ExclusionsState>,
        shared: Shared,
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
        ) -> WinResult<Option<PathBuf>> {
            let patterns = filters
                .iter()
                .map(|filter| format!("{} | {}", filter.label, filter.patterns))
                .collect();
            let _ = self.events.send(Event::Browsed(patterns));
            if let Some(state) = self.meanwhile.take() {
                *self.shared.lock().unwrap() = state;
            }
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

        /// The windows of the test process belong to `putty.exe`; no other process is known.
        fn process_exe(&mut self, process_id: u32) -> Option<ExeName> {
            // SAFETY: no preconditions.
            let own = unsafe { GetCurrentProcessId() };
            (process_id == own).then(|| exe("putty.exe"))
        }
    }

    /// The tray as the dialog sees it: the shared state, and the requests into a channel.
    /// `starting` is what the tray sends while the dialog starts: the shared state changes
    /// once the dialog read it the first time, and the refresh posted then is refused (the
    /// dialog's window is not published yet). `reads` counts the reads of the state.
    struct FakeLink {
        state: Shared,
        events: Sender<Event>,
        starting: Option<ExclusionsState>,
        reads: Arc<AtomicUsize>,
    }

    impl ExclusionsLink for FakeLink {
        fn state(&mut self) -> ExclusionsState {
            let mut shared = self.state.lock().unwrap();
            let current = shared.clone();
            if let Some(next) = self.starting.take() {
                *shared = next;
            }
            self.reads.fetch_add(1, Ordering::SeqCst);
            current
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
        reads: Arc<AtomicUsize>,
        hwnd: HWND,
    }

    impl Drop for Opened {
        fn drop(&mut self) {
            // A failed test leaves no dialog behind.
            self.dialog.dismiss();
        }
    }

    /// The running applications of the tests.
    fn running_programs() -> Vec<Program> {
        vec![
            program("notepad.exe", Some("Notepad")),
            program("winword.exe", Some("Microsoft Word")),
        ]
    }

    /// How a test opens the dialog (see [`FakePrompter`] and [`FakeLink`]).
    struct Setup {
        chosen: WinResult<Option<PathBuf>>,
        meanwhile: Option<ExclusionsState>,
        starting: Option<ExclusionsState>,
        running: Vec<Program>,
    }

    impl Default for Setup {
        fn default() -> Self {
            Self {
                chosen: Ok(None),
                meanwhile: None,
                starting: None,
                running: running_programs(),
            }
        }
    }

    /// Opens the dialog on `initial`, the Open dialog answering `chosen`, and waits until it
    /// is shown (far off-screen).
    fn open(initial: ExclusionsState, chosen: Option<PathBuf>) -> Opened {
        open_with(
            initial,
            Setup {
                chosen: Ok(chosen),
                ..Setup::default()
            },
        )
    }

    /// [`open`] as `setup` has it.
    fn open_with(initial: ExclusionsState, setup: Setup) -> Opened {
        let (sender, events) = mpsc::channel();
        let answers: Answers = Arc::default();
        let shared: Shared = Arc::new(Mutex::new(initial));
        let running: Running = Arc::new(Mutex::new(setup.running));
        let reads = Arc::new(AtomicUsize::new(0));
        let prompter = FakePrompter {
            events: sender.clone(),
            answers: Arc::clone(&answers),
            chosen: setup.chosen,
            meanwhile: setup.meanwhile,
            shared: Arc::clone(&shared),
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
            starting: setup.starting,
            reads: Arc::clone(&reads),
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
            reads,
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

        /// [`refresh`](Self::refresh), and waits until the dialog has shown it.
        fn refreshed(&self, state: ExclusionsState) {
            let before = self.reads.load(Ordering::SeqCst);
            self.refresh(state);
            wait_until("the refresh was shown", || {
                self.reads.load(Ordering::SeqCst) > before
            });
            // The read comes before the rows are set: the dialog answers once it is done.
            send(self.hwnd, WM_NULL, 0, 0);
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

    /// The edit of the combo box: its only child.
    fn combo_edit(dialog: HWND) -> HWND {
        // SAFETY: only reads the window list.
        unsafe { GetWindow(item(dialog, ID_APP), GW_CHILD) }.unwrap()
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

    /// Types `typed` at the end of the empty edit of the combo box as the keyboard does
    /// (`WM_CHAR`): the edit reports each change to the combo box, which reports
    /// `CBN_EDITCHANGE` to the dialog. (A `WM_SETTEXT`, to the combo box or its edit, reports
    /// nothing, as the handler's own changes do not.)
    fn type_into_edit(dialog: HWND, typed: &str) {
        let edit = combo_edit(dialog);
        for unit in typed.encode_utf16() {
            send(edit, WM_CHAR, usize::from(unit), 1);
        }
    }

    /// The key `key` (with the `lParam` of its scan code) pressed in `window`, as the keyboard
    /// sends it to the window with the focus.
    fn press(window: HWND, key: u16, scan: isize) {
        send(window, WM_KEYDOWN, usize::from(key), scan);
    }

    fn click(dialog: HWND, id: u16) {
        send(item(dialog, id), BM_CLICK, 0, 0);
    }

    /// Gives control `id` the keyboard focus, as a click with the mouse on it does (the dialog
    /// manager's `WM_NEXTDLGCTL`).
    fn focus_on(dialog: HWND, id: u16) {
        send(dialog, WM_NEXTDLGCTL, item(dialog, id).0 as usize, 1);
    }

    /// The window with the keyboard focus of the dialog's thread (null for none).
    fn focus_of(dialog: HWND) -> HWND {
        // SAFETY: only reads the thread of a window.
        let thread = unsafe { GetWindowThreadProcessId(dialog, None) };
        let mut info = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        // SAFETY: `info` is a valid out buffer with its size set.
        unsafe { GetGUIThreadInfo(thread, &mut info) }.unwrap();
        info.hwndFocus
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

    fn browsed() -> Event {
        Event::Browsed(vec!["Chương trình (*.exe) | *.exe".to_owned()])
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
        assert_eq!(focus_of(hwnd), combo_edit(hwnd), "the edit has the focus");
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

    /// A list the tray sends while the dialog is starting (its refresh is refused before the
    /// dialog's window is published) is shown once the dialog is up.
    #[test]
    fn a_list_sent_while_the_dialog_starts_is_shown_once_it_is_up() {
        let opened = open_with(
            defaults(),
            Setup {
                starting: Some(state(&["putty.exe"], false)),
                ..Setup::default()
            },
        );
        let hwnd = opened.hwnd;
        // `open` waited until the dialog was shown: what `init` posted itself came before.
        assert_eq!(
            rows(hwnd),
            [[
                "putty.exe".to_owned(),
                "PuTTY: a free SSH client".to_owned()
            ]]
        );
        assert_eq!(label(hwnd, ID_NOT_SAVED).replace('\n', " "), NOT_SAVED);
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
        // The tray shows the list again at once, before the worker answers: nothing moves.
        opened.refreshed(defaults());
        assert_eq!(selected(hwnd), -1);
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

    /// A click on "Thêm" gives it the focus; once the name is sent and the edit emptied,
    /// "Thêm" is disabled and the focus is in the edit, for the next name: never on the
    /// disabled button, nor nowhere.
    #[test]
    fn after_a_click_on_add_the_focus_is_in_the_edit() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, "putty");
        focus_on(hwnd, ID_ADD);
        assert_eq!(focus_of(hwnd), item(hwnd, ID_ADD));
        click(hwnd, ID_ADD);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Add(exe("putty.exe")))]
        );
        assert!(!is_enabled(hwnd, ID_ADD));
        assert_eq!(focus_of(hwnd), combo_edit(hwnd));
    }

    /// An application added that the tray's answer does not list (the edit was overtaken, by
    /// a hand edit or another dialog) is given up at that answer: a later list that holds it
    /// does not select it.
    #[test]
    fn an_added_application_that_the_answer_does_not_list_is_given_up() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, "putty");
        click(hwnd, ID_ADD);
        assert_eq!(opened.waiting().len(), 1);
        opened.refreshed(defaults());
        opened.refresh(state(&["mstsc.exe"], true));
        wait_until("the answer was shown", || row_count(hwnd) == 1);
        assert_eq!(selected(hwnd), -1);
        opened.refresh(state(&["mstsc.exe", "putty.exe"], true));
        wait_until("the later list was shown", || row_count(hwnd) == 2);
        assert_eq!(selected(hwnd), -1, "not selected any more");
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

    /// Choosing an entry of the list of running applications with the keyboard (the arrow key
    /// in the closed combo box) puts its executable into the edit, not the entry with its
    /// description, which the combo box shows first; "Thêm" then adds it.
    #[test]
    fn a_running_application_chosen_with_the_keyboard_puts_its_executable_into_the_edit() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        press(combo_edit(hwnd), VK_DOWN, SCAN_DOWN);
        wait_until("the first choice was put into the edit", || {
            label(hwnd, ID_APP) == "notepad.exe"
        });
        assert!(is_enabled(hwnd, ID_ADD));
        press(combo_edit(hwnd), VK_DOWN, SCAN_DOWN);
        wait_until("the second choice was put into the edit", || {
            label(hwnd, ID_APP) == "winword.exe"
        });
        click(hwnd, ID_ADD);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Add(exe("winword.exe")))]
        );
    }

    /// A running program whose image has no extension is chosen and added as the file reads
    /// it: `tool.exe`.
    #[test]
    fn a_running_application_without_an_extension_is_added_as_the_file_reads_it() {
        let opened = open_with(
            defaults(),
            Setup {
                running: vec![program("tool", None)],
                ..Setup::default()
            },
        );
        let hwnd = opened.hwnd;
        assert_eq!(combo_items(hwnd), ["tool"]);
        press(combo_edit(hwnd), VK_DOWN, SCAN_DOWN);
        wait_until("the choice was put into the edit", || {
            label(hwnd, ID_APP) == "tool.exe"
        });
        click(hwnd, ID_ADD);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Add(exe("tool.exe")))]
        );
    }

    /// "Xoá" is enabled with a selection and asks to remove the selected application, and keeps
    /// the focus: once the tray's list no longer holds it, the row that took its place is
    /// selected (the one before after the last row), so "Xoá" stays enabled and removes the
    /// next one; once the list is empty, the focus goes to the edit before "Xoá" is disabled.
    #[test]
    fn remove_sends_the_selected_application_and_selects_its_neighbour() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        select(hwnd, 2);
        assert!(is_enabled(hwnd, ID_REMOVE));
        focus_on(hwnd, ID_REMOVE);
        click(hwnd, ID_REMOVE);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Remove(exe("rdcman.exe")))]
        );
        // The tray shows the list again at once, before the worker answers: nothing moves.
        opened.refreshed(defaults());
        assert_eq!(selected(hwnd), 2);
        // The answer: the row of vmconnect.exe took its place and is selected.
        opened.refresh(state(&["mstsc.exe", "msrdc.exe", "vmconnect.exe"], true));
        wait_until("the answer was shown", || row_count(hwnd) == 3);
        assert_eq!(selected(hwnd), 2);
        assert!(is_enabled(hwnd, ID_REMOVE));
        assert_eq!(focus_of(hwnd), item(hwnd, ID_REMOVE));

        // Again: the last row goes, and the one before it is selected.
        click(hwnd, ID_REMOVE);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Remove(exe(
                "vmconnect.exe"
            )))]
        );
        opened.refresh(state(&["mstsc.exe", "msrdc.exe"], true));
        wait_until("the second answer was shown", || row_count(hwnd) == 2);
        assert_eq!(selected(hwnd), 1);
        assert_eq!(focus_of(hwnd), item(hwnd, ID_REMOVE));

        // Down to the only row: once it is gone, the focus is in the edit.
        opened.refreshed(state(&["msrdc.exe"], true));
        assert_eq!(selected(hwnd), 0);
        click(hwnd, ID_REMOVE);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Remove(exe("msrdc.exe")))]
        );
        opened.refresh(state(&[], true));
        wait_until("the empty list was shown", || row_count(hwnd) == 0);
        assert!(!is_enabled(hwnd, ID_REMOVE));
        assert_eq!(focus_of(hwnd), combo_edit(hwnd));
    }

    /// The Delete key in the list removes the selected application, as "Xoá" does; with
    /// nothing selected it does nothing, and other keys do nothing.
    #[test]
    fn the_delete_key_in_the_list_removes_the_selected_application() {
        let opened = open(defaults(), None);
        let hwnd = opened.hwnd;
        let list = item(hwnd, ID_LIST);
        press(list, VK_DELETE, SCAN_DELETE);
        assert_eq!(opened.waiting(), []);
        select(hwnd, 1);
        press(list, VK_DOWN, SCAN_DOWN);
        assert_eq!(opened.waiting(), []);
        let row = usize::try_from(selected(hwnd)).unwrap();
        press(list, VK_DELETE, SCAN_DELETE);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Remove(
                defaults().apps[row].clone()
            ))]
        );
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
        assert_eq!(opened.waiting(), [browsed()]);
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

    /// A refresh posted while the Open dialog was open did not reach the dialog: it shows the
    /// shared state once the Open dialog is gone.
    #[test]
    fn after_the_open_dialog_the_dialog_shows_the_state_again() {
        let chosen = PathBuf::from(r"C:\Tools\putty.exe");
        let opened = open_with(
            defaults(),
            Setup {
                chosen: Ok(Some(chosen.clone())),
                meanwhile: Some(state(&["putty.exe"], true)),
                ..Setup::default()
            },
        );
        let hwnd = opened.hwnd;
        click(hwnd, ID_BROWSE);
        assert_eq!(opened.waiting(), [browsed()]);
        assert_eq!(
            rows(hwnd),
            [[
                "putty.exe".to_owned(),
                "PuTTY: a free SSH client".to_owned()
            ]]
        );
        assert_eq!(label(hwnd, ID_APP), chosen.to_str().unwrap());
    }

    /// An Open dialog that cannot be shown is explained in a warning owned by the dialog, with
    /// the error; nothing else changes. One that failed because the dialog is being dismissed
    /// tells nothing.
    #[test]
    fn an_open_dialog_that_cannot_be_shown_is_explained() {
        let error = WinError::new(0x8004_0002_u32 as i32, "GetOpenFileNameW");
        let opened = open_with(
            defaults(),
            Setup {
                chosen: Err(error),
                ..Setup::default()
            },
        );
        let hwnd = opened.hwnd;
        type_into_edit(hwnd, "đang gõ");
        click(hwnd, ID_BROWSE);
        assert_eq!(
            opened.waiting(),
            [
                browsed(),
                asked(PromptKind::Warning, browse_error_prompt(&error).text())
            ]
        );
        assert_eq!(label(hwnd, ID_APP), "đang gõ");
        assert_eq!(row_count(hwnd), 4);
        drop(opened);

        let opened = open_with(
            defaults(),
            Setup {
                chosen: Err(WinError::new(E_ABORT, "GetOpenFileNameW")),
                ..Setup::default()
            },
        );
        click(opened.hwnd, ID_BROWSE);
        assert_eq!(opened.waiting(), [browsed()]);
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

    /// Where the target window of the picker tests lies: far off every monitor, apart from the
    /// dialog.
    const TARGET: (i32, i32) = (-20_000, -20_000);

    /// A plain top-level window of the test process at [`TARGET`], visible but off every
    /// monitor and never activated; a button, so that hit testing finds it.
    struct Target(HWND);

    impl Target {
        fn new() -> Self {
            // SAFETY: creates a window of a system class with no extra data; it is destroyed
            // on drop.
            let hwnd = unsafe {
                CreateWindowExW(
                    WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                    w!("BUTTON"),
                    w!("Raumakey target"),
                    WS_POPUP | WS_VISIBLE,
                    TARGET.0,
                    TARGET.1,
                    200,
                    200,
                    None,
                    None,
                    None,
                    None,
                )
            }
            .unwrap();
            Self(hwnd)
        }

        fn inside(&self) -> (i32, i32) {
            (TARGET.0 + 100, TARGET.1 + 100)
        }
    }

    impl Drop for Target {
        fn drop(&mut self) {
            // SAFETY: a window this thread created.
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }

    /// The center of control `id` of `dialog`, on the screen.
    fn center(dialog: HWND, id: u16) -> (i32, i32) {
        let mut frame = RECT::default();
        // SAFETY: `frame` is a valid out pointer.
        unsafe { GetWindowRect(item(dialog, id), &mut frame) }.unwrap();
        (
            (frame.left + frame.right) / 2,
            (frame.top + frame.bottom) / 2,
        )
    }

    /// Sends the mouse `message` at `screen` to `dialog`, as the pointer would.
    fn mouse(dialog: HWND, message: u32, screen: (i32, i32)) {
        let mut point = POINT {
            x: screen.0,
            y: screen.1,
        };
        // SAFETY: `point` is a valid in/out pointer.
        assert!(unsafe { ScreenToClient(dialog, &mut point) }.as_bool());
        let x = (point.x as i16 as u16) as isize;
        let y = (point.y as i16 as u16) as isize;
        let keys = if message == WM_LBUTTONUP { 0 } else { 1 };
        send(dialog, message, keys, x | (y << 16));
    }

    /// D55: a drag from the picker puts the executable of the window under the pointer into
    /// the edit while it goes on and the text typed before over no window (or the dialog
    /// itself); a lost capture puts the typed text back; a drop keeps the executable, with
    /// "Thêm" enabled and the focus in the edit, ready to add it.
    #[test]
    fn d55_a_drag_from_the_picker_puts_the_executable_into_the_edit() {
        let target = Target::new();
        let opened = open(state(&["mstsc.exe"], true), None);
        let hwnd = opened.hwnd;
        // A place of its own, which no dialog of another test covers (they lie at -32000).
        // SAFETY: moves a window without activating it; no pointers.
        unsafe {
            SetWindowPos(
                hwnd,
                None,
                -26_000,
                -26_000,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            )
        }
        .unwrap();
        let picker = center(hwnd, ID_PICKER);
        type_into_edit(hwnd, "abc");

        mouse(hwnd, WM_LBUTTONDOWN, picker);
        mouse(hwnd, WM_MOUSEMOVE, target.inside());
        assert_eq!(text(combo_edit(hwnd)), "putty.exe");
        mouse(hwnd, WM_MOUSEMOVE, picker);
        assert_eq!(text(combo_edit(hwnd)), "abc");
        mouse(hwnd, WM_MOUSEMOVE, target.inside());
        assert_eq!(text(combo_edit(hwnd)), "putty.exe");
        send(hwnd, WM_CAPTURECHANGED, 0, 0);
        assert_eq!(
            text(combo_edit(hwnd)),
            "abc",
            "a cancelled drag puts the text back"
        );

        mouse(hwnd, WM_LBUTTONDOWN, picker);
        mouse(hwnd, WM_MOUSEMOVE, target.inside());
        mouse(hwnd, WM_LBUTTONUP, target.inside());
        assert_eq!(text(combo_edit(hwnd)), "putty.exe");
        assert!(is_enabled(hwnd, ID_ADD));
        assert_eq!(focus_of(hwnd), combo_edit(hwnd));
        assert_eq!(opened.waiting(), [], "a drop adds nothing by itself");
        click(hwnd, ID_ADD);
        assert_eq!(
            opened.waiting(),
            [Event::Request(ExclusionsRequest::Add(exe("putty.exe")))]
        );

        // A drop over no known program puts the text back.
        type_into_edit(hwnd, "xyz");
        mouse(hwnd, WM_LBUTTONDOWN, picker);
        mouse(hwnd, WM_LBUTTONUP, picker);
        assert_eq!(text(combo_edit(hwnd)), "xyz");
    }
}
