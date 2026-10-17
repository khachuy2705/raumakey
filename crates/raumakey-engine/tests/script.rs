//! The platform-independent `script` module: parsing, events, and the simulated run loop.

mod common;

use common::{engine, run, telex};
use proptest::prelude::*;
use raumakey_engine::script::{
    self, KeyToken, ScriptError, event_for, format_transaction, parse_keys, script_host,
};
use raumakey_engine::{EngineState, STATE_VERSION};
use raumakey_model::error::EngineError;
use raumakey_model::input::{InputKind, vk};
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_model::transaction::EditTransaction;
use raumakey_model::utf16;

#[test]
fn parses_plain_characters_and_tokens() {
    let tokens = parse_keys("aB {BS}{SP}{SPACE}{SHIFT+SPACE}{ENTER}{TAB}{NAV}{MOUSE}{FOCUS}")
        .expect("valid");
    assert_eq!(
        tokens,
        vec![
            KeyToken::Char('a'),
            KeyToken::Char('B'),
            KeyToken::Char(' '),
            KeyToken::Backspace,
            KeyToken::Space,
            KeyToken::Space,
            KeyToken::ShiftSpace,
            KeyToken::Enter,
            KeyToken::Tab,
            KeyToken::Navigation,
            KeyToken::Mouse,
            KeyToken::Focus,
        ]
    );
    let tokens =
        parse_keys("{CTRL+c}{ALTGR+@}{SECURE}x{/SECURE}{LBRACE}{RBRACE}ệ😀").expect("valid");
    assert_eq!(
        tokens,
        vec![
            KeyToken::Ctrl('c'),
            KeyToken::AltGr('@'),
            KeyToken::SecureOn,
            KeyToken::Char('x'),
            KeyToken::SecureOff,
            KeyToken::Char('{'),
            KeyToken::Char('}'),
            KeyToken::Char('ệ'),
            KeyToken::Char('😀'),
        ]
    );
    assert_eq!(
        parse_keys("{bs}{Enter}"),
        Ok(vec![KeyToken::Backspace, KeyToken::Enter])
    );
    assert_eq!(parse_keys(""), Ok(Vec::new()));
}

#[test]
fn rejects_malformed_scripts() {
    assert_eq!(
        parse_keys("ab{BS"),
        Err(ScriptError::Unterminated { offset: 2 })
    );
    assert_eq!(
        parse_keys("{NOPE}"),
        Err(ScriptError::UnknownToken { offset: 0 })
    );
    assert_eq!(
        parse_keys("x{CTRL+}"),
        Err(ScriptError::InvalidChord { offset: 1 })
    );
    assert_eq!(
        parse_keys("{ALTGR+ab}"),
        Err(ScriptError::InvalidChord { offset: 0 })
    );
    assert!(
        !ScriptError::UnknownToken { offset: 0 }
            .to_string()
            .is_empty()
    );
}

#[test]
fn tokens_display_in_script_form_and_round_trip() {
    let script = "aZ {BS}{SPACE}{SHIFT+SPACE}{ENTER}{TAB}{NAV}{MOUSE}{FOCUS}{CTRL+c}{ALTGR+€}{SECURE}{/SECURE}{LBRACE}{RBRACE}";
    let tokens = parse_keys(script).expect("valid");
    let shown: String = tokens.iter().map(ToString::to_string).collect();
    assert_eq!(shown, script);
    assert_eq!(parse_keys(&shown), Ok(tokens));
}

#[test]
fn chords_name_braces_and_separator_keys() {
    let script =
        "{CTRL+LBRACE}{CTRL+RBRACE}{ALTGR+LBRACE}{ALTGR+RBRACE}{CTRL+SPACE}{CTRL+ENTER}{CTRL+TAB}";
    let tokens = parse_keys(script).expect("valid");
    assert_eq!(
        tokens,
        vec![
            KeyToken::Ctrl('{'),
            KeyToken::Ctrl('}'),
            KeyToken::AltGr('{'),
            KeyToken::AltGr('}'),
            KeyToken::Ctrl(' '),
            KeyToken::Ctrl('\r'),
            KeyToken::Ctrl('\t'),
        ]
    );
    let shown: String = tokens.iter().map(ToString::to_string).collect();
    assert_eq!(shown, script);
    assert_eq!(parse_keys(&shown), Ok(tokens));
    // Key names ignore ASCII case; a single character is always itself.
    assert_eq!(
        parse_keys("{ctrl+space}{Ctrl+Return}{CTRL+S}"),
        Ok(vec![
            KeyToken::Ctrl(' '),
            KeyToken::Ctrl('\r'),
            KeyToken::Ctrl('S')
        ])
    );
    assert_eq!(
        parse_keys("{CTRL+NOPE}"),
        Err(ScriptError::InvalidChord { offset: 0 })
    );
}

