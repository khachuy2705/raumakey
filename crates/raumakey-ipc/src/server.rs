//! Controller-side session (plan §7.3).
//!
//! One [`ServerSession`] serves one connected client. It validates `Hello`, assigns the client
//! id in `Welcome`, sends both snapshots right after the handshake and again whenever a
//! different one (normally a new revision) is published, checks that every later frame carries
//! the assigned id, and dispatches commands to a [`CommandHandler`]. Diagnostics from the
//! client are accepted only while the last runtime snapshot sent enables them.

use std::io::{Read, Write};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use raumakey_model::diag::DiagEvent;

use crate::error::{ErrorCode, IpcError};
use crate::frame::{EncodedPayload, PROTOCOL_VERSION, Sender};
use crate::link::{Link, LinkError};
use crate::message::{
    Arch, ClientKind, Command, MacroSnapshot, Message, MessageKind, RuntimeSnapshot,
};

/// Hands out client identifiers: 1, 2, 3, … up to `u64::MAX`, then
/// [`IpcError::ClientIdsExhausted`]. Never zero, never reused. Thread-safe.
#[derive(Debug)]
pub struct ClientIdAllocator {
    /// The next identifier; zero once every identifier was handed out.
    next: AtomicU64,
}

impl ClientIdAllocator {
    /// An allocator whose first identifier is 1.
    pub const fn new() -> Self {
        Self::starting_at(NonZeroU64::MIN)
    }

    /// An allocator whose first identifier is `first`. Tests use it to reach the end of the
    /// identifier space; two allocators serving one controller would reuse identifiers.
    #[doc(hidden)]
    pub const fn starting_at(first: NonZeroU64) -> Self {
        Self {
            next: AtomicU64::new(first.get()),
        }
    }

    /// The next identifier.
    pub fn allocate(&self) -> Result<NonZeroU64, IpcError> {
        let id = self
            .next
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                // `u64::MAX` wraps to the exhausted marker after it was handed out.
                (value != 0).then_some(value.wrapping_add(1))
            })
            .map_err(|_| IpcError::ClientIdsExhausted)?;
        NonZeroU64::new(id).ok_or(IpcError::ClientIdsExhausted)
    }
}

impl Default for ClientIdAllocator {
    fn default() -> Self {
        Self::new()
    }
}

/// The state the controller publishes: the runtime snapshot and the macro table, each
/// validated and serialized once so that every session frames the same bytes.
///
/// The two parts are independent. Replacing one never serializes the other again, and a
/// macro table that one frame cannot carry (more than [`MAX_MACROS`] entries, or a body over
/// [`MAX_PAYLOAD`]) is refused with a typed error while the runtime snapshot, the handshake,
/// and the previously published table stay as they were. A new state publishes an empty
/// table (revision 0) until [`PublishedState::set_macros`] succeeds.
///
/// [`MAX_MACROS`]: crate::message::MAX_MACROS
/// [`MAX_PAYLOAD`]: crate::frame::MAX_PAYLOAD
#[derive(Clone, Debug)]
pub struct PublishedState {
    server_revision: u64,
    runtime: Published<RuntimeSnapshot>,
    macros: Published<MacroSnapshot>,
}

/// A snapshot together with its serialized body.
#[derive(Clone, Debug)]
struct Published<T> {
    snapshot: Arc<T>,
    payload: Arc<EncodedPayload>,
}

impl PublishedState {
    /// Validates and serializes `runtime`; the macro table starts empty (revision 0).
    pub fn new(server_revision: u64, runtime: Arc<RuntimeSnapshot>) -> Result<Self, IpcError> {
        let runtime = Published::runtime(runtime)?;
        let macros = Published::macros(Arc::new(MacroSnapshot::empty(0)))?;
        Ok(Self {
            server_revision,
            runtime,
            macros,
        })
    }

    /// Server state revision announced in `Welcome`.
    pub const fn server_revision(&self) -> u64 {
        self.server_revision
    }

    /// Sets the server state revision announced in `Welcome` from now on.
    pub fn set_server_revision(&mut self, server_revision: u64) {
        self.server_revision = server_revision;
    }

    /// Configuration snapshot.
    pub const fn runtime(&self) -> &Arc<RuntimeSnapshot> {
        &self.runtime.snapshot
    }

    /// Macro snapshot.
    pub const fn macros(&self) -> &Arc<MacroSnapshot> {
        &self.macros.snapshot
    }

    /// Validates, serializes, and publishes a new configuration snapshot. On error nothing
    /// changes.
    pub fn set_runtime(&mut self, runtime: Arc<RuntimeSnapshot>) -> Result<(), IpcError> {
        self.runtime = Published::runtime(runtime)?;
        Ok(())
    }

