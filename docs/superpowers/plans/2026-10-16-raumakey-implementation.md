# Raumakey Implementation Plan

**Date:** 2026-10-16 (last updated 2026-10-17: release-gate evidence contract in §12,
implementation status in §14)
**Status:** Active; implementation in progress (see §14)
**Inputs:** `docs/superpowers/specs/2026-10-16-raumakey-design.md` (design, authoritative),
`docs/reference/evkey_clean_room_spec.md` (clean-room behavioral reference),
`docs/reference/evkey_compatibility_test_matrix.md` (compatibility vectors).

This plan turns the approved design into crate contracts, algorithms, file formats, and an ordered
task list. Where the design or the clean-room reference leaves a behavior open, this plan records a
**Raumakey decision (Dn)**. Decisions are product choices, not claims about EVKey behavior.
Nothing in this plan was derived from `restricted_evidence/`.

---

## 0. Ground rules for every task

1. **TDD.** Write the test, run it, observe it fail for the intended reason, then write the minimal
   production code, then refactor. Record the failing run in the task notes (command + failing test
   name).
2. `raumakey-model` and `raumakey-engine` contain no Win32, COM, registry, HWND, handle, raw pointer,
   clipboard, or process-global *mutable* state. Immutable lazily initialised tables (`OnceLock`) are
   allowed.
3. No panic may cross a COM, Win32 callback, or FFI boundary. Every exported function, COM method,
   window procedure, hook procedure, and thread entry point is wrapped in `catch_unwind` (helpers in
   `raumakey-windows::panic`).
4. Never log keystrokes, document text, clipboard content, macro triggers or replacements, or full
   paths that contain user content. Logging goes only through the typed `DiagEvent` API, which has
   no free-form string fields (see §2.10).
5. `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` must pass.
6. A successful build is not evidence of Windows runtime behavior. Runtime claims require the
   Windows acceptance harness (§12).
7. Do not add a dependency that is not listed in §1.3 without recording it in `THIRD_PARTY.md` and
   `deny.toml`.

---

## 1. Workspace and toolchain

### 1.1 Layout

```text
Cargo.toml                    workspace (resolver 3, edition 2024)
rust-toolchain.toml           pinned stable channel + MSVC targets + rustfmt/clippy
deny.toml                     cargo-deny licenses/bans/advisories/sources policy
LICENSE                       GPL-3.0 text
THIRD_PARTY.md                provenance and dependency notices
crates/
  raumakey-model/             shared domain types and invariants (no platform deps)
  raumakey-engine/            Vietnamese state machine (no platform deps)
  raumakey-macro/             macro table, case adaptation, persistence model, legacy text import
  raumakey-config/            versioned TOML/JSON documents, atomic file store, diagnostics sink
  raumakey-policy/            host policy resolution, probe cache, hotkey classifier
  raumakey-ipc/               framed control protocol, client/server state machines,
                              feature `evkey-compat`: legacy shared-memory codec
  raumakey-converter/         charset conversion, case/tone transforms, clipboard/file operations
  raumakey-migrate/           read-only EVKey importer
  raumakey-resources/         pure-Rust RES/ICO/manifest/version-info writer (build-dependency)
  raumakey-windows/           narrow safe wrappers around Win32 and COM (cfg(windows))
  raumakey-controller/        raumakey.exe: tray, panel, hooks, fallback transport, IPC server,
                              installer and startup subcommands
  raumakey-tsf/               raumakey_tip.dll: COM in-process TSF text input processor
xtask/                        ci, cross, inspect, package, audit, notices, release-gate
tests/                        package `raumakey-tests`: fixtures, contract suites, fuzz smoke,
                              bin `raumakey-test-runner`
tests/fixtures/               compatibility vectors, migration fixtures, config fixtures
tests/windows/                PowerShell acceptance suite (user-run, elevated where noted)
fuzz/                         cargo-fuzz project (own workspace, excluded from the main one)
```

Dependency direction (arrows point to dependencies):

```text
model <- engine, macro, config, policy, ipc, converter
config <- migrate, windows, controller, tsf
macro  <- migrate, controller, tsf
policy <- controller, tsf
ipc    <- controller, tsf
converter <- controller
windows <- controller, tsf
engine <- controller, tsf, tests
resources (build-dependency) <- controller, tsf
```

`raumakey-windows` may depend on core crates (`model`, `config`, `converter` ports). Core crates never
depend on `raumakey-windows`, `windows`, or `windows-sys`.

### 1.2 Toolchain (D1)

- Stable Rust pinned in `rust-toolchain.toml`, components `rustfmt`, `clippy`, targets
  `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, `aarch64-pc-windows-msvc`.
- **Windows host without Visual Studio:** link with `rust-lld` and the MSVC CRT + Windows SDK import
  libraries produced by `xwin splat`. The machine-local `CARGO_HOME/config.toml` sets
  `linker = "rust-lld"` and `-Lnative=` library paths per target. `.cargo/` in the repository is
  ignored by Git because it holds machine paths.
- **Linux host:** `cargo xwin build --target <triple>` (the "selected cross-build tool" of design
  §15.2).
- Resources (manifest, version info, icons) are produced by `raumakey-resources` from `build.rs` as a
  `.res` file passed to the linker with `cargo:rustc-link-arg-*`; no `rc.exe` is required.
- x86_64 test binaries run natively; i686 test binaries also run on x64 Windows (WOW64); ARM64 is
  compile-gated only.
- **D23 — static CRT for shipped artifacts.** `xtask cross`/`package` build release artifacts with
  `-C target-feature=+crt-static` (appended through `--config`, so machine-local linker flags are
  kept). The TIP is loaded into arbitrary processes and must not depend on the Visual C++
  redistributable.

### 1.3 Approved third-party crates

| Crate | Use | Crates |
|---|---|---|
| `serde` (+derive), `serde_json`, `toml` | documents, IPC payloads | model, config, macro, policy, ipc, migrate, tests, xtask |
| `unicode-normalization` | NFC/NFD | engine, macro, converter, model (validation only) |
| `sha2` | checksums, fingerprints | config, migrate, xtask |
| `crc32fast` | IPC frame checksum | ipc |
| `windows` | Win32/COM/TSF bindings | windows, controller, tsf (cfg(windows) only) |
| `proptest` | property tests (dev) | all core crates |
| `tempfile` | temp dirs in tests (dev) | config, migrate, converter, tests |
| `object` | PE inspection | xtask |

`raumakey-*` crates are `publish = false`, `license = "GPL-3.0-or-later"` (D2).

---

## 2. `raumakey-model` contracts

All types derive `Clone, Debug, PartialEq, Eq` where meaningful; document types also derive
`serde::{Serialize, Deserialize}`. Constructors validate invariants; fields that carry invariants
are private with accessors.

### 2.1 `identity`

```rust
pub struct Guid(pub u128);            // Display: "{0FC48F2B-4F91-44A0-B500-DE512E801432}"
pub const PRODUCT_NAME: &str = "Raumakey";
pub const TIP_CLSID: Guid;            // {0FC48F2B-4F91-44A0-B500-DE512E801432}
pub const TIP_PROFILE_GUID: Guid;     // {DB875058-BBF9-407C-85A8-950BE7D14C22}
pub const INSTALLER_UPGRADE_CODE: Guid; // {BF73893F-FCEB-4BAE-AE1C-E81D7107ECE5}
pub const NAMED_OBJECT_PREFIX: &str = "Local\\Raumakey.";
pub const REGISTRY_ROOT: &str = "Software\\Raumakey";   // under HKCU
pub const LANGID_VIETNAMESE: u16 = 0x042A;
pub const RAUMAKEY_INJECTED_TAG: u64 = 0x524B_5931;      // "RKY1", fits a 32-bit dwExtraInfo (D6)
```

The model never names EVKey identities. The legacy constants live only in
`raumakey-ipc::legacy` behind the `evkey-compat` feature.

### 2.2 `utf16`

`is_well_formed`, `validate -> Result<(), Utf16Error { index }>`, `is_scalar_boundary(units, i)`,
`scalar_count`, `single_scalar(units) -> Option<char>`, `encode(&str) -> Vec<u16>`,
`decode(&[u16]) -> Result<String, Utf16Error>`.

### 2.3 `host`

```rust
pub struct SessionId(pub [u8; 16]);
pub struct DocumentId(pub [u8; 16]);
pub struct ExeName(String);           // lowercase basename; non-empty; <= 255 chars; no \ / : NUL
pub struct HostContext {
    pub session_id: SessionId, pub process_id: u64, pub thread_id: u64,
    pub executable: ExeName, pub window_class: Option<String>,
    pub document_id: Option<DocumentId>, pub supports_tsf_range_edit: bool,
    pub is_secure_field: bool, pub is_elevated: bool,
    pub keyboard_layout: String, pub foreground_epoch: u64,
}
```

### 2.4 `input`

```rust
pub enum InputKind { Character, Backspace, Separator, Navigation, MouseReset, FocusReset,
                     ModeToggle, ConfigChanged, Shutdown }
