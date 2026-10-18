//! The settings panel ("Bảng cấu hình", D43; design §22.1) as pure data: the state it shows
//! ([`PanelState`], built by the tray, `crate::tray::Tray::panel_state`), what every control
//! shows for it ([`PanelView`]), the request that each action of the user makes
//! ([`request`], [`PanelRequest`]), and the dialog itself ([`panel_template`]: controls, ids,
//! Vietnamese labels, and the layout in dialog units). The Windows glue in `crate::desktop`
//! runs the dialog on a thread of its own and is a thin loop over these; everything here is
//! tested on every host.
//!
//! # Data flow
//!
//! The tray is the only source of the panel's data. The panel sends requests that carry the
//! target value ("spell checking = off"), never a toggle, and the tray answers each with the
//! whole state again, which the panel shows as it is: a refused request (the start with
//! Windows from the temporary folder) puts the box back. The tray also sends the state
//! whenever the settings change another way: Ctrl+Shift, a click on the icon, the menu, a
//! hand edit, `raumakey startup`. There is no Save or Cancel: every change takes effect and is
//! saved at once.
//!
//! # Layout
//!
//! One page, two columns, no tabs (dialog units, Segoe UI 9 pt, [`PANEL_WIDTH`] by
//! [`PANEL_HEIGHT`]):
//!
//! ```text
//! [x] Gõ tiếng Việt          Bật/tắt nhanh: Ctrl+Shift hoặc nhấp biểu tượng V/E
//! +- Cách gõ ---------------+  +- Gõ tắt ---------------------------+
//! | Kiểu gõ: [Telex      v] |  | [x] Bật gõ tắt                     |
//! | [ ] Đặt dấu kiểu mới    |  | [ ] Gõ tắt cả khi đang gõ tiếng Anh|
//! | [x] Cho gõ dấu sau ...  |  | [x] Viết hoa theo cách gõ ...      |
//! | [x] Kiểm tra chính tả   |  | [Danh sách gõ tắt…]                |
//! | [x] Giữ nguyên phím ... |  +------------------------------------+
//! +-------------------------+  +- Hệ thống -------------------------+
//!                              | [ ] Khởi động cùng Windows         |
//!                              | [x] Mở bảng này khi Raumakey chạy  |
//!                              +------------------------------------+
//! Tệp cấu hình: D:\Tools\Raumakey\raumakey.toml (cạnh raumakey.exe)
//! Mọi thay đổi có hiệu lực và được lưu ngay.
//! [Mở file cấu hình]  [Mặc định]                               [Đóng]
//! ```
//!
//! The three long check boxes wrap onto two lines (`BS_MULTILINE`). "Đóng" is `IDCANCEL` and
//! the default button, so Enter, Esc, and the close box all close the panel. The hint and the
//! footer are labels that the glue fills from [`PanelState::hint`] and
//! [`PanelState::config_file`]; the footer shortens a long path in its middle
//! (`SS_PATHELLIPSIS`). Without a folder ([`Location::Unavailable`]) "Mở file cấu hình" and
//! "Danh sách gõ tắt…" are disabled.
//!
//! Strings shown to the user are new Raumakey strings in Vietnamese (design §22; not taken
//! from any other input method). Nothing here holds keystrokes, document text, or macro text:
//! the state holds settings, the path of `raumakey.toml`, and the toggle hotkey's name.

use std::path::{Path, PathBuf};

use raumakey_config::StartupMode;
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::identity::PRODUCT_NAME;
use raumakey_model::options::InputMethod;
use raumakey_windows::dialog_box::notifications::{BN_CLICKED, CBN_SELCHANGE};
use raumakey_windows::dialog_template::{
    Control, DialogRect, DialogTemplate, IDCANCEL, STATIC_ID, styles,
};
use raumakey_windows::window::AppMessage;

use crate::install::PRODUCT_VERSION;
use crate::notice::location_text;
use crate::settings::{Change, Location};

