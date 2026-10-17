//! TIP-side session (plan §7.3).
//!
//! A [`ClientSession`] outlives individual connections: it keeps the last valid snapshots and
//! the reconnect [`Backoff`] while connections come and go. Each connection performs the
//! handshake `Hello` → `Welcome` → `RuntimeSnapshot` + `MacroSnapshot`, and every later frame
//! must carry the assigned client id.

use std::io::{Read, Write};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::Duration;

use raumakey_model::diag::DiagEvent;

use crate::backoff::Backoff;
use crate::error::{ErrorCode, IpcError, MalformedReason};
use crate::frame::{PROTOCOL_VERSION, Sender};
use crate::link::{Link, LinkError};
use crate::message::{Arch, ClientKind, Command, MacroSnapshot, Message, RuntimeSnapshot};

/// Who the client is, as announced in `Hello`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ClientIdentity {
    /// Client process kind.
    pub client_kind: ClientKind,
    /// Client process id.
    pub pid: u32,
    /// Client binary architecture.
    pub arch: Arch,
}

impl ClientIdentity {
    /// The identity of the running process.
    pub fn current(client_kind: ClientKind) -> Self {
        Self {
            client_kind,
            pid: std::process::id(),
            arch: Arch::current(),
        }
    }
}

/// Result of a successful handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectInfo {
    /// Identifier assigned by the server.
    pub client_id: NonZeroU64,
    /// Server state revision at the time of the handshake.
    pub server_revision: u64,
}

/// Something the server sent after the handshake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientEvent {
    /// A new configuration snapshot (already stored as the last valid one).
    RuntimeSnapshot(Arc<RuntimeSnapshot>),
    /// A new macro snapshot (already stored as the last valid one).
    MacroSnapshot(Arc<MacroSnapshot>),
    /// The Vietnamese/English mode changed.
    ModeChanged {
        /// New mode.
        vietnamese_mode: bool,
        /// Server state revision of the change.
        revision: u64,
    },
    /// Reset every engine session without changing committed text.
    ResetAll,
    /// Outcome of a command sent with [`ClientSession::send_command`].
    CommandResult {
        /// Request id returned by `send_command`.
        request_id: u64,
        /// The command succeeded.
        ok: bool,
        /// Result code.
        code: ErrorCode,
    },
    /// Reply to [`ClientSession::ping`].
    Pong {
        /// Request id returned by `ping`.
        request_id: u64,
    },
    /// The server closed the connection in an orderly way; the session is disconnected.
    Goodbye,
}

struct Connection<S> {
    link: Link<S>,
    client_id: NonZeroU64,
    server_revision: u64,
}

struct Handshake {
    info: ConnectInfo,
    runtime: Arc<RuntimeSnapshot>,
    macros: Arc<MacroSnapshot>,
}

/// Client state machine over any `Read + Write` transport.
///
/// Reads may time out (`WouldBlock`/`TimedOut`): [`ClientSession::recv`] then returns
/// `Ok(None)` and keeps partial frames. Every error from `recv`, `connect`, or a send drops
/// the connection; snapshots survive until newer valid ones arrive.
pub struct ClientSession<S> {
    identity: ClientIdentity,
    connection: Option<Connection<S>>,
    next_request_id: u64,
    runtime: Option<Arc<RuntimeSnapshot>>,
    macros: Option<Arc<MacroSnapshot>>,
    vietnamese_mode: Option<bool>,
    backoff: Backoff,
}

impl<S> ClientSession<S> {
    /// A disconnected session.
    pub fn new(identity: ClientIdentity) -> Self {
        Self {
            identity,
            connection: None,
            next_request_id: 1,
            runtime: None,
            macros: None,
            vietnamese_mode: None,
            backoff: Backoff::new(),
        }
    }

    /// The identity announced in `Hello`.
    pub const fn identity(&self) -> ClientIdentity {
        self.identity
    }

    /// True while a handshaken connection is live.
    pub const fn is_connected(&self) -> bool {
        self.connection.is_some()
    }

    /// Identifier assigned by the server for the live connection.
    pub fn client_id(&self) -> Option<NonZeroU64> {
        self.connection.as_ref().map(|c| c.client_id)
    }

    /// Server state revision announced in `Welcome` for the live connection.
    pub fn server_revision(&self) -> Option<u64> {
        self.connection.as_ref().map(|c| c.server_revision)
    }

    /// Last valid configuration snapshot, kept across reconnects.
    pub fn runtime(&self) -> Option<Arc<RuntimeSnapshot>> {
        self.runtime.clone()
    }

    /// Last valid macro snapshot, kept across reconnects.
    pub fn macros(&self) -> Option<Arc<MacroSnapshot>> {
        self.macros.clone()
    }

