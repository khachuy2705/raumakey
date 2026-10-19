# Fix vòng 1 — hồ sơ clean-room reconstruction EVKey 6.0.5.0

- Người sửa: subagent fix (`round1`), sau review độc lập tại [`review_round1.md`](review_round1.md).
- Phạm vi: `README.md`, `EVKey_reconstruction_report.md`, `clean_room_spec.md`, `compatibility_test_matrix.md`, `diagrams/evkey_architecture.html`, `evidence/artifact_manifest.json`.
- Nguyên tắc: **không chạy bất kỳ binary EVKey nào** (không execute, không Wine, không dynamic), chỉ sửa văn bản/HTML/JSON và chạy công cụ kiểm tra.
- `research/*.md` (4 báo cáo nguồn) và `artifacts/*` **không bị sửa**; chúng vẫn là bằng chứng gốc.
- `review_round1.md` được giữ nguyên làm bằng chứng review.

## 0. Tổng hợp 16 phát hiện

| ID | Mức | Trạng thái | Tóm tắt thay đổi |
|---|---|---|---|
| B1 | Blocking | Fixed | `;` → `,` trong nhãn message sequence §6 |
| B2 | Blocking | Fixed | `;` → `,` trong nhãn message sequence §7 |
| M1 | Major | Fixed | Tách "vị trí engine" khỏi `**Quan sát.**` thành câu ranh giới riêng |
| M2 | Major | Fixed | Header cột §17 → `EVKey (observed / inferred)` + nhãn `(Observed)`/`(Inferred)` trong ô |
| M3 | Major | Fixed | Test 8 viết lại: `\`+`'` liền nhau, final literal `'`, không space |
| M4 | Major | Fixed | Thêm `§14.1 Đăng ký COM/TSF phía controller (Observed)` + `clean_room_spec.md §6.1` + MUST bullet |
| m1 | Minor | Fixed | Nêu rõ controller tạo mapping + 2 event; DLL mở/tạo mutex; adapter chịu cả hai thứ tự |
| m2 | Minor | Fixed | "CFG không bật ở bất kỳ file nào; cookie và GuardFlags x86 không đồng đều" |
| m3 | Minor | Fixed | `0x18` = 4 byte observed; đơn vị UTF-16 code unit = Proposed (từ `cch` TSF); sửa `EditTransaction` + test 53 |
| m4 | Minor | Fixed | NFC default → **Proposed**, không phải default EVKey |
| m5 | Minor | Fixed | Hạ/nhãn lại bằng chứng test 18, 25, 58 |
| m6 | Minor | Fixed | Test 48 thêm citation `unikey/keyhook/vietkey.cpp:967-1017` |
| m7 | Minor | Fixed | 3 export Ghidra vào README + `restricted_evidence` trong manifest (size/SHA-256 + cảnh báo phân phối) |
| m8 | Minor | Fixed | HTML: style/legend `Proposed` riêng, áp cho 4 box đề xuất; đổi tên legend rose → `TSF edit / guard path` |
| m9 | Minor | Fixed | Bỏ khẳng định signedness cho trường 4 byte: raw u32 + range check, ghi signedness Unknown |
| m10 | Minor | Fixed | `\n` → `<br/>` trong flowchart Mermaid |

## 1. Chi tiết từng phát hiện

### B1 — `EVKey_reconstruction_report.md` (pre-fix dòng 109 → nay dòng 111)

- Trước: `TIP->>IPC: Open objects; op 5 synchronize`
- Sau: `TIP->>IPC: Open objects, op 5 synchronize`
- (`;` là dấu phân tách câu lệnh trong `sequenceDiagram` của Mermaid nên gây `PARSE_FAIL`.)

### B2 — `EVKey_reconstruction_report.md` (pre-fix dòng 132 → nay dòng 134)

- Trước: `TIP->>TIP: reject sentinel 0xEBCA1234; ToUnicode`
- Sau: `TIP->>TIP: reject sentinel 0xEBCA1234, ToUnicode`
- Dòng `op 1 {char, Shift, Caps, seq}` giữ nguyên (như review đã chứng minh: `{}` không phải nguyên nhân).

### M1 — `EVKey_reconstruction_report.md` §1 (nay dòng 7–9)

- Câu `**Quan sát.**` không còn chứa "engine phía server"; chỉ còn quan sát: controller có hidden window/hook/surfaces, DLL là COM TSF TIP, DLL gửi character/backspace/reset và nhận `backspaces + UTF-16 replacement`.
- Thêm đoạn `**Ranh giới quan sát / suy luận của câu trên.**` (dòng 9) nêu rõ:
  - **quan sát**: DLL gửi request + phía đối diện tồn tại — citation `research/evkime_dlls.md:130-132`, `research/evkime_dlls.md:269`;
  - **suy luận**: vị trí engine phía controller — không quan sát được trong `EVKey64.exe`.

### M2 — `EVKey_reconstruction_report.md` §17 (nay dòng 271–274)

- Header: `| Trục | EVKey quan sát | ... |` → `| Trục | EVKey (observed / inferred) | ... |`
- Ô `Integration`: thêm `(Observed)`.
- Ô `Engine location`: `controller-side sau IPC (Inferred); exact internals Unknown`.

### M3 — `compatibility_test_matrix.md` test 8 (nay dòng 24)

- Trước (input hiển thị `\\` + space + `'`, mơ hồ về số backslash và về space):
  `| 8 | `\\ ' ` | VIQR | backslash arm escape; apostrophe literal, final `'` (không dấu) | ... |`
