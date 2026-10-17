# Hướng dẫn build và dùng thử Raumakey

Tài liệu này hướng dẫn cách dựng môi trường, build, đóng gói và dùng thử Raumakey trên Windows.

Raumakey là một tệp `raumakey.exe` duy nhất, dùng giống UniKey hay EVKey: chép vào thư mục nào cũng
được, bấm chạy là gõ được, không cần cài đặt hay quyền quản trị. Cấu hình được lưu trong tệp
`raumakey.toml` nằm cạnh `raumakey.exe`.

> **Trạng thái:** Raumakey 0.1.0 là bản phát triển. Mọi bước build và test tự động đều đạt, nhưng
> bộ gõ **chưa được kiểm chứng khi chạy thật** trong ứng dụng Windows. Mục 7 liệt kê những gì bản
> này chưa làm được.

## 1. Cần chuẩn bị gì

| Thành phần | Ghi chú |
|---|---|
| Windows 10 hoặc 11, x64 | Dùng cho cả máy build lẫn máy chạy thử. Bản ARM64 chỉ được kiểm tra biên dịch, chưa từng chạy. |
| Git | Để lấy mã nguồn. |
| Rust qua `rustup` | `rust-toolchain.toml` ghim Rust 1.99.1. Lần build đầu, `rustup` tự cài toolchain, `rustfmt`, `clippy` và ba target `x86_64-pc-windows-msvc`, `i686-pc-windows-msvc`, `aarch64-pc-windows-msvc`. |
| Linker MSVC và thư viện Windows SDK | Chọn cách A hoặc cách B ở mục 1.1. |
| `cargo-deny` (tuỳ chọn) | Chỉ cần cho `cargo xtask audit` và `release-gate`: `cargo install cargo-deny --locked`. |

Không cần `rc.exe`: tài nguyên (icon, version, manifest) do crate `raumakey-resources` tự tạo.

### 1.1 Linker và thư viện Windows

**Cách A: Visual Studio Build Tools (khuyên dùng nếu máy chưa có gì).** Cài Visual Studio 2022
Build Tools với workload **Desktop development with C++**, và đánh dấu các thành phần sau:

- MSVC v143 build tools cho x64/x86.
- MSVC v143 build tools cho ARM64. `cargo xtask cross` build cả ba target, và `cargo xtask package`
  đóng thêm gói ARM64, nên cần thành phần này.
- Windows 11 SDK.

Cài xong là `cargo` dùng được ngay, không phải cấu hình gì thêm.

**Cách B: không cài Visual Studio (rust-lld + xwin).** Máy phát triển chính của dự án dùng cách
này.

1. Cài `xwin` 0.10.0 rồi tải CRT và thư viện SDK cho cả ba kiến trúc. Lệnh tải yêu cầu bạn chấp
   nhận giấy phép của Microsoft:

   ```powershell
   cargo install xwin --version 0.10.0 --locked
   xwin --accept-license --arch x86,x86_64,aarch64 splat --output D:\xwin\sysroot --use-winsysroot-style --preserve-ms-arch-notation --disable-symlinks
   ```

   Bắt buộc có `--disable-symlinks`: thiếu nó, `splat` báo lỗi 1314 (không có quyền tạo symlink).

2. Thêm cấu hình sau vào `%CARGO_HOME%\config.toml` (mặc định là
   `%USERPROFILE%\.cargo\config.toml`). Cấu hình này là của riêng từng máy, không nằm trong kho mã.
   Tên thư mục phiên bản MSVC (`14.44.17.14`) và SDK (`10.0.26100`) phải khớp với những gì `xwin`
   đã tải về:

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
2. Clippy với `-D warnings`, cả với feature mặc định lẫn sáu feature `evkey-compat`.
3. Test toàn workspace với cả hai bộ feature.
4. Bộ fuzz smoke.

Một số test được đánh dấu `#[ignore]` vì cần kiểm tra thủ công trên Windows; chúng không chạy
trong `ci`.

Có thể chạy thêm test 32-bit (chạy qua WOW64 trên Windows x64):

```powershell
cargo test --locked -p raumakey-engine -p raumakey-macro -p raumakey-session -p raumakey-tsf -p raumakey-windows -p raumakey-tests --target i686-pc-windows-msvc --lib --bins --tests
```

Với cách B, doctest của target i686 không link được (linker lấy nhầm `msvcrt.lib` x64), nên lệnh
trên bỏ doctest.

