//! `dialog_template`: in-memory extended dialog templates (pure). Every test decodes the built
//! buffer field by field, the way the dialog manager reads it: little-endian fields, UTF-16
//! strings up to their NUL, each item on a DWORD boundary.

use raumakey_windows::dialog_template::{
    BUTTON_ATOM, COMBOBOX_ATOM, Control, ControlClass, DIALOG_STYLE, DROPPED_LIST_HEIGHT,
    DialogFont, DialogRect, DialogTemplate, EDIT_ATOM, IDCANCEL, IDOK, LIST_VIEW_CLASS,
    MESSAGE_BOX_FONT_SIZE, STATIC_ATOM, STATIC_ID, TemplateBytes, styles,
};
use raumakey_windows::error::{E_INVALIDARG, WinErrorKind};

/// The class of a decoded item: a predefined class atom or a class name.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Class {
    Atom(u16),
    Name(String),
}

/// A decoded position and size: `x`, `y`, `cx`, `cy`.
type Rect = (i16, i16, i16, i16);

/// The decoded `DLGTEMPLATEEX` header.
#[derive(Debug)]
struct Header {
    version: u16,
    signature: u16,
    help_id: u32,
    ex_style: u32,
    style: u32,
    count: u16,
    rect: Rect,
    menu: u16,
    class: u16,
    title: String,
    point_size: u16,
    weight: u16,
    italic: u8,
    charset: u8,
    typeface: String,
}

/// One decoded `DLGITEMTEMPLATEEX`.
#[derive(Debug)]
struct Item {
    /// Byte offset of the item in the buffer.
    offset: usize,
    help_id: u32,
    ex_style: u32,
    style: u32,
    rect: Rect,
    id: u32,
    class: Class,
    title: String,
    extra: u16,
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> [u8; N] {
        let field: [u8; N] = self.bytes[self.at..self.at + N].try_into().unwrap();
        self.at += N;
        field
    }

    fn u8(&mut self) -> u8 {
        self.take::<1>()[0]
    }

    fn u16(&mut self) -> u16 {
        u16::from_le_bytes(self.take())
    }

    fn i16(&mut self) -> i16 {
        i16::from_le_bytes(self.take())
    }

    fn u32(&mut self) -> u32 {
        u32::from_le_bytes(self.take())
    }

    fn rect(&mut self) -> Rect {
        (self.i16(), self.i16(), self.i16(), self.i16())
    }

    /// A NUL-terminated UTF-16 string; ill-formed UTF-16 fails the test.
    fn string(&mut self) -> String {
        let mut units = Vec::new();
        loop {
            match self.u16() {
                0 => return String::from_utf16(&units).unwrap(),
                unit => units.push(unit),
            }
        }
    }

    /// `sz_Or_Ord`: `0xFFFF` and an atom, or a string.
    fn class(&mut self) -> Class {
        let first = u16::from_le_bytes([self.bytes[self.at], self.bytes[self.at + 1]]);
        if first == 0xFFFF {
            self.at += 2;
            Class::Atom(self.u16())
        } else {
            Class::Name(self.string())
        }
    }

    /// Skips the padding up to the next DWORD boundary, which must be zero.
    fn align(&mut self) {
        while !self.at.is_multiple_of(4) {
            assert_eq!(self.u8(), 0, "padding at {}", self.at - 1);
        }
    }
}

fn decode(template: &TemplateBytes) -> (Header, Vec<Item>) {
    let bytes = template.as_bytes();
    assert_eq!(
        template.as_ptr() as usize % 4,
        0,
        "the buffer is DWORD-aligned"
    );
    assert_eq!(bytes.as_ptr() as usize, template.as_ptr() as usize);
    let mut reader = Reader { bytes, at: 0 };
    let header = Header {
        version: reader.u16(),
        signature: reader.u16(),
        help_id: reader.u32(),
        ex_style: reader.u32(),
        style: reader.u32(),
        count: reader.u16(),
        rect: reader.rect(),
        menu: reader.u16(),
        class: reader.u16(),
        title: reader.string(),
        point_size: reader.u16(),
        weight: reader.u16(),
        italic: reader.u8(),
        charset: reader.u8(),
        typeface: reader.string(),
    };
    let mut items = Vec::new();
    for _ in 0..header.count {
        reader.align();
        let offset = reader.at;
        items.push(Item {
            offset,
            help_id: reader.u32(),
            ex_style: reader.u32(),
            style: reader.u32(),
            rect: reader.rect(),
            id: reader.u32(),
            class: reader.class(),
            title: reader.string(),
            extra: reader.u16(),
        });
    }
    assert_eq!(
        reader.at,
        bytes.len(),
        "the template ends after its last item"
    );
    (header, items)
}

