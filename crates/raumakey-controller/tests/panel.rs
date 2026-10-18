//! The settings panel (`panel`, D43; design §22.1) as pure data: the state it shows (built
//! from the tray's settings), what each control shows for it, the request each user action
//! on a control makes, and the dialog template (controls, ids, Vietnamese labels, layout).
//! Pure: these tests run on every host.

use std::path::{Path, PathBuf};

use raumakey_config::StartupMode;
use raumakey_controller::install::PRODUCT_VERSION;
use raumakey_controller::panel::{
    CHECK_BOXES, ControlValue, DEFAULTS_QUESTION, ID_AUTO_RESTORE, ID_CLOSE, ID_CONFIG_FILE,
    ID_CONFIG_LOCATION, ID_DEFAULTS, ID_FREE_MARKING, ID_HINT, ID_MACRO_AUTO_CASE, ID_MACRO_LIST,
    ID_MACROS, ID_MACROS_WHEN_ENGLISH, ID_METHOD, ID_MODERN_TONE, ID_OPEN_CONFIG, ID_SHOW_ON_START,
    ID_SPELL_CHECK, ID_STARTUP, ID_VIETNAMESE, OFF_SCREEN, PANEL_REFRESH, PanelRequest, PanelState,
    PanelView, ValueKind, confirmation, off_screen_panel_template, panel_template, request,
    value_kind,
};
use raumakey_controller::settings::{Change, Location};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::hotkey::Hotkey;
use raumakey_model::keymap::{KeyAction, KeyBinding, KeyMap, Tone};
use raumakey_model::options::InputMethod;
use raumakey_windows::dialog_box::is_reserved;
use raumakey_windows::dialog_box::notifications::{BN_CLICKED, CBN_SELCHANGE, EN_CHANGE};
use raumakey_windows::dialog_template::{
    Control, ControlClass, DROPPED_LIST_HEIGHT, DialogRect, DialogTemplate, IDCANCEL, IDOK,
    STATIC_ID, styles,
};
use raumakey_windows::window::is_postable;

fn config_path() -> PathBuf {
    PathBuf::from(r"D:\Tools\Raumakey\raumakey.toml")
}

/// The runtime snapshot of a fresh `raumakey.toml`: the defaults, Vietnamese mode, Telex.
fn runtime() -> RuntimeSnapshot {
    let mut runtime = RuntimeSnapshot::defaults_v1();
    runtime.config_revision = 1;
    runtime
}

fn state_of(runtime: &RuntimeSnapshot) -> PanelState {
    PanelState::new(
        runtime,
        StartupMode::None,
        true,
        Location::ExeDir,
        Some(&config_path()),
    )
}

fn state() -> PanelState {
    state_of(&runtime())
}

/// A key map with one binding, `id`.
fn keymap(id: &str) -> KeyMap {
    KeyMap {
        id: id.to_owned(),
        version: 1,
        name: "Mine".to_owned(),
        bindings: vec![KeyBinding {
            key: 's',
            actions: vec![KeyAction::Tone(Tone::Acute)],
        }],
    }
}

// --- The state --------------------------------------------------------------------------------

/// Every value of the panel comes from the tray's settings: the runtime snapshot (mode, the
/// method in effect, the options, the toggle hotkey), the startup mode, the panel at start,
/// and where the file lives.
#[test]
fn the_state_is_built_from_the_settings_in_effect() {
    let state = state();
    assert_eq!(
        state,
        PanelState {
            vietnamese: true,
            method: InputMethod::Telex,
            user_defined_listed: false,
            modern_tone: false,
            free_marking: true,
            spell_check: true,
            auto_restore: true,
            macros: true,
            macros_when_english: false,
            macro_auto_case: true,
            startup: StartupMode::None,
            show_on_start: true,
            toggle_hotkey: Some("Ctrl+Shift".to_owned()),
            location: Location::ExeDir,
            config_path: Some(config_path()),
        }
    );
    let mut runtime = runtime();
    runtime.vietnamese_mode = false;
    runtime.options.enabled = false;
    runtime.options.input_method = InputMethod::Viqr;
    runtime.options.modern_tone = true;
    runtime.options.free_marking = false;
    runtime.options.spell_check = false;
    runtime.options.auto_restore_invalid = false;
    runtime.options.macros_enabled = false;
    runtime.options.macros_when_disabled = true;
    runtime.options.auto_case_macro = false;
    runtime.hotkeys.toggle_mode = vec![
        "Ctrl+Alt+Z".parse::<Hotkey>().unwrap(),
        "Ctrl+Shift".parse::<Hotkey>().unwrap(),
    ];
    let state = PanelState::new(
        &runtime,
        StartupMode::Shortcut,
        false,
        Location::LocalAppData,
        Some(Path::new(
            r"C:\Users\An\AppData\Local\Raumakey\raumakey.toml",
        )),
    );
    assert!(!state.vietnamese);
    assert_eq!(state.method, InputMethod::Viqr);
    assert!(state.modern_tone);
    assert!(!state.free_marking);
    assert!(!state.spell_check);
    assert!(!state.auto_restore);
    assert!(!state.macros);
    assert!(state.macros_when_english);
    assert!(!state.macro_auto_case);
    assert_eq!(state.startup, StartupMode::Shortcut);
    assert!(!state.show_on_start);
    assert_eq!(
        state.toggle_hotkey.as_deref(),
        Some("Ctrl+Alt+Z"),
        "the first toggle hotkey"
    );
    assert_eq!(state.location, Location::LocalAppData);
    runtime.hotkeys.toggle_mode.clear();
    assert_eq!(state_of(&runtime).toggle_hotkey, None);
}

