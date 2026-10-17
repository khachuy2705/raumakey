//! The text service COM object: the text input processor and every sink TSF calls, in one
//! object (plan §10.3), wired to the session core.
//!
//! The object translates each TSF callback into calls on [`ServiceCore`] and the rules of
//! [`crate::adapter`], and reads and writes TSF objects for them; everything it decides lives
//! in those modules. [`crate::server`] describes the behavior; this module keeps the
//! Win32/COM glue thin:
//!
//! - `context`: identities, the focused context, selections, text, and windows;
//! - `edit_session`: the edit of a key as one synchronous read-write edit session;
//! - `record`: the edit record of `OnEndEdit`;
//! - `scope`: the input scope of a context, read in a read-only edit session;
//! - `compartment`: the keyboard open/close compartment.
//!
//! No `RefCell` borrow is held while the object calls into TSF or sends input: TSF calls the
//! sinks back synchronously (an edit session reports its edit to `OnEndEdit` before
//! `RequestEditSession` returns, a compartment write reports to `OnChange`). A call that finds
//! a cell in use leaves it alone.

mod compartment;
mod context;
mod edit_session;
mod record;
mod scope;

use core::cell::{Cell, RefCell};
use core::sync::atomic::{AtomicU64, Ordering};
use std::rc::Rc;

use raumakey_model::host::{DocumentId, HostContext, SessionId};
use raumakey_model::input::{ReplayKey, vk};
use raumakey_windows::com_server::ObjectGuard;
use raumakey_windows::error::{E_INVALIDARG, E_UNEXPECTED, hresult_from_win32, win32};
use raumakey_windows::foreground::current_process_is_elevated;
use raumakey_windows::input::{message_extra_info, replay_inputs, send};
use raumakey_windows::keyboard::{is_altgr_layout, layout_for_thread, layout_name, translate_key};
use raumakey_windows::module::Module;
use raumakey_windows::panic::guard_value;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, GetKeyState};
use windows::Win32::UI::TextServices::{
    GUID_COMPARTMENT_KEYBOARD_OPENCLOSE, ITfCompartment, ITfCompartmentEventSink,
    ITfCompartmentEventSink_Impl, ITfCompartmentMgr, ITfContext, ITfDocumentMgr, ITfEditRecord,
    ITfKeyEventSink, ITfKeyEventSink_Impl, ITfKeystrokeMgr, ITfSource, ITfTextEditSink,
    ITfTextEditSink_Impl, ITfTextInputProcessor_Impl, ITfTextInputProcessorEx,
    ITfTextInputProcessorEx_Impl, ITfThreadFocusSink, ITfThreadFocusSink_Impl, ITfThreadMgr,
    ITfThreadMgrEventSink, ITfThreadMgrEventSink_Impl, TF_CONTEXT_EDIT_CONTEXT_FLAGS,
    TF_ES_ASYNCDONTCARE, TF_ES_SYNC,
};
use windows::core::{
    BOOL, Error, GUID, HRESULT, IUnknown, IUnknownImpl, Interface, Ref, Result, implement,
};

use self::context::{
    base_context, focused_context, identity, manager_of, top_context, window_class,
};
use self::edit_session::EditPort;
use self::record::TsfRecord;
use crate::activation::{Connections, Sink, SinkPort};
use crate::adapter::documents::Contexts;
use crate::adapter::host::{HostFacts, executable_name, session_id};
use crate::adapter::keypath::{self, with_core};
use crate::adapter::keys::{
    AltGrFilter, KeyMessage, Layout, LayoutTracker, ModifierKeys, key_text,
};
use crate::adapter::mode::{ModeMirror, mode_of};
use crate::adapter::record::observe;
use crate::mouse_hook;
use crate::session::{KeyStroke, ServiceCore};

/// Activations of text services in this process, for their session ids.
static ACTIVATIONS: AtomicU64 = AtomicU64::new(0);

/// The text service. One object is the text input processor and each of its sinks, so TSF
/// holds the same object through every interface.
///
/// It lives in the apartment that created it (`Agile = false`: no `IAgileObject`, no
/// free-threaded marshaler); TSF calls it on the thread that owns the thread manager. Its
/// state is in `RefCell`s, and it never holds a borrow across a call into TSF.
#[implement(
    ITfTextInputProcessorEx,
    ITfKeyEventSink,
    ITfThreadMgrEventSink,
    ITfTextEditSink,
    ITfThreadFocusSink,
    ITfCompartmentEventSink,
    Agile = false
)]
pub(crate) struct TextService {
    /// The current activation; `None` while the service is not activated.
    activation: RefCell<Option<Activation>>,
    /// The session core; the TSF thread owns it.
    core: RefCell<ServiceCore>,
    /// What the activation tracks for the adapter; `None` while the service is not
    /// activated.
    adapter: RefCell<Option<Adapter>>,
    /// Edit sessions of this service in progress: `OnEndEdit` skips the changes they report.
    own_edits: Cell<u32>,
    /// Counts the object for `DllCanUnloadNow`.
    _counted: ObjectGuard,
}

/// What an activation keeps until `Deactivate`.
struct Activation {
    /// The thread manager TSF activated the service with.
    thread_mgr: ITfThreadMgr,
    /// The client id TSF gave the service.
    client_id: u32,
    /// The advised sinks.
    connections: Connections<Connection>,
}

