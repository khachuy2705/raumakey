# EVKey64.exe — static reconstruction notes

## Scope and evidence policy

- Target: `/root/.hermes/cache/scratch/rea-extracted-5943e685-e4f4-4e5f-b140-1e9cad4c5211/x64/EVKey64.exe`
- SHA-256: `5490731bb563cd22d36f3816c4eef9ed9c3b487cdc6023330002a59858e2f5ad`
- MD5 (identity/reputation only): `b0a3ecbe1246a74b34a063f0fa05691a`
- Analysis was **static only**. The executable was not run and the submitted binary was not modified.
- REA was attempted with provider `ghidra`, but Ghidra was unavailable (`GHIDRA_INSTALL_DIR` unset; provider diagnostics also reported unsupported local Java 25). The fallback was radare2 (`aaa`, `pdc`, xrefs), GNU `objdump`, `rabin2`, `strings`, and read-only 7-Zip PE/resource extraction.
- Labels below use **Observed**, **Inferred**, and **Unknown** deliberately. Decompiled names such as `main_window_proc` are analyst labels, not original symbols.

## 1. Binary identity and recoverability

**Observed**

- PE32+ Windows GUI executable, AMD64, image base `0x140000000`, PE entry RVA `0x48140` / VA `0x140048140`.
- File size: 810,352 bytes. Six sections: `.text`, `.rdata`, `.data`, `.pdata`, `.rsrc`, `.reloc`.
- Linker 14.16; PE timestamp `2026-06-17 21:06:39`; version resource `6.0.5.0`; product `EVKey - Vietnamese Keyboard`.
- Hardening flags: ASLR (`DYNAMIC_BASE`, `HIGH_ENTROPY_VA`), NX, large-address aware, terminal-server aware, stack canary detected. Authenticode certificate table exists (1,392 bytes), but signer/trust was not validated.
- Manifest requests `asInvoker`, `uiAccess=false` (manifest string at `0x1400d204b`).
- Inventory: 366 imports, 1,639 analyzed procedures, and 15,010 provider-defined strings.
- RTTI exposes controller/UI class names: `CMainWindow`, `CMainWnd`, `CDlgConfig`, `CTabSetting`, `CEVMenu`, `CDlgConverter`, `CDlgMacro`, `CDlgSpecialCase`, `CDlgCustomizeIcon`, `CDlgUserDefine`, `CAboutDlg`, `CEvClipboard`, `CIniFile`, and `CMutex` (`0x14009b440`–`0x14009b940`).

**Inferred**

- The program is readily recoverable at the subsystem level, but not at original-source fidelity. MFC/ATL-style object layouts, inlined STL operations, and stripped user function names make type and switch recovery imperfect.

## 2. Resource/UI reconstruction

**Observed**

- Resource inventory contains 10 dialog templates (IDs 111, 113, 115, 116, 117, 118, 119, 131, 141, 148), two menus (112/126), icons/cursor/font, two sounds, `LOGTEXT/134`, and `BIN/140`.
- Dialog 111 is the compact main panel and exposes `Keyboard hook`, `IME`, language selection, and the `evkeyvn.com` link.
- Dialog 113 is About EVKey.
- Dialog 115 is typing/macro behavior: spell checking, macros, free marks, clipboard key sending, auto-capitalization, and keyboard-layout handling.
- Dialog 116 is general/startup behavior: auto-run, run as administrator, IME for Metro/browsers, language, sounds, auto-update check, IME integration, tray customization, special cases, and colors.
- Dialog 117 is hotkey configuration.
- Dialog 118 is the charset/text/clipboard converter.
- Dialog 119 is macro definition; dialog 131 is user-defined input method; dialog 141 customizes tray colors; dialog 148 edits per-application special cases.
- Menu 112/126 exposes converter, clipboard conversion, macro table, spell check, macros, special cases, input methods, charsets, restart, restart-as-admin, configuration, About, and Exit.
- Main hidden/controller window class is `EVKey_MainWnd` (`0x140075b10`, `0x140075bd8`, `0x140075c38`). Its window procedure is recovered at `0x140033450`.

**Inferred**

