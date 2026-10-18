//! `file_dialog`: what the Open dialog is asked (its flags and the list of file types) and
//! how its answer is read, pure. The dialog itself is shown through `Dialog::choose_file`
//! (`dialog_box_choices`, where the test that shows it is manual).

use std::path::PathBuf;

use raumakey_windows::error::WinErrorKind;
use raumakey_windows::file_dialog::{
    FileFilter, OPEN_FLAGS, PATH_CAPACITY, chosen_path, filter_units, flags,
};

fn units(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

#[test]
fn the_open_flags_ask_for_an_existing_file_in_an_explorer_dialog() {
    assert_eq!(flags::OFN_HIDEREADONLY, 0x0000_0004);
    assert_eq!(flags::OFN_NOCHANGEDIR, 0x0000_0008);
    assert_eq!(flags::OFN_PATHMUSTEXIST, 0x0000_0800);
    assert_eq!(flags::OFN_FILEMUSTEXIST, 0x0000_1000);
    assert_eq!(flags::OFN_EXPLORER, 0x0008_0000);
    assert_eq!(
        OPEN_FLAGS,
        flags::OFN_EXPLORER
            | flags::OFN_FILEMUSTEXIST
            | flags::OFN_PATHMUSTEXIST
            | flags::OFN_HIDEREADONLY
            | flags::OFN_NOCHANGEDIR
    );
    // A long path fits the answer's buffer.
    const { assert!(PATH_CAPACITY >= 32_768) };
}

/// `lpstrFilter`: each label and its patterns, each with its NUL, and one more NUL after the
/// last pair.
#[test]
fn the_filter_lists_each_label_and_its_patterns() {
    let programs = [FileFilter {
        label: "Chương trình (*.exe)",
        patterns: "*.exe",
    }];
    assert_eq!(
        filter_units(&programs).unwrap(),
        units("Chương trình (*.exe)\0*.exe\0\0")
    );
    let two = [
        programs[0],
        FileFilter {
            label: "Mọi tệp",
            patterns: "*.*",
        },
    ];
    assert_eq!(
        filter_units(&two).unwrap(),
        units("Chương trình (*.exe)\0*.exe\0Mọi tệp\0*.*\0\0")
    );
}

/// A list that the dialog would read otherwise than written is refused: none at all, a NUL
/// inside a text, an empty label or patterns (an empty text would end the list there).
#[test]
fn a_filter_the_dialog_would_misread_is_refused() {
    let cases: [&[FileFilter<'_>]; 5] = [
        &[],
        &[FileFilter {
            label: "a\0b",
            patterns: "*.exe",
        }],
        &[FileFilter {
            label: "Chương trình",
            patterns: "*.exe\0*.com",
        }],
        &[FileFilter {
            label: "",
            patterns: "*.exe",
        }],
        &[FileFilter {
            label: "Chương trình",
            patterns: "",
        }],
    ];
    for filters in cases {
        assert_eq!(
            filter_units(filters).map_err(|error| error.kind()),
            Err(WinErrorKind::InvalidInput),
            "{filters:?}"
        );
    }
}

/// The answer is the path up to the first NUL of the buffer; an empty one is no answer.
#[test]
fn the_chosen_path_ends_at_the_first_nul() {
    let mut buffer = units(r"C:\Program Files\PuTTY\putty.exe");
    buffer.extend([0, u16::from(b'x'), 0]);
    assert_eq!(
        chosen_path(&buffer),
        Some(PathBuf::from(r"C:\Program Files\PuTTY\putty.exe"))
    );
    assert_eq!(chosen_path(&[0, 65, 0]), None);
    assert_eq!(chosen_path(&[]), None);
    assert_eq!(
        chosen_path(&units(r"D:\Công cụ\mstsc.exe")),
        Some(PathBuf::from(r"D:\Công cụ\mstsc.exe")),
        "without a NUL, the whole buffer"
    );
}
