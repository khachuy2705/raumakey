# Review độc lập vòng 1 — hồ sơ reconstruction EVKey 6.0.5.0

- Người review: subagent độc lập (`deleg_8afa7fb6`), không sửa bất kỳ deliverable nào. Chỉ tạo file này.
- Phạm vi đọc: `README.md`, `EVKey_reconstruction_report.md`, `clean_room_spec.md`, `compatibility_test_matrix.md`, `diagrams/evkey_architecture.html`, `evidence/artifact_manifest.json`, `research/*.md` (4 báo cáo nguồn), `artifacts/evkime_pseudocode.md`.
- Phương pháp: đối chiếu từng yêu cầu gốc (brief tổng hợp: 6 output, 8 contract, ma trận test, sơ đồ Mermaid, manifest JSON, nhãn observed/inferred/unknown, `evkau.exe`, không claim engine internals, không claim Authenticode) → kiểm tra citation theo dòng → kiểm tra lại **độc lập** trên artifact gốc (hash, PE, string, Ghidra export) → render Mermaid/HTML thật trên browser.
- Không chạy bất kỳ binary nào của EVKey; không sửa file deliverable.

## 0. Tổng kết

| Mức | Số lượng | ID |
|---|---:|---|
| **Blocking** | 2 | B1, B2 |
| **Major** | 4 | M1, M2, M3, M4 |
| **Minor** | 10 | m1–m10 |

Không phát hiện: sai hash/size, sai tên updater (`evkau.exe` đúng ở cả 4 file), link tương đối hỏng, JSON lỗi, placeholder TBD/TODO, JS/external dependency trong HTML, overclaim license, claim đã xác minh Authenticode, hay claim exact engine internals EVKey. Chi tiết ở §3.

---

## 1. Blocking

### B1 — Hai sơ đồ Mermaid bắt buộc không render (semicolon trong message)

- **File:line**: `EVKey_reconstruction_report.md:109` (trong block Mermaid dòng 94–115, sơ đồ startup §6).
- **Nội dung**: ```TIP->>IPC: Open objects; op 5 synchronize```
- **Bằng chứng**: render bằng mermaid v11.12 trong browser thật (Chromium, page `file:///root/.hermes/cache/scratch/mermaid_check.html`, 4 block của report):
  ```
  BLOCK 0: PARSE_OK          (flowchart §4)
  BLOCK 1: PARSE_FAIL: Parse error on line 15: | ...ts; op 5 synchronize  EXE->>W: Message  | ... got 'NEWLINE'
  BLOCK 2: PARSE_FAIL: Parse error on line 9:  | ...xEBCA1234; ToUnicode  TIP->>IPC: op 1 { | ... got 'NEWLINE'
  BLOCK 3: PARSE_OK          (stateDiagram-v2 §8)
  RENDER_FAIL: (dừng ở block 1)
  ```
  Mermaid coi `;` là dấu phân tách câu lệnh trong `sequenceDiagram`, nên phần sau `;` bị parse thành câu lệnh mới → lỗi. Trên GitHub/VS Code/mermaid.live hai sơ đồ này chỉ hiện hộp lỗi.
- **Đề xuất sửa**: đổi `;` thành `,` (đã kiểm chứng bằng thực nghiệm: `v1_semicolon_comma: PARSE_OK`, `v2a_semicolon_only: PARSE_OK` — tức chỉ cần sửa `;`, giữ nguyên `{char, Shift, Caps, seq}` vẫn parse OK; ngược lại `v2c_braces_only` (giữ `;`) vẫn `PARSE_FAIL`). Ghi chú tại chỗ: không dùng `;` trong nhãn message của `sequenceDiagram`.

### B2 — Cùng lỗi tại sơ đồ luồng gõ tiếng Việt

- **File:line**: `EVKey_reconstruction_report.md:132` (trong block Mermaid dòng 123–143, sơ đồ keystroke §7).
- **Nội dung**: ```TIP->>TIP: reject sentinel 0xEBCA1234; ToUnicode```
- **Bằng chứng**: như B1 (`BLOCK 2: PARSE_FAIL ... line 9`).
- **Đề xuất sửa**: `TIP->>TIP: reject sentinel 0xEBCA1234, ToUnicode`. Lưu ý dòng 133 (`op 1 {char, Shift, Caps, seq}`) **không** phải nguyên nhân: sau khi sửa dấu `;`, biến thể còn nguyên `{}` vẫn `PARSE_OK`.