- The UI architecture is a hidden top-level controller window plus modeless/modal MFC dialogs and a tray menu. The hidden controller owns the message loop, raw-input routing, shell notifications, timers, and dialog dispatch.

## 3. Entry, startup, and controller lifecycle

**Observed**

- The PE entry stub at `0x140048140` enters CRT initialization. The recovered CRT path calls the application controller `0x140034f90` at call site `0x1400480cd`.
- `0x140034f90`:
  - initializes settings and process state;
  - dynamically resolves `ChangeWindowMessageFilter` from `user32.dll` and permits message `0x402` (`0x1400350b7`–`0x1400350c1`);
  - registers `EVKey_MainWnd` (`RegisterClassExW`, `0x1400350f3`);
  - creates the hidden main window and initially calls `ShowWindow(..., 0)` (`0x14003523a`–`0x1400352a7`);
  - creates an event and worker thread;
  - registers `SHELLHOOK` via `RegisterWindowMessageW` and calls `RegisterShellHookWindow` (`0x1400353a6`–`0x1400353c3`);
  - initializes tray state, IME state, macro path `\evkmacro.txt`, raw input, and the standard `GetMessageW`/`TranslateMessage`/`DispatchMessageW` loop;
  - on teardown, unloads helpers and cleans up IME IPC.
- Main window procedure `0x140033450` handles standard messages and internal/custom commands. It registers `TaskbarCreated`, sets five timer IDs `0x4EBC`–`0x4EC0` with 200/400/600/800 ms staggered intervals, routes `WM_INPUT`, dispatches tray/menu commands, and posts quit on shutdown.
- `TaskbarCreated` (`0x140075b90`) is used to restore the tray icon after Explorer restarts. `ImmersiveColorSet` (`0x140075bb0`) is handled for theme changes.
- One explicit controller post is `PostMessageW(hwnd, WM_COMMAND=0x111, 0xC352, 0)` at `0x140035435`–`0x14003544a`.

**Unknown**

- A complete symbolic mapping of every private command ID and timer branch was not recovered; resource command IDs and the decompiler’s large switch require manual structure/type recovery for exact names.

## 4. Configuration and persistence

**Observed**

- Portable/file settings references:
  - `\setting.ini` at `0x14006c830`;
  - `setting.tree` at/near `0x14006c854`;
  - `\except_app.dat` at `0x14006cc78`;
  - `\evkmacro.txt` at `0x140075b30`.
- Registry settings root: `Software\MyQ\EVKey` at `0x14006cc18` and `0x14006cc50`.
- Settings loader/migrator `0x14000b310` opens/query-values under this root, loops over a table of setting objects, and can delete the old key with `SHDeleteKeyW` (`0x14000b495`–`0x14000b4b9`).
- Tree-style groups/fields observed in data and the parser include: `setting`, `blacklist`, `on_vn`, `off_vn`, `user_method_input`, `to_cs`, `special_cases`, and `special_case_icons` (the first character of a few strings is missed by the string scanner, but adjacent UTF-16 data confirms the names).
- Scalar setting names at `0x14006c890`–`0x14006cbc8` include:
  - UI/input: `SGui`, `Type`, `Codec`, `SCodec`, `SInput`, `FMark`, `SMetro`, `SBrowser`, `SModern`, `UseIME`, `InputMech`, `UseSpCase`;
  - startup/admin/hotkeys: `RAdmin`, `SSKey`, `RKey`, `FKey`, `AutoStart`;
  - correction/macro: `SCheck`, `AResKey`, `DisFOK`, `CSuggestion`, `CSuggestion2`, `EMacro`, `AMarcro`, `AUpper`, `CUnicode`;
  - conversion: `Lang`, `ICCharset`, `OCCharset`, `CConvert`, `ToLower`, `ToUpper`, `ToSenCase`, `ToCapEachWord`, `RemoveTone`;
  - behavior/UI: `NSound`, `AUpdate`, `GMode`, `SConso`, `CTIcon`, `EColor`, `EBColor`, `VColor`, `VBColor`, `NPCAM`, `DefSound`, `ThemeColor`.
