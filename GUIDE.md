# Hướng dẫn build và dùng thử Raumakey

Tài liệu này hướng dẫn cách dựng môi trường, build, đóng gói, cài đặt và dùng thử Raumakey trên
Windows.

> **Trạng thái:** Raumakey 0.1.0 là bản phát triển. Mọi bước build và test tự động đều đạt, nhưng
> bộ gõ **chưa được kiểm chứng khi chạy thật** trong ứng dụng Windows. Chỉ nên cài trên máy dùng để
> thử. Mục 7 liệt kê những gì bản này chưa làm được.

## 1. Cần chuẩn bị gì

| Thành phần | Ghi chú |
|---|---|
| Windows 10 hoặc 11, x64 | Dùng cho cả máy build lẫn máy chạy thử. Bản ARM64 chỉ được kiểm tra biên dịch, chưa từng chạy. |
| Git | Để lấy mã nguồn. |
| Rust qua `rustup` | `rust-toolchain.toml` ghim Rust 1.99.1. Lần build đầu, `rustup` tự cài toolchain, `rustfmt`, `clippy` và ba target `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, `aarch64-pc-windows-msvc`. |
| Linker MSVC và thư viện Windows SDK | Chọn cách A hoặc cách B ở mục 1.1. |
| `cargo-deny` (tuỳ chọn) | Chỉ cần cho `cargo xtask audit` và `release-gate`: `cargo install cargo-deny --locked`. |
| WiX Toolset (tuỳ chọn) | Khi có lệnh `wix` trong `PATH`, `cargo xtask package` build thêm file MSI. Không có WiX thì bỏ qua MSI. |

Không cần `rc.exe`: tài nguyên (icon, version, manifest) do crate `raumakey-resources` tự tạo.

### 1.1 Linker và thư viện Windows

**Cách A: Visual Studio Build Tools (khuyên dùng nếu chưa có gì).** Cài Visual Studio 2022 Build
Tools với workload **Desktop development with C++**. Hãy đánh dấu các thành phần sau:

- MSVC v143 build tools cho x64/x86.
- MSVC v143 build tools cho ARM64. `cargo xtask package` luôn đóng cả gói ARM64 nên cần thành phần
  này.
- Windows 11 SDK.

Xong là `cargo` dùng được ngay, không phải cấu hình gì thêm.

**Cách B: không cài Visual Studio (rust-lld + xwin).** Máy phát triển chính của dự án dùng cách
này.

1. Cài `xwin` 0.10.0 rồi tải CRT và thư viện SDK cho cả ba kiến trúc. Bạn phải chấp nhận giấy
   phép của Microsoft để tải:

   ```powershell
   cargo install xwin --version 0.10.0 --locked
   xwin --accept-license --arch x86,x86_64,aarch64 splat --output D:\xwin\sysroot --use-winsysroot-style --preserve-ms-arch-notation --disable-symlinks
   ```

   Phải có `--disable-symlinks`: nếu không, `splat` báo lỗi 1314 (thiếu quyền tạo symlink).

2. Thêm cấu hình sau vào `%CARGO_HOME%\config.toml` (mặc định là `%USERPROFILE%\.cargo\config.toml`).
   Đây là cấu hình riêng của từng máy, không nằm trong kho mã. Tên thư mục phiên bản MSVC
   (`14.44.17.14`) và SDK (`10.0.26100`) phải khớp với những gì `xwin` đã tải về:

   ```toml
   [target.x86_64-pc-windows-msvc]
   linker = "rust-lld"
   rustflags = [
     "-Lnative=D:/xwin/sysroot/VC/Tools/MSVC/14.44.17.14/lib/x64",
     "-Lnative=D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/um/x64",
     "-Lnative=D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/ucrt/x64",
   ]

   [target.i686-pc-windows-msvc]
   linker = "rust-lld"
   rustflags = [
     "-Lnative=D:/xwin/sysroot/VC/Tools/MSVC/14.44.17.14/lib/x86",
     "-Lnative=D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/um/x86",
     "-Lnative=D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/ucrt/x86",
   ]

   [target.aarch64-pc-windows-msvc]
   linker = "rust-lld"
   rustflags = [
     "-Lnative=D:/xwin/sysroot/VC/Tools/MSVC/14.44.17.14/lib/arm64",
     "-Lnative=D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/um/arm64",
     "-Lnative=D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/ucrt/arm64",
   ]

   # Build script và proc-macro chạy trên máy host không nhận rustflags của target khi có
   # --target; lld-link đọc LIB sau /LIBPATH nên đặt thư viện x64 ở đây.
   [env]
   LIB = "D:/xwin/sysroot/VC/Tools/MSVC/14.44.17.14/lib/x64;D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/um/x64;D:/xwin/sysroot/Windows Kits/10/Lib/10.0.26100/ucrt/x64"
   ```

### 1.2 Lệnh tắt `cargo xtask`

Các lệnh build của dự án nằm trong crate `xtask`. Có thể gọi bằng `cargo run -p xtask -- <lệnh>`.
Muốn gõ ngắn `cargo xtask <lệnh>` thì thêm alias vào `%CARGO_HOME%\config.toml`:

```toml
[alias]
xtask = "run --quiet --package xtask --"
```

Phần còn lại của tài liệu này dùng dạng ngắn `cargo xtask`.

## 2. Lấy mã nguồn

```powershell
git clone https://github.com/khachuy2705/raumakey.git
cd raumakey
```

Thư mục `restricted_evidence/` (nếu có trên máy bạn) không nằm trong Git và không được dùng khi
phát triển; xem `AGENTS.md`.

## 3. Build và test khi phát triển

| Việc | Lệnh |
|---|---|
| Build toàn bộ workspace (debug, máy host) | `cargo build --workspace` |
| Chạy toàn bộ test | `cargo test --workspace` |
| Kiểm tra định dạng | `cargo fmt --all --check` |
| Bộ kiểm tra đầy đủ như CI | `cargo xtask ci` |
| CI nhưng bỏ bộ fuzz smoke | `cargo xtask ci --no-fuzz` |

`cargo xtask ci` lần lượt chạy:

1. `cargo fmt --check`.
2. Clippy với `-D warnings`, cả với feature mặc định lẫn năm feature `evkey-compat`.
3. Test toàn workspace với cả hai bộ feature.
4. Bộ fuzz smoke.

Một số test được đánh dấu `#[ignore]` vì cần kiểm tra thủ công trên Windows; chúng không chạy
trong `ci`.

