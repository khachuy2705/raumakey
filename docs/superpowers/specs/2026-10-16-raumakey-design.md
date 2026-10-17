# Raumakey Rust Rewrite Design

**Date:** 2026-10-16 (last updated 2026-10-17: implementation status added as §20)

**Status:** Approved; implementation in progress (see §20)

**Product:** Raumakey

**Repository:** `https://github.com/khachuy2705/raumakey.git`

## 1. Purpose

Raumakey is a GPL-compatible Vietnamese input method for Windows, rewritten in Rust. It provides a native Win32 controller and configuration UI, a COM TSF Text Input Processor, Telex/VNI/VIQR/user-defined input methods, macros, text conversion, application-specific compatibility policy, and safe read-only migration from EVKey data.

The first public release is one full-parity release rather than a sequence of partial releases. Development may use internal milestones, but none is represented as the completed product.

## 2. Success criteria

The release must:

1. Run on Windows 10 or later on x86_64 after user-run acceptance testing.
2. Cross-compile the controller, TIP, and supporting artifacts for i686, x86_64, and ARM64 MSVC targets.
3. Support Telex, VNI, VIQR, and user-defined mappings; old/new tone placement; semantic backspace; invalid-word restoration; NFC and NFD output; macros; clipboard/file text conversion; and per-application policy.
4. Provide a lightweight native Win32 tray controller and dialogs.
5. Register a new Raumakey COM/TSF identity without replacing or impersonating EVKey.
6. Import supported EVKey settings and macros without modifying or deleting the source data.
7. Meet the security, privacy, TDD, review, and verification gates in this document.

A successful cross-build, whether on a Linux or a Windows host, is not evidence of Windows runtime correctness. Runtime x86_64 acceptance remains pending until the Windows harness and manual matrix pass on Windows 10 or later.

## 3. Product identity

Raumakey uses fixed identities distinct from EVKey:

- TSF COM CLSID: `{0FC48F2B-4F91-44A0-B500-DE512E801432}`
- TSF language profile GUID: `{DB875058-BBF9-407C-85A8-950BE7D14C22}`
- Installer upgrade code: `{BF73893F-FCEB-4BAE-AE1C-E81D7107ECE5}`
- Named-object prefix: `Local\\Raumakey.` followed by the session/user-qualified object name
- Registry root owned by the product: `HKCU\\Software\\Raumakey`

The migration component may read known EVKey paths. No Raumakey component may register EVKey's CLSID/profile, write beneath EVKey's registry root, use EVKey branding, or overwrite an EVKey file.

## 4. Legal and evidence boundary

The original reconstruction dossier is retained under `restricted_evidence/` and excluded from Git. Implementation and code-review agents must not read that directory. They may use only the approved design, implementation plan, clean-room behavioral specification, compatibility vectors, code already written in the repository, and properly licensed upstream source.

Before reusing or translating UniKey source, the implementation must record the exact repository URL, immutable commit, license text, affected files, and required notices in `THIRD_PARTY.md`. The repository's final GPL-compatible license version must match the actual upstream obligations; it must not be guessed. A source translation is treated as a derivative and retains attribution.

EVKey decompiler output, pseudocode, resources, icons, strings, class layouts, and control flow are forbidden implementation inputs.

## 5. Architecture

Raumakey is a Cargo workspace with inward-pointing dependencies.

```text
crates/
  raumakey-model        shared domain types and invariants
  raumakey-engine       platform-independent Vietnamese state machine
  raumakey-macro        macro matching, case adaptation, persistence model
  raumakey-config       versioned configuration and atomic file storage
  raumakey-policy       host capability and transport selection
  raumakey-ipc          typed control channel and gated legacy codec
  raumakey-windows      narrow safe wrappers around Win32 and COM
  raumakey-controller   native EXE: tray, dialogs, hooks, lifecycle
  raumakey-tsf          COM in-process TIP cdylib
  raumakey-converter    text and clipboard conversion
  raumakey-migrate      read-only EVKey importer
  raumakey-resources    pure-Rust RES/ICO/manifest/version-info writer (build-dependency of the binaries)
xtask/                  build, inspection, packaging, and verification commands
tests/                  shared fixtures, contract suites, and Windows harness
fuzz/                   cargo-fuzz targets (separate workspace, excluded from the main one)
```

`raumakey-resources` and `fuzz/` were added by the implementation plan (§1.1, §11, §12).

`raumakey-model` and `raumakey-engine` must not depend on Windows APIs, HWND, COM interfaces, registry types, handles, raw pointers, clipboard APIs, or process-global mutable state. Windows adapters depend on core crates, never the reverse.

### 5.1 Core contracts

The canonical contracts are the `InputEvent`, `EngineOptions`, `EngineState`, `EditTransaction`, `HostContext`, `HostPolicy`, and typed errors described in `docs/reference/evkey_clean_room_spec.md`, with Raumakey-owned version numbers and identities.

`EngineState` is session-scoped and contains no operating-system resources. `EditTransaction.delete_before` counts host-visible UTF-16 code units, is range-checked before application, and never splits a surrogate pair.

### 5.2 TSF path

The default input path is:

```text
physical key
  -> TsfAdapter normalizes InputEvent
  -> in-process VietnameseEngine processes session state
  -> EditTransaction
  -> one TSF edit session applies the transaction atomically
  -> successful apply commits next EngineState
```

The engine runs inside the TIP process for the TSF path. Raw keystrokes and document text do not travel to the controller. Each TSF activation/document session owns a serial executor and independent state. Configuration and macro snapshots are immutable for the duration of one request.

