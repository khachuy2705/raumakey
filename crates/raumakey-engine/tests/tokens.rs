//! Macro triggers across punctuation (plan D28, §3.8): a whitespace boundary also looks up
//! the token typed since the previous whitespace, including the boundary punctuation that
//! committed words inside it, and replaces the whole token.
//!
//! The engine tracks a token only while macros are active, the lookup has triggers with
//! boundary punctuation, and the text committed so far starts such a trigger. It then owns the
//! whole token's rendered text, so an adapter can check every unit it deletes. Without such
//! triggers, punctuation commits words exactly as before.

mod common;

use std::sync::Arc;

use common::{MacroDouble, engine_with, nfd, run, telex, txs, typed, viqr, vni};
use raumakey_engine::script::{format_transaction, script_host};
use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_model::input::{InputEvent, InputKind, vk};
use raumakey_model::macros::{Boundary, CaseMode};
use raumakey_model::options::EngineOptions;
use raumakey_model::utf16;
use unicode_normalization::UnicodeNormalization;

const PASS: &str = "Tx(0,\"\",false,None,false)";
const PASS_RESET: &str = "Tx(0,\"\",false,None,true)";

/// The engine with the token trigger `v.v`.
fn vv() -> VietnameseEngine {
    engine_with(MacroDouble::new(&[("v.v", "vân vân")]))
}

fn owned(state: &EngineState) -> String {
    String::from_utf16_lossy(state.owned_utf16())
}

#[test]
fn d28_a_trigger_with_punctuation_expands_at_whitespace_in_every_method() {
    let engine = vv();
    for options in [telex(), vni(), viqr()] {
        assert_eq!(
            txs(&engine, &options, "v.v{SPACE}"),
            [PASS, PASS, PASS, "Tx(3,\"vân vân \",true,None,true)"],
            "{:?}",
            options.input_method
        );
    }
    assert_eq!(
        txs(&engine, &telex(), "v.v{ENTER}")[3],
        "Tx(3,\"vân vân\",true,Some(RETURN),true)"
    );
    assert_eq!(typed(&engine, &telex(), "v.v{TAB}"), "vân vân\t");
    // The whole token is owned while it is typed; the boundary ends it.
    assert_eq!(owned(&run(&engine, &telex(), "v.").final_state), "v.");
    assert_eq!(owned(&run(&engine, &telex(), "v.v").final_state), "v.v");
    assert!(run(&engine, &telex(), "v.v{SPACE}").final_state.is_idle());
    // Words before and after the token are unaffected.
    assert_eq!(
        typed(&engine, &telex(), "xin v.v nhes{SPACE}"),
        "xin vân vân nhé "
    );
}

#[test]
fn d28_the_token_is_looked_up_as_rendered_then_as_raw_keys() {
    let macros = MacroDouble::new(&[("v.v", "vân vân")]);
    let engine = engine_with(Arc::clone(&macros));
    typed(&engine, &telex(), "v.v{SPACE}");
    let calls = macros.calls();
    assert_eq!(
        calls,
        [
            // At the dot: the word `v` (plan §3.8), which does not expand.
            ("v".to_owned(), Boundary::Punctuation, CaseMode::Adaptive),
            // At the space: the token first, which expands.
            ("v.v".to_owned(), Boundary::Whitespace, CaseMode::Adaptive),
        ]
    );
    // A token typed with transformations matches as rendered or as raw keys.
    let rendered = engine_with(MacroDouble::new(&[("ê.v", "e mũ chấm v")]));
    assert_eq!(typed(&rendered, &telex(), "ee.v{SPACE}"), "e mũ chấm v ");
    let raw = engine_with(MacroDouble::new(&[("ee.v", "hai e chấm v")]));
    assert_eq!(typed(&raw, &telex(), "ee.v{SPACE}"), "hai e chấm v ");
    // The deletion covers the rendered token in the output normalization: `ê.v` is four
    // units in NFD.
    let nfd_run = txs(&rendered, &nfd(telex()), "ee.v{SPACE}");
    let insert: String = "e mũ chấm v ".nfd().collect();
    assert_eq!(
        nfd_run[4],
        format!(
            "Tx(4,{},true,None,true)",
            raumakey_engine::script::quote(&insert)
        )
    );
}

