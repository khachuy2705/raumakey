# Review độc lập vòng 2 — sau fix vòng 1

- Phạm vi: đọc độc lập `evidence/review_round1.md`, `evidence/fix_round1.md`, sáu deliverable hiện tại, bốn báo cáo nguồn `research/*.md`, pseudocode chọn lọc và ba file restricted evidence.
- Nguyên tắc: không dựa vào trạng thái “Fixed” của fix summary; mỗi ID được đối chiếu lại với nội dung hiện tại và nguồn được trích dẫn.
- Không chạy bất kỳ EXE/DLL EVKey nào. Kiểm tra binary chỉ đọc bytes trực tiếp từ ZIP bằng Python.
- Không sửa deliverable. File duy nhất được tạo trong dossier ở vòng này là báo cáo review này.

## 0. Kết quả và verdict

| Hạng mục | Kết quả |
|---|---:|
| Finding vòng 1 được kiểm tra | 16 |
| PASS | **16** |
| FAIL | **0** |
| Finding mới Blocking | 0 |
| Finding mới Major | 0 |
| Finding mới Minor | **1** (`N1`) |

**Kết luận về finding vòng 1:** B1, B2, M1–M4 và m1–m10 đều đã đóng theo tiêu chí ban đầu.

**Release verdict: HOLD / NOT RELEASE-READY.** Có một regression hình ảnh mới mức **Minor** trong sơ đồ HTML: nhãn `Policy (Proposed)` tràn ra ngoài box. Vì acceptance yêu cầu không phát sinh regression, chưa thể xác nhận release sạch dù 16/16 finding cũ đều PASS.

---

## 1. PASS/FAIL cho toàn bộ 16 ID

| ID | Mức cũ | Kết quả | Bằng chứng độc lập vòng 2 |
|---|---|---|---|
| B1 | Blocking | **PASS** | `EVKey_reconstruction_report.md:111` dùng `Open objects, op 5 synchronize`; sequence diagram parse và render thành công trong Chromium với Mermaid 11.17.2. |
| B2 | Blocking | **PASS** | `EVKey_reconstruction_report.md:134` dùng `reject sentinel 0xEBCA1234, ToUnicode`; sequence diagram parse và render thành công. |
| M1 | Major | **PASS** | `report:7` chỉ mô tả DLL gửi request sang phía đối diện; `report:9` nói rõ vị trí engine là **suy luận**, không quan sát được; `report:11` dùng nhãn `Suy luận có giới hạn`. Khớp `research/evkime_dlls.md:130-132,269`. |
| M2 | Major | **PASS** | Header §17 là `EVKey (observed / inferred)`; dòng Integration có `(Observed)`, Engine location có `(Inferred)` và `exact internals Unknown` (`report:271-274`). |
| M3 | Major | **PASS** | Test 8 ghi một backslash rồi apostrophe liền nhau, đúng 2 phím, final text `'` literal. Parser ký tự xác nhận code span đầu chứa đúng **1** backslash, không có space. |
| M4 | Major | **PASS** | Report §14.1 và spec §6.1 có CLSID, profile GUID, HKLM + hai WOW64 views, `regsvr32` `/s`/`/u`/`runas`, `input.dll!InstallLayoutOrTip`, profile format, `-ineime`/`-r -ineime`, category/capability và yêu cầu migration/verify. Nội dung đối chiếu đúng `research/evkey64_exe.md:176-186` và `research/evkime_dlls.md:22-46`. |
| m1 | Minor | **PASS** | Report §14.2 và spec dòng 209 phân biệt controller tạo mapping/events, DLL mở chúng và mở/tạo mutex; adapter chịu hai thứ tự startup. Quét tĩnh ZIP xác nhận `EVKeyIME_Mutex_v1` UTF-16 có ở 4 DLL, không có ở 3 EXE; pseudocode dòng 100-103 có `OpenMutexW` rồi `CreateMutexW` khi thiếu. |
| m2 | Minor | **PASS** | `report:41` tách đúng: không PE nào set `GUARD_CF`; cookie và x86 `GuardFlags` mới không đồng đều. Khớp `research/pe_inventory.md:16,55-67`. |
| m3 | Minor | **PASS** | `0x18` chỉ được gọi là trường 4 byte ở mức Observed; đơn vị UTF-16 host code unit được gắn **Proposed/suy luận** trong report, spec và test 53. |
| m4 | Minor | **PASS** | Spec dòng 275 ghi `Proposed default NFC`, đồng thời nhắc EVKey default chưa xác lập; không còn mâu thuẫn với §3.2 dòng 65. |
| m5 | Minor | **PASS** | Test 18 ghi `≤32`, stride “biểu kiến”, không phải public ABI; no-OOB/eviction là Proposed/Inferred. Test 25 tách combo observed khỏi reset/text unchanged Inferred/Proposed. Test 58 tách `TaskbarCreated` observed khỏi “mode không đổi” Inferred. |
| m6 | Minor | **PASS** | Test 48 có citation `unikey/keyhook/vietkey.cpp:967-1017` và link báo cáo baseline; range hợp lệ trong checkout commit cố định. |
| m7 | Minor | **PASS** | README có bảng/cảnh báo restricted evidence; manifest có `restricted_evidence`, `not_an_implementation_input=true` và 3 file. Cả 3 size/SHA-256 khớp bytes hiện tại. |
| m8 | Minor | **PASS** | Bốn component đề xuất dùng viền đứt fuchsia và nhãn Proposed; legend có `Proposed component (clean-room)`, `Controller (observed)`, `TSF edit / guard path`. Chromium cho computed style đúng. Tuy nhiên fix này phát sinh regression hình ảnh mới N1 bên dưới. |
| m9 | Minor | **PASS** | Các trường `0x10/0x14/0x18/0x1c` dùng raw `u32`, ghi `signedness Unknown`; test 35 dùng “bit cao set” và range validation, không diễn giải giá trị âm. |
| m10 | Minor | **PASS** | Không còn literal `\n` trong bất kỳ Mermaid block nào; flowchart dùng `<br/>`. Chromium xác nhận 0/7 block hiển thị literal `<br/>` hoặc `\n`. |