/// "Gõ tiếng Việt": Vietnamese mode (check box).
pub const ID_VIETNAMESE: u16 = 100;
/// The hint about the toggle hotkey and the icon (label).
pub const ID_HINT: u16 = 101;
/// "Kiểu gõ:": the input method (drop-down list).
pub const ID_METHOD: u16 = 110;
/// "Đặt dấu kiểu mới (oà, uý)" (check box).
pub const ID_MODERN_TONE: u16 = 111;
/// "Cho gõ dấu sau phụ âm cuối (toans → toán)" (check box).
pub const ID_FREE_MARKING: u16 = 112;
/// "Kiểm tra chính tả" (check box).
pub const ID_SPELL_CHECK: u16 = 113;
/// "Giữ nguyên phím đã gõ khi từ không phải tiếng Việt" (check box).
pub const ID_AUTO_RESTORE: u16 = 114;
/// "Bật gõ tắt" (check box).
pub const ID_MACROS: u16 = 120;
/// "Gõ tắt cả khi đang gõ tiếng Anh" (check box).
pub const ID_MACROS_WHEN_ENGLISH: u16 = 121;
/// "Viết hoa theo cách gõ (Ko → Không, KO → KHÔNG)" (check box).
pub const ID_MACRO_AUTO_CASE: u16 = 122;
/// "Danh sách gõ tắt…" (button).
pub const ID_MACRO_LIST: u16 = 123;
/// "Khởi động cùng Windows" (check box).
pub const ID_STARTUP: u16 = 130;
/// "Mở bảng này khi Raumakey chạy" (check box).
pub const ID_SHOW_ON_START: u16 = 131;
/// The path of `raumakey.toml` and where it lives (label).
pub const ID_CONFIG_FILE: u16 = 140;
/// "Mở file cấu hình" (button).
pub const ID_OPEN_CONFIG: u16 = 141;
/// "Mặc định" (button).
pub const ID_DEFAULTS: u16 = 142;
/// "Đóng": the Cancel button, which the dialog box answers by closing.
pub const ID_CLOSE: u16 = IDCANCEL;

/// The check boxes, in the order of [`PanelView::checks`].
pub const CHECK_BOXES: [u16; 10] = [
    ID_VIETNAMESE,
    ID_MODERN_TONE,
    ID_FREE_MARKING,
    ID_SPELL_CHECK,
    ID_AUTO_RESTORE,
    ID_MACROS,
    ID_MACROS_WHEN_ENGLISH,
    ID_MACRO_AUTO_CASE,
    ID_STARTUP,
    ID_SHOW_ON_START,
];

/// The items of the method list in order, with their method; the last one ("own input
/// method") only when it is listed ([`PanelState::user_defined_listed`]).
const METHODS: [(InputMethod, &str); 4] = [
    (InputMethod::Telex, "Telex"),
    (InputMethod::Vni, "VNI"),
    (InputMethod::Viqr, "VIQR"),
    (InputMethod::UserDefined, "Kiểu gõ riêng"),
];

/// The question of "Mặc định" ("Reset the items of the groups … to their defaults? The
/// typing mode, the start with Windows, and the macro list do not change."), in a yes/no box
/// owned by the panel.
pub const DEFAULTS_QUESTION: &str = "Đặt lại các mục của nhóm \"Cách gõ\" và \"Gõ tắt\" về mặc \
định? Chế độ gõ, việc khởi động cùng Windows và danh sách gõ tắt không đổi.";

/// The footer without a folder: "Configuration file: none, so changes are not saved."
const CONFIG_FILE_UNAVAILABLE: &str = "Tệp cấu hình: không có, nên các thay đổi không được lưu.";

/// The private message that asks the open panel to show the shared state again (`WM_APP + 1`
/// of the panel's own window; below the range the dialog procedure keeps for itself).
pub const PANEL_REFRESH: AppMessage = match AppMessage::new(1) {
    Some(message) => message,
    None => panic!("offset outside the WM_APP range"),
};

