//! Option switches, event handling (plan §3.8), capacity, and every engine error (plan §3.9).

mod common;

use std::sync::Arc;

use common::{MacroDouble, engine, engine_with, run, telex, txs, type_keys, typed, viqr, vni};
use raumakey_engine::script::{format_transaction, script_host};
use raumakey_engine::{EngineState, HISTORY_CAPACITY, STATE_VERSION, VietnameseEngine, WordForm};
use raumakey_model::error::{EngineError, ResetReason};
use raumakey_model::host::{DocumentId, SessionId};
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::{InputEvent, InputKind, vk};
use raumakey_model::keymap::{
    BaseVowel, KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, Tone,
};
use raumakey_model::macros::NoMacros;
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_model::transaction::EditTransaction;
use raumakey_model::utf16;

const PASS: &str = "Tx(0,\"\",false,None,false)";
const PASS_RESET: &str = "Tx(0,\"\",false,None,true)";

fn state_after(options: &EngineOptions, keys: &str) -> EngineState {
    run(&engine(), options, keys).final_state
}

#[test]
fn spell_check_on_and_off() {
    let mut off = telex();
    off.spell_check = false;
    assert_eq!(type_keys(&telex(), "kw"), "kw");
    assert_eq!(type_keys(&off, "kw"), "kư");
    assert_eq!(type_keys(&telex(), "clas"), "clas");
    assert_eq!(type_keys(&telex(), "after"), "àter");
    // D52: without spell checking the structure check still stops at the second vowel group,
    // and the transformed word is restored at the boundary.
    assert_eq!(type_keys(&off, "after"), "àter");
    assert_eq!(type_keys(&off, "after{SPACE}"), "after ");
    assert_eq!(type_keys(&off, "tieengs{SPACE}"), "tiếng ");
}

/// D52: with spell checking off the engine still checks the structure of a syllable:
/// Vietnamese consonants, one group of vowels, and a coda of `c ch m n ng nh p t` (or none).
/// A key that would leave another structure is typed as it is, and a transformed word that
/// lost the structure is restored at the boundary (with `auto_restore_invalid`).
#[test]
fn d52_the_structure_check_keeps_english_words_without_spell_check() {
    let mut off = telex();
    off.spell_check = false;
    for (keys, expected) in [
        // The mark key would reach back over consonants that end no syllable.
        ("EVKey", "EVKey"),
        ("EVKey{SPACE}", "EVKey "),
        // Telex tone keys inside an English word (`x r s`) transform it as with spell
        // checking (`Êps`, `Ễcl`); the boundary restores the keys.
        ("Express{SPACE}", "Express "),
        ("Excel{SPACE}", "Excel "),
        // The second group of vowels makes the word literal; the standalone `ư` of `W` is
        // never taken back by a later `w`, and the boundary restores the keys.
        ("Windows", "Ưindows"),
        ("Windows{SPACE}", "Windows "),
        ("Google", "Gôgle"),
        ("Google{SPACE}", "Google "),
        ("Vietnamese", "Vietnamese"),
        // Letters that are not Vietnamese (`f j w z`) make no syllable.
        ("fast", "fast"),
        ("java", "java"),
        // Vietnamese and spell-check-free words keep working.
        ("tieengs vieetj{SPACE}", "tiếng việt "),
        ("hoaf", "hòa"),
        ("kw", "kư"),
        // Marks need a Vietnamese onset (as before D52): `gr`, `str`, `kr` take none.
        ("green{SPACE}", "green "),
        ("street{SPACE}", "street "),
        ("ddi", "đi"),
    ] {
        assert_eq!(type_keys(&off, keys), expected, "{keys}");
    }
    // At the boundary a transformed word must end as a Vietnamese rhyme (after its onset,
    // `gi` and `qu` included) whose tone fits its coda; any other is restored, as with spell
    // checking.
    for (keys, expected) in [
        ("were{SPACE}", "were "),
        ("users{SPACE}", "users "),
        ("text{SPACE}", "text "),
        ("mouse{SPACE}", "mouse "),
        ("sweet{SPACE}", "sweet "),
        ("awesome{SPACE}", "awesome "),
        ("Win{SPACE}", "Win "),
        ("swim{SPACE}", "swim "),
        ("nguwowif{SPACE}", "người "),
        ("gieengs{SPACE}", "giếng "),
        ("giuwax{SPACE}", "giữa "),
        ("quoocs{SPACE}", "quốc "),
        ("quyeets{SPACE}", "quyết "),
        ("dduwowngf{SPACE}", "đường "),
        ("xooong{SPACE}", "xoong "),
        ("thuowr{SPACE}", "thuở "),
        // Spell checking alone forbids these onsets; the rhyme is Vietnamese.
        ("kaf{SPACE}", "kà "),
    ] {
        assert_eq!(type_keys(&off, keys), expected, "{keys}");
    }
    // Without auto-restore the word stays as it was typed.
    let mut keep = off.clone();
    keep.auto_restore_invalid = false;
    assert_eq!(type_keys(&keep, "Google{SPACE}"), "Gôgle ");
    // Spell checking is unchanged: still stricter than the structure check.
    for (keys, expected) in [
        ("EVKey", "EVKey"),
        ("Windows{SPACE}", "Windows "),
        ("Kroong", "Kroong"),
        ("Google{SPACE}", "Google "),
    ] {
        assert_eq!(type_keys(&telex(), keys), expected, "spell check {keys}");
    }
}