fn rect(x: i16, y: i16, cx: i16, cy: i16) -> DialogRect {
    DialogRect::new(x, y, cx, cy)
}

const CHILD: u32 = styles::WS_CHILD | styles::WS_VISIBLE;

#[test]
fn style_constants_have_their_win32_values() {
    assert_eq!(styles::WS_POPUP, 0x8000_0000);
    assert_eq!(styles::WS_CHILD, 0x4000_0000);
    assert_eq!(styles::WS_VISIBLE, 0x1000_0000);
    assert_eq!(styles::WS_CAPTION, 0x00C0_0000);
    assert_eq!(styles::WS_BORDER, 0x0080_0000);
    assert_eq!(styles::WS_VSCROLL, 0x0020_0000);
    assert_eq!(styles::WS_SYSMENU, 0x0008_0000);
    assert_eq!(styles::WS_GROUP, 0x0002_0000);
    assert_eq!(styles::WS_MINIMIZEBOX, 0x0002_0000);
    assert_eq!(styles::WS_TABSTOP, 0x0001_0000);
    assert_eq!(styles::DS_3DLOOK, 0x0004);
    assert_eq!(styles::DS_SETFONT, 0x0040);
    assert_eq!(styles::DS_MODALFRAME, 0x0080);
    assert_eq!(styles::DS_CENTER, 0x0800);
    assert_eq!(styles::WS_EX_NOACTIVATE, 0x0800_0000);
    assert_eq!(styles::WS_EX_APPWINDOW, 0x0004_0000);
    assert_eq!(styles::BS_PUSHBUTTON, 0x0000);
    assert_eq!(styles::BS_DEFPUSHBUTTON, 0x0001);
    assert_eq!(styles::BS_AUTOCHECKBOX, 0x0003);
    assert_eq!(styles::BS_GROUPBOX, 0x0007);
    assert_eq!(styles::BS_MULTILINE, 0x2000);
    assert_eq!(styles::SS_LEFT, 0x0000);
    assert_eq!(styles::SS_NOPREFIX, 0x0080);
    assert_eq!(styles::SS_ENDELLIPSIS, 0x4000);
    assert_eq!(styles::SS_PATHELLIPSIS, 0x8000);
    assert_eq!(styles::CBS_DROPDOWNLIST, 0x0003);
    assert_eq!(styles::CBS_DROPDOWN, 0x0002);
    assert_eq!(styles::CBS_AUTOHSCROLL, 0x0040);
    assert_eq!(styles::ES_AUTOHSCROLL, 0x0080);
    assert_eq!(styles::LVS_REPORT, 0x0001);
    assert_eq!(styles::LVS_SINGLESEL, 0x0004);
    assert_eq!(styles::LVS_SHOWSELALWAYS, 0x0008);
    assert_eq!((IDOK, IDCANCEL, STATIC_ID), (1, 2, 0xFFFF));
    assert_eq!(
        (BUTTON_ATOM, EDIT_ATOM, STATIC_ATOM, COMBOBOX_ATOM),
        (0x0080, 0x0081, 0x0082, 0x0085)
    );
    assert_eq!(LIST_VIEW_CLASS, "SysListView32");
    assert_eq!(
        DIALOG_STYLE,
        styles::WS_POPUP
            | styles::WS_CAPTION
            | styles::WS_SYSMENU
            | styles::DS_MODALFRAME
            | styles::DS_SETFONT
            | styles::DS_3DLOOK
    );
}