- Sau: `| 8 | `\`+`'` — một backslash rồi apostrophe **liền nhau** (không có space giữa hai phím) | VIQR | escape tiêu thụ apostrophe ngay sau `\` → final text là `'` literal, không dấu; đúng 2 phím, không xen space | Baseline oracle: `unikey/keyhook/vietkey.cpp:415-443`; [...] |`
- Ký tự không đổi: **một** backslash theo ngay sau bởi **một** apostrophe; final text được phát biểu tường minh. Markdown escaping được kiểm chứng bằng cách đọc lại file (dòng 24 hiện chỉ chứa đúng một backslash trong mỗi code span).

### M4 — Đăng ký COM/TSF (mục mới)

`EVKey_reconstruction_report.md`:

- Thêm `### 14.1 Đăng ký COM/TSF phía controller (Observed)` (dòng 214) với: COM path `Software\Classes\CLSID\{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}\InprocServer32`, CLSID `{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}`, profile GUID `{F74F81AF-6F78-42C7-B006-2630C950B9A2}`, hai registry view `KEY_WOW64_64KEY=0x100`/`KEY_WOW64_32KEY=0x200` dưới HKLM, helper `%SystemRoot%\System32\regsvr32.exe` + `%SystemRoot%\SysWOW64\regsvr32.exe` với `/s`, `/s /u`, `runas`, `input.dll` + `InstallLayoutOrTip` với profile string `%04X:{CLSID}{GUID}`, và mode `-ineime` / `-r -ineime`.
- Citation: `research/evkey64_exe.md:176-177`, `:178`, `:179`, `:186`; địa chỉ `0x140035ff0`, `0x140076240`, `0x140076380-0x140076400`, `0x14003556e-0x1400355b7`.
- §6 (nay dòng 119) bổ sung: controller là phía đăng ký COM server và cài TIP profile, trỏ về §14.1 (giải quyết việc bước Mermaid `Register/Install TIP profile as configured` trước đây không có citation).

`clean_room_spec.md`:

- §2 module 6 `TsfAdapter` mở rộng: "COM TIP (đúng CLSID/profile GUID/category đã quan sát) … **và đăng ký/migration TIP identity-profile**".
- Thêm `### 6.1 TSF TIP identity, đăng ký và migration` (dòng 217) — yêu cầu chính xác: giữ đúng CLSID/profile GUID/COM path/2 WOW64 view; đăng ký qua `regsvr32` `/s`·`/u`·`runas`; cài profile qua `input.dll!InstallLayoutOrTip` với đúng format profile; `TIPCAP_*` parity; đúng registry view theo bitness; migration idempotent + rollback + không phá TIP khác (không để lại profile trùng khi GUID đổi); SHOULD verify sau khi cài, MUST NOT coi exit code `regsvr32` là đủ.
- §10 `MUST` thêm bullet tương ứng (dòng 267).

### m1 — Ownership mapping/events/mutex

