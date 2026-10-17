//! The control-channel named pipe (design §10).
//!
//! [`PipeServer::bind`] creates the first instance with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so the
//! bind fails when any other process already owns the name (squatting defence). Every instance
//! is duplex, byte-mode, blocking-mode with overlapped I/O, rejects remote clients, and carries
//! the AppContainer-client descriptor of [`crate::security`] (plan D27): full access for the
//! user, client access for `ALL APPLICATION PACKAGES`, and no mandatory label.
//! [`connect_client`] opens the pipe with [`PIPE_CLIENT_ACCESS`] (not `GENERIC_WRITE`, whose
//! `FILE_WRITE_ATTRIBUTES` and `FILE_WRITE_EA` an AppContainer is not granted) and
//! identification-level impersonation only, and verifies before returning that the pipe was not
//! created below medium integrity and that the server belongs to the same user and session.
//!
//! # Sandboxed clients (D27)
//!
//! A TIP loaded in an AppContainer host of the user connects like any other TIP:
//!
//! - **Reaching the pipe.** `\\.\pipe\…` names are not redirected for AppContainers, so such a
//!   client opens the controller's pipe by its usual name, and the DACL alone decides. `Local\`
//!   is different: an AppContainer's `Local\` resolves to its own `AppContainerNamedObjects`
//!   directory, so a named kernel object the controller creates under `Local\` cannot be found
//!   under that name from an AppContainer. The IPC path uses no named kernel object;
//!   everything travels over the pipe.
//! - **The server's check of the client.** The user of an AppContainer token is the user's own
//!   SID and its session is the user's session, so
//!   [`PipeConnection::verify_peer_same_user_and_session`] accepts such a client and still
//!   rejects other users (whom the DACL already refuses) and other sessions.
//! - **The client's check of the server.** An AppContainer cannot open the controller's process
//!   and falls back to the pipe owner ([`PeerUserCheck::PipeOwner`]).
//! - **Accepted risk.** Any AppContainer process of the user can connect, read the snapshots,
//!   and send commands. It cannot add server instances or change the pipe's security.
//!
//! A TIP in a low-integrity host that is not an AppContainer cannot connect, by design: the
//! pipe has no label, so it counts as medium with `NO_WRITE_UP`, and such a process gets no
//! write-class right, so it can neither open the pipe for writing, nor add instances, nor
//! change the DACL or owner. That TIP stays on defaults v1. (A low label would have let it do
//! all of that and pose as the controller.)
//!
//! # Refusing a pipe created below medium integrity
//!
//! A low-integrity process can still create a *new* pipe, for example under the controller's
//! name before the controller starts (the controller's bind then fails). Such a pipe always
//! carries its creator's low label, and no process can label an object above its own level, so
//! every client refuses a pipe whose label is below medium with
//! [`WinErrorKind::UntrustedPeer`]; the controller's pipe has no label (medium). An
//! AppContainer cannot create such a pipe at all. A medium-integrity process of the same user
//! can still create or join the pipe; it has the user's full rights anyway.
//!
//! The layer tests check all of this in-process: a thread impersonates a low-integrity copy of
//! the process token or an AppContainer (lowbox) token, connects or is refused, exchanges data,
//! is verified on both ends, is refused the rights it must not have, and creates squatting
//! pipes that clients refuse; the anonymous logon checks that a token the DACL does not list is
//! refused. What needs real processes or other logons is checked by hand on a test machine:
//!
//! 1. A pipe client in a real low-integrity process (an executable labeled low with
//!    `icacls <exe> /setintegritylevel low`) is refused with `AccessDenied` when it opens the
//!    pipe.
//! 2. The TIP in a real AppContainer host (for example a Store app's text box) connects and
//!    follows the controller's mode, settings, and macros; the controller verifies it through
//!    its process token or, if the token cannot be opened, through impersonation.
//! 3. A client started as another user (`runas /user:…`) is refused with `AccessDenied` when
//!    it opens the pipe.
//! 4. A client of the same user in another session that opens this session's pipe name is
//!    rejected by the controller's peer check (session mismatch).
//! 5. A real low-integrity process that creates the pipe name before the controller starts
//!    makes the controller's bind fail, and TIPs refuse that pipe (`UntrustedPeer`).
//!
//! [`PipeConnection`] implements [`Read`] and [`Write`] for the `raumakey-ipc` sessions:
//!
//! - a read timeout ([`PipeConnection::set_read_timeout`]) reports [`io::ErrorKind::TimedOut`]
//!   (`WouldBlock` for a zero timeout), which the sessions treat as "no message yet"; bytes of a
//!   partially received frame are never lost;
//! - a [`PipeCanceller`] aborts a blocked read or write from another thread with
//!   [`io::ErrorKind::ConnectionAborted`]; cancellation is permanent for that connection;
//! - a write timeout (default [`DEFAULT_WRITE_TIMEOUT`]) bounds how long a peer that stopped
//!   reading can block the writer.
//!
//! A closed peer reads as end of stream (`Ok(0)`) and writes fail with `BrokenPipe`.

