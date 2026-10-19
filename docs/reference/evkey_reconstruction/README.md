# Hồ sơ clean-room reconstruction EVKey 6.0.5.0

Hồ sơ này tổng hợp phân tích tĩnh gói `EVKey.zip`; không chạy bất kỳ PE nào, không quan sát runtime và không khẳng định tương đương mã nguồn với EVKey hay UniKey.

## Điểm vào

| Tài liệu | Mục đích |
|---|---|
| [`EVKey_reconstruction_report.md`](EVKey_reconstruction_report.md) | Báo cáo hợp nhất: kiến trúc, lifecycle, typing, IPC/TSF, cấu hình, tương thích và ranh giới rewrite |
| [`clean_room_spec.md`](clean_room_spec.md) | Đặc tả trung lập ngôn ngữ với kiểu dữ liệu, interface, MUST/SHOULD/MAY |
| [`compatibility_test_matrix.md`](compatibility_test_matrix.md) | Ma trận kiểm thử tương thích và oracle có phân cấp bằng chứng |
| [`diagrams/evkey_architecture.html`](diagrams/evkey_architecture.html) | Sơ đồ kiến trúc SVG/HTML tự chứa, không JavaScript |
| [`evidence/artifact_manifest.json`](evidence/artifact_manifest.json) | Danh tính ZIP, bảy PE, quan hệ trùng lặp, nguồn baseline và đầu ra |

## Nguồn bằng chứng

- [`research/pe_inventory.md`](research/pe_inventory.md): danh tính, PE metadata, imports/exports, resources, hardening và chứng thư.
- [`research/evkey64_exe.md`](research/evkey64_exe.md): controller x64, UI/tray, hook, persistence, startup/elevation, updater và phía server IPC.
- [`research/evkime_dlls.md`](research/evkime_dlls.md): COM TIP/TSF, bridge IPC, edit session, reset, replay sentinel và parity kiến trúc.
- [`research/unikey_baseline.md`](research/unikey_baseline.md): baseline mã nguồn UniKey tại commit cố định; đây là mô hình so sánh, không phải bằng chứng engine EVKey.
- Pseudocode chọn lọc: [`artifacts/evkey64_pseudocode.md`](artifacts/evkey64_pseudocode.md), [`artifacts/evkime_pseudocode.md`](artifacts/evkime_pseudocode.md).

### Bằng chứng hạn chế — decompiler output (không cấp cho nhóm implementation)

Ba export Ghidra đầy đủ nằm trong `artifacts/` và **không** phải input implementation. Nhóm clean-room implementation MUST NOT nhận các file này; chúng chỉ phục vụ address-level follow-up của nhóm review/phân tích ([`clean_room_spec.md:288`](clean_room_spec.md#L288)).

| File | Bytes | SHA-256 |
|---|---:|---|
| [`artifacts/evkime32_ghidra_all.c`](artifacts/evkime32_ghidra_all.c) | 995079 | `bb1d72efdd76611090d7537f67211d57c84e26900df3bc4dbe010324cafc39b4` |
| [`artifacts/evkime64_ghidra_all.c`](artifacts/evkime64_ghidra_all.c) | 931261 | `688aee9e4938f5ae7e11b4093cdb7d32b2e9c73ac972e780165bd52800bd1eab` |
| [`artifacts/evkimearm64_ghidra_all.c`](artifacts/evkimearm64_ghidra_all.c) | 1077216 | `33681e349f4609b7e626357c489b2b2a2b9589c8c994569ab9faf42b024e57f7` |

Cảnh báo phân phối: các file này chứa chi tiết disassembly/decompilation của binary có bản quyền; chỉ chia sẻ trong phạm vi nhóm phân tích, không đưa vào hợp đồng spec/test, và không sao chép control flow vào code mới. Danh tính đầy đủ nằm trong nhóm `restricted_evidence` của [`evidence/artifact_manifest.json`](evidence/artifact_manifest.json).

## Phạm vi artifact

Gói gốc có ba EXE (`x86`, `x64`, `ARM64`) và bốn đường dẫn DLL TIP; hai bản `evkime32.dll` là byte-identical. EXE có version resource `6.0.5.0`; DLL không có version resource. Certificate table được parse nhưng **không** xác minh Authenticode image digest hay chuỗi tin cậy. Danh tính chính xác nằm trong manifest và inventory ([`research/pe_inventory.md:13-19`](research/pe_inventory.md#L13-L19), [`research/pe_inventory.md:23-31`](research/pe_inventory.md#L23-L31), [`research/pe_inventory.md:273-285`](research/pe_inventory.md#L273-L285)).

## Nhãn độ tin cậy

- **Quan sát / Observed**: trực tiếp từ PE, resource, string, import/export, disassembly hoặc decompilation tĩnh. Vẫn có giới hạn do stripped symbols và decompiler.
- **Suy luận / Inferred**: diễn giải kiến trúc từ nhiều quan sát; không được dùng như ABI hoặc hành vi đã xác minh.
- **Baseline UniKey**: hành vi có trong mã nguồn UniKey được trích dẫn `file:line`; chỉ là oracle tham khảo cho engine EVKey chưa thấy trong DLL.
- **Đề xuất / Proposed**: quyết định thiết kế cho rewrite sạch; không phải mô tả binary.
- **Chưa rõ / Unknown**: chưa đủ bằng chứng tĩnh hoặc chỉ có thể giải quyết bằng source/runtime test.

Địa chỉ dạng `0x140...` thuộc `EVKey64.exe`; `0x180...` thường thuộc `evkime64.dll`; địa chỉ x86/ARM64 được ghi riêng. Link `#Lx-Ly` dùng quy ước renderer hỗ trợ anchor dòng; trích dẫn vẫn luôn chứa đường dẫn và dải dòng đọc được ngay cả khi viewer không sinh anchor.

## Cách đọc an toàn

1. Bắt đầu từ báo cáo hợp nhất và kiểm tra nhãn trước mỗi kết luận.
2. Với claim binary, theo citation về một trong ba báo cáo EVKey; địa chỉ được kèm khi báo cáo nguồn có.
3. Với thuật toán gõ, phân biệt rõ **đã quan sát ở EVKey integration** với **baseline UniKey**.
4. Dùng clean-room spec làm hợp đồng implementation mới, không chép pseudocode/decompiler output.
5. Dùng test matrix theo cột “mức bằng chứng”; kết quả ghi “baseline/proposed oracle” không được quảng bá thành hành vi EVKey đã quan sát.

## Ràng buộc clean-room

Nhóm đặc tả chỉ sử dụng sự kiện/hợp đồng hành vi đã trích dẫn; nhóm triển khai nên không tiếp xúc binary, pseudocode hoặc mã nguồn không được phép. Trước khi tái sử dụng bất kỳ mã UniKey nào, phải đọc license thực tế tại repository/commit tương ứng; hồ sơ này không suy đoán hay tuyên bố license ngoài các file đã quan sát.
