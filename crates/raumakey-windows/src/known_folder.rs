//! Folder lookups (read-only).

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::System::SystemInformation::{
    GetSystemDirectoryW, GetSystemWindowsDirectoryW, GetSystemWow64DirectoryW,
};
use windows::Win32::UI::Shell::{
    FOLDERID_LocalAppData, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86, FOLDERID_Startup,
    KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath,
};
use windows::core::{GUID, PCWSTR};

use crate::error::{E_FAIL, ResultExt, WinError, WinResult, win32};
use crate::strings::read_pwstr;

/// `%LOCALAPPDATA%` (`FOLDERID_LocalAppData`), the parent of the fallback data root
/// `%LOCALAPPDATA%\Raumakey`, used when the folder of `raumakey.exe` is not (D40).
pub fn local_app_data() -> WinResult<PathBuf> {
    known_folder(&FOLDERID_LocalAppData, "SHGetKnownFolderPath(LocalAppData)")
}

/// The user's Startup folder (`FOLDERID_Startup`), where the startup shortcut lives. The folder
/// is not created or verified.
pub fn startup_folder() -> WinResult<PathBuf> {
    known_folder(&FOLDERID_Startup, "SHGetKnownFolderPath(Startup)")
}

/// `FOLDERID_ProgramFiles`. A 32-bit process on 64-bit Windows receives `Program Files (x86)`.
pub fn program_files() -> WinResult<PathBuf> {
    known_folder(&FOLDERID_ProgramFiles, "SHGetKnownFolderPath(ProgramFiles)")
}

/// `FOLDERID_ProgramFilesX86`: `Program Files (x86)` on 64-bit Windows, for 64-bit and 32-bit
/// processes alike; `Program Files` on 32-bit Windows.
pub fn program_files_x86() -> WinResult<PathBuf> {
    known_folder(
        &FOLDERID_ProgramFilesX86,
        "SHGetKnownFolderPath(ProgramFilesX86)",
    )
}

/// Directory of the running executable.
pub fn current_exe_dir() -> WinResult<PathBuf> {
    let exe = std::env::current_exe().map_err(|error| WinError::from_io(&error, "current_exe"))?;
    exe.parent()
        .map(PathBuf::from)
        .ok_or_else(|| WinError::new(E_FAIL, "current_exe parent"))
}

/// The Windows directory (`GetSystemWindowsDirectoryW`, shared on terminal servers).
pub fn windows_dir() -> WinResult<PathBuf> {
    // SAFETY: the closure passes a writable buffer of the given length.
    sized_path(
        |buffer| unsafe { GetSystemWindowsDirectoryW(buffer) },
        "GetSystemWindowsDirectoryW",
    )
}

/// The native system directory as seen by this process (`System32`; for a 32-bit process on
/// 64-bit Windows the file system redirects it to `SysWOW64`).
pub fn system_dir() -> WinResult<PathBuf> {
    // SAFETY: the closure passes a writable buffer of the given length.
    sized_path(
        |buffer| unsafe { GetSystemDirectoryW(buffer) },
        "GetSystemDirectoryW",
    )
}

/// `SysWOW64`, or `None` on 32-bit Windows.
pub fn syswow64_dir() -> WinResult<Option<PathBuf>> {
    let call = |buffer: Option<&mut [u16]>| {
        // SAFETY: the buffer, when given, is writable for its whole length.
        unsafe { GetSystemWow64DirectoryW(buffer) }
    };
    match sized_path(call, "GetSystemWow64DirectoryW") {
        Ok(path) => Ok(Some(path)),
        Err(error) if error.is_win32(win32::ERROR_CALL_NOT_IMPLEMENTED) => Ok(None),
        Err(error) => Err(error),
    }
}

fn known_folder(id: &GUID, context: &'static str) -> WinResult<PathBuf> {
    // SAFETY: `id` is a valid known-folder id; the returned string is freed below.
    let text = unsafe { SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY, None) }.context(context)?;
    // SAFETY: on success the string is NUL-terminated and allocated with CoTaskMemAlloc.
    let units = unsafe { read_pwstr(PCWSTR(text.0)) };
    // SAFETY: frees the string returned above exactly once.
    unsafe { CoTaskMemFree(Some(text.0.cast_const().cast())) };
    if units.is_empty() {
        return Err(WinError::new(E_FAIL, context));
    }
    Ok(PathBuf::from(OsString::from_wide(&units)))
}

/// Calls a `GetXxxDirectoryW`-style function that returns the length written (without NUL),
/// or the size needed (with NUL) when the buffer is too small, or 0 on failure.
fn sized_path(
    call: impl Fn(Option<&mut [u16]>) -> u32,
    context: &'static str,
) -> WinResult<PathBuf> {
    let mut buffer = vec![0_u16; 260];
    for _ in 0..4 {
        let len = call(Some(&mut buffer)) as usize;
        if len == 0 {
            return Err(WinError::last(context));
        }
        if len < buffer.len() {
            let units = buffer.get(..len).unwrap_or(&[]);
            return Ok(PathBuf::from(OsString::from_wide(units)));
        }
        buffer = vec![0_u16; len + 1];
    }
    Err(WinError::from_win32(
        win32::ERROR_INSUFFICIENT_BUFFER,
        context,
    ))
}
