//! Deterministic smoke run of every fuzz target (plan §12, design §15.1).
//!
//! Each test runs one target of [`raumakey_tests::fuzz_targets`] over a hand-written corpus of
//! edge cases ([`corpus`]), then over inputs from a seeded splitmix64 generator ([`inputs`],
//! [`rng`]). The cargo-fuzz project (`fuzz/`, not created yet) is meant to drive the same
//! target bodies with coverage feedback; here they run under `cargo test` on every host,
//! without instrumentation.
//!
//! | Test | Seed | Default inputs |
//! |---|---|---|
//! | `engine_sequences` | `0x5241_554D_414B_4559` | 3,000 |
//! | `malformed_utf16` | `0x0000_0000_D800_DC00` | 3,000 |
//! | `config_documents` | `0x636F_6E66_6967_0001` | 3,000 |
//! | `config_concurrency` | `0x636F_6E66_6967_0046` | 16 |
//! | `macro_files` | `0x6D61_6372_6F00_0001` | 3,000 |
//! | `ipc_frames` | `0x524D_4B31_0000_0001` | 8,000 |
//! | `legacy_sequence_wrap` (`evkey-compat`) | `0xFFFF_FFFF_0000_1038` | 5,000 |
//! | `transaction_application` | `0x0000_2000_0000_0001` | 50,000 |
//!
//! With the default parallel test threads, the defaults keep the whole suite well within
//! 15 seconds of a debug build on the development host: about 3 s, with or without
//! `evkey-compat` (measured 2026-10-17). On one thread (`--test-threads=1`) the same runs took
//! 14 to 15 s, but an earlier review measured up to 22 s there even before `config_concurrency`
//! was added: the host mixes performance and efficiency cores, so single-threaded times vary
//! with scheduling, and `config_concurrency` also depends on the disk, because it writes and
//! flushes real files. A CI that runs tests on one thread should expect the longer times.
//! `RAUMAKEY_FUZZ_ITERATIONS=<n>` replaces every default (`0` runs only the corpus); larger
//! values make a longer soak run with the same seeds.
//!
//! A failing input is identified on standard error by its target and either its corpus index
//! or its seed and input number; the generator is deterministic, so that identifies the bytes.
//! Inputs themselves are never printed.

// The helpers live in `fuzz_smoke/` because every `.rs` file directly in `tests/` is a test
// target of its own.
#[path = "fuzz_smoke/corpus.rs"]
mod corpus;
#[path = "fuzz_smoke/inputs.rs"]
mod inputs;
#[path = "fuzz_smoke/rng.rs"]
mod rng;

use raumakey_tests::fuzz_targets;
use rng::SplitMix64;

/// Seed of `engine_sequences` ("RAUMAKEY" in ASCII).
const SEED_ENGINE: u64 = 0x5241_554D_414B_4559;
/// Seed of `malformed_utf16`.
const SEED_UTF16: u64 = 0x0000_0000_D800_DC00;
/// Seed of `config_documents`.
const SEED_CONFIG: u64 = 0x636F_6E66_6967_0001;
/// Seed of `config_concurrency` ("config" and matrix row #46).
const SEED_CONFIG_CONCURRENCY: u64 = 0x636F_6E66_6967_0046;
/// Seed of `macro_files`.
const SEED_MACROS: u64 = 0x6D61_6372_6F00_0001;
/// Seed of `ipc_frames` ("RMK1" and the protocol version).
const SEED_IPC: u64 = 0x524D_4B31_0000_0001;
/// Seed of `legacy_sequence_wrap`.
#[cfg(feature = "evkey-compat")]
const SEED_LEGACY: u64 = 0xFFFF_FFFF_0000_1038;
/// Seed of `transaction_application`.
const SEED_TRANSACTION: u64 = 0x0000_2000_0000_0001;

/// Default generated inputs of `engine_sequences`.
const DEFAULT_ENGINE: usize = 3_000;
/// Default generated inputs of `malformed_utf16`.
const DEFAULT_UTF16: usize = 3_000;
/// Default generated inputs of `config_documents`.
const DEFAULT_CONFIG: usize = 3_000;
/// Default generated inputs of `config_concurrency`: each commit writes, flushes, and swaps a
/// real file, so far fewer inputs fit the time budget.
const DEFAULT_CONFIG_CONCURRENCY: usize = 16;
/// Default generated inputs of `macro_files`.
const DEFAULT_MACROS: usize = 3_000;
/// Default generated inputs of `ipc_frames`.
const DEFAULT_IPC: usize = 8_000;
/// Default generated inputs of `legacy_sequence_wrap`.
#[cfg(feature = "evkey-compat")]
const DEFAULT_LEGACY: usize = 5_000;
/// Default generated inputs of `transaction_application`.
const DEFAULT_TRANSACTION: usize = 50_000;