#[test]
fn ctrl_separators_are_separator_events_with_ctrl() {
    let host = script_host().expect("host");
    for (token, text, key) in [
        (KeyToken::Ctrl(' '), " ", vk::SPACE),
        (KeyToken::Ctrl('\r'), "\r", vk::RETURN),
        (KeyToken::Ctrl('\t'), "\t", vk::TAB),
    ] {
        let event = event_for(&token, &host, false);
        assert_eq!(event.kind, InputKind::Separator, "{token}");
        assert_eq!(event.text_utf16, utf16::encode(text), "{token}");
        assert_eq!(event.virtual_key, Some(key), "{token}");
        assert!(event.modifiers.ctrl && event.is_shortcut(), "{token}");
        assert!(event.validate().is_ok(), "{token}");
    }
}

#[test]
fn run_script_from_resets_when_the_simulator_rejects_a_transaction() {
    let engine = engine();
    let host = script_host().expect("host");
    // The engine believes it owns `â`, but the document is empty (the host changed it).
    let stale = run(&engine, &telex(), "aa").final_state;
    let tokens = parse_keys("as").expect("valid");
    let outcome =
        script::run_script_from(&engine, &telex(), &tokens, &host, "", stale).expect("script runs");
    let shown: Vec<String> = outcome
        .steps
        .iter()
        .map(|step| format_transaction(&step.transaction))
        .collect();
    // `a` undoes the circumflex: the deletion exceeds the empty document and is rejected.
    assert_eq!(
        shown,
        [
            "Tx(1,\"aa\",true,None,false)",
            "Tx(0,\"\",false,None,false)"
        ]
    );
    assert!(!outcome.steps[0].accepted, "rejected");
    assert!(
        outcome.steps[1].accepted,
        "the next key starts from a reset state"
    );
    // The rejected key still passes through; nothing of the stale word is touched.
    assert_eq!(outcome.final_text, "as");
    assert_eq!(
        outcome.final_state.owned_utf16(),
        utf16::encode("s").as_slice()
    );

    // A deletion that would split a surrogate pair is rejected as well.
    let stale = run(&engine, &telex(), "a").final_state;
    let tokens = parse_keys("a").expect("valid");
    let outcome = script::run_script_from(&engine, &telex(), &tokens, &host, "😀", stale)
        .expect("script runs");
    assert_eq!(
        format_transaction(&outcome.steps[0].transaction),
        "Tx(1,\"â\",true,None,false)"
    );
    assert!(!outcome.steps[0].accepted);
    assert_eq!(outcome.final_text, "😀a");
    assert!(outcome.final_state.is_idle());
}

#[test]
fn events_have_the_documented_shape() {
    let host = script_host().expect("host");
    let upper = event_for(&KeyToken::Char('A'), &host, false);
    assert_eq!(upper.kind, InputKind::Character);
    assert_eq!(upper.text_utf16, utf16::encode("A"));
    assert!(upper.modifiers.shift);
    assert!(
        !event_for(&KeyToken::Char('a'), &host, false)
            .modifiers
            .shift
    );
    for (token, text, key) in [
        (KeyToken::Space, " ", vk::SPACE),
        (KeyToken::Enter, "\r", vk::RETURN),
        (KeyToken::Tab, "\t", vk::TAB),
    ] {
        let event = event_for(&token, &host, false);
        assert_eq!(event.kind, InputKind::Separator);
        assert_eq!(event.text_utf16, utf16::encode(text));
        assert_eq!(event.virtual_key, Some(key));
        assert!(!event.modifiers.shift);
    }
    let shift_space = event_for(&KeyToken::ShiftSpace, &host, false);
    assert_eq!(shift_space.kind, InputKind::Separator);
    assert!(shift_space.modifiers.shift);
    let ctrl = event_for(&KeyToken::Ctrl('c'), &host, false);
    assert!(ctrl.modifiers.ctrl && ctrl.is_shortcut());
    assert_eq!(ctrl.kind, InputKind::Character);
    assert!(ctrl.text_utf16.is_empty(), "a shortcut types no text");
    assert_eq!(ctrl.virtual_key, Some(u32::from('C')));
    let alt_gr = event_for(&KeyToken::AltGr('@'), &host, false);
    assert!(alt_gr.modifiers.ctrl && alt_gr.modifiers.alt && alt_gr.modifiers.alt_gr);
    assert!(!alt_gr.is_shortcut());
    assert_eq!(
        event_for(&KeyToken::Backspace, &host, false).kind,
        InputKind::Backspace
    );
    assert_eq!(
        event_for(&KeyToken::Navigation, &host, false).kind,
        InputKind::Navigation
    );
    assert_eq!(
        event_for(&KeyToken::Mouse, &host, false).kind,
        InputKind::MouseReset
    );
    assert_eq!(
        event_for(&KeyToken::Focus, &host, false).kind,
        InputKind::FocusReset
    );
    let secure_char = event_for(&KeyToken::Char('a'), &host, true);
    assert!(secure_char.host.is_secure_field);
    let on = event_for(&KeyToken::SecureOn, &host, false);
    assert_eq!(on.kind, InputKind::FocusReset);
    assert!(on.host.is_secure_field);
    let off = event_for(&KeyToken::SecureOff, &host, true);
    assert_eq!(off.kind, InputKind::FocusReset);
    assert!(!off.host.is_secure_field);
    for token in parse_keys("a{BS}{SPACE}{ENTER}{NAV}{CTRL+v}{ALTGR+x}").expect("valid") {
        assert!(event_for(&token, &host, false).validate().is_ok());
    }
}

