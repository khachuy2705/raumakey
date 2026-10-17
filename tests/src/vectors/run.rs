//! In-process execution of one vector and its result record (design §15.3, matrix pass
//! criteria).

use core::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;

use raumakey_engine::script::{self, KeyToken, ScriptOutcome, ScriptRunError, ScriptStep};
use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_model::error::ResetReason;
use raumakey_model::host::HostContext;
use raumakey_model::options::{DEFAULTS_VERSION, EngineOptions};
use raumakey_model::policy::SecureFieldMode;
use raumakey_model::transaction::{EditTransaction, TextSimulator};
use raumakey_model::utf16;
use raumakey_policy::CompatibilityPolicy;
use serde::Serialize;

use super::{Evidence, Vector, VectorError, VectorMacro};

/// Architecture of this process, as Rust names it: `x86_64`, `x86`, or `aarch64`.
pub const PROCESS_ARCHITECTURE: &str = std::env::consts::ARCH;

/// Outcome of one vector.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Every expectation held.
    Pass,
    /// At least one [`Failure`].
    Fail,
}

/// The configuration a vector ran with: the "config revision" of design §15.3 and of the
/// matrix pass criteria.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct ConfigRevision {
    /// Version of the engine defaults the options start from (`DEFAULTS_VERSION`, plan D4).
    pub defaults_version: u32,
    /// Revision of the `raumakey.toml` store the options were read from. `None` in-process: the
    /// options are defaults v1 plus the vector's overrides, and no store is read.
    pub store_revision: Option<u64>,
}

impl ConfigRevision {
    /// Defaults v1 and no store: every in-process run.
    pub const IN_PROCESS: Self = Self {
        defaults_version: DEFAULTS_VERSION,
        store_revision: None,
    };
}

/// What kind of application a vector typed into.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ApplicationKind {
    /// A simulated document (`raumakey_model::transaction::TextSimulator`) in the runner's own
    /// process, not a Windows application.
    InProcess,
}

/// The application a vector typed into.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Application {
    /// Executable name the engine sees: `raumakey_engine::script::SCRIPT_EXE` in-process.
    pub name: String,
    /// Version of the application. In-process it is this package's version: the simulated
    /// document is built into the runner.
    pub version: String,
    /// Kind of application.
    pub kind: ApplicationKind,
}

impl Application {
    /// The in-process simulated document.
    pub fn in_process() -> Self {
        Self {
            name: script::SCRIPT_EXE.to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            kind: ApplicationKind::InProcess,
        }
    }
}

/// Operating system name and build. [`run_vector`] leaves it unset; the runner fills it in.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize)]
pub struct OsInfo {
    /// `std::env::consts::OS`: `windows`, `linux`, …
    pub name: String,
    /// Windows `CurrentBuild`, such as `26100`.
    pub build: Option<String>,
    /// Windows `UBR`, the update build revision.
    pub ubr: Option<u32>,
    /// Windows `DisplayVersion`, such as `24H2`.
    pub display_version: Option<String>,
}

/// Why a vector failed. Serialized with a `kind` tag; `step` is a zero-based key index.
///
/// `Debug` shows the kind and step only; expected and actual values leave the crate through
/// `Serialize`, in the runner's report.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Failure {
    /// The vector is invalid (see [`super::VectorError`]).
    Setup {
        /// The broken rule.
        reason: String,
    },
    /// The engine rejected the event of a key.
    EngineError {
        /// Key index.
        step: usize,
        /// The engine error.
        error: String,
    },
    /// The transaction of a key did not apply to the document, so its state was not committed.
    NotApplied {
        /// Key index.
        step: usize,
    },
    /// The run produced another number of transactions than expected.
    TransactionCount {
        /// Expected count (at least).
        expected: usize,
        /// Actual count.
        actual: usize,
    },
    /// A transaction differs.
    Transaction {
        /// Key index.
        step: usize,
        /// Expected, in matrix notation.
        expected: String,
        /// Actual, in matrix notation.
        actual: String,
    },
    /// The final document text differs.
    FinalText {
        /// Expected text.
        expected: String,
        /// Actual text.
        actual: String,
    },
    /// The engine owns another word after the last key.
    Owned {
        /// Expected word.
        expected: String,
        /// Actual word.
        actual: String,
    },
    /// The engine is idle when it should own a word, or the reverse.
    Idle {
        /// Expected idleness.
        expected: bool,
        /// Actual idleness.
        actual: bool,
    },
    /// The word the engine owns is not the end of the document.
    OwnedNotSuffix,
    /// The engine or the macro table panicked while the vector ran. The panic message is not
    /// recorded: it is not vector data.
    Panic,
}

impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Setup { .. } => f.write_str("Setup"),
            Self::EngineError { step, .. } => write!(f, "EngineError {{ step: {step} }}"),
            Self::NotApplied { step } => write!(f, "NotApplied {{ step: {step} }}"),
            Self::TransactionCount { expected, actual } => write!(
                f,
                "TransactionCount {{ expected: {expected}, actual: {actual} }}"
            ),
            Self::Transaction { step, .. } => write!(f, "Transaction {{ step: {step} }}"),
            Self::FinalText { .. } => f.write_str("FinalText"),
            Self::Owned { .. } => f.write_str("Owned"),
            Self::Idle { expected, actual } => {
                write!(f, "Idle {{ expected: {expected}, actual: {actual} }}")
            }
            Self::OwnedNotSuffix => f.write_str("OwnedNotSuffix"),
            Self::Panic => f.write_str("Panic"),
        }
    }
}

