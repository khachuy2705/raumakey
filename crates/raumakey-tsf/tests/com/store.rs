//! A minimal text store of an application (`ITextStoreACP`) that declares input scopes and
//! can behave like a host that TSF's own text store does not imitate, for the secure-field
//! tests (matrix #29) and `hosts`.
//!
//! TSF's own text store, which the other tests use, has no input scope and grants every
//! lock. [`AppStore`] holds a UTF-16 buffer and a selection, grants every lock synchronously
//! (one at a time) unless a test asks otherwise, and answers `GUID_PROP_INPUTSCOPE` with an
//! `ITfInputScope` of its scopes, as an application that supports input scopes does. Its
//! text changes only through TSF (edit sessions), so it never reports changes of its own. It
//! has no layout and no window.
//!
//! What a test can ask for:
//!
//! - [`AppStore::refuse_next_sync_write`]: the next synchronous read-write lock is refused
//!   with `TS_E_SYNCHRONOUS`, as by a host that cannot grant it at once; TSF then answers
//!   the edit session with `TF_E_SYNCHRONOUS`.
//! - [`AppStore::select_inserted_text`]: `SetText` leaves the new text selected instead of
//!   moving the caret after it, so that only a text service that sets the selection itself
//!   leaves a caret after its edit.
//! - [`AppStore::defer_async_locks`]: asynchronous lock requests are answered with
//!   `TS_S_ASYNC` and granted only by [`AppStore::grant_deferred`], as by a host that is busy
//!   when TSF asks; TSF then runs those edit sessions later.
//! - [`AppStore::on_next_lock`]: an action runs at the next lock request, before the store
//!   answers it, as when the host's thread retrieves a message while TSF waits for a lock.

use core::cell::{Cell, RefCell};
use core::mem::ManuallyDrop;

use windows::Win32::Foundation::{E_INVALIDARG, E_NOTIMPL, E_UNEXPECTED, HWND, POINT, RECT, S_OK};
use windows::Win32::System::Com::{CoTaskMemAlloc, FORMATETC, IDataObject};
use windows::Win32::System::Variant::{VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_UNKNOWN};
use windows::Win32::UI::TextServices::{
    GUID_PROP_INPUTSCOPE, ITextStoreACP, ITextStoreACP_Impl, ITextStoreACPSink, ITfInputScope,
    ITfInputScope_Impl, InputScope, TEXT_STORE_LOCK_FLAGS, TS_AE_END, TS_ATTRVAL, TS_E_INVALIDPOS,
    TS_E_NOLOCK, TS_E_SYNCHRONOUS, TS_LF_READWRITE, TS_LF_SYNC, TS_RT_PLAIN, TS_RUNINFO,
    TS_S_ASYNC, TS_SELECTION_ACP, TS_SELECTIONSTYLE, TS_SS_NOHIDDENTEXT, TS_STATUS, TS_TEXTCHANGE,
};
use windows::core::{
    BOOL, BSTR, ComObject, Error, GUID, HRESULT, IUnknown, Interface, PCWSTR, PWSTR, Ref, Result,
    implement,
};

fn failure(code: HRESULT) -> Error {
    Error::from_hresult(code)
}

/// The input scopes of a field, as an application implements them.
#[implement(ITfInputScope)]
struct Scopes(Vec<i32>);

impl ITfInputScope_Impl for Scopes_Impl {
    fn GetInputScopes(&self, scopes: *mut *mut InputScope, count: *mut u32) -> Result<()> {
        let bytes = self.0.len() * size_of::<InputScope>();
        // SAFETY: a plain allocation that the caller frees with CoTaskMemFree.
        let array = unsafe { CoTaskMemAlloc(bytes.max(1)) }.cast::<InputScope>();
        if array.is_null() {
            return Err(failure(E_UNEXPECTED));
        }
        for (index, scope) in self.0.iter().enumerate() {
            // SAFETY: the allocation holds `len` scopes.
            unsafe { array.add(index).write(InputScope(*scope)) };
        }
        // SAFETY: the caller's out parameters.
        unsafe {
            *scopes = array;
            *count = u32::try_from(self.0.len()).unwrap_or(0);
        }
        Ok(())
    }

    fn GetPhrase(&self, _phrases: *mut *mut BSTR, _count: *mut u32) -> Result<()> {
        Err(failure(E_NOTIMPL))
    }

    fn GetRegularExpression(&self) -> Result<BSTR> {
        Err(failure(E_NOTIMPL))
    }

    fn GetSRGS(&self) -> Result<BSTR> {
        Err(failure(E_NOTIMPL))
    }