/// Width of the panel in dialog units.
pub const PANEL_WIDTH: i16 = 330;
/// Height of the panel in dialog units.
pub const PANEL_HEIGHT: i16 = 205;
/// Where [`off_screen_panel_template`] places the panel (both coordinates), far off every
/// monitor.
pub const OFF_SCREEN: i16 = -30_000;

/// What the panel shows: the settings in effect, as the tray holds them.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PanelState {
    /// Vietnamese mode (`[input] enabled`).
    pub vietnamese: bool,
    /// The input method in effect (of the runtime snapshot: a user method without a usable
    /// key map is in effect as the default method).
    pub method: InputMethod,
    /// "Kiểu gõ riêng" is listed: a user method can take effect, because it is in effect or
    /// the snapshot carries the key map that `[input] user_keymap` names.
    pub user_defined_listed: bool,
    /// `[input] modern_tone`.
    pub modern_tone: bool,
    /// `[input] free_marking`.
    pub free_marking: bool,
    /// `[input] spell_check`.
    pub spell_check: bool,
    /// `[input] auto_restore_invalid`.
    pub auto_restore: bool,
    /// `[macros] enabled`.
    pub macros: bool,
    /// `[macros] when_disabled`.
    pub macros_when_english: bool,
    /// `[macros] auto_case`.
    pub macro_auto_case: bool,
    /// `[system] startup`; the check box is checked unless it is `none`.
    pub startup: StartupMode,
    /// `[system] show_panel_on_start`.
    pub show_on_start: bool,
    /// The first toggle hotkey of `[hotkeys] toggle_mode`, as it is written ("Ctrl+Shift");
    /// `None` without one.
    pub toggle_hotkey: Option<String>,
    /// Where the configuration lives.
    pub location: Location,
    /// The path of `raumakey.toml`; `None` for [`Location::Unavailable`].
    pub config_path: Option<PathBuf>,
}

impl PanelState {
    /// The state of the settings in effect: the runtime snapshot (the mode, the method in
    /// effect, the options, the key maps, the hotkeys), `startup`, `show_on_start`, and where
    /// `raumakey.toml` lives.
    pub fn new(
        runtime: &RuntimeSnapshot,
        startup: StartupMode,
        show_on_start: bool,
        location: Location,
        config_path: Option<&Path>,
    ) -> Self {
        let options = &runtime.options;
        let usable_keymap = options
            .user_keymap_id
            .as_deref()
            .is_some_and(|id| runtime.keymaps.iter().any(|map| map.id == id));
        Self {
            vietnamese: runtime.vietnamese_mode,
            method: options.input_method,
            user_defined_listed: options.input_method == InputMethod::UserDefined || usable_keymap,
            modern_tone: options.modern_tone,
            free_marking: options.free_marking,
            spell_check: options.spell_check,
            auto_restore: options.auto_restore_invalid,
            macros: options.macros_enabled,
            macros_when_english: options.macros_when_disabled,
            macro_auto_case: options.auto_case_macro,
            startup,
            show_on_start,
            toggle_hotkey: runtime.hotkeys.toggle_mode.first().map(ToString::to_string),
            location,
            config_path: config_path.map(Path::to_path_buf),
        }
    }

    /// Whether changes can be saved: there is a folder for `raumakey.toml`.
    pub fn can_save(&self) -> bool {
        self.location != Location::Unavailable && self.config_path.is_some()
    }

    /// The hint beside "Gõ tiếng Việt" ("Quick switch: Ctrl+Shift or click the V/E icon"),
    /// read only: the hotkeys are edited in the file.
    pub fn hint(&self) -> String {
        match &self.toggle_hotkey {
            Some(hotkey) => format!("Bật/tắt nhanh: {hotkey} hoặc nhấp biểu tượng V/E"),
            None => "Bật/tắt nhanh: nhấp biểu tượng V/E".to_owned(),
        }
    }

