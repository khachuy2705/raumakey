//! Semantic key maps shared by built-in and user-defined input methods.

use core::fmt;
use core::str::FromStr;

use serde::{Deserialize, Serialize};

/// Schema version of [`KeyMap`].
pub const KEYMAP_SCHEMA_VERSION: u32 = 1;

/// Vietnamese tones (sắc, huyền, hỏi, ngã, nặng).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Tone {
    /// Sắc.
    Acute,
    /// Huyền.
    Grave,
    /// Hỏi.
    Hook,
    /// Ngã.
    Tilde,
    /// Nặng.
    Dot,
}

/// Vowels that take a circumflex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BaseVowel {
    /// a → â
    A,
    /// e → ê
    E,
    /// o → ô
    O,
}

/// A semantic action bound to a key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyAction {
    /// Set (or undo) a tone.
    Tone(Tone),
    /// Remove the tone.
    RemoveTone,
    /// Circumflex on any of a/e/o.
    Circumflex,
    /// Circumflex on one specific vowel (Telex `aa`, `ee`, `oo`).
    CircumflexOn(BaseVowel),
    /// Breve on a.
    Breve,
    /// Horn on o/u (`uo` → `ươ`).
    Horn,
    /// Horn, else breve, else a standalone `ư` (Telex `w`).
    HornOrBreve,
    /// Stroke d → đ.
    Stroke,
    /// Escape the next key (VIQR backslash).
    Escape,
}

/// Error parsing a [`KeyAction`] text form.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyActionParseError;

impl fmt::Display for KeyActionParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown key action")
    }
}

impl std::error::Error for KeyActionParseError {}

impl fmt::Display for KeyAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Tone(Tone::Acute) => "tone:acute",
            Self::Tone(Tone::Grave) => "tone:grave",
            Self::Tone(Tone::Hook) => "tone:hook",
            Self::Tone(Tone::Tilde) => "tone:tilde",
            Self::Tone(Tone::Dot) => "tone:dot",
            Self::RemoveTone => "remove-tone",
            Self::Circumflex => "circumflex",
            Self::CircumflexOn(BaseVowel::A) => "circumflex:a",
            Self::CircumflexOn(BaseVowel::E) => "circumflex:e",
            Self::CircumflexOn(BaseVowel::O) => "circumflex:o",
            Self::Breve => "breve",
            Self::Horn => "horn",
            Self::HornOrBreve => "horn-or-breve",
            Self::Stroke => "stroke",
            Self::Escape => "escape",
        })
    }
}

impl FromStr for KeyAction {
    type Err = KeyActionParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Ok(match text {
            "tone:acute" => Self::Tone(Tone::Acute),
            "tone:grave" => Self::Tone(Tone::Grave),
            "tone:hook" => Self::Tone(Tone::Hook),
            "tone:tilde" => Self::Tone(Tone::Tilde),
            "tone:dot" => Self::Tone(Tone::Dot),
            "remove-tone" => Self::RemoveTone,
            "circumflex" => Self::Circumflex,
            "circumflex:a" => Self::CircumflexOn(BaseVowel::A),
            "circumflex:e" => Self::CircumflexOn(BaseVowel::E),
            "circumflex:o" => Self::CircumflexOn(BaseVowel::O),
            "breve" => Self::Breve,
            "horn" => Self::Horn,
            "horn-or-breve" => Self::HornOrBreve,
            "stroke" => Self::Stroke,
            "escape" => Self::Escape,
            _ => return Err(KeyActionParseError),
        })
    }
}

impl Serialize for KeyAction {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for KeyAction {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        text.parse().map_err(serde::de::Error::custom)
    }
}

/// Binding of one key to ordered candidate actions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyBinding {
    /// The key character (printable ASCII).
    #[serde(with = "single_char")]
    pub key: char,
    /// Candidate actions, tried in order; if none applies the key is literal.
    pub actions: Vec<KeyAction>,
}

/// Why a key map was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyMapError {
    /// Id is not `[A-Za-z0-9._-]{1,64}`.
    InvalidId,
    /// Name longer than 128 characters.
    InvalidName,
    /// Schema version not supported.
    UnsupportedVersion,
    /// Key outside printable ASCII.
    InvalidKey(char),
    /// Key bound twice (ASCII letters compare case-insensitively).
    DuplicateKey(char),
    /// A binding has no actions or more than four.
    InvalidActionCount(char),
}

impl fmt::Display for KeyMapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => f.write_str("invalid key map id"),
            Self::InvalidName => f.write_str("invalid key map name"),
            Self::UnsupportedVersion => f.write_str("unsupported key map version"),
            Self::InvalidKey(_) => f.write_str("key must be printable ASCII"),
            Self::DuplicateKey(_) => f.write_str("key is bound more than once"),
            Self::InvalidActionCount(_) => f.write_str("a key needs one to four actions"),
        }
    }
}

impl std::error::Error for KeyMapError {}

/// A versioned semantic key map.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyMap {
    /// Stable identifier.
    pub id: String,
    /// Schema version.
    pub version: u32,
    /// Display name.
    pub name: String,
    /// Bindings.
    pub bindings: Vec<KeyBinding>,
}

impl KeyMap {
    /// Maximum actions per key.
    pub const MAX_ACTIONS: usize = 4;

    /// Validates the key map.
    pub fn validate(&self) -> Result<(), KeyMapError> {
        let id_ok = !self.id.is_empty()
            && self.id.len() <= 64
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if !id_ok {
            return Err(KeyMapError::InvalidId);
        }
        if self.name.chars().count() > 128 {
            return Err(KeyMapError::InvalidName);
        }
        if self.version != KEYMAP_SCHEMA_VERSION {
            return Err(KeyMapError::UnsupportedVersion);
        }
        let mut seen = Vec::with_capacity(self.bindings.len());
        for binding in &self.bindings {
            let key = binding.key;
            if !matches!(key, '\u{21}'..='\u{7E}') {
                return Err(KeyMapError::InvalidKey(key));
            }
            let folded = key.to_ascii_lowercase();
            if seen.contains(&folded) {
                return Err(KeyMapError::DuplicateKey(key));
            }
            seen.push(folded);
            if binding.actions.is_empty() || binding.actions.len() > Self::MAX_ACTIONS {
                return Err(KeyMapError::InvalidActionCount(key));
            }
        }
        Ok(())
    }

    /// Candidate actions for `key`; ASCII letters match case-insensitively.
    pub fn lookup(&self, key: char) -> Option<&[KeyAction]> {
        let folded = key.to_ascii_lowercase();
        self.bindings
            .iter()
            .find(|b| b.key.to_ascii_lowercase() == folded)
            .map(|b| b.actions.as_slice())
    }
}

mod single_char {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &char, serializer: S) -> Result<S::Ok, S::Error> {
        let mut buffer = [0u8; 4];
        serializer.serialize_str(value.encode_utf8(&mut buffer))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<char, D::Error> {
        let text = String::deserialize(deserializer)?;
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(c),
            _ => Err(serde::de::Error::custom("expected exactly one character")),
        }
    }
}
