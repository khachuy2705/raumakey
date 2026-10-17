//! The in-process COM server of `raumakey_tip.dll` (plan §10.3, §10.4; design §12): the four
//! DLL exports and the text service they serve, which turns TSF callbacks into calls on the
//! session core ([`ServiceCore`](crate::session::ServiceCore)). The rules the text service
//! follows on its own live in [`adapter`](crate::adapter), apart from COM; the text service
//! only reads and writes TSF objects for them.
//!
//! # The DLL and its text service
//!
//! - [`DllGetClassObject`] serves a class factory (`raumakey_windows::com_server`) for
//!   [`CLSID`] only. Its `CreateInstance` rejects aggregation and creates a text service.
//! - The text service is one COM object that implements `ITfTextInputProcessorEx` (with
//!   `ITfTextInputProcessor`), `ITfKeyEventSink`, `ITfThreadMgrEventSink`, `ITfTextEditSink`,
//!   `ITfThreadFocusSink`, and `ITfCompartmentEventSink`, and owns a `ServiceCore`.
//! - `ActivateEx` (and `Activate`) stores the thread manager and the client id and advises
//!   the sinks of [`Sink::ALL`](crate::activation::Sink::ALL) in order: the thread manager
//!   event sink and the thread focus sink through the thread manager's `ITfSource`, the key
//!   event sink through `ITfKeystrokeMgr::AdviseKeyEventSink(tid, sink, TRUE)`, and a
//!   compartment event sink on `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` of the thread manager's
//!   compartment manager. When an advise fails, the sinks advised before it are unadvised
//!   again and its error is returned. A null thread manager is `E_INVALIDARG`. Activating an
//!   active service ends the first activation first. A successful activation then gives
//!   itself a new `SessionId`, installs the mouse hook of the thread
//!   ([`mouse_hook`](crate::mouse_hook)), shows the core's mode in the open/close compartment,
//!   and takes over the focus of the thread manager (`ITfThreadMgr::GetFocus`). The
//!   activation flags (`TF_TMAE_*`) change nothing.
//! - `Deactivate` removes the mouse hook, unadvises the text edit sink and every sink the
//!   activation advised, also after an unadvise fails (it returns the first error), releases
//!   the thread manager, and gives the next activation a fresh core. Deactivating twice, or
//!   without an activation, only resets the core.
//! - [`DllCanUnloadNow`] is `S_OK` only while no class factory, text service, or edit session
//!   of this server is alive and no server lock is held.
//! - [`DllRegisterServer`] and [`DllUnregisterServer`] register and remove the COM class, the
//!   language profile ([`PROFILE_GUID`], LANGID `0x042A`, description "Raumakey", icon index
//!   `PROFILE_ICON_INDEX` of this DLL), and the TSF categories, in the registry view of the
//!   DLL's bitness (`raumakey_windows::tsf_registration`). `build.rs` embeds the icons.
//! - [`CLSID`] and [`PROFILE_GUID`] are statics, so that their binary form is in the image,
//!   where `xtask inspect` looks for them.
//!
//! # The key path, the documents, and the mode
//!
//! The text service does what the [`session`](crate::session) contract asks of the COM layer:
//!
//! - **Keys.** `OnTestKeyDown`, `OnKeyDown`, `OnTestKeyUp`, and `OnKeyUp` decode `wParam` and
//!   `lParam` (virtual key, scan code, extended flag, repeat), read the modifiers from the
//!   thread's key state (`GetKeyState`) and the message extra info zero-extended, both before
//!   anything calls into TSF or the host (which could retrieve another message), translate a
//!   key-down with `ToUnicodeEx` without changing the keyboard state, and build the
//!   `HostContext` of the key's context ([`adapter::keys`](crate::adapter::keys),
//!   [`adapter::host`](crate::adapter::host)). The keyboard layout of the thread is checked
//!   at every key; a new layout tells the core whether it has AltGr. On an AltGr layout,
//!   right Alt is AltGr for the engine and the hotkeys, and the synthetic left Ctrl that
//!   Windows sends with it never reaches the core. The core answers whether the key is eaten.
//! - **Edits.** An edit of the core is one synchronous read-write edit session
//!   (`TF_ES_SYNC | TF_ES_READWRITE`) that runs
//!   [`replace_before_caret`](crate::adapter::edit::replace_before_caret) on the selection
//!   (matrix #40, #41). A session that TSF refuses or cannot run synchronously changes nothing
//!   and is `EditError::Denied`: the word resets and the key reaches the application.
//! - **Replays.** A replay key of the core (`KeyDownOutcome::replay`) is sent with `SendInput`,
//!   tagged `RAUMAKEY_INJECTED_TAG` (`raumakey_windows::input`), at the end of `OnKeyDown`,
//!   after the core call returned and its edit session ended, never inside an edit session.
//!   `SendInput` only appends to the input stream, so the application receives the replayed
//!   key after `OnKeyDown` has returned and before any key typed later; posting it to a window
//!   instead could let a later key overtake it. The tag makes the core let the key through
//!   (matrix #39). When `SendInput` is refused (UIPI, a locked desktop), the replayed key is
//!   lost: the original key was eaten already.
//! - **Documents.** A context gets its `DocumentId` when a key or the focus first reaches it
//!   ([`adapter::documents`](crate::adapter::documents)) and keeps it until TSF pops it or
//!   uninitializes its document manager; the core then drops its state.
//! - **Focus.** `ITfThreadMgrEventSink::OnSetFocus`, a context pushed on the focused document
//!   manager, the pop of the focused context (the base context takes over), and activation
//!   move the focus: the `ITfTextEditSink` is advised on the focused context's `ITfSource`
//!   only, the input scope is read again, and `ServiceCore::set_focus` ends the words of both
//!   documents (matrix #20). A key from another context moves the text edit sink there too.
//!   `ITfThreadFocusSink` and the key event sink's `OnSetFocus` end the words as well: keys
//!   may have been missed meanwhile.
//! - **Changes.** `ITfTextEditSink::OnEndEdit` reads the edit record with its read-only cookie
//!   ([`adapter::record`](crate::adapter::record)) and reports the change to
//!   `ServiceCore::selection_changed` (matrix #19). A record that shows neither a text change
//!   nor a selection change (an edit session of another text service that only sets
//!   properties) is not reported, so it does not end the word. TSF reports the edit of one of
//!   the service's own sessions before `RequestEditSession` returns; such reports are skipped.
//! - **Mouse.** The `WH_MOUSE` hook of the TSF thread reports every button press to
//!   `ServiceCore::mouse_down` (matrix #19).
//! - **Secure fields.** The input scope of a context (`GUID_PROP_INPUTSCOPE`) is read with a
//!   read-only edit session at every focus change (`TF_ES_ASYNCDONTCARE`) and, while unknown,
//!   synchronously at a key ([`adapter::scope`](crate::adapter::scope)). A focus change
//!   forgets the answer read before, so that a field whose scope changed meanwhile counts as
//!   unread until the new read has run, also when the host makes that read wait. A password
//!   or PIN scope makes the context a secure field, and so does a scope that was not read
//!   yet: keys there follow the policy's secure-field mode (D25, matrix #29), and in the
//!   default `Bypass` mode, the only one until the TIP receives snapshots, they bypass every
//!   transformation. A scope that changes while the field keeps the focus is seen at the next
//!   focus change only.
//! - **Mode.** The keyboard open/close compartment shows the core's Vietnamese mode
//!   ([`adapter::mode`](crate::adapter::mode)): activation writes it, and so does every key
//!   call after which the mode changed (a local Ctrl+Shift toggle, D24). A change of the
//!   compartment by anyone else, such as the input indicator, sets the core's mode.
//!
//! # Not done yet
//!
//! - The IPC snapshot client thread: the text service runs on defaults v1 (Telex, no
//!   macros, no application rules, Vietnamese on) and is never connected, so it classifies
//!   hotkeys itself (D24). A macro therefore never expands yet.
//! - The TSF range-edit probe edit and its `Command::ReportProbe` (matrix #27), opt-in
//!   diagnostics, and the `evkey-compat` legacy path: keys tagged with the legacy sentinel
//!   `0xEBCA1234` are not bypassed.
//! - The activation flags (`TF_TMAE_*`, such as secure mode), a language bar item, and the
//!   input mode compartment (`GUID_COMPARTMENT_KEYBOARD_INPUTMODE_CONVERSION`).
//! - Open issue, outside this crate: `raumakey_windows::tsf_registration` registers the
//!   profile for LANGID `0x042A` with no substitute keyboard layout (`hklSubstitute` null).
//!   While Raumakey is active, the thread then probably runs on the Vietnamese system layout,
//!   whose number row types Vietnamese letters, and the text service translates keys with
//!   the thread's layout and lets digits through: that would break digits and VNI for every
//!   user. Registering a substitute layout (for example US English, `0x04090409`, or a
//!   configurable one) is a change for `raumakey-windows` and plan §10.4. Run manual check
//!   step 9 before relying on the text service for digits or VNI.
//!
//! # Threading rules
//!
//! - The class is registered with `ThreadingModel = Apartment`. TSF creates the text service
//!   and calls all its methods on the thread that owns the thread manager, a single-threaded
//!   apartment (the host's UI thread).
//! - The text service is not agile: it answers neither `IAgileObject` nor `IMarshal` (no
//!   free-threaded marshaler), so a call from another apartment goes through a COM proxy to
//!   its own thread. Its state lives in `RefCell`s that only that thread touches. The mouse
//!   hook is a hook of that thread, never a global hook.
//! - It never holds a `RefCell` borrow while it calls into TSF, because TSF calls sinks back
//!   synchronously (a new foreground key event sink gets `OnSetFocus` while
//!   `AdviseKeyEventSink` runs, an edit session reports to `OnEndEdit` before
//!   `RequestEditSession` returns, a compartment write reports to `OnChange`). A call that
//!   finds a cell in use passes through: a busy core is not reset, a key that arrives while
//!   the core is in use reaches the application, and an activation change made inside
//!   another one is refused with `HRESULT_FROM_WIN32(ERROR_BUSY)`. Nothing panics.
//! - Every export, COM method, and the hook procedure run inside a `raumakey_windows::panic`
//!   guard: a panic becomes `E_UNEXPECTED` (or a mouse message passed on unchanged) and never
//!   unwinds into TSF, Windows, or the host. Inside the DLL, the exports install the quiet
//!   panic hook, which prints nothing.
//! - The object and lock counts are atomic, so `DllCanUnloadNow` may run on any thread.
//! - Nothing is logged; the `Debug` output of the types that hold keys or text hides them.
//!
//! # Manual check list
//!
//! The in-process tests (`tests/com`) cannot show TSF activating the text service, and their
//! documents belong to no application; nothing here has run inside a Windows host application
//! yet. Check by hand on Windows 10 or later, from an elevated command prompt, with the DLL in
//! a folder that every application can read:
//!
//! 1. Register with the `regsvr32` of the DLL's bitness: on 64-bit Windows,
//!    `%SystemRoot%\System32\regsvr32.exe <folder>\x64\raumakey_tip.dll` for the x64 TIP
//!    (the ARM64 TIP on ARM64 Windows) and `%SystemRoot%\SysWOW64\regsvr32.exe
//!    <folder>\x86\raumakey_tip.dll` for the x86 TIP. `regsvr32` reports success.
//! 2. The profile appears under Settings > Time & language > Language (Language & region on
//!    Windows 11) > Vietnamese > Options (Language options) > Keyboards, as "Raumakey" with
//!    the Raumakey app icon, or can be added there. Add Vietnamese first if it is missing.
//! 3. In Notepad, switch to Raumakey and type `tieengs vieetj nguwowif`: Notepad shows
//!    "tiếng việt người", with the caret after the last word. Backspace right after `aa`
//!    (shown as "â") leaves "a".
//! 4. Tap Ctrl+Shift and type `aa`: "aa". Tap Ctrl+Shift again and type ` aa`: "aa â".
//! 5. Type `ba ba`, click right after the first "ba", and type `s`: "bas ba" (the click ended
//!    the word). The same with the arrow keys instead of the click.
//! 6. Type `a` in one Notepad window, switch to another one with Alt+Tab, and type `a`: each
//!    window shows "a"; nothing was deleted in either window.
//! 7. In a password field that declares the password input scope (for example a password
//!    field in Microsoft Edge, whose reveal button shows what was typed), type `aa`: the
//!    field holds "aa".
//! 8. Type `after`, then Enter: the line holds "after" (the word was restored) and the caret
//!    is on the next line, once; the Enter was replayed with `SendInput`. Once the IPC client
//!    delivers macros (a later step), a macro `ko` → "không" followed by Enter gives "không"
//!    and one new line.
//! 9. Tap Ctrl+Shift for English and type `1234567890-=[]`: the same characters appear. If
//!    Vietnamese letters appear instead, the profile runs on the Vietnamese system layout
//!    (the registration passes no substitute layout), which breaks VNI and digits.
//! 10. Switch between Raumakey and another keyboard (Windows+Space) several times while
//!     typing, close Notepad, and open it again: nothing crashes, and typing still works.
//! 11. `regsvr32 /u` with the same `regsvr32.exe` and path reports success, and Raumakey is
//!     gone from the keyboard list. Programs that still have the DLL loaded keep it until
//!     they exit.
//!
//! The `#[ignore]`d test `registration::the_built_dll_registers_and_unregisters_itself` in
//! `tests/com` runs steps 1 and 11 on the DLL that Cargo built and reads the registry back.

