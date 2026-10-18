//! In-memory dialog templates (pure): the controller's settings panel and macro list are dialog
//! boxes built at run time, because the build has no resource compiler and
//! `raumakey-resources` emits no dialog resources.
//!
//! [`DialogTemplate`] collects the dialog's title, size, position, styles, font, and
//! [`Control`]s, and [`DialogTemplate::build`] lays them out as an extended template
//! ([`TemplateBytes`]) that `DialogBoxIndirectParamW` reads
//! ([`run_dialog`](crate::dialog_box)). All coordinates and sizes are dialog units (DLUs):
//! a horizontal unit is a quarter of the font's average character width, a vertical unit an
//! eighth of its height, so a layout keeps its proportions at every font size and DPI.
//!
//! # Layout of the buffer
//!
//! All fields are little-endian; every string is UTF-16 with a terminating NUL.
//!
//! - `DLGTEMPLATEEX`: `dlgVer` 1 and `signature` `0xFFFF` (which mark the extended form),
//!   `helpID` 0, `exStyle`, `style`, the item count (a WORD), `x`, `y`, `cx`, `cy` (signed
//!   WORDs), menu `0x0000` (none), window class `0x0000` (the predefined dialog class), and the
//!   title. The style always has `DS_SETFONT`, so the font follows: point size (WORD), weight
//!   (WORD), italic (BYTE), character set (BYTE), and the typeface name.
//! - One `DLGITEMTEMPLATEEX` per control, each starting on a DWORD boundary: `helpID` 0,
//!   `exStyle`, `style`, `x`, `y`, `cx`, `cy`, the id (a DWORD), the class (`0xFFFF` and the
//!   atom of a predefined class, or the class name: [`ControlClass`]), the title, and a
//!   creation-data size of 0.
//!
//! The buffer is kept in `u32` words, so it is DWORD-aligned as the dialog manager requires.
//!
//! # Ids
//!
//! Control ids are `u16` values in `1..=0xFFFE`, unique within the dialog; Windows reports a
//! control's notifications with its id in the low word of `wParam`. Use [`IDOK`] for the button
//! that confirms the dialog (Enter presses it) and [`IDCANCEL`] for the one that cancels it
//! (Esc and the close box send it). Labels and group boxes that no code addresses may share
//! [`STATIC_ID`] (`0xFFFF`), which is written as the DWORD `0xFFFF_FFFF` (`IDC_STATIC`, -1, as
//! resource compilers write it), so its low word is `0xFFFF`.
//!
//! # Validation
//!
//! [`DialogTemplate::build`] fails with `E_INVALIDARG`
//! ([`WinErrorKind::InvalidInput`](crate::error::WinErrorKind)) when a text (the title, the
//! typeface, a control's text) contains a NUL, when an id is 0, repeated, or [`STATIC_ID`] on a
//! control other than a label or a group box, and when there are more than `u16::MAX`
//! controls.
//!
//! It also refuses what would make the dialog manager read the template differently from how
//! it was written, and possibly past its end: a text whose first unit is `0xFFFF` (U+FFFF), as
//! a control's text is an `sz_Or_Ord` where that marker announces an ordinal and the unit after
//! the ordinal is read as the creation-data size (the title and the typeface are plain strings,
//! refused the same way so that no text starts with the marker; U+FFFF elsewhere is kept), and
//! the point size [`MESSAGE_BOX_FONT_SIZE`]. A built template is therefore read exactly as laid
//! out above, within its bytes.
//!
//! Texts may name user content (a label can show the path of the settings file), so `Debug`
//! shows their lengths only.

use core::fmt;
use std::collections::HashSet;

use crate::error::{E_INVALIDARG, WinError, WinResult};
use crate::strings::WideString;