    /// Validates, serializes, and publishes a new macro table. A table that one frame cannot
    /// carry fails with [`IpcError::LimitExceeded`] or [`IpcError::Oversize`]; the previously
    /// published table then stays in place and the runtime snapshot is unaffected.
    pub fn set_macros(&mut self, macros: Arc<MacroSnapshot>) -> Result<(), IpcError> {
        self.macros = Published::macros(macros)?;
        Ok(())
    }
}

impl Published<RuntimeSnapshot> {
    fn runtime(snapshot: Arc<RuntimeSnapshot>) -> Result<Self, IpcError> {
        let payload = Arc::new(EncodedPayload::runtime_snapshot(&snapshot)?);
        Ok(Self { snapshot, payload })
    }
}

impl Published<MacroSnapshot> {
    fn macros(snapshot: Arc<MacroSnapshot>) -> Result<Self, IpcError> {
        let payload = Arc::new(EncodedPayload::macro_snapshot(&snapshot)?);
        Ok(Self { snapshot, payload })
    }
}

/// The client's `Hello`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientHello {
    /// Client process kind.
    pub client_kind: ClientKind,
    /// Client process id as reported by the client.
    pub pid: u32,
    /// Client binary architecture.
    pub arch: Arch,
    /// Protocol version requested by the client.
    pub protocol: u16,
}

/// Something the client sent after the handshake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ServerEvent {
    /// A command was executed and answered with `CommandResult`.
    ///
    /// The handler's effects stand even when the result could not be written, so the event is
    /// reported in that case too (the controller still broadcasts a mode change, for
    /// example). The session is then closed: [`ServerSession::is_closed`] is true and the next
    /// call returns [`IpcError::NotConnected`].
    Command {
        /// Request id of the command frame (echoed in the result).
        request_id: u64,
        /// The command.
        command: Command,
        /// Code returned by the handler and sent to the client.
        code: ErrorCode,
    },
    /// A typed diagnostic event; delivered only while the last runtime snapshot sent to the
    /// client enables diagnostics (others are dropped).
    Diagnostic(DiagEvent),
    /// The client pinged; `Pong` was sent.
    Ping {
        /// Request id of the ping.
        request_id: u64,
    },
    /// Reply to [`ServerSession::ping`].
    Pong {
        /// Request id returned by `ping`.
        request_id: u64,
    },
    /// The client closed the connection in an orderly way.
    Goodbye,
}

/// Executes client commands for a [`ServerSession`].
pub trait CommandHandler {
    /// Executes `command` from `client_id` and returns the result code
    /// ([`ErrorCode::OK`] on success).
    fn handle_command(&mut self, client_id: NonZeroU64, command: &Command) -> ErrorCode;
}

impl<F> CommandHandler for F
where
    F: FnMut(NonZeroU64, &Command) -> ErrorCode,
{
    fn handle_command(&mut self, client_id: NonZeroU64, command: &Command) -> ErrorCode {
        self(client_id, command)
    }
}

/// Which snapshots [`ServerSession::publish`] sent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PublishOutcome {
    /// A runtime snapshot that differs from the last one sent was sent.
    pub runtime_sent: bool,
    /// A macro snapshot that differs from the last one sent was sent.
    pub macros_sent: bool,
}

/// True when `current` differs from the body last sent (cheap when it is the same `Arc`).
fn is_new(sent: Option<&Arc<EncodedPayload>>, current: &Arc<EncodedPayload>) -> bool {
    sent.is_none_or(|sent| !Arc::ptr_eq(sent, current) && **sent != **current)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    AwaitingHello,
    Ready,
    Closed,
}

/// Server state machine for one client over any `Read + Write` transport.
///
/// Reads may time out (`WouldBlock`/`TimedOut`): [`ServerSession::serve_one`] then returns
/// `Ok(None)` and keeps partial frames. Protocol violations are reported to the client with an
/// `Error` message and close the session.
pub struct ServerSession<S> {
    link: Link<S>,
    client_id: NonZeroU64,
    state: State,
    hello: Option<ClientHello>,
    /// Body of the last runtime snapshot sent to this client.
    sent_runtime: Option<Arc<EncodedPayload>>,
    /// Body of the last macro snapshot sent to this client.
    sent_macros: Option<Arc<EncodedPayload>>,
    /// `diagnostics_enabled` of the last runtime snapshot sent to this client.
    diagnostics_enabled: bool,
    next_request_id: u64,
}

impl<S> ServerSession<S> {
    /// Identifier assigned to this client (allocate it with [`ClientIdAllocator`]).
    pub const fn client_id(&self) -> NonZeroU64 {
        self.client_id
    }

    /// The client's `Hello`, once the handshake succeeded.
    pub const fn hello(&self) -> Option<&ClientHello> {
        self.hello.as_ref()
    }