/// D34: with spell checking off, `Circumflex` (VNI `6`, VIQR `^`) applies to the rightmost
/// `a`, `e`, or `o` of the vowel cluster.
#[test]
fn d34_circumflex_without_spell_check_marks_the_rightmost_a_e_or_o() {
    let mut vni_off = vni();
    vni_off.spell_check = false;
    let mut viqr_off = viqr();
    viqr_off.spell_check = false;
    for (vni_keys, viqr_keys, expected) in [
        ("oa6", "oa^", "oâ"),
        ("ao6", "ao^", "aô"),
        ("eo6", "eo^", "eô"),
        ("oe6", "oe^", "oê"),
        ("eao6", "eao^", "eaô"),
        ("koa6", "koa^", "koâ"),
        ("oan6", "oan^", "oân"),
        ("uoa6", "uoa^", "uoâ"),
    ] {
        assert_eq!(type_keys(&vni_off, vni_keys), expected, "vni {vni_keys}");
        assert_eq!(
            type_keys(&viqr_off, viqr_keys),
            expected,
            "viqr {viqr_keys}"
        );
    }
    // The candidate already marked undoes, wherever it is (plan §3.6).
    assert_eq!(type_keys(&vni_off, "o6a6"), "oa6");
    // With spell checking on these clusters are not Vietnamese, so the key stays literal.
    assert_eq!(type_keys(&vni(), "oa6"), "oa6");
    assert_eq!(type_keys(&viqr(), "ao^"), "ao^");
}

#[test]
fn spelling_follows_qu_and_g_orthography() {
    // `ă` takes a `p` coda and `êu` is a rhyme after `qu` (co quắp, quặp, quều quào).
    for (keys, expected) in [
        ("quawps{SPACE}", "quắp "),
        ("quawpj{SPACE}", "quặp "),
        ("queeuf{SPACE}", "quều "),
        ("Quawps{SPACE}", "Quắp "),
    ] {
        assert_eq!(type_keys(&telex(), keys), expected, "telex {keys}");
    }
    assert_eq!(type_keys(&vni(), "qua8p1{SPACE}"), "quắp ");
    assert_eq!(type_keys(&viqr(), "que^u`{SPACE}"), "quều ");
    // Plain `g` never precedes e ê y (`gh` does), so English words stay untouched.
    for (keys, expected) in [
        ("gets{SPACE}", "gets "),
        ("gems{SPACE}", "gems "),
        ("gefs{SPACE}", "gefs "),
        ("ghees{SPACE}", "ghế "),
        ("gier{SPACE}", "giẻ "),
        ("gieets{SPACE}", "giết "),
    ] {
        assert_eq!(type_keys(&telex(), keys), expected, "telex {keys}");
    }
}