/// Window, dialog, and control style bits used by templates.
pub mod styles {
    /// `WS_POPUP`: a top-level window (dialogs).
    pub const WS_POPUP: u32 = 0x8000_0000;
    /// `WS_CHILD`: a child window (controls).
    pub const WS_CHILD: u32 = 0x4000_0000;
    /// `WS_VISIBLE`: the window is shown when it is created.
    pub const WS_VISIBLE: u32 = 0x1000_0000;
    /// `WS_CAPTION`: a title bar.
    pub const WS_CAPTION: u32 = 0x00C0_0000;
    /// `WS_BORDER`: a thin border; the dialog manager draws it as a sunken edge.
    pub const WS_BORDER: u32 = 0x0080_0000;
    /// `WS_VSCROLL`: a vertical scroll bar (the list of a drop-down list).
    pub const WS_VSCROLL: u32 = 0x0020_0000;
    /// `WS_SYSMENU`: the window menu and the close box in the title bar.
    pub const WS_SYSMENU: u32 = 0x0008_0000;
    /// `WS_GROUP` (on a control): the first control of a group; the arrow keys move within a
    /// group, and Tab moves to the next group's control that has `WS_TABSTOP`.
    pub const WS_GROUP: u32 = 0x0002_0000;
    /// `WS_MINIMIZEBOX` (on a dialog; the same bit as `WS_GROUP`): a minimize button.
    pub const WS_MINIMIZEBOX: u32 = 0x0002_0000;
    /// `WS_TABSTOP`: Tab moves the focus to this control.
    pub const WS_TABSTOP: u32 = 0x0001_0000;
    /// `DS_3DLOOK`: three-dimensional borders around the controls.
    pub const DS_3DLOOK: u32 = 0x0004;
    /// `DS_SETFONT`: the template names the dialog's font after its title.
    pub const DS_SETFONT: u32 = 0x0040;
    /// `DS_MODALFRAME`: the frame of a modal dialog box.
    pub const DS_MODALFRAME: u32 = 0x0080;
    /// `DS_CENTER`: the dialog is centered in the work area of its monitor.
    pub const DS_CENTER: u32 = 0x0800;
    /// `WS_EX_APPWINDOW`: a taskbar button even when the window is owned.
    pub const WS_EX_APPWINDOW: u32 = 0x0004_0000;
    /// `WS_EX_NOACTIVATE`: clicking the window does not make it the foreground window, and it
    /// has no taskbar button (unless `WS_EX_APPWINDOW`). Tests use it so that their dialogs
    /// never take the foreground.
    pub const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    /// `BS_PUSHBUTTON`: a push button.
    pub const BS_PUSHBUTTON: u32 = 0x0000;
    /// `BS_DEFPUSHBUTTON`: the default push button (Enter presses it).
    pub const BS_DEFPUSHBUTTON: u32 = 0x0001;
    /// `BS_AUTOCHECKBOX`: a check box that toggles itself when clicked.
    pub const BS_AUTOCHECKBOX: u32 = 0x0003;
    /// `BS_GROUPBOX`: a frame with a caption around other controls.
    pub const BS_GROUPBOX: u32 = 0x0007;
    /// `BS_MULTILINE`: the button's text wraps onto several lines.
    pub const BS_MULTILINE: u32 = 0x2000;
    /// `SS_LEFT`: left-aligned text that wraps.
    pub const SS_LEFT: u32 = 0x0000;
    /// `SS_NOPREFIX`: `&` is shown as is instead of underlining the next character.
    pub const SS_NOPREFIX: u32 = 0x0080;
    /// `SS_ENDELLIPSIS`: text that does not fit ends with an ellipsis.
    pub const SS_ENDELLIPSIS: u32 = 0x4000;
    /// `SS_PATHELLIPSIS`: a path that does not fit loses characters in its middle.
    pub const SS_PATHELLIPSIS: u32 = 0x8000;
    /// `CBS_DROPDOWNLIST`: a combo box whose text is always one of its items.
    pub const CBS_DROPDOWNLIST: u32 = 0x0003;
    /// `ES_AUTOHSCROLL`: the edit scrolls horizontally while the user types.
    pub const ES_AUTOHSCROLL: u32 = 0x0080;
    /// `LVS_REPORT`: a list view with columns.
    pub const LVS_REPORT: u32 = 0x0001;
    /// `LVS_SINGLESEL`: at most one item is selected.
    pub const LVS_SINGLESEL: u32 = 0x0004;
    /// `LVS_SHOWSELALWAYS`: the selection stays visible while the list has no focus.
    pub const LVS_SHOWSELALWAYS: u32 = 0x0008;
}

