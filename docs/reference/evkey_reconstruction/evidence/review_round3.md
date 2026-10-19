# Review vòng 3 — xác minh release cuối sau fix N1

## 0. Phạm vi và nguyên tắc

- Đọc `evidence/review_round2.md` và `evidence/fix_round2.md` trước khi kiểm tra.
- Chỉ xác minh lại phạm vi release được yêu cầu: geometry/render của `diagrams/evkey_architecture.html`, tính tự chứa/an toàn tĩnh của HTML, và integrity/path của `evidence/artifact_manifest.json`.
- Mở HTML trực tiếp bằng Chromium thật; không suy luận geometry từ source đơn thuần.
- Không chạy bất kỳ EXE/DLL EVKey nào. Restricted evidence chỉ được đọc bytes để tính size và SHA-256.
- Không sửa deliverable. File duy nhất được tạo là báo cáo review này.

## 1. Verdict

**PASS / RELEASE-READY trong phạm vi xác minh cuối.**

- N1 đã đóng: toàn bộ 8 label trong component `Policy` nằm trong rectangle.
- Toàn bộ **41/41 label** của **13/13 component group** nằm trong component rectangle tương ứng.
- **0** cặp component rectangle chồng nhau.
- Legend và updater đều ở ngoài cả ba boundary.
- HTML tự chứa, không có script/link/external URL/event handler và không tải resource ngoài.
- Manifest parse thành công; mọi path được yêu cầu tồn tại; restricted evidence **3/3** khớp size và SHA-256.
- Hash HTML hiện tại khớp chính xác hash sau sửa trong `fix_round2.md`.

Không phát hiện finding Blocking, Major hoặc Minor mới.

## 2. Geometry và render trong Chromium thật

Mở:

`file:///root/evkey_reconstruction/diagrams/evkey_architecture.html`

Runtime:

`HeadlessChrome/145.0.0.0` trên Linux, Blink/WebKit 537.36.

Phép đo gọi `SVGGraphicsElement.getBBox()` cho mỗi rectangle và mỗi `text`, sau đó quy đổi qua CTM về SVG user-space trước khi so sánh bốn biên với epsilon `0.001`.

### 2.1 Containment của mọi component group

| Component | Số label | Kết quả |
|---|---:|---:|
| Windows / TSF Host App | 2 | PASS |
| evkime COM TIP | 3 | PASS |
| TSF EditSession | 3 | PASS |
| Named Mapping | 3 | PASS |
| Sync Objects | 4 | PASS |
| Compatibility Contract | 3 | PASS |
| EVKey_MainWnd / Controller | 3 | PASS |
| LegacyIpcAdapter (Proposed) | 2 | PASS |
| VietnameseEngine (Proposed) | 3 | PASS |
| ConfigStore / MacroStore | 3 | PASS |
| Policy | 8 | PASS |
| Tray / UI / Hotkeys | 2 | PASS |
| evkau.exe — External Updater | 2 | PASS |
| **Tổng** | **41** | **41/41 PASS** |

### 2.2 N1 — Policy / Proposed

Rectangle Policy: `x=1160`, `y=350`, `width=105`, `height=205`, biên `[1160,1265] × [350,555]`.

| Label | getBBox x..right | getBBox y..bottom | Contained |
|---|---:|---:|---:|
| Policy | 1188.601..1235.436 | 367.212..381.923 | true |
| (Proposed) | 1188.042..1235.958 | 387.135..395.961 | true |
| special apps | 1183.253..1240.856 | 411.135..419.961 | true |
| TsfCache | 1192.831..1231.270 | 431.135..439.961 | true |
| transport | 1190.440..1233.670 | 451.135..459.961 | true |
| secure field | 1183.253..1240.856 | 471.135..479.961 | true |
| reset rules | 1185.644..1238.456 | 491.135..499.961 | true |
| x86/x64/ARM64 | 1180.855..1243.249 | 511.135..519.961 | true |

Kết quả: `allContained=true`. Nhãn hai dòng `Policy` / `(Proposed)` không tràn và có khoảng trống rõ với box `VietnameseEngine` bên trái.

### 2.3 Overlap và boundary

- Component rectangle: **13**.
- Pairwise rectangle intersection có diện tích dương: **0**.
- Ba boundary kết thúc tại `y=660`.
- Legend bbox sau transform: `x=45.000..606.055`, `y=713.232..783.942`; không giao boundary nào.
- Updater rectangle: `x=915..1205`, `y=715..785`; không giao boundary nào.

### 2.4 Kiểm tra hình ảnh

Chụp và đọc full-page render ở viewport `1600 × 1400`. Sơ đồ, arrow, boundary, legend, updater và ba card dưới cùng hiển thị đầy đủ; không thấy text clipping, chồng component, lệch layout hoặc regression thị giác. `Policy / (Proposed)` hiển thị thành hai dòng, căn giữa trong box.

## 3. HTML self-contained / attack surface

Đối chiếu cả source bằng HTML parser và DOM/runtime trong Chromium:

```text
<script> tags:             0
<link> tags:               0
inline on*= handlers:      0
http(s)/protocol-relative: 0
runtime scripts:           0
runtime links:             0
runtime resource entries:  0
```

Các `url(#arr)`, `url(#arrO)`, `url(#arrR)` là tham chiếu marker SVG nội bộ, không phải external URL.

Kết quả: **PASS**.

## 4. Manifest evidence

`evidence/artifact_manifest.json` parse JSON thành công; `schema_version=1`.

### 4.1 Path tồn tại

- `generated_artifacts`: **6/6** regular files tồn tại.
- `evidence_notes`: **4/4** regular files tồn tại.
- `restricted_evidence.files`: **3/3** regular files tồn tại.
- Tổng path trong ba nhóm tại thời điểm đọc manifest: **13/13** tồn tại.

### 4.2 Restricted evidence hash/size

| Path | Size thực tế | SHA-256 thực tế | Kết quả |
|---|---:|---|---:|
| `artifacts/evkime32_ghidra_all.c` | 995079 | `bb1d72efdd76611090d7537f67211d57c84e26900df3bc4dbe010324cafc39b4` | PASS |
| `artifacts/evkime64_ghidra_all.c` | 931261 | `688aee9e4938f5ae7e11b4093cdb7d32b2e9c73ac972e780165bd52800bd1eab` | PASS |
| `artifacts/evkimearm64_ghidra_all.c` | 1077216 | `33681e349f4609b7e626357c489b2b2a2b9589c8c994569ab9faf42b024e57f7` | PASS |

Kết quả: **3/3 size match, 3/3 SHA-256 match**.

## 5. Danh tính HTML sau sửa

- Path: `diagrams/evkey_architecture.html`
- Size thực tế: `13242` bytes
- SHA-256 thực tế: `622e1cc9319362a533d50925ca31d3faeabb1b1faa427b0d866336d8c49b42b7`
- SHA-256 khai báo trong `evidence/fix_round2.md`: `622e1cc9319362a533d50925ca31d3faeabb1b1faa427b0d866336d8c49b42b7`
- Kết quả: **MATCH**.

## 6. Kết luận

N1 được xác minh đóng bằng phép đo `getBBox()` trong Chromium thật và kiểm tra hình ảnh độc lập. Geometry HTML, self-contained constraints và manifest evidence đều PASS. **Release verdict: PASS / RELEASE-READY trong phạm vi review vòng 3.**
