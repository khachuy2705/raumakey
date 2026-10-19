# EVKey PE static inventory

## Scope and safety

- Source tree: `/root/.hermes/cache/scratch/rea-extracted-5943e685-e4f4-4e5f-b140-1e9cad4c5211`
- Coverage: **7 PE files**; every regular file in the extracted tree was inventoried.
- Static analysis only. No sample was executed; Wine and `ldd` were not used.
- Tools/evidence: `file`, GNU `objdump` compatibility check, `rabin2` PE parser, Python standard-library PE header parsing, `sha256`/`md5`, and OpenSSL PKCS#7/X.509 parsing. MD5 is included only for reputation matching.
- Signature caveat: Authenticode certificate tables and X.509 chains were parsed, but Authenticode image-digest verification was unavailable (`osslsigncode`/`pesign` absent and `rabin2 -T` could not verify these blobs). “Present” therefore does **not** mean trusted or integrity-verified.

## Executive findings

- The bundle contains three GUI executables (x86, x64, ARM64) and four IME DLL copies: two x86 copies, one x64, one ARM64.
- `x64/evkime32.dll` and `x86/evkime32.dll` are byte-for-byte identical (same SHA-256); this is an intentional-looking 32-bit companion DLL shipped beside the 64-bit executable for cross-bitness support.
- The three EXEs report product/file version **6.0.5.0**; the DLLs have no VERSION resource. All seven carry a Visual C++/MSVC-style Rich header and have a PE compile timestamp on 2026-06-17 within 87 seconds of one another.
- All enable ASLR (`DYNAMIC_BASE`) and DEP (`NX_COMPAT`) and contain base relocations. Six of seven expose a load-config security cookie; `x64/evkime64.dll` does not. No file sets the PE `GUARD_CF` characteristic; the x86 builds contain `CF_INSTRUMENTED` GuardFlags but therefore are not reported as fully CFG-enabled. 64-bit/ARM64 files set `HIGH_ENTROPY_VA`.
- Every file has an Authenticode certificate table embedding the same self-issued leaf certificate `CN=LamQuangMinh`; OpenSSL trust validation fails because the certificate is self-signed. No countersignature/timestamp token was identified.
- No executable exports symbols. DLLs export the four standard COM in-process server entry points; architecture variants have matching export-name sets.
- Import capabilities fit a desktop input-method utility: global keyboard hooks and synthetic input (`SetWindowsHookExW`, `SendInput`), clipboard APIs, registry persistence/configuration, shared-memory mappings/events/mutexes, COM, and shell execution. Static imports alone do not establish malicious use.

## Identity and metadata

| File | Bytes | Machine | Kind | PE timestamp (UTC) | SHA-256 | MD5 |
|---|---:|---|---|---|---|---|
| `arm64/EVKeyArm64.exe` | 819056 | ARM64 | EXE | 2026-06-17T14:06:22Z | `7c2aa18379e673954cd04cdd2e6cc3f4c8c5f531a80666b20c20ed90772b4ba8` | `1ceb42f62718b034f22bb71db475d41e` |
| `arm64/evkimearm64.dll` | 214896 | ARM64 | DLL | 2026-06-17T14:06:22Z | `5f288a34a1e44fe8ea240b76e61c77d068a80c508a05e85cbfcf289652e693f9` | `36ae5909695697f767a8790f9a73bb24` |
| `x64/EVKey64.exe` | 810352 | x64 (AMD64) | EXE | 2026-06-17T14:06:39Z | `5490731bb563cd22d36f3816c4eef9ed9c3b487cdc6023330002a59858e2f5ad` | `b0a3ecbe1246a74b34a063f0fa05691a` |
| `x64/evkime32.dll` | 157040 | x86 (I386) | DLL | 2026-06-17T14:06:17Z | `91b978ab3c816f78125afae8d4fe2390d2fa1ce3d696d2df223cb6f6b193d53d` | `34d05675b75974e4b1fcc3d002a88eb9` |
| `x64/evkime64.dll` | 187248 | x64 (AMD64) | DLL | 2026-06-17T14:07:44Z | `1d53028833d6f44bb1dc47baf2ac7c5cea92aa9194a44d841fb26fcf25b9a65f` | `d7e9284c89dd189c6b806dfdc02de89c` |
| `x86/EVKey32.exe` | 708464 | x86 (I386) | EXE | 2026-06-17T14:06:18Z | `56e06c73aa35674de12f9d4a4dc2485717b57ddc332f8c878e7019c529923c8b` | `a309f463dd3c5b6d1b5ec80921941869` |
| `x86/evkime32.dll` | 157040 | x86 (I386) | DLL | 2026-06-17T14:06:17Z | `91b978ab3c816f78125afae8d4fe2390d2fa1ce3d696d2df223cb6f6b193d53d` | `34d05675b75974e4b1fcc3d002a88eb9` |

### Build and version evidence

| File | Linker | OS / subsystem minimum | Image version | Rich-header tool records | PDB path |
|---|---|---|---|---|---|
| `arm64/EVKeyArm64.exe` | 14.51 | 6.2 / 6.2 | 0.0 | Linker1400, Resource900, Cvtres1400, Utc1900_CPP, Import0, Implib1400, Utc1900_CVTCIL_C, Utc1900_C, Masm1400 | none |
| `arm64/evkimearm64.dll` | 14.51 | 6.2 / 6.2 | 0.0 | Linker1400, Cvtres1400, Export1400, Utc1900_LTCG_CPP, Import0, Implib1400, Utc1900_C, Utc1900_CPP, Masm1400 | none |
| `x64/EVKey64.exe` | 14.16 | 5.2 / 5.2 | 0.0 | Linker1400, Resource900, Cvtres1400, Utc1900_CPP, Import0, Implib900, Utc1500_CVTCIL_C, Utc1500_C, Masm1400, Utc1900_C, Utc1810_C, Utc1810_CPP, Masm1210 | none |
| `x64/evkime32.dll` | 14.16 | 5.1 / 5.1 | 0.0 | Linker1400, Cvtres1400, Export1400, Utc1900_LTCG_CPP, Import0, Implib900, Utc1500_C, Masm1400, Utc1900_C, Utc1900_CPP, Utc1810_C, Utc1810_CPP, Masm1210 | none |
| `x64/evkime64.dll` | 14.16 | 5.2 / 5.2 | 0.0 | Linker1400, Cvtres1400, Export1400, Utc1900_LTCG_CPP, Import0, Implib900, Utc1500_C, Masm1400, Utc1900_C, Utc1900_CPP, Utc1810_C, Utc1810_CPP, Masm1210 | none |
| `x86/EVKey32.exe` | 14.16 | 5.1 / 5.1 | 0.0 | Linker1400, Resource900, Cvtres1400, Utc1900_CPP, Import0, Implib900, Utc1500_C, Masm1400, Utc1900_C, Utc1810_C, Utc1810_CPP, Masm1210 | none |
| `x86/evkime32.dll` | 14.16 | 5.1 / 5.1 | 0.0 | Linker1400, Cvtres1400, Export1400, Utc1900_LTCG_CPP, Import0, Implib900, Utc1500_C, Masm1400, Utc1900_C, Utc1900_CPP, Utc1810_C, Utc1810_CPP, Masm1210 | none |

- `arm64/EVKeyArm64.exe`: FileVersion: 6.0.5.0; ProductVersion: 6.0.5.0; CompanyName: lamquangminh10@gmail.com; FileDescription: EVKey - Vietnamese Keyboard; InternalName: EVKey Vietnamese Keyboard; OriginalFilename: EVKey - Vietnamese Keyboard; ProductName: EVKey - Vietnamese Keyboard
- `arm64/evkimearm64.dll`: no VERSION resource parsed
- `x64/EVKey64.exe`: FileVersion: 6.0.5.0; ProductVersion: 6.0.5.0; CompanyName: lamquangminh10@gmail.com; FileDescription: EVKey - Vietnamese Keyboard; InternalName: EVKey Vietnamese Keyboard; OriginalFilename: EVKey - Vietnamese Keyboard; ProductName: EVKey - Vietnamese Keyboard
- `x64/evkime32.dll`: no VERSION resource parsed
- `x64/evkime64.dll`: no VERSION resource parsed
- `x86/EVKey32.exe`: FileVersion: 6.0.5.0; ProductVersion: 6.0.5.0; CompanyName: lamquangminh10@gmail.com; FileDescription: EVKey - Vietnamese Keyboard; InternalName: EVKey Vietnamese Keyboard; OriginalFilename: EVKey - Vietnamese Keyboard; ProductName: EVKey - Vietnamese Keyboard
- `x86/evkime32.dll`: no VERSION resource parsed

Compiler assessment: Rich-header records such as `Linker1400`, `CvtCIL...`, and `Utc...`, Microsoft calling convention metadata, and MSVC runtime imports support a Microsoft Visual C++ toolchain. Exact source compiler flags cannot be recovered from these headers alone.

## Hardening and PE layout