use styles::{
    BS_AUTOCHECKBOX, BS_DEFPUSHBUTTON, BS_GROUPBOX, BS_PUSHBUTTON, CBS_DROPDOWNLIST, DS_3DLOOK,
    DS_CENTER, DS_MODALFRAME, DS_SETFONT, ES_AUTOHSCROLL, LVS_REPORT, LVS_SHOWSELALWAYS,
    LVS_SINGLESEL, SS_LEFT, WS_BORDER, WS_CAPTION, WS_CHILD, WS_GROUP, WS_POPUP, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

/// The style of every dialog: a pop-up window with a title bar, a close box, and the frame of
/// a modal dialog, whose font the template names. [`DialogTemplate`] adds `DS_CENTER` unless
/// the dialog has an explicit position ([`DialogTemplate::at`]). The template has no
/// `WS_VISIBLE`: the dialog box shows itself once `WM_INITDIALOG` has filled its controls.
pub const DIALOG_STYLE: u32 =
    WS_POPUP | WS_CAPTION | WS_SYSMENU | DS_MODALFRAME | DS_SETFONT | DS_3DLOOK;

/// `IDOK`: the id of the button that confirms the dialog; Enter sends it when no other default
/// button is set.
pub const IDOK: u16 = 1;
/// `IDCANCEL`: the id of the button that cancels the dialog; Esc and the close box send it.
pub const IDCANCEL: u16 = 2;
/// The id shared by labels and group boxes that no code addresses (`IDC_STATIC`); the template
/// holds `0xFFFF_FFFF`.
pub const STATIC_ID: u16 = 0xFFFF;

/// Atom of the predefined `Button` class (push buttons, check boxes, group boxes).
pub const BUTTON_ATOM: u16 = 0x0080;
/// Atom of the predefined `Edit` class.
pub const EDIT_ATOM: u16 = 0x0081;
/// Atom of the predefined `Static` class (labels).
pub const STATIC_ATOM: u16 = 0x0082;
/// Atom of the predefined `ComboBox` class.
pub const COMBOBOX_ATOM: u16 = 0x0085;
/// Class name of the list view common control; `InitCommonControlsEx` registers it.
pub const LIST_VIEW_CLASS: &str = "SysListView32";

/// Height in DLUs that a drop-down list adds below its closed box for its list (about eight
/// items at the default font). Common Controls 6 sizes the list to its items by itself.
pub const DROPPED_LIST_HEIGHT: i16 = 96;

/// The default typeface: the Windows UI font, which covers Vietnamese.
pub const DEFAULT_TYPEFACE: &str = "Segoe UI";
/// The default point size.
pub const DEFAULT_POINT_SIZE: u16 = 9;
/// The point size that asks the dialog manager for the message box font, after which it reads
/// no further font fields: an undocumented special value that USER honours, so the weight,
/// style, and typeface written after it would be read as the first control.
/// [`DialogTemplate::build`] refuses it.
pub const MESSAGE_BOX_FONT_SIZE: u16 = 0x7FFF;
/// `FW_NORMAL`: the default weight.
pub const FW_NORMAL: u16 = 400;
/// `DEFAULT_CHARSET`: the font's character set follows the system locale; with a TrueType
/// typeface the dialog's Unicode text is shown whatever the locale.
pub const DEFAULT_CHARSET: u8 = 1;

/// `sz_Or_Ord` marker: an ordinal (here a class atom) follows. No text of a template starts
/// with it.
const ORDINAL: u16 = 0xFFFF;
/// The DWORD id written for [`STATIC_ID`].
const STATIC_ID_DWORD: u32 = 0xFFFF_FFFF;

/// A rectangle in dialog units: the position of its upper-left corner (relative to the
/// dialog's client area for a control) and its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DialogRect {
    /// Horizontal position.
    pub x: i16,
    /// Vertical position.
    pub y: i16,
    /// Width.
    pub cx: i16,
    /// Height.
    pub cy: i16,
}

impl DialogRect {
    /// A rectangle at `x`, `y` of `cx` by `cy` dialog units.
    pub const fn new(x: i16, y: i16, cx: i16, cy: i16) -> Self {
        Self { x, y, cx, cy }
    }
}

/// The dialog's font (`DS_SETFONT`). The default is [`DEFAULT_TYPEFACE`] at
/// [`DEFAULT_POINT_SIZE`] points, [`FW_NORMAL`], upright, [`DEFAULT_CHARSET`].
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DialogFont {
    typeface: String,
    point_size: u16,
    weight: u16,
    italic: bool,
    charset: u8,
}

impl Default for DialogFont {
    fn default() -> Self {
        Self::new(DEFAULT_TYPEFACE, DEFAULT_POINT_SIZE)
    }
}

