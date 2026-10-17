//! Keyboard state, key translation, and layout names.
//!
//! [`KeyboardSnapshot::key_state`] and [`klid_from_hkl`] are pure. The Windows functions only
//! read: [`translate_key`] calls `ToUnicodeEx` with flag bit 2 so the kernel keyboard state
//! (dead keys) of the thread is not changed (Windows 10 1607 and later; older systems ignore
//! the flag).

/// `0x80`: the key is down in a key state array.
const DOWN: u8 = 0x80;
/// `0x01`: the key is toggled on in a key state array.
const TOGGLED: u8 = 0x01;

const VK_SHIFT: usize = 0x10;
const VK_CONTROL: usize = 0x11;
const VK_MENU: usize = 0x12;
const VK_CAPITAL: usize = 0x14;
const VK_LWIN: usize = 0x5B;
const VK_LSHIFT: usize = 0xA0;
const VK_LCONTROL: usize = 0xA2;
const VK_LMENU: usize = 0xA4;
const VK_RMENU: usize = 0xA5;

/// Modifier and toggle state used to translate a key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyboardSnapshot {
    /// Shift held.
    pub shift: bool,
    /// Ctrl held.
    pub ctrl: bool,
    /// Left Alt held.
    pub alt: bool,
    /// Right Alt held. On AltGr layouts (see [`is_altgr_layout`]) this is AltGr and is
    /// translated as Ctrl+Alt; on other layouts callers treat it as Alt.
    pub alt_gr: bool,
    /// A Windows key held.
    pub win: bool,
    /// Caps Lock toggled on.
    pub caps_lock: bool,
}

impl KeyboardSnapshot {
    /// A 256-byte key state array for `ToUnicodeEx` with the generic and left/right modifier
    /// entries set.
    pub fn key_state(&self) -> [u8; 256] {
        let mut state = [0_u8; 256];
        let mut set = |index: usize, value: u8| {
            if let Some(slot) = state.get_mut(index) {
                *slot = value;
            }
        };
        if self.shift {
            set(VK_SHIFT, DOWN);
            set(VK_LSHIFT, DOWN);
        }
        if self.ctrl || self.alt_gr {
            set(VK_CONTROL, DOWN);
            set(VK_LCONTROL, DOWN);
        }
        if self.alt {
            set(VK_MENU, DOWN);
            set(VK_LMENU, DOWN);
        }
        if self.alt_gr {
            set(VK_MENU, DOWN);
            set(VK_RMENU, DOWN);
        }
        if self.win {
            set(VK_LWIN, DOWN);
        }
        if self.caps_lock {
            set(VK_CAPITAL, TOGGLED);
        }
        state
    }

    /// Reads the physical modifier state (`GetAsyncKeyState`, which is current even on a
    /// low-level hook thread) and the Caps Lock toggle (`GetKeyState`). Read-only.
    ///
    /// Inside a low-level hook the state does not yet include the key being processed.
    #[cfg(windows)]
    pub fn capture() -> Self {
        use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};

        let pressed = |vk: usize| {
            // SAFETY: no preconditions; the high bit reports the key as down.
            let state = unsafe { GetAsyncKeyState(vk as i32) };
            state < 0
        };
        // SAFETY: no preconditions; the low bit reports the toggle state.
        let caps = unsafe { GetKeyState(VK_CAPITAL as i32) } & 1 != 0;
        Self {
            shift: pressed(VK_SHIFT),
            ctrl: pressed(VK_CONTROL),
            alt: pressed(VK_LMENU),
            alt_gr: pressed(VK_RMENU),
            win: pressed(VK_LWIN) || pressed(0x5C),
            caps_lock: caps,
        }
    }
}

/// How the KLID (`HKLM\SYSTEM\CurrentControlSet\Control\Keyboard Layouts` key name) of a
/// keyboard layout handle is found.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KlidSource {
    /// The KLID follows from the handle (standard layouts and IMEs).
    Direct(String),
    /// A layout variant: the KLID is the layout whose `Layout Id` value equals `layout_id`.
    LayoutId {
        /// `Layout Id` (low 12 bits of the device part of the handle).
        layout_id: u16,
        /// Input language of the handle.
        language: u16,
    },
}

/// Derives the KLID of a keyboard layout handle (`HKL`). Only the low 32 bits are meaningful
/// (64-bit handles are sign-extended).
pub fn klid_from_hkl(hkl: usize) -> KlidSource {
    let value = hkl as u32;
    let language = (value & 0xFFFF) as u16;
    let device = (value >> 16) as u16;
    match device & 0xF000 {
        0xE000 => KlidSource::Direct(format!("{value:08X}")),
        0xF000 => KlidSource::LayoutId {
            layout_id: device & 0x0FFF,
            language,
        },
        _ if device == 0 => KlidSource::Direct(format!("0000{language:04X}")),
        _ => KlidSource::Direct(format!("0000{device:04X}")),
    }
}

