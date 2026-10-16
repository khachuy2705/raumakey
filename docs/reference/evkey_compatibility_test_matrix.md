# Ma trận kiểm thử tương thích EVKey clean-room

## Quy ước

- `Tx(d,"s",c,r,x)` = `EditTransaction{delete_before:d, insert_utf16:"s", consume_original:c, replay_key:r, reset_context:x}`.
- Mỗi bước áp transaction lên text/caret hiện tại. Chuỗi input viết theo key vật lý/character tuần tự.
- **Observed integration**: transport/protocol được thấy trong EVKey binary; không đồng nghĩa output ngôn ngữ đã thấy.
- **Baseline oracle**: kỳ vọng từ source UniKey được trích dẫn; phải xác nhận riêng với EVKey runtime hợp pháp trước khi gọi là tương thích EVKey.
- **Proposed oracle**: quyết định spec/an toàn mới.

Các mapping Telex/VNI/VIQR baseline: `unikey/newkey/encode.cpp:453-489,504-565,618-635` và `x-unikey/src/ukengine/inputproc.cpp:100-162`, tổng hợp tại [`research/unikey_baseline.md:221-244`](research/unikey_baseline.md#L221-L244). Contract EVKey quan sát chỉ là char/backspace → backspaces + UTF-16 replacement qua TSF ([`research/evkime_dlls.md:134-168`](research/evkime_dlls.md#L134-L168)).

## A. Engine và phương thức gõ

| # | Chuỗi input | Options/context | Transaction/oracle cuối | Mức bằng chứng |
|---:|---|---|---|---|
| 1 | `a a` | Telex, VN on | bước 2: `Tx(1,"â",true,None,false)`; `â` | Baseline oracle: `unikey/newkey/encode.cpp:504-565`; [`research/unikey_baseline.md:225-230`](research/unikey_baseline.md#L225-L230) |
| 2 | `d d` | Telex | `Tx(1,"đ",true,None,false)`; `đ` | Baseline oracle: cùng mapping |
| 3 | `a s` | Telex | `Tx(1,"á",true,None,false)`; `á` | Baseline oracle: tone handler `unikey/keyhook/vietkey.cpp:690-768`; [`research/unikey_baseline.md:206-213`](research/unikey_baseline.md#L206-L213) |
| 4 | `a 6` | VNI | `Tx(1,"â",true,None,false)`; `â` | Baseline oracle: `encode.cpp:618-635` |
| 5 | `a 1` | VNI | `Tx(1,"á",true,None,false)`; `á` | Baseline oracle: `encode.cpp:481-483` |
| 6 | `a ^` | VIQR | `Tx(1,"â",true,None,false)`; `â` | Baseline oracle: `encode.cpp:475-489,620-635` |
| 7 | `a '` | VIQR | `Tx(1,"á",true,None,false)`; `á` | Baseline oracle: cùng nguồn |
| 8 | `\`+`'` — một backslash rồi apostrophe **liền nhau** (không có space giữa hai phím) | VIQR | escape tiêu thụ apostrophe ngay sau `\` → final text là `'` literal, không dấu; đúng 2 phím, không xen space | Baseline oracle: `unikey/keyhook/vietkey.cpp:415-443`; [`research/unikey_baseline.md:230`](research/unikey_baseline.md#L225-L230) |
| 9 | action không hợp lệ trên ký tự không phải mục tiêu | mọi method | không mất key; pass-through hoặc `insert_utf16` literal | Baseline oracle: `x-unikey/.../ukengine.cpp:492-550,961-1017`; [`research/unikey_baseline.md:451-466`](research/unikey_baseline.md#L451-L466) |
| 10 | `q u a s` | Telex | final `quá`; tone không gắn sai vào `u` onset | Baseline oracle (`qu` special): `unikey/keyhook/vietkey.cpp:709-743`; [`research/unikey_baseline.md:206-213`](research/unikey_baseline.md#L206-L213) |
| 11 | `g i a s` | Telex | final `giá`; xử lý `gi` onset đặc biệt | Baseline oracle: cùng nguồn |
| 12 | vowel + tone + thêm coda | modern_tone mỗi giá trị | tone được reposition theo style và coda; expected string lấy từ bảng golden baseline của implementation | Baseline oracle, không EVKey-observed: `x-unikey/.../ukengine.cpp:936-959,1505-1684`; [`research/unikey_baseline.md:270-287`](research/unikey_baseline.md#L270-L287) |
| 13 | `a s s` | Telex | action tone lặp hoàn nguyên dấu và giữ action literal theo baseline; exact final phải được golden-engine xác nhận | Baseline oracle: `vietkey.cpp:745-768`; không ghi thành EVKey expected |
| 14 | `a a a` | Telex | decoration lặp hoàn nguyên + literal action; exact final là golden baseline | Baseline oracle: `vietkey.cpp:555-634`; [`research/unikey_baseline.md:214-219`](research/unikey_baseline.md#L214-L219) |
| 15 | chuỗi đã conversion nhưng invalid + space | `spell_check=true`, `auto_restore_invalid=true` | replacement raw keystrokes rồi append space | Baseline oracle: `x-unikey/.../ukengine.cpp:2183-2233,2270-2360`; [`research/unikey_baseline.md:290-301`](research/unikey_baseline.md#L290-L301) |
| 16 | cùng #15 | `auto_restore_invalid=false` | không restore; commit rendered word + space | Baseline oracle/proposed option contract |
| 17 | `a a Backspace` | Telex | semantic backspace khôi phục state trước; expected final `a` | Baseline oracle: `ukengine.cpp:1921-1977`; EVKey op 2 observed tại [`research/evkime_dlls.md:121-128`](research/evkime_dlls.md#L121-L128), x64 `0x180003c20` |
| 18 | 33 lần transform rồi Backspace | TSF history capacity boundary | không OOB; entry cũ nhất evict có xác định; fallback op 2 khi local history không có | Observed integration (decompiler-derived): history tối đa **≤32**, per-entry stride **biểu kiến** `0x208` — con số này không phải public ABI ([`research/evkime_dlls.md:155-162`](research/evkime_dlls.md#L155-L162)); "no OOB" và quy tắc evict là **Proposed/Inferred**, exact text Proposed |
| 19 | click trái/phải giữa composition rồi action tone | TSF | transaction reset/end composition; action không sửa text trước click | Observed integration: [`research/evkime_dlls.md:52-79`](research/evkime_dlls.md#L52-L79), x64 `0x180001bf0` |
| 20 | focus/document đổi giữa composition | TSF | end/reset state; không cross-document delete | Observed integration: [`research/evkime_dlls.md:155-162`](research/evkime_dlls.md#L155-L162) |

## B. Modifier, mode, hotkey và host policy

| # | Input | Options/context | Kỳ vọng | Mức bằng chứng |
|---:|---|---|---|---|
| 21 | Ctrl+C / Alt+menu | VN on | bypass engine, không consume; reset theo policy | Baseline + observed classifier: [`research/evkime_dlls.md:145-153`](research/evkime_dlls.md#L145-L153), x64 `0x180003c20` |
| 22 | Ctrl+Alt tạo ký tự trên layout AltGr | `alt_gr=true` | không coi như shortcut conflict; character được chuyển đúng một lần | Baseline oracle: `unikey/keyhook/keyhook.cpp:658-677`; [`research/unikey_baseline.md:131-146`](research/unikey_baseline.md#L131-L146) |
| 23 | Shift/Caps với chữ hoa | Telex | case ký tự Việt giữ đúng; Shift flag và Caps truyền riêng | IPC fields observed: [`research/evkime_dlls.md:102-119`](research/evkime_dlls.md#L102-L119); output case baseline/proposed |
| 24 | toggle English/Vietnamese | composition active | reset trước đổi mode; tray/config hội tụ | UI/hotkey observed: [`research/evkey64_exe.md:118-148`](research/evkey64_exe.md#L118-L148); exact toggle default Unknown |
| 25 | hard reset hotkey | `Ctrl+Shift+Alt+F12` và `...+Space` | engine/context reset; text đã commit không đổi | Observed: help text/UI liệt kê hai combo hard reset ([`research/evkey64_exe.md:133-145`](research/evkey64_exe.md#L133-L145), `0x1400713a0`). "Engine/context reset" và "text đã commit không đổi" là **Inferred/Proposed**, chưa quan sát runtime |
| 26 | host thuộc 9-app TIP list | mỗi tên Edge/Chromium/Teams | policy nhánh special được chọn, text result vẫn bằng generic oracle | Observed list/cache: [`research/evkime_dlls.md:189-217`](research/evkime_dlls.md#L189-L217); semantics nhánh Inferred |
| 27 | host chưa cache, probe thành công | generic app | persist DWORD 1 dưới `HKCU\Software\EVKey\TsfCache\v<version>\<exe>` | Observed integration: cùng nguồn |
| 28 | host game/built-in set | controller policy | không giả định blacklist; verify từng named set giữ riêng và action do config quyết định | Observed names, semantics Unknown: [`research/evkey64_exe.md:215-240`](research/evkey64_exe.md#L215-L240) |
| 29 | secure/password field | policy Restricted | bypass transform/macro/clipboard, không lưu raw key | Proposed security oracle |

## C. IPC, TSF và parity

| # | Kịch bản | Thiết lập | Kỳ vọng | Mức bằng chứng |
|---:|---|---|---|---|
| 30 | op 1 success | sequence N→N+1, response matching | đọc `+0x18`, length chẵn `+0x1c`, UTF-16 tại `+0x38`; apply một lần | Observed: [`research/evkime_dlls.md:81-130`](research/evkime_dlls.md#L81-L130), x64 `0x180005470/0x180005550` |
| 31 | mutex không lấy được 100 ms | legacy IPC | không ghi/apply output; pass-through/reset an toàn | Timeout observed; fallback proposed, cùng nguồn |
| 32 | response quá 50 ms | legacy IPC | bỏ response; không double-apply nếu signal đến muộn | Timeout/sequence observed; error handling proposed |
| 33 | stale response event trước request | signal sẵn | adapter drain signal, rồi đợi response sequence mới | Observed: [`research/evkime_dlls.md:92-100`](research/evkime_dlls.md#L92-L100) |
| 34 | responseSeq != requestSeq | op bất kỳ | reject response, không sửa text | Observed contract, cùng nguồn |
| 35 | output length lẻ / >`0x1000` / có bit cao set | malicious/corrupt peer | reject, reset bridge, không OOB; không diễn giải trường 4 byte theo signedness (dùng raw u32 + range check) | Proposed security oracle dựa schema observed; signedness Unknown |
| 36 | timestamp cũ >2.000 ms | bridge state | stale; reset/reconnect, không apply | Comparison observed: [`research/evkime_dlls.md:112-119`](research/evkime_dlls.md#L112-L119); reaction proposed |
| 37 | op 3 | reset gate `+0x24` on/off | on: request reset; off: chỉ reset local; không diễn giải cờ khác | Observed behavior: [`research/evkime_dlls.md:114-128`](research/evkime_dlls.md#L114-L128) |
| 38 | op 5 sau connect | named objects đủ | sync thành công trước xử lý char | Observed: cùng nguồn, x64 `0x1800052c0` |
| 39 | replay key | transaction có `replay_key` | `SendInput` down/up với `dwExtraInfo=0xEBCA1234`; callback bypass | Observed: [`research/evkime_dlls.md:145-166`](research/evkime_dlls.md#L145-L166), x64 `0x180002a70` |
| 40 | TSF replacement | `delete_before=2`, insert `"ế"` | range lùi 2 UTF-16 units, SetText, caret collapse cuối; một edit session | Observed mechanism: [`research/evkime_dlls.md:145-166`](research/evkime_dlls.md#L145-L166), x64 `0x180002f00`; concrete content proposed |
| 41 | apply failure/context mismatch | range ngắn hơn delete | không commit EngineState; reset/rollback | Proposed atomicity oracle |
| 42 | cùng vector op 1/2/3/5 | x86, x64, ARM64 adapter | bytes offsets/names/timeouts/sentinel và semantic result đồng nhất | Observed parity: [`research/evkime_dlls.md:271-282`](research/evkime_dlls.md#L271-L282) |
| 43 | DLL x86 ở hai package paths | hash fixture | cả hai SHA-256 bằng `91b978...d53d`; chạy một contract suite, ghi alias | Observed identity: [`research/pe_inventory.md:23-31`](research/pe_inventory.md#L23-L31) |

## D. Config, macro, Unicode, lifecycle

| # | Kịch bản | Thiết lập | Kỳ vọng | Mức bằng chứng |
|---:|---|---|---|---|
| 44 | migrate Registry + file cùng tồn tại | fixture version cũ | precedence do schema công bố; field unknown bảo toàn; nguồn không bị xóa trước verify | Persistence sources observed, migration behavior proposed: [`research/evkey64_exe.md:69-95`](research/evkey64_exe.md#L69-L95) |
| 45 | corrupt một field | config version hỗ trợ | chỉ field lỗi default/quarantine; report lỗi, không silent reset toàn bộ | Proposed; cố ý không chép all-or-default của UniKey cũ ([`research/unikey_baseline.md:347-357`](research/unikey_baseline.md#L347-L357)) |
| 46 | concurrent config commit | same expected revision | đúng một commit thành công; bên kia nhận `Conflict` | Proposed concurrency oracle |
| 47 | macro trigger + space | macro enabled, exact word boundary | delete trigger, insert replacement rồi delimiter | Baseline oracle: `unikey/keyhook/vietkey.cpp:967-1039`; [`research/unikey_baseline.md:303-313`](research/unikey_baseline.md#L303-L313) |
| 48 | trigger nằm giữa word | macro enabled | không expand (chỉ expand tại word boundary) | Baseline oracle: `unikey/keyhook/vietkey.cpp:967-1017` (macro chỉ chạy tại word stop/Enter, lookup theo trigger) + Proposed boundary oracle; EVKey exact Unknown ([`research/unikey_baseline.md:303-313`](research/unikey_baseline.md#L303-L313)) |
| 49 | Shift+Space/escape boundary | macro enabled | không expand hoặc commit literal theo policy công bố | Baseline oracle: cùng nguồn; EVKey exact Unknown |
| 50 | trigger viết HOA/Title/lower | `auto_case_macro=true` | replacement biến đổi case tương ứng | Baseline oracle: `x-unikey/.../ukengine.cpp:2049-2157`; [`research/unikey_baseline.md:315-321`](research/unikey_baseline.md#L315-L321) |
| 51 | VN off + always macro | `macros_when_disabled=true` | expansion vẫn hoạt động ở boundary | Baseline oracle: `unikey/keyhook/vietkey.cpp:146-161`; không EVKey-observed |
| 52 | trigger NFC, input NFD | normalization=NFC | lookup cùng entry; output well-formed NFC | Proposed Unicode oracle |
| 53 | output NFD | normalization=NFD | canonical-equivalent với NFC oracle; `delete_before` diễn giải theo **UTF-16 host code unit** (Proposed — suy ra từ `cch`/range của TSF), không glyph; bằng chứng wire chỉ nói "4 byte characters/backspaces", signedness Unknown | Proposed + baseline charset-aware principle: [`research/unikey_baseline.md:283-288`](research/unikey_baseline.md#L283-L288) |
| 54 | supplementary scalar quanh caret | emoji + Vietnamese word | không split surrogate; delete chỉ phạm vi engine sở hữu | Proposed security/correctness oracle |
| 55 | startup/shutdown | normal controller | mapping `0x1038`, request/response events và worker sẵn trước TIP requests; shutdown signal + wait tối đa 2.000 ms rồi close | Observed: [`research/evkey64_exe.md:172-186`](research/evkey64_exe.md#L172-L186), `0x140035850/0x1400357a0` |
| 56 | elevated autostart migration | RAdmin on/off | scheduled task highest-available chỉ khi explicit; normal path shortcut; Run key chỉ cleanup | Observed + inferred: [`research/evkey64_exe.md:97-116`](research/evkey64_exe.md#L97-L116), `0x140036850/0x140036990` |
| 57 | updater không có | update command | báo unavailable; controller không tự network/download | Observed boundary: [`research/evkey64_exe.md:150-170`](research/evkey64_exe.md#L150-L170), updater đúng tên `evkau.exe` |
| 58 | Explorer restart | `TaskbarCreated` | dựng lại tray icon, mode không đổi | Observed: `TaskbarCreated` được register/handle và dùng để restore tray icon ([`research/evkey64_exe.md:61-63`](research/evkey64_exe.md#L61-L63), `0x140033450`). "Mode không đổi" là **Inferred**, không suy ra được chỉ từ việc message được handle |

## Tiêu chí pass toàn bộ

- Mỗi test phải lưu: architecture, Windows build, host app/version, config revision, input events, transaction sequence và final text.
- Test “Baseline oracle” chỉ pass baseline; không đổi nhãn thành “EVKey observed” nếu chưa có phép đo EVKey độc lập.
- Parity phải so sánh transaction semantic và byte layout legacy, không so địa chỉ hàm.
- Fuzz thêm quanh UTF-16 malformed, length/offset, sequence wrap, timeout race, focus/mouse reset và concurrent config update.