Có thể chạy thêm test 32-bit (chạy qua WOW64 trên Windows x64):

```powershell
cargo test --locked -p raumakey-engine -p raumakey-macro -p raumakey-tsf -p raumakey-windows -p raumakey-tests --target i686-pc-windows-msvc --lib --bins --tests
```

Với cách B, doctest của target i686 không link được (linker lấy nhầm `msvcrt.lib` x64), nên lệnh
trên bỏ doctest.

## 4. Build bản phát hành

Chạy lần lượt ba lệnh:

```powershell
cargo xtask cross --release     # build raumakey.exe, raumakey_tip.dll, raumakey-test-runner.exe cho x86_64, i686, aarch64
cargo xtask inspect --release   # kiểm tra PE: kiến trúc, subsystem, export, manifest, tài nguyên, CLSID, CRT tĩnh
cargo xtask package --release   # đóng gói x64 và ARM64, kèm SHA256SUMS, manifest.json, script cài đặt
```

Bản release link CRT tĩnh (`+crt-static`) nên không cần cài Visual C++ Redistributable.

Kết quả nằm trong `target\package\`:

| Đường dẫn | Nội dung |
|---|---|
| `raumakey-0.1.0-windows-x64\` | Gói để dùng thử: `raumakey.exe`, `x64\raumakey_tip.dll`, `x86\raumakey_tip.dll`, `tools\raumakey-test-runner.exe`, `install.ps1`, `uninstall.ps1`, `README.txt`, `LICENSE`, `THIRD_PARTY.md`, `SHA256SUMS`, `manifest.json` |
| `raumakey-0.1.0-windows-arm64\` | Gói ARM64, chỉ kiểm tra biên dịch, không kèm script cài đặt |
| `raumakey.wxs` | Mã nguồn WiX; có thêm file MSI nếu máy có `wix` |

Bỏ `--release` ở cả ba lệnh thì được bản debug (thư mục có hậu tố `-debug`), chỉ dùng khi phát
triển.

Nên kiểm tra gói vừa đóng bằng chạy thử (dry run). Lệnh này chỉ in kế hoạch đăng ký, không thay đổi
máy và không cần quyền quản trị viên:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File target\package\raumakey-0.1.0-windows-x64\install.ps1 -DryRun
```

`cargo xtask release-gate` chạy toàn bộ cổng phát hành: `ci`, `audit`, `cross --release`,
`inspect --release`, kiểm tra review toàn nhánh, `package --release` và chạy thử cả controller lẫn
hai script cài đặt. Lệnh này cần `cargo-deny` và một file review được duyệt
(`target\release-evidence\whole-branch-review.json`). Đạt cổng phát hành chỉ có nghĩa là
"đã kiểm chứng cross-build", chưa phải đã kiểm chứng khi chạy thật.

