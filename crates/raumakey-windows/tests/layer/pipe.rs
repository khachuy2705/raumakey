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
use raumakey_windows::error::WinErrorKind;
use raumakey_windows::pipe::{PeerUserCheck, PipeConnection, PipeRole, PipeServer, connect_client};
use raumakey_windows::sid::{current_session_id, current_user_sid_string};

use crate::support::unique_suffix;

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
