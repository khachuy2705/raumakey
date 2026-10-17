//! A thread manager for activation tests: the real TSF thread manager of this process for
//! every sink source, with a stand-in keystroke manager.
//!
//! TSF accepts `ITfKeystrokeMgr::AdviseKeyEventSink` only with the client id of a text
//! service that TSF itself activated. For the id that `ITfThreadMgr::Activate` gives an
//! application, or one from `ITfClientId::GetClientId`, it fails with `E_INVALIDARG`
//! (observed on Windows 11; see `activation::the_real_thread_manager_refuses_the_key_sink`).
//! TSF activates only registered text services, and registering writes
//! `HKEY_LOCAL_MACHINE`, so an in-process test cannot obtain such an id.
//!
//! [`ThreadMgrShim`] therefore answers `ITfKeystrokeMgr` itself, the way TSF treats an
//! activated text service: it keeps a reference to the key event sink until it is
//! unadvised, and it calls the sink back while the text service is still inside
//! `ActivateEx` and `Deactivate`. It forwards `ITfSource` and `ITfCompartmentMgr` to the real
//! thread manager, so the other three sinks are advised with real TSF. It records every call
//! and can fail one step of activation. Of the `ITfThreadMgr` methods it forwards only
//! `GetFocus`, which activation calls to find the focused document, and records it; the
//! others fail with `E_NOTIMPL`, so that a new call shows up in these tests.

use core::cell::{Cell, RefCell};

use windows::Win32::Foundation::{
    E_ACCESSDENIED, E_FAIL, E_INVALIDARG, E_NOTIMPL, E_OUTOFMEMORY, HWND, LPARAM, WPARAM,
};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, IEnumGUID};
use windows::Win32::UI::TextServices::{
    CLSID_TF_ThreadMgr, GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, IEnumTfDocumentMgrs,
    IEnumTfFunctionProviders, ITfCompartment, ITfCompartmentEventSink, ITfCompartmentMgr,
    ITfCompartmentMgr_Impl, ITfContext, ITfDocumentMgr, ITfFunctionProvider, ITfKeyEventSink,
    ITfKeystrokeMgr, ITfKeystrokeMgr_Impl, ITfSource, ITfSource_Impl, ITfThreadFocusSink,
    ITfThreadMgr, ITfThreadMgr_Impl, ITfThreadMgrEventSink, TF_PRESERVEDKEY,
};
use windows::core::{
    BOOL, BSTR, ComObject, Error, GUID, HRESULT, IUnknown, Interface, PCWSTR, Ref, Result,
    implement,
};

/// `CONNECT_E_NOCONNECTION`: nothing is advised under that cookie or client id.
const CONNECT_E_NOCONNECTION: HRESULT = HRESULT(0x8004_0200_u32 as i32);
/// `CONNECT_E_ADVISELIMIT`: a sink is already advised.
const CONNECT_E_ADVISELIMIT: HRESULT = HRESULT(0x8004_0201_u32 as i32);

/// The real thread manager of this thread, activated, and the client id it gave this
/// "application".
pub fn activated_thread_manager() -> (ITfThreadMgr, u32) {
    // SAFETY: COM is initialized on this thread (the tests run in an STA).
    let thread_mgr: ITfThreadMgr =
        unsafe { CoCreateInstance(&CLSID_TF_ThreadMgr, None, CLSCTX_INPROC_SERVER) }
            .expect("the TSF thread manager is created");
    // SAFETY: a live thread manager of this thread.
    let client_id = unsafe { thread_mgr.Activate() }.expect("the thread manager is activated");
    (thread_mgr, client_id)
}

/// One step of activation that the shim can fail, with the error it fails with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailAt {
    /// `ITfSource::AdviseSink` for `ITfThreadMgrEventSink` fails with `E_OUTOFMEMORY`.
    ThreadMgrEvents,
    /// `ITfSource::AdviseSink` for `ITfThreadFocusSink` fails with `E_ACCESSDENIED`.
    ThreadFocus,
    /// `ITfKeystrokeMgr::AdviseKeyEventSink` fails with `E_INVALIDARG`.
    KeyEvents,
    /// `ITfCompartmentMgr::GetCompartment` fails with `E_FAIL`.
    KeyboardOpenClose,
}