    fn GetXML(&self) -> Result<BSTR> {
        Err(failure(E_NOTIMPL))
    }
}

/// See the [module documentation](self).
#[implement(ITextStoreACP)]
pub struct AppStore {
    text: RefCell<Vec<u16>>,
    selection: Cell<(i32, i32)>,
    scopes: RefCell<Vec<i32>>,
    sink: RefCell<Option<ITextStoreACPSink>>,
    locked: Cell<bool>,
    /// The attributes TSF asked for, which `RetrieveRequestedAttrs` answers.
    requested: RefCell<Vec<GUID>>,
    /// Synchronous read-write lock requests still to refuse.
    refuse_writes: Cell<u32>,
    /// Lock requests refused so far.
    refused: Cell<u32>,
    /// `SetText` selects the new text ([`AppStore::select_inserted_text`]).
    selects_inserted: Cell<bool>,
    /// Asynchronous lock requests wait ([`AppStore::defer_async_locks`]).
    defers_async: Cell<bool>,
    /// The flags of the lock requests that wait, merged; `None` when none waits.
    deferred: Cell<Option<u32>>,
    /// Runs at the next lock request ([`AppStore::on_next_lock`]).
    next_lock: RefCell<Option<Box<dyn FnOnce()>>>,
}

impl AppStore {
    /// An empty field with `scopes`.
    pub fn new(scopes: &[i32]) -> ComObject<Self> {
        ComObject::new(Self {
            text: RefCell::new(Vec::new()),
            selection: Cell::new((0, 0)),
            scopes: RefCell::new(scopes.to_vec()),
            sink: RefCell::new(None),
            locked: Cell::new(false),
            requested: RefCell::new(Vec::new()),
            refuse_writes: Cell::new(0),
            refused: Cell::new(0),
            selects_inserted: Cell::new(false),
            defers_async: Cell::new(false),
            deferred: Cell::new(None),
            next_lock: RefCell::new(None),
        })
    }

    /// Refuses the next synchronous read-write lock request with `TS_E_SYNCHRONOUS`, as a
    /// host does that cannot grant it at once. Read-only locks are granted as before.
    pub fn refuse_next_sync_write(&self) {
        self.refuse_writes.set(self.refuse_writes.get() + 1);
    }

    /// The lock requests refused so far.
    pub fn refused(&self) -> u32 {
        self.refused.get()
    }

    /// Whether to refuse the lock request `flags`; counts a refusal.
    fn refuses(&self, flags: u32) -> bool {
        let sync_write = flags & TS_LF_SYNC != 0 && flags & TS_LF_READWRITE.0 == TS_LF_READWRITE.0;
        if self.locked.get() || (sync_write && self.refuse_writes.get() > 0) {
            if !self.locked.get() {
                self.refuse_writes.set(self.refuse_writes.get() - 1);
            }
            self.refused.set(self.refused.get() + 1);
            true
        } else {
            false
        }
    }

    /// From now on, `SetText` leaves the new text selected when the selection ended inside
    /// or at the end of the replaced text, as some hosts do, instead of moving the caret after
    /// the new text.
    pub fn select_inserted_text(&self) {
        self.selects_inserted.set(true);
    }

    /// The selection, as (start, end) positions.
    pub fn selection(&self) -> (i32, i32) {
        self.selection.get()
    }

    /// From now on, an asynchronous lock request waits: it is answered with `TS_S_ASYNC` and
    /// granted by [`AppStore::grant_deferred`]. Synchronous requests are answered as before.
    pub fn defer_async_locks(&self) {
        self.defers_async.set(true);
    }

    /// Whether a lock request waits.
    pub fn has_deferred(&self) -> bool {
        self.deferred.get().is_some()
    }

    /// Stops deferring and grants the lock requests that wait, as the host does once it is
    /// free: TSF runs the edit sessions that waited for them.
    pub fn grant_deferred(&self) {
        self.defers_async.set(false);
        if let Some(flags) = self.deferred.take() {
            let _ = self.grant(flags).expect("the store has a sink");
        }
    }

    /// Grants the lock `flags` at once: TSF runs its edit sessions inside `OnLockGranted`.
    /// Returns the result of the sessions.
    fn grant(&self, flags: u32) -> Result<HRESULT> {
        let sink = self
            .sink
            .borrow()
            .clone()
            .ok_or_else(|| failure(E_UNEXPECTED))?;
        self.locked.set(true);
        // SAFETY: a live sink of this thread; the lock is granted at once.
        let granted = unsafe { sink.OnLockGranted(TEXT_STORE_LOCK_FLAGS(flags & !TS_LF_SYNC)) };
        self.locked.set(false);
        Ok(granted.map_or_else(|error| error.code(), |()| S_OK))
    }

