//! Engine-level rows of the compatibility matrix (A1–A18, B21–B23, B29, D47–D54).

mod common;

use common::{
    MacroDouble, engine, engine_with, modern, nfd, run, telex, txs, type_keys, typed, viqr, vni,
};
use raumakey_engine::script::{self, KeyToken, format_transaction, script_host};
use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_model::input::{InputEvent, Modifiers, vk};
use raumakey_model::macros::{Boundary, CaseMode};
use raumakey_model::options::Normalization;
use raumakey_model::transaction::TextSimulator;
use raumakey_model::utf16;
use unicode_normalization::{UnicodeNormalization, is_nfc, is_nfd};

const PASS: &str = "Tx(0,\"\",false,None,false)";
const PASS_RESET: &str = "Tx(0,\"\",false,None,true)";

#[test]
fn a01_telex_double_a_makes_circumflex() {
    assert_eq!(
        txs(&engine(), &telex(), "aa"),
        [PASS, "Tx(1,\"â\",true,None,false)"]
    );
    assert_eq!(type_keys(&telex(), "aa"), "â");
}

#[test]
fn a02_telex_double_d_makes_stroke() {
    assert_eq!(
        txs(&engine(), &telex(), "dd"),
        [PASS, "Tx(1,\"đ\",true,None,false)"]
    );
    assert_eq!(type_keys(&telex(), "dd"), "đ");
}

#[test]
fn a03_telex_tone_key() {
    assert_eq!(
        txs(&engine(), &telex(), "as"),
        [PASS, "Tx(1,\"á\",true,None,false)"]
    );
}

#[test]
fn a04_a05_vni_marks_and_tones() {
    assert_eq!(
        txs(&engine(), &vni(), "a6"),
        [PASS, "Tx(1,\"â\",true,None,false)"]
    );
    assert_eq!(
        txs(&engine(), &vni(), "a1"),
        [PASS, "Tx(1,\"á\",true,None,false)"]
    );
}

#[test]
fn a06_a07_viqr_marks_and_tones() {
    assert_eq!(
        txs(&engine(), &viqr(), "a^"),
        [PASS, "Tx(1,\"â\",true,None,false)"]
    );
    assert_eq!(
        txs(&engine(), &viqr(), "a'"),
        [PASS, "Tx(1,\"á\",true,None,false)"]
    );
}

#[test]
fn a08_viqr_backslash_escapes_the_next_key() {
    let outcome = run(&engine(), &viqr(), "\\'");
    assert_eq!(outcome.steps.len(), 2, "exactly two keys");
    assert_eq!(outcome.final_text, "'");
    let shown: Vec<String> = outcome
        .steps
        .iter()
        .map(|s| format_transaction(&s.transaction))
        .collect();
    assert_eq!(shown, [PASS, "Tx(1,\"'\",true,None,true)"]);
    // The escaped key is literal: no tone lands on the previous word either.
    assert_eq!(type_keys(&viqr(), "a\\'"), "a'");
    assert_eq!(type_keys(&viqr(), "Nam\\.{SPACE}"), "Nam. ");
    // Escaped boundary punctuation ends the word (an escape boundary commits the word
    // unchanged), so the next word is typed as usual.
    assert_eq!(
        txs(&engine(), &viqr(), "a\\."),
        [PASS, PASS, "Tx(1,\".\",true,None,true)"]
    );
    for (keys, expected) in [
        ("\\'a'", "'á"),
        ("ba\\(", "ba("),
        ("\\\\", "\\"),
        // A toned word before an escaped period is kept, not restored to its keys.
        ("ho.c\\.{SPACE}", "học. "),
        ("Vie^.t\\?{SPACE}", "Việt? "),
        // Any other escaped key stays in the word as a literal, so the rest of the word
        // is literal too.
        ("\\^a'", "^a'"),
        ("a\\^", "a^"),
        ("a\\^{SPACE}", "a^ "),
        ("\\dd", "dd"),
        ("d\\dd", "ddd"),
        ("\\da'", "da'"),
        ("e\\-mail{SPACE}", "e-mail "),
        ("x\\*2", "x*2"),
        // An unbound key disarms the escape.
        ("\\b", "\\b"),
        ("\\ba'", "\\ba'"),
    ] {
        assert_eq!(type_keys(&viqr(), keys), expected, "viqr {keys}");
    }
}

