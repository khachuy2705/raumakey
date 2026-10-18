# Thiết kế viết lại Raumakey bằng Rust

**Ngày:** 2026-10-16 (cập nhật lần cuối 2026-10-18: bổ sung trạng thái triển khai thành §20;
chuyển sang bản portable một file ở §21; bảng cấu hình và danh sách gõ tắt ở §22)

**Trạng thái:** Đã được duyệt; đang triển khai (xem §20). §21 thay thế các phần nói về TIP TSF,
installer và thư mục dữ liệu; khi có mâu thuẫn, §21 thắng. §22 bổ sung bảng cấu hình cho §21 và
thắng §21 ở những chỗ hai mục khác nhau.

**Sản phẩm:** Raumakey

**Kho mã:** `https://github.com/khachuy2705/raumakey.git`

## 1. Mục đích

Raumakey là một kiểu gõ tiếng Việt cho Windows, tương thích GPL, được viết lại bằng Rust. Nó cung cấp một controller và giao diện cấu hình Win32 native, một TSF Text Input Processor dạng COM, các kiểu gõ Telex/VNI/VIQR/tự định nghĩa, macro, chuyển đổi văn bản, chính sách tương thích riêng cho từng ứng dụng, và migration (chuyển dữ liệu) an toàn, chỉ đọc, từ dữ liệu EVKey.

Release công khai đầu tiên là một release duy nhất tương đương đầy đủ (full parity) thay vì một chuỗi các release từng phần. Quá trình phát triển có thể dùng các mốc nội bộ, nhưng không mốc nào được trình bày như sản phẩm đã hoàn thành.

## 2. Tiêu chí thành công

Release phải:

1. Chạy trên Windows 10 trở lên trên x86_64 sau khi người dùng tự chạy test nghiệm thu.
2. Biên dịch chéo (cross-compile) controller, TIP và các artifact hỗ trợ cho các target MSVC i686, x86_64 và ARM64.
3. Hỗ trợ Telex, VNI, VIQR và ánh xạ phím tự định nghĩa; đặt dấu kiểu cũ/kiểu mới; backspace theo ngữ nghĩa; khôi phục từ không hợp lệ; đầu ra NFC và NFD; macro; chuyển đổi văn bản trên clipboard/tệp; và chính sách theo từng ứng dụng.
4. Cung cấp một tray controller Win32 native gọn nhẹ cùng các hộp thoại.
5. Đăng ký một danh tính COM/TSF mới của Raumakey mà không thay thế hay mạo danh EVKey.
6. Nhập các thiết lập và macro EVKey được hỗ trợ mà không sửa đổi hay xoá dữ liệu nguồn.
7. Đáp ứng các gate về bảo mật, quyền riêng tư, TDD, review và kiểm chứng trong tài liệu này.

Một lần cross-build thành công, dù trên host Linux hay host Windows, không phải là bằng chứng cho tính đúng đắn khi chạy thật trên Windows. Nghiệm thu khi chạy thật trên x86_64 vẫn đang chờ cho đến khi harness Windows và ma trận kiểm tra thủ công đạt trên Windows 10 trở lên.

## 3. Danh tính sản phẩm

Raumakey dùng các danh tính cố định, khác biệt với EVKey:

- CLSID COM của TSF: `{0FC48F2B-4F91-44A0-B500-DE512E801432}`
- GUID profile ngôn ngữ TSF: `{DB875058-BBF9-407C-85A8-950BE7D14C22}`
- Upgrade code của installer: `{BF73893F-FCEB-4BAE-AE1C-E81D7107ECE5}`
- Tiền tố named object: `Local\\Raumakey.` theo sau là tên object được định danh theo session/người dùng
- Gốc registry thuộc sở hữu của sản phẩm: `HKCU\\Software\\Raumakey`

Thành phần migration được phép đọc các đường dẫn EVKey đã biết. Không thành phần nào của Raumakey được phép đăng ký CLSID/profile của EVKey, ghi vào bên dưới gốc registry của EVKey, dùng thương hiệu EVKey, hoặc ghi đè một tệp EVKey.

## 4. Ranh giới pháp lý và bằng chứng

Hồ sơ tái dựng ban đầu được lưu giữ dưới `restricted_evidence/` và bị loại khỏi Git. Các agent triển khai và review mã không được đọc thư mục đó. Chúng chỉ được dùng thiết kế đã được duyệt, kế hoạch triển khai, đặc tả hành vi clean-room, các vector tương thích, mã đã được viết trong kho mã, và mã nguồn upstream có giấy phép hợp lệ.

Trước khi tái sử dụng hoặc chuyển dịch mã nguồn UniKey, phần triển khai phải ghi lại chính xác URL kho mã, commit bất biến, văn bản giấy phép, các tệp bị ảnh hưởng và các thông báo bắt buộc vào `THIRD_PARTY.md`. Phiên bản giấy phép tương thích GPL cuối cùng của kho mã phải khớp với các nghĩa vụ upstream thực tế; không được phỏng đoán. Một bản chuyển dịch mã nguồn được coi là tác phẩm phái sinh và giữ nguyên ghi công.

Đầu ra decompiler của EVKey, pseudocode, resource, icon, chuỗi, bố cục class và luồng điều khiển là những đầu vào bị cấm dùng cho việc triển khai.

## 5. Kiến trúc

Raumakey là một Cargo workspace với các phụ thuộc hướng vào trong.

```text
crates/
  raumakey-model        shared domain types and invariants
  raumakey-engine       platform-independent Vietnamese state machine
  raumakey-macro        macro matching, case adaptation, persistence model
  raumakey-config       versioned configuration and atomic file storage
  raumakey-policy       host capability and transport selection
  raumakey-ipc          typed control channel and gated legacy codec
  raumakey-windows      narrow safe wrappers around Win32 and COM
  raumakey-controller   native EXE: tray, dialogs, hooks, lifecycle
  raumakey-tsf          COM in-process TIP cdylib
  raumakey-converter    text and clipboard conversion
  raumakey-migrate      read-only EVKey importer
  raumakey-resources    pure-Rust RES/ICO/manifest/version-info writer (build-dependency of the binaries)
xtask/                  build, inspection, packaging, and verification commands
tests/                  shared fixtures, contract suites, and Windows harness
fuzz/                   cargo-fuzz targets (separate workspace, excluded from the main one)
```

`raumakey-resources` và `fuzz/` được bổ sung bởi kế hoạch triển khai (§1.1, §11, §12).

`raumakey-model` và `raumakey-engine` không được phụ thuộc vào Windows API, HWND, interface COM, kiểu registry, handle, con trỏ thô (raw pointer), API clipboard, hay trạng thái khả biến toàn cục của tiến trình. Các adapter Windows phụ thuộc vào các crate lõi, không bao giờ theo chiều ngược lại.

### 5.1 Các hợp đồng lõi

Các hợp đồng chuẩn là `InputEvent`, `EngineOptions`, `EngineState`, `EditTransaction`, `HostContext`, `HostPolicy` và các lỗi có kiểu (typed error) được mô tả trong `docs/reference/evkey_clean_room_spec.md`, với số phiên bản và danh tính thuộc sở hữu của Raumakey.

`EngineState` có phạm vi theo session và không chứa tài nguyên nào của hệ điều hành. `EditTransaction.delete_before` đếm các code unit UTF-16 mà host nhìn thấy, được kiểm tra phạm vi trước khi áp dụng, và không bao giờ tách đôi một cặp surrogate.

### 5.2 Đường TSF

> **Thay đổi ngày 2026-10-17 (D38, D39, §21):** bản phát hành không còn dùng đường TSF. Đường gõ
> chính là hook bàn phím của controller (§5.3, §21.2). Mục này mô tả TIP, vẫn được giữ trong kho
> nhưng không được phát hành.

Đường nhập liệu mặc định là:

```text
physical key
  -> TsfAdapter normalizes InputEvent
  -> in-process VietnameseEngine processes session state
  -> EditTransaction
  -> one TSF edit session applies the transaction atomically
  -> successful apply commits next EngineState
```

Với đường TSF, engine chạy bên trong tiến trình của TIP. Phím gõ thô và văn bản tài liệu không được chuyển tới controller. Mỗi phiên kích hoạt/tài liệu TSF sở hữu một executor tuần tự và trạng thái độc lập. Các snapshot cấu hình và macro là bất biến trong suốt thời gian của một yêu cầu.

Nếu việc áp dụng qua TSF thất bại, context thay đổi, hoặc vùng khả dụng ngắn hơn `delete_before`, adapter không commit trạng thái kế tiếp được đề xuất. Nó kết thúc composition và reset hoặc cho phím đi qua (pass through) tuỳ theo chính sách.

### 5.3 Đường dự phòng (fallback)

> **Thay đổi ngày 2026-10-17 (D39, §21.2):** trong bản portable, đây là đường gõ duy nhất: mọi host
> dùng transport `SyntheticKey` qua hook bàn phím của `raumakey.exe`.

`CompatibilityPolicy` chọn đúng một transport cho mỗi host:

1. `TSF` theo mặc định.
2. `SyntheticKey` khi một quy tắc host tường minh hoặc một lần thăm dò khả năng (capability probe) thất bại đòi hỏi điều đó.
3. `Clipboard` chỉ khi được bật tường minh và không bao giờ dùng trong ô bảo mật.

Controller sở hữu trạng thái session của đường dự phòng, được phân vùng theo process ID, thread ID, epoch foreground, và danh tính tài liệu khi có. Các phím được phát lại mang một sentinel riêng của Raumakey và bị hook bỏ qua. Không phím vật lý nào được phép xử lý bởi nhiều hơn một transport.

Đường dự phòng clipboard chụp snapshot quyền sở hữu sequence của clipboard, chỉ ghi trong suốt thời gian của thao tác, và chỉ khôi phục dữ liệu trước đó nếu sequence của clipboard vẫn chứng minh rằng Raumakey sở hữu lần cập nhật mới nhất. Nó không bao giờ khôi phục đè lên nội dung clipboard mới hơn của một ứng dụng khác.

## 6. Kiểu gõ và hành vi của engine

Engine hỗ trợ:

- Telex, VNI, VIQR và các bảng phím ngữ nghĩa tự định nghĩa có phiên bản.
- Kiểm tra tính hợp lệ của chuỗi nguyên âm/phụ âm tiếng Việt.
- Xử lý phụ âm đầu `qu` và `gi`.
- Đặt dấu kiểu cũ/kiểu mới có thể cấu hình.
- Huỷ dấu phụ/dấu thanh khi gõ lặp, với đường dự phòng là ký tự nguyên văn.
- Backspace theo ngữ nghĩa dùng lịch sử trạng thái có giới hạn.
- Kiểm tra chính tả và tuỳ chọn khôi phục các từ đã chuyển đổi nhưng không hợp lệ về phím gõ thô.
- Các chế độ chuẩn hoá NFC, NFD và giữ nguyên (preserve) mà không tạo ra UTF-16 sai dạng.
- Giữ nguyên kiểu chữ thường/hoa chữ đầu/hoa toàn bộ.

Các hành động ngữ nghĩa không hợp lệ không được âm thầm nuốt mất phím gốc. Mọi phép biến đổi được biểu diễn dưới dạng một `EditTransaction`; việc chèn đầu ra là trách nhiệm của adapter.

Ma trận tương thích là hợp đồng hành vi ban đầu. Các vector baseline vẫn được gắn nhãn baseline cho đến khi một quan sát hợp pháp trên EVKey lúc chạy thật hoặc một quyết định sản phẩm tường minh của Raumakey xác lập điều khác.

## 7. Macro và chuyển đổi

Mỗi mục macro chứa một ID ổn định, trigger NFC, nội dung thay thế NFC, cờ bật, chính sách ranh giới, và chính sách chữ hoa/thường. Việc tra cứu là tất định và dựa trên Unicode scalar, không dựa trên offset byte. Việc bung macro (expansion) chỉ xảy ra tại một ranh giới đã được cấu hình. Việc xử lý macro trong lúc chế độ tiếng Việt đang tắt chỉ có thể hoạt động khi người dùng bật tuỳ chọn đó.

Bộ chuyển đổi hỗ trợ chuyển đổi văn bản trong bộ nhớ và các thao tác tường minh trên clipboard/tệp. Theo mặc định, chuyển đổi tệp không bao giờ ghi đè tệp nguồn. Chuyển đổi clipboard bị tắt đối với ô bảo mật và báo cáo xung đột quyền sở hữu thay vì thay thế nội dung clipboard mới hơn.

## 8. Cấu hình và lưu trữ bền vững

> **Thay bởi D40 (§21.3):** thư mục dữ liệu là thư mục chứa `raumakey.exe`, với các tệp
> `raumakey.toml`, `raumakey-macros.toml`, …; `%LOCALAPPDATA%\Raumakey` chỉ dùng khi thư mục đó
> không ghi được. Chế độ portable bằng tệp đánh dấu bị bỏ. Phần còn lại của mục này (revision, ghi
> nguyên tử, xung đột) vẫn áp dụng.

Thư mục dữ liệu gốc mặc định:

```text
%LOCALAPPDATA%\\Raumakey\\
  config.toml
  macros.toml
  app_rules.toml
  migration.json
  logs\\
```

Chế độ portable chỉ được bật bởi một tệp đánh dấu `raumakey.portable` nằm cạnh tệp thực thi. Nó không bao giờ được suy ra chỉ từ việc có quyền ghi.

Mỗi store chứa `version` và `revision` tăng đơn điệu. Một lần ghi dùng một tệp tạm trong cùng thư mục, flush bền vững, `ReplaceFileW`, và một checksum đọc lại. Việc kiểm tra revision lạc quan trả về `Conflict` thay vì âm thầm ghi đè lên một bên ghi khác. Các trường không xác định khi migration được giữ lại trong một extension map.