impl FailAt {
    /// Every step, in the order activation reaches them.
    pub const ALL: [Self; 4] = [
        Self::ThreadMgrEvents,
        Self::ThreadFocus,
        Self::KeyEvents,
        Self::KeyboardOpenClose,
    ];

    /// The error of the failed step.
    pub fn code(self) -> HRESULT {
        match self {
            Self::ThreadMgrEvents => E_OUTOFMEMORY,
            Self::ThreadFocus => E_ACCESSDENIED,
            Self::KeyEvents => E_INVALIDARG,
            Self::KeyboardOpenClose => E_FAIL,
        }
    }
}

/// A call the shim received.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    /// `ITfSource::AdviseSink` on the thread manager, with the cookie that real TSF returned
    /// (`None` when the call failed).
    AdviseSink {
        sink: &'static str,
        cookie: Option<u32>,
    },
    /// `ITfSource::UnadviseSink` on the thread manager.
    UnadviseSink { cookie: u32 },
    /// `ITfKeystrokeMgr::AdviseKeyEventSink`.
    AdviseKeyEventSink { client_id: u32, foreground: bool },
    /// `ITfKeystrokeMgr::UnadviseKeyEventSink`.
    UnadviseKeyEventSink { client_id: u32 },
    /// `ITfCompartmentMgr::GetCompartment`; whether it asked for the keyboard open/close
    /// compartment.
    GetCompartment { keyboard_open_close: bool },
    /// `ITfThreadMgr::GetFocus`, forwarded to the real thread manager.
    GetFocus,
}

/// A call the shim made back into the text service while it was inside one of its own
/// calls: the method, its result, and, for a key test, whether the key was eaten.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Callback {
    pub method: &'static str,
    pub code: HRESULT,
    pub eaten: Option<bool>,
}

/// See the [module documentation](self).
#[implement(
    ITfThreadMgr,
    ITfSource,
    ITfCompartmentMgr,
    ITfKeystrokeMgr,
    Agile = false
)]
pub struct ThreadMgrShim {
    real: ITfThreadMgr,
    source: ITfSource,
    compartments: ITfCompartmentMgr,
    client_id: u32,
    /// The step to fail, if any.
    pub fail: Cell<Option<FailAt>>,
    /// Every call, in order.
    pub events: RefCell<Vec<Event>>,
    /// The calls back into the text service, in order.
    pub callbacks: RefCell<Vec<Callback>>,
    /// The advised key event sink, held as TSF holds it.
    pub key_sink: RefCell<Option<ITfKeyEventSink>>,
}

impl ThreadMgrShim {
    /// A shim over the activated `real` thread manager, which gave `client_id`.
    pub fn new(real: &ITfThreadMgr, client_id: u32) -> ComObject<Self> {
        ComObject::new(Self {
            real: real.clone(),
            source: real.cast().expect("the thread manager is an ITfSource"),
            compartments: real.cast().expect("the thread manager has compartments"),
            client_id,
            fail: Cell::new(None),
            events: RefCell::new(Vec::new()),
            callbacks: RefCell::new(Vec::new()),
            key_sink: RefCell::new(None),
        })
    }

    fn log(&self, event: Event) {
        self.events.borrow_mut().push(event);
    }

    fn fails(&self, step: FailAt) -> Result<()> {
        if self.fail.get() == Some(step) {
            Err(Error::from_hresult(step.code()))
        } else {
            Ok(())
        }
    }

    fn call_back(&self, method: &'static str, result: Result<Option<bool>>) {
        let (code, eaten) = match result {
            Ok(eaten) => (HRESULT(0), eaten),
            Err(error) => (error.code(), None),
        };
        self.callbacks.borrow_mut().push(Callback {
            method,
            code,
            eaten,
        });
    }
}

fn not_implemented<T>() -> Result<T> {
    Err(Error::from_hresult(E_NOTIMPL))
}

