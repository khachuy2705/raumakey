//! Contract suite: `VietnameseEngine` with the real `raumakey_macro::MacroTable` (plan §3.8,
//! §4; matrix #47–#54).
//!
//! The engine's own tests use a lookup double that adapts case differently, has no case or
//! boundary policies, and does not renormalize to NFC. Here the engine and the table run
//! together, so a disagreement between them fails a test. Every script is run through an
//! adapter loop that checks the contract at each step: the transaction validates and applies,
//! no insert exceeds `MAX_INSERT_UNITS`, the document stays well-formed UTF-16, and the
//! engine's word stays the document's suffix.

use std::sync::Arc;

use raumakey_engine::script::{self, KeyToken};
use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_macro::{EntryError, MacroError, MacroTable};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};
use raumakey_model::transaction::{MAX_INSERT_UNITS, TextSimulator};
use raumakey_model::utf16;
use unicode_normalization::{UnicodeNormalization, is_nfc, is_nfd};

const PASS: &str = "Tx(0,\"\",false,None,false)";
const PASS_RESET: &str = "Tx(0,\"\",false,None,true)";

/// An enabled entry with the default policies (delimiter boundary, adaptive case).
fn entry(id: u64, trigger: &str, replacement: &str) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    }
}

/// The engine with a real table built from `entries`.
fn engine(entries: Vec<MacroEntry>) -> VietnameseEngine {
    let table = MacroTable::new(entries).expect("the test entries are valid");
    VietnameseEngine::new(Vec::new(), Arc::new(table))
}

fn options(method: InputMethod) -> EngineOptions {
    EngineOptions {
        input_method: method,
        ..EngineOptions::defaults_v1()
    }
}

fn telex() -> EngineOptions {
    options(InputMethod::Telex)
}

fn nfd(mut options: EngineOptions) -> EngineOptions {
    options.normalization = Normalization::Nfd;
    options
}

/// The final document, the transactions in matrix notation, and the final state.
struct Run {
    text: String,
    txs: Vec<String>,
    state: EngineState,
}

/// Runs a key script through an adapter loop that checks the contract at every step.
fn run(engine: &VietnameseEngine, options: &EngineOptions, keys: &str) -> Run {
    let tokens = script::parse_keys(keys).expect("valid key script");
    let host = script::script_host().expect("valid host");
    let mut state = EngineState::new();
    let mut document = TextSimulator::new();
    let mut secure = false;
    let mut txs = Vec::new();
    for token in &tokens {
        secure = match token {
            KeyToken::SecureOn => true,
            KeyToken::SecureOff => false,
            _ => secure,
        };
        let event = script::event_for(token, &host, secure);
        let result = engine
            .process(&event, options, &state)
            .expect("the engine processes every scripted key");
        let tx = &result.transaction;
        assert!(
            tx.insert_utf16.len() <= MAX_INSERT_UNITS,
            "{keys}: an insert exceeds MAX_INSERT_UNITS"
        );
        document
            .apply(&event, tx)
            .expect("every transaction applies to the document");
        assert!(
            utf16::is_well_formed(document.units()),
            "{keys}: ill-formed"
        );
        assert!(
            document.units().ends_with(result.next.owned_utf16()),
            "{keys}: the owned word is not the document suffix"
        );
        txs.push(script::format_transaction(tx));
        state = result.next;
    }
    Run {
        text: document.as_string(),
        txs,
        state,
    }
}

fn typed(engine: &VietnameseEngine, options: &EngineOptions, keys: &str) -> String {
    run(engine, options, keys).text
}

/// `Tx(delete,"insert",true,None,true)`: a replacement followed by a printable delimiter.
fn expansion(delete: u32, insert: &str) -> String {
    format!("Tx({delete},{},true,None,true)", script::quote(insert))
}