Trên máy phát triển chính, thư mục `release\` ở gốc kho mã chứa sẵn một bản x64 đã build để dùng
thử. Thư mục này không được đưa lên Git.

## 5. Cài đặt và dùng thử

### 5.1 Cài đặt

1. Chép cả thư mục `raumakey-0.1.0-windows-x64` vào một thư mục chỉ quản trị viên sửa được, ví dụ
   `C:\Program Files\Raumakey`. Mọi ứng dụng sẽ nạp `raumakey_tip.dll` từ chỗ đã cài. Nếu cài từ
   thư mục người dùng ghi được (như `Downloads`), Raumakey vẫn cài nhưng in cảnh báo (quyết định
   D37): một tiến trình chạy dưới tài khoản của bạn có thể thay DLL đó.
2. Gói chưa được ký số. Nếu nhận gói từ máy khác, hãy so checksum với `SHA256SUMS`:

   ```powershell
   Get-FileHash -Algorithm SHA256 "C:\Program Files\Raumakey\x64\raumakey_tip.dll"
   ```

3. Mở PowerShell trong thư mục đó và chạy (Windows sẽ hỏi quyền quản trị viên):

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1
   ```

   Script đăng ký TIP 64-bit và 32-bit bằng `regsvr32` đúng bitness, đọc lại registry để kiểm
   tra, rồi ghi bản ghi cài đặt `HKLM\Software\Raumakey\Install`. Nếu một bước lỗi, các thay đổi
   của lần chạy đó được hoàn tác. Cửa sổ quản trị viên sẽ chờ bạn nhấn Enter trước khi đóng.

4. Thêm bàn phím Raumakey. Lệnh cài đặt chỉ đăng ký bộ gõ, không tự thêm nó vào danh sách bàn phím
   của bạn. Vào **Settings > Time & language > Language & region**. Nếu chưa có **Vietnamese**,
   chọn **Add a language** để thêm. Sau đó chọn **Vietnamese > Language options > Keyboards >
   Add a keyboard > Raumakey**. Giao diện Windows tiếng Việt cũng theo đúng đường này, với tên
   mục tương ứng.

### 5.2 Gõ thử

1. Mở Notepad, nhấn **Windows+Space** để chọn Raumakey.
2. Gõ `tieengs vieetj nguwowif`. Kết quả phải là "tiếng việt người", con trỏ nằm sau chữ cuối.
3. Nhấn **Ctrl+Shift** để tắt tiếng Việt, gõ `aa` sẽ ra "aa". Nhấn **Ctrl+Shift** lần nữa để bật
   lại, gõ ` aa` sẽ ra " â".

Danh sách dưới đây giúp phát hiện lỗi mà test tự động không thấy. Hãy ghi lại bước nào không đúng
như mô tả.

| # | Thao tác | Kết quả đúng |
|---:|---|---|
| 1 | Gõ `aa` (hiện "â") rồi Backspace | Còn lại "a" |
| 2 | Gõ `ba ba`, click ngay sau chữ "ba" đầu, gõ `s` | "bas ba" (click đã kết thúc từ). Thử lại bằng phím mũi tên thay cho click |
| 3 | Gõ `a` ở một cửa sổ Notepad, Alt+Tab sang cửa sổ khác, gõ `a` | Mỗi cửa sổ hiện "a", không cửa sổ nào bị xoá chữ |
| 4 | Trong ô mật khẩu (ví dụ ô mật khẩu trên Microsoft Edge, bấm nút hiện mật khẩu để xem), gõ `aa` | Ô chứa "aa": bộ gõ không biến đổi phím trong ô mật khẩu |
| 5 | Gõ `after` rồi Enter | Dòng chứa "after" (từ không phải tiếng Việt được khôi phục), con trỏ xuống dòng đúng một lần |
| 6 | **Quan trọng (D36).** Tắt tiếng Việt bằng Ctrl+Shift, gõ `1234567890-=[]` | Hiện đúng `1234567890-=[]`. Nếu ra chữ tiếng Việt (ă, â, ê…), bộ gõ đang chạy trên layout bàn phím tiếng Việt của Windows; khi đó số và VNI sẽ hỏng. Hãy báo lại |
| 7 | Chuyển qua lại giữa Raumakey và bàn phím khác (Windows+Space) vài lần trong lúc gõ, đóng rồi mở lại Notepad | Không có gì bị treo hay crash, vẫn gõ được |
| 8 | Thử trong các ứng dụng bạn hay dùng: trình duyệt, Word, ô chat, ô tìm kiếm của Start | Gõ đúng, không mất chữ, không lặp chữ |

### 5.3 Biểu tượng khay hệ thống và các lệnh khác

- Chạy `raumakey.exe` không kèm tham số sẽ hiện biểu tượng **V/E** ở khay hệ thống. Click để đổi
  V/E, click phải để mở menu, chọn "Thoát" để đóng. Ở bản này biểu tượng **chưa điều khiển bộ gõ**,
  vì TIP chưa kết nối với controller. Hãy dùng Ctrl+Shift để bật hoặc tắt tiếng Việt.
