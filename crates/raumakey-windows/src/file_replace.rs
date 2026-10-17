//! Atomic file replacement for the configuration stores (design §8).

use std::io;
use std::path::Path;

use raumakey_config::FileReplacer;
use windows::Win32::Storage::FileSystem::{
    MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    REPLACEFILE_IGNORE_MERGE_ERRORS, ReplaceFileW,
};
use windows::core::PCWSTR;

use crate::error::{WinError, WinResult, win32};
use crate::strings::WideString;

/// [`FileReplacer`] built on `ReplaceFileW`.
///
/// When the target exists, `ReplaceFileW` swaps the replacement in and keeps the target's
/// identity, security descriptor, and attributes. When it does not exist (first commit, or a
/// concurrent delete), the replacement is renamed with
/// `MoveFileExW(MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH)`. When `ReplaceFileW`
/// fails after removing the target but before renaming the replacement
/// (`ERROR_UNABLE_TO_MOVE_REPLACEMENT[_2]`), the rename fallback completes the swap. No backup
/// file is created. Errors carry OS codes only.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReplaceFileReplacer;

impl FileReplacer for ReplaceFileReplacer {
    fn replace(&self, replacement: &Path, target: &Path) -> io::Result<()> {
        replace_file(replacement, target).map_err(io::Error::from)
    }
}

fn replace_file(replacement: &Path, target: &Path) -> WinResult<()> {
    let replacement = WideString::from_os(replacement.as_os_str())?;
    let target = WideString::from_os(target.as_os_str())?;
    // SAFETY: both paths are NUL-terminated and outlive the call; no backup, no reserved data.
    let replaced = unsafe {
        ReplaceFileW(
            target.as_pcwstr(),
            replacement.as_pcwstr(),
            PCWSTR::null(),
            REPLACEFILE_IGNORE_MERGE_ERRORS,
            None,
            None,
        )
    };
    match replaced {
        Ok(()) => Ok(()),
        Err(error) => {
            let error = WinError::new(error.code().0, "ReplaceFileW");
            let rename = error.is_win32(win32::ERROR_FILE_NOT_FOUND) && exists(&replacement)
                || error.is_win32(win32::ERROR_UNABLE_TO_MOVE_REPLACEMENT)
                || error.is_win32(win32::ERROR_UNABLE_TO_MOVE_REPLACEMENT_2);
            if rename {
                move_file(&replacement, &target)
            } else {
                Err(error)
            }
        }
    }
}

fn move_file(from: &WideString, to: &WideString) -> WinResult<()> {
    // SAFETY: both paths are NUL-terminated and outlive the call.
    unsafe {
        MoveFileExW(
            from.as_pcwstr(),
            to.as_pcwstr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    }
    .map_err(|error| WinError::new(error.code().0, "MoveFileExW"))
}

fn exists(path: &WideString) -> bool {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStringExt;
    Path::new(&OsString::from_wide(path.as_units())).exists()
}
