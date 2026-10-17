//! `pipe`: a real in-process server/client pair on unique test pipe names.

use std::io::{ErrorKind, Read, Write};
use std::num::NonZeroU64;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use raumakey_ipc::client::{ClientEvent, ClientIdentity, ClientSession};
use raumakey_ipc::error::ErrorCode;
use raumakey_ipc::message::{ClientKind, Command, MacroSnapshot, RuntimeSnapshot};
use raumakey_ipc::server::{PublishedState, ServerEvent, ServerSession};
use std::os::windows::io::{FromRawHandle, OwnedHandle};

use raumakey_windows::error::{WinErrorKind, WinResult};
use raumakey_windows::pipe::{
    PeerIdentity, PeerUserCheck, PipeConnection, PipeRole, PipeServer, connect_client,
};
use raumakey_windows::security::{AppContainerClientSecurity, PIPE_CLIENT_ACCESS};
use raumakey_windows::sid::{current_session_id, current_user_sid_string};
use raumakey_windows::strings::WideString;
use windows::Win32::Foundation::{
    ERROR_ACCESS_DENIED, GENERIC_READ, GENERIC_WRITE, GetLastError, HLOCAL, LocalFree, WIN32_ERROR,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES};
use windows::Win32::Storage::FileSystem::{
    CreateFileW, DELETE, FILE_FLAGS_AND_ATTRIBUTES, FILE_SHARE_NONE, FILE_WRITE_ATTRIBUTES,
    OPEN_EXISTING, PIPE_ACCESS_DUPLEX, WRITE_DAC, WRITE_OWNER,
};
use windows::Win32::System::Pipes::{
    CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
    PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
};

use crate::support::{TestToken, as_anonymous, unique_suffix};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

fn test_pipe_name() -> String {
    format!("\\\\.\\pipe\\Raumakey-Test-{}-pipe", unique_suffix())
}

/// Binds a server, connects one client on another thread, and returns both ends.
fn connected_pair() -> (PipeServer, PipeConnection, PipeConnection) {
    let name = test_pipe_name();
    let mut server = PipeServer::bind(&name).unwrap();
    let client = thread::spawn(move || connect_client(&name, CONNECT_TIMEOUT).unwrap());
    let accepted = server.accept().unwrap();
    (server, accepted, client.join().unwrap())
}

/// What [`connect_with`] returns: the server, the client's result, and the accepted end
/// (`None` when no client connected).
type Attempt<T> = (PipeServer, WinResult<T>, Option<PipeConnection>);

/// Binds a server on a fresh name and runs `client` (given the name) while the server accepts
/// on another thread. When the client fails or panics, the server is stopped, so a refused
/// client never leaves `accept` waiting; a panic is resumed once the server thread ended.
fn connect_with<T: Send>(client: impl FnOnce(String) -> WinResult<T> + Send) -> Attempt<T> {
    let name = test_pipe_name();
    let mut server = PipeServer::bind(&name).unwrap();
    let stopper = server.stopper();
    thread::scope(|scope| {
        let accepting = scope.spawn(move || {
            let accepted = server.accept();
            (server, accepted)
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| client(name)));
        if !matches!(result, Ok(Ok(_))) {
            stopper.stop().unwrap();
        }
        let (server, accepted) = accepting.join().unwrap();
        match result {
            Ok(result) => (server, result, accepted.ok()),
            Err(payload) => std::panic::resume_unwind(payload),
        }
    })
}

/// A failing assertion inside the client closure must fail the test, not leave `accept` waiting.
#[test]
fn connect_with_stops_the_server_when_the_client_panics() {
    let outcome = std::panic::catch_unwind(|| {
        connect_with::<()>(|_name| panic!("client closure panicked on purpose"))
    });
    assert!(outcome.is_err());
}

/// `connect_client` plus the client's own peer check, run as `token`.
fn connect_as(token: &TestToken, name: &str) -> WinResult<(PipeConnection, PeerIdentity)> {
    token.impersonate(|| {
        let client = connect_client(name, CONNECT_TIMEOUT)?;
        let server = client.verify_peer_same_user_and_session()?;
        Ok((client, server))
    })
}

