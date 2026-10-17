//! Hotkey classification (design §11, plan §6, D5 and D24).
//!
//! [`HotkeyClassifier`] recognizes the configured toggle and hard-reset hotkeys in a stream of
//! key transitions before they reach the engine, and tells the adapter which events to withhold
//! from the application. It tracks the modifier keys itself, accepting both the generic codes
//! (`SHIFT`, `CONTROL`, `MENU`, as TSF key sinks report them) and the left/right codes (`LSHIFT`
//! … `RMENU`, `LWIN`, `RWIN`, as low-level hooks report them) from
//! [`raumakey_model::input::vk`].
//!
//! - A [`HotkeyKey::VirtualKey`] hotkey fires on the key-down of its key while exactly its
//!   modifiers are held. The match has `consume = true`, and the whole key press is withheld
//!   from the application: the auto-repeats and the key-up of that key are reported as
//!   [`KeyVerdict::Swallow`] until its key-up arrives, even when the modifiers are released
//!   first. An auto-repeat never fires a hotkey, so holding a hotkey runs its action once.
//! - A [`HotkeyKey::ModifiersOnly`] hotkey fires on the release of the last held modifier when
//!   the modifiers held since the first one went down were exactly the configured set, all of
//!   them were down at the same time, and no other key went down meanwhile. The match has
//!   `consume = false`: the release must still reach the application.
//! - Modifier key events are never withheld.
//! - On a keyboard layout with AltGr ([`HotkeyClassifier::set_altgr_layout`]), right Alt is
//!   AltGr rather than Alt: it never counts as Alt, no key hotkey matches while it is held
//!   (every key then types an AltGr character), and a hold during which it went down never
//!   fires a modifier-only hotkey. Ctrl+Alt from the left keys stays an ordinary shortcut.
//!
//! Key transitions carry no repeat flag. The classifier assumes, as with Windows typematic
//! repeat, that only the most recently pressed key repeats: a key-down is an auto-repeat when the
//! same key was the last one to go down and has not gone up since. A consumed key that goes down
//! again after another key (a modifier included) went down is therefore a new press whose key-up
//! was lost.
//!
//! Hotkeys that the text parser of [`Hotkey`] would reject (a key without modifiers, a
//! modifier-only hotkey with fewer than two modifiers, or a key code that is a modifier or
//! outside `1..=0xFE`) are ignored. When the same combination is configured for several actions,
//! `toggle_mode` wins over `hard_reset`, and the first entry wins within a list.
//!
//! # Adapter contract
//!
//! - Feed every physical key transition exactly once and in order, after dropping input that
//!   carries `RAUMAKEY_INJECTED_TAG`, through [`HotkeyClassifier::classify`].
//!   [`HotkeyClassifier::on_key`] reports only the matches, so it suits callers that never
//!   withhold events.
//! - Withhold every event whose verdict [`consumes`](KeyVerdict::consumes) from both the
//!   application and the engine: the key-down of a key hotkey, its auto-repeats, and its key-up.
//!   Withholding only the first key-down would deliver the repeats as shortcuts and a key-up
//!   whose key-down the application never saw. Hiding the key can leave the application seeing
//!   a lone Alt or Win press and release, which some hosts treat as a menu gesture; masking
//!   that release is the adapter's job.
//! - On AltGr layouts Windows reports AltGr as a synthetic left-Ctrl transition followed by
//!   right Alt. Adapters must drop that synthetic `LCONTROL` key-down and key-up before
//!   feeding the classifier, report AltGr as `RMENU` (an adapter that only sees the generic
//!   `MENU` code can tell right Alt by its extended-key flag), and keep
//!   [`HotkeyClassifier::set_altgr_layout`] in step with the active layout; otherwise AltGr
//!   chords would look like Ctrl+Alt or Alt shortcuts.
//! - Call [`HotkeyClassifier::reset`] on focus changes and whenever transitions may have been
//!   missed, and [`HotkeyClassifier::interrupt`] on mouse button presses.
//!
//! The classifier never logs and keeps no key history beyond the held modifiers, the keys whose
//! press it consumed, and the last key that went down.

use std::collections::BTreeMap;

use raumakey_model::hotkey::{Hotkey, HotkeyAction, HotkeyConfig, HotkeyKey};
use raumakey_model::input::vk;

/// A configured hotkey that matched a key event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HotkeyMatch {
    /// Action to run.
    pub action: HotkeyAction,
    /// The key event must not reach the application: `true` for key hotkeys, whose auto-repeats
    /// and key-up are then withheld as well (see [`KeyVerdict::Swallow`]), `false` for
    /// modifier-only hotkeys.
    pub consume: bool,
}

