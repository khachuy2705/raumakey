# Kế hoạch triển khai Raumakey

**Ngày:** 2026-10-16 (cập nhật lần cuối 2026-10-18: hợp đồng bằng chứng cho cổng phát hành ở §12,
trạng thái triển khai ở §14, các quyết định ở §15, bản portable một tệp ở D38–D42 và §16, bảng
cấu hình và danh sách gõ tắt ở D43–D45 và §17)
**Trạng thái:** Đang áp dụng; việc triển khai đang được tiến hành (xem §14, §16 và §17). D38–D42
thay thế các phần nói về TIP, installer và thư mục dữ liệu (thiết kế §21); D43–D45 thêm bảng cấu
hình (thiết kế §22).
**Đầu vào:** `docs/superpowers/specs/2026-10-16-raumakey-design.md` (thiết kế, có thẩm quyền),
`docs/reference/evkey_clean_room_spec.md` (tài liệu tham chiếu hành vi clean-room),
`docs/reference/evkey_compatibility_test_matrix.md` (các vector tương thích).

Kế hoạch này biến thiết kế đã được duyệt thành các hợp đồng của crate, thuật toán, định dạng tệp
và một danh sách task có thứ tự. Ở những chỗ thiết kế hoặc tài liệu tham chiếu clean-room còn để
ngỏ một hành vi, kế hoạch này ghi lại một **quyết định Raumakey (Dn)**. Các quyết định là lựa chọn
của sản phẩm, không phải khẳng định về hành vi của EVKey.
Không có nội dung nào trong kế hoạch này được rút ra từ `restricted_evidence/`.

---

## 0. Quy tắc nền cho mọi task

1. **TDD.** Viết test, chạy nó, quan sát thấy nó fail đúng vì lý do dự kiến, rồi viết lượng mã
   production tối thiểu, rồi refactor. Ghi lại lần chạy fail vào ghi chép task (lệnh + tên test
   fail).
2. `raumakey-model` và `raumakey-engine` không chứa Win32, COM, registry, HWND, handle, con trỏ thô,
   clipboard hay trạng thái toàn cục cấp tiến trình *có thể thay đổi*. Được phép dùng các bảng bất
   biến được khởi tạo lười (`OnceLock`).
3. Không panic nào được phép vượt qua ranh giới COM, callback Win32 hay FFI. Mọi hàm được export,
   phương thức COM, window procedure, hook procedure và điểm vào của thread đều được bọc trong
   `catch_unwind` (các hàm trợ giúp nằm trong `raumakey-windows::panic`).
4. Không bao giờ ghi log phím gõ, văn bản tài liệu, nội dung clipboard, trigger hay văn bản thay thế
   của macro, hoặc đường dẫn đầy đủ có chứa nội dung của người dùng. Việc ghi log chỉ đi qua API có
   kiểu `DiagEvent`, vốn không có trường chuỗi dạng tự do nào (xem §2.10).
5. `cargo fmt --all --check` và `cargo clippy --workspace --all-targets -- -D warnings` phải đạt.
6. Một lần build thành công không phải là bằng chứng về hành vi runtime trên Windows. Mọi khẳng định
   về runtime đều cần đến harness nghiệm thu trên Windows (§12).
7. Không thêm phụ thuộc (dependency) nào không được liệt kê ở §1.3 nếu chưa ghi nó vào
   `THIRD_PARTY.md` và `deny.toml`.

---

## 1. Workspace và toolchain

### 1.1 Bố cục

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

Hướng phụ thuộc (mũi tên chỉ về phía crate được phụ thuộc):

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

`raumakey-windows` có thể phụ thuộc vào các crate lõi (các port trong `model`, `config`,
`converter`). Các crate lõi không bao giờ phụ thuộc vào `raumakey-windows`, `windows` hay
`windows-sys`.

### 1.2 Toolchain (D1)

- Rust stable được ghim trong `rust-toolchain.toml`, với các component `rustfmt`, `clippy` và các
  target `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, `aarch64-pc-windows-msvc`.
- **Host Windows không có Visual Studio:** link bằng `rust-lld` cùng các thư viện import của MSVC
  CRT + Windows SDK do `xwin splat` tạo ra. Tệp `CARGO_HOME/config.toml` cục bộ trên máy đặt
  `linker = "rust-lld"` và các đường dẫn thư viện `-Lnative=` cho từng target. Thư mục `.cargo/`
  trong kho mã được Git bỏ qua vì nó chứa các đường dẫn riêng của máy.
- **Host Linux:** `cargo xwin build --target <triple>` ("công cụ cross-build được chọn" trong thiết
  kế §15.2).
- Tài nguyên (manifest, thông tin phiên bản, biểu tượng) được `raumakey-resources` tạo ra từ
  `build.rs` dưới dạng một tệp `.res` truyền cho linker bằng `cargo:rustc-link-arg-*`; không cần
  `rc.exe`.
- Binary test x86_64 chạy native; binary test i686 cũng chạy được trên Windows x64 (WOW64); ARM64
  chỉ kiểm tra biên dịch (compile-gated).
- **D23 — CRT tĩnh cho các artifact được phát hành.** `xtask cross`/`package` build các artifact
  release với `-C target-feature=+crt-static` (nối thêm qua `--config`, nhờ vậy các cờ linker cục
  bộ của máy vẫn được giữ). TIP được nạp vào các tiến trình bất kỳ nên không được phụ thuộc vào gói
  Visual C++ redistributable.

### 1.3 Các crate bên thứ ba được duyệt

| Crate | Công dụng | Dùng trong crate |
|---|---|---|
| `serde` (+derive), `serde_json`, `toml` | tài liệu, payload IPC | model, config, macro, policy, ipc, migrate, tests, xtask |
| `unicode-normalization` | NFC/NFD | engine, macro, converter, model (chỉ để kiểm tra hợp lệ) |
| `sha2` | checksum, dấu vân tay (fingerprint) | config, migrate, xtask |
| `crc32fast` | checksum của frame IPC | ipc |
| `windows` | binding Win32/COM/TSF | windows, controller, tsf (chỉ khi cfg(windows)) |
| `proptest` | test thuộc tính (dev) | mọi crate lõi |
| `tempfile` | thư mục tạm trong test (dev) | config, migrate, converter, tests |
| `object` | kiểm tra tệp PE | xtask |

Các crate `raumakey-*` có `publish = false`, `license = "GPL-3.0-or-later"` (D2).

---

## 2. Hợp đồng của `raumakey-model`

Mọi kiểu đều derive `Clone, Debug, PartialEq, Eq` ở những chỗ có ý nghĩa; các kiểu tài liệu còn
derive thêm `serde::{Serialize, Deserialize}`. Constructor kiểm tra các bất biến; các trường mang
bất biến là private và có accessor.

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

Model không bao giờ nêu tên các định danh của EVKey. Các hằng số legacy chỉ nằm trong
`raumakey-ipc::legacy`, phía sau feature `evkey-compat`.

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

`InputEvent::validate()` từ chối: văn bản trên các kind khác `Character`/`Separator`; UTF-16 sai
dạng; nhiều hơn `MAX_EVENT_TEXT_UNITS = 16` đơn vị. `is_shortcut()` là
`(ctrl || alt || win) && !alt_gr`. Các constructor: `character(char, host)`,
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

**D4 — giá trị mặc định có phiên bản v1** (`EngineOptions::defaults_v1()`,
`DEFAULTS_VERSION = 1`): enabled `true`, Telex, NFC, `modern_tone = false` (òa/úy),
`free_marking = true`, `spell_check = true`, `auto_restore_invalid = true`,
`macros_enabled = true`, `macros_when_disabled = false`, `auto_case_macro = true`, không có bảng
phím người dùng, `context_limit_codepoints = 32` (khoảng hợp lệ 8..=128).

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

Dạng TOML/chuỗi của các action: `tone:acute|grave|hook|tilde|dot`, `remove-tone`, `circumflex`,
`circumflex:a|e|o`, `breve`, `horn`, `horn-or-breve`, `stroke`, `escape`.
Kiểm tra hợp lệ: id `[A-Za-z0-9._-]{1,64}`, name ≤ 128 ký tự, các phím là ký tự ASCII in được
`0x21..=0x7E`, không trùng nhau (chữ cái ASCII được so sánh không phân biệt hoa thường), 1..=4
action cho mỗi phím, version = phiên bản schema.

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
- `validate(available_units, max_delete)`: phần chèn phải đúng dạng và ≤ `MAX_INSERT_UNITS`;
  `delete_before ≤ available` và `≤ max_delete`; `replay_key` chỉ đi kèm `consume_original`.
- `TextSimulator::apply(&event, &tx)`: từ chối thao tác xoá vượt quá văn bản hiện có hoặc bắt đầu ở
  giữa một cặp surrogate; cắt bớt văn bản, nối thêm `insert_utf16`; nếu `!consume_original` thì áp
  dụng hiệu ứng tự nhiên của sự kiện (văn bản của Character/Separator được nối thêm, `Backspace`
  xoá scalar cuối cùng, các kind khác không làm gì); nếu `replay_key` được đặt thì nối thêm văn
  bản của phím được phát lại (`RETURN` → `"\r"`, `TAB` → `"\t"`, `SPACE` → `" "`). Đây là oracle
  mà các bộ test tương thích sử dụng ("mỗi bước áp dụng transaction lên văn bản/caret hiện tại").

### 2.8 `policy` (chỉ gồm các kiểu; phần phân giải nằm trong `raumakey-policy`)

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

### 2.9 `macros` (các kiểu và port tra cứu)

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
    fn has_token_triggers(&self) -> bool { false }                              // D28
    fn is_trigger_prefix(&self, prefix: &str, case_mode: CaseMode) -> bool { false } // D28
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

**D5 — phím tắt mặc định:** chuyển chế độ bằng `Ctrl+Shift` (chỉ nhấn-nhả các phím bổ trợ); đặt
lại cứng (hard reset) bằng `Ctrl+Shift+Alt+F12` và `Ctrl+Shift+Alt+Space` (ma trận #25).

`DiagEvent::to_log_line()` tạo ra `"<ts> <component> <code> rev=<n|-> exe=<name|-> vec=<id|->"`.
Không có API nào nhận văn bản tuỳ ý để ghi log.

---

## 3. `raumakey-engine`

### 3.1 API công khai

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
    pub fn owned_utf16(&self) -> &[u16];       // text before the caret the engine may replace:
                                               // the rendered current word, preceded by the
                                               // committed part of its token while one is
                                               // tracked (D28)
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

`process` là một hàm thuần của `(self, event, options, prior)`. Engine không bao giờ thay đổi
`prior`. Adapter chỉ commit `next` sau khi đã áp dụng `transaction` thành công.

### 3.2 Bảng phím có sẵn

| Kiểu gõ | Gán phím |
|---|---|
| Telex | `s`→Tone(Acute) `f`→Tone(Grave) `r`→Tone(Hook) `x`→Tone(Tilde) `j`→Tone(Dot) `z`→RemoveTone `a`→CircumflexOn(A) `e`→CircumflexOn(E) `o`→CircumflexOn(O) `w`→HornOrBreve `d`→Stroke |
| VNI | `1` Acute `2` Grave `3` Hook `4` Tilde `5` Dot `0` RemoveTone `6` Circumflex `7` Horn `8` Breve `9` Stroke |
| VIQR | `'` Acute `` ` `` Grave `?` Hook `~` Tilde `.` Dot `-` RemoveTone `^` Circumflex `(` Breve `+` Horn `*` Horn `d` Stroke `\` Escape |

Phím chữ cái ASCII khớp không phân biệt hoa thường; chữ cái *nguyên văn* giữ hoa/thường như khi gõ.

### 3.3 Mô hình từ

Một từ là một dãy phần tử (item) cộng với một dấu thanh ở cấp độ từ:

```text
Item   = Letter { base: 'a'..='z', upper: bool, mark: None|Circumflex|Breve|Horn|Stroke,
                  from_w: bool /* standalone ư produced by Telex w */ }
       | Other(char)               // digit, symbol, backslash, non-ASCII: makes the word non-Vietnamese
Word   = { items, tone: Option<(Tone, item_index)>, literal_mode: bool, transformed: bool }
```

Các nguyên âm tiếng Việt (theo `base`+`mark`): a ă â e ê i o ô ơ u ư y. Các chữ cái `f j w z`
(không có dấu phụ) không phải là chữ cái tiếng Việt.

### 3.4 Ngữ pháp âm tiết (kiểm tra chính tả và đặt dấu thanh)

Các chữ cái được so sánh ở dạng chữ thường, có dấu phụ và không có dấu thanh.

**Âm đầu:** `"" b c ch d đ g gh gi h k kh l m n ng ngh nh p ph qu r s t th tr v x`.
Ràng buộc chính tả: `k`, `gh`, `ngh` chỉ đứng trước `e ê i y`; `c`, `ng` không bao giờ đứng trước
`e ê i y`; `g` trước `i` là âm đầu gộp `gi` (`gì`, `gìn`, `giếng`); `q` bắt buộc phải có `u` theo
sau và `qu` bắt buộc phải có một nguyên âm theo sau.

**Vần** (`cluster: allowed codas`; `open` nghĩa là được phép không có âm cuối):

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

Sau `qu` (`u` thuộc về âm đầu), các cụm nguyên âm được phép là:

```text
a: open c ch m n ng nh t   ă: c m n ng t   â: n ng t   e: open n t   ê: open ch n nh
i: open ch n nh t          y: open ch n nh t           yê: n t       ơ: open   ô: c
ai ay ao ây eo: open
```

Sau `gi` (âm đầu gộp, `i` thuộc về âm đầu), mọi cụm nguyên âm của bảng chính không bắt đầu bằng
`i` hoặc `y` đều được phép; `g` + một cụm nguyên âm bắt đầu bằng `i` cũng hợp lệ.