use std::io::{self, Read, Write};
use std::os::windows::io::OwnedHandle;
use std::sync::Arc;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HANDLE, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT};
use windows::Win32::Security::{RevertToSelf, TOKEN_QUERY};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_FIRST_PIPE_INSTANCE, FILE_FLAG_OVERLAPPED, FILE_FLAGS_AND_ATTRIBUTES,
    FILE_SHARE_NONE, OPEN_EXISTING, PIPE_ACCESS_DUPLEX, ReadFile, SECURITY_IDENTIFICATION,
    SECURITY_SQOS_PRESENT, WriteFile,
};
use windows::Win32::System::IO::{CancelIoEx, GetOverlappedResult, OVERLAPPED};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, DisconnectNamedPipe, GetNamedPipeClientProcessId,
    GetNamedPipeClientSessionId, GetNamedPipeServerProcessId, GetNamedPipeServerSessionId,
    ImpersonateNamedPipeClient, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT, WaitNamedPipeW,
};
use windows::Win32::System::Threading::{
    CreateEventW, GetCurrentThread, OpenThreadToken, ResetEvent, SetEvent, WaitForMultipleObjects,
    WaitForSingleObject,
};

use crate::error::{
    E_ACCESSDENIED, E_INVALIDARG, ResultExt, WinError, WinErrorKind, WinResult, hresult_from_win32,
    win32,
};
use crate::handle;
use crate::security::{
    AppContainerClientSecurity, PIPE_CLIENT_ACCESS, file_object_label_rid, file_object_owner_sid,
    file_object_sddl, is_label_at_least_medium,
};
use crate::sid::{
    current_session_id, current_user_sid_string, process_user_sid_string, token_user_sid_string,
};
use crate::strings::WideString;
use windows::core::PCWSTR;

/// Prefix of every local pipe name.
pub const PIPE_NAMESPACE: &str = "\\\\.\\pipe\\";
/// Longest accepted pipe name (characters), as limited by the pipe file system.
pub const MAX_PIPE_NAME_LEN: usize = 256;
/// Default bound on one blocked write.
pub const DEFAULT_WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Advisory size of each pipe buffer.
const BUFFER_SIZE: u32 = 64 * 1024;
/// `WAIT_OBJECT_0 + 1`: the second handle of a two-handle wait was signaled.
const WAIT_OBJECT_1: WAIT_EVENT = WAIT_EVENT(WAIT_OBJECT_0.0 + 1);

/// Which end of the pipe a connection is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PipeRole {
    /// Created by [`PipeServer`] and connected to a client.
    Server,
    /// Opened by [`connect_client`].
    Client,
}

/// How the peer's user was established.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PeerUserCheck {
    /// From the access token of the peer process.
    ProcessToken,
    /// From the client's token while impersonating it (server side; used when the client
    /// process cannot be opened, for example because it runs elevated).
    Impersonation,
    /// From the owner of the pipe object (client side; used when the server process cannot be
    /// opened, for example from an AppContainer). The server sets the owner explicitly to its
    /// user SID, and an owner can only be set to a SID the creator holds, so another account
    /// cannot forge it. The owner belongs to the pipe, not to one instance, so this check does
    /// not tell the controller's instances from instances another medium-integrity process of
    /// the same user added; AppContainer and low-integrity processes cannot add instances.
    PipeOwner,
}