/// How one advised sink is unadvised.
enum Connection {
    /// A sink advised through `ITfSource::AdviseSink`, with the cookie it returned.
    Source { source: ITfSource, cookie: u32 },
    /// The key event sink, advised for the client id of the activation.
    KeyEvents { keystrokes: ITfKeystrokeMgr },
}

/// What the adapter tracks during one activation.
struct Adapter {
    /// The client id TSF gave the activation.
    client_id: u32,
    /// The process, the thread, and the session of the activation.
    facts: HostFacts,
    /// The keyboard open/close compartment of the thread manager.
    compartment: Option<ITfCompartment>,
    /// The contexts keys or the focus reached, with their document ids.
    contexts: Contexts,
    /// The context that has the focus, with the text edit sink.
    focus: Option<Focus>,
    /// The keyboard layout of the thread.
    layout: LayoutTracker,
    /// Drops the synthetic left Ctrl of AltGr.
    altgr: AltGrFilter,
    /// What the open/close compartment shows.
    mirror: ModeMirror,
}

/// The context that has the focus (or got the last key), and the text edit sink advised on
/// it, whose source holds the context.
struct Focus {
    /// The COM identity of the context.
    identity: usize,
    /// The COM identity of its document manager, when known.
    manager: Option<usize>,
    /// The text edit sink; `None` when the context refused it.
    sink: Option<EditSink>,
}

impl Focus {
    /// Unadvises the text edit sink and releases the context.
    fn release(self) {
        if let Some(sink) = self.sink {
            // SAFETY: the source and the cookie it returned for the sink. A failure means
            // that the context dropped the sink already.
            let _ = unsafe { sink.source.UnadviseSink(sink.cookie) };
        }
    }
}

/// An `ITfTextEditSink` advised on a context: the context's source and the cookie.
struct EditSink {
    source: ITfSource,
    cookie: u32,
}

/// What the thread's message state says about the key message being processed.
struct MessageState {
    /// The modifier keys (`GetKeyState`).
    keys: ModifierKeys,
    /// The message extra information (`GetMessageExtraInfo`), zero-extended.
    extra_info: u64,
}

/// The four calls of the key event sink.
#[derive(Clone, Copy, PartialEq, Eq)]
enum KeyCall {
    TestDown,
    Down,
    TestUp,
    Up,
}

impl KeyCall {
    fn is_down(self) -> bool {
        matches!(self, Self::TestDown | Self::Down)
    }
}

/// How a context's input scope is read.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScopeRead {
    /// Only while it is unknown, synchronously (a key is waiting for the answer).
    IfUnknown,
    /// Again, synchronously when TSF allows it and later otherwise (a focus change). The
    /// answer read before is forgotten first: until the new read has run, the field counts as
    /// unread, which a key treats as [`ScopeRead::IfUnknown`] does.
    Refresh,
}

/// The [`SinkPort`] of one text service over a thread manager.
struct ThreadMgrSinks<'a> {
    thread_mgr: &'a ITfThreadMgr,
    client_id: u32,
    /// The text service, which is every sink.
    service: &'a IUnknown,
}

impl ThreadMgrSinks<'_> {
    /// Advises the service as the `sink` interface on `source`.
    fn advise_source(&self, source: ITfSource, sink: &GUID) -> Result<Connection> {
        // SAFETY: a live source of this thread and an IID the service implements; TSF takes
        // its own reference to the service.
        let cookie = unsafe { source.AdviseSink(sink, self.service) }?;
        Ok(Connection::Source { source, cookie })
    }
}

impl SinkPort for ThreadMgrSinks<'_> {
    type Connection = Connection;
    type Error = Error;

    fn advise(&mut self, sink: Sink) -> Result<Connection> {
        match sink {
            Sink::ThreadMgrEvents => {
                self.advise_source(self.thread_mgr.cast()?, &ITfThreadMgrEventSink::IID)
            }
            Sink::ThreadFocus => {
                self.advise_source(self.thread_mgr.cast()?, &ITfThreadFocusSink::IID)
            }
            Sink::KeyEvents => {
                let keystrokes: ITfKeystrokeMgr = self.thread_mgr.cast()?;
                let sink: ITfKeyEventSink = self.service.cast()?;
                // SAFETY: a live keystroke manager and sink of this thread; TSF takes its own
                // reference to the sink.
                unsafe { keystrokes.AdviseKeyEventSink(self.client_id, &sink, true) }?;
                Ok(Connection::KeyEvents { keystrokes })
            }
            Sink::KeyboardOpenClose => {
                let compartments: ITfCompartmentMgr = self.thread_mgr.cast()?;
                // SAFETY: a live compartment manager; the GUID is a constant.
                let compartment =
                    unsafe { compartments.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) }?;
                self.advise_source(compartment.cast()?, &ITfCompartmentEventSink::IID)
            }
        }
    }

    fn unadvise(&mut self, _sink: Sink, connection: Connection) -> Result<()> {
        match connection {
            // SAFETY: the source and the cookie it returned for the sink.
            Connection::Source { source, cookie } => unsafe { source.UnadviseSink(cookie) },
            // SAFETY: the keystroke manager and the client id the sink was advised for.
            Connection::KeyEvents { keystrokes } => unsafe {
                keystrokes.UnadviseKeyEventSink(self.client_id)
            },
        }
    }
}