#[test]
fn free_marking_on_and_off() {
    let mut off = telex();
    off.free_marking = false;
    assert_eq!(type_keys(&telex(), "tiengse"), "tiếng");
    assert_eq!(type_keys(&off, "tiengse"), "tiengse");
    assert_eq!(type_keys(&off, "tieesng"), "tiếng");
    assert_eq!(type_keys(&telex(), "did"), "đi");
    assert_eq!(type_keys(&off, "did"), "did");
    assert_eq!(type_keys(&off, "ddi"), "đi");
    assert_eq!(type_keys(&off, "nguowif"), "người");
    let mut vni_off = vni();
    vni_off.free_marking = false;
    assert_eq!(type_keys(&vni_off, "tie6ng1"), "tiêng1");
    assert_eq!(type_keys(&vni_off, "tie61ng"), "tiếng");
}

#[test]
fn auto_restore_on_and_off() {
    let mut off = telex();
    off.auto_restore_invalid = false;
    assert_eq!(type_keys(&telex(), "after{SPACE}"), "after ");
    assert_eq!(type_keys(&off, "after{SPACE}"), "àter ");
    assert_eq!(type_keys(&telex(), "windows{SPACE}"), "windows ");
    assert_eq!(type_keys(&off, "windows{SPACE}"), "ưindows ");
    // Valid words and words without transformations are never restored.
    assert_eq!(type_keys(&telex(), "tieengs{SPACE}"), "tiếng ");
    assert_eq!(type_keys(&telex(), "aaa{SPACE}"), "aa ");
    // Restore also happens at punctuation and Enter boundaries.
    assert_eq!(type_keys(&telex(), "after,"), "after,");
    let enter = run(&engine(), &telex(), "after{ENTER}");
    assert_eq!(enter.final_text, "after\r");
    assert_eq!(
        format_transaction(&enter.steps[5].transaction),
        "Tx(4,\"after\",true,Some(RETURN),true)"
    );
}

#[test]
fn auto_restore_applies_the_tone_coda_rule() {
    // The letters form a complete syllable, but c/ch/p/t codas take only sắc or nặng.
    for (keys, expected) in [
        ("tacf{SPACE}", "tacf "),
        ("tapx{SPACE}", "tapx "),
        ("sachr{SPACE}", "sachr "),
        ("hocf{SPACE}", "hocf "),
        ("vieetf{SPACE}", "vieetf "),
        ("tacs{SPACE}", "tác "),
        ("hocj{SPACE}", "học "),
        ("sachs{SPACE}", "sách "),
        ("tapj{SPACE}", "tạp "),
        ("vieetj{SPACE}", "việt "),
        ("tanf{SPACE}", "tàn "),
    ] {
        assert_eq!(type_keys(&telex(), keys), expected, "telex {keys}");
    }
    assert_eq!(type_keys(&vni(), "tac2{SPACE}"), "tac2 ");
    assert_eq!(type_keys(&vni(), "tac1{SPACE}"), "tác ");
    assert_eq!(type_keys(&viqr(), "tac`{SPACE}"), "tac` ");
    // Without auto-restore the rendered word is committed.
    let mut off = telex();
    off.auto_restore_invalid = false;
    assert_eq!(type_keys(&off, "tacf{SPACE}"), "tàc ");
    assert_eq!(type_keys(&off, "sachr{SPACE}"), "sảch ");
}

#[test]
fn disabled_mode_passes_characters_through() {
    let mut off = telex();
    off.enabled = false;
    let shown = txs(&engine(), &off, "aas{SPACE}dd{BS}");
    assert!(shown.iter().all(|tx| tx == PASS), "{shown:?}");
    assert_eq!(type_keys(&off, "aas{SPACE}dd{BS}"), "aas d");
    assert!(state_after(&off, "aas").is_idle());
}