#[test]
fn the_header_is_an_extended_template_with_the_default_style_font_and_title() {
    let template = DialogTemplate::new("Bảng cấu hình", 300, 200);
    assert_eq!(template.style(), DIALOG_STYLE | styles::DS_CENTER);
    let (header, items) = decode(&template.build().unwrap());
    assert_eq!((header.version, header.signature), (1, 0xFFFF));
    assert_eq!(header.help_id, 0);
    assert_eq!(header.ex_style, 0);
    assert_eq!(header.style, DIALOG_STYLE | styles::DS_CENTER);
    assert_eq!(header.count, 0);
    assert!(items.is_empty());
    assert_eq!(header.rect, (0, 0, 300, 200));
    assert_eq!(
        (header.menu, header.class),
        (0, 0),
        "no menu, the dialog class"
    );
    assert_eq!(header.title, "Bảng cấu hình");
    assert_eq!(
        (
            header.point_size,
            header.weight,
            header.italic,
            header.charset
        ),
        (9, 400, 0, 1)
    );
    assert_eq!(header.typeface, "Segoe UI");
    assert_eq!(DialogFont::default().typeface(), "Segoe UI");
}

#[test]
fn an_explicit_position_drops_ds_center_and_extra_styles_are_added() {
    let template = DialogTemplate::new("Raumakey", 120, 80)
        .at(-30_000, -29_000)
        .with_style(styles::WS_MINIMIZEBOX)
        .with_ex_style(styles::WS_EX_NOACTIVATE);
    let expected = DIALOG_STYLE | styles::WS_MINIMIZEBOX;
    assert_eq!(template.style(), expected);
    assert_eq!(template.ex_style(), styles::WS_EX_NOACTIVATE);
    let (header, _) = decode(&template.build().unwrap());
    assert_eq!(header.style, expected);
    assert_eq!(header.style & styles::DS_CENTER, 0);
    assert_eq!(header.ex_style, styles::WS_EX_NOACTIVATE);
    assert_eq!(header.rect, (-30_000, -29_000, 120, 80));
}

#[test]
fn a_custom_font_is_written_after_the_title() {
    let font = DialogFont::new("Tahoma", 10)
        .with_weight(700)
        .italic()
        .with_charset(163);
    assert_eq!(font.typeface(), "Tahoma");
    let template = DialogTemplate::new("", 10, 10).with_font(font);
    let (header, _) = decode(&template.build().unwrap());
    assert_eq!(header.title, "");
    assert_eq!(
        (
            header.point_size,
            header.weight,
            header.italic,
            header.charset
        ),
        (10, 700, 1, 163)
    );
    assert_eq!(header.typeface, "Tahoma");
}