impl TextService {
    /// A text service that is not activated, with a fresh core.
    fn new() -> Self {
        Self {
            activation: RefCell::new(None),
            core: RefCell::new(ServiceCore::new()),
            adapter: RefCell::new(None),
            own_edits: Cell::new(0),
            _counted: ObjectGuard::new(),
        }
    }
}

/// Creates a text service for the class factory and returns its `IUnknown`.
pub(crate) fn create() -> Result<IUnknown> {
    Ok(TextService::new().into())
}

/// An error that carries only `code`, leaving the thread's COM error object alone.
fn error(code: i32) -> Error {
    Error::from_hresult(HRESULT(code))
}

/// Runs the body of a COM method; a panic becomes `E_UNEXPECTED` and never crosses the COM
/// boundary.
fn guarded<T>(body: impl FnOnce() -> Result<T>) -> Result<T> {
    guard_value(Err(error(E_UNEXPECTED)), body)
}

/// The refusal of an activation change made while another one is in progress (a call that
/// TSF makes into the service while the service is inside one of its own calls).
fn busy() -> Error {
    error(hresult_from_win32(win32::ERROR_BUSY))
}

/// The open/close compartment of a successful activation, from the connection of its sink.
fn open_close_compartment(connections: &Connections<Connection>) -> Option<ITfCompartment> {
    match connections.connection(Sink::KeyboardOpenClose) {
        Some(Connection::Source { source, .. }) => source.cast().ok(),
        _ => None,
    }
}

/// The process, thread, and a new session of an activation.
fn host_facts() -> Option<HostFacts> {
    // SAFETY: no preconditions.
    let (process_id, thread_id) = unsafe { (GetCurrentProcessId(), GetCurrentThreadId()) };
    let activation = ACTIVATIONS.fetch_add(1, Ordering::Relaxed).wrapping_add(1);
    let path = Module::executable().and_then(Module::file_path).ok();
    let path = path.as_deref().map(|path| path.to_string_lossy());
    let executable = executable_name(path.as_deref())?;
    Some(HostFacts {
        session_id: session_id(process_id, activation),
        process_id: u64::from(process_id),
        thread_id: u64::from(thread_id),
        executable,
        is_elevated: current_process_is_elevated().unwrap_or(false),
    })
}

/// The name and the AltGr flag of the layout `handle`.
fn describe_layout(handle: isize) -> (String, bool) {
    let name = layout_name(handle).unwrap_or_else(|| format!("{:08X}", (handle as u32) & 0xFFFF));
    (name, is_altgr_layout(handle))
}

/// The high bit of `GetKeyState`: the key is down for the message being processed.
fn key_is_down(virtual_key: u32) -> bool {
    // SAFETY: no preconditions; reads this thread's key state.
    i32::try_from(virtual_key).is_ok_and(|key| unsafe { GetKeyState(key) } < 0)
}

/// The low bit of `GetKeyState`: the toggle key is on.
fn key_is_toggled(virtual_key: u32) -> bool {
    // SAFETY: as above.
    i32::try_from(virtual_key).is_ok_and(|key| unsafe { GetKeyState(key) } & 1 != 0)
}

/// The physical state of right Alt, which is already down while the synthetic left Ctrl of
/// AltGr is processed.
fn right_alt_is_down() -> bool {
    // SAFETY: no preconditions; reads the asynchronous key state.
    let state = unsafe { GetAsyncKeyState(vk::RMENU as i32) };
    state < 0
}

/// Sends a replay key with `SendInput`, tagged `RAUMAKEY_INJECTED_TAG`
/// (`raumakey_windows::input`). The key was eaten already; when the system refuses the input
/// (UIPI, a locked desktop) the key is lost, which nothing here can undo.
fn send_replay(key: ReplayKey) {
    if let Ok(events) = replay_inputs(&key) {
        let _ = send(&events);
    }
}

impl TextService_Impl {
    /// `ActivateEx` and `Activate`: stores the thread manager and the client id, advises
    /// every sink of [`Sink::ALL`], then starts the adapter (see [`Self::start`]). When an
    /// advise fails, the sinks advised before it are unadvised again and its error is
    /// returned; the service then is not activated.
    fn activate(&self, thread_mgr: Ref<ITfThreadMgr>, client_id: u32) -> Result<()> {
        let thread_mgr = thread_mgr.ok().map_err(|_| error(E_INVALIDARG))?.clone();
        // TSF deactivates a service before it activates it again. Should it not, the first
        // activation ends here, so that none of its sinks stays advised; its unadvise errors
        // do not stop the new activation.
        if let Some(previous) = self.take_activation()? {
            let _ = self.end(previous);
        }
        self.reset_core();
        let service = self.as_interface::<IUnknown>();
        let mut port = ThreadMgrSinks {
            thread_mgr: &thread_mgr,
            client_id,
            service: &service,
        };
        let connections = Connections::advise_all(&mut port)?;
        let compartment = open_close_compartment(&connections);
        let activation = Activation {
            thread_mgr: thread_mgr.clone(),
            client_id,
            connections,
        };
        // No borrow was held while TSF ran; a call it made meanwhile may have activated the
        // service already, and then that activation stays.
        match self.activation.try_borrow_mut() {
            Ok(mut slot) if slot.is_none() => *slot = Some(activation),
            _ => {
                let _ = self.unadvise(activation);
                return Err(busy());
            }
        }
        self.start(&thread_mgr, client_id, compartment);
        Ok(())
    }