/// Text a key produces.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Translation {
    /// UTF-16 units produced (empty for keys without text).
    pub units: Vec<u16>,
    /// The key is a dead key; `units` holds its spacing form.
    pub dead_key: bool,
}

#[cfg(windows)]
pub use win::{is_altgr_layout, layout_for_thread, layout_name, translate_key};

#[cfg(windows)]
mod win {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        GetKeyboardLayout, GetKeyboardLayoutNameW, HKL, ToUnicodeEx, VkKeyScanExW,
    };

    use super::{KeyboardSnapshot, KlidSource, Translation, klid_from_hkl};
    use crate::registry::{RegKey, RegRoot, RegView};

    /// `ToUnicodeEx` flag: do not change the keyboard state.
    const NO_STATE_CHANGE: u32 = 0x4;
    /// Registry key listing installed layouts.
    const LAYOUTS_KEY: &str = "SYSTEM\\CurrentControlSet\\Control\\Keyboard Layouts";

    fn hkl(value: isize) -> HKL {
        HKL(value as *mut core::ffi::c_void)
    }

    /// Text produced by `vk`/`scan` under `snapshot` on layout `layout` (an `HKL` value),
    /// without changing any keyboard state.
    pub fn translate_key(
        vk: u32,
        scan: u32,
        snapshot: &KeyboardSnapshot,
        layout: isize,
    ) -> Translation {
        let state = snapshot.key_state();
        let mut buffer = [0_u16; 16];
        // SAFETY: the state array and output buffer are valid for the call; flag bit 2 keeps
        // the kernel keyboard state unchanged.
        let count = unsafe {
            ToUnicodeEx(
                vk,
                scan,
                &state,
                &mut buffer,
                NO_STATE_CHANGE,
                Some(hkl(layout)),
            )
        };
        let (len, dead_key) = match count {
            count if count < 0 => (1, true),
            count => (count as usize, false),
        };
        Translation {
            units: buffer.get(..len.min(buffer.len())).unwrap_or(&[]).to_vec(),
            dead_key,
        }
    }

    /// Keyboard layout (`HKL` value) of thread `thread_id` (0 = the calling thread).
    pub fn layout_for_thread(thread_id: u32) -> isize {
        // SAFETY: no preconditions; an unknown thread yields a null layout.
        unsafe { GetKeyboardLayout(thread_id) }.0 as isize
    }

    /// KLID of layout `layout` (8 uppercase hex digits), or `None` when it cannot be derived.
    /// Layout variants are resolved through the read-only `Keyboard Layouts` registry key.
    pub fn layout_name(layout: isize) -> Option<String> {
        if layout == 0 {
            return None;
        }
        if layout == layout_for_thread(0) {
            let mut name = [0_u16; 9];
            // SAFETY: the buffer has the KL_NAMELENGTH units the function requires.
            if unsafe { GetKeyboardLayoutNameW(&mut name) }.is_ok() {
                return Some(crate::strings::from_wide_until_nul(&name).to_ascii_uppercase());
            }
        }
        match klid_from_hkl(layout as usize) {
            KlidSource::Direct(klid) => Some(klid),
            KlidSource::LayoutId { layout_id, .. } => find_layout_by_id(layout_id),
        }
    }

    fn find_layout_by_id(layout_id: u16) -> Option<String> {
        let layouts =
            RegKey::open_read(RegRoot::LocalMachine, RegView::Default, LAYOUTS_KEY).ok()??;
        layouts.enum_subkeys().ok()?.into_iter().find_map(|klid| {
            let id = layouts
                .open_subkey(&klid)
                .ok()??
                .get_string("Layout Id")
                .ok()??;
            (u16::from_str_radix(id.trim(), 16).ok()? == layout_id)
                .then(|| klid.to_ascii_uppercase())
        })
    }

    /// Heuristic: true when the layout needs Ctrl+Alt (AltGr) to type at least one printable
    /// ASCII, Latin-1, or Latin Extended-A character, or the euro sign. Such layouts produce
    /// text with Ctrl+Alt, so Ctrl+Alt+key there is not a shortcut (matrix #22).
    pub fn is_altgr_layout(layout: isize) -> bool {
        let candidates = (0x21_u16..=0x7E)
            .chain(0xA0..=0x17F)
            .chain(core::iter::once(0x20AC));
        for unit in candidates {
            // SAFETY: no memory preconditions; an unknown layout yields -1.
            let scan = unsafe { VkKeyScanExW(unit, hkl(layout)) };
            if scan == -1 {
                continue;
            }
            let shift_state = (scan as u16 >> 8) & 0xFF;
            if shift_state & 0x06 == 0x06 {
                return true;
            }
        }
        false
    }
}