impl fmt::Debug for DialogFont {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DialogFont")
            .field("typeface", &self.typeface)
            .field("point_size", &self.point_size)
            .field("weight", &self.weight)
            .field("italic", &self.italic)
            .field("charset", &self.charset)
            .finish()
    }
}

impl DialogFont {
    /// `typeface` at `point_size` points, [`FW_NORMAL`], upright, [`DEFAULT_CHARSET`]. A
    /// typeface with a NUL or that starts with U+FFFF, and the point size
    /// [`MESSAGE_BOX_FONT_SIZE`], make [`DialogTemplate::build`] fail.
    pub fn new(typeface: &str, point_size: u16) -> Self {
        Self {
            typeface: typeface.to_owned(),
            point_size,
            weight: FW_NORMAL,
            italic: false,
            charset: DEFAULT_CHARSET,
        }
    }

    /// The same font with weight `weight` (`FW_*`, 100 to 900).
    #[must_use]
    pub fn with_weight(mut self, weight: u16) -> Self {
        self.weight = weight;
        self
    }

    /// The same font in italics.
    #[must_use]
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }

    /// The same font with character set `charset` (`*_CHARSET`).
    #[must_use]
    pub fn with_charset(mut self, charset: u8) -> Self {
        self.charset = charset;
        self
    }

    /// The typeface name.
    pub fn typeface(&self) -> &str {
        &self.typeface
    }
}

/// The window class of a control.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ControlClass {
    /// `Button` (atom [`BUTTON_ATOM`]): push buttons, check boxes, group boxes.
    Button,
    /// `Edit` (atom [`EDIT_ATOM`]).
    Edit,
    /// `Static` (atom [`STATIC_ATOM`]): labels.
    Static,
    /// `ComboBox` (atom [`COMBOBOX_ATOM`]).
    ComboBox,
    /// `SysListView32` ([`LIST_VIEW_CLASS`]), written by name.
    ListView,
}

impl ControlClass {
    /// The atom of a predefined class; `None` for a class written by name.
    pub const fn atom(self) -> Option<u16> {
        match self {
            Self::Button => Some(BUTTON_ATOM),
            Self::Edit => Some(EDIT_ATOM),
            Self::Static => Some(STATIC_ATOM),
            Self::ComboBox => Some(COMBOBOX_ATOM),
            Self::ListView => None,
        }
    }

    /// The class name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Button => "Button",
            Self::Edit => "Edit",
            Self::Static => "Static",
            Self::ComboBox => "ComboBox",
            Self::ListView => LIST_VIEW_CLASS,
        }
    }
}

/// The kinds of control, for the id rules: only labels and group boxes may use [`STATIC_ID`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Kind {
    CheckBox,
    PushButton,
    GroupBox,
    Label,
    DropDownList,
    Edit,
    ListView,
}

impl Kind {
    const fn class(self) -> ControlClass {
        match self {
            Self::CheckBox | Self::PushButton | Self::GroupBox => ControlClass::Button,
            Self::Label => ControlClass::Static,
            Self::DropDownList => ControlClass::ComboBox,
            Self::Edit => ControlClass::Edit,
            Self::ListView => ControlClass::ListView,
        }
    }

    const fn may_be_static(self) -> bool {
        matches!(self, Self::GroupBox | Self::Label)
    }
}

/// One control of a [`DialogTemplate`], made by one of the typed constructors. Every control is
/// `WS_CHILD | WS_VISIBLE`; the ones that take the focus also have `WS_TABSTOP`.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Control {
    kind: Kind,
    id: u16,
    text: String,
    rect: DialogRect,
    style: u32,
    ex_style: u32,
}

impl fmt::Debug for Control {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The text may be user content (a path); show its length only.
        f.debug_struct("Control")
            .field("class", &self.class())
            .field("id", &self.id)
            .field("text_len", &self.text.len())
            .field("rect", &self.rect)
            .field("style", &format_args!("{:#010x}", self.style))
            .field("ex_style", &format_args!("{:#010x}", self.ex_style))
            .finish()
    }
}

impl Control {
    fn new(kind: Kind, id: u16, text: &str, rect: DialogRect, style: u32) -> Self {
        Self {
            kind,
            id,
            text: text.to_owned(),
            rect,
            style: WS_CHILD | WS_VISIBLE | style,
            ex_style: 0,
        }
    }