pub struct Modifiers { pub shift, pub caps_lock, pub ctrl, pub alt, pub alt_gr, pub win: bool }
pub struct InputEvent {
    pub kind: InputKind, pub text_utf16: Vec<u16>, pub virtual_key: Option<u32>,
    pub scan_code: Option<u32>, pub modifiers: Modifiers, pub repeat: bool,
    pub injected_tag: Option<u64>, pub monotonic_time_ms: u64, pub host: HostContext,
}
pub struct ModifierSet { pub shift, pub ctrl, pub alt, pub win: bool }
pub struct ReplayKey { pub virtual_key: u32, pub scan_code: Option<u32>, pub modifiers: ModifierSet }
pub mod vk { BACK=0x08, TAB=0x09, RETURN=0x0D, SHIFT=0x10, CONTROL=0x11, MENU=0x12, ESCAPE=0x1B,
             SPACE=0x20, PRIOR..DOWN=0x21..0x28, DELETE=0x2E, F12=0x7B, ... }
```

`InputEvent::validate()` rejects: text on kinds other than `Character`/`Separator`; ill-formed
UTF-16; more than `MAX_EVENT_TEXT_UNITS = 16` units. `is_shortcut()` is
`(ctrl || alt || win) && !alt_gr`. Constructors: `character(char, host)`,
`separator(char, vk, host)`, `backspace(host)`, `navigation(vk, host)`, `simple(kind, host)`.

### 2.5 `options`

```rust
pub enum InputMethod { Telex, Vni, Viqr, UserDefined }
pub enum Normalization { Nfc, Nfd, Preserve }
pub struct EngineOptions {
    pub enabled: bool, pub input_method: InputMethod, pub normalization: Normalization,
    pub modern_tone: bool, pub free_marking: bool, pub spell_check: bool,
    pub auto_restore_invalid: bool, pub macros_enabled: bool, pub macros_when_disabled: bool,
    pub auto_case_macro: bool, pub user_keymap_id: Option<String>,
    pub context_limit_codepoints: u32,
}
```

**D4 — versioned defaults v1** (`EngineOptions::defaults_v1()`, `DEFAULTS_VERSION = 1`):
enabled `true`, Telex, NFC, `modern_tone = false` (òa/úy), `free_marking = true`,
`spell_check = true`, `auto_restore_invalid = true`, `macros_enabled = true`,
`macros_when_disabled = false`, `auto_case_macro = true`, no user keymap,
`context_limit_codepoints = 32` (valid range 8..=128).

### 2.6 `keymap`

```rust
pub enum Tone { Acute, Grave, Hook, Tilde, Dot }            // sắc huyền hỏi ngã nặng
pub enum BaseVowel { A, E, O }
pub enum KeyAction {
    Tone(Tone), RemoveTone,
    Circumflex,                  // any of a/e/o (VNI 6, VIQR ^)
    CircumflexOn(BaseVowel),     // Telex aa/ee/oo
    Breve,                       // VNI 8, VIQR (
    Horn,                        // VNI 7, VIQR + *
    HornOrBreve,                 // Telex w (also standalone ư)
    Stroke,                      // VNI 9, Telex/VIQR d
    Escape,                      // VIQR backslash
}
pub struct KeyBinding { pub key: char, pub actions: Vec<KeyAction> }   // ordered candidates
pub struct KeyMap { pub id: String, pub version: u32, pub name: String, pub bindings: Vec<KeyBinding> }
pub const KEYMAP_SCHEMA_VERSION: u32 = 1;
```

TOML/string form of actions: `tone:acute|grave|hook|tilde|dot`, `remove-tone`, `circumflex`,
`circumflex:a|e|o`, `breve`, `horn`, `horn-or-breve`, `stroke`, `escape`.
Validation: id `[A-Za-z0-9._-]{1,64}`, name ≤ 128 chars, keys printable ASCII `0x21..=0x7E`, unique
(ASCII letters compare case-insensitively), 1..=4 actions per key, version = schema version.

### 2.7 `transaction`

```rust
pub struct EditTransaction {
    pub delete_before: u32, pub insert_utf16: Vec<u16>, pub consume_original: bool,
    pub replay_key: Option<ReplayKey>, pub reset_context: bool,
}
pub enum TransactionError { MalformedInsert, DeleteExceedsContext { requested, available },
    DeleteExceedsPolicy { requested, max }, InsertTooLong, SplitsSurrogate, ReplayWithoutConsume }
pub const MAX_INSERT_UNITS: usize = 8192;
pub struct TextSimulator { /* UTF-16 text, caret at end */ }
```

- `EditTransaction::pass_through()`, `pass_through_reset()`, `replace(delete, &str)`.
- `validate(available_units, max_delete)`: insert well-formed and ≤ `MAX_INSERT_UNITS`;
  `delete_before ≤ available` and `≤ max_delete`; `replay_key` only with `consume_original`.
- `TextSimulator::apply(&event, &tx)`: rejects a delete that exceeds the text or starts inside a
  surrogate pair; truncates, appends `insert_utf16`; if `!consume_original` applies the event's
  natural effect (Character/Separator text is appended, `Backspace` deletes the last scalar,
  other kinds do nothing); if `replay_key` is set appends the replayed key's text (`RETURN` → `"\r"`,
  `TAB` → `"\t"`, `SPACE` → `" "`). This is the oracle used by the compatibility suites
  ("each step applies the transaction to the current text/caret").

### 2.8 `policy` (types only; resolution lives in `raumakey-policy`)

```rust
pub enum Transport { Tsf, SyntheticKey, Clipboard }
pub enum SecureFieldMode { Bypass, Restricted }
pub struct HostPolicy {
    pub transport: Transport, pub reset_on_mouse: bool, pub permit_replay: bool,
    pub max_delete_before: u32, pub macro_allowed: bool, pub secure_field_mode: SecureFieldMode,
    pub vietnamese_allowed: bool,
}
pub struct RuleAction {               // every field optional: None = inherit
    pub transport: Option<Transport>, pub reset_on_mouse: Option<bool>, pub macros: Option<bool>,
    pub vietnamese: Option<bool>, pub max_delete_before: Option<u32>,
}
pub struct AppRule { pub exe: ExeName, pub window_class: Option<String>, pub action: RuleAction }
pub struct NamedSet { pub name: String, pub members: Vec<ExeName>, pub action: RuleAction }
pub struct ProbeRecord { pub exe: ExeName, pub probe_version: u32, pub tsf_range_edit: bool }
pub enum ProbeResult { TsfRangeEditOk, TsfRangeEditFailed }
pub struct CompatibilityConfig {
    pub clipboard_enabled: bool, pub default_reset_on_mouse: bool,
    pub secure_field_mode: SecureFieldMode, pub default_max_delete_before: u32,
    pub rules: Vec<AppRule>, pub named_sets: Vec<NamedSet>, pub probes: Vec<ProbeRecord>,
}
pub const PROBE_VERSION: u32 = 1;
```

### 2.9 `macros` (types and lookup port)

```rust
pub struct MacroId(pub u64);
pub enum BoundaryPolicy { Delimiter, Whitespace }   // default Delimiter
pub enum CasePolicy { Adapt, Exact }                 // default Adapt
pub struct MacroEntry { pub id: MacroId, pub trigger: String, pub replacement: String,
                        pub enabled: bool, pub boundary: BoundaryPolicy, pub case_policy: CasePolicy }
pub enum Boundary { Whitespace, Punctuation }
pub enum CaseMode { Exact, Adaptive }
pub enum CasePattern { Lower, Title, Upper, Mixed }
pub struct MacroExpansion { pub id: MacroId, pub replacement: String }   // NFC, case-adapted
pub trait MacroLookup: Send + Sync {
    fn lookup(&self, typed: &str, boundary: Boundary, case_mode: CaseMode) -> Option<MacroExpansion>;
}
pub struct NoMacros;
```

### 2.10 `hotkey`, `error`, `diag`

```rust
pub enum HotkeyKey { ModifiersOnly, VirtualKey(u32) }
pub struct Hotkey { pub ctrl: bool, pub shift: bool, pub alt: bool, pub win: bool, pub key: HotkeyKey }
// text form: "Ctrl+Shift", "Ctrl+Shift+Alt+F12", "Alt+Z"; parse/Display round-trip
pub struct HotkeyConfig { pub toggle_mode: Vec<Hotkey>, pub hard_reset: Vec<Hotkey> }
pub enum HotkeyAction { ToggleMode, HardReset }

pub enum EngineError { InvalidUtf16, InvalidStateVersion, ContextMismatch, CapacityExceeded,
                       UnsupportedInputMethod, InternalInvariant }
pub enum ResetReason { Navigation, Mouse, Focus, ModeToggle, ConfigChanged, Shutdown, HardReset,
                       ApplyFailed, ContextChanged, WordCommitted }

