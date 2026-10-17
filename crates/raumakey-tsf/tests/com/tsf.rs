//! An activated text service with documents of TSF's own text store, for the in-process tests
//! of the key path.
//!
//! A context that a document manager creates without a text store of the application is
//! backed by a text store of TSF itself: it grants synchronous edit sessions, and its ranges
//! shift, read, and replace text like those of an application. The tests play the
//! application on it: [`Document::app_types`] inserts a key's text the way an application does
//! when a key reaches it, with an edit session that the text service did not run, which TSF
//! reports to the text service's `ITfTextEditSink`. They also play TSF where it would need
//! a focused window: a test thread has none, so TSF never calls `OnSetFocus` here
//! ([`Tip::focus`] calls it).
//!
//! Only letters and Space are typed: Enter and Tab can make the engine replay a key, which the
//! text service sends with `SendInput`, and no test may send input to the desktop.

use core::cell::RefCell;
use core::mem::ManuallyDrop;

use raumakey_windows::com_server::object_count;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Variant::{
    VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_I4, VariantClear,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyboardState, MAPVK_VK_TO_VSC, MapVirtualKeyW, SetKeyboardState, VK_CONTROL, VK_LCONTROL,
};
use windows::Win32::UI::TextServices::{
    GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, ITfCompartment, ITfCompartmentMgr, ITfContext,
    ITfDocumentMgr, ITfEditSession, ITfEditSession_Impl, ITfKeyEventSink, ITfRange,
    ITfTextInputProcessorEx, ITfThreadMgr, ITfThreadMgrEventSink, TF_ANCHOR_END,
    TF_DEFAULT_SELECTION, TF_ES_READ, TF_ES_READWRITE, TF_ES_SYNC, TF_POPF_ALL, TF_SELECTION,
};
use windows::core::{ComObject, IUnknown, Interface, Result, implement};

use crate::shim::{ThreadMgrShim, activated_thread_manager};
use crate::support::{references, text_service};

/// A text service activated on the real thread manager of this thread, through the shim.
pub struct Tip {
    pub real: ITfThreadMgr,
    pub client_id: u32,
    pub shim: ComObject<ThreadMgrShim>,
    pub thread_mgr: ITfThreadMgr,
    pub service: ITfTextInputProcessorEx,
    /// The references to the service before activation.
    pub baseline: u32,
}

impl Tip {
    /// A new text service, activated after `before` ran on the real thread manager, with
    /// what `before` returned.
    pub fn activate_after<T>(before: impl FnOnce(&ITfThreadMgr, u32) -> T) -> (Self, T) {
        let (real, client_id) = activated_thread_manager();
        let prepared = before(&real, client_id);
        let shim = ThreadMgrShim::new(&real, client_id);
        let thread_mgr: ITfThreadMgr = shim.to_interface();
        let service = text_service();
        let baseline = references(&service);
        // SAFETY: a live text service and thread manager of this thread.
        unsafe { service.ActivateEx(&thread_mgr, client_id, 0) }.expect("ActivateEx");
        let tip = Self {
            real,
            client_id,
            shim,
            thread_mgr,
            service,
            baseline,
        };
        (tip, prepared)
    }

    /// A new text service, activated.
    pub fn activate() -> Self {
        Self::activate_after(|_, _| ()).0
    }

    pub fn keys(&self) -> ITfKeyEventSink {
        self.service.cast().expect("ITfKeyEventSink")
    }

    pub fn events(&self) -> ITfThreadMgrEventSink {
        self.service.cast().expect("ITfThreadMgrEventSink")
    }

    /// Moves the focus to `document`, as TSF reports it while the thread has the focus.
    pub fn focus(&self, document: Option<&Document>) {
        let manager = document.map(|document| &document.manager);
        // SAFETY: a live thread manager and document manager of this thread.
        unsafe { self.real.SetFocus(manager) }.expect("SetFocus");
        // SAFETY: a live sink of this thread; the previous focus may be null.
        unsafe { self.events().OnSetFocus(manager, None::<&ITfDocumentMgr>) }.expect("OnSetFocus");
    }