/// Identity of a verified peer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerIdentity {
    /// Peer process id.
    pub process_id: u32,
    /// Peer session id.
    pub session_id: u32,
    /// Peer user SID.
    pub user_sid: String,
    /// How `user_sid` was obtained.
    pub method: PeerUserCheck,
}

/// Server end: owns the pipe name and hands out connected instances.
pub struct PipeServer {
    name: WideString,
    security: AppContainerClientSecurity,
    pending: Option<OwnedHandle>,
    connect_event: OwnedHandle,
    stop: Arc<OwnedHandle>,
}

impl core::fmt::Debug for PipeServer {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PipeServer")
            .field("pending", &self.pending.is_some())
            .finish_non_exhaustive()
    }
}

/// Stops a [`PipeServer`] from another thread: the current and every later
/// [`PipeServer::accept`] fail with a `Cancelled` error.
#[derive(Clone, Debug)]
pub struct PipeStopper {
    event: Arc<OwnedHandle>,
}

impl PipeStopper {
    /// Requests the stop (idempotent).
    pub fn stop(&self) -> WinResult<()> {
        // SAFETY: the event handle is owned by the shared Arc and valid.
        unsafe { SetEvent(handle::raw(&self.event)) }.context("SetEvent(pipe stop)")
    }
}

/// Aborts blocked reads and writes of one [`PipeConnection`] from another thread.
#[derive(Clone, Debug)]
pub struct PipeCanceller {
    event: Arc<OwnedHandle>,
}

impl PipeCanceller {
    /// Cancels the connection's current and every later read and write.
    pub fn cancel(&self) -> WinResult<()> {
        // SAFETY: the event handle is owned by the shared Arc and valid.
        unsafe { SetEvent(handle::raw(&self.event)) }.context("SetEvent(pipe cancel)")
    }
}

enum ConnectOutcome {
    Connected,
    ClientGone,
    Stopped,
}

impl PipeServer {
    /// Creates the first instance of `name` (`\\.\pipe\…`). Fails with `AccessDenied` when the
    /// name is already owned by any process, including another Raumakey instance.
    pub fn bind(name: &str) -> WinResult<Self> {
        let name = validate_pipe_name(name)?;
        let security = AppContainerClientSecurity::for_current_user()?;
        let first = create_instance(&name, &security, true)?;
        Ok(Self {
            name,
            security,
            pending: Some(first),
            connect_event: create_event(true)?,
            stop: Arc::new(create_event(true)?),
        })
    }

    /// A handle that cancels [`PipeServer::accept`] from another thread.
    pub fn stopper(&self) -> PipeStopper {
        PipeStopper {
            event: Arc::clone(&self.stop),
        }
    }

    /// Waits for the next client. Blocks until a client connects or the server is stopped
    /// (`Cancelled`). A fresh instance is prepared before returning, so clients can always
    /// connect while the caller handles the returned connection.
    ///
    /// The peer is not verified here; call
    /// [`PipeConnection::verify_peer_same_user_and_session`] before trusting it.
    pub fn accept(&mut self) -> WinResult<PipeConnection> {
        loop {
            if self.stop_requested() {
                return Err(cancelled("accept"));
            }
            let instance = match self.pending.take() {
                Some(instance) => instance,
                None => create_instance(&self.name, &self.security, false)?,
            };
            match self.connect(&instance) {
                Ok(ConnectOutcome::Connected) => {
                    // Best effort: a failure here is retried by the next accept.
                    self.pending = create_instance(&self.name, &self.security, false).ok();
                    return PipeConnection::new(instance, PipeRole::Server);
                }
                Ok(ConnectOutcome::ClientGone) => {
                    // SAFETY: `instance` is a valid server instance handle.
                    let _ = unsafe { DisconnectNamedPipe(handle::raw(&instance)) };
                    self.pending = Some(instance);
                }
                Ok(ConnectOutcome::Stopped) => {
                    self.pending = Some(instance);
                    return Err(cancelled("accept"));
                }
                Err(error) => return Err(error),
            }
        }
    }