| File | DLL characteristics | ASLR + reloc | DEP | CFG | Stack cookie | Load-config GuardFlags | Overlay / certificate |
|---|---|---|---|---|---|---|---|
| `arm64/EVKeyArm64.exe` | `HIGH_ENTROPY_VA, DYNAMIC_BASE/ASLR, NX_COMPAT/DEP, TERMINAL_SERVER_AWARE` | yes (reloc dir 3968 B) | yes | no | yes | `0 (none)` | 1392 B / cert @817664 (1392 B) |
| `arm64/evkimearm64.dll` | `HIGH_ENTROPY_VA, DYNAMIC_BASE/ASLR, NX_COMPAT/DEP` | yes (reloc dir 1868 B) | yes | no | yes | `0 (none)` | 1392 B / cert @213504 (1392 B) |
| `x64/EVKey64.exe` | `HIGH_ENTROPY_VA, DYNAMIC_BASE/ASLR, NX_COMPAT/DEP, TERMINAL_SERVER_AWARE` | yes (reloc dir 3684 B) | yes | no | yes | `0 (none)` | 1392 B / cert @808960 (1392 B) |
| `x64/evkime32.dll` | `DYNAMIC_BASE/ASLR, NX_COMPAT/DEP` | yes (reloc dir 5992 B) | yes | no | yes | `CF_INSTRUMENTED` | 1392 B / cert @155648 (1392 B) |
| `x64/evkime64.dll` | `HIGH_ENTROPY_VA, DYNAMIC_BASE/ASLR, NX_COMPAT/DEP` | yes (reloc dir 1808 B) | yes | no | no | `0 (none)` | 1392 B / cert @185856 (1392 B) |
| `x86/EVKey32.exe` | `DYNAMIC_BASE/ASLR, NX_COMPAT/DEP, TERMINAL_SERVER_AWARE` | yes (reloc dir 22412 B) | yes | no | yes | `CF_INSTRUMENTED` | 1392 B / cert @707072 (1392 B) |
| `x86/evkime32.dll` | `DYNAMIC_BASE/ASLR, NX_COMPAT/DEP` | yes (reloc dir 5992 B) | yes | no | yes | `CF_INSTRUMENTED` | 1392 B / cert @155648 (1392 B) |

Notes: “stack cookie” is based on the parser-detected load-config security cookie. CFG is conservatively reported from the PE `GUARD_CF` characteristic and GuardFlags, not inferred from architecture. The reported overlay is primarily the end-of-file certificate table.

### Sections

- `arm64/EVKeyArm64.exe`: .text(472064 B,240060434a0c), .rdata(139776 B,ed7c1f67e4a8), .data(31744 B,bad8c050df5e), .pdata(15360 B,eb594d1700fc), .fptable(512 B,076a27c79e5a), .rsrc(153088 B,6eaf0e49b05c), .reloc(4096 B,4b0dd9fbb7f8)
- `arm64/evkimearm64.dll`: .text(148480 B,82c4e1972944), .rdata(51200 B,fc31225d99db), .data(4096 B,063be41b3fd7), .pdata(5632 B,9ea14c64208f), .fptable(512 B,076a27c79e5a), .rsrc(512 B,11458510cbf0), .reloc(2048 B,ea537945dd13)
- `x64/EVKey64.exe`: .text(427520 B,389f5246bfab), .rdata(168448 B,5094e0cea9f2), .data(32256 B,2d08f5ae6e1d), .pdata(22528 B,1aa70c54e817), .rsrc(153088 B,1b79ab48304a), .reloc(4096 B,62b19369ec5b)
- `x64/evkime32.dll`: .text(107520 B,ebe334f8d51d), .rdata(37376 B,c04bb6294f01), .data(3072 B,a0729e456796), .rsrc(512 B,3bfb34dcd2fb), .reloc(6144 B,6b6caa0d6d6f)
- `x64/evkime64.dll`: .text(115712 B,6fb332cf5d81), .rdata(55296 B,3a9b0cdee7ce), .data(4096 B,a3ea78f590a2), .pdata(7168 B,cd036e3743eb), .rsrc(512 B,39c42cfeb4b4), .reloc(2048 B,57ec67e5e3e3)
- `x86/EVKey32.exe`: .text(386048 B,13c0df550b4f), .rdata(116736 B,dab24ff0f673), .data(27648 B,2c568d6be724), .rsrc(153088 B,0af12e58aa3e), .reloc(22528 B,a6b0c08bef97)
- `x86/evkime32.dll`: .text(107520 B,ebe334f8d51d), .rdata(37376 B,c04bb6294f01), .data(3072 B,a0729e456796), .rsrc(512 B,3bfb34dcd2fb), .reloc(6144 B,6b6caa0d6d6f)

## Imports

Names below are the complete normal-import list returned by the PE parser, grouped by library (delay imports, if any, are not separately surfaced by this parser).

### `arm64/EVKeyArm64.exe` (362 imports)