    /// The keyboard open/close compartment of the thread manager.
    fn open_close_compartment(&self) -> ITfCompartment {
        let manager: ITfCompartmentMgr = self.real.cast().expect("compartments");
        // SAFETY: a live compartment manager; the GUID is a constant.
        unsafe { manager.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) }
            .expect("the open/close compartment")
    }

    /// The number the open/close compartment holds, `None` when it holds no `VT_I4`.
    pub fn open_close(&self) -> Option<i32> {
        // SAFETY: a live compartment.
        let mut value = unsafe { self.open_close_compartment().GetValue() }.expect("GetValue");
        // SAFETY: `vt` tells which member of the union is valid; `lVal` is read only for
        // `VT_I4`.
        let number = unsafe {
            let inner = &value.Anonymous.Anonymous;
            (inner.vt == VT_I4).then(|| inner.Anonymous.lVal)
        };
        // SAFETY: a VARIANT that GetValue initialized; clearing releases what it owns.
        unsafe { VariantClear(&mut value) }.expect("VariantClear");
        number
    }

    /// Writes the open/close compartment, as the language bar or the input indicator does.
    pub fn set_open_close(&self, number: i32) {
        let value = VARIANT {
            Anonymous: VARIANT_0 {
                Anonymous: ManuallyDrop::new(VARIANT_0_0 {
                    vt: VT_I4,
                    wReserved1: 0,
                    wReserved2: 0,
                    wReserved3: 0,
                    Anonymous: VARIANT_0_0_0 { lVal: number },
                }),
            },
        };
        // SAFETY: a live compartment and a VT_I4 VARIANT, which owns nothing.
        unsafe {
            self.open_close_compartment()
                .SetValue(self.client_id, &value)
        }
        .expect("SetValue");
    }

    /// References to the service beyond those before activation.
    pub fn held(&self) -> u32 {
        references(&self.service) - self.baseline
    }

    /// Deactivates the service and checks that nothing holds it any more.
    pub fn end(self) {
        // SAFETY: a live text service of this thread.
        unsafe { self.service.Deactivate() }.expect("Deactivate");
        assert_eq!(
            references(&self.service),
            self.baseline,
            "nothing holds the service after Deactivate"
        );
        let Self {
            real,
            shim,
            thread_mgr,
            service,
            ..
        } = self;
        drop((service, thread_mgr, shim));
        assert_eq!(object_count(), 0, "no object of the server is left");
        // SAFETY: the thread manager activated by `activated_thread_manager`.
        unsafe { real.Deactivate() }.expect("the thread manager is deactivated");
    }
}

/// The body of an edit session, called with its cookie.
type SessionBody = Box<dyn FnOnce(u32)>;

/// An edit session that runs a closure, as an application or another text service would.
#[implement(ITfEditSession)]
struct Session {
    body: RefCell<Option<SessionBody>>,
}

impl ITfEditSession_Impl for Session_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        if let Some(body) = self.body.borrow_mut().take() {
            body(ec);
        }
        Ok(())
    }
}

/// The selection of `context` as a range.
fn selection(context: &ITfContext, ec: u32) -> ITfRange {
    let mut selection = [TF_SELECTION::default()];
    let mut fetched = 0;
    // SAFETY: a valid edit cookie and writable out parameters.
    unsafe { context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selection, &mut fetched) }
        .expect("GetSelection");
    assert_eq!(fetched, 1);
    // SAFETY: GetSelection filled the range; taking it moves its reference out once.
    unsafe { ManuallyDrop::take(&mut selection[0].range) }.expect("a selection range")
}

/// Makes `caret` the selection of `context`.
fn select(context: &ITfContext, ec: u32, caret: ITfRange) {
    let mut selection = [TF_SELECTION {
        range: ManuallyDrop::new(Some(caret)),
        style: Default::default(),
    }];
    // SAFETY: a valid edit cookie and a selection whose range lives for the call.
    unsafe { context.SetSelection(ec, &selection) }.expect("SetSelection");
    // SAFETY: the range was moved into the selection above and is dropped once here.
    unsafe { ManuallyDrop::drop(&mut selection[0].range) };
}

/// A document manager with one context of TSF's own text store.
pub struct Document {
    pub manager: ITfDocumentMgr,
    pub context: ITfContext,
    client_id: u32,
}

impl Document {
    pub fn new(tip: &Tip) -> Self {
        Self::create(&tip.real, tip.client_id)
    }

    /// Another context pushed on the document manager of `document`, as a document of its own.
    pub fn push_on(document: &Document) -> Self {
        Self {
            manager: document.manager.clone(),
            context: Self::push_context(&document.manager, document.client_id),
            client_id: document.client_id,
        }
    }

    /// A document of the thread manager `real` that gave `client_id`.
    pub fn create(real: &ITfThreadMgr, client_id: u32) -> Self {
        Self::create_on(real, client_id, None)
    }