#[test]
fn d28_only_a_whole_token_matches() {
    let engine = vv();
    for (keys, expected) in [
        ("v.x{SPACE}", "v.x "),
        // A trigger inside a token never expands (matrix #48).
        ("x.v.v{SPACE}", "x.v.v "),
        ("v.v.v{SPACE}", "v.v.v "),
        ("v.vx{SPACE}", "v.vx "),
        ("xv.v{SPACE}", "xv.v "),
        // Punctuation does not end a token, so it expands nothing.
        ("v.v,", "v.v,"),
        ("v.v.{SPACE}", "v.v. "),
        // Shift+Space commits unchanged (D7).
        ("v.v{SHIFT+SPACE}", "v.v "),
    ] {
        assert_eq!(typed(&engine, &telex(), keys), expected, "{keys}");
    }
    // A token that can no longer match is not owned, and the next whitespace starts a new one.
    assert!(run(&engine, &telex(), "v.x.").final_state.is_idle());
    assert_eq!(
        typed(&engine, &telex(), "x.v.v v.v{SPACE}"),
        "x.v.v vân vân "
    );
}

#[test]
fn d28_without_token_triggers_punctuation_commits_words_as_before() {
    // Triggers without boundary punctuation: no token is tracked, and the transactions are
    // those of an engine without macros.
    let engine = engine_with(MacroDouble::new(&[("ko", "không")]));
    let plain = common::engine();
    for keys in ["v.v{SPACE}", "a.b,c{SPACE}", "..{SPACE}", "v.{BS}{BS}"] {
        assert_eq!(
            txs(&engine, &telex(), keys),
            txs(&plain, &telex(), keys),
            "{keys}"
        );
    }
    assert_eq!(
        txs(&engine, &telex(), "v.v{SPACE}"),
        [PASS, PASS_RESET, PASS, PASS_RESET]
    );
    assert_eq!(
        run(&engine, &telex(), "v.").final_state,
        run(&plain, &telex(), "v.").final_state
    );
}

#[test]
fn d28_a_token_can_start_with_punctuation_and_end_with_it() {
    let engine = engine_with(MacroDouble::new(&[("...", "…"), ("v.", "về")]));
    assert_eq!(typed(&engine, &telex(), "...{SPACE}"), "… ");
    assert_eq!(typed(&engine, &telex(), "a ...{SPACE}"), "a … ");
    assert_eq!(typed(&engine, &telex(), "v.{SPACE}"), "về ");
    assert_eq!(typed(&engine, &telex(), "v..{SPACE}"), "v.. ");
}

#[test]
fn d28_reset_events_end_the_token() {
    let engine = vv();
    for reset in ["{NAV}", "{MOUSE}", "{FOCUS}", "{CTRL+c}", "{SECURE}"] {
        let keys = format!("v.{reset}v{{SPACE}}");
        let outcome = run(&engine, &telex(), &keys);
        assert_eq!(outcome.final_text, "v.v ", "{keys}");
        assert_eq!(
            format_transaction(&outcome.steps[2].transaction),
            PASS_RESET,
            "{keys}"
        );
    }
    let host = script_host().expect("host");
    let prior = run(&engine, &telex(), "v.").final_state;
    assert!(!prior.is_idle());
    for kind in [
        InputKind::ModeToggle,
        InputKind::ConfigChanged,
        InputKind::Shutdown,
    ] {
        let event = InputEvent::simple(kind, host.clone());
        let result = engine.process(&event, &telex(), &prior).expect("processed");
        assert_eq!(format_transaction(&result.transaction), PASS_RESET);
        assert!(result.next.is_idle(), "{kind:?}");
    }
    // An option change the engine is not told about (macros turned off) ends the token too.
    let mut off = telex();
    off.macros_enabled = false;
    let v = InputEvent::character('v', host);
    let result = engine.process(&v, &off, &prior).expect("processed");
    assert_eq!(format_transaction(&result.transaction), PASS_RESET);
    assert!(result.next.is_idle());
}

#[test]
fn d28_a_secure_field_key_never_continues_a_token() {
    let engine = vv();
    let host = script_host().expect("host");
    let prior = run(&engine, &telex(), "v.").final_state;
    let mut secure = host.clone();
    secure.is_secure_field = true;
    let result = engine
        .process(&InputEvent::character('v', secure), &telex(), &prior)
        .expect("processed");
    assert_eq!(format_transaction(&result.transaction), PASS_RESET);
    assert!(result.next.is_idle());
}

#[test]
fn d28_backspace_inside_the_current_word_keeps_the_token() {
    let engine = vv();
    assert_eq!(typed(&engine, &telex(), "v.x{BS}v{SPACE}"), "vân vân ");
    let back = run(&engine, &telex(), "v.v{BS}");
    assert_eq!(
        format_transaction(&back.steps[3].transaction),
        "Tx(1,\"\",true,None,false)"
    );
    assert_eq!(owned(&back.final_state), "v.");
    assert_eq!(typed(&engine, &telex(), "v.v{BS}v{SPACE}"), "vân vân ");
}

