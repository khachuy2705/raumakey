# Raumakey Rust Rewrite Design

**Date:** 2026-10-16

**Status:** Approved

**Product:** Raumakey

**Repository:** `/opt/project/raumakey`

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

A successful Linux cross-build is not evidence of Windows runtime correctness. Runtime x86_64 acceptance remains pending until the Windows harness and manual matrix pass on Windows 10 or later.

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
xtask/                  build, resource, packaging, and verification commands
tests/                  shared fixtures, contract suites, and Windows harness
```

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

`xtask` drives cross-compilation, resources, installer generation, package manifests, checksums, and artifact collection. The x86_64 package contains the x86_64 controller and both x86_64 and i686 TIPs. ARM64 artifacts are compile-gated and packaged separately until runtime validation exists.

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

### 15.1 Linux-hosted tests

Every production behavior follows red-green-refactor. The workspace runs unit and integration tests for engine behavior, macros, stores, policy, codec, migration fixtures, and converter behavior. Property tests assert deterministic output, valid UTF-16, safe transaction application, and state invariants. Fuzz targets cover input sequences, malformed UTF-16, configuration, macro files, IPC frames, sequence wrap, and transaction boundaries.

Windows APIs are behind narrow ports so state commit/rollback, context changes, HRESULT mapping, and registration planning can be tested with fakes on Linux.

### 15.2 Cross-build gates

The release gate cross-builds `i686-pc-windows-msvc`, `x86_64-pc-windows-msvc`, and `aarch64-pc-windows-msvc` using LLVM and Windows SDK/import libraries supplied through the selected cross-build tool. It validates exports, PE architecture, manifests, resources, COM identities, and expected artifact inventory.

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

The project may be reported as **cross-build verified** only after all Linux-hosted quality gates and all three MSVC target builds pass with fresh evidence. It may be reported as **runtime x86_64 verified on Windows 10+** only after the user-run automated and manual Windows acceptance results pass and are attached to the release evidence.