    /// A document whose context has the text store `store` of an application.
    pub fn with_store(tip: &Tip, store: &IUnknown) -> Self {
        Self::create_on(&tip.real, tip.client_id, Some(store))
    }

    fn create_on(real: &ITfThreadMgr, client_id: u32, store: Option<&IUnknown>) -> Self {
        // SAFETY: a live, activated thread manager of this thread.
        let manager = unsafe { real.CreateDocumentMgr() }.expect("CreateDocumentMgr");
        let context = Self::push_context_of(&manager, client_id, store);
        Self {
            manager,
            context,
            client_id,
        }
    }

    /// Creates a context of TSF's own text store and pushes it on `manager`.
    pub fn push_context(manager: &ITfDocumentMgr, client_id: u32) -> ITfContext {
        Self::push_context_of(manager, client_id, None)
    }

    /// Creates a context of `store`, or of TSF's own text store, and pushes it on `manager`.
    fn push_context_of(
        manager: &ITfDocumentMgr,
        client_id: u32,
        store: Option<&IUnknown>,
    ) -> ITfContext {
        let mut context = None;
        let mut cookie = 0;
        // SAFETY: a live document manager, a live text store or none, and writable out
        // parameters.
        unsafe { manager.CreateContext(client_id, 0, store, &mut context, &mut cookie) }
            .expect("CreateContext");
        let context = context.expect("a context");
        // SAFETY: a live document manager and a context it created.
        unsafe { manager.Push(&context) }.expect("Push");
        context
    }

    /// Runs `body` in an edit session of `context` with `flags`.
    fn run(
        context: &ITfContext,
        client_id: u32,
        read_write: bool,
        body: impl FnOnce(u32) + 'static,
    ) {
        let session: ITfEditSession = ComObject::new(Session {
            body: RefCell::new(Some(Box::new(body))),
        })
        .to_interface();
        let flags = if read_write {
            TF_ES_SYNC | TF_ES_READWRITE
        } else {
            TF_ES_SYNC | TF_ES_READ
        };
        // SAFETY: a live context and session of this thread.
        let result = unsafe { context.RequestEditSession(client_id, &session, flags) }
            .expect("RequestEditSession");
        assert!(result.is_ok(), "the edit session ran: {result:?}");
    }

    /// The text of the document.
    pub fn text(&self) -> String {
        let text = std::rc::Rc::new(RefCell::new(String::new()));
        let out = std::rc::Rc::clone(&text);
        let context = self.context.clone();
        Self::run(&self.context, self.client_id, false, move |ec| {
            // SAFETY: a valid edit cookie of a read session.
            let (start, end) = unsafe { (context.GetStart(ec), context.GetEnd(ec)) };
            let (start, end) = (start.expect("GetStart"), end.expect("GetEnd"));
            // SAFETY: as above.
            unsafe { start.ShiftEndToRange(ec, &end, TF_ANCHOR_END) }.expect("ShiftEndToRange");
            let mut buffer = vec![0_u16; 256];
            let mut count = 0;
            // SAFETY: as above, with a writable buffer.
            unsafe { start.GetText(ec, 0, &mut buffer, &mut count) }.expect("GetText");
            *out.borrow_mut() = String::from_utf16_lossy(&buffer[..count as usize]);
        });
        text.borrow().clone()
    }

    /// The application inserts `text` at the selection and puts the caret after it, as it
    /// does with a key that reaches it.
    pub fn app_types(&self, text: &str) {
        let units: Vec<u16> = text.encode_utf16().collect();
        let context = self.context.clone();
        Self::run(&self.context, self.client_id, true, move |ec| {
            let range = selection(&context, ec);
            // SAFETY: a valid read-write cookie.
            unsafe { range.SetText(ec, 0, &units) }.expect("SetText");
            // SAFETY: as above.
            unsafe { range.Collapse(ec, TF_ANCHOR_END) }.expect("Collapse");
            select(&context, ec, range);
        });
    }

    /// The application moves the caret `units` from the start of the text, as a click or
    /// an arrow key does.
    pub fn app_moves_caret(&self, units: i32) {
        let context = self.context.clone();
        Self::run(&self.context, self.client_id, true, move |ec| {
            // SAFETY: a valid read-write cookie.
            let caret = unsafe { context.GetStart(ec) }.expect("GetStart");
            let mut moved = 0;
            // SAFETY: as above; no halt condition.
            unsafe { caret.ShiftEnd(ec, units, &mut moved, core::ptr::null()) }.expect("ShiftEnd");
            // SAFETY: as above.
            unsafe { caret.ShiftStart(ec, units, &mut moved, core::ptr::null()) }
                .expect("ShiftStart");
            select(&context, ec, caret);
        });
    }