pub enum Component { Engine, Tsf, Controller, Config, Macro, Policy, Ipc, Converter, Migrate, Installer }
pub struct DiagCode(pub u32);
pub struct VectorId(String);          // [A-Za-z0-9._-]{1,32}
pub struct DiagEvent { pub timestamp_ms: u64, pub component: Component, pub code: DiagCode,
                       pub revision: Option<u64>, pub host_exe: Option<ExeName>,
                       pub vector_id: Option<VectorId> }
```

**D5 — default hotkeys:** toggle `Ctrl+Shift` (modifier-only tap); hard reset
`Ctrl+Shift+Alt+F12` and `Ctrl+Shift+Alt+Space` (matrix #25).

`DiagEvent::to_log_line()` produces `"<ts> <component> <code> rev=<n|-> exe=<name|-> vec=<id|->"`.
There is no API that accepts arbitrary text for logging.

---

## 3. `raumakey-engine`

### 3.1 Public API

```rust
pub const STATE_VERSION: u32 = 1;
pub const HISTORY_CAPACITY: usize = 32;
pub enum WordForm { Empty, C, V, CV, VC, CVC, NonVietnamese }
pub struct EngineState { /* private */ }
impl EngineState {
    pub fn new() -> Self;                      // also Default
    pub fn version(&self) -> u32;
    pub fn generation(&self) -> u64;
    pub fn is_idle(&self) -> bool;             // no owned text
    pub fn owned_utf16(&self) -> &[u16];       // rendered current word believed before the caret
    pub fn word_form(&self) -> WordForm;
}
pub struct EngineResult { pub transaction: EditTransaction, pub next: EngineState }
pub struct VietnameseEngine { /* Arc<KeymapSet>, Arc<dyn MacroLookup> */ }
impl VietnameseEngine {
    pub fn new(user_keymaps: Vec<KeyMap>, macros: Arc<dyn MacroLookup>) -> Self;
    pub fn builtin_keymap(method: InputMethod) -> Option<&'static KeyMap>;
    pub fn process(&self, event: &InputEvent, options: &EngineOptions, prior: &EngineState)
        -> Result<EngineResult, EngineError>;
    pub fn reset(&self, prior: &EngineState, reason: ResetReason) -> EngineState;
}
```

`process` is a pure function of `(self, event, options, prior)`. The engine never mutates `prior`.
The adapter commits `next` only after it applied `transaction` successfully.

### 3.2 Built-in key maps

| Method | Bindings |
|---|---|
| Telex | `s`→Tone(Acute) `f`→Tone(Grave) `r`→Tone(Hook) `x`→Tone(Tilde) `j`→Tone(Dot) `z`→RemoveTone `a`→CircumflexOn(A) `e`→CircumflexOn(E) `o`→CircumflexOn(O) `w`→HornOrBreve `d`→Stroke |
| VNI | `1` Acute `2` Grave `3` Hook `4` Tilde `5` Dot `0` RemoveTone `6` Circumflex `7` Horn `8` Breve `9` Stroke |
| VIQR | `'` Acute `` ` `` Grave `?` Hook `~` Tilde `.` Dot `-` RemoveTone `^` Circumflex `(` Breve `+` Horn `*` Horn `d` Stroke `\` Escape |

ASCII letter keys match case-insensitively; the case of a *literal* letter is the case typed.

### 3.3 Word model

A word is a sequence of items plus one word-level tone:

```text
Item   = Letter { base: 'a'..='z', upper: bool, mark: None|Circumflex|Breve|Horn|Stroke,
                  from_w: bool /* standalone ư produced by Telex w */ }
       | Other(char)               // digit, symbol, backslash, non-ASCII: makes the word non-Vietnamese
Word   = { items, tone: Option<(Tone, item_index)>, literal_mode: bool, transformed: bool }
```

Vietnamese vowels (by `base`+`mark`): a ă â e ê i o ô ơ u ư y. Letters `f j w z` (unmarked) are not
Vietnamese letters.

### 3.4 Syllable grammar (spell check and tone placement)

Letters are compared in lowercase with marks and without tone.

**Onsets:** `"" b c ch d đ g gh gi h k kh l m n ng ngh nh p ph qu r s t th tr v x`.
Orthographic constraints: `k`, `gh`, `ngh` only before `e ê i y`; `c`, `ng` never before
`e ê i y`; `g` before `i` is the merged `gi` onset (`gì`, `gìn`, `giếng`); `q` must be followed by
`u` and `qu` must be followed by a vowel.

**Rhymes** (`cluster: allowed codas`; `open` means no coda is allowed):

```text
a: open c ch m n ng nh p t      ă: c m n ng p t          â: c m n ng p t
e: open c m n ng p t            ê: open ch m n nh p t    i: open ch m n nh p t
o: open c m n ng p t            ô: open c m n ng p t     ơ: open m n p t
u: open c m n ng p t            ư: open c m n ng t       y: open
ai ao au ay âu ây eo êu iu oi ôi ơi ui ưi ưu ia ua ưa uơ: open
iê: c m n ng p t                yê: m n ng t             uâ: n ng t
uê: open ch n nh                uy: open ch n nh p t     uô: c m n ng t
ươ: c m n ng p t                oa: open c ch m n ng nh p t
oă: c m n ng t                  oe: open n t             oo: c ng
uyê: n t
iêu yêu oai oay oao oeo uây uôi ươi ươu uyu uya: open
```

After `qu` (the `u` belongs to the onset) the allowed clusters are:

```text
a: open c ch m n ng nh t   ă: c m n ng t   â: n ng t   e: open n t   ê: open ch n nh
i: open ch n nh t          y: open ch n nh t           yê: n t       ơ: open   ô: c
ai ay ao ây eo: open
```

After `gi` (merged onset, `i` belongs to the onset) every main-table cluster that does not start
with `i` or `y` is allowed; `g` + a cluster starting with `i` is also valid.

`SYLLABLES` = every onset × allowed (cluster + coda) combination, honoring the constraints above,
kept as a sorted list.

- `classify(letters) -> Invalid | Prefix | Complete`: `Complete` iff `letters ∈ SYLLABLES`;
  `Prefix` iff some syllable starts with `letters` (the empty word and onset prefixes such as
  `n`, `ng`, `ngh`, `q`, `qu` are prefixes).
- **Tone/coda rule** (complete words only): codas `c ch p t` allow only Acute, Dot, or no tone.
- `split(letters) -> (onset, cluster, coda)`: longest onset match with `qu` when followed by a
  vowel, `gi` only when the `i` is followed by another vowel; cluster = maximal vowel run after the
  onset; coda = the rest.

### 3.5 Tone placement

Given the split of the current word:

1. If the cluster contains a vowel with a mark (â ê ô ă ơ ư), the tone goes on the **last** marked
   vowel (`ươ`→ơ, `ưu`→ư, `iê`/`yê`/`uyê`→ê, `uô`→ô, `uâ`→â).
2. Else if there is a coda, the tone goes on the last vowel of the cluster (`toán`, `hoàng`).
3. Else by cluster length: 1 → that vowel; 3 → middle vowel (`oai`, `uya`, `uyu`, `oeo`);
   2 → if `modern_tone` and the cluster is `oa`, `oe`, or `uy` then the second vowel
   (`hoà`, `khoẻ`, `thuỷ`), otherwise the first vowel (`hòa`, `của`, `mía`, `mái`).
4. Longer or non-parsable words keep the last computed position (`tone_position` is frozen when the
   word stops being a valid prefix).

### 3.6 Action application

Processing a `Character` that is a single ASCII scalar in Vietnamese mode:

1. If the word is in `literal_mode` (after an undo or overflow), append the key literally.
2. If `spell_check` is on and the current letters are not a valid prefix (`Invalid`), append
   literally (no transformation on non-Vietnamese words).
3. Look up the key's candidate actions; for each, `try_apply` returns `Applied`, `Undo`, or
   `NotApplicable`. The first non-`NotApplicable` result wins. Otherwise append the key literally.
4. When `spell_check` is on, an `Applied` candidate is accepted only if the resulting letters are a
   valid prefix; otherwise it is `NotApplicable`.
5. When `free_marking` is off, tone and mark actions apply only while the word ends inside its vowel
   cluster (no coda typed yet), and `Stroke` only when the word is exactly the onset `d`.

`try_apply` rules:

- **Tone(t):** needs a cluster vowel. Same tone already present → `Undo`. Different or no tone →
  set tone (replace).
- **RemoveTone:** tone present → remove (`Applied`, no literal); otherwise `NotApplicable`.
- **CircumflexOn(v):** target = last cluster vowel with base `v`. Already circumflex → `Undo`;
  breve/none → set circumflex.
- **Circumflex:** candidates = cluster vowels with base a/e/o, last to first; a cluster vowel that
  already has a circumflex → `Undo`; otherwise the first candidate whose result is a valid prefix,
  or (spell check off) the last candidate.