#[test]
fn d47_a_trigger_and_space_become_the_replacement_and_the_space() {
    let engine = engine(vec![entry(1, "ko", "không")]);
    let outcome = run(&engine, &telex(), "ko{SPACE}");
    assert_eq!(outcome.txs, [PASS, PASS, &expansion(2, "không ")]);
    assert_eq!(outcome.text, "không ");
    assert!(outcome.state.is_idle(), "the boundary ends the word");
    for method in [InputMethod::Vni, InputMethod::Viqr] {
        assert_eq!(typed(&engine, &options(method), "ko{SPACE}"), "không ");
    }
    // Typed space characters and the next word work the same way.
    assert_eq!(typed(&engine, &telex(), "ko ko{SPACE}"), "không không ");
}

#[test]
fn d47_punctuation_enter_and_tab_are_boundaries_too() {
    let engine = engine(vec![entry(1, "ko", "không")]);
    let comma = run(&engine, &telex(), "ko,");
    assert_eq!(comma.txs[2], expansion(2, "không,"));
    assert_eq!(comma.text, "không,");
    assert_eq!(typed(&engine, &telex(), "ko."), "không.");
    let enter = run(&engine, &telex(), "ko{ENTER}");
    assert_eq!(enter.txs[2], "Tx(2,\"không\",true,Some(RETURN),true)");
    assert_eq!(enter.text, "không\r");
    let tab = run(&engine, &telex(), "ko{TAB}");
    assert_eq!(tab.txs[2], "Tx(2,\"không\",true,Some(TAB),true)");
    assert_eq!(tab.text, "không\t");
}

#[test]
fn d47_the_rendered_word_is_looked_up_before_the_raw_keys() {
    // Plan §3.8: the rendered NFC word first, then the raw keys when they differ.
    let both = engine(vec![entry(1, "đi", "đi đâu"), entry(2, "ddi", "raw keys")]);
    assert_eq!(typed(&both, &telex(), "ddi{SPACE}"), "đi đâu ");
    let raw_only = engine(vec![entry(1, "aa", "chữ â")]);
    assert_eq!(typed(&raw_only, &telex(), "aa{SPACE}"), "chữ â ");
    // Without a macro the invalid transformed word `đc` is restored to its raw keys (#15).
    assert_eq!(typed(&raw_only, &telex(), "ddc{SPACE}"), "ddc ");
    let stroke = engine(vec![entry(1, "đc", "được")]);
    assert_eq!(typed(&stroke, &telex(), "ddc{SPACE}"), "được ");
    // A macro wins over the restore of an invalid transformed word (matrix #15).
    let after = engine(vec![entry(1, "after", "sau đó")]);
    assert_eq!(typed(&after, &telex(), "after{SPACE}"), "sau đó ");
    let unrelated = engine(vec![entry(1, "ko", "không")]);
    assert_eq!(typed(&unrelated, &telex(), "after{SPACE}"), "after ");
}

#[test]
fn d48_a_trigger_inside_a_word_never_expands() {
    let engine = engine(vec![entry(1, "ko", "không")]);
    assert_eq!(typed(&engine, &telex(), "xko{SPACE}"), "xko ");
    assert_eq!(typed(&engine, &telex(), "kon{SPACE}"), "kon ");
    assert_eq!(typed(&engine, &telex(), "kok"), "kok");
    assert_eq!(typed(&engine, &telex(), "ko"), "ko");
    assert_eq!(typed(&engine, &telex(), "ko.ko"), "không.ko");
}

#[test]
fn d49_shift_space_escapes_shortcuts_and_secure_fields_commit_the_trigger() {
    let engine = engine(vec![entry(1, "ko", "không")]);
    let shift_space = run(&engine, &telex(), "ko{SHIFT+SPACE}");
    assert_eq!(shift_space.txs, [PASS, PASS, PASS_RESET]);
    assert_eq!(shift_space.text, "ko ");
    assert_eq!(typed(&engine, &options(InputMethod::Viqr), "ko\\."), "ko.");
    assert_eq!(typed(&engine, &telex(), "ko{CTRL+SPACE}"), "ko ");
    assert_eq!(typed(&engine, &telex(), "{SECURE}ko{SPACE}"), "ko ");
}

