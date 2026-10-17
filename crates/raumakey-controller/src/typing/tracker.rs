//! The keys the user holds, as the low-level keyboard hook sees them.

use core::fmt;

use raumakey_model::input::{Modifiers, vk};
use raumakey_windows::hooks::KeyboardEvent;
use raumakey_windows::keyboard::KeyboardSnapshot;

/// The bit of a low-level hook scan code that marks the synthetic left Ctrl that Windows
/// sends ahead of right Alt on an AltGr layout (scan code `0x21D` instead of `0x1D`).
const SYNTHETIC_SCAN: u32 = 0x200;

/// What the tracker made of one key transition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Transition {
    /// A key-down; `repeat` when the key was already down (an auto-repeat: a low-level hook
    /// has no repeat bit).
    Down {
        /// The key was down before this key-down.
        repeat: bool,
    },
    /// A key-up.
    Up,
    /// The synthetic left Ctrl that Windows sends with AltGr. It is no key of the user: the
    /// tracker ignores it, and so must the session core (the `raumakey_policy::hotkey`
    /// adapter contract).
    SyntheticCtrl,
}

/// Modifier state, keys held down, and the Caps Lock toggle, from the key events of the hook.
///
/// The thread key state (`GetKeyState`) of a hook thread is stale, because the thread
/// receives no keyboard messages, so the modifiers of a key and the key state that
/// `ToUnicodeEx` translates it with come from here. Feed it every key event of the hook
/// except those Raumakey injected (matrix #39), in order:
///
/// - Every key-down marks its key down; a key-down of a key already down is a repeat.
/// - Caps Lock starts from the desktop's toggle ([`new`](Self::new)) and flips on each key-down
///   of Caps Lock that is not a repeat.
/// - Low-level hooks report the left and right modifier codes (`LSHIFT` … `RMENU`, `LWIN`,
///   `RWIN`); the generic codes, which other programs may inject, count as left keys.
/// - The synthetic left Ctrl of AltGr (scan code `0x21D`) is ignored.
///
/// A key whose release the hook did not see stays down here until
/// [`forget_missed_releases`](Self::forget_missed_releases) drops it: the secure desktop
/// (Ctrl+Alt+Del, the lock screen, a UAC prompt) and windows of a higher integrity level take
/// the input away from the hook.
///
/// `Debug` shows how many keys are down and the Caps Lock toggle, never which keys.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct KeyTracker {
    down: [u64; 4],
    caps_lock: bool,
}

impl KeyTracker {
    /// No key down; Caps Lock as the desktop reports it when the hook starts.
    pub fn new(caps_lock: bool) -> Self {
        Self {
            down: [0; 4],
            caps_lock,
        }
    }

    /// Records one key transition of the hook and says what it was.
    pub fn observe(&mut self, event: &KeyboardEvent) -> Transition {
        if event.vk == vk::LCONTROL && event.scan & SYNTHETIC_SCAN != 0 {
            return Transition::SyntheticCtrl;
        }
        if event.key_up {
            self.set(event.vk, false);
            return Transition::Up;
        }
        let repeat = self.is_down(event.vk);
        self.set(event.vk, true);
        if event.vk == vk::CAPITAL && !repeat {
            self.caps_lock = !self.caps_lock;
        }
        Transition::Down { repeat }
    }

    /// Whether `virtual_key` is down as far as the tracker knows.
    pub fn is_down(&self, virtual_key: u32) -> bool {
        bit(virtual_key).is_some_and(|(word, mask)| self.down[word] & mask != 0)
    }

    /// The Caps Lock toggle.
    pub fn caps_lock(&self) -> bool {
        self.caps_lock
    }

    /// Sets the Caps Lock toggle, read again from the desktop after the hook may have missed
    /// Caps Lock presses.
    pub fn set_caps_lock(&mut self, caps_lock: bool) {
        self.caps_lock = caps_lock;
    }

    /// Drops every key held down here, other than `except` (the key being handled), for
    /// which `still_down` (the physical state, `GetAsyncKeyState`) says it is up: the hook
    /// missed its release. Returns whether any key was dropped.
    ///
    /// The physical state only ever clears keys here: inside a low-level hook it does not yet
    /// include the key being handled, and it never shows a key-down that a hook blocked, which
    /// is why a held key that Raumakey replaced may be dropped too (only its repeat flag
    /// depends on it, and only the last key pressed auto-repeats).
    pub fn forget_missed_releases(
        &mut self,
        except: u32,
        mut still_down: impl FnMut(u32) -> bool,
    ) -> bool {
        let mut forgot = false;
        for virtual_key in 0..256_u32 {
            if virtual_key != except && self.is_down(virtual_key) && !still_down(virtual_key) {
                self.set(virtual_key, false);
                forgot = true;
            }
        }
        forgot
    }

    /// The modifiers of a key for the session core. On an AltGr layout right Alt is AltGr:
    /// the key then types a character and is no shortcut (matrix #22).
    pub fn modifiers(&self, altgr_layout: bool) -> Modifiers {
        let keys = self.keys();
        Modifiers {
            shift: keys.shift,
            caps_lock: self.caps_lock,
            ctrl: keys.ctrl,
            alt: keys.left_alt || keys.right_alt,
            alt_gr: altgr_layout && keys.right_alt,
            win: keys.win,
        }
    }

    /// The state `ToUnicodeEx` translates a key with (`raumakey_windows::keyboard`): right
    /// Alt is AltGr on an AltGr layout and Alt otherwise.
    pub fn snapshot(&self, altgr_layout: bool) -> KeyboardSnapshot {
        let keys = self.keys();
        KeyboardSnapshot {
            shift: keys.shift,
            ctrl: keys.ctrl,
            alt: keys.left_alt || (keys.right_alt && !altgr_layout),
            alt_gr: keys.right_alt && altgr_layout,
            win: keys.win,
            caps_lock: self.caps_lock,
        }
    }

    fn keys(&self) -> HeldModifiers {
        let any = |keys: &[u32]| keys.iter().any(|&key| self.is_down(key));
        HeldModifiers {
            shift: any(&[vk::LSHIFT, vk::RSHIFT, vk::SHIFT]),
            ctrl: any(&[vk::LCONTROL, vk::RCONTROL, vk::CONTROL]),
            left_alt: any(&[vk::LMENU, vk::MENU]),
            right_alt: self.is_down(vk::RMENU),
            win: any(&[vk::LWIN, vk::RWIN]),
        }
    }

    fn set(&mut self, virtual_key: u32, down: bool) {
        if let Some((word, mask)) = bit(virtual_key) {
            if down {
                self.down[word] |= mask;
            } else {
                self.down[word] &= !mask;
            }
        }
    }

    fn held(&self) -> u32 {
        self.down.iter().map(|word| word.count_ones()).sum()
    }
}

impl fmt::Debug for KeyTracker {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyTracker")
            .field("keys_down", &self.held())
            .field("caps_lock", &self.caps_lock)
            .finish()
    }
}

/// The modifier keys held, by side.
struct HeldModifiers {
    shift: bool,
    ctrl: bool,
    left_alt: bool,
    right_alt: bool,
    win: bool,
}

/// The word and mask of `virtual_key` in the key set; `None` outside `0..256`.
fn bit(virtual_key: u32) -> Option<(usize, u64)> {
    let index = usize::try_from(virtual_key)
        .ok()
        .filter(|index| *index < 256)?;
    Some((index / 64, 1 << (index % 64)))
}