- `0x14000bcc0` is a large serialization/update routine referenced by settings code; it accesses the table of these values. `0x14000b310` also parses `blacklist`, `on_vn`, `off_vn`, and special-case records.

**Inferred**

- EVKey supports both registry and file/tree persistence, likely with migration/fallback depending on writeability/portable placement. The UI error “Cannot save settings... move EVKey to another directory” supports file-based persistence beside the executable.

**Unknown**

- Exact precedence between registry, `setting.ini`, and `setting.tree`, and defaults for every scalar, were not completely reconstructed.

## 5. Startup mechanisms and elevation

**Observed**

- Legacy/current-run registry path `Software\Microsoft\Windows\CurrentVersion\Run` is referenced at `0x140076490`/`0x140076500`.
- Function `0x140036600` opens/creates that HKCU key, deletes value `EVKey`, queries it, and closes it (`0x140036600`–`0x140036691`). No `RegSetValueExW` import exists.
- A separate autostart routine recovered around `0x140015880` builds a `.lnk`, uses COM shell-link interfaces, sets description `EVKey - Vietnamese Keyboard`, and saves/deletes the link.
- Elevated startup is implemented through Task Scheduler:
  - embedded resource `BIN/140` is UTF-16 task XML;
  - task URI/name: `\EVKey - Vietnamese Keyboard`;
  - logon trigger enabled;
  - `RunLevel=HighestAvailable`;
  - command placeholder `[EVKey_DIR]` and arguments `-ft`;
  - multiple instances policy `IgnoreNew`.
- `0x140036990` extracts `BIN/140`, substitutes the executable path, writes `%TEMP%\evktask.xml` as UTF-16LE, then invokes elevated `SCHTASKS` with `runas` and `/Create /XML ... /TN "EVKey - Vietnamese Keyboard" /F` (`0x140036e26`–`0x1400372fa`). Its opposite branch invokes elevated `SCHTASKS /Delete /TN "EVKey - Vietnamese Keyboard" /F` (`0x140037318`–`0x140037525`).
- Controller `0x140036850` selects among ordinary startup, scheduled-task startup, and cleanup according to settings/admin state.

**Inferred**

- Normal non-elevated auto-start uses a Startup-folder shortcut; “run as administrator on startup” uses the highest-available scheduled task. The Run-key routine appears to be compatibility/migration cleanup rather than the active writer.

## 6. Tray and window messages

**Observed**

- Tray operations call `Shell_NotifyIconW` in `0x140016020` and `0x140016ec0`.
- `0x140016ec0` builds/updates `NOTIFYICONDATAW`, switches icons/tooltips according to Vietnamese on/off state, attempts modify first, and falls back through add/delete style operations. English tooltips include:
  - `EVKey - Click to turn off Vietnamese mode`;
  - `EVKey - Click to turn on Vietnamese mode`.
- Shell integration strings: `SHELLHOOK`, `TaskbarCreated`, `Shell_TrayWnd`, `TrayNotifyWnd`, and `Notification Area`.
- No fixed private numeric value for `SHELLHOOK`/`TaskbarCreated` exists; both are registered messages resolved at runtime.

**Inferred**

- The tray icon is the primary always-visible controller surface; the main window stays hidden until a tray/menu/hotkey command requests configuration or a tool dialog.

## 7. Keyboard hook and hotkeys

**Observed**

- `0x1400340b0` installs `SetWindowsHookExW(WH_KEYBOARD_LL=13, callback=0x140031d70, hInstance, threadId=0)`.
- The low-level hook callback is `0x140031d70`; it reads modifier/key state, routes EVKey commands, can reinstall/reset the hook, and calls `CallNextHookEx` on pass-through paths.
- No `RegisterHotKey` import exists. Hotkeys are recognized inside the low-level keyboard hook/raw-input/message controller rather than through system hotkey registration.
- Dialog 117 exposes modifiers `Ctrl`, `Shift`, `Alt`, `Win` and commands:
  - toggle mechanism;
  - switch English/Vietnamese;
  - F1 On VN, F2 Off VN, F3 Unicode, F5 Configuration, F6 Converter, F7 Convert Clipboard, F8 Macro Table, F9 On/Off Macro, F11 On/Off special cases/blacklist app, F12/Space Reset EVKey.