/// D25 with the real table: the engine never expands a macro, word or token, for a key in a
/// secure field; whether the field is transformed at all is the adapter's call (the policy's
/// secure-field mode, `Bypass` by default).
#[test]
fn d25_no_macro_expands_for_a_key_in_a_secure_field() {
    let engine = engine(vec![entry(1, "ko", "không"), entry(2, "v.v", "vân vân")]);
    assert_eq!(
        typed(
            &engine,
            &telex(),
            "{SECURE}vieetj ko v.v{SPACE}{/SECURE}ko v.v{SPACE}"
        ),
        "việt ko v.v không vân vân "
    );
}

#[test]
fn d50_the_replacement_follows_the_case_of_the_typed_trigger() {
    let engine = engine(vec![
        entry(1, "ko", "không"),
        entry(2, "sdt", "số điện thoại"),
    ]);
    assert_eq!(typed(&engine, &telex(), "ko{SPACE}"), "không ");
    assert_eq!(typed(&engine, &telex(), "Ko{SPACE}"), "Không ");
    assert_eq!(typed(&engine, &telex(), "KO{SPACE}"), "KHÔNG ");
    assert_eq!(typed(&engine, &telex(), "Sdt{SPACE}"), "Số điện thoại ");
    assert_eq!(typed(&engine, &telex(), "SDT{SPACE}"), "SỐ ĐIỆN THOẠI ");
    // Without automatic case only the exact trigger matches.
    let mut exact = telex();
    exact.auto_case_macro = false;
    assert_eq!(typed(&engine, &exact, "KO{SPACE}"), "KO ");
    assert_eq!(typed(&engine, &exact, "ko{SPACE}"), "không ");
}

#[test]
fn d50_mixed_case_keeps_the_stored_replacement() {
    // Plan §4: "anything else → as stored". The engine's double would title-case these.
    let engine = engine(vec![
        entry(1, "tphcm", "thành phố Hồ Chí Minh"),
        entry(2, "koo", "không"),
        entry(3, "ko", "không"),
    ]);
    assert_eq!(
        typed(&engine, &telex(), "TpHcm{SPACE}"),
        "thành phố Hồ Chí Minh "
    );
    assert_eq!(typed(&engine, &telex(), "KoO{SPACE}"), "không ");
    assert_eq!(typed(&engine, &telex(), "kO{SPACE}"), "không ");
    assert_eq!(
        typed(&engine, &telex(), "Tphcm{SPACE}"),
        "Thành phố Hồ Chí Minh "
    );
    assert_eq!(
        typed(&engine, &telex(), "TPHCM{SPACE}"),
        "THÀNH PHỐ HỒ CHÍ MINH "
    );
}

/// Pins `MacroTable` rule 5, a code decision that design §20.9 lists and that the plan does
/// not record yet (design §20.6 item 10, plan §14.5 item 8): only a case difference between
/// the typed and the stored trigger adapts the replacement, so a trigger typed exactly as
/// stored inserts the stored text.
///
/// This departs from the wording of plan §4, under which the case pattern of the typed trigger
/// alone decides: `UBND` typed as stored (every cased letter upper, more than one letter) would
/// give `ỦY BAN NHÂN DÂN`, and `VN` would give `VIETNAM`. When the decision is recorded as a
/// plan decision, cite it here; if the plan keeps its §4 wording instead, this test changes
/// with the table.
#[test]
fn unrecorded_decision_a_trigger_typed_as_stored_is_not_case_adapted() {
    let engine = engine(vec![
        entry(1, "UBND", "Ủy ban nhân dân"),
        entry(2, "vn", "Việt Nam"),
        entry(3, "VN", "Vietnam"),
    ]);
    assert_eq!(typed(&engine, &telex(), "UBND{SPACE}"), "Ủy ban nhân dân ");
    assert_eq!(typed(&engine, &telex(), "ubnd{SPACE}"), "Ủy ban nhân dân ");
    assert_eq!(typed(&engine, &telex(), "Ubnd{SPACE}"), "Ủy ban nhân dân ");
    // Case-specific entries each insert their own replacement; a third spelling takes the
    // lowest identifier among the case-insensitive matches.
    assert_eq!(typed(&engine, &telex(), "VN{SPACE}"), "Vietnam ");
    assert_eq!(typed(&engine, &telex(), "vn{SPACE}"), "Việt Nam ");
    assert_eq!(typed(&engine, &telex(), "Vn{SPACE}"), "Việt Nam ");
}

