//! Contract suite: the machine-readable compatibility vectors of `tests/fixtures/compat/`
//! (plan §12, design §15.3).
//!
//! Every embedded vector runs in-process with the real engine and the real `MacroTable` and must
//! pass. The other tests keep the fixture set complete and traceable: every engine-level matrix
//! row that plan §12 names and every plan §3.10 golden vector has a vector, the embedded list
//! names exactly the files of the fixture directory, every vector names an existing Rust test
//! as the source of its expectations, and no two vectors run the same setup. Others keep the
//! vectors honest: every id is a diagnostic `VectorId`, the evidence labels follow the rules of
//! `raumakey_tests::vectors` that can be checked mechanically, and the NFD vectors keep their
//! meaning (escaped combining marks, NFD input for #52, NFD output for #53).
//!
//! A failing vector is shown as the JSON record the runner's report would hold for it: vector
//! data only.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use raumakey_engine::script::KeyToken;
use raumakey_model::diag::VectorId;
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};
use raumakey_tests::vectors::{self, EMBEDDED, Evidence, Status, Vector};
use unicode_normalization::char::is_combining_mark;
use unicode_normalization::{is_nfc, is_nfd};

use InputMethod::{Telex, Viqr, Vni};

/// The engine-level matrix rows of plan §12: A1–A18, B21–B23, B29, D47–D54.
const ENGINE_ROWS: [&str; 30] = [
    "A1", "A2", "A3", "A4", "A5", "A6", "A7", "A8", "A9", "A10", "A11", "A12", "A13", "A14", "A15",
    "A16", "A17", "A18", "B21", "B22", "B23", "B29", "D47", "D48", "D49", "D50", "D51", "D52",
    "D53", "D54",
];

/// The golden vectors of plan §3.10: method, modern tone placement, key script, final text,
/// with defaults v1 otherwise.
const GOLDEN: &[(InputMethod, bool, &str, &str)] = &[
    (Telex, false, "aa", "â"),
    (Telex, false, "dd", "đ"),
    (Telex, false, "as", "á"),
    (Telex, false, "aw", "ă"),
    (Telex, false, "ow", "ơ"),
    (Telex, false, "uw", "ư"),
    (Telex, false, "w", "ư"),
    (Telex, false, "ww", "w"),
    (Telex, false, "tw", "tư"),
    (Telex, false, "nhw", "như"),
    (Telex, false, "ass", "as"),
    (Telex, false, "aaa", "aa"),
    (Telex, false, "ddd", "dd"),
    (Telex, false, "tieengs", "tiếng"),
    (Telex, false, "vieetj", "việt"),
    (Telex, false, "nguwowif", "người"),
    (Telex, false, "nguoiwf", "người"),
    (Telex, false, "thuowr", "thuở"),
    (Telex, false, "huow", "huơ"),
    (Telex, false, "truowngf", "trường"),
    (Telex, false, "ruowuj", "rượu"),
    (Telex, false, "khuyeens", "khuyến"),
    (Telex, false, "quas", "quá"),
    (Telex, false, "quys", "quý"),
    (Telex, false, "quyeets", "quyết"),
    (Telex, false, "gias", "giá"),
    (Telex, false, "gif", "gì"),
    (Telex, false, "giuwax", "giữa"),
    (Telex, false, "giuwowngf", "giường"),
    (Telex, false, "ddi", "đi"),
    (Telex, false, "did", "đi"),
    (Telex, false, "Ddaay", "Đây"),
    (Telex, false, "VIEETJ", "VIỆT"),
    (Telex, false, "hoaf", "hòa"),
    (Telex, true, "hoaf", "hoà"),
    (Telex, false, "thuyr", "thủy"),
    (Telex, true, "thuyr", "thuỷ"),
    (Telex, false, "khoer", "khỏe"),
    (Telex, true, "khoer", "khoẻ"),
    (Telex, false, "hoafn", "hoàn"),
    (Telex, true, "hoafn", "hoàn"),
    (Telex, false, "chaof", "chào"),
    (Telex, false, "cuar", "của"),
    (Telex, false, "muoons", "muốn"),
    (Telex, false, "nghieeng", "nghiêng"),
    (Telex, false, "awa", "â"),
    (Telex, false, "tesst", "test"),
    (Telex, false, "after{SPACE}", "after "),
    (Telex, false, "windows{SPACE}", "windows "),
    (Telex, false, "class", "class"),
    (Vni, false, "tie6ng1", "tiếng"),
    (Vni, false, "vie65t", "việt"),
    (Vni, false, "nguo72i", "người"),
    (Vni, false, "a66", "a6"),
    (Vni, false, "d9", "đ"),
    (Viqr, false, "tie^'ng", "tiếng"),
    (Viqr, false, "vie^.t", "việt"),
    (Viqr, false, "ngu+o+`i", "người"),
    (Viqr, false, "\\'", "'"),
    (Viqr, false, "dd", "đ"),
];