    /// Starts the adapter of a new activation: the session and the facts of the process,
    /// the mouse hook of the thread, the mode in the open/close compartment, and the focus
    /// of the thread manager, whose text edit sink is advised.
    fn start(
        &self,
        thread_mgr: &ITfThreadMgr,
        client_id: u32,
        compartment: Option<ITfCompartment>,
    ) {
        let Some(facts) = host_facts() else {
            return;
        };
        match self.adapter.try_borrow_mut() {
            Ok(mut adapter) => {
                *adapter = Some(Adapter {
                    client_id,
                    facts,
                    compartment,
                    contexts: Contexts::default(),
                    focus: None,
                    layout: LayoutTracker::default(),
                    altgr: AltGrFilter::default(),
                    mirror: ModeMirror::default(),
                });
            }
            Err(_) => return,
        }
        let service = self.to_object();
        let on_press: Rc<dyn Fn()> = Rc::new(move || service.mouse_pressed());
        // Without the hook, clicks that the host never reports go unnoticed; activation goes
        // on (the edit still checks the text before the caret).
        let _ = mouse_hook::install(self.owner(), on_press);
        self.sync_mode();
        let focus = focused_context(thread_mgr);
        self.set_focus(focus.as_ref());
    }

    /// `Deactivate`: ends the adapter, unadvises every sink the activation advised, also
    /// after a failure, drops the references to TSF, and gives the next activation a fresh
    /// core. Deactivating a service that is not activated only resets the core.
    fn deactivate(&self) -> Result<()> {
        let result = match self.take_activation()? {
            Some(activation) => self.end(activation),
            None => {
                self.stop();
                Ok(())
            }
        };
        self.reset_core();
        result
    }

    /// Ends `activation`: the adapter, then its sinks.
    fn end(&self, activation: Activation) -> Result<()> {
        self.stop();
        self.unadvise(activation)
    }

    /// Ends what [`Self::start`] began: removes the mouse hook, unadvises the text edit sink,
    /// and forgets the contexts.
    fn stop(&self) {
        mouse_hook::remove(self.owner());
        let adapter = self
            .adapter
            .try_borrow_mut()
            .ok()
            .and_then(|mut slot| slot.take());
        if let Some(focus) = adapter.and_then(|adapter| adapter.focus) {
            focus.release();
        }
    }

    /// Identifies this service to the mouse hook.
    fn owner(&self) -> usize {
        core::ptr::from_ref(self).addr()
    }

    /// Takes the activation out of its cell, so that no borrow is held while TSF runs.
    /// Fails with [`busy`] when an activation change is in progress.
    fn take_activation(&self) -> Result<Option<Activation>> {
        let mut slot = self.activation.try_borrow_mut().map_err(|_| busy())?;
        Ok(slot.take())
    }

    /// Unadvises every sink of `activation` and drops its references to TSF.
    ///
    /// # Errors
    ///
    /// The first error of an unadvise; every sink is unadvised all the same.
    fn unadvise(&self, mut activation: Activation) -> Result<()> {
        let service = self.as_interface::<IUnknown>();
        let mut port = ThreadMgrSinks {
            thread_mgr: &activation.thread_mgr,
            client_id: activation.client_id,
            service: &service,
        };
        activation.connections.unadvise_all(&mut port)
    }

    /// Replaces the core with a fresh one. A core in use belongs to a call that TSF
    /// interrupted with this one: it is left alone (re-entrancy passes through), and the
    /// next activation or deactivation resets it.
    fn reset_core(&self) {
        if let Ok(mut core) = self.core.try_borrow_mut() {
            *core = ServiceCore::new();
        }
    }

    /// The four key event methods: builds the key stroke and calls the core; afterwards the
    /// open/close compartment shows the core's mode.
    ///
    /// The thread's key state and the message extra information describe the message that
    /// the thread retrieved last, so they are read first: a call into TSF or the host (the
    /// document manager, the view, the input scope read) could retrieve another message.
    fn key_event(
        &self,
        context: Ref<ITfContext>,
        wparam: WPARAM,
        lparam: LPARAM,
        call: KeyCall,
    ) -> Result<BOOL> {
        let state = MessageState {
            keys: ModifierKeys::read(key_is_down, key_is_toggled),
            // Zero-extended, as `SendInput` stored it (`KeyStroke::extra_info`).
            extra_info: message_extra_info() as u64,
        };
        let message = KeyMessage::decode(wparam.0, lparam.0);
        let context = context.ok().ok();
        let eaten = match self.key_stroke(context, &message, &state, call.is_down()) {
            Some((key, client_id)) => self.call_core(&key, context, client_id, call),
            None => false,
        };
        self.sync_mode();
        Ok(BOOL::from(eaten))
    }