    fn stop_requested(&self) -> bool {
        // SAFETY: the event handle is valid; a zero timeout only polls.
        let state = unsafe { WaitForSingleObject(handle::raw(&self.stop), 0) };
        state == WAIT_OBJECT_0
    }

    fn connect(&self, instance: &OwnedHandle) -> WinResult<ConnectOutcome> {
        let pipe = handle::raw(instance);
        let event = handle::raw(&self.connect_event);
        // SAFETY: the event handle is valid.
        unsafe { ResetEvent(event) }.context("ResetEvent")?;
        let mut overlapped = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        // SAFETY: `overlapped` stays alive and in place until the operation completed: every
        // path below either sees it complete or cancels it and waits for the completion.
        let started = unsafe { ConnectNamedPipe(pipe, Some(&raw mut overlapped)) };
        match started {
            Ok(()) => return Ok(ConnectOutcome::Connected),
            Err(error) if is_win32(&error, win32::ERROR_PIPE_CONNECTED) => {
                return Ok(ConnectOutcome::Connected);
            }
            Err(error) if is_win32(&error, win32::ERROR_NO_DATA) => {
                return Ok(ConnectOutcome::ClientGone);
            }
            Err(error) if is_win32(&error, win32::ERROR_IO_PENDING) => {}
            Err(error) => return Err(WinError::new(error.code().0, "ConnectNamedPipe")),
        }
        let handles = [event, handle::raw(&self.stop)];
        // SAFETY: both handles are valid events.
        let waited = unsafe { WaitForMultipleObjects(&handles, false, u32::MAX) };
        if waited == WAIT_OBJECT_0 {
            let mut transferred = 0_u32;
            // SAFETY: the operation completed; `overlapped` is the one it used.
            return match unsafe { GetOverlappedResult(pipe, &overlapped, &mut transferred, false) }
            {
                Ok(()) => Ok(ConnectOutcome::Connected),
                Err(error) if is_win32(&error, win32::ERROR_NO_DATA) => {
                    Ok(ConnectOutcome::ClientGone)
                }
                Err(error) => Err(WinError::new(error.code().0, "ConnectNamedPipe")),
            };
        }
        // Stop requested (or the wait failed): cancel and wait until `overlapped` is released.
        let failure = (waited != WAIT_OBJECT_1).then(|| WinError::last("WaitForMultipleObjects"));
        // SAFETY: cancels only the operation that uses `overlapped`; completion is awaited
        // below so the structure is not released early.
        let _ = unsafe { CancelIoEx(pipe, Some(&raw const overlapped)) };
        let mut transferred = 0_u32;
        // SAFETY: waits for the cancelled (or completed) operation on `overlapped`.
        let finished = unsafe { GetOverlappedResult(pipe, &overlapped, &mut transferred, true) };
        if finished.is_ok() {
            // A client connected just before the stop; drop it so the instance can be reused.
            // SAFETY: `pipe` is a valid server instance handle.
            let _ = unsafe { DisconnectNamedPipe(pipe) };
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(ConnectOutcome::Stopped),
        }
    }
}

/// Opens the server end of `name`, waiting up to `timeout` while every instance is busy, and
/// verifies that the server belongs to the same user and session.
///
/// Fails with `NotFound` immediately when no server exists, `TimedOut` when all instances stayed
/// busy, and `AccessDenied` when the server is not the same user and session.
pub fn connect_client(name: &str, timeout: Duration) -> WinResult<PipeConnection> {
    let name = validate_pipe_name(name)?;
    let deadline = Instant::now().checked_add(timeout);
    loop {
        // SAFETY: the name outlives the call; the returned handle is owned below.
        let opened = unsafe {
            CreateFileW(
                name.as_pcwstr(),
                PIPE_CLIENT_ACCESS,
                FILE_SHARE_NONE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED | SECURITY_SQOS_PRESENT | SECURITY_IDENTIFICATION,
                None,
            )
        };
        match opened {
            Ok(raw) => {
                // SAFETY: CreateFileW returned a new handle owned by nobody else.
                let connection =
                    PipeConnection::new(unsafe { handle::take(raw) }, PipeRole::Client)?;
                connection
                    .verify_peer_same_user_and_session()
                    .map_err(|error| error.with_context("pipe server identity"))?;
                return Ok(connection);
            }
            Err(error) if is_win32(&error, win32::ERROR_PIPE_BUSY) => {
                let left = deadline.map_or(Duration::MAX, |deadline| {
                    deadline.saturating_duration_since(Instant::now())
                });
                if left.is_zero() {
                    return Err(WinError::from_win32(win32::ERROR_TIMEOUT, "WaitNamedPipeW"));
                }
                // SAFETY: the name outlives the call.
                let ready = unsafe { WaitNamedPipeW(name.as_pcwstr(), timeout_ms(Some(left))) };
                if !ready.as_bool() {
                    let error = WinError::last("WaitNamedPipeW");
                    if error.is_win32(win32::ERROR_SEM_TIMEOUT) {
                        return Err(WinError::from_win32(win32::ERROR_TIMEOUT, "WaitNamedPipeW"));
                    }
                    if error.kind() != WinErrorKind::Busy {
                        return Err(error);
                    }
                }
            }
            Err(error) => return Err(WinError::new(error.code().0, "CreateFileW(pipe)")),
        }
    }
}