#[test]
fn case_policy_exact_entries_insert_the_stored_text_in_any_case() {
    let mut vn = entry(1, "vn", "Việt Nam");
    vn.case_policy = CasePolicy::Exact;
    let engine = engine(vec![vn]);
    for keys in ["vn{SPACE}", "Vn{SPACE}", "VN{SPACE}"] {
        assert_eq!(typed(&engine, &telex(), keys), "Việt Nam ", "{keys}");
    }
    let mut exact = telex();
    exact.auto_case_macro = false;
    assert_eq!(typed(&engine, &exact, "VN{SPACE}"), "VN ");
}

#[test]
fn whitespace_only_entries_expand_at_whitespace_and_never_at_punctuation() {
    let mut message = entry(1, "tm", "tin nhắn");
    message.boundary = BoundaryPolicy::Whitespace;
    let engine_whitespace = engine(vec![message.clone()]);
    assert_eq!(
        typed(&engine_whitespace, &telex(), "tm{SPACE}"),
        "tin nhắn "
    );
    assert_eq!(
        typed(&engine_whitespace, &telex(), "tm{ENTER}"),
        "tin nhắn\r"
    );
    assert_eq!(typed(&engine_whitespace, &telex(), "tm{TAB}"), "tin nhắn\t");
    for keys in ["tm,", "tm.", "tm!", "tm;"] {
        assert_eq!(
            typed(&engine_whitespace, &telex(), keys),
            keys,
            "{keys} must not expand"
        );
    }
    // At punctuation the first entry whose policy allows it wins, even with a higher id.
    let both = engine(vec![message, entry(2, "tm", "thời mới")]);
    assert_eq!(typed(&both, &telex(), "tm,"), "thời mới,");
    assert_eq!(typed(&both, &telex(), "tm{SPACE}"), "tin nhắn ");
}

#[test]
fn d52_nfd_input_matches_the_nfc_trigger_and_the_output_is_nfc() {
    let engine = engine(vec![
        entry(1, "â", "âm"),
        entry(2, "tiếng", "tiếng Việt"),
        // Stored decomposed: the table keeps the NFC form.
        entry(3, "vie\u{302}\u{323}t", "Việt Nam"),
    ]);
    for (keys, expected) in [
        ("a\u{302}{SPACE}", "âm "),
        ("tie\u{302}\u{301}ng{SPACE}", "tiếng Việt "),
        ("tieengs{SPACE}", "tiếng Việt "),
        ("vieetj{SPACE}", "Việt Nam "),
        ("vie\u{323}\u{302}t{SPACE}", "Việt Nam "),
    ] {
        let text = typed(&engine, &telex(), keys);
        assert_eq!(text, expected, "{keys}");
        assert!(is_nfc(&text), "{keys}");
    }
    // The same while Vietnamese mode is off.
    let mut off = telex();
    off.enabled = false;
    off.macros_when_disabled = true;
    assert_eq!(typed(&engine, &off, "a\u{302}{SPACE}"), "âm ");
}

#[test]
fn d53_nfd_output_is_canonically_equivalent_and_deletes_utf16_units() {
    let engine = engine(vec![
        entry(1, "ko", "không"),
        entry(2, "tiếng", "tiếng Việt"),
    ]);
    let short = run(&engine, &nfd(telex()), "ko{SPACE}");
    assert_eq!(short.txs[2], expansion(2, "kho\u{302}ng "));
    let long = run(&engine, &nfd(telex()), "tieengs{SPACE}");
    let replacement: String = "tiếng Việt ".nfd().collect();
    // `tiếng` is 5 units in NFC and 7 (t i e U+0302 U+0301 n g) in NFD: the deletion counts
    // the host-visible UTF-16 units of the NFD word.
    assert_eq!(long.txs[7], expansion(7, &replacement));
    for keys in ["ko{SPACE}", "tieengs{SPACE}", "Ko{ENTER}", "a{SPACE}ko,"] {
        let nfd_text = typed(&engine, &nfd(telex()), keys);
        let nfc_text = typed(&engine, &telex(), keys);
        assert!(is_nfd(&nfd_text), "{keys}");
        assert_eq!(nfd_text.nfc().collect::<String>(), nfc_text, "{keys}");
    }
}