    /// Pops every context: TSF reports `OnPopContext` and `OnUninitDocumentMgr`.
    pub fn close(&self) {
        // SAFETY: a live document manager.
        unsafe { self.manager.Pop(TF_POPF_ALL) }.expect("Pop");
    }
}

/// `lParam` of a key event with `scan_code`: a first key-down, or a key-up.
fn lparam(scan_code: u32, up: bool) -> LPARAM {
    let mut flags = 1 | (scan_code << 16);
    if up {
        flags |= 0xC000_0000;
    }
    LPARAM(flags as i32 as isize)
}

fn scan_code(virtual_key: u32) -> u32 {
    // SAFETY: no preconditions; reads the layout of this thread.
    unsafe { MapVirtualKeyW(virtual_key, MAPVK_VK_TO_VSC) }
}

/// `wParam` and `lParam` of a first key-down (or, when `up`, a key-up) of `virtual_key`.
pub fn key_message(virtual_key: u32, up: bool) -> (WPARAM, LPARAM) {
    (
        WPARAM(virtual_key as usize),
        lparam(scan_code(virtual_key), up),
    )
}

/// Puts the Ctrl keys (`VK_CONTROL`, `VK_LCONTROL`) of this thread's key state down or up,
/// as the state is while the thread processes keys typed with Ctrl held, or not.
/// `SetKeyboardState` changes the key state of the calling thread only; it sends no input.
pub fn hold_ctrl(down: bool) {
    let mut state = [0_u8; 256];
    // SAFETY: a writable buffer of 256 key states.
    unsafe { GetKeyboardState(&mut state) }.expect("GetKeyboardState");
    for key in [VK_CONTROL, VK_LCONTROL] {
        state[usize::from(key.0)] = if down { 0x80 } else { 0 };
    }
    // SAFETY: 256 key states for this thread.
    unsafe { SetKeyboardState(&state) }.expect("SetKeyboardState");
}

/// A key-down in `context` as TSF delivers it: `OnTestKeyDown`, then `OnKeyDown` when the
/// test claimed the key. Returns (claimed, eaten).
pub fn press(tip: &Tip, context: Option<&ITfContext>, virtual_key: u32) -> (bool, bool) {
    let keys = tip.keys();
    let (wparam, lparam) = key_message(virtual_key, false);
    // SAFETY: a live sink of this thread; the context may be null.
    let claimed = unsafe { keys.OnTestKeyDown(context, wparam, lparam) }
        .expect("OnTestKeyDown")
        .as_bool();
    let eaten = if claimed {
        // SAFETY: as above.
        unsafe { keys.OnKeyDown(context, wparam, lparam) }
            .expect("OnKeyDown")
            .as_bool()
    } else {
        false
    };
    (claimed, eaten)
}

/// A key-up as TSF delivers it. Returns whether it was eaten.
pub fn release(tip: &Tip, context: Option<&ITfContext>, virtual_key: u32) -> bool {
    let keys = tip.keys();
    let (wparam, lparam) = key_message(virtual_key, true);
    // SAFETY: a live sink of this thread; the context may be null.
    let claimed = unsafe { keys.OnTestKeyUp(context, wparam, lparam) }
        .expect("OnTestKeyUp")
        .as_bool();
    if claimed {
        // SAFETY: as above.
        unsafe { keys.OnKeyUp(context, wparam, lparam) }
            .expect("OnKeyUp")
            .as_bool()
    } else {
        false
    }
}

/// Types `text` (lowercase ASCII letters and spaces; the key state of a test thread holds no
/// Shift) in `document`: each key goes through the text service, and a key it does not eat
/// reaches the application, which inserts its text.
pub fn type_text(tip: &Tip, document: &Document, text: &str) {
    for c in text.chars() {
        let virtual_key = match c {
            ' ' => 0x20,
            c if c.is_ascii_lowercase() => u32::from(c.to_ascii_uppercase()),
            _ => panic!("only lowercase letters and spaces are typed here"),
        };
        let (_, eaten) = press(tip, Some(&document.context), virtual_key);
        if !eaten {
            document.app_types(&c.to_string());
        }
        assert!(!release(tip, Some(&document.context), virtual_key));
    }
}