If TSF application fails, the context changes, or the available range is shorter than `delete_before`, the adapter does not commit the proposed next state. It ends the composition and resets or passes through according to policy.

### 5.3 Fallback path

`CompatibilityPolicy` selects exactly one transport for a host:

1. `TSF` by default.
2. `SyntheticKey` when an explicit host rule or failed capability probe requires it.
3. `Clipboard` only when explicitly enabled and never in a secure field.

The controller owns fallback session state partitioned by process ID, thread ID, foreground epoch, and document identity when available. Replayed keys carry a Raumakey-specific sentinel and are ignored by the hook. No physical key may be processed by more than one transport.

Clipboard fallback snapshots clipboard sequence ownership, writes only for the duration of the operation, and restores the previous data only if the clipboard sequence still proves Raumakey owns the latest update. It never restores over another application's newer clipboard content.

## 6. Input methods and engine behavior

The engine supports:

- Telex, VNI, VIQR, and versioned user-defined semantic key maps.
- Vietnamese vowel/consonant sequence validation.
- `qu` and `gi` onset handling.
- Configurable old/new tone placement.
- Repeated mark/tone undo with literal fallback.
- Semantic backspace using bounded state history.
- Spell checking and optional restoration of invalid converted words to raw keystrokes.
- NFC, NFD, and preserve normalization modes without malformed UTF-16.
- Lower/title/upper case preservation.

Invalid semantic actions must not silently swallow the original key. Every transformation is expressed as an `EditTransaction`; output injection is an adapter responsibility.

The compatibility matrix is the initial behavioral contract. Baseline vectors remain labeled as baseline until a legal EVKey runtime observation or an explicit Raumakey product decision establishes otherwise.

## 7. Macros and conversion

Macro entries contain a stable ID, NFC trigger, NFC replacement, enabled flag, boundary policy, and case policy. Lookup is deterministic and based on Unicode scalars, not byte offsets. Expansion occurs only at a configured boundary. Macro processing can operate while Vietnamese mode is off only when the user enables that option.

The converter supports in-memory text conversion and explicit clipboard/file operations. File conversion never overwrites the source by default. Clipboard conversion is disabled for secure fields and reports ownership conflicts rather than replacing newer clipboard content.

## 8. Configuration and persistence

Default data root:

```text
%LOCALAPPDATA%\\Raumakey\\
  config.toml
  macros.toml
  app_rules.toml
  migration.json
  logs\\
```

Portable mode is enabled only by a `raumakey.portable` marker beside the executable. It is never inferred merely from write access.

Every store contains `version` and monotonically increasing `revision`. A write uses a same-directory temporary file, durable flush, `ReplaceFileW`, and a read-back checksum. Optimistic revision checking returns `Conflict` instead of silently overwriting another writer. Unknown migrated fields are retained in an extension map.

The controller publishes immutable config/macro revisions to TIP clients over the typed control channel. A disconnected TIP continues with its last valid snapshot and reconnects with bounded exponential backoff.

## 9. Migration

Migration discovers supported EVKey registry, settings, exception, and macro sources. It:

1. Opens sources read-only.
2. Parses fields independently, quarantining corrupt fields rather than resetting the entire configuration.
3. Preserves unknown fields and raw source bytes needed for rollback evidence.
4. Normalizes only fields whose new schema requires it.
5. Commits Raumakey data atomically.
6. Reads back and verifies revision/checksum.
7. Marks the migration complete only after verification.

Migration is idempotent. It never deletes, renames, rewrites, unregisters, or changes permissions on an EVKey source.

## 10. IPC and process boundaries

The normal control channel is a same-user, same-session named pipe with an ACL restricted to the user's SID. Frames contain protocol version, message kind, payload length, client identity, request ID, and checksum. Lengths are validated before allocation. The normal channel carries configuration, macro snapshots, control commands, and opt-in diagnostics; it does not carry raw user keystrokes or document text.

The EVKey legacy wire codec is an optional Cargo feature named `evkey-compat`, disabled in release builds by default. It preserves the documented field widths, offsets, sequence matching, timeouts, and validation rules. Activation fails closed if a conflicting EVKey named object already exists. A legacy transaction is never retried after an ambiguous timeout because replay could double-apply text.

## 11. Native controller and UI

The controller uses a hidden Win32 window, one tray icon, and modeless/modal native dialogs. The compact panel exposes Vietnamese mode, input method, tone style, and expand/collapse. Expanded tabs cover input, hotkeys, macros, application rules, conversion, system behavior, and product information.

The UI thread performs no engine, file, network, or blocking IPC work. Background work posts typed results back to the hidden window. `TaskbarCreated` recreates the tray icon without changing mode. All user-visible resources and icons are newly created for Raumakey.

Hotkeys are classified before engine input and include a user-configurable mode toggle and hard reset. Modifier handling distinguishes AltGr from ordinary Ctrl+Alt shortcuts. A reset commits or ends the current composition without changing already committed text.

## 12. COM/TSF registration and lifecycle

The TIP is a Rust `cdylib` exposing only required COM exports. All FFI and COM entry points catch panics and convert failures to HRESULT values; no unwind crosses an ABI boundary.

Registration uses the fixed Raumakey CLSID/profile GUID, the correct WOW64 registry view for the DLL bitness, keyboard TIP categories, and declared capabilities. The installer sequence is register COM, install language profile, verify registry/profile state, then commit installer state. Failure rolls back only Raumakey-owned entries.