/// A Backspace over the punctuation at the end of a tracked token's committed part restores
/// the state before that punctuation: the word before it, with its own history, and the
/// token around it. The engine owns that punctuation, so it deletes it itself.
#[test]
fn d28_backspace_over_a_tracked_tokens_punctuation_restores_the_word_before_it() {
    let engine = vv();
    let back = run(&engine, &telex(), "v.{BS}");
    assert_eq!(
        format_transaction(&back.steps[2].transaction),
        "Tx(1,\"\",true,None,false)"
    );
    assert_eq!(back.final_text, "v");
    assert_eq!(owned(&back.final_state), "v");
    assert_eq!(typed(&engine, &telex(), "v.{BS}.v{SPACE}"), "vân vân ");
    assert_eq!(
        typed(&engine, &telex(), "v.v{BS}{BS}{BS}v.v{SPACE}"),
        "vân vân "
    );
    // The word before the punctuation is still the current word: `vv.v` is not `v.v`.
    assert_eq!(typed(&engine, &telex(), "v.{BS}v.v{SPACE}"), "vv.v ");
    // The restored word keeps its history, so semantic backspace goes on inside it.
    let marked = engine_with(MacroDouble::new(&[("ô.v", "ô chấm v")]));
    let restored = run(&marked, &telex(), "oo.{BS}{BS}");
    assert_eq!(restored.final_text, "o");
    assert_eq!(
        format_transaction(&restored.steps[4].transaction),
        "Tx(1,\"o\",true,None,false)"
    );
    // Nested tokens unwind one punctuation at a time.
    let nested = engine_with(MacroDouble::new(&[("a.b.c", "abc")]));
    assert_eq!(typed(&nested, &telex(), "a.b.{BS}.c{SPACE}"), "abc ");
    assert_eq!(
        typed(&nested, &telex(), "a.b.{BS}{BS}{BS}.b.c{SPACE}"),
        "abc "
    );
    // A token that starts with punctuation unwinds to the idle state before it.
    let dots = engine_with(MacroDouble::new(&[("...", "…")]));
    let first = run(&dots, &telex(), ".{BS}");
    assert_eq!(
        format_transaction(&first.steps[1].transaction),
        "Tx(1,\"\",true,None,false)"
    );
    assert!(first.final_state.is_idle());
    assert_eq!(typed(&dots, &telex(), "..{BS}..{SPACE}"), "… ");
}

/// A limitation of D28: punctuation that ended a token which could no longer match was never
/// owned, so a Backspace over it restores nothing. The token stays lost until the next
/// whitespace or reset, and nothing is edited.
#[test]
fn d28_backspace_over_the_punctuation_of_a_lost_token_restores_nothing() {
    let engine = vv();
    let lost = run(&engine, &telex(), "v,{BS}.v{SPACE}");
    assert_eq!(lost.final_text, "v.v ");
    assert!(
        lost.steps
            .iter()
            .all(|step| step.transaction.is_pass_through()),
        "nothing is edited"
    );
    assert_eq!(
        format_transaction(&lost.steps[2].transaction),
        PASS,
        "the Backspace reaches the application"
    );
    assert_eq!(
        typed(&engine, &telex(), "v,{BS}.v v.v{SPACE}"),
        "v.v vân vân "
    );
}

/// Auto-restore inside a tracked token replaces only the current word, never the committed
/// part of the token (`v.` stays as typed).
#[test]
fn d28_auto_restore_inside_a_token_replaces_only_the_word() {
    let engine = vv();
    let space = run(&engine, &telex(), "v.ddk{SPACE}");
    assert_eq!(
        format_transaction(&space.steps[5].transaction),
        "Tx(2,\"ddk \",true,None,true)"
    );
    assert_eq!(space.final_text, "v.ddk ");
    let comma = run(&engine, &telex(), "v.ddk,");
    assert_eq!(
        format_transaction(&comma.steps[5].transaction),
        "Tx(2,\"ddk,\",true,None,true)"
    );
    assert_eq!(comma.final_text, "v.ddk,");
}

/// Punctuation with no word before it, outside a token, passes through without a reset: the
/// engine owned nothing, so there is nothing to end (as plan §3.8 has it without tokens).
#[test]
fn d28_punctuation_without_a_word_passes_through_without_a_reset() {
    let engine = vv();
    assert_eq!(txs(&engine, &telex(), ".,"), [PASS, PASS]);
    assert_eq!(txs(&engine, &telex(), "x.,"), [PASS, PASS_RESET, PASS]);
}