/// "Kiểu gõ riêng" is listed when a user method can take effect: it is in effect, or the
/// snapshot carries the key map that `user_keymap` names (choosing Telex keeps the item, so
/// the user can choose it again). Without a usable key map it is not listed.
#[test]
fn the_user_method_is_listed_only_with_a_usable_key_map() {
    let mut runtime = runtime();
    assert!(!state_of(&runtime).user_defined_listed);
    runtime.options.user_keymap_id = Some("mine".to_owned());
    assert!(
        !state_of(&runtime).user_defined_listed,
        "an id without its key map"
    );
    runtime.keymaps = vec![keymap("other")];
    assert!(!state_of(&runtime).user_defined_listed);
    runtime.keymaps.push(keymap("mine"));
    let telex = state_of(&runtime);
    assert!(telex.user_defined_listed, "the key map can be chosen");
    assert_eq!(telex.method, InputMethod::Telex);
    runtime.options.input_method = InputMethod::UserDefined;
    let user = state_of(&runtime);
    assert!(user.user_defined_listed);
    assert_eq!(user.method, InputMethod::UserDefined);
}

#[test]
fn nothing_can_be_saved_without_a_folder() {
    assert!(state().can_save());
    let nowhere = PanelState::new(
        &runtime(),
        StartupMode::None,
        true,
        Location::Unavailable,
        None,
    );
    assert!(!nowhere.can_save());
}

// --- What the controls show ---------------------------------------------------------------

/// The hint names the first toggle hotkey; without one it names the icon only.
#[test]
fn the_hint_names_the_toggle_hotkey_and_the_icon() {
    assert_eq!(
        state().hint(),
        "Bật/tắt nhanh: Ctrl+Shift hoặc nhấp biểu tượng V/E"
    );
    let mut runtime = runtime();
    runtime.hotkeys.toggle_mode = vec!["Ctrl+Alt+Z".parse::<Hotkey>().unwrap()];
    assert_eq!(
        state_of(&runtime).hint(),
        "Bật/tắt nhanh: Ctrl+Alt+Z hoặc nhấp biểu tượng V/E"
    );
    runtime.hotkeys.toggle_mode.clear();
    assert_eq!(
        state_of(&runtime).hint(),
        "Bật/tắt nhanh: nhấp biểu tượng V/E"
    );
}

/// The footer names `raumakey.toml` on a line of its own and where it lives on the next, as
/// the welcome of a first run does (with the reason for `%LOCALAPPDATA%`); without a folder
/// its one line says that changes are not saved.
#[test]
fn the_footer_names_the_configuration_file_and_where_it_lives() {
    assert_eq!(
        state().config_file(),
        r"Tệp cấu hình: D:\Tools\Raumakey\raumakey.toml"
    );
    assert_eq!(state().config_location(), "(cạnh raumakey.exe)");
    let local = Path::new(r"C:\Users\An\AppData\Local\Raumakey\raumakey.toml");
    let state = PanelState::new(
        &runtime(),
        StartupMode::None,
        true,
        Location::LocalAppData,
        Some(local),
    );
    assert_eq!(
        state.config_file(),
        r"Tệp cấu hình: C:\Users\An\AppData\Local\Raumakey\raumakey.toml"
    );
    assert_eq!(
        state.config_location(),
        "(trong %LOCALAPPDATA%\\Raumakey, vì thư mục của raumakey.exe không ghi được hoặc là thư \
         mục được bảo vệ)"
    );
    let nowhere = PanelState::new(
        &runtime(),
        StartupMode::None,
        true,
        Location::Unavailable,
        None,
    );
    assert_eq!(
        nowhere.config_file(),
        "Tệp cấu hình: không có, nên các thay đổi không được lưu."
    );
    assert_eq!(nowhere.config_location(), "");
}

/// The path label shortens a long path in its middle (`SS_PATHELLIPSIS`), which keeps what
/// follows its last backslash: that is the file name, never a part of the location note.
#[test]
fn the_path_label_ends_with_the_file_name() {
    for (location, path) in [
        (Location::ExeDir, r"D:\Tools\Raumakey\raumakey.toml"),
        (
            Location::LocalAppData,
            r"C:\Users\anh\AppData\Local\Raumakey\raumakey.toml",
        ),
        (
            Location::LocalAppData,
            r"C:\Users\Nguyen Van Anh\AppData\Local\Raumakey\raumakey.toml",
        ),
    ] {
        let state = PanelState::new(
            &runtime(),
            StartupMode::None,
            true,
            location,
            Some(Path::new(path)),
        );
        let text = state.config_file();
        let last = text.rsplit('\\').next().unwrap();
        assert!(last.starts_with("raumakey.toml"), "{text}");
        assert!(!state.config_location().is_empty(), "{location:?}");
    }
}