- `raumakey.exe help` in toàn bộ lệnh. Một vài lệnh hữu ích:

  ```powershell
  .\raumakey.exe --version
  .\raumakey.exe convert --from tcvn3 --to unicode --in cu.txt --out moi.txt
  .\raumakey.exe convert --from vni-windows --to unicode --clipboard
  .\raumakey.exe startup shortcut   # khởi động cùng Windows qua shortcut trong thư mục Startup
  .\raumakey.exe startup none       # tắt khởi động cùng Windows
  ```

  Bảng mã hỗ trợ: `unicode`, `unicode-nfd`, `viqr`, `tcvn3`, `vni-windows`, `viscii`, `cp1258`,
  `ncr-decimal`, `ncr-hex`, `utf8-literal`.

## 6. Gỡ cài đặt

1. Trong **Settings > Time & language > Language & region > Vietnamese > Language options**, gỡ
   bàn phím Raumakey khỏi danh sách.
2. Trong thư mục đã cài, chạy (Windows sẽ hỏi quyền quản trị viên):

   ```powershell
   powershell -NoProfile -ExecutionPolicy Bypass -File uninstall.ps1
   ```

   Lệnh này chỉ xoá các mục đăng ký của Raumakey. Thêm `-DryRun` để xem trước mà không thay đổi
   gì.
3. Ứng dụng nào còn đang nạp DLL thì giữ nó tới khi ứng dụng thoát. Hãy đăng xuất hoặc khởi động
   lại máy rồi mới xoá thư mục cài đặt.

## 7. Bản này chưa làm được gì

- **Chỉ có Telex với tuỳ chọn mặc định.** Engine đã hỗ trợ VNI, VIQR và kiểu gõ tự định nghĩa,
  nhưng TIP chưa có kết nối IPC tới controller nên chưa chọn được kiểu gõ, chưa đổi được cấu hình
  và **macro chưa hoạt động**.
- Chưa có bảng cài đặt, chưa có hook bàn phím cho phím tắt toàn cục, chưa có đường dự phòng cho
  ứng dụng không hỗ trợ TSF.
- Lệnh `migrate` (nhập cấu hình và macro từ EVKey) chưa có trong bản này.
- Trong ô mật khẩu, mọi phím đi thẳng vào ứng dụng, không biến đổi.
- Gói chưa được ký số nên SmartScreen hoặc phần mềm diệt virus có thể cảnh báo.
- Chưa ai thử bộ gõ trong ứng dụng thật. Mục 5.2 là danh sách cần kiểm tra.

Kế hoạch §14 ghi đầy đủ việc đã xong và việc còn lại.

## 8. Xử lý sự cố

| Hiện tượng | Cách xử lý |
|---|---|
| "running scripts is disabled on this system" | Chạy script bằng đúng lệnh trong tài liệu này: `powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1`. Lệnh này không đổi chính sách của máy. |
| Không thấy Raumakey khi Add a keyboard | Thêm ngôn ngữ Vietnamese trước. Chạy lại `install.ps1` và xem có báo lỗi không, rồi đăng xuất và đăng nhập lại. |
| Ctrl+Shift chuyển bàn phím thay vì bật/tắt tiếng Việt | Windows đang dùng Ctrl+Shift làm phím chuyển layout. Đổi ở **Settings > Time & language > Typing > Advanced keyboard settings > Input language hot keys**. |
| Gõ số ra chữ tiếng Việt | Xem bước 6 ở mục 5.2 (D36). Hãy báo lại và tạm thời dùng bàn phím khác. |
| Ứng dụng bị treo hoặc crash khi dùng Raumakey | Chuyển sang bàn phím khác bằng Windows+Space, gỡ cài đặt theo mục 6 và báo lại tên ứng dụng. |
| Build lỗi `LNK1181: cannot open input file 'kernel32.lib'` | Thiếu Windows SDK (cách A) hoặc sai đường dẫn `-Lnative`/`LIB` (cách B). Kiểm tra lại tên thư mục phiên bản MSVC và SDK. |
| `cargo xtask package` báo thiếu artifact | Chạy `cargo xtask cross --release` trước. Cách A cần thêm thành phần MSVC ARM64. |
| `cargo xtask audit` báo không thấy `cargo-deny` | `cargo install cargo-deny --locked`. |

## 9. Tài liệu liên quan

- `README.md`: giới thiệu dự án.
- `docs/superpowers/specs/2026-10-16-raumakey-design.md`: thiết kế.
- `docs/superpowers/plans/2026-10-16-raumakey-implementation.md`: kế hoạch triển khai. §14 ghi
  trạng thái, §15 ghi các quyết định.
- `docs/reference/evkey_compatibility_test_matrix.md`: ma trận kiểm thử tương thích.
- `THIRD_PARTY.md`: nguồn gốc mã và giấy phép bên thứ ba.