    /// A check box that toggles itself when clicked (`BS_AUTOCHECKBOX`). Add
    /// [`styles::BS_MULTILINE`] with [`with_style`](Self::with_style) for a text that wraps.
    pub fn check_box(id: u16, text: &str, rect: DialogRect) -> Self {
        Self::new(Kind::CheckBox, id, text, rect, WS_TABSTOP | BS_AUTOCHECKBOX)
    }

    /// A push button (`BS_PUSHBUTTON`).
    pub fn push_button(id: u16, text: &str, rect: DialogRect) -> Self {
        Self::new(Kind::PushButton, id, text, rect, WS_TABSTOP | BS_PUSHBUTTON)
    }

    /// The default push button (`BS_DEFPUSHBUTTON`), which Enter presses; usually [`IDOK`].
    pub fn default_push_button(id: u16, text: &str, rect: DialogRect) -> Self {
        Self::new(
            Kind::PushButton,
            id,
            text,
            rect,
            WS_TABSTOP | BS_DEFPUSHBUTTON,
        )
    }

    /// A group box (`BS_GROUPBOX`): a frame with a caption. It may use [`STATIC_ID`].
    pub fn group_box(id: u16, text: &str, rect: DialogRect) -> Self {
        Self::new(Kind::GroupBox, id, text, rect, BS_GROUPBOX)
    }

    /// A left-aligned label (`Static`, `SS_LEFT`); its text wraps. Add
    /// [`styles::SS_PATHELLIPSIS`], [`styles::SS_ENDELLIPSIS`], or [`styles::SS_NOPREFIX`]
    /// with [`with_style`](Self::with_style). It may use [`STATIC_ID`].
    pub fn label(id: u16, text: &str, rect: DialogRect) -> Self {
        Self::new(Kind::Label, id, text, rect, SS_LEFT)
    }

    /// A drop-down list (`CBS_DROPDOWNLIST | WS_VSCROLL`): a combo box whose text is always one
    /// of its items. `rect.cy` is the height of the closed box for the layout (Windows sizes
    /// it to the font); the template's height adds [`DROPPED_LIST_HEIGHT`] for the list.
    pub fn drop_down_list(id: u16, rect: DialogRect) -> Self {
        let rect = DialogRect {
            cy: rect.cy.saturating_add(DROPPED_LIST_HEIGHT),
            ..rect
        };
        Self::new(
            Kind::DropDownList,
            id,
            "",
            rect,
            WS_TABSTOP | WS_VSCROLL | CBS_DROPDOWNLIST,
        )
    }

    /// A single-line edit (`ES_AUTOHSCROLL`, `WS_BORDER`), empty until the dialog's handler
    /// fills it.
    pub fn edit(id: u16, rect: DialogRect) -> Self {
        Self::new(
            Kind::Edit,
            id,
            "",
            rect,
            WS_TABSTOP | WS_BORDER | ES_AUTOHSCROLL,
        )
    }

    /// A list view with columns and at most one selected row (`SysListView32`, `LVS_REPORT |
    /// LVS_SINGLESEL | LVS_SHOWSELALWAYS`, `WS_BORDER`), empty until the dialog's handler adds
    /// its columns and rows.
    pub fn list_view(id: u16, rect: DialogRect) -> Self {
        Self::new(
            Kind::ListView,
            id,
            "",
            rect,
            WS_TABSTOP | WS_BORDER | LVS_REPORT | LVS_SINGLESEL | LVS_SHOWSELALWAYS,
        )
    }

    /// The same control as the first of a group (`WS_GROUP`).
    #[must_use]
    pub fn group(self) -> Self {
        self.with_style(WS_GROUP)
    }

    /// The same control with the style bits `bits` added.
    #[must_use]
    pub fn with_style(mut self, bits: u32) -> Self {
        self.style |= bits;
        self
    }

    /// The same control with the extended style bits `bits` added.
    #[must_use]
    pub fn with_ex_style(mut self, bits: u32) -> Self {
        self.ex_style |= bits;
        self
    }

    /// The control's window class.
    pub fn class(&self) -> ControlClass {
        self.kind.class()
    }

    /// The control id.
    pub fn id(&self) -> u16 {
        self.id
    }

    /// The control's text (empty for edits, drop-down lists, and list views).
    pub fn text(&self) -> &str {
        &self.text
    }

    /// The rectangle written to the template.
    pub fn rect(&self) -> DialogRect {
        self.rect
    }