    /// Runs `action` once, at the next lock request, before the store answers it.
    pub fn on_next_lock(&self, action: impl FnOnce() + 'static) {
        *self.next_lock.borrow_mut() = Some(Box::new(action));
    }

    /// Changes the input scopes of the field and tells TSF, as an application does when a
    /// field changes its kind.
    pub fn set_scopes(&self, scopes: &[i32]) {
        *self.scopes.borrow_mut() = scopes.to_vec();
        let sink = self.sink.borrow().clone();
        if let Some(sink) = sink {
            let end = self.len();
            // SAFETY: a live sink of this thread; no lock is held.
            unsafe { sink.OnAttrsChange(0, end, &[GUID_PROP_INPUTSCOPE]) }.expect("OnAttrsChange");
        }
    }

    fn len(&self) -> i32 {
        i32::try_from(self.text.borrow().len()).unwrap_or(i32::MAX)
    }

    fn check_lock(&self) -> Result<()> {
        if self.locked.get() {
            Ok(())
        } else {
            Err(failure(TS_E_NOLOCK))
        }
    }

    /// Records the requested attributes that the store supports.
    fn request(&self, count: u32, attributes: *const GUID) {
        let mut requested = self.requested.borrow_mut();
        requested.clear();
        if attributes.is_null() {
            return;
        }
        for index in 0..count as usize {
            // SAFETY: TSF passes `count` GUIDs.
            let attribute = unsafe { *attributes.add(index) };
            if attribute == GUID_PROP_INPUTSCOPE {
                requested.push(attribute);
            }
        }
    }

    /// The input scope of the field as a VARIANT that the caller owns.
    fn scope_value(&self) -> VARIANT {
        let scope: ITfInputScope = Scopes(self.scopes.borrow().clone()).into();
        VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_UNKNOWN,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 {
                        punkVal: ManuallyDrop::new(Some(scope.into())),
                    },
                }),
            },
        }
    }
}

