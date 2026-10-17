# Nguồn gốc và thông báo của bên thứ ba

Raumakey được cấp phép theo GPL-3.0-or-later (xem `LICENSE`).

## UniKey

**Không sử dụng.** Không có tệp mã nguồn UniKey nào được sao chép, chuyển dịch hay phỏng
theo. Engine tiếng Việt được viết dựa trên kế hoạch triển khai Raumakey
(`docs/superpowers/plans/2026-10-16-raumakey-implementation.md`), đặc tả hành vi clean-room và
ma trận tương thích. Các hành vi được gắn nhãn "baseline" trong ma trận chỉ được coi là kỳ vọng
về hành vi.

Nếu sau này mã nguồn UniKey được tái sử dụng hoặc chuyển dịch, phải ghi lại tại đây **trước khi**
thay đổi được đưa vào: URL kho mã, commit bất biến, văn bản giấy phép, các tệp bị ảnh hưởng và
các thông báo bắt buộc.

## EVKey

Không sử dụng bất kỳ tệp nhị phân, tài nguyên, biểu tượng, chuỗi, đầu ra của trình dịch ngược
(decompiler), mã giả, bố cục lớp hay luồng điều khiển nào của EVKey. Hồ sơ tái dựng nằm dưới
`restricted_evidence/` bị loại khỏi Git và không được các agent triển khai hay agent review đọc.
Raumakey dùng CLSID, GUID profile, mã nâng cấp (upgrade code), tiền tố tên đối tượng
(named-object prefix), gốc registry, biểu tượng và chuỗi của riêng mình.

Cargo feature tùy chọn `evkey-compat` triển khai bố cục bộ nhớ chia sẻ cũ (legacy) được mô tả
trong `docs/reference/evkey_clean_room_spec.md` §6 nhằm đảm bảo khả năng tương tác; feature này
bị tắt theo mặc định.

## Các crate Rust phụ thuộc

Các phụ thuộc runtime và build được tải về từ crates.io và được khóa trong `Cargo.lock`.
`cargo run -p xtask -- notices` sinh lại danh sách đầy đủ kèm biểu thức giấy phép, và
`cargo run -p xtask -- audit` thực thi các quy tắc trong `deny.toml`.

| Crate | Giấy phép | Dùng cho |
|---|---|---|
| serde, serde_derive, serde_json | MIT OR Apache-2.0 | tài liệu, payload IPC |
| toml | MIT OR Apache-2.0 | tệp cấu hình |
| unicode-normalization | MIT OR Apache-2.0 | NFC/NFD |
| sha2 | MIT OR Apache-2.0 | checksum |
| crc32fast | MIT OR Apache-2.0 | checksum của frame IPC |
| windows, windows-core | MIT OR Apache-2.0 | binding Win32/COM/TSF |
| object | Apache-2.0 OR MIT | kiểm tra tệp PE trong `xtask` (không phân phối kèm) |
| proptest, tempfile | MIT OR Apache-2.0 | chỉ dùng cho test (không phân phối kèm) |

## Công cụ build (không phân phối kèm)

- Toolchain Rust (MIT OR Apache-2.0).
- Trên các host Windows không có Visual Studio, CRT của MSVC và các thư viện import của Windows
  SDK được lấy bằng `xwin`; việc sử dụng chúng chịu sự điều chỉnh của các điều khoản giấy phép
  của Microsoft, mà nhà phát triển chấp nhận khi tải chúng về. Các bản build release liên kết CRT
  tĩnh (`+crt-static`, do `xtask` thiết lập), nên các package không phụ thuộc vào gói Visual C++
  redistributable.

## Nguồn đối chiếu kiểm chứng (oracle)

Các bảng dữ liệu bảng mã trong `raumakey-converter` được viết dựa trên các tiêu chuẩn bảng mã
đã công bố (RFC 1456 cho VISCII/VIQR, TCVN 5712:1993, Windows-1258, VNI Windows). Các bản triển
khai codec độc lập có thể được dùng làm oracle cho test trong quá trình phát triển; không có mã
hay tệp dữ liệu nào được sao chép từ chúng.
