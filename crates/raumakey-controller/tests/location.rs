//! `system::is_inside_folder` (pure): whether a file lies inside a folder that only
//! administrators can change, such as Program Files. The elevated logon task refuses a
//! `raumakey.exe` elsewhere, and `install` warns about TIPs elsewhere: code that runs with
//! administrator rights must not be replaceable by the account's own programs.

use std::path::Path;

use raumakey_controller::system::is_inside_folder;

const PROGRAM_FILES: &str = "C:\\Program Files";

fn inside(path: &str) -> bool {
    is_inside_folder(Path::new(path), Path::new(PROGRAM_FILES))
}

#[test]
fn files_below_the_folder_are_inside() {
    for path in [
        "C:\\Program Files\\Raumakey\\raumakey.exe",
        "C:\\Program Files\\Raumakey\\x86\\raumakey_tip.dll",
        // Windows ignores ASCII case and accepts `/`.
        "c:\\PROGRAM FILES\\raumakey\\raumakey.exe",
        "C:/Program Files/Raumakey/raumakey.exe",
    ] {
        assert!(inside(path), "{path}");
    }
    assert!(is_inside_folder(
        Path::new("C:\\Program Files\\Raumakey\\raumakey.exe"),
        Path::new("C:\\Program Files\\")
    ));
}

#[test]
fn the_folder_itself_and_other_folders_are_outside() {
    for path in [
        "C:\\Program Files",
        "C:\\Program Files\\",
        "C:\\Program Files Evil\\raumakey.exe",
        "C:\\Program Files (x86)\\Raumakey\\raumakey.exe",
        "C:\\Users\\a\\Downloads\\Raumakey\\raumakey.exe",
        "C:\\Raumakey\\raumakey.exe",
        "D:\\Program Files\\Raumakey\\raumakey.exe",
        "Program Files\\Raumakey\\raumakey.exe",
        "",
    ] {
        assert!(!inside(path), "{path}");
    }
}

/// Path forms that this lexical check does not normalize count as outside.
#[test]
fn other_path_forms_are_outside() {
    for path in [
        "\\\\?\\C:\\Program Files\\Raumakey\\raumakey.exe",
        "\\\\.\\C:\\Program Files\\Raumakey\\raumakey.exe",
        "\\\\server\\share\\Program Files\\raumakey.exe",
        "C:\\PROGRA~1\\Raumakey\\raumakey.exe",
    ] {
        assert!(!inside(path), "{path}");
    }
}

/// A `.` or `..` component, or a name that Windows reduces to one (`...`, `. `), could lead
/// out of the folder; an empty component is not a plain name either.
#[test]
fn paths_that_could_lead_out_of_the_folder_are_outside() {
    for path in [
        "C:\\Program Files\\..\\Users\\a\\raumakey.exe",
        "C:\\Program Files\\Raumakey\\..\\..\\Users\\a\\raumakey.exe",
        "C:\\Program Files\\.\\raumakey.exe",
        "C:\\Program Files\\...\\raumakey.exe",
        "C:\\Program Files\\. \\raumakey.exe",
        "C:\\Program Files\\Raumakey\\\\raumakey.exe",
        "C:\\Program Files\\Raumakey\\",
    ] {
        assert!(!inside(path), "{path}");
    }
}

/// Only an absolute folder on a drive contains anything.
#[test]
fn an_empty_or_relative_folder_contains_nothing() {
    assert!(!is_inside_folder(
        Path::new("C:\\Program Files\\a.exe"),
        Path::new("")
    ));
    assert!(!is_inside_folder(
        Path::new("Program Files\\a.exe"),
        Path::new("Program Files")
    ));
    assert!(!is_inside_folder(
        Path::new("\\Program Files\\a.exe"),
        Path::new("\\Program Files")
    ));
}