    /// The style written to the template.
    pub fn style(&self) -> u32 {
        self.style
    }

    /// The extended style written to the template.
    pub fn ex_style(&self) -> u32 {
        self.ex_style
    }
}

/// A dialog box to be laid out as an extended template: title, size, position, styles, font,
/// and controls (see the module documentation).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct DialogTemplate {
    title: String,
    rect: DialogRect,
    centered: bool,
    style: u32,
    ex_style: u32,
    font: DialogFont,
    controls: Vec<Control>,
}

impl fmt::Debug for DialogTemplate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DialogTemplate")
            .field("title_len", &self.title.len())
            .field("rect", &self.rect)
            .field("style", &format_args!("{:#010x}", self.style()))
            .field("ex_style", &format_args!("{:#010x}", self.ex_style))
            .field("font", &self.font)
            .field("controls", &self.controls)
            .finish()
    }
}

impl DialogTemplate {
    /// A dialog of `cx` by `cy` dialog units with the title `title`, centered on its monitor,
    /// with [`DIALOG_STYLE`] and the default font, and no controls yet.
    pub fn new(title: &str, cx: i16, cy: i16) -> Self {
        Self {
            title: title.to_owned(),
            rect: DialogRect::new(0, 0, cx, cy),
            centered: true,
            style: DIALOG_STYLE,
            ex_style: 0,
            font: DialogFont::default(),
            controls: Vec::new(),
        }
    }

    /// The same dialog at `x`, `y` (relative to the owner's client area, or to the screen
    /// without an owner) instead of centered: the style loses `DS_CENTER`. Windows still moves
    /// a dialog that would leave the screen back onto it when it is created.
    #[must_use]
    pub fn at(mut self, x: i16, y: i16) -> Self {
        self.rect.x = x;
        self.rect.y = y;
        self.centered = false;
        self
    }

    /// The same dialog with the style bits `bits` added (such as
    /// [`styles::WS_MINIMIZEBOX`]).
    #[must_use]
    pub fn with_style(mut self, bits: u32) -> Self {
        self.style |= bits;
        self
    }

    /// The same dialog with the extended style bits `bits` added (such as
    /// [`styles::WS_EX_NOACTIVATE`]).
    #[must_use]
    pub fn with_ex_style(mut self, bits: u32) -> Self {
        self.ex_style |= bits;
        self
    }

    /// The same dialog with the font `font`.
    #[must_use]
    pub fn with_font(mut self, font: DialogFont) -> Self {
        self.font = font;
        self
    }

    /// The same dialog with `control` added after the others (the tab order follows the order
    /// of the controls).
    #[must_use]
    pub fn with_control(mut self, control: Control) -> Self {
        self.push(control);
        self
    }

    /// Adds `control` after the others.
    pub fn push(&mut self, control: Control) {
        self.controls.push(control);
    }

    /// The title.
    pub fn title(&self) -> &str {
        &self.title
    }

    /// The style written to the template: the styles given plus `DS_CENTER` unless the dialog
    /// has an explicit position.
    pub fn style(&self) -> u32 {
        if self.centered {
            self.style | DS_CENTER
        } else {
            self.style
        }
    }

    /// The extended style written to the template.
    pub fn ex_style(&self) -> u32 {
        self.ex_style
    }

    /// The controls, in tab order.
    pub fn controls(&self) -> &[Control] {
        &self.controls
    }