use core::ffi::c_void;
use core::hint::black_box;

use raumakey_model::identity::{TIP_CLSID, TIP_PROFILE_GUID};
use raumakey_windows::com_server::{can_unload_now, get_class_object};
use raumakey_windows::error::WinError;
use raumakey_windows::module::Module;
use raumakey_windows::panic::{guard_hresult, guard_hresult_raw, install_quiet_panic_hook};
use raumakey_windows::tsf_registration::{
    PROFILE_DESCRIPTION, PROFILE_ICON_INDEX, register_in_process, unregister_in_process,
};
use windows::core::{Error, GUID, HRESULT};

use crate::text_service;

/// [`TIP_CLSID`], the class this server serves, as a COM `GUID`.
pub static CLSID: GUID = GUID::from_u128(TIP_CLSID.to_u128());

/// [`TIP_PROFILE_GUID`], the language profile that [`DllRegisterServer`] registers, as a COM
/// `GUID`.
pub static PROFILE_GUID: GUID = GUID::from_u128(TIP_PROFILE_GUID.to_u128());

/// [`CLSID`] and [`PROFILE_GUID`] as the image stores them. Release checks (`xtask inspect`)
/// look for the binary form of both GUIDs in `raumakey_tip.dll`; reading the statics through
/// `black_box` keeps an optimized build from comparing against immediates and dropping
/// them.
fn identities() -> (&'static GUID, &'static GUID) {
    black_box((&CLSID, &PROFILE_GUID))
}