Controller công bố các revision cấu hình/macro bất biến tới các client TIP qua kênh điều khiển có kiểu. Một TIP bị mất kết nối tiếp tục dùng snapshot hợp lệ cuối cùng của nó và kết nối lại với exponential backoff có giới hạn.

## 9. Migration

Migration phát hiện các nguồn registry, thiết lập, ngoại lệ và macro EVKey được hỗ trợ. Nó:

1. Mở các nguồn ở chế độ chỉ đọc.
2. Phân tích từng trường một cách độc lập, cách ly các trường bị hỏng thay vì reset toàn bộ cấu hình.
3. Giữ lại các trường không xác định và các byte nguồn thô cần cho bằng chứng hoàn tác (rollback).
4. Chỉ chuẩn hoá những trường mà schema mới đòi hỏi.
5. Commit dữ liệu Raumakey một cách nguyên tử.
6. Đọc lại và kiểm chứng revision/checksum.
7. Chỉ đánh dấu migration hoàn tất sau khi kiểm chứng.

Migration có tính luỹ đẳng (idempotent). Nó không bao giờ xoá, đổi tên, ghi lại, huỷ đăng ký, hay thay đổi quyền trên một nguồn EVKey.

## 10. IPC và ranh giới tiến trình

Kênh điều khiển thông thường là một named pipe cùng người dùng, cùng session, có ACL cấp toàn quyền cho SID của người dùng và, theo D27 của kế hoạch, chỉ cấp quyền truy cập client cho các tiến trình AppContainer của chính người dùng đó; client từ chối một pipe có mandatory label thấp hơn medium. Các frame chứa phiên bản giao thức, loại thông điệp, độ dài payload, danh tính client, request ID và checksum. Độ dài được kiểm tra tính hợp lệ trước khi cấp phát. Kênh thông thường mang cấu hình, snapshot macro, lệnh điều khiển, và chẩn đoán opt-in (chỉ bật khi người dùng chọn); nó không mang phím gõ thô của người dùng hay văn bản tài liệu.

Wire codec cũ (legacy) của EVKey là một Cargo feature tuỳ chọn tên `evkey-compat`, mặc định bị tắt trong các build release. Nó giữ nguyên độ rộng trường, offset, cách khớp sequence, timeout và các quy tắc kiểm tra hợp lệ đã được tài liệu hoá. Việc kích hoạt thất bại theo hướng đóng (fail closed) nếu đã tồn tại một named object EVKey xung đột. Một transaction legacy không bao giờ được thử lại sau một timeout mơ hồ, vì việc phát lại có thể áp dụng văn bản hai lần.

## 11. Controller native và UI

> **Thay đổi ngày 2026-10-17 (D42, §21.5):** release đầu tiên chỉnh tuỳ chọn bằng menu khay; bảng
> cài đặt và các tab được hoãn lại.
>
> **Thay đổi ngày 2026-10-18 (D43–D45, §22):** có bảng cấu hình một trang, không có tab, và hộp
> thoại danh sách gõ tắt. Các tab quy tắc ứng dụng, chuyển đổi và phím tắt vẫn được hoãn lại.

Controller dùng một cửa sổ Win32 ẩn, một icon tray, và các hộp thoại native dạng modeless/modal. Bảng thu gọn hiển thị chế độ tiếng Việt, kiểu gõ, kiểu đặt dấu, và nút mở rộng/thu gọn. Các tab mở rộng bao gồm nhập liệu, phím tắt, macro, quy tắc ứng dụng, chuyển đổi, hành vi hệ thống, và thông tin sản phẩm.

Thread UI không thực hiện công việc engine, tệp, mạng, hay IPC chặn (blocking). Công việc nền gửi (post) các kết quả có kiểu về cửa sổ ẩn. `TaskbarCreated` tạo lại icon tray mà không thay đổi chế độ. Mọi resource và icon mà người dùng nhìn thấy đều được tạo mới cho Raumakey.

Phím tắt được phân loại trước khi vào engine và bao gồm phím bật/tắt chế độ và phím hard reset mà người dùng cấu hình được. Việc xử lý phím bổ trợ phân biệt AltGr với các tổ hợp Ctrl+Alt thông thường. Một lần reset sẽ commit hoặc kết thúc composition hiện tại mà không thay đổi văn bản đã được commit.

## 12. Đăng ký COM/TSF và vòng đời

> **Thay đổi ngày 2026-10-17 (D38, §21.1):** TIP không còn được phát hành hay đăng ký. Mục này áp
> dụng khi TIP được phát hành trở lại.

TIP là một `cdylib` Rust chỉ export các hàm COM bắt buộc. Mọi entry point FFI và COM đều bắt panic và chuyển lỗi thành giá trị HRESULT; không có unwind nào vượt qua ranh giới ABI.

Việc đăng ký dùng CLSID/GUID profile cố định của Raumakey, registry view WOW64 đúng với bitness của DLL, các category TIP bàn phím, và các khả năng đã khai báo. Trình tự của installer là đăng ký COM, cài đặt profile ngôn ngữ, kiểm chứng trạng thái registry/profile, rồi commit trạng thái installer. Khi thất bại, chỉ các mục thuộc sở hữu của Raumakey được hoàn tác (rollback).

Việc kích hoạt tạo trạng thái riêng cho từng tài liệu và advise các sink cần thiết. Việc huỷ kích hoạt kết thúc composition, unadvise các sink, gỡ các hook, huỷ các công việc có giới hạn, và giải phóng mọi tài nguyên. Điều kiện cho phép unload DLL phản ánh số lượng COM object và lock còn tồn tại.

## 13. Ranh giới khởi động, installer và cập nhật

> **Thay bởi D38 và D41 (§21.1, §21.4):** không có installer, MSI hay script cài đặt; package x86_64
> chỉ chứa `raumakey.exe` và tài liệu. Khởi động cùng Windows là một mục của menu khay, tạo
> shortcut trong thư mục Startup. Phần về manifest `asInvoker`, CRT tĩnh, ARM64 compile-gated và
> updater vẫn áp dụng.

Manifest mặc định của tệp thực thi là `asInvoker` với `uiAccess=false`. Tự khởi động thông thường dùng một shortcut trong thư mục Startup. Một scheduled task được nâng quyền chỉ được tạo sau khi người dùng chọn tường minh, và được gỡ bỏ một cách idempotent khi bị tắt.

`xtask` điều khiển việc biên dịch chéo, kiểm tra PE, tạo installer, manifest của package, checksum, và thu thập artifact. Thông tin phiên bản, icon, và manifest của các tệp thực thi được nhúng lúc build: `build.rs` của mỗi crate binary gọi `raumakey-resources` và truyền tệp `.res` cho linker, nên không cần `rc.exe` (kế hoạch D1; các tệp `build.rs` chưa tồn tại, xem §20). Các binary được phát hành link tĩnh MSVC CRT, nên TIP không bao giờ phụ thuộc vào Visual C++ redistributable (kế hoạch D23). Package x86_64 chứa controller x86_64 và cả hai TIP x86_64 và i686. Các artifact ARM64 chỉ kiểm tra biên dịch (compile-gated) và được đóng gói riêng cho đến khi có kiểm chứng khi chạy thật.

Release đầu tiên không bắt chước giao thức updater chưa rõ của EVKey. Logic cập nhật nằm ngoài lõi tin cậy của engine và controller. Một updater trong tương lai phải yêu cầu manifest đã được xác thực và kiểm chứng hash cùng chữ ký của package trước khi cài đặt. Cho đến khi có chứng chỉ ký, các package được báo cáo tường minh là chưa ký.

## 14. Bảo mật và quyền riêng tư

- Ô bảo mật/ô mật khẩu bỏ qua phép biến đổi, macro, transport clipboard, và chẩn đoán nội dung. Chế độ opt-in `Restricted` giữ phép biến đổi tiếng Việt trong ô bảo mật nhưng vẫn không bao giờ bung macro hay dùng clipboard (kế hoạch D25).
- Named object và pipe bị giới hạn trong người dùng/session hiện tại; pipe điều khiển cũng chấp nhận các client AppContainer của cùng người dùng nhưng chỉ với quyền truy cập client, và client từ chối một pipe được tạo dưới mức toàn vẹn trung bình (medium integrity) (kế hoạch D27).
- Mọi độ dài, offset, sequence, giá trị UTF-16 và trường cấu hình đến từ bên ngoài đều được kiểm tra tính hợp lệ trước khi dùng.
- Không có telemetry mặc định.
- Log opt-in có thể chứa timestamp, thành phần, mã lỗi, revision, basename tệp thực thi của host, và ID test vector; chúng không được chứa phím thô, văn bản tài liệu, dữ liệu clipboard, trigger/nội dung thay thế của macro, hay đường dẫn đầy đủ chứa nội dung của người dùng.
- Panic, lock bị poisoned, lỗi COM, lỗi I/O và timeout đều có đường phục hồi có kiểu.
- Kiểm toán phụ thuộc và giấy phép là các cổng phát hành.

## 15. Chiến lược test

### 15.1 Test không phụ thuộc host

Mọi hành vi production đều tuân theo chu trình red-green-refactor. Workspace chạy unit test và integration test cho hành vi engine, macro, store, chính sách, codec, fixture migration, và hành vi bộ chuyển đổi. Property test khẳng định đầu ra tất định, UTF-16 hợp lệ, áp dụng transaction an toàn, và các bất biến trạng thái. Các fuzz target bao phủ chuỗi đầu vào, UTF-16 sai dạng, cấu hình, tệp macro, frame IPC, sequence quay vòng (wrap), và ranh giới transaction.

Windows API được đặt sau các port hẹp để commit/rollback trạng thái, thay đổi context, ánh xạ HRESULT, và lập kế hoạch đăng ký có thể được test bằng fake trên bất kỳ host nào.

Trên host Windows, `raumakey-windows` còn chạy integration test Win32 native. Các test có tác dụng phụ trên toàn máy hoặc người dùng nhìn thấy được (hook toàn cục, `SendInput`, clipboard thật, icon thông báo, scheduled task, `regsvr32`, nâng quyền, đăng ký TSF) được đánh dấu `#[ignore]` và chạy thủ công.

### 15.2 Các gate cross-build

Cổng phát hành cross-build `i686-pc-windows-msvc`, `x86_64-pc-windows-msvc` và `aarch64-pc-windows-msvc` bằng LLVM. Trên host Windows không có Visual Studio, các target link bằng `rust-lld` với MSVC CRT và các import library của Windows SDK do `xwin` tạo ra, được cấu hình trong Cargo home cục bộ của máy. Trên host Linux, `cargo xwin` cung cấp chúng (kế hoạch D1). Cổng này kiểm tra tính hợp lệ của export, kiến trúc PE, manifest, resource, danh tính COM, và danh mục artifact dự kiến.

### 15.3 Harness nghiệm thu trên Windows

`raumakey-test-runner.exe` thực thi các vector tất định và xuất JSON chứa test ID, OS/build, kiến trúc, ứng dụng/phiên bản, revision cấu hình, chuỗi transaction, văn bản cuối cùng, và trạng thái pass/fail. Nó không bao giờ ghi lại đầu vào không liên quan của người dùng.

Một bộ test PowerShell kiểm chứng việc cài đặt, cả hai registry view, đăng ký COM, cài đặt profile, các chế độ khởi động, nâng cấp, hoàn tác (rollback), và gỡ cài đặt. Nghiệm thu thủ công bao gồm Notepad, Office, Edge/Chrome, Windows Terminal, ô mật khẩu, khởi động lại Explorer, nhiều tài liệu, chuyển layout bàn phím, và chính sách ứng dụng.

## 16. Cổng chất lượng

Với mỗi task triển khai:

1. Một test liên quan được viết và được quan sát thấy fail vì đúng lý do dự định.
2. Mã production tối thiểu làm cho test đó pass.
3. Toàn bộ suite áp dụng được đều pass mà không có lần fail nào bị bỏ qua.
4. Một reviewer độc lập kiểm tra việc tuân thủ đặc tả và chất lượng mã.
5. Các phát hiện quan trọng được sửa và review lại trước task phụ thuộc kế tiếp.

Trước khi đóng gói:

- `cargo fmt --check`
- Clippy cho workspace, không cho phép cảnh báo (warnings denied)
- Toàn bộ test của workspace
- Các bộ smoke test property và fuzz
- Kiểm toán lỗ hổng phụ thuộc và giấy phép
- Cả ba cross-build
- Kiểm tra PE/export/manifest/resource
- Kiểm tra hợp đồng chạy thử (dry run) của installer
- Review độc lập trên toàn bộ branch
- Manifest SHA-256 cho mọi artifact

## 17. Các giai đoạn bàn giao

Thứ tự triển khai nội bộ là:

1. Workspace, nguồn gốc (provenance), toolchain, và các hợp đồng lõi.
2. Engine và các kiểu gõ.
3. Macro, chuyển đổi, store, và migration.
4. Chính sách, IPC điều khiển, và codec legacy.
5. Controller native, tray, hộp thoại, phím tắt, và đường dự phòng.
6. TIP COM TSF và đăng ký.
7. Installer, cross-build, harness Windows, và tài liệu.
8. Gia cố (hardening), kiểm toán, review toàn bộ, và release candidate.

Các giai đoạn này là các gate kỹ thuật, không phải các release sản phẩm từng phần.

## 18. Những mục ngoài phạm vi được nêu rõ cho release đầu tiên