#[test]
fn a09_inapplicable_actions_never_lose_keys() {
    for (options, keys) in [
        (telex(), "bs"),
        (telex(), "z"),
        (telex(), "bz"),
        (telex(), "pj"),
        (vni(), "b1"),
        (vni(), "b6"),
        (vni(), "e8"),
        (vni(), "i7"),
        (vni(), "b9"),
        (viqr(), "b'"),
        (viqr(), "e("),
        (viqr(), "b+"),
        (viqr(), "^"),
    ] {
        let outcome = run(&engine(), &options, keys);
        assert_eq!(outcome.final_text, keys, "{keys}");
        for step in &outcome.steps {
            let tx = &step.transaction;
            assert!(
                tx.is_pass_through() || tx.insert_string().ends_with(&step.token.to_string()),
                "{keys}: {}",
                format_transaction(tx)
            );
        }
    }
}

#[test]
fn a10_qu_onset_keeps_tone_off_the_u() {
    assert_eq!(
        txs(&engine(), &telex(), "quas"),
        [PASS, PASS, PASS, "Tx(1,\"á\",true,None,false)"]
    );
    assert_eq!(type_keys(&telex(), "quas"), "quá");
}

#[test]
fn a11_gi_onset() {
    assert_eq!(
        txs(&engine(), &telex(), "gias"),
        [PASS, PASS, PASS, "Tx(1,\"á\",true,None,false)"]
    );
    assert_eq!(type_keys(&telex(), "gias"), "giá");
}

#[test]
fn a12_tone_follows_style_and_coda() {
    // Old style: the tone moves from o to a when the coda arrives.
    assert_eq!(
        txs(&engine(), &telex(), "hoafn"),
        [
            PASS,
            PASS,
            PASS,
            "Tx(2,\"òa\",true,None,false)",
            "Tx(2,\"oàn\",true,None,false)"
        ]
    );
    // Modern style: the tone is already on a; the coda is a plain append.
    assert_eq!(
        txs(&engine(), &modern(telex()), "hoafn"),
        [PASS, PASS, PASS, "Tx(1,\"à\",true,None,false)", PASS]
    );
    for (keys, old, new) in [
        ("hoafng", "hoàng", "hoàng"),
        ("toanf", "toàn", "toàn"),
        ("khoern", "khoẻn", "khoẻn"),
        ("thuyr", "thủy", "thuỷ"),
        ("huyf", "hùy", "huỳ"),
        ("huyfnh", "huỳnh", "huỳnh"),
    ] {
        assert_eq!(type_keys(&telex(), keys), old, "old {keys}");
        assert_eq!(type_keys(&modern(telex()), keys), new, "modern {keys}");
    }
}

#[test]
fn a13_repeated_tone_undoes_and_keeps_the_key() {
    assert_eq!(
        txs(&engine(), &telex(), "ass"),
        [
            PASS,
            "Tx(1,\"á\",true,None,false)",
            "Tx(1,\"as\",true,None,false)"
        ]
    );
    assert_eq!(type_keys(&telex(), "ass"), "as");
}

#[test]
fn a14_repeated_mark_undoes_and_keeps_the_key() {
    assert_eq!(
        txs(&engine(), &telex(), "aaa"),
        [
            PASS,
            "Tx(1,\"â\",true,None,false)",
            "Tx(1,\"aa\",true,None,false)"
        ]
    );
    assert_eq!(type_keys(&telex(), "aaa"), "aa");
}

#[test]
fn a15_invalid_converted_word_is_restored_at_the_boundary() {
    let shown = txs(&engine(), &telex(), "after{SPACE}");
    assert_eq!(
        shown.last().map(String::as_str),
        Some("Tx(4,\"after \",true,None,true)")
    );
    assert_eq!(type_keys(&telex(), "after{SPACE}"), "after ");
}

#[test]
fn a16_no_restore_when_disabled() {
    let mut options = telex();
    options.auto_restore_invalid = false;
    let shown = txs(&engine(), &options, "after{SPACE}");
    assert_eq!(shown.last().map(String::as_str), Some(PASS_RESET));
    assert_eq!(type_keys(&options, "after{SPACE}"), "àter ");
}