    /// Latest Vietnamese mode reported by the server.
    pub const fn vietnamese_mode(&self) -> Option<bool> {
        self.vietnamese_mode
    }

    /// Reconnect backoff state.
    pub const fn backoff(&self) -> &Backoff {
        &self.backoff
    }

    /// Records a failed connection attempt and returns how long to wait before the next.
    /// The backoff resets on every successful handshake.
    pub fn next_reconnect_delay(&mut self) -> Duration {
        self.backoff.next_delay()
    }

    /// Drops the connection without a `Goodbye`; snapshots are kept.
    pub fn disconnect(&mut self) {
        self.connection = None;
    }

    fn take_request_id(&mut self) -> u64 {
        let id = self.next_request_id;
        self.next_request_id = self.next_request_id.wrapping_add(1).max(1);
        id
    }

    fn store_runtime(&mut self, snapshot: Arc<RuntimeSnapshot>) {
        self.vietnamese_mode = Some(snapshot.vietnamese_mode);
        self.runtime = Some(snapshot);
    }
}

impl<S: Read + Write> ClientSession<S> {
    /// Performs the handshake over a freshly connected transport, replacing any live
    /// connection. Read timeouts during the handshake fail it.
    pub fn connect(&mut self, stream: S) -> Result<ConnectInfo, IpcError> {
        self.connection = None;
        let mut link = Link::new(stream, Sender::Server);
        let hello_id = self.take_request_id();
        let mut assigned = None;
        match handshake(&mut link, self.identity, hello_id, &mut assigned) {
            Ok(done) => {
                self.store_runtime(done.runtime);
                self.macros = Some(done.macros);
                self.connection = Some(Connection {
                    link,
                    client_id: done.info.client_id,
                    server_revision: done.info.server_revision,
                });
                self.backoff.reset();
                Ok(done.info)
            }
            Err(error) => {
                if error.is_protocol_violation() {
                    let reply = Message::Error {
                        code: error.wire_code(),
                    };
                    // Once a valid `Welcome` assigned an id, the server checks every frame
                    // against it; before that the report carries zero, like `Hello`.
                    let client_id = assigned.map_or(0, NonZeroU64::get);
                    // Best effort: the connection is dropped either way.
                    let _ = link.send(client_id, hello_id, &reply);
                }
                Err(error)
            }
        }
    }

    /// Reads until one event is available. Answers `Ping` automatically. Returns `Ok(None)`
    /// when the transport timed out before a whole message arrived.
    pub fn recv(&mut self) -> Result<Option<ClientEvent>, IpcError> {
        loop {
            let connection = self.connection.as_mut().ok_or(IpcError::NotConnected)?;
            let expected = connection.client_id.get();
            let frame = match connection.link.recv() {
                Ok(Some(frame)) => frame,
                Ok(None) => return Ok(None),
                Err(error) => return Err(self.fail(error)),
            };
            if frame.client_id != expected {
                return Err(self.fail(IpcError::ClientIdMismatch {
                    expected,
                    actual: frame.client_id,
                }));
            }
            let request_id = frame.request_id;
            let event = match frame.message {
                Message::RuntimeSnapshot(snapshot) => {
                    let snapshot = Arc::new(snapshot);
                    self.store_runtime(Arc::clone(&snapshot));
                    ClientEvent::RuntimeSnapshot(snapshot)
                }
                Message::MacroSnapshot(snapshot) => {
                    let snapshot = Arc::new(snapshot);
                    self.macros = Some(Arc::clone(&snapshot));
                    ClientEvent::MacroSnapshot(snapshot)
                }
                Message::ModeChanged {
                    vietnamese_mode,
                    revision,
                } => {
                    self.vietnamese_mode = Some(vietnamese_mode);
                    ClientEvent::ModeChanged {
                        vietnamese_mode,
                        revision,
                    }
                }
                Message::ResetAll => ClientEvent::ResetAll,
                Message::CommandResult { ok, code } => ClientEvent::CommandResult {
                    request_id,
                    ok,
                    code,
                },
                Message::Pong => ClientEvent::Pong { request_id },
                Message::Ping => {
                    self.send(request_id, &Message::Pong)?;
                    continue;
                }
                Message::Goodbye => {
                    self.connection = None;
                    ClientEvent::Goodbye
                }
                Message::Error { code } => {
                    self.connection = None;
                    return Err(IpcError::Remote(code));
                }
                other => return Err(self.fail(IpcError::UnexpectedMessage(other.kind()))),
            };
            return Ok(Some(event));
        }
    }

    /// Sends a command; the matching [`ClientEvent::CommandResult`] carries the returned id.
    pub fn send_command(&mut self, command: Command) -> Result<u64, IpcError> {
        self.send_request(&Message::Command { command })
    }