    /// The footer: the path of `raumakey.toml` and where it lives, as "Giới thiệu" names it;
    /// without a folder, that changes are not saved.
    pub fn config_file(&self) -> String {
        match &self.config_path {
            Some(path) if self.can_save() => format!(
                "Tệp cấu hình: {} {}",
                path.display(),
                location_text(self.location)
            ),
            _ => CONFIG_FILE_UNAVAILABLE.to_owned(),
        }
    }

    /// What every control shows for this state.
    pub fn view(&self) -> PanelView {
        let listed = if self.user_defined_listed { 4 } else { 3 };
        let methods = METHODS.get(..listed).unwrap_or(&METHODS);
        let can_save = self.can_save();
        PanelView {
            checks: CHECK_BOXES
                .iter()
                .map(|&id| (id, self.checked(id)))
                .collect(),
            methods: methods.iter().map(|(_, name)| *name).collect(),
            method: methods
                .iter()
                .position(|(method, _)| *method == self.method),
            texts: vec![(ID_HINT, self.hint()), (ID_CONFIG_FILE, self.config_file())],
            enabled: vec![(ID_OPEN_CONFIG, can_save), (ID_MACRO_LIST, can_save)],
        }
    }

    /// Whether check box `id` is checked.
    fn checked(&self, id: u16) -> bool {
        match id {
            ID_VIETNAMESE => self.vietnamese,
            ID_MODERN_TONE => self.modern_tone,
            ID_FREE_MARKING => self.free_marking,
            ID_SPELL_CHECK => self.spell_check,
            ID_AUTO_RESTORE => self.auto_restore,
            ID_MACROS => self.macros,
            ID_MACROS_WHEN_ENGLISH => self.macros_when_english,
            ID_MACRO_AUTO_CASE => self.macro_auto_case,
            ID_STARTUP => self.startup != StartupMode::None,
            ID_SHOW_ON_START => self.show_on_start,
            _ => false,
        }
    }
}

/// What every control of the panel shows for a [`PanelState`] ([`PanelState::view`]): the
/// glue sets exactly this, control by control.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PanelView {
    /// Each check box ([`CHECK_BOXES`], in that order) and whether it is checked.
    pub checks: Vec<(u16, bool)>,
    /// The items of the method list ([`ID_METHOD`]), in order.
    pub methods: Vec<&'static str>,
    /// The selected item of the method list.
    pub method: Option<usize>,
    /// Each label filled from the state ([`ID_HINT`], [`ID_CONFIG_FILE`]) and its text.
    pub texts: Vec<(u16, String)>,
    /// Each button that needs a folder and whether it is enabled.
    pub enabled: Vec<(u16, bool)>,
}

/// What the panel asks of the tray. Every value is the target the user chose.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum PanelRequest {
    /// Change a setting, as the menu does (the mode, the method, a switch, the panel at
    /// start, or "Mặc định" as [`Change::TypingDefaults`], asked with
    /// [`DEFAULTS_QUESTION`] first).
    Change(Change),
    /// Start with Windows (`true`: the Startup-folder shortcut) or not.
    Startup(bool),
    /// "Mở file cấu hình": open `raumakey.toml` in Notepad.
    OpenConfig,
    /// "Danh sách gõ tắt…": open the macro list (D44).
    MacroList,
}

/// The kind of value a control holds, which the glue reads after the user changed it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ValueKind {
    /// A check box: whether it is checked (it toggles itself when clicked).
    Check,
    /// The method list: its selected item.
    Choice,
}

/// The value of a control after the user changed it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum ControlValue {
    /// A check box is checked (or not).
    Checked(bool),
    /// The selected item of the method list, if any.
    Selected(Option<usize>),
}

/// The kind of value control `id` holds; `None` for buttons, labels, and unknown ids.
pub fn value_kind(id: u16) -> Option<ValueKind> {
    if CHECK_BOXES.contains(&id) {
        Some(ValueKind::Check)
    } else if id == ID_METHOD {
        Some(ValueKind::Choice)
    } else {
        None
    }
}