Activation creates per-document state and advises required sinks. Deactivation ends composition, unadvises sinks, removes hooks, cancels bounded work, and frees all resources. DLL unload eligibility reflects outstanding COM object and lock counts.

## 13. Startup, installer, and update boundary

The default executable manifest is `asInvoker` with `uiAccess=false`. Normal autostart uses a Startup shortcut. An elevated scheduled task is created only after an explicit user choice and is removed idempotently when disabled.

`xtask` drives cross-compilation, PE inspection, installer generation, package manifests, checksums, and artifact collection. Version information, icons, and the executables' manifests are embedded at build time: each binary crate's `build.rs` calls `raumakey-resources` and passes the `.res` file to the linker, so no `rc.exe` is needed (plan D1; the `build.rs` files do not exist yet, see §20). Shipped binaries link the MSVC CRT statically, so the TIP never depends on the Visual C++ redistributable (plan D23). The x86_64 package contains the x86_64 controller and both x86_64 and i686 TIPs. ARM64 artifacts are compile-gated and packaged separately until runtime validation exists.

The first release does not imitate the unknown EVKey updater protocol. Update logic is outside the engine and controller trust core. A future updater must require an authenticated manifest and verify package hash and signature before installation. Until a signing certificate exists, packages are explicitly reported as unsigned.

## 14. Security and privacy

- Secure/password fields bypass transformation, macros, clipboard transport, and content diagnostics.
- Named objects and pipes are restricted to the current user/session.
- Every external length, offset, sequence, UTF-16 value, and config field is validated before use.
- No default telemetry exists.
- Opt-in logs may contain timestamps, component, error code, revision, host executable basename, and test vector ID; they must not contain raw keys, document text, clipboard data, macro trigger/replacement, or full paths containing user content.
- Panics, poisoned locks, COM errors, I/O errors, and timeouts have typed recovery paths.
- Dependency and license audits are release gates.

## 15. Testing strategy

### 15.1 Host-independent tests

Every production behavior follows red-green-refactor. The workspace runs unit and integration tests for engine behavior, macros, stores, policy, codec, migration fixtures, and converter behavior. Property tests assert deterministic output, valid UTF-16, safe transaction application, and state invariants. Fuzz targets cover input sequences, malformed UTF-16, configuration, macro files, IPC frames, sequence wrap, and transaction boundaries.

Windows APIs are behind narrow ports so state commit/rollback, context changes, HRESULT mapping, and registration planning can be tested with fakes on any host.

On a Windows host, `raumakey-windows` also runs native Win32 integration tests. Tests with machine-wide or user-visible side effects (global hooks, `SendInput`, the real clipboard, the notification icon, scheduled tasks, `regsvr32`, elevation, TSF registration) are `#[ignore]`d and run manually.

### 15.2 Cross-build gates