/// The record of one vector run: the fields of design §15.3 and of the matrix pass criteria
/// (test id, OS and build, architecture, application and version, configuration revision,
/// input events, transaction sequence, final text, pass/fail), plus the vector's rows,
/// evidence, and source.
#[derive(Clone, PartialEq, Eq, Serialize)]
pub struct VectorResult {
    /// Vector id.
    pub id: String,
    /// Matrix rows.
    pub rows: Vec<String>,
    /// Evidence levels: a pass meets this oracle and no other.
    pub evidence: Vec<Evidence>,
    /// Decisions behind a `raumakey-decision` level.
    pub decisions: Vec<String>,
    /// The Rust test the expectations were transcribed from.
    pub source: String,
    /// Operating system and build, filled in by the runner.
    pub os: Option<OsInfo>,
    /// Architecture of the process the engine ran in ([`PROCESS_ARCHITECTURE`]).
    pub architecture: String,
    /// Application and version.
    pub application: Application,
    /// Configuration revision.
    pub config_revision: ConfigRevision,
    /// The effective engine options; `None` when they are invalid.
    pub options: Option<EngineOptions>,
    /// The secure-field mode of the compatibility policy the run used (D25).
    pub secure_field_mode: SecureFieldMode,
    /// The vector's macro entries.
    pub macros: Vec<VectorMacro>,
    /// Document text before the first key.
    pub initial_text: String,
    /// The input events, as a key script.
    pub keys: String,
    /// Every transaction of the run, in matrix notation.
    pub transactions: Vec<String>,
    /// Document text after the last key; `None` when the run did not complete.
    pub final_text: Option<String>,
    /// Pass or fail.
    pub status: Status,
    /// Every failed expectation, in check order.
    pub failures: Vec<Failure>,
}

impl fmt::Debug for VectorResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VectorResult")
            .field("id", &self.id)
            .field("status", &self.status)
            .field("failures", &self.failures)
            .finish_non_exhaustive()
    }
}

/// Runs `vector` in-process and records the result. An invalid vector, an engine error, and a
/// panic of the engine or the macro table are each a [`Failure`] of this vector, so one bad
/// vector never stops a run of many (with the workspace's `panic = "unwind"`).
pub fn run_vector(vector: &Vector) -> VectorResult {
    run_vector_with(vector, execute)
}

/// [`run_vector`] with the step that runs the engine replaced, so that tests can make it fail.
pub(super) fn run_vector_with(
    vector: &Vector,
    execute: impl FnOnce(&Vector) -> Result<ScriptOutcome, Failure>,
) -> VectorResult {
    let mut result = VectorResult {
        id: vector.id.clone(),
        rows: vector.rows.clone(),
        evidence: vector.evidence.clone(),
        decisions: vector.decisions.clone(),
        source: vector.source.clone(),
        os: None,
        architecture: PROCESS_ARCHITECTURE.to_owned(),
        application: Application::in_process(),
        config_revision: ConfigRevision::IN_PROCESS,
        options: vector.effective_options().ok(),
        secure_field_mode: vector.secure_field_mode(),
        macros: vector.macros.clone(),
        initial_text: vector.initial_text.clone(),
        keys: vector.keys.clone(),
        transactions: Vec::new(),
        final_text: None,
        status: Status::Fail,
        failures: Vec::new(),
    };
    // Nothing outlives the closure: the engine and the table are built inside it, so a panic
    // leaves no broken state behind.
    match panic::catch_unwind(AssertUnwindSafe(|| execute(vector))) {
        Ok(Ok(outcome)) => check(vector, &outcome, &mut result),
        Ok(Err(failure)) => result.failures.push(failure),
        Err(_) => result.failures.push(Failure::Panic),
    }
    if result.failures.is_empty() {
        result.status = Status::Pass;
    }
    result
}

/// Validates the vector, builds the engine with a real `MacroTable`, and runs the key script
/// from `initial_text` the way the TIP does (see [`run_as_adapter`]).
fn execute(vector: &Vector) -> Result<ScriptOutcome, Failure> {
    let setup = |error: VectorError| Failure::Setup {
        reason: error.to_string(),
    };
    vector.validate().map_err(setup)?;
    let options = vector.effective_options().map_err(setup)?;
    let tokens = vector.tokens().map_err(setup)?;
    let table = vector.macro_table().map_err(setup)?;
    let engine = VietnameseEngine::new(Vec::new(), Arc::new(table));
    let host = script::script_host().map_err(|_| Failure::Setup {
        reason: "the script host name is invalid".to_owned(),
    })?;
    let policy = CompatibilityPolicy::new(Arc::new(vector.compatibility()));
    run_as_adapter(
        &engine,
        &options,
        &policy,
        &tokens,
        &host,
        &vector.initial_text,
    )
    .map_err(|error| Failure::EngineError {
        step: error.step,
        error: error.error.to_string(),
    })
}