/// What every control shows for a state: the handler sets exactly this.
#[test]
fn the_view_sets_every_control_from_the_state() {
    assert_eq!(
        state().view(),
        PanelView {
            checks: vec![
                (ID_VIETNAMESE, true),
                (ID_MODERN_TONE, false),
                (ID_FREE_MARKING, true),
                (ID_SPELL_CHECK, true),
                (ID_AUTO_RESTORE, true),
                (ID_MACROS, true),
                (ID_MACROS_WHEN_ENGLISH, false),
                (ID_MACRO_AUTO_CASE, true),
                (ID_STARTUP, false),
                (ID_SHOW_ON_START, true),
            ],
            methods: vec!["Telex", "VNI", "VIQR"],
            method: Some(0),
            texts: vec![
                (
                    ID_HINT,
                    "Bật/tắt nhanh: Ctrl+Shift hoặc nhấp biểu tượng V/E".to_owned()
                ),
                (
                    ID_CONFIG_FILE,
                    r"Tệp cấu hình: D:\Tools\Raumakey\raumakey.toml".to_owned()
                ),
            ],
            wrapped: vec![
                (
                    ID_FREE_MARKING,
                    "Cho gõ dấu sau phụ âm cuối (toans\u{a0}→\u{a0}toán)".to_owned()
                ),
                (
                    ID_AUTO_RESTORE,
                    "Giữ nguyên phím đã gõ khi từ không phải tiếng Việt".to_owned()
                ),
                (
                    ID_MACRO_AUTO_CASE,
                    "Viết hoa theo cách gõ (Ko\u{a0}→\u{a0}Không,\u{a0}KO\u{a0}→\u{a0}KHÔNG)"
                        .to_owned()
                ),
                (ID_CONFIG_LOCATION, "(cạnh raumakey.exe)".to_owned()),
            ],
            enabled: vec![(ID_OPEN_CONFIG, true), (ID_MACRO_LIST, true)],
        }
    );
    let ids: Vec<u16> = state().view().checks.iter().map(|(id, _)| *id).collect();
    assert_eq!(ids, CHECK_BOXES);
}

/// Every control that shows more than one line (the check boxes with `BS_MULTILINE`, the
/// location note) gets its text from the view's `wrapped`, which the glue wraps at spaces
/// itself, since Windows would cut a Vietnamese word: the check boxes with their labels of
/// the template, and no control that shows one line.
#[test]
fn the_controls_of_several_lines_get_texts_that_the_glue_wraps() {
    let template = panel_template();
    let view = state().view();
    let wrapped: Vec<u16> = view.wrapped.iter().map(|(id, _)| *id).collect();
    for control in template.controls() {
        let several_lines = control.style() & styles::BS_MULTILINE != 0
            || (control.class() == ControlClass::Static && control.rect().cy >= 16);
        assert_eq!(
            wrapped.contains(&control.id()),
            several_lines,
            "{}",
            control.id()
        );
        let text = view.wrapped.iter().find(|(id, _)| *id == control.id());
        if let (ControlClass::Button, Some((_, text))) = (control.class(), text) {
            assert_eq!(control.text(), text);
        }
    }
    assert!(view.texts.iter().all(|(id, _)| !wrapped.contains(id)));
}

/// Each check box follows its own value, and no other.
#[test]
fn each_check_box_follows_its_own_value() {
    type Set = fn(&mut PanelState);
    let cases: [(u16, Set); 10] = [
        (ID_VIETNAMESE, |state| state.vietnamese = !state.vietnamese),
        (ID_MODERN_TONE, |state| {
            state.modern_tone = !state.modern_tone
        }),
        (ID_FREE_MARKING, |state| {
            state.free_marking = !state.free_marking;
        }),
        (ID_SPELL_CHECK, |state| {
            state.spell_check = !state.spell_check
        }),
        (ID_AUTO_RESTORE, |state| {
            state.auto_restore = !state.auto_restore
        }),
        (ID_MACROS, |state| state.macros = !state.macros),
        (ID_MACROS_WHEN_ENGLISH, |state| {
            state.macros_when_english = !state.macros_when_english;
        }),
        (ID_MACRO_AUTO_CASE, |state| {
            state.macro_auto_case = !state.macro_auto_case;
        }),
        (ID_STARTUP, |state| state.startup = StartupMode::Shortcut),
        (ID_SHOW_ON_START, |state| {
            state.show_on_start = !state.show_on_start;
        }),
    ];
    let before = state().view().checks;
    for (id, set) in cases {
        let mut changed = state();
        set(&mut changed);
        let after = changed.view().checks;
        for ((left_id, left), (right_id, right)) in before.iter().zip(&after) {
            assert_eq!(left_id, right_id);
            assert_eq!(left != right, *left_id == id, "{id}: {left_id}");
        }
    }
    // The elevated logon task starts Raumakey too: checked.
    let mut elevated = state();
    elevated.startup = StartupMode::ElevatedTask;
    assert!(elevated.view().checks.contains(&(ID_STARTUP, true)));
}

