//! Synthetic keyboard input for the fallback transport (plan §10.1, matrix #39).
//!
//! The builders are pure: they turn backspace counts, UTF-16 text, and replay keys into
//! [`SyntheticKey`] records, all tagged with [`RAUMAKEY_INJECTED_TAG`] in `dwExtraInfo` so that
//! Raumakey's own hooks recognize and ignore them. [`send`] (Windows only) is the only function
//! that injects anything.
//!
//! On the receiving side, hooks read the tag from their records ([`is_raumakey_extra_info`]);
//! the TIP, whose key event sinks run while a message is retrieved, reads it with
//! [`message_extra_info`] (`GetMessageExtraInfo`, Windows only) and bypasses its own keys with
//! [`current_message_is_raumakey_injected`].
//!
//! The clock and the settings of input are read here too (Windows only): the tick of the
//! last input ([`last_input_tick`]), the system clock in milliseconds ([`tick_count`]), and
//! the user's double-click time ([`double_click_time`]), by which the controller tells a
//! double-click on its notification icon from two clicks.

use core::fmt;

use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::{ReplayKey, vk};
use raumakey_model::transaction::MAX_INSERT_UNITS;
use raumakey_model::utf16;

/// Most backspaces one builder call produces.
pub const MAX_BACKSPACES: u32 = 4096;
/// Most UTF-16 units one builder call produces (the transaction insert limit).
pub const MAX_TEXT_UNITS: usize = MAX_INSERT_UNITS;

/// `dwExtraInfo` of every event built here. The tag fits in 32 bits (D6), so it is exact on
/// every target.
const TAG: usize = RAUMAKEY_INJECTED_TAG as usize;

const KEYEVENTF_EXTENDEDKEY: u32 = 0x0001;
const KEYEVENTF_KEYUP: u32 = 0x0002;
const KEYEVENTF_UNICODE: u32 = 0x0004;

/// Set-1 make code of Backspace.
const SCAN_BACKSPACE: u16 = 0x0E;
/// Set-1 make codes of the left modifier keys (layout independent).
const SCAN_CONTROL: u16 = 0x1D;
const SCAN_SHIFT: u16 = 0x2A;
const SCAN_ALT: u16 = 0x38;
const SCAN_LWIN: u16 = 0x5B;

/// Why input could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum InputError {
    /// More events than one call may produce ([`MAX_BACKSPACES`], [`MAX_TEXT_UNITS`]).
    TooManyEvents,
    /// The text is not well-formed UTF-16 (a lone surrogate would corrupt the document).
    IllFormedText,
    /// The virtual key is not in `1..=0xFE`.
    InvalidVirtualKey,
    /// The scan code does not fit in 16 bits.
    InvalidScanCode,
}

impl fmt::Display for InputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TooManyEvents => "too many synthetic input events",
            Self::IllFormedText => "text is not well-formed UTF-16",
            Self::InvalidVirtualKey => "virtual key cannot be injected",
            Self::InvalidScanCode => "scan code does not fit in 16 bits",
        })
    }
}

impl std::error::Error for InputError {}

/// One synthetic keyboard event, independent of the Win32 `INPUT` layout.
///
/// `Debug` prints the fields, which for Unicode events include the code unit; never log these
/// records.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SyntheticKey {
    /// `wVk` (0 for Unicode events).
    pub virtual_key: u16,
    /// `wScan`: the scan code, or the UTF-16 code unit for Unicode events.
    pub scan_code: u16,
    /// `KEYEVENTF_KEYUP`.
    pub key_up: bool,
    /// `KEYEVENTF_UNICODE`.
    pub unicode: bool,
    /// `KEYEVENTF_EXTENDEDKEY`.
    pub extended: bool,
    /// `dwExtraInfo`.
    pub extra_info: usize,
}

impl SyntheticKey {
    const fn key(virtual_key: u16, scan_code: u16, extended: bool, key_up: bool) -> Self {
        Self {
            virtual_key,
            scan_code,
            key_up,
            unicode: false,
            extended,
            extra_info: TAG,
        }
    }

    const fn unicode_unit(unit: u16, key_up: bool) -> Self {
        Self {
            virtual_key: 0,
            scan_code: unit,
            key_up,
            unicode: true,
            extended: false,
            extra_info: TAG,
        }
    }

    /// The `KEYBDINPUT.dwFlags` bits.
    pub const fn flags(&self) -> u32 {
        let mut flags = 0;
        if self.extended {
            flags |= KEYEVENTF_EXTENDEDKEY;
        }
        if self.key_up {
            flags |= KEYEVENTF_KEYUP;
        }
        if self.unicode {
            flags |= KEYEVENTF_UNICODE;
        }
        flags
    }