---

## 2. Major

### M1 — Nhãn "Quan sát" bao gồm suy luận về vị trí engine phía controller

- **File:line**: `EVKey_reconstruction_report.md:7` (đoạn mở đầu bằng `**Quan sát.**`), mâu thuẫn với `EVKey_reconstruction_report.md:9` (`**Suy luận có giới hạn.** Engine tiếng Việt nằm phía controller sau cầu IPC...`) và `diagrams/evkey_architecture.html:58` (đưa "Engine tiếng Việt thuộc phía controller" vào card **Suy luận**).
- **Bằng chứng**: bằng chứng nguồn chỉ chứng minh phía DLL *gửi request* và phía đối diện *tồn tại*: `research/evkime_dlls.md:130` ("the server/controller implementing the other side is outside these DLLs") và `research/evkime_dlls.md:269` ("not directly present in the analyzed DLL code"). Không có dòng nào trong 4 báo cáo nguồn quan sát được engine nằm trong `EVKey64.exe`.
- **Đề xuất sửa**: chuyển vế "và engine phía server" ra khỏi câu `**Quan sát.**` (ví dụ tách thành câu `**Suy luận.**` riêng), hoặc viết "controller phía server nhận character/backspace (observed) — vị trí engine là suy luận".

### M2 — Cột "EVKey quan sát" trong bảng §17 chứa nội dung suy luận

- **File:line**: `EVKey_reconstruction_report.md:247` (header `| Trục | EVKey quan sát | UniKey baseline | Kết luận rewrite |`) và ô tương ứng `EVKey_reconstruction_report.md:250` (`| Engine location | controller-side sau IPC; exact internals Unknown | ... |`).
- **Bằng chứng**: "controller-side" là suy luận (xem M1); ô `Integration` dòng 249 cũng gộp "LL hook trong EXE + COM TSF TIP + IPC" (observed) với vị trí engine (inferred) dưới cùng một cột.
- **Đề xuất sửa**: đổi header cột thành `EVKey (observed / inferred)` và thêm nhãn trong ô: `controller-side sau IPC (Inferred); internals Unknown`.

### M3 — Vector test 8 (VIQR escape) không thi hành được như viết

- **File:line**: `compatibility_test_matrix.md:24` — `| 8 | `\\ ' ` | VIQR | backslash arm escape; apostrophe literal, final `'` (không dấu) | Baseline oracle: `unikey/keyhook/vietkey.cpp:415-443` ...`
- **Bằng chứng**: input hiển thị là `\\` + space + `'`, nhưng quy tắc được viện dẫn (`research/unikey_baseline.md:230`: "Backslash được xem là escape trong VIQR/VIQR*") áp escape cho **ký tự ngay sau** backslash. Với vector này, ký tự ngay sau là space → escape tiêu thụ space (không phải `'`), và `'` sau đó không còn ở trạng thái escaped; đồng thời kết quả cuối chỉ ghi `'`, không nói gì về space đã phát. Vector mơ hồ về số lượng backslash và không thể derive final text một cách duy nhất.
- **Đề xuất sửa**: ghi rõ chuỗi phím là `\` rồi `'` liền nhau (`\'`), expected final = `'`; hoặc nếu cố ý test có space thì phải ghi final text đầy đủ (ví dụ `` ` '``) và lý do. Bổ sung cột "final text" thay vì chỉ chú thích.

### M4 — Thiếu toàn bộ phần đăng ký COM/TSF phía EXE mà bằng chứng đã có

