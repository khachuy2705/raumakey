//! Property tests: random scripts never panic, every transaction validates and applies, the
//! owned text is always the document suffix, output is well-formed UTF-16, and processing is
//! deterministic. A rejected transaction (stale state against a changed document) resets the
//! engine instead of committing the state.

mod common;

use std::sync::Arc;

use common::MacroDouble;
use proptest::prelude::*;
use raumakey_engine::script::{self, KeyToken, event_for, script_host};
use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_model::input::InputEvent;
use raumakey_model::keymap::{
    BaseVowel, KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, Tone,
};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};
use raumakey_model::transaction::TextSimulator;
use raumakey_model::utf16;

/// Id of the user key map registered with every engine in this suite.
const USER_MAP: &str = "props";

fn token() -> impl Strategy<Value = KeyToken> {
    let chars: Vec<char> = concat!(
        "aAbcdDeEfFgGhiIjJkKlmnoOpqQrsStTuUvwWxyYz0123456789",
        "'`?~.-^(+*\\ ,;:!\"[]{}<>/|=@đưÂ😀\u{301}\u{302}"
    )
    .chars()
    .collect();
    prop_oneof![
        8 => prop::sample::select(chars).prop_map(KeyToken::Char),
        1 => prop::sample::select(vec![
            KeyToken::Backspace,
            KeyToken::Space,
            KeyToken::ShiftSpace,
            KeyToken::Enter,
            KeyToken::Tab,
            KeyToken::Navigation,
            KeyToken::Mouse,
            KeyToken::Focus,
            KeyToken::Ctrl('c'),
            KeyToken::Ctrl(' '),
            KeyToken::Ctrl('\r'),
            KeyToken::Ctrl('\t'),
            KeyToken::AltGr('a'),
            KeyToken::AltGr('@'),
            KeyToken::AltGr('}'),
            KeyToken::SecureOn,
            KeyToken::SecureOff,
        ]),
    ]
}

/// Extra modifiers held with a key: Alt and Win turn any key into a shortcut.
#[derive(Clone, Copy, Debug)]
enum Held {
    Nothing,
    Alt,
    Win,
    Shift,
}

fn held() -> impl Strategy<Value = Held> {
    prop_oneof![
        12 => Just(Held::Nothing),
        1 => Just(Held::Alt),
        1 => Just(Held::Win),
        1 => Just(Held::Shift),
    ]
}

fn event(token: &KeyToken, held: Held, secure: bool) -> InputEvent {
    let host = script_host().expect("host");
    let mut event = event_for(token, &host, secure);
    match held {
        Held::Nothing => {}
        Held::Alt => event.modifiers.alt = true,
        Held::Win => event.modifiers.win = true,
        Held::Shift => event.modifiers.shift = true,
    }
    event
}

fn options() -> impl Strategy<Value = EngineOptions> {
    (
        prop::sample::select(vec![
            InputMethod::Telex,
            InputMethod::SimpleTelex,
            InputMethod::Vni,
            InputMethod::Viqr,
            InputMethod::UserDefined,
        ]),
        prop::sample::select(vec![
            Normalization::Nfc,
            Normalization::Nfd,
            Normalization::Preserve,
        ]),
        any::<[bool; 8]>(),
        0u32..=160,
    )
        .prop_map(|(method, normalization, flags, limit)| EngineOptions {
            enabled: flags[0] || flags[7],
            input_method: method,
            normalization,
            modern_tone: flags[1],
            free_marking: flags[2],
            spell_check: flags[3] || flags[7],
            auto_restore_invalid: flags[4],
            macros_enabled: flags[5],
            macros_when_disabled: flags[6],
            auto_case_macro: flags[7],
            user_keymap_id: Some(USER_MAP.to_owned()),
            context_limit_codepoints: limit,
            charset: Default::default(),
        })
}

fn binding(key: char, actions: &[KeyAction]) -> KeyBinding {
    KeyBinding {
        key,
        actions: actions.to_vec(),
    }
}

