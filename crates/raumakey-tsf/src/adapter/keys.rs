//! Key events as `ITfKeyEventSink` reports them: decoding, modifiers, text, AltGr, and the
//! keyboard layout.

use core::fmt;

use raumakey_model::input::{MAX_EVENT_TEXT_UNITS, Modifiers, vk};
use raumakey_windows::keyboard::{KeyboardSnapshot, Translation};

/// Bits of a key message's `lParam`.
const SCAN_SHIFT: u32 = 16;
const SCAN_MASK: u32 = 0xFF;
const EXTENDED_BIT: u32 = 1 << 24;
const PREVIOUS_STATE_BIT: u32 = 1 << 30;
const TRANSITION_BIT: u32 = 1 << 31;

/// A key event decoded from the `wParam` and `lParam` that TSF passes to the key event sink
/// (those of `WM_KEYDOWN` and `WM_KEYUP`).
///
/// `Debug` shows only the repeat and release flags: the key is what the user typed.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct KeyMessage {
    /// Virtual-key code: the low word of `wParam`.
    pub virtual_key: u32,
    /// Hardware scan code, without the `E0` prefix: bits 16–23 of `lParam`.
    pub scan_code: u32,
    /// The key has the `E0` prefix (bit 24): right Alt, right Ctrl, the navigation block.
    pub extended: bool,
    /// The key was down before this event (bit 30): an auto-repeat, or any key-up.
    pub repeat: bool,
    /// The key is being released (bit 31).
    pub released: bool,
}

impl KeyMessage {
    /// Decodes a key event. Only the low 32 bits of `lParam` are read, so a sign-extended and
    /// a zero-extended value decode alike.
    pub fn decode(wparam: usize, lparam: isize) -> Self {
        // Truncation keeps the low word of `wParam` and the low 32 bits of `lParam`.
        let flags = lparam as u32;
        Self {
            virtual_key: (wparam & 0xFFFF) as u32,
            scan_code: (flags >> SCAN_SHIFT) & SCAN_MASK,
            extended: flags & EXTENDED_BIT != 0,
            repeat: flags & PREVIOUS_STATE_BIT != 0,
            released: flags & TRANSITION_BIT != 0,
        }
    }
}

impl fmt::Debug for KeyMessage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyMessage")
            .field("repeat", &self.repeat)
            .field("released", &self.released)
            .finish_non_exhaustive()
    }
}

/// The modifier keys as the thread's key state (`GetKeyState`) reports them for the key
/// event being processed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ModifierKeys {
    /// A Shift key is down.
    pub shift: bool,
    /// A Ctrl key is down. While AltGr is held on an AltGr layout this includes the
    /// synthetic left Ctrl that Windows holds down with it.
    pub ctrl: bool,
    /// Left Alt is down.
    pub left_alt: bool,
    /// Right Alt is down: AltGr on an AltGr layout, Alt otherwise.
    pub right_alt: bool,
    /// A Windows key is down.
    pub win: bool,
    /// Caps Lock is toggled on.
    pub caps_lock: bool,
}

impl ModifierKeys {
    /// Reads the modifier keys: `down(vk)` says whether a key is down (the high bit of
    /// `GetKeyState`), `toggled(vk)` whether a toggle key is on (the low bit).
    pub fn read(down: impl Fn(u32) -> bool, toggled: impl Fn(u32) -> bool) -> Self {
        let right_alt = down(vk::RMENU);
        Self {
            shift: down(vk::SHIFT),
            ctrl: down(vk::CONTROL),
            // A state that reports only the generic Alt code is taken for left Alt.
            left_alt: down(vk::LMENU) || (down(vk::MENU) && !right_alt),
            right_alt,
            win: down(vk::LWIN) || down(vk::RWIN),
            caps_lock: toggled(vk::CAPITAL),
        }
    }

    /// The modifiers of the key for the engine. On an AltGr layout right Alt is AltGr: the
    /// key then types a character and is not a shortcut (matrix #22), although Ctrl and Alt
    /// are reported as held.
    pub fn modifiers(&self, altgr_layout: bool) -> Modifiers {
        Modifiers {
            shift: self.shift,
            caps_lock: self.caps_lock,
            ctrl: self.ctrl,
            alt: self.left_alt || self.right_alt,
            alt_gr: altgr_layout && self.right_alt,
            win: self.win,
        }
    }

    /// The modifier state `ToUnicodeEx` translates the key with
    /// (`raumakey_windows::keyboard::translate_key`): right Alt is AltGr on an AltGr layout
    /// and Alt otherwise.
    pub fn snapshot(&self, altgr_layout: bool) -> KeyboardSnapshot {
        KeyboardSnapshot {
            shift: self.shift,
            ctrl: self.ctrl,
            alt: self.left_alt || (self.right_alt && !altgr_layout),
            alt_gr: self.right_alt && altgr_layout,
            win: self.win,
            caps_lock: self.caps_lock,
        }
    }
}