fn exchange(client: &mut PipeConnection, server: &mut PipeConnection) {
    client.write_all(b"hello").unwrap();
    let mut buf = [0_u8; 5];
    server.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"hello");
    server.write_all(b"snapshot").unwrap();
    let mut buf = [0_u8; 8];
    client.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"snapshot");
}

/// Opens the client end of `name` with exactly `access`; the Win32 error code on failure.
fn open_raw(name: &str, access: u32) -> Result<OwnedHandle, WIN32_ERROR> {
    let wide = WideString::new(name).unwrap();
    // SAFETY: the name outlives the call; a returned handle is owned below.
    let opened = unsafe {
        CreateFileW(
            wide.as_pcwstr(),
            access,
            FILE_SHARE_NONE,
            None,
            OPEN_EXISTING,
            FILE_FLAGS_AND_ATTRIBUTES(0),
            None,
        )
    };
    match opened {
        // SAFETY: CreateFileW returned a new handle owned by nobody else.
        Ok(handle) => Ok(unsafe { OwnedHandle::from_raw_handle(handle.0) }),
        Err(error) => Err(WIN32_ERROR::from_error(&error).unwrap()),
    }
}

/// Adds a server instance to the existing pipe `name` (default security); the Win32 error code
/// on failure.
fn add_instance(name: &str) -> Result<OwnedHandle, WIN32_ERROR> {
    create_instance(name, None)
}

/// Security attributes for a descriptor written in SDDL (test-only), freed on drop.
struct SddlAttributes {
    attributes: SECURITY_ATTRIBUTES,
}

impl SddlAttributes {
    fn new(sddl: &str) -> Self {
        let text = WideString::new(sddl).unwrap();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        // SAFETY: `text` is NUL-terminated and outlives the call; the LocalAlloc'd descriptor
        // is freed on drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_pcwstr(),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .unwrap();
        Self {
            attributes: SECURITY_ATTRIBUTES {
                nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
                lpSecurityDescriptor: descriptor.0,
                bInheritHandle: false.into(),
            },
        }
    }
}

impl Drop for SddlAttributes {
    fn drop(&mut self) {
        // SAFETY: frees the descriptor ConvertStringSecurityDescriptorToSecurityDescriptorW
        // allocated, once.
        unsafe { LocalFree(Some(HLOCAL(self.attributes.lpSecurityDescriptor))) };
    }
}

/// Creates a server instance of `name`: a new pipe (with `security`, or the default security
/// when `None`) or another instance of an existing one; the Win32 error code on failure.
fn create_instance(
    name: &str,
    security: Option<&SECURITY_ATTRIBUTES>,
) -> Result<OwnedHandle, WIN32_ERROR> {
    let wide = WideString::new(name).unwrap();
    // SAFETY: the name and the security attributes outlive the call; a returned handle is
    // owned below.
    let pipe = unsafe {
        CreateNamedPipeW(
            wide.as_pcwstr(),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            4096,
            4096,
            0,
            security.map(std::ptr::from_ref),
        )
    };
    if pipe.is_invalid() {
        // SAFETY: reads the calling thread's last-error value right after the call.
        return Err(unsafe { GetLastError() });
    }
    // SAFETY: CreateNamedPipeW returned a new handle owned by nobody else.
    Ok(unsafe { OwnedHandle::from_raw_handle(pipe.0) })
}

#[test]
fn the_pipe_grants_the_user_and_app_packages_and_has_no_label() {
    let (_server, mut accepted, mut client) = connected_pair();
    let sid = current_user_sid_string().unwrap();
    // Applying the descriptor maps the user's GENERIC_ALL to FILE_ALL_ACCESS (FA).
    let expected = format!("O:{sid}D:P(A;;FA;;;{sid})(A;;0x12008b;;;AC)");
    assert_eq!(client.security_sddl().unwrap(), expected);
    assert_eq!(accepted.security_sddl().unwrap(), expected);
    // A normal client of the same user still connects and is verified.
    exchange(&mut client, &mut accepted);
    let peer = accepted.verify_peer_same_user_and_session().unwrap();
    assert_eq!(peer.method, PeerUserCheck::ProcessToken);
}