/// A COM error for a failed `raumakey-windows` call; it carries only the code.
fn com_error(error: WinError) -> Error {
    Error::from_hresult(error.hresult())
}

/// Installs the quiet panic hook of `raumakey_windows::panic` when this code runs inside
/// `raumakey_tip.dll`, so that a panic message, which may have been formatted from user
/// content, is never printed in the host process. When the library is linked into an
/// executable (a test binary), that executable keeps its own hook.
fn quiet_panics_in_the_dll() {
    if let (Ok(this), Ok(executable)) = (Module::containing_this_crate(), Module::executable())
        && this != executable
    {
        install_quiet_panic_hook();
    }
}

/// `DllGetClassObject`: the class factory of the text service for [`CLSID`].
///
/// Clears `*ppv` and returns `E_POINTER` for a null pointer, `CLASS_E_CLASSNOTAVAILABLE` for
/// any other class, and otherwise queries a new class factory for `riid` (usually
/// `IClassFactory`; `E_NOINTERFACE` for interfaces that a factory lacks). The factory
/// counts as a live object, and so does every text service it creates.
///
/// # Safety
///
/// `rclsid` and `riid` must each be null or point to a GUID; `ppv` must be null or point to
/// writable storage for an interface pointer.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    guard_hresult_raw(|| {
        quiet_panics_in_the_dll();
        let (served, _profile) = identities();
        // SAFETY: the caller's pointers are passed on under the same contract.
        unsafe { get_class_object(rclsid, riid, ppv, served, text_service::create) }
    })
}