/// The drop-down list holds Telex, VNI, VIQR, and "Kiểu gõ riêng" only when it is listed;
/// the method in effect is selected.
#[test]
fn the_method_list_and_its_selection_follow_the_state() {
    for (method, index) in [
        (InputMethod::Telex, 0),
        (InputMethod::Vni, 1),
        (InputMethod::Viqr, 2),
    ] {
        let mut state = state();
        state.method = method;
        let view = state.view();
        assert_eq!(view.methods, ["Telex", "VNI", "VIQR"]);
        assert_eq!(view.method, Some(index), "{method:?}");
        state.user_defined_listed = true;
        let view = state.view();
        assert_eq!(view.methods, ["Telex", "VNI", "VIQR", "Kiểu gõ riêng"]);
        assert_eq!(view.method, Some(index), "{method:?}");
    }
    let mut user = state();
    user.method = InputMethod::UserDefined;
    user.user_defined_listed = true;
    assert_eq!(user.view().method, Some(3));
    // Never the case for a state built from a snapshot: nothing is selected then.
    user.user_defined_listed = false;
    assert_eq!(user.view().method, None);
}

/// Without a folder, "Mở file cấu hình" and "Danh sách gõ tắt…" are disabled.
#[test]
fn without_a_folder_the_file_buttons_are_disabled() {
    let nowhere = PanelState::new(
        &runtime(),
        StartupMode::None,
        true,
        Location::Unavailable,
        None,
    );
    let view = nowhere.view();
    assert_eq!(
        view.enabled,
        [(ID_OPEN_CONFIG, false), (ID_MACRO_LIST, false)]
    );
    assert!(view.texts.contains(&(
        ID_CONFIG_FILE,
        "Tệp cấu hình: không có, nên các thay đổi không được lưu.".to_owned()
    )));
    assert!(view.wrapped.contains(&(ID_CONFIG_LOCATION, String::new())));
}

// --- Requests ---------------------------------------------------------------------------------

/// What the handler reads from a control after the user changed it: a check box's state, the
/// drop-down list's selection, nothing for buttons and labels.
#[test]
fn check_boxes_and_the_method_list_hold_values() {
    for id in CHECK_BOXES {
        assert_eq!(value_kind(id), Some(ValueKind::Check), "{id}");
    }
    assert_eq!(value_kind(ID_METHOD), Some(ValueKind::Choice));
    for id in [
        ID_HINT,
        ID_MACRO_LIST,
        ID_CONFIG_FILE,
        ID_CONFIG_LOCATION,
        ID_OPEN_CONFIG,
        ID_DEFAULTS,
        ID_CLOSE,
        IDOK,
        STATIC_ID,
        0,
        999,
    ] {
        assert_eq!(value_kind(id), None, "{id}");
    }
}

/// The meaningful actions: a click on a check box asks for its new value (absolute, never a
/// toggle), a selection in the list for its method, and the buttons for their action.
fn expected_request(id: u16, code: u16, value: Option<ControlValue>) -> Option<PanelRequest> {
    use PanelRequest::Change as C;
    match (id, code, value) {
        (ID_OPEN_CONFIG, BN_CLICKED, _) => Some(PanelRequest::OpenConfig),
        (ID_MACRO_LIST, BN_CLICKED, _) => Some(PanelRequest::MacroList),
        (ID_DEFAULTS, BN_CLICKED, _) => Some(C(Change::TypingDefaults)),
        (ID_METHOD, CBN_SELCHANGE, Some(ControlValue::Selected(Some(index)))) => [
            InputMethod::Telex,
            InputMethod::Vni,
            InputMethod::Viqr,
            InputMethod::UserDefined,
        ]
        .get(index)
        .map(|method| C(Change::InputMethod(*method))),
        (_, BN_CLICKED, Some(ControlValue::Checked(on))) => match id {
            ID_VIETNAMESE => Some(C(Change::VietnameseMode(on))),
            ID_MODERN_TONE => Some(C(Change::ModernTone(on))),
            ID_FREE_MARKING => Some(C(Change::FreeMarking(on))),
            ID_SPELL_CHECK => Some(C(Change::SpellCheck(on))),
            ID_AUTO_RESTORE => Some(C(Change::AutoRestore(on))),
            ID_MACROS => Some(C(Change::Macros(on))),
            ID_MACROS_WHEN_ENGLISH => Some(C(Change::MacrosWhenEnglish(on))),
            ID_MACRO_AUTO_CASE => Some(C(Change::MacroAutoCase(on))),
            ID_STARTUP => Some(PanelRequest::Startup(on)),
            ID_SHOW_ON_START => Some(C(Change::ShowPanelOnStart(on))),
            _ => None,
        },
        _ => None,
    }
}