#[test]
fn disabled_mode_tracks_words_only_for_macros() {
    let engine = engine_with(MacroDouble::new(&[("ko", "không"), ("vn", "Việt Nam")]));
    let mut off = telex();
    off.enabled = false;
    off.macros_when_disabled = true;
    assert_eq!(typed(&engine, &off, "ko{SPACE}vn."), "không Việt Nam.");
    // Tracked literally: no Vietnamese transformation, no restore.
    assert_eq!(typed(&engine, &off, "aas dd{SPACE}"), "aas dd ");
    // Semantic backspace works on the tracked word.
    assert_eq!(typed(&engine, &off, "kox{BS}{SPACE}"), "không ");
    let state = run(&engine, &off, "ko").final_state;
    assert_eq!(state.owned_utf16(), utf16::encode("ko").as_slice());
}

/// D25: the engine no longer passes secure-field keys through (the adapter does that in
/// `Bypass` mode, before the engine); it processes them as any other key, except that it
/// never expands a macro, which `Restricted` mode needs.
#[test]
fn secure_field_keys_are_processed_without_macros() {
    let host = script_host().expect("host");
    let engine = engine_with(MacroDouble::new(&[("tiếng", "tiếng Việt")]));
    let prior = state_after(&telex(), "tieeng");
    let mut secure = host.clone();
    secure.is_secure_field = true;
    for (event, expected) in [
        (
            InputEvent::character('s', secure.clone()),
            "Tx(3,\"ếng\",true,None,false)",
        ),
        (
            InputEvent::backspace(secure.clone()),
            "Tx(1,\"\",true,None,false)",
        ),
        (
            InputEvent::separator(' ', vk::SPACE, secure.clone()),
            PASS_RESET,
        ),
    ] {
        let result = engine.process(&event, &telex(), &prior).expect("processed");
        let plain = InputEvent {
            host: host.clone(),
            ..event.clone()
        };
        let outside = engine.process(&plain, &telex(), &prior).expect("processed");
        assert_eq!(format_transaction(&result.transaction), expected);
        if event.kind != InputKind::Separator {
            assert_eq!(result, outside, "{:?}", event.kind);
        }
    }
    let typed_tieengs = state_after(&telex(), "tieengs");
    let space = InputEvent::separator(' ', vk::SPACE, secure);
    let result = engine
        .process(&space, &telex(), &typed_tieengs)
        .expect("processed");
    assert_eq!(format_transaction(&result.transaction), PASS_RESET);
    let outside = InputEvent::separator(' ', vk::SPACE, host);
    let expanded = engine
        .process(&outside, &telex(), &typed_tieengs)
        .expect("processed");
    assert_eq!(
        format_transaction(&expanded.transaction),
        "Tx(5,\"tiếng Việt \",true,None,true)"
    );
}

#[test]
fn reset_events_pass_through_and_reset() {
    let engine = engine();
    let host = script_host().expect("host");
    let prior = state_after(&telex(), "tieeng");
    assert!(!prior.is_idle());
    for kind in [
        InputKind::Navigation,
        InputKind::MouseReset,
        InputKind::FocusReset,
        InputKind::ModeToggle,
        InputKind::ConfigChanged,
        InputKind::Shutdown,
    ] {
        let event = InputEvent::simple(kind, host.clone());
        let result = engine.process(&event, &telex(), &prior).expect("processed");
        assert_eq!(
            format_transaction(&result.transaction),
            PASS_RESET,
            "{kind:?}"
        );
        assert!(result.next.is_idle(), "{kind:?}");
        assert_eq!(result.next.word_form(), WordForm::Empty);
    }
    // After a navigation the next tone key does not touch the old word.
    assert_eq!(type_keys(&telex(), "tieeng{NAV}s"), "tiêngs");
    assert_eq!(type_keys(&telex(), "a{MOUSE}a"), "aa");
    assert_eq!(type_keys(&telex(), "a{FOCUS}a"), "aa");
}