fn embedded_vectors() -> Vec<Vector> {
    match vectors::load_embedded() {
        Ok(vectors) => vectors,
        Err(error) => panic!("the embedded fixtures must load: {error}"),
    }
}

fn workspace_root() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .map_or_else(|| manifest_dir.to_path_buf(), Path::to_path_buf)
}

#[test]
fn every_embedded_vector_passes() {
    let vectors = embedded_vectors();
    assert!(!vectors.is_empty(), "no vectors are embedded");
    let failed: Vec<String> = vectors
        .iter()
        .map(vectors::run_vector)
        .filter(|result| result.status != Status::Pass)
        .map(|result| serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.id.clone()))
        .collect();
    assert!(
        failed.is_empty(),
        "{} of {} vectors failed:\n{}",
        failed.len(),
        vectors.len(),
        failed.join("\n")
    );
}

/// Plan §2.10: a test vector id travels in diagnostics only as a `VectorId`
/// (`[A-Za-z0-9._-]{1,32}`, design §14), so every fixture id must be one.
#[test]
fn every_vector_id_is_a_diagnostic_vector_id() {
    let invalid: Vec<String> = embedded_vectors()
        .into_iter()
        .filter(|vector| VectorId::new(&vector.id).is_err())
        .map(|vector| vector.id)
        .collect();
    assert!(
        invalid.is_empty(),
        "{} ids are not valid VectorIds: {invalid:?}",
        invalid.len()
    );
}

/// The VIQR keys with an action binding (plan §3.2; letters match case-insensitively).
const VIQR_BOUND_KEYS: &str = "'`?~.-^(+*dD\\";
/// The VIQR-bound keys that are also boundary punctuation (plan §3.8).
const VIQR_BOUND_PUNCTUATION: &str = "'?.(\\";

/// The character a token types, if any (AltGr characters are typed normally, matrix #22).
fn typed_char(token: &KeyToken) -> Option<char> {
    match *token {
        KeyToken::Char(c) | KeyToken::AltGr(c) => Some(c),
        _ => None,
    }
}

/// Indices of the VIQR-escaped boundary punctuation keys of a key script: a bound key after a
/// `\` that armed the escape (plan §3.6), when the key is boundary punctuation.
fn escaped_boundaries(tokens: &[KeyToken]) -> Vec<usize> {
    let mut found = Vec::new();
    let mut armed = false;
    for (index, token) in tokens.iter().enumerate() {
        let typed = typed_char(token);
        if armed && typed.is_some_and(|c| VIQR_BOUND_KEYS.contains(c)) {
            armed = false;
            if typed.is_some_and(|c| VIQR_BOUND_PUNCTUATION.contains(c)) {
                found.push(index);
            }
            continue;
        }
        armed = typed == Some('\\');
    }
    found
}

/// Plan §3.6 keeps a VIQR-escaped key in the word as a literal; the engine instead commits the
/// word at escaped boundary punctuation, a code decision that design §20.9 lists. The expected
/// values rest on that decision whenever keys follow the escaped key or the vector asserts
/// transactions or the engine's word; such a vector must name the decision (matrix pass
/// criteria: a Raumakey decision is never reported as baseline alone).
#[test]
fn vectors_resting_on_the_viqr_escape_decision_name_it() {
    let mut unlabelled = Vec::new();
    for vector in embedded_vectors() {
        let enabled = vector
            .effective_options()
            .is_ok_and(|options| options.enabled);
        let Ok(tokens) = vector.tokens() else {
            continue;
        };
        if vector.method != Viqr || !enabled {
            continue;
        }
        let depends = escaped_boundaries(&tokens).into_iter().any(|index| {
            index + 1 < tokens.len()
                || vector.transactions.is_some()
                || vector.steps.iter().any(|step| step.index >= index)
                || vector.owned.is_some()
                || vector.idle.is_some()
        });
        let named = vector.evidence.contains(&Evidence::RaumakeyDecision)
            && vector
                .decisions
                .iter()
                .any(|decision| decision == "design §20.9");
        if depends && !named {
            unlabelled.push(vector.id);
        }
    }
    assert!(
        unlabelled.is_empty(),
        "vectors resting on the VIQR escape decision (design §20.9) without naming it: \
         {unlabelled:?}"
    );
}

