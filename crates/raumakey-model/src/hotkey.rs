//! Hotkey definitions (design §11, plan D5).

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::input::vk;

/// What a hotkey triggers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyAction {
    /// Toggle Vietnamese/English.
    ToggleMode,
    /// Reset engine/context state without changing committed text.
    HardReset,
}

/// The non-modifier part of a hotkey.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HotkeyKey {
    /// The modifiers are pressed and released with no other key in between.
    ModifiersOnly,
    /// A virtual key pressed while the modifiers are held.
    VirtualKey(u32),
}

/// A hotkey such as `Ctrl+Shift` or `Ctrl+Shift+Alt+F12`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hotkey {
    /// Ctrl.
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt.
    pub alt: bool,
    /// Windows key.
    pub win: bool,
    /// Key part.
    pub key: HotkeyKey,
}

/// Why a hotkey string was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HotkeyParseError {
    /// No tokens.
    Empty,
    /// Unknown key or modifier name.
    UnknownToken,
    /// A modifier appears twice.
    DuplicateModifier,
    /// More than one non-modifier key.
    MultipleKeys,
    /// A key hotkey needs at least one modifier.
    NeedsModifier,
    /// A modifier-only hotkey needs at least two modifiers.
    NeedsTwoModifiers,
}

impl fmt::Display for HotkeyParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty hotkey",
            Self::UnknownToken => "unknown key name",
            Self::DuplicateModifier => "modifier repeated",
            Self::MultipleKeys => "only one key may follow the modifiers",
            Self::NeedsModifier => "a key hotkey needs a modifier",
            Self::NeedsTwoModifiers => "a modifier-only hotkey needs two modifiers",
        })
    }
}

impl std::error::Error for HotkeyParseError {}

const NAMED_KEYS: &[(&str, u32)] = &[
    ("Space", vk::SPACE),
    ("Enter", vk::RETURN),
    ("Tab", vk::TAB),
    ("Esc", vk::ESCAPE),
    ("Backspace", vk::BACK),
    ("Delete", vk::DELETE),
    ("Insert", vk::INSERT),
    ("Home", vk::HOME),
    ("End", vk::END),
    ("PageUp", vk::PRIOR),
    ("PageDown", vk::NEXT),
    ("Left", vk::LEFT),
    ("Up", vk::UP),
    ("Right", vk::RIGHT),
    ("Down", vk::DOWN),
    ("Pause", vk::PAUSE),
];

fn key_from_name(token: &str) -> Option<u32> {
    if let Some((_, code)) = NAMED_KEYS
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(token))
    {
        return Some(*code);
    }
    if token.eq_ignore_ascii_case("Return") {
        return Some(vk::RETURN);
    }
    if token.eq_ignore_ascii_case("Escape") {
        return Some(vk::ESCAPE);
    }
    let bytes = token.as_bytes();
    if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
        return Some(u32::from(bytes[0].to_ascii_uppercase()));
    }
    if let Some(number) = token.strip_prefix(['F', 'f'])
        && let Ok(n) = number.parse::<u32>()
        && (1..=24).contains(&n)
    {
        return Some(vk::F1 + n - 1);
    }
    if let Some(hex) = token
        .strip_prefix("0x")
        .or_else(|| token.strip_prefix("0X"))
        && let Ok(code) = u32::from_str_radix(hex, 16)
        && (1..=0xFE).contains(&code)
        && !vk::is_modifier(code)
    {
        return Some(code);
    }
    None
}

fn key_name(code: u32) -> String {
    if let Some((name, _)) = NAMED_KEYS.iter().find(|(_, c)| *c == code) {
        return (*name).to_owned();
    }
    match code {
        0x30..=0x39 | 0x41..=0x5A => char::from_u32(code).map_or_else(String::new, String::from),
        c if (vk::F1..=vk::F24).contains(&c) => format!("F{}", c - vk::F1 + 1),
        c => format!("0x{c:02X}"),
    }
}

impl FromStr for Hotkey {
    type Err = HotkeyParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let mut hotkey = Hotkey {
            ctrl: false,
            shift: false,
            alt: false,
            win: false,
            key: HotkeyKey::ModifiersOnly,
        };
        let mut any = false;
        for raw in text.split('+') {
            let token = raw.trim();
            if token.is_empty() {
                return Err(if any {
                    HotkeyParseError::UnknownToken
                } else {
                    HotkeyParseError::Empty
                });
            }
            any = true;
            let modifier = match token.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => Some(&mut hotkey.ctrl),
                "shift" => Some(&mut hotkey.shift),
                "alt" => Some(&mut hotkey.alt),
                "win" | "windows" => Some(&mut hotkey.win),
                _ => None,
            };
            match modifier {
                Some(flag) if *flag => return Err(HotkeyParseError::DuplicateModifier),
                Some(flag) => *flag = true,
                None => {
                    if hotkey.key != HotkeyKey::ModifiersOnly {
                        return Err(HotkeyParseError::MultipleKeys);
                    }
                    let code = key_from_name(token).ok_or(HotkeyParseError::UnknownToken)?;
                    hotkey.key = HotkeyKey::VirtualKey(code);
                }
            }
        }
        if !any {
            return Err(HotkeyParseError::Empty);
        }
        let modifiers = [hotkey.ctrl, hotkey.shift, hotkey.alt, hotkey.win]
            .iter()
            .filter(|m| **m)
            .count();
        match hotkey.key {
            HotkeyKey::ModifiersOnly if modifiers < 2 => Err(HotkeyParseError::NeedsTwoModifiers),
            HotkeyKey::VirtualKey(_) if modifiers == 0 => Err(HotkeyParseError::NeedsModifier),
            _ => Ok(hotkey),
        }
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts: Vec<String> = Vec::with_capacity(5);
        if self.ctrl {
            parts.push("Ctrl".to_owned());
        }
        if self.shift {
            parts.push("Shift".to_owned());
        }
        if self.alt {
            parts.push("Alt".to_owned());
        }
        if self.win {
            parts.push("Win".to_owned());
        }
        if let HotkeyKey::VirtualKey(code) = self.key {
            parts.push(key_name(code));
        }
        f.write_str(&parts.join("+"))
    }
}

impl Serialize for Hotkey {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Hotkey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Configured hotkeys.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HotkeyConfig {
    /// Vietnamese/English toggle.
    #[serde(default = "default_toggle")]
    pub toggle_mode: Vec<Hotkey>,
    /// Hard reset.
    #[serde(default = "default_hard_reset")]
    pub hard_reset: Vec<Hotkey>,
}

fn default_toggle() -> Vec<Hotkey> {
    vec![Hotkey {
        ctrl: true,
        shift: true,
        alt: false,
        win: false,
        key: HotkeyKey::ModifiersOnly,
    }]
}

fn default_hard_reset() -> Vec<Hotkey> {
    let combo = |code| Hotkey {
        ctrl: true,
        shift: true,
        alt: true,
        win: false,
        key: HotkeyKey::VirtualKey(code),
    };
    vec![combo(vk::F12), combo(vk::SPACE)]
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            toggle_mode: default_toggle(),
            hard_reset: default_hard_reset(),
        }
    }
}