/// Every control, notification, and value: the actions above make their request, everything
/// else (focus notifications, other codes, the Close button, which closes the panel, labels,
/// unknown ids) makes none.
#[test]
fn every_action_on_every_control_makes_its_request_or_none() {
    let ids = [
        ID_VIETNAMESE,
        ID_HINT,
        ID_METHOD,
        ID_MODERN_TONE,
        ID_FREE_MARKING,
        ID_SPELL_CHECK,
        ID_AUTO_RESTORE,
        ID_MACROS,
        ID_MACROS_WHEN_ENGLISH,
        ID_MACRO_AUTO_CASE,
        ID_MACRO_LIST,
        ID_STARTUP,
        ID_SHOW_ON_START,
        ID_CONFIG_FILE,
        ID_CONFIG_LOCATION,
        ID_OPEN_CONFIG,
        ID_DEFAULTS,
        ID_CLOSE,
        IDOK,
        STATIC_ID,
        0,
        999,
    ];
    // Every notification code of buttons (BN_CLICKED 0 to BN_KILLFOCUS 7) and drop-down lists
    // (CBN_SELCHANGE 1 to CBN_CLOSEUP 8), and one of edits.
    let codes = [BN_CLICKED, CBN_SELCHANGE, 2, 3, 4, 5, 6, 7, 8, EN_CHANGE];
    let mut values = vec![
        None,
        Some(ControlValue::Checked(false)),
        Some(ControlValue::Checked(true)),
        Some(ControlValue::Selected(None)),
    ];
    values.extend((0..6).map(|index| Some(ControlValue::Selected(Some(index)))));
    let mut made = 0;
    for id in ids {
        for code in codes {
            for value in &values {
                let expected = expected_request(id, code, *value);
                made += usize::from(expected.is_some());
                assert_eq!(request(id, code, *value), expected, "{id} {code} {value:?}");
            }
        }
    }
    // 10 check boxes × 2 values, 4 methods, 3 buttons × every value.
    assert_eq!(made, 20 + 4 + 3 * values.len());
}

/// Clicks as the handler reads them: the new state of the box, the new selection.
#[test]
fn clicks_ask_for_the_new_value() {
    assert_eq!(
        request(
            ID_SPELL_CHECK,
            BN_CLICKED,
            Some(ControlValue::Checked(false))
        ),
        Some(PanelRequest::Change(Change::SpellCheck(false)))
    );
    assert_eq!(
        request(ID_STARTUP, BN_CLICKED, Some(ControlValue::Checked(true))),
        Some(PanelRequest::Startup(true))
    );
    assert_eq!(
        request(
            ID_METHOD,
            CBN_SELCHANGE,
            Some(ControlValue::Selected(Some(1)))
        ),
        Some(PanelRequest::Change(Change::InputMethod(InputMethod::Vni)))
    );
    assert_eq!(
        request(
            ID_METHOD,
            CBN_SELCHANGE,
            Some(ControlValue::Selected(Some(3)))
        ),
        Some(PanelRequest::Change(Change::InputMethod(
            InputMethod::UserDefined
        )))
    );
}

/// "Mặc định" asks the user first; nothing else does.
#[test]
fn only_the_defaults_ask_first() {
    assert_eq!(
        DEFAULTS_QUESTION,
        "Đặt lại các mục của nhóm \"Cách gõ\" và \"Gõ tắt\" về mặc định? Chế độ gõ, việc khởi \
         động cùng Windows, việc mở bảng khi Raumakey chạy và danh sách gõ tắt không đổi."
    );
    assert_eq!(
        confirmation(PanelRequest::Change(Change::TypingDefaults)),
        Some(DEFAULTS_QUESTION)
    );
    for other in [
        PanelRequest::Change(Change::SpellCheck(false)),
        PanelRequest::Change(Change::VietnameseMode(true)),
        PanelRequest::Startup(true),
        PanelRequest::OpenConfig,
        PanelRequest::MacroList,
    ] {
        assert_eq!(confirmation(other), None, "{other:?}");
    }
}

/// The private message that asks the open panel to show the shared state again.
#[test]
fn the_refresh_message_is_private_and_not_reserved_by_the_dialog() {
    assert!(is_postable(PANEL_REFRESH.id()));
    assert!(!is_reserved(PANEL_REFRESH.id()));
}

// --- The dialog template ----------------------------------------------------------------------