    /// Runs `body` on the adapter of the activation; `None` when the service is not
    /// activated or a call that uses the adapter is in progress. The borrow ends with
    /// `body`, which must not call into TSF.
    fn with_adapter<T>(&self, body: impl FnOnce(&mut Adapter) -> T) -> Option<T> {
        let mut adapter = self.adapter.try_borrow_mut().ok()?;
        Some(body(adapter.as_mut()?))
    }

    /// The key stroke of `message` in `context`, with the thread's message `state`, and the
    /// client id of the activation; `None` for a key the core must not see: the service is
    /// not activated, or the key is the synthetic left Ctrl of AltGr.
    fn key_stroke(
        &self,
        context: Option<&ITfContext>,
        message: &KeyMessage,
        state: &MessageState,
        down: bool,
    ) -> Option<(KeyStroke, u32)> {
        let layout = self.layout()?;
        let drops = self.with_adapter(|adapter| {
            adapter
                .altgr
                .drops(message, down, layout.altgr, right_alt_is_down)
        })?;
        if drops {
            return None;
        }
        let identity = context.and_then(|context| self.enter(context, ScopeRead::IfUnknown));
        let (host, client_id) = self.with_adapter(|adapter| {
            let document = identity.and_then(|identity| adapter.contexts.get(identity));
            (
                adapter.facts.context(document, &layout.name),
                adapter.client_id,
            )
        })?;
        // Only a key-down types; translating it leaves the keyboard state alone.
        let text = if down {
            key_text(translate_key(
                message.virtual_key,
                message.scan_code,
                &state.keys.snapshot(layout.altgr),
                layout.handle,
            ))
        } else {
            Vec::new()
        };
        let key = KeyStroke {
            virtual_key: message.virtual_key,
            scan_code: message.scan_code,
            extended: message.extended,
            repeat: message.repeat,
            modifiers: state.keys.modifiers(layout.altgr),
            text,
            extra_info: state.extra_info,
            host,
        };
        Some((key, client_id))
    }

    /// The core call of `call`; `false` (the key reaches the application) when the core is
    /// in use.
    fn call_core(
        &self,
        key: &KeyStroke,
        context: Option<&ITfContext>,
        client_id: u32,
        call: KeyCall,
    ) -> bool {
        match call {
            KeyCall::TestDown => with_core(&self.core, |core| core.test_key_down(key)),
            KeyCall::Down => {
                let mut document = EditPort::new(context, client_id, &self.own_edits);
                keypath::key_down(&self.core, key, &mut document, send_replay)
            }
            KeyCall::TestUp => with_core(&self.core, |core| core.test_key_up(key)),
            KeyCall::Up => with_core(&self.core, |core| core.key_up(key)),
        }
        .unwrap_or(false)
    }

    /// The keyboard layout of the thread. When it changed, the core learns whether it has
    /// AltGr.
    fn layout(&self) -> Option<Layout> {
        let handle = layout_for_thread(0);
        let (layout, changed) = self.with_adapter(|adapter| {
            let changed = adapter
                .layout
                .observe(handle, describe_layout)
                .map(|layout| layout.altgr);
            adapter
                .layout
                .current()
                .cloned()
                .map(|layout| (layout, changed))
        })??;
        if let Some(altgr) = changed {
            let _ = with_core(&self.core, |core| core.set_altgr_layout(altgr));
        }
        Some(layout)
    }

    /// Tracks `context`, which a key or the focus reached: gives it a document id (the core
    /// drops the state of a document that makes room for it), moves the text edit sink to
    /// it, reads the window class of a new context, and reads its input scope as `read`
    /// says. Returns its identity.
    fn enter(&self, context: &ITfContext, read: ScopeRead) -> Option<usize> {
        let identity = identity(context)?;
        // The document manager of a context never changes: it is read for a new context.
        let known =
            self.with_adapter(|adapter| adapter.contexts.get(identity).map(|info| info.manager))?;
        let manager = known.unwrap_or_else(|| manager_of(context));
        let entered = self.with_adapter(|adapter| adapter.contexts.enter(identity, manager))?;
        if let Some(evicted) = entered.evicted {
            let _ = with_core(&self.core, |core| core.document_closed(evicted));
        }
        if entered.new {
            let class = window_class(context);
            self.with_adapter(|adapter| {
                if let Some(info) = adapter.contexts.get_mut(identity) {
                    info.window_class = class;
                }
            });
        }
        self.follow(context, identity, manager);
        match read {
            ScopeRead::IfUnknown => {
                let unknown = self
                    .with_adapter(|adapter| {
                        adapter
                            .contexts
                            .get(identity)
                            .is_some_and(|info| info.secure.is_none())
                    })
                    .unwrap_or(false);
                if unknown {
                    self.read_scope(context, identity, TF_ES_SYNC);
                }
            }
            ScopeRead::Refresh => {
                // The answer read before may be out of date: until the new read has run, the
                // field counts as unread, so a key reads it synchronously or bypasses.
                self.with_adapter(|adapter| {
                    if let Some(info) = adapter.contexts.get_mut(identity) {
                        info.secure = None;
                    }
                });
                self.read_scope(context, identity, TF_ES_ASYNCDONTCARE);
            }
        }
        Some(identity)
    }

