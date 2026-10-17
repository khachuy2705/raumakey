//! Machine-readable compatibility vectors (plan §12, design §15.3): the fixture schema, the
//! loader, and an in-process executor.
//!
//! # Fixtures
//!
//! `tests/fixtures/compat/*.toml` hold vectors for every engine-level row of
//! `docs/reference/evkey_compatibility_test_matrix.md` (A1–A18, B21–B23, B29, D47–D54) and for
//! the golden vectors of plan §3.10. Their expectations are transcribed from the engine's own
//! tests (`crates/raumakey-engine/tests/matrix.rs` and `golden.rs`, named in each vector's
//! `source`), never invented:
//!
//! - A vector asserts a transaction only where its source test asserts that transaction.
//! - `text` is always present. Where the source asserts only transactions, it is the text those
//!   transactions produce from `initial_text` (plan §2.7). Where the source asserts that the
//!   result is NFD and canonically equivalent to an NFC string, it is that string's NFD form,
//!   the only string with both properties.
//! - Assertions that no key script can express (raw modifier events such as Alt or Caps Lock,
//!   macro lookup calls, the per-step undo relation of A18) stay in the source tests.
//!
//! Every file is compiled into the crate ([`EMBEDDED`]), so a packaged runner needs no external
//! files; `tests/compat_vectors.rs` checks that the list names exactly the files of the
//! directory.
//!
//! # Schema, version 1
//!
//! ```toml
//! schema_version = 1
//!
//! [[vector]]
//! id = "a01-telex-aa"
//! rows = ["A1"]
//! evidence = ["baseline"]
//! source = "crates/raumakey-engine/tests/matrix.rs::a01_telex_double_a_makes_circumflex"
//! method = "telex"
//! keys = "aa"
//! transactions = ['Tx(0,"",false,None,false)', 'Tx(1,"â",true,None,false)']
//! text = "â"
//! ```
//!
//! A file holds `schema_version` ([`SCHEMA_VERSION`]) and one or more `[[vector]]` tables.
//! Unknown keys are errors.
//!
//! | Key | Required | Meaning |
//! |---|---|---|
//! | `id` | yes | Unique across all files: groups of lowercase ASCII letters and digits joined by `-`, at most [`MAX_ID_LEN`] (32) characters, so that every id is a valid `raumakey_model::diag::VectorId`, the form in which a diagnostic names a vector (plan §2.10, design §14) |
//! | `rows` | no | Matrix rows the vector covers: `A1`–`A20`, `B21`–`B29`, `C30`–`C43`, `D44`–`D58`. Golden vectors have none |
//! | `evidence` | yes | Evidence levels, see below: `baseline`, `proposed`, `raumakey-decision` |
//! | `decisions` | with `raumakey-decision` only | The decisions the expected value rests on: `D<n>`, `plan §<section>`, or `design §<section>` |
//! | `source` | yes | `<file>::<test>`: the Rust test the expectations were transcribed from, relative to the workspace root |
//! | `method` | yes | `telex`, `vni`, or `viqr` |
//! | `options` | no | Overrides of [`EngineOptions::defaults_v1`] (plan D4): `enabled`, `normalization` (`nfc`, `nfd`, `preserve`), `modern_tone`, `free_marking`, `spell_check`, `auto_restore_invalid`, `macros_enabled`, `macros_when_disabled`, `auto_case_macro`, `context_limit_codepoints`. An override may repeat a default to state a precondition of the matrix row |
//! | `macros` | no | `{ trigger, replacement }` entries of a real `raumakey_macro::MacroTable`, with identifiers 1, 2, … in order, enabled, and the default boundary and case policies |
//! | `initial_text` | no | Document text before the first key; empty by default |
//! | `keys` | yes | A non-empty key script in the grammar of `raumakey_engine::script::parse_keys` |
//! | `transactions` | no | Every transaction, one per key, in `raumakey_engine::script::format_transaction` notation |
//! | `steps` | no, not with `transactions` | `{ index, transaction }`: the transactions of selected keys, by ascending zero-based key index |
//! | `text` | yes | The document text after the last key |
//! | `owned` | no | The word the engine owns after the last key |
//! | `idle` | no, not with `owned` | Whether the engine owns no word after the last key |
//!
//! Key scripts type every character as itself; `{BS}`, `{SPACE}`, `{SHIFT+SPACE}`, `{ENTER}`,
//! `{TAB}`, `{NAV}`, `{MOUSE}`, `{FOCUS}`, `{CTRL+x}`, `{ALTGR+x}`, `{SECURE}` … `{/SECURE}`,
//! `{LBRACE}`, and `{RBRACE}` are special keys (see `raumakey_engine::script`). Transactions
//! read `Tx(delete,"insert",consume,replay,reset)`, as in the matrix.
//!
//! # Evidence
//!
//! `evidence` lists, in the matrix's order, the levels that the matrix's evidence column gives
//! for the vector's rows: `baseline` for a "Baseline oracle" (UniKey source) and `proposed` for
//! a "Proposed" oracle (a new specification or safety decision). Observed integration describes
//! transport, not output, and adds no level. Plan §3.10 golden vectors without a row, and such
//! a vector ended by a space (as the plan's `after` and `windows` vectors are), are `baseline`:
//! the matrix takes the Telex, VNI, and VIQR mappings from the UniKey baseline. No other vector
//! without a row is `baseline`; the words after `qu` that the plan's rhyme table omits carry
//! `raumakey-decision` alone.
//!
//! `raumakey-decision` is added when the expected value rests on a Raumakey decision that the
//! documents name, and `decisions` names it: a plan decision (`D7`, Shift+Space commits the word
//! unchanged), a plan rule that design §20.5 records as replacing a matrix expectation
//! (`plan §3.9`, the fallback of row #18), or a code decision that design §20.9 lists and the
//! plan does not record yet (`design §20.9`: VIQR-escaped boundary punctuation commits the word;
//! the extra rhymes after `qu`; `transformed` follows the current tone and marks). Every VIQR
//! vector whose expected values would change if escaped boundary punctuation stayed in the word,
//! as plan §3.6 has it (keys after the escaped key, or asserted transactions or engine word),
//! names `design §20.9`. Other vectors may still depend on implementation choices that the
//! documents do not single out. `tests/compat_vectors.rs` checks the rules of this paragraph
//! and the previous one that can be checked mechanically.
//!
//! A pass means that the engine produced the vector's expected values; the label says which
//! kind of oracle those values stand for. Nothing here was compared with UniKey or EVKey: no
//! vector is EVKey-observed, a `baseline` pass is never evidence of EVKey behavior (matrix pass
//! criteria), and an in-process pass says nothing about TSF or a real Windows application
//! (design §19).
//!
//! # Execution
//!
//! [`run_vector`] runs one vector in-process with the real `VietnameseEngine` and a real
//! `MacroTable`, through `raumakey_engine::script::run_script_with` against the model's
//! `TextSimulator`, and returns a [`VectorResult`] with the fields of design §15.3. Besides the
//! expectations it requires every transaction to apply and the engine's word to stay the
//! document's suffix.
//!
//! The types that hold key scripts, texts, or macros redact them in `Debug`; their content
//! leaves the crate only through `Serialize`, in the runner's report.

