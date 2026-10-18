//! The programs running on this desktop, as the exclusions dialog of the controller lists
//! them (D48): the applications with a window, and the description of a program.
//!
//! Pure: which top-level windows count as an application's ([`is_application_window`] of the
//! [`WindowFacts`]: visible, without an owner, no tool window, and not cloaked, as the windows
//! of the taskbar), one program per executable ([`distinct_programs`]: sorted by name, without
//! the calling process), and how the description of a program is read from its version
//! resource ([`description_from`]): the `FileDescription` of the first `StringFileInfo` table
//! that holds one, looked for in each translation the resource lists ([`TRANSLATION_QUERY`],
//! [`translations`]), then in US English ([`FALLBACK_TRANSLATIONS`]), since some programs list
//! a translation that no table of theirs has ([`description_queries`], [`description_query`],
//! [`description_text`]).
//!
//! On Windows: [`running_applications`] (the programs with such a window, other than the
//! calling process; [`applications_with_windows`] with the process to leave out),
//! [`window_facts`] (of one window), [`running_processes`] (every process whose image can be
//! queried), [`process_description`] (the description of a running process's program), and
//! [`file_description`] (of a program file). They only read: the window manager's lists and
//! window attributes (`DwmGetWindowAttribute`), process image names
//! (`QueryFullProcessImageNameW`, with `PROCESS_QUERY_LIMITED_INFORMATION`), and version
//! resources (`GetFileVersionInfoW`, into a buffer of their own). A process that cannot be
//! queried (another user's, a protected one) is left out, and a program without a description
//! has none; nothing fails.
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

/// The translations whose `StringFileInfo` tables are looked at after those the version
/// resource lists, in order: US English in Unicode (`040904b0`), in Windows Latin 1
/// (`040904e4`), and without a code page (`04090000`). Programs that list another translation
/// than the one their table has (such as the language-neutral `0000`) keep their strings in one
/// of these.
pub const FALLBACK_TRANSLATIONS: [(u16, u16); 3] =
    [(0x0409, 0x04B0), (0x0409, 0x04E4), (0x0409, 0x0000)];

/// A running program: the executable of its process, and that process.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RunningProgram {
    /// The lowercase executable basename of the process.
    pub exe: ExeName,
    /// The process.
    pub process_id: u32,
}

/// What decides whether a top-level window is an application's ([`is_application_window`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WindowFacts {
    /// Its style (`GWL_STYLE`).
    pub style: u32,
    /// Its extended style (`GWL_EXSTYLE`).
    pub ex_style: u32,
    /// It has an owner (`GW_OWNER`).
    pub owned: bool,
    /// The window manager keeps it off the screen (`DWMWA_CLOAKED` is not zero): a window of
    /// the shell that waits hidden (Settings while it is not open, the frame host of apps, the
    /// text input host), of a suspended app, or on another virtual desktop.
    pub cloaked: bool,
}