/// The request of the user's action on control `id`: notification `code` (`WM_COMMAND`) with
/// the control's new `value` (read as [`value_kind`] says; `None` for the others). A click
/// on a check box asks for its new state, a new selection in the method list for its method,
/// and the buttons for their action. Every other notification, "Đóng" (the dialog box closes
/// the panel itself), labels, and unknown ids ask nothing.
pub fn request(id: u16, code: u16, value: Option<ControlValue>) -> Option<PanelRequest> {
    use PanelRequest::Change as Set;
    match (id, code) {
        (ID_OPEN_CONFIG, BN_CLICKED) => Some(PanelRequest::OpenConfig),
        (ID_MACRO_LIST, BN_CLICKED) => Some(PanelRequest::MacroList),
        (ID_DEFAULTS, BN_CLICKED) => Some(Set(Change::TypingDefaults)),
        (ID_METHOD, CBN_SELCHANGE) => match value {
            Some(ControlValue::Selected(Some(index))) => METHODS
                .get(index)
                .map(|(method, _)| Set(Change::InputMethod(*method))),
            _ => None,
        },
        (_, BN_CLICKED) => {
            let Some(ControlValue::Checked(on)) = value else {
                return None;
            };
            let change = match id {
                ID_VIETNAMESE => Change::VietnameseMode(on),
                ID_MODERN_TONE => Change::ModernTone(on),
                ID_FREE_MARKING => Change::FreeMarking(on),
                ID_SPELL_CHECK => Change::SpellCheck(on),
                ID_AUTO_RESTORE => Change::AutoRestore(on),
                ID_MACROS => Change::Macros(on),
                ID_MACROS_WHEN_ENGLISH => Change::MacrosWhenEnglish(on),
                ID_MACRO_AUTO_CASE => Change::MacroAutoCase(on),
                ID_SHOW_ON_START => Change::ShowPanelOnStart(on),
                ID_STARTUP => return Some(PanelRequest::Startup(on)),
                _ => return None,
            };
            Some(Set(change))
        }
        _ => None,
    }
}

/// The yes/no question the panel asks before it sends `request`: [`DEFAULTS_QUESTION`] for
/// "Mặc định", nothing for the others.
pub fn confirmation(request: PanelRequest) -> Option<&'static str> {
    (request == PanelRequest::Change(Change::TypingDefaults)).then_some(DEFAULTS_QUESTION)
}

/// The title of the panel: `Raumakey <version> – Bảng cấu hình` (with an en dash).
pub fn panel_title() -> String {
    format!("{PRODUCT_NAME} {PRODUCT_VERSION} \u{2013} Bảng cấu hình")
}

const fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
    DialogRect::new(x, y, cx, cy)
}

/// A check box whose label wraps onto two lines.
fn two_line_check_box(id: u16, text: &str, rect: DialogRect) -> Control {
    Control::check_box(id, text, rect).with_style(styles::BS_MULTILINE)
}

