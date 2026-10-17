# Xử lý tiếp sau review — 2026-10-17

## Phạm vi và phát hiện

Đầu vào: thiết kế và kế hoạch đã được duyệt, ma trận tương thích, mã nguồn và test của kho mã.
Không sử dụng bằng chứng bị hạn chế hay bản triển khai upstream nào. Cây Git ban đầu: sạch,
commit `24ba7a6` (`update p1`). Đây là một review có trọng tâm vào phần triển khai, không phải
review độc lập toàn branch vốn bắt buộc phải có trước khi release.

- Kiến trúc thư viện phân lớp đã có, nhưng `raumakey-controller/src/main.rs`,
  `raumakey-tsf/src/lib.rs` và trình chạy nghiệm thu dùng chung vẫn chỉ là stub. Việc test thư
  viện đạt không chứng minh rằng đã có một kiểu gõ hoạt động được.
- `xtask/src/cargo.rs` đã bỏ sót các lựa chọn feature cũ (legacy) của Windows, controller và
  TIP. Vì vậy CI không bao phủ toàn bộ đường tương thích đã khai báo. Các lệnh phân giải phụ
  thuộc cũng bỏ sót `--locked`.
- `xtask/src/tasks.rs` đã âm thầm biến việc thiếu một bộ fuzz bắt buộc thành một ghi chú bỏ qua.
- `xtask/src/evidence.rs` chấp nhận bất kỳ tập bước đạt không rỗng nào là đã kiểm chứng, kể cả
  danh sách gate chưa đầy đủ. `gate.rs` không có bước review toàn branch.
- Hành vi với ô bảo mật vẫn chưa nhất quán: chính sách `compat.rs` cho phép gõ tiếng Việt trong
  `Restricted`, trong khi engine `process.rs` bỏ qua mọi ô bảo mật. Cần thống nhất hợp đồng
  trước khi nối các adapter; đợt bổ sung này không thay đổi hành vi nào.
- Bộ lưu trữ (store) flush tệp tạm trước khi thay thế. Mối lo ngại hiện có về độ bền dữ liệu của
  nó vẫn còn bỏ ngỏ; review này không chứng minh được độ bền khi mất điện.

## Phần bổ sung đã triển khai

CI bao phủ cả năm feature `evkey-compat` của workspace, dùng lockfile và báo lỗi sớm nếu thiếu
bộ fuzz. Bằng chứng release yêu cầu đúng danh sách gate theo đúng thứ tự. Một gate review toàn
branch mới đối chiếu bản ghi do reviewer cung cấp với revision Git sạch hiện tại trước khi đóng
gói và ghi lại SHA-256 của bản ghi đó. Kế hoạch §12 mô tả schema của bản ghi và các giới hạn
của nó. Lỗi khi lấy trạng thái Git không còn bị âm thầm tính là cây mã nguồn sạch.

Không bổ sung chức năng sản phẩm, ngữ nghĩa migration (chuyển dữ liệu), quyết định về ô bảo mật
hay khẳng định nào về runtime. Không tạo ra phê duyệt nào của reviewer độc lập.

## Bằng chứng TDD

Các test được chạy bằng toolchain Rust 1.99.1 portable sẵn có cho Windows, thông qua PowerShell
từ WSL, dùng cấu hình linker MSVC sẵn có của máy.

1. Mốc ban đầu (baseline): `cargo test --locked -p xtask` — 96 test đạt.
2. Đỏ (red): cùng lệnh trên — 95 test đạt, 4 test fail:
   - `cargo::tests::compat_ci_covers_every_workspace_legacy_adapter`
   - `cargo::tests::dependency_resolving_commands_preserve_the_lockfile`
   - `evidence::tests::incomplete_duplicate_or_reordered_gates_cannot_claim_verification`
   - `gate::tests::steps_follow_the_documented_order`
3. Đỏ (red): `cargo test --locked -p xtask missing_fuzz_suite` —
   `tasks::tests::missing_fuzz_suite_fails_before_starting_ci_commands` fail vì CI đã cố khởi
   chạy Cargo thay vì trả về lỗi thiếu bộ fuzz.
4. Đỏ (red) ở hợp đồng API: `cargo test --locked -p xtask review` không biên dịch được vì
   `validate_review` khi đó chưa tồn tại. Các test được viết trước khi thêm API đó.
5. Xanh (green): `cargo fmt --all` và `cargo test --locked -p xtask` — 102 test đạt.
6. `cargo run --locked -p xtask -- ci --no-fuzz` — đạt bước kiểm tra định dạng, Clippy với
   `-D warnings` cho cả hai tập feature, và test workspace: 1,241 test đạt với cấu hình mặc
   định; 1,315 test đạt với cả năm feature tương thích. Cả hai lần chạy vẫn giữ nguyên 8 test
   thủ công bị ignore sẵn có. Nhật ký: `target/ci-review-followup.log`.
7. `cargo run --locked -p xtask -- ci` — thoát với mã 1 như dự kiến do thiếu `fuzz_smoke`.
8. `cargo run --locked -p xtask -- release-gate` — thoát với mã 1 tại bước CI, như dự kiến. Tệp
   `target/release-evidence/release-gate-1792205238.json` được sinh ra đã được kiểm tra:
   `passed=false`, `verification_level="not verified"`, `runtime_verified=false`, sáu bước
   tiếp theo ở trạng thái `not-run`, và không có sản phẩm release (artifact) nào. Các nhật ký
   được sinh ra này là bằng chứng cục bộ, không phải chứng nhận release đã được commit.

Review độc lập cho đợt bổ sung này vẫn đang chờ. Không chạy lại cross-build hay test nghiệm thu
trên ứng dụng chủ Windows nào cho thay đổi chỉ liên quan đến công cụ (tooling) này.

## Việc còn lại

Triển khai các fuzz target tất định và các test tích hợp macro/engine; sau đó tiếp tục với các
script build tài nguyên và việc port edit session TSF được mô tả trong thiết kế §20.10. Việc
triển khai controller và TIP, nghiệm thu trên Windows và review độc lập vẫn là bắt buộc.
