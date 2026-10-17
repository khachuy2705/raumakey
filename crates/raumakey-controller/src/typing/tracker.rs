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
/// [`forget_missed_releases`](Self::forget_missed_releases) drops it, and a modifier the hook
/// never saw go down or up is corrected by [`sync_modifiers`](Self::sync_modifiers): the
/// secure desktop (Ctrl+Alt+Del, the lock screen, a UAC prompt) and windows of a higher
/// integrity level take the input away from the hook. Both read the physical state
/// (`GetAsyncKeyState`), which never shows a key-down that the hook blocked; the handler
/// reports such keys ([`settle`](Self::settle)), and the tracker never takes them for missed
/// releases.
///
/// `Debug` shows how many keys are down and the Caps Lock toggle, never which keys.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct KeyTracker {
    down: [u64; 4],
    /// Keys down whose press the hook withheld from the system: no key-down of the press
    /// reached it, so the physical state shows them up.
    withheld: [u64; 4],
    caps_lock: bool,
}

/// The side-specific modifier codes that low-level hooks report.
const MODIFIERS: [u32; 8] = [
    vk::LSHIFT,
    vk::RSHIFT,
    vk::LCONTROL,
    vk::RCONTROL,
    vk::LMENU,
    vk::RMENU,
    vk::LWIN,
    vk::RWIN,
];

impl KeyTracker {
    /// No key down; Caps Lock as the desktop reports it when the hook starts.
    pub fn new(caps_lock: bool) -> Self {
        Self {
            down: [0; 4],
            withheld: [0; 4],
            caps_lock,
        }
    }

    /// Records one key transition of the hook and says what it was. A key-down counts as
    /// delivered to the system until [`settle`](Self::settle) says otherwise.
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

    /// Reports what became of the key-down of `virtual_key` just observed: whether it
    /// `reached` the system (the hook let it through, or sent it again). The first key-down of
    /// a press that did not reach it leaves the key withheld until a later key-down of the
    /// same press reaches it, or the key goes up.
    pub fn settle(&mut self, virtual_key: u32, repeat: bool, reached: bool) {
        if !self.is_down(virtual_key) {
            return;
        }
        if reached {
            set_bit(&mut self.withheld, virtual_key, false);
        } else if !repeat {
            set_bit(&mut self.withheld, virtual_key, true);
        }
    }

    /// Whether `virtual_key` is down and its press was withheld from the system.
    pub fn is_withheld(&self, virtual_key: u32) -> bool {
        self.is_down(virtual_key) && get_bit(&self.withheld, virtual_key)
    }

    /// Whether `virtual_key` is down as far as the tracker knows.
    pub fn is_down(&self, virtual_key: u32) -> bool {
        get_bit(&self.down, virtual_key)
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

    /// Drops every key held down here, other than `except` (the key being handled) and the
    /// keys withheld from the system, for which `still_down` (the physical state,
    /// `GetAsyncKeyState`) says it is up: the hook missed its release. Returns whether any key
    /// was dropped; the withheld keys are dropped too then, since their releases may have
    /// gone missing with the others.
    ///
    /// The physical state only ever clears keys here: inside a low-level hook it does not yet
    /// include the key being handled, and it never shows a key-down that a hook blocked.
    pub fn forget_missed_releases(
        &mut self,
        except: u32,
        mut still_down: impl FnMut(u32) -> bool,
    ) -> bool {
        let mut forgot = false;
        for virtual_key in 0..256_u32 {
            if virtual_key != except
                && self.is_down(virtual_key)
                && !self.is_withheld(virtual_key)
                && !still_down(virtual_key)
            {
                self.set(virtual_key, false);
                forgot = true;
            }
        }
        if forgot {
            for virtual_key in 0..256_u32 {
                if virtual_key != except && self.is_withheld(virtual_key) {
                    self.set(virtual_key, false);
                }
            }
        }
        forgot
    }

    /// Sets the eight side-specific modifier codes from `physically_down` (the physical
    /// state, `GetAsyncKeyState`), both ways: a modifier the hook never saw go down counts as
    /// held, one whose release it missed as released. The generic codes are dropped (the
    /// side-specific ones stand for them). While right Alt is down on an AltGr layout, left
    /// Ctrl is left as it is: Windows holds a synthetic left Ctrl down with AltGr. Returns
    /// whether a modifier held here turned out released. Call it before a key-down that is
    /// not a modifier: the physical state of the other keys is current then.
    pub fn sync_modifiers(
        &mut self,
        altgr_layout: bool,
        mut physically_down: impl FnMut(u32) -> bool,
    ) -> bool {
        let altgr_held = altgr_layout && physically_down(vk::RMENU);
        let mut released = false;
        for modifier in MODIFIERS {
            if modifier == vk::LCONTROL && altgr_held {
                continue;
            }
            let down = physically_down(modifier);
            if self.is_down(modifier) && !down {
                released = true;
            }
            self.set(modifier, down);
        }
        for generic in [vk::SHIFT, vk::CONTROL, vk::MENU] {
            self.set(generic, false);
        }
        released
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

    /// Marks `virtual_key` down or up; a key that goes up, or down again, is no longer
    /// withheld.
    fn set(&mut self, virtual_key: u32, down: bool) {
        if down != self.is_down(virtual_key) {
            set_bit(&mut self.withheld, virtual_key, false);
        }
        set_bit(&mut self.down, virtual_key, down);
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

fn get_bit(set: &[u64; 4], virtual_key: u32) -> bool {
    bit(virtual_key).is_some_and(|(word, mask)| set[word] & mask != 0)
}

fn set_bit(set: &mut [u64; 4], virtual_key: u32, on: bool) {
    if let Some((word, mask)) = bit(virtual_key) {
        if on {
            set[word] |= mask;
        } else {
            set[word] &= !mask;
        }
    }
}