    /// Moves the text edit sink to `context` unless it is advised there already.
    fn follow(&self, context: &ITfContext, identity: usize, manager: Option<usize>) {
        let Some(previous) = self.with_adapter(|adapter| {
            if adapter
                .focus
                .as_ref()
                .is_some_and(|focus| focus.identity == identity)
            {
                None
            } else {
                Some(adapter.focus.take())
            }
        }) else {
            return;
        };
        let Some(previous) = previous else {
            return;
        };
        if let Some(previous) = previous {
            previous.release();
        }
        let mut focus = Some(Focus {
            identity,
            manager,
            sink: self.advise_edit_sink(context),
        });
        self.with_adapter(|adapter| {
            if adapter.focus.is_none() {
                adapter.focus = focus.take();
            }
        });
        // A focus that could not be stored is undone.
        if let Some(focus) = focus {
            focus.release();
        }
    }

    /// Advises the service as the `ITfTextEditSink` of `context`.
    fn advise_edit_sink(&self, context: &ITfContext) -> Option<EditSink> {
        let source: ITfSource = context.cast().ok()?;
        let service = self.as_interface::<IUnknown>();
        // SAFETY: a live source of this thread and an IID the service implements; TSF takes
        // its own reference to the service.
        let cookie = unsafe { source.AdviseSink(&ITfTextEditSink::IID, &*service) }.ok()?;
        Some(EditSink { source, cookie })
    }

    /// Unadvises the text edit sink: no context has the focus.
    fn unfollow(&self) {
        if let Some(focus) = self.with_adapter(|adapter| adapter.focus.take()).flatten() {
            focus.release();
        }
    }

    /// Requests a read of the input scope of `context`; its answer is recorded for the
    /// document when the session runs, synchronously or (`TF_ES_ASYNCDONTCARE`) later.
    fn read_scope(
        &self,
        context: &ITfContext,
        identity: usize,
        flags: TF_CONTEXT_EDIT_CONTEXT_FLAGS,
    ) {
        let Some((client_id, session)) =
            self.with_adapter(|adapter| (adapter.client_id, adapter.facts.session_id))
        else {
            return;
        };
        let service = self.to_object();
        let deliver = Box::new(move |secure: bool| service.scope_read(identity, session, secure));
        scope::request(context, client_id, flags, deliver);
    }

    /// Records the input scope that a session of `session` read for the context `identity`.
    fn scope_read(&self, identity: usize, session: SessionId, secure: bool) {
        self.with_adapter(|adapter| {
            if adapter.facts.session_id == session
                && let Some(info) = adapter.contexts.get_mut(identity)
            {
                info.secure = Some(secure);
            }
        });
    }

    /// The focus moved to `context` (the top context of the focused document manager), or
    /// to no context: the text edit sink follows, the input scope is read again, the key
    /// state starts again, and the core ends the words of both documents (matrix #20).
    fn set_focus(&self, context: Option<&ITfContext>) {
        if self.with_adapter(|_| ()).is_none() {
            return;
        }
        let document = match context {
            Some(context) => self
                .enter(context, ScopeRead::Refresh)
                .and_then(|identity| self.document_of(identity)),
            None => {
                self.unfollow();
                None
            }
        };
        self.reset_keys();
        let _ = with_core(&self.core, |core| core.set_focus(document));
    }

    /// The thread or the key event sink gained (`focused`) or lost the focus: keys may have
    /// been missed, so the key state starts again and the core ends the words.
    fn refocus(&self, focused: bool) {
        self.reset_keys();
        let document = if focused {
            self.focused_document()
        } else {
            None
        };
        let _ = with_core(&self.core, |core| core.set_focus(document));
    }

    /// Forgets the key transitions the AltGr filter saw.
    fn reset_keys(&self) {
        self.with_adapter(|adapter| adapter.altgr.reset());
    }

    /// The document id of the context `identity`.
    fn document_of(&self, identity: usize) -> Option<DocumentId> {
        self.with_adapter(|adapter| adapter.contexts.get(identity).map(|info| info.id))
            .flatten()
    }

    /// The document that has the focus.
    fn focused_document(&self) -> Option<DocumentId> {
        self.with_adapter(|adapter| {
            let identity = adapter.focus.as_ref()?.identity;
            adapter.contexts.get(identity).map(|info| info.id)
        })
        .flatten()
    }

    /// The host context of the focused document, for a mouse press.
    fn focused_host(&self) -> Option<HostContext> {
        self.with_adapter(|adapter| {
            let document = adapter
                .focus
                .as_ref()
                .and_then(|focus| adapter.contexts.get(focus.identity));
            let layout = adapter
                .layout
                .current()
                .map_or("", |layout| layout.name.as_str());
            adapter.facts.context(document, layout)
        })
    }

    /// A mouse button went down in the host's windows (the `WH_MOUSE` hook, matrix #19).
    fn mouse_pressed(&self) {
        if let Some(host) = self.focused_host() {
            let _ = with_core(&self.core, |core| core.mouse_down(&host));
        }
    }