#[test]
fn typed_controls_get_their_default_styles() {
    let at = rect(1, 2, 3, 4);
    let cases = [
        (
            Control::check_box(10, "a", at),
            ControlClass::Button,
            CHILD | styles::WS_TABSTOP | styles::BS_AUTOCHECKBOX,
        ),
        (
            Control::push_button(11, "b", at),
            ControlClass::Button,
            CHILD | styles::WS_TABSTOP | styles::BS_PUSHBUTTON,
        ),
        (
            Control::default_push_button(IDOK, "c", at),
            ControlClass::Button,
            CHILD | styles::WS_TABSTOP | styles::BS_DEFPUSHBUTTON,
        ),
        (
            Control::group_box(STATIC_ID, "d", at),
            ControlClass::Button,
            CHILD | styles::BS_GROUPBOX,
        ),
        (
            Control::label(STATIC_ID, "e", at),
            ControlClass::Static,
            CHILD | styles::SS_LEFT,
        ),
        (
            Control::drop_down_list(12, at),
            ControlClass::ComboBox,
            CHILD | styles::WS_TABSTOP | styles::WS_VSCROLL | styles::CBS_DROPDOWNLIST,
        ),
        (
            Control::drop_down_combo(15, at),
            ControlClass::ComboBox,
            CHILD
                | styles::WS_TABSTOP
                | styles::WS_VSCROLL
                | styles::CBS_DROPDOWN
                | styles::CBS_AUTOHSCROLL,
        ),
        (
            Control::edit(13, at),
            ControlClass::Edit,
            CHILD | styles::WS_TABSTOP | styles::WS_BORDER | styles::ES_AUTOHSCROLL,
        ),
        (
            Control::list_view(14, at),
            ControlClass::ListView,
            CHILD
                | styles::WS_TABSTOP
                | styles::WS_BORDER
                | styles::LVS_REPORT
                | styles::LVS_SINGLESEL
                | styles::LVS_SHOWSELALWAYS,
        ),
    ];
    for (control, class, style) in cases {
        assert_eq!(control.class(), class, "{control:?}");
        assert_eq!(control.style(), style, "{control:?}");
        assert_eq!(control.ex_style(), 0, "{control:?}");
        let height = if class == ControlClass::ComboBox {
            4 + DROPPED_LIST_HEIGHT
        } else {
            4
        };
        assert_eq!(control.rect(), rect(1, 2, 3, height), "{control:?}");
    }
    assert_eq!(
        Control::check_box(10, "a", at)
            .with_style(styles::BS_MULTILINE)
            .group()
            .style(),
        CHILD
            | styles::WS_TABSTOP
            | styles::BS_AUTOCHECKBOX
            | styles::BS_MULTILINE
            | styles::WS_GROUP
    );
    assert_eq!(
        Control::label(20, "path", at)
            .with_style(styles::SS_PATHELLIPSIS | styles::SS_NOPREFIX)
            .style(),
        CHILD | styles::SS_PATHELLIPSIS | styles::SS_NOPREFIX
    );
    assert_eq!(
        Control::edit(21, at).with_ex_style(0x0200).ex_style(),
        0x0200
    );
    assert_eq!(ControlClass::Button.atom(), Some(BUTTON_ATOM));
    assert_eq!(ControlClass::Edit.atom(), Some(EDIT_ATOM));
    assert_eq!(ControlClass::Static.atom(), Some(STATIC_ATOM));
    assert_eq!(ControlClass::ComboBox.atom(), Some(COMBOBOX_ATOM));
    assert_eq!(ControlClass::ListView.atom(), None);
    assert_eq!(ControlClass::ListView.name(), LIST_VIEW_CLASS);
}

#[test]
fn items_sit_on_dword_boundaries_with_class_atoms_or_names_ids_and_titles() {
    let template = DialogTemplate::new("Raumakey", 200, 150)
        .with_control(Control::group_box(
            STATIC_ID,
            "Kiểu gõ",
            rect(7, 7, 186, 40),
        ))
        .with_control(Control::label(STATIC_ID, "Bảng mã:", rect(14, 20, 40, 8)))
        .with_control(Control::drop_down_list(101, rect(60, 18, 120, 12)).group())
        .with_control(Control::check_box(
            102,
            "Kiểm tra chính tả",
            rect(14, 52, 150, 10),
        ))
        .with_control(Control::edit(103, rect(14, 66, 100, 12)))
        .with_control(Control::list_view(104, rect(14, 82, 170, 40)))
        .with_control(Control::default_push_button(
            IDOK,
            "Đóng",
            rect(90, 130, 50, 14),
        ))
        .with_control(Control::push_button(
            IDCANCEL,
            "Hủy",
            rect(143, 130, 50, 14),
        ));
    let (header, items) = decode(&template.build().unwrap());
    assert_eq!(header.count, 8);
    let expected: [(u32, Class, &str, Rect); 8] = [
        (0xFFFF_FFFF, Class::Atom(0x0080), "Kiểu gõ", (7, 7, 186, 40)),
        (
            0xFFFF_FFFF,
            Class::Atom(0x0082),
            "Bảng mã:",
            (14, 20, 40, 8),
        ),
        (
            101,
            Class::Atom(0x0085),
            "",
            (60, 18, 120, 12 + DROPPED_LIST_HEIGHT),
        ),
        (
            102,
            Class::Atom(0x0080),
            "Kiểm tra chính tả",
            (14, 52, 150, 10),
        ),
        (103, Class::Atom(0x0081), "", (14, 66, 100, 12)),
        (
            104,
            Class::Name("SysListView32".to_owned()),
            "",
            (14, 82, 170, 40),
        ),
        (1, Class::Atom(0x0080), "Đóng", (90, 130, 50, 14)),
        (2, Class::Atom(0x0080), "Hủy", (143, 130, 50, 14)),
    ];
    for ((item, control), (id, class, title, at)) in
        items.iter().zip(template.controls()).zip(expected)
    {
        assert_eq!(item.offset % 4, 0, "{item:?}");
        assert_eq!(item.help_id, 0, "{item:?}");
        assert_eq!(item.extra, 0, "no creation data: {item:?}");
        assert_eq!(item.id, id, "{item:?}");
        assert_eq!(item.class, class, "{item:?}");
        assert_eq!(item.title, title, "{item:?}");
        assert_eq!(item.rect, at, "{item:?}");
        assert_eq!(item.style, control.style(), "{item:?}");
        assert_eq!(item.ex_style, control.ex_style(), "{item:?}");
    }
    assert_eq!(items[2].style & styles::WS_GROUP, styles::WS_GROUP);
}