/// Intended (D27): a low-integrity host that is not an AppContainer cannot connect, and its
/// TIP stays on defaults v1. The unlabeled pipe counts as medium with `NO_WRITE_UP`.
#[test]
fn a_low_integrity_client_that_is_not_an_app_container_is_refused() {
    let token = TestToken::low_integrity();
    let (_server, client, accepted) = connect_with(|name| connect_as(&token, &name));
    assert_eq!(
        client.err().map(|e| e.kind()),
        Some(WinErrorKind::AccessDenied)
    );
    assert!(accepted.is_none());
}

/// A pipe that a low-integrity process created (for example under the controller's name before
/// the controller started) carries that process's low label, and clients refuse it.
#[test]
fn a_normal_client_refuses_a_pipe_created_by_a_low_integrity_process() {
    let name = test_pipe_name();
    let low = TestToken::low_integrity();
    let _squatter = low.impersonate(|| create_instance(&name, None)).unwrap();
    let error = connect_client(&name, CONNECT_TIMEOUT).unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::UntrustedPeer, "{error}");
}

#[test]
fn an_app_container_client_refuses_a_pipe_created_by_a_low_integrity_process() {
    let name = test_pipe_name();
    let low = TestToken::low_integrity();
    // Even with the controller's DACL, which lets the AppContainer open the pipe, the pipe gets
    // its creator's low label.
    let security = AppContainerClientSecurity::for_current_user().unwrap();
    let _squatter = low
        .impersonate(|| create_instance(&name, Some(security.as_attributes())))
        .unwrap();
    let error = TestToken::app_container()
        .impersonate(|| connect_client(&name, CONNECT_TIMEOUT))
        .unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::UntrustedPeer, "{error}");
}

/// Pins the label check to the label's actual level: a pipe explicitly labeled medium (which a
/// medium or higher process may set) is accepted like an unlabeled one.
#[test]
fn a_client_accepts_a_pipe_explicitly_labeled_medium() {
    let name = test_pipe_name();
    let sid = current_user_sid_string().unwrap();
    let medium = SddlAttributes::new(&format!("O:{sid}D:P(A;;GA;;;{sid})S:(ML;;NW;;;ME)"));
    let _server = create_instance(&name, Some(&medium.attributes)).unwrap();
    let client = connect_client(&name, CONNECT_TIMEOUT).unwrap();
    let sddl = client.security_sddl().unwrap();
    assert!(sddl.contains("(ML;;NW;;;ME)"), "{sddl}");
}

#[test]
fn a_low_integrity_process_can_neither_add_instances_nor_change_the_pipe_security() {
    let name = test_pipe_name();
    let _server = PipeServer::bind(&name).unwrap();
    let token = TestToken::low_integrity();
    assert_eq!(
        token.impersonate(|| add_instance(&name)).err(),
        Some(ERROR_ACCESS_DENIED),
        "FILE_CREATE_PIPE_INSTANCE"
    );
    for (access, what) in [(WRITE_DAC.0, "WRITE_DAC"), (WRITE_OWNER.0, "WRITE_OWNER")] {
        let opened = token.impersonate(|| open_raw(&name, access));
        assert_eq!(opened.err(), Some(ERROR_ACCESS_DENIED), "{what}");
    }
}

#[test]
fn an_app_container_client_of_the_same_user_connects_and_is_verified() {
    let token = TestToken::app_container();
    let (_server, client, accepted) = connect_with(|name| connect_as(&token, &name));
    let (mut client, server_peer) = client.unwrap();
    let mut accepted = accepted.expect("accepted");
    let sid = current_user_sid_string().unwrap();
    // An AppContainer cannot open the server's process, so it checks the pipe owner.
    assert_eq!(server_peer.method, PeerUserCheck::PipeOwner);
    assert_eq!(server_peer.user_sid, sid);
    exchange(&mut client, &mut accepted);
    accepted.verify_peer_same_user_and_session().unwrap();
    // The user of an AppContainer token is the user's own SID.
    assert_eq!(accepted.client_user_sid_by_impersonation().unwrap(), sid);
}