/// `DllCanUnloadNow`: `S_OK` when no object of this server is alive and no server lock is
/// held, `S_FALSE` otherwise.
///
/// # Safety
///
/// None: the export takes no arguments.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    guard_hresult_raw(can_unload_now)
}

/// `DllRegisterServer`: registers this DLL as the COM server of [`CLSID`], the Raumakey
/// language profile, and its TSF categories, in the registry view of the DLL's bitness, and
/// rolls back on failure (`raumakey_windows::tsf_registration::register_in_process`).
/// Writes `HKEY_LOCAL_MACHINE`, so it needs an elevated caller.
///
/// # Safety
///
/// None: the export takes no arguments.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    guard_hresult(|| {
        quiet_panics_in_the_dll();
        let dll = Module::containing_this_crate()
            .and_then(Module::file_path)
            .map_err(com_error)?;
        register_in_process(&dll, PROFILE_DESCRIPTION, PROFILE_ICON_INDEX).map_err(com_error)
    })
}

/// `DllUnregisterServer`: removes the TSF categories, the language profile, and the COM class
/// of [`CLSID`] from the registry view of the DLL's bitness, touching nothing else
/// (`raumakey_windows::tsf_registration::unregister_in_process`). Needs an elevated caller.
///
/// # Safety
///
/// None: the export takes no arguments.
#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    guard_hresult(|| {
        quiet_panics_in_the_dll();
        unregister_in_process().map_err(com_error)
    })
}