/// A user key map with multi-action candidates, letters, digits, and bound punctuation.
fn user_map() -> KeyMap {
    KeyMap {
        id: USER_MAP.to_owned(),
        version: KEYMAP_SCHEMA_VERSION,
        name: "Property test map".to_owned(),
        bindings: vec![
            binding('s', &[KeyAction::Tone(Tone::Acute)]),
            binding('f', &[KeyAction::Tone(Tone::Grave)]),
            binding('r', &[KeyAction::Tone(Tone::Hook)]),
            binding('x', &[KeyAction::Tone(Tone::Tilde)]),
            binding('j', &[KeyAction::Tone(Tone::Dot)]),
            binding('z', &[KeyAction::RemoveTone]),
            binding('w', &[KeyAction::HornOrBreve]),
            binding('d', &[KeyAction::Stroke]),
            binding('6', &[KeyAction::Circumflex]),
            binding('7', &[KeyAction::Horn, KeyAction::Breve]),
            binding(
                ';',
                &[
                    KeyAction::CircumflexOn(BaseVowel::A),
                    KeyAction::CircumflexOn(BaseVowel::E),
                    KeyAction::CircumflexOn(BaseVowel::O),
                    KeyAction::Breve,
                ],
            ),
            binding('[', &[KeyAction::Horn]),
            binding('/', &[KeyAction::Escape]),
            binding('\\', &[KeyAction::Escape]),
            binding('.', &[KeyAction::Tone(Tone::Dot), KeyAction::RemoveTone]),
        ],
    }
}

fn macros() -> Arc<MacroDouble> {
    MacroDouble::new(&[
        ("ko", "không"),
        ("vn", "Việt Nam"),
        ("đc", "được"),
        ("a1", "😀"),
        // Token triggers (D28).
        ("v.v", "vân vân"),
        ("...", "…"),
    ])
}

fn engine_with_macros(macros: Arc<MacroDouble>) -> VietnameseEngine {
    let map = user_map();
    assert!(map.validate().is_ok(), "the test key map is valid");
    VietnameseEngine::new(vec![map], macros)
}

fn engine() -> VietnameseEngine {
    engine_with_macros(macros())
}