- Tái sử dụng thương hiệu, GUID, icon, resource, hay giao thức updater của EVKey.
- Tuyên bố tương đương từng byte hoặc tương đương về cách triển khai với EVKey.
- Tuyên bố hỗ trợ khi chạy thật cho ARM64 hoặc i686 trước khi các kiến trúc đó được chạy trên hệ thống Windows thật.
- Telemetry trên cloud, đồng bộ tài khoản, cập nhật tự động qua mạng, hoặc UI dựa trên web.
- Xoá hoặc sửa đổi dữ liệu ứng dụng cũ trong quá trình migration.

## 19. Câu chữ nghiệm thu cuối cùng

Dự án chỉ được báo cáo là **đã kiểm chứng cross-build (cross-build verified)** sau khi mọi cổng chất lượng không phụ thuộc host và cả ba build target MSVC đều đạt với bằng chứng mới. Dự án chỉ được báo cáo là **đã kiểm chứng khi chạy thật trên x86_64 với Windows 10+ (runtime x86_64 verified on Windows 10+)** sau khi các kết quả nghiệm thu Windows tự động và thủ công do người dùng chạy đều đạt và được đính kèm vào bằng chứng release.

## 20. Trạng thái triển khai (tính đến 2026-10-17)

Đợt tiếp nối (follow-up) ở §20.11 thay thế trạng thái Git và `xtask` dưới đây. Các mục khác
vẫn là một snapshot lịch sử, không phải bằng chứng rằng các bước kiểm tra của chúng đã được đợt
tiếp nối thực hiện lại. Kế hoạch §14 theo dõi trạng thái hiện tại theo từng task; kế hoạch §14.5
thay thế các bước tiếp theo ở §20.10.

Mục này là một snapshot có ghi ngày về những gì kho mã chứa và những việc còn lại. Mục này không thay đổi các yêu cầu trong §1–§19. Bằng chứng được thu thập ngày 2026-10-17 trên host phát triển Windows 11 x86_64 (Rust 1.99.1, `rust-lld` + `xwin`, kế hoạch D1). Bằng chứng đến từ việc chạy các gate trong §20.2 và từ một đợt rà soát chỉ đọc (read-only audit) mọi crate so với kế hoạch triển khai, trong đó mỗi phát hiện được kiểm tra lần thứ hai với mã nguồn. Không có nội dung nào ở đây được rút ra từ `restricted_evidence/`.

Cùng ngày đó, §2, §5, §13, §15 và §19 đã được điều chỉnh cho khớp với các quyết định D1 và D23 của kế hoạch và với crate `raumakey-resources` mà kế hoạch đã bổ sung. Không có yêu cầu nào thay đổi.

### 20.1 Tóm tắt

- **Đã triển khai (thư viện):** mười crate thư viện và `xtask` đã được triển khai và có test: `raumakey-model`, `-engine`, `-macro`, `-config`, `-policy`, `-ipc` (bao gồm codec `evkey-compat`), `-converter`, `-migrate`, `-resources` và `-windows`. Tám thư viện đã hoàn chỉnh so với kế hoạch; `raumakey-migrate`, `raumakey-windows` và `xtask` gần hoàn chỉnh (§20.4). Tổng cộng chúng có khoảng 39k dòng mã nguồn (bao gồm unit test nằm trong cùng tệp) và 33k dòng integration test. 1,235 test đạt, hoặc 1,309 test khi bật `evkey-compat`.
- **Chưa bắt đầu (sản phẩm):** controller (`raumakey.exe`) và TSF TIP (`raumakey_tip.dll`) là các stub rỗng. Chưa có mã thư viện nào chạy trong một binary phát hành, và chưa có hàng nào của ma trận được quan sát trong một ứng dụng chủ Windows thật.
- **Chưa bắt đầu (kiểm chứng):** package `raumakey-tests` là một stub rỗng; binary `raumakey-test-runner` của nó là `fn main() {}` nhưng vẫn được build và đóng gói. Các fixture, bộ test hợp đồng (contract suite), bộ fuzz smoke, workspace `fuzz/`, bộ test nghiệm thu PowerShell và danh sách kiểm tra thủ công đều chưa tồn tại.
- **Chưa có trong Git:** ngoài commit tài liệu `771c3af`, mọi thứ đều chưa được theo dõi (untracked): 271 tệp, bao gồm `Cargo.lock`, kế hoạch, `LICENSE` và `THIRD_PARTY.md`.
- **Cách diễn đạt khi phát hành (§19):** không thể tuyên bố "cross-build verified" lẫn "runtime x86_64 verified". Cổng phát hành sẽ dừng ở bước kiểm tra PE. Ngay cả khi `xtask release-gate` đạt thì cũng chưa đủ cơ sở cho "cross-build verified", dù nó ghi lại đúng cách diễn đạt đó: nó không có bước review toàn branch, chỉ ghi chú việc thiếu bộ fuzz smoke, và không bao giờ chạy các test `raumakey-windows/evkey-compat` (§20.6, mục 7).

### 20.2 Bằng chứng của các gate

| Gate | Lệnh | Kết quả ngày 2026-10-17 |
|---|---|---|
| Định dạng | `cargo fmt --all --check` | Đạt |
| Lint | `cargo clippy --workspace --all-targets -- -D warnings`, với feature mặc định và với `raumakey-ipc/evkey-compat,raumakey-windows/evkey-compat` | Đạt |
| Test | `cargo test --workspace` | 1,235 đạt, 0 không đạt, 8 bị bỏ qua |
| Test với codec legacy | như trên, với `--features raumakey-ipc/evkey-compat,raumakey-windows/evkey-compat` | 1,309 đạt, 0 không đạt, 8 bị bỏ qua |
| Tác vụ CI | `cargo xtask ci` | Báo đạt, nhưng bỏ qua bộ fuzz smoke (không có target `fuzz_smoke`) và không bao giờ bật `raumakey-windows/evkey-compat`, nên 13 test của adapter legacy và các lint của module đó không bao giờ chạy |
| Cross-build | `cargo xtask cross` và `cargo xtask cross --release` | Đạt cho x86_64, i686 và aarch64, build ra các binary stub |
| Kiểm tra PE | `cargo xtask inspect` và `--release` | **Không đạt**: 27 trên 57 (debug) và 27 trên 66 (release) bước kiểm tra. `raumakey.exe` là một binary console không có tài nguyên manifest, phiên bản hay icon; `raumakey_tip.dll` thiếu bốn export COM, các tài nguyên của nó và các GUID định danh. Bước kiểm tra CRT tĩnh (D23) đạt cho cả chín artifact release |
| Rà soát phụ thuộc | `cargo xtask audit` (cargo-deny 0.20.2) | Đạt, kèm cảnh báo về một `syn` trùng lặp và các giấy phép được cho phép nhưng không dùng đến |
| Đóng gói | `cargo xtask package` | Tạo các package debug x64 và ARM64 (hậu tố `-debug`) và `raumakey-debug.wxs` từ các binary stub; `package --release` chưa được chạy; MSI bị bỏ qua vì chưa cài `wix`; chưa ký |
| Cổng phát hành | `cargo xtask release-gate` | Chưa chạy. Nó sẽ dừng ở `inspect` và ghi `package` cùng bước chạy thử (dry run) của installer là chưa chạy. Khi `inspect` đạt, bước chạy thử vẫn sẽ không đạt, vì `raumakey.exe` dạng stub không in ra kế hoạch cài đặt nào nêu tên CLSID và GUID của profile |
| Nghiệm thu Windows | harness §15.3 và ma trận thủ công | Chưa có |

8 test bị bỏ qua là các test thủ công của `raumakey-windows` được mô tả ở §15.1. Không có ghi nhận nào cho thấy chúng từng được chạy. `cargo xtask` là một alias chỉ được định nghĩa trong cấu hình Cargo cục bộ của máy; một bản clone mới phải dùng `cargo run -p xtask -- <command>`.

### 20.3 Trạng thái theo giai đoạn bàn giao (§17)

| Giai đoạn | Trạng thái | Ghi chú |
|---|---|---|
| 1. Workspace, nguồn gốc (provenance), toolchain, hợp đồng lõi | Xong, chưa commit | Workspace (resolver 3, edition 2024), toolchain ghim ở 1.99.1 với ba target MSVC, `LICENSE` GPL-3.0, `THIRD_PARTY.md`, `deny.toml`, `raumakey-model` |
| 2. Engine và các kiểu gõ | Xong (thư viện) | Mọi golden vector của kế hoạch §3.10 và mọi hàng ma trận ở mức engine đều có test; chưa có binary nào gọi engine |
| 3. Macro, chuyển đổi, store, migration | Gần xong (thư viện) | Còn thiếu: nguồn registry Windows cho migration, việc dò tìm các thư mục EVKey mặc định, việc nối vào dòng lệnh |
| 4. Chính sách, IPC điều khiển, codec legacy | Xong (thư viện) | Còn thiếu ở phía controller: một server hub nhiều client và một bộ dựng snapshot |
| 5. Controller, tray, hộp thoại, phím tắt, đường dự phòng (fallback) | Chỉ có các khối xây dựng | `raumakey-resources` đã sẵn sàng. `raumakey-windows` có các wrapper cho hook, input, clipboard, pipe, shell và task scheduler, nhưng chưa có các wrapper cho cửa sổ, hộp thoại, `PostMessageW` và tải icon mà controller cần (§20.6, mục 3). `raumakey-controller` là `fn main() {}` |
| 6. TIP COM TSF và đăng ký | Chỉ có mã đăng ký | Planner, executor và phần xác minh có trong `raumakey-windows` và được test bằng fake; port đăng ký Windows mới chỉ chạy trong một test thủ công; `raumakey-tsf` chỉ là một doc comment |
| 7. Installer, cross-build, harness Windows, tài liệu | Một phần | `xtask` đã được triển khai (§20.2); harness, fixture, fuzzing, bộ test PowerShell và danh sách kiểm tra thủ công chưa bắt đầu |
| 8. Gia cố, rà soát (audit), review, bản release candidate | Chưa bắt đầu | `xtask audit` đạt; chưa có bản ghi review nào |

Các giai đoạn không diễn ra theo thứ tự: công cụ của giai đoạn 7 (`xtask`) đã có trong khi các binary của giai đoạn 5 và 6 (controller và TIP) chưa được bắt đầu. Kế hoạch §13 cho phép điều này, vì `xtask` không phụ thuộc vào binary nào trong hai binary đó.

### 20.4 Trạng thái theo thành phần

"Hoàn chỉnh" nghĩa là mọi yêu cầu của kế hoạch đối với thư viện đều có mã và test. Điều này không nói lên gì về hành vi runtime trên Windows.

| Thành phần | Kế hoạch | Trạng thái | Số test đạt | Thiếu sót chính |
|---|---|---|---|---|
| `raumakey-model` | §2 | Hoàn chỉnh | 44 | Các kiểu mang nội dung derive `Debug` (§20.7) |
| `raumakey-engine` | §3 | Hoàn chỉnh; một nhánh mã chưa có test | 125 | Chưa từng chạy với `MacroTable` thật; phương án dự phòng Circumflex của kế hoạch §3.6 khi tắt kiểm tra chính tả chưa có test (§20.8, mục 9); một số quyết định về ngữ pháp chưa được ghi lại (§20.9) |
| `raumakey-macro` | §4 | Hoàn chỉnh | 149 | Chưa được nối vào: controller phải nạp `macros.toml` vào một `MacroStore` bằng `from_entries` và lưu `entries()` trở lại, giữ nguyên `MacroDocument.entry_extras` và `extensions` (kế hoạch §4) |
| `raumakey-config` | §5 | Hoàn chỉnh | 136 | Chưa được nối vào; độ bền của commit (§20.7); các tệp `.lock` và `.bak-*` tích tụ dần |
| `raumakey-policy` | §6 | Hoàn chỉnh | 107 | Chưa được nối vào; giới hạn bản ghi probe khác nhau giữa các tầng (§20.7) |
| `raumakey-ipc` | §7 | Hoàn chỉnh | 107; 168 khi bật `evkey-compat` | Chưa có server hub hay bộ dựng snapshot; codec legacy chưa được kết nối với engine nào |
| `raumakey-converter` | §8 | Hoàn chỉnh | 127 | Chưa có CLI hay UI; `config.toml` lưu tên bảng mã dưới dạng chuỗi không được kiểm tra |
| `raumakey-migrate` | §9 | Gần hoàn chỉnh | 115 | Chưa có adapter `LegacyRegistry` cho Windows, chưa dò tìm thư mục mặc định, chưa có lệnh `migrate`; `FIELD_MAP` rỗng (D18) |
| `raumakey-resources` | §11 | Hoàn chỉnh | 82 | Chưa có `build.rs` nào gọi nó, nên chưa có binary nào mang manifest, thông tin phiên bản hay icon |
| `raumakey-windows` | §10.1, §10.4 | Gần hoàn chỉnh | 147 và 8 test thủ công; 160 khi bật `evkey-compat` | Phần xác minh đăng ký đọc các khoá registry `CTF\TIP` thay vì liệt kê profile như kế hoạch §10.4 yêu cầu (§20.7). Còn thiếu các wrapper mà controller và TIP cần: cửa sổ, cửa sổ message-only, hộp thoại, tải icon, đường dẫn module của DLL, `GetMessageExtraInfo`, sink TSF. Các đường thực thi có tác dụng phụ chỉ được chứng minh bằng test thủ công |
| `raumakey-controller` | §10.2 | Chưa bắt đầu | 0 | Toàn bộ kế hoạch §10.2 (§20.6) |
| `raumakey-tsf` | §10.3 | Chưa bắt đầu | 0 | Toàn bộ kế hoạch §10.3 (§20.6) |
| `xtask` | §12 | Gần hoàn chỉnh | 96 | Tập feature của CI, việc bỏ qua fuzz, gate review, các thông báo giấy phép (notice) (§20.6) |
| `raumakey-tests`, `fuzz/`, `tests/windows/` | §12 | Chưa bắt đầu | 0 | Fixture, bộ test hợp đồng, target fuzz, runner nghiệm thu, bộ test nghiệm thu |