/// One connected end of the pipe.
pub struct PipeConnection {
    pipe: OwnedHandle,
    role: PipeRole,
    read_event: OwnedHandle,
    write_event: OwnedHandle,
    cancel: Arc<OwnedHandle>,
    read_timeout: Option<Duration>,
    write_timeout: Option<Duration>,
}

impl core::fmt::Debug for PipeConnection {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PipeConnection")
            .field("role", &self.role)
            .field("read_timeout", &self.read_timeout)
            .field("write_timeout", &self.write_timeout)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Direction {
    Read,
    Write,
}

/// The caller's buffer for one transfer.
enum Buffer<'a> {
    Read(&'a mut [u8]),
    Write(&'a [u8]),
}

impl Buffer<'_> {
    const fn direction(&self) -> Direction {
        match self {
            Self::Read(_) => Direction::Read,
            Self::Write(_) => Direction::Write,
        }
    }
}

impl PipeConnection {
    fn new(pipe: OwnedHandle, role: PipeRole) -> WinResult<Self> {
        Ok(Self {
            pipe,
            role,
            read_event: create_event(true)?,
            write_event: create_event(true)?,
            cancel: Arc::new(create_event(true)?),
            read_timeout: None,
            write_timeout: Some(DEFAULT_WRITE_TIMEOUT),
        })
    }

    /// Which end this is.
    pub const fn role(&self) -> PipeRole {
        self.role
    }

    /// Read timeout: `None` blocks (until data, end of stream, or cancellation),
    /// `Some(Duration::ZERO)` polls.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        self.read_timeout = timeout;
    }

    /// The read timeout.
    pub const fn read_timeout(&self) -> Option<Duration> {
        self.read_timeout
    }

    /// Write timeout: `None` blocks until the peer has drained enough data.
    pub fn set_write_timeout(&mut self, timeout: Option<Duration>) {
        self.write_timeout = timeout;
    }

    /// The write timeout.
    pub const fn write_timeout(&self) -> Option<Duration> {
        self.write_timeout
    }

    /// A handle that aborts this connection's reads and writes from another thread.
    pub fn canceller(&self) -> PipeCanceller {
        PipeCanceller {
            event: Arc::clone(&self.cancel),
        }
    }

    /// Process id of the client end.
    pub fn client_process_id(&self) -> WinResult<u32> {
        let mut pid = 0_u32;
        // SAFETY: valid pipe handle and out pointer.
        unsafe { GetNamedPipeClientProcessId(self.raw(), &mut pid) }
            .context("GetNamedPipeClientProcessId")?;
        Ok(pid)
    }

    /// Process id of the server end.
    pub fn server_process_id(&self) -> WinResult<u32> {
        let mut pid = 0_u32;
        // SAFETY: valid pipe handle and out pointer.
        unsafe { GetNamedPipeServerProcessId(self.raw(), &mut pid) }
            .context("GetNamedPipeServerProcessId")?;
        Ok(pid)
    }

    /// Process id of the other end.
    pub fn peer_process_id(&self) -> WinResult<u32> {
        match self.role {
            PipeRole::Server => self.client_process_id(),
            PipeRole::Client => self.server_process_id(),
        }
    }

