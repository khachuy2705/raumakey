# Raumakey

Raumakey là bộ gõ tiếng Việt cho Windows, phát hành theo giấy phép GPL-3.0-or-later, viết bằng Rust và tích hợp trực tiếp với Win32 và TSF. Dự án đang trong giai đoạn phát triển.

Trạng thái ngày 2026-10-17: các thư viện lõi (model, engine, macro, cấu hình, policy, IPC, chuyển mã, migration, tài nguyên, lớp bọc Windows), dịch vụ văn bản TSF (`raumakey_tip.dll`: lớp COM và đường xử lý phím, chưa kết nối với controller), dòng lệnh của controller (install, uninstall, startup, convert) cùng một biểu tượng khay tối giản, các bộ test dùng chung (fuzz smoke, vector tương thích, test runner) và công cụ build `xtask` đã được triển khai và đạt toàn bộ test tự động. IPC hub, hook bàn phím, transport đường dự phòng (fallback) và bảng cài đặt của controller, cũng như bộ nghiệm thu trên Windows, chưa được viết. Raumakey chưa được thử trong ứng dụng Windows thật, nên hãy xem đây là bản phát triển. Mục 14 của kế hoạch triển khai liệt kê việc đã xong và việc còn lại; mục 20 của thiết kế ghi bằng chứng chi tiết của lần chụp trạng thái trước đó.

## Build và dùng thử

[`GUIDE.md`](GUIDE.md) hướng dẫn dựng môi trường, build, đóng gói, cài đặt, dùng thử và gỡ cài đặt.

## Mục tiêu

- Windows 10 trở lên
- Nghiệm thu khi chạy thật: x86_64
- Cổng biên dịch: các target MSVC i686, x86_64 và ARM64
- Giao diện Win32 native, controller ở khay hệ thống, COM TSF TIP, transport đường dự phòng, macro, chuyển mã, cấu hình và migration chỉ đọc từ dữ liệu EVKey

## Tài liệu

- Hướng dẫn build và dùng thử: [`GUIDE.md`](GUIDE.md)
- Thiết kế và trạng thái triển khai: [`docs/superpowers/specs/2026-10-16-raumakey-design.md`](docs/superpowers/specs/2026-10-16-raumakey-design.md)
- Kế hoạch triển khai: [`docs/superpowers/plans/2026-10-16-raumakey-implementation.md`](docs/superpowers/plans/2026-10-16-raumakey-implementation.md)
- Đặc tả hành vi clean-room: [`docs/reference/evkey_clean_room_spec.md`](docs/reference/evkey_clean_room_spec.md)
- Ma trận tương thích: [`docs/reference/evkey_compatibility_test_matrix.md`](docs/reference/evkey_compatibility_test_matrix.md)

Hồ sơ tái dựng gốc được giữ cục bộ trong `restricted_evidence/` và không đưa lên Git. Agent triển khai bị cấm đọc thư mục này.
