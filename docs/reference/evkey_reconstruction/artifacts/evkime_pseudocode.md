# EVKey IME DLLs — reconstructed pseudocode

> Static reconstruction only. Addresses below are x64 virtual addresses unless a cross-architecture table says otherwise. Ghidra output is pseudocode, not original source.

## 1. `DllMain` / process initialization — `0x180001130`

```text
DllMainLike(module, reason):
  if reason == PROCESS_ATTACH:
    g_module = module
    DisableThreadLibraryCalls(module)
    host = basename(GetModuleFileNameA(NULL))
    append_debug_log("DllMain: DLL_PROCESS_ATTACH host=%s", host)
  else if reason == PROCESS_DETACH:
    host = basename(GetModuleFileNameA(NULL))
    append_debug_log("DllMain: DLL_PROCESS_DETACH host=%s", host)
  return via CRT dispatcher
```

No application protocol state or typing engine is initialized in `DllMain`. The loader entry is `0x180006d20`; it dispatches to this routine through the CRT DLL startup path.

## 2. COM/TSF object construction — `0x180001440`

```text
ClassFactory_CreateInstance(outer, iid, out):
  reject aggregation when outer != NULL
  obj = allocate(0x41d0)
  install seven CEVKeyIME interface vtables
  initialize refcount, TSF manager/client IDs, sink cookies,
             current context/composition pointers,
             bridge handles, state counters, history buffers
  increment global DLL object count
  QueryInterface(iid, out)
  Release(obj)
```

The interfaces recovered from RTTI are `ITfTextInputProcessor`, `ITfTextInputProcessorEx`, `ITfThreadMgrEventSink`, `ITfCompositionSink`, `ITfDisplayAttributeProvider`, and associated COM identities.

## 3. TSF activation and hook installation — `0x180002460`

```text
ActivateEx(this, threadMgr, clientId):
  this.threadMgr = AddRef(threadMgr)
  this.clientId = clientId
  hostExe = lowercase(basename(GetModuleFileNameA(NULL)))
  initialize/read per-application TSF cache

  for attempt in 1..3:
    if bridge_connect(): break
    Sleep(30 ms)

  advise ITfThreadMgrEventSink on thread manager
  advise keystroke/event sink(s)
  obtain TSF category/display-attribute manager state

  if globalMouseHook == NULL:
    globalActiveIme = this
    globalMouseHook = SetWindowsHookExW(WH_MOUSE=7,
                                        MouseHookProc_0x180001bf0,
                                        NULL,
                                        GetCurrentThreadId())
```

The hook is thread-local (`dwThreadId` is current thread), despite global variables holding the hook and active object within the DLL instance loaded in that process.

## 4. Mouse hook / context reset — `0x180001bf0`

```text
MouseHookProc(code, message, mouseInfo):
  if code >= 0 and message in {WM_LBUTTONDOWN, WM_RBUTTONDOWN}
     and activeIme != NULL and activeIme.composition != NULL:

    choose an available TSF context/edit-session owner
    if none:
      composition.EndComposition(NULL)
    else:
      request a CEndCompSession to end composition safely in TSF

    unadvise/release composition state
    if bridge.shared && shared.resetEnabled(+0x24):
      bridge_request(RESET=3)
    clear local historyDepth and composition counters

  return CallNextHookEx(globalMouseHook, code, message, mouseInfo)
```

## 5. Shared-memory connection — `0x1800052c0`

```text
bridge_connect(bridge):
  if bridge.view != NULL: return true

  bridge.map = OpenFileMappingW(0xF001F, false,
                                "EVKeyIME_SharedMem_v1")
  bridge.view = MapViewOfFile(bridge.map, 0xF001F, 0, 0, 0x1038)
  bridge.requestEvent  = OpenEventW(0x1F0003, false,
                                    "EVKeyIME_Request_v1")
  bridge.responseEvent = OpenEventW(0x100000, false,
                                    "EVKeyIME_Response_v1")
  bridge.mutex = OpenMutexW(0x100000, false,
                            "EVKeyIME_Mutex_v1")
  if missing mutex:
    bridge.mutex = CreateMutexW(NULL, false, "EVKeyIME_Mutex_v1")
  if any required object is missing:
    close/unmap all and return false

  bridge_request(CONNECT_OR_SYNC=5)
  release mutex if held
  return true
```

## 6. Request/response handshake — `0x180005470`

```text
bridge_request(op):
  ensure connected
  lock mutex with 100 ms timeout; record whether lock was obtained
  drain stale response-event signals with zero-time waits

  shared.op(+0x00) = op
  expected = atomic_increment(shared.requestSeq(+0x10))
  SetEvent(requestEvent)

  wait up to 50 ms for responseEvent
  success = signaled && shared.responseSeq(+0x14) == expected

  if !success and mutex held:
    release mutex
  return success
```

Observed partial shared-memory layout:

```text
+0x000  uint32 op                  # observed 1,2,3,5
+0x004  uint32 inputChar           # UTF-16 code unit for op 1
+0x008  uint32 shiftPressed
+0x00c  uint32 capsLockOn
+0x010  int32  requestSeq          # interlocked increment
+0x014  int32  responseSeq         # must equal requestSeq
+0x018  int32  backspaces
+0x01c  int32  outputByteLength    # divided by 2 in DLL
+0x020  uint32 flag20              # semantics unresolved
+0x024  uint32 resetEnabled        # gates op 3 requests
+0x028  uint32 flag28              # semantics unresolved
+0x02c  uint32 flag2c              # semantics unresolved
+0x030  uint64 timestampOrEpoch    # compared with GetTickCount64, 2000 ms window
+0x038  byte   utf16Output[0x1000] # copied using outputByteLength
TOTAL = 0x1038
```

## 7. Character processing bridge — `0x180005550`

```text
bridge_process_char(ch, shift, caps):
  ensure connected
  shared.inputChar = ch
  shared.shiftPressed = shift
  shared.capsLockOn = caps

  if bridge_request(PROCESS_CHAR=1):
    backs = shared.backspaces
    outputBytes = shared.outputByteLength
    outputUnits = outputBytes / 2
    outputUTF16 = copy(shared+0x38, outputBytes)
    release mutex
    return (backs, outputUTF16)
```

## 8. TSF key state machine — `0x180002cf0`, `0x180003c20`

```text
OnTestKeyDown(context, vk, ..., eaten):
  eaten = false
  ignore injected events whose GetMessageExtraInfo() == 0xEBCA1234
  reject/reset when bridge state is stale or unavailable
  classify modifiers and supported keys
  report whether OnKeyDown should consume the key

OnKeyDown(context, vk, ..., eaten):
  eaten = false
  ignore injected events tagged 0xEBCA1234

  if bridge/config state is temporarily invalid:
    submit edit session to flush/end composition
    replay original vk with SendInput-tagged event
    eaten = true
    return

  if vk == VK_BACK:
    if local TSF history has an entry:
      pop one 0x208-byte history snapshot
      submit edit session restoring its saved UTF-16/context state
      reset/synchronize bridge as needed
    else:
      bridge_request(BACKSPACE=2)
      consume shared.backspaces and shared UTF-16 output
      submit an edit session to delete/replace surrounding text
    replay VK_BACK with sentinel if required
    return

  text = ToUnicode(vk, keyboardState)
  if exactly one UTF-16 code unit:
    (backs, replacement) = bridge_process_char(text[0], shift, caps)
    submit CEditSession(ApplyReplacement)
      - move TSF range backward by `backs`
      - replace range with `replacement`
      - update/start composition and display attributes
      - save bounded local history/context snapshot
    suppress or replay original physical key according to result
```

`SendInput` helper `0x180002a70` emits a key-down/key-up pair and sets `dwExtraInfo = 0xEBCA1234`. The TSF key callbacks test this marker, preventing recursion. It is not the primary Unicode text emitter.

## 9. Unicode replacement/edit session — `0x180002f00`

```text
ApplyReplacement(editCookie, context, result):
  if deletion/backspace count > 0:
    obtain selection/range
    ShiftStart(editCookie, -backspaces)
    verify surrounding text where required

  if no active composition and change is non-empty:
    start TSF composition on selected range
    attach display attribute

  obtain composition/range
  assemble UTF-16 replacement from bridge response and local context
  range.SetText(editCookie, 0, replacement, replacementLength)
  move/collapse selection to end
  update composition/display attribute state

  on separator/reset/end conditions:
    end composition, release range/context, clear counters
```

This is the key distinction from the legacy UniKey hook: Unicode is committed with TSF range operations, while injected physical keys are only fallback/replay/control events.

## 10. App-specific behavior

```text
specialApps = {
  "msedge.exe", "msedgewebview2.exe", "chrome.exe", "brave.exe",
  "opera.exe", "vivaldi.exe", "chromium.exe", "teams.exe", "ms-teams.exe"
}

on activation:
  hostExe = lowercase executable basename
  specialApps bypass/alter TsfCache probing
  other apps read HKCU\Software\EVKey\TsfCache\v<version>\<hostExe>

when a backspace/context compatibility probe succeeds:
  cache DWORD 1 under the host executable name
```

The exact user-visible meaning of the cache bit is inferred as a TSF/backspace/context compatibility decision; static evidence proves the cache controls an alternate path but does not provide its original source-level name.

## 11. Cross-architecture evidence map

| Role | x64 | x86 | ARM64 |
|---|---:|---:|---:|
| DllMain-like body | `0x180001130` | `0x10001110` | `0x180001560` |
| ActivateEx / install `WH_MOUSE` | `0x180002460` | `0x10002380` | `0x180003340` |
| Deactivate / main-window notify | `0x180002780` | `0x10002630` | `0x1800036d0` |
| SendInput sentinel pair | `0x180002a70` | `0x10002920` | `0x180003a30` |
| Main OnKeyDown / `ToUnicode` | `0x180003c20` | `0x10003970` | `0x180004ce0` |
| Shared-memory connect | `0x1800052c0` | `0x10004cf0` | `0x180006370` |

All three architectures contain the same four exports, named kernel objects, special-application list, request protocol, and `0xEBCA1234` injection marker.