#[test]
fn d51_macros_work_while_vietnamese_is_off_only_when_enabled() {
    let mut exact = entry(2, "vn", "Việt Nam");
    exact.case_policy = CasePolicy::Exact;
    let mut message = entry(3, "tm", "tin nhắn");
    message.boundary = BoundaryPolicy::Whitespace;
    let engine = engine(vec![entry(1, "ko", "không"), exact, message]);
    let mut off = telex();
    off.enabled = false;
    assert_eq!(typed(&engine, &off, "aas ko{SPACE}"), "aas ko ");
    off.macros_when_disabled = true;
    assert_eq!(typed(&engine, &off, "aas ko{SPACE}"), "aas không ");
    assert_eq!(
        run(&engine, &off, "ko{SPACE}").txs,
        [PASS, PASS, &expansion(2, "không ")]
    );
    // The policies of the table hold in disabled mode too.
    assert_eq!(typed(&engine, &off, "VN{SPACE}"), "Việt Nam ");
    assert_eq!(typed(&engine, &off, "tm,"), "tm,");
    off.macros_enabled = false;
    assert_eq!(typed(&engine, &off, "ko{SPACE}"), "ko ");
    let mut on = telex();
    on.macros_enabled = false;
    assert_eq!(typed(&engine, &on, "ko{SPACE}"), "ko ");
}

#[test]
fn disabled_entries_never_expand() {
    let mut off = entry(1, "ko", "không");
    off.enabled = false;
    let engine = engine(vec![off]);
    assert_eq!(typed(&engine, &telex(), "ko{SPACE}"), "ko ");
}

#[test]
fn the_lowest_identifier_wins_among_equal_triggers() {
    let engine = engine(vec![entry(5, "ko", "không"), entry(2, "ko", "khong")]);
    assert_eq!(typed(&engine, &telex(), "ko{SPACE}"), "khong ");
    assert_eq!(typed(&engine, &telex(), "KO{SPACE}"), "KHONG ");
}

/// The longest replacement in NFC output (D30): 4,095 astral scalars and a letter, 8,191
/// UTF-16 units, so that a printable delimiter still fits one insert of `MAX_INSERT_UNITS`.
fn nfc_longest() -> String {
    format!("{}a", "😀".repeat(4095))
}

/// The longest replacement in NFD output (D30): 2,730 × "ế" (one unit in NFC, three in NFD)
/// and a letter, 2,731 units in NFC and 8,191 in NFD.
fn nfd_longest() -> String {
    format!("{}a", "ế".repeat(2730))
}

/// Changed for D30: the longest replacement used to be 4,096 astral scalars, exactly
/// `MAX_INSERT_UNITS` units, which fit only before a replayed Enter. D30 rejects it, so the
/// longest is now one unit shorter and fits before every delimiter.
#[test]
fn replacements_at_the_insert_limit_expand() {
    let longest = nfc_longest();
    let decomposing = nfd_longest();
    let engine = engine(vec![
        entry(2, "fit", &longest),
        entry(3, "nfd", &decomposing),
    ]);
    assert_eq!(utf16::encode(&longest).len(), MAX_INSERT_UNITS - 1);
    // Enter is replayed, so the insert holds the replacement only.
    let enter = run(&engine, &telex(), "fit{ENTER}");
    assert_eq!(enter.text, format!("{longest}\r"));
    assert!(enter.txs[3].starts_with("Tx(3,"));
    assert!(enter.txs[3].ends_with(",true,Some(RETURN),true)"));
    // 8191 units plus the space: exactly at the limit.
    assert_eq!(
        typed(&engine, &telex(), "fit{SPACE}"),
        format!("{longest} ")
    );
    // In NFC the decomposing replacement is 2731 units, far below the limit.
    assert_eq!(
        typed(&engine, &telex(), "nfd{SPACE}"),
        format!("{decomposing} ")
    );
}