    /// Session id of the other end.
    pub fn peer_session_id(&self) -> WinResult<u32> {
        let mut session = 0_u32;
        match self.role {
            // SAFETY: valid pipe handle and out pointer.
            PipeRole::Server => unsafe { GetNamedPipeClientSessionId(self.raw(), &mut session) }
                .context("GetNamedPipeClientSessionId")?,
            // SAFETY: valid pipe handle and out pointer.
            PipeRole::Client => unsafe { GetNamedPipeServerSessionId(self.raw(), &mut session) }
                .context("GetNamedPipeServerSessionId")?,
        }
        Ok(session)
    }

    /// Verifies that the other end runs in this session as this user.
    ///
    /// On the client side it first refuses a pipe whose mandatory label is below medium
    /// ([`WinErrorKind::UntrustedPeer`]): a lower-integrity process created that pipe (see the
    /// module documentation).
    ///
    /// The user comes from the peer's process token; when that token cannot be opened (an
    /// elevated peer seen from a non-elevated process, or the controller seen from an
    /// AppContainer) the server side impersonates the client at identification level and the
    /// client side checks the pipe owner instead (see [`PeerUserCheck`]). A mismatch fails with
    /// `AccessDenied`. An AppContainer peer of this user and session passes: its token's user
    /// is the user's own SID.
    pub fn verify_peer_same_user_and_session(&self) -> WinResult<PeerIdentity> {
        if self.role == PipeRole::Client {
            self.verify_pipe_label()?;
        }
        let session_id = self.peer_session_id()?;
        if session_id != current_session_id()? {
            return Err(WinError::new(E_ACCESSDENIED, "pipe peer session"));
        }
        let process_id = self.peer_process_id()?;
        let (user_sid, method) = match process_user_sid_string(process_id) {
            Ok(sid) => (sid, PeerUserCheck::ProcessToken),
            Err(error) if error.kind() == WinErrorKind::AccessDenied => match self.role {
                PipeRole::Server => (
                    self.client_user_sid_by_impersonation()?,
                    PeerUserCheck::Impersonation,
                ),
                PipeRole::Client => (self.server_pipe_owner_sid()?, PeerUserCheck::PipeOwner),
            },
            Err(error) => return Err(error),
        };
        if user_sid != current_user_sid_string()? {
            return Err(WinError::new(E_ACCESSDENIED, "pipe peer user"));
        }
        Ok(PeerIdentity {
            process_id,
            session_id,
            user_sid,
            method,
        })
    }

    /// Refuses a pipe whose mandatory label is below medium with
    /// [`WinErrorKind::UntrustedPeer`] (`ERROR_INVALID_LABEL`).
    fn verify_pipe_label(&self) -> WinResult<()> {
        let label = file_object_label_rid(self.raw())?;
        if is_label_at_least_medium(label) {
            Ok(())
        } else {
            Err(WinError::from_win32(
                win32::ERROR_INVALID_LABEL,
                "pipe server integrity",
            ))
        }
    }

    /// Server side: the client's user SID, read from its token while impersonating it at
    /// identification level. The thread reverts before returning; if reverting fails the
    /// process aborts rather than continue under a foreign identity.
    pub fn client_user_sid_by_impersonation(&self) -> WinResult<String> {
        if self.role != PipeRole::Server {
            return Err(WinError::new(E_INVALIDARG, "ImpersonateNamedPipeClient"));
        }
        // SAFETY: valid server pipe handle; the guard below reverts on every path.
        unsafe { ImpersonateNamedPipeClient(self.raw()) }.context("ImpersonateNamedPipeClient")?;
        let revert = RevertOnDrop;
        let mut token = HANDLE::default();
        // SAFETY: GetCurrentThread is a pseudo handle; `token` is a valid out pointer. The token
        // is opened with the process identity (`OpenAsSelf`), as identification-level tokens
        // cannot open objects themselves.
        let opened = unsafe { OpenThreadToken(GetCurrentThread(), TOKEN_QUERY, true, &mut token) }
            .context("OpenThreadToken");
        drop(revert);
        opened?;
        // SAFETY: OpenThreadToken returned a new handle owned by nobody else.
        let token = unsafe { handle::take(token) };
        token_user_sid_string(handle::raw(&token))
    }

