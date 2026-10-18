//! The programs running on this desktop, as the exclusions dialog of the controller lists
//! them (D48): the applications with a window, and the description of a program.
//!
//! Pure: which top-level windows count as an application's ([`is_application_window`]: visible,
//! without an owner, and no tool window, as the windows of the taskbar), one program per
//! executable ([`distinct_programs`]: sorted by name, without the calling process), and how
//! the description of a program is read from its version resource: its first translation
//! ([`TRANSLATION_QUERY`], [`first_translation`]) names the `StringFileInfo` table of its
//! `FileDescription` ([`description_query`], [`description_text`]).
//!
//! On Windows: [`running_applications`] (the programs with such a window, other than the
//! calling process; [`applications_with_windows`] with the process to leave out),
//! [`running_processes`] (every process whose image can be queried), [`process_description`]
//! (the description of a running process's program), and [`file_description`] (of a program
//! file). They only read: the window manager's lists, process image names
//! (`QueryFullProcessImageNameW`, with `PROCESS_QUERY_LIMITED_INFORMATION`), and version
//! resources (`GetFileVersionInfoW`). A process that cannot be queried (another user's, a
//! protected one) is left out, and a program without a description has none; nothing fails.
//!
//! Only executable names ([`ExeName`]) and the descriptions of programs (what their publishers
//! wrote into them) are returned: never a window's title, a full image path, or a command
//! line. The work is bounded but not instant (one query per process, one version resource per
//! program asked for), so it belongs on a thread that may wait a moment, such as a dialog's
//! own.

use raumakey_model::host::ExeName;

/// `WS_VISIBLE`.
const WS_VISIBLE: u32 = 0x1000_0000;
/// `WS_EX_TOOLWINDOW`: a floating toolbar, which has no taskbar button.
const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;

/// The `VerQueryValueW` path of the translations of a version resource: pairs of language and
/// code page, little-endian words.
pub const TRANSLATION_QUERY: &str = r"\VarFileInfo\Translation";

/// A running program: the executable of its process, and that process.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RunningProgram {
    /// The lowercase executable basename of the process.
    pub exe: ExeName,
    /// The process.
    pub process_id: u32,
}

/// Whether a top-level window of `style` and `ex_style` (`GWL_STYLE`, `GWL_EXSTYLE`), with an
/// owner or not, is an application's window: it is visible (`WS_VISIBLE`), has no owner (a
/// window that belongs to another, such as a dialog box of an application, does not count on
/// its own), and is no tool window (`WS_EX_TOOLWINDOW`).
pub const fn is_application_window(style: u32, ex_style: u32, owned: bool) -> bool {
    style & WS_VISIBLE != 0 && !owned && ex_style & WS_EX_TOOLWINDOW == 0
}

/// `found` with each executable once (the first found of it), sorted by executable name, and
/// without the process `skip` (the calling one).
pub fn distinct_programs(
    found: impl IntoIterator<Item = RunningProgram>,
    skip: Option<u32>,
) -> Vec<RunningProgram> {
    let mut programs: Vec<RunningProgram> = Vec::new();
    for program in found {
        if Some(program.process_id) == skip || programs.iter().any(|known| known.exe == program.exe)
        {
            continue;
        }
        programs.push(program);
    }
    programs.sort_by(|a, b| a.exe.cmp(&b.exe));
    programs
}

/// The `VerQueryValueW` path of the `FileDescription` of the translation `language` and
/// `codepage`: `\StringFileInfo\040904b0\FileDescription` (lowercase hexadecimal).
pub fn description_query(language: u16, codepage: u16) -> String {
    format!(r"\StringFileInfo\{language:04x}{codepage:04x}\FileDescription")
}

/// The first translation of `bytes`, the value of [`TRANSLATION_QUERY`]: its language and code
/// page; `None` when there is none.
pub fn first_translation(bytes: &[u8]) -> Option<(u16, u16)> {
    match bytes {
        [l0, l1, c0, c1, ..] => Some((
            u16::from_le_bytes([*l0, *l1]),
            u16::from_le_bytes([*c0, *c1]),
        )),
        _ => None,
    }
}