/// Expects matrix #47 (and #53 for NFD output) for a macro that `MacroTable` accepts: typing
/// `keys` gives `expected`, or with NFD output an NFD text canonically equivalent to it. A
/// table that rejected the entry would also satisfy the contract, because the user could then
/// not create a macro that never works; callers that need the entry accepted check it first.
fn assert_accepted_macro_expands(
    candidate: MacroEntry,
    options: &EngineOptions,
    keys: &str,
    expected: &str,
) {
    let Ok(table) = MacroTable::new(vec![candidate]) else {
        return;
    };
    let engine = VietnameseEngine::new(Vec::new(), Arc::new(table));
    let text = typed(&engine, options, keys);
    if options.normalization == Normalization::Nfd {
        assert!(is_nfd(&text), "{keys}: the output is not NFD");
    }
    assert!(
        text.nfc().eq(expected.nfc()),
        "{keys}: an accepted macro never expands"
    );
}

/// D30 (resolves plan §14.4 item 4; formerly the ignored `BUG` test
/// `d47_d53_accepted_replacements_beyond_the_insert_limit_expand`): a replacement is valid
/// only if it fits one insert with a delimiter in NFC and in NFD, so every replacement the
/// table accepts expands, in NFC and NFD output, before a printable delimiter, Enter, and
/// Tab, also when the typed case would adapt it (the table then inserts the stored text). The
/// "Silent fallback" of design §20.7 cannot be reached with valid macros, and one unit more
/// is rejected when the macro is created.
#[test]
fn d47_d53_every_accepted_replacement_expands_at_every_boundary() {
    // "ß" becomes "SS": the Title and Upper forms would be 8,192 units in NFD.
    let growing = format!("ß{}", "ệ".repeat(2730));
    let cases = [
        ("big", nfc_longest(), "big"),
        ("nfd", nfd_longest(), "nfd"),
        ("ss", growing.clone(), "Ss"),
        ("ss", growing, "SS"),
    ];
    for (trigger, replacement, typed_trigger) in cases {
        assert!(
            MacroTable::new(vec![entry(1, trigger, &replacement)]).is_ok(),
            "{trigger}: the longest replacements are valid"
        );
        for options in [telex(), nfd(telex())] {
            for (boundary, delimiter) in [
                ("{SPACE}", " "),
                (",", ","),
                ("{ENTER}", "\r"),
                ("{TAB}", "\t"),
            ] {
                assert_accepted_macro_expands(
                    entry(1, trigger, &replacement),
                    &options,
                    &format!("{typed_trigger}{boundary}"),
                    &format!("{replacement}{delimiter}"),
                );
            }
        }
    }
    for over in [
        "😀".repeat(4096),
        "ế".repeat(2731),
        format!("{}aa", "ế".repeat(2730)),
    ] {
        assert!(matches!(
            MacroTable::new(vec![entry(1, "big", &over)]),
            Err(MacroError::InvalidEntry {
                reason: EntryError::ReplacementTooLong,
                ..
            })
        ));
    }
}

#[test]
fn d54_expansions_with_supplementary_scalars_never_split_a_pair() {
    let engine = engine(vec![entry(1, "a1", "😀")]);
    let outcome = run(&engine, &telex(), "😀 a1{SPACE}{BS}{BS}{BS}");
    assert_eq!(outcome.text, "😀");
    assert_eq!(outcome.txs[4], expansion(2, "😀 "));
    assert_eq!(&outcome.txs[5..], [PASS, PASS, PASS]);
}

/// Expects matrix #47 for a trigger that `MacroTable` accepts: typed with default options and
/// followed by a space, it expands. A table that rejected the trigger would also satisfy the
/// contract, because the user could then not create a macro that never works.
fn assert_accepted_trigger_expands(trigger: &str, keys: &str) {
    let candidate = entry(1, trigger, "mở rộng");
    let Ok(table) = MacroTable::new(vec![candidate]) else {
        return;
    };
    let engine = VietnameseEngine::new(Vec::new(), Arc::new(table));
    for method in [InputMethod::Telex, InputMethod::Vni, InputMethod::Viqr] {
        assert_eq!(
            typed(&engine, &options(method), keys),
            "mở rộng ",
            "{method:?}: an accepted trigger never expands"
        );
    }
}

