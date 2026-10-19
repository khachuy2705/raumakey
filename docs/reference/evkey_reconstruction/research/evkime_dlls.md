# Static reverse engineering report: `evkime` DLL family

## Scope and evidence standard

This report covers static analysis only. None of the DLLs was loaded or executed. Claims are labeled:

- **Observed** — directly supported by PE metadata, imports/strings, disassembly, or Ghidra decompilation.
- **Inferred** — architectural interpretation supported by multiple observed facts or by comparison with UniKey source.
- **Unknown** — not reliably recoverable from this static pass.

Primary reconstructed pseudocode: [`../artifacts/evkime_pseudocode.md`](../artifacts/evkime_pseudocode.md). Full Ghidra exports are also retained for address-level follow-up.

## 1. Artifact identity

| Artifact | Format | Size | SHA-256 |
|---|---|---:|---|
| `x64/evkime64.dll` | PE32+ DLL, x86-64, Windows 5.02, 6 sections | 187,248 | `1d53028833d6f44bb1dc47baf2ac7c5cea92aa9194a44d841fb26fcf25b9a65f` |
| `x64/evkime32.dll` | PE32 DLL, i386, Windows 5.01, 5 sections | 157,040 | `91b978ab3c816f78125afae8d4fe2390d2fa1ce3d696d2df223cb6f6b193d53d` |
| `x86/evkime32.dll` | byte-identical copy of the preceding x86 image | 157,040 | `91b978ab3c816f78125afae8d4fe2390d2fa1ce3d696d2df223cb6f6b193d53d` |
| `arm64/evkimearm64.dll` | PE32+ DLL, ARM64, Windows 6.02, 7 sections | 214,896 | `5f288a34a1e44fe8ea240b76e61c77d068a80c508a05e85cbfcf289652e693f9` |

All three analyzed architectures expose exactly four standard in-process COM server exports:

```text
DllCanUnloadNow
DllGetClassObject
DllRegisterServer
DllUnregisterServer
```

**Observed:** there are no legacy `MyKeyHook`, `InitSharedMem`, or engine exports. RTTI identifies `CEVKeyIME`, `CEditSession`, `CEndCompSession`, `CDisplayAttributeInfo`, and `CEnumDisplayAttributeInfo`, plus TSF interfaces including `ITfTextInputProcessor(Ex)`, `ITfThreadMgrEventSink`, `ITfCompositionSink`, and `ITfDisplayAttributeProvider`.

**Conclusion:** this family is a TSF Text Input Processor COM DLL, not the old exported global keyboard-hook DLL represented by `src-win/keyhook/keyhook.def`.

## 2. Loader and COM lifecycle

### Observed

- x64 loader entry: `0x180006d20`; DllMain-like application body: `0x180001130`.
- Process attach stores the module handle and calls `DisableThreadLibraryCalls`.
- Attach/detach log the host executable name to `D:\evkime_dll_log.txt`, falling back to `%TEMP%\evkime_dll_log.txt`.
- No protocol mapping, thread hook, or engine object is created from DllMain.
- `DllGetClassObject` x64 `0x1800015a0` checks one CLSID and returns `CClassFactory`.
- Factory routine x64 `0x180001440` allocates a `CEVKeyIME` object of `0x41d0` bytes and installs seven interface vtable views.
- `DllCanUnloadNow` uses a global object/lock counter.
- Registration code registers a keyboard TIP, language/profile (`vi-VN` appears in static data), display-attribute provider, and TSF capability categories including `TIP_KEYBOARD`, `TIPCAP_UIELEMENTENABLED`, `TIPCAP_SECUREMODE`, `TIPCAP_IMMERSIVESUPPORT`, `TIPCAP_SYSTRAYSUPPORT`, `TIPCAP_INPUTMODECOMPARTMENT`, and `TIPCAP_COMLESS`.

### Inferred

A `CEVKeyIME` instance owns state for one activated TSF processor in the hosting process/thread. This replaces the legacy design in which a process-global `VietKey VnKbd` object was attached to a mapped `SharedMem` block in every hooked process.

## 3. Hook model and window messages

### Observed

Activation (`0x180002460` x64) performs TSF sink setup and installs:

```text
SetWindowsHookExW(7, 0x180001bf0, NULL, GetCurrentThreadId())
```

Hook ID 7 is `WH_MOUSE`. The callback at `0x180001bf0` watches only:

- `WM_LBUTTONDOWN` (`0x201`)
- `WM_RBUTTONDOWN` (`0x204`)

On those messages it ends any active TSF composition, unadvises/releases composition state, optionally requests engine reset, clears local history/counters, and chains with `CallNextHookEx`.