- Help text gives hard reset combinations `Ctrl+Shift+Alt+F12` or `Ctrl+Shift+Alt+Space` (`0x1400713a0`) and charset shortcut `Ctrl+Alt+Shift+F4` (`0x140072560`).

**Unknown**

- Exact default modifier masks for every configurable command are not established solely from the static UI/resource text.

## 8. Update subsystem

**Observed**

- Auto-update setting key: `AUpdate` (`0x14006caf0`); UI offers “Auto check for update at boot time” and manual “Check for update”.
- `0x140017480` builds sibling path `\evkau.exe`, validates/queries it, allocates a two-byte worker argument, and starts update worker thread `0x1400178e0`.
- `0x1400178e0` checks the updater’s version/result; it compares against `0x190` (400) and displays an “Auto Update is old” message when unsuitable. It then launches the updater via `ShellExecuteW`.
- The argument builder visibly emits ` -p`, ` -s`, ` -f64`, and ` -v` fragments (`0x14006d748`, `0x14006d750`, `0x14006d758`, `0x14006d778`), controlled by caller flags/version state.
- URLs present in this EXE:
  - `https://evkeyvn.com` (`0x14006e810`);
  - feedback form at `https://docs.google.com/forms/d/e/1FAIpQLSd3X5CjDNFBK9M7GkmByLqbXECJc5FSJmEAdafghe8qaZZNRg/viewform` (`0x14006e850`);
  - `https://www.facebook.com/bogotiengvietevkey` (`0x14006e930`).
- There are no WinINet, WinHTTP, URLMon, Winsock, or other networking DLL imports in this EXE. The analyzed extraction directory does not contain `evkau.exe`.

**Inferred**

- Network update discovery/download is delegated to the external `evkau.exe`; EVKey64.exe only checks/launches it and handles UI/settings.

**Unknown**

- Actual update API/download URL, transport, signature verification, and package format are not recoverable from this EXE alone because `evkau.exe` is absent.

## 9. IME DLL integration and IPC

**Observed**

- Companion strings: `\evkime64.dll` (`0x140075d30`, `0x1400761f8`) and `vkime32.dll` (`0x14007621c`; spelling observed exactly). The extracted package contains `evkime64.dll` and `evkime32.dll`, but those DLLs were outside this EXE-only pass.
- IME COM registration path: `Software\Classes\CLSID\{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}\InprocServer32` (`0x140076240`). Function `0x140035ff0` queries this path under HKLM and handles 32-/64-bit registry views (`KEY_WOW64_64KEY=0x100`, `KEY_WOW64_32KEY=0x200`).
- Registration helpers reference `%SystemRoot%\System32\regsvr32.exe`, `%SystemRoot%\SysWOW64\regsvr32.exe`, `/s`, `/s /u`, and `runas` (`0x140076380`–`0x140076400`).
- Main initialization loads `input.dll`, resolves `InstallLayoutOrTip`, and calls it with profile string `%04X:{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}{F74F81AF-6F78-42C7-B006-2630C950B9A2}` (`0x14003556e`–`0x1400355b7`).
- IPC setup `0x140035850` creates:
  - named file mapping `EVKeyIME_SharedMem_v1`, size `0x1038` bytes;
  - mapped view with all-access request;
  - auto-reset events `EVKeyIME_Request_v1` and `EVKeyIME_Response_v1`;
  - a worker thread at `0x140035ab0`.
- The shared block is initialized and receives controller/input state fields at offsets including `+0x20`, `+0x24`, `+0x28`, `+0x2c`, `+0x30`. Shutdown `0x1400357a0` clears the run flag, signals/waits up to 2,000 ms, unmaps, closes handles, and deletes the critical section.
- Command-line strings include `-ineime` and `-r -ineime`, indicating internal IME registration/restart modes.

**Inferred**

- The system-registered IME DLL is loaded by Windows text services/input infrastructure, while EVKey64.exe exchanges state/requests through named shared memory plus request/response events. This is IPC, not classic process injection.