#[test]
fn transactions_format_in_matrix_notation() {
    assert_eq!(
        format_transaction(&EditTransaction::pass_through()),
        "Tx(0,\"\",false,None,false)"
    );
    assert_eq!(
        format_transaction(&EditTransaction::replace(1, "â")),
        "Tx(1,\"â\",true,None,false)"
    );
    let mut tx = EditTransaction::replace(2, "a\"\\\r\t\n");
    tx.reset_context = true;
    assert_eq!(
        format_transaction(&tx),
        "Tx(2,\"a\\\"\\\\\\r\\t\\n\",true,None,true)"
    );
}

#[test]
fn run_script_applies_transactions_to_the_simulator() {
    let outcome = run(&engine(), &telex(), "aa");
    assert_eq!(outcome.final_text, "â");
    assert_eq!(outcome.steps.len(), 2);
    assert!(outcome.steps.iter().all(|step| step.accepted));
    assert_eq!(outcome.steps[0].token, KeyToken::Char('a'));
    assert_eq!(
        outcome.final_state.owned_utf16(),
        utf16::encode("â").as_slice()
    );
    assert_eq!(outcome.final_state.version(), STATE_VERSION);

    let committed = run(&engine(), &telex(), "vieetj{SPACE}");
    assert_eq!(committed.final_text, "việt ");
    assert!(committed.final_state.is_idle());
}

#[test]
fn run_script_reports_engine_errors_with_the_step() {
    let options = EngineOptions {
        input_method: InputMethod::UserDefined,
        user_keymap_id: Some("missing".to_owned()),
        ..EngineOptions::defaults_v1()
    };
    let tokens = parse_keys("{SPACE}a").expect("valid");
    let error = script::run_script(&engine(), &options, &tokens).expect_err("unsupported");
    assert_eq!(error.step, 1);
    assert_eq!(error.error, EngineError::UnsupportedInputMethod);
    assert!(!error.to_string().is_empty());
}

#[test]
fn run_script_with_prefilled_text_and_custom_host() {
    let host = script_host().expect("host");
    let tokens = parse_keys("aa").expect("valid");
    let outcome =
        script::run_script_with(&engine(), &telex(), &tokens, &host, "😀").expect("script runs");
    assert_eq!(outcome.final_text, "😀â");
    assert_eq!(EngineState::new().generation(), 0);
}

fn any_token() -> impl Strategy<Value = KeyToken> {
    prop_oneof![
        4 => any::<char>().prop_map(KeyToken::Char),
        2 => any::<char>().prop_map(KeyToken::Ctrl),
        2 => any::<char>().prop_map(KeyToken::AltGr),
        1 => prop::sample::select(vec!['{', '}', ' ', '\r', '\t', '+']).prop_map(KeyToken::Ctrl),
        1 => prop::sample::select(vec!['{', '}', ' ', '\r', '\t', '+']).prop_map(KeyToken::AltGr),
        1 => prop::sample::select(vec![
            KeyToken::Backspace,
            KeyToken::Space,
            KeyToken::ShiftSpace,
            KeyToken::Enter,
            KeyToken::Tab,
            KeyToken::Navigation,
            KeyToken::Mouse,
            KeyToken::Focus,
            KeyToken::SecureOn,
            KeyToken::SecureOff,
        ]),
    ]
}

proptest! {
    #[test]
    fn every_token_round_trips_through_its_script_form(
        tokens in prop::collection::vec(any_token(), 0..24),
    ) {
        let shown: String = tokens.iter().map(ToString::to_string).collect();
        prop_assert_eq!(parse_keys(&shown), Ok(tokens));
    }
}