mod run;
#[cfg(test)]
mod tests;

pub use run::{
    Application, ApplicationKind, ConfigRevision, Failure, OsInfo, PROCESS_ARCHITECTURE, Status,
    VectorResult, run_vector,
};

use core::fmt;
use std::collections::BTreeSet;

use raumakey_engine::script::{self, KeyToken, ScriptError};
use raumakey_macro::{MacroError, MacroTable};
use raumakey_model::diag::VectorId;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization, OptionsError};
use serde::{Deserialize, Serialize};

/// Schema version of the fixture files this loader reads.
pub const SCHEMA_VERSION: u32 = 1;

/// Longest vector id, in bytes (ids are ASCII): the limit of
/// `raumakey_model::diag::VectorId` (plan §2.10), the only form in which a diagnostic may name a
/// test vector (design §14).
pub const MAX_ID_LEN: usize = 32;

/// One fixture file compiled into the crate.
#[derive(Clone, Copy)]
pub struct EmbeddedFixture {
    /// File name in `tests/fixtures/compat/`.
    pub name: &'static str,
    /// File content.
    pub text: &'static str,
}

impl fmt::Debug for EmbeddedFixture {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EmbeddedFixture")
            .field("name", &self.name)
            .field("bytes", &self.text.len())
            .finish()
    }
}