#[test]
fn a17_semantic_backspace_restores_the_previous_state() {
    let outcome = run(&engine(), &telex(), "aa{BS}");
    assert_eq!(outcome.final_text, "a");
    assert_eq!(
        format_transaction(&outcome.steps[2].transaction),
        "Tx(1,\"a\",true,None,false)"
    );
    assert_eq!(
        outcome.final_state.owned_utf16(),
        utf16::encode("a").as_slice()
    );
    // Another backspace removes the last owned letter; then the engine is idle.
    let outcome = run(&engine(), &telex(), "aa{BS}{BS}{BS}");
    assert_eq!(outcome.final_text, "");
    assert!(outcome.final_state.is_idle());
}

#[test]
fn a18_history_is_bounded_and_falls_back_to_pass_through() {
    let mut options = telex();
    options.context_limit_codepoints = 64;
    let keys = "ddaawbchklmnpqtvbchklmnpqtvbchklm";
    assert_eq!(keys.chars().count(), 33);
    let engine = engine();
    // Text after each of the 33 keys.
    let mut after_key = Vec::new();
    for count in 1..=33 {
        let prefix: String = keys.chars().take(count).collect();
        after_key.push(typed(&engine, &options, &prefix));
    }
    let script: String = format!("{keys}{}", "{BS}".repeat(35));
    let outcome = run(&engine, &options, &script);
    assert!(outcome.steps.iter().all(|step| step.accepted));
    let mut sim = TextSimulator::new();
    for (index, step) in outcome.steps.iter().enumerate() {
        sim.apply(&step.event, &step.transaction).expect("applies");
        if index < 33 {
            assert_eq!(sim.as_string(), after_key[index], "key {index}");
            continue;
        }
        let backspace = index - 32; // 1-based
        let tx = format_transaction(&step.transaction);
        if backspace <= 32 {
            // Restores the snapshot taken before key 33 - backspace + 1.
            assert!(step.transaction.consume_original, "bs {backspace}: {tx}");
            assert!(!step.transaction.reset_context, "bs {backspace}: {tx}");
            assert_eq!(sim.as_string(), after_key[32 - backspace], "bs {backspace}");
        } else if backspace == 33 {
            // 32 snapshots used: the oldest (empty word) was evicted.
            assert_eq!(tx, PASS_RESET, "bs {backspace}");
            assert_eq!(sim.as_string(), "");
        } else {
            assert_eq!(tx, PASS, "bs {backspace}");
        }
    }
    assert!(outcome.final_state.is_idle());
}

#[test]
fn b21_shortcuts_bypass_the_engine_and_reset() {
    let outcome = run(&engine(), &telex(), "a{CTRL+c}a");
    let shown: Vec<String> = outcome
        .steps
        .iter()
        .map(|s| format_transaction(&s.transaction))
        .collect();
    assert_eq!(shown, [PASS, PASS_RESET, PASS]);
    assert_eq!(
        outcome.final_text, "aa",
        "no circumflex across the shortcut"
    );
    // A shortcut that carries text is not consumed either.
    let host = script_host().expect("host");
    let mut event = InputEvent::character('s', host);
    event.modifiers.alt = true;
    let after_a = run(&engine(), &telex(), "a").final_state;
    let result = engine()
        .process(&event, &telex(), &after_a)
        .expect("processed");
    assert_eq!(format_transaction(&result.transaction), PASS_RESET);
    assert!(result.next.is_idle());
}