/// The name of a sink interface for the log.
fn sink_name(iid: GUID) -> &'static str {
    if iid == ITfThreadMgrEventSink::IID {
        "ITfThreadMgrEventSink"
    } else if iid == ITfThreadFocusSink::IID {
        "ITfThreadFocusSink"
    } else if iid == ITfCompartmentEventSink::IID {
        "ITfCompartmentEventSink"
    } else {
        "another sink"
    }
}

impl ITfSource_Impl for ThreadMgrShim_Impl {
    fn AdviseSink(&self, riid: *const GUID, punk: Ref<IUnknown>) -> Result<u32> {
        // SAFETY: TSF callers pass a valid IID pointer or null.
        let iid = unsafe { riid.as_ref() }
            .copied()
            .unwrap_or_else(GUID::zeroed);
        let sink = sink_name(iid);
        let step = if iid == ITfThreadMgrEventSink::IID {
            Some(FailAt::ThreadMgrEvents)
        } else if iid == ITfThreadFocusSink::IID {
            Some(FailAt::ThreadFocus)
        } else {
            None
        };
        let result = match step.map(|step| self.fails(step)) {
            Some(Err(error)) => Err(error),
            // SAFETY: the caller's IID and sink are passed on to the real source.
            _ => unsafe { self.source.AdviseSink(riid, punk.as_ref()) },
        };
        self.log(Event::AdviseSink {
            sink,
            cookie: result.as_ref().ok().copied(),
        });
        result
    }

    fn UnadviseSink(&self, dwcookie: u32) -> Result<()> {
        self.log(Event::UnadviseSink { cookie: dwcookie });
        // SAFETY: the cookie is passed on to the real source, which checks it.
        unsafe { self.source.UnadviseSink(dwcookie) }
    }
}

impl ITfCompartmentMgr_Impl for ThreadMgrShim_Impl {
    fn GetCompartment(&self, rguid: *const GUID) -> Result<ITfCompartment> {
        // SAFETY: TSF callers pass a valid GUID pointer or null.
        let guid = unsafe { rguid.as_ref() }.copied();
        self.log(Event::GetCompartment {
            keyboard_open_close: guid == Some(GUID_COMPARTMENT_KEYBOARD_OPENCLOSE),
        });
        self.fails(FailAt::KeyboardOpenClose)?;
        // SAFETY: the caller's GUID pointer is passed on to the real compartment manager.
        unsafe { self.compartments.GetCompartment(rguid) }
    }

    fn ClearCompartment(&self, tid: u32, rguid: *const GUID) -> Result<()> {
        // SAFETY: as above.
        unsafe { self.compartments.ClearCompartment(tid, rguid) }
    }

    fn EnumCompartments(&self) -> Result<IEnumGUID> {
        // SAFETY: a live compartment manager.
        unsafe { self.compartments.EnumCompartments() }
    }
}