/// An [`EmbeddedFixture`] for a file of `tests/fixtures/compat/`.
macro_rules! fixture {
    ($name:literal) => {
        EmbeddedFixture {
            name: $name,
            text: include_str!(concat!("../fixtures/compat/", $name)),
        }
    };
}

/// Every fixture file of `tests/fixtures/compat/`, compiled into the crate.
pub const EMBEDDED: &[EmbeddedFixture] = &[
    fixture!("matrix-a-engine.toml"),
    fixture!("matrix-b-modifiers.toml"),
    fixture!("matrix-d-macros.toml"),
    fixture!("golden-plan.toml"),
    fixture!("golden-after-qu.toml"),
];

/// Evidence level of a vector's expected value (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Evidence {
    /// The matrix cites the UniKey baseline ("Baseline oracle"). A pass shows agreement with
    /// that baseline, never with an observed EVKey run.
    Baseline,
    /// The matrix marks the expected value as a proposed oracle: a new specification or safety
    /// decision.
    Proposed,
    /// The expected value rests on a Raumakey decision named in [`Vector::decisions`].
    RaumakeyDecision,
}

/// Overrides of [`EngineOptions::defaults_v1`]; `None` keeps the default.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OptionOverrides {
    /// [`EngineOptions::enabled`].
    #[serde(default)]
    pub enabled: Option<bool>,
    /// [`EngineOptions::normalization`].
    #[serde(default)]
    pub normalization: Option<Normalization>,
    /// [`EngineOptions::modern_tone`].
    #[serde(default)]
    pub modern_tone: Option<bool>,
    /// [`EngineOptions::free_marking`].
    #[serde(default)]
    pub free_marking: Option<bool>,
    /// [`EngineOptions::spell_check`].
    #[serde(default)]
    pub spell_check: Option<bool>,
    /// [`EngineOptions::auto_restore_invalid`].
    #[serde(default)]
    pub auto_restore_invalid: Option<bool>,
    /// [`EngineOptions::macros_enabled`].
    #[serde(default)]
    pub macros_enabled: Option<bool>,
    /// [`EngineOptions::macros_when_disabled`].
    #[serde(default)]
    pub macros_when_disabled: Option<bool>,
    /// [`EngineOptions::auto_case_macro`].
    #[serde(default)]
    pub auto_case_macro: Option<bool>,
    /// [`EngineOptions::context_limit_codepoints`].
    #[serde(default)]
    pub context_limit_codepoints: Option<u32>,
}

impl OptionOverrides {
    /// Writes every override into `options`.
    pub fn apply(&self, options: &mut EngineOptions) {
        let Self {
            enabled,
            normalization,
            modern_tone,
            free_marking,
            spell_check,
            auto_restore_invalid,
            macros_enabled,
            macros_when_disabled,
            auto_case_macro,
            context_limit_codepoints,
        } = *self;
        set(&mut options.enabled, enabled);
        set(&mut options.normalization, normalization);
        set(&mut options.modern_tone, modern_tone);
        set(&mut options.free_marking, free_marking);
        set(&mut options.spell_check, spell_check);
        set(&mut options.auto_restore_invalid, auto_restore_invalid);
        set(&mut options.macros_enabled, macros_enabled);
        set(&mut options.macros_when_disabled, macros_when_disabled);
        set(&mut options.auto_case_macro, auto_case_macro);
        set(
            &mut options.context_limit_codepoints,
            context_limit_codepoints,
        );
    }
}