### 20.5 Độ phủ của ma trận tương thích

Độ phủ trên 58 hàng của `docs/reference/evkey_compatibility_test_matrix.md`:

| Độ phủ | Hàng | Số lượng |
|---|---|---|
| Có test thư viện kiểm tra (assert) | #1–#8, #10–#18, #21–#23, #28, #30–#38, #45–#54 | 40 |
| Được kiểm tra một phần | #9, #25, #26, #27, #29, #41, #42, #44, #55, #56 | 10 |
| Chỉ ở runtime; các khối xây dựng đã có test | #19, #20, #24, #39, #40, #58 | 6 |
| Chưa được bao phủ | #57 (chưa có lệnh `update`) | 1 |
| Không áp dụng | #43 (cần các binary EVKey) | 1 |

- Chưa có hàng nào đáp ứng tiêu chí đạt của chính ma trận. Các tiêu chí đó yêu cầu mỗi lần chạy phải ghi lại kiến trúc, bản build Windows, ứng dụng chủ và phiên bản của nó, revision cấu hình, các sự kiện input, chuỗi transaction và văn bản cuối cùng, trong khi runner lẽ ra phải ghi lại chúng, `raumakey-test-runner`, là một stub rỗng.
- Các test engine cho các hàng macro #47–#52 dùng test double; chưa có test nào chạy engine với `raumakey_macro::MacroTable` thật.
- #9 chưa được kiểm tra với bảng phím tự định nghĩa. #18 chỉ chạm tới đường loại bỏ lịch sử (history eviction) khi giới hạn context được nâng lên trên mức mặc định.
- #30–#38 chỉ chạy với `--features raumakey-ipc/evkey-compat`, trên một fake tất định. Các test trên kernel object thật cần `raumakey-windows/evkey-compat`, thứ mà `xtask ci` bỏ sót. Các test tương đương (parity) của #42 mới chỉ chạy trên x86_64: không có gate nào chạy các test i686 dưới WOW64, còn ARM64 chỉ được biên dịch.
- Các quyết định của Raumakey thay thế giá trị kỳ vọng của ba hàng: #18 lùi về pass-through và reset thay vì op 2 legacy (kế hoạch §3.9), #27 lưu probe trong `app_rules.toml` (kế hoạch §6), và #39 gắn tag cho các lần replay bằng `RAUMAKEY_INJECTED_TAG` (D6).
- Crate policy mâu thuẫn với #29 (§20.8, mục 1).

### 20.6 Việc còn lại

Nhóm A chặn mọi tuyên bố về runtime; trong nhóm này, mục 3 là điều kiện tiên quyết của mục 1 và 2, và sau đó hai mục này có thể tiến hành song song. Trong nhóm B, chế độ `live` của mục 5 và phần lớn mục 6 cần nhóm A; mục 4 và 7 thì không. Nhóm C không phụ thuộc vào nhóm nào trong hai nhóm trên, và mục 8 được làm trước tiên (§20.10). Các thiếu sót thư viện trong §20.4 và các lỗi trong §20.7 cũng là việc còn lại; cần sửa những lỗi vi phạm một yêu cầu thiết kế trước khi phát hành, bắt đầu từ độ bền của commit (§8 yêu cầu một lần flush bền vững).

**A. Các binary phát hành (những mục này chặn mọi tuyên bố về runtime)**