/// The text of a key for [`KeyStroke::text`](crate::session::KeyStroke::text): what the layout
/// types. A dead key types nothing yet (the next key types the combined character), and a key
/// that would type more than one engine event holds
/// ([`MAX_EVENT_TEXT_UNITS`]) counts as a key that
/// types nothing, which ends the word.
pub fn key_text(translation: Translation) -> Vec<u16> {
    if translation.dead_key || translation.units.len() > MAX_EVENT_TEXT_UNITS {
        Vec::new()
    } else {
        translation.units
    }
}

/// Drops the synthetic left Ctrl transitions that Windows sends with AltGr on AltGr layouts
/// (the `raumakey_policy::hotkey` adapter contract), so that AltGr never looks like Ctrl+Alt.
///
/// Windows reports AltGr as a left Ctrl transition followed by the right Alt transition, at
/// the same time. When the left Ctrl key-down is processed, right Alt is therefore already
/// down in the physical key state, which a real left Ctrl press does not have. The key-up of
/// a dropped Ctrl is dropped as well, whichever release comes first. TSF calls the test
/// method and then the method itself for one transition; both get the same answer.
///
/// `Debug` hides the last key.
#[derive(Clone, Default)]
pub struct AltGrFilter {
    /// A synthetic left Ctrl is down: its key-up is dropped too.
    held: bool,
    /// The last transition and whether it was dropped.
    last: Option<(Transition, bool)>,
}

/// One key transition, as the filter remembers it.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Transition {
    virtual_key: u32,
    scan_code: u32,
    extended: bool,
    down: bool,
}

impl AltGrFilter {
    /// Whether to drop the transition `key` (a key-down when `down`): it is the synthetic left
    /// Ctrl of AltGr. `right_alt_down` reads the physical state of right Alt
    /// (`GetAsyncKeyState(VK_RMENU)`); it is called only for a left Ctrl key-down on an AltGr
    /// layout.
    pub fn drops(
        &mut self,
        key: &KeyMessage,
        down: bool,
        altgr_layout: bool,
        right_alt_down: impl FnOnce() -> bool,
    ) -> bool {
        let transition = Transition {
            virtual_key: key.virtual_key,
            scan_code: key.scan_code,
            extended: key.extended,
            down,
        };
        if let Some((last, dropped)) = self.last
            && last == transition
        {
            // The test call and the call itself of one transition.
            return dropped;
        }
        let left_ctrl = matches!(key.virtual_key, vk::CONTROL | vk::LCONTROL) && !key.extended;
        let dropped = match (left_ctrl, down) {
            (true, true) if altgr_layout && right_alt_down() => {
                self.held = true;
                true
            }
            (true, false) if self.held => {
                self.held = false;
                true
            }
            _ => false,
        };
        self.last = Some((transition, dropped));
        dropped
    }

    /// Forgets the transitions seen so far, for a focus change, when transitions may have
    /// been missed.
    pub fn reset(&mut self) {
        self.held = false;
        self.last = None;
    }
}

impl fmt::Debug for AltGrFilter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AltGrFilter")
            .field("held", &self.held)
            .finish_non_exhaustive()
    }
}

/// A keyboard layout and what derives from it.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Layout {
    /// The layout handle (`HKL`) as a number.
    pub handle: isize,
    /// Its KLID, such as `00000409`, for [`HostContext::keyboard_layout`](raumakey_model::host::HostContext::keyboard_layout).
    pub name: String,
    /// The layout has AltGr (`raumakey_windows::keyboard::is_altgr_layout`).
    pub altgr: bool,
}

/// Follows the keyboard layout of the TSF thread, which the user can switch at any time.
/// Describing a layout reads the registry and scans the layout, so it happens once per
/// change, not per key.
#[derive(Clone, Debug, Default)]
pub struct LayoutTracker {
    current: Option<Layout>,
}

impl LayoutTracker {
    /// Records that the layout `handle` is active. When it differs from the last one,
    /// `describe` gives its name and whether it has AltGr, and the new layout is returned (the
    /// COM layer then calls `ServiceCore::set_altgr_layout`); otherwise `describe` is not
    /// called and `None` is returned.
    pub fn observe(
        &mut self,
        handle: isize,
        describe: impl FnOnce(isize) -> (String, bool),
    ) -> Option<&Layout> {
        if self
            .current
            .as_ref()
            .is_some_and(|layout| layout.handle == handle)
        {
            return None;
        }
        let (name, altgr) = describe(handle);
        self.current = Some(Layout {
            handle,
            name,
            altgr,
        });
        self.current.as_ref()
    }

    /// The layout seen last.
    pub fn current(&self) -> Option<&Layout> {
        self.current.as_ref()
    }
}