/// What an adapter does with one key event (see [`HotkeyClassifier::classify`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyVerdict {
    /// Not part of a hotkey: deliver the event as usual.
    Pass,
    /// Withhold the event from the application and the engine: an auto-repeat or the key-up of
    /// a key whose key-down a key hotkey consumed.
    Swallow,
    /// A configured hotkey matched: run its action, and withhold the event when
    /// [`HotkeyMatch::consume`] is set.
    Hotkey(HotkeyMatch),
}

impl KeyVerdict {
    /// True when the event must not reach the application or the engine:
    /// [`Swallow`](Self::Swallow), or a match whose [`consume`](HotkeyMatch::consume) is set.
    pub const fn consumes(self) -> bool {
        match self {
            Self::Pass => false,
            Self::Swallow => true,
            Self::Hotkey(found) => found.consume,
        }
    }

    /// The matched hotkey, if any.
    pub const fn hotkey(self) -> Option<HotkeyMatch> {
        match self {
            Self::Hotkey(found) => Some(found),
            Self::Pass | Self::Swallow => None,
        }
    }
}

/// Logical modifier bits.
const SHIFT: u8 = 1;
const CTRL: u8 = 1 << 1;
const ALT: u8 = 1 << 2;
const WIN: u8 = 1 << 3;

/// Physical modifier key bits: the generic code and both sides of each modifier.
const GENERIC_SHIFT: u16 = 1;
const LEFT_SHIFT: u16 = 1 << 1;
const RIGHT_SHIFT: u16 = 1 << 2;
const GENERIC_CTRL: u16 = 1 << 3;
const LEFT_CTRL: u16 = 1 << 4;
const RIGHT_CTRL: u16 = 1 << 5;
const GENERIC_ALT: u16 = 1 << 6;
const LEFT_ALT: u16 = 1 << 7;
const RIGHT_ALT: u16 = 1 << 8;
const LEFT_WIN: u16 = 1 << 9;
const RIGHT_WIN: u16 = 1 << 10;

const SHIFT_KEYS: u16 = GENERIC_SHIFT | LEFT_SHIFT | RIGHT_SHIFT;
const CTRL_KEYS: u16 = GENERIC_CTRL | LEFT_CTRL | RIGHT_CTRL;
const ALT_KEYS: u16 = GENERIC_ALT | LEFT_ALT | RIGHT_ALT;
const WIN_KEYS: u16 = LEFT_WIN | RIGHT_WIN;

/// A modifier key code.
#[derive(Clone, Copy, Debug)]
struct ModifierKey {
    /// Bit set while the key is down.
    bit: u16,
    /// Bits cleared when the key goes up. Releasing a generic code releases the whole modifier;
    /// releasing one side also releases the generic code, so mixed reporting cannot leave a
    /// modifier stuck.
    release: u16,
}

impl ModifierKey {
    fn of(code: u32) -> Option<Self> {
        let (bit, release) = match code {
            vk::SHIFT => (GENERIC_SHIFT, SHIFT_KEYS),
            vk::LSHIFT => (LEFT_SHIFT, LEFT_SHIFT | GENERIC_SHIFT),
            vk::RSHIFT => (RIGHT_SHIFT, RIGHT_SHIFT | GENERIC_SHIFT),
            vk::CONTROL => (GENERIC_CTRL, CTRL_KEYS),
            vk::LCONTROL => (LEFT_CTRL, LEFT_CTRL | GENERIC_CTRL),
            vk::RCONTROL => (RIGHT_CTRL, RIGHT_CTRL | GENERIC_CTRL),
            vk::MENU => (GENERIC_ALT, ALT_KEYS),
            vk::LMENU => (LEFT_ALT, LEFT_ALT | GENERIC_ALT),
            vk::RMENU => (RIGHT_ALT, RIGHT_ALT | GENERIC_ALT),
            vk::LWIN => (LEFT_WIN, LEFT_WIN),
            vk::RWIN => (RIGHT_WIN, RIGHT_WIN),
            _ => return None,
        };
        Some(Self { bit, release })
    }
}

/// Logical modifiers of a set of held modifier keys. On an AltGr layout right Alt is AltGr,
/// which is not Alt.
fn logical(held: u16, altgr_layout: bool) -> u8 {
    let held = if altgr_layout {
        held & !RIGHT_ALT
    } else {
        held
    };
    [
        (SHIFT_KEYS, SHIFT),
        (CTRL_KEYS, CTRL),
        (ALT_KEYS, ALT),
        (WIN_KEYS, WIN),
    ]
    .into_iter()
    .filter(|(keys, _)| held & keys != 0)
    .fold(0, |mods, (_, bit)| mods | bit)
}

/// Logical modifiers of a configured hotkey.
fn hotkey_modifiers(hotkey: &Hotkey) -> u8 {
    [
        (hotkey.shift, SHIFT),
        (hotkey.ctrl, CTRL),
        (hotkey.alt, ALT),
        (hotkey.win, WIN),
    ]
    .into_iter()
    .filter(|(on, _)| *on)
    .fold(0, |mods, (_, bit)| mods | bit)
}