impl ITfKeystrokeMgr_Impl for ThreadMgrShim_Impl {
    fn AdviseKeyEventSink(
        &self,
        tid: u32,
        psink: Ref<ITfKeyEventSink>,
        fforeground: BOOL,
    ) -> Result<()> {
        self.log(Event::AdviseKeyEventSink {
            client_id: tid,
            foreground: fforeground.as_bool(),
        });
        self.fails(FailAt::KeyEvents)?;
        if tid != self.client_id {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let sink = psink.ok()?.clone();
        if self.key_sink.borrow().is_some() {
            return Err(Error::from_hresult(CONNECT_E_ADVISELIMIT));
        }
        *self.key_sink.borrow_mut() = Some(sink.clone());
        // Calls into the text service while it is inside ActivateEx, as TSF calls a new
        // foreground sink: the keyboard focus, then a key test.
        // SAFETY: a live sink of this thread; the context may be null for these calls.
        let focus = unsafe { sink.OnSetFocus(true) };
        self.call_back("OnSetFocus(TRUE)", focus.map(|()| None));
        // SAFETY: as above.
        let tested =
            unsafe { sink.OnTestKeyDown(None::<&ITfContext>, WPARAM(0x41), LPARAM(0x001E_0001)) };
        self.call_back("OnTestKeyDown", tested.map(|eaten| Some(eaten.as_bool())));
        Ok(())
    }

    fn UnadviseKeyEventSink(&self, tid: u32) -> Result<()> {
        self.log(Event::UnadviseKeyEventSink { client_id: tid });
        if tid != self.client_id {
            return Err(Error::from_hresult(E_INVALIDARG));
        }
        let sink = self
            .key_sink
            .borrow_mut()
            .take()
            .ok_or_else(|| Error::from_hresult(CONNECT_E_NOCONNECTION))?;
        // The text service loses the keyboard focus while it is inside Deactivate.
        // SAFETY: a live sink of this thread.
        let focus = unsafe { sink.OnSetFocus(false) };
        self.call_back("OnSetFocus(FALSE)", focus.map(|()| None));
        Ok(())
    }

    fn GetForeground(&self) -> Result<GUID> {
        not_implemented()
    }

    fn TestKeyDown(&self, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        not_implemented()
    }

    fn TestKeyUp(&self, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        not_implemented()
    }

    fn KeyDown(&self, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        not_implemented()
    }

    fn KeyUp(&self, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        not_implemented()
    }

    fn GetPreservedKey(
        &self,
        _pic: Ref<ITfContext>,
        _pprekey: *const TF_PRESERVEDKEY,
    ) -> Result<GUID> {
        not_implemented()
    }

    fn IsPreservedKey(
        &self,
        _rguid: *const GUID,
        _pprekey: *const TF_PRESERVEDKEY,
    ) -> Result<BOOL> {
        not_implemented()
    }

    fn PreserveKey(
        &self,
        _tid: u32,
        _rguid: *const GUID,
        _prekey: *const TF_PRESERVEDKEY,
        _pchdesc: &PCWSTR,
        _cchdesc: u32,
    ) -> Result<()> {
        not_implemented()
    }

    fn UnpreserveKey(&self, _rguid: *const GUID, _pprekey: *const TF_PRESERVEDKEY) -> Result<()> {
        not_implemented()
    }

    fn SetPreservedKeyDescription(
        &self,
        _rguid: *const GUID,
        _pchdesc: &PCWSTR,
        _cchdesc: u32,
    ) -> Result<()> {
        not_implemented()
    }

    fn GetPreservedKeyDescription(&self, _rguid: *const GUID) -> Result<BSTR> {
        not_implemented()
    }

    fn SimulatePreservedKey(&self, _pic: Ref<ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        not_implemented()
    }
}

impl ITfThreadMgr_Impl for ThreadMgrShim_Impl {
    fn Activate(&self) -> Result<u32> {
        not_implemented()
    }

    fn Deactivate(&self) -> Result<()> {
        not_implemented()
    }

    fn CreateDocumentMgr(&self) -> Result<ITfDocumentMgr> {
        not_implemented()
    }

    fn EnumDocumentMgrs(&self) -> Result<IEnumTfDocumentMgrs> {
        not_implemented()
    }

    fn GetFocus(&self) -> Result<ITfDocumentMgr> {
        self.log(Event::GetFocus);
        // SAFETY: a live thread manager of this thread.
        unsafe { self.real.GetFocus() }
    }

    fn SetFocus(&self, _pdimfocus: Ref<ITfDocumentMgr>) -> Result<()> {
        not_implemented()
    }

    fn AssociateFocus(&self, _hwnd: HWND, _pdimnew: Ref<ITfDocumentMgr>) -> Result<ITfDocumentMgr> {
        not_implemented()
    }

    fn IsThreadFocus(&self) -> Result<BOOL> {
        not_implemented()
    }

    fn GetFunctionProvider(&self, _clsid: *const GUID) -> Result<ITfFunctionProvider> {
        not_implemented()
    }

    fn EnumFunctionProviders(&self) -> Result<IEnumTfFunctionProviders> {
        not_implemented()
    }

    fn GetGlobalCompartment(&self) -> Result<ITfCompartmentMgr> {
        not_implemented()
    }
}