---

## 2. Finding mới

### N1 — Minor — nhãn `Policy (Proposed)` tràn khỏi box

- **File:** `diagrams/evkey_architecture.html:48`.
- **Nguồn regression:** m8 thêm hậu tố `(Proposed)` nhưng box Policy vẫn rộng 105 SVG units.
- **Đo bằng API SVG trong Chromium:** box `x=1160`, `width=105`, tức biên `[1160,1265]`; bounding box của text là `x=1145.754`, `width=132.492`, tức biên `[1145.754,1278.246]`.
- **Mức tràn:** khoảng 14.25 units bên trái và 13.25 units bên phải. Không có hai component rectangle chồng nhau, nhưng title không còn nằm trong component box và gần chạm box `VietnameseEngine` bên trái.
- **Ảnh render thật:** xác nhận title Policy vượt khỏi viền fuchsia ở viewport Chromium.
- **Đề xuất:** tăng chiều rộng box Policy và điều chỉnh vị trí/khoảng cách, hoặc chia title thành hai dòng/giảm font; sau đó đo lại toàn bộ text bounding boxes và chụp lại screenshot.

Không phát hiện finding mới về nội dung kỹ thuật, citation, artifact identity, contract, test matrix, Mermaid parse/render, dependency HTML hay overclaim.

---

## 3. Regression validation đầy đủ

### 3.1 Mermaid — parse + render thật

Trích block bằng regex chương trình `re.findall(r"```mermaid\n(.*?)```", text, re.S)` trên toàn bộ Markdown trong dossier; tìm thấy **7** block:

- `EVKey_reconstruction_report.md`: 4 block.
- `research/unikey_baseline.md`: 3 block.

Dùng local pinned `mermaid@11.17.2`, classic script `node_modules/mermaid/dist/mermaid.min.js`, trang harness `file://`, và Chromium thật. Với từng block gọi riêng `await mermaid.parse(source)` rồi `await mermaid.render(id, source)`.

| File / block | Parse | Render | SVG length | `foreignObject` |
|---|---|---|---:|---|
| report #0 flowchart | PASS | PASS | 28154 | yes |
| report #1 startup sequence | PASS | PASS | 31077 | no |
| report #2 typing sequence | PASS | PASS | 30801 | no |
| report #3 state diagram | PASS | PASS | 38356 | yes |
| UniKey #0 startup sequence | PASS | PASS | 27495 | no |
| UniKey #1 key rewrite sequence | PASS | PASS | 28507 | no |
| UniKey #2 engine flowchart | PASS | PASS | 22566 | yes |

Kết quả: **7/7 PARSE_OK, 7/7 RENDER_OK**. Kiểm tra `innerText` từng SVG: **0/7** có literal `<br/>`, **0/7** có literal `\n`. Screenshot flowchart cho thấy label xuống dòng thật.

### 3.2 JSON, links, citations và bảng

- `python3 -m json.tool evidence/artifact_manifest.json`: `VALID_JSON`, exit 0.
- Relative links trong README/report/spec/matrix: **156 checked, 0 broken, 0 bad line anchors**.
- Citation source: **197 occurrences, 0 missing/out-of-range**:
  - 151 citation tới file trong dossier;
  - 46 citation/range tới checkout UniKey (full path hoặc shorthand được resolve về cây canonical `unikey/`/`x-unikey/`).
- Checkout baseline: `git rev-parse HEAD` = `e3b8f3b53bdaa700945a62e99129df625509cd38`; worktree clean.
- Markdown tables: **16 groups**, **0 inconsistent column-width group**.

### 3.3 Test matrix và tám contract