- **ADVAPI32.dll** (8): `RegCloseKey`, `RegOpenKeyExW`, `RegQueryValueExW`, `OpenProcessToken`, `DuplicateToken`, `GetTokenInformation`, `RegCreateKeyExW`, `RegDeleteValueW`
- **COMCTL32.dll** (10): `ImageList_GetIconSize`, `ImageList_Draw`, `DefSubclassProc`, `RemoveWindowSubclass`, `SetWindowSubclass`, `ImageList_DrawEx`, `ImageList_SetBkColor`, `ImageList_ReplaceIcon`, `ImageList_Destroy`, `ImageList_Create`
- **COMDLG32.dll** (3): `ChooseColorW`, `GetSaveFileNameW`, `GetOpenFileNameW`
- **CRYPT32.dll** (2): `CryptStringToBinaryW`, `CryptBinaryToStringW`
- **GDI32.dll** (29): `SetBkColor`, `SetTextColor`, `BitBlt`, `CreateCompatibleBitmap`, `LineTo`, `RoundRect`, `ArcTo`, `SetArcDirection`, `GetTextExtentPoint32W`, `GetTextMetricsW`, `CreateSolidBrush`, `GetTextExtentPoint32A`, `PatBlt`, `ExtTextOutW`, `Rectangle`, `GetStockObject`, `MoveToEx`, `SetBkMode`, `SelectObject`, `AddFontMemResourceEx`, `DeleteObject`, `DeleteDC`, `CreateFontW`, `CreatePen`, `GetDeviceCaps`, `CreateRoundRectRgn`, `CreateDIBSection`, `TextOutW`, `CreateCompatibleDC`
- **gdiplus.dll** (26): `GdipCreateFromHDC`, `GdipSetPenDashStyle`, `GdipSetPenWidth`, `GdipCreatePen1`, `GdipAddPathArcI`, `GdipClosePathFigure`, `GdipSetSmoothingMode`, `GdipResetPath`, `GdipDeletePath`, `GdipCreatePath`, `GdipGetImageEncoders`, `GdipGetImageEncodersSize`, `GdipCreateHICONFromBitmap`, `GdipCreateBitmapFromHICON`, `GdipCreateBitmapFromStream`, `GdipSaveImageToStream`, `GdipDisposeImage`, `GdipCloneImage`, `GdiplusStartup`, `GdipFree`, `GdipAlloc`, `GdipDeleteGraphics`, `GdipDrawRectangleI`, `GdipDrawPath`, `GdipStartPathFigure`, `GdipDeletePen`
- **KERNEL32.dll** (123): `GetProcAddress`, `LoadLibraryW`, `MulDiv`, `lstrcpyW`, `VerifyVersionInfoW`, `K32GetProcessImageFileNameW`, `CreateToolhelp32Snapshot`, `Process32FirstW`, `Process32NextW`, `lstrlenW`, `QueryFullProcessImageNameW`, `GlobalReAlloc`, `lstrcpyA`, `lstrlenA`, `WideCharToMultiByte`, `FreeLibrary`, `ReleaseMutex`, `CreateMutexW`, `GetTickCount64`, `CreateFileMappingW`, `MapViewOfFile`, `UnmapViewOfFile`, `GetFileAttributesW`, `GetSystemWindowsDirectoryW`, `VirtualProtect`, `LoadLibraryExW`, `SetLastError`, `LocalAlloc`, `LocalFree`, `MultiByteToWideChar`, `GetProcessHeap`, `SetEnvironmentVariableW`, `FreeEnvironmentStringsW`, `GetEnvironmentStringsW`, `GetCommandLineW`, `GetModuleFileNameW`, `GetCPInfo`, `GetOEMCP`, `GetACP`, `IsValidCodePage`, `FindNextFileW`, `FindFirstFileExW`, `FindClose`, `SetStdHandle`, `ReadConsoleW`, `LCMapStringW`, `CompareStringW`, `SetFilePointerEx`, `GetFileSizeEx`, `GetConsoleMode`, `GetConsoleOutputCP`, `GetFileType`, `HeapReAlloc`, `HeapAlloc`, `HeapFree`, `WriteFile`, `GetStdHandle`, `GetModuleHandleExW`, `TerminateProcess`, `ExitProcess`, `MoveFileExW`, `ReadFile`, `GetFileAttributesExW`, `InitializeCriticalSectionEx`, `EncodePointer`, `FlsFree`, `FlsSetValue`, `FlsGetValue`, `FlsAlloc`, `RaiseException`, `RtlPcToFileHeader`, `RtlUnwindEx`, `RtlLookupFunctionEntry`, `InitializeSListHead`, `GetSystemTimeAsFileTime`, `GetStartupInfoW`, `SetUnhandledExceptionFilter`, `SleepConditionVariableSRW`, `WakeAllConditionVariable`, `AcquireSRWLockExclusive`, `ReleaseSRWLockExclusive`, `OpenProcess`, `SetPriorityClass`, `GetCurrentThreadId`, `GetCurrentProcessId`, `GetCurrentProcess`, `Sleep`, `DeleteCriticalSection`, `LeaveCriticalSection`, `EnterCriticalSection`, `InitializeCriticalSection`, `GetLastError`, `OutputDebugStringA`, `GetTempPathW`, `GetModuleHandleW`, `VerSetConditionMask`, `DeleteFileW`, `QueryPerformanceCounter`, `GetPrivateProfileSectionW`, `GetPrivateProfileStringW`, `GlobalFree`, `GlobalLock`, `GlobalUnlock`, `GlobalSize`, `GlobalAlloc`, `GetTickCount`, `CreateThread`, `CreateEventW`, `WaitForSingleObject`, `SetEvent`, `CloseHandle`, `FindResourceW`, `SizeofResource`, `LockResource`, `LoadResource`, `GetStringTypeW`, `CreateFileW`, `FlushFileBuffers`, `HeapSize`, `SetEndOfFile`, `WriteConsoleW`, `GetCommandLineA`, `RtlUnwind`
- **ole32.dll** (6): `GetHGlobalFromStream`, `CoInitialize`, `CoCreateInstance`, `CoUninitialize`, `CoInitializeEx`, `CreateStreamOnHGlobal`
- **OLEAUT32.dll** (4): `VariantCopy`, `VariantClear`, `SysStringLen`, `SysReAllocString`
- **SHELL32.dll** (5): `Shell_NotifyIconW`, `SHGetFolderPathW`, `ShellExecuteW`, `ExtractIconExW`, `SHGetStockIconInfo`
- **SHLWAPI.dll** (2): `SHDeleteKeyW`, `PathFileExistsW`
- **USER32.dll** (134): `GetWindowTextLengthW`, `IntersectRect`, `EnumThreadWindows`, `SystemParametersInfoW`, `InvalidateRgn`, `GetSubMenu`, `DestroyMenu`, `LoadMenuW`, `GetSystemMetrics`, `CallWindowProcW`, `GetClassNameW`, `GetParent`, `GetWindowLongPtrW`, `InflateRect`, `SetWindowRgn`, `GetFocus`, `EndPaint`, `CreateDialogParamW`, `SetLayeredWindowAttributes`, `DefWindowProcW`, `SetMenuDefaultItem`, `CheckMenuItem`, `IsClipboardFormatAvailable`, `GetClipboardOwner`, `LockWindowUpdate`, `SetFocus`, `GetWindowTextA`, `SetWindowTextA`, `IsWindowEnabled`, `SendMessageA`, `CopyIcon`, `CopyImage`, `DrawFocusRect`, `SetWindowLongPtrW`, `GetSysColor`, `DrawTextW`, `SetWindowPos`, `BeginPaint`, `RegisterRawInputDevices`, `GetRawInputData`, `IsDialogMessageW`, `UnhookWindowsHookEx`, `SetWindowsHookExW`, `RegisterShellHookWindow`, `FindWindowW`, `GetMenuDefaultItem`, `RegisterClassExW`, `PostQuitMessage`, `DispatchMessageW`, `TranslateMessage`, `GetMessageW`, `RegisterWindowMessageW`, `LoadStringW`, `TrackPopupMenu`, `SetForegroundWindow`, `ToAsciiEx`, `DialogBoxParamW`, `BringWindowToTop`, `DrawTextA`, `GetDC`, `ReleaseDC`, `DestroyIcon`, `CreateIconIndirect`, `OpenClipboard`, `CloseClipboard`, `SetClipboardData`, `GetClipboardData`, `RegisterClipboardFormatW`, `GetClipboardFormatNameW`, `EmptyClipboard`, `PostMessageW`, `GetKeyState`, `GetAsyncKeyState`, `GetKeyboardState`, `VkKeyScanW`, `SendInput`, `LoadKeyboardLayoutW`, `ActivateKeyboardLayout`, `GetKeyboardLayoutNameW`, `CallNextHookEx`, `GetKeyboardLayout`, `VkKeyScanExW`, `MapVirtualKeyExW`, `wsprintfW`, `OpenDesktopW`, `SetThreadDesktop`, `CloseDesktop`, `GetThreadDesktop`, `DestroyWindow`, `CreateWindowExW`, `LoadIconW`, `OpenWindowStationW`, `CloseWindowStation`, `SetProcessWindowStation`, `GetProcessWindowStation`, `SendMessageW`, `IsWindow`, `IsWindowUnicode`, `GetForegroundWindow`, `LoadCursorW`, `SetCursor`, `RedrawWindow`, `GetWindowDC`, `UpdateWindow`, `KillTimer`, `SetTimer`, `ReleaseCapture`, `SetCapture`, `MoveWindow`, `ShowWindow`, `DrawIconEx`, `PtInRect`, `FillRect`, `GetWindowRect`, `InvalidateRect`, `EnableWindow`, `EndDialog`, `SetWindowTextW`, `SendDlgItemMessageW`, `GetDlgItem`, `GetAncestor`, `GetGUIThreadInfo`, `LoadImageW`, `GetWindowThreadProcessId`, `EnumWindows`, `FindWindowExW`, `EnumChildWindows`, `WindowFromPoint`, `ScreenToClient`, `GetCursorPos`, `MessageBeep`, `MessageBoxW`, `GetClientRect`, `GetWindowTextW`
- **UxTheme.dll** (7): `SetWindowTheme`, `GetThemeInt`, `GetThemeColor`, `GetThemeBackgroundContentRect`, `DrawThemeBackground`, `CloseThemeData`, `OpenThemeData`
- **VERSION.dll** (3): `GetFileVersionInfoW`, `VerQueryValueW`, `GetFileVersionInfoSizeW`

### `arm64/evkimearm64.dll` (111 imports)

- **ADVAPI32.dll** (8): `RegDeleteKeyW`, `RegCloseKey`, `RegEnumKeyW`, `RegCreateKeyExW`, `RegDeleteTreeW`, `RegSetValueExW`, `RegOpenKeyExW`, `RegQueryValueExW`
- **KERNEL32.dll** (86): `CreateFileW`, `HeapSize`, `HeapReAlloc`, `WriteConsoleW`, `SetEndOfFile`, `InterlockedFlushSList`, `GetFileSizeEx`, `GetModuleFileNameA`, `GetTempPathA`, `DisableThreadLibraryCalls`, `GetCurrentThreadId`, `Sleep`, `GetLastError`, `OutputDebugStringW`, `LoadLibraryW`, `GetProcAddress`, `FreeLibrary`, `lstrcmpiW`, `CreateMutexW`, `WaitForSingleObject`, `OpenEventW`, `ReleaseMutex`, `OpenFileMappingW`, `UnmapViewOfFile`, `GetTickCount64`, `SetEvent`, `CloseHandle`, `MapViewOfFile`, `OpenMutexW`, `GetModuleFileNameW`, `GetStringTypeW`, `ReadConsoleW`, `ReadFile`, `SetFilePointerEx`, `SetStdHandle`, `GetProcessHeap`, `LCMapStringW`, `QueryPerformanceCounter`, `GetCurrentProcessId`, `GetSystemTimeAsFileTime`, `InitializeSListHead`, `GetStartupInfoW`, `GetModuleHandleW`, `LoadLibraryExW`, `RtlLookupFunctionEntry`, `RtlUnwindEx`, `RtlPcToFileHeader`, `RaiseException`, `VerSetConditionMask`, `VerifyVersionInfoW`, `SetLastError`, `FlsAlloc`, `FlsGetValue`, `FlsSetValue`, `FlsFree`, `EncodePointer`, `EnterCriticalSection`, `LeaveCriticalSection`, `InitializeCriticalSectionEx`, `DeleteCriticalSection`, `GetCurrentProcess`, `ExitProcess`, `TerminateProcess`, `GetModuleHandleExW`, `FlushFileBuffers`, `WriteFile`, `GetConsoleOutputCP`, `GetConsoleMode`, `HeapFree`, `HeapAlloc`, `GetStdHandle`, `GetFileType`, `FindClose`, `FindFirstFileExW`, `FindNextFileW`, `IsValidCodePage`, `GetACP`, `GetOEMCP`, `GetCPInfo`, `GetCommandLineA`, `GetCommandLineW`, `MultiByteToWideChar`, `WideCharToMultiByte`, `GetEnvironmentStringsW`, `FreeEnvironmentStringsW`, `VirtualProtect`
- **ole32.dll** (3): `StringFromGUID2`, `CoTaskMemFree`, `CoCreateInstance`
- **OLEAUT32.dll** (3): `VariantCopy`, `VariantClear`, `SysReAllocString`
- **USER32.dll** (11): `GetKeyboardState`, `UnhookWindowsHookEx`, `PostMessageW`, `SetWindowsHookExW`, `ToUnicode`, `GetMessageExtraInfo`, `SendInput`, `GetKeyState`, `CallNextHookEx`, `FindWindowW`, `wsprintfW`

### `x64/EVKey64.exe` (366 imports)