- `EVKey_reconstruction_report.md` thêm `### 14.2 Named objects và ownership (Observed)` (dòng 226): controller tạo `EVKeyIME_SharedMem_v1` (`0x1038`) + `EVKeyIME_Request_v1` + `EVKeyIME_Response_v1`; DLL chỉ mở mapping/events; mutex `EVKeyIME_Mutex_v1` **opens or creates** nên DLL tự tạo nếu thiếu; adapter MUST chịu cả hai thứ tự khởi động.
- Citation: `research/evkey64_exe.md:180-184` (`0x140035850`), `research/evkime_dlls.md:92`, `artifacts/evkime_pseudocode.md:100-103`.
- `clean_room_spec.md` §6 (dòng 209) thêm đoạn ownership/thứ tự khởi động với cùng citation.

### m2 — CFG/cookie (report nay dòng 41)

- Trước: "nhưng CFG/cookie không đồng đều".
- Sau: "**CFG không được bật ở bất kỳ file nào** (không file nào set characteristic `GUARD_CF`); chỉ **cookie** (6/7 file, thiếu `x64/evkime64.dll`) và **`GuardFlags` của bản x86** (`CF_INSTRUMENTED`) là không đồng đều" + citation `research/pe_inventory.md:16`.

### m3 — Đơn vị trường wire `0x18`

- `clean_room_spec.md` §3.4 `EditTransaction.delete_before` (dòng 90): chú thích đổi thành **Proposed** ("đếm theo UTF-16 host-visible code unit, suy ra từ `cch`/range của TSF; bằng chứng wire chỉ nói '4 byte characters/backspaces'").
- `clean_room_spec.md` §6 bảng (dòng 198): cột `0x18` ghi rõ trường **4 byte**, đơn vị UTF-16 = **Proposed**, MUST validate range.
- `compatibility_test_matrix.md` test 53 (dòng 84): `delete_before` theo **UTF-16 host code unit (Proposed)**, không glyph; wire chỉ nói 4 byte characters/backspaces, signedness Unknown.
- `EVKey_reconstruction_report.md` §14.3 (dòng 246): ô `0x18` nêu "Quan sát là trường **4 byte**"; diễn giải UTF-16 code unit là **Proposed/suy luận**.

### m4 — NFC default (clean_room_spec §10 SHOULD, nay dòng 275)

- Trước: "NFC mặc định, NFD/Preserve testable…".
- Sau: "**Proposed** default NFC (EVKey chưa xác lập default; §3.2 giữ 'Exact default là Unknown'), NFD/Preserve testable…". Hết mâu thuẫn với §3.2.

### m5 — Nhãn bằng chứng test 18/25/58

- Test 18 (dòng 34): `Observed integration (decompiler-derived)` ghi **≤32**, stride **biểu kiến** `0x208`, "không phải public ABI"; "no OOB"/quy tắc evict là **Proposed/Inferred**.
- Test 25 (dòng 46): chỉ combo hard reset là **Observed** (help text/UI); "engine/context reset" và "text đã commit không đổi" là **Inferred/Proposed**.
- Test 58 (dòng 89): chỉ `TaskbarCreated` register/handle + restore tray icon là **Observed**; "mode không đổi" là **Inferred**.

### m6 — Test 48 (dòng 79)

- Thêm citation baseline: `unikey/keyhook/vietkey.cpp:967-1017` (macro chỉ chạy tại word stop/Enter) + link `research/unikey_baseline.md:303-313`; expectation nêu rõ "chỉ expand tại word boundary".

### m7 — Ghidra exports thành restricted evidence

- `README.md` thêm mục `### Bằng chứng hạn chế — decompiler output (không cấp cho nhóm implementation)` (dòng 23) với bảng size + SHA-256 của 3 file và cảnh báo phân phối.
- `evidence/artifact_manifest.json` thêm nhóm `restricted_evidence` (role `decompiler-output-for-address-level-follow-up-only`, `not_an_implementation_input: true`, `distribution_note_vi`, 3 file kèm `size_bytes`/`sha256`/`scope`/`format`).
- Cả hai nơi nêu rõ: nhóm implementation clean-room MUST NOT nhận các file này; không đưa vào spec/test; không sao chép control flow.

### m8 — HTML Proposed style + legend