    /// True after a successful handshake and before the session closed.
    pub fn is_ready(&self) -> bool {
        self.state == State::Ready
    }

    /// True once the session closed (orderly or after an error).
    pub fn is_closed(&self) -> bool {
        self.state == State::Closed
    }

    fn take_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        id
    }

    fn ensure_ready(&self) -> Result<(), IpcError> {
        if self.state == State::Ready {
            Ok(())
        } else {
            Err(IpcError::NotConnected)
        }
    }
}

impl<S: Read + Write> ServerSession<S> {
    /// A session for a freshly accepted transport whose client will receive `client_id`.
    pub fn new(stream: S, client_id: NonZeroU64) -> Self {
        Self {
            link: Link::new(stream, Sender::Client),
            client_id,
            state: State::AwaitingHello,
            hello: None,
            sent_runtime: None,
            sent_macros: None,
            diagnostics_enabled: false,
            next_request_id: 1,
        }
    }

    /// Reads and validates `Hello`, answers `Welcome`, then sends both snapshots.
    /// Read timeouts during the handshake fail it.
    pub fn handshake(&mut self, state: &PublishedState) -> Result<ClientHello, IpcError> {
        match self.state {
            State::AwaitingHello => {}
            State::Ready => return Err(IpcError::UnexpectedMessage(MessageKind::Hello)),
            State::Closed => return Err(IpcError::NotConnected),
        }
        let frame = match self.link.recv_required() {
            Ok(frame) => frame,
            Err(error) => return Err(self.fail(error)),
        };
        let hello = match frame.message {
            Message::Hello {
                client_kind,
                pid,
                arch,
                protocol,
            } => ClientHello {
                client_kind,
                pid,
                arch,
                protocol,
            },
            Message::Error { code } => {
                self.state = State::Closed;
                return Err(IpcError::Remote(code));
            }
            other => return Err(self.fail(IpcError::UnexpectedMessage(other.kind()))),
        };
        if frame.client_id != 0 {
            return Err(self.fail(IpcError::ClientIdMismatch {
                expected: 0,
                actual: frame.client_id,
            }));
        }
        if hello.protocol != PROTOCOL_VERSION {
            return Err(self.fail(IpcError::ProtocolMismatch {
                protocol: hello.protocol,
            }));
        }
        self.hello = Some(hello);
        let welcome = Message::Welcome {
            client_id: self.client_id.get(),
            server_revision: state.server_revision,
        };
        self.write(frame.request_id, &welcome)?;
        self.state = State::Ready;
        self.send_runtime(state)?;
        self.send_macros(state)?;
        Ok(hello)
    }

    /// Sends each snapshot that differs from the one this client last received: a new
    /// revision, or new content under the same revision (a table published under the
    /// placeholder revision 0, for example). Identical snapshots are not sent again.
    pub fn publish(&mut self, state: &PublishedState) -> Result<PublishOutcome, IpcError> {
        self.ensure_ready()?;
        let mut outcome = PublishOutcome::default();
        if is_new(self.sent_runtime.as_ref(), &state.runtime.payload) {
            self.send_runtime(state)?;
            outcome.runtime_sent = true;
        }
        if is_new(self.sent_macros.as_ref(), &state.macros.payload) {
            self.send_macros(state)?;
            outcome.macros_sent = true;
        }
        Ok(outcome)
    }

    /// Tells the client that the Vietnamese/English mode changed.
    pub fn send_mode_changed(
        &mut self,
        vietnamese_mode: bool,
        revision: u64,
    ) -> Result<(), IpcError> {
        self.ensure_ready()?;
        let request_id = self.take_request_id();
        self.write(
            request_id,
            &Message::ModeChanged {
                vietnamese_mode,
                revision,
            },
        )
    }

    /// Tells the client to reset every engine session.
    pub fn send_reset_all(&mut self) -> Result<(), IpcError> {
        self.ensure_ready()?;
        let request_id = self.take_request_id();
        self.write(request_id, &Message::ResetAll)
    }

    /// Sends `Ping`; the matching [`ServerEvent::Pong`] carries the returned id.
    pub fn ping(&mut self) -> Result<u64, IpcError> {
        self.ensure_ready()?;
        let request_id = self.take_request_id();
        self.write(request_id, &Message::Ping)?;
        Ok(request_id)
    }