- **ADVAPI32.dll** (8): `RegCloseKey`, `RegOpenKeyExW`, `RegQueryValueExW`, `DuplicateToken`, `OpenProcessToken`, `GetTokenInformation`, `RegCreateKeyExW`, `RegDeleteValueW`
- **COMCTL32.dll** (10): `ImageList_GetIconSize`, `ImageList_Draw`, `DefSubclassProc`, `RemoveWindowSubclass`, `SetWindowSubclass`, `ImageList_DrawEx`, `ImageList_SetBkColor`, `ImageList_ReplaceIcon`, `ImageList_Destroy`, `ImageList_Create`
- **COMDLG32.dll** (3): `ChooseColorW`, `GetSaveFileNameW`, `GetOpenFileNameW`
- **CRYPT32.dll** (2): `CryptStringToBinaryW`, `CryptBinaryToStringW`
- **GDI32.dll** (29): `SetBkColor`, `TextOutW`, `BitBlt`, `CreateCompatibleBitmap`, `LineTo`, `RoundRect`, `ArcTo`, `SetArcDirection`, `GetTextExtentPoint32W`, `GetTextMetricsW`, `CreateSolidBrush`, `GetTextExtentPoint32A`, `PatBlt`, `ExtTextOutW`, `Rectangle`, `SetTextColor`, `MoveToEx`, `SetBkMode`, `SelectObject`, `AddFontMemResourceEx`, `DeleteObject`, `DeleteDC`, `CreateFontW`, `CreatePen`, `GetDeviceCaps`, `CreateRoundRectRgn`, `CreateDIBSection`, `GetStockObject`, `CreateCompatibleDC`
- **gdiplus.dll** (26): `GdipCreateFromHDC`, `GdipSetPenDashStyle`, `GdipSetPenWidth`, `GdipDeletePen`, `GdipCreatePen1`, `GdipAddPathArcI`, `GdipSetSmoothingMode`, `GdipStartPathFigure`, `GdipResetPath`, `GdipCreatePath`, `GdipGetImageEncoders`, `GdipGetImageEncodersSize`, `GdipCreateHICONFromBitmap`, `GdipCreateBitmapFromHICON`, `GdipCreateBitmapFromStream`, `GdipSaveImageToStream`, `GdipDisposeImage`, `GdipCloneImage`, `GdiplusStartup`, `GdipFree`, `GdipAlloc`, `GdipDeleteGraphics`, `GdipDrawRectangleI`, `GdipDrawPath`, `GdipClosePathFigure`, `GdipDeletePath`
- **KERNEL32.dll** (126): `LoadLibraryW`, `GetModuleFileNameW`, `GetModuleHandleW`, `OutputDebugStringA`, `GetTempPathW`, `SetPriorityClass`, `VerifyVersionInfoW`, `CreateToolhelp32Snapshot`, `Process32FirstW`, `Process32NextW`, `lstrlenW`, `QueryFullProcessImageNameW`, `GlobalReAlloc`, `lstrcpyA`, `lstrlenA`, `WideCharToMultiByte`, `FreeLibrary`, `ReleaseMutex`, `CreateMutexW`, `GetTickCount64`, `MapViewOfFile`, `UnmapViewOfFile`, `CreateFileMappingW`, `GetSystemWindowsDirectoryW`, `GetFileAttributesW`, `VirtualProtect`, `HeapAlloc`, `HeapReAlloc`, `HeapFree`, `HeapSize`, `GetProcessHeap`, `RaiseException`, `InitializeCriticalSectionAndSpinCount`, `LoadLibraryExW`, `IsProcessorFeaturePresent`, `MulDiv`, `LocalFree`, `SetLastError`, `MultiByteToWideChar`, `GetEnvironmentStringsW`, `GetCommandLineW`, `GetCommandLineA`, `GetCPInfo`, `GetOEMCP`, `IsValidCodePage`, `FindNextFileA`, `FindFirstFileExA`, `FindClose`, `SetStdHandle`, `SetFilePointerEx`, `ReadConsoleW`, `LCMapStringW`, `CompareStringW`, `GetConsoleMode`, `GetConsoleCP`, `GetFileType`, `GetStringTypeW`, `GetACP`, `WriteFile`, `GetStdHandle`, `GetModuleFileNameA`, `GetModuleHandleExW`, `ExitProcess`, `MoveFileExW`, `ReadFile`, `GetFileAttributesExW`, `TlsFree`, `TlsSetValue`, `TlsGetValue`, `TlsAlloc`, `EncodePointer`, `RtlPcToFileHeader`, `RtlUnwindEx`, `OutputDebugStringW`, `InitializeSListHead`, `GetSystemTimeAsFileTime`, `GetStartupInfoW`, `IsDebuggerPresent`, `TerminateProcess`, `SetUnhandledExceptionFilter`, `UnhandledExceptionFilter`, `RtlVirtualUnwind`, `RtlLookupFunctionEntry`, `RtlCaptureContext`, `WaitForSingleObjectEx`, `ResetEvent`, `Sleep`, `DeleteCriticalSection`, `LeaveCriticalSection`, `EnterCriticalSection`, `InitializeCriticalSection`, `GetLastError`, `GetCurrentThreadId`, `GetCurrentProcessId`, `GetCurrentProcess`, `OpenProcess`, `GetProcAddress`, `VerSetConditionMask`, `DeleteFileW`, `lstrcpyW`, `QueryPerformanceCounter`, `GetPrivateProfileSectionW`, `GetPrivateProfileStringW`, `CreateEventW`, `GetTickCount`, `CloseHandle`, `WaitForSingleObject`, `SetEvent`, `CreateThread`, `GlobalFree`, `GlobalUnlock`, `GlobalLock`, `GlobalSize`, `GlobalAlloc`, `FindResourceW`, `SizeofResource`, `LoadResource`, `LockResource`, `FreeEnvironmentStringsW`, `SetEnvironmentVariableW`, `CreateFileW`, `FlushFileBuffers`, `WriteConsoleW`, `RtlUnwind`, `SetEndOfFile`, `LocalAlloc`
- **ole32.dll** (6): `CreateStreamOnHGlobal`, `CoCreateInstance`, `CoUninitialize`, `CoInitialize`, `CoInitializeEx`, `GetHGlobalFromStream`
- **OLEAUT32.dll** (4): `VariantCopy`, `VariantClear`, `SysStringLen`, `SysReAllocString`
- **PSAPI.DLL** (1): `GetProcessImageFileNameW`
- **SHELL32.dll** (5): `ShellExecuteW`, `Shell_NotifyIconW`, `SHGetFolderPathW`, `ExtractIconExW`, `SHGetStockIconInfo`
- **SHLWAPI.dll** (2): `SHDeleteKeyW`, `PathFileExistsW`
- **USER32.dll** (134): `BeginPaint`, `EndPaint`, `InvalidateRgn`, `GetWindowTextLengthW`, `IntersectRect`, `EnumThreadWindows`, `SystemParametersInfoW`, `RegisterRawInputDevices`, `GetClassNameW`, `GetParent`, `InflateRect`, `SetWindowRgn`, `GetFocus`, `DialogBoxParamW`, `CreateDialogParamW`, `SetLayeredWindowAttributes`, `DefWindowProcW`, `GetRawInputData`, `SetMenuDefaultItem`, `CheckMenuItem`, `IsClipboardFormatAvailable`, `GetClipboardOwner`, `LockWindowUpdate`, `SetFocus`, `GetWindowTextA`, `SetWindowTextA`, `IsWindowEnabled`, `SendMessageA`, `CopyIcon`, `CopyImage`, `DrawFocusRect`, `SetWindowLongPtrW`, `GetSysColor`, `DrawTextW`, `SetWindowPos`, `DestroyWindow`, `CreateWindowExW`, `LoadIconW`, `LoadCursorW`, `SetCursor`, `RedrawWindow`, `IsDialogMessageW`, `LoadStringW`, `UnhookWindowsHookEx`, `SetWindowsHookExW`, `RegisterShellHookWindow`, `FindWindowW`, `GetMenuDefaultItem`, `RegisterClassExW`, `PostQuitMessage`, `DispatchMessageW`, `TranslateMessage`, `GetMessageW`, `RegisterWindowMessageW`, `CallNextHookEx`, `SetForegroundWindow`, `ToAsciiEx`, `BringWindowToTop`, `TrackPopupMenu`, `GetSubMenu`, `DestroyMenu`, `LoadMenuW`, `GetSystemMetrics`, `CallWindowProcW`, `DrawTextA`, `GetDC`, `ReleaseDC`, `DestroyIcon`, `CreateIconIndirect`, `OpenClipboard`, `CloseClipboard`, `SetClipboardData`, `GetClipboardData`, `RegisterClipboardFormatW`, `GetClipboardFormatNameW`, `EmptyClipboard`, `PostMessageW`, `GetWindowLongPtrW`, `GetKeyState`, `GetAsyncKeyState`, `GetKeyboardState`, `VkKeyScanW`, `SendInput`, `LoadKeyboardLayoutW`, `ActivateKeyboardLayout`, `GetKeyboardLayoutNameW`, `GetKeyboardLayout`, `VkKeyScanExW`, `MapVirtualKeyExW`, `wsprintfW`, `OpenDesktopW`, `SetThreadDesktop`, `GetWindowDC`, `UpdateWindow`, `KillTimer`, `CloseDesktop`, `GetThreadDesktop`, `OpenWindowStationW`, `CloseWindowStation`, `SetProcessWindowStation`, `GetProcessWindowStation`, `SendMessageW`, `IsWindow`, `SetTimer`, `ReleaseCapture`, `SetCapture`, `MoveWindow`, `ShowWindow`, `DrawIconEx`, `PtInRect`, `FillRect`, `GetWindowRect`, `InvalidateRect`, `EnableWindow`, `EndDialog`, `SetWindowTextW`, `SendDlgItemMessageW`, `GetDlgItem`, `GetAncestor`, `GetGUIThreadInfo`, `LoadImageW`, `GetWindowThreadProcessId`, `EnumWindows`, `FindWindowExW`, `EnumChildWindows`, `WindowFromPoint`, `ScreenToClient`, `GetCursorPos`, `MessageBeep`, `MessageBoxW`, `GetClientRect`, `GetWindowTextW`, `GetForegroundWindow`, `IsWindowUnicode`
- **UxTheme.dll** (7): `SetWindowTheme`, `GetThemeInt`, `GetThemeColor`, `GetThemeBackgroundContentRect`, `DrawThemeBackground`, `CloseThemeData`, `OpenThemeData`
- **VERSION.dll** (3): `GetFileVersionInfoW`, `VerQueryValueW`, `GetFileVersionInfoSizeW`