/// Titles of every length parity (and a character outside the BMP, which takes two units)
/// leave the next item on a DWORD boundary.
#[test]
fn every_title_length_keeps_the_next_item_aligned() {
    for length in 0..6 {
        for tail in ["", "😀"] {
            let title = format!("{}{tail}", "x".repeat(length));
            let template = DialogTemplate::new(&title, 50, 50)
                .with_control(Control::label(STATIC_ID, &title, rect(0, 0, 10, 10)))
                .with_control(Control::check_box(5, &title, rect(0, 10, 10, 10)))
                .with_control(Control::list_view(6, rect(0, 20, 10, 10)));
            let (header, items) = decode(&template.build().unwrap());
            assert_eq!(header.title, title);
            assert_eq!(items.len(), 3);
            for item in &items {
                assert_eq!(item.offset % 4, 0, "{title:?}: {item:?}");
            }
            assert_eq!(items[0].title, title);
            assert_eq!(items[1].title, title);
        }
    }
}

#[test]
fn vietnamese_text_round_trips_through_utf16() {
    let texts = [
        "Kiểm tra chính tả",
        "Đặt dấu kiểu mới (oà, uý)",
        "Gõ tắt (macro)",
        "Bỏ dấu tự do: ươ ƯƠ ỹ Ỹ",
    ];
    let mut template = DialogTemplate::new(texts[0], 200, 100);
    for (index, text) in (1_u16..).zip(texts) {
        template.push(Control::check_box(
            index,
            text,
            rect(5, 10 * index as i16, 150, 10),
        ));
    }
    let (header, items) = decode(&template.build().unwrap());
    assert_eq!(header.title, texts[0]);
    let titles: Vec<&str> = items.iter().map(|item| item.title.as_str()).collect();
    assert_eq!(titles, texts);
}

fn assert_invalid(template: &DialogTemplate, case: &str) {
    let error = template.build().unwrap_err();
    assert_eq!(error.code(), E_INVALIDARG, "{case}: {error}");
    assert_eq!(error.kind(), WinErrorKind::InvalidInput, "{case}");
}

#[test]
fn a_nul_in_any_text_is_rejected() {
    let at = rect(0, 0, 10, 10);
    assert_invalid(&DialogTemplate::new("a\0b", 10, 10), "title");
    assert_invalid(
        &DialogTemplate::new("a", 10, 10).with_font(DialogFont::new("Segoe\0UI", 9)),
        "typeface",
    );
    for control in [
        Control::check_box(1, "x\0", at),
        Control::push_button(1, "\0", at),
        Control::label(STATIC_ID, "a\0b", at),
        Control::group_box(STATIC_ID, "\0x", at),
    ] {
        let case = format!("{control:?}");
        assert_invalid(
            &DialogTemplate::new("a", 10, 10).with_control(control),
            &case,
        );
    }
}

