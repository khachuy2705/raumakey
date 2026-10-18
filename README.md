# Raumakey

Raumakey là bộ gõ tiếng Việt cho Windows, phát hành theo giấy phép GPL-3.0-or-later và viết bằng Rust. Raumakey là một tệp `raumakey.exe` duy nhất, dùng giống UniKey hay EVKey: chép vào thư mục nào cũng được, bấm chạy là gõ được, không cần cài đặt hay quyền quản trị. Lần chạy đầu, Raumakey tạo tệp cấu hình `raumakey.toml` ngay cạnh `raumakey.exe`; các lần sau đọc lại tệp đó. Bảng cấu hình (mở từ khay hệ thống, hoặc bằng cách chạy lại `raumakey.exe`) chỉnh kiểu gõ, cách đặt dấu, kiểm tra chính tả, gõ tắt và khởi động cùng Windows; hộp thoại "Danh sách gõ tắt" thêm, sửa và xoá các mục gõ tắt. Mọi thay đổi được lưu ngay. Dự án đang trong giai đoạn phát triển.

Trạng thái ngày 2026-10-18:

- Đã triển khai và đạt toàn bộ test tự động:
  - các thư viện lõi: model, engine, macro, cấu hình, policy, IPC, chuyển mã, migration, tài nguyên, lớp bọc Windows;
  - lõi phiên dùng chung (`raumakey-session`);
  - đường gõ bằng hook bàn phím trong `raumakey.exe`, cùng menu khay;
  - bảng cấu hình và hộp thoại danh sách gõ tắt (thiết kế §22);
  - các bộ test dùng chung và công cụ build `xtask`.
- Từ 2026-10-17, dịch vụ văn bản TSF (`raumakey_tip.dll`) vẫn còn trong kho mã nhưng không còn được phát hành (thiết kế §21).
- Raumakey chưa được thử trong ứng dụng Windows thật, nên hãy xem đây là bản phát triển.
- Kế hoạch triển khai ghi trạng thái ở mục 14, 16 và 17.

## Build và dùng thử

[`GUIDE.md`](GUIDE.md) hướng dẫn dựng môi trường, build, đóng gói, dùng thử và gỡ bỏ.

## Mục tiêu

- Windows 10 trở lên
- Nghiệm thu khi chạy thật: x86_64
- Cổng biên dịch: các target MSVC i686, x86_64 và ARM64
- Một tệp `raumakey.exe` portable: hook bàn phím, menu khay, bảng cấu hình, danh sách gõ tắt, chuyển mã, cấu hình cạnh exe, và migration chỉ đọc từ dữ liệu EVKey (chưa có)

## Tài liệu

- Hướng dẫn build và dùng thử: [`GUIDE.md`](GUIDE.md)
- Thiết kế và trạng thái triển khai: [`docs/superpowers/specs/2026-10-16-raumakey-design.md`](docs/superpowers/specs/2026-10-16-raumakey-design.md)
- Kế hoạch triển khai: [`docs/superpowers/plans/2026-10-16-raumakey-implementation.md`](docs/superpowers/plans/2026-10-16-raumakey-implementation.md)
- Đặc tả hành vi clean-room: [`docs/reference/evkey_clean_room_spec.md`](docs/reference/evkey_clean_room_spec.md)
- Ma trận tương thích: [`docs/reference/evkey_compatibility_test_matrix.md`](docs/reference/evkey_compatibility_test_matrix.md)

Hồ sơ tái dựng gốc được giữ cục bộ trong `restricted_evidence/` và không đưa lên Git. Agent triển khai bị cấm đọc thư mục này.