#[test]
fn b21_shortcut_separators_bypass_macros_and_restore() {
    let macros = MacroDouble::new(&[("ko", "không")]);
    let engine = engine_with(macros.clone());
    let host = script_host().expect("host");
    let ctrl = Modifiers {
        ctrl: true,
        ..Modifiers::default()
    };
    let alt = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    let win = Modifiers {
        win: true,
        ..Modifiers::default()
    };
    let hard_reset_chord = Modifiers {
        ctrl: true,
        shift: true,
        alt: true,
        ..Modifiers::default()
    };
    for (keys, separator, virtual_key, modifiers) in [
        // A pending macro trigger: Ctrl+Space (IDE completion), Alt+Enter.
        ("ko", ' ', vk::SPACE, ctrl),
        ("ko", '\r', vk::RETURN, alt),
        ("ko", '\t', vk::TAB, ctrl),
        // A word that the boundary would restore.
        ("after", ' ', vk::SPACE, ctrl),
        ("after", '\r', vk::RETURN, alt),
        ("after", ' ', vk::SPACE, win),
        ("after", ' ', vk::SPACE, alt),
        ("after", ' ', vk::SPACE, hard_reset_chord),
    ] {
        let before = run(&engine, &telex(), keys);
        let event =
            InputEvent::separator(separator, virtual_key, host.clone()).with_modifiers(modifiers);
        let result = engine
            .process(&event, &telex(), &before.final_state)
            .expect("processed");
        assert_eq!(
            format_transaction(&result.transaction),
            PASS_RESET,
            "{keys} + {modifiers:?} {virtual_key:#x}"
        );
        assert!(result.next.is_idle());
        // The document keeps the word; only the key's own effect is added.
        let mut sim = TextSimulator::with_text(&before.final_text);
        sim.apply(&event, &result.transaction).expect("applies");
        assert_eq!(sim.as_string(), format!("{}{separator}", before.final_text));
    }
    assert!(
        macros.calls().is_empty(),
        "a shortcut never looks up macros"
    );
    // Scripted: the next word starts fresh after the shortcut.
    assert_eq!(
        txs(&engine, &telex(), "ko{CTRL+SPACE}a"),
        [PASS, PASS, PASS_RESET, PASS]
    );
    assert_eq!(typed(&engine, &telex(), "ko{CTRL+SPACE}"), "ko ");
    assert_eq!(typed(&engine, &telex(), "after{CTRL+ENTER}"), "àter\r");
    let mut off = telex();
    off.enabled = false;
    off.macros_when_disabled = true;
    assert_eq!(typed(&engine, &off, "ko{CTRL+SPACE}"), "ko ");
    assert_eq!(typed(&engine, &off, "ko{CTRL+TAB}"), "ko\t");
    assert!(macros.calls().is_empty());
}

#[test]
fn b22_altgr_characters_are_typed_once() {
    let outcome = run(&engine(), &telex(), "a{ALTGR+@}");
    assert_eq!(outcome.final_text, "a@");
    assert_eq!(
        format_transaction(&outcome.steps[1].transaction),
        PASS,
        "not treated as a shortcut"
    );
    assert!(!outcome.final_state.is_idle(), "the word continues");
    // AltGr characters go through the Vietnamese engine like any character.
    assert_eq!(type_keys(&telex(), "{ALTGR+a}{ALTGR+a}"), "â");
    assert_eq!(type_keys(&telex(), "{ALTGR+€}"), "€");
}

#[test]
fn b23_case_comes_from_the_typed_text() {
    assert_eq!(type_keys(&telex(), "VIEETJ"), "VIỆT");
    assert_eq!(type_keys(&telex(), "Vieetj"), "Việt");
    assert_eq!(type_keys(&telex(), "vIEETJ"), "vIỆT");
    assert_eq!(type_keys(&telex(), "DDAAY"), "ĐÂY");
    assert_eq!(type_keys(&telex(), "DDaay"), "Đây");
    // Caps Lock is reported separately and never applied twice.
    let host = script_host().expect("host");
    let engine = engine();
    let mut state = EngineState::new();
    let mut sim = TextSimulator::new();
    for c in ['A', 'A', 'S'] {
        let event = InputEvent::character(c, host.clone()).with_modifiers(Modifiers {
            caps_lock: true,
            ..Modifiers::default()
        });
        let result = engine.process(&event, &telex(), &state).expect("processed");
        sim.apply(&event, &result.transaction).expect("applies");
        state = result.next;
    }
    assert_eq!(sim.as_string(), "Ấ");
}

