use raumakey_model::keymap::{
    BaseVowel, KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, KeyMapError, Tone,
};
use raumakey_model::options::{
    DEFAULTS_VERSION, EngineOptions, InputMethod, Normalization, OptionsError, OutputCharset,
};

/// D51: the product defaults are version 2: spell checking is off (the structure check of the
/// engine still keeps English words such as `EVKey` untouched); everything else is version 1.
#[test]
fn defaults_v2_turn_spell_checking_off_and_keep_the_rest_of_v1() {
    assert_eq!(DEFAULTS_VERSION, 2);
    let v2 = EngineOptions::defaults_v2();
    assert!(!v2.spell_check);
    assert_eq!(
        v2,
        EngineOptions {
            spell_check: false,
            ..EngineOptions::defaults_v1()
        }
    );
    assert!(v2.validate().is_ok());
}

#[test]
fn defaults_v1_are_versioned_product_decisions() {
    let o = EngineOptions::defaults_v1();
    assert!(o.enabled);
    assert_eq!(o.input_method, InputMethod::Telex);
    assert_eq!(o.normalization, Normalization::Nfc);
    assert!(!o.modern_tone);
    assert!(o.free_marking);
    assert!(o.spell_check);
    assert!(o.auto_restore_invalid);
    assert!(o.macros_enabled);
    assert!(!o.macros_when_disabled);
    assert!(o.auto_case_macro);
    assert_eq!(o.user_keymap_id, None);
    assert_eq!(o.context_limit_codepoints, 32);
    assert_eq!(EngineOptions::default(), o);
    assert!(o.validate().is_ok());
}

#[test]
fn options_validation() {
    let mut o = EngineOptions::defaults_v1();
    o.context_limit_codepoints = 7;
    assert_eq!(o.validate(), Err(OptionsError::ContextLimitOutOfRange));
    o.context_limit_codepoints = 129;
    assert_eq!(o.validate(), Err(OptionsError::ContextLimitOutOfRange));
    o.context_limit_codepoints = 128;
    assert!(o.validate().is_ok());
    o.input_method = InputMethod::UserDefined;
    assert_eq!(o.validate(), Err(OptionsError::MissingUserKeymap));
    o.user_keymap_id = Some("mine".into());
    assert!(o.validate().is_ok());
}

/// D54: the charset the hook types in, with the converter's names; Unicode by default, also
/// for options written before the field existed.
#[test]
fn the_output_charset_has_stable_names_and_defaults_to_unicode() {
    let names: Vec<String> = OutputCharset::ALL
        .iter()
        .map(|charset| serde_json::to_string(charset).unwrap())
        .collect();
    assert_eq!(
        names,
        [
            "\"unicode\"",
            "\"tcvn3\"",
            "\"vni-windows\"",
            "\"unicode-nfd\"",
            "\"cp1258\""
        ]
    );
    for charset in OutputCharset::ALL {
        assert_eq!(
            charset.name(),
            serde_json::to_string(&charset).unwrap().trim_matches('"')
        );
    }
    assert_eq!(OutputCharset::default(), OutputCharset::Unicode);
    assert_eq!(EngineOptions::defaults_v1().charset, OutputCharset::Unicode);
    assert_eq!(EngineOptions::defaults_v2().charset, OutputCharset::Unicode);
    let mut json = serde_json::to_value(EngineOptions::defaults_v2()).unwrap();
    json.as_object_mut().unwrap().remove("charset");
    let old: EngineOptions = serde_json::from_value(json).unwrap();
    assert_eq!(old, EngineOptions::defaults_v2());
}

#[test]
fn option_enums_use_stable_names() {
    assert_eq!(
        serde_json::to_string(&InputMethod::UserDefined).unwrap(),
        "\"user\""
    );
    assert_eq!(serde_json::to_string(&InputMethod::Vni).unwrap(), "\"vni\"");
    // D53: Simple Telex.
    assert_eq!(
        serde_json::to_string(&InputMethod::SimpleTelex).unwrap(),
        "\"simple-telex\""
    );
    assert_eq!(
        serde_json::from_str::<InputMethod>("\"simple-telex\"").unwrap(),
        InputMethod::SimpleTelex
    );
    assert_eq!(
        serde_json::to_string(&Normalization::Preserve).unwrap(),
        "\"preserve\""
    );
    let o = EngineOptions::defaults_v1();
    let json = serde_json::to_string(&o).unwrap();
    assert_eq!(serde_json::from_str::<EngineOptions>(&json).unwrap(), o);
}