- Test rows: **58**.
- Numbering: đúng liên tục **1..58**.
- Categories: **A=20, B=9, C=14, D=15**.
- Đủ 8 contract: `InputEvent`, `EngineOptions`, `EngineState`, `EditTransaction`, `HostContext`, `CompatibilityPolicy`, `ConfigStore`, `MacroStore`.
- `EditTransaction` có đúng 5 field: `delete_before`, `insert_utf16`, `consume_original`, `replay_key`, `reset_context`.
- Offsets `0x00..0x38`, mapping `0x1038`, opcodes `1/2/3/5`, timeout `100/50/2000 ms` vẫn có và được giới hạn trong adapter compatibility.

### 3.4 Artifact identity, hash và size

Đọc ZIP bằng `zipfile`; không execute file bên trong:

- ZIP: **1,504,714 bytes**, SHA-256 `59ac1ceeb7fce4407f5565a9ecf885210b450a3b789e035f06d07728e26e78bc` — khớp manifest.
- ZIP chứa đúng 7 regular files được khai báo; **7/7** size và SHA-256 khớp manifest.
- Hai `evkime32.dll`: cùng SHA-256 `91b978ab3c816f78125afae8d4fe2390d2fa1ce3d696d2df223cb6f6b193d53d`.
- Restricted evidence: **3/3** size/hash khớp:
  - `evkime32_ghidra_all.c`: 995079 bytes, `bb1d72ef...cafc39b4`;
  - `evkime64_ghidra_all.c`: 931261 bytes, `688aee9e...00bd1eab`;
  - `evkimearm64_ghidra_all.c`: 1077216 bytes, `33681e34...024e57f7`.
- Sáu deliverable hiện tại khớp toàn bộ hash/size được ghi ở `fix_round1.md` V10.

### 3.5 Updater, placeholder và ranh giới claim

- Sáu generated artifacts có **7** lần `evkau.exe`, **0** lần `evkmau.exe`.
- **0** token placeholder nguyên từ: `TODO`, `TBD`, `FIXME`, `PLACEHOLDER`.
- Quét bytes tĩnh: `evkau.exe` UTF-16 có trong cả 3 EXE; không có `evkmau.exe` ASCII/UTF-16 trong 7 PE.
- Authenticode vẫn ghi đúng caveat `authenticode_image_digest_verified=false`; không có claim trust/integrity đã xác minh.
- Engine placement và exact EVKey transform vẫn được phân biệt Inferred/Unknown/Baseline; không phát hiện overclaim mới.
- License vẫn yêu cầu tham khảo file license tại commit, không tự khẳng định license cụ thể.

### 3.6 HTML tự chứa và phân biệt Proposed/Observed

Kiểm tra source và đọc lại DOM/runtime trong Chromium:

```text
<script>: 0
inline on*= handler: 0
<link>: 0
@import: 0
http(s) URL: 0
loaded resource entries: 0
component rectangle overlaps: 0
```

- Computed style của 4 component Proposed: stroke `rgb(232,121,249)`, dash `6px, 4px`, fill fuchsia translucent.
- Legend có đủ 6 mục và tách `Controller (observed)` khỏi `Proposed component (clean-room)`.
- Ba card `Quan sát / Suy luận / Chưa rõ` hiện diện.
- Dependency/JS-free: **PASS**.
- Proposed-vs-observed distinction: **PASS**.
- Text containment: **FAIL 1 label** — N1.

---

## 4. Lệnh và harness đã chạy

```sh
python3 /root/.hermes/cache/scratch/evkey_round2_audit.py
# JSON, identity, ZIP 7/7, restricted 3/3, links 156/0,
# tables 16/0, tests 58 (20/9/14/15), contracts 8/8

python3 /root/.hermes/cache/scratch/evkey_round2_citations.py
# total 197, bad []

python3 /root/.hermes/cache/scratch/evkey_round2_deep_audit.py
# EditTransaction 5 fields; static ZIP string scan; no PE execution

python3 -m json.tool evidence/artifact_manifest.json >/dev/null
# VALID_JSON

node -p "require('/root/.hermes/cache/scratch/node_modules/mermaid/package.json').version"
# 11.17.2

git -C /root/.hermes/cache/scratch/unikey-source rev-parse HEAD
# e3b8f3b53bdaa700945a62e99129df625509cd38
```

Browser harness:

- `/root/.hermes/cache/scratch/evkey_round2_mermaid.html`: 7 block trích trực tiếp, parse + render từng block trong Chromium.
- `file:///root/evkey_reconstruction/diagrams/evkey_architecture.html`: đọc DOM, resource list, computed CSS, SVG `getBBox()`, rectangle intersections và screenshot.

## 5. Điều kiện để đổi verdict sang PASS

Chỉ còn N1: sửa containment của nhãn `Policy (Proposed)`, sau đó chạy lại screenshot + `getBBox()` cho toàn bộ label và xác nhận không có label nào vượt component box. Không cần mở lại 16 finding vòng 1 nếu thay đổi chỉ giới hạn ở geometry/style của box Policy, nhưng vẫn phải chạy lại HTML dependency check và hash deliverable liên quan.