The release gate cross-builds `i686-pc-windows-msvc`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc` with LLVM. On a Windows host without Visual Studio, the targets link with `rust-lld` against the MSVC CRT and Windows SDK import libraries produced by `xwin`, configured in the machine-local Cargo home. On a Linux host, `cargo xwin` supplies them (plan D1). It validates exports, PE architecture, manifests, resources, COM identities, and expected artifact inventory.

### 15.3 Windows acceptance harness

`raumakey-test-runner.exe` executes deterministic vectors and exports JSON containing test ID, OS/build, architecture, application/version, config revision, transaction sequence, final text, and pass/fail status. It never captures unrelated user input.

A PowerShell suite verifies install, both registry views, COM registration, profile installation, startup modes, upgrade, rollback, and uninstall. Manual acceptance covers Notepad, Office, Edge/Chrome, Windows Terminal, password fields, Explorer restart, multiple documents, layout switching, and application policy.

## 16. Quality gates

For each implementation task:

1. A relevant test is written and observed failing for the intended reason.
2. Minimal production code makes it pass.
3. The full applicable suite passes without ignored failures.
4. An independent reviewer checks specification compliance and code quality.
5. Important findings are fixed and re-reviewed before the next dependent task.

Before packaging:

- `cargo fmt --check`
- Clippy for the workspace with warnings denied
- Full workspace tests
- Property and fuzz smoke suites
- Dependency vulnerability and license audit
- All three cross-builds
- PE/export/manifest/resource inspection
- Installer dry-run contract checks
- Whole-branch independent review
- SHA-256 manifest for every artifact

## 17. Delivery stages

Internal implementation order is:

1. Workspace, provenance, toolchain, and core contracts.
2. Engine and input methods.
3. Macro, conversion, stores, and migration.
4. Policy, control IPC, and legacy codec.
5. Native controller, tray, dialogs, hotkeys, and fallback path.
6. TSF COM TIP and registration.
7. Installer, cross-builds, Windows harness, and documentation.
8. Hardening, audit, full review, and release candidate.

These stages are engineering gates, not partial product releases.

## 18. Explicit non-goals for the first release

- Reusing EVKey branding, GUIDs, icons, resources, or updater protocol.
- Claiming byte-for-byte or implementation equivalence with EVKey.
- Runtime support claims for ARM64 or i686 before those architectures are executed on real Windows systems.
- Cloud telemetry, account synchronization, automatic network update, or a web-based UI.
- Deleting or modifying legacy application data during migration.

## 19. Final acceptance wording

The project may be reported as **cross-build verified** only after all host-independent quality gates and all three MSVC target builds pass with fresh evidence. It may be reported as **runtime x86_64 verified on Windows 10+** only after the user-run automated and manual Windows acceptance results pass and are attached to the release evidence.

## 20. Implementation status (as of 2026-10-17)

This section is a dated snapshot of what the repository contains and what is still outstanding. It does not change the requirements in §1–§19. The evidence was gathered on 2026-10-17 on the Windows 11 x86_64 development host (Rust 1.99.1, `rust-lld` + `xwin`, plan D1). It comes from running the gates in §20.2 and from a read-only audit of every crate against the implementation plan, with each finding checked a second time against the code. Nothing here was derived from `restricted_evidence/`.

On the same date, §2, §5, §13, §15, and §19 were aligned with plan decisions D1 and D23 and with the `raumakey-resources` crate that the plan added. No requirement changed.

### 20.1 Summary

- **Implemented (libraries):** ten library crates and `xtask` are implemented and tested: `raumakey-model`, `-engine`, `-macro`, `-config`, `-policy`, `-ipc` (including the `evkey-compat` codec), `-converter`, `-migrate`, `-resources`, and `-windows`. Eight of the libraries are complete against the plan; `raumakey-migrate`, `raumakey-windows`, and `xtask` are mostly complete (§20.4). Together they hold about 39k lines of source (including in-file unit tests) and 33k lines of integration tests. 1,235 tests pass, or 1,309 with `evkey-compat`.
- **Not started (product):** the controller (`raumakey.exe`) and the TSF TIP (`raumakey_tip.dll`) are empty stubs. No library code runs in a shipped binary yet, and no matrix row has been observed in a real Windows host application.
- **Not started (verification):** the `raumakey-tests` package is an empty stub; its `raumakey-test-runner` binary is `fn main() {}` but is built and packaged. The fixtures, contract suites, fuzz smoke suite, `fuzz/` workspace, PowerShell acceptance suite, and manual checklist do not exist.
- **Not in Git:** apart from the docs commit `771c3af`, everything is untracked: 271 files, including `Cargo.lock`, the plan, `LICENSE`, and `THIRD_PARTY.md`.
- **Release wording (§19):** neither "cross-build verified" nor "runtime x86_64 verified" can be claimed. The release gate would stop at PE inspection. Even a passing `xtask release-gate` would not yet justify "cross-build verified", although it records that wording: it has no whole-branch review step, only notes a missing fuzz smoke suite, and never runs the `raumakey-windows/evkey-compat` tests (§20.6, item 7).

### 20.2 Gate evidence

| Gate | Command | Result on 2026-10-17 |
|---|---|---|
| Formatting | `cargo fmt --all --check` | Pass |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings`, with default features and with `raumakey-ipc/evkey-compat,raumakey-windows/evkey-compat` | Pass |
| Tests | `cargo test --workspace` | 1,235 passed, 0 failed, 8 ignored |
| Tests with the legacy codec | the same, with `--features raumakey-ipc/evkey-compat,raumakey-windows/evkey-compat` | 1,309 passed, 0 failed, 8 ignored |
| CI task | `cargo xtask ci` | Reports a pass, but skips the fuzz smoke suite (there is no `fuzz_smoke` target) and never enables `raumakey-windows/evkey-compat`, so 13 legacy-adapter tests and that module's lints never run |
| Cross-build | `cargo xtask cross` and `cargo xtask cross --release` | Pass for x86_64, i686, and aarch64, building stub binaries |
| PE inspection | `cargo xtask inspect` and `--release` | **Fail**: 27 of 57 (debug) and 27 of 66 (release) checks. `raumakey.exe` is a console binary without manifest, version, or icon resources; `raumakey_tip.dll` lacks the four COM exports, its resources, and the identity GUIDs. The static-CRT check (D23) passes for all nine release artifacts |
| Dependency audit | `cargo xtask audit` (cargo-deny 0.20.2) | Pass, with warnings for a duplicate `syn` and unused license allowances |
| Packaging | `cargo xtask package` | Produces debug x64 and ARM64 packages (`-debug` suffix) and `raumakey-debug.wxs` from the stub binaries; `package --release` was not run; MSI skipped because `wix` is not installed; unsigned |
| Release gate | `cargo xtask release-gate` | Not run. It would stop at `inspect` and record `package` and the installer dry-run as not run. Once `inspect` passes, the dry run would still fail, because the stub `raumakey.exe` prints no plan naming the CLSID and profile GUID |
| Windows acceptance | §15.3 harness and manual matrix | Not available |

The 8 ignored tests are the manual `raumakey-windows` tests described in §15.1. Nothing records that they have ever been run. `cargo xtask` is an alias defined only in the machine-local Cargo configuration; a fresh clone must use `cargo run -p xtask -- <command>`.

### 20.3 Status by delivery stage (§17)

| Stage | Status | Notes |
|---|---|---|
| 1. Workspace, provenance, toolchain, core contracts | Done, not committed | Workspace (resolver 3, edition 2024), toolchain pinned to 1.99.1 with the three MSVC targets, GPL-3.0 `LICENSE`, `THIRD_PARTY.md`, `deny.toml`, `raumakey-model` |
| 2. Engine and input methods | Done (library) | Every plan §3.10 golden vector and every engine-level matrix row has a test; no binary calls the engine yet |
| 3. Macro, conversion, stores, migration | Mostly done (library) | Missing: a Windows registry source for migration, discovery of default EVKey folders, command-line wiring |
| 4. Policy, control IPC, legacy codec | Done (library) | Missing on the controller side: a multi-client server hub and a snapshot builder |
| 5. Controller, tray, dialogs, hotkeys, fallback path | Building blocks only | `raumakey-resources` is ready. `raumakey-windows` has the hook, input, clipboard, pipe, shell, and task-scheduler wrappers, but not the window, dialog, `PostMessageW`, and icon-loading wrappers the controller needs (§20.6, item 3). `raumakey-controller` is `fn main() {}` |
| 6. TSF COM TIP and registration | Registration code only | Planner, executor, and verification exist in `raumakey-windows` and are tested with fakes; the Windows registration port has run only in a manual test; `raumakey-tsf` is a single doc comment |
| 7. Installer, cross-builds, Windows harness, documentation | Partial | `xtask` is implemented (§20.2); the harness, fixtures, fuzzing, PowerShell suite, and manual checklist have not started |
| 8. Hardening, audit, review, release candidate | Not started | `xtask audit` passes; no review records exist |

