//! Loader rules and executor records.

use std::sync::Arc;

use raumakey_engine::VietnameseEngine;
use raumakey_engine::script::{self, KeyToken, SCRIPT_EXE};
use raumakey_macro::MacroError;
use raumakey_model::diag::VectorId;
use raumakey_model::macros::{MacroId, NoMacros};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization, OptionsError};
use raumakey_model::policy::SecureFieldMode;

use super::run::{check, run_vector_with};
use super::*;

const PASS: &str = "Tx(0,\"\",false,None,false)";

/// A file with one vector made of the given TOML lines (after the `[[vector]]` header).
fn file(vector: &str) -> String {
    format!("schema_version = 1\n\n[[vector]]\n{vector}\n")
}

/// The required keys of a valid vector typing `aa`.
const BASE: &str = r#"id = "sample"
evidence = ["baseline"]
source = "crates/raumakey-engine/tests/matrix.rs::a01_telex_double_a_makes_circumflex"
method = "telex"
keys = "aa"
text = "â""#;

fn base_with(extra: &str) -> String {
    file(&format!("{BASE}\n{extra}"))
}

/// `BASE` with one key replaced.
fn base_replacing(key: &str, line: &str) -> String {
    let lines: Vec<&str> = BASE
        .lines()
        .map(|current| {
            if current.starts_with(&format!("{key} =")) {
                line
            } else {
                current
            }
        })
        .collect();
    file(&lines.join("\n"))
}

fn kind(text: &str) -> LoadErrorKind {
    match parse_fixture("sample.toml", text) {
        Ok(vectors) => panic!("accepted {} vectors", vectors.len()),
        Err(error) => {
            assert_eq!(error.file, "sample.toml");
            error.kind
        }
    }
}

/// The rule a vector breaks; the error names the vector's id.
fn invalid(text: &str) -> VectorError {
    let expected_id = text
        .lines()
        .find_map(|line| line.strip_prefix("id = \""))
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or_default();
    match kind(text) {
        LoadErrorKind::Invalid { id, error } => {
            assert_eq!(id, expected_id);
            error
        }
        other => panic!("expected an invalid vector, got {other:?}"),
    }
}

fn parsed(text: &str) -> Vector {
    match parse_fixture("sample.toml", text) {
        Ok(mut vectors) if vectors.len() == 1 => vectors.remove(0),
        Ok(vectors) => panic!("expected one vector, got {}", vectors.len()),
        Err(error) => panic!("rejected: {error}"),
    }
}

fn vector(keys: &str, text: &str) -> Vector {
    Vector {
        id: "programmatic".to_owned(),
        rows: Vec::new(),
        evidence: vec![Evidence::Baseline],
        decisions: Vec::new(),
        source: "tests/x.rs::y".to_owned(),
        method: InputMethod::Telex,
        options: OptionOverrides::default(),
        macros: Vec::new(),
        initial_text: String::new(),
        keys: keys.to_owned(),
        transactions: None,
        steps: Vec::new(),
        text: text.to_owned(),
        owned: None,
        idle: None,
    }
}

#[test]
fn a_valid_file_parses_with_defaults() {
    let vector = parsed(&base_with(
        "rows = [\"A1\"]\ntransactions = ['Tx(0,\"\",false,None,false)', 'Tx(1,\"â\",true,None,false)']",
    ));
    assert_eq!(vector.id, "sample");
    assert_eq!(vector.rows, ["A1"]);
    assert_eq!(vector.evidence, [Evidence::Baseline]);
    assert!(vector.decisions.is_empty());
    assert_eq!(vector.method, InputMethod::Telex);
    assert_eq!(vector.options, OptionOverrides::default());
    assert!(vector.macros.is_empty());
    assert_eq!(vector.initial_text, "");
    assert_eq!(vector.keys, "aa");
    assert_eq!(vector.transactions.as_ref().map(Vec::len), Some(2));
    assert!(vector.steps.is_empty());
    assert_eq!(vector.text, "â");
    assert_eq!(vector.owned, None);
    assert_eq!(vector.idle, None);
    assert_eq!(vector.validate(), Ok(()));
}

