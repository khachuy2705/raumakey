//! Built-in key maps (plan §3.2) and the user key map registry.

use std::sync::OnceLock;

use raumakey_model::error::EngineError;
use raumakey_model::keymap::{
    BaseVowel, KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, Tone,
};
use raumakey_model::options::{EngineOptions, InputMethod};

const TELEX: &[(char, KeyAction)] = &[
    ('s', KeyAction::Tone(Tone::Acute)),
    ('f', KeyAction::Tone(Tone::Grave)),
    ('r', KeyAction::Tone(Tone::Hook)),
    ('x', KeyAction::Tone(Tone::Tilde)),
    ('j', KeyAction::Tone(Tone::Dot)),
    ('z', KeyAction::RemoveTone),
    ('a', KeyAction::CircumflexOn(BaseVowel::A)),
    ('e', KeyAction::CircumflexOn(BaseVowel::E)),
    ('o', KeyAction::CircumflexOn(BaseVowel::O)),
    ('w', KeyAction::HornOrBreve),
    ('d', KeyAction::Stroke),
];

const VNI: &[(char, KeyAction)] = &[
    ('1', KeyAction::Tone(Tone::Acute)),
    ('2', KeyAction::Tone(Tone::Grave)),
    ('3', KeyAction::Tone(Tone::Hook)),
    ('4', KeyAction::Tone(Tone::Tilde)),
    ('5', KeyAction::Tone(Tone::Dot)),
    ('0', KeyAction::RemoveTone),
    ('6', KeyAction::Circumflex),
    ('7', KeyAction::Horn),
    ('8', KeyAction::Breve),
    ('9', KeyAction::Stroke),
];

const VIQR: &[(char, KeyAction)] = &[
    ('\'', KeyAction::Tone(Tone::Acute)),
    ('`', KeyAction::Tone(Tone::Grave)),
    ('?', KeyAction::Tone(Tone::Hook)),
    ('~', KeyAction::Tone(Tone::Tilde)),
    ('.', KeyAction::Tone(Tone::Dot)),
    ('-', KeyAction::RemoveTone),
    ('^', KeyAction::Circumflex),
    ('(', KeyAction::Breve),
    ('+', KeyAction::Horn),
    ('*', KeyAction::Horn),
    ('d', KeyAction::Stroke),
    ('\\', KeyAction::Escape),
];

fn build(id: &str, name: &str, table: &[(char, KeyAction)]) -> KeyMap {
    KeyMap {
        id: id.to_owned(),
        version: KEYMAP_SCHEMA_VERSION,
        name: name.to_owned(),
        bindings: table
            .iter()
            .map(|&(key, action)| KeyBinding {
                key,
                actions: vec![action],
            })
            .collect(),
    }
}

/// Returns the built-in key map of `method`; `UserDefined` has none.
pub(crate) fn builtin(method: InputMethod) -> Option<&'static KeyMap> {
    static TELEX_MAP: OnceLock<KeyMap> = OnceLock::new();
    static SIMPLE_TELEX_MAP: OnceLock<KeyMap> = OnceLock::new();
    static VNI_MAP: OnceLock<KeyMap> = OnceLock::new();
    static VIQR_MAP: OnceLock<KeyMap> = OnceLock::new();
    match method {
        InputMethod::Telex => Some(TELEX_MAP.get_or_init(|| build("telex", "Telex", TELEX))),
        // The keys of Telex; the engine leaves out the standalone `w` (D53).
        InputMethod::SimpleTelex => {
            Some(SIMPLE_TELEX_MAP.get_or_init(|| build("simple-telex", "Simple Telex", TELEX)))
        }
        InputMethod::Vni => Some(VNI_MAP.get_or_init(|| build("vni", "VNI", VNI))),
        InputMethod::Viqr => Some(VIQR_MAP.get_or_init(|| build("viqr", "VIQR", VIQR))),
        InputMethod::UserDefined => None,
    }
}

/// Immutable registry of validated user key maps.
#[derive(Clone, Debug, Default)]
pub(crate) struct KeymapSet {
    user: Vec<KeyMap>,
}

impl KeymapSet {
    /// Keeps every valid key map; the first map with a given id wins.
    pub(crate) fn new(user_keymaps: Vec<KeyMap>) -> Self {
        let mut user: Vec<KeyMap> = Vec::with_capacity(user_keymaps.len());
        for map in user_keymaps {
            if map.validate().is_ok() && !user.iter().any(|known| known.id == map.id) {
                user.push(map);
            }
        }
        Self { user }
    }

    /// Resolves the key map selected by `options`.
    pub(crate) fn resolve(&self, options: &EngineOptions) -> Result<&KeyMap, EngineError> {
        if let Some(map) = builtin(options.input_method) {
            return Ok(map);
        }
        let id = options
            .user_keymap_id
            .as_deref()
            .ok_or(EngineError::UnsupportedInputMethod)?;
        self.user
            .iter()
            .find(|map| map.id == id)
            .ok_or(EngineError::UnsupportedInputMethod)
    }
}

#[cfg(test)]
mod tests {
    use raumakey_model::keymap::{BaseVowel, KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, Tone};

    use super::*;

    fn actions(map: &KeyMap, key: char) -> Vec<KeyAction> {
        map.lookup(key)
            .map(<[KeyAction]>::to_vec)
            .unwrap_or_default()
    }