- Thêm token `--fuchsia:#e879f9` và class `.proposed{fill:rgba(74,22,92,.5);stroke:var(--fuchsia);stroke-dasharray:6 4}`.
- Áp `.proposed` cho 4 box đề xuất: `LegacyIpcAdapter (Proposed)`, `VietnameseEngine (Proposed)`, `ConfigStore / MacroStore` (+ nhãn `(Proposed)`), `Policy (Proposed)`.
- Legend thêm mục `Proposed component (clean-room)` (swatch đứt nét fuchsia) và đổi tên rose từ `Guard / reset boundary` → **`TSF edit / guard path`**; mục `Controller / Engine` → `Controller (observed)`.
- Ghi chú dưới sơ đồ cập nhật để giải thích ký hiệu mới.
- Bất biến giữ nguyên: arrow vẽ **trước** box (opaque mask), legend nằm **ngoài** cả ba boundary.

### m9 — Signedness trường 4 byte

- `clean_room_spec.md` §6 bảng (dòng 196–199): `i32` → `u32` cho `0x10/0x14/0x18/0x1c`, mỗi ô ghi **`signedness Unknown`**; thêm đoạn giải thích MUST giữ raw u32/bytes + validate range.
- `EVKey_reconstruction_report.md` §14.3 (dòng 234, 244–247): tiêu đề nêu `signedness Unknown`, các ô `0x10/0x14/0x1c` ghi raw u32.
- `compatibility_test_matrix.md` test 35 (dòng 61): "âm" → "có bit cao set", expect nêu rõ không diễn giải theo signedness.
- Ghi chú: `signedness` vẫn xuất hiện như nhãn **Unknown** (đúng yêu cầu) trong các dòng trên.

### m10 — `\n` → `<br/>`

- Trong flowchart §4: `evkime COM TIP<br/>per activation/thread`, `EVKeyIME_SharedMem_v1<br/>0x1038`, `Request/Response events<br/>EVKeyIME_Mutex_v1`, `evkau.exe<br/>external updater, absent`.
- Kiểm tra: không còn chuỗi `\n` literal trong block Mermaid nào của report.

## 2. Validation (chạy sau khi sửa)

### V1 — JSON

```sh
python3 -m json.tool evidence/artifact_manifest.json
```
→ `VALID JSON` (exit 0). `restricted_evidence.files` = 3.

### V2 — Link tương đối

```
links checked: 156   broken: []
```
(README/report/spec/matrix; bỏ link `http…` và anchor thuần.)

### V3 — Token cấm

```
occurrences: 0 (mọi token placeholder bị cấm) — quét README, report, spec, matrix, HTML
```
(Phạm vi quét là sáu deliverable; file bằng chứng `review_round1.md` nói *về* các token này nên được giữ nguyên và không nằm trong phạm vi đó.)

### V4 — Ma trận test

```
rows: 58   sequence 1..58: True   A/B/C/D: 20 9 14 15
```

### V5 — Contract bắt buộc

```
contracts: {'InputEvent': 5, 'EngineOptions': 3, 'EngineState': 8, 'EditTransaction': 4,
            'HostContext': 6, 'CompatibilityPolicy': 2, 'ConfigStore': 3, 'MacroStore': 3}
EditTransaction fields: {'delete_before': True, 'insert_utf16': True, 'consume_original': True,
                         'replay_key': True, 'reset_context': True}
```

### V6 — HTML thuần + hình học sơ đồ

```
<script>: 0 | on*=: 0 | http(s)://: 0 | @import: 0 | <link>: 0
arrows drawn before boxes: True | boxes: 13
proposed boxes: 4 | outside-boundary boxes (updater): [(915, 715, 290, 70, 'ext')]
legend outside boundaries: True | centered labels inside boxes: True
```

### V7 — Mermaid (browser thật, mermaid 11.17.2 cài local, `node_modules/mermaid/dist/mermaid.min.js`)

Trang kiểm tra: `/root/.hermes/cache/scratch/mermaid_r1_check.html` (4 block trích **trực tiếp** từ `EVKey_reconstruction_report.md`).