#[test]
fn injected_events_are_ignored() {
    let engine = engine();
    let host = script_host().expect("host");
    let prior = state_after(&telex(), "a");
    let event = InputEvent::character('a', host).with_injected_tag(RAUMAKEY_INJECTED_TAG);
    let result = engine.process(&event, &telex(), &prior).expect("processed");
    assert_eq!(result.transaction, EditTransaction::pass_through());
    assert_eq!(result.next, prior, "state unchanged");
    // Another tag is processed normally.
    let other = InputEvent::character('a', script_host().expect("host")).with_injected_tag(7);
    let result = engine.process(&other, &telex(), &prior).expect("processed");
    assert_eq!(result.transaction, EditTransaction::replace(1, "â"));
}

#[test]
fn shortcuts_reset_and_pass_through() {
    assert_eq!(txs(&engine(), &telex(), "a{CTRL+a}"), [PASS, PASS_RESET]);
    let host = script_host().expect("host");
    let prior = state_after(&telex(), "aa");
    let mut ctrl_backspace = InputEvent::backspace(host);
    ctrl_backspace.modifiers.ctrl = true;
    let result = engine()
        .process(&ctrl_backspace, &telex(), &prior)
        .expect("processed");
    assert_eq!(format_transaction(&result.transaction), PASS_RESET);
    assert!(result.next.is_idle());
}

#[test]
fn altgr_is_not_a_shortcut() {
    assert_eq!(
        txs(&engine(), &telex(), "{ALTGR+a}{ALTGR+s}"),
        [PASS, "Tx(1,\"á\",true,None,false)"]
    );
    assert_eq!(type_keys(&viqr(), "{ALTGR+~}"), "~");
}

#[test]
fn multi_scalar_and_control_characters_commit_the_word() {
    let engine = engine();
    let host = script_host().expect("host");
    let prior = state_after(&telex(), "aa");
    let event = InputEvent::text(utf16::encode("xy"), host.clone());
    let result = engine.process(&event, &telex(), &prior).expect("processed");
    assert_eq!(format_transaction(&result.transaction), PASS_RESET);
    assert!(result.next.is_idle());
    let idle = engine
        .process(&event, &telex(), &EngineState::new())
        .expect("processed");
    assert_eq!(format_transaction(&idle.transaction), PASS);
    let control = InputEvent::character('\u{1b}', host);
    let result = engine
        .process(&control, &telex(), &prior)
        .expect("processed");
    assert_eq!(format_transaction(&result.transaction), PASS_RESET);
}

#[test]
fn whitespace_characters_are_boundaries() {
    assert_eq!(type_keys(&telex(), "after aa"), "after â");
    assert_eq!(type_keys(&telex(), "ddi\tddi"), "đi\tđi");
    // `â` alone is not a complete syllable: it is restored at the boundary.
    assert_eq!(type_keys(&telex(), "aa\taa"), "aa\tâ");
    let shown = txs(&engine(), &telex(), "after ");
    assert_eq!(shown[5], "Tx(4,\"after \",true,None,true)");
}

#[test]
fn overflow_drops_ownership_until_the_boundary() {
    let mut options = telex();
    options.context_limit_codepoints = 8;
    let outcome = run(&engine(), &options, "nghieengsss aa");
    let shown: Vec<String> = outcome
        .steps
        .iter()
        .map(|s| format_transaction(&s.transaction))
        .collect();
    assert_eq!(outcome.final_text, "nghiêngsss â");
    assert_eq!(shown[8], PASS_RESET, "the 9th key overflows");
    assert_eq!(shown[9], PASS);
    assert_eq!(shown[10], PASS);
    assert_eq!(shown[11], PASS, "boundary after overflow");
    let state = run(&engine(), &options, "nghieengss").final_state;
    assert!(state.is_idle());
    assert_eq!(state.word_form(), WordForm::NonVietnamese);
    // Backspace during overflow passes through.
    assert_eq!(type_keys(&options, "nghieengss{BS}{BS}s"), "nghiêngs");
    // Out-of-range limits are clamped into 8..=128.
    options.context_limit_codepoints = 0;
    let shown = txs(&engine(), &options, &"b".repeat(10));
    assert_eq!(shown[7], PASS);
    assert_eq!(shown[8], PASS_RESET);
    options.context_limit_codepoints = u32::MAX;
    let shown = txs(&engine(), &options, &"b".repeat(130));
    assert_eq!(shown[127], PASS);
    assert_eq!(shown[128], PASS_RESET);
    assert_eq!(shown[129], PASS);
    assert_eq!(type_keys(&options, &"a".repeat(200)), "a".repeat(199));
}