/// Runs `tokens` in `host` from a document holding `initial_text`, as the session core does
/// (`raumakey_session`, D25): the compatibility policy is resolved for each key.
///
/// - A key of a secure field whose policy does not allow Vietnamese (`Bypass`, the default)
///   never reaches the engine: it passes through and ends the word
///   (`Tx(0,"",false,None,true)`), so neither transformation nor macros apply there.
/// - Every other key runs through the engine with the vector's options, Vietnamese and
///   macros turned off where the policy says so (in `Restricted` mode a secure field keeps
///   Vietnamese and loses macros).
///
/// As in `raumakey_engine::script::run_script_from`, the next state is committed only when
/// the simulated document accepted the transaction; otherwise the engine resets and the key
/// passes through.
fn run_as_adapter(
    engine: &VietnameseEngine,
    options: &EngineOptions,
    policy: &CompatibilityPolicy,
    tokens: &[KeyToken],
    host: &HostContext,
    initial_text: &str,
) -> Result<ScriptOutcome, ScriptRunError> {
    let mut simulator = TextSimulator::with_text(initial_text);
    let mut state = EngineState::new();
    let mut secure = false;
    let mut steps = Vec::with_capacity(tokens.len());
    for (step, token) in tokens.iter().enumerate() {
        match token {
            KeyToken::SecureOn => secure = true,
            KeyToken::SecureOff => secure = false,
            _ => {}
        }
        let event = script::event_for(token, host, secure);
        let resolved = policy.resolve(&event.host);
        let (transaction, next) = if event.host.is_secure_field && !resolved.vietnamese_allowed {
            (
                EditTransaction::pass_through_reset(),
                engine.reset(&state, ResetReason::ContextChanged),
            )
        } else {
            let mut effective = options.clone();
            effective.enabled &= resolved.vietnamese_allowed;
            effective.macros_enabled &= resolved.macro_allowed;
            let result = engine
                .process(&event, &effective, &state)
                .map_err(|error| ScriptRunError { step, error })?;
            (result.transaction, result.next)
        };
        let accepted = simulator.apply(&event, &transaction).is_ok();
        if accepted {
            state = next;
        } else {
            state = engine.reset(&state, ResetReason::ApplyFailed);
            // The key passes through; a pass-through edit always applies.
            let _ = simulator.apply(&event, &EditTransaction::pass_through_reset());
        }
        steps.push(ScriptStep {
            token: *token,
            event,
            transaction,
            accepted,
        });
    }
    Ok(ScriptOutcome {
        final_text: simulator.as_string(),
        steps,
        final_state: state,
    })
}

/// Records the run in `result` and adds a [`Failure`] for every expectation that does not
/// hold.
pub(super) fn check(vector: &Vector, outcome: &ScriptOutcome, result: &mut VectorResult) {
    let actual: Vec<String> = outcome
        .steps
        .iter()
        .map(|step| script::format_transaction(&step.transaction))
        .collect();
    let failures = &mut result.failures;
    for (step, record) in outcome.steps.iter().enumerate() {
        if !record.accepted {
            failures.push(Failure::NotApplied { step });
        }
    }
    if let Some(expected) = &vector.transactions {
        if expected.len() != actual.len() {
            failures.push(Failure::TransactionCount {
                expected: expected.len(),
                actual: actual.len(),
            });
        }
        for (step, (expected, actual)) in expected.iter().zip(&actual).enumerate() {
            if expected != actual {
                failures.push(Failure::Transaction {
                    step,
                    expected: expected.clone(),
                    actual: actual.clone(),
                });
            }
        }
    }
    for expectation in &vector.steps {
        match actual.get(expectation.index) {
            Some(actual) if *actual == expectation.transaction => {}
            Some(actual) => failures.push(Failure::Transaction {
                step: expectation.index,
                expected: expectation.transaction.clone(),
                actual: actual.clone(),
            }),
            None => failures.push(Failure::TransactionCount {
                expected: expectation.index + 1,
                actual: actual.len(),
            }),
        }
    }
    if outcome.final_text != vector.text {
        failures.push(Failure::FinalText {
            expected: vector.text.clone(),
            actual: outcome.final_text.clone(),
        });
    }
    let owned_units = outcome.final_state.owned_utf16();
    let owned = String::from_utf16_lossy(owned_units);
    if let Some(expected) = &vector.owned
        && *expected != owned
    {
        failures.push(Failure::Owned {
            expected: expected.clone(),
            actual: owned,
        });
    }
    let idle = outcome.final_state.is_idle();
    if let Some(expected) = vector.idle
        && expected != idle
    {
        failures.push(Failure::Idle {
            expected,
            actual: idle,
        });
    }
    if !utf16::encode(&outcome.final_text).ends_with(owned_units) {
        failures.push(Failure::OwnedNotSuffix);
    }
    result.transactions = actual;
    result.final_text = Some(outcome.final_text.clone());
}