fn set<T>(field: &mut T, value: Option<T>) {
    if let Some(value) = value {
        *field = value;
    }
}

/// A macro entry of a vector.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VectorMacro {
    /// Trigger text.
    pub trigger: String,
    /// Replacement text.
    pub replacement: String,
}

impl fmt::Debug for VectorMacro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "VectorMacro {{ trigger: <{} chars>, replacement: <{} chars> }}",
            self.trigger.chars().count(),
            self.replacement.chars().count()
        )
    }
}

/// The expected transaction of one key.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StepExpectation {
    /// Zero-based key index.
    pub index: usize,
    /// The transaction in `raumakey_engine::script::format_transaction` notation.
    pub transaction: String,
}

impl fmt::Debug for StepExpectation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "StepExpectation {{ index: {}, .. }}", self.index)
    }
}

/// One compatibility vector (see the module docs for the schema).
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vector {
    /// Unique id.
    pub id: String,
    /// Matrix rows, such as `A1` or `D47`.
    #[serde(default)]
    pub rows: Vec<String>,
    /// Evidence levels.
    pub evidence: Vec<Evidence>,
    /// Decisions behind a `raumakey-decision` level.
    #[serde(default)]
    pub decisions: Vec<String>,
    /// `<file>::<test>` the expectations were transcribed from.
    pub source: String,
    /// Input method.
    pub method: InputMethod,
    /// Overrides of defaults v1.
    #[serde(default)]
    pub options: OptionOverrides,
    /// Macro entries.
    #[serde(default)]
    pub macros: Vec<VectorMacro>,
    /// Document text before the first key.
    #[serde(default)]
    pub initial_text: String,
    /// Key script.
    pub keys: String,
    /// Every expected transaction, one per key.
    #[serde(default)]
    pub transactions: Option<Vec<String>>,
    /// Expected transactions of selected keys.
    #[serde(default)]
    pub steps: Vec<StepExpectation>,
    /// Expected document text after the last key.
    pub text: String,
    /// Expected word the engine owns after the last key.
    #[serde(default)]
    pub owned: Option<String>,
    /// Whether the engine is expected to own no word after the last key.
    #[serde(default)]
    pub idle: Option<bool>,
}

impl fmt::Debug for Vector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Vector")
            .field("id", &self.id)
            .field("rows", &self.rows)
            .field("evidence", &self.evidence)
            .field("method", &self.method)
            .field("macros", &self.macros.len())
            .field(
                "keys",
                &format_args!("<{} chars>", self.keys.chars().count()),
            )
            .finish_non_exhaustive()
    }
}

/// Why a vector is invalid. Variants carry no key script, text, or macro.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VectorError {
    /// `id` is empty, longer than [`MAX_ID_LEN`], or not groups of lowercase ASCII letters and
    /// digits joined by `-`. Every id that passes is a valid `VectorId`.
    InvalidId,
    /// A row is not a matrix row, or repeats.
    InvalidRow(String),
    /// `evidence` is empty or repeats a level.
    InvalidEvidence,
    /// `decisions` is empty although `evidence` holds `raumakey-decision`, is not empty
    /// although it does not, or holds a malformed or repeated decision.
    InvalidDecisions,
    /// `source` is not `<file>.rs::<test>`.
    InvalidSource,
    /// `method` is `user`: vectors carry no user key maps.
    UnsupportedMethod,
    /// The effective options are invalid.
    InvalidOptions(OptionsError),
    /// The key script does not parse (`Some`), or is empty (`None`).
    InvalidKeys(Option<ScriptError>),
    /// `transactions` does not hold one entry per key.
    TransactionCount {
        /// Keys in the script.
        keys: usize,
        /// Entries in `transactions`.
        transactions: usize,
    },
    /// A step index is out of range, repeats, or is not ascending.
    InvalidStep {
        /// The offending index.
        index: usize,
    },
    /// Both `transactions` and `steps`.
    TransactionsAndSteps,
    /// Both `owned` and `idle`.
    OwnedAndIdle,
    /// The macro table rejects the entries.
    InvalidMacros(MacroError),
}