- **File:line**: `EVKey_reconstruction_report.md:210-233` (mục §14 "TSF và IPC protocol") — không xuất hiện chuỗi `CLSID`, `{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}`, `regsvr32`, `InstallLayoutOrTip`, `input.dll`, `-ineime`; dòng `EVKey_reconstruction_report.md:106` chỉ có `EXE->>OS: Register/Install TIP profile as configured` (không citation).
- **Bằng chứng**: `research/evkey64_exe.md:176-186` ghi rõ: đường dẫn đăng ký COM `Software\Classes\CLSID\{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}\InprocServer32`, hàm `0x140035ff0` query HKLM với hai view WOW64, helper `regsvr32.exe`/`regsvr32.exe / SysWOW64` với `/s`, `/s /u`, `runas`, `input.dll` + `InstallLayoutOrTip` với profile `%04X:{0BAD015B-...}{F74F81AF-...}`, và các mode `-ineime`, `-r -ineime`. Kiểm tra độc lập string trong binary: tôi đã tìm thấy `%04X:{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}{F74F81AF-6F78-42C7-B006-2630C950B9A2}`, `Software\Classes\CLSID\{0BAD015B-...}\InprocServer32` trong `x64/EVKey64.exe`.
- **Vì sao major**: brief yêu cầu report có mục "TSF/IPC protocol" và clean-room spec yêu cầu giữ "COM TIP/profile/category" (`clean_room_spec.md:295`); thiếu CLSID + cơ chế cài profile TIP khiến nhóm implementation không thể đăng ký/di trú TIP tương đương, và không thể kiểm tra "parity TSF identity".
- **Đề xuất sửa**: thêm tiểu mục trong §14 "Đăng ký COM/TSF phía controller (Observed)" với CLSID, hai registry view, helper regsvr32/runas, `input.dll`+`InstallLayoutOrTip` profile string và `-ineime`; citation `research/evkey64_exe.md:176-186`, địa chỉ `0x140035ff0`, `0x14003556e-0x1400355b7`. Bổ sung bullet tương ứng trong `clean_room_spec.md` §2/§20.1 (TsfAdapter phải cài profile TIP).

---

## 3. Minor