/// A control's text is an `sz_Or_Ord` in the template: one whose first unit is `0xFFFF`
/// (U+FFFF) would be read as an ordinal followed by the creation-data size, so the dialog
/// manager would misread the rest of the template and could read past its end. The title and
/// the typeface are plain strings, refused the same way so that no text of a template starts
/// with the marker; U+FFFF anywhere else is an ordinary character.
#[test]
fn a_text_that_starts_with_the_ordinal_marker_is_rejected() {
    let at = rect(0, 0, 10, 10);
    for control in [
        Control::label(10, "\u{FFFF}AB", at),
        Control::label(STATIC_ID, "\u{FFFF}", at),
        Control::check_box(11, "\u{FFFF}A", at),
        Control::push_button(12, "\u{FFFF}x", at),
        Control::default_push_button(IDOK, "\u{FFFF}", at),
        Control::group_box(STATIC_ID, "\u{FFFF}Kiểu gõ", at),
    ] {
        let case = format!("{control:?}");
        assert_invalid(
            &DialogTemplate::new("a", 10, 10).with_control(control),
            &case,
        );
    }
    assert_invalid(&DialogTemplate::new("\u{FFFF}", 10, 10), "title");
    assert_invalid(
        &DialogTemplate::new("a", 10, 10).with_font(DialogFont::new("\u{FFFF}Segoe UI", 9)),
        "typeface",
    );
    let template = DialogTemplate::new("a\u{FFFF}", 10, 10)
        .with_font(DialogFont::new("Segoe UI\u{FFFF}", 9))
        .with_control(Control::label(10, "x\u{FFFF}y", at));
    let (header, items) = decode(&template.build().unwrap());
    assert_eq!(header.title, "a\u{FFFF}");
    assert_eq!(header.typeface, "Segoe UI\u{FFFF}");
    assert_eq!(items[0].title, "x\u{FFFF}y");
}

/// A point size of `0x7FFF` asks for the message box font, after which the dialog manager
/// reads no further font fields (an undocumented special value that USER honours), so it
/// would read the weight, the style, and the typeface as the first control. It is refused;
/// every other size is written as given.
#[test]
fn the_point_size_that_asks_for_the_message_box_font_is_rejected() {
    assert_eq!(MESSAGE_BOX_FONT_SIZE, 0x7FFF);
    assert_invalid(
        &DialogTemplate::new("a", 10, 10).with_font(DialogFont::new("Segoe UI", 0x7FFF)),
        "point size 0x7FFF",
    );
    for size in [1, 72, 0x7FFE] {
        let template = DialogTemplate::new("a", 10, 10)
            .with_font(DialogFont::new("Segoe UI", size))
            .with_control(Control::label(STATIC_ID, "x", rect(0, 0, 10, 10)));
        let (header, items) = decode(&template.build().unwrap());
        assert_eq!(header.point_size, size);
        assert_eq!(items.len(), 1);
    }
}

#[test]
fn ids_are_non_zero_and_unique_except_the_static_id_of_labels_and_group_boxes() {
    let at = rect(0, 0, 10, 10);
    let template = |controls: Vec<Control>| {
        let mut template = DialogTemplate::new("ids", 100, 100);
        for control in controls {
            template.push(control);
        }
        template
    };
    // Labels and group boxes may share the static id, and labels may have ids of their own.
    let shared = template(vec![
        Control::label(STATIC_ID, "a", at),
        Control::label(STATIC_ID, "b", at),
        Control::group_box(STATIC_ID, "c", at),
        Control::group_box(STATIC_ID, "d", at),
        Control::label(30, "e", at),
        Control::check_box(31, "f", at),
    ]);
    assert!(shared.build().is_ok());
    assert_invalid(&template(vec![Control::check_box(0, "zero", at)]), "id 0");
    assert_invalid(&template(vec![Control::label(0, "zero", at)]), "label id 0");
    assert_invalid(
        &template(vec![Control::check_box(7, "a", at), Control::edit(7, at)]),
        "duplicate",
    );
    assert_invalid(
        &template(vec![Control::label(8, "a", at), Control::label(8, "b", at)]),
        "duplicate label id",
    );
    for control in [
        Control::check_box(STATIC_ID, "a", at),
        Control::push_button(STATIC_ID, "a", at),
        Control::default_push_button(STATIC_ID, "a", at),
        Control::drop_down_list(STATIC_ID, at),
        Control::drop_down_combo(STATIC_ID, at),
        Control::edit(STATIC_ID, at),
        Control::list_view(STATIC_ID, at),
    ] {
        let case = format!("{control:?}");
        assert_invalid(&template(vec![control]), &case);
    }
}