#[test]
fn history_capacity_constant() {
    assert_eq!(HISTORY_CAPACITY, 32);
    assert_eq!(STATE_VERSION, 1);
}

#[test]
fn user_defined_key_maps() {
    let map = KeyMap {
        id: "mine".to_owned(),
        version: KEYMAP_SCHEMA_VERSION,
        name: "Mine".to_owned(),
        bindings: vec![
            KeyBinding {
                key: 'q',
                actions: vec![KeyAction::Tone(Tone::Acute)],
            },
            KeyBinding {
                key: ';',
                actions: vec![KeyAction::CircumflexOn(BaseVowel::A), KeyAction::Breve],
            },
            KeyBinding {
                key: '=',
                actions: vec![KeyAction::Stroke],
            },
        ],
    };
    let engine = VietnameseEngine::new(vec![map], Arc::new(NoMacros));
    let options = EngineOptions {
        input_method: InputMethod::UserDefined,
        user_keymap_id: Some("mine".to_owned()),
        ..EngineOptions::defaults_v1()
    };
    assert_eq!(typed(&engine, &options, "a;q"), "ấ");
    assert_eq!(typed(&engine, &options, "d=a"), "đa");
    assert_eq!(typed(&engine, &options, "a;;"), "a;");
    assert_eq!(typed(&engine, &options, "as"), "as");
    assert!(VietnameseEngine::builtin_keymap(InputMethod::Telex).is_some());
    assert!(VietnameseEngine::builtin_keymap(InputMethod::UserDefined).is_none());
}

#[test]
fn generation_changes_only_with_state() {
    let engine = engine();
    let host = script_host().expect("host");
    let idle = EngineState::new();
    let space = InputEvent::separator(' ', vk::SPACE, host.clone());
    let unchanged = engine.process(&space, &telex(), &idle).expect("processed");
    assert_eq!(unchanged.next.generation(), idle.generation());
    let a = InputEvent::character('a', host);
    let first = engine.process(&a, &telex(), &idle).expect("processed");
    assert_ne!(first.next.generation(), idle.generation());
    let second = engine
        .process(&a, &telex(), &first.next)
        .expect("processed");
    assert_ne!(second.next.generation(), first.next.generation());
    let reset = engine.reset(&second.next, ResetReason::HardReset);
    assert!(reset.is_idle());
    assert_eq!(reset.version(), STATE_VERSION);
    assert_ne!(reset.generation(), second.next.generation());
    assert_eq!(EngineState::default(), EngineState::new());
}

#[test]
fn word_forms() {
    for (keys, form) in [
        ("", WordForm::Empty),
        ("ng", WordForm::C),
        ("a", WordForm::V),
        ("uwow", WordForm::V),
        ("hoa", WordForm::CV),
        ("anh", WordForm::VC),
        ("tieengs", WordForm::CVC),
        ("class", WordForm::NonVietnamese),
        ("a1", WordForm::NonVietnamese),
    ] {
        assert_eq!(state_after(&telex(), keys).word_form(), form, "{keys}");
    }
}

#[test]
fn error_invalid_utf16() {
    let host = script_host().expect("host");
    let event = InputEvent::text(vec![0xD800], host);
    assert_eq!(
        engine().process(&event, &telex(), &EngineState::new()),
        Err(EngineError::InvalidUtf16)
    );
}

#[test]
fn error_capacity_exceeded() {
    let host = script_host().expect("host");
    let event = InputEvent::text(vec![u16::from(b'a'); 17], host);
    assert_eq!(
        engine().process(&event, &telex(), &EngineState::new()),
        Err(EngineError::CapacityExceeded)
    );
}

