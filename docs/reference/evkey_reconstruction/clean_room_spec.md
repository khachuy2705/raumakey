# Đặc tả clean-room tái triển khai EVKey

## 1. Mục tiêu, chuẩn từ khóa và provenance

Đặc tả này trung lập ngôn ngữ, tách engine thuần khỏi Windows/TSF. **MUST** là tương thích quan sát được hoặc yêu cầu an toàn; **SHOULD** là hành vi suy luận/user-facing cần giữ; **MAY** là lựa chọn implementation. Cầu IPC legacy là adapter tương thích, không phải API nội bộ lý tưởng. DLL chỉ cho thấy việc forward event tới controller; không có bằng chứng DLL chứa thuật toán engine EVKey ([`research/evkime_dlls.md:145-168`](research/evkime_dlls.md#L145-L168), địa chỉ x64 `0x180005550`, `0x180002f00`).

## 2. Module bắt buộc

1. `InputNormalizer`: key/UTF-16 + modifier → `InputEvent`.
2. `VietnameseEngine`: event + options + state → `EditTransaction` + state mới.
3. `StateRepository`: sở hữu state theo `HostContext` đã chọn.
4. `MacroStore`: lookup/mutation/persistence macro độc lập charset.
5. `CompatibilityPolicy`: chọn cơ chế TSF/replay/clipboard và reset policy theo host.
6. `TsfAdapter`: COM TIP (đúng CLSID/profile GUID/category đã quan sát), sink, composition/edit session, range replacement, **và đăng ký/migration TIP identity-profile**.
7. `LegacyIpcAdapter`: tương thích named objects/schema/opcodes hiện có.
8. `Controller`: config/UI/tray/hotkey/lifecycle; phục vụ engine request.
9. `ConfigStore`: schema/version/migration/atomic commit.
10. `UpdateLauncher`: boundary ngoài tiến trình; không nằm trong trusted engine.

## 3. Kiểu dữ liệu chuẩn

### 3.1 `InputEvent`

```text
InputEvent {
  kind: Character | Backspace | Separator | Navigation | MouseReset |
        FocusReset | ModeToggle | ConfigChanged | Shutdown,
  text_utf16: sequence<uint16>,       // rỗng trừ Character/Separator
  virtual_key: optional<uint32>,
  scan_code: optional<uint32>,
  shift: bool,
  caps_lock: bool,
  ctrl: bool,
  alt: bool,
  alt_gr: bool,                       // phân biệt Ctrl+Alt tạo ký tự
  win: bool,
  repeat: bool,
  injected_tag: optional<uint64>,
  monotonic_time_ms: uint64,
  host: HostContext
}
```

MUST không coi một surrogate đơn lẻ là ký tự Unicode hoàn chỉnh. MUST bypass event có `injected_tag == 0xEBCA1234` trong TSF legacy adapter để tránh recursion ([`research/evkime_dlls.md:145-153`](research/evkime_dlls.md#L145-L153), x64 `0x180002a70`, `0x180003c20`).

### 3.2 `EngineOptions`

```text
EngineOptions {
  enabled: bool,
  input_method: Telex | VNI | VIQR | UserDefined,
  normalization: NFC | NFD | Preserve,
  modern_tone: bool,
  free_marking: bool,
  spell_check: bool,
  auto_restore_invalid: bool,
  macros_enabled: bool,
  macros_when_disabled: bool,
  auto_case_macro: bool,
  user_keymap_id: optional<string>,
  context_limit_codepoints: uint32
}
```

SHOULD hỗ trợ các lựa chọn đã lộ trong UI/config như spell check, macro, free mark, modern style, charset/input mechanism và clipboard fallback ([`research/evkey64_exe.md:32-40`](research/evkey64_exe.md#L32-L40), [`research/evkey64_exe.md:81-87`](research/evkey64_exe.md#L81-L87)). Exact default là Unknown, vì vậy MUST do migration/default policy phiên bản hóa quyết định, không hard-code từ suy đoán.

### 3.3 `EngineState`

```text
EngineState {
  version: uint32,
  raw_keys: sequence<InputAtom>,
  canonical_symbols: sequence<CanonicalVietnameseSymbol>,
  rendered_utf16: sequence<uint16>,
  word_form: Unknown | Empty | C | V | CV | VC | CVC | NonVietnamese,
  tone_position: optional<uint32>,
  temporary_literal_mode: bool,
  escape_armed: bool,
  pending_macro_key: sequence<UnicodeScalar>,
  generation: uint64
}
```

EngineState MUST không chứa HWND, COM pointer, raw pointer hoặc handle. SHOULD giữ raw-key history để auto-restore và semantic backspace. Mô hình `C/V/CV/VC/CVC` là lựa chọn baseline UniKey, không phải nội bộ EVKey đã chứng minh ([`research/unikey_baseline.md:246-281`](research/unikey_baseline.md#L246-L281); nguồn gốc: `x-unikey/src/ukengine/ukengine.h:127-145`, `ukengine.cpp:1347-1684`).

### 3.4 `EditTransaction`

```text
EditTransaction {
  delete_before: uint32,              // Proposed: đếm theo UTF-16 host-visible code unit (suy ra từ ngữ nghĩa `cch`/range của TSF); bằng chứng wire chỉ nói "4 byte characters/backspaces"
  insert_utf16: sequence<uint16>,
  consume_original: bool,
  replay_key: optional<ReplayKey>,
  reset_context: bool
}
ReplayKey { virtual_key:uint32, scan_code:optional<uint32>, modifiers:ModifierSet }
```

Hợp đồng:
- `delete_before` MUST không âm, MUST được kiểm tra với range hiện có và MUST không vượt context host đã xác nhận.
- `insert_utf16` MUST là UTF-16 well-formed; rỗng là hợp lệ.
- `consume_original=true` nghĩa host không chuyển event gốc xuống app.
- `replay_key` MUST chỉ dùng khi event gốc cần phát lại; TSF legacy adapter MUST gắn sentinel `0xEBCA1234`.
- `reset_context=true` yêu cầu commit/end composition và xóa state sau khi áp transaction.
- Apply MUST nguyên tử trong một TSF edit session: lùi range, `SetText`, cập nhật selection/composition ([`research/evkime_dlls.md:145-166`](research/evkime_dlls.md#L145-L166), x64 `0x180002f00`).

### 3.5 `HostContext`

```text
HostContext {
  session_id: opaque128,
  process_id: uint64,
  thread_id: uint64,
  executable_basename: lowercase-string,
  window_class: optional<string>,
  document_id: optional<opaque128>,
  supports_tsf_range_edit: bool,
  is_secure_field: bool,
  is_elevated: bool,
  keyboard_layout: string,
  foreground_epoch: uint64
}
```

`session_id` MUST uniquely phân vùng state ít nhất theo TSF activation/document context. Không được mô phỏng slot IPC đơn thành một global engine không phân vùng; binary không làm rõ server giữ state theo process/thread/window hay global ([`research/evkime_dlls.md:170-187`](research/evkime_dlls.md#L170-L187)).

## 4. Interface chính xác

```text
interface VietnameseEngine {
  process(event: InputEvent, options: EngineOptions,
          prior: EngineState) -> Result<EngineResult, EngineError>
  reset(prior: EngineState, reason: ResetReason) -> EngineState
}
EngineResult { transaction: EditTransaction, next: EngineState }
EngineError = InvalidUtf16 | InvalidStateVersion | ContextMismatch |
              CapacityExceeded | UnsupportedInputMethod | InternalInvariant

interface CompatibilityPolicy {
  resolve(host: HostContext, config: CompatibilityConfig) -> HostPolicy
  record_probe(host: HostContext, probe: ProbeResult) -> Result<void, PolicyError>
}
HostPolicy {
  transport: TSF | SyntheticKey | Clipboard,
  reset_on_mouse: bool,
  permit_replay: bool,
  max_delete_before: uint32,
  macro_allowed: bool,
  secure_field_mode: Bypass | Restricted
}

interface ConfigStore {
  load() -> Result<VersionedConfig, ConfigError>
  migrate(from: VersionedConfig) -> Result<VersionedConfig, ConfigError>
  commit(expected_revision:uint64, value:VersionedConfig)
      -> Result<uint64, ConfigError>
  watch(after_revision:uint64) -> Stream<ConfigChange>
}
ConfigError = NotFound | Corrupt | UnsupportedVersion | PermissionDenied |
              Conflict | IoFailure

interface MacroStore {
  lookup(normalized_trigger:sequence<UnicodeScalar>, boundary:Boundary,
         case_pattern:CasePattern) -> optional<MacroExpansion>
  upsert(expected_revision:uint64, entry:MacroEntry) -> Result<uint64, MacroError>
  remove(expected_revision:uint64, id:MacroId) -> Result<uint64, MacroError>
  list(snapshot_revision:uint64) -> sequence<MacroEntry>
}
MacroEntry { id, trigger_nfc, replacement_nfc, enabled, boundary_policy, case_policy }
```

ConfigStore/MacroStore MUST ghi atomic (temp + durable flush + replace hoặc transaction DB), validate length/UTF-16/Unicode, và dùng optimistic revision để không silent overwrite. Macro SHOULD lookup ở word boundary, hỗ trợ case adaptation và escape expansion; đây là baseline UniKey (`unikey/keyhook/vietkey.cpp:967-1039`, `x-unikey/src/ukengine/ukengine.cpp:2049-2157`) được tổng hợp tại [`research/unikey_baseline.md:303-321`](research/unikey_baseline.md#L303-L321), chưa phải kết quả EVKey đã chứng minh.

## 5. Luồng engine

1. Validate `InputEvent` và resolve `HostPolicy`.
2. Bypass secure field/modifier conflict hoặc reset theo policy.
3. Map key sang action semantic; Telex/VNI/VIQR mapping SHOULD theo baseline được kiểm thử riêng (`x-unikey/src/ukengine/inputproc.cpp:100-162`; [`research/unikey_baseline.md:234-244`](research/unikey_baseline.md#L234-L244)).
4. Tính state mới, tone target, undo/reposition/restore.
5. Render theo normalization đã chọn.
6. Diff suffix cũ/mới thành `delete_before + insert_utf16`.
7. Commit state chỉ sau khi host báo apply thành công; apply failure MUST rollback hoặc reset.

Action không hợp lệ MUST đi qua literal, không nuốt phím. Duplicate mark SHOULD hoàn nguyên rồi phát literal action key; tone/qu/gi/restore là baseline, không được gắn nhãn EVKey-observed ([`research/unikey_baseline.md:451-466`](research/unikey_baseline.md#L451-L466)).

## 6. Legacy IPC compatibility adapter

Named objects và schema này chỉ dành cho interoperability với DLL/controller legacy; core mới MUST gọi `VietnameseEngine` qua typed in-process interface hoặc framed IPC riêng có version/client identity.

| Offset | Kiểu | Adapter legacy MUST |
|---:|---|---|
| `0x00` | u32 | opcode |
| `0x04` | u32 | một UTF-16 code unit cho op 1 |
| `0x08` | u32 | Shift flag |
| `0x0c` | u32 | Caps flag |
| `0x10` | u32 | request sequence, tăng atomic; **signedness Unknown** — giữ raw 4 byte và validate wrap thay vì diễn giải `i32` |
| `0x14` | u32 | response sequence; **signedness Unknown** — so khớp raw 4 byte |
| `0x18` | u32 | số characters/backspaces cần xóa/backspace (trường **4 byte**); **signedness Unknown**; đơn vị "UTF-16 code unit" là diễn giải **Proposed** từ ngữ nghĩa `cch` của TSF, MUST validate range |
| `0x1c` | u32 | output length theo byte, phải chẵn và `<=0x1000`; **signedness Unknown** |
| `0x20` | u32 | cờ chưa rõ; preserve, không suy diễn |
| `0x24` | u32 | cờ gate reset theo hành vi quan sát |
| `0x28` | u32 | cờ chưa rõ; preserve |
| `0x2c` | u32 | cờ chưa rõ; preserve |
| `0x30` | u64 | tick/epoch, cửa sổ validity 2.000 ms |
| `0x38` | byte[0x1000] | UTF-16LE replacement |

Total mapping MUST là `0x1038`. Names MUST là `EVKeyIME_SharedMem_v1`, `EVKeyIME_Request_v1`, `EVKeyIME_Response_v1`, `EVKeyIME_Mutex_v1`.

Ownership và thứ tự khởi động (Observed): controller tạo mapping `EVKeyIME_SharedMem_v1` + hai auto-reset event `EVKeyIME_Request_v1`/`EVKeyIME_Response_v1`; DLL chỉ mở mapping/events, còn mutex `EVKeyIME_Mutex_v1` thì mở **hoặc tự tạo nếu chưa có** (`EVKeyIME_Mutex_v1` chỉ tồn tại trong DLL, không trong EXE). Adapter legacy MUST tạo mutex khi thiếu và MUST xử lý được **cả hai thứ tự** (controller trước hay TIP trước) ([`research/evkey64_exe.md:180-184`](research/evkey64_exe.md#L180-L184), [`research/evkime_dlls.md:92`](research/evkime_dlls.md#L92), [`artifacts/evkime_pseudocode.md:100-103`](artifacts/evkime_pseudocode.md#L100-L103), x64 `0x140035850`).

Các trường 4 byte MUST được đối xử như **raw u32/bytes**: signedness không có trong bằng chứng nguồn, nên MUST validate range thay vì phụ thuộc một cách diễn giải `i32` có dấu. Riêng đơn vị của `0x18` ("số characters/backspaces") chưa được bằng chứng nêu tường minh; cách đọc "UTF-16 code unit" là **Proposed** (suy ra từ ngữ nghĩa `cch` của TSF), không phải phát biểu để tương thích wire.

Adapter MUST hỗ trợ op `1=process character`, `2=backspace`, `3=reset`, `5=connect/sync`; MUST lock mutex tối đa 100 ms, drain stale response, tăng `+0x10`, signal request, chờ response tối đa 50 ms và chỉ nhận khi `+0x14` khớp sequence ([`research/evkime_dlls.md:81-132`](research/evkime_dlls.md#L81-L132), x64 `0x180005470`, `0x180005550`).

Trên timeout, sequence mismatch, length lẻ/quá lớn, stale epoch hoặc handle mất: MUST không áp shared output; MUST release mutex nếu sở hữu; SHOULD pass-through event gốc và reset context; MAY reconnect với backoff. Không retry cùng transaction theo cách có thể double-apply.

### 6.1 TSF TIP identity, đăng ký và migration

Danh tính TIP là **hợp đồng tương thích quan sát được** và MUST được bảo toàn đúng giá trị ([`research/evkey64_exe.md:176-186`](research/evkey64_exe.md#L176-L186)):

- CLSID: `{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}`.
- Profile GUID: `{F74F81AF-6F78-42C7-B006-2630C950B9A2}`.
- COM in-proc path: `Software\Classes\CLSID\{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}\InprocServer32`, đọc dưới **HKLM** cho cả hai registry view WOW64 (`KEY_WOW64_64KEY=0x100`, `KEY_WOW64_32KEY=0x200`).
- Đăng ký/unregister in-proc server bằng `%SystemRoot%\System32\regsvr32.exe` hoặc `%SystemRoot%\SysWOW64\regsvr32.exe` với `/s`, `/s /u`, và `runas` khi cần elevation.
- Cài profile TIP bằng `input.dll!InstallLayoutOrTip` với profile string `%04X:{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}{F74F81AF-6F78-42C7-B006-2630C950B9A2}`; MUST giữ đúng định dạng profile này thay vì tự thiết kế format mới.
- Mode nội bộ `-ineime` và `-r -ineime` (đăng ký/restart IME) MUST có đường tương đương, hoặc bị bỏ **có chủ ý và có ghi nhận**, không silent.

Yêu cầu đăng ký/migration:

- `TsfAdapter` MUST đăng ký TIP với đúng CLSID + profile GUID + category/capability (keyboard TIP cùng các `TIPCAP_*` mà DLL registration khai báo: `TIPCAP_UIELEMENTENABLED`, `TIPCAP_SECUREMODE`, `TIPCAP_IMMERSIVESUPPORT`, `TIPCAP_SYSTRAYSUPPORT`, `TIPCAP_INPUTMODECOMPARTMENT`, `TIPCAP_COMLESS`) để Windows TSF nhận diện parity ([`research/evkime_dlls.md:22-46`](research/evkime_dlls.md#L22-L46)).
- MUST đăng ký đúng registry **view theo bitness** của DLL (64-bit vào view 64, 32-bit vào view 32).
- Migration MUST: kiểm tra registration cũ trước; chỉ unregister đúng CLSID của mình (không phá TIP khác); idempotent khi chạy lại; rollback nếu bước sau fail (thứ tự register COM → install profile → verify); không để lại profile trùng khi profile GUID đổi.
- SHOULD có bước **verify** đọc lại registry và trạng thái `InstallLayoutOrTip` trước khi báo thành công; MUST NOT coi exit code của `regsvr32` là đủ.

## 7. Threading và sở hữu state

- Mỗi TSF activation/document session MUST có actor/serial executor riêng; chỉ executor đó mutate `EngineState` và COM composition.
- Config/Macro snapshot MUST immutable trong một request; update tạo revision mới.
- Legacy IPC adapter MUST serialize toàn cục bằng named mutex vì chỉ có một request slot; typed API mới SHOULD multiplex theo `session_id`.
- UI thread không được block trên engine/network; IPC legacy bị chặn tối đa theo timeout nêu trên.
- Shutdown MUST ngừng nhận request, signal worker, chờ hữu hạn, end composition, unadvise sink, unhook mouse, unmap/close; binary controller dùng timeout 2.000 ms ([`research/evkey64_exe.md:180-186`](research/evkey64_exe.md#L180-L186), `0x1400357a0`).

## 8. Lỗi, bảo mật và quyền riêng tư

- MUST validate mọi length/offset/sequence trước đọc/ghi shared memory.
- MUST dùng ACL theo user/session cho named objects trong implementation mới; security descriptor legacy là Unknown ([`research/evkime_dlls.md:130-132`](research/evkime_dlls.md#L130-L132)).
- MUST bypass/log tối thiểu trong password/secure fields; không lưu raw keystroke, clipboard text hoặc macro expansion vào telemetry mặc định.
- MUST không ghi debug log vào path cố định; DLL quan sát có ghi host name vào `D:\...` hoặc `%TEMP%` ([`research/evkime_dlls.md:35-46`](research/evkime_dlls.md#L35-L46), x64 `0x180001130`), đây là lựa chọn **không sao chép**.
- Clipboard transport SHOULD opt-in và restore clipboard an toàn; không dùng trong secure field.
- Update MUST nằm ngoài engine; verify nguồn, hash/signature theo trust policy trước install. Không suy đoán protocol của `evkau.exe`, vì updater vắng mặt ([`research/evkey64_exe.md:150-170`](research/evkey64_exe.md#L150-L170), `0x140017480`, `0x1400178e0`).

## 9. Migration

1. Discover registry `Software\MyQ\EVKey`, file `setting.ini`/`setting.tree`, macro `evkmacro.txt`, app exceptions; chỉ đọc, không delete nguồn trong lần đầu ([`research/evkey64_exe.md:69-95`](research/evkey64_exe.md#L69-L95), `0x14000b310`).
2. Parse vào `VersionedConfig` bằng field-level validation; unknown field giữ trong extension map.
3. Normalize macro trigger/replacement theo NFC nhưng lưu cả raw source cho rollback.
4. Commit store mới atomic, đọc lại và so revision/checksum.
5. Chỉ đánh dấu migrated sau verify; rollback không mất dữ liệu.
6. `TsfCache` là compatibility hint, không phải chân lý; MAY re-probe theo version ([`research/evkime_dlls.md:189-217`](research/evkime_dlls.md#L189-L217)).

## 10. MUST / SHOULD / MAY tổng hợp

### MUST

- Tách engine khỏi TSF/UI; transaction đúng năm field đã định nghĩa.
- TSF edit atomically; sentinel replay; mouse/focus reset; modifier/AltGr không bị nuốt.
- Đăng ký TSF TIP đúng CLSID/profile GUID/category quan sát, đúng WOW64 view, và migrate an toàn (idempotent, rollback, không phá TIP khác) — chi tiết §6.1.
- Legacy adapter đúng offsets/opcodes/names/timeouts/sequence và parity x86/x64/ARM64.
- Versioned state/config, ownership serial, validation, rollback, secure-field bypass.
- Không khẳng định engine EVKey là UniKey và không phụ thuộc decompiler-derived source structure.

### SHOULD

- Telex/VNI/VIQR, `qu/gi`, tone reposition, duplicate undo, spell/invalid restore, macro boundary/case theo baseline có citation.
- **Proposed** default NFC (EVKey chưa xác lập default; §3.2 giữ "Exact default là Unknown"), NFD/Preserve testable; per-app policy và cache re-probe.
- Startup non-admin mặc định; elevation explicit, least privilege.

### MAY

- Rust/C++/C#/Java hoặc ngôn ngữ khác; SQLite/JSON/registry cho store nếu giữ contract.
- IPC mới dùng named pipe/RPC với authentication, framing, cancellation và client ID.
- Clipboard/synthetic-key fallback theo policy, không phải engine API.

## 11. Không sao chép và ranh giới pháp lý

Không sao chép: global single-slot làm internal API, fixed debug log, unversioned raw structs, implicit pointer/handle sharing, UI-coupled engine, hard-coded process tables như một denylist duy nhất, hoặc update không xác minh. Danh sách process trong EXE là nhiều nhóm có semantics chưa rõ, không được flatten ([`research/evkey64_exe.md:215-240`](research/evkey64_exe.md#L215-L240)).

Nhóm clean-room spec chỉ phát hành interface/behavior/test vectors có provenance; nhóm implementation không nhận binary, disassembly, pseudocode hay decompiler output. Mọi hành vi chưa chứng minh mang nhãn baseline/proposed. Trước khi dùng mã UniKey, **phải tham khảo license thực tế trong repository tại commit đang dùng**; tài liệu này không khẳng định một license cụ thể ngoài những file repository đã quan sát.