**Observed negative boundary**

- EVKey64.exe imports no `WriteProcessMemory`, `VirtualAllocEx`, `CreateRemoteThread`, `NtMapViewOfSection`, or equivalent classic remote-injection primitives. Its `LoadLibraryW` calls are local (`user32.dll`, `input.dll`, UI/theme helpers). `SetWindowsHookExW` installs a low-level callback located in the EXE itself (`0x140031d70`).

**Unknown**

- Exact `0x1038` shared-memory struct layout and request opcodes require a companion DLL pass and/or manual data-flow reconstruction.

## 10. Clipboard and input behavior

**Observed**

- `CEvClipboard` RTTI is at `0x14009b440`.
- Clipboard controller `0x14002b4e0` opens the clipboard, reads registered/standard formats, converts text/RTF-like content, writes results, and reports failures/success to the converter UI.
- Helpers `0x1400045e0`, `0x140004b20`, and `0x140004c50` retry `OpenClipboard`/`EmptyClipboard`/`CloseClipboard`, use `GlobalAlloc/Lock/Unlock`, and call `GetClipboardData`/`SetClipboardData`.
- Settings/UI expose “Use Clipboard for send key” and F7 clipboard conversion.
- Main window procedure calls clipboard conversion at `0x14003388e` and `0x140033d6b`.
- Raw input is registered twice in startup (`RegisterRawInputDevices` at `0x1400355e0` and `0x1400355f0`) and consumed with `GetRawInputData` in `0x140033450`.

**Inferred**

- Clipboard mode is a compatibility fallback for applications where synthetic key delivery is unreliable, separate from the explicit converter tool.

## 11. Process/application compatibility and exclusion rules

**Observed**

- Configuration parser explicitly handles a `blacklist` collection and per-app `on_vn`, `off_vn`, `user_method_input`, `to_cs`, and `special_cases` records (`0x14000b5fc`–`0x14000b6f4`). Dialog 148 edits application, Vietnamese state, charset, input method, and mechanism.
- Static pointer tables in `.data` at `0x1400a4460`–`0x1400a46c8` reference these process/title groups:
  - Browser set A (16): `chrome.exe`, `epic.exe`, `sfive.exe`, `slimjet.exe`, `browser.exe`, `safari.exe`, `opera.exe`, `iexplore.exe`, `microsoftedge.exe`, `firefox.exe`, `waterfox.exe`, `vivaldi.exe`, `kinza.exe`, `brave.exe`, `whale.exe`, `msedge.exe`.
  - Short special set: `excel.exe`, `et.exe`, `terminal.exe`.
  - Desktop/editor set: `emeditor.exe`, `afterfx.exe`, `mindmanager.exe`, `coreldrw.exe`, `firefox.exe`.
  - Browser set B adds `excel.exe` to the 16-browser family.
  - Messenger set: `telegram.exe`, `viber.exe`.
  - Browser set C (13): `slimjet.exe`, `chrome.exe`, `epic.exe`, `sfive.exe`, `browser.exe`, `brave.exe`, `msedge.exe`, `avastbrowser.exe`, `onlynumercs.vshost.exe`, `firefox.exe`, `iexplore.exe`, `waterfox.exe`, `microsoftedge.exe`.
  - Creative/special set: `audition.exe`, `crossfire.dat`, `photoshop.exe`.
  - Game set: `league of legends.exe`, `dota2.exe`, `tslgame.exe`, `tslgame_be.exe`, `tslgame_eac.exe`, `execbubg.exe`, `fxgame.exe`, `csgo.exe`, `hl2.exe`.
  - Legacy/special set: `audition.exe`, `crossfire.dat`, `acad.exe`, `winuapentry.exe`, `hh.exe`, `windowsterminal.exe`.
  - Window-title suffixes: `- google sheets`, `- microsoft excel`, `- google trang tính` (last Vietnamese string is partially decoded by the generic scanner).