    /// `OnEndEdit`: a change of `context` that this service's edit sessions did not make is
    /// reported to the core with what its edit record shows (matrix #19); a record that shows
    /// neither a text change nor a selection change is no change and is not reported. The
    /// edits of the service's own sessions are reported while they are in progress (TSF
    /// reports before `RequestEditSession` returns) and are skipped.
    fn end_edit(&self, context: Ref<ITfContext>, cookie: u32, record: Ref<ITfEditRecord>) {
        if self.own_edits.get() > 0 {
            return;
        }
        let (Ok(context), Ok(record)) = (context.ok(), record.ok()) else {
            return;
        };
        let Some(document) = identity(context).and_then(|identity| self.document_of(identity))
        else {
            return;
        };
        let observed = observe(&mut TsfRecord::new(context, record, cookie));
        if let Some(change) = observed.change() {
            let _ = with_core(&self.core, |core| core.selection_changed(document, change));
        }
    }

    /// `OnUninitDocumentMgr`: the contexts of `manager` are gone.
    fn manager_closed(&self, manager: usize) {
        let Some((closed, focus)) = self.with_adapter(|adapter| {
            (
                adapter.contexts.remove_manager(manager),
                adapter
                    .focus
                    .take_if(|focus| focus.manager == Some(manager)),
            )
        }) else {
            return;
        };
        if let Some(focus) = focus {
            focus.release();
        }
        for document in closed {
            let _ = with_core(&self.core, |core| core.document_closed(document));
        }
    }

    /// `OnPushContext`: a context pushed on the focused document manager takes the focus.
    fn context_pushed(&self, context: &ITfContext) {
        let focused_manager = self
            .with_adapter(|adapter| adapter.focus.as_ref().and_then(|focus| focus.manager))
            .flatten();
        let manager = manager_of(context);
        if manager.is_some() && manager == focused_manager {
            self.set_focus(Some(context));
        }
    }

    /// `OnPopContext`: the context is gone. When it had the focus, the context below it, the
    /// base of its document manager, takes the focus (TSF reports the pop while the popped
    /// context is still on top).
    fn context_popped(&self, context: &ITfContext) {
        let Some(popped) = identity(context) else {
            return;
        };
        let Some((closed, focus)) = self.with_adapter(|adapter| {
            (
                adapter.contexts.remove(popped),
                adapter.focus.take_if(|focus| focus.identity == popped),
            )
        }) else {
            return;
        };
        if let Some(focus) = focus {
            focus.release();
            // SAFETY: a live context of this thread.
            let below = unsafe { context.GetDocumentMgr() }
                .ok()
                .and_then(|manager| base_context(&manager))
                .filter(|base| identity(base) != Some(popped));
            self.set_focus(below.as_ref());
        }
        if let Some(document) = closed {
            let _ = with_core(&self.core, |core| core.document_closed(document));
        }
    }

    /// Shows the core's mode in the open/close compartment, unless it shows it already.
    fn sync_mode(&self) {
        let Some(mode) = with_core(&self.core, |core| core.vietnamese_mode()) else {
            return;
        };
        let Some((write, compartment, client_id)) = self.with_adapter(|adapter| {
            (
                adapter.mirror.sync(mode),
                adapter.compartment.clone(),
                adapter.client_id,
            )
        }) else {
            return;
        };
        let Some(mode) = write else {
            return;
        };
        // The write reports to `OnChange` before it returns; no borrow is held.
        let written = compartment.is_some_and(|compartment| {
            compartment::write_open_close(&compartment, client_id, mode).is_ok()
        });
        if !written {
            self.with_adapter(|adapter| adapter.mirror.write_failed());
        }
    }

    /// `OnChange` of the open/close compartment: its value sets the core's mode.
    fn compartment_changed(&self, guid: Option<GUID>) {
        if guid != Some(GUID_COMPARTMENT_KEYBOARD_OPENCLOSE) {
            return;
        }
        let Some(compartment) = self
            .with_adapter(|adapter| adapter.compartment.clone())
            .flatten()
        else {
            return;
        };
        let value = compartment::read_open_close(&compartment);
        if let Some(mode) = self
            .with_adapter(|adapter| adapter.mirror.changed(mode_of(value)))
            .flatten()
        {
            let _ = with_core(&self.core, |core| core.set_vietnamese_mode(mode));
        }
    }
}

impl ITfTextInputProcessor_Impl for TextService_Impl {
    fn Activate(&self, ptim: Ref<ITfThreadMgr>, tid: u32) -> Result<()> {
        guarded(|| self.activate(ptim, tid))
    }

    fn Deactivate(&self) -> Result<()> {
        guarded(|| self.deactivate())
    }
}

impl ITfTextInputProcessorEx_Impl for TextService_Impl {
    /// The activation flags (`TF_TMAE_*`, such as secure mode or UI-less mode) change
    /// nothing: the service has no user interface, files, or connections yet.
    fn ActivateEx(&self, ptim: Ref<ITfThreadMgr>, tid: u32, _dwflags: u32) -> Result<()> {
        guarded(|| self.activate(ptim, tid))
    }
}

impl ITfKeyEventSink_Impl for TextService_Impl {
    fn OnSetFocus(&self, fforeground: BOOL) -> Result<()> {
        guarded(|| {
            self.refocus(fforeground.as_bool());
            Ok(())
        })
    }