```
TITLE: DONE 0:PARSE_OK/RENDER_OK ; 1:PARSE_OK/RENDER_OK ; 2:PARSE_OK/RENDER_OK ; 3:PARSE_OK/RENDER_OK
[
 {index:0, parse:"PARSE_OK", render:"RENDER_OK", svg_len:27894, has_foreignObject:true,  literal_br:false, literal_backslash_n:false},
 {index:1, parse:"PARSE_OK", render:"RENDER_OK", svg_len:30886, has_foreignObject:false, literal_br:false, literal_backslash_n:false},
 {index:2, parse:"PARSE_OK", render:"RENDER_OK", svg_len:30491, has_foreignObject:false, literal_br:false, literal_backslash_n:false},
 {index:3, parse:"PARSE_OK", render:"RENDER_OK", svg_len:38093, has_foreignObject:true,  literal_br:false, literal_backslash_n:false}
]
```

- Block 1 và 2 (hai `sequenceDiagram` từng `PARSE_FAIL`) nay `PARSE_OK`/`RENDER_OK`.
- `literal_br:false` và `literal_backslash_n:false` chứng minh `<br/>` được render thành xuống dòng thật (không hiện literal), và không còn `\n` literal.
- `research/unikey_baseline.md` (3 block) không bị sửa; review vòng 1 đã xác nhận `PARSE_OK` và fix lần này không chạm file đó.

### V8 — Citation in-range (sau khi thêm citation mới)

- Citation trỏ file trong dossier: **151**, out-of-range **0**.
- Citation trỏ UniKey checkout (full path `unikey/…`, `x-unikey/…`): **27**, out-of-range **0**.
- Citation shorthand (`vietkey.cpp:…`, `ukengine.cpp:…`, `encode.cpp:…`): **19**, out-of-range **0** (resolve theo basename trong `/root/.hermes/cache/scratch/unikey-source`).
- Tổng **197** citation, **0** lỗi.

### V9 — Screenshot render

- `file:///root/evkey_reconstruction/diagrams/evkey_architecture.html`: svg 1180×805 CSS px, `scripts:0`, legend hiển thị đủ 6 mục gồm `Proposed component (clean-room)` và `TSF edit / guard path`, 4 box `(Proposed)` có viền đứt nét fuchsia, arrow nằm sau box, legend nằm ngoài boundary.

### V10 — Hash sau sửa

| File | Bytes | SHA-256 |
|---|---:|---|
| `README.md` | 5978 | `da08a5ac1a1ef46a0f1ac3c19d1456270e0bc28339e8e24744d591a4510c23db` |
| `EVKey_reconstruction_report.md` | 30482 | `b507bedb4990870f2b4e14e72de05b7bb38d2dbc6577ab11e7726ec94ff312fa` |
| `clean_room_spec.md` | 19794 | `630bc201dcc6c811d08736b07cbb3862e775ceac04973a1a2aad938f1f9e9afb` |
| `compatibility_test_matrix.md` | 15421 | `44a37eb47c62a6c9b655f2bb22903c4fb088e1bb9c31501017ccd2727f043379` |
| `diagrams/evkey_architecture.html` | 13179 | `39b68bf6e270677dbfe4c5f569bf4e7b9f20a1cb567b4467021b13bba15eb0ac` |
| `evidence/artifact_manifest.json` | 4421 | `dc87f324e84b1d29d70adfd09a0068660846c06f208670c6976bf2157fa5c676` |

`research/*.md` và `artifacts/*` giữ nguyên hash đã ghi trong review vòng 1 (không sửa).

## 3. Ghi chú

1. **Line hash**: các deliverable **không nhúng hash theo dòng**; citation chỉ là `file:dòng-dòng`. Vì vậy không có bảng line-hash nào cần đồng bộ. Manifest ghi rõ `analysis.line_hashes_embedded=false` kèm ghi chú.
2. Do chèn nội dung, số dòng của `EVKey_reconstruction_report.md`, `clean_room_spec.md`, `README.md` đã tăng; các citation **nội bộ dossier** trong văn bản vẫn trỏ đúng file nguồn `research/*` (không phụ thuộc số dòng của deliverable). Riêng tham chiếu chéo `clean_room_spec.md:288` trong README đã được cập nhật khớp vị trí mới.
3. `evidence/review_round1.md` giữ nguyên (không sửa) và được liệt kê trong `evidence_notes` của manifest cùng `evidence/fix_round1.md`.
4. Không thay đổi nào đối với `research/`, `artifacts/`, hay bất kỳ binary nào; không chạy EVKey.