    /// The Win32 `INPUT` record (keyboard member, time 0 = system time stamp).
    #[cfg(windows)]
    pub fn to_input(&self) -> windows::Win32::UI::Input::KeyboardAndMouse::INPUT {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, VIRTUAL_KEY,
        };
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(self.virtual_key),
                    wScan: self.scan_code,
                    dwFlags: KEYBD_EVENT_FLAGS(self.flags()),
                    time: 0,
                    dwExtraInfo: self.extra_info,
                },
            },
        }
    }
}

/// `count` Backspace presses (down/up pairs).
pub fn keyboard_inputs_for_backspaces(count: u32) -> Result<Vec<SyntheticKey>, InputError> {
    if count > MAX_BACKSPACES {
        return Err(InputError::TooManyEvents);
    }
    let backspace = vk::BACK as u16;
    Ok((0..count)
        .flat_map(|_| {
            [
                SyntheticKey::key(backspace, SCAN_BACKSPACE, false, false),
                SyntheticKey::key(backspace, SCAN_BACKSPACE, false, true),
            ]
        })
        .collect())
}

/// `KEYEVENTF_UNICODE` down/up pairs, one per UTF-16 code unit (a surrogate pair is sent as
/// its two units). Ill-formed text is refused.
pub fn unicode_text_inputs(units: &[u16]) -> Result<Vec<SyntheticKey>, InputError> {
    if units.len() > MAX_TEXT_UNITS {
        return Err(InputError::TooManyEvents);
    }
    if !utf16::is_well_formed(units) {
        return Err(InputError::IllFormedText);
    }
    Ok(units
        .iter()
        .flat_map(|&unit| {
            [
                SyntheticKey::unicode_unit(unit, false),
                SyntheticKey::unicode_unit(unit, true),
            ]
        })
        .collect())
}

/// Replays `key`: modifier downs (Ctrl, Shift, Alt, Win), key down, key up, then modifier ups in
/// reverse order. A missing scan code is sent as 0.
pub fn replay_inputs(key: &ReplayKey) -> Result<Vec<SyntheticKey>, InputError> {
    let virtual_key = u16::try_from(key.virtual_key)
        .ok()
        .filter(|code| (1..=0xFE).contains(code))
        .ok_or(InputError::InvalidVirtualKey)?;
    let scan_code =
        u16::try_from(key.scan_code.unwrap_or(0)).map_err(|_| InputError::InvalidScanCode)?;
    let modifiers = key.modifiers;
    let held: Vec<(u16, u16, bool)> = [
        (modifiers.ctrl, vk::CONTROL as u16, SCAN_CONTROL, false),
        (modifiers.shift, vk::SHIFT as u16, SCAN_SHIFT, false),
        (modifiers.alt, vk::MENU as u16, SCAN_ALT, false),
        (modifiers.win, vk::LWIN as u16, SCAN_LWIN, true),
    ]
    .into_iter()
    .filter(|(on, ..)| *on)
    .map(|(_, code, scan, extended)| (code, scan, extended))
    .collect();
    let extended = is_extended_key(key.virtual_key);
    let mut events = Vec::with_capacity(held.len() * 2 + 2);
    events.extend(
        held.iter()
            .map(|&(code, scan, ext)| SyntheticKey::key(code, scan, ext, false)),
    );
    events.push(SyntheticKey::key(virtual_key, scan_code, extended, false));
    events.push(SyntheticKey::key(virtual_key, scan_code, extended, true));
    events.extend(
        held.iter()
            .rev()
            .map(|&(code, scan, ext)| SyntheticKey::key(code, scan, ext, true)),
    );
    Ok(events)
}

/// The key-down alone of a key that a low-level keyboard hook blocked and sends again after
/// an edit (`SendInput` appends to the input stream, so the edit and this key-down reach the
/// application in that order). The physical key-up follows on its own, and the modifiers the
/// user holds are already down in the system's key state, so none is pressed or released
/// here: [`replay_inputs`] would release a modifier the user still holds.
///
/// `extended` is the `LLKHF_EXTENDED` flag of the blocked key (numeric-pad Enter, the right
/// Ctrl and Alt keys, the navigation block).
pub fn key_down_input(
    virtual_key: u32,
    scan_code: u32,
    extended: bool,
) -> Result<SyntheticKey, InputError> {
    let virtual_key = u16::try_from(virtual_key)
        .ok()
        .filter(|code| (1..=0xFE).contains(code))
        .ok_or(InputError::InvalidVirtualKey)?;
    let scan_code = u16::try_from(scan_code).map_err(|_| InputError::InvalidScanCode)?;
    Ok(SyntheticKey::key(virtual_key, scan_code, extended, false))
}