#[test]
fn files_need_schema_version_1_and_a_vector() {
    let without = BASE.to_owned();
    assert_eq!(
        kind(&format!("[[vector]]\n{without}\n")),
        LoadErrorKind::UnsupportedSchema
    );
    assert_eq!(
        kind(&format!("schema_version = 2\n[[vector]]\n{without}\n")),
        LoadErrorKind::UnsupportedSchema
    );
    assert_eq!(
        kind("schema_version = \"1\"\n"),
        LoadErrorKind::UnsupportedSchema
    );
    assert_eq!(kind("schema_version = 1\n"), LoadErrorKind::Empty);
}

#[test]
fn syntax_errors_and_unknown_keys_name_the_line() {
    match kind(&base_with("expected = \"â\"")) {
        LoadErrorKind::Syntax { line, message } => {
            assert_eq!(line, Some(10), "{message}");
            assert!(message.contains("expected"), "{message}");
        }
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        kind("schema_version = 1\nfoo = 1\n"),
        LoadErrorKind::Syntax { line: Some(2), .. }
    ));
    assert!(matches!(
        kind("schema_version = 1\n[[vector]\n"),
        LoadErrorKind::Syntax { line: Some(2), .. }
    ));
    assert!(matches!(
        kind(&base_with("options = { spell_checking = false }")),
        LoadErrorKind::Syntax { .. }
    ));
}

#[test]
fn ids_are_lowercase_groups_and_unique() {
    for id in [
        "",
        "A01",
        "a01_telex",
        "a01--telex",
        "-a01",
        "a01-",
        "a01 telex",
        "ä01",
    ] {
        assert_eq!(
            invalid(&base_replacing("id", &format!("id = \"{id}\""))),
            VectorError::InvalidId,
            "{id:?}"
        );
    }
    let longest = "a".repeat(MAX_ID_LEN);
    assert!(
        parse_fixture(
            "x.toml",
            &base_replacing("id", &format!("id = \"{longest}\""))
        )
        .is_ok()
    );
    let too_long = "a".repeat(MAX_ID_LEN + 1);
    assert_eq!(
        invalid(&base_replacing("id", &format!("id = \"{too_long}\""))),
        VectorError::InvalidId
    );
    let twice = format!("{}\n[[vector]]\n{BASE}\n", base_with(""));
    assert_eq!(
        kind(&twice),
        LoadErrorKind::DuplicateId("sample".to_owned())
    );
    let first = EmbeddedFixture {
        name: "first.toml",
        text: "schema_version = 1\n[[vector]]\nid = \"same\"\nevidence = [\"baseline\"]\nsource = \"a.rs::b\"\nmethod = \"telex\"\nkeys = \"a\"\ntext = \"a\"\n",
    };
    let second = EmbeddedFixture {
        name: "second.toml",
        ..first
    };
    let error = load(&[first, second]).err();
    assert_eq!(
        error,
        Some(LoadError {
            file: "second.toml".to_owned(),
            kind: LoadErrorKind::DuplicateId("same".to_owned()),
        })
    );
}

#[test]
fn ids_are_diagnostic_vector_ids() {
    // Plan §2.10: `VectorId` is `[A-Za-z0-9._-]{1,32}`, the only form in which a log may name
    // a test vector (design §14).
    let longest = "a".repeat(32);
    let vector = parsed(&base_replacing("id", &format!("id = \"{longest}\"")));
    assert!(VectorId::new(&vector.id).is_ok());
    for id in [
        "a".repeat(33),
        "a08-viqr-toned-word-kept-before-escaped-period".to_owned(),
    ] {
        assert_eq!(
            invalid(&base_replacing("id", &format!("id = \"{id}\""))),
            VectorError::InvalidId,
            "{id}"
        );
    }
}

#[test]
fn rows_name_matrix_rows_in_their_section() {
    for row in ["A1", "A20", "B21", "B29", "C30", "C43", "D44", "D58"] {
        let vector = parsed(&base_with(&format!("rows = [\"{row}\"]")));
        assert_eq!(vector.rows, [row]);
    }
    for row in [
        "A0", "A21", "B20", "B30", "C29", "C44", "D43", "D59", "E1", "a1", "A01", "A", "1", "",
    ] {
        assert_eq!(
            invalid(&base_with(&format!("rows = [\"{row}\"]"))),
            VectorError::InvalidRow(row.to_owned()),
            "{row:?}"
        );
    }
    assert_eq!(
        invalid(&base_with("rows = [\"A1\", \"A1\"]")),
        VectorError::InvalidRow("A1".to_owned())
    );
}

