# Fix vòng 2 — N1 geometry của sơ đồ HTML

## Phạm vi

Chỉ sửa `diagrams/evkey_architecture.html`; không thay đổi nội dung kỹ thuật, báo cáo, đặc tả, ma trận kiểm thử, manifest artifact gốc hoặc binary.

## N1 — Đã sửa

Finding: tiêu đề `Policy (Proposed)` rộng hơn box Policy (`105` SVG units), tràn hai phía.

Thay đổi:

- Tách tiêu đề thành hai dòng: `Policy` (class `btitle`) và `(Proposed)` (class `tiny`).
- Dịch các nhãn nội dung xuống, vẫn nằm trong box `x=1160`, `y=350`, `width=105`, `height=205`.
- Giữ nguyên style `box proposed` và ranh giới Proposed/Observed.

## Xác minh bằng Chromium thật

Mở trực tiếp `file:///root/evkey_reconstruction/diagrams/evkey_architecture.html`, dùng SVG DOM `getBBox()` để đo box và toàn bộ tám label trong group Policy.

Kết quả:

- Box: `x=1160`, `y=350`, `width=105`, `height=205`.
- `Policy`: contained = `true`.
- `(Proposed)`: contained = `true`.
- `special apps`, `TsfCache`, `transport`, `secure field`, `reset rules`, `x86/x64/ARM64`: contained = `true`.
- `allContained = true`.
- `<script> = 0`, `<link> = 0`, external URL = `0`.
- Screenshot render xác nhận tiêu đề không còn vượt viền và không chạm component bên trái.

## Danh tính file sau sửa

- Path: `diagrams/evkey_architecture.html`
- Size: `13242` bytes
- SHA-256: `622e1cc9319362a533d50925ca31d3faeabb1b1faa427b0d866336d8c49b42b7`

Không chạy EXE/DLL EVKey trong vòng sửa này.