#[test]
fn an_app_container_client_times_out_on_a_busy_pipe() {
    let name = test_pipe_name();
    let _server = PipeServer::bind(&name).unwrap();
    // Takes the only instance: nothing accepts, so no further instance is created.
    let _first = connect_client(&name, CONNECT_TIMEOUT).unwrap();
    let token = TestToken::app_container();
    let error = token
        .impersonate(|| connect_client(&name, Duration::from_millis(200)))
        .unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::TimedOut, "{error}");
}

/// The anonymous logon, whose user and groups the DACL does not list, is refused when it opens
/// the pipe. It is not in Everyone or Authenticated Users, so this does not show that another
/// interactive user is refused; the descriptor tests check ACE by ACE that the DACL lists only
/// the user and `ALL APPLICATION PACKAGES`.
#[test]
fn the_anonymous_logon_is_refused_at_open() {
    let (_server, client, accepted) =
        connect_with(|name| as_anonymous(|| connect_client(&name, CONNECT_TIMEOUT)));
    assert_eq!(client.unwrap_err().kind(), WinErrorKind::AccessDenied);
    assert!(accepted.is_none());
}

/// The pipe file system drops `FILE_CREATE_PIPE_INSTANCE` from client opens and checks it only
/// when a server instance is added, so that right is tested with `CreateNamedPipeW`.
#[test]
fn app_container_clients_get_client_access_only() {
    let name = test_pipe_name();
    let _server = PipeServer::bind(&name).unwrap();
    let token = TestToken::app_container();
    for (access, what) in [
        ((GENERIC_READ | GENERIC_WRITE).0, "GENERIC_WRITE"),
        (
            PIPE_CLIENT_ACCESS | FILE_WRITE_ATTRIBUTES.0,
            "FILE_WRITE_ATTRIBUTES",
        ),
        (PIPE_CLIENT_ACCESS | WRITE_DAC.0, "WRITE_DAC"),
        (PIPE_CLIENT_ACCESS | WRITE_OWNER.0, "WRITE_OWNER"),
        (PIPE_CLIENT_ACCESS | DELETE.0, "DELETE"),
    ] {
        let opened = token.impersonate(|| open_raw(&name, access));
        assert_eq!(opened.err(), Some(ERROR_ACCESS_DENIED), "{what}");
    }
    assert_eq!(
        token.impersonate(|| add_instance(&name)).err(),
        Some(ERROR_ACCESS_DENIED),
        "an AppContainer must not add server instances"
    );
    // The client access itself is granted.
    token
        .impersonate(|| open_raw(&name, PIPE_CLIENT_ACCESS))
        .unwrap();
}

#[test]
fn server_and_client_exchange_bytes() {
    let (_server, mut accepted, mut client) = connected_pair();
    assert_eq!(accepted.role(), PipeRole::Server);
    assert_eq!(client.role(), PipeRole::Client);
    client.write_all(b"ping").unwrap();
    let mut buf = [0_u8; 4];
    accepted.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"ping");
    accepted.write_all(b"pong!").unwrap();
    let mut buf = [0_u8; 5];
    client.read_exact(&mut buf).unwrap();
    assert_eq!(&buf, b"pong!");
}

#[test]
fn a_second_server_cannot_take_an_owned_name() {
    let name = test_pipe_name();
    let _server = PipeServer::bind(&name).unwrap();
    let error = PipeServer::bind(&name).unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::AccessDenied, "{error}");
}

#[test]
fn names_outside_the_pipe_namespace_are_rejected() {
    for bad in [
        "Raumakey",
        "\\\\server\\pipe\\x",
        "\\\\.\\pipe\\",
        "\\\\.\\pipe\\a\0b",
    ] {
        assert_eq!(
            PipeServer::bind(bad).unwrap_err().kind(),
            WinErrorKind::InvalidInput,
            "{bad:?}"
        );
        assert_eq!(
            connect_client(bad, Duration::from_millis(10))
                .unwrap_err()
                .kind(),
            WinErrorKind::InvalidInput,
            "{bad:?}"
        );
    }
}

#[test]
fn accept_can_be_cancelled_from_another_thread() {
    let mut server = PipeServer::bind(&test_pipe_name()).unwrap();
    let stopper = server.stopper();
    let stop = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        stopper.stop().unwrap();
    });
    let started = Instant::now();
    let error = server.accept().unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::Cancelled, "{error}");
    assert!(started.elapsed() < Duration::from_secs(5));
    stop.join().unwrap();
    // The stop is sticky.
    assert_eq!(server.accept().unwrap_err().kind(), WinErrorKind::Cancelled);
}