#[test]
fn evidence_and_decisions_go_together() {
    assert_eq!(
        invalid(&base_replacing("evidence", "evidence = []")),
        VectorError::InvalidEvidence
    );
    assert_eq!(
        invalid(&base_replacing(
            "evidence",
            "evidence = [\"baseline\", \"baseline\"]"
        )),
        VectorError::InvalidEvidence
    );
    assert!(matches!(
        kind(&base_replacing("evidence", "evidence = [\"observed\"]")),
        LoadErrorKind::Syntax { .. }
    ));
    let decision = "evidence = [\"baseline\", \"raumakey-decision\"]";
    assert_eq!(
        invalid(&base_replacing("evidence", decision)),
        VectorError::InvalidDecisions
    );
    assert_eq!(
        invalid(&base_with("decisions = [\"D7\"]")),
        VectorError::InvalidDecisions
    );
    for good in ["D7", "D24", "plan §3.9", "design §20.9"] {
        let text = base_replacing("evidence", &format!("{decision}\ndecisions = [\"{good}\"]"));
        let vector = parsed(&text);
        assert_eq!(
            vector.evidence,
            [Evidence::Baseline, Evidence::RaumakeyDecision]
        );
        assert_eq!(vector.decisions, [good]);
    }
    for bad in ["", "D", "D7a", "d7", "plan", "plan §", "design 20.9", "D7 "] {
        let text = base_replacing("evidence", &format!("{decision}\ndecisions = [\"{bad}\"]"));
        assert_eq!(invalid(&text), VectorError::InvalidDecisions, "{bad:?}");
    }
    let repeated = base_replacing(
        "evidence",
        &format!("{decision}\ndecisions = [\"D7\", \"D7\"]"),
    );
    assert_eq!(invalid(&repeated), VectorError::InvalidDecisions);
}

#[test]
fn sources_name_a_test_in_a_rust_file() {
    for source in [
        "",
        "matrix.rs",
        "matrix.rs::",
        "::a01",
        "matrix.toml::a01",
        "matrix.rs::a01::b",
        "matrix.rs::a 01",
    ] {
        assert_eq!(
            invalid(&base_replacing("source", &format!("source = \"{source}\""))),
            VectorError::InvalidSource,
            "{source:?}"
        );
    }
}

#[test]
fn user_defined_key_maps_are_not_supported() {
    assert_eq!(
        invalid(&base_replacing("method", "method = \"user\"")),
        VectorError::UnsupportedMethod
    );
}

#[test]
fn options_override_defaults_v1() {
    let vector = parsed(&base_replacing(
        "method",
        "method = \"vni\"\noptions = { normalization = \"nfd\", modern_tone = true, context_limit_codepoints = 64, enabled = false, macros_when_disabled = true }",
    ));
    let expected = EngineOptions {
        input_method: InputMethod::Vni,
        normalization: Normalization::Nfd,
        modern_tone: true,
        context_limit_codepoints: 64,
        charset: Default::default(),
        enabled: false,
        macros_when_disabled: true,
        ..EngineOptions::defaults_v1()
    };
    // The text does not matter for loading.
    assert_eq!(vector.effective_options(), Ok(expected));
    let mut all = EngineOptions::defaults_v1();
    OptionOverrides {
        enabled: Some(false),
        normalization: Some(Normalization::Preserve),
        modern_tone: Some(true),
        free_marking: Some(false),
        spell_check: Some(false),
        auto_restore_invalid: Some(false),
        macros_enabled: Some(false),
        macros_when_disabled: Some(true),
        auto_case_macro: Some(false),
        context_limit_codepoints: Some(8),
        // A policy setting: it changes no engine option.
        secure_field_mode: Some(SecureFieldMode::Restricted),
    }
    .apply(&mut all);
    assert_eq!(
        all,
        EngineOptions {
            enabled: false,
            input_method: InputMethod::Telex,
            normalization: Normalization::Preserve,
            modern_tone: true,
            free_marking: false,
            spell_check: false,
            auto_restore_invalid: false,
            macros_enabled: false,
            macros_when_disabled: true,
            auto_case_macro: false,
            user_keymap_id: None,
            context_limit_codepoints: 8,
            charset: Default::default(),
        }
    );
    assert_eq!(
        invalid(&base_with("options = { context_limit_codepoints = 4 }")),
        VectorError::InvalidOptions(OptionsError::ContextLimitOutOfRange)
    );
}