/// The binding key of a hotkey the text parser would accept: a key with at least one modifier
/// and a non-modifier code in `1..=0xFE`, or at least two modifiers alone (`None` key).
fn binding_key(hotkey: &Hotkey) -> Option<(u8, Option<u32>)> {
    let mods = hotkey_modifiers(hotkey);
    match hotkey.key {
        HotkeyKey::ModifiersOnly if mods.count_ones() >= 2 => Some((mods, None)),
        HotkeyKey::VirtualKey(code)
            if mods != 0 && (1..=0xFE).contains(&code) && !vk::is_modifier(code) =>
        {
            Some((mods, Some(code)))
        }
        _ => None,
    }
}

/// A set of virtual-key codes below 256, which covers every hotkey key. Larger codes are never
/// members.
#[derive(Clone, Copy, Debug, Default)]
struct KeySet([u64; 4]);

impl KeySet {
    /// Word index and bit of `code`; the word index is out of range for codes above 255.
    fn slot(code: u32) -> Option<(usize, u64)> {
        let word = usize::try_from(code / 64).ok()?;
        Some((word, 1_u64 << (code % 64)))
    }

    fn contains(&self, code: u32) -> bool {
        Self::slot(code)
            .and_then(|(word, bit)| self.0.get(word).map(|w| w & bit != 0))
            .unwrap_or(false)
    }

    fn insert(&mut self, code: u32) {
        if let Some((word, bit)) = Self::slot(code)
            && let Some(w) = self.0.get_mut(word)
        {
            *w |= bit;
        }
    }

    /// Removes `code` and says whether it was a member.
    fn remove(&mut self, code: u32) -> bool {
        let member = self.contains(code);
        if let Some((word, bit)) = Self::slot(code)
            && let Some(w) = self.0.get_mut(word)
        {
            *w &= !bit;
        }
        member
    }
}

/// One continuous hold of modifier keys: from a modifier going down while none was held until
/// the last held modifier goes up.
#[derive(Clone, Copy, Debug, Default)]
struct Hold {
    /// Logical modifiers held at any point of the hold.
    seen: u8,
    /// All of `seen` were down at the same time at some point.
    together: bool,
    /// Another key or AltGr went down, or other input interrupted the hold.
    spoiled: bool,
}

/// Recognizes configured hotkeys in a stream of key transitions (see the module
/// documentation for the matching rules and the adapter contract).
#[derive(Clone, Debug)]
pub struct HotkeyClassifier {
    /// `(modifiers, key)` to action; a `None` key is a modifier-only hotkey.
    bindings: BTreeMap<(u8, Option<u32>), HotkeyAction>,
    /// Modifier keys currently down.
    held: u16,
    /// The current hold; meaningful only while `held != 0`.
    hold: Hold,
    /// Keys whose key-down a key hotkey consumed and whose key-up has not arrived: their
    /// auto-repeats and key-up are swallowed.
    consumed: KeySet,
    /// The key that went down last, while it stays down: the only key that can auto-repeat.
    last_down: Option<u32>,
    /// The active layout has AltGr, so right Alt is AltGr rather than Alt.
    altgr_layout: bool,
}

impl HotkeyClassifier {
    /// Creates a classifier for the hotkeys of `config`.
    pub fn new(config: &HotkeyConfig) -> Self {
        let mut bindings = BTreeMap::new();
        let configured = config
            .toggle_mode
            .iter()
            .map(|h| (h, HotkeyAction::ToggleMode))
            .chain(
                config
                    .hard_reset
                    .iter()
                    .map(|h| (h, HotkeyAction::HardReset)),
            );
        for (hotkey, action) in configured {
            if let Some(key) = binding_key(hotkey) {
                bindings.entry(key).or_insert(action);
            }
        }
        Self {
            bindings,
            held: 0,
            hold: Hold::default(),
            consumed: KeySet::default(),
            last_down: None,
            altgr_layout: false,
        }
    }

    /// Says whether the active keyboard layout has AltGr (design §11, matrix #22). When it has,
    /// right Alt (`RMENU`) is AltGr: it does not count as Alt, no key hotkey matches while it is
    /// held, and a hold during which it went down never fires a modifier-only hotkey. Otherwise,
    /// the default, right Alt is Alt.
    ///
    /// The setting takes effect with the next transition and survives [`reset`](Self::reset);
    /// adapters update it whenever the active layout changes.
    pub fn set_altgr_layout(&mut self, altgr_layout: bool) {
        self.altgr_layout = altgr_layout;
    }

    /// Whether right Alt is AltGr (see [`set_altgr_layout`](Self::set_altgr_layout)).
    pub fn altgr_layout(&self) -> bool {
        self.altgr_layout
    }