Đang chạy test thì không cần tắt Caps Lock hay UniKey: các test không phụ thuộc trạng thái bàn
phím của máy.

## 4. Build bản phát hành

Chạy lần lượt ba lệnh:

```powershell
cargo xtask cross --release     # build mọi artifact cho x86_64, i686, aarch64
cargo xtask inspect --release   # kiểm tra PE: kiến trúc, subsystem, export, manifest, tài nguyên, CRT tĩnh
cargo xtask package --release   # đóng gói bản portable x64 và ARM64
```

Bản release link CRT tĩnh (`+crt-static`) nên không cần cài Visual C++ Redistributable.

Kết quả nằm trong `target\package\`:

| Đường dẫn | Nội dung |
|---|---|
| `raumakey-0.1.0-windows-x64\` | Bản dùng thử: `raumakey.exe`, `README.txt`, `LICENSE`, `THIRD_PARTY.md`, `SHA256SUMS`, `manifest.json` |
| `raumakey-0.1.0-windows-arm64\` | Bản ARM64, chỉ kiểm tra biên dịch |

Chỉ có `raumakey.exe` là cần để chạy; các tệp khác là tài liệu, giấy phép và checksum. Bỏ
`--release` ở cả ba lệnh thì được bản debug (thư mục có hậu tố `-debug`), chỉ dùng khi phát triển.

`cross` vẫn build cả TIP TSF (`raumakey_tip.dll`) và test runner, vì mã của chúng còn trong kho,
nhưng gói phát hành không chứa chúng (thiết kế §21.1).

`cargo xtask release-gate` chạy toàn bộ cổng phát hành: `ci`, `audit`, `cross --release`,
`inspect --release`, kiểm tra review toàn nhánh, `package --release`, và `portable-check`
(gói chứa đúng các tệp trên, `raumakey.exe --version` chạy được). Lệnh này cần `cargo-deny` và một
file review được duyệt (`target\release-evidence\whole-branch-review.json`). Đạt cổng phát hành chỉ
có nghĩa là "đã kiểm chứng cross-build", chưa phải đã kiểm chứng khi chạy thật.

Trên máy phát triển chính, thư mục `release\` ở gốc kho mã chứa sẵn bản x64 đã build để dùng thử.
Thư mục này không được đưa lên Git.

## 5. Dùng thử

### 5.1 Chạy

1. Chép `raumakey.exe` (hoặc cả thư mục `raumakey-0.1.0-windows-x64`) vào một thư mục bạn có quyền
   ghi, ví dụ `D:\Tools\Raumakey`.
2. Tắt các bộ gõ tiếng Việt khác (UniKey, EVKey…), nếu không hai bộ gõ sẽ cùng biến đổi một phím.
3. Bấm đúp `raumakey.exe`. Gói chưa được ký số nên SmartScreen có thể cảnh báo; chọn **More info >
   Run anyway** nếu bạn tin bản build này. Biểu tượng **V** hiện ở khay hệ thống (góc phải thanh
   tác vụ; có thể nằm trong mũi tên ^).
4. Lần chạy đầu, Raumakey tạo `raumakey.toml` ngay cạnh `raumakey.exe`. Từ lần chạy thứ hai,
   Raumakey đọc lại tệp đó. Nếu thư mục không ghi được, hoặc nằm trong `C:\Program Files`, thư mục
   Windows hay thư mục tạm (ví dụ khi bạn chạy exe ngay trong tệp ZIP), cấu hình được lưu ở
   `%LOCALAPPDATA%\Raumakey\raumakey.toml`. Lần chạy đầu, một hộp thoại cho biết tệp cấu hình nằm
   ở đâu; về sau, mục **Giới thiệu** trong menu cho biết đường dẫn đang dùng.
5. Chỉ chạy được một Raumakey mỗi lúc: chạy lần nữa khi đang chạy thì lần sau tự thoát.

### 5.2 Gõ và đổi chế độ

- Mở Notepad, gõ `tieengs vieetj nguwowif`. Kết quả phải là "tiếng việt người".
- **Ctrl+Shift** hoặc click vào biểu tượng để đổi giữa tiếng Việt (**V**) và tiếng Anh (**E**).
  Chế độ được lưu lại cho lần chạy sau.
- Click phải vào biểu tượng để mở menu.

### 5.3 Menu khay

| Mục | Tác dụng |
|---|---|
| Gõ tiếng Việt (Ctrl+Shift) | Bật hoặc tắt tiếng Việt |
| Telex, VNI, VIQR | Chọn kiểu gõ |
| Kiểm tra chính tả | Chỉ bỏ dấu khi từ đúng chính tả tiếng Việt |
| Đặt dấu kiểu mới (oà, uý) | Đặt dấu thanh theo kiểu mới thay cho kiểu cũ (òa, úy) |
| Gõ tắt (macro) | Bật hoặc tắt macro trong `raumakey-macros.toml` |
| Khởi động cùng Windows | Tạo hoặc xoá shortcut `Raumakey.lnk` trong thư mục Startup của bạn |
| Mở file cấu hình | Mở `raumakey.toml` bằng Notepad |
| Giới thiệu | Phiên bản và đường dẫn tệp cấu hình |
| Thoát | Tắt Raumakey |

Mọi thay đổi trong menu được ghi ngay vào `raumakey.toml`. Nếu bạn sửa tệp bằng tay rồi lưu,
Raumakey nạp lại trong vòng vài giây. Một giá trị không hợp lệ được thay bằng giá trị mặc định.
Tệp không đọc được (ví dụ sai cú pháp TOML) thì Raumakey báo một lần, chạy với cấu hình mặc định
và không ghi đè tệp của bạn. Nếu tệp đã hỏng từ trước khi chạy, Raumakey hỏi bạn có muốn đổi tên
nó thành `raumakey.toml.bak-…` rồi tạo tệp mới không. Khi Raumakey ghi lại tệp, các dòng comment bạn
thêm vào không được giữ. Nếu một thay đổi chưa lưu được (ví dụ khi phần mềm diệt virus đang khoá
tệp), nó vẫn có hiệu lực và Raumakey tự thử lưu lại.

Về "Khởi động cùng Windows":

- Khi bạn chạy `raumakey.exe` ngay trong tệp ZIP, Raumakey không tạo shortcut và nhắc bạn giải nén
  ra một thư mục cố định trước, vì Windows sẽ xoá thư mục tạm đó.
- Nếu bạn chuyển exe sang chỗ khác, lần chạy kế tiếp sửa lại shortcut. Một shortcut đang trỏ tới
  một bản Raumakey khác vẫn còn trên máy thì được giữ nguyên.

Nếu `raumakey.toml` chọn một bảng phím tự định nghĩa (`method = "user"`), không mục nào trong
Telex/VNI/VIQR được đánh dấu và tooltip ghi "(kiểu gõ riêng)". Nếu bạn bật khởi động cùng Windows
bằng tác vụ quản trị (`raumakey startup elevated-task`, chạy bằng quyền quản trị), muốn tắt thì
chạy `raumakey startup none` cũng bằng quyền quản trị. Lệnh
`raumakey startup <none|shortcut|elevated-task>` cũng ghi lựa chọn vào `raumakey.toml`, nên menu
luôn hiện đúng.

### 5.4 Tệp cấu hình

Các tệp Raumakey tạo cạnh `raumakey.exe`:

| Tệp | Nội dung |
|---|---|
| `raumakey.toml` | Cấu hình: chế độ, kiểu gõ, tuỳ chọn, phím tắt, khởi động cùng Windows |
| `raumakey.toml.lock` | Tệp khoá dùng khi ghi cấu hình; để nguyên |
| `raumakey-macros.toml` | Macro (gõ tắt); không tự tạo, bạn tạo nếu cần |

`raumakey.toml` có các bảng chính:

```toml
[input]
enabled = true          # đang gõ tiếng Việt
method = "telex"        # "telex", "vni" hoặc "viqr"
spell_check = true      # kiểm tra chính tả
modern_tone = false     # đặt dấu kiểu mới (oà, uý)