/// 512 cases by default; `PROPTEST_CASES` overrides it for stress runs.
fn cases() -> u32 {
    std::env::var("PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(512)
}

proptest! {
    #![proptest_config(ProptestConfig { cases: cases(), ..ProptestConfig::default() })]

    #[test]
    fn random_scripts_keep_every_invariant(
        keys in prop::collection::vec((token(), held()), 0..80),
        options in options(),
        prefill in prop::sample::select(vec!["", "x", "😀", "e\u{302}"]),
    ) {
        let macros = macros();
        let engine = engine_with_macros(Arc::clone(&macros));
        let mut sim = TextSimulator::with_text(prefill);
        let mut state = EngineState::new();
        let mut secure = false;
        for (token, held) in &keys {
            secure = match token {
                KeyToken::SecureOn => true,
                KeyToken::SecureOff => false,
                _ => secure,
            };
            let event = event(token, *held, secure);
            let queries = (macros.calls().len(), macros.prefix_calls().len());
            let result = engine.process(&event, &options, &state);
            prop_assert!(result.is_ok(), "{:?}", result);
            let result = result.expect("checked");
            let tx = &result.transaction;
            let available = u32::try_from(sim.units().len()).expect("small");
            prop_assert!(tx.validate(available, u32::MAX).is_ok());
            prop_assert!(tx.delete_before as usize <= state.owned_utf16().len());
            prop_assert!(utf16::is_well_formed(&tx.insert_utf16));
            if event.is_shortcut() {
                // Shortcuts bypass the engine (matrix #21).
                prop_assert!(tx.is_pass_through() && tx.reset_context, "{:?}", tx);
                prop_assert!(result.next.is_idle());
            }
            if event.host.is_secure_field {
                // The engine never looks up a macro in a secure field (D25, matrix #29).
                prop_assert_eq!(
                    (macros.calls().len(), macros.prefix_calls().len()),
                    queries
                );
            }
            if tx.reset_context {
                prop_assert!(result.next.is_idle());
            }
            prop_assert!(sim.apply(&event, tx).is_ok());
            prop_assert!(utf16::is_well_formed(sim.units()));
            state = result.next;
            prop_assert!(sim.units().ends_with(state.owned_utf16()));
            prop_assert!(utf16::is_well_formed(state.owned_utf16()));
            prop_assert!(state.is_idle() || !state.owned_utf16().is_empty());
        }
    }

    #[test]
    fn processing_is_deterministic(
        tokens in prop::collection::vec(token(), 0..60),
        options in options(),
    ) {
        let engine = engine();
        let first = script::run_script(&engine, &options, &tokens).expect("runs");
        let second = script::run_script(&engine, &options, &tokens).expect("runs");
        prop_assert_eq!(&first, &second);
        prop_assert!(first.steps.iter().all(|step| step.accepted));
        prop_assert!(utf16::is_well_formed(&utf16::encode(&first.final_text)));
    }

    #[test]
    fn stale_states_are_rejected_or_applied_never_committed_blindly(
        typed in prop::collection::vec(token(), 0..30),
        later in prop::collection::vec(token(), 1..30),
        options in options(),
        document in prop::sample::select(vec!["", "x", "😀", "e\u{302}", "ab"]),
    ) {
        // A state built against one document, then used against another (the host changed
        // the text behind the engine's back).
        let engine = engine();
        let host = script_host().expect("host");
        let stale = script::run_script(&engine, &options, &typed).expect("runs").final_state;
        let outcome = script::run_script_from(&engine, &options, &later, &host, document, stale)
            .expect("runs");
        let mut sim = TextSimulator::with_text(document);
        for step in &outcome.steps {
            prop_assert!(utf16::is_well_formed(&step.transaction.insert_utf16));
            let mut probe = sim.clone();
            let applies = probe.apply(&step.event, &step.transaction).is_ok();
            // `accepted` is exactly the simulator's verdict.
            prop_assert_eq!(applies, step.accepted);
            if step.accepted {
                sim = probe;
            } else {
                // Not committed: the key passes through and the engine starts over.
                let pass = raumakey_model::transaction::EditTransaction::pass_through_reset();
                prop_assert!(sim.apply(&step.event, &pass).is_ok());
            }
        }
        prop_assert_eq!(sim.as_string(), outcome.final_text.clone());
        if outcome.steps.last().is_some_and(|step| !step.accepted) {
            prop_assert!(outcome.final_state.is_idle());
        }
    }

    #[test]
    fn engine_never_mutates_its_inputs(
        keys in prop::collection::vec((token(), held()), 1..40),
        options in options(),
    ) {
        let engine = engine();
        let mut state = EngineState::new();
        for (token, held) in &keys {
            let event = event(token, *held, false);
            let before = state.clone();
            let result = engine.process(&event, &options, &state).expect("processed");
            prop_assert_eq!(&state, &before);
            let again = engine.process(&event, &options, &state).expect("processed");
            prop_assert_eq!(&again, &result);
            state = result.next;
        }
    }

    #[test]
    fn arbitrary_utf16_never_panics(
        units in prop::collection::vec(any::<u16>(), 0..20),
        options in options(),
    ) {
        let engine = engine();
        let host = script_host().expect("host");
        let after_word = script::run_script(
            &engine,
            &options,
            &script::parse_keys("tieeng").expect("valid"),
        )
        .expect("runs")
        .final_state;
        for prior in [EngineState::new(), after_word] {
            let event = InputEvent::text(units.clone(), host.clone());
            match engine.process(&event, &options, &prior) {
                Ok(result) => prop_assert!(utf16::is_well_formed(&result.transaction.insert_utf16)),
                Err(error) => prop_assert!(matches!(
                    error,
                    raumakey_model::error::EngineError::InvalidUtf16
                        | raumakey_model::error::EngineError::CapacityExceeded
                )),
            }
        }
    }
}