The stages did not run in order: the stage 7 tooling (`xtask`) exists while the stage 5 and 6 binaries (controller and TIP) have not been started. Plan §13 allows this, because `xtask` depends on neither binary.

### 20.4 Status by component

"Complete" means that every plan requirement for the library has code and tests. It says nothing about runtime behavior on Windows.

| Component | Plan | Status | Tests passing | Main gaps |
|---|---|---|---|---|
| `raumakey-model` | §2 | Complete | 44 | Content-bearing types derive `Debug` (§20.7) |
| `raumakey-engine` | §3 | Complete; one branch untested | 125 | Never run with the real `MacroTable`; the plan §3.6 Circumflex fallback with spell checking off is untested (§20.8, item 9); several grammar decisions unrecorded (§20.9) |
| `raumakey-macro` | §4 | Complete | 149 | Not wired: the controller must load `macros.toml` into a `MacroStore` with `from_entries` and save `entries()` back, keeping `MacroDocument.entry_extras` and `extensions` (plan §4) |
| `raumakey-config` | §5 | Complete | 136 | Not wired; commit durability (§20.7); `.lock` and `.bak-*` files accumulate |
| `raumakey-policy` | §6 | Complete | 107 | Not wired; probe-record limits differ between layers (§20.7) |
| `raumakey-ipc` | §7 | Complete | 107; 168 with `evkey-compat` | No server hub or snapshot builder; the legacy codec is not connected to any engine |
| `raumakey-converter` | §8 | Complete | 127 | No CLI or UI; `config.toml` stores charset names as unchecked strings |
| `raumakey-migrate` | §9 | Mostly complete | 115 | No Windows `LegacyRegistry` adapter, no default folder discovery, no `migrate` command; `FIELD_MAP` is empty (D18) |
| `raumakey-resources` | §11 | Complete | 82 | No `build.rs` calls it, so no binary carries a manifest, version information, or icons |
| `raumakey-windows` | §10.1, §10.4 | Mostly complete | 147 and 8 manual; 160 with `evkey-compat` | Registration verification reads the `CTF\TIP` registry keys instead of enumerating the profile, as plan §10.4 requires (§20.7). Wrappers the controller and TIP need are missing: windows, message-only windows, dialogs, icon loading, the DLL module path, `GetMessageExtraInfo`, TSF sinks. Side-effecting paths are proven only by manual tests |
| `raumakey-controller` | §10.2 | Not started | 0 | Everything in plan §10.2 (§20.6) |
| `raumakey-tsf` | §10.3 | Not started | 0 | Everything in plan §10.3 (§20.6) |
| `xtask` | §12 | Mostly complete | 96 | CI feature set, fuzz skip, review gate, notices (§20.6) |
| `raumakey-tests`, `fuzz/`, `tests/windows/` | §12 | Not started | 0 | Fixtures, contract suites, fuzz targets, acceptance runner, acceptance suite |

### 20.5 Compatibility-matrix coverage

Coverage of the 58 rows of `docs/reference/evkey_compatibility_test_matrix.md`:

| Coverage | Rows | Count |
|---|---|---|
| Asserted by library tests | #1–#8, #10–#18, #21–#23, #28, #30–#38, #45–#54 | 40 |
| Partly asserted | #9, #25, #26, #27, #29, #41, #42, #44, #55, #56 | 10 |
| Runtime only; building blocks tested | #19, #20, #24, #39, #40, #58 | 6 |
| Not covered | #57 (no `update` command) | 1 |
| Not applicable | #43 (needs EVKey binaries) | 1 |

- No row meets the matrix's own pass criteria yet. Those require every run to record architecture, Windows build, host application and version, configuration revision, input events, transaction sequence, and final text, and the runner that should record them, `raumakey-test-runner`, is an empty stub.
- The engine tests for macro rows #47–#52 use test doubles; no test runs the engine with the real `raumakey_macro::MacroTable`.
- #9 is not checked for user-defined key maps. #18 reaches the history eviction path only with the context limit raised above its default.
- #30–#38 run only with `--features raumakey-ipc/evkey-compat`, over a deterministic fake. The tests over real kernel objects need `raumakey-windows/evkey-compat`, which `xtask ci` omits. The #42 parity tests have run only on x86_64: no gate runs the i686 tests under WOW64, and ARM64 is only compiled.
- Raumakey decisions replace the expected value of three rows: #18 falls back to pass-through and reset instead of legacy op 2 (plan §3.9), #27 stores probes in `app_rules.toml` (plan §6), and #39 tags replays with `RAUMAKEY_INJECTED_TAG` (D6).
- The policy crate contradicts #29 (§20.8, item 1).

### 20.6 Outstanding work