impl fmt::Display for VectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId => f.write_str("invalid id"),
            Self::InvalidRow(row) => write!(f, "invalid or repeated matrix row {row}"),
            Self::InvalidEvidence => f.write_str("evidence is empty or repeats a level"),
            Self::InvalidDecisions => f.write_str(
                "decisions must name the decisions of a raumakey-decision level, and only then",
            ),
            Self::InvalidSource => f.write_str("source must be <file>.rs::<test>"),
            Self::UnsupportedMethod => f.write_str("vectors support telex, vni, and viqr only"),
            Self::InvalidOptions(error) => write!(f, "invalid options: {error}"),
            Self::InvalidKeys(Some(error)) => write!(f, "invalid key script: {error}"),
            Self::InvalidKeys(None) => f.write_str("the key script is empty"),
            Self::TransactionCount { keys, transactions } => write!(
                f,
                "{transactions} transactions for {keys} keys; give one per key or use steps"
            ),
            Self::InvalidStep { index } => write!(
                f,
                "step {index} is out of range, repeated, or not in ascending order"
            ),
            Self::TransactionsAndSteps => f.write_str("use either transactions or steps"),
            Self::OwnedAndIdle => f.write_str("use either owned or idle"),
            Self::InvalidMacros(error) => write!(f, "invalid macros: {error}"),
        }
    }
}

impl std::error::Error for VectorError {}

impl Vector {
    /// Checks every rule of the schema that does not need other vectors.
    pub fn validate(&self) -> Result<(), VectorError> {
        if !is_id(&self.id) {
            return Err(VectorError::InvalidId);
        }
        let mut rows = BTreeSet::new();
        for row in &self.rows {
            if !is_matrix_row(row) || !rows.insert(row.as_str()) {
                return Err(VectorError::InvalidRow(row.clone()));
            }
        }
        let levels: BTreeSet<Evidence> = self.evidence.iter().copied().collect();
        if levels.is_empty() || levels.len() != self.evidence.len() {
            return Err(VectorError::InvalidEvidence);
        }
        let decisions: BTreeSet<&str> = self.decisions.iter().map(String::as_str).collect();
        let decided = levels.contains(&Evidence::RaumakeyDecision);
        if decided == decisions.is_empty()
            || decisions.len() != self.decisions.len()
            || !decisions.iter().all(|decision| is_decision(decision))
        {
            return Err(VectorError::InvalidDecisions);
        }
        if !is_source(&self.source) {
            return Err(VectorError::InvalidSource);
        }
        self.effective_options()?;
        let keys = self.tokens()?.len();
        if let Some(transactions) = &self.transactions {
            if !self.steps.is_empty() {
                return Err(VectorError::TransactionsAndSteps);
            }
            if transactions.len() != keys {
                return Err(VectorError::TransactionCount {
                    keys,
                    transactions: transactions.len(),
                });
            }
        }
        let mut next = 0;
        for step in &self.steps {
            if step.index < next || step.index >= keys {
                return Err(VectorError::InvalidStep { index: step.index });
            }
            next = step.index + 1;
        }
        if self.owned.is_some() && self.idle.is_some() {
            return Err(VectorError::OwnedAndIdle);
        }
        self.macro_table()?;
        Ok(())
    }