/// A description as a version resource holds it: its units up to the first NUL, without the
/// spaces around them; `None` when nothing is left. Unpaired surrogates become U+FFFD.
pub fn description_text(units: &[u16]) -> Option<String> {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    let text = String::from_utf16_lossy(units.get(..end)?);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

#[cfg(windows)]
pub use win::{
    applications_with_windows, file_description, process_description, running_applications,
    running_processes,
};

#[cfg(windows)]
mod win {
    use core::ffi::c_void;
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;

    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW,
    };
    use windows::Win32::System::ProcessStatus::EnumProcesses;
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetWindow, GetWindowLongW,
    };
    use windows::core::{BOOL, PCWSTR};

    use super::{
        RunningProgram, TRANSLATION_QUERY, description_query, description_text, distinct_programs,
        first_translation, is_application_window,
    };
    use crate::foreground::{process_image_basename, process_image_units, window_process_id};
    use crate::panic::guard_bool;

    /// The most processes [`running_processes`] asks for, far more than a desktop runs.
    const MAX_PROCESSES: usize = 1 << 16;

    /// The programs with an application's window ([`super::is_application_window`]) on this
    /// desktop now, one per executable and sorted by name ([`super::distinct_programs`]),
    /// without the calling process. A window whose process cannot be queried is left out.
    pub fn running_applications() -> Vec<RunningProgram> {
        // SAFETY: no preconditions.
        let own = unsafe { GetCurrentProcessId() };
        applications_with_windows(Some(own))
    }

    /// [`running_applications`] without the process `skip` instead of the calling one (`None`:
    /// every process, the calling one too).
    pub fn applications_with_windows(skip: Option<u32>) -> Vec<RunningProgram> {
        let found = top_level_windows().into_iter().filter_map(|hwnd| {
            let window = HWND(hwnd as *mut c_void);
            // SAFETY: reading the longs and the owner of a window has no memory
            // preconditions; a window that is gone meanwhile reads 0.
            let (style, ex_style, owner) = unsafe {
                (
                    GetWindowLongW(window, GWL_STYLE) as u32,
                    GetWindowLongW(window, GWL_EXSTYLE) as u32,
                    GetWindow(window, GW_OWNER),
                )
            };
            let owned = owner.is_ok_and(|owner| !owner.is_invalid());
            if !is_application_window(style, ex_style, owned) {
                return None;
            }
            let process_id = window_process_id(hwnd)?;
            let exe = process_image_basename(process_id).ok()?;
            Some(RunningProgram { exe, process_id })
        });
        distinct_programs(found.collect::<Vec<_>>(), skip)
    }

    /// The handle values of the top-level windows of this desktop, in Z order.
    fn top_level_windows() -> Vec<isize> {
        unsafe extern "system" fn each(hwnd: HWND, lparam: LPARAM) -> BOOL {
            guard_bool(|| {
                // SAFETY: `lparam` is the vector of the enumeration below, which outlives it
                // and is used by nothing else meanwhile.
                let found = unsafe { &mut *(lparam.0 as *mut Vec<isize>) };
                found.push(hwnd.0 as isize);
                true
            })
        }
        let mut found: Vec<isize> = Vec::new();
        // SAFETY: the callback runs during the call only and gets a pointer to `found`, which
        // outlives the call.
        let _ = unsafe { EnumWindows(Some(each), LPARAM(&raw mut found as isize)) };
        found
    }

    /// Every process of the system whose image can be queried, with its executable, in the
    /// order `EnumProcesses` gives them (processes that cannot be queried are left out).
    pub fn running_processes() -> Vec<RunningProgram> {
        let mut ids = vec![0_u32; 1024];
        loop {
            let bytes = u32::try_from(ids.len() * size_of::<u32>()).unwrap_or(u32::MAX);
            let mut needed = 0_u32;
            // SAFETY: `ids` is writable for `bytes` bytes, and `needed` is a valid out pointer.
            if unsafe { EnumProcesses(ids.as_mut_ptr(), bytes, &mut needed) }.is_err() {
                return Vec::new();
            }
            let count = needed as usize / size_of::<u32>();
            // A full buffer may have been too small: ask again with a larger one.
            if count < ids.len() || ids.len() >= MAX_PROCESSES {
                ids.truncate(count.min(ids.len()));
                break;
            }
            ids = vec![0_u32; ids.len() * 4];
        }
        ids.into_iter()
            .filter(|&id| id != 0)
            .filter_map(|process_id| {
                let exe = process_image_basename(process_id).ok()?;
                Some(RunningProgram { exe, process_id })
            })
            .collect()
    }

    /// The description of the program that process `process_id` runs ([`file_description`]
    /// of its image); `None` when the process cannot be queried or its program has none. The
    /// image's path is read but never returned.
    pub fn process_description(process_id: u32) -> Option<String> {
        let mut path = process_image_units(process_id).ok()?;
        path.push(0);
        description_of(&path)
    }

    /// The `FileDescription` of the program file at `path`, in the first translation of its
    /// version resource; `None` when the file has no version resource, no description, or
    /// cannot be read.
    pub fn file_description(path: &Path) -> Option<String> {
        let path: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        description_of(&path)
    }

    /// [`file_description`] of the NUL-terminated path `path`.
    fn description_of(path: &[u16]) -> Option<String> {
        let (&0, inner) = path.split_last()? else {
            return None;
        };
        if inner.is_empty() || inner.contains(&0) {
            // No path, or a NUL inside it: no file is named so.
            return None;
        }
        let name = PCWSTR(path.as_ptr());
        // SAFETY: `name` is NUL-terminated and outlives the call.
        let size = unsafe { GetFileVersionInfoSizeW(name, None) };
        if size == 0 {
            return None;
        }
        let mut block = vec![0_u8; size as usize];
        // SAFETY: `block` is writable for `size` bytes; `name` is NUL-terminated.
        unsafe { GetFileVersionInfoW(name, None, size, block.as_mut_ptr().cast()) }.ok()?;
        let translation = query(&block, TRANSLATION_QUERY, 1)?;
        let (language, codepage) = first_translation(translation)?;
        let units = query(&block, &description_query(language, codepage), 2)?;
        let (pairs, _) = units.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect();
        description_text(&units)
    }

    /// The value of `sub_block` in the version resource `block`, as bytes within `block`
    /// (`VerQueryValueW`), whose length the resource gives in units of `unit` bytes (1 for
    /// binary values, 2 for strings, which count their characters); `None` when there is no
    /// such value or it would not lie within `block`.
    fn query<'a>(block: &'a [u8], sub_block: &str, unit: usize) -> Option<&'a [u8]> {
        let sub_block: Vec<u16> = sub_block.encode_utf16().chain([0]).collect();
        let mut value: *mut c_void = core::ptr::null_mut();
        let mut length = 0_u32;
        // SAFETY: `block` holds the version resource that GetFileVersionInfoW wrote, which
        // the call only reads; `sub_block` is NUL-terminated; both out pointers are valid.
        let found = unsafe {
            VerQueryValueW(
                block.as_ptr().cast(),
                PCWSTR(sub_block.as_ptr()),
                &mut value,
                &mut length,
            )
        }
        .as_bool();
        if !found || value.is_null() {
            return None;
        }
        // The value lies within `block` (VerQueryValueW points into the block it was given);
        // checked rather than trusted.
        let start = (value as usize).checked_sub(block.as_ptr() as usize)?;
        let end = start.checked_add((length as usize).checked_mul(unit)?)?;
        block.get(start..end)
    }
}