1. **TSF TIP (`raumakey-tsf`, kế hoạch §10.3).**
   - Export `DllGetClassObject`, `DllCanUnloadNow`, `DllRegisterServer` và `DllUnregisterServer`, có panic guard bao bọc.
   - Một class factory và một `TextService` hiện thực `ITfTextInputProcessorEx` cùng năm sink của kế hoạch §10.3 (key event, thread manager event, text edit, thread focus, compartment event), cộng với vòng đời kích hoạt và huỷ kích hoạt.
   - Trạng thái theo từng tài liệu, và đường `OnTestKeyDown`/`OnKeyDown` áp dụng một edit session và chỉ commit trạng thái kế tiếp sau khi edit session đó thành công.
   - Các phím replay được gửi bằng `SendInput` và gắn tag `RAUMAKEY_INJECTED_TAG` sau một edit session thành công, cơ chế bỏ qua tương ứng thông qua `GetMessageExtraInfo` (ma trận #39), và việc phát hiện input scope mật khẩu.
   - Probe khả năng sửa range của TSF, được báo cáo bằng `Command::ReportProbe` (§5.3, ma trận #27), và các sự kiện `Diagnostic` tuỳ chọn bật (opt-in) chỉ được gửi khi `RuntimeSnapshot.diagnostics_enabled` được đặt (§10).
   - Thread client snapshot có backoff và mặc định v1, và việc phân loại phím tắt cục bộ khi mất kết nối (D24).
   - Bộ đếm object và lock, và một `build.rs` cho các tài nguyên của nó.
   - Đường legacy `evkey-compat`, đường này phải gắn tag và bỏ qua `0xEBCA1234` (đặc tả clean-room §3.1, §3.4).

   Cần thêm trước một port edit-session/range, để commit và hoàn tác (rollback) có thể được test bằng fake (§15.1). Adapter phải gọi `engine.reset()` khi tài liệu thay đổi: engine trả lời một sự kiện reset mang owner khác bằng `ContextMismatch`.
2. **Controller (`raumakey-controller`, kế hoạch §10.2).**
   - Subsystem GUI, gắn vào console khi chạy lệnh con.
   - CLI (`install`, `uninstall`, `startup`, `migrate`, `convert`, `update`, `--version`). CLI phải chấp nhận các tuỳ chọn theo bất kỳ thứ tự nào, vì `install.ps1` được đóng gói và cổng phát hành sắp xếp chúng theo thứ tự khác nhau.
   - Một instance duy nhất, cửa sổ ẩn, vòng lặp message và worker thread.
   - Tray với các icon V/E và xử lý `TaskbarCreated`; bảng cài đặt dạng gọn và các tab với chuỗi tiếng Việt mới. Quyết định xem các chuỗi đó có trở thành tài nguyên RT_STRING hay không; nếu có, mở rộng `raumakey_resources::Artifact::resources`, hàm hiện chưa phát ra string table nào.
   - Server hub IPC gọi `verify_peer_same_user_and_session` trên mọi kết nối (§20.7), công bố snapshot, lưu bền các probe được báo cáo, ghi các sự kiện `Diagnostic` opt-in vào `DiagnosticsLog`, và hiện thực phía controller của D24 (nuốt các phím tắt `VirtualKey`, broadcast `ModeChanged` và `ResetAll`).
   - Hook thread với bộ phân loại D24 và các nhiệm vụ adapter của nó.
   - Kênh truyền dự phòng (fallback) với session map, các backspace theo D20 và kênh truyền qua clipboard.
   - Vòng đời cấu hình: thư mục dữ liệu gốc theo D9, các store, chẩn đoán, và kiểm tra hợp lệ các tên bảng mã trong `[conversion]` của `config.toml`.
   - Server legacy `evkey-compat`, panic hook im lặng, và một `build.rs` cho các tài nguyên của nó.
3. **Các khối xây dựng còn thiếu trong `raumakey-windows`.**
   - Window class, cửa sổ message-only, `PostMessageW`, hộp thoại và control.
   - Tải icon từ chính tài nguyên của module, và module handle cùng đường dẫn của DLL (`DllRegisterServer` cần chúng).
   - `GetMessageExtraInfo`, các helper cho sink TSF và edit session, phát hiện input scope, và theo dõi thay đổi cửa sổ foreground.
   - Một adapter Windows chỉ đọc cho port `LegacyRegistry` của migration, và các thư mục EVKey mặc định ứng viên cho `raumakey_migrate::discover` khi `migrate` không nhận `--from`.
   - Xác minh profile bằng cách liệt kê profile đã cài đặt qua TSF (kế hoạch §10.4).
   - Điều phối cài đặt x64 + x86 với hoàn tác (rollback) xuyên kiến trúc và bản ghi `HKLM\Software\Raumakey\Install`.

**B. Hạ tầng kiểm chứng**

4. `raumakey-tests`: các vector máy đọc được trong `tests/fixtures/compat/*.toml`; các bộ test hợp đồng, trong đó có một bộ chạy engine với `MacroTable` thật; `fuzz_targets` và `tests/fuzz_smoke.rs`; và workspace cargo-fuzz `fuzz/`.
5. `raumakey-test-runner`: `vectors --json` và `live --app notepad`, ghi lại các trường mà tiêu chí đạt của ma trận yêu cầu.
6. `tests/windows/Invoke-RaumakeyAcceptance.ps1` và `MANUAL_ACCEPTANCE.md`. Chạy 8 test thủ công và giữ kết quả của chúng làm bằng chứng.
7. `xtask`:
   - Thêm `raumakey-windows/evkey-compat`, cùng các feature của controller và TIP, vào tập feature của CI.
   - Làm cho việc thiếu bộ fuzz smoke khiến `release-gate` không đạt, thay vì chỉ thêm một ghi chú.
   - Thêm một ô bằng chứng cho review toàn branch (§16), và truyền `--locked`.
   - Đưa các thông báo giấy phép được sinh ra và văn bản giấy phép vào package.
   - Mở rộng `inspect` để kiểm tra Common Controls v6, PerMonitorV2 và mục supportedOS.
   - Chạy các test i686 dưới WOW64, và biên dịch mã nguồn WiX trong CI.
   - Ngăn `package` đóng gói các binary không đạt `inspect`.

**C. Quy trình, nguồn gốc, tài liệu**

8. Commit phần triển khai. Cho đến lúc đó sẽ không có lịch sử, bằng chứng phát hành không thể xác định cây mã nguồn, và các seed hồi quy của proptest có thể bị mất. Từ nay trở đi, ghi lại lần chạy test fail của mỗi task và mỗi lần review độc lập (§16); mã hiện có không có thứ nào trong hai thứ này.
9. `THIRD_PARTY.md`: MSVC CRT được link vào các binary phát hành (D23) nhưng lại được liệt kê là "not shipped". Ghi lại căn cứ cho việc này, đưa kèm thông báo giấy phép cho các crate phụ thuộc gián tiếp, và dẫn nguồn của bảng VNI Windows.
10. Duy trì kế hoạch:
    - Viết nội dung cho D17 và D21, vốn chỉ được tiêu đề §10.4 trích dẫn, và thêm một mục lục quyết định.
    - Ghi lại các quyết định ở mức mã được liệt kê trong §20.9.
    - Cập nhật bảng phụ thuộc của kế hoạch §1.1 và §1.3 cho khớp với các manifest.
    - Thêm `RuntimeSnapshot.diagnostics_enabled` và giới hạn 4 KiB cho message điều khiển vào kế hoạch §7.
    - Ghi lại rằng `RegisterProfile` thay thế `InstallLayoutOrTip`, một điểm lệch khỏi đặc tả clean-room §6.1, và liệu các chế độ `-ineime` / `-r -ineime` có được một đường tương đương hay bị bỏ, như §6.1 yêu cầu đối với chúng.

### 20.7 Rủi ro đã biết và nợ kỹ thuật

Mỗi mục đã được xác nhận với mã nguồn. Các mục được đánh dấu *(Windows)* vẫn cần xác nhận trên một hệ thống thật.

- **Độ bền của commit.** `AtomicStore` flush tệp tạm nhưng không flush thao tác hoán đổi. `ReplaceFileW` chạy không có write-through, và không có lần flush thư mục nào theo sau, nên một revision đã commit có thể không còn sau khi mất điện.
- **Host chạy sandbox (Windows, kế hoạch D27).** Pipe điều khiển cấp quyền truy cập client cho `ALL APPLICATION PACKAGES` và không mang mandatory label, nên các TIP trong host AppContainer kết nối được; các test trong tiến trình xác nhận điều này bằng một token AppContainer được giả danh (impersonate), nhưng chưa thử với host AppContainer thật nào. Bất kỳ tiến trình AppContainer nào của người dùng cũng có thể đọc cấu hình và macro và gửi lệnh client. Các TIP trong host mức toàn vẹn thấp (low integrity) không phải AppContainer thì không kết nối được và giữ mặc định v1; theo D24, khi đó cả hook của controller lẫn TIP như vậy đều phân loại phím bật/tắt chỉ gồm phím bổ trợ. Client từ chối pipe có nhãn dưới mức toàn vẹn trung bình (medium), nên một tiến trình mức toàn vẹn thấp chiếm tên pipe trước không thể giả làm controller. Các AppContainer ít đặc quyền hơn (less-privileged) và token bị hạn chế (restricted token) không được chấp nhận.
- **Xác minh đăng ký TSF (Windows).** Bước xác minh kiểm tra các khoá nội bộ của TSF dưới `HKLM\SOFTWARE\Microsoft\CTF\TIP` thay vì liệt kê profile qua TSF, và chưa từng chạy với một lần đăng ký thật. Nếu khoá đó được dùng chung giữa view registry 32-bit và 64-bit, việc huỷ đăng ký một TIP sẽ xoá profile mà TIP kia dựa vào. Hoàn tác (rollback) để lại một profile vốn đã tồn tại trước khi đăng ký nhưng mang các tham số mới.
- **Ô bảo mật trong host dùng đường dự phòng.** Ngoài TSF, tín hiệu mật khẩu duy nhất là style `ES_PASSWORD` trên các control Edit và RichEdit. Chưa có bước kiểm tra UI Automation nào, và không có message IPC nào chuyển trạng thái bảo mật mà TIP phát hiện được tới controller.
- **Quyền riêng tư dựa trên quy ước.** Một số kiểu chứa văn bản đã gõ, văn bản tài liệu, văn bản macro hoặc mã phím derive `Debug`, trong đó có `InputEvent`, `EditTransaction`, `MacroEntry`, `EngineState`, `ClipboardText`, `MacroDocument` và `KeyboardEvent`. Hiện chưa có gì log chúng, nhưng chỉ một `{:?}` trong một log hoặc thông điệp panic là sẽ vi phạm §14.
- **Peer của pipe chưa được xác minh.** `PipeServer::accept` trả về một kết nối chưa được xác minh, nên controller phải tự gọi `verify_peer_same_user_and_session`.
- **Giới hạn bản ghi probe.** `app_rules.toml` không có giới hạn, snapshot IPC cho phép 4,096 bản ghi, còn cache của policy giữ 1,024 bản ghi và loại bỏ phần còn lại một cách im lặng.
- **Test double cho macro.** Các double macro của engine khác `MacroTable` ở việc nhận diện chữ hoa/thường, `CasePolicy::Exact`, `BoundaryPolicy` và việc chuẩn hoá lại NFC, nên engine và bảng có thể không thống nhất mà không có test nào fail.
- **Phương án dự phòng im lặng.** Một nội dung thay thế macro dài hơn 8,192 đơn vị UTF-16 sẽ commit từ mà không mở rộng và không có tín hiệu nào. Chưa có test nào bao phủ trường hợp này.
- **Tắt hook.** `HookThread::stop` tách (detach) thread sau 2 s, điều này có thể khiến các hook vẫn còn được cài.
- **Package hiện tại.** `install.ps1`, `uninstall.ps1` được sinh ra và các custom action của MSI gọi những lệnh con của controller chưa tồn tại. Chúng sẽ thoát với mã 0 mà không đăng ký gì cả.
- **Rác trong thư mục dữ liệu gốc.** Các tệp `<file>.lock` không bao giờ bị xoá, và `set_aside` để lại các bản sao `<file>.bak-<ms>` không bao giờ được dọn.
- **Test đạt một cách hình thức.** Ba test converter chỉ dành cho Windows tự bỏ qua hoàn toàn khi thư mục tạm không nằm trên một ký tự ổ đĩa, administrative share không truy cập được, hoặc volume không tạo tên 8.3; test thứ tư bỏ qua trường hợp UNC của nó khi share không truy cập được. Tất cả chúng vẫn báo thành công.
- **Dọn dẹp.** `serde` trong `raumakey-migrate`, `toml` trong `xtask` và `windows-core` trong `raumakey-windows` là các phụ thuộc không dùng đến. `raumakey-windows` phụ thuộc vào `raumakey-ipc` vô điều kiện. Chỉ số icon của profile TIP được định nghĩa ở cả `raumakey-resources` lẫn `raumakey-windows`, và không có gì kiểm tra rằng hai giá trị đó khớp nhau.

### 20.8 Câu hỏi thiết kế còn mở

Các câu hỏi này cần một quyết định sản phẩm. Ghi lại mỗi câu trả lời thành một quyết định của kế hoạch (Dn), rồi điều chỉnh thiết kế này, kế hoạch, mã nguồn và test cho khớp.

Cả chín mục đã được quyết định ngày 2026-10-17; kế hoạch §15 ghi các câu trả lời thành D17, D21 và D25–D37. Cho đến khi mã nguồn và thiết kế này được điều chỉnh cho khớp (kế hoạch §14.5 bước 3), phần văn bản dưới đây mô tả các câu hỏi đúng như khi chúng được đặt ra.

1. **Chế độ ô bảo mật `Restricted`.** §14 và ma trận #29 nói rằng ô bảo mật bỏ qua biến đổi, macro và clipboard. Kế hoạch §6 và `raumakey-policy` cho phép `Restricted` giữ biến đổi, và có một test khẳng định điều này. Engine bỏ qua ô bảo mật ở mọi chế độ, nên hiện tại thiết lập này không có tác dụng.
2. **Hook bàn phím toàn cục của controller (D24).** Hook nhận mọi phím trên desktop, kể cả các phím gõ vào host TSF, trong khi §5.2 nói rằng phím gõ thô ở lại trong TIP trên đường TSF. Hoặc nêu rõ các quy tắc quyền riêng tư cho handler của hook (chỉ phân loại, không lưu gì, chỉ chạy engine cho host dùng đường dự phòng), hoặc sửa lại D24. Đồng thời quyết định điều gì xảy ra khi một TIP mất kết nối và cả hai phía có thể cùng phân loại một phím.
3. **Nhập thiết lập EVKey.** `FIELD_MAP` được phát hành ở dạng rỗng (D18). Do đó một lần nhập mang vào macro và các named set dạng danh sách tệp thực thi, còn mọi thiết lập khác chỉ được giữ ở dạng bằng chứng thô. Tiêu chí thành công 6 ("nhập các thiết lập EVKey được hỗ trợ") được đáp ứng chỉ vì không có thiết lập nào được khai báo là được hỗ trợ. Xác nhận điều này, hoặc lên kế hoạch cho các ánh xạ.
4. **Bằng chứng migration.** §9 bước 3 giữ các byte nguồn thô làm bằng chứng cho hoàn tác (rollback). Bộ nhập chỉ giữ dấu vân tay (fingerprint) của tệp macro, không bao giờ giữ văn bản macro (một lựa chọn vì quyền riêng tư); nó giữ `setting.tree` dưới dạng base64 trong hạn mức 8 MiB, `setting.ini` dưới dạng các giá trị đã phân tích, và các giá trị registry dưới dạng giá trị có kiểu. §9 cũng liệt kê các nguồn ngoại lệ, vốn không tồn tại: ngoại lệ ứng dụng chỉ đến qua heuristic danh sách tệp thực thi.
5. **Cài đặt profile TSF.** Kế hoạch §10.4 dùng `ITfInputProcessorProfileMgr::RegisterProfile`. Đặc tả clean-room §6.1 cài đặt profile bằng `InstallLayoutOrTip` (và PHẢI giữ định dạng chuỗi profile của nó), xác minh trạng thái đó, và yêu cầu các chế độ `-ineime` / `-r -ineime` phải có một đường tương đương hoặc một sự lược bỏ có chủ ý và được ghi lại. Hãy quyết định, rồi viết D17 và D21. Cho đến khi chúng nói khác đi, kế hoạch §10.4 yêu cầu bước xác minh phải liệt kê profile, điều mà mã nguồn chưa làm (§20.7).
6. **Truy cập pipe từ host AppContainer và host mức toàn vẹn thấp** (§20.7). Thêm ACE và nhãn cần thiết, hoặc chấp nhận rằng các TIP trong những host như vậy chạy với mặc định v1.
7. **Thứ tự ưu tiên của quy tắc ứng dụng.** Trong cùng một tầng, named set hoặc quy tắc được khai báo đầu tiên quyết định từng trường. Kế hoạch §6 liệt kê các named set "theo thứ tự khai báo" bên trong một chuỗi mà ở đó các bước sau ghi đè các bước trước.
8. **Nội dung package.** Ngoài các binary mà §13 nêu tên, package x64 còn đóng gói `tools/raumakey-test-runner.exe`, các script cài đặt và gỡ cài đặt, và một README.
9. **Phương án dự phòng Circumflex.** Khi tắt kiểm tra chính tả, kế hoạch §3.6 chọn "ứng viên cuối cùng" trong một danh sách được sắp từ cuối lên đầu, trong khi mã nguồn chọn nguyên âm ở bên phải nhất. Cách diễn đạt này mơ hồ, và chưa có test nào bao phủ trường hợp đó.

### 20.9 Các quyết định

Kế hoạch ghi lại các quyết định D1, D2, D4–D7, D9, D13, D17–D21, D23 và D24. Các số còn lại không được dùng.

| Quyết định | Tóm tắt | Thiết kế § | Triển khai |
|---|---|---|---|
| D1 | Toolchain và host: Rust stable được ghim; `rust-lld` + `xwin` trên Windows, `cargo xwin` trên Linux; tài nguyên từ `build.rs`; test i686 dưới WOW64 | §13, §15 | Xong, trừ các tệp `build.rs` và việc chạy test i686 dưới WOW64 |
| D2 | `publish = false`; GPL-3.0-or-later | §1, §4 | Xong |
| D4 | Bộ mặc định có phiên bản của engine, v1 | §6 | Xong |
| D5 | Phím tắt: bật/tắt `Ctrl+Shift`; reset cứng `Ctrl+Shift+Alt+F12` và `Ctrl+Shift+Alt+Space` | §11 | Xong phần thư viện |
| D6 | Tag cho input được chèn `0x524B5931` ("RKY1") | §5.3 | Xong phần thư viện |
| D7 | Shift+Space commit từ mà không thay đổi | §7 | Xong |
| D9 | Thư mục dữ liệu gốc portable `<exe_dir>/data/`, chỉ khi có tệp đánh dấu `raumakey.portable` | §8 | Xong phần thư viện, chưa được nối vào |
| D13 | Các named set mặc định: chromium-family, terminals, games | §5.3 | Xong |
| D17, D21 | Các mục đăng ký TSF, view registry, category, xác minh | §12 | Có mã trong `raumakey-windows`; thiếu văn bản quyết định |
| D18 | Thiết lập chỉ được nhập qua `FIELD_MAP`, vốn được phát hành ở dạng rỗng | §2, §9 | Xong |
| D19 | Các bảng mã của converter | §7 | Xong |
| D20 | Đường dự phòng (fallback) gửi một backspace cho mỗi scalar bị xoá | §5.3 | Chưa bắt đầu (controller) |
| D23 | CRT tĩnh trong các artifact phát hành | §13 | Xong trong `xtask`; `inspect --release` không tìm thấy import VC runtime nào |
| D24 | Một chủ sở hữu phím tắt duy nhất | §11 | Bộ phân loại đã xong; giao thức sở hữu chưa được triển khai |

Mã nguồn cũng đưa ra những quyết định mà kế hoạch chưa ghi lại:

- **Engine:** `transformed` được suy ra từ dấu thanh và các dấu phụ hiện tại (kế hoạch để ngỏ điểm này); các chữ cái được tính là tiền tố âm tiết nếu các dấu thêm vào sau đó vẫn có thể biến chúng thành tiền tố âm tiết; năm vần bổ sung sau `qu`; không có `g` đơn trước e/ê/y; một phím dấu câu ranh giới được escape kiểu VIQR sẽ commit từ; phương án dự phòng Circumflex (§20.8, mục 9).
- **Model và macro:** một trigger được gõ đúng y như khi lưu thì không bao giờ được điều chỉnh chữ hoa/thường; việc nhập legacy cắt khoảng trắng ở hai đầu, giữ các nội dung thay thế chỉ gồm khoảng trắng, từ chối đầu vào lớn hơn 16 MiB hoặc có hơn 100,000 dòng macro (không tính dòng trống, dòng chú thích và dòng tiêu đề), và xuất CRLF không có BOM; một phím tắt chỉ gồm phím bổ trợ cần ít nhất hai phím bổ trợ.
- **Policy và IPC:** thứ tự ưu tiên "khai báo trước thì thắng" trong một tầng; giới hạn của cache probe và thứ tự loại bỏ của nó; giới hạn 4 KiB cho message điều khiển; `RuntimeSnapshot.diagnostics_enabled`; các giới hạn legacy (tối đa 0x800 đơn vị bị xoá, số thứ tự bỏ qua 0, epoch tương lai bị từ chối, bridge bị huỷ sau ba lần trao đổi thất bại); kích hoạt legacy chấp nhận một mutex đã tồn tại, như đặc tả clean-room §6 yêu cầu.
- **Cấu hình và migration:** `Conflict` cũng được trả về khi tệp đã thay đổi dưới cùng một revision; một revision đã cạn sẽ bắt đầu lại từ 0; thời gian chờ lock bị giới hạn ở 10 s; văn bản macro không bao giờ đi vào `migration.json`.
- **Converter, tài nguyên và tầng Windows:** port clipboard chỉ ghi khi số thứ tự (sequence number) không thay đổi; manifest còn đặt active code page UTF-8; việc đăng ký TSF chạy bên trong `DllRegisterServer` dưới `regsvr32` có cùng bitness; guard ghi registry áp dụng cùng các quy tắc cho HKCU và HKLM.

### 20.10 Các bước tiếp theo được khuyến nghị

1. Sửa tập feature của `xtask ci` và làm cho `release-gate` không đạt khi thiếu bộ fuzz smoke hoặc chưa có review toàn branch nào được ghi lại. Sau đó commit cây hiện tại, để các công việc sau có lịch sử và bằng chứng truy vết được.
2. Quyết định các mục 2, 5 và 6 của §20.8 và ghi chúng thành quyết định của kế hoạch: chúng định nghĩa hook thread, lệnh cài đặt cùng bước xác minh của nó, và ACL của pipe mà TIP kết nối qua. Quyết định các mục 1, 3 và 4 trước khi viết đường xử lý ô bảo mật của TIP và lệnh `migrate`.
3. Thêm các tệp `build.rs` và các khối xây dựng còn thiếu của `raumakey-windows`. Sau đó triển khai TIP: bốn export COM, class factory và đăng ký của nó, và đường xử lý phím phía sau một port edit-session, với fake cho các hàng ma trận #19, #20, #40 và #41.
4. Triển khai controller: trước tiên là subsystem GUI và CLI với `install`/`uninstall --dry-run`, những thứ cùng với bước 3 sẽ giúp `xtask inspect` và bước chạy thử (dry run) của installer đạt; sau đó là hub IPC, tray, hook thread và bảng cài đặt.
5. Xây dựng `raumakey-tests` và runner nghiệm thu, sau đó là bộ test PowerShell và danh sách kiểm tra thủ công, rồi chạy 8 test thủ công.
6. Giải quyết các câu hỏi còn lại của §20.8 (mục 7–9) và cập nhật lại mục này.

### 20.11 Đợt tiếp nối sau review (2026-10-17)

Một đợt review có trọng tâm đối chiếu với mã nguồn hiện tại đã xác nhận rằng controller, TIP và
test runner dùng chung vẫn là stub. Cây ban đầu sạch tại `24ba7a6`, nên nhận định trước đó rằng
các tệp triển khai chưa được theo dõi (untracked) không còn đúng nữa. Đây không phải là một đợt
review độc lập toàn branch.

Đợt tăng trưởng (increment) tiếp theo sửa các lỗ hổng kiểm chứng trong §20.6 B7: CI bao phủ mọi
feature legacy của workspace; các lệnh Cargo có phân giải phụ thuộc dùng `--locked`; thiếu fuzz
smoke test thì CI và cổng phát hành không đạt; và cần có một xác nhận (attestation) review toàn
branch gắn với revision trước khi đóng gói trong release-gate. Bằng chứng cũng từ chối các bước
gate bị thiếu, bị trùng hoặc bị đổi thứ tự, thay vì coi bất kỳ danh sách không rỗng nào gồm các
bước thành công là một lần kiểm chứng đầy đủ.

Bản thân bộ fuzz vẫn chưa tồn tại, nên CI mặc định và cổng phát hành được dự kiến sẽ không đạt
cho đến khi bộ này được triển khai. `ci --no-fuzz` vẫn có sẵn cho các lần kiểm tra một phần có
chủ đích. Hợp đồng của bản ghi review được mô tả trong kế hoạch §12. Không có bản ghi duyệt nào
bị làm giả. Nội dung package, các bước kiểm tra đóng gói độc lập, nghiệm thu Windows và việc
triển khai sản phẩm vẫn là việc còn lại. Sự bất đồng giữa policy và engine về ô bảo mật
`Restricted` đã được xác nhận lại trong mã nguồn và vẫn chưa được giải quyết (§20.8 mục 1).

Xem `docs/superpowers/plans/2026-10-17-review-followup.md` để biết phạm vi và bằng chứng TDD.
Đợt tiếp nối đã đạt kiểm tra định dạng, Clippy ở chế độ coi cảnh báo là lỗi, và test workspace
trên Windows: 1,241 test với feature mặc định và 1,315 test với mọi feature tương thích; 8 test
thủ công hiện có vẫn bị bỏ qua trong mỗi lần chạy. `xtask` có 102 test đạt. Các đường thất bại
của CI mặc định và release-gate đã được thực thi và ghi nhận đúng `not verified`.

## 21. Bản portable một file (thay đổi thiết kế ngày 2026-10-17)

Chủ dự án quyết định đổi cách phát hành. Với TIP TSF, người dùng phải đăng ký COM vào `HKLM`, chạy
script cài đặt bằng quyền quản trị, rồi thêm bàn phím trong Settings. Cách đó quá phức tạp so với
các bộ gõ tương đương như UniKey và EVKey, vốn chỉ cần một tệp exe. Mục này mô tả thiết kế mới.
Kế hoạch §15 ghi các quyết định D38–D42, còn kế hoạch §16 ghi các task và trạng thái của chúng.
Khi mục này mâu thuẫn với các mục trước, mục này thắng.

### 21.1 Phát hành (D38)

- Raumakey phát hành dưới dạng **một tệp `raumakey.exe`** (x86_64, CRT tĩnh, manifest
  `asInvoker`). Người dùng chép tệp vào thư mục bất kỳ và chạy trực tiếp. Không có installer,
  không cần quyền quản trị, không đăng ký COM/TSF và không ghi registry.
- Package x86_64 chỉ gồm `raumakey.exe`, `README.txt`, `LICENSE`, `THIRD_PARTY.md`, `SHA256SUMS`
  và `manifest.json`. TIP, script cài đặt, test runner và MSI không còn nằm trong package. Package
  ARM64 vẫn chỉ được kiểm tra biên dịch (compile-gated).
- Mã của TIP (`raumakey-tsf`), các lệnh `install`/`uninstall` và các khối đăng ký TSF vẫn ở lại kho
  mã, vẫn được build và kiểm tra (`cross`, `inspect`), nhưng không được phát hành. Chúng có thể
  quay lại sau này dưới dạng một chế độ nâng cao.
- Bước `installer-dry-run` của cổng phát hành được thay bằng `portable-check`: package chứa đúng
  các tệp trên, và `raumakey.exe --version` chạy được.

### 21.2 Đường gõ: hook bàn phím (D39)

```text
phím vật lý
  -> WH_KEYBOARD_LL trên luồng hook của raumakey.exe
  -> KeyStroke: phím ảo, scan code, phím bổ trợ, văn bản theo layout của cửa sổ foreground
  -> ServiceCore (crate raumakey-session, dùng chung với TIP) -> VietnameseEngine
  -> EditTransaction
  -> SendInput: Backspace × delete_before, rồi ký tự Unicode (KEYEVENTF_UNICODE),
     gắn RAUMAKEY_INJECTED_TAG
  -> phím gốc bị chặn (hook trả về khác 0) khi transaction tiêu thụ nó
```

- Engine chạy trong `raumakey.exe`. Hook không đọc được văn bản của ứng dụng, nên mọi sửa đổi là
  sửa "mù" dựa trên phần văn bản mà engine tự sở hữu, như UniKey và EVKey vẫn làm.
- Lõi phiên của TIP được tách thành crate `raumakey-session`, không phụ thuộc Windows, để TIP và
  hook dùng chung một bộ quy tắc: phân loại phím tắt (D24), bỏ qua phím do Raumakey gửi (ma trận
  #39), kết thúc từ, macro, chính sách ô bảo mật (D25).
- Mỗi tài liệu có trạng thái engine riêng, xác định bằng cặp (cửa sổ foreground, cửa sổ có focus).
  Từ kết thúc khi click chuột (`WH_MOUSE_LL`), khi đổi cửa sổ foreground hoặc focus, khi gõ phím
  điều hướng, và khi gõ tổ hợp Ctrl/Alt/Win.
- Phím do Raumakey gửi mang `RAUMAKEY_INJECTED_TAG` nên hook cho chúng đi qua.
- Phím tắt bật/tắt tiếng Việt (mặc định Ctrl+Shift, D5) do lõi phiên phân loại, như ở TIP (D24).
- Với cửa sổ của tiến trình chạy quyền cao hơn, UIPI chặn `SendInput`, nên hook cho phím đi qua mà
  không biến đổi. Muốn gõ tiếng Việt ở đó, người dùng chạy `raumakey.exe` bằng quyền quản trị.
- Về ô mật khẩu, hook chỉ nhận ra Edit control có `ES_PASSWORD` và áp dụng chế độ ô bảo mật của
  chính sách cho nó (D25). Hook không nhận ra được các ô mật khẩu khác, ví dụ ô mật khẩu trên trình
  duyệt.
- Callback của hook phải trả lời nhanh, dưới `LowLevelHooksTimeout`. Engine và `SendInput` chạy
  ngay trong callback, không có I/O và không chờ khoá của luồng khác. Thay đổi cấu hình được chuyển
  tới luồng hook và áp dụng ở phím kế tiếp.
- Trạng thái phím được đọc từ các sự kiện hook. Chỉ hai cách đọc trực tiếp được tin cậy:
  `GetAsyncKeyState` cho biết một phím có đang được giữ hay không, và `GetKeyState(VK_CAPITAL)` cho
  biết trạng thái Caps Lock. Raumakey dùng chúng để:
  - bỏ các phím mà hook không thấy lúc được thả (sau Win+L hoặc Ctrl+Alt+Del), đồng thời đặt lại
    bộ phân loại phím tắt;
  - đồng bộ phím bổ trợ trước mỗi phím;
  - đọc lại Caps Lock khi đổi tài liệu, hoặc sau 1 giây không có sự kiện nào.

  Khi Ctrl, Alt hoặc Win đang thực sự được giữ (ngoài trường hợp AltGr), Raumakey không sửa chữ,
  để một Backspace không bao giờ thành Ctrl+Backspace.
- Nhấn Alt một mình và F10 đưa cửa sổ vào chế độ menu, nên cả hai đều kết thúc từ.
- Khi cửa sổ foreground dùng bộ gõ tiếng Trung, Nhật hoặc Hàn, mọi phím đi thẳng và từ kết thúc.
- Windows có thể âm thầm gỡ một hook mức thấp, ví dụ khi callback quá chậm. Vì vậy cứ 2 giây
  controller lại so nhịp sự kiện của hook với `GetLastInputInfo`. Nếu hệ thống vẫn nhận input mà
  hook đã im hơn 1 giây, controller cài lại hook với cấu hình hiện tại.
- Giới hạn:
  - Việc chuyển sang một cửa sổ chạy quyền cao hơn rồi quay lại mà hook không nhận được phím nào
    (ví dụ chỉ dùng chuột) không kết thúc từ đang gõ.
  - Khi gõ bằng AltGr, các phím Backspace được gửi lúc phím Ctrl giả của AltGr vẫn đang được giữ.
- D39 thay thế D26: hook giờ là transport gõ, không chỉ đọc phím tắt.
- Rủi ro đã biết:
  - Ứng dụng có tự hoàn thành (thanh địa chỉ trình duyệt, Excel) có thể làm lệch các phím
    Backspace.
  - Phần mềm diệt virus hoặc chống gian lận có thể cảnh báo một hook bàn phím toàn cục.
  - Hai bộ gõ cùng chạy (UniKey và Raumakey) sẽ biến đổi cùng một phím.

### 21.3 Cấu hình (D40)

- Thư mục dữ liệu là **thư mục chứa `raumakey.exe`**. Các tệp:
  - `raumakey.toml`: cấu hình;
  - `raumakey-macros.toml`: macro, tuỳ chọn;
  - `raumakey-app-rules.toml` và `raumakey-migration.json`;
  - thư mục `raumakey-logs\`.

  Tên có tiền tố vì các tệp nằm chung với tệp khác của người dùng. Mỗi tệp store có thêm một tệp
  khoá `<tệp>.lock`.
- Lần chạy đầu, khi chưa có `raumakey.toml`, Raumakey ghi cấu hình mặc định vào đó. Từ lần chạy thứ
  hai, Raumakey đọc lại tệp này. Một tệp không đọc được không làm controller dừng: controller chạy
  với cấu hình mặc định, báo lỗi, và không âm thầm ghi đè tệp hỏng.
- Raumakey chọn thư mục theo thứ tự sau:
  1. Nếu đã có `raumakey.toml` cạnh exe, dùng thư mục của exe, kể cả khi thư mục đó không ghi được.
     Khi đó việc lưu thất bại và Raumakey báo lỗi.
  2. Nếu chưa có, dùng thư mục của exe khi nó ghi được và không nằm trong một thư mục được bảo vệ:
     Program Files, Program Files (x86), thư mục Windows, hoặc thư mục tạm của người dùng
     (`%TEMP%`). Explorer chạy exe nằm trong tệp ZIP từ `%TEMP%` rồi xoá đi.
  3. Ngoài ra, dùng `%LOCALAPPDATA%\Raumakey`.

  Nhờ vậy, một bản trong Program Files dùng cùng một tệp `%LOCALAPPDATA%` dù chạy bằng quyền thường
  hay quyền quản trị. Rủi ro được chấp nhận: khi chạy bằng quyền quản trị, Raumakey đọc một tệp mà
  chương trình chạy quyền thường ghi được. Chế độ portable bằng tệp đánh dấu `raumakey.portable` bị
  bỏ. D40 thay thế D9 và đoạn đầu của §8.
- Khi Raumakey ghi lại tệp, các dòng comment và cách trình bày trong tệp không được giữ, và một giá
  trị không hợp lệ được thay bằng giá trị mặc định.
- Thay đổi từ menu được ghi ngay. Tệp sửa bằng tay được nạp lại trong vòng vài giây, vì
  controller kiểm tra tệp mỗi 2 giây. Một tệp đọc ra hỏng hoặc thiếu `version` được đọc lại một
  lần sau 300 ms, vì trình soạn thảo có thể đang ghi dở.
- Một thay đổi chưa lưu được (ví dụ khi phần mềm diệt virus đang khoá tệp) vẫn có hiệu lực. Raumakey
  thử lưu lại ở mỗi lần kiểm tra và một lần nữa khi thoát.
- Hộp thoại thông báo chạy trên luồng riêng, nên trong lúc nó mở, khay vẫn nhận được lệnh thoát, lệnh
  đăng xuất và `TaskbarCreated`. Nếu thêm biểu tượng thất bại, Raumakey thử lại ở mỗi nhịp 2 giây.

### 21.4 Khởi động cùng Windows (D41)

- Mục menu "Khởi động cùng Windows": khi bật, Raumakey tạo shortcut `Raumakey.lnk` trỏ tới
  `raumakey.exe` đang chạy, đặt trong thư mục Startup của người dùng; khi tắt, Raumakey xoá
  shortcut đó. Trạng thái được lưu ở `[system] startup` (`shortcut` hoặc `none`).
- Lúc khởi động, Raumakey chỉ sửa lại shortcut khi shortcut bị mất hoặc trỏ tới một exe không còn
  tồn tại (exe đã bị chuyển chỗ). Shortcut đang trỏ tới một bản Raumakey khác vẫn còn tồn tại thì
  được giữ nguyên.
- Exe nằm trong `%TEMP%` (chạy thẳng từ tệp ZIP) không được tạo shortcut. Raumakey nhắc người dùng
  giải nén ra một thư mục cố định trước.
- Sửa tay `startup` trong tệp cấu hình cũng tạo hoặc xoá shortcut theo các quy tắc trên. Khi xoá,
  Raumakey chỉ xoá shortcut trỏ tới chính exe này.
- Lệnh `raumakey startup <mode>` cũng ghi chế độ vào `[system] startup`, nên menu và dòng lệnh luôn
  thống nhất. Raumakey không sửa shortcut khi chế độ là `elevated-task`.
- Việc này không cần quyền quản trị. Chế độ `elevated-task` chỉ còn dùng được qua dòng lệnh
  (`raumakey startup elevated-task`).

### 21.5 Giao diện (D42)

- Click vào biểu tượng V/E để đổi chế độ. Menu khay có các mục:
  - "Gõ tiếng Việt (Ctrl+Shift)";
  - "Telex", "VNI", "VIQR";
  - "Kiểm tra chính tả";
  - "Đặt dấu kiểu mới (oà, uý)";
  - "Gõ tắt (macro)";
  - "Khởi động cùng Windows";
  - "Mở file cấu hình", mở `raumakey.toml` bằng Notepad;
  - "Giới thiệu", hiện phiên bản và đường dẫn của tệp cấu hình;
  - "Thoát".
- Bảng cài đặt và các tab của §11 được hoãn lại. (Từ 2026-10-18, §22 thêm bảng cấu hình và mục
  "Bảng cấu hình…" ở đầu menu.)

### 21.6 Những phần bị thay thế hoặc hoãn

| Phần | Trước | Từ 2026-10-17 |
|---|---|---|
| §5.2 Đường TSF | Đường gõ mặc định | Không phát hành; mã giữ trong kho |
| §5.3 Đường dự phòng | Chỉ cho host đặc biệt | Đường gõ duy nhất (D39) |
| §8 Thư mục dữ liệu | `%LOCALAPPDATA%\Raumakey`, portable bằng tệp đánh dấu | Thư mục của exe (D40) |
| §10 IPC giữa TIP và controller | Kênh điều khiển có kiểu | Không cần khi không có TIP; hoãn lại |
| §11 Bảng cài đặt | Bảng thu gọn và các tab | Menu khay (D42); từ 2026-10-18 có bảng cấu hình một trang và danh sách gõ tắt (D43–D45, §22) |
| §12 Đăng ký COM/TSF | Bước cài đặt | Không phát hành |
| §13 Installer, MSI, script | Có | Không có (D38); khởi động cùng Windows qua menu (D41) |
| §2 tiêu chí 5 | Đăng ký danh tính COM/TSF mới | Hoãn lại cùng TIP |
| Kế hoạch D9, D26, D33 | Thư mục dữ liệu, hook chỉ đọc phím tắt, nội dung package | Thay bởi D40, D39, D38 |

## 22. Bảng cấu hình và danh sách gõ tắt (thay đổi thiết kế ngày 2026-10-18)

Chủ dự án yêu cầu một hộp thoại để chọn cấu hình, thay vì chỉ sửa tệp. Ảnh chụp giao diện EVKey mà
chủ dự án gửi kèm chỉ được dùng để hiểu nhu cầu: người dùng quen với một bảng cấu hình mở từ khay
hệ thống. Theo quy định clean-room, bố cục, câu chữ và icon của các hộp thoại Raumakey do Raumakey
tự thiết kế; không chép bố cục, chuỗi, tài nguyên hay cấu trúc tab của EVKey. Kế hoạch §15 ghi các
quyết định D43–D45, kế hoạch §17 ghi task và trạng thái.

### 22.1 Bảng cấu hình (D43)

- **Cách mở:**
  - mục đầu tiên của menu khay, "Bảng cấu hình…";
  - chạy lại `raumakey.exe` khi Raumakey đang chạy (D45, §22.3);
  - lúc khởi động, khi `[system] show_panel_on_start = true`. Đây là giá trị mặc định của schema,
    nên lần chạy đầu luôn mở bảng.
- **Dạng hộp thoại:** một hộp thoại Win32 dựng từ template trong bộ nhớ, vì toolchain không có
  trình biên dịch `.rc`. Font Segoe UI 9 pt, co giãn theo DPI của màn hình (manifest đã khai báo
  PerMonitorV2 và Common Controls v6). Hộp thoại có nút riêng trên thanh tác vụ và mang icon của
  Raumakey. Tiêu đề: "Raumakey <phiên bản> – Bảng cấu hình".
- **Bố cục:** một trang, hai cột, không có tab:

```text
[x] Gõ tiếng Việt          Bật/tắt nhanh: Ctrl+Shift hoặc nhấp biểu tượng V/E
+- Cách gõ -------------------------+  +- Gõ tắt ----------------------------+
| Kiểu gõ: [Telex             v]    |  | [x] Bật gõ tắt                      |
| [ ] Đặt dấu kiểu mới (oà, uý)     |  | [ ] Gõ tắt cả khi đang gõ tiếng Anh |
| [x] Cho gõ dấu sau phụ âm cuối    |  | [x] Viết hoa theo cách gõ           |
|     (toans -> toán)               |  |     (Ko -> Không, KO -> KHÔNG)      |
| [x] Kiểm tra chính tả             |  | [Danh sách gõ tắt…]                 |
| [x] Giữ nguyên phím đã gõ khi từ  |  +-------------------------------------+
|     không phải tiếng Việt         |  +- Hệ thống --------------------------+
+-----------------------------------+  | [ ] Khởi động cùng Windows          |
                                       | [x] Mở bảng này khi Raumakey chạy   |
                                       | [Ứng dụng loại trừ…]  (từ D48)      |
                                       +-------------------------------------+
Mọi thay đổi có hiệu lực và được lưu ngay.
Tệp cấu hình: D:\Tools\Raumakey\raumakey.toml
(cạnh raumakey.exe)
[Mở file cấu hình]  [Mặc định]                                          [Đóng]
```

  Đường dẫn nằm trong một nhãn riêng, rút gọn ở giữa khi quá dài, nên tên `raumakey.toml` luôn
  hiện. Vị trí nằm trên dòng dưới; với `%LOCALAPPDATA%\Raumakey`, dòng này nói thêm lý do: thư mục
  của `raumakey.exe` không ghi được hoặc là thư mục được bảo vệ.

- **Mục và khoá tương ứng:**

| Mục trong bảng | Khoá |
|---|---|
| Gõ tiếng Việt | `[input] enabled` (cũng là Ctrl+Shift và click biểu tượng) |
| Kiểu gõ | `[input] method`: Telex, VNI, VIQR; mục "Kiểu gõ riêng" chỉ hiện khi tệp cấu hình có key map dùng được cho `user_keymap` |
| Đặt dấu kiểu mới (oà, uý) | `[input] modern_tone` |
| Cho gõ dấu sau phụ âm cuối | `[input] free_marking` |
| Kiểm tra chính tả | `[input] spell_check` |
| Giữ nguyên phím đã gõ khi từ không phải tiếng Việt | `[input] auto_restore_invalid` |
| Bật gõ tắt | `[macros] enabled` |
| Gõ tắt cả khi đang gõ tiếng Anh | `[macros] when_disabled` |
| Viết hoa theo cách gõ | `[macros] auto_case` |
| Khởi động cùng Windows | `[system] startup` và shortcut `Raumakey.lnk` (D41) |
| Mở bảng này khi Raumakey chạy | `[system] show_panel_on_start` |

- **Không có nút Lưu hay Huỷ.** Mỗi thay đổi có hiệu lực ngay như một lựa chọn trong menu, và worker
  ghi nó ngay vào `raumakey.toml`.
- **Tray là nguồn dữ liệu duy nhất.** Bảng gửi yêu cầu mang giá trị đích (ví dụ "kiểm tra chính tả
  = tắt"), không gửi lệnh "đảo". Sau mỗi yêu cầu của bảng, tray gửi lại toàn bộ trạng thái, và bảng
  hiện đúng trạng thái đó. Tray cũng gửi lại trạng thái mỗi khi cấu hình đổi theo đường khác:
  Ctrl+Shift, click biểu tượng, menu khay, tệp sửa tay, lệnh `raumakey startup`. Một yêu cầu bị từ
  chối, ví dụ "Khởi động cùng Windows" khi exe nằm trong `%TEMP%`, làm ô đó trở về giá trị cũ, kèm
  hộp thông báo như khi chọn từ menu.
- **"Mặc định"** hỏi lại người dùng, rồi đặt các mục của nhóm "Cách gõ" và "Gõ tắt" về giá trị mặc
  định của schema. Nút này không đổi chế độ Việt/Anh, việc khởi động cùng Windows, mục "Mở bảng này
  khi Raumakey chạy", danh sách gõ tắt, hay các khoá chỉ sửa được trong tệp.
- **Chỉ sửa được trong tệp:** `[input] normalization`, `context_limit`, `user_keymap` và
  `[[keymaps]]`, `[hotkeys]`, `[system] diagnostics`, `[conversion]`. Lý do không đưa phím tắt vào
  bảng: một phím tắt có phím thường như Alt+Z làm cửa sổ đang gõ mở menu, vì hook chặn Z nhưng Alt
  vẫn tới ứng dụng. Lý do không đưa dạng Unicode vào bảng: dạng tổ hợp (NFD) chưa được kiểm chứng
  với cách sửa mù bằng Backspace. Cả hai cần thêm việc ở hook. Bảng hiện phím bật/tắt đang dùng,
  chỉ để đọc.
- **Khi không có thư mục lưu** (`Location::Unavailable`): dòng tệp cấu hình nói các thay đổi không
  được lưu; nút "Mở file cấu hình" và "Danh sách gõ tắt…" bị tắt.
- **Luồng:**
  - Bảng chạy trên một luồng riêng, là một hộp thoại modal không có owner, như các hộp thông báo
    (§21.3). Luồng của tray không bao giờ chờ bảng.
  - Yêu cầu của bảng đi vào inbox của tray, kèm `WAKE`. Trạng thái mới đi tới bảng qua một biến dùng
    chung và một thông điệp riêng.
  - Mỗi lúc chỉ có một bảng: mở lại thì bảng đang mở được đưa lên trước.
  - Thoát Raumakey, đóng tiến trình hay đăng xuất đều đóng bảng mà không chờ.
- **Lần chạy đầu:** khi bảng mở lúc khởi động, hộp thoại chào không hiện nữa, vì bảng đã cho biết
  tệp cấu hình nằm ở đâu và cách bật/tắt nhanh. Các cảnh báo về tệp cấu hình vẫn hiện như trước.

### 22.2 Danh sách gõ tắt (D44)

- Nút "Danh sách gõ tắt…" trong bảng mở hộp thoại sửa `raumakey-macros.toml`. Hộp thoại có:
  - một danh sách ba cột: "Gõ tắt", "Thay bằng" và "Trạng thái". Cột trạng thái ghi "đang tắt"
    cho mục có `enabled = false`, và "không dùng" cho mục mà bảng macro của hook bỏ qua (mục sai
    quy tắc, trùng id với một mục trước, hoặc trùng từ gõ tắt với một mục có id nhỏ hơn);
  - hai ô nhập cho từ gõ tắt và nội dung thay thế;
  - các nút "Thêm / Sửa", "Xoá", "Lưu", "Đóng".
- Chọn một dòng thì hai ô nhập hiện nội dung của dòng đó. "Thêm / Sửa" sửa mục có cùng từ gõ tắt
  nếu đã có, hoặc thêm một mục mới. Raumakey kiểm tra theo đúng quy tắc của `raumakey-macro` (D29,
  D30):
  - từ gõ tắt có 1–32 ký tự sau chuẩn hoá NFC, không có khoảng trắng hay ký tự điều khiển;
  - nội dung thay thế không rỗng, không có ký tự NUL, và vừa một lần chèn của engine;
  - không quá `MAX_MACROS` (20.000) mục.

  Lỗi hiện bằng tiếng Việt trong hộp thoại.
- **Lưu:**
  - "Lưu" ghi cả danh sách trong một commit của store: ghi nguyên tử, có khoá và kiểm tra revision.
  - Id và các trường `enabled`, `boundary`, `case` của mỗi mục được giữ. Mục mới được bật, với các
    giá trị mặc định.
  - Các khoá lạ và các mục không giải mã được cũng được giữ (ma trận #45).
  - Khi chưa có `raumakey-macros.toml`, lần lưu đầu tạo tệp.
  - Nếu tệp bị sửa ở nơi khác trong lúc hộp thoại mở, Raumakey hỏi có ghi danh sách đang sửa đè lên
    không.
- **Tệp hỏng:** hộp thoại không mở, và một hộp thông báo đề nghị người dùng sửa tệp bằng tay.
  Raumakey không bao giờ ghi đè một tệp hỏng.
- **Luồng:**
  - Hộp thoại đọc và ghi tệp trên luồng của chính nó, bằng một store riêng.
  - Sau khi lưu, tray yêu cầu worker kiểm tra tệp ngay, nên hook nhận bảng macro mới sau một lượt
    kiểm tra, không phải chờ nhịp 2 giây.
  - `Settings` vẫn không bao giờ ghi `raumakey-macros.toml`.
- Văn bản của macro không bao giờ được ghi log và không đi qua answer của worker.
- Đóng hộp thoại khi còn thay đổi chưa lưu thì Raumakey hỏi lại. Thoát Raumakey đóng hộp thoại mà
  không lưu.

### 22.3 Chạy lại exe khi Raumakey đang chạy (D45)

- Khi controller của cùng người dùng trong cùng phiên đang chạy, lần chạy mới làm ba việc rồi thoát
  với mã 0:
  1. gọi `AllowSetForegroundWindow(ASFW_ANY)`, để bản đang chạy được đưa bảng lên trước;
  2. gửi thông điệp đăng ký `Raumakey.<sidhash>.show-panel` tới mọi cửa sổ cấp cao nhất;
  3. thoát.

  Bản đang chạy mở bảng cấu hình, hoặc đưa bảng đang mở lên trước. Cách này giống UniKey và EVKey:
  bấm lại vào exe thì bảng cấu hình hiện ra.
- Controller chạy bằng quyền quản trị cho thông điệp này đi qua UIPI, như `TaskbarCreated`.
- D45 thay câu "không báo cho bản đang chạy" ở bước 1 của chế độ chạy.

### 22.4 Menu khay (sửa D42)

- Thêm "Bảng cấu hình…" ở đầu menu, ngăn với "Gõ tiếng Việt (Ctrl+Shift)" bằng một separator. Các
  mục còn lại giữ nguyên.
- Click vào biểu tượng vẫn đổi chế độ Việt/Anh. Từ D46 (§22.6), nháy đúp mở bảng cấu hình và giữ
  nguyên chế độ.

### 22.5 Kiểm thử

- Test không phụ thuộc host:
  - các byte của template hộp thoại;
  - trạng thái bảng được dựng từ cấu hình;
  - yêu cầu ứng với từng thao tác trên bảng;
  - effect của tray khi mở, cập nhật và đóng bảng;
  - mô hình danh sách gõ tắt và việc giữ khoá lạ khi lưu.
- Test trên Windows: hộp thoại thật chạy trên luồng riêng, được điều khiển bằng `SendMessageW` từ
  luồng test. Hộp thoại test nằm ngoài màn hình và không lấy focus.
- Danh sách kiểm tra thủ công nằm ở module `desktop` của controller và ở `GUIDE.md` §5.6. Một test
  tự động thành công không phải bằng chứng về giao diện khi chạy thật.

### 22.6 Nháy đúp vào biểu tượng (D46)

- Nháy đúp vào biểu tượng V/E ở khay hệ thống mở bảng cấu hình, giống mục "Bảng cấu hình…".
- Một cú nháy vẫn đổi chế độ ngay, không chờ: với một bộ gõ, chế độ phải đổi trước khi người dùng
  gõ phím đầu tiên. Vì vậy cú nháy đầu của một lần nháy đúp đã đổi chế độ; khi nhận ra nháy đúp,
  tray trả chế độ về như trước cú nháy đầu (qua đúng đường đổi chế độ: biểu tượng, hook, lưu tệp)
  rồi mở bảng.
- Shell báo một lần nháy đúp theo thứ tự: nhấn (`WM_LBUTTONDOWN`), nhả (`WM_LBUTTONUP`),
  `NIN_SELECT`, `WM_LBUTTONDBLCLK` (thay cho lần nhấn thứ hai), nhả, và tuỳ phiên bản Windows,
  thêm một `NIN_SELECT` nữa. Lần nhấn thứ hai không thành nháy đúp (quá chậm, hoặc chuột đã dịch
  đi) thì tới dưới dạng `WM_LBUTTONDOWN`. `NIN_SELECT` tới lúc nhả chuột, dù giữ chuột lâu đến
  đâu, nên tray dựa vào các lần nhấn, không dựa vào khoảng cách giữa hai `NIN_SELECT`:
  - `WM_LBUTTONDBLCLK` là nháy đúp. Tray trả lại chế độ mà `NIN_SELECT` gần nhất đã đổi, nếu sau
    `NIN_SELECT` đó không có lần nhấn nào (tức nó là cú nháy đầu của lần nháy đúp này) và nó tới
    trước `WM_LBUTTONDBLCLK` không quá thời gian nháy đúp của hệ thống (`GetDoubleClickTime`)
    cộng 2 giây. Bảng luôn mở.
  - `NIN_SELECT` đầu tiên sau `WM_LBUTTONDBLCLK`, trước lần nhấn kế tiếp, là của cú nháy thứ hai.
    Nó bị bỏ qua nếu tới trong thời gian nháy đúp kể từ lần nhả chuột sau `WM_LBUTTONDBLCLK`;
    khi chưa thấy lần nhả nào, trong thời gian nháy đúp cộng 2 giây kể từ `WM_LBUTTONDBLCLK`.
    Một lần nhấn mới kết thúc việc chờ này, nên một cú nháy thật về sau không bao giờ bị mất.
  - Khi shell đã báo một lần nhấn hay một `WM_LBUTTONDBLCLK`, chỉ `WM_LBUTTONDBLCLK` mới là nháy
    đúp, và hai `NIN_SELECT` là hai cú nháy. Với shell chưa báo lần nhấn nào (dự phòng), hai
    `NIN_SELECT` cách nhau không quá thời gian nháy đúp là một lần nháy đúp.
- Các mốc thời gian là lúc tray xử lý thông báo; 2 giây cộng thêm dành cho lúc luồng của tray
  hay Explorer bận. Nháy ba lần là một lần nháy đúp và một cú nháy; nháy bốn lần là hai lần nháy
  đúp.
- Ở mỗi nhịp 2 giây của bộ hẹn giờ, tray đọc lại thời gian nháy đúp, và quên cú nháy hay lần nhả
  đã quá hạn, để chúng không sống lại khi đồng hồ `GetTickCount` quay vòng sau 49,7 ngày. Lần
  nhấn thứ hai đang được giữ thì vẫn được chờ: lần nhả của nó mới tính.
- Chọn biểu tượng bằng bàn phím (`NIN_KEYSELECT`) vẫn chỉ đổi chế độ. Sau nó, hay sau menu chuột
  phải, một lần nháy đúp không trả lại chế độ của cú nháy trước đó.

### 22.7 Loại trừ ứng dụng (D47)

- Chủ dự án yêu cầu: trong một số ứng dụng, ví dụ ô nhập trong cửa sổ Remote Desktop, Raumakey
  luôn gõ tiếng Anh; danh sách nằm trong tệp cấu hình. Thiết kế: một bảng mới trong
  `raumakey.toml`:

```toml
[exclusions]
# Ứng dụng luôn gõ tiếng Anh: tên tệp exe, không phân biệt hoa thường
apps = ["mstsc.exe", "msrdc.exe", "rdcman.exe", "vmconnect.exe"]
```

- Mặc định gồm các ứng dụng điều khiển máy từ xa: Remote Desktop Connection (`mstsc.exe`), ứng
  dụng Remote Desktop và Windows App (`msrdc.exe`), Remote Desktop Connection Manager
  (`rdcman.exe`) và Hyper-V VM Connection (`vmconnect.exe`). Phím gõ vào các cửa sổ đó được gửi
  sang máy kia, nơi có bộ gõ riêng; Raumakey sửa chữ bằng Backspace ở đó sẽ làm lệch chữ.
  `apps = []` tắt mọi loại trừ. Một tệp cũ không có bảng `[exclusions]` dùng danh sách mặc định.
- Trong một ứng dụng bị loại trừ, Raumakey không biến đổi phím nào và không mở rộng gõ tắt, như
  khi tắt tiếng Việt và tắt gõ tắt, bất kể chế độ đang chọn. Phím tắt Ctrl+Shift vẫn đổi chế độ
  chung.
- Cơ chế dùng lại chính sách ứng dụng sẵn có (§5.3, kế hoạch D13): `Settings` thêm vào đầu danh
  sách quy tắc của snapshot một quy tắc cho mỗi exe bị loại trừ, với `vietnamese = false` và
  `macros = false`. Theo thứ tự phân giải chính sách, quy tắc này thắng mọi quy tắc chỉ theo exe và
  mọi named set của `raumakey-app-rules.toml`; chỉ một quy tắc có `window_class` cho cùng exe, đặt
  trong `raumakey-app-rules.toml`, mới thắng nó, và chỉ với lớp cửa sổ đó.
- Hook đã biết tên exe của cửa sổ foreground, nên việc gõ tuân theo loại trừ ngay ở phím đầu tiên.
- Ứng dụng UWP chạy bên trong cửa sổ khung của `ApplicationFrameHost.exe`, và đó là exe
  foreground mà hook và tray thấy, nên một mục `[exclusions]` ghi exe riêng của một ứng dụng UWP
  không có tác dụng.
- Biểu tượng khay hiện **E** khi ứng dụng ở foreground bị loại trừ, với tooltip, ví dụ "Raumakey:
  đang gõ tiếng Anh trong mstsc.exe (ứng dụng loại trừ)". Tray theo dõi cửa sổ foreground bằng
  `SetWinEventHook(EVENT_SYSTEM_FOREGROUND)` (out-of-context, trên luồng của tray). Chế độ chung,
  dấu chọn trong menu và bảng cấu hình không đổi; khi rời ứng dụng đó, biểu tượng hiện lại chế độ
  chung.
- Tray phân giải chính sách với cùng exe và lớp cửa sổ như hook: lớp của cửa sổ đang có focus bàn
  phím trong luồng của cửa sổ foreground, không có thì lớp của chính cửa sổ foreground. Nhờ vậy
  một quy tắc có `window_class` cho biểu tượng và việc gõ cùng một kết quả. Watcher chỉ báo khi
  cửa sổ foreground đổi; focus chuyển bên trong một cửa sổ được tray thấy ở nhịp 2 giây kế tiếp.
- Ban đầu danh sách chỉ sửa trong tệp, như chủ dự án yêu cầu; từ D48 (§22.8) nó cũng được sửa trong
  hộp thoại "Ứng dụng loại trừ". Sửa tay có hiệu lực sau khoảng 2 giây. Một mục không phải tên exe
  hợp lệ bị bỏ qua và được báo theo đường dẫn schema (`exclusions.apps[2]`).

### 22.8 Hộp thoại ứng dụng loại trừ (D48)

- Chủ dự án muốn quản lý danh sách loại trừ (§22.7) trong một cửa sổ riêng, không chỉ bằng cách
  sửa tệp. Ảnh hộp thoại "Special Cases" của EVKey chỉ dùng để hiểu nhu cầu; bố cục, câu chữ và
  cách chọn ứng dụng của Raumakey là thiết kế riêng, và chỉ quản lý một việc: ứng dụng luôn gõ tiếng
  Anh. Các tuỳ chọn riêng cho từng ứng dụng (luôn bật tiếng Việt, kiểu gõ hay bảng mã riêng) nằm
  ngoài phạm vi.
- Mở bằng nút **Ứng dụng loại trừ…** trong nhóm "Hệ thống" của bảng cấu hình.
- Hộp thoại "Raumakey – Ứng dụng loại trừ" gồm:
  - một dòng giải thích: trong các ứng dụng của danh sách, Raumakey luôn gõ tiếng Anh và không mở
    rộng gõ tắt; mọi thay đổi có hiệu lực và được lưu ngay;
  - danh sách hai cột, **Tệp chương trình** và **Mô tả**. Mô tả là `FileDescription` của chương
    trình khi tìm thấy nó đang chạy; với bốn ứng dụng mặc định, Raumakey có sẵn tên sản phẩm;
  - ô **Ứng dụng:** dạng combo nhập được: gõ hoặc dán tên, đường dẫn, hoặc chọn một ứng dụng đang
    có cửa sổ (nút **Làm mới** đọc lại danh sách này); nút **Duyệt…** mở hộp thoại chọn tệp
    `.exe`;
  - các nút **Thêm**, **Xoá**, **Mặc định** (hỏi lại trước), **Đóng**. Phím Delete trong danh
    sách xoá như **Xoá**; sau khi xoá, dòng kế tiếp (nếu xoá dòng cuối thì dòng trước nó) được
    chọn để xoá tiếp. Hộp thoại không bao giờ vô hiệu hoá nút đang giữ focus mà không chuyển
    focus về ô **Ứng dụng:** trước, nên người dùng bàn phím không bị mất focus (ví dụ sau khi
    bấm **Thêm**, focus nằm ở ô nhập, sẵn cho tên kế tiếp).
- Nếu hộp thoại chọn tệp của **Duyệt…** không mở được, một cảnh báo nói rõ lỗi; hộp thoại không
  lặng lẽ bỏ qua.
- Tên nhập vào được chuẩn hoá đúng như một mục của `[exclusions] apps` (`normalize_excluded_app`).
  Tên không hợp lệ thì hộp thoại báo; tên đã có thì dòng đó được chọn.
- Như bảng cấu hình, không có nút Lưu: mỗi lần thêm, xoá hay đặt lại đi qua tray tới worker,
  được áp dụng và ghi ngay vào `raumakey.toml`; một thay đổi chưa lưu được vẫn có hiệu lực và được
  thử lưu lại. Hộp thoại hiện danh sách mà tray gửi lại, nên cũng đổi theo khi tệp được sửa tay.
- Hộp thoại chạy trên luồng riêng như bảng cấu hình; mỗi lúc chỉ có một; thoát Raumakey đóng nó.
  Một thay đổi bấm ngay trước khi thoát, còn nằm trong hàng đợi của tray, vẫn được gửi tới worker
  và được lưu trước khi worker kết thúc, như lần đổi chế độ cuối cùng bằng phím tắt.