    /// Defaults v1 with [`Vector::method`] and the overrides; fails for `user` and for options
    /// that [`EngineOptions::validate`] rejects.
    pub fn effective_options(&self) -> Result<EngineOptions, VectorError> {
        if self.method == InputMethod::UserDefined {
            return Err(VectorError::UnsupportedMethod);
        }
        let mut options = EngineOptions {
            input_method: self.method,
            ..EngineOptions::defaults_v1()
        };
        self.options.apply(&mut options);
        options.validate().map_err(VectorError::InvalidOptions)?;
        Ok(options)
    }

    /// The parsed key script.
    pub fn tokens(&self) -> Result<Vec<KeyToken>, VectorError> {
        let tokens = script::parse_keys(&self.keys)
            .map_err(|error| VectorError::InvalidKeys(Some(error)))?;
        if tokens.is_empty() {
            return Err(VectorError::InvalidKeys(None));
        }
        Ok(tokens)
    }

    /// The macro entries: identifiers 1, 2, … in order, enabled, default policies.
    pub fn macro_entries(&self) -> Vec<MacroEntry> {
        self.macros
            .iter()
            .zip(1_u64..)
            .map(|(entry, id)| MacroEntry {
                id: MacroId(id),
                trigger: entry.trigger.clone(),
                replacement: entry.replacement.clone(),
                enabled: true,
                boundary: BoundaryPolicy::default(),
                case_policy: CasePolicy::default(),
            })
            .collect()
    }

    /// The real macro table of the vector.
    pub fn macro_table(&self) -> Result<MacroTable, VectorError> {
        MacroTable::new(self.macro_entries()).map_err(VectorError::InvalidMacros)
    }
}