### `x64/evkime32.dll` (113 imports)

- **ADVAPI32.dll** (8): `RegDeleteKeyW`, `RegCloseKey`, `RegEnumKeyW`, `RegCreateKeyExW`, `RegDeleteTreeW`, `RegSetValueExW`, `RegOpenKeyExW`, `RegQueryValueExW`
- **KERNEL32.dll** (88): `SetEndOfFile`, `ReadFile`, `ReadConsoleW`, `GetModuleHandleW`, `HeapReAlloc`, `GetModuleFileNameA`, `GetTempPathA`, `DisableThreadLibraryCalls`, `InterlockedDecrement`, `InterlockedIncrement`, `InterlockedCompareExchange`, `GetCurrentThreadId`, `Sleep`, `GetLastError`, `OutputDebugStringW`, `LoadLibraryW`, `GetProcAddress`, `FreeLibrary`, `lstrcmpiW`, `CreateMutexW`, `WaitForSingleObject`, `OpenEventW`, `ReleaseMutex`, `OpenFileMappingW`, `UnmapViewOfFile`, `GetTickCount64`, `SetEvent`, `CloseHandle`, `MapViewOfFile`, `OpenMutexW`, `GetModuleFileNameW`, `HeapSize`, `CreateFileW`, `WriteConsoleW`, `SetFilePointerEx`, `SetStdHandle`, `GetStringTypeW`, `GetProcessHeap`, `FreeEnvironmentStringsW`, `GetEnvironmentStringsW`, `GetCommandLineW`, `UnhandledExceptionFilter`, `SetUnhandledExceptionFilter`, `GetCurrentProcess`, `TerminateProcess`, `IsProcessorFeaturePresent`, `QueryPerformanceCounter`, `GetCurrentProcessId`, `GetSystemTimeAsFileTime`, `InitializeSListHead`, `IsDebuggerPresent`, `GetStartupInfoW`, `DecodePointer`, `RtlUnwind`, `RaiseException`, `InterlockedFlushSList`, `SetLastError`, `EncodePointer`, `EnterCriticalSection`, `LeaveCriticalSection`, `DeleteCriticalSection`, `InitializeCriticalSectionAndSpinCount`, `TlsAlloc`, `TlsGetValue`, `TlsSetValue`, `TlsFree`, `LoadLibraryExW`, `ExitProcess`, `GetModuleHandleExW`, `MultiByteToWideChar`, `WideCharToMultiByte`, `FlushFileBuffers`, `WriteFile`, `GetConsoleCP`, `GetConsoleMode`, `HeapFree`, `HeapAlloc`, `GetACP`, `GetStdHandle`, `GetFileType`, `LCMapStringW`, `FindClose`, `FindFirstFileExA`, `FindNextFileA`, `IsValidCodePage`, `GetOEMCP`, `GetCPInfo`, `GetCommandLineA`
- **ole32.dll** (3): `StringFromGUID2`, `CoTaskMemFree`, `CoCreateInstance`
- **OLEAUT32.dll** (3): `VariantCopy`, `VariantClear`, `SysReAllocString`
- **USER32.dll** (11): `GetKeyboardState`, `UnhookWindowsHookEx`, `PostMessageW`, `SetWindowsHookExW`, `ToUnicode`, `GetMessageExtraInfo`, `SendInput`, `GetKeyState`, `CallNextHookEx`, `FindWindowW`, `wsprintfW`

### `x64/evkime64.dll` (113 imports)

- **ADVAPI32.dll** (8): `RegDeleteKeyW`, `RegCloseKey`, `RegEnumKeyW`, `RegCreateKeyExW`, `RegDeleteTreeW`, `RegSetValueExW`, `RegOpenKeyExW`, `RegQueryValueExW`
- **KERNEL32.dll** (88): `SetEndOfFile`, `ReadFile`, `ReadConsoleW`, `RtlUnwindEx`, `HeapReAlloc`, `GetModuleFileNameA`, `GetTempPathA`, `DisableThreadLibraryCalls`, `GetCurrentThreadId`, `Sleep`, `GetLastError`, `OutputDebugStringW`, `LoadLibraryW`, `GetProcAddress`, `FreeLibrary`, `lstrcmpiW`, `CreateMutexW`, `WaitForSingleObject`, `OpenEventW`, `ReleaseMutex`, `OpenFileMappingW`, `UnmapViewOfFile`, `GetTickCount64`, `SetEvent`, `CloseHandle`, `MapViewOfFile`, `OpenMutexW`, `GetModuleFileNameW`, `HeapSize`, `CreateFileW`, `WriteConsoleW`, `SetFilePointerEx`, `SetStdHandle`, `GetStringTypeW`, `GetProcessHeap`, `FreeEnvironmentStringsW`, `GetEnvironmentStringsW`, `GetCommandLineW`, `RtlCaptureContext`, `RtlLookupFunctionEntry`, `RtlVirtualUnwind`, `UnhandledExceptionFilter`, `SetUnhandledExceptionFilter`, `GetCurrentProcess`, `TerminateProcess`, `IsProcessorFeaturePresent`, `QueryPerformanceCounter`, `GetCurrentProcessId`, `GetSystemTimeAsFileTime`, `InitializeSListHead`, `IsDebuggerPresent`, `GetStartupInfoW`, `GetModuleHandleW`, `RtlPcToFileHeader`, `RaiseException`, `InterlockedFlushSList`, `SetLastError`, `EncodePointer`, `EnterCriticalSection`, `LeaveCriticalSection`, `DeleteCriticalSection`, `InitializeCriticalSectionAndSpinCount`, `TlsAlloc`, `TlsGetValue`, `TlsSetValue`, `TlsFree`, `LoadLibraryExW`, `ExitProcess`, `GetModuleHandleExW`, `MultiByteToWideChar`, `WideCharToMultiByte`, `FlushFileBuffers`, `WriteFile`, `GetConsoleCP`, `GetConsoleMode`, `HeapFree`, `HeapAlloc`, `GetACP`, `GetStdHandle`, `GetFileType`, `LCMapStringW`, `FindClose`, `FindFirstFileExA`, `FindNextFileA`, `IsValidCodePage`, `GetOEMCP`, `GetCPInfo`, `GetCommandLineA`
- **ole32.dll** (3): `StringFromGUID2`, `CoTaskMemFree`, `CoCreateInstance`
- **OLEAUT32.dll** (3): `VariantCopy`, `VariantClear`, `SysReAllocString`
- **USER32.dll** (11): `GetKeyboardState`, `UnhookWindowsHookEx`, `PostMessageW`, `SetWindowsHookExW`, `ToUnicode`, `GetMessageExtraInfo`, `SendInput`, `GetKeyState`, `CallNextHookEx`, `FindWindowW`, `wsprintfW`

### `x86/EVKey32.exe` (365 imports)