#[test]
fn the_item_count_must_fit_in_a_word() {
    let at = rect(0, 0, 1, 1);
    let mut template = DialogTemplate::new("many", 10, 10);
    for _ in 0..u16::MAX {
        template.push(Control::label(STATIC_ID, "", at));
    }
    let (header, items) = decode(&template.build().unwrap());
    assert_eq!(header.count, u16::MAX);
    assert_eq!(items.len(), usize::from(u16::MAX));
    template.push(Control::label(STATIC_ID, "", at));
    assert_invalid(&template, "65536 items");
}

/// Texts may name user content (a label can show a path), so `Debug` shows lengths only.
#[test]
fn debug_output_shows_no_text() {
    let secret = "C:\\Users\\Bí mật\\raumakey.toml";
    let control = Control::label(40, secret, rect(0, 0, 10, 10));
    let template = DialogTemplate::new(secret, 10, 10).with_control(control.clone());
    let bytes = template.build().unwrap();
    for shown in [
        format!("{control:?}"),
        format!("{template:?}"),
        format!("{bytes:?}"),
    ] {
        assert!(!shown.contains("Bí mật"), "{shown}");
        assert!(!shown.contains("Users"), "{shown}");
    }
    assert_eq!(control.text(), secret);
    assert_eq!(control.id(), 40);
    assert_eq!(template.title(), secret);
}

/// The macro list editor's controls: a multi-line edit that wraps its lines and takes Enter
/// as a line break, and a list view whose rows the dialog supplies on demand (owner data).
#[test]
fn multi_line_edits_and_virtual_list_views_get_their_styles() {
    assert_eq!(styles::ES_MULTILINE, 0x0004);
    assert_eq!(styles::ES_AUTOVSCROLL, 0x0040);
    assert_eq!(styles::ES_WANTRETURN, 0x1000);
    assert_eq!(styles::LVS_OWNERDATA, 0x1000);
    let at = rect(1, 2, 3, 4);
    let edit = Control::multiline_edit(30, at);
    assert_eq!(edit.class(), ControlClass::Edit);
    assert_eq!(
        edit.style(),
        CHILD
            | styles::WS_TABSTOP
            | styles::WS_BORDER
            | styles::WS_VSCROLL
            | styles::ES_MULTILINE
            | styles::ES_AUTOVSCROLL
            | styles::ES_WANTRETURN
    );
    assert_eq!(
        edit.style() & styles::ES_AUTOHSCROLL,
        0,
        "long lines wrap instead of scrolling sideways"
    );
    let list = Control::virtual_list_view(31, at);
    assert_eq!(list.class(), ControlClass::ListView);
    assert_eq!(
        list.style(),
        Control::list_view(31, at).style() | styles::LVS_OWNERDATA
    );
    for control in [edit, list] {
        assert_eq!(control.rect(), at, "{control:?}");
        assert_eq!(control.text(), "");
        assert_eq!(control.ex_style(), 0);
    }
    let (_, items) = decode(
        &DialogTemplate::new("Danh sách", 100, 100)
            .with_control(Control::multiline_edit(30, at))
            .with_control(Control::virtual_list_view(31, at))
            .build()
            .unwrap(),
    );
    assert_eq!(items[0].class, Class::Atom(EDIT_ATOM));
    assert_eq!(items[1].class, Class::Name(LIST_VIEW_CLASS.to_owned()));
    assert_ne!(items[1].style & styles::LVS_OWNERDATA, 0);
}