/// Matrix #29 under D25: whether a secure field is transformed at all is the adapter's call
/// (the policy's secure-field mode; the default `Bypass` passes every key through before the
/// engine). The engine itself never expands a macro, and never looks one up, for an event in
/// a secure field; otherwise it processes the event normally, as `Restricted` mode needs.
#[test]
fn b29_secure_fields_never_expand_macros() {
    let macros = MacroDouble::new(&[("ko", "không"), ("v.v", "vân vân")]);
    let outcome = run(
        &engine_with(macros.clone()),
        &telex(),
        "{SECURE}vieetj ko{SPACE}v.v{SPACE}{BS}{/SECURE}aa",
    );
    assert_eq!(outcome.final_text, "việt ko v.vâ");
    let shown: Vec<String> = outcome
        .steps
        .iter()
        .map(|step| format_transaction(&step.transaction))
        .collect();
    assert_eq!(
        shown,
        [
            PASS_RESET,
            PASS,
            PASS,
            PASS,
            "Tx(1,\"ê\",true,None,false)",
            PASS,
            "Tx(2,\"ệt\",true,None,false)",
            PASS_RESET,
            PASS,
            PASS,
            PASS_RESET,
            PASS,
            PASS_RESET,
            PASS,
            PASS_RESET,
            PASS,
            PASS_RESET,
            PASS,
            "Tx(1,\"â\",true,None,false)",
        ]
    );
    assert!(
        outcome.steps[..16]
            .iter()
            .all(|step| step.event.host.is_secure_field || step.token == KeyToken::SecureOn)
    );
    assert!(
        macros.calls().is_empty() && macros.prefix_calls().is_empty(),
        "no macro lookup in a secure field"
    );
    // The same keys outside a secure field expand both macros.
    assert_eq!(
        typed(&engine_with(macros), &telex(), "vieetj ko{SPACE}v.v{SPACE}"),
        "việt không vân vân "
    );
}

#[test]
fn d47_macro_expands_at_a_boundary() {
    let macros = MacroDouble::new(&[("ko", "không")]);
    let engine = engine_with(macros);
    let shown = txs(&engine, &telex(), "ko{SPACE}");
    assert_eq!(shown, [PASS, PASS, "Tx(2,\"không \",true,None,true)"]);
    assert_eq!(typed(&engine, &telex(), "ko{SPACE}"), "không ");
    let macros = MacroDouble::new(&[("ko", "không")]);
    assert_eq!(
        typed(&engine_with(macros.clone()), &telex(), "ko,"),
        "không,"
    );
    assert_eq!(
        macros.calls(),
        [("ko".to_owned(), Boundary::Punctuation, CaseMode::Adaptive)]
    );
    let enter = run(&engine, &telex(), "ko{ENTER}");
    assert_eq!(
        format_transaction(&enter.steps[2].transaction),
        "Tx(2,\"không\",true,Some(RETURN),true)"
    );
    assert_eq!(enter.final_text, "không\r");
    let tab = run(&engine, &telex(), "ko{TAB}");
    assert_eq!(
        format_transaction(&tab.steps[2].transaction),
        "Tx(2,\"không\",true,Some(TAB),true)"
    );
    assert_eq!(tab.final_text, "không\t");
}

#[test]
fn d48_trigger_inside_a_word_does_not_expand() {
    let engine = engine_with(MacroDouble::new(&[("ko", "không")]));
    assert_eq!(typed(&engine, &telex(), "xko{SPACE}"), "xko ");
    assert_eq!(typed(&engine, &telex(), "kon{SPACE}"), "kon ");
    assert_eq!(typed(&engine, &telex(), "kok"), "kok");
}

#[test]
fn d49_shift_space_and_escape_commit_without_expansion() {
    let macros = MacroDouble::new(&[("ko", "không")]);
    let engine = engine_with(macros.clone());
    let shown = txs(&engine, &telex(), "ko{SHIFT+SPACE}");
    assert_eq!(shown, [PASS, PASS, PASS_RESET]);
    assert_eq!(typed(&engine, &telex(), "ko{SHIFT+SPACE}"), "ko ");
    // Shift+Space also skips the auto-restore.
    assert_eq!(type_keys(&telex(), "after{SHIFT+SPACE}"), "àter ");
    // A VIQR escaped boundary commits the word literally.
    assert_eq!(typed(&engine, &viqr(), "ko\\."), "ko.");
    assert!(macros.calls().is_empty());
}

