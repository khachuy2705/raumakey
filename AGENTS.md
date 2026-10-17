# Quy định cho kho mã Raumakey

## Ranh giới clean-room

Agent triển khai và agent review **KHÔNG ĐƯỢC** đọc, tìm kiếm, tóm tắt hay dùng bất cứ thứ gì nằm trong `restricted_evidence/`.

Các nguồn được phép dùng khi triển khai:

- `docs/superpowers/specs/`
- `docs/superpowers/plans/`
- `docs/reference/evkey_clean_room_spec.md`
- `docs/reference/evkey_compatibility_test_matrix.md`
- mã nguồn và test được tạo trong kho mã này
- mã nguồn UniKey upstream có giấy phép hợp lệ, chỉ sau khi repository, commit và giấy phép của nó đã được ghi vào `THIRD_PARTY.md`

Không sao chép control flow từ bản dịch ngược (decompiler) của EVKey, cũng không sao chép tài nguyên, icon, chuỗi, cấu trúc class hay thương hiệu của EVKey. Raumakey dùng định danh sản phẩm và định danh TSF mới.

## Quy tắc phát triển

- Phát triển theo TDD: phải thấy một test liên quan fail trước khi thêm mã production.
- Crate engine và model không được dùng Win32, COM, registry, HWND, handle hay raw pointer.
- Không để panic vượt qua ranh giới COM, callback Win32 hay FFI.
- Không bao giờ ghi log phím người dùng gõ, nội dung văn bản, nội dung clipboard hay nội dung thay thế của macro.
- Định dạng bằng `cargo fmt`; lint bằng Clippy với `-D warnings`.
- Cross-build thành công không phải là bằng chứng hành vi khi chạy thật trên Windows là đúng.

## Ngôn ngữ tài liệu

- Mọi tài liệu của dự án viết bằng tiếng Việt có dấu đầy đủ, vì Raumakey dành cho người Việt. Quy định này áp dụng cho README, GUIDE, thiết kế, kế hoạch, ghi chép task, ghi chép review, `THIRD_PARTY.md`, file này và `README.txt` trong gói phát hành.
- Giữ nguyên mã, định danh, lệnh, đường dẫn, mã quyết định (D25), số mục (§3.8) và các thuật ngữ kỹ thuật tiếng Anh quen dùng (engine, macro, TIP, TSF, IPC, hook, snapshot…).
- Comment và rustdoc trong mã nguồn vẫn viết bằng tiếng Anh. Script `.ps1` sinh ra phải là ASCII, vì Windows PowerShell 5.1 đọc file không có BOM theo bảng mã ANSI.