    fn OnTestKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guarded(|| self.key_event(pic, wparam, lparam, KeyCall::TestDown))
    }

    fn OnTestKeyUp(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guarded(|| self.key_event(pic, wparam, lparam, KeyCall::TestUp))
    }

    fn OnKeyDown(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guarded(|| self.key_event(pic, wparam, lparam, KeyCall::Down))
    }

    fn OnKeyUp(&self, pic: Ref<ITfContext>, wparam: WPARAM, lparam: LPARAM) -> Result<BOOL> {
        guarded(|| self.key_event(pic, wparam, lparam, KeyCall::Up))
    }

    fn OnPreservedKey(&self, _pic: Ref<ITfContext>, _rguid: *const GUID) -> Result<BOOL> {
        guarded(|| Ok(BOOL::from(false)))
    }
}

impl ITfThreadMgrEventSink_Impl for TextService_Impl {
    fn OnInitDocumentMgr(&self, _pdim: Ref<ITfDocumentMgr>) -> Result<()> {
        guarded(|| Ok(()))
    }

    fn OnUninitDocumentMgr(&self, pdim: Ref<ITfDocumentMgr>) -> Result<()> {
        guarded(|| {
            if let Some(manager) = pdim.ok().ok().and_then(identity) {
                self.manager_closed(manager);
            }
            Ok(())
        })
    }

    fn OnSetFocus(
        &self,
        pdimfocus: Ref<ITfDocumentMgr>,
        _pdimprevfocus: Ref<ITfDocumentMgr>,
    ) -> Result<()> {
        guarded(|| {
            let context = pdimfocus.ok().ok().and_then(top_context);
            self.set_focus(context.as_ref());
            Ok(())
        })
    }

    fn OnPushContext(&self, pic: Ref<ITfContext>) -> Result<()> {
        guarded(|| {
            if let Ok(context) = pic.ok() {
                self.context_pushed(context);
            }
            Ok(())
        })
    }

    fn OnPopContext(&self, pic: Ref<ITfContext>) -> Result<()> {
        guarded(|| {
            if let Ok(context) = pic.ok() {
                self.context_popped(context);
            }
            Ok(())
        })
    }
}

impl ITfTextEditSink_Impl for TextService_Impl {
    fn OnEndEdit(
        &self,
        pic: Ref<ITfContext>,
        ecreadonly: u32,
        peditrecord: Ref<ITfEditRecord>,
    ) -> Result<()> {
        guarded(|| {
            self.end_edit(pic, ecreadonly, peditrecord);
            Ok(())
        })
    }
}

impl ITfThreadFocusSink_Impl for TextService_Impl {
    fn OnSetThreadFocus(&self) -> Result<()> {
        guarded(|| {
            self.refocus(true);
            Ok(())
        })
    }

    fn OnKillThreadFocus(&self) -> Result<()> {
        guarded(|| {
            self.refocus(false);
            Ok(())
        })
    }
}

impl ITfCompartmentEventSink_Impl for TextService_Impl {
    fn OnChange(&self, rguid: *const GUID) -> Result<()> {
        guarded(|| {
            // SAFETY: TSF passes a pointer to the GUID of the compartment, or null.
            let guid = unsafe { rguid.as_ref() }.copied();
            self.compartment_changed(guid);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use raumakey_windows::error::{hresult_from_win32, win32};
    use windows::Win32::UI::TextServices::ITfTextInputProcessorEx;
    use windows::core::{ComObject, HRESULT};

    use super::TextService;

    #[test]
    fn deactivation_leaves_a_fresh_core_for_the_next_activation() {
        let service = ComObject::new(TextService::new());
        {
            let mut core = service.core.borrow_mut();
            core.set_connected(true);
            core.set_vietnamese_mode(false);
        }
        let processor: ITfTextInputProcessorEx = service.to_interface();
        // SAFETY: a call on a live in-process object.
        unsafe { processor.Deactivate() }.expect("Deactivate");
        let core = service.core.borrow();
        assert!(!core.is_connected());
        assert!(core.vietnamese_mode());
    }

    #[test]
    fn a_core_in_use_is_left_alone_instead_of_panicking() {
        let service = ComObject::new(TextService::new());
        let processor: ITfTextInputProcessorEx = service.to_interface();
        // A call into the core is in progress when TSF calls back.
        let mut busy = service.core.borrow_mut();
        busy.set_connected(true);
        // SAFETY: a call on a live in-process object.
        let result = unsafe { processor.Deactivate() };
        assert!(result.is_ok(), "{result:?}");
        assert!(busy.is_connected(), "the busy core was not replaced");
    }

    #[test]
    fn an_activation_change_inside_another_is_refused_instead_of_panicking() {
        let service = ComObject::new(TextService::new());
        let processor: ITfTextInputProcessorEx = service.to_interface();
        let in_progress = service.activation.borrow_mut();
        // SAFETY: a call on a live in-process object.
        let result = unsafe { processor.Deactivate() };
        assert_eq!(
            result.err().map(|error| error.code()),
            Some(HRESULT(hresult_from_win32(win32::ERROR_BUSY)))
        );
        drop(in_progress);
        // SAFETY: as above.
        unsafe { processor.Deactivate() }.expect("Deactivate once the change is over");
    }
}