/// The panel as a dialog template, centered on its monitor, with a minimize box (it has a
/// taskbar button of its own). The labels filled from the state are empty here; the check
/// boxes, the method list, and the enabled buttons are set by the glue as well
/// ([`PanelState::view`]).
pub fn panel_template() -> DialogTemplate {
    // Left column x 7..165, right column x 169..323; the controls of a group 7 units in.
    DialogTemplate::new(&panel_title(), PANEL_WIDTH, PANEL_HEIGHT)
        .with_style(styles::WS_MINIMIZEBOX)
        .with_control(
            Control::check_box(ID_VIETNAMESE, "Gõ tiếng Việt", rect(7, 7, 76, 10)).group(),
        )
        .with_control(
            Control::label(ID_HINT, "", rect(90, 8, 233, 8))
                .with_style(styles::SS_NOPREFIX | styles::SS_ENDELLIPSIS),
        )
        .with_control(Control::group_box(
            STATIC_ID,
            "Cách gõ",
            rect(7, 22, 158, 104),
        ))
        .with_control(Control::label(STATIC_ID, "Kiểu gõ:", rect(14, 37, 34, 8)))
        .with_control(Control::drop_down_list(ID_METHOD, rect(50, 35, 108, 12)).group())
        .with_control(Control::check_box(
            ID_MODERN_TONE,
            "Đặt dấu kiểu mới (oà, uý)",
            rect(14, 52, 144, 10),
        ))
        .with_control(two_line_check_box(
            ID_FREE_MARKING,
            "Cho gõ dấu sau phụ âm cuối (toans → toán)",
            rect(14, 65, 144, 18),
        ))
        .with_control(Control::check_box(
            ID_SPELL_CHECK,
            "Kiểm tra chính tả",
            rect(14, 86, 144, 10),
        ))
        .with_control(two_line_check_box(
            ID_AUTO_RESTORE,
            "Giữ nguyên phím đã gõ khi từ không phải tiếng Việt",
            rect(14, 99, 144, 18),
        ))
        .with_control(Control::group_box(
            STATIC_ID,
            "Gõ tắt",
            rect(169, 22, 154, 84),
        ))
        .with_control(Control::check_box(ID_MACROS, "Bật gõ tắt", rect(176, 35, 140, 10)).group())
        .with_control(Control::check_box(
            ID_MACROS_WHEN_ENGLISH,
            "Gõ tắt cả khi đang gõ tiếng Anh",
            rect(176, 48, 140, 10),
        ))
        .with_control(two_line_check_box(
            ID_MACRO_AUTO_CASE,
            "Viết hoa theo cách gõ (Ko → Không, KO → KHÔNG)",
            rect(176, 61, 140, 18),
        ))
        .with_control(Control::push_button(
            ID_MACRO_LIST,
            "Danh sách gõ tắt…",
            rect(176, 84, 84, 14),
        ))
        .with_control(Control::group_box(
            STATIC_ID,
            "Hệ thống",
            rect(169, 110, 154, 42),
        ))
        .with_control(
            Control::check_box(
                ID_STARTUP,
                "Khởi động cùng Windows",
                rect(176, 123, 140, 10),
            )
            .group(),
        )
        .with_control(Control::check_box(
            ID_SHOW_ON_START,
            "Mở bảng này khi Raumakey chạy",
            rect(176, 136, 140, 10),
        ))
        .with_control(
            Control::label(ID_CONFIG_FILE, "", rect(7, 159, 316, 8))
                .with_style(styles::SS_NOPREFIX | styles::SS_PATHELLIPSIS),
        )
        .with_control(Control::label(
            STATIC_ID,
            "Mọi thay đổi có hiệu lực và được lưu ngay.",
            rect(7, 170, 316, 8),
        ))
        .with_control(
            Control::push_button(ID_OPEN_CONFIG, "Mở file cấu hình", rect(7, 184, 76, 14)).group(),
        )
        .with_control(Control::push_button(
            ID_DEFAULTS,
            "Mặc định",
            rect(87, 184, 56, 14),
        ))
        .with_control(Control::default_push_button(
            ID_CLOSE,
            "Đóng",
            rect(267, 184, 56, 14),
        ))
}

/// [`panel_template`] for tests: at ([`OFF_SCREEN`], [`OFF_SCREEN`]) instead of centered, and
/// never activated (`WS_EX_NOACTIVATE`, which also drops the taskbar button). Windows moves a
/// dialog that would start off every monitor back onto one, so the glue moves this one off
/// again before it is shown.
pub fn off_screen_panel_template() -> DialogTemplate {
    panel_template()
        .at(OFF_SCREEN, OFF_SCREEN)
        .with_ex_style(styles::WS_EX_NOACTIVATE)
}
