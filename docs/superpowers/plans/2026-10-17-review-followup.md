# Review follow-up — 2026-10-17

## Scope and findings

Inputs: approved design and plan, compatibility matrix, repository source and tests.
No restricted evidence or upstream implementation was used. Initial Git tree: clean,
commit `24ba7a6` (`update p1`). This is a targeted implementation review, not the independent
whole-branch review required for release.

- The layered library architecture is present, but `raumakey-controller/src/main.rs`,
  `raumakey-tsf/src/lib.rs`, and the shared acceptance runner are still stubs. Library test
  success does not establish a working input method.
- `xtask/src/cargo.rs` omitted Windows, controller, and TIP legacy feature selections. CI
  therefore did not cover the entire declared compatibility path. Dependency-resolving
  commands also omitted `--locked`.
- `xtask/src/tasks.rs` silently converted a missing required fuzz suite into a skip note.
- `xtask/src/evidence.rs` accepted any nonempty collection of passing steps as verified,
  including incomplete gate lists. `gate.rs` had no whole-branch review step.
- Secure-field behavior remains inconsistent: policy `compat.rs` permits Vietnamese in
  `Restricted`, while engine `process.rs` bypasses all secure fields. Resolve the contract
  before wiring the adapters; no behavior was changed in this increment.
- The store flushes the temporary file before replacement. Its existing durability concern
  remains open; this review does not establish power-loss durability.

## Implemented increment

CI covers all five workspace `evkey-compat` features, uses the lockfile, and fails early if
the fuzz suite is absent. Release evidence requires the exact ordered gate list. A new
whole-branch review gate checks a reviewer-supplied record against the current clean Git
revision before packaging and records its SHA-256. Plan §12 documents the record schema
and its limits. Git status failure no longer silently counts as a clean source tree.

No product functionality, migration semantics, secure-field decisions, or runtime claims
were added. No independent reviewer approval was created.

## TDD evidence

Tests ran with the existing portable Rust 1.99.1 Windows toolchain through PowerShell from
WSL, using the machine's existing MSVC linker configuration.

1. Baseline: `cargo test --locked -p xtask` — 96 passed.
2. Red: same command — 95 passed, 4 failed:
   - `cargo::tests::compat_ci_covers_every_workspace_legacy_adapter`
   - `cargo::tests::dependency_resolving_commands_preserve_the_lockfile`
   - `evidence::tests::incomplete_duplicate_or_reordered_gates_cannot_claim_verification`
   - `gate::tests::steps_follow_the_documented_order`
3. Red: `cargo test --locked -p xtask missing_fuzz_suite` —
   `tasks::tests::missing_fuzz_suite_fails_before_starting_ci_commands` failed because CI
   attempted to start Cargo instead of returning the missing-suite error.
4. Red API contract: `cargo test --locked -p xtask review` failed to compile because
   `validate_review` did not yet exist. Tests were written before adding that API.
5. Green: `cargo fmt --all` and `cargo test --locked -p xtask` — 102 passed.
6. `cargo run --locked -p xtask -- ci --no-fuzz` — passed formatting, Clippy with
   `-D warnings` for both feature sets, and workspace tests: 1,241 passed with defaults;
   1,315 passed with all five compatibility features. Both runs retained the 8 existing
   ignored manual tests. Log: `target/ci-review-followup.log`.
7. `cargo run --locked -p xtask -- ci` — expected exit 1 for missing `fuzz_smoke`.
8. `cargo run --locked -p xtask -- release-gate` — expected exit 1 at CI. The generated
   `target/release-evidence/release-gate-1792205238.json` was checked: `passed=false`,
   `verification_level="not verified"`, `runtime_verified=false`, the following six steps
   `not-run`, and no release artifacts. These generated logs are local evidence, not
   committed release certification.

Independent review of this increment remains pending. No cross-build or Windows host-app
acceptance test was rerun for this tooling-only change.

## Remaining work

Implement deterministic fuzz targets and macro/engine integration tests; then proceed with
the resource build scripts and TSF edit-session port described in design §20.10. Controller
and TIP implementation, Windows acceptance, and independent review remain required.