#[test]
fn key_action_text_form_round_trips() {
    let all = [
        (KeyAction::Tone(Tone::Acute), "tone:acute"),
        (KeyAction::Tone(Tone::Grave), "tone:grave"),
        (KeyAction::Tone(Tone::Hook), "tone:hook"),
        (KeyAction::Tone(Tone::Tilde), "tone:tilde"),
        (KeyAction::Tone(Tone::Dot), "tone:dot"),
        (KeyAction::RemoveTone, "remove-tone"),
        (KeyAction::Circumflex, "circumflex"),
        (KeyAction::CircumflexOn(BaseVowel::A), "circumflex:a"),
        (KeyAction::CircumflexOn(BaseVowel::E), "circumflex:e"),
        (KeyAction::CircumflexOn(BaseVowel::O), "circumflex:o"),
        (KeyAction::Breve, "breve"),
        (KeyAction::Horn, "horn"),
        (KeyAction::HornOrBreve, "horn-or-breve"),
        (KeyAction::Stroke, "stroke"),
        (KeyAction::Escape, "escape"),
    ];
    for (action, text) in all {
        assert_eq!(action.to_string(), text);
        assert_eq!(text.parse::<KeyAction>().unwrap(), action);
    }
    assert!("tone:sharp".parse::<KeyAction>().is_err());
    assert!("circumflex:u".parse::<KeyAction>().is_err());
    assert!("".parse::<KeyAction>().is_err());
}

fn sample_map() -> KeyMap {
    KeyMap {
        id: "my-map".into(),
        version: KEYMAP_SCHEMA_VERSION,
        name: "My map".into(),
        bindings: vec![
            KeyBinding {
                key: 's',
                actions: vec![KeyAction::Tone(Tone::Acute)],
            },
            KeyBinding {
                key: 'w',
                actions: vec![KeyAction::HornOrBreve],
            },
            KeyBinding {
                key: '6',
                actions: vec![KeyAction::Circumflex],
            },
        ],
    }
}

#[test]
fn keymap_validation_accepts_well_formed_map() {
    assert!(sample_map().validate().is_ok());
}

#[test]
fn keymap_validation_rejects_bad_maps() {
    let mut m = sample_map();
    m.id = "bad id".into();
    assert_eq!(m.validate(), Err(KeyMapError::InvalidId));

    let mut m = sample_map();
    m.version = 2;
    assert_eq!(m.validate(), Err(KeyMapError::UnsupportedVersion));

    let mut m = sample_map();
    m.bindings.push(KeyBinding {
        key: 'S',
        actions: vec![KeyAction::Tone(Tone::Grave)],
    });
    assert_eq!(m.validate(), Err(KeyMapError::DuplicateKey('S')));

    let mut m = sample_map();
    m.bindings.push(KeyBinding {
        key: ' ',
        actions: vec![KeyAction::Breve],
    });
    assert_eq!(m.validate(), Err(KeyMapError::InvalidKey(' ')));

    let mut m = sample_map();
    m.bindings.push(KeyBinding {
        key: 'é',
        actions: vec![KeyAction::Breve],
    });
    assert_eq!(m.validate(), Err(KeyMapError::InvalidKey('é')));

    let mut m = sample_map();
    m.bindings.push(KeyBinding {
        key: 'q',
        actions: vec![],
    });
    assert_eq!(m.validate(), Err(KeyMapError::InvalidActionCount('q')));

    let mut m = sample_map();
    m.bindings.push(KeyBinding {
        key: 'q',
        actions: vec![KeyAction::Breve; 5],
    });
    assert_eq!(m.validate(), Err(KeyMapError::InvalidActionCount('q')));

    let mut m = sample_map();
    m.name = "n".repeat(129);
    assert_eq!(m.validate(), Err(KeyMapError::InvalidName));
}

#[test]
fn keymap_lookup_is_case_insensitive_for_letters() {
    let m = sample_map();
    assert_eq!(m.lookup('s'), Some(&[KeyAction::Tone(Tone::Acute)][..]));
    assert_eq!(m.lookup('S'), Some(&[KeyAction::Tone(Tone::Acute)][..]));
    assert_eq!(m.lookup('6'), Some(&[KeyAction::Circumflex][..]));
    assert_eq!(m.lookup('x'), None);
}

#[test]
fn keymap_toml_round_trip() {
    let m = sample_map();
    let text = toml::to_string(&m).unwrap();
    assert!(text.contains("tone:acute"));
    let back: KeyMap = toml::from_str(&text).unwrap();
    assert_eq!(back, m);
    let bad = "id = \"x\"\nversion = 1\nname = \"x\"\nbindings = [{ key = \"ab\", actions = [\"breve\"] }]\n";
    assert!(toml::from_str::<KeyMap>(bad).is_err());
}