#[test]
fn key_scripts_parse_and_are_not_empty() {
    assert_eq!(
        invalid(&base_replacing("keys", "keys = \"\"")),
        VectorError::InvalidKeys(None)
    );
    assert!(matches!(
        invalid(&base_replacing("keys", "keys = \"a{NOPE}\"")),
        VectorError::InvalidKeys(Some(_))
    ));
    let vector = parsed(&base_replacing("keys", "keys = \"a{BS}{CTRL+c}\""));
    assert_eq!(
        vector.tokens(),
        Ok(vec![
            KeyToken::Char('a'),
            KeyToken::Backspace,
            KeyToken::Ctrl('c')
        ])
    );
}

#[test]
fn transactions_and_steps_are_checked_against_the_keys() {
    assert_eq!(
        invalid(&base_with(&format!("transactions = ['{PASS}']"))),
        VectorError::TransactionCount {
            keys: 2,
            transactions: 1
        }
    );
    assert_eq!(
        invalid(&base_with(&format!(
            "steps = [{{ index = 2, transaction = '{PASS}' }}]"
        ))),
        VectorError::InvalidStep { index: 2 }
    );
    assert_eq!(
        invalid(&base_with(&format!(
            "steps = [{{ index = 1, transaction = '{PASS}' }}, {{ index = 0, transaction = '{PASS}' }}]"
        ))),
        VectorError::InvalidStep { index: 0 }
    );
    assert_eq!(
        invalid(&base_with(&format!(
            "steps = [{{ index = 1, transaction = '{PASS}' }}, {{ index = 1, transaction = '{PASS}' }}]"
        ))),
        VectorError::InvalidStep { index: 1 }
    );
    assert_eq!(
        invalid(&base_with(&format!(
            "transactions = ['{PASS}', '{PASS}']\nsteps = [{{ index = 0, transaction = '{PASS}' }}]"
        ))),
        VectorError::TransactionsAndSteps
    );
    let vector = parsed(&base_with(&format!(
        "steps = [{{ index = 0, transaction = '{PASS}' }}, {{ index = 1, transaction = 'x' }}]"
    )));
    assert_eq!(vector.steps.len(), 2);
}

#[test]
fn owned_and_idle_are_exclusive() {
    assert_eq!(
        invalid(&base_with("owned = \"â\"\nidle = false")),
        VectorError::OwnedAndIdle
    );
    assert_eq!(parsed(&base_with("idle = false")).idle, Some(false));
    assert_eq!(
        parsed(&base_with("owned = \"â\"")).owned.as_deref(),
        Some("â")
    );
}

#[test]
fn macros_build_a_real_table() {
    let vector = parsed(&base_with(
        "macros = [{ trigger = \"ko\", replacement = \"không\" }, { trigger = \"vn\", replacement = \"Việt Nam\" }]",
    ));
    let entries = vector.macro_entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].id, MacroId(1));
    assert_eq!(entries[1].id, MacroId(2));
    assert!(entries.iter().all(|entry| entry.enabled));
    assert_eq!(entries[1].trigger, "vn");
    assert_eq!(entries[1].replacement, "Việt Nam");
    assert!(vector.macro_table().is_ok());
    assert!(matches!(
        invalid(&base_with(
            "macros = [{ trigger = \"k o\", replacement = \"không\" }]"
        )),
        VectorError::InvalidMacros(MacroError::InvalidEntry { .. })
    ));
}

#[test]
fn a_passing_vector_records_the_run() {
    let mut aa = vector("aa", "â");
    aa.transactions = Some(vec![
        PASS.to_owned(),
        "Tx(1,\"â\",true,None,false)".to_owned(),
    ]);
    aa.owned = Some("â".to_owned());
    let result = run_vector(&aa);
    assert_eq!(result.status, Status::Pass, "{:?}", result.failures);
    assert!(result.failures.is_empty());
    assert_eq!(result.id, "programmatic");
    assert_eq!(
        result.transactions,
        aa.transactions.clone().unwrap_or_default()
    );
    assert_eq!(result.final_text.as_deref(), Some("â"));
    assert_eq!(result.keys, "aa");
    assert_eq!(result.architecture, std::env::consts::ARCH);
    assert_eq!(result.application.name, SCRIPT_EXE);
    assert_eq!(result.application.kind, ApplicationKind::InProcess);
    assert_eq!(result.config_revision, ConfigRevision::IN_PROCESS);
    assert_eq!(result.config_revision.defaults_version, 1);
    assert_eq!(result.config_revision.store_revision, None);
    assert_eq!(result.options, Some(EngineOptions::defaults_v1()));
    assert_eq!(result.os, None, "the runner fills in the OS");
}