impl ITextStoreACP_Impl for AppStore_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Ref<IUnknown>, _dwmask: u32) -> Result<()> {
        // SAFETY: TSF passes a valid IID pointer.
        if unsafe { riid.as_ref() }.copied() != Some(ITextStoreACPSink::IID) {
            return Err(failure(E_INVALIDARG));
        }
        let sink: ITextStoreACPSink = punk.ok()?.cast()?;
        *self.sink.borrow_mut() = Some(sink);
        Ok(())
    }

    fn UnadviseSink(&self, _punk: Ref<IUnknown>) -> Result<()> {
        self.sink.borrow_mut().take();
        Ok(())
    }

    fn RequestLock(&self, dwlockflags: u32) -> Result<HRESULT> {
        let action = self.next_lock.borrow_mut().take();
        if let Some(action) = action {
            action();
        }
        if self.refuses(dwlockflags) {
            return Ok(TS_E_SYNCHRONOUS);
        }
        if dwlockflags & TS_LF_SYNC == 0 && self.defers_async.get() {
            let waiting = self.deferred.get().unwrap_or(0);
            self.deferred.set(Some(waiting | dwlockflags));
            return Ok(TS_S_ASYNC);
        }
        self.grant(dwlockflags)
    }

    fn GetStatus(&self) -> Result<TS_STATUS> {
        Ok(TS_STATUS {
            dwDynamicFlags: 0,
            dwStaticFlags: TS_SS_NOHIDDENTEXT,
        })
    }

    fn QueryInsert(
        &self,
        acpteststart: i32,
        acptestend: i32,
        _cch: u32,
        pacpresultstart: *mut i32,
        pacpresultend: *mut i32,
    ) -> Result<()> {
        // SAFETY: the caller's out parameters.
        unsafe {
            *pacpresultstart = acpteststart;
            *pacpresultend = acptestend;
        }
        Ok(())
    }

    fn GetSelection(
        &self,
        _ulindex: u32,
        ulcount: u32,
        pselection: *mut TS_SELECTION_ACP,
        pcfetched: *mut u32,
    ) -> Result<()> {
        self.check_lock()?;
        let (start, end) = self.selection.get();
        // SAFETY: the caller's out parameters, with room for `ulcount` selections.
        unsafe {
            if ulcount == 0 {
                *pcfetched = 0;
                return Ok(());
            }
            *pselection = TS_SELECTION_ACP {
                acpStart: start,
                acpEnd: end,
                style: TS_SELECTIONSTYLE {
                    ase: TS_AE_END,
                    fInterimChar: BOOL::from(false),
                },
            };
            *pcfetched = 1;
        }
        Ok(())
    }

    fn SetSelection(&self, ulcount: u32, pselection: *const TS_SELECTION_ACP) -> Result<()> {
        self.check_lock()?;
        if ulcount == 0 {
            return Ok(());
        }
        // SAFETY: TSF passes `ulcount` selections.
        let selection = unsafe { *pselection };
        let len = self.len();
        if !(0 <= selection.acpStart
            && selection.acpStart <= selection.acpEnd
            && selection.acpEnd <= len)
        {
            return Err(failure(TS_E_INVALIDPOS));
        }
        self.selection.set((selection.acpStart, selection.acpEnd));
        Ok(())
    }

    fn GetText(
        &self,
        acpstart: i32,
        acpend: i32,
        pchplain: PWSTR,
        cchplainreq: u32,
        pcchplainret: *mut u32,
        prgruninfo: *mut TS_RUNINFO,
        cruninforeq: u32,
        pcruninforet: *mut u32,
        pacpnext: *mut i32,
    ) -> Result<()> {
        self.check_lock()?;
        let text = self.text.borrow();
        let len = self.len();
        let end = if acpend == -1 { len } else { acpend };
        if !(0 <= acpstart && acpstart <= end && end <= len) {
            return Err(failure(TS_E_INVALIDPOS));
        }
        let mut count = u32::try_from(end - acpstart).unwrap_or(0);
        if cchplainreq > 0 {
            count = count.min(cchplainreq);
        }
        let start = usize::try_from(acpstart).unwrap_or(0);
        // SAFETY: the caller's buffers: `cchplainreq` units of text and `cruninforeq` runs;
        // out parameters may be null when not wanted.
        unsafe {
            if cchplainreq > 0 && !pchplain.is_null() {
                core::ptr::copy_nonoverlapping(
                    text.as_ptr().add(start),
                    pchplain.0,
                    count as usize,
                );
            }
            if !pcchplainret.is_null() {
                *pcchplainret = if cchplainreq > 0 { count } else { 0 };
            }
            if !pcruninforet.is_null() {
                *pcruninforet = 0;
            }
            if cruninforeq > 0 && !prgruninfo.is_null() && count > 0 {
                *prgruninfo = TS_RUNINFO {
                    uCount: count,
                    r#type: TS_RT_PLAIN,
                };
                if !pcruninforet.is_null() {
                    *pcruninforet = 1;
                }
            }
            if !pacpnext.is_null() {
                *pacpnext = acpstart + i32::try_from(count).unwrap_or(0);
            }
        }
        Ok(())
    }

    fn SetText(
        &self,
        _dwflags: u32,
        acpstart: i32,
        acpend: i32,
        pchtext: &PCWSTR,
        cch: u32,
    ) -> Result<TS_TEXTCHANGE> {
        self.check_lock()?;
        let len = self.len();
        if !(0 <= acpstart && acpstart <= acpend && acpend <= len) {
            return Err(failure(TS_E_INVALIDPOS));
        }
        let units: Vec<u16> = if cch == 0 || pchtext.is_null() {
            Vec::new()
        } else {
            // SAFETY: TSF passes `cch` units.
            unsafe { core::slice::from_raw_parts(pchtext.0, cch as usize) }.to_vec()
        };
        let inserted = i32::try_from(units.len()).unwrap_or(i32::MAX);
        let (start, end) = (acpstart as usize, acpend as usize);
        self.text.borrow_mut().splice(start..end, units);
        let new_end = acpstart + inserted;
        // The selection follows the text: after the replaced units it shifts, inside them it
        // goes to the end of the new text. With `select_inserted_text`, a selection that ended
        // inside or at the end of the replaced units covers the new text instead.
        let adjust = |position: i32| {
            if position >= acpend {
                position - (acpend - acpstart) + inserted
            } else if position > acpstart {
                new_end
            } else {
                position
            }
        };
        let (start, end) = self.selection.get();
        if self.selects_inserted.get() && acpstart <= end && end <= acpend {
            self.selection.set((acpstart, new_end));
        } else {
            self.selection.set((adjust(start), adjust(end)));
        }
        Ok(TS_TEXTCHANGE {
            acpStart: acpstart,
            acpOldEnd: acpend,
            acpNewEnd: new_end,
        })
    }

    fn GetFormattedText(&self, _acpstart: i32, _acpend: i32) -> Result<IDataObject> {
        Err(failure(E_NOTIMPL))
    }

    fn GetEmbedded(
        &self,
        _acppos: i32,
        _rguidservice: *const GUID,
        _riid: *const GUID,
    ) -> Result<IUnknown> {
        Err(failure(E_NOTIMPL))
    }

    fn QueryInsertEmbedded(
        &self,
        _pguidservice: *const GUID,
        _pformatetc: *const FORMATETC,
    ) -> Result<BOOL> {
        Ok(BOOL::from(false))
    }

    fn InsertEmbedded(
        &self,
        _dwflags: u32,
        _acpstart: i32,
        _acpend: i32,
        _pdataobject: Ref<IDataObject>,
    ) -> Result<TS_TEXTCHANGE> {
        Err(failure(E_NOTIMPL))
    }

    fn InsertTextAtSelection(
        &self,
        _dwflags: u32,
        _pchtext: &PCWSTR,
        _cch: u32,
        _pacpstart: *mut i32,
        _pacpend: *mut i32,
        _pchange: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        Err(failure(E_NOTIMPL))
    }

    fn InsertEmbeddedAtSelection(
        &self,
        _dwflags: u32,
        _pdataobject: Ref<IDataObject>,
        _pacpstart: *mut i32,
        _pacpend: *mut i32,
        _pchange: *mut TS_TEXTCHANGE,
    ) -> Result<()> {
        Err(failure(E_NOTIMPL))
    }

    fn RequestSupportedAttrs(
        &self,
        _dwflags: u32,
        cfilterattrs: u32,
        pafilterattrs: *const GUID,
    ) -> Result<()> {
        self.request(cfilterattrs, pafilterattrs);
        Ok(())
    }

    fn RequestAttrsAtPosition(
        &self,
        _acppos: i32,
        cfilterattrs: u32,
        pafilterattrs: *const GUID,
        _dwflags: u32,
    ) -> Result<()> {
        self.request(cfilterattrs, pafilterattrs);
        Ok(())
    }

    fn RequestAttrsTransitioningAtPosition(
        &self,
        _acppos: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const GUID,
        _dwflags: u32,
    ) -> Result<()> {
        self.requested.borrow_mut().clear();
        Ok(())
    }

    fn FindNextAttrTransition(
        &self,
        _acpstart: i32,
        acphalt: i32,
        _cfilterattrs: u32,
        _pafilterattrs: *const GUID,
        _dwflags: u32,
        pacpnext: *mut i32,
        pffound: *mut BOOL,
        plfoundoffset: *mut i32,
    ) -> Result<()> {
        // SAFETY: the caller's out parameters. The attributes never change within the text.
        unsafe {
            *pacpnext = acphalt;
            *pffound = BOOL::from(false);
            *plfoundoffset = 0;
        }
        Ok(())
    }

    fn RetrieveRequestedAttrs(
        &self,
        ulcount: u32,
        paattrvals: *mut TS_ATTRVAL,
        pcfetched: *mut u32,
    ) -> Result<()> {
        let requested = core::mem::take(&mut *self.requested.borrow_mut());
        let mut fetched = 0_u32;
        for (index, attribute) in requested.into_iter().take(ulcount as usize).enumerate() {
            let value = TS_ATTRVAL {
                idAttr: attribute,
                dwOverlapId: 0,
                varValue: self.scope_value(),
            };
            // SAFETY: TSF passes room for `ulcount` values and takes over what they own.
            unsafe { paattrvals.add(index).write(value) };
            fetched += 1;
        }
        // SAFETY: the caller's out parameter.
        unsafe { *pcfetched = fetched };
        Ok(())
    }

    fn GetEndACP(&self) -> Result<i32> {
        self.check_lock()?;
        Ok(self.len())
    }

    fn GetActiveView(&self) -> Result<u32> {
        Ok(1)
    }

    fn GetACPFromPoint(&self, _vcview: u32, _ptscreen: *const POINT, _dwflags: u32) -> Result<i32> {
        Err(failure(E_NOTIMPL))
    }

    fn GetTextExt(
        &self,
        _vcview: u32,
        _acpstart: i32,
        _acpend: i32,
        _prc: *mut RECT,
        _pfclipped: *mut BOOL,
    ) -> Result<()> {
        Err(failure(E_NOTIMPL))
    }

    fn GetScreenExt(&self, _vcview: u32) -> Result<RECT> {
        Err(failure(E_NOTIMPL))
    }

    fn GetWnd(&self, _vcview: u32) -> Result<HWND> {
        Ok(HWND::default())
    }
}