#[test]
fn d50_macro_case_follows_the_trigger() {
    let macros = MacroDouble::new(&[("ko", "không")]);
    let engine = engine_with(macros.clone());
    assert_eq!(typed(&engine, &telex(), "ko{SPACE}"), "không ");
    assert_eq!(typed(&engine, &telex(), "Ko{SPACE}"), "Không ");
    assert_eq!(typed(&engine, &telex(), "KO{SPACE}"), "KHÔNG ");
    assert!(
        macros
            .calls()
            .iter()
            .all(|(_, _, mode)| *mode == CaseMode::Adaptive)
    );
    let mut exact = telex();
    exact.auto_case_macro = false;
    assert_eq!(typed(&engine, &exact, "KO{SPACE}"), "KO ");
    assert_eq!(
        macros.calls().last().map(|(_, _, mode)| *mode),
        Some(CaseMode::Exact)
    );
}

#[test]
fn d51_macros_while_vietnamese_is_off() {
    let engine = engine_with(MacroDouble::new(&[("ko", "không")]));
    let mut off = telex();
    off.enabled = false;
    assert_eq!(typed(&engine, &off, "aas ko{SPACE}"), "aas ko ");
    off.macros_when_disabled = true;
    assert_eq!(typed(&engine, &off, "aas ko{SPACE}"), "aas không ");
    let shown = txs(&engine, &off, "ko{SPACE}");
    assert_eq!(shown, [PASS, PASS, "Tx(2,\"không \",true,None,true)"]);
    off.macros_enabled = false;
    assert_eq!(typed(&engine, &off, "ko{SPACE}"), "ko ");
}

#[test]
fn d51_d53_disabled_mode_macros_use_the_output_normalization() {
    let engine = engine_with(MacroDouble::new(&[("ko", "không")]));
    let mut off = nfd(telex());
    off.enabled = false;
    off.macros_when_disabled = true;
    // The same expansion as in Vietnamese mode: NFD output, counted in UTF-16 units.
    let outcome = run(&engine, &off, "ko{SPACE}");
    assert_eq!(outcome.final_text, "kho\u{302}ng ");
    assert!(is_nfd(&outcome.final_text));
    assert_eq!(
        format_transaction(&outcome.steps[2].transaction),
        "Tx(2,\"kho\u{302}ng \",true,None,true)"
    );
    let enter = run(&engine, &off, "ko{ENTER}");
    assert_eq!(
        format_transaction(&enter.steps[2].transaction),
        "Tx(2,\"kho\u{302}ng\",true,Some(RETURN),true)"
    );
    let mut on = nfd(telex());
    on.macros_when_disabled = true;
    assert_eq!(typed(&engine, &on, "ko{SPACE}"), outcome.final_text);
    // NFC and Preserve insert the stored (NFC) replacement.
    for normalization in [Normalization::Nfc, Normalization::Preserve] {
        off.normalization = normalization;
        let text = typed(&engine, &off, "ko{SPACE}");
        assert_eq!(text, "không ", "{normalization:?}");
        assert!(is_nfc(&text));
    }
    // The tracked word itself is never normalized: typed combining marks stay as typed.
    off.normalization = Normalization::Nfd;
    assert_eq!(typed(&engine, &off, "o\u{302}{SPACE}"), "o\u{302} ");
    off.normalization = Normalization::Nfc;
    assert_eq!(typed(&engine, &off, "o\u{302}{SPACE}"), "o\u{302} ");
}

#[test]
fn d52_nfd_input_matches_nfc_trigger() {
    let macros = MacroDouble::new(&[("â", "âm"), ("tiếng", "tiếng Việt")]);
    let engine = engine_with(macros.clone());
    // `a` followed by a typed combining circumflex.
    let outcome = run(&engine, &telex(), "a\u{302}{SPACE}");
    assert_eq!(outcome.final_text, "âm ");
    assert!(is_nfc(&outcome.final_text));
    assert_eq!(macros.calls()[0].0, "â");
    // NFD output mode still looks up the NFC word and emits NFD.
    let outcome = run(&engine, &nfd(telex()), "tieengs{SPACE}");
    assert_eq!(outcome.final_text.nfc().collect::<String>(), "tiếng Việt ");
    assert!(is_nfd(&outcome.final_text));
    assert!(macros.calls().iter().any(|(typed, _, _)| typed == "tiếng"));
}