    /// Reads one client message and handles it: commands go to `handler` and are answered
    /// with `CommandResult`, `Ping` is answered with `Pong`. Returns `Ok(None)` when the
    /// transport timed out before a whole message arrived.
    ///
    /// A command's event is returned even when its result cannot be written, because the
    /// handler already ran; the session is closed then (see [`ServerEvent::Command`]).
    ///
    /// `Diagnostic` events are opt-in (plan §7.2): unless the last runtime snapshot sent to
    /// this client enabled diagnostics, they are dropped and reading goes on. A client may
    /// still send one that crossed a snapshot opting out, so this is not a protocol error.
    pub fn serve_one<H: CommandHandler + ?Sized>(
        &mut self,
        handler: &mut H,
    ) -> Result<Option<ServerEvent>, IpcError> {
        loop {
            self.ensure_ready()?;
            let frame = match self.link.recv() {
                Ok(Some(frame)) => frame,
                Ok(None) => return Ok(None),
                Err(error) => return Err(self.fail(error)),
            };
            let expected = self.client_id.get();
            if frame.client_id != expected {
                return Err(self.fail(IpcError::ClientIdMismatch {
                    expected,
                    actual: frame.client_id,
                }));
            }
            let request_id = frame.request_id;
            let event = match frame.message {
                Message::Command { command } => {
                    let code = handler.handle_command(self.client_id, &command);
                    self.send_command_result(request_id, code);
                    ServerEvent::Command {
                        request_id,
                        command,
                        code,
                    }
                }
                Message::Diagnostic(event) if self.diagnostics_enabled => {
                    ServerEvent::Diagnostic(event)
                }
                Message::Diagnostic(_) => continue,
                Message::Ping => {
                    self.write(request_id, &Message::Pong)?;
                    ServerEvent::Ping { request_id }
                }
                Message::Pong => ServerEvent::Pong { request_id },
                Message::Goodbye => {
                    self.state = State::Closed;
                    ServerEvent::Goodbye
                }
                Message::Error { code } => {
                    self.state = State::Closed;
                    return Err(IpcError::Remote(code));
                }
                other => return Err(self.fail(IpcError::UnexpectedMessage(other.kind()))),
            };
            return Ok(Some(event));
        }
    }

    /// Sends `Goodbye` and closes the session. Closing a closed session is a no-op.
    pub fn close(&mut self) -> Result<(), IpcError> {
        if self.state == State::Closed {
            return Ok(());
        }
        let request_id = self.take_request_id();
        let result = self.write(request_id, &Message::Goodbye);
        self.state = State::Closed;
        result
    }

    fn send_runtime(&mut self, state: &PublishedState) -> Result<(), IpcError> {
        let request_id = self.take_request_id();
        self.write_payload(request_id, &state.runtime.payload)?;
        self.sent_runtime = Some(Arc::clone(&state.runtime.payload));
        self.diagnostics_enabled = state.runtime().diagnostics_enabled;
        Ok(())
    }

    /// Answers a command that already ran. A result that cannot be written closes the
    /// session instead of failing the call, so that the caller still learns about the command.
    fn send_command_result(&mut self, request_id: u64, code: ErrorCode) {
        let result = Message::CommandResult {
            ok: code.is_ok(),
            code,
        };
        if self.write(request_id, &result).is_err() {
            self.state = State::Closed;
        }
    }

    fn send_macros(&mut self, state: &PublishedState) -> Result<(), IpcError> {
        let request_id = self.take_request_id();
        self.write_payload(request_id, &state.macros.payload)?;
        self.sent_macros = Some(Arc::clone(&state.macros.payload));
        Ok(())
    }

    fn write(&mut self, request_id: u64, message: &Message) -> Result<(), IpcError> {
        let client_id = self.frame_client_id();
        let result = self.link.send(client_id, request_id, message);
        self.after_send(result)
    }

    fn write_payload(&mut self, request_id: u64, payload: &EncodedPayload) -> Result<(), IpcError> {
        let client_id = self.frame_client_id();
        let result = self.link.send_payload(client_id, request_id, payload);
        self.after_send(result)
    }

    fn after_send(&mut self, result: Result<(), LinkError>) -> Result<(), IpcError> {
        match result {
            Ok(()) => Ok(()),
            Err(LinkError::Encode(error)) => Err(error),
            Err(LinkError::Transport(error)) => {
                self.state = State::Closed;
                Err(error)
            }
        }
    }

    /// Frames sent before a `Hello` was accepted carry client id zero; `Welcome` and every
    /// later frame carry the assigned id.
    fn frame_client_id(&self) -> u64 {
        if self.hello.is_some() {
            self.client_id.get()
        } else {
            0
        }
    }

    /// Closes the session after telling the client about a protocol violation.
    fn fail(&mut self, error: IpcError) -> IpcError {
        if self.state != State::Closed && error.is_protocol_violation() {
            let request_id = self.take_request_id();
            let client_id = self.frame_client_id();
            let reply = Message::Error {
                code: error.wire_code(),
            };
            // Best effort: the session closes either way.
            let _ = self.link.send(client_id, request_id, &reply);
        }
        self.state = State::Closed;
        error
    }
}