/// The controls in tab order: (id, class, text, extra style bits that must be set).
fn expected_controls() -> Vec<(u16, ControlClass, &'static str, u32)> {
    use ControlClass::{Button, ComboBox, Static};
    use styles::WS_GROUP;
    let check = styles::BS_AUTOCHECKBOX | styles::WS_TABSTOP;
    let two_lines = check | styles::BS_MULTILINE;
    let push = styles::BS_PUSHBUTTON | styles::WS_TABSTOP;
    vec![
        (ID_VIETNAMESE, Button, "Gõ tiếng Việt", check | WS_GROUP),
        (ID_HINT, Static, "", styles::SS_NOPREFIX | WS_GROUP),
        (STATIC_ID, Button, "Cách gõ", styles::BS_GROUPBOX),
        (STATIC_ID, Static, "Kiểu gõ:", 0),
        (
            ID_METHOD,
            ComboBox,
            "",
            styles::CBS_DROPDOWNLIST | styles::WS_TABSTOP | WS_GROUP,
        ),
        (
            ID_MODERN_TONE,
            Button,
            "Đặt dấu kiểu mới (oà,\u{a0}uý)",
            check | WS_GROUP,
        ),
        (
            ID_FREE_MARKING,
            Button,
            "Cho gõ dấu sau phụ âm cuối (toans\u{a0}→\u{a0}toán)",
            two_lines,
        ),
        (ID_SPELL_CHECK, Button, "Kiểm tra chính tả", check),
        (
            ID_AUTO_RESTORE,
            Button,
            "Giữ nguyên phím đã gõ khi từ không phải tiếng Việt",
            two_lines,
        ),
        (STATIC_ID, Button, "Gõ tắt", styles::BS_GROUPBOX | WS_GROUP),
        (ID_MACROS, Button, "Bật gõ tắt", check | WS_GROUP),
        (
            ID_MACROS_WHEN_ENGLISH,
            Button,
            "Gõ tắt cả khi đang gõ tiếng Anh",
            check,
        ),
        (
            ID_MACRO_AUTO_CASE,
            Button,
            "Viết hoa theo cách gõ (Ko\u{a0}→\u{a0}Không,\u{a0}KO\u{a0}→\u{a0}KHÔNG)",
            two_lines,
        ),
        (ID_MACRO_LIST, Button, "Danh sách gõ tắt…", push | WS_GROUP),
        (
            STATIC_ID,
            Button,
            "Hệ thống",
            styles::BS_GROUPBOX | WS_GROUP,
        ),
        (
            ID_STARTUP,
            Button,
            "Khởi động cùng Windows",
            check | WS_GROUP,
        ),
        (
            ID_SHOW_ON_START,
            Button,
            "Mở bảng này khi Raumakey chạy",
            check,
        ),
        (
            STATIC_ID,
            Static,
            "Mọi thay đổi có hiệu lực và được lưu ngay.",
            WS_GROUP,
        ),
        (
            ID_CONFIG_FILE,
            Static,
            "",
            styles::SS_NOPREFIX | styles::SS_PATHELLIPSIS,
        ),
        (ID_CONFIG_LOCATION, Static, "", styles::SS_NOPREFIX),
        (ID_OPEN_CONFIG, Button, "Mở file cấu hình", push | WS_GROUP),
        (ID_DEFAULTS, Button, "Mặc định", push),
        (
            IDCANCEL,
            Button,
            "Đóng",
            styles::BS_DEFPUSHBUTTON | styles::WS_TABSTOP,
        ),
    ]
}

/// D43: the title, the font, a minimize box, and the controls of the design's sketch in tab
/// order, with their exact Vietnamese labels. The hint and the footer are filled from the
/// state by the handler.
#[test]
fn the_template_has_the_title_and_the_controls_of_the_design() {
    let template = panel_template();
    assert_eq!(
        template.title(),
        format!("Raumakey {PRODUCT_VERSION} \u{2013} Bảng cấu hình")
    );
    assert_eq!(template.ex_style(), 0);
    let style = template.style();
    assert_ne!(style & styles::DS_CENTER, 0, "centered on its monitor");
    assert_ne!(style & styles::WS_MINIMIZEBOX, 0);
    assert_ne!(style & styles::WS_SYSMENU, 0, "the close box");
    let controls = template.controls();
    let actual: Vec<(u16, ControlClass, &str)> = controls
        .iter()
        .map(|control| (control.id(), control.class(), control.text()))
        .collect();
    let expected: Vec<(u16, ControlClass, &str)> = expected_controls()
        .into_iter()
        .map(|(id, class, text, _)| (id, class, text))
        .collect();
    assert_eq!(actual, expected);
    for (control, (id, _, _, bits)) in controls.iter().zip(expected_controls()) {
        assert_eq!(control.style() & bits, bits, "{id}");
        assert_eq!(
            control.style() & styles::WS_GROUP,
            bits & styles::WS_GROUP,
            "{id}: the first of a group or not"
        );
        assert_eq!(control.ex_style(), 0, "{id}");
    }
    // Only the multi-line boxes wrap.
    for control in controls {
        let multiline = control.style() & styles::BS_MULTILINE != 0;
        assert_eq!(
            multiline,
            [ID_FREE_MARKING, ID_AUTO_RESTORE, ID_MACRO_AUTO_CASE].contains(&control.id()),
            "{}",
            control.id()
        );
    }
    template.build().unwrap();
}

/// What a control of the panel is, for the arrow keys.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Check,
    MethodList,
    PushButton,
    /// A label or a group box, which never takes the focus.
    Text,
}

fn kind_of(control: &Control) -> Kind {
    let style = control.style() & 0x0F;
    match control.class() {
        ControlClass::ComboBox => Kind::MethodList,
        ControlClass::Button if style == styles::BS_AUTOCHECKBOX => Kind::Check,
        ControlClass::Button
            if style == styles::BS_PUSHBUTTON || style == styles::BS_DEFPUSHBUTTON =>
        {
            Kind::PushButton
        }
        _ => Kind::Text,
    }
}

/// The groups of the dialog in order (`WS_GROUP` starts a group).
fn groups(template: &DialogTemplate) -> Vec<Vec<&Control>> {
    let mut groups: Vec<Vec<&Control>> = Vec::new();
    for control in template.controls() {
        if control.style() & styles::WS_GROUP != 0 || groups.is_empty() {
            groups.push(Vec::new());
        }
        groups.last_mut().unwrap().push(control);
    }
    groups
}