/// Keys whose scan code carries the `E0` prefix on every keyboard: the navigation block,
/// Print Screen, the right Ctrl/Alt, the Windows and Applications keys, numeric-pad divide,
/// Num Lock, and the browser/media/launch keys.
pub const fn is_extended_key(virtual_key: u32) -> bool {
    matches!(
        virtual_key,
        0x21..=0x28 | 0x2C..=0x2E | 0x5B..=0x5D | 0x6F | 0x90 | 0xA3 | 0xA5 | 0xA6..=0xB7
    )
}

/// True when `extra_info` is Raumakey's injection tag.
pub const fn is_raumakey_extra_info(extra_info: usize) -> bool {
    extra_info as u64 == RAUMAKEY_INJECTED_TAG
}

/// The extra information of the message the calling thread retrieved last
/// (`GetMessageExtraInfo`): for keyboard input, the `dwExtraInfo` its sender gave `SendInput`.
/// A thread that has not retrieved input yet reads 0. The bits are returned unchanged, so a
/// 32-bit value with the top bit set reads the same on i686 as on 64-bit targets.
#[cfg(windows)]
pub fn message_extra_info() -> usize {
    // SAFETY: no preconditions; reads the calling thread's message state.
    unsafe { windows::Win32::UI::WindowsAndMessaging::GetMessageExtraInfo() }.0 as usize
}

/// True when the message the calling thread is processing was injected by Raumakey: its
/// [`message_extra_info`] is [`RAUMAKEY_INJECTED_TAG`] (see [`is_raumakey_extra_info`]). The
/// TIP's key event sinks bypass such keys (matrix #39).
#[cfg(windows)]
pub fn current_message_is_raumakey_injected() -> bool {
    is_raumakey_extra_info(message_extra_info())
}

/// The tick (`GetTickCount` base, milliseconds) of the last input event of this session
/// (`GetLastInputInfo`), physical or injected, keyboard or mouse. Read-only.
#[cfg(windows)]
pub fn last_input_tick() -> crate::error::WinResult<u32> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetLastInputInfo, LASTINPUTINFO};

    let mut info = LASTINPUTINFO {
        cbSize: u32::try_from(size_of::<LASTINPUTINFO>()).unwrap_or(u32::MAX),
        dwTime: 0,
    };
    // SAFETY: `info` is a valid, size-initialized out structure.
    if unsafe { GetLastInputInfo(&mut info) }.as_bool() {
        Ok(info.dwTime)
    } else {
        Err(crate::error::WinError::last("GetLastInputInfo"))
    }
}

/// Milliseconds since the system started (`GetTickCount`), the base of [`last_input_tick`]
/// and of the time stamps of hook events. It wraps around after 49.7 days, so two ticks are
/// compared by their difference (`later.wrapping_sub(earlier)`).
#[cfg(windows)]
pub fn tick_count() -> u32 {
    // SAFETY: no preconditions; reads the system clock.
    unsafe { windows::Win32::System::SystemInformation::GetTickCount() }
}

/// The double-click time of the user's mouse settings (`GetDoubleClickTime`), in
/// milliseconds: the longest time between the clicks of a double-click. Windows keeps it
/// between 1 and 5000 (500 unless the user changed it).
#[cfg(windows)]
pub fn double_click_time() -> u32 {
    // SAFETY: no preconditions; reads a setting of the session.
    unsafe { windows::Win32::UI::Input::KeyboardAndMouse::GetDoubleClickTime() }
}

/// What [`send`] injected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SendReport {
    /// Events passed in.
    pub requested: usize,
    /// Events the system accepted (`SendInput` return value).
    pub sent: usize,
}

impl SendReport {
    /// True when every event was injected.
    pub const fn is_complete(&self) -> bool {
        self.sent == self.requested
    }
}

/// Injects `keys` with `SendInput`.
///
/// Fails when nothing could be injected (for example blocked by UIPI, or the input desktop is
/// locked). A partial injection is reported through [`SendReport::is_complete`]; the caller must
/// treat the host text as unknown and reset its context.
#[cfg(windows)]
pub fn send(keys: &[SyntheticKey]) -> crate::error::WinResult<SendReport> {
    use windows::Win32::UI::Input::KeyboardAndMouse::{INPUT, SendInput};

    if keys.is_empty() {
        return Ok(SendReport {
            requested: 0,
            sent: 0,
        });
    }
    let inputs: Vec<INPUT> = keys.iter().map(SyntheticKey::to_input).collect();
    let size = i32::try_from(size_of::<INPUT>()).unwrap_or(i32::MAX);
    // SAFETY: `inputs` is a valid slice of initialized keyboard INPUT records of `size` bytes.
    let sent = unsafe { SendInput(&inputs, size) } as usize;
    if sent == 0 {
        return Err(crate::error::WinError::last("SendInput"));
    }
    Ok(SendReport {
        requested: keys.len(),
        sent,
    })
}
