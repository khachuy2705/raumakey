//! The Open dialog of the common dialog box library, to choose one existing file (the
//! exclusions dialog of the controller lets the user pick a program with it, D48).
//!
//! What the dialog is asked is pure: [`OPEN_FLAGS`] (an Explorer-style dialog for one file
//! that exists, in a folder that exists, without the read-only box, and which leaves the
//! process's current folder as it was), the list of file types ([`FileFilter`],
//! [`filter_units`]), and how its answer is read ([`chosen_path`], from a buffer of
//! [`PATH_CAPACITY`] units). On Windows, [`open_file`] shows it (`GetOpenFileNameW`) owned by
//! a window of the calling thread and returns once the user chose a file or cancelled; its
//! modal loop runs on that thread meanwhile. A dialog box of [`crate::dialog_box`] shows it
//! through `Dialog::choose_file`, which also closes it when the dialog is dismissed.
//!
//! The path chosen is user content: nothing here logs it, and `Debug` of a [`FileFilter`]
//! shows Raumakey's own labels only.

use std::path::PathBuf;

use crate::error::{E_INVALIDARG, WinError, WinResult};

/// `OFN_*` flags of `OPENFILENAMEW`.
pub mod flags {
    /// `OFN_HIDEREADONLY`: no "Open as read-only" box.
    pub const OFN_HIDEREADONLY: u32 = 0x0000_0004;
    /// `OFN_NOCHANGEDIR`: the process's current folder is put back after the dialog.
    pub const OFN_NOCHANGEDIR: u32 = 0x0000_0008;
    /// `OFN_PATHMUSTEXIST`: only paths in folders that exist.
    pub const OFN_PATHMUSTEXIST: u32 = 0x0000_0800;
    /// `OFN_FILEMUSTEXIST`: only files that exist.
    pub const OFN_FILEMUSTEXIST: u32 = 0x0000_1000;
    /// `OFN_EXPLORER`: the Explorer-style dialog.
    pub const OFN_EXPLORER: u32 = 0x0008_0000;
}

/// The flags of every Open dialog that [`open_file`] shows.
pub const OPEN_FLAGS: u32 = flags::OFN_EXPLORER
    | flags::OFN_FILEMUSTEXIST
    | flags::OFN_PATHMUSTEXIST
    | flags::OFN_HIDEREADONLY
    | flags::OFN_NOCHANGEDIR;

/// The UTF-16 units of the buffer that receives the path chosen, its NUL included: the
/// longest path of Windows fits.
pub const PATH_CAPACITY: usize = 32_768;

/// One entry of the dialog's list of file types: what the list shows, and the patterns of the
/// files it shows then (`*.exe`; several are separated by `;`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FileFilter<'a> {
    /// What the list shows, such as `Chương trình (*.exe)`.
    pub label: &'a str,
    /// The patterns, such as `*.exe`.
    pub patterns: &'a str,
}

/// The `lpstrFilter` of `filters`: each label and its patterns, each followed by a NUL, and
/// one more NUL after the last pair. Fails with `E_INVALIDARG` for no filter at all, and for a
/// label or patterns that are empty or hold a NUL, which would end the list where it is.
pub fn filter_units(filters: &[FileFilter<'_>]) -> WinResult<Vec<u16>> {
    if filters.is_empty() {
        return Err(WinError::new(E_INVALIDARG, "file dialog filter"));
    }
    let mut units = Vec::new();
    for filter in filters {
        for text in [filter.label, filter.patterns] {
            if text.is_empty() || text.contains('\0') {
                return Err(WinError::new(E_INVALIDARG, "file dialog filter"));
            }
            units.extend(text.encode_utf16());
            units.push(0);
        }
    }
    units.push(0);
    Ok(units)
}

/// The path that the dialog wrote into `buffer`: its units up to the first NUL (the whole
/// buffer without one); `None` when that is empty.
pub fn chosen_path(buffer: &[u16]) -> Option<PathBuf> {
    let end = buffer
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(buffer.len());
    let units = buffer.get(..end).filter(|units| !units.is_empty())?;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt as _;
        Some(PathBuf::from(std::ffi::OsString::from_wide(units)))
    }
    #[cfg(not(windows))]
    {
        Some(PathBuf::from(String::from_utf16_lossy(units)))
    }
}

#[cfg(windows)]
pub use win::open_file;

#[cfg(windows)]
mod win {
    use std::path::PathBuf;

    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::Controls::Dialogs::{
        CommDlgExtendedError, GetOpenFileNameW, OPEN_FILENAME_FLAGS, OPENFILENAMEW,
    };
    use windows::core::{PCWSTR, PWSTR};

    use super::{FileFilter, OPEN_FLAGS, PATH_CAPACITY, chosen_path, filter_units};
    use crate::error::{E_FAIL, WinError, WinResult};

    /// Shows the Open dialog for one existing file of the types `filters` ([`OPEN_FLAGS`]),
    /// owned by `owner` (a window of the calling thread, which the dialog disables while it is
    /// open; `None` for none), and returns the path the user chose, or `None` when the user
    /// cancelled. Its modal loop runs on the calling thread until then. Fails for a filter
    /// that [`filter_units`] refuses, and when the dialog could not be shown: the error's
    /// code holds `CommDlgExtendedError` in an `HRESULT` of `FACILITY_ITF` (`0x8004xxxx`).
    pub fn open_file(
        owner: Option<HWND>,
        filters: &[FileFilter<'_>],
    ) -> WinResult<Option<PathBuf>> {
        let filter = filter_units(filters)?;
        let mut path = vec![0_u16; PATH_CAPACITY];
        let mut dialog = OPENFILENAMEW {
            lStructSize: size_of::<OPENFILENAMEW>() as u32,
            hwndOwner: owner.unwrap_or_default(),
            lpstrFilter: PCWSTR(filter.as_ptr()),
            nFilterIndex: 1,
            lpstrFile: PWSTR(path.as_mut_ptr()),
            nMaxFile: PATH_CAPACITY as u32,
            Flags: OPEN_FILENAME_FLAGS(OPEN_FLAGS),
            ..Default::default()
        };
        // SAFETY: the structure is fully initialized with its size; the filter (double-NUL
        // terminated) and the path buffer of `nMaxFile` units outlive the call, which writes
        // only into the path buffer and the structure.
        let chosen = unsafe { GetOpenFileNameW(&mut dialog) }.as_bool();
        if chosen {
            return Ok(chosen_path(&path));
        }
        // SAFETY: no preconditions; reads the error of this thread's last common dialog.
        let error = unsafe { CommDlgExtendedError() }.0;
        if error == 0 {
            // The user cancelled or closed the dialog.
            return Ok(None);
        }
        Err(WinError::new(dialog_error(error), "GetOpenFileNameW"))
    }

    /// The `HRESULT` of the common dialog error `error` (`CDERR_*`, `FNERR_*`): its code in
    /// an error of `FACILITY_ITF` (`0x8004xxxx`), or `E_FAIL` for a code that does not fit.
    fn dialog_error(error: u32) -> i32 {
        match u16::try_from(error) {
            Ok(code) => (0x8004_0000_u32 | u32::from(code)) as i32,
            Err(_) => E_FAIL,
        }
    }
}