- **ADVAPI32.dll** (8): `RegCloseKey`, `RegOpenKeyExW`, `RegQueryValueExW`, `DuplicateToken`, `OpenProcessToken`, `GetTokenInformation`, `RegCreateKeyExW`, `RegDeleteValueW`
- **COMCTL32.dll** (10): `ImageList_GetIconSize`, `ImageList_Draw`, `DefSubclassProc`, `RemoveWindowSubclass`, `SetWindowSubclass`, `ImageList_DrawEx`, `ImageList_SetBkColor`, `ImageList_ReplaceIcon`, `ImageList_Destroy`, `ImageList_Create`
- **COMDLG32.dll** (3): `ChooseColorW`, `GetSaveFileNameW`, `GetOpenFileNameW`
- **CRYPT32.dll** (2): `CryptStringToBinaryW`, `CryptBinaryToStringW`
- **GDI32.dll** (29): `SetBkColor`, `TextOutW`, `BitBlt`, `CreateCompatibleBitmap`, `LineTo`, `RoundRect`, `ArcTo`, `SetArcDirection`, `GetTextExtentPoint32W`, `GetTextMetricsW`, `CreateSolidBrush`, `GetTextExtentPoint32A`, `PatBlt`, `ExtTextOutW`, `Rectangle`, `SetTextColor`, `MoveToEx`, `SetBkMode`, `SelectObject`, `AddFontMemResourceEx`, `DeleteObject`, `DeleteDC`, `CreateFontW`, `CreatePen`, `GetDeviceCaps`, `CreateRoundRectRgn`, `CreateDIBSection`, `GetStockObject`, `CreateCompatibleDC`
- **gdiplus.dll** (26): `GdipCreateFromHDC`, `GdipSetPenDashStyle`, `GdipSetPenWidth`, `GdipDeletePen`, `GdipCreatePen1`, `GdipAddPathArcI`, `GdipSetSmoothingMode`, `GdipStartPathFigure`, `GdipResetPath`, `GdipCreatePath`, `GdipGetImageEncoders`, `GdipGetImageEncodersSize`, `GdipCreateHICONFromBitmap`, `GdipCreateBitmapFromHICON`, `GdipCreateBitmapFromStream`, `GdipSaveImageToStream`, `GdipDisposeImage`, `GdipCloneImage`, `GdiplusStartup`, `GdipFree`, `GdipAlloc`, `GdipDeleteGraphics`, `GdipDrawRectangleI`, `GdipDrawPath`, `GdipClosePathFigure`, `GdipDeletePath`
- **KERNEL32.dll** (125): `MulDiv`, `lstrcpyW`, `LoadLibraryW`, `GetModuleFileNameW`, `GetModuleHandleW`, `OutputDebugStringA`, `GetTempPathW`, `SetPriorityClass`, `VerifyVersionInfoW`, `CreateToolhelp32Snapshot`, `Process32FirstW`, `Process32NextW`, `lstrlenW`, `QueryFullProcessImageNameW`, `GlobalReAlloc`, `lstrcpyA`, `lstrlenA`, `WideCharToMultiByte`, `FreeLibrary`, `ReleaseMutex`, `CreateMutexW`, `InterlockedExchange`, `GetTickCount64`, `MapViewOfFile`, `UnmapViewOfFile`, `CreateFileMappingW`, `GetSystemWindowsDirectoryW`, `GetFileAttributesW`, `DecodePointer`, `VirtualProtect`, `HeapAlloc`, `HeapReAlloc`, `HeapFree`, `HeapSize`, `GetProcessHeap`, `RaiseException`, `InitializeCriticalSectionAndSpinCount`, `LoadLibraryExW`, `IsProcessorFeaturePresent`, `DeleteCriticalSection`, `LocalFree`, `SetLastError`, `MultiByteToWideChar`, `SetEndOfFile`, `SetEnvironmentVariableW`, `FreeEnvironmentStringsW`, `GetEnvironmentStringsW`, `GetCommandLineW`, `GetCommandLineA`, `GetCPInfo`, `GetOEMCP`, `IsValidCodePage`, `FindNextFileA`, `FindFirstFileExA`, `FindClose`, `SetStdHandle`, `SetFilePointerEx`, `ReadConsoleW`, `LCMapStringW`, `CompareStringW`, `GetConsoleMode`, `GetConsoleCP`, `GetFileType`, `GetStringTypeW`, `GetACP`, `WriteFile`, `GetStdHandle`, `GetModuleFileNameA`, `GetModuleHandleExW`, `ExitProcess`, `MoveFileExW`, `ReadFile`, `GetFileAttributesExW`, `TlsFree`, `TlsSetValue`, `TlsGetValue`, `TlsAlloc`, `EncodePointer`, `RtlUnwind`, `OutputDebugStringW`, `InitializeSListHead`, `GetSystemTimeAsFileTime`, `GetStartupInfoW`, `IsDebuggerPresent`, `TerminateProcess`, `SetUnhandledExceptionFilter`, `UnhandledExceptionFilter`, `WaitForSingleObjectEx`, `ResetEvent`, `LeaveCriticalSection`, `EnterCriticalSection`, `InitializeCriticalSection`, `GetLastError`, `GetCurrentThreadId`, `GetCurrentProcessId`, `GetCurrentProcess`, `OpenProcess`, `GetProcAddress`, `VerSetConditionMask`, `DeleteFileW`, `InterlockedDecrement`, `InterlockedIncrement`, `WriteConsoleW`, `FlushFileBuffers`, `CreateFileW`, `Sleep`, `QueryPerformanceCounter`, `GetPrivateProfileSectionW`, `GetPrivateProfileStringW`, `CreateEventW`, `GetTickCount`, `CloseHandle`, `WaitForSingleObject`, `SetEvent`, `CreateThread`, `GlobalFree`, `GlobalUnlock`, `GlobalLock`, `GlobalSize`, `GlobalAlloc`, `FindResourceW`, `SizeofResource`, `LoadResource`, `LockResource`, `LocalAlloc`
- **ole32.dll** (6): `CreateStreamOnHGlobal`, `CoCreateInstance`, `CoUninitialize`, `CoInitialize`, `CoInitializeEx`, `GetHGlobalFromStream`
- **OLEAUT32.dll** (4): `VariantClear`, `VariantCopy`, `SysReAllocString`, `SysStringLen`
- **PSAPI.DLL** (1): `GetProcessImageFileNameW`
- **SHELL32.dll** (5): `Shell_NotifyIconW`, `ShellExecuteW`, `ExtractIconExW`, `SHGetFolderPathW`, `SHGetStockIconInfo`
- **SHLWAPI.dll** (2): `SHDeleteKeyW`, `PathFileExistsW`
- **USER32.dll** (134): `UnhookWindowsHookEx`, `LoadStringW`, `IsDialogMessageW`, `GetRawInputData`, `RegisterRawInputDevices`, `BeginPaint`, `EndPaint`, `InvalidateRgn`, `GetWindowTextLengthW`, `IntersectRect`, `EnumThreadWindows`, `SystemParametersInfoW`, `SetWindowsHookExW`, `DestroyMenu`, `LoadMenuW`, `GetSystemMetrics`, `CallWindowProcW`, `GetClassNameW`, `GetParent`, `RegisterShellHookWindow`, `InflateRect`, `SetWindowRgn`, `GetFocus`, `CreateDialogParamW`, `SetLayeredWindowAttributes`, `DefWindowProcW`, `SetMenuDefaultItem`, `CheckMenuItem`, `IsClipboardFormatAvailable`, `GetClipboardOwner`, `LockWindowUpdate`, `SetFocus`, `GetWindowTextA`, `SetWindowTextA`, `IsWindowEnabled`, `SendMessageA`, `CopyIcon`, `CopyImage`, `DrawFocusRect`, `SetWindowLongW`, `GetSysColor`, `DrawTextW`, `SetWindowPos`, `DestroyWindow`, `CreateWindowExW`, `FindWindowW`, `GetMenuDefaultItem`, `RegisterClassExW`, `PostQuitMessage`, `DispatchMessageW`, `TranslateMessage`, `GetMessageW`, `RegisterWindowMessageW`, `CallNextHookEx`, `SetForegroundWindow`, `ToAsciiEx`, `BringWindowToTop`, `TrackPopupMenu`, `GetWindowLongW`, `DrawTextA`, `LoadIconW`, `LoadCursorW`, `SetCursor`, `RedrawWindow`, `GetSubMenu`, `GetDC`, `ReleaseDC`, `DestroyIcon`, `CreateIconIndirect`, `OpenClipboard`, `CloseClipboard`, `SetClipboardData`, `GetClipboardData`, `RegisterClipboardFormatW`, `GetClipboardFormatNameW`, `EmptyClipboard`, `PostMessageW`, `GetKeyState`, `GetAsyncKeyState`, `GetKeyboardState`, `VkKeyScanW`, `SendInput`, `DialogBoxParamW`, `LoadKeyboardLayoutW`, `ActivateKeyboardLayout`, `GetKeyboardLayoutNameW`, `GetKeyboardLayout`, `VkKeyScanExW`, `MapVirtualKeyExW`, `wsprintfW`, `OpenDesktopW`, `SetThreadDesktop`, `CloseDesktop`, `GetThreadDesktop`, `OpenWindowStationW`, `CloseWindowStation`, `SetProcessWindowStation`, `GetWindowDC`, `UpdateWindow`, `KillTimer`, `GetProcessWindowStation`, `SendMessageW`, `IsWindow`, `IsWindowUnicode`, `GetForegroundWindow`, `GetWindowTextW`, `GetClientRect`, `MessageBoxW`, `SetTimer`, `ReleaseCapture`, `SetCapture`, `MoveWindow`, `ShowWindow`, `DrawIconEx`, `PtInRect`, `FillRect`, `GetWindowRect`, `InvalidateRect`, `EnableWindow`, `EndDialog`, `SetWindowTextW`, `SendDlgItemMessageW`, `GetDlgItem`, `GetAncestor`, `GetGUIThreadInfo`, `LoadImageW`, `GetWindowThreadProcessId`, `EnumWindows`, `FindWindowExW`, `EnumChildWindows`, `WindowFromPoint`, `ScreenToClient`, `GetCursorPos`, `MessageBeep`
- **UxTheme.dll** (7): `SetWindowTheme`, `GetThemeInt`, `GetThemeColor`, `GetThemeBackgroundContentRect`, `DrawThemeBackground`, `CloseThemeData`, `OpenThemeData`
- **VERSION.dll** (3): `GetFileVersionInfoW`, `VerQueryValueW`, `GetFileVersionInfoSizeW`