/// Whether a top-level `window` is an application's window: it is visible (`WS_VISIBLE`), has
/// no owner (a window that belongs to another, such as a dialog box of an application, does
/// not count on its own), is no tool window (`WS_EX_TOOLWINDOW`), and is not cloaked (a window
/// of the shell that waits hidden is no application the user has open here).
pub const fn is_application_window(window: &WindowFacts) -> bool {
    window.style & WS_VISIBLE != 0
        && !window.owned
        && window.ex_style & WS_EX_TOOLWINDOW == 0
        && !window.cloaked
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

/// The translations of `bytes`, the value of [`TRANSLATION_QUERY`]: each language and code
/// page, in order; bytes after the last whole pair are left out.
pub fn translations(bytes: &[u8]) -> Vec<(u16, u16)> {
    let (pairs, _) = bytes.as_chunks::<4>();
    pairs
        .iter()
        .map(|&[l0, l1, c0, c1]| (u16::from_le_bytes([l0, l1]), u16::from_le_bytes([c0, c1])))
        .collect()
}

/// The `VerQueryValueW` paths of the `FileDescription`, in the order they are tried: each
/// translation that `translation` (the value of [`TRANSLATION_QUERY`]; `None` when the
/// resource has none) lists, then the [`FALLBACK_TRANSLATIONS`], each translation once.
pub fn description_queries(translation: Option<&[u8]>) -> Vec<String> {
    let mut tried: Vec<(u16, u16)> = Vec::new();
    for pair in translations(translation.unwrap_or(&[]))
        .into_iter()
        .chain(FALLBACK_TRANSLATIONS)
    {
        if !tried.contains(&pair) {
            tried.push(pair);
        }
    }
    tried
        .into_iter()
        .map(|(language, codepage)| description_query(language, codepage))
        .collect()
}

/// The description of a version resource whose translations are `translation` (see
/// [`description_queries`]): the first value, read by `value` (the UTF-16 units of the value at
/// a `VerQueryValueW` path; `None` when there is none), that holds a description
/// ([`description_text`]). `None` when no path does.
pub fn description_from(
    translation: Option<&[u8]>,
    mut value: impl FnMut(&str) -> Option<Vec<u16>>,
) -> Option<String> {
    description_queries(translation)
        .iter()
        .find_map(|query| value(query).and_then(|units| description_text(&units)))
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
    running_processes, window_facts,
};

#[cfg(windows)]
mod win {
    use core::ffi::c_void;
    use std::os::windows::ffi::OsStrExt as _;
    use std::path::Path;

    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
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
        RunningProgram, TRANSLATION_QUERY, WindowFacts, description_from, distinct_programs,
        is_application_window,
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
            if !is_application_window(&window_facts(hwnd)) {
                return None;
            }
            let process_id = window_process_id(hwnd)?;
            let exe = process_image_basename(process_id).ok()?;
            Some(RunningProgram { exe, process_id })
        });
        distinct_programs(found.collect::<Vec<_>>(), skip)
    }

    /// The facts of the top-level window `window` (a handle value) that
    /// [`super::is_application_window`] decides on. A window that is gone meanwhile reads as
    /// hidden: no style, no owner, not cloaked.
    pub fn window_facts(window: isize) -> WindowFacts {
        let window = HWND(window as *mut c_void);
        // SAFETY: reading the longs and the owner of a window has no memory preconditions; a
        // window that is gone meanwhile reads 0.
        let (style, ex_style, owner) = unsafe {
            (
                GetWindowLongW(window, GWL_STYLE) as u32,
                GetWindowLongW(window, GWL_EXSTYLE) as u32,
                GetWindow(window, GW_OWNER),
            )
        };
        let mut cloaked = 0_u32;
        // SAFETY: `cloaked` is writable for the size passed and outlives the call; a window
        // that is gone makes the call fail.
        let read = unsafe {
            DwmGetWindowAttribute(
                window,
                DWMWA_CLOAKED,
                (&raw mut cloaked).cast(),
                size_of::<u32>() as u32,
            )
        };
        WindowFacts {
            style,
            ex_style,
            owned: owner.is_ok_and(|owner| !owner.is_invalid()),
            cloaked: read.is_ok() && cloaked != 0,
        }
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

    /// The `FileDescription` of the program file at `path`, looked for in the translations of
    /// its version resource as [`super::description_from`] tries them; `None` when the file
    /// has no version resource, no description, or cannot be read. Any file may be named: the
    /// resource is read into a buffer of this call's own, which only `VerQueryValueW` reads
    /// and writes (it converts the strings of a 16-bit resource in place).
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
        let translation = query(&mut block, TRANSLATION_QUERY, 1);
        description_from(translation.as_deref(), |sub_block| {
            let bytes = query(&mut block, sub_block, 2)?;
            let (pairs, _) = bytes.as_chunks::<2>();
            Some(pairs.iter().map(|&pair| u16::from_le_bytes(pair)).collect())
        })
    }

    /// A copy of the value of `sub_block` in the version resource `block`
    /// (`VerQueryValueW`), whose length the resource gives in units of `unit` bytes (1 for
    /// binary values, 2 for strings, which count their characters). `None` when there is no
    /// such value, and when it would not lie within `block`: a length that runs past the end
    /// of the block marks a broken resource, whose value is never cut short to fit.
    fn query(block: &mut [u8], sub_block: &str, unit: usize) -> Option<Vec<u8>> {
        let sub_block: Vec<u16> = sub_block.encode_utf16().chain([0]).collect();
        let mut value: *mut c_void = core::ptr::null_mut();
        let mut length = 0_u32;
        // SAFETY: `block` holds the version resource that GetFileVersionInfoW wrote, with the
        // room it keeps for VerQueryValueW, which may write into the block (it converts the
        // strings of a 16-bit resource in place): the pointer comes from the exclusive borrow,
        // for the call only. `sub_block` is NUL-terminated; both out pointers are valid.
        let found = unsafe {
            VerQueryValueW(
                block.as_mut_ptr().cast::<c_void>(),
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
        // checked rather than trusted, and copied out before the block is lent again.
        let start = (value as usize).checked_sub(block.as_ptr() as usize)?;
        let end = start.checked_add((length as usize).checked_mul(unit)?)?;
        block.get(start..end).map(<[u8]>::to_vec)
    }
}