- **Breve:** target = last cluster `a`; already breve → `Undo`; otherwise set breve.
- **Horn:** if the cluster contains `u` immediately followed by `o` (outside a `qu` onset): if both
  already horned (or `o` horned with nothing after) → `Undo`; else horn the pair: both `u` and `o`
  when something follows the `o`, only the `o` (`uơ`: `thuở`, `huơ`) when the `o` ends the word.
  Otherwise candidates are single `u`/`o` cluster vowels (first valid result wins; `Undo` if the
  chosen vowel is already horned).
- **HornOrBreve (Telex w):** Horn candidates, then Breve; first valid wins. With no vowel in the
  word, append a standalone `ư` (`from_w = true`, case of the key). Undo of a standalone `ư`
  removes it.
- **Stroke:** target = onset `d` (first item). Already `đ` → `Undo`; otherwise set stroke.
- **Escape (VIQR `\`):** handled before actions: the backslash is appended as `Other('\\')` and
  `escape_armed` is set. When armed and the next key has any VIQR action binding, that backslash item
  is replaced by the key as a literal `Other` (`\'` yields `'`, matrix #8); any other key disarms and
  is processed normally.

**Undo:** revert the transformation (remove the tone / mark / standalone `ư`), append the current key
as a literal, set `literal_mode` for the rest of the word. `a s s` → `as`, `a a a` → `aa`, `w w` →
`w`, `d d d` → `dd`.

**`uơ` fix-up:** when a letter is appended after a cluster ending in plain `u` + horned `o` (outside a
`qu` onset) and the new letter is a coda consonant or the glide `i`/`u`, the `u` also receives the
horn (`huow` → `huơ`; `huow`+`ng` → `hương`; `nguo7i` → `ngươi`).

### 3.7 Rendering and diff

- Each letter renders to its precomposed lowercase form with the tone applied to the tone index,
  then uppercased when `upper`. `Other(c)` renders as `c`.
- `Nfc` and `Preserve` emit precomposed text; `Nfd` emits canonical decomposition
  (`unicode-normalization`).
- `diff(old, new)`: common prefix in UTF-16 units, backed off so it never splits a surrogate pair and
  never separates a base character from following combining marks; `delete_before = old.len() -
  prefix`, `insert = new[prefix..]`.
- If the diff is exactly "append the event's own text" the transaction is pass-through
  (`consume_original = false`, empty insert); otherwise `consume_original = true`.

### 3.8 Event handling

| Event | Behavior |
|---|---|
| `injected_tag == RAUMAKEY_INJECTED_TAG` | pass-through, state unchanged |
| `host.is_secure_field` | pass-through with `reset_context`, state reset (matrix #29) |
| `Character` with `is_shortcut()` | pass-through with `reset_context`, state reset (matrix #21) |
| `Character` with `alt_gr` | processed as a normal character (matrix #22) |
| `Character` boundary punctuation (`. , ; : ! ? " ' ( ) [ ] { } < > / \ |`), when not consumed by an action | word boundary with `Boundary::Punctuation` |
| `Character` other non-letter (digits, `@ # $ % & * - _ + = ~ ` ^`, non-ASCII) | appended as `Other` |
| `Character` with more than one scalar | commit the word (state reset), pass-through |
| `Separator` (space, tab, enter; also `Character` `' '`, `'\t'`, `'\r'`, `'\n'`) | word boundary with `Boundary::Whitespace` |
| `Backspace` | semantic backspace (§3.9) |
| `Navigation`, `MouseReset`, `FocusReset`, `ModeToggle`, `ConfigChanged`, `Shutdown` | pass-through with `reset_context`, state reset |

**Word boundary:**

1. Shift+Space (separator `' '` with `shift`) commits the word unchanged: no macro, no restore (D7).
2. Macros are active when `macros_enabled && (enabled || macros_when_disabled)`. Lookup the
   rendered NFC word, then the raw-key text if different, with `CaseMode::Adaptive` when
   `auto_case_macro`, else `Exact`.
3. Otherwise, if `enabled && spell_check && auto_restore_invalid && word.transformed` and the
   rendered word is not a complete valid syllable (including the tone/coda rule) → replace it with
   the raw keys (matrix #15). With `auto_restore_invalid = false`, commit as rendered (matrix #16).
4. A replacement is emitted as `Tx(owned_len, text + delimiter, consume = true)` for printable
   delimiters; for Enter/Tab it is `Tx(owned_len, text, consume = true, replay_key = <vk>)`.
   Without a replacement the delimiter passes through. The state resets; `reset_context = true`
   whenever a non-empty word is committed.

**Disabled mode:** characters pass through. When macros are active while disabled, the engine still
tracks the typed word literally so that a boundary can expand a macro.

### 3.9 History, capacity, errors

- Every state change of the current word pushes the previous word snapshot into a ring of
  `HISTORY_CAPACITY` (32) entries; the oldest is evicted.
- **Backspace:** idle → pass-through. Snapshot available → restore it and emit the diff (`a a BS` →
  `a`, matrix #17). No snapshot left but owned text → pass-through and reset (matrix #18 fallback).
- **Overflow:** when a word would exceed `context_limit_codepoints` keys, the word is dropped from
  ownership and further characters pass through until the next boundary (no OOB, deterministic).
- **Errors:** ill-formed text → `InvalidUtf16`; `prior.version() != STATE_VERSION` →
  `InvalidStateVersion`; non-idle state owned by another `(session_id, document_id)` →
  `ContextMismatch`; `UserDefined` with an unknown or missing keymap → `UnsupportedInputMethod`;
  text longer than 16 units → `CapacityExceeded`.

### 3.10 Golden vectors the engine must pass (Telex unless stated)

```text
aa→â  dd→đ  as→á  aw→ă  ow→ơ  uw→ư  w→ư  ww→w  tw→tư  nhw→như  ass→as  aaa→aa  ddd→dd
tieengs→tiếng  vieetj→việt  nguwowif→người  nguoiwf→người  thuowr→thuở  huow→huơ
truowngf→trường  ruowuj→rượu  khuyeens→khuyến  quas→quá  quys→quý  quyeets→quyết
gias→giá  gif→gì  giuwax→giữa  giuwowngf→giường  ddi→đi  did→đi  Ddaay→Đây  VIEETJ→VIỆT
hoaf→hòa (old) / hoà (modern)  thuyr→thủy / thuỷ  khoer→khỏe / khoẻ  hoafn→hoàn (both)
chaof→chào  cuar→của  muoons→muốn  nghieeng→nghiêng  awa→â  tesst→test
after+space→"after " (auto-restore)  windows+space→"windows "  class→class
VNI: tie6ng1→tiếng  vie65t→việt  nguo72i→người  a66→a6  d9→đ
VIQR: tie^'ng→tiếng  vie^.t→việt  ngu+o+`i→người  \'→'  dd→đ
```

---

## 4. `raumakey-macro`

- `MacroTable::new(entries) -> Result<Self, MacroError>`: indexes enabled entries by exact NFC
  trigger and by lowercase trigger. Implements `MacroLookup`.
- Lookup order: exact trigger; then (Adaptive only) lowercase match. Ties resolve to the lowest
  `MacroId`. `BoundaryPolicy::Whitespace` entries expand only on `Boundary::Whitespace`.
- Case adaptation (Adaptive and `CasePolicy::Adapt`): pattern of the typed trigger — all lowercase →
  replacement as stored; first letter upper and the rest lower (or a single upper letter) → Title
  (uppercase first letter of the replacement); every cased letter upper and more than one letter →
  Upper; anything else → as stored.
- Validation: trigger 1..=64 scalars without whitespace or control characters; replacement
  1..=4096 scalars without NUL; both stored NFC.
- `MacroStore { revision, entries }`: `upsert(expected_revision, entry) -> Result<u64, MacroError>`,
  `remove(expected_revision, id)`, `list()`, `snapshot() -> Arc<MacroTable>`; stale revision →
  `MacroError::Conflict`. New IDs are `max(id) + 1`.
- The persisted `MacroDocument` lives in `raumakey-config` (§5) so that the config crate depends
  only on the model; `MacroStore::from_entries(revision, entries)` and `MacroStore::entries()`
  convert to and from it.
- `parse_legacy_text(bytes) -> LegacyMacroImport { entries, issues }`: decodes UTF-8 (with/without
  BOM) or UTF-16LE/BE with BOM; skips blank lines and lines starting with `#`, `;`, `//`, or
  containing `DO NOT DELETE`; splits each remaining line at the first `:`; invalid lines become
  issues with line numbers only (no content). `export_text(entries) -> String` writes
  `trigger:replacement` lines.

---

## 5. `raumakey-config`

- `DataRoot::resolve(exe_dir, local_app_data) -> DataRoot`: portable iff `exe_dir/raumakey.portable`
  exists → `exe_dir/data/`; otherwise `%LOCALAPPDATA%/Raumakey/` (D9). Never inferred from write
  access.
- Documents (each with `version: u32`, `revision: u64`, and an `extensions` map that retains unknown
  keys):
  - `ConfigDocument` → `config.toml` (sections `input`, `macros`, `hotkeys`, `system`,
    `conversion`, `[[keymaps]]`; see §5.1).
  - `MacroDocument { version, revision, entries: Vec<MacroEntry>, extensions }` → `macros.toml`
    (array `[[macro]]`). Structural decoding only; semantic macro validation happens when
    `raumakey-macro` builds a table.
  - `AppRulesDocument` → `app_rules.toml` (wraps `CompatibilityConfig`).
  - `MigrationRecord` → `migration.json`.
- **Field-level loading:** syntax errors → `ConfigError::Corrupt` (the file is left untouched; the
  caller runs on defaults and must not overwrite it silently). For a syntactically valid file each
  known field is decoded independently; a bad field falls back to its default and is reported as a
  `FieldIssue { path, reason_code }`; unknown fields go to `extensions` (matrix #45).
  `version > CURRENT` → `UnsupportedVersion`; older versions run `migrate`.
- **Atomic commit** `AtomicStore<D>::commit(expected_revision, doc) -> Result<u64, ConfigError>`:
  take an exclusive lock on `<file>.lock` (`std::fs::File::lock`), re-read the current revision,
  `Conflict` if it differs from `expected_revision` (matrix #46), set `revision + 1`, serialize, write
  `<file>.tmp-<pid>-<n>` in the same directory, `sync_all`, replace via the injected `FileReplacer`
  (`StdReplacer` = `std::fs::rename`; Windows adapter = `ReplaceFileW`), read back, compare SHA-256,
  release the lock, return the new revision.
- `ConfigError = NotFound | Corrupt | UnsupportedVersion | PermissionDenied | Conflict | IoFailure`.
- `watch`: `AtomicStore::poll_revision()` returns the on-disk revision cheaply; the controller polls
  and the store notifies in-process subscribers through `std::sync::mpsc` on commit.
- `DiagnosticsLog`: opt-in sink writing `DiagEvent::to_log_line()` to `logs/raumakey-<yyyymmdd>.log`,
  max 1 MiB per file, 5 files kept.

### 5.1 `config.toml` schema v1

```toml
version = 1
revision = 1
[input]
enabled = true
method = "telex"            # telex | vni | viqr | user
user_keymap = "my-map"      # optional
normalization = "nfc"       # nfc | nfd | preserve
modern_tone = false
free_marking = true
spell_check = true
auto_restore_invalid = true
context_limit = 32
[macros]
enabled = true
when_disabled = false
auto_case = true
[hotkeys]
toggle_mode = ["Ctrl+Shift"]
hard_reset = ["Ctrl+Shift+Alt+F12", "Ctrl+Shift+Alt+Space"]
[system]
startup = "none"            # none | shortcut | elevated-task
show_panel_on_start = true
diagnostics = false
[conversion]
source = "unicode"
target = "unicode"
case = "none"               # none | upper | lower | title
remove_tones = false
[[keymaps]]
id = "my-map"
version = 1
name = "My map"
bindings = [ { key = "s", actions = ["tone:acute"] } ]
```

`app_rules.toml` v1:

```toml
version = 1
revision = 1
clipboard_enabled = false
default_reset_on_mouse = true
secure_field_mode = "bypass"     # bypass | restricted
default_max_delete_before = 64
[[rule]]
exe = "notepad.exe"
window_class = "Notepad"         # optional
transport = "synthetic-key"      # tsf | synthetic-key | clipboard (all action fields optional)
reset_on_mouse = true
macros = true
vietnamese = false
max_delete_before = 64
[[named_set]]
name = "chromium-family"
members = ["msedge.exe", "chrome.exe"]
action = { reset_on_mouse = true }
[[probe]]
exe = "legacy.exe"
probe_version = 1
tsf_range_edit = false
```

---

## 6. `raumakey-policy`

- `CompatibilityPolicy::new(config: Arc<CompatibilityConfig>)`;
  `resolve(&self, host: &HostContext) -> HostPolicy`;
  `record_probe(&mut self, host, ProbeResult) -> Result<(), PolicyError>` (updates the in-memory
  probe list; the controller persists it in `app_rules.toml`).
- Resolution order: base defaults → named sets in declared order → app rules (exe match; a rule with
  `window_class` beats one without) → capability: `!host.supports_tsf_range_edit` or a failed probe
  with the current `PROBE_VERSION` forces `SyntheticKey` unless a rule explicitly selected a
  transport → `Clipboard` is downgraded to `SyntheticKey` unless `clipboard_enabled` → secure field:
  `macro_allowed = false`, never `Clipboard`, `vietnamese_allowed = false` in `Bypass` mode.
- **D13 default named sets** (data in `CompatibilityConfig::defaults_v1()`):
  `chromium-family` = msedge.exe, chrome.exe, brave.exe, vivaldi.exe, opera.exe, ms-teams.exe,
  teams.exe, slack.exe, discord.exe → `reset_on_mouse = true`; `terminals` = windowsterminal.exe,
  wt.exe, openconsole.exe, conhost.exe → `reset_on_mouse = true`; `games` = empty, no action.
  Text results for these hosts equal the generic oracle (matrix #26); each set keeps its own action
  (matrix #28).
- `hotkey::HotkeyClassifier`: fed key-down/key-up events with modifier state; reports
  `HotkeyAction` for configured combos. A `ModifiersOnly` hotkey fires on release of the last
  modifier only if no other key was pressed while the modifiers were held.
- `ProbeCache` helpers: upsert by exe, keep only the newest `probe_version`.

---

## 7. `raumakey-ipc`

### 7.1 Frame

```text
offset size field
0      4    magic "RMK1"
4      2    protocol_version (1)
6      2    kind
8      2    flags (0)
10     2    header_len (40)
12     4    payload_len (<= MAX_PAYLOAD = 8 MiB)
16     8    client_id
24     8    request_id
32     4    checksum (CRC-32 over bytes 0..32 and the payload)
36     4    reserved (0)
40     ..   payload (UTF-8 JSON of the message body)
```

`FrameDecoder` accepts bytes incrementally, validates header length, version, kind, and
`payload_len` **before** allocating, verifies the checksum, then decodes the JSON body.
Errors: `BadMagic, UnsupportedVersion, UnknownKind, Oversize, ChecksumMismatch, Malformed`.

### 7.2 Messages

`Hello { client_kind: Tip|Tool, pid, arch, protocol }` → `Welcome { client_id, server_revision }`;
`RuntimeSnapshot { config_revision, options, keymaps, hotkeys, compatibility, vietnamese_mode }`;
`MacroSnapshot { macro_revision, entries }`; `Command { ToggleMode | SetMode(bool) | HardReset |
ReportProbe(ProbeRecord) }`; `CommandResult { ok, code }`; `ModeChanged { vietnamese_mode, revision }`;
`ResetAll`; `Diagnostic(DiagEvent)` (opt-in only); `Ping`/`Pong`; `Error { code }`; `Goodbye`.
No message type carries keystrokes or document text.

### 7.3 State machines

`ClientSession` (TIP side): handshake, keeps the last valid snapshot, `Backoff` (100 ms doubling to
10 s, deterministic) for reconnects. `ServerSession` (controller side): validates `Hello`, assigns
`client_id`, checks every later frame carries it, sends snapshots on connect and on every new
revision. Transport is any `Read + Write`; the named pipe lives in `raumakey-windows`.

### 7.4 Legacy codec (`evkey-compat`, off by default)

Implements clean-room spec §6 exactly: mapping size `0x1038`, offsets `0x00..0x38`, opcodes
1/2/3/5, raw-u32 fields with range validation, even output length `<= 0x1000`, epoch validity
2000 ms, mutex wait 100 ms, response wait 50 ms, stale-response drain, sequence matching, no retry
after an ambiguous timeout, unknown flags preserved, legacy sentinel `0xEBCA1234`. The object names
are constants in this module only. `LegacyClient`/`LegacyServer` run over a `LegacyChannel` trait
(shared block + request/response signals + mutex + clock) so matrix #30–#38, #42, #55 are tested with
fakes. Activation fails closed when an EVKey object already exists.

---

## 8. `raumakey-converter`

- `Charset { Unicode, UnicodeNfd, Viqr, Tcvn3, VniWindows, Viscii, Cp1258, NcrDecimal, NcrHex,
  Utf8Literal }` (D19). Legacy byte charsets are represented in text form through Windows-1252
  (`cp1252` byte↔char), which is how such text appears on the clipboard and in Unicode documents.
- `decode(text, from) -> Result<String, ConvertError>` (to Unicode NFC),
  `encode(text, to) -> Result<String, ConvertError>`,
  `convert(text, from, to, options) -> Result<String, ConvertError>`;
  `options { case: None|Upper|Lower|Title, remove_tones: bool }`.
- Byte-level `decode_bytes`/`encode_bytes` for file conversion.
- TCVN3 has no precomposed uppercase toned letters; encoding them falls back to the lowercase code and
  the result reports `lossy = true`.
- `ClipboardPort { sequence_number(), read_text(), write_text() }`;
  `convert_clipboard(port, from, to, options, secure_field) -> Result<ClipboardOutcome, ConvertError>`:
  refuses in secure fields; reads, converts, verifies the sequence number is unchanged before
  writing, reports `OwnershipConflict` otherwise; never restores over newer clipboard content.
- `convert_file(src, dst, from, to, options, overwrite) -> Result<FileOutcome, ConvertError>`: refuses
  `dst == src`; refuses an existing `dst` unless `overwrite`; writes via temp file + rename.

---

## 9. `raumakey-migrate`

- Ports: `LegacyRegistry` (read-only: values and subkeys of a path) and `SourceFs` (read-only:
  read file, list directory). No write method exists on either port.
- Sources: registry `HKCU\Software\MyQ\EVKey` (settings) and `HKCU\Software\EVKey\TsfCache`
  (hints only); files `setting.ini`, `setting.tree`, `evkmacro.txt` in a user-chosen or discovered
  EVKey folder.
- Parsing is field-independent: macros through `raumakey_macro::parse_legacy_text`; INI through a
  tolerant parser; registry values as typed raw values; `setting.tree` preserved as raw bytes.
- **D18 mapping policy:** a field is imported into the Raumakey schema only through the
  `FIELD_MAP` table; the table ships empty for settings because no permitted source documents EVKey
  setting names or value encodings. Every unmapped field is preserved in the migration record's
  extension data. Values that look like lists of `*.exe` names are imported as a named set
  `evkey-import-<n>` with no action (the user decides the action; matrix #28).
- `commit`: snapshot the current Raumakey documents, write new revisions through `AtomicStore`, read
  back and verify revision + checksum, then write `migration.json` with status `completed`, source
  fingerprints (SHA-256, size), counts, quarantined fields (name + reason code), and preserved raw
  values. Any failure restores the snapshot documents and records status `rolled-back`.
- Idempotent: same fingerprints and `completed` → `NoChange`. Sources are never written, renamed,
  deleted, unregistered, or re-permissioned.

---

## 10. Windows layer

### 10.1 `raumakey-windows` (all modules `cfg(windows)` except pure planners)

`error` (HRESULT/Win32 mapping), `panic` (`guard_hresult`, `guard_lresult`, `guard_bool`, quiet
panic hook that records only `file:line`), `strings`, `sid` (current user SID, SID hash, session id),
`names` (`Local\Raumakey.<sidhash>.<name>`, pipe name `\\.\pipe\Raumakey.<session>.<sidhash>.control`),
`security` (DACL granting only the current user SID), `pipe` (server with
`PIPE_REJECT_REMOTE_CLIENTS`, client-process session/user verification, client connect with timeout),
`registry` (views Default/64/32, read-only open for foreign roots, write guard that only allows
paths under `Software\Raumakey` or the Raumakey CLSID/TIP keys), `known_folder`, `file_replace`
(`ReplaceFileW` implementation of `raumakey_config::FileReplacer`), `clipboard` (implements
`raumakey_converter::ClipboardPort`), `input` (SendInput backspaces/Unicode text/replay with
`RAUMAKEY_INJECTED_TAG`), `hooks` (low-level keyboard/mouse hooks), `foreground` (pid, tid, exe,
class, elevation, password-edit detection), `keyboard` (`ToUnicodeEx` without state change, layout
name), `shell` (shortcuts, notify icon, `TaskbarCreated`), `task_scheduler` (elevated logon task),
`com` (apartment guard), `single_instance`, `tsf_registration` (§10.4), `legacy`
(`evkey-compat` named objects with fail-closed activation).

### 10.2 `raumakey-controller` (`raumakey.exe`, `asInvoker`, `uiAccess=false`)

- Subcommands: (none) run; `install [--dry-run] [--tip-x64 <path>] [--tip-x86 <path>]`;
  `uninstall [--dry-run]`; `startup <none|shortcut|elevated-task>`; `migrate [--from <dir>]
  [--dry-run]`; `convert --from <cs> --to <cs> [--in <file> --out <file>] [--clipboard]`;
  `update` (reports "updates unavailable", matrix #57); `--version`.
- Single instance (named mutex), hidden message window, one tray icon (V/E icons), `TaskbarCreated`
  recreates the icon without changing mode (matrix #58).
- UI thread does no engine, file, network, or blocking IPC work. A worker thread owns the stores,
  migration, conversion, and IPC broadcast; results return through an `mpsc` channel plus
  `PostMessageW(WM_APP_RESULT)`.
- Compact panel: Vietnamese mode, input method, tone style, expand/collapse. Expanded tabs: Input,
  Hotkeys, Macros, App rules, Conversion, System, About. All UI strings are new Raumakey strings
  (Vietnamese with diacritics). Icons are generated by `raumakey-resources`.
- IPC server (named pipe) publishes `RuntimeSnapshot`/`MacroSnapshot` revisions; receives
  `ToggleMode`, `SetMode`, `HardReset`, `ReportProbe`.
- A dedicated hook thread with its own message loop owns the low-level keyboard and mouse hooks.
  The keyboard hook always classifies hotkeys (D24); engine processing in the hook happens only for
  hosts whose resolved transport is `SyntheticKey` or `Clipboard`.
- Fallback transport: sessions keyed by `(pid, tid, foreground_epoch, document)` with an
  LRU bound; injected keys carry the sentinel and are ignored; backspace count = number of scalars in
  the deleted suffix (D20); clipboard transport snapshots the sequence number and restores only when
  Raumakey still owns the latest update.
- Startup: Startup-folder shortcut by default choice; elevated scheduled task only on explicit choice,
  removed idempotently (matrix #56).
- `install` sequence: register COM (64-bit view for the x64 TIP, 32-bit view for the x86 TIP) →
  register TSF profile and categories → verify by reading back → commit installer state
  (`HKLM\Software\Raumakey\Install`). Failure rolls back only Raumakey-owned entries. `--dry-run`
  prints the plan.

### 10.3 `raumakey-tsf` (`raumakey_tip.dll`)

- Exports exactly `DllGetClassObject`, `DllCanUnloadNow`, `DllRegisterServer`,
  `DllUnregisterServer`.
- `TextService` implements `ITfTextInputProcessorEx`, `ITfKeyEventSink`, `ITfThreadMgrEventSink`,
  `ITfTextEditSink`, `ITfThreadFocusSink`, `ITfCompartmentEventSink` (keyboard open/close).
- Per-document state repository keyed by document id; focus/document change → reset (matrix #20).
  Selection change not made by Raumakey → reset (matrix #19).
- Key path: `OnTestKeyDown` runs the engine without committing and caches the result keyed by
  `(vk, scan, generation)`; `OnKeyDown` applies it in one synchronous read-write edit session:
  shift the selection start back by `delete_before`, require the shifted count to equal it and the
  covered text to equal the engine's owned suffix, `SetText`, collapse to the end. Only then commit
  `next`. Any failure → do not commit, reset, pass the key through (matrix #40, #41).
- Replay keys are sent with `SendInput` and `RAUMAKEY_INJECTED_TAG`; events whose
  `GetMessageExtraInfo()` equals the tag are bypassed (matrix #39).
- Secure fields (input scope password) bypass all transformation (matrix #29).
- Snapshot: IPC client thread keeps the last valid `RuntimeSnapshot`/`MacroSnapshot`
  (immutable `Arc`), reconnects with bounded exponential backoff; defaults v1 until the first
  snapshot.
- **D24 — single hotkey owner.** While connected to the controller the TIP does not classify
  hotkeys: the controller's global low-level keyboard hook is the only classifier (it sees every key
  before the application), swallows `VirtualKey` hotkeys, and broadcasts `ModeChanged`/`ResetAll`.
  Only while disconnected does the TIP classify hotkeys itself (toggle flips its local mode; hard
  reset resets its sessions). A key is therefore never classified twice.
- `DllCanUnloadNow` reflects live object and lock counts.

### 10.4 TSF registration plan (D17, D21)

Pure planner (testable everywhere) + executor (Windows). Entries:
`HKLM\Software\Classes\CLSID\{TIP_CLSID}` (default value "Raumakey"),
`...\InprocServer32` (default = DLL path, `ThreadingModel = Apartment`) in the view matching the DLL
bitness; profile via `ITfInputProcessorProfileMgr::RegisterProfile` (LANGID 0x042A,
`TIP_PROFILE_GUID`, description "Raumakey", icon index 0 from the DLL); categories
`GUID_TFCAT_TIP_KEYBOARD`, `GUID_TFCAT_TIPCAP_SECUREMODE`, `GUID_TFCAT_TIPCAP_UIELEMENTENABLED`,
`GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT`, `GUID_TFCAT_TIPCAP_COMLESS`,
`GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT`, `GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT`. Verification reads every
key back and enumerates the profile. Unregistration removes only these entries.

---

## 11. `raumakey-resources`

`ResWriter` (RT_MANIFEST, RT_VERSION, RT_ICON/RT_GROUP_ICON, RT_STRING), `VersionInfo`, controller
manifest (`asInvoker`, `uiAccess="false"`, Common Controls v6, PerMonitorV2 DPI awareness,
Windows 10 supportedOS), icon renderer (original pixel glyphs: app mark, `V`, `E`) producing 16/20/24/
32/48 px 32-bit images, `emit(out_dir, artifact) -> PathBuf` for build scripts.

---

## 12. Tests, harness, fuzz, xtask

- `tests/fixtures/compat/*.toml`: machine-readable vectors for every engine-level matrix row (A1–A18,
  B21–B23, B29, D47–D54) plus golden word lists. Key tokens: single characters, `{BS}`, `{SPACE}`,
  `{SHIFT+SPACE}`, `{ENTER}`, `{TAB}`, `{NAV}`, `{MOUSE}`, `{FOCUS}`, `{CTRL+x}`, `{ALTGR+x}`,
  `{SECURE}` (marks the following keys as typed in a secure field).
- `raumakey-tests` contract suites: engine vectors, macro/engine integration, policy matrix rows,
  config concurrency (#46), migration fixtures (#44, #45), IPC frames, legacy codec rows (with
  `--features evkey-compat`), converter round trips.
- Fuzz: `fuzz/` (cargo-fuzz targets: engine sequences, malformed UTF-16, config, macro files, IPC
  frames, legacy sequence wrap, transaction application). The same target bodies live in
  `raumakey_tests::fuzz_targets` and run deterministically in `tests/fuzz_smoke.rs` (seeded PRNG) so
  the smoke suite runs under `cargo test` on every host.
- `raumakey-test-runner.exe`: `vectors [--json <out>]` runs fixtures in-process and exports JSON
  (test id, OS/build, architecture, application/version, config revision, transaction sequence,
  final text, pass/fail); `live --app notepad` (user-run) drives Notepad through the installed TIP and
  reads back the window text. It never captures unrelated input.
- `tests/windows/Invoke-RaumakeyAcceptance.ps1` (user-run, elevated): install, both registry views,
  COM registration, profile installation, startup modes, upgrade, rollback, uninstall; writes JSON
  evidence. `tests/windows/MANUAL_ACCEPTANCE.md`: Notepad, Office, Edge/Chrome, Windows Terminal,
  password fields, Explorer restart, multiple documents, layout switching, application policy.
- `xtask`: `ci` (fmt, clippy `-D warnings` incl. `--features evkey-compat`, tests), `cross
  [--release]` (three MSVC targets; native linker config on Windows, `cargo xwin` elsewhere),
  `inspect` (PE machine, exports, manifest, resources, COM identity strings, artifact inventory),
  `package` (x64 package = x64 controller + x64 and x86 TIPs; ARM64 package separate; install/
  uninstall scripts; WiX source with `INSTALLER_UPGRADE_CODE`, MSI when `wix` is available;
  `SHA256SUMS`; `manifest.json` marking packages unsigned), `audit` (cargo-deny), `notices`
  (dependency license list), `release-gate` (all gates in order, evidence JSON under
  `target/release-evidence/`).

### Release-gate evidence contract (2026-10-17 follow-up)

`ci` and `release-gate` fail if the `raumakey-tests/fuzz_smoke` test target is missing.
`ci --no-fuzz` is an explicit partial local check; it does not establish release readiness.
CI enables `evkey-compat` on IPC, Windows, controller, TIP, and shared tests. Cargo commands
that resolve dependencies (`clippy`, `test`, `build`, and `metadata`) pass `--locked`.

The release steps are, in order: `ci`, `audit`, `cross`, `inspect`, `whole-branch-review`,
`package`, `installer-dry-run`. Evidence can say `cross-build verified` only when this complete
ordered list passes, with no missing or duplicate steps. It never claims runtime verification.

An independent reviewer supplies `<target-dir>/release-evidence/whole-branch-review.json`:

```json
{
  "schema_version": 1,
  "scope": "whole-branch",
  "source_revision": "<full Git commit hash reviewed>",
  "reviewer": "<independent reviewer identity>",
  "approved": true,
  "unresolved_findings": 0
}
```

This is a schema example, not review evidence. All fields are required. The review must match
the current clean Git revision; unknown fields, invalid JSON, unsupported versions, a blank
reviewer, rejection, or unresolved findings fail the gate. The successful step records the
SHA-256 of the supplied bytes. Keep the record with the gate evidence. This validates an
attestation; it cannot establish the reviewer's independence or replace an actual review.
Standalone `package` remains an assembly command and does not establish release readiness.

Matrix rows not executable in this repository: #43 (needs EVKey binaries; recorded as not
applicable), runtime rows that need a live Windows desktop (#19, #20, #24, #39, #40, #58 in their
runtime form) are covered by fakes in unit tests and by the manual acceptance checklist.

---

## 13. Task order

| Stage | Tasks |
|---|---|
| 1 | Toolchain, workspace skeleton, `LICENSE`, `THIRD_PARTY.md`, `deny.toml`, `raumakey-model` |
| 2 | `raumakey-engine` (+ golden vectors), `raumakey-macro` |
| 3 | `raumakey-config`, `raumakey-converter`, `raumakey-migrate` |
| 4 | `raumakey-policy`, `raumakey-ipc` (+ `evkey-compat`) |
| 5 | `raumakey-resources`, `raumakey-windows`, `raumakey-controller` |
| 6 | `raumakey-tsf`, TSF registration |
| 7 | `xtask`, `tests/` harness + fixtures + fuzz, PowerShell suite, documentation |
| 8 | Hardening, audit, whole-branch review, release-gate evidence |

Stages may run in parallel when their dependencies are complete. Each task ends with fmt, clippy,
the crate's tests, and an independent review against this plan and the design.

---

## 14. Implementation status (2026-10-17)

This section tracks the tasks of §13. It describes the Git revision that contains it; work that
exists only outside that revision is listed in §14.3. Design §20 holds the detailed evidence of
its earlier snapshot (gate results, compatibility-matrix coverage, risks, and open questions);
§14.6 lists the gates rerun for this revision. "Done" means that the contracts of this plan have
code and tests, apart from the gaps listed in §14.2 and §14.4. It never means that runtime
behavior was verified in a Windows host application (§0 rule 6).

### 14.1 Summary

- **Done:** the core libraries `raumakey-model`, `-engine`, `-macro`, `-config`, `-converter`,
  `-policy`, and `-ipc` (with the `evkey-compat` codec), and `raumakey-resources`, apart from the
  gaps in §14.2 and the defects in §14.4. `raumakey-migrate`, `raumakey-windows`, and `xtask` are
  mostly done. `xtask` includes the release-gate evidence contract of §12. The TIP has its
  platform-independent session core (`raumakey_tip::session`), tested with fakes.
- **Not done:** the controller (`raumakey.exe`) is an empty stub and the TIP
  (`raumakey_tip.dll`) has no COM layer, so Raumakey cannot be installed or used.
  `raumakey-tests` is a stub, and `fuzz/`, `tests/fixtures/`, and `tests/windows/` do not exist.
- **Gates:** `cargo xtask ci` fails by design until the `fuzz_smoke` target exists (§12);
  `ci --no-fuzz` passes (§14.6). Neither "cross-build verified" nor "runtime x86_64 verified"
  can be claimed (design §19).

### 14.2 By stage

| Stage | Status | Done | Not done |
|---|---|---|---|
| 1 | Done | Workspace, toolchain pin (Rust 1.99.1, three MSVC targets), `LICENSE`, `THIRD_PARTY.md`, `deny.toml`, `raumakey-model` | `THIRD_PARTY.md` corrections (design §20.6 item 9); §1.1 and §1.3 do not match the manifests |
| 2 | Done (library) | `raumakey-engine` with every §3.10 golden vector; `raumakey-macro` | No test runs the engine with the real `MacroTable`; the Circumflex fallback with spell checking off has no test (design §20.8 item 9); the macro defects in §14.4 items 1, 2, and 4 |
| 3 | Mostly done (library) | `raumakey-config`, `raumakey-converter`, `raumakey-migrate` (sources, parsing, commit with rollback, idempotence) | Commit durability (design §20.7); a Windows `LegacyRegistry` adapter; default EVKey folder discovery; the `migrate` and `convert` commands |
| 4 | Done (library) | `raumakey-policy`; `raumakey-ipc` with frames, messages, client and server sessions, backoff, and the `evkey-compat` codec | The controller's server hub and snapshot builder; the policy and the engine disagree on `Restricted` secure fields (design §20.8 item 1) |
| 5 | Building blocks only | `raumakey-resources`; `raumakey-windows` wrappers for errors, panic guards, strings, handles, SIDs, object names, security, pipes, registry, known folders, file replacement, clipboard, input (including `GetMessageExtraInfo`), hooks, foreground, keyboard, processes, shell, task scheduler, COM apartments, single instance, legacy objects, hidden and message-only windows with the message loop and `PostMessageW`, popup menus, icons, modules, COM server support (class factory, object and lock counts), and message boxes | No `build.rs` calls `raumakey-resources`; dialogs and controls, and the building blocks of §14.5 item 3; `raumakey-controller` is `fn main() {}` |
| 6 | Session core and registration code | TSF registration planner, executor, and verification in `raumakey-windows`, tested with fakes; the TIP's platform-independent session core (`raumakey_tip::session`: document port, key path, per-document state, mouse-press resets, probe bookkeeping, D24 while disconnected, snapshots), tested with fakes | The COM layer (§14.5 item 4); verification does not enumerate the profile (§10.4); D17 and D21 have no decision text |
| 7 | Partial | `xtask` commands `ci`, `cross`, `inspect`, `package`, `audit`, `notices`, and `release-gate` | `raumakey-tests` (fixtures, contract suites, fuzz targets, `fuzz_smoke`, runner); `fuzz/`; PowerShell suite; manual checklist; the `xtask` items of design §20.6 item 7 that the follow-up left open (§14.5 item 7); documentation (§14.5 item 8) |
| 8 | Not started | `xtask audit` passed in the design §20 snapshot (design §20.2; not rerun) | Hardening (design §20.7), whole-branch review, release-gate evidence |

Design §20.6 item 8 is half done: the implementation is in Git (`24ba7a6` and later revisions).
The other half is open for the code before the follow-up: §0 rule 1 and §13 require each task to
record its failing test run and an independent review, and that code has neither. The
2026-10-17 follow-up (`2026-10-17-review-followup.md`) records TDD evidence for its `xtask`
increment only; it was a targeted review, not an independent whole-branch review, and the
independent review of its own increment is still pending. `2026-10-17-phase1-task-notes.md`
records the failing test runs and the independent reviews of the work merged since.

### 14.3 Started, not in this revision

On 2026-10-17 the following work was started in a local Git worktree on the development host
(`target/wt/tests`, detached at `24ba7a6`; uncommitted and not pushed). It has code and tests but
is still under review and still changing, and none of it is part of this revision. The list
describes it as it was when this revision was written:

- `raumakey-tests`: `fuzz_targets` with the seven targets of §12, `tests/fuzz_smoke.rs` with a
  seeded PRNG, a macro/engine suite that runs the engine with the real `MacroTable`,
  compatibility vectors in `tests/fixtures/compat/`, the runner's `vectors` mode (`live` is not
  implemented), and a `build.rs` that gives the runner its resources.

Once this work is merged, `xtask ci` can run its fuzz step, and this section must be updated.

### 14.4 Defects found after design §20

The 2026-10-17 Phase 1 work found these defects; item 4 shows that the "Silent fallback" risk
of design §20.7 is reachable with valid input. Each was confirmed against the code of
`4507b1b`. Item 3 is fixed; items 1, 2, and 4 wait for the decisions of §14.5 step 2.

1. `MacroTable` accepts triggers that contain boundary punctuation, such as `v.v`. When no
   input-method action consumes the punctuation key, the engine commits the word at it (§3.8),
   so the macro never expands (matrix #47): `v.v` never expands in Telex, VNI, or VIQR. VIQR
   binds `'`, `?`, `.`, `(`, and `\` as actions, and a user key map can bind any punctuation, so
   a VIQR trigger such as `a.` still matches through its raw keys. Decide whether validation
   rejects such triggers or matching continues across punctuation.
2. A trigger may have up to 64 scalars (§4), but a word that takes more keys than
   `context_limit_codepoints` (default 32, range 8..=128) overflows (§3.9) and never expands.
   With the defaults no trigger of 33–64 scalars expands. Marks and tones can add up to four
   keys (Telex `dduwowngf` types the five-scalar `đường` in nine keys), so some triggers of
   29–32 scalars fail too, and a lower limit fails shorter triggers.
3. Fixed on 2026-10-17. `RuntimeSnapshot::validate` accepted `UserDefined` with a
   `user_keymap_id` that was not among the snapshot's key maps, and the engine then returned
   `UnsupportedInputMethod` for every character. `validate` now rejects such a snapshot with
   `InvalidKeymap` (test `a_user_defined_method_needs_its_key_map_in_the_snapshot`).
4. The 4,096-scalar replacement limit (§4) does not keep a macro's insert within the 8,192-unit
   insert limit (§2.7). The insert is the replacement in the configured normalization plus a
   printable delimiter, so 4,096 astral scalars, or more than 2,730 letters such as `ệ` with NFD
   (three units each), exceed it. The engine then commits the word as rendered, with no
   expansion, no auto-restore, and no signal (design §20.7, "Silent fallback").

### 14.5 Remaining work, in order

This order replaces design §20.10. Its step 1 is done: `24ba7a6` committed the tree, and this
revision commits the `xtask` fixes of the 2026-10-17 follow-up. Steps 7 and 8 do not depend on
steps 3–6 and may run in parallel with them (§13). Every step records its failing test runs and
its independent review (§0 rule 1, §13).

1. Review and merge the work in §14.3.
2. Decide design §20.8 items 1–9 and §14.4 items 1, 2, and 4, record the answers as decisions
   (item 5 is answered by writing D17 and D21), and align the code and tests with them.
3. Add the `build.rs` files and the remaining `raumakey-windows` building blocks: dialogs and
   controls, TSF sink and edit-session helpers, input-scope detection, foreground-change
   tracking, the `LegacyRegistry` adapter and default EVKey folders, profile enumeration, x64 +
   x86 install orchestration, a `ChangeWindowMessageFilterEx` wrapper so that an elevated
   controller receives `TaskbarCreated` (matrix #58), a DPI query, and a message loop with
   `IsDialogMessageW` for the modeless panel.
4. Implement the TIP COM layer (§10.3) on the session core: the four exports, the class factory
   on the `raumakey-windows` COM server support, `TextService` with its activation lifecycle and
   sinks, edit sessions, replay through `SendInput`, the snapshot client thread, reporting mouse
   presses to the core, the range-edit probe edit and its `ReportProbe` (sent again after a
   reconnect while the session core still holds the result), opt-in `Diagnostic` events (only
   while `RuntimeSnapshot.diagnostics_enabled` is set), and the `evkey-compat`
   path.
5. Implement the controller (§10.2). First the Windows GUI subsystem (attaching to the console
   for subcommands) and the CLI with `install`/`uninstall --dry-run`, accepting options in any
   order, because the packaged `install.ps1` and the release gate order them differently. With
   steps 3 and 4 this lets `xtask inspect` and the installer dry run pass. Then the other
   subcommands (`startup`, `migrate`, `convert`, `update`, `--version`); the configuration
   lifecycle (D9 data root, stores, validation of the `[conversion]` charset names); the IPC hub
   and snapshot builder, calling `verify_peer_same_user_and_session` on every connection; the
   tray, hook thread, fallback transport, and panel; opt-in diagnostics; and the `evkey-compat`
   legacy server.
6. Finish `raumakey-tests` and the runner (`vectors`, `live`), the `fuzz/` workspace,
   `tests/windows/Invoke-RaumakeyAcceptance.ps1`, and `MANUAL_ACCEPTANCE.md`. Run the manual
   `#[ignore]`d `raumakey-windows` tests (10 in this revision) and keep their
   results as evidence.
7. Finish `xtask`: ship notices and license texts, check the manifest's Common Controls v6,
   PerMonitorV2, and supportedOS entries in `inspect`, run the i686 tests under WOW64, compile
   the WiX source in CI, and stop `package` from shipping artifacts that fail `inspect`.
8. Documentation: correct `THIRD_PARTY.md`; add a decision index; record the code-level decisions
   of design §20.9; align §1.1, §1.3, and §7 with the code.
9. Fix the risks of design §20.7 that break a design requirement, starting with commit
   durability. Then run a whole-branch review, the release gate, and Windows acceptance.

### 14.6 Evidence for this revision

Run on 2026-10-17 on the Windows 11 x86_64 development host (Rust 1.99.1, `rust-lld` + `xwin`):

| Command | Result |
|---|---|
| `cargo fmt --all --check` | Pass |
| `cargo xtask ci --no-fuzz`: Clippy with `-D warnings`, default features and all five `evkey-compat` features | Pass |
| `cargo xtask ci --no-fuzz`: `cargo test --locked --workspace` | 1,442 passed, 0 failed, 10 ignored |
| The same tests with all five `evkey-compat` features | 1,515 passed, 0 failed, 10 ignored |
| `cargo test --locked -p raumakey-windows -p raumakey-tsf --target i686-pc-windows-msvc` (WOW64) | 346 passed, 0 failed, 10 ignored |
| `cargo xtask cross` (debug: x86_64, i686, aarch64) | Pass |

The 10 ignored tests are the manual `raumakey-windows` tests (design §15.1); menu tracking and
message boxes added two. `cargo xtask ci` without `--no-fuzz` was not rerun: the follow-up
observed its intended failure at the missing `fuzz_smoke` target on the same `xtask` code
(`2026-10-17-review-followup.md`, step 7), and the unit test
`missing_fuzz_suite_fails_before_starting_ci_commands` passed in this run. PE inspection,
packaging, and the audit were not rerun for this revision; design §20.2 has their last results.