#[test]
fn every_mismatch_is_a_failure() {
    let mut wrong = vector("aa", "a");
    wrong.transactions = Some(vec![PASS.to_owned(), PASS.to_owned()]);
    wrong.idle = Some(true);
    let result = run_vector(&wrong);
    assert_eq!(result.status, Status::Fail);
    assert_eq!(
        result.failures,
        [
            Failure::Transaction {
                step: 1,
                expected: PASS.to_owned(),
                actual: "Tx(1,\"â\",true,None,false)".to_owned(),
            },
            Failure::FinalText {
                expected: "a".to_owned(),
                actual: "â".to_owned(),
            },
            Failure::Idle {
                expected: true,
                actual: false,
            },
        ]
    );
    let mut steps = vector("aa{BS}", "a");
    steps.steps = vec![StepExpectation {
        index: 2,
        transaction: PASS.to_owned(),
    }];
    steps.owned = Some("â".to_owned());
    let result = run_vector(&steps);
    assert_eq!(
        result.failures,
        [
            Failure::Transaction {
                step: 2,
                expected: PASS.to_owned(),
                actual: "Tx(1,\"a\",true,None,false)".to_owned(),
            },
            Failure::Owned {
                expected: "â".to_owned(),
                actual: "a".to_owned(),
            },
        ]
    );
}

/// The real engine never emits a transaction that the document rejects, nor leaves its word
/// anywhere but at the end of the document, so these checks run on a doctored outcome.
#[test]
fn unapplied_transactions_and_a_stray_word_are_failures() {
    let engine = VietnameseEngine::new(Vec::new(), Arc::new(NoMacros));
    let tokens = match script::parse_keys("aa") {
        Ok(tokens) => tokens,
        Err(error) => panic!("valid script: {error}"),
    };
    let mut outcome = match script::run_script(&engine, &EngineOptions::defaults_v1(), &tokens) {
        Ok(outcome) => outcome,
        Err(error) => panic!("the script runs: {error}"),
    };
    let aa = vector("aa", "â");
    outcome.steps[1].accepted = false;
    let mut result = run_vector(&aa);
    result.failures.clear();
    check(&aa, &outcome, &mut result);
    assert_eq!(result.failures, [Failure::NotApplied { step: 1 }]);

    // The engine still owns `â`, but the document ends in something else.
    outcome.steps[1].accepted = true;
    outcome.final_text = "x".to_owned();
    let stray = vector("aa", "x");
    let mut result = run_vector(&stray);
    result.failures.clear();
    check(&stray, &outcome, &mut result);
    assert_eq!(result.failures, [Failure::OwnedNotSuffix]);
}

/// A panic in the engine or the table fails its vector and nothing else: the runner still
/// writes its report and exits with 1. The panic message is not vector data and stays out of
/// the record.
#[test]
fn a_panicking_run_is_a_failure_of_its_vector() {
    let aa = vector("aa", "â");
    let result = run_vector_with(&aa, |_| panic!("a deliberate engine bug"));
    assert_eq!(result.status, Status::Fail);
    assert_eq!(result.failures, [Failure::Panic]);
    assert!(result.transactions.is_empty());
    assert_eq!(result.final_text, None);
    assert_eq!(format!("{:?}", result.failures), "[Panic]");
    let json = serde_json::to_string(&result).unwrap_or_default();
    assert!(json.contains("\"kind\":\"panic\""), "{json}");
    assert!(!json.contains("deliberate"), "{json}");
    // The same vector runs normally afterwards.
    assert_eq!(run_vector(&aa).status, Status::Pass);
}

#[test]
fn invalid_vectors_fail_setup_without_running() {
    let mut count = vector("aa", "â");
    count.transactions = Some(vec![PASS.to_owned()]);
    let result = run_vector(&count);
    assert_eq!(result.status, Status::Fail);
    assert!(matches!(result.failures[..], [Failure::Setup { .. }]));
    assert!(result.transactions.is_empty());
    assert_eq!(result.final_text, None);
    let mut user = vector("aa", "â");
    user.method = InputMethod::UserDefined;
    let result = run_vector(&user);
    assert_eq!(result.options, None);
    assert!(matches!(result.failures[..], [Failure::Setup { .. }]));
}