/// Environment variable that replaces every default input count.
const ITERATIONS_ENV: &str = "RAUMAKEY_FUZZ_ITERATIONS";

/// The number of generated inputs: `RAUMAKEY_FUZZ_ITERATIONS` when set, else `default`.
fn iterations(default: usize) -> usize {
    match std::env::var(ITERATIONS_ENV) {
        Ok(value) => match value.trim().parse() {
            Ok(count) => count,
            Err(_) => panic!("{ITERATIONS_ENV} must be a non-negative integer"),
        },
        Err(_) => default,
    }
}

/// Names the input being run when a target panics.
struct Context {
    target: &'static str,
    source: String,
}

impl Drop for Context {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!(
                "fuzz target `{}` found a problem with {}",
                self.target, self.source
            );
        }
    }
}

/// Runs `target` over `corpus`, then over `default` (or `RAUMAKEY_FUZZ_ITERATIONS`) inputs from
/// `generate` seeded with `seed`.
fn smoke(
    name: &'static str,
    target: fn(&[u8]),
    corpus: &[Vec<u8>],
    seed: u64,
    default: usize,
    generate: fn(&mut SplitMix64) -> Vec<u8>,
) {
    for (index, input) in corpus.iter().enumerate() {
        let _context = Context {
            target: name,
            source: format!("corpus entry {index}"),
        };
        target(input);
    }
    let mut rng = SplitMix64::new(seed);
    for number in 0..iterations(default) {
        let input = generate(&mut rng);
        let _context = Context {
            target: name,
            source: format!("generated input {number} of seed {seed:#018x}"),
        };
        target(&input);
    }
}

#[test]
fn engine_sequences() {
    smoke(
        "engine_sequences",
        fuzz_targets::engine_sequences,
        &corpus::engine_sequences(),
        SEED_ENGINE,
        DEFAULT_ENGINE,
        inputs::engine_sequences,
    );
}

#[test]
fn malformed_utf16() {
    smoke(
        "malformed_utf16",
        fuzz_targets::malformed_utf16,
        &corpus::malformed_utf16(),
        SEED_UTF16,
        DEFAULT_UTF16,
        inputs::malformed_utf16,
    );
}

#[test]
fn config_documents() {
    smoke(
        "config_documents",
        fuzz_targets::config_documents,
        &corpus::config_documents(),
        SEED_CONFIG,
        DEFAULT_CONFIG,
        inputs::config_documents,
    );
}

#[test]
fn config_concurrency() {
    smoke(
        "config_concurrency",
        fuzz_targets::config_concurrency,
        &corpus::config_concurrency(),
        SEED_CONFIG_CONCURRENCY,
        DEFAULT_CONFIG_CONCURRENCY,
        inputs::config_concurrency,
    );
}

#[test]
fn macro_files() {
    smoke(
        "macro_files",
        fuzz_targets::macro_files,
        &corpus::macro_files(),
        SEED_MACROS,
        DEFAULT_MACROS,
        inputs::macro_files,
    );
}

#[test]
fn ipc_frames() {
    smoke(
        "ipc_frames",
        fuzz_targets::ipc_frames,
        &corpus::ipc_frames(),
        SEED_IPC,
        DEFAULT_IPC,
        inputs::ipc_frames,
    );
}

#[cfg(feature = "evkey-compat")]
#[test]
fn legacy_sequence_wrap() {
    smoke(
        "legacy_sequence_wrap",
        fuzz_targets::legacy_sequence_wrap,
        &corpus::legacy_sequence_wrap(),
        SEED_LEGACY,
        DEFAULT_LEGACY,
        inputs::legacy_sequence_wrap,
    );
}

#[test]
fn transaction_application() {
    smoke(
        "transaction_application",
        fuzz_targets::transaction_application,
        &corpus::transaction_application(),
        SEED_TRANSACTION,
        DEFAULT_TRANSACTION,
        inputs::transaction_application,
    );
}

#[test]
fn every_target_accepts_the_empty_input_and_one_byte() {
    for input in [&[][..], &[0x00][..], &[0xFF][..]] {
        fuzz_targets::engine_sequences(input);
        fuzz_targets::malformed_utf16(input);
        fuzz_targets::config_documents(input);
        fuzz_targets::config_concurrency(input);
        fuzz_targets::macro_files(input);
        fuzz_targets::ipc_frames(input);
        #[cfg(feature = "evkey-compat")]
        fuzz_targets::legacy_sequence_wrap(input);
        fuzz_targets::transaction_application(input);
    }
}