`SYLLABLES` = mọi tổ hợp âm đầu × (cụm nguyên âm + âm cuối) được phép, tuân theo các ràng buộc
ở trên, lưu dưới dạng một danh sách đã sắp xếp.

- `classify(letters) -> Invalid | Prefix | Complete`: `Complete` khi và chỉ khi
  `letters ∈ SYLLABLES`; `Prefix` khi và chỉ khi có âm tiết nào đó bắt đầu bằng `letters` (từ rỗng
  và các tiền tố âm đầu như `n`, `ng`, `ngh`, `q`, `qu` đều là tiền tố).
- **Luật thanh/âm cuối** (chỉ từ hoàn chỉnh): âm cuối `c ch p t` chỉ nhận Acute, Dot hay không dấu.
- `split(letters) -> (onset, cluster, coda)`: khớp âm đầu dài nhất, nhận `qu` khi sau nó là nguyên
  âm, `gi` chỉ khi sau `i` còn một nguyên âm khác; cụm nguyên âm = dãy nguyên âm dài nhất sau âm
  đầu; âm cuối = phần còn lại.

### 3.5 Đặt dấu thanh

Dựa trên kết quả tách của từ hiện tại:

1. Nếu cụm nguyên âm chứa một nguyên âm có dấu phụ (â ê ô ă ơ ư), dấu thanh đặt trên nguyên âm có
   dấu phụ **cuối cùng** (`ươ`→ơ, `ưu`→ư, `iê`/`yê`/`uyê`→ê, `uô`→ô, `uâ`→â).
2. Ngược lại, nếu có âm cuối, dấu thanh đặt trên nguyên âm cuối cùng của cụm (`toán`, `hoàng`).
3. Ngược lại, theo độ dài cụm: 1 → nguyên âm đó; 3 → nguyên âm giữa (`oai`, `uya`, `uyu`, `oeo`);
   2 → nếu `modern_tone` bật và cụm là `oa`, `oe` hoặc `uy` thì đặt trên nguyên âm thứ hai
   (`hoà`, `khoẻ`, `thuỷ`), nếu không thì trên nguyên âm thứ nhất (`hòa`, `của`, `mía`, `mái`).
4. Từ dài hơn hoặc không phân tích được giữ vị trí tính được gần nhất (`tone_position` bị cố định
   khi từ không còn là một tiền tố hợp lệ).

### 3.6 Áp dụng hành động

Xử lý một `Character` là một scalar ASCII đơn trong chế độ tiếng Việt:

1. Nếu từ đang ở `literal_mode` (sau một lần undo hoặc tràn), nối thêm phím ở dạng nguyên văn.
2. Nếu `spell_check` bật và các chữ cái hiện tại không là tiền tố hợp lệ (`Invalid`), nối thêm
   nguyên văn (không biến đổi các từ không phải tiếng Việt).
3. Tra hành động ứng viên của phím; với mỗi hành động, `try_apply` trả về `Applied`, `Undo` hoặc
   `NotApplicable`. Kết quả đầu tiên khác `NotApplicable` được chọn; nếu không, nối phím nguyên văn.
4. Khi `spell_check` bật, ứng viên `Applied` chỉ được chấp nhận nếu các chữ cái thu được là tiền tố
   hợp lệ; nếu không, nó được coi là `NotApplicable`.
5. Khi `free_marking` tắt, hành động dấu thanh và dấu phụ chỉ áp dụng khi từ còn kết thúc trong cụm
   nguyên âm (chưa gõ âm cuối), và `Stroke` chỉ áp dụng khi từ đúng bằng âm đầu `d`.

Quy tắc của `try_apply`:

- **Tone(t):** cần một nguyên âm trong cụm. Đã có đúng dấu thanh đó → `Undo`. Dấu thanh khác hoặc
  chưa có dấu thanh → đặt dấu thanh (thay thế).
- **RemoveTone:** có dấu thanh → xoá (`Applied`, không nối nguyên văn); ngược lại `NotApplicable`.
- **CircumflexOn(v):** đích = nguyên âm cuối cùng trong cụm có base `v`. Đã có dấu mũ → `Undo`;
  dấu trăng/không dấu → đặt dấu mũ.
- **Circumflex:** ứng viên = các nguyên âm trong cụm có base a/e/o, từ cuối về đầu; một nguyên âm
  trong cụm đã có dấu mũ → `Undo`; ngược lại chọn ứng viên đầu tiên, tính từ phải, có kết quả là
  tiền tố hợp lệ, nếu không có thì chọn ứng viên ngoài cùng bên phải (quy tắc 4 loại ứng viên này
  khi bật kiểm tra chính tả). Vì vậy, khi tắt kiểm tra chính tả, Circumflex áp dụng cho `a`, `e`
  hoặc `o` ngoài cùng bên phải của cụm (D34).
- **Breve:** đích = `a` cuối cùng trong cụm; đã có dấu trăng → `Undo`; ngược lại đặt dấu trăng.
- **Horn:** nếu cụm chứa `u` đứng ngay trước `o` (không nằm trong âm đầu `qu`): nếu cả hai đã có
  dấu móc (hoặc `o` có dấu móc và sau nó không có gì) → `Undo`; ngược lại đặt dấu móc cho cặp: cả
  `u` và `o` khi có gì đó theo sau `o`, chỉ `o` (`uơ`: `thuở`, `huơ`) khi `o` kết thúc từ. Nếu
  không, ứng viên là các nguyên âm `u`/`o` đơn lẻ trong cụm (kết quả hợp lệ đầu tiên được chọn;
  `Undo` nếu nguyên âm được chọn đã có dấu móc).
- **HornOrBreve (Telex w):** các ứng viên Horn, rồi đến Breve; ứng viên hợp lệ đầu tiên được chọn.
  Khi từ không có nguyên âm, nối thêm một `ư` đứng riêng (`from_w = true`, hoa/thường theo phím).
  Undo một `ư` đứng riêng sẽ xoá nó.