/// The arrow keys move the focus to the next or previous control of the same group
/// (`GetNextDlgGroupItem`, which also returns labels and group boxes): so each group holds
/// check boxes only, the method list alone, push buttons only, or texts only. An arrow key on
/// a check box never reaches the method list (where the next arrow would change and save the
/// method), a button, or a label, which would take the focus out of sight.
#[test]
fn arrow_keys_stay_within_each_group_of_check_boxes() {
    let template = panel_template();
    let focusable: Vec<Vec<u16>> = groups(&template)
        .iter()
        .map(|group| {
            group
                .iter()
                .filter(|control| kind_of(control) != Kind::Text)
                .map(|control| control.id())
                .collect::<Vec<_>>()
        })
        .filter(|group| !group.is_empty())
        .collect();
    assert_eq!(
        focusable,
        [
            vec![ID_VIETNAMESE],
            vec![ID_METHOD],
            vec![
                ID_MODERN_TONE,
                ID_FREE_MARKING,
                ID_SPELL_CHECK,
                ID_AUTO_RESTORE
            ],
            vec![ID_MACROS, ID_MACROS_WHEN_ENGLISH, ID_MACRO_AUTO_CASE],
            vec![ID_MACRO_LIST],
            vec![ID_STARTUP, ID_SHOW_ON_START],
            vec![ID_OPEN_CONFIG, ID_DEFAULTS, ID_CLOSE],
        ]
    );
    for group in groups(&template) {
        let kinds: Vec<Kind> = group.iter().map(|control| kind_of(control)).collect();
        assert!(kinds.iter().all(|kind| *kind == kinds[0]), "{kinds:?}");
        if kinds[0] == Kind::MethodList {
            assert_eq!(group.len(), 1, "the method list alone");
        }
    }
}

/// "Đóng" is the Cancel button and the only default button: Enter, Esc, and the close box all
/// close the panel (there is no OK: every change is saved at once).
#[test]
fn close_is_the_default_cancel_button() {
    let template = panel_template();
    let defaults: Vec<u16> = template
        .controls()
        .iter()
        .filter(|control| {
            control.class() == ControlClass::Button
                && control.style() & 0x0F == styles::BS_DEFPUSHBUTTON
        })
        .map(Control::id)
        .collect();
    assert_eq!(defaults, [IDCANCEL]);
    assert_eq!(ID_CLOSE, IDCANCEL);
    assert!(
        !template
            .controls()
            .iter()
            .any(|control| control.id() == IDOK)
    );
}

/// The test variant: the same dialog at an explicit position far off the screen, never
/// activated (no taskbar button either).
#[test]
fn the_off_screen_variant_is_the_same_dialog_never_activated() {
    let normal = panel_template();
    let off = off_screen_panel_template();
    assert_eq!(off.title(), normal.title());
    assert_eq!(off.controls(), normal.controls());
    assert_eq!(off.ex_style(), styles::WS_EX_NOACTIVATE);
    assert_eq!(off.style() & styles::DS_CENTER, 0);
    assert_eq!(off.style(), normal.style() & !styles::DS_CENTER);
    const { assert!(OFF_SCREEN <= -10_000) };
    off.build().unwrap();
}

/// The rectangle a control takes on the dialog: a drop-down list without its list.
fn shown_rect(control: &Control) -> DialogRect {
    let mut rect = control.rect();
    if control.class() == ControlClass::ComboBox {
        rect.cy -= DROPPED_LIST_HEIGHT;
    }
    rect
}

fn inside(inner: DialogRect, outer: DialogRect) -> bool {
    inner.x >= outer.x
        && inner.y >= outer.y
        && inner.x + inner.cx <= outer.x + outer.cx
        && inner.y + inner.cy <= outer.y + outer.cy
}

fn overlap(a: DialogRect, b: DialogRect) -> bool {
    a.x < b.x + b.cx && b.x < a.x + a.cx && a.y < b.y + b.cy && b.y < a.y + a.cy
}

/// The size of the template, read from its header bytes (offsets 22 and 24 of
/// `DLGTEMPLATEEX`): width and height in dialog units.
fn dialog_size(template: &DialogTemplate) -> (i16, i16) {
    let bytes = template.build().unwrap();
    let bytes = bytes.as_bytes();
    let word = |at: usize| i16::from_le_bytes([bytes[at], bytes[at + 1]]);
    (word(22), word(24))
}