#[test]
fn connecting_to_a_missing_pipe_fails_fast() {
    let started = Instant::now();
    let error = connect_client(&test_pipe_name(), Duration::from_secs(3)).unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::NotFound, "{error}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn reads_time_out_and_keep_the_connection_usable() {
    let (_server, mut accepted, mut client) = connected_pair();
    accepted.set_read_timeout(Some(Duration::from_millis(50)));
    assert_eq!(accepted.read_timeout(), Some(Duration::from_millis(50)));
    let mut buf = [0_u8; 8];
    let started = Instant::now();
    let error = accepted.read(&mut buf).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::TimedOut);
    assert!(started.elapsed() >= Duration::from_millis(40));
    accepted.set_read_timeout(Some(Duration::ZERO));
    assert_eq!(
        accepted.read(&mut buf).unwrap_err().kind(),
        ErrorKind::WouldBlock
    );
    client.write_all(b"late").unwrap();
    accepted.set_read_timeout(Some(Duration::from_secs(5)));
    let mut got = [0_u8; 4];
    accepted.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"late");
}

#[test]
fn a_canceller_aborts_a_blocked_read() {
    let (_server, mut accepted, _client) = connected_pair();
    let canceller = accepted.canceller();
    let cancel = thread::spawn(move || {
        thread::sleep(Duration::from_millis(50));
        canceller.cancel().unwrap();
    });
    let mut buf = [0_u8; 8];
    let error = accepted.read(&mut buf).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::ConnectionAborted);
    cancel.join().unwrap();
    // Cancellation is permanent for the connection.
    assert_eq!(
        accepted.write(b"x").unwrap_err().kind(),
        ErrorKind::ConnectionAborted
    );
}

#[test]
fn the_reader_sees_buffered_bytes_then_end_of_stream() {
    let (_server, mut accepted, mut client) = connected_pair();
    client.write_all(b"abc").unwrap();
    drop(client);
    let mut all = Vec::new();
    accepted.read_to_end(&mut all).unwrap();
    assert_eq!(all, b"abc");
    assert_eq!(
        accepted.write(b"x").unwrap_err().kind(),
        ErrorKind::BrokenPipe
    );
}

#[test]
fn both_ends_verify_the_peer_as_same_user_and_session() {
    let (_server, accepted, client) = connected_pair();
    let pid = std::process::id();
    let sid = current_user_sid_string().unwrap();
    let session = current_session_id().unwrap();
    assert_eq!(accepted.client_process_id().unwrap(), pid);
    assert_eq!(client.server_process_id().unwrap(), pid);
    assert_eq!(accepted.peer_process_id().unwrap(), pid);

    let peer = accepted.verify_peer_same_user_and_session().unwrap();
    assert_eq!(peer.process_id, pid);
    assert_eq!(peer.session_id, session);
    assert_eq!(peer.user_sid, sid);
    assert_eq!(peer.method, PeerUserCheck::ProcessToken);

    let peer = client.verify_peer_same_user_and_session().unwrap();
    assert_eq!(peer.process_id, pid);
    assert_eq!(peer.user_sid, sid);

    // The fallbacks used when the peer's token cannot be opened.
    assert_eq!(accepted.client_user_sid_by_impersonation().unwrap(), sid);
    assert_eq!(client.server_pipe_owner_sid().unwrap(), sid);
    assert!(client.client_user_sid_by_impersonation().is_err());
}

#[test]
fn large_transfers_complete_in_both_directions() {
    let (_server, mut accepted, mut client) = connected_pair();
    let payload: Vec<u8> = (0..1_048_576_u32).map(|i| (i % 251) as u8).collect();
    let expected = payload.clone();
    let writer = thread::spawn(move || {
        client.write_all(&payload).unwrap();
        let mut echo = vec![0_u8; payload.len()];
        client.read_exact(&mut echo).unwrap();
        assert_eq!(echo, payload);
    });
    let mut received = vec![0_u8; expected.len()];
    accepted.read_exact(&mut received).unwrap();
    assert_eq!(received, expected);
    accepted.write_all(&received).unwrap();
    writer.join().unwrap();
}