#[test]
fn d53_nfd_output_is_canonically_equivalent_and_counts_utf16_units() {
    for keys in [
        "tieengs",
        "vieetj",
        "nguwowif",
        "Ddaay",
        "VIEETJ",
        "thuowr",
        "aa{BS}",
        "ass",
        "hoafn",
        "after{SPACE}",
    ] {
        let nfc_text = type_keys(&telex(), keys);
        let nfd_text = type_keys(&nfd(telex()), keys);
        assert!(is_nfd(&nfd_text), "{keys}");
        assert_eq!(nfd_text.nfc().collect::<String>(), nfc_text, "{keys}");
    }
    let shown = txs(&engine(), &nfd(telex()), "tieengs");
    // `tiêng` = t i e U+0302 n g: the tone replaces the 4 units after `ti` (never splitting the
    // base `e` from its combining marks) with `e` U+0302 U+0301 `n` `g`.
    assert_eq!(shown[3], "Tx(1,\"e\u{302}\",true,None,false)");
    assert_eq!(shown[6], "Tx(4,\"e\u{302}\u{301}ng\",true,None,false)");
    let mut preserve = telex();
    preserve.normalization = Normalization::Preserve;
    assert_eq!(type_keys(&preserve, "tieengs"), "tiếng");
}

#[test]
fn d54_supplementary_scalars_are_never_split() {
    let host = script_host().expect("host");
    let engine = engine();
    let tokens = script::parse_keys("aa{BS}{BS}{BS}").expect("valid");
    let outcome = script::run_script_with(&engine, &telex(), &tokens, &host, "😀").expect("runs");
    assert_eq!(outcome.final_text, "");
    assert!(outcome.steps.iter().all(|s| s.accepted));
    // The engine deletes only what it owns; the emoji goes with the final pass-through backspace.
    assert_eq!(
        format_transaction(&outcome.steps[3].transaction),
        "Tx(1,\"\",true,None,false)"
    );
    assert_eq!(format_transaction(&outcome.steps[4].transaction), PASS);

    let outcome = run(&engine, &telex(), "😀{SPACE}tieengs");
    assert_eq!(outcome.final_text, "😀 tiếng");
    let mut off = telex();
    off.spell_check = false;
    let outcome = run(&engine, &off, "a😀aa");
    assert!(outcome.steps.iter().all(|s| s.accepted));
    assert!(utf16::is_well_formed(&utf16::encode(&outcome.final_text)));
    for step in &outcome.steps {
        assert!(utf16::is_well_formed(&step.transaction.insert_utf16));
    }
    let outcome = run(&engine, &telex(), "😀aa{BS}{BS}");
    assert_eq!(outcome.final_text, "😀");
    assert_eq!(
        format_transaction(&outcome.steps[4].transaction),
        "Tx(1,\"\",true,None,false)"
    );
}

#[test]
fn every_matrix_script_keeps_owned_text_as_document_suffix() {
    let engine: VietnameseEngine = engine_with(MacroDouble::new(&[("ko", "không")]));
    for keys in [
        "aa",
        "dd",
        "as",
        "quas",
        "gias",
        "hoafn",
        "ass",
        "aaa",
        "after{SPACE}",
        "aa{BS}",
        "ko{SPACE}",
        "a{CTRL+c}a",
        "{SECURE}aa{/SECURE}aa",
        "😀aa{BS}",
    ] {
        let tokens = script::parse_keys(keys).expect("valid");
        let host = script_host().expect("host");
        let mut state = EngineState::new();
        let mut sim = TextSimulator::new();
        let mut secure = false;
        for token in &tokens {
            secure = match token {
                KeyToken::SecureOn => true,
                KeyToken::SecureOff => false,
                _ => secure,
            };
            let event = script::event_for(token, &host, secure);
            let result = engine.process(&event, &telex(), &state).expect("processed");
            sim.apply(&event, &result.transaction).expect("applies");
            state = result.next;
            assert!(sim.units().ends_with(state.owned_utf16()), "{keys}");
        }
    }
}