#[test]
fn the_escape_scan_follows_plan_3_6() {
    let tokens = |keys: &str| match raumakey_engine::script::parse_keys(keys) {
        Ok(tokens) => tokens,
        Err(error) => panic!("{keys}: {error}"),
    };
    assert_eq!(escaped_boundaries(&tokens("\\'a'")), [1]);
    assert_eq!(escaped_boundaries(&tokens("ho.c\\.{SPACE}")), [5]);
    // An escaped backslash is boundary punctuation; the key after it is typed normally.
    assert_eq!(escaped_boundaries(&tokens("\\\\'")), [1]);
    // Bound keys that are not punctuation, and unbound keys, end no word.
    assert!(escaped_boundaries(&tokens("\\^a'")).is_empty());
    assert!(escaped_boundaries(&tokens("\\da'")).is_empty());
    assert!(escaped_boundaries(&tokens("\\b'")).is_empty());
    assert!(escaped_boundaries(&tokens("\\{SPACE}'")).is_empty());
    assert!(escaped_boundaries(&tokens("a'")).is_empty());
}

/// A combining mark typed as itself is invisible in most editors, and an editor that
/// normalizes on save would turn an NFD vector into an NFC one that may still pass. Fixture
/// files therefore write every combining mark as a TOML `\u` escape.
#[test]
fn fixture_sources_write_combining_marks_as_escapes() {
    let mut raw = Vec::new();
    for fixture in EMBEDDED {
        for (index, line) in fixture.text.lines().enumerate() {
            if line.chars().any(is_combining_mark) {
                raw.push(format!("{}:{}", fixture.name, index + 1));
            }
        }
    }
    assert!(
        raw.is_empty(),
        "raw combining marks (write them as \\u escapes): {raw:?}"
    );
}

/// The NFD vectors keep their meaning: NFD output is NFD (matrix #53), and some D52 vector
/// types NFD input (matrix #52: trigger NFC, input NFD).
#[test]
fn nfd_vectors_type_and_expect_nfd_text() {
    let vectors = embedded_vectors();
    for vector in &vectors {
        let nfd_output = vector
            .effective_options()
            .is_ok_and(|options| options.normalization == Normalization::Nfd);
        if nfd_output {
            assert!(
                is_nfd(&vector.text),
                "{}: the expected text is not NFD",
                vector.id
            );
        }
    }
    assert!(
        vectors
            .iter()
            .any(|vector| vector.rows.iter().any(|row| row == "D52") && !is_nfc(&vector.keys)),
        "no D52 vector types NFD input"
    );
}

/// The matrix rows give the evidence level of a matrix vector. A vector without a row is
/// `baseline` only as a plan §3.10 golden vector, whose mappings the matrix takes from the
/// UniKey baseline, or as such a vector ended by a space, as the plan's own `after` and
/// `windows` vectors are (`raumakey_tests::vectors`, "Evidence").
#[test]
fn rowless_baseline_vectors_are_plan_golden_vectors() {
    let mut unsupported = Vec::new();
    for vector in embedded_vectors() {
        if !vector.rows.is_empty() || !vector.evidence.contains(&Evidence::Baseline) {
            continue;
        }
        let golden = GOLDEN.iter().any(|&(method, modern_tone, keys, text)| {
            let options = EngineOptions {
                input_method: method,
                modern_tone,
                ..EngineOptions::defaults_v1()
            };
            let as_listed = vector.keys == keys && vector.text == text;
            let with_space =
                vector.keys == format!("{keys}{{SPACE}}") && vector.text == format!("{text} ");
            (as_listed || with_space)
                && vector.macros.is_empty()
                && vector.initial_text.is_empty()
                && vector
                    .effective_options()
                    .is_ok_and(|effective| effective == options)
        });
        if !golden {
            unsupported.push(vector.id);
        }
    }
    assert!(
        unsupported.is_empty(),
        "baseline vectors with neither a matrix row nor a plan §3.10 golden vector: \
         {unsupported:?}"
    );
}