- Separate foreground/immersive checks reference `steam.exe`, `steamwebhelper.exe`, `msedgewebview2.exe`, and `applicationframehost.exe` (`0x14006d9d8`–`0x14006da40`).
- Process enumeration imports include `CreateToolhelp32Snapshot`, `Process32FirstW`, `Process32NextW`, `QueryFullProcessImageNameW`, and `GetProcessImageFileNameW`.

**Inferred**

- These are built-in compatibility/default-special-case tables, not one flat denylist. The game set is the strongest candidate for default hook/typing exclusion; browser/editor/messenger groups likely select IME, clipboard, or key-sending workarounds.

**Unknown**

- The exact semantic action attached to each table is not proven by string adjacency alone. A faithful reconstruction should preserve them as separate named compatibility sets until every lookup call site is typed; it should not merge all names into a single blacklist.

## 12. Mutex and single-instance behavior

**Observed**

- `CreateMutexW` is called only at `0x1400356b1` inside `0x140035630` (`CMutex` construction).
- Arguments at the call are `lpMutexAttributes=NULL`, `bInitialOwner=FALSE`, `lpName=NULL`; therefore this is an **unnamed mutex**.
- No named mutex string or second `CreateMutexW` call was found.

**Inferred**

- The recovered mutex is an internal synchronization wrapper, not by itself a cross-process single-instance key. Single-instance behavior may instead rely on `EVKey_MainWnd`, scheduled-task policy, or other controller discovery logic.

**Unknown**

- A definitive single-instance arbitration path was not isolated in this pass.

## 13. Reconstruction-oriented subsystem map

| Analyst label | Address | Role | Confidence |
|---|---:|---|---|
| CRT/PE entry | `0x140048140` | entry stub/CRT | Observed |
| app main/controller | `0x140034f90` | hidden window, initialization, message loop | Observed |
| main window procedure | `0x140033450` | messages, timers, tray/menu/raw input | Observed |
| low-level keyboard hook | `0x140031d70` | key/modifier and EVKey hotkey routing | Observed |
| hook installer | `0x1400340b0` | `WH_KEYBOARD_LL` installation | Observed |
| tray controller | `0x140016ec0` | add/modify/delete icon and tooltip | Observed |
| config registry/tree loader | `0x14000b310` | scalar settings, blacklist/special cases | Observed |
| config serializer | `0x14000bcc0` | large settings read/write/update routine | Inferred from call/data use |
| update dispatcher | `0x140017480` | locate/check updater, spawn worker | Observed |
| update worker | `0x1400178e0` | validate/launch `evkau.exe` | Observed |
| startup mode controller | `0x140036850` | shortcut/task/cleanup selection | Observed + inferred branch labels |
| scheduled-task manager | `0x140036990` | extract XML; `SCHTASKS` create/delete | Observed |
| legacy Run-key helper | `0x140036600` | delete/query `HKCU\...\Run\EVKey` | Observed |
| IME registration query | `0x140035ff0` | CLSID `InprocServer32`, registry views | Observed |
| IME IPC setup | `0x140035850` | shared mapping/events/thread | Observed |
| IME IPC cleanup | `0x1400357a0` | stop/wait/unmap/close | Observed |
| clipboard controller | `0x14002b4e0` | clipboard conversion/format handling | Observed |

## 14. Limitations and follow-up boundaries

- REA/Ghidra was unavailable, so the pseudocode artifact is radare2 `pdc`; some function boundaries and large switch structures are visibly imperfect.
- Static analysis did not observe actual runtime state, registry contents, task creation, tray interaction, network traffic, or DLL loading.
- The supplied UniKey source tree was not used as evidence of equivalence. No EVKey behavior above is claimed merely because UniKey implements something similar.
- The companion `evkime64.dll`/`evkime32.dll` should be analyzed separately to recover the complete shared-memory schema, request opcodes, TSF/IME entry points, and system-side text injection behavior.
- `evkau.exe` is absent, so the update protocol and real network endpoint remain unknown.
- Exact command-ID names, default hotkey masks, every settings default, and the action attached to each process compatibility table remain partial.

## Artifacts

- Selected static pseudocode export: `/root/evkey_reconstruction/artifacts/evkey64_pseudocode.md`
- This report: `/root/evkey_reconstruction/research/evkey64_exe.md`