Deactivation (`0x180002780`) unhooks it and, under a preserved state condition, finds `EVKey_MainWnd` and posts message `0x403`:

```text
FindWindowW(L"EVKey_MainWnd", NULL)
PostMessageW(hwnd, 0x403, 0, 0)
```

### Inferred

- The mouse hook exists to terminate/reset composition when caret/focus changes by mouse. It is not the Vietnamese key transform hook.
- Message `0x403` (`WM_USER + 3`) notifies the EVKey controller when a TIP deactivation/state transition matters. Its exact symbolic name is **unknown**.

## 4. Interprocess engine protocol

### Named objects (observed)

```text
EVKeyIME_SharedMem_v1     file mapping, mapped size 0x1038
EVKeyIME_Request_v1       request event
EVKeyIME_Response_v1      response event
EVKeyIME_Mutex_v1         synchronization mutex
```

The bridge object opens the mapping and events and opens or creates the mutex. A request:

1. waits up to 100 ms for the mutex;
2. drains stale response signals;
3. writes an operation code at shared offset `+0x00`;
4. atomically increments request sequence at `+0x10`;
5. signals the request event;
6. waits up to 50 ms for the response event;
7. accepts the response only if `+0x14` equals the new request sequence.

### Recovered mapping layout

| Offset | Size | Meaning | Status |
|---:|---:|---|---|
| `0x00` | 4 | operation code | **Observed** |
| `0x04` | 4 | input UTF-16 code unit | **Observed** for op 1 |
| `0x08` | 4 | Shift pressed | **Observed** |
| `0x0c` | 4 | Caps Lock active | **Observed** |
| `0x10` | 4 | request sequence | **Observed** |
| `0x14` | 4 | response sequence | **Observed** |
| `0x18` | 4 | number of characters/backspaces to replace | **Observed** |
| `0x1c` | 4 | output byte length | **Observed**; DLL divides by two for UTF-16 units |
| `0x20` | 4 | state flag | **Unknown name/semantics** |
| `0x24` | 4 | flag gating reset request | **Observed behavior**, inferred name |
| `0x28` | 4 | state flag | **Unknown name/semantics** |
| `0x2c` | 4 | state flag | **Unknown name/semantics** |
| `0x30` | 8 | tick/epoch value | **Observed** comparison to `GetTickCount64`; 2,000 ms validity window |
| `0x38` | `0x1000` | UTF-16 replacement/output bytes | **Observed** |

### Operation codes

| Code | Reconstructed operation | Evidence |
|---:|---|---|
| `1` | Process a character | x64 `0x180005550`: writes char/Shift/Caps, then reads backs/UTF-16 output |
| `2` | Process Backspace / obtain replacement | x64 `0x180003c20` backspace branch reads `+0x18`, `+0x1c`, `+0x38` |
| `3` | Reset engine/composition context | x64 `0x180005660` and mouse/context teardown paths |
| `5` | Connect/synchronize bridge | x64 `0x1800052c0` immediately after opening objects |

**Observed:** engine requests are serialized by a named mutex and correlated by sequence numbers. The mapping is process-shared; the server/controller implementing the other side is outside these DLLs.

**Unknown:** exact server-side implementation, operation enum names, remaining flag names, and whether security descriptors restrict the namespace. Those require the controller executable/source or runtime IPC capture, neither used here.

## 5. Keyboard event state machine

### Entry points

- Probable `OnTestKeyDown`: x64 `0x180002cf0`.
- Probable `OnKeyDown`: x64 `0x180003c20`.
- Edit-session replacement executor: x64 `0x180002f00`.
- Physical-key replay helper: x64 `0x180002a70`.

Names are inferred from TSF vtable placement and call semantics; addresses and behavior are observed.

### Reconstructed flow

1. `OnTestKeyDown`/`OnKeyDown` rejects self-injected events when `GetMessageExtraInfo() == 0xEBCA1234`.
2. Modifier/key classification excludes unsuitable Ctrl/Alt combinations and handles Backspace, Enter, space, digits, letters, and OEM punctuation specially.
3. `GetKeyboardState` + `ToUnicode` converts a virtual key into one UTF-16 code unit.
4. The DLL sends `{op=1, char, shift, caps}` to the external engine.
5. The response supplies `backspaces` and a UTF-16 replacement string.
6. A `CEditSession` moves an `ITfRange` backward by the requested amount, verifies surrounding text on guarded paths, calls TSF `SetText`, updates selection, and starts/updates/ends composition plus display attributes.
7. When the original physical key must be replayed, `SendInput` emits key-down and key-up tagged with `0xEBCA1234`, preventing callback recursion.

### Backspace and restoration

The DLL has two cooperating strategies:

- **TSF-local history:** `CEVKeyIME` contains a bounded history area. The decompilation shows a maximum of 32 entries and an apparent per-entry stride of `0x208`, storing up to 0x100 UTF-16 units plus metadata. On Backspace it can pop a snapshot and submit an edit session restoring saved text/context.
- **Engine request:** when local restoration is unavailable, operation 2 asks the external engine for deletion count and replacement text. The edit session applies both atomically to the TSF range.

Composition is ended/reset on mouse clicks, thread-manager/context changes, incompatible keys/modifiers, stale bridge state, and separators according to branch state.

### Unicode emission

**Observed:** transformed text is emitted primarily through TSF range operations (`ITfRange::SetText` via vtable calls in `0x180002f00`). `SendInput` is used for tagged physical-key replay/control, not to type the returned Vietnamese UTF-16 string. `PostMessageW` is used for the EVKey controller notification, not normal text output.

This differs materially from old UniKey `src-win/keyhook/keyhook.cpp`, where Unicode could be emitted through clipboard paste or posted `WM_CHAR`/`WM_IME_CHAR` messages.

## 6. State ownership: thread, process, and shared server

### Observed

- Each COM `CEVKeyIME` object has its own TSF manager/client ID, sink cookies, context/composition pointers, edit-session state, undo/history buffers, counters, and bridge handles.
- The mouse hook handle and active TIP pointer are DLL globals in the host process, but the hook itself is installed for the current thread only.
- The named mapping/events/mutex are shared across processes and connect all architecture variants to the same controller-side protocol.
- Explicit `TlsAlloc`/`TlsGetValue` imports resolve to the statically linked CRT/FLS support. No application-owned TLS allocation was found in the EVKey-specific call graph.

### Inferred

- **Per TSF activation/thread:** active composition, selection/context, edit-session history, suppression/replay state.
- **Per host process/DLL instance:** module handle, host executable basename, one current mouse-hook binding/global active instance, app compatibility cache state.
- **Cross-process/controller:** Vietnamese engine state and configuration behind `EVKeyIME_SharedMem_v1` and the request/response events.

### Unknown

The mapping has one apparent request slot. Static analysis cannot prove whether the external controller maintains one `UkEngine` per client process, per thread, per foreground window, or a single global engine reset on context changes. The DLL protocol does not carry an obvious PID/TID field in the mapped `0x1038` bytes, although the controller may infer clients by synchronization or other state.

## 7. Application-specific behavior

### Observed special list

All three architectures embed the same nine executable names:

```text
msedge.exe
msedgewebview2.exe
chrome.exe
brave.exe
opera.exe
vivaldi.exe
chromium.exe
teams.exe
ms-teams.exe
```

The host basename is lowercased and compared case-insensitively. Non-special applications use a registry compatibility cache:

```text
HKCU\Software\EVKey\TsfCache\v<version>\<host-exe> = DWORD 1
```

Old version subkeys under `HKCU\Software\EVKey\TsfCache` are pruned. A successful context/backspace probe can persist `1` for the host. Special applications bypass or force parts of this cache logic.

### Inferred

The cache chooses between two TSF deletion/context strategies, likely to accommodate browsers/Electron/Teams behavior. The exact source-level option name and the precise distinction between the two paths remain unknown.

## 8. Comparison with UniKey architecture and engine

### Legacy Windows hook (`src-win/keyhook`)

UniKey source shows:

- named shared mapping `UniKeyHookDllFileMap` created in `DllMain`;
- exported keyboard/mouse callbacks (`MyKeyHook`, `MyMouseHook`);
- a process-global `VietKey VnKbd` buffer;
- key classification and transformation performed inside the injected DLL;
- backspaces synthesized before replacement output;
- Unicode emitted by clipboard paste or direct window messages.

Relevant source evidence:

- `src-win/keyhook/keyhook.cpp:74-149` — DllMain and mapping;
- `:178-230` — keyboard hook and `VietKey::process`;
- `:234-304` — Unicode/ANSI output and backspace sequencing;
- `src-win/keyhook/vietkey.cpp:66-227` — main transformation state machine;
- `:274-611` — breve/roof, double character, tone, shortcuts, VNI handling;
- `src-win/keyhook/vietkey.h:23-55` — 40-key local buffer and output arrays.

### Modern `ukengine`

The later UniKey engine is the closer conceptual match for the external EVKey engine:

- `UkInputProcessor` maps input methods to semantic events.
- `UkEngine` tracks word form, consonant/vowel sequences, tone position, original keystrokes, conversion flags, and restoration state.
- It returns exactly the abstraction visible in EVKey IPC: **backspace count + replacement bytes + output type**.

Source evidence:

- `x-unikey/src/ukengine/inputproc.cpp:100-162` — Telex, VNI, VIQR mappings;
- `ukengine.cpp:268-291` — semantic dispatch (`processRoof`, `processHook`, `processDd`, `processTone`, etc.);
- `ukengine.cpp:492-630` — roof/circumflex transform and undo;
- `ukengine.cpp:1734-1808` — main `process`, outputs backs and replacement;
- `ukengine.cpp:1818-1899` — encoding and backspace-count calculation;
- `ukengine.cpp:1921-1977` — semantic Backspace and tone reposition;
- `ukengine.cpp:2183-2233` — restore original keystrokes;
- `ukengine.cpp:2275-2315` — word-end spell check and auto-restore.

### Input method transformations

The source mappings recover the likely controller-side behavior:

- **Telex:** `s/f/r/x/j` tones, `z` remove tone, `aa/ee/oo` roofs, `dd`, `w` hook/bowl shortcut, bracket shortcuts.
- **VNI:** `1..5` tones, `0` remove tone, `6` roof, `7` horn, `8` breve, `9` đ.
- **VIQR:** `'`, `` ` ``, `?`, `~`, `.` tones; `^` roof; `+`/`*` horn; `(` breve; `D` for đ; `\` escape.
- Tone placement is sequence-aware; roof/horn changes can reposition an existing tone. Duplicate marks can undo/revert, and invalid/non-Vietnamese words can restore original keystrokes when enabled.

**Important calibration:** these transformations are **source-observed in UniKey** and architecturally consistent with EVKey’s response contract, but they are not directly present in the analyzed DLL code. The DLL forwards character events to an external controller/engine. Exact EVKey modifications, option defaults, and controller-side deviations from UniKey remain unknown.

## 9. Cross-architecture equivalence

| Role | x64 | x86 | ARM64 |
|---|---:|---:|---:|
| DllMain-like body | `0x180001130` | `0x10001110` | `0x180001560` |
| ActivateEx / hook setup | `0x180002460` | `0x10002380` | `0x180003340` |
| Deactivate / message `0x403` | `0x180002780` | `0x10002630` | `0x1800036d0` |
| tagged `SendInput` | `0x180002a70` | `0x10002920` | `0x180003a30` |
| main key handler / `ToUnicode` | `0x180003c20` | `0x10003970` | `0x180004ce0` |
| mapping/event connection | `0x1800052c0` | `0x10004cf0` | `0x180006370` |

All architectures contain the same exports, named IPC objects, special-app list, controller window name/message, `0x1038` mapping size, request timeout logic, operation codes, and injection sentinel. This strongly supports a single protocol and state machine compiled for three ABIs.

## 10. Reconstruction-ready contract

A compatible DLL-side clean implementation needs these layers:

1. **COM/TSF server:** class factory, `ITfTextInputProcessorEx`, keystroke/thread-manager/composition sinks, display attribute provider.
2. **Per-activation context:** TSF manager/client ID, sink cookies, active context/composition/range, bounded undo/context snapshots.
3. **Bridge:** exact named objects, `0x1038` mapping, request/response sequence correlation, 100 ms mutex and 50 ms response bounds.
4. **Key filter:** `ToUnicode`, modifier classification, Backspace/separator/context reset rules, sentinel-based replay suppression.
5. **Edit sessions:** delete/replace via TSF ranges, selection updates, composition lifecycle, display attributes.
6. **Controller engine:** UniKey-like `UkEngine` producing `{backs, UTF-16 replacement}`, with Telex/VNI/VIQR mapping, word-form validation, tone reposition, duplicate-mark undo, Backspace reflow, and optional original-keystroke restoration.
7. **Compatibility policy:** nine special apps and per-host `TsfCache` probing/persistence.

## 11. Limitations

- No DLL or companion sample was executed.
- REA’s configured Ghidra provider was unavailable (`GHIDRA_INSTALL_DIR` unset), so analysis used the already-installed local Ghidra 11.4.2 headless tool directly. Full auto-analysis and decompilation completed for all three architectures.
- Ghidra reported one non-fatal `WindowsResourceReference` analyzer class-loading warning under Java 25. Core PE, RTTI, control-flow, reference, and decompiler passes completed and exports were produced.
- Stripped application symbols mean many functions retain `FUN_<address>` names. TSF method names are reconstructed from vtable placement and behavior.
- Original C++ types, local names, comments, exact field names, and some COM interface identities are not recoverable exactly.
- Static analysis cannot prove timing/race behavior, host-specific TSF quirks, or server-side engine partitioning.
- Exact EVKey controller transformations cannot be claimed from these DLLs alone; UniKey source supplies the reference engine model, not proof of byte-identical EVKey engine code.