#[test]
fn every_engine_level_matrix_row_has_a_vector() {
    let covered: BTreeSet<String> = embedded_vectors()
        .into_iter()
        .flat_map(|vector| vector.rows)
        .collect();
    let missing: Vec<&str> = ENGINE_ROWS
        .into_iter()
        .filter(|row| !covered.contains(*row))
        .collect();
    assert!(
        missing.is_empty(),
        "matrix rows without a vector: {missing:?}"
    );
}

#[test]
fn every_plan_golden_vector_has_a_vector() {
    let vectors = embedded_vectors();
    let mut missing = Vec::new();
    for &(method, modern_tone, keys, text) in GOLDEN {
        let options = EngineOptions {
            input_method: method,
            modern_tone,
            ..EngineOptions::defaults_v1()
        };
        let found = vectors.iter().any(|vector| {
            vector.keys == keys
                && vector.text == text
                && vector.macros.is_empty()
                && vector.initial_text.is_empty()
                && vector
                    .effective_options()
                    .is_ok_and(|effective| effective == options)
        });
        if !found {
            missing.push(format!("{method:?} modern_tone={modern_tone} {keys}"));
        }
    }
    assert!(
        missing.is_empty(),
        "plan §3.10 golden vectors without a fixture: {missing:?}"
    );
}

#[test]
fn the_embedded_list_matches_the_fixture_directory() {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("fixtures")
        .join("compat");
    let mut on_disk = BTreeMap::new();
    let entries = fs::read_dir(&directory)
        .unwrap_or_else(|error| panic!("tests/fixtures/compat must exist: {error}"));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| panic!("unreadable directory entry: {error}"));
        let name = entry.file_name().to_string_lossy().into_owned();
        assert!(
            name.ends_with(".toml"),
            "{name}: only .toml fixtures belong in tests/fixtures/compat"
        );
        let text = fs::read_to_string(entry.path())
            .unwrap_or_else(|error| panic!("{name} is unreadable: {error}"));
        on_disk.insert(name, text);
    }
    let mut embedded = BTreeMap::new();
    for fixture in EMBEDDED {
        let previous = embedded.insert(fixture.name.to_owned(), fixture.text.to_owned());
        assert!(previous.is_none(), "{} is embedded twice", fixture.name);
    }
    assert_eq!(
        embedded.keys().collect::<Vec<_>>(),
        on_disk.keys().collect::<Vec<_>>(),
        "vectors::EMBEDDED must list exactly the files of tests/fixtures/compat"
    );
    for (name, text) in &on_disk {
        assert!(
            embedded.get(name) == Some(text),
            "{name}: the embedded text differs from the file"
        );
    }
}

#[test]
fn every_vector_names_an_existing_source_test() {
    let root = workspace_root();
    let mut files: BTreeMap<String, String> = BTreeMap::new();
    for vector in embedded_vectors() {
        let Some((file, test)) = vector.source.split_once("::") else {
            panic!("{}: source must be <file>::<test>", vector.id);
        };
        let text = files
            .entry(file.to_owned())
            .or_insert_with(|| fs::read_to_string(root.join(file)).unwrap_or_default());
        assert!(
            text.contains(&format!("fn {test}(")),
            "{}: {} names no test of the workspace",
            vector.id,
            vector.source
        );
    }
}

#[test]
fn no_two_vectors_share_a_setup() {
    let mut seen: BTreeMap<String, String> = BTreeMap::new();
    for vector in embedded_vectors() {
        let options = vector
            .effective_options()
            .ok()
            .and_then(|options| serde_json::to_string(&options).ok())
            .unwrap_or_default();
        let macros: Vec<(&str, &str)> = vector
            .macros
            .iter()
            .map(|entry| (entry.trigger.as_str(), entry.replacement.as_str()))
            .collect();
        let setup = format!(
            "{options}|{:?}|{}|{}|{}",
            vector.secure_field_mode(),
            serde_json::to_string(&macros).unwrap_or_default(),
            vector.initial_text,
            vector.keys
        );
        if let Some(other) = seen.insert(setup, vector.id.clone()) {
            panic!(
                "{} repeats the setup of {other}; merge their expectations",
                vector.id
            );
        }
    }
}