    #[test]
    fn telex_follows_plan_table() {
        let map = builtin(InputMethod::Telex).expect("telex");
        assert!(map.validate().is_ok());
        assert_eq!(map.id, "telex");
        let expected = [
            ('s', KeyAction::Tone(Tone::Acute)),
            ('f', KeyAction::Tone(Tone::Grave)),
            ('r', KeyAction::Tone(Tone::Hook)),
            ('x', KeyAction::Tone(Tone::Tilde)),
            ('j', KeyAction::Tone(Tone::Dot)),
            ('z', KeyAction::RemoveTone),
            ('a', KeyAction::CircumflexOn(BaseVowel::A)),
            ('e', KeyAction::CircumflexOn(BaseVowel::E)),
            ('o', KeyAction::CircumflexOn(BaseVowel::O)),
            ('w', KeyAction::HornOrBreve),
            ('d', KeyAction::Stroke),
        ];
        for (key, action) in expected {
            assert_eq!(actions(map, key), vec![action], "key {key}");
            assert_eq!(
                actions(map, key.to_ascii_uppercase()),
                vec![action],
                "upper {key}"
            );
        }
        assert_eq!(map.bindings.len(), expected.len());
        assert!(map.lookup('q').is_none());
        assert!(map.lookup('1').is_none());
    }

    #[test]
    fn vni_follows_plan_table() {
        let map = builtin(InputMethod::Vni).expect("vni");
        assert!(map.validate().is_ok());
        assert_eq!(map.id, "vni");
        let expected = [
            ('1', KeyAction::Tone(Tone::Acute)),
            ('2', KeyAction::Tone(Tone::Grave)),
            ('3', KeyAction::Tone(Tone::Hook)),
            ('4', KeyAction::Tone(Tone::Tilde)),
            ('5', KeyAction::Tone(Tone::Dot)),
            ('0', KeyAction::RemoveTone),
            ('6', KeyAction::Circumflex),
            ('7', KeyAction::Horn),
            ('8', KeyAction::Breve),
            ('9', KeyAction::Stroke),
        ];
        for (key, action) in expected {
            assert_eq!(actions(map, key), vec![action], "key {key}");
        }
        assert_eq!(map.bindings.len(), expected.len());
        assert!(map.lookup('s').is_none());
        assert!(map.lookup('d').is_none());
    }

    #[test]
    fn viqr_follows_plan_table() {
        let map = builtin(InputMethod::Viqr).expect("viqr");
        assert!(map.validate().is_ok());
        assert_eq!(map.id, "viqr");
        let expected = [
            ('\'', KeyAction::Tone(Tone::Acute)),
            ('`', KeyAction::Tone(Tone::Grave)),
            ('?', KeyAction::Tone(Tone::Hook)),
            ('~', KeyAction::Tone(Tone::Tilde)),
            ('.', KeyAction::Tone(Tone::Dot)),
            ('-', KeyAction::RemoveTone),
            ('^', KeyAction::Circumflex),
            ('(', KeyAction::Breve),
            ('+', KeyAction::Horn),
            ('*', KeyAction::Horn),
            ('d', KeyAction::Stroke),
            ('\\', KeyAction::Escape),
        ];
        for (key, action) in expected {
            assert_eq!(actions(map, key), vec![action], "key {key}");
        }
        assert_eq!(map.bindings.len(), expected.len());
        assert_eq!(actions(map, 'D'), vec![KeyAction::Stroke]);
    }

    #[test]
    fn user_defined_has_no_builtin_map() {
        assert!(builtin(InputMethod::UserDefined).is_none());
    }

    fn user_map(id: &str, key: char) -> KeyMap {
        KeyMap {
            id: id.to_owned(),
            version: KEYMAP_SCHEMA_VERSION,
            name: id.to_owned(),
            bindings: vec![KeyBinding {
                key,
                actions: vec![KeyAction::Tone(Tone::Acute)],
            }],
        }
    }

    #[test]
    fn registry_resolves_builtins_and_user_maps() {
        let invalid = KeyMap {
            version: 99,
            ..user_map("broken", 'q')
        };
        let set = KeymapSet::new(vec![user_map("mine", 'q'), user_map("mine", 'k'), invalid]);
        let mut options = EngineOptions::defaults_v1();
        assert_eq!(set.resolve(&options).map(|m| m.id.as_str()), Ok("telex"));
        options.input_method = InputMethod::Viqr;
        assert_eq!(set.resolve(&options).map(|m| m.id.as_str()), Ok("viqr"));
        options.input_method = InputMethod::UserDefined;
        assert_eq!(
            set.resolve(&options).map(|m| m.id.as_str()),
            Err(EngineError::UnsupportedInputMethod)
        );
        options.user_keymap_id = Some("mine".to_owned());
        let mine = set.resolve(&options).expect("user map");
        assert!(mine.lookup('q').is_some(), "first map with an id wins");
        options.user_keymap_id = Some("broken".to_owned());
        assert_eq!(
            set.resolve(&options).map(|m| m.id.as_str()),
            Err(EngineError::UnsupportedInputMethod),
            "invalid maps are not registered"
        );
        options.user_keymap_id = Some("absent".to_owned());
        assert!(set.resolve(&options).is_err());
    }
}