#[test]
fn successive_accepts_serve_concurrent_clients() {
    let name = test_pipe_name();
    let mut server = PipeServer::bind(&name).unwrap();
    let clients: Vec<_> = (0..3_u8)
        .map(|index| {
            let name = name.clone();
            thread::spawn(move || {
                let mut client = connect_client(&name, CONNECT_TIMEOUT).unwrap();
                client.write_all(&[index]).unwrap();
                let mut echo = [0_u8; 1];
                client.read_exact(&mut echo).unwrap();
                assert_eq!(echo[0], index + 100);
            })
        })
        .collect();
    for _ in 0..3 {
        let mut accepted = server.accept().unwrap();
        let mut byte = [0_u8; 1];
        accepted.read_exact(&mut byte).unwrap();
        accepted.write_all(&[byte[0] + 100]).unwrap();
    }
    for client in clients {
        client.join().unwrap();
    }
}

#[test]
fn ipc_sessions_handshake_and_exchange_commands_over_the_pipe() {
    let name = test_pipe_name();
    let mut server = PipeServer::bind(&name).unwrap();
    let mut runtime = RuntimeSnapshot::defaults_v1();
    runtime.config_revision = 3;
    let mut state = PublishedState::new(11, Arc::new(runtime)).unwrap();
    state.set_macros(Arc::new(MacroSnapshot::empty(4))).unwrap();

    let server_thread = thread::spawn(move || {
        let connection = server.accept().unwrap();
        connection.verify_peer_same_user_and_session().unwrap();
        let mut session = ServerSession::new(connection, NonZeroU64::new(9).unwrap());
        let hello = session.handshake(&state).unwrap();
        assert_eq!(hello.client_kind, ClientKind::Tip);
        assert_eq!(hello.pid, std::process::id());
        let mut handler = |_: NonZeroU64, command: &Command| {
            if *command == Command::ToggleMode {
                ErrorCode::OK
            } else {
                ErrorCode::COMMAND_UNSUPPORTED
            }
        };
        let mut events = Vec::new();
        loop {
            match session.serve_one(&mut handler).unwrap() {
                Some(ServerEvent::Goodbye) => break,
                Some(event) => events.push(event),
                None => {}
            }
        }
        events
    });

    let pipe = connect_client(&name, CONNECT_TIMEOUT).unwrap();
    let mut client = ClientSession::new(ClientIdentity::current(ClientKind::Tip));
    let info = client.connect(pipe).unwrap();
    assert_eq!(info.client_id.get(), 9);
    assert_eq!(info.server_revision, 11);
    assert_eq!(client.runtime().unwrap().config_revision, 3);
    assert_eq!(client.macros().unwrap().macro_revision, 4);

    let request = client.send_command(Command::ToggleMode).unwrap();
    match client.recv().unwrap() {
        Some(ClientEvent::CommandResult { request_id, ok, .. }) => {
            assert_eq!(request_id, request);
            assert!(ok);
        }
        other => panic!("unexpected {other:?}"),
    }
    let ping = client.ping().unwrap();
    assert_eq!(
        client.recv().unwrap(),
        Some(ClientEvent::Pong { request_id: ping })
    );
    client.close().unwrap();

    let events = server_thread.join().unwrap();
    assert!(matches!(
        events.first(),
        Some(ServerEvent::Command {
            command: Command::ToggleMode,
            code: ErrorCode::OK,
            ..
        })
    ));
    assert!(matches!(events.get(1), Some(ServerEvent::Ping { .. })));
}

#[test]
fn ipc_reads_time_out_into_ok_none() {
    let (_server, accepted, client) = connected_pair();
    let mut accepted = accepted;
    accepted.set_read_timeout(Some(Duration::from_millis(30)));
    let mut session = ServerSession::new(accepted, NonZeroU64::new(1).unwrap());
    // A handshake needs a Hello; with none arriving the read times out and the handshake fails
    // without hanging.
    let state = PublishedState::new(1, Arc::new(RuntimeSnapshot::defaults_v1())).unwrap();
    assert!(session.handshake(&state).is_err());
    drop(client);
}