#[test]
fn error_context_mismatch() {
    let prior = state_after(&telex(), "aa");
    let mut other_session = script_host().expect("host");
    other_session.session_id = SessionId::from_u128(99);
    let event = InputEvent::character('s', other_session);
    assert_eq!(
        engine().process(&event, &telex(), &prior),
        Err(EngineError::ContextMismatch)
    );
    let mut other_document = script_host().expect("host");
    other_document.document_id = Some(DocumentId::from_u128(5));
    let event = InputEvent::character('s', other_document.clone());
    assert_eq!(
        engine().process(&event, &telex(), &prior),
        Err(EngineError::ContextMismatch)
    );
    // An idle state may be used by any context.
    let idle = engine()
        .process(&event, &telex(), &EngineState::new())
        .expect("idle state is unowned");
    assert!(!idle.next.is_idle());
}

#[test]
fn error_unsupported_input_method() {
    let host = script_host().expect("host");
    let event = InputEvent::character('a', host);
    let mut options = EngineOptions {
        input_method: InputMethod::UserDefined,
        ..EngineOptions::defaults_v1()
    };
    assert_eq!(
        engine().process(&event, &options, &EngineState::new()),
        Err(EngineError::UnsupportedInputMethod),
        "missing key map id"
    );
    options.user_keymap_id = Some("unknown".to_owned());
    assert_eq!(
        engine().process(&event, &options, &EngineState::new()),
        Err(EngineError::UnsupportedInputMethod),
        "unknown key map id"
    );
}

#[test]
fn error_internal_invariant_for_malformed_events() {
    let host = script_host().expect("host");
    let mut event = InputEvent::navigation(vk::LEFT, host);
    event.text_utf16 = utf16::encode("x");
    assert_eq!(
        engine().process(&event, &telex(), &EngineState::new()),
        Err(EngineError::InternalInvariant)
    );
}

/// A second `w` that undoes a standalone `ư` (Telex `w`) away from the end of the word puts
/// back the key that typed it: the letter is never lost (`wiw`, not `iw`). Right after it, the
/// second `w` undoes it as before (`ww` is `w`).
#[test]
fn undoing_a_standalone_u_horn_puts_its_key_back() {
    let mut off = telex();
    off.spell_check = false;
    for options in [&telex(), &off] {
        for (keys, expected) in [("ww", "w"), ("wiw", "wiw"), ("Twiw", "Twiw")] {
            assert_eq!(type_keys(options, keys), expected, "{keys}");
        }
    }
    // With spell checking `ưe` is no syllable, so the `w` after it is typed as it is.
    assert_eq!(type_keys(&off, "wew"), "wew");
    assert_eq!(type_keys(&telex(), "wew"), "ưew");
}

/// D53: Simple Telex is Telex without the standalone `w`: `w` changes only a vowel before it
/// (`aw` → `ă`, `ow` → `ơ`, `uw` → `ư`), and stays `w` everywhere else, with or without
/// spell checking.
#[test]
fn d53_simple_telex_types_w_only_on_a_vowel() {
    let mut simple = telex();
    simple.input_method = InputMethod::SimpleTelex;
    let mut simple_off = simple.clone();
    simple_off.spell_check = false;
    for options in [&simple, &simple_off] {
        for (keys, expected) in [
            ("w", "w"),
            ("kw", "kw"),
            ("Windows", "Windows"),
            ("word{SPACE}", "word "),
            ("aw", "ă"),
            ("ow", "ơ"),
            ("uw", "ư"),
            ("tuwowngf", "tường"),
            ("nguwowif", "người"),
            ("dduwowngf", "đường"),
            ("hoawcj", "hoặc"),
            ("tieengs vieetj", "tiếng việt"),
        ] {
            assert_eq!(type_keys(options, keys), expected, "{keys}");
        }
    }
    // Telex keeps the standalone `w`.
    assert_eq!(type_keys(&telex(), "w"), "ư");
    assert_eq!(type_keys(&telex(), "tw"), "tư");
}