/// D28 (resolves plan §14.4 item 1; this test was ignored as a `BUG`): a whitespace boundary
/// also looks up the token typed since the previous whitespace, so a trigger with boundary
/// punctuation expands in every input method.
#[test]
fn d47_an_accepted_trigger_with_boundary_punctuation_expands() {
    assert!(MacroTable::new(vec![entry(1, "v.v", "mở rộng")]).is_ok());
    assert_accepted_trigger_expands("v.v", "v.v{SPACE}");
}

/// D28 with the real table: the token is replaced as a whole at Space, Enter, and Tab, its
/// case adapts, it works while Vietnamese is off, and only a whole token matches (matrix #48).
#[test]
fn d28_token_triggers_expand_with_the_real_table() {
    let engine = engine(vec![
        entry(1, "v.v", "vân vân"),
        entry(2, "tp.hcm", "Thành phố Hồ Chí Minh"),
        entry(3, "ko", "không"),
    ]);
    let outcome = run(&engine, &telex(), "v.v{SPACE}");
    assert_eq!(outcome.txs, [PASS, PASS, PASS, &expansion(3, "vân vân ")]);
    assert!(outcome.state.is_idle());
    for method in [InputMethod::Vni, InputMethod::Viqr] {
        assert_eq!(typed(&engine, &options(method), "v.v{SPACE}"), "vân vân ");
    }
    assert_eq!(
        run(&engine, &telex(), "v.v{ENTER}").txs[3],
        "Tx(3,\"vân vân\",true,Some(RETURN),true)"
    );
    assert_eq!(typed(&engine, &telex(), "v.v{TAB}"), "vân vân\t");
    assert_eq!(
        typed(&engine, &telex(), "ko v.v tp.hcm{SPACE}"),
        "không vân vân Thành phố Hồ Chí Minh "
    );
    assert_eq!(typed(&engine, &telex(), "V.V{SPACE}"), "VÂN VÂN ");
    assert_eq!(
        typed(&engine, &telex(), "TP.HCM{SPACE}"),
        "THÀNH PHỐ HỒ CHÍ MINH "
    );
    for keys in ["x.v.v{SPACE}", "v.v.v{SPACE}", "v.v,", "v.ko{SPACE}"] {
        let text = typed(&engine, &telex(), keys);
        assert!(!text.contains("vân"), "{keys}: {text}");
    }
    assert_eq!(typed(&engine, &telex(), "v.ko{SPACE}"), "v.không ");
    let mut off = telex();
    off.enabled = false;
    off.macros_when_disabled = true;
    assert_eq!(typed(&engine, &off, "v.v{SPACE}"), "vân vân ");
    let nfd_text = typed(&engine, &nfd(telex()), "tp.hcm{SPACE}");
    assert!(is_nfd(&nfd_text));
    assert_eq!(nfd_text.nfc().collect::<String>(), "Thành phố Hồ Chí Minh ");
}

/// D29 (resolves plan §14.4 item 2): a trigger has at most 32 scalars, the default context
/// limit, so the longest trigger typed one key per scalar expands with defaults v1, and a
/// longer one is rejected when the macro is created.
#[test]
fn d29_the_longest_trigger_expands_with_the_default_context_and_a_longer_one_is_rejected() {
    assert!(matches!(
        MacroTable::new(vec![entry(1, &"b".repeat(33), "mở rộng")]),
        Err(MacroError::InvalidEntry {
            reason: EntryError::TriggerTooLong,
            ..
        })
    ));
    let longest = "b".repeat(32);
    assert_eq!(
        EngineOptions::defaults_v1().context_limit_codepoints,
        u32::try_from(longest.chars().count()).expect("small")
    );
    assert!(MacroTable::new(vec![entry(1, &longest, "mở rộng")]).is_ok());
    assert_accepted_trigger_expands(&longest, &format!("{longest}{{SPACE}}"));
}
