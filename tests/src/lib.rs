//! Raumakey shared fixtures, contract suites, and fuzz smoke targets.
//!
//! - [`fuzz_targets`]: the fuzz target bodies (plan §12, design §15.1). The deterministic smoke
//!   suite (`tests/fuzz_smoke.rs`) runs them under `cargo test`; the cargo-fuzz project
//!   (`fuzz/`, not created yet) is meant to drive the same functions with coverage feedback.
//! - [`vectors`]: the machine-readable compatibility vectors of `tests/fixtures/compat/`
//!   (plan §12): schema, loader, and in-process executor.
//! - [`runner`]: the command line, environment probe, and JSON report of the
//!   `raumakey-test-runner` binary (design §15.3).
//!
//! The integration tests hold the suites: `tests/fuzz_smoke.rs`, `tests/macro_engine.rs` (the
//! engine with the real `raumakey_macro::MacroTable`, matrix #47–#54), `tests/compat_vectors.rs`
//! (every vector), and `tests/test_runner.rs` (the runner's command line, report, and binary).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod fuzz_targets;
pub mod runner;
pub mod vectors;