    /// Owner SID of the pipe object (meaningful on the client side; see
    /// [`PeerUserCheck::PipeOwner`]).
    pub fn server_pipe_owner_sid(&self) -> WinResult<String> {
        file_object_owner_sid(self.raw())
    }

    /// Owner, DACL, and mandatory label of the pipe object in SDDL form (both ends see the
    /// same pipe security).
    pub fn security_sddl(&self) -> WinResult<String> {
        file_object_sddl(self.raw())
    }

    fn raw(&self) -> HANDLE {
        handle::raw(&self.pipe)
    }

    fn cancelled(&self) -> bool {
        // SAFETY: valid event handle; a zero timeout only polls.
        let state = unsafe { WaitForSingleObject(handle::raw(&self.cancel), 0) };
        state == WAIT_OBJECT_0
    }

    /// Runs one overlapped read or write to completion, timeout, or cancellation.
    fn transfer(&self, buffer: Buffer<'_>) -> io::Result<usize> {
        if self.cancelled() {
            return Err(io::ErrorKind::ConnectionAborted.into());
        }
        let direction = buffer.direction();
        let (event, timeout) = match direction {
            Direction::Read => (handle::raw(&self.read_event), self.read_timeout),
            Direction::Write => (handle::raw(&self.write_event), self.write_timeout),
        };
        let mut overlapped = OVERLAPPED {
            hEvent: event,
            ..Default::default()
        };
        let pipe = self.raw();
        let limit = u32::MAX as usize;
        // SAFETY: `buffer` stays borrowed for this whole call and `overlapped` lives on this
        // frame. The function does not return before the operation completed (directly, after
        // the wait, or after CancelIoEx plus a blocking GetOverlappedResult), so neither is
        // released while the system still uses it.
        let started = unsafe {
            match buffer {
                Buffer::Read(data) => {
                    let end = data.len().min(limit);
                    ReadFile(pipe, data.get_mut(..end), None, Some(&raw mut overlapped))
                }
                Buffer::Write(data) => {
                    let end = data.len().min(limit);
                    WriteFile(pipe, data.get(..end), None, Some(&raw mut overlapped))
                }
            }
        };
        if let Err(error) = &started
            && !is_win32(error, win32::ERROR_IO_PENDING)
        {
            return transfer_error(direction, error.code().0);
        }
        let ms = timeout_ms(timeout);
        let waited = if started.is_ok() {
            WAIT_OBJECT_0
        } else {
            let handles = [event, handle::raw(&self.cancel)];
            // SAFETY: both handles are valid events.
            unsafe { WaitForMultipleObjects(&handles, false, ms) }
        };
        let mut transferred = 0_u32;
        if waited == WAIT_OBJECT_0 {
            // SAFETY: the operation completed; `overlapped` is the one it used.
            return match unsafe { GetOverlappedResult(pipe, &overlapped, &mut transferred, false) }
            {
                Ok(()) => Ok(transferred as usize),
                Err(error) => transfer_error(direction, error.code().0),
            };
        }
        // Timed out, cancelled, or the wait failed: cancel and wait for the completion.
        // SAFETY: cancels only this operation; completion is awaited right below.
        let _ = unsafe { CancelIoEx(pipe, Some(&raw const overlapped)) };
        // SAFETY: waits for the cancelled (or just completed) operation on `overlapped`.
        let finished = unsafe { GetOverlappedResult(pipe, &overlapped, &mut transferred, true) };
        match finished {
            // It completed before the cancellation took effect: keep the data.
            Ok(()) => Ok(transferred as usize),
            Err(error) if is_win32(&error, win32::ERROR_OPERATION_ABORTED) => {
                if waited == WAIT_TIMEOUT {
                    if timeout == Some(Duration::ZERO) {
                        Err(io::ErrorKind::WouldBlock.into())
                    } else {
                        Err(io::ErrorKind::TimedOut.into())
                    }
                } else if waited == WAIT_OBJECT_1 {
                    Err(io::ErrorKind::ConnectionAborted.into())
                } else {
                    Err(io::Error::from(WinError::last("WaitForMultipleObjects")))
                }
            }
            Err(error) => transfer_error(direction, error.code().0),
        }
    }
}