    /// Feeds one key transition and says what to do with it: `vk` is the virtual-key code and
    /// `is_down` is true for a key-down (including auto-repeat) and false for a key-up.
    ///
    /// Adapters withhold the event from the application and the engine when the verdict
    /// [`consumes`](KeyVerdict::consumes) it, and run the action of a
    /// [`KeyVerdict::Hotkey`].
    #[must_use]
    pub fn classify(&mut self, vk: u32, is_down: bool) -> KeyVerdict {
        let verdict = match (ModifierKey::of(vk), is_down) {
            (Some(key), true) => {
                self.modifier_down(key);
                KeyVerdict::Pass
            }
            (Some(key), false) => self.modifier_up(key),
            (None, true) => self.key_down(vk),
            (None, false) => self.key_up(vk),
        };
        if is_down {
            self.last_down = Some(vk);
        } else if self.last_down == Some(vk) {
            self.last_down = None;
        }
        verdict
    }

    /// Feeds one key transition like [`classify`](Self::classify) and returns only the matched
    /// hotkey, if any; `consume` says whether this event must be withheld from the application.
    ///
    /// The auto-repeats and the key-up of a consumed key return `None` here although they must
    /// be withheld too, so adapters that withhold events use [`classify`](Self::classify).
    pub fn on_key(&mut self, vk: u32, is_down: bool) -> Option<HotkeyMatch> {
        self.classify(vk, is_down).hotkey()
    }

    /// Forgets all key state: the modifiers that may still be held, the consumed keys, and the
    /// last key that went down. Call it on focus changes and whenever transitions may have been
    /// missed.
    pub fn reset(&mut self) {
        self.held = 0;
        self.hold = Hold::default();
        self.consumed = KeySet::default();
        self.last_down = None;
    }

    /// Reports non-keyboard input such as a mouse click: a modifier-only hotkey whose
    /// modifiers are held now does not fire. Held modifiers stay tracked.
    pub fn interrupt(&mut self) {
        if self.held != 0 {
            self.hold.spoiled = true;
        }
    }

    fn modifier_down(&mut self, key: ModifierKey) {
        if self.held == 0 {
            self.hold = Hold::default();
        }
        self.held |= key.bit;
        if self.altgr_layout && key.bit == RIGHT_ALT {
            self.hold.spoiled = true;
        }
        let now = logical(self.held, self.altgr_layout);
        let grew = now & !self.hold.seen != 0;
        self.hold.seen |= now;
        if now == self.hold.seen {
            self.hold.together = true;
        } else if grew {
            self.hold.together = false;
        }
    }

    fn modifier_up(&mut self, key: ModifierKey) -> KeyVerdict {
        if self.held & key.release == 0 {
            return KeyVerdict::Pass;
        }
        self.held &= !key.release;
        if self.held != 0 {
            return KeyVerdict::Pass;
        }
        let hold = core::mem::take(&mut self.hold);
        if hold.spoiled || !hold.together {
            return KeyVerdict::Pass;
        }
        self.bindings
            .get(&(hold.seen, None))
            .map_or(KeyVerdict::Pass, |&action| {
                KeyVerdict::Hotkey(HotkeyMatch {
                    action,
                    consume: false,
                })
            })
    }

    fn key_down(&mut self, code: u32) -> KeyVerdict {
        if self.held != 0 {
            self.hold.spoiled = true;
        }
        let repeat = self.last_down == Some(code);
        if self.consumed.contains(code) {
            if repeat {
                return KeyVerdict::Swallow;
            }
            // Another key went down since the consumed press, so this is a new press and the
            // key-up of the earlier one was lost.
            self.consumed.remove(code);
        }
        if repeat {
            // An auto-repeat of a press the application saw: consuming it would withhold the
            // key-up as well and leave the key stuck down in the host.
            return KeyVerdict::Pass;
        }
        if self.altgr_layout && self.held & RIGHT_ALT != 0 {
            // An AltGr chord types a character.
            return KeyVerdict::Pass;
        }
        let mods = logical(self.held, self.altgr_layout);
        let Some(&action) = self.bindings.get(&(mods, Some(code))) else {
            return KeyVerdict::Pass;
        };
        self.consumed.insert(code);
        KeyVerdict::Hotkey(HotkeyMatch {
            action,
            consume: true,
        })
    }

    fn key_up(&mut self, code: u32) -> KeyVerdict {
        if self.consumed.remove(code) {
            KeyVerdict::Swallow
        } else {
            KeyVerdict::Pass
        }
    }
}

impl Default for HotkeyClassifier {
    /// A classifier for the default hotkeys (plan D5).
    fn default() -> Self {
        Self::new(&HotkeyConfig::default())
    }
}