| ID | File:line | Vấn đề | Bằng chứng | Đề xuất |
|---|---|---|---|---|
| m1 | `EVKey_reconstruction_report.md:212`, `clean_room_spec.md:207` | Liệt kê `EVKeyIME_Mutex_v1` chung với mapping/events, không nói bên nào **tạo** hay **mở**; trong khi §6 (`:104`) nói EXE tạo mapping + 2 event, không nhắc mutex | String `EVKeyIME_Mutex_v1` **chỉ có trong 4 DLL**, không có trong cả 3 EXE (kiểm tra string UTF-16/ASCII); `research/evkime_dlls.md:92` "opens or creates the mutex"; `artifacts/evkime_pseudocode.md:100-103` (OpenMutexW → nếu thiếu thì CreateMutexW) | Ghi rõ: controller tạo mapping + `EVKeyIME_Request_v1` + `EVKeyIME_Response_v1`; DLL tự tạo mutex nếu chưa có; adapter mới phải chịu được thứ tự này. Thêm citation trực tiếp cho dòng 212 |
| m2 | `EVKey_reconstruction_report.md:39` | "CFG/cookie không đồng đều" gộp hai thứ khác nhau | `research/pe_inventory.md:16`: **không file nào** set `GUARD_CF`; chỉ cookie (6/7, thiếu `x64/evkime64.dll`) và GuardFlags x86 (`CF_INSTRUMENTED`) là không đồng đều | Viết "cookie và GuardFlags x86 không đồng đều; CFG không bật ở mọi file" |
| m3 | `clean_room_spec.md:90`, `:198`, `compatibility_test_matrix.md:84` | Đơn vị của trường wire `0x18` được khẳng định là "đơn vị (UTF-16)" trong khi bằng chứng nguồn chỉ nói "number of characters/backspaces" | `research/evkime_dlls.md:112`; giải thích UTF-16 code unit chỉ **suy ra được** từ `ShiftStart(cch)` của TSF (`artifacts/evkime_pseudocode.md:221`) | Ghi chú rằng đơn vị của `0x18` chưa được evidence nêu tường minh; "UTF-16 code unit" là diễn giải Proposed (dựa trên ngữ nghĩa `cch` của TSF) |
| m4 | `clean_room_spec.md:250` vs `clean_room_spec.md:65` | §10 SHOULD "NFC mặc định" mâu thuẫn §3.2 "Exact default là Unknown ... không hard-code từ suy đoán" | `research/evkey64_exe.md:95` (defaults chưa phục hồi); test 52 (`compatibility_test_matrix.md:83`) đã ghi Proposed | Đổi thành "Proposed default NFC (chưa xác lập ở EVKey)" |
| m5 | `compatibility_test_matrix.md:34` (test 18), `:46` (test 25), `:89` (test 58) | Nhãn bằng chứng cao hơn thực tế cho một phần kỳ vọng | `research/evkime_dlls.md:159` ("apparent per-entry stride"), `report:182` tự nhắc "không nên biến thành public engine ABI"; test 25/58 chỉ có UI text và `TaskbarCreated` là observed, phần "text đã commit không đổi"/"mode không đổi" là suy luận | Tách phần suy luận sang Proposed/Inferred trong chính ô đó (test 18: "≤32, stride apparent — decompiler-derived") |
| m6 | `compatibility_test_matrix.md:79` (test 48) | Claim baseline "không expand khi trigger giữa word" nhưng không có file:line | Brief: mọi claim UniKey phải cite `file:line`; row 47/49 đều có (`vietkey.cpp:967-1039`) | Thêm citation `unikey/keyhook/vietkey.cpp:967-1017` |
| m7 | `evidence/artifact_manifest.json:44-55`, `README.md:21` | Ba export Ghidra lớn trong `artifacts/` (`evkime32_ghidra_all.c` 995.079 B, `evkime64_ghidra_all.c` 931.261 B, `evkimearm64_ghidra_all.c` 1.077.216 B) không được nhắc ở README/report/manifest | `ls -la artifacts/`; `clean_room_spec.md:263` nói nhóm implementation không được nhận decompiler output — nhưng các file này nằm ngay trong dossier | Thêm nhóm `restricted_evidence` trong manifest (kèm ghi chú phân phối), hoặc chuyển ra ngoài cây deliverable |
| m8 | `diagrams/evkey_architecture.html:53`, `:43-49` | Legend "Guard / reset boundary" (rose) được dùng cho box `TSF EditSession` và `Compatibility Contract`; các box `LegacyIpcAdapter`, `VietnameseEngine`, `ConfigStore/MacroStore`, `Policy` là **component đề xuất** nhưng nằm trong boundary "EVKEY CONTROLLER PROCESS" màu observed, chỉ có caption nhỏ "exact EVKey internals: UNKNOWN" | Ảnh render (viewport: legend nằm ngoài boundary, màu đúng, card hiển thị đủ); quan sát tương quan nhãn ↔ hình | Thêm entry legend "Proposed component (clean-room)" và style riêng (ví dụ stroke đứt) cho 4 box đề xuất; đổi tên nhãn legend rose thành "TSF edit/guard path" |
| m9 | `clean_room_spec.md:196-199` vs `EVKey_reconstruction_report.md:220-223` | Spec khai báo `i32` cho `0x10/0x14/0x18/0x1c`, report ghi "4 byte" không có signedness; signedness là giả định của decompiler | `research/evkime_dlls.md:110-113` (chỉ "4 bytes"); test 35 mới là nơi kiểm tra "length âm" | Ghi rõ signedness là giả định adapter (hoặc dùng `u32` + kiểm tra range Proposed) |
| m10 | `EVKey_reconstruction_report.md:43-72` | Nhãn dùng `\n` trong flowchart (ví dụ dòng 47, 51). Mermaid 11 render thành xuống dòng (đã kiểm chứng), nhưng nhiều renderer cũ (GitHub pinned) hiển thị literal `\n` | Render thực tế: `contains_literal_backslash_n=false` với mermaid 11 | Đổi `\n` → `<br/>` để an toàn đa renderer |

---

## 4. Những điểm đã kiểm tra và **đạt** (để parent biết độ phủ)

Đối chiếu yêu cầu gốc:

- **6 output bắt buộc**: đủ, đúng đường dẫn; `README.md` có navigation/scope/nhãn; `artifact_manifest.json` hợp lệ (`python3 -m json.tool` → OK) và là JSON thật.
- **8 contract bắt buộc** trong `clean_room_spec.md`: `InputEvent` (§3.1), `EngineOptions` (§3.2), `EngineState` (§3.3), `EditTransaction` (§3.4, đúng 5 field `delete_before/insert_utf16/consume_original/replay_key/reset_context`), `HostContext` (§3.5), `CompatibilityPolicy`/`ConfigStore`/`MacroStore` (§4). Đủ 8/8. Offsets `0x00..0x38`, opcode 1/2/3/5, timeout 100/50/2.000 ms có mặt và được đóng khung "compatibility-only" (`clean_room_spec.md:188`, `:207`).
- **Ma trận test**: đúng 58 dòng, số liên tục 1..58, chia A(20)/B(9)/C(14)/D(15); đủ các nhóm yêu cầu: Telex/VNI/VIQR (1-7), qu/gi (10,11), tone reposition (12), repeated mark undo (13,14), invalid-word restore (15,16), backspace (17,18), mouse reset (19), modifiers/AltGr (21,22), app policy (26-28), IPC timeout/sequence (31-34,36), TSF replay sentinel (39), config migration (44-46,56), macro boundary/case (47-51), Unicode normalization (52,53), x86/x64/ARM64 parity (42). Mỗi dòng có mức bằng chứng; các giá trị baseline/proposed được ghi rõ.
- **Citation**: 118 citation dạng `file:dòng-dòng` trong 4 deliverable — **tất cả** nằm trong phạm vi file nguồn và **khớp** với anchor `#Lx-Ly`; không có citation vượt số dòng (pe_inventory 309, evkey64_exe 292, evkime_dlls 304, unikey_baseline 486).
- **Link tương đối**: 0 link hỏng trong README/report/spec/matrix.
- **Tên updater**: `evkau.exe` đúng chính tả ở cả 4 nơi (report §4/§5/§16/§19, spec §8, matrix test 57, HTML updater box); không có token `evkmau.exe`; không có `TODO/TBD/FIXME/placeholder`.
- **Danh tính artifact**: SHA-256 và size của cả 7 PE khớp **thực tế** trên extraction; ZIP `59ac1cee…78bc`, 1.504.714 byte khớp file gốc (`sha256sum`, `stat -c%s`); ZIP chỉ chứa đúng 7 PE + 3 thư mục; hai `evkime32.dll` byte-identical; EXE có `6.0.5.0`, DLL **không** có VERSION resource (kiểm tra string UTF-16 độc lập).
- **Authenticode**: caveat đúng và nhất quán — cert leaf `CN=LamQuangMinh` self-signed (`openssl verify` → `error 18 ... self-signed certificate; verification failed`), manifest `authenticode_image_digest_verified=false`, không nơi nào claim đã xác minh digest/trust.
- **Mô hình TSF vs hook/injection**: đúng và có bằng chứng cứng — DLL export đúng 4 entry COM (`rabin2 -E`: `DllCanUnloadNow/DllGetClassObject/DllRegisterServer/DllUnregisterServer`), không có `MyKeyHook/InitSharedMem/GetSharedMem/SetKeyMode`; EXE **không** import `WriteProcessMemory/CreateRemoteThread/VirtualAllocEx/NtMapViewOfSection` (chỉ `LoadLibraryW`, `SetWindowsHookExW`).
- **Địa chỉ chủ chốt kiểm chứng lại độc lập**: `x64/EVKey64.exe` cài hook bằng `ecx = r9 + 0xd` (=13, `WH_KEYBOARD_LL`) với HOOKPROC `0x140031d70`, `dwThreadId=0` (`artifacts/evkey64_pseudocode.md:4080-4090`) → claim §4/E3 đúng; DLL x64 `SetWindowsHookExW(7, FUN_180001bf0, NULL, GetCurrentThreadId())` (`artifacts/evkime64_ghidra_all.c:1211`) → `WH_MOUSE` current-thread đúng; mapping `0x1038` (`evkey64_pseudocode.md:4735/4748/4759`, `evkime_pseudocode.md:95/149`); shutdown `WaitForSingleObject ... 0x7d0` (2000 ms, `evkey64_pseudocode.md:4888/8040`); string `\evkau.exe` + `str.evkau.exe` trong `0x140017480`; sentinel `0xEBCA1234` có mặt dạng immediate trong DLL x64/x86 (EXE: 0, đúng như thiết kế DLL-side); 9 tên app đặc biệt và `TsfCache\v%d` đúng trong `evkime64.dll`; log `D:\evkime_dll_log.txt` đúng.
- **UniKey baseline**: repo `https://github.com/hochanh/unikey-source.git`, commit `e3b8f3b53bdaa700945a62e99129df625509cd38` khớp checkout thật (`git rev-parse HEAD`).
- **License**: không overclaim — cả 4 nơi chỉ yêu cầu "tham khảo license thực tế tại repository/commit", manifest ghi `license_note_vi`, không khẳng định GPL/MIT/copyright cụ thể.
- **HTML**: 0 `<script>`, 0 `on*=` handler, 0 URL `http(s)`, 0 `@import`/`<link>` ngoài; CSS inline trong `<style>`; `.legend` có định nghĩa; `viewBox 0 0 1320 900`, svg rộng 1180 CSS px; arrow được vẽ **trước** box (dòng 26-34 trước 36-51) → box đè arrow đúng như chú thích dòng 54; legend (translate 45,725) và updater box (915-1205 × 715-785) đều **ngoài** cả ba boundary (host 30-375×55-660, bridge 420-780×55-660, controller 825-1290×55-660); 3 card Quan sát/Suy luận/Chưa rõ hiển thị đúng; ảnh render xác nhận không chồng lấn.
- **Mermaid khác**: flowchart §4 và stateDiagram-v2 §8 `PARSE_OK`; 3 block trong `research/unikey_baseline.md` (nguồn) cũng `PARSE_OK`.