impl Read for PipeConnection {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.transfer(Buffer::Read(buf))
    }
}

impl Write for PipeConnection {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        self.transfer(Buffer::Write(buf))
    }

    fn flush(&mut self) -> io::Result<()> {
        // Writes complete only once the data is in the pipe; there is no user-space buffer.
        Ok(())
    }
}

/// Reverts impersonation when dropped; aborts the process if that fails.
struct RevertOnDrop;

impl Drop for RevertOnDrop {
    fn drop(&mut self) {
        // SAFETY: the thread is impersonating (set up by the caller); RevertToSelf has no other
        // preconditions.
        if unsafe { RevertToSelf() }.is_err() {
            std::process::abort();
        }
    }
}

fn transfer_error(direction: Direction, code: i32) -> io::Result<usize> {
    let broken = [
        win32::ERROR_BROKEN_PIPE,
        win32::ERROR_PIPE_NOT_CONNECTED,
        win32::ERROR_NO_DATA,
    ]
    .iter()
    .any(|error| hresult_from_win32(*error) == code);
    match (direction, broken) {
        (Direction::Read, true) => Ok(0),
        (Direction::Write, true) => Err(io::ErrorKind::BrokenPipe.into()),
        _ => Err(io::Error::from(WinError::new(code, "pipe transfer"))),
    }
}

fn is_win32(error: &windows::core::Error, code: u32) -> bool {
    error.code().0 == hresult_from_win32(code)
}

fn cancelled(context: &'static str) -> WinError {
    WinError::from_win32(win32::ERROR_OPERATION_ABORTED, context)
}

/// Milliseconds for a Win32 wait: `None` is infinite, a nonzero duration rounds up.
pub(crate) fn timeout_ms(timeout: Option<Duration>) -> u32 {
    match timeout {
        None => u32::MAX,
        Some(timeout) if timeout.is_zero() => 0,
        Some(timeout) => {
            let ms = timeout.as_nanos().div_ceil(1_000_000);
            u32::try_from(ms).unwrap_or(u32::MAX - 1).min(u32::MAX - 1)
        }
    }
}

fn validate_pipe_name(name: &str) -> WinResult<WideString> {
    let invalid = || WinError::new(E_INVALIDARG, "pipe name");
    let prefix = name.get(..PIPE_NAMESPACE.len()).ok_or_else(invalid)?;
    if !prefix.eq_ignore_ascii_case(PIPE_NAMESPACE)
        || name.len() == PIPE_NAMESPACE.len()
        || name.chars().count() > MAX_PIPE_NAME_LEN
    {
        return Err(invalid());
    }
    WideString::new(name).map_err(|_| invalid())
}

fn create_instance(
    name: &WideString,
    security: &AppContainerClientSecurity,
    first: bool,
) -> WinResult<OwnedHandle> {
    let mut open_mode: FILE_FLAGS_AND_ATTRIBUTES = PIPE_ACCESS_DUPLEX | FILE_FLAG_OVERLAPPED;
    if first {
        open_mode |= FILE_FLAG_FIRST_PIPE_INSTANCE;
    }
    // SAFETY: the name and the security attributes outlive the call.
    let pipe = unsafe {
        CreateNamedPipeW(
            name.as_pcwstr(),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            BUFFER_SIZE,
            BUFFER_SIZE,
            0,
            Some(security.as_ptr()),
        )
    };
    if pipe.is_invalid() {
        return Err(WinError::last("CreateNamedPipeW"));
    }
    // SAFETY: CreateNamedPipeW returned a new handle owned by nobody else.
    Ok(unsafe { handle::take(pipe) })
}

/// An unnamed event with default security.
fn create_event(manual_reset: bool) -> WinResult<OwnedHandle> {
    // SAFETY: no name and no security attributes; the handle is owned below.
    let event = unsafe { CreateEventW(None, manual_reset, false, PCWSTR::null()) }
        .context("CreateEventW")?;
    // SAFETY: CreateEventW returned a new handle owned by nobody else.
    Ok(unsafe { handle::take(event) })
}