### `x86/evkime32.dll` (113 imports)

- **ADVAPI32.dll** (8): `RegDeleteKeyW`, `RegCloseKey`, `RegEnumKeyW`, `RegCreateKeyExW`, `RegDeleteTreeW`, `RegSetValueExW`, `RegOpenKeyExW`, `RegQueryValueExW`
- **KERNEL32.dll** (88): `SetEndOfFile`, `ReadFile`, `ReadConsoleW`, `GetModuleHandleW`, `HeapReAlloc`, `GetModuleFileNameA`, `GetTempPathA`, `DisableThreadLibraryCalls`, `InterlockedDecrement`, `InterlockedIncrement`, `InterlockedCompareExchange`, `GetCurrentThreadId`, `Sleep`, `GetLastError`, `OutputDebugStringW`, `LoadLibraryW`, `GetProcAddress`, `FreeLibrary`, `lstrcmpiW`, `CreateMutexW`, `WaitForSingleObject`, `OpenEventW`, `ReleaseMutex`, `OpenFileMappingW`, `UnmapViewOfFile`, `GetTickCount64`, `SetEvent`, `CloseHandle`, `MapViewOfFile`, `OpenMutexW`, `GetModuleFileNameW`, `HeapSize`, `CreateFileW`, `WriteConsoleW`, `SetFilePointerEx`, `SetStdHandle`, `GetStringTypeW`, `GetProcessHeap`, `FreeEnvironmentStringsW`, `GetEnvironmentStringsW`, `GetCommandLineW`, `UnhandledExceptionFilter`, `SetUnhandledExceptionFilter`, `GetCurrentProcess`, `TerminateProcess`, `IsProcessorFeaturePresent`, `QueryPerformanceCounter`, `GetCurrentProcessId`, `GetSystemTimeAsFileTime`, `InitializeSListHead`, `IsDebuggerPresent`, `GetStartupInfoW`, `DecodePointer`, `RtlUnwind`, `RaiseException`, `InterlockedFlushSList`, `SetLastError`, `EncodePointer`, `EnterCriticalSection`, `LeaveCriticalSection`, `DeleteCriticalSection`, `InitializeCriticalSectionAndSpinCount`, `TlsAlloc`, `TlsGetValue`, `TlsSetValue`, `TlsFree`, `LoadLibraryExW`, `ExitProcess`, `GetModuleHandleExW`, `MultiByteToWideChar`, `WideCharToMultiByte`, `FlushFileBuffers`, `WriteFile`, `GetConsoleCP`, `GetConsoleMode`, `HeapFree`, `HeapAlloc`, `GetACP`, `GetStdHandle`, `GetFileType`, `LCMapStringW`, `FindClose`, `FindFirstFileExA`, `FindNextFileA`, `IsValidCodePage`, `GetOEMCP`, `GetCPInfo`, `GetCommandLineA`
- **ole32.dll** (3): `StringFromGUID2`, `CoTaskMemFree`, `CoCreateInstance`
- **OLEAUT32.dll** (3): `VariantCopy`, `VariantClear`, `SysReAllocString`
- **USER32.dll** (11): `GetKeyboardState`, `UnhookWindowsHookEx`, `PostMessageW`, `SetWindowsHookExW`, `ToUnicode`, `GetMessageExtraInfo`, `SendInput`, `GetKeyState`, `CallNextHookEx`, `FindWindowW`, `wsprintfW`

## Exports

- `arm64/EVKeyArm64.exe`: none
- `arm64/evkimearm64.dll` (4): `DllCanUnloadNow`, `DllGetClassObject`, `DllRegisterServer`, `DllUnregisterServer`
- `x64/EVKey64.exe`: none
- `x64/evkime32.dll` (4): `DllCanUnloadNow`, `DllGetClassObject`, `DllRegisterServer`, `DllUnregisterServer`
- `x64/evkime64.dll` (4): `DllCanUnloadNow`, `DllGetClassObject`, `DllRegisterServer`, `DllUnregisterServer`
- `x86/EVKey32.exe`: none
- `x86/evkime32.dll` (4): `DllCanUnloadNow`, `DllGetClassObject`, `DllRegisterServer`, `DllUnregisterServer`

## Resources and manifests

| File | Resource count | Type summary | Resource languages |
|---|---:|---|---|
| `arm64/EVKeyArm64.exe` | 42 | CURSOR=1, DIALOG=10, FONT=1, FONTDIR=1, GROUP_CURSOR=1, GROUP_ICON=3, ICON=6, MANIFEST=1, MENU=2, STRING=1, UNKNOWN (2392)=10, UNKNOWN (2428)=1, UNKNOWN (2444)=1, UNKNOWN (2452)=2, VERSION=1 | LANG_ENGLISH |
| `arm64/evkimearm64.dll` | 1 | MANIFEST=1 | LANG_ENGLISH |
| `x64/EVKey64.exe` | 42 | CURSOR=1, DIALOG=10, FONT=1, FONTDIR=1, GROUP_CURSOR=1, GROUP_ICON=3, ICON=6, MANIFEST=1, MENU=2, STRING=1, UNKNOWN (2392)=10, UNKNOWN (2428)=1, UNKNOWN (2444)=1, UNKNOWN (2452)=2, VERSION=1 | LANG_ENGLISH |
| `x64/evkime32.dll` | 1 | MANIFEST=1 | LANG_ENGLISH |
| `x64/evkime64.dll` | 1 | MANIFEST=1 | LANG_ENGLISH |
| `x86/EVKey32.exe` | 42 | CURSOR=1, DIALOG=10, FONT=1, FONTDIR=1, GROUP_CURSOR=1, GROUP_ICON=3, ICON=6, MANIFEST=1, MENU=2, STRING=1, UNKNOWN (2392)=10, UNKNOWN (2428)=1, UNKNOWN (2444)=1, UNKNOWN (2452)=2, VERSION=1 | LANG_ENGLISH |
| `x86/evkime32.dll` | 1 | MANIFEST=1 | LANG_ENGLISH |

### Manifest: `arm64/EVKeyArm64.exe`

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3"><dependency><dependentAssembly><assemblyIdentity type="Win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" publicKeyToken="6595b64144ccf1df" language="*" processorArchitecture="*"></assemblyIdentity></dependentAssembly></dependency><trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"></requestedExecutionLevel></requestedPrivileges></security></trustInfo><compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"></supportedOS><supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"></supportedOS><supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"></supportedOS><supportedOS Id="{e2011457-1546-43c5-a5fe-008deee3d3f0}"></supportedOS><supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"></supportedOS></application></compatibility></assembly>
```

### Manifest: `arm64/evkimearm64.dll`

```xml
<?xml version='1.0' encoding='UTF-8' standalone='yes'?>
<assembly xmlns='urn:schemas-microsoft-com:asm.v1' manifestVersion='1.0'>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level='asInvoker' uiAccess='false' />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
```

### Manifest: `x64/EVKey64.exe`

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3"><dependency><dependentAssembly><assemblyIdentity type="Win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" publicKeyToken="6595b64144ccf1df" language="*" processorArchitecture="*"></assemblyIdentity></dependentAssembly></dependency><trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"></requestedExecutionLevel></requestedPrivileges></security></trustInfo><compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"></supportedOS><supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"></supportedOS><supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"></supportedOS><supportedOS Id="{e2011457-1546-43c5-a5fe-008deee3d3f0}"></supportedOS><supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"></supportedOS></application></compatibility></assembly>
```

### Manifest: `x64/evkime32.dll`

```xml
<?xml version='1.0' encoding='UTF-8' standalone='yes'?>
<assembly xmlns='urn:schemas-microsoft-com:asm.v1' manifestVersion='1.0'>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level='asInvoker' uiAccess='false' />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
```

### Manifest: `x64/evkime64.dll`

```xml
<?xml version='1.0' encoding='UTF-8' standalone='yes'?>
<assembly xmlns='urn:schemas-microsoft-com:asm.v1' manifestVersion='1.0'>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level='asInvoker' uiAccess='false' />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
```

### Manifest: `x86/EVKey32.exe`

```xml
<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0" xmlns:asmv3="urn:schemas-microsoft-com:asm.v3"><dependency><dependentAssembly><assemblyIdentity type="Win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" publicKeyToken="6595b64144ccf1df" language="*" processorArchitecture="*"></assemblyIdentity></dependentAssembly></dependency><trustInfo xmlns="urn:schemas-microsoft-com:asm.v3"><security><requestedPrivileges><requestedExecutionLevel level="asInvoker" uiAccess="false"></requestedExecutionLevel></requestedPrivileges></security></trustInfo><compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"></supportedOS><supportedOS Id="{1f676c76-80e1-4239-95bb-83d0f6d0da78}"></supportedOS><supportedOS Id="{4a2f28e3-53b9-4441-ba9c-d69d4a4a6e38}"></supportedOS><supportedOS Id="{e2011457-1546-43c5-a5fe-008deee3d3f0}"></supportedOS><supportedOS Id="{35138b9a-5d96-4fbd-8e2d-a2440225f93a}"></supportedOS></application></compatibility></assembly>
```