[macros]
enabled = true          # bật gõ tắt

[system]
startup = "none"        # "none" hoặc "shortcut" (khởi động cùng Windows)
```

Tên khoá đầy đủ và giá trị mặc định nằm trong tệp mà lần chạy đầu tạo ra; kế hoạch §5 mô tả toàn
bộ schema.

### 5.5 Gõ tắt (macro)

Tạo tệp `raumakey-macros.toml` (mã hoá UTF-8) cạnh `raumakey.toml`, ví dụ:

```toml
version = 1
revision = 1

[[macro]]
id = 1
trigger = "vn"
replacement = "Việt Nam"

[[macro]]
id = 2
trigger = "ko"
replacement = "không"

[[macro]]
id = 3
trigger = "hcm"
replacement = "Thành phố Hồ Chí Minh"
enabled = false         # tạm tắt mục này
```

- Mỗi mục có `id` là một số nguyên không trùng nhau, `trigger` (từ gõ tắt, tối đa 32 ký tự) và
  `replacement` (nội dung thay thế).
- Gõ từ gõ tắt rồi gõ dấu cách hoặc dấu câu thì từ đó được thay bằng nội dung thay thế. Kiểu chữ
  hoa của từ gõ tắt được áp cho nội dung thay thế: `ko` cho ra "không", `Ko` cho ra "Không", `KO`
  cho ra "KHÔNG".
- Mục "Gõ tắt (macro)" trong menu phải đang bật. Raumakey nạp lại tệp vài giây sau khi bạn lưu.
- Mục nào sai (thiếu `id`, từ gõ tắt quá dài…) sẽ bị bỏ qua, các mục còn lại vẫn dùng được.

### 5.6 Danh sách kiểm tra

Danh sách dưới đây giúp phát hiện lỗi mà test tự động không thấy. Hãy ghi lại bước nào không đúng
như mô tả và báo lại.

| # | Thao tác | Kết quả đúng |
|---:|---|---|
| 1 | Trong Notepad, gõ `aa` (hiện "â") rồi Backspace | Còn lại "a" |
| 2 | Gõ `ba ba`, click ngay sau chữ "ba" đầu, gõ `s` | "bas ba": click đã kết thúc từ. Thử lại với phím mũi tên thay cho click |
| 3 | Gõ `a` ở một cửa sổ, Alt+Tab sang cửa sổ khác, gõ `a` | Mỗi cửa sổ hiện "a", không cửa sổ nào bị xoá chữ |
| 4 | Gõ `after` rồi Enter | Dòng chứa "after" (từ không phải tiếng Việt được giữ nguyên), con trỏ xuống dòng đúng một lần |
| 5 | Chọn VNI trong menu, gõ `tie61ng vie65t` | "tiếng việt" |
| 6 | Bật Caps Lock, gõ `tieengs` | "TIẾNG" |
| 7 | Gõ trong trình duyệt, Word, ô chat, ô tìm kiếm của Start | Gõ đúng, không mất chữ, không lặp chữ |
| 8 | Gõ trong một cửa sổ chạy quyền quản trị (ví dụ PowerShell "Run as administrator") | Phím đi thẳng, không biến đổi (cần chạy Raumakey bằng quyền quản trị để gõ tiếng Việt ở đó) |
| 9 | Bật "Khởi động cùng Windows", đăng xuất rồi đăng nhập lại | Raumakey tự chạy, đúng chế độ đã lưu |
| 10 | Thoát Raumakey, chạy lại | Các tuỳ chọn đã chọn vẫn còn |
| 11 | Khoá máy (Win+L), mở lại, gõ `aa`, rồi nhấn Ctrl+Shift | "â", và Ctrl+Shift vẫn đổi được V/E ngay lần đầu |
| 12 | Gõ `xin `, khoá máy, đổi Caps Lock ở màn hình khoá, mở lại, gõ `tieengs` | Cả từ cùng kiểu chữ ("TIẾNG" hoặc "tiếng"), không lẫn hoa thường |
| 13 | Gõ `tie`, nhấn Alt một mình, rồi gõ `e` | Không có Backspace nào lọt vào menu của Notepad |
| 14 | Gõ nhanh, nhấn chồng phím, giữ phím để lặp | Không mất chữ, không lặp chữ, không đảo thứ tự |

### 5.7 Dòng lệnh

`raumakey.exe help` in toàn bộ lệnh. Một vài lệnh hữu ích:

```powershell
.\raumakey.exe --version
.\raumakey.exe convert --from tcvn3 --to unicode --in cu.txt --out moi.txt
.\raumakey.exe convert --from vni-windows --to unicode --clipboard
```

Bảng mã hỗ trợ: `unicode`, `unicode-nfd`, `viqr`, `tcvn3`, `vni-windows`, `viscii`, `cp1258`,
`ncr-decimal`, `ncr-hex`, `utf8-literal`. Các lệnh `install`/`uninstall` dành cho TIP TSF, không
còn trong bản phát hành (thiết kế §21.1).

## 6. Gỡ bỏ

1. Nếu đã bật "Khởi động cùng Windows", tắt nó trong menu trước (để xoá shortcut trong thư mục
   Startup).
2. Chọn **Thoát**.
3. Xoá thư mục chứa `raumakey.exe`. Nếu cấu hình nằm ở `%LOCALAPPDATA%\Raumakey`, xoá cả thư mục
   đó.

Raumakey không ghi gì vào registry.

## 7. Bản này chưa làm được gì

- Chưa ai thử bộ gõ trong ứng dụng thật. Mục 5.6 là danh sách cần kiểm tra.
- Hook không đọc được văn bản của ứng dụng, nên sửa chữ bằng Backspace rồi gõ lại, giống UniKey.
  Ứng dụng có tự hoàn thành (thanh địa chỉ trình duyệt, Excel) có thể làm lệch chữ.
- Ô mật khẩu chỉ được nhận ra khi là ô Edit kiểu cũ của Windows; ô mật khẩu trên trình duyệt được
  xử lý như ô thường.
- Khi đang dùng bộ gõ tiếng Trung, Nhật hoặc Hàn của Windows, Raumakey để mọi phím đi thẳng.
- Nếu Windows gỡ hook (ví dụ khi máy quá tải), Raumakey tự cài lại trong vòng khoảng 2 giây.
- Chưa có bảng cài đặt; các tuỳ chọn khác ngoài menu phải sửa trong `raumakey.toml`.
- Chưa có giao diện quản lý macro: bạn tự viết `raumakey-macros.toml`.
- Lệnh `migrate` (nhập cấu hình và macro từ EVKey) chưa có.
- Gói chưa được ký số nên SmartScreen hoặc phần mềm diệt virus có thể cảnh báo, nhất là vì
  Raumakey dùng hook bàn phím toàn cục.

Kế hoạch §16 ghi trạng thái của bản portable; §14 ghi phần còn lại của dự án.

## 8. Xử lý sự cố

| Hiện tượng | Cách xử lý |
|---|---|
| Không thấy biểu tượng V/E | Mở mũi tên ^ ở khay hệ thống; trong **Settings > Personalization > Taskbar > Other system tray icons** bật Raumakey để luôn hiện. |
| Gõ ra chữ bị lặp hoặc lộn xộn | Một bộ gõ khác (UniKey, EVKey, bàn phím tiếng Việt của Windows) đang chạy cùng; tắt nó đi. |
| Ctrl+Shift chuyển bàn phím Windows thay vì đổi V/E | Windows đang dùng Ctrl+Shift để đổi layout. Đổi ở **Settings > Time & language > Typing > Advanced keyboard settings > Input language hot keys**. |
| Không gõ được tiếng Việt trong một ứng dụng chạy quyền quản trị | Chạy `raumakey.exe` bằng **Run as administrator**. |
| Cấu hình không được lưu cạnh exe | Thư mục không ghi được, hoặc nằm trong Program Files, thư mục Windows hay thư mục tạm; xem đường dẫn trong mục **Giới thiệu** (thường là `%LOCALAPPDATA%\Raumakey`). Muốn giữ cấu hình cạnh exe thì chép exe sang một thư mục bình thường, ví dụ `D:\Tools\Raumakey`. |
| Sửa `raumakey.toml` xong không thấy tác dụng | Kiểm tra lỗi cú pháp TOML; giá trị không hợp lệ được thay bằng mặc định. |
| Build lỗi `LNK1181: cannot open input file 'kernel32.lib'` | Thiếu Windows SDK (cách A) hoặc sai đường dẫn `-Lnative`/`LIB` (cách B). Kiểm tra lại tên thư mục phiên bản MSVC và SDK. |
| `cargo xtask package` báo thiếu artifact | Chạy `cargo xtask cross --release` trước. Cách A cần thêm thành phần MSVC ARM64. |
| `cargo xtask audit` báo không thấy `cargo-deny` | `cargo install cargo-deny --locked`. |

## 9. Tài liệu liên quan

- `README.md`: giới thiệu dự án.
- `docs/superpowers/specs/2026-10-16-raumakey-design.md`: thiết kế; §21 mô tả bản portable.
- `docs/superpowers/plans/2026-10-16-raumakey-implementation.md`: kế hoạch triển khai. §14 ghi
  trạng thái, §15 ghi các quyết định, §16 ghi bản portable.
- `docs/reference/evkey_compatibility_test_matrix.md`: ma trận kiểm thử tương thích.
- `THIRD_PARTY.md`: nguồn gốc mã và giấy phép bên thứ ba.