/// Groups of lowercase ASCII letters and digits joined by `-`, at most [`MAX_ID_LEN`] long,
/// and so a valid [`VectorId`].
fn is_id(id: &str) -> bool {
    id.len() <= MAX_ID_LEN
        && VectorId::new(id).is_ok()
        && id.split('-').all(|group| {
            !group.is_empty()
                && group
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// A decimal number without a leading zero.
fn is_number(text: &str) -> bool {
    !text.is_empty() && !text.starts_with('0') && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// `A1`–`A20`, `B21`–`B29`, `C30`–`C43`, or `D44`–`D58`: the sections of the matrix.
fn is_matrix_row(row: &str) -> bool {
    let mut chars = row.chars();
    let Some(section) = chars.next() else {
        return false;
    };
    let digits = chars.as_str();
    if !is_number(digits) {
        return false;
    }
    let Ok(number) = digits.parse::<u32>() else {
        return false;
    };
    let rows = match section {
        'A' => 1..=20,
        'B' => 21..=29,
        'C' => 30..=43,
        'D' => 44..=58,
        _ => return false,
    };
    rows.contains(&number)
}

/// `D<n>`, `plan §<section>`, or `design §<section>`, where a section is dot-separated numbers.
fn is_decision(decision: &str) -> bool {
    if let Some(number) = decision.strip_prefix('D') {
        return is_number(number);
    }
    ["plan §", "design §"].iter().any(|prefix| {
        decision.strip_prefix(prefix).is_some_and(|section| {
            section
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        })
    })
}

/// `<file>.rs::<test>`, where the test name is an identifier.
fn is_source(source: &str) -> bool {
    let Some((file, test)) = source.split_once("::") else {
        return false;
    };
    file.len() > ".rs".len()
        && file.ends_with(".rs")
        && !test.is_empty()
        && test
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

/// What is wrong with a fixture file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoadErrorKind {
    /// Not TOML, or not this schema: the parser's message (which may quote the fixture) and
    /// the 1-based line, when known.
    Syntax {
        /// Line of the error.
        line: Option<usize>,
        /// Parser message.
        message: String,
    },
    /// `schema_version` is missing or not [`SCHEMA_VERSION`].
    UnsupportedSchema,
    /// The file holds no vector.
    Empty,
    /// Two vectors share an id, in one file or across files.
    DuplicateId(String),
    /// A vector breaks a rule of the schema.
    Invalid {
        /// The vector's id.
        id: String,
        /// The broken rule.
        error: VectorError,
    },
}

/// A fixture file failed to load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    /// Fixture file name.
    pub file: String,
    /// What is wrong.
    pub kind: LoadErrorKind,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.file)?;
        match &self.kind {
            LoadErrorKind::Syntax {
                line: Some(line),
                message,
            } => write!(f, "line {line}: {message}"),
            LoadErrorKind::Syntax {
                line: None,
                message,
            } => f.write_str(message),
            LoadErrorKind::UnsupportedSchema => {
                write!(f, "schema_version must be {SCHEMA_VERSION}")
            }
            LoadErrorKind::Empty => f.write_str("the file holds no vector"),
            LoadErrorKind::DuplicateId(id) => write!(f, "vector id {id} is not unique"),
            LoadErrorKind::Invalid { id, error } => write!(f, "vector {id}: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// The part of a file read before the schema version is known.
#[derive(Deserialize)]
struct Header {
    schema_version: Option<toml::Value>,
}

/// A fixture file of schema version 1.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureFile {
    /// Checked through [`Header`] first.
    #[serde(rename = "schema_version")]
    _schema_version: u32,
    #[serde(default, rename = "vector")]
    vectors: Vec<Vector>,
}

/// A parser error with the 1-based line where its span starts.
fn syntax(text: &str, error: &toml::de::Error) -> LoadErrorKind {
    let line = error.span().map(|span| {
        let before = text.as_bytes().get(..span.start).unwrap_or(text.as_bytes());
        before.iter().filter(|&&byte| byte == b'\n').count() + 1
    });
    LoadErrorKind::Syntax {
        line,
        message: error.message().to_owned(),
    }
}

/// Parses and validates one fixture file named `file`.
pub fn parse_fixture(file: &str, text: &str) -> Result<Vec<Vector>, LoadError> {
    let fail = |kind| LoadError {
        file: file.to_owned(),
        kind,
    };
    let header: Header = toml::from_str(text).map_err(|error| fail(syntax(text, &error)))?;
    let version = header
        .schema_version
        .as_ref()
        .and_then(toml::Value::as_integer);
    if version != Some(i64::from(SCHEMA_VERSION)) {
        return Err(fail(LoadErrorKind::UnsupportedSchema));
    }
    let parsed: FixtureFile = toml::from_str(text).map_err(|error| fail(syntax(text, &error)))?;
    if parsed.vectors.is_empty() {
        return Err(fail(LoadErrorKind::Empty));
    }
    let mut ids = BTreeSet::new();
    for vector in &parsed.vectors {
        vector.validate().map_err(|error| {
            fail(LoadErrorKind::Invalid {
                id: vector.id.clone(),
                error,
            })
        })?;
        if !ids.insert(vector.id.as_str()) {
            return Err(fail(LoadErrorKind::DuplicateId(vector.id.clone())));
        }
    }
    Ok(parsed.vectors)
}

/// Parses every file, in order, and checks that ids are unique across files.
pub fn load(fixtures: &[EmbeddedFixture]) -> Result<Vec<Vector>, LoadError> {
    let mut vectors: Vec<Vector> = Vec::new();
    let mut ids = BTreeSet::new();
    for fixture in fixtures {
        for vector in parse_fixture(fixture.name, fixture.text)? {
            if !ids.insert(vector.id.clone()) {
                return Err(LoadError {
                    file: fixture.name.to_owned(),
                    kind: LoadErrorKind::DuplicateId(vector.id),
                });
            }
            vectors.push(vector);
        }
    }
    Ok(vectors)
}

/// Every vector of [`EMBEDDED`].
pub fn load_embedded() -> Result<Vec<Vector>, LoadError> {
    load(EMBEDDED)
}