Group A blocks every runtime claim; within it, item 3 is a prerequisite of items 1 and 2, which can then proceed in parallel. In group B, the `live` mode of item 5 and most of item 6 need group A; items 4 and 7 do not. Group C depends on neither group, and item 8 comes first (§20.10). The library gaps in §20.4 and the defects in §20.7 are outstanding as well; fix those that break a design requirement before release, starting with commit durability (§8 requires a durable flush).

**A. Shipped binaries (these block every runtime claim)**

1. **TSF TIP (`raumakey-tsf`, plan §10.3).**
   - Exports `DllGetClassObject`, `DllCanUnloadNow`, `DllRegisterServer`, and `DllUnregisterServer`, behind panic guards.
   - A class factory and a `TextService` implementing `ITfTextInputProcessorEx` and the five sinks of plan §10.3 (key event, thread manager event, text edit, thread focus, compartment event), plus the activation and deactivation lifecycle.
   - Per-document state, and the `OnTestKeyDown`/`OnKeyDown` path that applies one edit session and commits the next state only after it succeeds.
   - Replay keys sent with `SendInput` and tagged `RAUMAKEY_INJECTED_TAG` after a successful edit session, the matching bypass through `GetMessageExtraInfo` (matrix #39), and detection of password input scopes.
   - The TSF range-edit capability probe, reported with `Command::ReportProbe` (§5.3, matrix #27), and opt-in `Diagnostic` events sent only while `RuntimeSnapshot.diagnostics_enabled` is set (§10).
   - The snapshot client thread with backoff and defaults v1, and local hotkey classification while disconnected (D24).
   - Object and lock counts, and a `build.rs` for its resources.
   - The `evkey-compat` legacy path, which must tag and bypass `0xEBCA1234` (clean-room spec §3.1, §3.4).

   Add an edit-session/range port first, so commit and rollback can be tested with fakes (§15.1). The adapter must call `engine.reset()` when the document changes: the engine answers a reset event that carries a different owner with `ContextMismatch`.
2. **Controller (`raumakey-controller`, plan §10.2).**
   - GUI subsystem, attaching to the console for subcommands.
   - The CLI (`install`, `uninstall`, `startup`, `migrate`, `convert`, `update`, `--version`). It must accept options in any order, because the packaged `install.ps1` and the release gate order them differently.
   - Single instance, hidden window, message loop, and worker thread.
   - Tray with the V/E icons and `TaskbarCreated` handling; compact panel and tabs with new Vietnamese strings. Decide whether those strings become RT_STRING resources; if so, extend `raumakey_resources::Artifact::resources`, which emits no string table today.
   - IPC server hub that calls `verify_peer_same_user_and_session` on every connection (§20.7), publishes snapshots, persists reported probes, records opt-in `Diagnostic` events in `DiagnosticsLog`, and implements the controller side of D24 (swallow `VirtualKey` hotkeys, broadcast `ModeChanged` and `ResetAll`).
   - Hook thread with the D24 classifier and its adapter duties.
   - Fallback transport with the session map, D20 backspaces, and the clipboard transport.
   - Configuration lifecycle: D9 data root, stores, diagnostics, and validation of the `[conversion]` charset names in `config.toml`.
   - The `evkey-compat` legacy server, the quiet panic hook, and a `build.rs` for its resources.
3. **Missing building blocks in `raumakey-windows`.**
   - Window class, message-only window, `PostMessageW`, dialogs, and controls.
   - Icon loading from the module's own resources, and the DLL's module handle and path (`DllRegisterServer` needs them).
   - `GetMessageExtraInfo`, TSF sink and edit-session helpers, input-scope detection, and foreground-change tracking.
   - A read-only Windows adapter for the migration `LegacyRegistry` port, and default EVKey folder candidates for `raumakey_migrate::discover` when `migrate` gets no `--from`.
   - Profile verification that enumerates the installed profile through TSF (plan §10.4).
   - x64 + x86 install orchestration with cross-architecture rollback and the `HKLM\Software\Raumakey\Install` record.

**B. Verification infrastructure**

4. `raumakey-tests`: machine-readable vectors in `tests/fixtures/compat/*.toml`; contract suites, including one that runs the engine with the real `MacroTable`; `fuzz_targets` and `tests/fuzz_smoke.rs`; and the `fuzz/` cargo-fuzz workspace.
5. `raumakey-test-runner`: `vectors --json` and `live --app notepad`, recording the fields the matrix pass criteria require.
6. `tests/windows/Invoke-RaumakeyAcceptance.ps1` and `MANUAL_ACCEPTANCE.md`. Run the 8 manual tests and keep their results as evidence.
7. `xtask`:
   - Add `raumakey-windows/evkey-compat`, and the controller and TIP features, to the CI feature set.
   - Make a missing fuzz smoke suite fail `release-gate` instead of only adding a note.
   - Add an evidence slot for the whole-branch review (§16), and pass `--locked`.
   - Ship the generated notices and license texts in packages.
   - Extend `inspect` to check Common Controls v6, PerMonitorV2, and the supportedOS entry.
   - Run the i686 tests under WOW64, and compile the WiX source in CI.
   - Stop `package` from shipping binaries that fail `inspect`.

**C. Process, provenance, documentation**

8. Commit the implementation. Until then there is no history, release evidence cannot identify the source tree, and the proptest regression seeds can be lost. From now on, record each task's failing test run and each independent review (§16); the existing code has neither.
9. `THIRD_PARTY.md`: the MSVC CRT is linked into the shipped binaries (D23) but listed as "not shipped". Record its basis, ship notices for transitive crates, and cite the source of the VNI Windows table.
10. Plan upkeep:
    - Write statements for D17 and D21, which only the §10.4 heading cites, and add a decision index.
    - Record the code-level decisions listed in §20.9.
    - Update the dependency tables of plan §1.1 and §1.3 to match the manifests.
    - Add `RuntimeSnapshot.diagnostics_enabled` and the 4 KiB control-message cap to plan §7.
    - Record that `RegisterProfile` replaces `InstallLayoutOrTip`, a deviation from clean-room spec §6.1, and whether the `-ineime` / `-r -ineime` modes get an equivalent path or are dropped, as §6.1 requires for them.

### 20.7 Known risks and technical debt

Each item was confirmed against the code. Items marked *(Windows)* still need confirmation on a real system.

- **Commit durability.** `AtomicStore` flushes the temporary file but not the swap. `ReplaceFileW` runs without write-through, and no directory flush follows, so a committed revision may not survive a power loss.
- **AppContainer and low-integrity hosts (Windows).** The control pipe and the named objects grant access only to the user SID, with no AppContainer ACE and no low mandatory label, yet the TIP declares immersive support. A TIP loaded in such a host cannot connect and stays on defaults v1. Under D24, both the controller hook and that TIP then classify the modifier-only toggle.
- **TSF registration verification (Windows).** Verification checks TSF-internal keys under `HKLM\SOFTWARE\Microsoft\CTF\TIP` instead of enumerating profiles through TSF, and it has never run against a real registration. If that key is shared between the 32-bit and 64-bit registry views, unregistering one TIP removes the profile the other relies on. Rollback leaves a profile that existed before registration with the new parameters.
- **Secure fields in fallback hosts.** Outside TSF, the only password signal is the `ES_PASSWORD` style on Edit and RichEdit controls. No UI Automation check exists, and no IPC message carries a secure state detected by the TIP to the controller.
- **Privacy by convention.** Several types that hold typed text, document text, macro text, or key codes derive `Debug`, among them `InputEvent`, `EditTransaction`, `MacroEntry`, `EngineState`, `ClipboardText`, `MacroDocument`, and `KeyboardEvent`. Nothing logs them today, but a single `{:?}` in a log or panic message would break §14.
- **Unverified pipe peers.** `PipeServer::accept` returns an unverified connection, so the controller must call `verify_peer_same_user_and_session` itself.
- **Probe-record limits.** `app_rules.toml` has no cap, IPC snapshots allow 4,096 records, and the policy cache keeps 1,024, evicting the rest silently.
- **Macro test doubles.** The engine's macro doubles differ from `MacroTable` in case detection, `CasePolicy::Exact`, `BoundaryPolicy`, and NFC renormalization, so the engine and the table could disagree without any test failing.
- **Silent fallback.** A macro replacement longer than 8,192 UTF-16 units commits the word without expansion and without any signal. No test covers it.
- **Hook shutdown.** `HookThread::stop` detaches the thread after 2 s, which can leave the hooks installed.
- **Packages today.** The generated `install.ps1`, `uninstall.ps1`, and MSI custom actions call controller subcommands that do not exist. They would exit 0 without registering anything.
- **Data-root clutter.** `<file>.lock` files are never removed, and `set_aside` leaves `<file>.bak-<ms>` copies that are never pruned.
- **Tests that pass vacuously.** Three Windows-only converter tests skip themselves entirely when the temporary folder is not on a drive letter, the administrative share is unreachable, or the volume creates no 8.3 names; a fourth skips its UNC case when the share is unreachable. All of them still report success.
- **Cleanup.** `serde` in `raumakey-migrate`, `toml` in `xtask`, and `windows-core` in `raumakey-windows` are unused dependencies. `raumakey-windows` depends on `raumakey-ipc` unconditionally. The TIP profile icon index is defined in both `raumakey-resources` and `raumakey-windows`, and nothing checks that the two values match.

### 20.8 Open design questions

These need a product decision. Record each answer as a plan decision (Dn), then align this design, the plan, the code, and the tests.

1. **Secure-field mode `Restricted`.** §14 and matrix #29 say secure fields bypass transformation, macros, and clipboard. Plan §6 and `raumakey-policy` let `Restricted` keep transformation, and a test asserts this. The engine bypasses secure fields in every mode, so the setting currently has no effect.
2. **The controller's global keyboard hook (D24).** The hook receives every key on the desktop, including keys typed into TSF hosts, while §5.2 says raw keystrokes stay in the TIP on the TSF path. Either state the privacy rules for the hook handler (classify only, store nothing, run the engine only for fallback hosts) or revise D24. Also decide what happens while a TIP is disconnected and both sides may classify the same key.
3. **EVKey settings import.** `FIELD_MAP` ships empty (D18). An import therefore brings in macros and executable-list named sets and keeps every other setting only as raw evidence. Success criterion 6 ("import supported EVKey settings") is met only because no setting is declared supported. Confirm this, or plan the mappings.
4. **Migration evidence.** §9 step 3 keeps raw source bytes for rollback evidence. The importer keeps only a fingerprint of the macro file, never macro text (a privacy choice); it keeps `setting.tree` as base64 within an 8 MiB budget, `setting.ini` as parsed values, and registry values as typed values. §9 also lists exception sources, which do not exist: application exceptions arrive only through the executable-list heuristic.
5. **TSF profile installation.** Plan §10.4 uses `ITfInputProcessorProfileMgr::RegisterProfile`. Clean-room spec §6.1 installs the profile with `InstallLayoutOrTip` (and MUST keep its profile-string format), verifies that state, and requires the `-ineime` / `-r -ineime` modes to have an equivalent path or a deliberate, recorded omission. Decide, and write D17 and D21. Until they say otherwise, plan §10.4 requires verification to enumerate the profile, which the code does not do yet (§20.7).
6. **Pipe access from AppContainer and low-integrity hosts** (§20.7). Add the required ACE and label, or accept that TIPs in such hosts run on defaults v1.
7. **App-rule precedence.** Within one tier, the first declared named set or rule decides each field. Plan §6 lists named sets "in declared order" within a chain where later steps override earlier ones.
8. **Package contents.** Besides the binaries §13 names, the x64 package ships `tools/raumakey-test-runner.exe`, install and uninstall scripts, and a README.
9. **Circumflex fallback.** With spell checking off, plan §3.6 picks "the last candidate" of a list ordered last to first, while the code picks the rightmost vowel. The wording is ambiguous, and no test covers the case.

### 20.9 Decisions

The plan records decisions D1, D2, D4–D7, D9, D13, D17–D21, D23, and D24. The other numbers are unused.

| Decision | Summary | Design § | Implementation |
|---|---|---|---|
| D1 | Toolchain and hosts: pinned stable Rust; `rust-lld` + `xwin` on Windows, `cargo xwin` on Linux; resources from `build.rs`; i686 tests under WOW64 | §13, §15 | Done, except the `build.rs` files and running the i686 tests under WOW64 |
| D2 | `publish = false`; GPL-3.0-or-later | §1, §4 | Done |
| D4 | Versioned engine defaults v1 | §6 | Done |
| D5 | Hotkeys: toggle `Ctrl+Shift`; hard reset `Ctrl+Shift+Alt+F12` and `Ctrl+Shift+Alt+Space` | §11 | Library done |
| D6 | Injected-input tag `0x524B5931` ("RKY1") | §5.3 | Library done |
| D7 | Shift+Space commits the word unchanged | §7 | Done |
| D9 | Portable data root `<exe_dir>/data/`, only with a `raumakey.portable` marker | §8 | Library done, not wired |
| D13 | Default named sets: chromium-family, terminals, games | §5.3 | Done |
| D17, D21 | TSF registration entries, registry views, categories, verification | §12 | Code in `raumakey-windows`; decision text missing |
| D18 | Settings are imported only through `FIELD_MAP`, which ships empty | §2, §9 | Done |
| D19 | Converter charsets | §7 | Done |
| D20 | Fallback sends one backspace per deleted scalar | §5.3 | Not started (controller) |
| D23 | Static CRT in shipped artifacts | §13 | Done in `xtask`; `inspect --release` finds no VC runtime imports |
| D24 | Single hotkey owner | §11 | Classifier done; ownership protocol not implemented |

The code also makes decisions that the plan does not record yet:

- **Engine:** `transformed` is derived from the current tone and marks (the plan leaves this open); letters count as a syllable prefix if later marks could still make them one; five extra rhymes after `qu`; no plain `g` before e/ê/y; a VIQR-escaped boundary punctuation key commits the word; the Circumflex fallback (§20.8, item 9).
- **Model and macros:** a trigger typed exactly as stored is never case-adapted; legacy import trims edge whitespace, keeps whitespace-only replacements, rejects input over 16 MiB or with more than 100,000 macro lines (blank, comment, and header lines not counted), and exports CRLF without a BOM; a modifier-only hotkey needs at least two modifiers.
- **Policy and IPC:** first-declared-wins precedence within a tier; the probe cache cap and its eviction order; the 4 KiB control-message cap; `RuntimeSnapshot.diagnostics_enabled`; legacy bounds (at most 0x800 deleted units, sequence numbers skip 0, future epochs rejected, the bridge dropped after three failed exchanges); legacy activation accepts an existing mutex, as clean-room spec §6 requires.
- **Configuration and migration:** `Conflict` is also returned when the file changed under the same revision; an exhausted revision restarts at 0; lock waits are bounded at 10 s; macro text never enters `migration.json`.
- **Converter, resources, and Windows layer:** the clipboard port writes only while the sequence number is unchanged; the manifest also sets the UTF-8 active code page; TSF registration runs inside `DllRegisterServer` under the `regsvr32` of matching bitness; the registry write guard applies the same rules to HKCU and HKLM.

### 20.10 Recommended next steps

1. Fix the `xtask ci` feature set and make `release-gate` fail when the fuzz smoke suite is missing or no whole-branch review is recorded. Then commit the current tree, so later work has history and traceable evidence.
2. Decide §20.8 items 2, 5, and 6 and record them as plan decisions: they define the hook thread, the install command and its verification, and the pipe ACL the TIP connects through. Decide items 1, 3, and 4 before the TIP's secure-field path and the `migrate` command are written.
3. Add the `build.rs` files and the missing `raumakey-windows` building blocks. Then implement the TIP: its four COM exports, class factory, and registration, and the key path behind an edit-session port, with fakes for matrix rows #19, #20, #40, and #41.
4. Implement the controller: first the GUI subsystem and the CLI with `install`/`uninstall --dry-run`, which together with step 3 let `xtask inspect` and the installer dry-run pass; then the IPC hub, tray, hook thread, and panel.
5. Build `raumakey-tests` and the acceptance runner, then the PowerShell suite and the manual checklist, and run the 8 manual tests.
6. Settle the remaining §20.8 questions (items 7–9) and refresh this section.