    /// Sends a typed diagnostic event when the latest snapshot says the user opted in.
    /// Returns `Ok(false)` without sending anything otherwise.
    pub fn send_diagnostic(&mut self, event: &DiagEvent) -> Result<bool, IpcError> {
        if self.connection.is_none() {
            return Err(IpcError::NotConnected);
        }
        let opted_in = self
            .runtime
            .as_ref()
            .is_some_and(|snapshot| snapshot.diagnostics_enabled);
        if !opted_in {
            return Ok(false);
        }
        self.send_request(&Message::Diagnostic(event.clone()))?;
        Ok(true)
    }

    /// Sends `Ping`; the matching [`ClientEvent::Pong`] carries the returned id.
    pub fn ping(&mut self) -> Result<u64, IpcError> {
        self.send_request(&Message::Ping)
    }

    /// Sends `Goodbye` and drops the connection. Closing a disconnected session is a no-op.
    pub fn close(&mut self) -> Result<(), IpcError> {
        let request_id = self.take_request_id();
        let Some(mut connection) = self.connection.take() else {
            return Ok(());
        };
        connection
            .link
            .send(connection.client_id.get(), request_id, &Message::Goodbye)
            .map_err(LinkError::into_error)
    }

    fn send_request(&mut self, message: &Message) -> Result<u64, IpcError> {
        if self.connection.is_none() {
            return Err(IpcError::NotConnected);
        }
        let request_id = self.take_request_id();
        self.send(request_id, message)?;
        Ok(request_id)
    }

    fn send(&mut self, request_id: u64, message: &Message) -> Result<(), IpcError> {
        let connection = self.connection.as_mut().ok_or(IpcError::NotConnected)?;
        match connection
            .link
            .send(connection.client_id.get(), request_id, message)
        {
            Ok(()) => Ok(()),
            Err(LinkError::Encode(error)) => Err(error),
            Err(LinkError::Transport(error)) => {
                self.connection = None;
                Err(error)
            }
        }
    }

    /// Drops the connection after telling the server about a protocol violation.
    fn fail(&mut self, error: IpcError) -> IpcError {
        let request_id = self.take_request_id();
        if let Some(mut connection) = self.connection.take()
            && error.is_protocol_violation()
        {
            let reply = Message::Error {
                code: error.wire_code(),
            };
            // Best effort: the connection is dropped either way.
            let _ = connection
                .link
                .send(connection.client_id.get(), request_id, &reply);
        }
        error
    }
}

/// Runs the client side of the handshake. `assigned` receives the client id as soon as a
/// valid `Welcome` arrived, so that a later failure can be reported with it.
fn handshake<S: Read + Write>(
    link: &mut Link<S>,
    identity: ClientIdentity,
    hello_id: u64,
    assigned: &mut Option<NonZeroU64>,
) -> Result<Handshake, IpcError> {
    let hello = Message::Hello {
        client_kind: identity.client_kind,
        pid: identity.pid,
        arch: identity.arch,
        protocol: PROTOCOL_VERSION,
    };
    link.send(0, hello_id, &hello)
        .map_err(LinkError::into_error)?;
    let welcome = link.recv_required()?;
    let (client_id, server_revision) = match welcome.message {
        Message::Welcome {
            client_id,
            server_revision,
        } => (client_id, server_revision),
        Message::Error { code } => return Err(IpcError::Remote(code)),
        other => return Err(IpcError::UnexpectedMessage(other.kind())),
    };
    if welcome.client_id != client_id {
        return Err(IpcError::ClientIdMismatch {
            expected: client_id,
            actual: welcome.client_id,
        });
    }
    let client_id =
        NonZeroU64::new(client_id).ok_or(IpcError::Malformed(MalformedReason::InvalidClientId))?;
    *assigned = Some(client_id);
    let mut runtime = None;
    let mut macros = None;
    loop {
        if let (Some(runtime), Some(macros)) = (&runtime, &macros) {
            return Ok(Handshake {
                info: ConnectInfo {
                    client_id,
                    server_revision,
                },
                runtime: Arc::clone(runtime),
                macros: Arc::clone(macros),
            });
        }
        let frame = link.recv_required()?;
        if frame.client_id != client_id.get() {
            return Err(IpcError::ClientIdMismatch {
                expected: client_id.get(),
                actual: frame.client_id,
            });
        }
        match frame.message {
            Message::RuntimeSnapshot(snapshot) if runtime.is_none() => {
                runtime = Some(Arc::new(snapshot));
            }
            Message::MacroSnapshot(snapshot) if macros.is_none() => {
                macros = Some(Arc::new(snapshot));
            }
            Message::Error { code } => return Err(IpcError::Remote(code)),
            other => return Err(IpcError::UnexpectedMessage(other.kind())),
        }
    }
}