- **Stroke:** đích = âm đầu `d` (phần tử đầu tiên). Đã là `đ` → `Undo`; ngược lại đặt nét gạch.
- **Escape (VIQR `\`):** được xử lý trước các hành động: dấu gạch chéo ngược được nối thêm dưới dạng
  `Other('\\')` và bật `escape_armed`. Khi cờ này bật và phím tiếp theo có bất kỳ gán hành động
  VIQR nào, phần tử gạch chéo ngược đó được thay bằng chính phím ấy dưới dạng `Other` nguyên văn
  (`\'` cho ra `'`, ma trận #8); mọi phím khác sẽ tắt cờ và được xử lý bình thường.

**Undo:** huỷ phép biến đổi (xoá dấu thanh / dấu phụ / `ư` đứng riêng), nối thêm phím hiện tại
dưới dạng nguyên văn, đặt `literal_mode` cho phần còn lại của từ. `a s s` → `as`, `a a a` → `aa`,
`w w` → `w`, `d d d` → `dd`.

**Chỉnh `uơ`:** khi nối thêm một chữ cái sau một cụm kết thúc bằng `u` không dấu + `o` có dấu móc
(không nằm trong âm đầu `qu`) và chữ cái mới là một phụ âm cuối hoặc bán âm `i`/`u`, thì `u`
cũng nhận dấu móc (`huow` → `huơ`; `huow`+`ng` → `hương`; `nguo7i` → `ngươi`).

### 3.7 Kết xuất và diff

- Mỗi chữ cái được kết xuất thành dạng chữ thường dựng sẵn, với dấu thanh đặt tại chỉ số dấu thanh,
  rồi chuyển thành chữ hoa khi `upper`. `Other(c)` được kết xuất thành `c`.
- `Nfc` và `Preserve` xuất văn bản dựng sẵn; `Nfd` xuất dạng phân tách chuẩn tắc
  (`unicode-normalization`).
- `diff(old, new)`: tiền tố chung tính theo đơn vị UTF-16, được lùi lại để không bao giờ tách đôi
  cặp surrogate hay tách ký tự cơ sở khỏi dấu kết hợp theo sau nó; `delete_before = old.len() -
  prefix`, `insert = new[prefix..]`.
- Nếu diff đúng bằng "nối thêm văn bản của chính sự kiện", transaction là pass-through (cho đi qua)
  (`consume_original = false`, phần chèn rỗng); ngược lại `consume_original = true`.

### 3.8 Xử lý sự kiện

| Sự kiện | Hành vi |
|---|---|
| `injected_tag == RAUMAKEY_INJECTED_TAG` | cho đi qua, trạng thái không đổi |
| `host.is_secure_field` | được xử lý như mọi sự kiện khác, ngoại trừ việc không có macro nào (macro từ hay macro token) được tra cứu hoặc mở rộng (D25). Việc ô bảo mật có được biến đổi hay không là quyết định của adapter dựa trên chính sách host (§6): ở `Bypass`, adapter cho phím đi qua trước engine và kết thúc từ; ở `Restricted`, adapter chạy engine với macro bị tắt |
| `Character` có `is_shortcut()` | cho đi qua kèm `reset_context`, đặt lại trạng thái (ma trận #21) |
| `Character` có `alt_gr` | được xử lý như một ký tự bình thường (ma trận #22) |
| `Character` là dấu câu ranh giới (`. , ; : ! ? " ' ( ) [ ] { } < > / \ |`), khi không bị một hành động tiêu thụ | ranh giới từ với `Boundary::Punctuation` |
| `Character` là ký tự khác không phải chữ cái (chữ số, `@ # $ % & * - _ + = ~ ` ^`, ký tự không phải ASCII) | được nối thêm dưới dạng `Other` |
| `Character` có nhiều hơn một scalar | commit từ (đặt lại trạng thái), cho đi qua |
| `Separator` (dấu cách, tab, enter; cũng như `Character` `' '`, `'\t'`, `'\r'`, `'\n'`) | ranh giới từ với `Boundary::Whitespace` |
| `Backspace` | backspace theo ngữ nghĩa (§3.9) |
| `Navigation`, `MouseReset`, `FocusReset`, `ModeToggle`, `ConfigChanged`, `Shutdown` | cho đi qua kèm `reset_context`, đặt lại trạng thái |

**Ranh giới từ:**

1. Shift+Space (phân cách `' '` kèm `shift`) commit từ không đổi: không macro, không khôi phục (D7).
2. Macro hoạt động khi `macros_enabled && (enabled || macros_when_disabled)` và sự kiện không nằm
   trong ô bảo mật. Tại khoảng trắng, trước hết tra token đang theo dõi (D28, bên dưới): văn bản
   NFC đã kết xuất của nó, rồi các phím thô của nó nếu khác; sau đó là từ NFC đã kết xuất, rồi
   văn bản phím thô nếu khác, với `CaseMode::Adaptive` khi `auto_case_macro`, ngược lại `Exact`.
   Tại dấu câu, chỉ từ được tra cứu.
3. Ngược lại, nếu `enabled && spell_check && auto_restore_invalid && word.transformed` và từ đã
   kết xuất không phải âm tiết hoàn chỉnh hợp lệ (kể cả luật thanh/âm cuối) → thay nó bằng các phím
   thô (ma trận #15). Với `auto_restore_invalid = false`, commit như đã kết xuất (ma trận #16).
4. Phép thay thế xoá các đơn vị nó thay thế: toàn bộ token đang sở hữu nếu là macro token, các đơn
   vị của từ hiện tại nếu không. Nó được phát ra là `Tx(delete, text + delimiter, consume = true)`
   với ký tự phân cách in được; với Enter/Tab là `Tx(delete, text, consume = true, replay_key =
   <vk>)`. Khi không có thay thế, ký tự phân cách được cho đi qua. Khoảng trắng đặt lại trạng thái;
   `reset_context = true` mỗi khi engine sở hữu văn bản (một từ hoặc một token). Dấu câu tiếp nối
   một token được cho đi qua với `reset_context = false`.

**Token (D28).** Token là văn bản được gõ kể từ khoảng trắng trước đó. Trigger token có hiệu lực khi
macro đang hoạt động và bộ tra cứu macro có một trigger đang bật chứa dấu câu ranh giới
(`MacroLookup::has_token_triggers`); nếu không, dấu câu commit từ đúng như ở trên. Khi dấu câu ranh
giới commit một từ mà không có thay thế, engine giữ token nếu văn bản đã commit (dạng NFC đã kết
xuất hoặc dạng phím thô, kể cả dấu câu này) là phần mở đầu của một trigger đang bật
(`MacroLookup::is_trigger_prefix`, khớp chính xác hoặc, chỉ với adaptive, không phân biệt hoa
thường). Khi đó engine sở hữu văn bản đã kết xuất của token, tức phần đã commit rồi đến từ hiện tại,
để adapter có thể kiểm tra mọi đơn vị mà một phép thay thế token xoá; quyền sở hữu giới hạn ở
trigger dài nhất (32 scalar, D29) cộng một từ, và phép kiểm tra `max_delete_before` của adapter vẫn
áp dụng. Backspace bên trong từ hiện tại giữ token; khi từ rỗng, nó xoá dấu câu đã kết thúc phần đã
commit và trở về trạng thái trước dấu câu đó (từ đứng trước dấu câu, cùng với lịch sử của nó). Token
kết thúc, và không token mới nào bắt đầu trước khoảng trắng hoặc lần đặt lại kế tiếp, khi văn bản đã
commit không mở đầu trigger nào, một macro từ hoặc tự khôi phục thay thế một từ tại dấu câu, dấu câu
được escape theo VIQR commit một từ (ma trận #49), một từ vượt quá giới hạn context, hoặc gõ văn bản
không phải là từ. Các sự kiện đặt lại, phím tắt, phím trong ô bảo mật, một thay đổi tuỳ chọn làm tắt
trigger token, và khoảng trắng cũng kết thúc token; sau một lần đặt lại, từ kế tiếp bắt đầu một
token mới. Chỉ toàn bộ token mới khớp (ma trận #48): `x.v.v` không bao giờ mở rộng `v.v`. Hạn chế:
dấu câu đã kết thúc một token không còn khả năng khớp thì chưa từng được sở hữu, nên một lần
Backspace qua nó không khôi phục gì (`v,` Backspace `.v` Space cho ra `v.v `, không được mở rộng, và
không có gì bị chỉnh sửa).

**Chế độ tắt:** ký tự được cho đi qua. Khi macro hoạt động trong lúc tắt, engine vẫn theo dõi từ
đang gõ ở dạng nguyên văn để một ranh giới có thể mở rộng macro.

### 3.9 Lịch sử, dung lượng, lỗi

- Mỗi thay đổi trạng thái của từ hiện tại đẩy snapshot trước đó của từ vào một vòng đệm gồm
  `HISTORY_CAPACITY` (32) mục; mục cũ nhất bị loại bỏ.
- **Backspace:** trạng thái rỗi → cho đi qua. Còn snapshot → khôi phục và phát ra diff (`a a BS` →
  `a`, ma trận #17). Hết snapshot nhưng vẫn sở hữu văn bản → cho đi qua và đặt lại (đường dự phòng
  (fallback) của ma trận #18). Token đang được theo dõi tuân theo các quy tắc D28 của §3.8.
- **Tràn:** khi một từ sắp vượt quá `context_limit_codepoints` phím, từ đó bị bỏ khỏi quyền sở hữu,
  token đang được theo dõi cũng bị bỏ, và các ký tự tiếp theo được cho đi qua tới ranh giới kế tiếp
  (không truy cập ngoài biên (OOB), có tính tất định).
- **Lỗi:** văn bản sai định dạng → `InvalidUtf16`; `prior.version() != STATE_VERSION` →
  `InvalidStateVersion`; trạng thái không rỗi thuộc về một `(session_id, document_id)` khác →
  `ContextMismatch`; `UserDefined` với bảng phím không xác định hoặc bị thiếu →
  `UnsupportedInputMethod`; văn bản dài hơn 16 đơn vị → `CapacityExceeded`.

### 3.10 Các vector chuẩn (golden) mà engine phải đạt (Telex nếu không ghi khác)

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

- `MacroTable::new(entries) -> Result<Self, MacroError>`: lập chỉ mục các mục đang bật theo
  trigger NFC chính xác và theo trigger chữ thường. Triển khai `MacroLookup`.
- Thứ tự tra cứu: trigger chính xác; sau đó (chỉ với Adaptive) khớp theo chữ thường. Khi trùng,
  chọn `MacroId` nhỏ nhất. Các mục `BoundaryPolicy::Whitespace` chỉ được mở rộng khi gặp
  `Boundary::Whitespace`.
- Thích ứng chữ hoa/thường (Adaptive và `CasePolicy::Adapt`): dựa trên mẫu của trigger đã gõ —
  toàn bộ là chữ thường → văn bản thay thế như đã lưu; chữ cái đầu viết hoa và phần còn lại viết
  thường (hoặc chỉ một chữ cái viết hoa) → Title (viết hoa chữ cái đầu của văn bản thay thế); mọi
  chữ cái có phân biệt hoa/thường đều viết hoa và có nhiều hơn một chữ cái → Upper; mọi trường hợp
  khác → như đã lưu. Nếu văn bản sau khi thích ứng vượt quá 4,096 scalar hoặc không còn vừa một lần
  chèn kèm một ký tự phân cách ở NFC hoặc NFD (`ß` thành `SS`), văn bản đã lưu được dùng.
- Kiểm tra hợp lệ: trigger 1..=32 scalar (D29), không chứa khoảng trắng hay ký tự điều khiển; văn
  bản thay thế 1..=4096 scalar, không chứa NUL, có độ dài UTF-16 ở NFC và ở NFD, cộng thêm một đơn
  vị phân cách, tối đa là `MAX_INSERT_UNITS` (8,192) (D30, `fits_one_insert`); cả hai được lưu ở
  dạng NFC. Các vi phạm là `EntryError::TriggerTooLong` / `ReplacementTooLong` khi tạo và là các
  vấn đề `trigger-too-long` / `replacement-too-long` trong `parse_legacy_text`.
- `MacroTable` triển khai các truy vấn token của `MacroLookup` (D28): `has_token_triggers()` (có
  một trigger đang bật chứa dấu câu ranh giới, `raumakey_model::input::is_boundary_punctuation`)
  và `is_trigger_prefix(prefix, case_mode)` (tiền tố NFC là phần đầu chính xác của một trigger
  đang bật, hoặc ở chế độ adaptive, dạng chữ thường của nó là phần đầu của một trigger chữ thường).
  Phép kiểm tra adaptive chuyển riêng tiền tố về chữ thường, nên một trigger tiếng Hy Lạp có sigma
  ở cuối từ (final sigma) chỉ được mở rộng khi gõ đúng như đã lưu.
- Dữ liệu người dùng được nạp bằng `MacroStore::from_entries_lenient`, hàm này báo cáo theo vị trí
  các mục vi phạm D29 hoặc D30 trong khi các mục hợp lệ vẫn tiếp tục hoạt động. Bên gọi phải giữ
  nguyên văn các mục bị báo cáo và ghi chúng trở lại mà không thay đổi (vòng đời cấu hình của
  controller; chưa có API cho việc này).
- `MacroStore { revision, entries }`: `upsert(expected_revision, entry) -> Result<u64, MacroError>`,
  `remove(expected_revision, id)`, `list()`, `snapshot() -> Arc<MacroTable>`; revision đã cũ →
  `MacroError::Conflict`. ID mới là `max(id) + 1`.
- `MacroDocument` được lưu bền nằm trong `raumakey-config` (§5) để crate config chỉ phụ thuộc vào
  model; `MacroStore::from_entries(revision, entries)` và `MacroStore::entries()` chuyển đổi từ nó
  và sang nó.
- `parse_legacy_text(bytes) -> LegacyMacroImport { entries, issues }`: giải mã UTF-8 (có/không có
  BOM) hoặc UTF-16LE/BE có BOM; bỏ qua dòng trống và các dòng bắt đầu bằng `#`, `;`, `//`, hoặc
  chứa `DO NOT DELETE`; tách mỗi dòng còn lại tại dấu `:` đầu tiên; các dòng không hợp lệ trở thành
  vấn đề chỉ kèm số dòng (không kèm nội dung). `export_text(entries) -> String` ghi các dòng
  `trigger:replacement`.

---

## 5. `raumakey-config`

- `DataRoot::resolve(exe_dir, local_app_data) -> DataRoot`: portable khi và chỉ khi
  `exe_dir/raumakey.portable` tồn tại → `exe_dir/data/`; nếu không thì `%LOCALAPPDATA%/Raumakey/`
  (D9). Không bao giờ suy ra từ quyền ghi. (Thay bởi D40: thư mục dữ liệu là thư mục của exe khi
  ghi được, ngược lại là `%LOCALAPPDATA%\Raumakey`.)
- Các tài liệu (mỗi tài liệu có `version: u32`, `revision: u64`, và một map `extensions` giữ lại
  các khoá không xác định):
  - `ConfigDocument` → `config.toml` (các phần `input`, `macros`, `hotkeys`, `system`,
    `conversion`, `[[keymaps]]`; xem §5.1).
  - `MacroDocument { version, revision, entries: Vec<MacroEntry>, extensions }` → `macros.toml`
    (mảng `[[macro]]`). Chỉ giải mã về mặt cấu trúc; việc kiểm tra hợp lệ ngữ nghĩa của macro diễn
    ra khi `raumakey-macro` dựng bảng.
  - `AppRulesDocument` → `app_rules.toml` (bọc `CompatibilityConfig`).
  - `MigrationRecord` → `migration.json`.
- **Nạp theo từng trường:** lỗi cú pháp → `ConfigError::Corrupt` (tệp được giữ nguyên; bên gọi
  chạy với giá trị mặc định và không được âm thầm ghi đè tệp). Với một tệp hợp lệ về cú pháp, mỗi
  trường đã biết được giải mã độc lập; một trường sai sẽ quay về giá trị mặc định của nó và được
  báo cáo dưới dạng `FieldIssue { path, reason_code }`; các trường không xác định được đưa vào
  `extensions` (ma trận #45).
  `version > CURRENT` → `UnsupportedVersion`; các phiên bản cũ hơn chạy `migrate`.
- **Commit nguyên tử** `AtomicStore<D>::commit(expected_revision, doc) -> Result<u64, ConfigError>`:
  lấy khoá độc quyền trên `<file>.lock` (`std::fs::File::lock`), đọc lại revision hiện tại,
  `Conflict` nếu nó khác `expected_revision` (ma trận #46), đặt `revision + 1`, tuần tự hoá, ghi
  `<file>.tmp-<pid>-<n>` trong cùng thư mục, `sync_all`, thay thế qua `FileReplacer` được truyền vào
  (`StdReplacer` = `std::fs::rename`; adapter Windows = `ReplaceFileW`), đọc lại, so sánh SHA-256,
  nhả khoá, trả về revision mới.
- `ConfigError = NotFound | Corrupt | UnsupportedVersion | PermissionDenied | Conflict | IoFailure`.
- `watch`: `AtomicStore::poll_revision()` trả về revision trên đĩa với chi phí thấp; controller thăm
  dò định kỳ và store thông báo cho các bên đăng ký trong cùng tiến trình qua `std::sync::mpsc` khi
  commit.
- `DiagnosticsLog`: sink chỉ bật khi người dùng chọn (opt-in), ghi `DiagEvent::to_log_line()` vào
  `logs/raumakey-<yyyymmdd>.log`, tối đa 1 MiB mỗi tệp, giữ lại 5 tệp.

### 5.1 Schema v1 của `config.toml`

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

Tệp `app_rules.toml` v1:

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
  `record_probe(&mut self, host, ProbeResult) -> Result<(), PolicyError>` (cập nhật danh sách probe
  trong bộ nhớ; controller lưu bền danh sách này trong `app_rules.toml`).
- Thứ tự phân giải: giá trị mặc định cơ sở → các named set theo thứ tự khai báo → quy tắc ứng dụng
  (khớp exe; quy tắc có `window_class` thắng quy tắc không có) → khả năng:
  `!host.supports_tsf_range_edit` hoặc một probe thất bại với `PROBE_VERSION` hiện tại buộc dùng
  `SyntheticKey` trừ khi một quy tắc đã chọn tường minh một transport → `Clipboard` bị hạ xuống
  `SyntheticKey` trừ khi `clipboard_enabled` → ô bảo mật: `macro_allowed = false`, không bao giờ
  `Clipboard`; `vietnamese_allowed = false` ở chế độ `Bypass` (mặc định), giữ nguyên ở chế độ
  `Restricted` (D25). Các adapter xử lý theo kết quả này trước engine: một phím trong ô bảo mật mà
  chính sách không cho phép tiếng Việt sẽ được cho đi qua và kết thúc từ mà không đến engine; mọi
  phím khác chạy engine với `enabled &= vietnamese_allowed` và `macros_enabled &= macro_allowed`.
- **Các named set mặc định theo D13** (dữ liệu trong `CompatibilityConfig::defaults_v1()`):
  `chromium-family` = msedge.exe, chrome.exe, brave.exe, vivaldi.exe, opera.exe, ms-teams.exe,
  teams.exe, slack.exe, discord.exe → `reset_on_mouse = true`; `terminals` = windowsterminal.exe,
  wt.exe, openconsole.exe, conhost.exe → `reset_on_mouse = true`; `games` = rỗng, không có hành
  động. Kết quả văn bản cho các host này bằng với oracle tổng quát (ma trận #26); mỗi tập giữ hành
  động riêng của nó (ma trận #28).
- `hotkey::HotkeyClassifier`: nhận các sự kiện key-down/key-up kèm trạng thái phím bổ trợ; báo
  `HotkeyAction` cho các tổ hợp đã cấu hình. Một hotkey `ModifiersOnly` kích hoạt khi nhả phím bổ
  trợ cuối cùng, chỉ khi không có phím nào khác được nhấn trong lúc giữ các phím bổ trợ.
- Các hàm hỗ trợ của `ProbeCache`: upsert theo exe, chỉ giữ `probe_version` mới nhất.

---

## 7. `raumakey-ipc`

### 7.1 Khung (frame)

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

`FrameDecoder` nhận byte theo từng phần, kiểm tra độ dài header, phiên bản, kind, và
`payload_len` **trước khi** cấp phát, xác minh checksum, rồi giải mã phần thân JSON.
Lỗi: `BadMagic, UnsupportedVersion, UnknownKind, Oversize, ChecksumMismatch, Malformed`.

### 7.2 Thông điệp

`Hello { client_kind: Tip|Tool, pid, arch, protocol }` → `Welcome { client_id, server_revision }`;
`RuntimeSnapshot { config_revision, options, keymaps, hotkeys, compatibility, vietnamese_mode }`;
`MacroSnapshot { macro_revision, entries }`; `Command { ToggleMode | SetMode(bool) | HardReset |
ReportProbe(ProbeRecord) }`; `CommandResult { ok, code }`; `ModeChanged { vietnamese_mode, revision }`;
`ResetAll`; `Diagnostic(DiagEvent)` (chỉ khi opt-in); `Ping`/`Pong`; `Error { code }`; `Goodbye`.
Không có loại thông điệp nào mang phím gõ hay văn bản tài liệu.

### 7.3 Máy trạng thái

`ClientSession` (phía TIP): bắt tay (handshake), giữ snapshot hợp lệ gần nhất, `Backoff` (100 ms,
nhân đôi đến 10 s, tất định) cho việc kết nối lại. `ServerSession` (phía controller): kiểm tra
`Hello`, cấp `client_id`, kiểm tra mọi frame sau đó đều mang nó, gửi snapshot khi kết nối và ở mỗi
revision mới. Transport là bất kỳ `Read + Write` nào; named pipe nằm trong `raumakey-windows`.

### 7.4 Codec cũ (`evkey-compat`, tắt theo mặc định)

Triển khai chính xác §6 của đặc tả clean-room: kích thước mapping `0x1038`, offset `0x00..0x38`,
opcode 1/2/3/5, các trường raw-u32 có kiểm tra phạm vi, độ dài đầu ra chẵn `<= 0x1000`, hiệu lực
epoch 2000 ms, chờ mutex 100 ms, chờ phản hồi 50 ms, xả các phản hồi cũ, khớp sequence, không thử
lại sau một timeout mơ hồ, giữ nguyên các cờ không xác định, sentinel cũ `0xEBCA1234`. Tên các đối
tượng là hằng số chỉ có trong module này. `LegacyClient`/`LegacyServer` chạy trên một trait
`LegacyChannel` (khối dùng chung + tín hiệu yêu cầu/phản hồi + mutex + đồng hồ) để các dòng
ma trận #30–#38, #42, #55 được test bằng fake. Việc kích hoạt thất bại theo hướng đóng (fail
closed) khi một đối tượng EVKey đã tồn tại.

---

## 8. `raumakey-converter`

- `Charset { Unicode, UnicodeNfd, Viqr, Tcvn3, VniWindows, Viscii, Cp1258, NcrDecimal, NcrHex,
  Utf8Literal }` (D19). Các bảng mã byte cũ được biểu diễn ở dạng văn bản thông qua Windows-1252
  (`cp1252` byte↔char), vì đó là cách văn bản như vậy xuất hiện trên clipboard và trong tài liệu
  Unicode.
- `decode(text, from) -> Result<String, ConvertError>` (sang Unicode NFC),
  `encode(text, to) -> Result<String, ConvertError>`,
  `convert(text, from, to, options) -> Result<String, ConvertError>`;
  `options { case: None|Upper|Lower|Title, remove_tones: bool }`.
- `decode_bytes`/`encode_bytes` ở mức byte để chuyển đổi tệp.
- TCVN3 không có chữ hoa có dấu thanh dựng sẵn; khi mã hoá các chữ này sẽ dùng mã chữ thường thay
  thế và kết quả báo `lossy = true`.
- `ClipboardPort { sequence_number(), read_text(), write_text() }`;
  `convert_clipboard(port, from, to, options, secure_field) -> Result<ClipboardOutcome, ConvertError>`:
  từ chối trong ô bảo mật; đọc, chuyển đổi, xác minh sequence number không đổi trước khi ghi, nếu
  không thì báo `OwnershipConflict`; không bao giờ khôi phục đè lên nội dung clipboard mới hơn.
- `convert_file(src, dst, from, to, options, overwrite) -> Result<FileOutcome, ConvertError>`: từ
  chối `dst == src`; từ chối `dst` đã tồn tại trừ khi `overwrite`; ghi qua tệp tạm + đổi tên.

---

## 9. `raumakey-migrate`

- Các port: `LegacyRegistry` (chỉ đọc: giá trị và khoá con của một đường dẫn) và `SourceFs` (chỉ
  đọc: đọc tệp, liệt kê thư mục). Không port nào có phương thức ghi.
- Nguồn: registry `HKCU\Software\MyQ\EVKey` (cài đặt) và `HKCU\Software\EVKey\TsfCache` (chỉ dùng
  làm gợi ý); các tệp `setting.ini`, `setting.tree`, `evkmacro.txt` trong một thư mục EVKey do
  người dùng chọn hoặc được tự phát hiện.
- Việc phân tích độc lập theo từng trường: macro qua `raumakey_macro::parse_legacy_text`; INI qua
  một parser chịu lỗi; giá trị registry dưới dạng giá trị thô có kiểu; `setting.tree` được giữ
  nguyên dưới dạng byte thô.
- **Chính sách ánh xạ D18:** một trường chỉ được nhập vào schema của Raumakey thông qua bảng
  `FIELD_MAP`; bảng này được phát hành rỗng đối với các cài đặt vì không có nguồn được phép nào mô
  tả tên cài đặt hay cách mã hoá giá trị của EVKey. Mọi trường chưa được ánh xạ đều được giữ lại
  trong dữ liệu mở rộng của bản ghi migration. Các giá trị trông giống danh sách tên `*.exe` được
  nhập thành một named set `evkey-import-<n>` không có hành động (người dùng quyết định hành động;
  ma trận #28).
- `commit`: chụp snapshot các tài liệu Raumakey hiện tại, ghi các revision mới thông qua
  `AtomicStore`, đọc lại và xác minh revision + checksum, sau đó ghi `migration.json` với trạng thái
  `completed`, dấu vân tay của nguồn (SHA-256, kích thước), các số đếm, các trường bị cách ly (tên +
  mã lý do), và các giá trị thô được giữ lại. Bất kỳ thất bại nào cũng khôi phục các tài liệu từ
  snapshot và ghi trạng thái `rolled-back`.
- Lũy đẳng (idempotent): cùng dấu vân tay và `completed` → `NoChange`. Nguồn không bao giờ bị ghi,
  đổi tên, xoá, hủy đăng ký, hoặc đổi quyền.

---

## 10. Tầng Windows

### 10.1 `raumakey-windows` (mọi module đều `cfg(windows)` trừ các planner thuần)

`error` (ánh xạ HRESULT/Win32), `panic` (`guard_hresult`, `guard_lresult`, `guard_bool`, panic hook
im lặng chỉ ghi lại `file:line`), `strings`, `sid` (SID của người dùng hiện tại, hash SID, session
id), `names` (`Local\Raumakey.<sidhash>.<name>`, tên pipe
`\\.\pipe\Raumakey.<session>.<sidhash>.control`), `security` (descriptor chỉ cho người dùng
`O:<user>D:P(A;;GA;;;<user>)` cho mọi đối tượng kernel có tên; descriptor cho client AppContainer
của control pipe `O:<user>D:P(A;;GA;;;<user>)(A;;0x12008b;;;AC)` không có nhãn bắt buộc (mandatory
label), D27), `pipe` (server với `FILE_FLAG_FIRST_PIPE_INSTANCE`, `PIPE_REJECT_REMOTE_CLIENTS`, và
descriptor cho client AppContainer; client kết nối có timeout, chỉ yêu cầu
`FILE_GENERIC_READ | FILE_WRITE_DATA` và từ chối pipe được gắn nhãn dưới mức toàn vẹn trung bình
(medium integrity); xác minh peer về người dùng và session),
`registry` (các view Default/64/32, mở chỉ đọc cho các root bên ngoài, chốt chặn ghi chỉ cho phép
các đường dẫn dưới `Software\Raumakey` hoặc các khoá CLSID/TIP của Raumakey), `known_folder`,
`file_replace` (bản triển khai `ReplaceFileW` của `raumakey_config::FileReplacer`), `clipboard`
(triển khai `raumakey_converter::ClipboardPort`), `input` (SendInput cho backspace/văn bản
Unicode/phát lại với `RAUMAKEY_INJECTED_TAG`), `hooks` (hook bàn phím/chuột mức thấp), `foreground`
(pid, tid, exe, class, nâng quyền, phát hiện ô mật khẩu), `keyboard` (`ToUnicodeEx` không thay đổi
trạng thái, tên bố cục bàn phím), `shell` (shortcut, notify icon, `TaskbarCreated`),
`task_scheduler` (logon task được nâng quyền), `com` (guard cho apartment), `single_instance`,
`tsf_registration` (§10.4), `legacy` (các đối tượng có tên của `evkey-compat` với kích hoạt
fail-closed).

### 10.2 `raumakey-controller` (`raumakey.exe`, `asInvoker`, `uiAccess=false`)

- Lệnh con: (không có) thì chạy; `install [--dry-run] [--tip-x64 <path>] [--tip-x86 <path>]`;
  `uninstall [--dry-run]`; `startup <none|shortcut|elevated-task>`; `migrate [--from <dir>]
  [--dry-run]`; `convert --from <cs> --to <cs> [--in <file> --out <file>] [--clipboard]`;
  `update` (báo "updates unavailable", ma trận #57); `--version`.
- Chỉ một instance (named mutex), cửa sổ thông điệp ẩn, một biểu tượng tray (biểu tượng V/E),
  `TaskbarCreated` tạo lại biểu tượng mà không đổi chế độ (ma trận #58).
- Luồng UI không làm việc engine, tệp, mạng, hay IPC chặn. Một luồng worker sở hữu các store,
  migration, chuyển đổi, và phát quảng bá IPC; kết quả trả về qua một kênh `mpsc` cùng với
  `PostMessageW(WM_APP_RESULT)`.
- Bảng thu gọn: chế độ tiếng Việt, kiểu gõ, kiểu đặt dấu, mở rộng/thu gọn. Các tab khi mở rộng:
  Nhập liệu, Hotkey, Macro, Quy tắc ứng dụng, Chuyển mã, Hệ thống, Giới thiệu. Mọi chuỗi giao diện
  là chuỗi mới của Raumakey (tiếng Việt có dấu). Biểu tượng được sinh bởi `raumakey-resources`.
- Server IPC (named pipe) phát hành các revision `RuntimeSnapshot`/`MacroSnapshot`; nhận
  `ToggleMode`, `SetMode`, `HardReset`, `ReportProbe`.
- Một luồng hook riêng với vòng lặp thông điệp riêng sở hữu các hook bàn phím và chuột mức thấp.
  Hook bàn phím luôn phân loại hotkey (D24); việc xử lý engine trong hook chỉ diễn ra với các host
  có transport đã phân giải là `SyntheticKey` hoặc `Clipboard`.
- Transport đường dự phòng (fallback): các session có khoá `(pid, tid, foreground_epoch, document)`
  với giới hạn LRU; các phím được inject mang sentinel và bị bỏ qua; số backspace = số scalar trong
  phần hậu tố bị xoá (D20); transport clipboard chụp lại sequence number và chỉ khôi phục khi
  Raumakey vẫn sở hữu lần cập nhật mới nhất.
- Khởi động cùng Windows: lựa chọn mặc định là shortcut trong thư mục Startup; scheduled task được
  nâng quyền chỉ khi được chọn tường minh, và được gỡ bỏ theo cách lũy đẳng (ma trận #56).
- Trình tự `install`: đăng ký COM (view 64-bit cho TIP x64, view 32-bit cho TIP x86) → đăng ký
  profile và category TSF → xác minh bằng cách đọc lại → commit trạng thái installer
  (`HKLM\Software\Raumakey\Install`). Khi thất bại chỉ hoàn tác (rollback) các mục do Raumakey sở
  hữu. `--dry-run` in ra kế hoạch thực hiện.

### 10.3 `raumakey-tsf` (`raumakey_tip.dll`)

- Export chính xác `DllGetClassObject`, `DllCanUnloadNow`, `DllRegisterServer`,
  `DllUnregisterServer`.
- `TextService` triển khai `ITfTextInputProcessorEx`, `ITfKeyEventSink`, `ITfThreadMgrEventSink`,
  `ITfTextEditSink`, `ITfThreadFocusSink`, `ITfCompartmentEventSink` (mở/đóng bàn phím).
- Kho trạng thái theo từng tài liệu, có khoá là document id; thay đổi focus/tài liệu → đặt lại (ma
  trận #20). Thay đổi vùng chọn không do Raumakey thực hiện → đặt lại (ma trận #19).
- Đường đi của phím: `OnTestKeyDown` chạy engine mà không commit và lưu đệm kết quả theo khoá
  `(vk, scan, generation)`; `OnKeyDown` áp dụng kết quả đó trong một edit session đọc-ghi đồng bộ
  duy nhất: dịch điểm đầu vùng chọn lùi lại `delete_before`, yêu cầu số đơn vị thực sự dịch được
  bằng đúng giá trị đó và văn bản được bao phủ bằng đúng hậu tố mà engine sở hữu, `SetText`, thu
  gọn về cuối. Chỉ khi đó mới commit `next`. Bất kỳ thất bại nào → không commit, đặt lại, cho phím
  đi qua (ma trận #40, #41).
- Các phím phát lại được gửi bằng `SendInput` và `RAUMAKEY_INJECTED_TAG`; các sự kiện có
  `GetMessageExtraInfo()` bằng tag này được bỏ qua (ma trận #39).
- Ô bảo mật (input scope password) bỏ qua mọi phép biến đổi (ma trận #29).
- Snapshot: luồng client IPC giữ `RuntimeSnapshot`/`MacroSnapshot` hợp lệ gần nhất
  (`Arc` bất biến), kết nối lại với backoff hàm mũ có giới hạn; dùng giá trị mặc định v1 cho đến
  snapshot đầu tiên.
- **D24 — một chủ sở hữu hotkey duy nhất.** Khi đang kết nối với controller, TIP không phân loại
  hotkey: hook bàn phím mức thấp toàn cục của controller là bộ phân loại duy nhất (nó thấy mọi phím
  trước ứng dụng), nuốt các hotkey `VirtualKey`, và phát quảng bá `ModeChanged`/`ResetAll`. Chỉ
  khi mất kết nối, TIP mới tự phân loại hotkey (toggle đảo chế độ cục bộ của nó; hard reset đặt lại
  các session của nó). Vì vậy một phím không bao giờ bị phân loại hai lần.
- `DllCanUnloadNow` phản ánh số đối tượng còn sống và số lock.

### 10.4 Kế hoạch đăng ký TSF (D17, D21)

Planner thuần (test được ở mọi nơi) + executor (Windows). Các mục:
`HKLM\Software\Classes\CLSID\{TIP_CLSID}` (giá trị mặc định "Raumakey"),
`...\InprocServer32` (mặc định = đường dẫn DLL, `ThreadingModel = Apartment`) trong view khớp với
bitness của DLL; profile qua `ITfInputProcessorProfileMgr::RegisterProfile` (LANGID 0x042A,
`TIP_PROFILE_GUID`, mô tả "Raumakey", chỉ số biểu tượng 0 từ DLL); các category
`GUID_TFCAT_TIP_KEYBOARD`, `GUID_TFCAT_TIPCAP_SECUREMODE`, `GUID_TFCAT_TIPCAP_UIELEMENTENABLED`,
`GUID_TFCAT_TIPCAP_INPUTMODECOMPARTMENT`, `GUID_TFCAT_TIPCAP_COMLESS`,
`GUID_TFCAT_TIPCAP_IMMERSIVESUPPORT`, `GUID_TFCAT_TIPCAP_SYSTRAYSUPPORT`. Bước xác minh đọc lại
mọi khoá và liệt kê profile. Việc hủy đăng ký chỉ xoá đúng các mục này.

---

## 11. `raumakey-resources`

`ResWriter` (RT_MANIFEST, RT_VERSION, RT_ICON/RT_GROUP_ICON, RT_STRING), `VersionInfo`, manifest của
controller (`asInvoker`, `uiAccess="false"`, Common Controls v6, nhận biết DPI PerMonitorV2,
supportedOS Windows 10), bộ vẽ biểu tượng (glyph pixel nguyên bản: dấu hiệu ứng dụng, `V`, `E`)
tạo ảnh 32-bit kích thước 16/20/24/32/48 px, `emit(out_dir, artifact) -> PathBuf` cho build script.

---

## 12. Test, harness, fuzz, xtask

- `tests/fixtures/compat/*.toml`: các vector máy đọc được cho mọi dòng ma trận ở mức engine (A1–A18,
  B21–B23, B29, D47–D54) cùng các danh sách từ golden. Các token phím: ký tự đơn, `{BS}`, `{SPACE}`,
  `{SHIFT+SPACE}`, `{ENTER}`, `{TAB}`, `{NAV}`, `{MOUSE}`, `{FOCUS}`, `{CTRL+x}`, `{ALTGR+x}`,
  `{SECURE}` (đánh dấu các phím tiếp theo là được gõ trong ô bảo mật).
- Các bộ test hợp đồng (contract) của `raumakey-tests`: vector engine, tích hợp macro/engine, các
  dòng ma trận policy, tính đồng thời của config (#46), fixture migration (#44, #45), frame IPC, các
  dòng codec cũ (với `--features evkey-compat`), chuyển đổi khứ hồi (round trip) của converter.
- Fuzz: `fuzz/` (các target cargo-fuzz: chuỗi đầu vào engine, UTF-16 hỏng, config, tệp macro,
  frame IPC, quay vòng sequence của codec cũ, áp dụng transaction). Phần thân của chính các target
  này nằm trong `raumakey_tests::fuzz_targets` và chạy tất định trong `tests/fuzz_smoke.rs` (PRNG có
  seed) để bộ smoke chạy được dưới `cargo test` trên mọi host.
- `raumakey-test-runner.exe`: `vectors [--json <out>]` chạy fixture trong tiến trình và xuất JSON
  (test id, OS/build, kiến trúc, ứng dụng/phiên bản, revision config, chuỗi transaction, văn bản
  cuối cùng, pass/fail); `live --app notepad` (người dùng tự chạy) điều khiển Notepad thông qua TIP
  đã cài đặt và đọc lại văn bản của cửa sổ. Nó không bao giờ thu nhận đầu vào không liên quan.
- `tests/windows/Invoke-RaumakeyAcceptance.ps1` (người dùng tự chạy, nâng quyền): cài đặt, cả hai
  view registry, đăng ký COM, cài đặt profile, các chế độ khởi động, nâng cấp, hoàn tác (rollback),
  gỡ cài đặt; ghi bằng chứng JSON. `tests/windows/MANUAL_ACCEPTANCE.md`: Notepad, Office,
  Edge/Chrome, Windows Terminal, ô mật khẩu, khởi động lại Explorer, nhiều tài liệu, chuyển bố cục
  bàn phím, chính sách ứng dụng.
- `xtask`: `ci` (fmt, clippy `-D warnings` kể cả `--features evkey-compat`, test), `cross
  [--release]` (ba target MSVC; cấu hình linker native trên Windows, `cargo xwin` ở nơi khác),
  `inspect` (machine PE, export, manifest, resource, chuỗi định danh COM, danh mục artifact),
  `package` (package x64 = controller x64 + TIP x64 và x86; package ARM64 riêng; script cài đặt/gỡ
  cài đặt; nguồn WiX với `INSTALLER_UPGRADE_CODE`, MSI khi có `wix`; `SHA256SUMS`; `manifest.json`
  đánh dấu các package là chưa ký), `audit` (cargo-deny), `notices` (danh sách giấy phép của các
  dependency), `release-gate` (mọi gate theo thứ tự, JSON bằng chứng nằm dưới
  `target/release-evidence/`).

### Hợp đồng bằng chứng của cổng phát hành (bổ sung 2026-10-17)

`ci` và `release-gate` không đạt nếu thiếu test target `raumakey-tests/fuzz_smoke`.
`ci --no-fuzz` là một phép kiểm tra cục bộ từng phần được nêu rõ; nó không xác lập sự sẵn sàng
phát hành. CI bật `evkey-compat` cho IPC, Windows, controller, TIP, và các test dùng chung. Các
lệnh Cargo có phân giải dependency (`clippy`, `test`, `build`, và `metadata`) đều truyền `--locked`.

Các bước phát hành, theo thứ tự, là: `ci`, `audit`, `cross`, `inspect`, `whole-branch-review`,
`package`, `installer-dry-run`. Bằng chứng chỉ được ghi `cross-build verified` khi toàn bộ danh
sách có thứ tự này đạt, không thiếu hay trùng bước nào. Nó không bao giờ khẳng định đã kiểm chứng
khi chạy thật.

`installer-dry-run` chạy `install --dry-run` và `uninstall --dry-run` của controller trong package
x64, rồi chạy `install.ps1 -DryRun` và `uninstall.ps1 -DryRun` bằng Windows PowerShell 5.1
(`%SystemRoot%\System32\WindowsPowerShell\v1.0\powershell.exe -NoProfile -ExecutionPolicy Bypass
-File`), đúng cách `README.txt` hướng dẫn người dùng. Cả bốn lần chạy phải thoát với mã 0, nhắc đến CLSID và GUID profile của Raumakey, và không
nhắc đến GUID nào của EVKey (§14.4 mục 8).

Một reviewer độc lập cung cấp `<target-dir>/release-evidence/whole-branch-review.json`:

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

Đây là ví dụ về schema, không phải bằng chứng review. Mọi trường đều bắt buộc. Review phải khớp
với revision Git sạch hiện tại; trường không xác định, JSON không hợp lệ, phiên bản không được hỗ
trợ, reviewer để trống, bị từ chối, hoặc còn phát hiện chưa được giải quyết đều làm gate không đạt.
Bước thành công ghi lại SHA-256 của các byte được cung cấp. Lưu bản ghi này cùng với bằng chứng của
gate. Việc này kiểm tra một bản xác nhận (attestation); nó không thể xác lập tính độc lập của
reviewer hay thay thế một review thực sự.
`package` chạy riêng lẻ vẫn chỉ là một lệnh lắp ráp và không xác lập sự sẵn sàng phát hành.

Các dòng ma trận không thực thi được trong kho mã này: #43 (cần binary của EVKey; được ghi là không
áp dụng), các dòng runtime cần một desktop Windows thật (#19, #20, #24, #39, #40, #58 ở dạng runtime
của chúng) được bao phủ bằng fake trong unit test và bằng danh sách kiểm tra nghiệm thu thủ công.

---

## 13. Thứ tự task

| Giai đoạn | Task |
|---|---|
| 1 | Toolchain, khung workspace, `LICENSE`, `THIRD_PARTY.md`, `deny.toml`, `raumakey-model` |
| 2 | `raumakey-engine` (+ golden vector), `raumakey-macro` |
| 3 | `raumakey-config`, `raumakey-converter`, `raumakey-migrate` |
| 4 | `raumakey-policy`, `raumakey-ipc` (+ `evkey-compat`) |
| 5 | `raumakey-resources`, `raumakey-windows`, `raumakey-controller` |
| 6 | `raumakey-tsf`, đăng ký TSF |
| 7 | `xtask`, harness `tests/` + fixture + fuzz, bộ PowerShell, tài liệu |
| 8 | Gia cố (hardening), audit, review toàn branch, bằng chứng cổng phát hành |

Các giai đoạn có thể chạy song song khi các phụ thuộc của chúng đã hoàn tất. Mỗi task kết thúc bằng
fmt, clippy, các test của crate, và một review độc lập đối chiếu với kế hoạch này và thiết kế.

---

## 14. Trạng thái triển khai (2026-10-17)

> **Từ 2026-10-18:** bản phát hành là `raumakey.exe` portable (D38–D42). §16 ghi trạng thái và
> bằng chứng mới nhất; mục này giữ trạng thái trước khi đổi thiết kế. TIP (§14.5 bước 4) và IPC hub
> (một phần của bước 5) được hoãn lại.

Mục này theo dõi các task của §13. Nó mô tả bản sửa đổi Git chứa chính nó; công việc chỉ tồn tại
bên ngoài bản sửa đổi đó được liệt kê ở §14.3. Thiết kế §20 chứa bằng chứng chi tiết của snapshot
trước đó của nó (kết quả các gate, độ phủ ma trận tương thích, rủi ro và câu hỏi mở); §14.6 liệt kê
các gate được chạy lại cho bản sửa đổi này. "Xong" nghĩa là các hợp đồng của kế hoạch này đã có code
và test, trừ các khoảng trống được liệt kê ở §14.2 và §14.4. Nó không bao giờ có nghĩa là hành vi
runtime đã được kiểm chứng trong một ứng dụng chủ Windows (§0 quy tắc 6).

### 14.1 Tóm tắt

- **Xong:** các thư viện lõi `raumakey-model`, `-engine`, `-macro`, `-config`, `-converter`,
  `-policy` và `-ipc` (cùng codec `evkey-compat`), và `raumakey-resources`, trừ các khoảng trống
  ở §14.2 và các lỗi ở §14.4. `raumakey-migrate`, `raumakey-windows` và `xtask` phần lớn đã xong.
  `xtask` bao gồm hợp đồng bằng chứng cổng phát hành của §12. TIP đã có lõi phiên (session core) và
  lớp COM, và controller đã có dòng lệnh và một tray tối thiểu.
  `raumakey-tests` có bộ fuzz smoke, một bộ test macro/engine với `MacroTable` thật,
  các vector tương thích, và chế độ `vectors` của runner.
- **Chưa xong:** TIP chưa có client IPC và chạy trên bộ mặc định v1 (defaults v1) với chế độ riêng
  của nó, và controller chưa có hub IPC, luồng hook (hook thread), transport dự phòng (fallback) hay
  bảng cài đặt. Chưa có gì được quan sát trong một ứng dụng chủ Windows: TIP có thể được đăng ký và
  dùng thử, nhưng danh sách kiểm tra thủ công của nó chưa được chạy. Trong các quyết định của §15,
  D25, D27 (phía pipe), D28–D30 và D34 đã được triển khai; D26 chờ luồng hook và D31 chờ `migrate`.
  Workspace `fuzz/`, chế độ `live` của runner và `tests/windows/` chưa tồn tại.
- **Các gate:** `cargo xtask ci`, `cross`, `inspect` (debug và release), `package` và các lần chạy
  thử (dry run) của installer đều đạt (§14.6). Không thể khẳng định "cross-build verified" (đã kiểm
  chứng cross-build) hay "runtime x86_64 verified" (đã kiểm chứng khi chạy thật trên x86_64) (thiết
  kế §19): cổng phát hành còn cần audit và một review toàn branch.

### 14.2 Theo giai đoạn

| Giai đoạn | Trạng thái | Đã xong | Chưa xong |
|---|---|---|---|
| 1 | Xong | Workspace, ghim toolchain (Rust 1.99.1, ba target MSVC), `LICENSE`, `THIRD_PARTY.md`, `deny.toml`, `raumakey-model` | Các chỉnh sửa cho `THIRD_PARTY.md` (thiết kế §20.6 mục 9); §1.1 và §1.3 không khớp với các manifest |
| 2 | Xong (thư viện) | `raumakey-engine` với mọi golden vector của §3.10; `raumakey-macro`; engine chạy với `MacroTable` thật trong `tests/tests/macro_engine.rs` | D28–D30 và D34 đã được triển khai và test; §14.4 mục 7 còn mở |
| 3 | Phần lớn xong (thư viện) | `raumakey-config`, `raumakey-converter`, `raumakey-migrate` (nguồn, phân tích cú pháp, commit có hoàn tác (rollback), tính idempotent) | Độ bền (durability) của commit (thiết kế §20.7); một adapter `LegacyRegistry` cho Windows; tự tìm thư mục EVKey mặc định; lệnh `migrate` (D31) |
| 4 | Xong (thư viện) | `raumakey-policy`; `raumakey-ipc` với frame, thông điệp, phiên client và server, backoff, và codec `evkey-compat` | Hub server và bộ dựng snapshot của controller, và cách nó xử lý các client AppContainer; D25 đã được triển khai trong engine, lõi phiên của TIP và bộ thực thi vector, và đường hook của controller phải áp dụng cùng gate đó; D27 đã được triển khai trong `raumakey-windows` (descriptor của pipe, quyền truy cập client, kiểm tra nhãn medium) |
| 5 | Một phần | `raumakey.exe`: subsystem GUI kèm tài nguyên, dòng lệnh (`install` và `uninstall` có chạy thử (dry run) và hoàn tác (rollback), `startup`, `convert`, `update`, `--version`; `migrate` báo rằng nó chưa khả dụng), và một chế độ chạy tray tối thiểu (một instance duy nhất, biểu tượng V/E, menu, `TaskbarCreated`); `raumakey-resources`; các wrapper của `raumakey-windows` cho lỗi, panic guard, chuỗi, handle, SID, tên đối tượng, bảo mật, pipe, registry, known folder, thay thế tệp, clipboard, input (bao gồm `GetMessageExtraInfo`), hook, foreground, bàn phím, tiến trình, shell, task scheduler, COM apartment, single instance, đối tượng legacy, cửa sổ ẩn và cửa sổ message-only cùng vòng lặp thông điệp và `PostMessageW`, menu popup, biểu tượng, module, hỗ trợ COM server (class factory, bộ đếm đối tượng và bộ đếm khoá), và hộp thoại thông báo (message box) | Hub IPC, luồng hook, transport dự phòng, bảng cài đặt, worker thread và vòng đời cấu hình (§14.5 bước 5); hộp thoại và control, và các khối xây dựng của §14.5 bước 6 |
| 6 | Một phần | Bộ lập kế hoạch, bộ thực thi và phần kiểm chứng đăng ký TSF trong `raumakey-windows`, được test bằng fake; lõi phiên độc lập nền tảng của TIP (`raumakey_tip::session`), được test bằng fake; lớp COM của `raumakey_tip.dll`: bốn export, class factory, `DllRegisterServer`, `TextService` cùng vòng đời kích hoạt và các sink của nó, đường đi của phím qua các edit session đồng bộ, input scope, hook `WH_MOUSE`, và compartment mở/đóng, được test trong tiến trình với TSF thread manager thật | Client IPC, probe edit, chẩn đoán, và đường `evkey-compat` (§14.5 bước 4); phần kiểm chứng không liệt kê (enumerate) profile (D21); §14.4 mục 5 |
| 7 | Một phần | Các lệnh `xtask` `ci`, `cross`, `inspect`, `package`, `audit`, `notices` và `release-gate`; `raumakey-tests`: tám target fuzz được chạy bởi `fuzz_smoke` (bảy target của §12 cộng với truy cập đồng thời vào cấu hình, ma trận #46), bộ test macro/engine, 211 vector tương thích trong `tests/fixtures/compat/` cho các hàng ma trận ở cấp engine và các golden vector của §3.10, và `raumakey-test-runner vectors [--json]` | Các bộ test hợp đồng khác của §12 (hàng chính sách (policy), fixture migration (chuyển dữ liệu), frame IPC, hàng codec legacy, round trip của converter); chế độ `live` của runner; `fuzz/`; bộ test PowerShell; danh sách kiểm tra thủ công; các mục `xtask` của thiết kế §20.6 mục 7 mà đợt follow-up còn để mở (§14.5 mục 7); tài liệu (§14.5 mục 8) |
| 8 | Chưa bắt đầu | `xtask audit` đã đạt trong snapshot của thiết kế §20 (thiết kế §20.2; chưa chạy lại) | Gia cố (hardening) (thiết kế §20.7), review toàn branch, bằng chứng cổng phát hành |

Thiết kế §20.6 mục 8 mới xong một nửa: phần triển khai đã nằm trong Git (`24ba7a6` và các bản sửa
đổi sau đó). Nửa còn lại vẫn mở đối với phần code có trước đợt follow-up: §0 quy tắc 1 và §13 yêu
cầu mỗi task ghi lại lần chạy test fail của nó và một review độc lập, và phần code đó thiếu cả hai.
Đợt follow-up ngày 2026-10-17 (`2026-10-17-review-followup.md`) chỉ ghi lại bằng chứng TDD cho phần
bổ sung `xtask` của nó; đó là một review có mục tiêu, không phải một review độc lập toàn branch, và
review độc lập cho phần bổ sung của chính nó vẫn đang chờ. `2026-10-17-phase1-task-notes.md` ghi
lại các lần chạy test fail và các review độc lập của công việc được merge từ đó đến nay.

### 14.3 Đã bắt đầu, chưa có trong bản sửa đổi này

Không có. Công việc bắt đầu ngày 2026-10-17 trong các worktree cục bộ đã được review và merge;
`2026-10-17-phase1-task-notes.md` ghi lại các lần chạy test fail và các review của nó.

### 14.4 Các lỗi phát hiện sau thiết kế §20

Công việc ngày 2026-10-17 đã phát hiện các lỗi sau; mục 4 cho thấy rủi ro "Silent fallback" (dự
phòng âm thầm) của thiết kế §20.7 có thể xảy ra với đầu vào hợp lệ. Các mục 1–4 đã được xác nhận
trên code của `4507b1b`. Các mục 1–4 đã được sửa; các mục 5, 6 và 7 đã được quyết định (D36, D37,
D35). Mục 8 và 9 được phát hiện và sửa sau đó, khi đóng gói bản dùng thử.

1. `MacroTable` chấp nhận các trigger chứa dấu câu ranh giới, chẳng hạn `v.v`. Khi không có hành
   động kiểu gõ nào tiêu thụ phím dấu câu, engine commit từ tại phím đó (§3.8), nên macro không bao
   giờ được mở rộng (ma trận #47): `v.v` không bao giờ được mở rộng trong Telex, VNI hay VIQR. VIQR
   gán `'`, `?`, `.`, `(` và `\` làm hành động, và một bảng phím do người dùng định nghĩa có thể gán
   bất kỳ dấu câu nào, nên một trigger VIQR như `a.` vẫn khớp qua các phím thô của nó. Đã sửa ngày
   2026-10-17 bằng D28 (tra cứu token tại khoảng trắng; `crates/raumakey-engine/tests/tokens.rs`, và
   `tests/tests/macro_engine.rs::d47_an_accepted_trigger_with_boundary_punctuation_expands`, không
   còn bị bỏ qua (ignored)).
2. Một trigger có thể có tối đa 64 scalar (§4), nhưng một từ cần nhiều phím hơn
   `context_limit_codepoints` (mặc định 32, khoảng 8..=128) sẽ bị tràn (§3.9) và không bao giờ được
   mở rộng. Với các giá trị mặc định, không trigger nào dài 33–64 scalar được mở rộng. Dấu phụ và
   dấu thanh có thể thêm tối đa bốn phím (Telex `dduwowngf` gõ ra `đường` gồm năm scalar bằng chín
   phím), nên một số trigger dài 29–32 scalar cũng không mở rộng được, và một giới hạn thấp hơn làm
   các trigger ngắn hơn cũng không mở rộng được. Đã sửa ngày 2026-10-17 bằng D29
   (`MAX_TRIGGER_SCALARS` = 32;
   `d29_the_longest_trigger_expands_with_the_default_context_and_a_longer_one_is_rejected`).
3. Đã sửa ngày 2026-10-17. `RuntimeSnapshot::validate` từng chấp nhận `UserDefined` với một
   `user_keymap_id` không nằm trong các bảng phím của snapshot, và khi đó engine trả về
   `UnsupportedInputMethod` cho mọi ký tự. Giờ đây `validate` từ chối snapshot như vậy bằng
   `InvalidKeymap` (test `a_user_defined_method_needs_its_key_map_in_the_snapshot`).
4. Giới hạn nội dung thay thế 4,096 scalar (§4) không giữ được phần chèn của một macro trong giới
   hạn chèn 8,192 đơn vị (§2.7). Phần chèn là nội dung thay thế ở dạng chuẩn hoá đã cấu hình cộng
   với một ký tự phân cách in được, nên 4,096 scalar astral (ngoài BMP), hoặc hơn 2,730 chữ cái như
   `ệ` với NFD (mỗi chữ ba đơn vị), sẽ vượt quá giới hạn đó. Khi đó engine commit từ đúng như đang
   hiển thị, không mở rộng, không tự khôi phục và không có tín hiệu nào (thiết kế §20.7,
   "Silent fallback"). Đã sửa ngày 2026-10-17 bằng D30 (`fits_one_insert` trong phần kiểm tra hợp
   lệ và phần điều chỉnh chữ hoa/thường;
   `d47_d53_every_accepted_replacement_expands_at_every_boundary`).
5. Được báo cáo bởi công việc TIP và chưa được kiểm tra trên Windows: `tsf_registration` đăng ký
   profile mà không có bố cục bàn phím (keyboard layout) thay thế (`RegisterProfile` với một `HKL`
   null). Nếu khi đó Windows chạy profile trên bố cục mặc định của ngôn ngữ tiếng Việt, hàng phím số
   và các phím ngoặc sẽ tạo ra ký tự tiếng Việt trước khi Raumakey nhận được các phím. Cần quyết
   định profile có chỉ định một bố cục thay thế hay không, ví dụ US (`00000409`). Đã quyết định bằng
   D36: không có bố cục thay thế; kiểm tra thủ công trên Windows phải xem xét hàng phím số và các
   phím ngoặc.
6. Câu hỏi mở từ công việc controller: `install` đăng ký các TIP tại nơi package được giải nén,
   thường là một thư mục mà người dùng có quyền ghi, và in ra một cảnh báo. Cần quyết định việc cài
   đặt có bắt buộc dùng Program Files hay sao chép package vào đó. Đã quyết định bằng D37: việc cài
   đặt từ bất kỳ thư mục nào vẫn được cho phép, kèm cảnh báo.
7. Câu hỏi mở từ D28: một macro từ khớp với từ đầu tiên của một token sẽ thắng tại dấu câu. Với
   `v` → `với` và `v.v` → `vân vân`, gõ `v.v ` cho ra `với.với `, nên `v.v` không bao giờ có thể
   được mở rộng. Việc hoãn macro từ tại dấu câu sẽ thay đổi hành vi dấu câu hiện có.
   Đã quyết định bằng D35: macro từ thắng.
8. Phát hiện ngày 2026-10-17 khi chạy thử package release: `install.ps1` và `uninstall.ps1` gọi
   `raumakey.exe` (subsystem GUI) bằng toán tử `&`, nên Windows PowerShell 5.1 không chờ controller
   và không đặt `$LASTEXITCODE`; `install.ps1 -DryRun` thoát với mã 1 mà không in kế hoạch nào.
   Bước `installer-dry-run` chỉ chạy controller trực tiếp nên không phát hiện được lỗi này. Đã sửa:
   script khởi động controller bằng `System.Diagnostics.ProcessStartInfo` với đầu ra được chuyển
   hướng, chờ nó kết thúc rồi lấy mã thoát, và `installer-dry-run` chạy thêm cả hai script (§12).
   Lần cài thật (qua UAC) vẫn chưa được chạy.
9. Phát hiện ngày 2026-10-17: các test trong tiến trình của `crates/raumakey-tsf/tests/com` fail
   khi Caps Lock của desktop đang bật. Luồng test nhận trạng thái phím bật/tắt của desktop, nên
   text service gõ ra chữ hoa còn tài liệu giả lập ứng dụng chèn chữ thường. Đã sửa:
   `Tip::activate_after` thả mọi phím và tắt mọi phím bật/tắt của luồng test bằng
   `SetKeyboardState`.

### 14.5 Việc còn lại, theo thứ tự

Thứ tự này thay thế thiết kế §20.10. Các bước 1 và 2 đã xong. Các bước 6–8 có thể chạy song song
với các bước 3–5 (§13). Mỗi bước ghi lại các lần chạy test fail và review độc lập của nó (§0 quy
tắc 1, §13).

1. Xong: công việc Phase 1 đã được review và merge (§14.3).
2. Xong: các câu hỏi mở của thiết kế §20.8 và §14.4 mục 1, 2 và 4 đã được quyết định (§15).
3. Phần lớn xong: D25, D27 (phía pipe), D28–D30 và D34 đã được triển khai, review và merge, và
   kế hoạch §2.9, §3, §4, §6, §10.1 cùng thiết kế §10, §14, §20.7 đã được đồng bộ. Còn lại: đồng bộ
   thiết kế §9 với D31; cho giới hạn D30 một mã lỗi riêng nếu UI cấu hình cần phân biệt nó với giới
   hạn số scalar. §14.4 mục 5, 6 và 7 đã được quyết định (D35–D37) và hiện không cần thay đổi code;
   UI cấu hình phải cảnh báo về xung đột D35.
4. Hoàn thiện TIP (§10.3): luồng client IPC nhận snapshot, probe edit dạng range edit và
   `ReportProbe` của nó (được gửi lại sau khi kết nối lại trong khi lõi phiên vẫn giữ kết quả),
   các sự kiện `Diagnostic` chỉ bật khi được chọn (opt-in) (chỉ khi
   `RuntimeSnapshot.diagnostics_enabled` được đặt), đường `evkey-compat`, và compartment chế độ
   nhập cùng trạng thái thanh ngôn ngữ (language bar).
5. Hoàn thiện controller (§10.2): vòng đời cấu hình (thư mục dữ liệu gốc D9, các store, chế độ và
   lựa chọn khởi động cùng Windows được lưu trong `config.toml`, kiểm tra hợp lệ các tên bảng mã
   trong `[conversion]`); hub IPC và bộ dựng snapshot, gọi `verify_peer_same_user_and_session` trên
   mọi kết nối; luồng hook theo D24 và D26; transport dự phòng; bảng cài đặt; worker thread; chẩn
   đoán chỉ bật khi được chọn (opt-in); `migrate` (D31, với adapter `LegacyRegistry` cho Windows và
   các thư mục EVKey mặc định); cài đặt package ARM64; và legacy server `evkey-compat`.
6. Các khối xây dựng `raumakey-windows` còn lại: hộp thoại và control, theo dõi thay đổi cửa sổ
   foreground, liệt kê profile để kiểm chứng (D21), một truy vấn DPI, một vòng lặp thông điệp với
   `IsDialogMessageW` cho bảng cài đặt không chặn (modeless), và các helper mà hiện nay TIP tự triển
   khai riêng (hook `WH_MOUSE` theo luồng, các helper sink và selection của TSF, các helper
   `VARIANT`).
7. Test và nghiệm thu: các bộ test hợp đồng khác của §12, chế độ `live` của runner, workspace
   `fuzz/`, `tests/windows/Invoke-RaumakeyAcceptance.ps1` và `MANUAL_ACCEPTANCE.md`. Chạy các danh
   sách kiểm tra thủ công của `raumakey_tip::server` và của module desktop của controller, cùng các
   test thủ công được đánh dấu `#[ignore]`, trên một host Windows, và lưu kết quả làm bằng chứng.
8. Hoàn thiện `xtask` và tài liệu: phát hành kèm notices và văn bản giấy phép; kiểm tra các mục
   Common Controls v6, PerMonitorV2 và supportedOS của manifest trong `inspect`; chạy các test i686
   dưới WOW64 trong CI và sửa lỗi link doctest của chúng; biên dịch nguồn WiX trong CI; không để
   `package` phát hành các artifact không đạt `inspect`; sửa `THIRD_PARTY.md`; thêm một chỉ mục
   quyết định; ghi lại các quyết định ở cấp code của thiết kế §20.9 và các quyết định được liệt kê
   trong `2026-10-17-phase1-task-notes.md`; đồng bộ §1.1, §1.3 và §7 với code. Đã xong ngày
   2026-10-17: mọi tài liệu của dự án chuyển sang tiếng Việt, `GUIDE.md` hướng dẫn build và dùng
   thử, và `README.txt` của package viết bằng tiếng Việt (D33).
9. Sửa các rủi ro của thiết kế §20.7 vi phạm một yêu cầu của thiết kế, bắt đầu với độ bền của
   commit. Sau đó chạy review toàn branch, cổng phát hành và nghiệm thu trên Windows.

### 14.6 Bằng chứng cho bản sửa đổi này

Chạy ngày 2026-10-17 trên host phát triển Windows 11 x86_64 (Rust 1.99.1, `rust-lld` + `xwin`):

| Lệnh | Kết quả |
|---|---|
| `cargo fmt --all --check` | Đạt |
| `cargo xtask ci`: Clippy với `-D warnings`, feature mặc định và cả năm feature `evkey-compat` | Đạt |
| `cargo xtask ci`: `cargo test --locked --workspace` | 1,955 đạt, 0 không đạt, 17 bị bỏ qua (ignored) |
| Cũng các test đó với cả năm feature `evkey-compat` | 2,029 đạt, 0 không đạt, 17 bị bỏ qua |
| `cargo xtask ci`: bước fuzz smoke | 10 đạt |
| `cargo test --locked -p raumakey-engine -p raumakey-macro -p raumakey-tsf -p raumakey-windows -p raumakey-tests --target i686-pc-windows-msvc --lib --bins --tests` (WOW64) | 930 đạt, 0 không đạt, 11 bị bỏ qua |
| `cargo xtask cross` (debug: x86_64, i686, aarch64) | Đạt |
| `cargo xtask inspect` | Cả 57 mục kiểm tra đều đạt |
| `raumakey-test-runner vectors` (debug, x86_64) | 211 trên 211 vector đạt |
| `cargo xtask cross --release` (x86_64, i686, aarch64) | Đạt |
| `cargo xtask inspect --release` | Cả 66 mục kiểm tra đều đạt |
| `cargo xtask package --release` | Đạt: package x64 (11 tệp) và ARM64 (7 tệp); bỏ qua MSI vì không có `wix` |
| `install.ps1 -DryRun` và `uninstall.ps1 -DryRun` của package release, chạy bằng Windows PowerShell 5.1.26100 | Cả hai thoát với mã 0 và in đầy đủ kế hoạch, kể cả từ thư mục có dấu cách và chữ tiếng Việt (§14.4 mục 8) |

Các test bị bỏ qua là các test Windows thủ công (thiết kế §15.1). Lần chạy i686 không gồm doctest:
bước link của chúng lấy nhầm `msvcrt.lib` x64 trong môi trường này, kể cả với các crate không thay
đổi (§14.5 bước 8). `cargo xtask release-gate` chưa được chạy vì chưa có file review toàn branch,
nên bước `installer-dry-run` mới chỉ được chạy bằng tay qua các lệnh ở trên. Audit chưa được chạy
lại; thiết kế §20.2 có kết quả gần nhất của nó. Lần cài thật qua UAC chưa được chạy. Không điều nào
trong số này là bằng chứng về hành vi runtime trong các ứng dụng chủ Windows.

---

## 15. Các quyết định được ghi lại ngày 2026-10-17

Chủ sản phẩm đã trả lời các câu hỏi mở của thiết kế §20.8 và các lỗi của §14.4 vào ngày
2026-10-17. Các quyết định này là lựa chọn sản phẩm, không phải khẳng định về hành vi của EVKey. Khi
một quyết định thay đổi hành vi, §14.5 liệt kê công việc mà nó cần; cho đến khi công việc đó được
merge, code vẫn có thể theo văn bản trước đó.

- **D17 — Đăng ký profile TSF.** `DllRegisterServer`, được chạy bởi `regsvr32` có số bit (bitness)
  tương ứng, đăng ký lớp COM và profile cho toàn máy bằng
  `ITfInputProcessorProfileMgr::RegisterProfile` (§10.4). Raumakey không gọi
  `InstallLayoutOrTip` và không tự thêm mình vào danh sách bàn phím của người dùng; người dùng tự
  thêm bàn phím trong Windows Settings. Các chế độ `-ineime` và `-r -ineime` của đặc tả clean-room
  (§6.1) được cố ý bỏ qua; đây là một điểm sai lệch so với đặc tả đó và đã được ghi nhận.
- **D21 — Các mục đăng ký và kiểm chứng.** Các mục là những mục của §10.4 trong registry view
  tương ứng với bitness của DLL. Phần kiểm chứng đọc lại mọi khoá và liệt kê profile đã cài đặt
  thông qua TSF; việc huỷ đăng ký chỉ xoá các mục này.
- **D25 — Các chế độ ô bảo mật.** `Bypass` là mặc định: trong một ô bảo mật, mọi phím đi qua mà
  không bị biến đổi, không có macro và không dùng clipboard (thiết kế §14, ma trận #29).
  `Restricted` (`secure_field_mode = "restricted"` trong `app_rules.toml`) giữ việc biến đổi tiếng
  Việt trong ô bảo mật nhưng không bao giờ mở rộng macro và không bao giờ dùng clipboard. Chính sách
  (policy) quyết định (§6); các adapter cho phím trong ô bảo mật đi qua trước engine khi chính sách
  đã phân giải không cho phép tiếng Việt; engine không bao giờ mở rộng macro trong ô bảo mật. Điều
  này thay thế hàng của §3.8 vốn khiến engine bỏ qua mọi ô bảo mật. Một host không bảo mật mà quy
  tắc của nó tắt tiếng Việt vẫn chạy engine ở chế độ tiếng Anh, nên `macros_when_disabled` vẫn hoạt
  động ở đó.
- **D26 — Quyền riêng tư của keyboard hook trong controller (sửa đổi D24; thay bởi D39 cho bản
  portable, nơi hook chạy engine cho mọi host).** Keyboard hook mức thấp
  toàn cục vẫn là nơi duy nhất sở hữu phím tắt khi có TIP đang kết nối. Handler của nó phân loại
  phím tắt chỉ từ virtual key và trạng thái phím bổ trợ (modifier), không lưu phím nào, không ghi
  log gì, và chỉ chạy engine cho các host có transport đã phân giải là `SyntheticKey` hoặc
  `Clipboard`.
- **D27 — Host chạy sandbox (được sửa đổi ngày 2026-10-17 sau review).** Descriptor của pipe điều
  khiển là `O:<user>D:P(A;;GA;;;<user>)(A;;0x12008b;;;AC)`: ngoài toàn quyền của người dùng,
  `ALL APPLICATION PACKAGES` chỉ có quyền truy cập client (`FILE_GENERIC_READ | FILE_WRITE_DATA`;
  không có quyền tạo instance, `WRITE_DAC`, `WRITE_OWNER` hay `DELETE`), và các client mở pipe với
  đúng quyền truy cập đó. Một TIP trong host AppContainer của người dùng kết nối và nhận cùng các
  snapshot runtime và macro như mọi TIP khác. Pipe không mang nhãn bắt buộc (mandatory label): lúc
  đầu quyết định này yêu cầu một nhãn low, nhưng review cho thấy nhãn đó trao cho mọi tiến trình mức
  toàn vẹn thấp (low integrity) của người dùng toàn quyền kiểm soát pipe (tiến trình đó có thể thay
  đổi DACL của pipe hoặc giả danh controller và đưa macro giả mạo cho các TIP mức medium và các TIP
  đã nâng quyền), trong khi các AppContainer kết nối được mà không cần nó. Vì vậy, một host mức toàn
  vẹn thấp không phải AppContainer không thể kết nối và TIP của nó vẫn dùng bộ mặc định v1. Mọi
  client đều từ chối một pipe có nhãn bắt buộc thấp hơn medium, nên một tiến trình mức toàn vẹn thấp
  chiếm tên pipe trước cũng không thể giả danh controller. Các đối tượng kernel có tên vẫn chỉ dành
  cho người dùng: đường IPC không dùng đối tượng nào, mutex single-instance chỉ thuộc về controller,
  và các đối tượng legacy `evkey-compat` mang văn bản theo từng phím. Rủi ro được chấp nhận: bất kỳ
  tiến trình AppContainer nào của cùng người dùng đều có thể đọc cấu hình và các macro và gửi lệnh
  client.
- **D28 — Trigger macro vượt qua dấu câu.** Khi macro đang hoạt động, một ranh giới khoảng trắng
  (cùng Enter hoặc Tab) cũng tra cứu token được gõ kể từ khoảng trắng trước đó, bao gồm cả dấu câu
  ranh giới đã commit các từ bên trong nó, và thay thế toàn bộ token. Khi đó `v.v` được mở rộng
  (ma trận #47).
- **D29 — Độ dài trigger.** `MAX_TRIGGER_SCALARS` là 32, bằng giới hạn context mặc định; các
  trigger dài hơn bị từ chối khi một macro được tạo hoặc được nhập (import). Với giới hạn context
  thấp hơn số phím của một trigger, trigger đó vẫn không thể mở rộng; UI cấu hình phải nói rõ điều
  này.
- **D30 — Độ dài nội dung thay thế.** Một nội dung thay thế chỉ hợp lệ nếu độ dài UTF-16 của nó ở
  NFC và ở NFD, cộng thêm một đơn vị phân cách, tối đa là `MAX_INSERT_UNITS` (8,192). Việc tạo và
  nhập từ chối các nội dung thay thế dài hơn bằng một lỗi có kiểu (typed error) hoặc một vấn đề
  nhập (import issue), nên đường dự phòng (fallback) âm thầm của engine (thiết kế §20.7) không thể
  xảy ra với các macro hợp lệ.
- **D31 — Phạm vi nhập từ EVKey.** Việc nhập chỉ mang vào các macro và các tập có tên của danh sách
  tệp thực thi (`FIELD_MAP` vẫn rỗng, D18). Các giá trị khác được giữ lại dưới dạng bằng chứng thô
  có kiểu, và `migration.json` giữ dấu vân tay (fingerprint) của tệp macro, không bao giờ giữ văn
  bản macro. Thiết kế §9 cần được đồng bộ với điều này.
- **D32 — Thứ tự ưu tiên của quy tắc ứng dụng.** Trong cùng một tầng của §6, tập có tên hoặc quy
  tắc được khai báo đầu tiên quyết định từng trường.
- **D33 — Nội dung package (thay bởi D38).** Ngoài các tệp nhị phân của thiết kế §13, package x64
  còn kèm theo
  `tools/raumakey-test-runner.exe`, `install.ps1`, `uninstall.ps1` và một README. README viết bằng
  tiếng Việt, mã hoá UTF-8 có BOM để mọi trình soạn thảo trên Windows nhận đúng bảng mã; hai script
  vẫn là ASCII vì Windows PowerShell 5.1 đọc file không có BOM theo bảng mã ANSI.
- **D34 — Đường dự phòng (fallback) cho dấu mũ.** Khi tắt kiểm tra chính tả, `Circumflex` áp dụng
  cho chữ `a`, `e` hoặc `o` ngoài cùng bên phải của cụm (§3.6).
- **D35 — Macro từ trước macro token.** Khi một macro từ khớp với từ mà dấu câu ranh giới commit,
  nó được mở rộng tại đó, kể cả khi dấu câu đó có thể tiếp tục một trigger token (§3.8, D28). Với
  `v` → `với` và `v.v` → `vân vân`, gõ `v.v ` cho ra `với.với `. UI cấu hình phải cảnh báo khi một
  trigger token bắt đầu bằng một từ mà bản thân nó cũng là một trigger.
- **D36 — Không có bố cục bàn phím thay thế.** Profile TSF được đăng ký mà không có bố cục thay thế
  (`RegisterProfile` với một `HKL` null), nên Windows chọn bố cục cơ sở. Kiểm tra thủ công trên
  Windows phải xác nhận rằng hàng phím số và các phím ngoặc gõ ra các ký tự US của chúng dưới
  profile Raumakey; nếu không, quyết định này cần được xem xét lại.
- **D37 — Vị trí cài đặt.** `install` đăng ký các TIP tại nơi đặt package, kể cả một thư mục mà
  người dùng có quyền ghi, và in ra một cảnh báo trong trường hợp đó. Rủi ro được chấp nhận: một
  tiến trình của người dùng có quyền ghi vào thư mục đó có thể thay thế một DLL mà sau đó mọi ứng
  dụng đều nạp.
- **D38 — Bản portable một tệp (thay thế D33 và phần cài đặt của thiết kế §12–§13).** Raumakey
  phát hành dưới dạng một tệp `raumakey.exe` x86_64. Người dùng chạy tệp trực tiếp, không cần
  installer, quyền quản trị hay đăng ký COM/TSF. Package x64 chỉ gồm `raumakey.exe`, `README.txt`,
  `LICENSE`, `THIRD_PARTY.md`, `SHA256SUMS` và `manifest.json`. TIP, `install.ps1`,
  `uninstall.ps1`, test runner và WiX/MSI không còn được phát hành, nhưng mã của chúng vẫn ở lại
  kho. Cổng phát hành thay `installer-dry-run` bằng `portable-check` (thiết kế §21.1).
- **D39 — Hook bàn phím là transport gõ (thay thế D26).** `raumakey.exe` gõ bằng
  `WH_KEYBOARD_LL` và `WH_MOUSE_LL` trên luồng hook riêng. Mỗi phím được dịch theo layout của cửa
  sổ foreground rồi đưa qua `ServiceCore` của crate mới `raumakey-session`, tách ra từ TIP. Văn bản
  được sửa bằng `SendInput`: gửi Backspace, rồi ký tự Unicode, tất cả gắn `RAUMAKEY_INJECTED_TAG`.
  Cửa sổ chạy quyền cao hơn nhận phím nguyên vẹn. Hook chỉ nhận ra ô mật khẩu là Edit control
  `ES_PASSWORD` (thiết kế §21.2).
- **D40 — Cấu hình nằm cạnh exe (thay thế D9).** Thư mục dữ liệu là thư mục chứa `raumakey.exe`
  khi đã có `raumakey.toml` ở đó, hoặc khi thư mục đó ghi được và không nằm trong Program Files,
  Program Files (x86), thư mục Windows hay `%TEMP%`. Ngoài ra Raumakey dùng
  `%LOCALAPPDATA%\Raumakey`, nên chạy bằng quyền thường hay quyền quản trị cũng dùng cùng một tệp.
  Tên tệp:
  `raumakey.toml`, `raumakey-macros.toml`, `raumakey-app-rules.toml`, `raumakey-migration.json`,
  `raumakey-logs\`. Lần chạy đầu ghi cấu hình mặc định; các lần sau đọc lại. Tệp sửa bằng tay được
  nạp lại sau tối đa khoảng 2 giây. Chế độ portable bằng tệp đánh dấu bị bỏ (thiết kế §21.3).
- **D41 — Khởi động cùng Windows qua menu khay.** Bật mục này thì Raumakey tạo shortcut
  `Raumakey.lnk` trong thư mục Startup của người dùng, trỏ tới exe đang chạy; tắt thì xoá shortcut.
  Trạng thái lưu ở `[system] startup`. Nếu exe bị chuyển chỗ, lần chạy kế tiếp sửa lại shortcut.
  `elevated-task` chỉ còn qua dòng lệnh (thiết kế §21.4).
- **D42 — Menu khay thay cho bảng cài đặt.** Menu có các mục:
  - "Gõ tiếng Việt (Ctrl+Shift)";
  - "Telex", "VNI", "VIQR";
  - "Kiểm tra chính tả";
  - "Đặt dấu kiểu mới (oà, uý)";
  - "Gõ tắt (macro)";
  - "Khởi động cùng Windows";
  - "Mở file cấu hình";
  - "Giới thiệu";
  - "Thoát".

  Mỗi thay đổi được ghi ngay vào `raumakey.toml`. Bảng cài đặt của §10.2 được hoãn lại (thiết kế
  §21.5). Từ 2026-10-18, D43 thêm "Bảng cấu hình…" ở đầu menu.

Các quyết định ngày 2026-10-18 (thiết kế §22):

- **D43 — Bảng cấu hình.** Một hộp thoại một trang, dựng từ template trong bộ nhớ và chạy trên
  luồng riêng. Bảng chỉnh các khoá của nhóm "Cách gõ" (`method`, `modern_tone`, `free_marking`,
  `spell_check`, `auto_restore_invalid`), nhóm "Gõ tắt" (`[macros] enabled`, `when_disabled`,
  `auto_case`), "Khởi động cùng Windows" và `show_panel_on_start`, cùng chế độ Việt/Anh.
  - Bảng mở từ mục menu đầu tiên "Bảng cấu hình…", khi chạy lại exe (D45), và lúc khởi động khi
    `show_panel_on_start = true`. Khi bảng mở lúc khởi động, hộp thoại chào lần đầu không hiện.
  - Thay đổi có hiệu lực và được lưu ngay; không có nút Lưu hay Huỷ.
  - Bảng gửi yêu cầu mang giá trị đích. Tray luôn gửi lại toàn bộ trạng thái cho bảng, nên bảng
    luôn khớp với menu, phím tắt và tệp sửa tay.
  - "Mặc định" đặt lại hai nhóm "Cách gõ" và "Gõ tắt".
  - Phím tắt, dạng Unicode, giới hạn ngữ cảnh, key map và chẩn đoán vẫn chỉ sửa được trong tệp.
- **D44 — Danh sách gõ tắt.** Hộp thoại thêm, sửa và xoá mục của `raumakey-macros.toml`, kiểm tra
  theo D29 và D30, rồi ghi bằng một commit của store riêng trên luồng của hộp thoại.
  - Khoá lạ, mục không giải mã được, id và các cờ của từng mục được giữ.
  - Tệp hỏng không bao giờ bị ghi đè. Khi tệp bị sửa ở nơi khác, Raumakey hỏi trước khi ghi đè.
  - Sau khi lưu, tray yêu cầu worker kiểm tra tệp ngay để hook nhận bảng macro mới. `Settings` vẫn
    không ghi tệp macro.
- **D45 — Chạy lại exe mở bảng cấu hình.** Khi controller đã chạy, lần chạy mới gọi
  `AllowSetForegroundWindow(ASFW_ANY)`, gửi thông điệp đăng ký `Raumakey.<sidhash>.show-panel` tới
  các cửa sổ cấp cao nhất, rồi thoát với mã 0. Bản đang chạy mở bảng hoặc đưa bảng lên trước.
  Controller chạy quyền quản trị cho thông điệp này qua UIPI.

---

## 16. Bản portable một tệp (2026-10-17)

Mục này theo dõi các task của D38–D42. Thứ tự §14.5 vẫn đúng với những phần không bị thay thế:
TIP (bước 4) và IPC hub (một phần của bước 5) được hoãn lại vì bản phát hành không có TIP.

### 16.1 Task

| # | Task | Phạm vi |
|---|---|---|
| P1 | Tách lõi phiên | Crate `raumakey-session`, chuyển nguyên `crates/raumakey-tsf/src/session/` cùng test; TIP re-export |
| P2 | Cấu hình portable | `DataRoot` theo thư mục exe, có đường dự phòng `%LOCALAPPDATA%`; tên tệp mới; module `settings` của controller (lần chạy đầu, nạp lại, snapshot cho lõi phiên, thay đổi từ menu, phát hiện sửa tay) |
| P3 | Package portable | `xtask package`: chỉ exe và tài liệu; bỏ WiX/MSI và script; `README.txt` mới; bước `portable-check` |
| P4 | Gõ bằng hook | Module `typing` của controller: `ServiceCore` ở vai trò hook, theo dõi phím bổ trợ, phím lặp và Caps Lock từ sự kiện hook, tài liệu theo cửa sổ foreground, sửa mù bằng `SendInput`, bỏ qua cửa sổ quyền cao hơn |
| P5 | Ghép nối | Chế độ chạy: nạp cấu hình, khởi động luồng hook, menu khay D42, khởi động cùng Windows D41, nạp lại khi tệp bị sửa, mục Giới thiệu |
| P6 | Tài liệu và bản phát hành | Thiết kế §21, kế hoạch §15–§16, `GUIDE.md`, `README.md`, bản build trong `release\` |

### 16.2 Trạng thái

Mọi task đã xong, đã được review độc lập và sửa theo review. `2026-10-17-phase1-task-notes.md`
(mục "Bản portable một tệp") ghi các lần chạy đỏ và kết quả review.

| Task | Trạng thái | Commit | Review |
|---|---|---|---|
| P1 | Xong | `85e1b97` | Chuyển nguyên, số test không đổi |
| P2 | Xong | `dc398a4`, `f14ba73` | Sửa 2 lỗi lớn: thư mục cấu hình khác nhau giữa chạy thường và chạy quyền quản trị; poll kẹt sau một lần đọc thất bại tạm thời |
| P3 | Xong | `80b85d3` | — |
| P4 | Xong | `e0a569e`, `e1eaf58` | Sửa 2 lỗi lớn: Ctrl+Shift mất tác dụng sau Win+L; Caps Lock đổi khi hook không được gọi |
| P5 | Xong | `c2ec3b8`, `4278a32` | Sửa 3 lỗi lớn: tiến trình chạy mà không có icon; lệnh `startup` và menu không thống nhất; shortcut trỏ vào `%TEMP%` |
| P6 | Xong | các commit tài liệu, bản build trong `release\` | — |

Bằng chứng cho bản sửa đổi `4278a32`, chạy ngày 2026-10-18 trên host phát triển Windows 11 x86_64
(Rust 1.99.1, `rust-lld` + `xwin`):

| Lệnh | Kết quả |
|---|---|
| `cargo fmt --all --check` | Đạt |
| `cargo xtask ci`: Clippy với `-D warnings`, feature mặc định và cả sáu feature `evkey-compat` | Đạt |
| `cargo xtask ci`: `cargo test --locked --workspace` | 2,244 đạt, 0 không đạt, 19 bị bỏ qua |
| Cũng các test đó với các feature `evkey-compat` | 2,318 đạt, 0 không đạt, 19 bị bỏ qua |
| `cargo xtask ci`: bước fuzz smoke | 10 đạt |
| Test i686 (WOW64), lệnh ở `GUIDE.md` §3 | 957 đạt, 0 không đạt, 12 bị bỏ qua |
| `cargo xtask cross --release` | Đạt (x86_64, i686, aarch64) |
| `cargo xtask inspect --release` | Cả 66 mục kiểm tra đều đạt |
| `cargo xtask package --release` | Đạt: mỗi package có 6 tệp |
| `raumakey.exe --version` của package | `Raumakey 0.1.0`, mã thoát 0, không ghi tệp nào |

Smoke test của `raumakey.exe` thật trên máy phát triển. Mỗi lần chạy vài giây, với hook mức thấp
thật được cài:

- Với một `raumakey.toml` có sẵn ở chế độ tiếng Anh: tiến trình chạy, tệp không đổi, và WM_CLOSE
  cho mã thoát 0.
- Lần chạy đầu trong một thư mục trống ngoài `%TEMP%`: `raumakey.toml` (cấu hình mặc định) và
  `raumakey.toml.lock` được tạo cạnh exe, còn `%LOCALAPPDATA%\Raumakey` không được tạo. Tiến trình
  có cửa sổ khay ẩn và hộp thoại chào trên một luồng riêng. WM_CLOSE gửi tới cửa sổ khay trong lúc
  hộp thoại còn mở cho mã thoát 0.
- `taskkill /PID` (không `/F`) chỉ gửi WM_CLOSE tới cửa sổ đang hiện, tức là hộp thoại, nên khi hộp
  thoại còn mở, lệnh này chỉ đóng hộp thoại.

Chưa kiểm chứng: việc gõ trong ứng dụng thật. Smoke test không gửi phím nào. Các danh sách kiểm tra
thủ công ở module `typing` và `desktop` của controller, và `GUIDE.md` §5.6, chưa được chạy. Không
điều nào ở trên là bằng chứng về hành vi khi chạy thật.

---

## 17. Bảng cấu hình và danh sách gõ tắt (2026-10-18)

Mục này theo dõi các task của D43–D45 (thiết kế §22). Mỗi task được làm trong một Git worktree
riêng, theo TDD, được review độc lập rồi mới merge.

### 17.1 Task

| # | Task | Phạm vi |
|---|---|---|
| S1 | Khối dựng hộp thoại | `raumakey-windows`: template hộp thoại trong bộ nhớ (thuần, test trên mọi host); hộp thoại modal với `DialogHandler`, chạy trên luồng riêng (`spawn_dialog`), đóng không cần chờ, chống gọi lồng vào handler, chặn panic; thông điệp đăng ký theo người dùng, gửi tới các cửa sổ cấp cao nhất, `AllowSetForegroundWindow` |
| S2 | Bảng cấu hình | `raumakey-controller`: các `Change` mới; trạng thái bảng và yêu cầu của bảng (thuần); tray mở, cập nhật và đóng bảng; mục menu "Bảng cấu hình…"; mở bảng lúc khởi động theo `show_panel_on_start`; chạy lại exe mở bảng (D45); glue Windows của bảng |
| S3 | Danh sách gõ tắt | Mô hình danh sách (thuần): kiểm tra theo D29 và D30, giữ khoá lạ; hộp thoại có list view; đọc và ghi trên luồng của hộp thoại; kiểm tra tệp ngay sau khi lưu |
| S4 | Tài liệu và bản phát hành | Thiết kế §22, kế hoạch §15 và §17, `GUIDE.md`, `README.md`, ghi chép task, bản build trong `release\` |

### 17.2 Trạng thái

| Task | Trạng thái | Commit | Review |
|---|---|---|---|
| S1 | Đang làm | — | — |
| S2 | Chưa bắt đầu | — | — |
| S3 | Chưa bắt đầu | — | — |
| S4 | Thiết kế §22 và kế hoạch D43–D45 đã viết | — | — |