## 5. Lệnh/kiểm tra đã chạy (bằng chứng)

```sh
# 1) Danh tính
sha256sum /root/.hermes/cache/documents/doc_934bc563bd94_EVKey.zip   # 59ac1cee...78bc
find /root/.hermes/cache/scratch/rea-extracted-5943e685-.../ -type f -exec sha256sum {} \;   # 7/7 khớp report §3
python3 -m json.tool evidence/artifact_manifest.json >/dev/null && echo VALID JSON          # VALID JSON
git -C /root/.hermes/cache/scratch/unikey-source rev-parse HEAD      # e3b8f3b53bdaa700945a62e99129df625509cd38

# 2) Citation / link / token
# 118 citation file:dòng-dòng → tất cả trong phạm vi, anchor #Lx-Ly khớp, 0 lỗi
# 0 link tương đối hỏng; 0 lần xuất hiện 'evkmau'; 0 TODO/TBD/FIXME/placeholder

# 3) Mermaid (browser thật, mermaid v11 ESM từ CDN, file trung gian ở /root/.hermes/cache/scratch/)
#    mermaid_check.html / mermaid_variants.html / mermaid_flow.html / mermaid_check2.html
#    → B1/B2 PARSE_FAIL; sau khi đổi ';'→',': PARSE_OK; flowchart/stateDiagram OK

# 4) HTML
#    grep: 0 <script>, 0 on*=, 0 http(s) URL; .legend có trong CSS; screenshot viewport + scroll-bottom OK

# 5) Binary (độc lập)
rabin2 -E x64/evkime64.dll        # 4 export COM, không có hook/engine export
rabin2 -ij x64/EVKey64.exe        # không có primitive injection
grep -n "SetWindowsHookExW" artifacts/evkime64_ghidra_all.c   # SetWindowsHookExW(7, FUN_180001bf0, 0, GetCurrentThreadId())
sed -n '4080,4090p' artifacts/evkey64_pseudocode.md           # ecx = r9 + 0xd → WH_KEYBOARD_LL=13
openssl x509 -in x64_EVKey64.exe.pem -noout -subject -issuer  # CN=LamQuangMinh / CN=LamQuangMinh
openssl verify -purpose any x64_EVKey64.exe.pem               # error 18: self-signed certificate
# string scan: EVKeyIME_Mutex_v1 chỉ có trong DLL; EVKeyIME_{SharedMem,Request,Response}_v1 có ở cả EXE và DLL
```

## 6. Kết luận cho vòng sửa

Hồ sơ **đạt phần lớn yêu cầu gốc** (danh tính, citation, nhãn, 8 contract, 58 test, HTML tự chứa, JSON, caveat Authenticode, tên `evkau.exe`, không claim engine internals, không overclaim license). Trước khi bàn giao **bắt buộc** sửa B1, B2 (hai sơ đồ bắt buộc không render) và nên sửa M1–M4 (nhãn observed/inferred ở §1 và §17; vector test 8; bổ sung phần đăng ký COM/TSF). Nhóm minor m1–m10 là chỉnh sửa nhỏ, chủ yếu là chú thích nguồn gốc/đơn vị và tính đầy đủ của manifest.