    /// Lays the dialog out as an extended template. Fails with `E_INVALIDARG` for a text with
    /// a NUL or that starts with U+FFFF, the point size [`MESSAGE_BOX_FONT_SIZE`], an id that is
    /// 0, repeated, or [`STATIC_ID`] on a control other than a label or a group box, and for
    /// more than `u16::MAX` controls (see "Validation" in the module documentation).
    pub fn build(&self) -> WinResult<TemplateBytes> {
        let count = u16::try_from(self.controls.len())
            .map_err(|_| WinError::new(E_INVALIDARG, "dialog item count"))?;
        let title = wide(&self.title, "dialog title")?;
        let typeface = wide(&self.font.typeface, "dialog typeface")?;
        if self.font.point_size == MESSAGE_BOX_FONT_SIZE {
            return Err(WinError::new(E_INVALIDARG, "dialog font point size"));
        }
        let mut ids = HashSet::new();
        for control in &self.controls {
            let valid = match control.id {
                0 => false,
                STATIC_ID => control.kind.may_be_static(),
                id => ids.insert(id),
            };
            if !valid {
                return Err(WinError::new(E_INVALIDARG, "dialog control id"));
            }
        }

        let mut out = Writer::default();
        out.word(1);
        out.word(0xFFFF);
        out.dword(0);
        out.dword(self.ex_style);
        out.dword(self.style());
        out.word(count);
        out.rect(self.rect);
        out.word(0);
        out.word(0);
        out.units(title.as_units_with_nul());
        out.word(self.font.point_size);
        out.word(self.font.weight);
        out.byte(u8::from(self.font.italic));
        out.byte(self.font.charset);
        out.units(typeface.as_units_with_nul());
        for control in &self.controls {
            let text = wide(&control.text, "dialog control text")?;
            out.align();
            out.dword(0);
            out.dword(control.ex_style);
            out.dword(control.style);
            out.rect(control.rect);
            out.dword(match control.id {
                STATIC_ID => STATIC_ID_DWORD,
                id => u32::from(id),
            });
            let class = control.class();
            match class.atom() {
                Some(atom) => {
                    out.word(ORDINAL);
                    out.word(atom);
                }
                None => out.units(wide(class.name(), "dialog control class")?.as_units_with_nul()),
            }
            out.units(text.as_units_with_nul());
            out.word(0);
        }
        Ok(out.finish())
    }
}

/// `text` as a NUL-terminated wide string; a NUL in it, or [`ORDINAL`] as its first unit (see
/// "Validation" in the module documentation), fails with `E_INVALIDARG` for `context`.
fn wide(text: &str, context: &'static str) -> WinResult<WideString> {
    let wide = WideString::new(text).map_err(|error| error.with_context(context))?;
    if wide.as_units_with_nul().first() == Some(&ORDINAL) {
        return Err(WinError::new(E_INVALIDARG, context));
    }
    Ok(wide)
}

/// Writes little-endian fields into a growing byte buffer.
#[derive(Default)]
struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    fn byte(&mut self, value: u8) {
        self.bytes.push(value);
    }

    fn word(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn dword(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    fn rect(&mut self, rect: DialogRect) {
        for value in [rect.x, rect.y, rect.cx, rect.cy] {
            self.bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    fn units(&mut self, units: &[u16]) {
        for &unit in units {
            self.word(unit);
        }
    }

    /// Pads with zero bytes up to the next DWORD boundary.
    fn align(&mut self) {
        while !self.bytes.len().is_multiple_of(4) {
            self.bytes.push(0);
        }
    }

    fn finish(self) -> TemplateBytes {
        let len = self.bytes.len();
        // Native-endian words of the little-endian bytes: in memory the words hold exactly
        // those bytes, in order, on any host.
        let words = self
            .bytes
            .chunks(4)
            .map(|chunk| {
                let mut word = [0_u8; 4];
                if let Some(head) = word.get_mut(..chunk.len()) {
                    head.copy_from_slice(chunk);
                }
                u32::from_ne_bytes(word)
            })
            .collect();
        TemplateBytes { words, len }
    }
}

/// A built extended dialog template: DWORD-aligned bytes that `DialogBoxIndirectParamW` reads
/// in place. It may be sent to another thread (the thread of a detached dialog).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct TemplateBytes {
    /// The bytes, in `u32` words for the DWORD alignment; the last word is zero-padded.
    words: Vec<u32>,
    /// The number of bytes used.
    len: usize,
}

impl fmt::Debug for TemplateBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The bytes hold the texts; show the length only.
        f.debug_struct("TemplateBytes")
            .field("len", &self.len)
            .finish()
    }
}

impl TemplateBytes {
    /// The template's bytes.
    pub fn as_bytes(&self) -> &[u8] {
        let len = self.len.min(self.words.len() * 4);
        // SAFETY: the words are initialized memory of `words.len() * 4` bytes, `len` does not
        // exceed that, `u8` has no alignment requirement, and the slice borrows `self`.
        unsafe { core::slice::from_raw_parts(self.words.as_ptr().cast::<u8>(), len) }
    }

    /// The number of bytes.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when there are no bytes (never for a built template).
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The DWORD-aligned start of the template, valid while `self` lives and is not changed.
    pub fn as_ptr(&self) -> *const u32 {
        self.words.as_ptr()
    }
}