/// The layout: every control lies inside the dialog with a margin, no two controls overlap
/// (except the group boxes around theirs), and each control of a group lies inside its box.
#[test]
fn the_layout_keeps_every_control_inside_its_group_without_overlap() {
    let template = panel_template();
    let (width, height) = dialog_size(&template);
    let client = DialogRect::new(7, 7, width - 14, height - 14);
    let controls = template.controls();
    for control in controls {
        assert!(
            inside(shown_rect(control), client),
            "{:?} in {client:?}",
            control
        );
    }
    let boxes: Vec<&Control> = controls
        .iter()
        .filter(|control| control.style() & 0x0F == styles::BS_GROUPBOX)
        .collect();
    assert_eq!(boxes.len(), 3);
    let others: Vec<&Control> = controls
        .iter()
        .filter(|control| control.style() & 0x0F != styles::BS_GROUPBOX)
        .collect();
    for (index, a) in others.iter().enumerate() {
        for b in &others[index + 1..] {
            assert!(
                !overlap(shown_rect(a), shown_rect(b)),
                "{a:?} overlaps {b:?}"
            );
        }
    }
    for (a, b) in [(0, 1), (0, 2), (1, 2)] {
        assert!(!overlap(boxes[a].rect(), boxes[b].rect()), "group boxes");
    }
    let groups: [(&str, &[u16]); 3] = [
        (
            "Cách gõ",
            &[
                ID_METHOD,
                ID_MODERN_TONE,
                ID_FREE_MARKING,
                ID_SPELL_CHECK,
                ID_AUTO_RESTORE,
            ],
        ),
        (
            "Gõ tắt",
            &[
                ID_MACROS,
                ID_MACROS_WHEN_ENGLISH,
                ID_MACRO_AUTO_CASE,
                ID_MACRO_LIST,
            ],
        ),
        ("Hệ thống", &[ID_STARTUP, ID_SHOW_ON_START]),
    ];
    for (caption, ids) in groups {
        let group = boxes
            .iter()
            .find(|control| control.text() == caption)
            .unwrap();
        // The caption takes the top of the box.
        let mut inner = group.rect();
        inner.y += 8;
        inner.cy -= 8;
        for id in ids {
            let control = controls.iter().find(|control| control.id() == *id).unwrap();
            assert!(inside(shown_rect(control), inner), "{id} in {caption}");
        }
    }
}

/// A conservative estimate of a text's width in dialog units: four units per character,
/// which is the average width of the letters by the definition of the unit (lower-case text
/// and spaces are narrower; diacritics take no width).
fn estimated_width(text: &str) -> i16 {
    i16::try_from(text.chars().count() * 4).unwrap()
}

/// Labels fit their controls at the panel's font: check boxes after their box (12 units),
/// on one line or, for the multi-line ones, on two (allowing a fifth of the second line for
/// the word wrap); buttons with a margin; the longest hint (with Ctrl+Shift) on its line.
#[test]
fn the_labels_fit_their_controls() {
    let template = panel_template();
    for control in template.controls() {
        let rect = control.rect();
        let text = control.text();
        let kind = control.style() & 0x0F;
        if control.class() != ControlClass::Button || kind == styles::BS_GROUPBOX {
            continue;
        }
        if kind == styles::BS_AUTOCHECKBOX {
            let room = rect.cx - 12;
            let lines = if control.style() & styles::BS_MULTILINE != 0 {
                assert!(rect.cy >= 16, "{text}: two lines need the height");
                room + room * 4 / 5
            } else {
                assert!(rect.cy >= 10, "{text}");
                room
            };
            assert!(estimated_width(text) <= lines, "{text}: {rect:?}");
        } else {
            assert!(rect.cy >= 14, "{text}");
            assert!(estimated_width(text) + 8 <= rect.cx, "{text}: {rect:?}");
        }
    }
    let label = |id: u16| {
        template
            .controls()
            .iter()
            .find(|control| control.id() == id)
            .unwrap()
            .rect()
    };
    assert!(estimated_width(&state().hint()) <= label(ID_HINT).cx);
    // The note of `%LOCALAPPDATA%`, the longest, wraps onto the location label's two lines:
    // the label cuts nothing off.
    let note = template
        .controls()
        .iter()
        .find(|control| control.id() == ID_CONFIG_LOCATION)
        .unwrap();
    assert_eq!(
        note.style() & (styles::SS_PATHELLIPSIS | styles::SS_ENDELLIPSIS),
        0
    );
    let local = PanelState::new(
        &runtime(),
        StartupMode::None,
        true,
        Location::LocalAppData,
        Some(Path::new(
            r"C:\Users\An\AppData\Local\Raumakey\raumakey.toml",
        )),
    );
    let place = label(ID_CONFIG_LOCATION);
    assert!(place.cy >= 16, "two lines need the height");
    assert!(estimated_width(local.config_location()) <= place.cx + place.cx * 4 / 5);
    assert!(estimated_width(state().config_location()) <= place.cx);
    let method_label = template
        .controls()
        .iter()
        .find(|control| control.text() == "Kiểu gõ:")
        .unwrap();
    assert!(estimated_width(method_label.text()) <= method_label.rect().cx);
    let longest = estimated_width("Kiểu gõ riêng") + 16;
    assert!(longest <= label(ID_METHOD).cx, "the longest method");
}

/// The examples in parentheses ("(oà, uý)", "(toans → toán)", "(Ko → Không, KO → KHÔNG)")
/// stay whole when a label is wrapped: their spaces are no-break spaces, which the wrap at
/// spaces never breaks, so a line never ends with "→".
#[test]
fn the_examples_of_the_labels_stay_on_one_line() {
    for control in panel_template().controls() {
        let text = control.text();
        let mut depth = 0_u32;
        for character in text.chars() {
            match character {
                '(' => depth += 1,
                ')' => depth = depth.saturating_sub(1),
                ' ' if depth > 0 => panic!("a breakable space in the example of {text:?}"),
                _ => {}
            }
        }
    }
}