#[test]
fn vectors_run_with_the_real_macro_table_and_the_initial_text() {
    let mut macro_vector = vector("Ko{SPACE}", "Không ");
    macro_vector.macros = vec![VectorMacro {
        trigger: "ko".to_owned(),
        replacement: "không".to_owned(),
    }];
    macro_vector.idle = Some(true);
    let result = run_vector(&macro_vector);
    assert_eq!(result.status, Status::Pass, "{:?}", result.failures);
    let mut emoji = vector("aa{BS}{BS}{BS}", "");
    emoji.initial_text = "😀".to_owned();
    let result = run_vector(&emoji);
    assert_eq!(result.status, Status::Pass, "{:?}", result.failures);
    assert_eq!(result.transactions.len(), 5);
}

fn ko_macro() -> Vec<VectorMacro> {
    vec![VectorMacro {
        trigger: "ko".to_owned(),
        replacement: "không".to_owned(),
    }]
}

/// Matrix #29 under D25: `{SECURE}` keys follow the compatibility policy of defaults v1,
/// whose secure-field mode is `Bypass`. As the TIP does, the executor passes every key of a
/// secure field through before the engine and ends the word (`Tx(0,"",false,None,true)`), so
/// neither transformation, nor macros, nor backspace history apply there; after the field,
/// Telex works again.
#[test]
fn b29_secure_fields_bypass_everything() {
    let mut bypass = vector("{SECURE}aas ko{SPACE}{BS}{/SECURE}aa", "aas koâ");
    bypass.macros = ko_macro();
    assert_eq!(bypass.secure_field_mode(), SecureFieldMode::Bypass);
    let result = run_vector(&bypass);
    assert_eq!(result.status, Status::Pass, "{:?}", result.failures);
    assert_eq!(result.transactions[..9], ["Tx(0,\"\",false,None,true)"; 9]);
    assert_eq!(result.transactions[11], "Tx(1,\"â\",true,None,false)");
}

/// D25 `Restricted` (an option override of the vector): secure fields keep Vietnamese
/// transformation, but no macro expands there (the policy turns macros off for the key, and
/// the engine never looks one up in a secure field).
#[test]
fn b29_restricted_secure_fields_transform_without_macros() {
    let mut restricted = vector(
        "{SECURE}vieetj ko{SPACE}{/SECURE}ko{SPACE}",
        "việt ko không ",
    );
    restricted.macros = ko_macro();
    restricted.options.secure_field_mode = Some(SecureFieldMode::Restricted);
    assert_eq!(restricted.secure_field_mode(), SecureFieldMode::Restricted);
    let result = run_vector(&restricted);
    assert_eq!(result.status, Status::Pass, "{:?}", result.failures);
    assert_eq!(result.transactions[6], "Tx(2,\"ệt\",true,None,false)");
    assert_eq!(result.transactions[10], "Tx(0,\"\",false,None,true)");
    assert_eq!(result.transactions[14], "Tx(2,\"không \",true,None,true)");
    // The mode is a policy setting, not an engine option.
    let toml = parsed(&base_with(
        "options = { secure_field_mode = \"restricted\" }",
    ));
    assert_eq!(toml.secure_field_mode(), SecureFieldMode::Restricted);
    assert_eq!(toml.effective_options(), Ok(EngineOptions::defaults_v1()));
    assert_eq!(
        parsed(&base_with("options = { secure_field_mode = \"bypass\" }")).secure_field_mode(),
        SecureFieldMode::Bypass
    );
}

#[test]
fn debug_output_hides_keys_texts_and_macros() {
    let mut secret = vector("bimaatj", "bímật");
    secret.initial_text = "giấu".to_owned();
    secret.macros = vec![VectorMacro {
        trigger: "kin".to_owned(),
        replacement: "kín đáo".to_owned(),
    }];
    secret.steps = vec![StepExpectation {
        index: 0,
        transaction: "Tx(0,\"bí\",false,None,false)".to_owned(),
    }];
    let result = run_vector(&secret);
    assert_eq!(result.status, Status::Fail);
    for shown in [
        format!("{secret:?}"),
        format!("{result:?}"),
        format!("{:?}", result.failures),
        format!("{:?}", secret.macros),
        format!("{:?}", secret.steps),
    ] {
        for hidden in ["bimaatj", "bí", "giấu", "kin", "kín", "mật"] {
            assert!(!shown.contains(hidden), "{hidden} in {shown}");
        }
    }
}