### Manifest: `x86/evkime32.dll`

```xml
<?xml version='1.0' encoding='UTF-8' standalone='yes'?>
<assembly xmlns='urn:schemas-microsoft-com:asm.v1' manifestVersion='1.0'>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level='asInvoker' uiAccess='false' />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
```

Resource observations: EXEs contain dialogs, menus, icons/cursors, font resources, strings, VERSION and MANIFEST data. DLLs contain only a MANIFEST resource; no DLL VERSION resource was found. `UNKNOWN (...)` labels are parser limitations for custom or numerically represented resource types.

## Authenticode / certificate tables

| File | Table | PKCS#7 | Leaf certificate | Trust result | Timestamp token |
|---|---|---|---|---|---|
| `arm64/EVKeyArm64.exe` | 1392 B; blob `3744e3cfed9be10ef5085b10df9c4a77c31b43e2083636f80791b4fc9042440a` | parsed | subject=CN=LamQuangMinh; issuer=CN=LamQuangMinh; serial=1A275EE4373C52BB44C1EE233E8ECDF0; notBefore=Jun 17 13:46:24 2026 GMT; notAfter=Jun 17 13:56:24 2031 GMT; sha256 Fingerprint=EA:92:08:2E:AA:E6:A7:EC:43:7B:02:14:6B:55:23:29:C8:32:ED:9A:EC:68:31:89:66:B7:9A:65:F9:6F:43:D9 | `CN=LamQuangMinh; error 18 at 0 depth lookup: self-signed certificate; error /root/.hermes/cache/scratch/evkey-certificates/arm64_EVKeyArm64.exe.pem: verification failed` | no |
| `arm64/evkimearm64.dll` | 1392 B; blob `dcc4718c3e9157636d31aedd776e809efb7bd82fcdc16554cbc986ff4f953e70` | parsed | subject=CN=LamQuangMinh; issuer=CN=LamQuangMinh; serial=1A275EE4373C52BB44C1EE233E8ECDF0; notBefore=Jun 17 13:46:24 2026 GMT; notAfter=Jun 17 13:56:24 2031 GMT; sha256 Fingerprint=EA:92:08:2E:AA:E6:A7:EC:43:7B:02:14:6B:55:23:29:C8:32:ED:9A:EC:68:31:89:66:B7:9A:65:F9:6F:43:D9 | `CN=LamQuangMinh; error 18 at 0 depth lookup: self-signed certificate; error /root/.hermes/cache/scratch/evkey-certificates/arm64_evkimearm64.dll.pem: verification failed` | no |
| `x64/EVKey64.exe` | 1392 B; blob `523b14511be72dc0509d7f9d559bd2cee2da3ed92882d023555106fea4ccbc8e` | parsed | subject=CN=LamQuangMinh; issuer=CN=LamQuangMinh; serial=1A275EE4373C52BB44C1EE233E8ECDF0; notBefore=Jun 17 13:46:24 2026 GMT; notAfter=Jun 17 13:56:24 2031 GMT; sha256 Fingerprint=EA:92:08:2E:AA:E6:A7:EC:43:7B:02:14:6B:55:23:29:C8:32:ED:9A:EC:68:31:89:66:B7:9A:65:F9:6F:43:D9 | `CN=LamQuangMinh; error 18 at 0 depth lookup: self-signed certificate; error /root/.hermes/cache/scratch/evkey-certificates/x64_EVKey64.exe.pem: verification failed` | no |
| `x64/evkime32.dll` | 1392 B; blob `8a9ac4a1e59250e4e015fdbf15d4f80663a612338f9a59b2147b23d4d9957820` | parsed | subject=CN=LamQuangMinh; issuer=CN=LamQuangMinh; serial=1A275EE4373C52BB44C1EE233E8ECDF0; notBefore=Jun 17 13:46:24 2026 GMT; notAfter=Jun 17 13:56:24 2031 GMT; sha256 Fingerprint=EA:92:08:2E:AA:E6:A7:EC:43:7B:02:14:6B:55:23:29:C8:32:ED:9A:EC:68:31:89:66:B7:9A:65:F9:6F:43:D9 | `CN=LamQuangMinh; error 18 at 0 depth lookup: self-signed certificate; error /root/.hermes/cache/scratch/evkey-certificates/x64_evkime32.dll.pem: verification failed` | no |
| `x64/evkime64.dll` | 1392 B; blob `86e25c5cb55e218b5f80ba9811ddb15b63bf7e06b1458445c422a716d22086a2` | parsed | subject=CN=LamQuangMinh; issuer=CN=LamQuangMinh; serial=1A275EE4373C52BB44C1EE233E8ECDF0; notBefore=Jun 17 13:46:24 2026 GMT; notAfter=Jun 17 13:56:24 2031 GMT; sha256 Fingerprint=EA:92:08:2E:AA:E6:A7:EC:43:7B:02:14:6B:55:23:29:C8:32:ED:9A:EC:68:31:89:66:B7:9A:65:F9:6F:43:D9 | `CN=LamQuangMinh; error 18 at 0 depth lookup: self-signed certificate; error /root/.hermes/cache/scratch/evkey-certificates/x64_evkime64.dll.pem: verification failed` | no |
| `x86/EVKey32.exe` | 1392 B; blob `ca28a62cd965fb161b4d49698526714b5e67b8a324445d0929aba3a17e481958` | parsed | subject=CN=LamQuangMinh; issuer=CN=LamQuangMinh; serial=1A275EE4373C52BB44C1EE233E8ECDF0; notBefore=Jun 17 13:46:24 2026 GMT; notAfter=Jun 17 13:56:24 2031 GMT; sha256 Fingerprint=EA:92:08:2E:AA:E6:A7:EC:43:7B:02:14:6B:55:23:29:C8:32:ED:9A:EC:68:31:89:66:B7:9A:65:F9:6F:43:D9 | `CN=LamQuangMinh; error 18 at 0 depth lookup: self-signed certificate; error /root/.hermes/cache/scratch/evkey-certificates/x86_EVKey32.exe.pem: verification failed` | no |
| `x86/evkime32.dll` | 1392 B; blob `8a9ac4a1e59250e4e015fdbf15d4f80663a612338f9a59b2147b23d4d9957820` | parsed | subject=CN=LamQuangMinh; issuer=CN=LamQuangMinh; serial=1A275EE4373C52BB44C1EE233E8ECDF0; notBefore=Jun 17 13:46:24 2026 GMT; notAfter=Jun 17 13:56:24 2031 GMT; sha256 Fingerprint=EA:92:08:2E:AA:E6:A7:EC:43:7B:02:14:6B:55:23:29:C8:32:ED:9A:EC:68:31:89:66:B7:9A:65:F9:6F:43:D9 | `CN=LamQuangMinh; error 18 at 0 depth lookup: self-signed certificate; error /root/.hermes/cache/scratch/evkey-certificates/x86_evkime32.dll.pem: verification failed` | no |

Signature interpretation: all PKCS#7 blobs parse and embed a certificate, but the leaf is self-signed and not anchored in the local trust store. Because image-digest verification was not available, this inventory does not claim that the signature cryptographically matches the current file bytes.

## File relationships

- Exact duplicate SHA-256 `91b978ab3c816f78125afae8d4fe2390d2fa1ce3d696d2df223cb6f6b193d53d`: `x64/evkime32.dll`, `x86/evkime32.dll`
- Executable family: `EVKey32.exe`, `EVKey64.exe`, and `EVKeyArm64.exe` share product/version metadata, manifests, near-identical resource type layouts, and broadly corresponding Win32 imports; they are architecture-specific builds rather than identical binaries.
- IME family: all `evkime*.dll` variants share the same export API and near-identical manifest structure, but carry no VERSION resource. The x86 DLL is duplicated into both x86 and x64 package directories; x64 and ARM64 variants are distinct builds.
- Signature family: all seven files use the same leaf certificate fingerprint/subject. Different binaries have different PKCS#7 blobs; the two byte-identical x86 DLL copies also have the same blob.

- Common DLL exports (4): `DllCanUnloadNow`, `DllGetClassObject`, `DllRegisterServer`, `DllUnregisterServer`

## Limitations and command evidence

- No dynamic behavior was observed, and static imports are capabilities—not proof that every API is reached at runtime.
- `rabin2 -T` returned `Missing bin header signature` for this Authenticode form. OpenSSL parsed the embedded PKCS#7 and certificate but does not by itself perform PE Authenticode hashing; trust and image-integrity conclusions are therefore kept separate.
- Representative commands used (repeated over all seven paths):
```sh
file -L '<file>'
sha256sum '<file>'; md5sum '<file>'
rabin2 -Ij '<file>'; rabin2 -ij '<file>'; rabin2 -Ej '<file>'
rabin2 -Uj '<file>'; rabin2 -V '<file>'; rabin2 -Hj '<file>'
openssl pkcs7 -inform DER -in '<extracted WIN_CERTIFICATE payload>' -print_certs
openssl x509 -in '<embedded certificate>' -noout -subject -issuer -serial -dates -fingerprint -sha256
openssl verify -purpose any '<embedded certificate>'
```