#[test]
fn d28_the_viqr_dot_is_an_action_key_that_ends_a_word_without_a_vowel() {
    // VIQR binds `.` to the dot tone. After `v` there is no vowel, so the dot is boundary
    // punctuation and the token goes on; after a vowel it is a tone, and the word's raw keys
    // match as before (plan §3.8).
    let engine = engine_with(MacroDouble::new(&[("v.v", "vân vân"), ("a.b", "a chấm b")]));
    assert_eq!(typed(&engine, &viqr(), "v.v{SPACE}"), "vân vân ");
    assert_eq!(typed(&engine, &viqr(), "a.{SPACE}"), "ạ ");
    assert_eq!(typed(&engine, &viqr(), "a.b{SPACE}"), "a chấm b ");
    assert_eq!(typed(&engine, &telex(), "a.b{SPACE}"), "a chấm b ");
    // An escaped dot commits the word unchanged and ends the token (matrix #49), and no token
    // starts again before the next whitespace, so `v.v` at the end of `v.v.v` is still inside
    // a longer token (matrix #48).
    assert_eq!(typed(&engine, &viqr(), "v\\.v{SPACE}"), "v.v ");
    assert_eq!(typed(&engine, &viqr(), "v\\.v.v{SPACE}"), "v.v.v ");
}

#[test]
fn d28_the_replacement_follows_the_case_of_the_typed_token() {
    let engine = vv();
    assert_eq!(typed(&engine, &telex(), "V.V{SPACE}"), "VÂN VÂN ");
    assert_eq!(typed(&engine, &telex(), "V.v{SPACE}"), "Vân vân ");
    let mut exact = telex();
    exact.auto_case_macro = false;
    assert_eq!(typed(&engine, &exact, "V.V{SPACE}"), "V.V ");
    assert_eq!(typed(&engine, &exact, "v.v{SPACE}"), "vân vân ");
}

#[test]
fn d28_tokens_expand_while_vietnamese_is_off_when_macros_are_on() {
    let engine = vv();
    let mut off = telex();
    off.enabled = false;
    off.macros_when_disabled = true;
    assert_eq!(
        txs(&engine, &off, "v.v{SPACE}"),
        [PASS, PASS, PASS, "Tx(3,\"vân vân \",true,None,true)"]
    );
    off.macros_when_disabled = false;
    assert_eq!(typed(&engine, &off, "v.v{SPACE}"), "v.v ");
    let mut on = telex();
    on.macros_enabled = false;
    assert_eq!(typed(&engine, &on, "v.v{SPACE}"), "v.v ");
}

#[test]
fn d28_a_replacement_at_punctuation_ends_the_token() {
    // The word-level macro `v` expands at the dot (plan §3.8), so the typed token is gone:
    // the token macro no longer matches, and the last word expands on its own.
    let engine = engine_with(MacroDouble::new(&[("v", "với"), ("v.v", "vân vân")]));
    assert_eq!(typed(&engine, &telex(), "v.v{SPACE}"), "với.với ");
}

#[test]
fn d28_ownership_is_bounded_by_the_trigger() {
    let engine = engine_with(MacroDouble::new(&[("a.b.c", "abc")]));
    // While the committed text starts the trigger, the token is owned …
    assert_eq!(owned(&run(&engine, &telex(), "a.b.").final_state), "a.b.");
    assert_eq!(owned(&run(&engine, &telex(), "a.b.c").final_state), "a.b.c");
    // … and once it cannot match any more, nothing is.
    assert!(run(&engine, &telex(), "a.b.c.").final_state.is_idle());
    assert!(run(&engine, &telex(), "a.b.c.d.").final_state.is_idle());
    assert!(run(&engine, &telex(), "a.x.").final_state.is_idle());
    assert_eq!(typed(&engine, &telex(), "a.b.c{SPACE}"), "abc ");
    // A word that overflows the context limit drops the token with it.
    let short = EngineOptions {
        context_limit_codepoints: EngineOptions::MIN_CONTEXT,
        charset: Default::default(),
        ..telex()
    };
    let overflow = run(&engine, &short, "a.b.cccccccccc");
    assert!(overflow.final_state.is_idle());
    assert_eq!(
        typed(&engine, &short, "a.b.cccccccccc{SPACE}"),
        "a.b.cccccccccc "
    );
}

#[test]
fn d28_the_token_needs_the_same_owner() {
    // The engine owns the token for its document only (matrix #20).
    let engine = vv();
    let prior = run(&engine, &telex(), "v.").final_state;
    let mut other = script_host().expect("host");
    other.document_id = Some(raumakey_model::host::DocumentId::from_u128(9));
    let event = InputEvent::separator(' ', vk::SPACE, other);
    assert_eq!(
        engine.process(&event, &telex(), &prior),
        Err(raumakey_model::error::EngineError::ContextMismatch)
    );
    assert!(utf16::is_well_formed(prior.owned_utf16()));
}
