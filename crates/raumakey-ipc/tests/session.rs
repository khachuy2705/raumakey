use std::io::{self, Cursor, Read, Write};
use std::num::NonZeroU64;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use raumakey_ipc::client::{ClientEvent, ClientIdentity, ClientSession};
use raumakey_ipc::error::{ErrorCode, IpcError, Limit, MalformedReason};
use raumakey_ipc::frame::{
    EncodedPayload, Frame, FrameDecoder, MAX_PAYLOAD, PROTOCOL_VERSION, encode_frame,
};
use raumakey_ipc::memory::{MemoryPipe, duplex};
use raumakey_ipc::message::{
    Arch, ClientKind, Command, MAX_MACROS, MacroSnapshot, Message, MessageKind, RuntimeSnapshot,
};
use raumakey_ipc::server::{
    ClientHello, ClientIdAllocator, PublishOutcome, PublishedState, ServerEvent, ServerSession,
};
use raumakey_model::diag::{Component, DiagCode, DiagEvent};
use raumakey_model::host::ExeName;
use raumakey_model::hotkey::{Hotkey, HotkeyKey};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::policy::ProbeRecord;

const SERVER_ID: u64 = 7;

fn id(value: u64) -> NonZeroU64 {
    NonZeroU64::new(value).unwrap()
}

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

fn identity() -> ClientIdentity {
    ClientIdentity {
        client_kind: ClientKind::Tip,
        pid: 4242,
        arch: Arch::X64,
    }
}

fn runtime(revision: u64) -> RuntimeSnapshot {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = revision;
    snapshot
}

fn macros(revision: u64) -> MacroSnapshot {
    MacroSnapshot {
        macro_revision: revision,
        entries: vec![MacroEntry {
            id: MacroId(1),
            trigger: "ko".to_owned(),
            replacement: "không".to_owned(),
            enabled: true,
            boundary: BoundaryPolicy::Delimiter,
            case_policy: CasePolicy::Adapt,
        }],
    }
}

fn published(server_revision: u64, config: u64, macro_revision: u64) -> PublishedState {
    let mut state = PublishedState::new(server_revision, Arc::new(runtime(config))).unwrap();
    state.set_macros(Arc::new(macros(macro_revision))).unwrap();
    state
}

/// `count` entries whose replacements are `len` ASCII characters long.
fn macro_table(revision: u64, count: usize, len: usize) -> MacroSnapshot {
    let replacement = "x".repeat(len);
    MacroSnapshot {
        macro_revision: revision,
        entries: (0..count as u64)
            .map(|i| MacroEntry {
                id: MacroId(i),
                trigger: format!("k{i}"),
                replacement: replacement.clone(),
                enabled: true,
                boundary: BoundaryPolicy::Whitespace,
                case_policy: CasePolicy::Exact,
            })
            .collect(),
    }
}

/// [`MAX_MACROS`] entries whose JSON body is exactly `target` bytes long.
fn macro_table_with_body_len(revision: u64, target: u64) -> MacroSnapshot {
    let mut table = macro_table(revision, MAX_MACROS, 1);
    let base = table.encoded_len().unwrap();
    let deficit = usize::try_from(target - base).unwrap();
    let (each, extra) = (deficit / MAX_MACROS, deficit % MAX_MACROS);
    for (index, entry) in table.entries.iter_mut().enumerate() {
        let grow = each + usize::from(index < extra);
        entry.replacement.push_str(&"x".repeat(grow));
    }
    assert_eq!(table.encoded_len(), Ok(target));
    table
}

fn diag() -> DiagEvent {
    DiagEvent {
        timestamp_ms: 5,
        component: Component::Tsf,
        code: DiagCode(77),
        revision: Some(1),
        host_exe: None,
        vector_id: None,
    }
}

/// Runs the server handshake on a thread while the client connects on this one.
fn connect_pair(
    state: &PublishedState,
    client_timeout: Option<Duration>,
    server_timeout: Option<Duration>,
) -> (ClientSession<MemoryPipe>, ServerSession<MemoryPipe>) {
    let (mut client_end, mut server_end) = duplex();
    client_end.set_read_timeout(client_timeout);
    server_end.set_read_timeout(server_timeout);
    thread::scope(|scope| {
        let server = scope.spawn(move || {
            let mut server = ServerSession::new(server_end, id(SERVER_ID));
            server.handshake(state).unwrap();
            server
        });
        let mut client = ClientSession::new(identity());
        client.connect(client_end).unwrap();
        (client, server.join().unwrap())
    })
}

/// A hand-driven peer for sending frames a well-behaved session never produces.
struct RawPeer {
    pipe: MemoryPipe,
    decoder: FrameDecoder,
}

impl RawPeer {
    fn new(pipe: MemoryPipe) -> Self {
        Self {
            pipe,
            decoder: FrameDecoder::new(),
        }
    }

    fn send(&mut self, client_id: u64, request_id: u64, message: Message) {
        let bytes = encode_frame(&Frame::new(client_id, request_id, message)).unwrap();
        self.pipe.write_all(&bytes).unwrap();
    }

    fn recv(&mut self) -> Frame {
        let mut buf = [0_u8; 4096];
        loop {
            if let Some(next) = self.decoder.next_frame() {
                return next.unwrap();
            }
            let n = self.pipe.read(&mut buf).unwrap();
            assert_ne!(n, 0, "peer closed");
            self.decoder.feed(&buf[..n]);
        }
    }
}

/// Plays the server side of a handshake by hand.
fn raw_server_handshake(peer: &mut RawPeer, client_id: u64) {
    let hello = peer.recv();
    assert!(matches!(hello.message, Message::Hello { .. }));
    peer.send(
        client_id,
        hello.request_id,
        Message::Welcome {
            client_id,
            server_revision: 1,
        },
    );
    peer.send(client_id, 1, Message::RuntimeSnapshot(runtime(1)));
    peer.send(client_id, 2, Message::MacroSnapshot(macros(1)));
}

#[test]
fn handshake_assigns_the_client_id_and_delivers_both_snapshots() {
    let state = published(3, 10, 20);
    let (client_end, server_end) = duplex();
    let (client, server, info, hello) = thread::scope(|scope| {
        let server = scope.spawn(|| {
            let mut server = ServerSession::new(server_end, id(SERVER_ID));
            let hello = server.handshake(&state).unwrap();
            (server, hello)
        });
        let mut client = ClientSession::new(identity());
        let info = client.connect(client_end).unwrap();
        let (server, hello) = server.join().unwrap();
        (client, server, info, hello)
    });
    assert_eq!(
        hello,
        ClientHello {
            client_kind: ClientKind::Tip,
            pid: 4242,
            arch: Arch::X64,
            protocol: PROTOCOL_VERSION,
        }
    );
    assert_eq!(server.hello(), Some(&hello));
    assert!(server.is_ready());
    assert_eq!(server.client_id(), id(SERVER_ID));
    assert_eq!(info.client_id, id(SERVER_ID));
    assert_eq!(info.server_revision, 3);
    assert!(client.is_connected());
    assert_eq!(client.client_id(), Some(id(SERVER_ID)));
    assert_eq!(client.server_revision(), Some(3));
    assert_eq!(client.runtime().unwrap().as_ref(), &runtime(10));
    assert_eq!(client.macros().unwrap().as_ref(), &macros(20));
    assert_eq!(client.vietnamese_mode(), Some(true));
}

#[test]
fn published_state_encodes_borrowed_snapshots_like_owned_messages() {
    let state = published(1, 2, 3);
    assert_eq!(state.server_revision(), 1);
    assert_eq!(state.runtime().config_revision, 2);
    assert_eq!(state.macros().macro_revision, 3);
    let borrowed = EncodedPayload::runtime_snapshot(state.runtime()).unwrap();
    let owned = EncodedPayload::encode(&Message::RuntimeSnapshot(runtime(2))).unwrap();
    assert_eq!(borrowed, owned);
    let borrowed = EncodedPayload::macro_snapshot(state.macros()).unwrap();
    let owned = EncodedPayload::encode(&Message::MacroSnapshot(macros(3))).unwrap();
    assert_eq!(borrowed, owned);
}

#[test]
fn published_state_rejects_invalid_snapshots_and_keeps_the_previous_ones() {
    let mut bad = runtime(2);
    bad.options.context_limit_codepoints = 0;
    let result = PublishedState::new(1, Arc::new(bad.clone()));
    assert_eq!(
        result.err(),
        Some(IpcError::Malformed(MalformedReason::InvalidOptions))
    );

    let mut state = published(1, 1, 1);
    assert_eq!(
        state.set_runtime(Arc::new(bad)),
        Err(IpcError::Malformed(MalformedReason::InvalidOptions))
    );
    assert_eq!(state.runtime().as_ref(), &runtime(1));
    assert_eq!(state.macros().as_ref(), &macros(1));
}

/// The longest executable name: `İ` (U+0130) lowercases to two characters, so 127 of them and
/// one letter make `ExeName::MAX_LEN` characters once stored. `ExeName::new` refuses a name
/// that lowercasing makes longer, so every name reads back on the peer.
fn longest_exe() -> ExeName {
    assert!(ExeName::new(&"\u{130}".repeat(128)).is_err());
    let name = ExeName::new(&format!("{}x", "\u{130}".repeat(127))).unwrap();
    assert_eq!(name.as_str().chars().count(), ExeName::MAX_LEN);
    name
}

#[test]
fn a_runtime_snapshot_no_client_could_decode_is_refused_before_it_is_published() {
    let mut bad = runtime(2);
    // Written as "Ctrl", which the client's parser rejects.
    bad.hotkeys.toggle_mode = vec![Hotkey {
        ctrl: true,
        shift: false,
        alt: false,
        win: false,
        key: HotkeyKey::ModifiersOnly,
    }];
    let refused = IpcError::Malformed(MalformedReason::InvalidHotkey);
    assert_eq!(bad.check_publishable(), Err(refused));
    assert_eq!(
        PublishedState::new(1, Arc::new(bad.clone())).err(),
        Some(refused)
    );
    let mut state = published(1, 1, 1);
    assert_eq!(state.set_runtime(Arc::new(bad)), Err(refused));
    assert_eq!(state.runtime().as_ref(), &runtime(1));
    // Clients keep connecting and receive the snapshot published before.
    let (client, server) = connect_pair(&state, None, None);
    assert!(server.is_ready());
    assert_eq!(client.runtime().unwrap().as_ref(), &runtime(1));
}

#[test]
fn a_probe_report_naming_the_longest_executable_name_reaches_the_server() {
    let state = published(1, 1, 1);
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), Some(ms(50)));
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    let report = Command::ReportProbe(ProbeRecord {
        exe: longest_exe(),
        probe_version: 1,
        tsf_range_edit: false,
    });
    let request_id = client.send_command(report.clone()).unwrap();
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Command {
            request_id,
            command: report,
            code: ErrorCode::OK
        }))
    );
    assert_eq!(
        client.recv(),
        Ok(Some(ClientEvent::CommandResult {
            request_id,
            ok: true,
            code: ErrorCode::OK
        }))
    );
    // Both sides go on.
    let ping = client.ping().unwrap();
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Ping { request_id: ping }))
    );
    assert_eq!(
        client.recv(),
        Ok(Some(ClientEvent::Pong { request_id: ping }))
    );
}

#[test]
fn a_new_published_state_carries_an_empty_macro_table_until_one_is_set() {
    let mut state = PublishedState::new(4, Arc::new(runtime(9))).unwrap();
    assert_eq!(state.server_revision(), 4);
    assert_eq!(state.macros().as_ref(), &MacroSnapshot::empty(0));
    state.set_macros(Arc::new(macros(3))).unwrap();
    state.set_server_revision(5);
    assert_eq!(state.server_revision(), 5);
    assert_eq!(state.macros().as_ref(), &macros(3));
    assert_eq!(state.runtime().as_ref(), &runtime(9));
}

#[test]
fn a_macro_table_that_does_not_fit_never_blocks_the_runtime_snapshot() {
    let mut state = PublishedState::new(1, Arc::new(runtime(10))).unwrap();
    // Within the entry limit, but the body exceeds 8 MiB.
    let too_big = macro_table(5, MAX_MACROS, 420);
    let Err(IpcError::Oversize { payload_len, max }) = state.set_macros(Arc::new(too_big)) else {
        panic!("an 8 MiB+ macro table must be refused");
    };
    assert_eq!(max, MAX_PAYLOAD);
    assert!(payload_len > u64::from(MAX_PAYLOAD));
    // Over the entry limit.
    let too_many = macro_table(6, MAX_MACROS + 1, 1);
    assert_eq!(
        state.set_macros(Arc::new(too_many)),
        Err(IpcError::LimitExceeded {
            limit: Limit::Macros,
            count: MAX_MACROS as u64 + 1
        })
    );
    assert_eq!(state.macros().as_ref(), &MacroSnapshot::empty(0));

    // The handshake still works and delivers the runtime snapshot.
    let (client, server) = connect_pair(&state, None, None);
    assert!(server.is_ready());
    assert_eq!(client.runtime().unwrap().config_revision, 10);
    assert_eq!(client.macros().unwrap().as_ref(), &MacroSnapshot::empty(0));
}

#[test]
fn a_table_published_under_the_placeholder_revision_still_reaches_connected_clients() {
    let mut state = PublishedState::new(1, Arc::new(runtime(1))).unwrap();
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), None);
    assert_eq!(client.macros().unwrap().as_ref(), &MacroSnapshot::empty(0));
    // A store that was never edited (or a migrated one) is at revision 0 as well.
    state.set_macros(Arc::new(macros(0))).unwrap();
    assert_eq!(
        server.publish(&state),
        Ok(PublishOutcome {
            runtime_sent: false,
            macros_sent: true
        })
    );
    match client.recv() {
        Ok(Some(ClientEvent::MacroSnapshot(snapshot))) => {
            assert_eq!(snapshot.as_ref(), &macros(0));
        }
        other => panic!("expected a macro snapshot, got {other:?}"),
    }
    // Publishing the same content again sends nothing.
    assert_eq!(server.publish(&state), Ok(PublishOutcome::default()));
    assert_eq!(client.recv(), Ok(None));
}

#[test]
fn a_refused_macro_table_keeps_the_previous_one_while_runtime_updates_flow() {
    let mut state = published(1, 10, 3);
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), None);
    assert!(
        state
            .set_macros(Arc::new(macro_table(4, 2_100, 4_000)))
            .is_err()
    );
    state.set_runtime(Arc::new(runtime(11))).unwrap();
    state.set_server_revision(2);
    assert_eq!(
        server.publish(&state),
        Ok(PublishOutcome {
            runtime_sent: true,
            macros_sent: false
        })
    );
    match client.recv() {
        Ok(Some(ClientEvent::RuntimeSnapshot(snapshot))) => {
            assert_eq!(snapshot.config_revision, 11);
        }
        other => panic!("expected a runtime snapshot, got {other:?}"),
    }
    assert_eq!(client.recv(), Ok(None));
    assert_eq!(client.macros().unwrap().macro_revision, 3);
}

#[test]
fn an_at_limit_macro_table_is_published_and_delivered() {
    let at_limit = macro_table_with_body_len(8, u64::from(MAX_PAYLOAD));
    assert_eq!(at_limit.check_publishable(), Ok(()));
    let one_byte_over = macro_table_with_body_len(8, u64::from(MAX_PAYLOAD) + 1);
    assert_eq!(
        one_byte_over.check_publishable(),
        Err(IpcError::Oversize {
            payload_len: u64::from(MAX_PAYLOAD) + 1,
            max: MAX_PAYLOAD
        })
    );

    let mut state = published(1, 1, 1);
    assert!(state.set_macros(Arc::new(one_byte_over)).is_err());
    state.set_macros(Arc::new(at_limit.clone())).unwrap();
    let (client, _server) = connect_pair(&state, None, None);
    let delivered = client.macros().unwrap();
    assert_eq!(delivered.entries.len(), MAX_MACROS);
    assert_eq!(delivered.as_ref(), &at_limit);
}

#[test]
fn server_rejects_an_unsupported_protocol_and_tells_the_client() {
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(client_end);
    peer.send(
        0,
        1,
        Message::Hello {
            client_kind: ClientKind::Tool,
            pid: 1,
            arch: Arch::X86,
            protocol: PROTOCOL_VERSION + 1,
        },
    );
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    assert_eq!(
        server.handshake(&published(1, 1, 1)),
        Err(IpcError::ProtocolMismatch {
            protocol: PROTOCOL_VERSION + 1
        })
    );
    assert!(server.is_closed());
    assert_eq!(
        peer.recv().message,
        Message::Error {
            code: ErrorCode::UNSUPPORTED_PROTOCOL
        }
    );
}

#[test]
fn server_requires_hello_first() {
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(client_end);
    peer.send(0, 1, Message::Ping);
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    assert_eq!(
        server.handshake(&published(1, 1, 1)),
        Err(IpcError::UnexpectedMessage(MessageKind::Ping))
    );
    assert_eq!(
        peer.recv().message,
        Message::Error {
            code: ErrorCode::UNEXPECTED_MESSAGE
        }
    );
}

#[test]
fn server_rejects_a_hello_that_already_carries_a_client_id() {
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(client_end);
    peer.send(
        5,
        1,
        Message::Hello {
            client_kind: ClientKind::Tip,
            pid: 1,
            arch: Arch::X64,
            protocol: PROTOCOL_VERSION,
        },
    );
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    assert_eq!(
        server.handshake(&published(1, 1, 1)),
        Err(IpcError::ClientIdMismatch {
            expected: 0,
            actual: 5
        })
    );
}

#[test]
fn server_handshake_reports_a_closed_stream() {
    let (client_end, server_end) = duplex();
    drop(client_end);
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    assert_eq!(server.handshake(&published(1, 1, 1)), Err(IpcError::Closed));
    assert!(server.is_closed());
}

#[test]
fn server_rejects_a_frame_with_another_client_id() {
    let state = published(1, 1, 1);
    let (client_end, mut server_end) = duplex();
    server_end.set_read_timeout(Some(ms(1_000)));
    let mut peer = RawPeer::new(client_end);
    peer.send(
        0,
        1,
        Message::Hello {
            client_kind: ClientKind::Tip,
            pid: 1,
            arch: Arch::X64,
            protocol: PROTOCOL_VERSION,
        },
    );
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    server.handshake(&state).unwrap();
    for _ in 0..3 {
        peer.recv();
    }
    peer.send(SERVER_ID + 1, 2, Message::Ping);
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    assert_eq!(
        server.serve_one(&mut handler),
        Err(IpcError::ClientIdMismatch {
            expected: SERVER_ID,
            actual: SERVER_ID + 1
        })
    );
    assert!(server.is_closed());
    assert_eq!(
        peer.recv().message,
        Message::Error {
            code: ErrorCode::CLIENT_ID_MISMATCH
        }
    );
    assert_eq!(server.serve_one(&mut handler), Err(IpcError::NotConnected));
}

#[test]
fn client_rejects_a_frame_with_another_client_id() {
    let (mut client_end, server_end) = duplex();
    client_end.set_read_timeout(Some(ms(1_000)));
    let mut peer = RawPeer::new(server_end);
    let mut client = thread::scope(|scope| {
        let server = scope.spawn(|| {
            raw_server_handshake(&mut peer, 9);
        });
        let mut client = ClientSession::new(identity());
        client.connect(client_end).unwrap();
        server.join().unwrap();
        client
    });
    peer.send(10, 3, Message::ResetAll);
    assert_eq!(
        client.recv(),
        Err(IpcError::ClientIdMismatch {
            expected: 9,
            actual: 10
        })
    );
    assert!(!client.is_connected());
    assert!(client.runtime().is_some());
    assert_eq!(
        peer.recv().message,
        Message::Error {
            code: ErrorCode::CLIENT_ID_MISMATCH
        }
    );
}

#[test]
fn client_rejects_a_welcome_whose_header_and_body_ids_differ() {
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(server_end);
    let result = thread::scope(|scope| {
        let server = scope.spawn(|| {
            let hello = peer.recv();
            peer.send(
                4,
                hello.request_id,
                Message::Welcome {
                    client_id: 5,
                    server_revision: 1,
                },
            );
        });
        let mut client = ClientSession::new(identity());
        let result = client.connect(client_end);
        server.join().unwrap();
        assert!(!client.is_connected());
        result
    });
    assert_eq!(
        result,
        Err(IpcError::ClientIdMismatch {
            expected: 5,
            actual: 4
        })
    );
}

#[test]
fn client_reports_server_errors_and_unexpected_messages_during_the_handshake() {
    for (reply, expected) in [
        (
            Message::Error {
                code: ErrorCode::UNSUPPORTED_PROTOCOL,
            },
            IpcError::Remote(ErrorCode::UNSUPPORTED_PROTOCOL),
        ),
        (
            Message::Pong,
            IpcError::UnexpectedMessage(MessageKind::Pong),
        ),
    ] {
        let (client_end, server_end) = duplex();
        let mut peer = RawPeer::new(server_end);
        let mut client = ClientSession::new(identity());
        let result = thread::scope(|scope| {
            let server = scope.spawn(|| {
                let hello = peer.recv();
                peer.send(0, hello.request_id, reply);
            });
            let result = client.connect(client_end);
            server.join().unwrap();
            result
        });
        assert_eq!(result.err(), Some(expected));
        assert!(!client.is_connected());
        assert!(client.runtime().is_none());
    }
}

#[test]
fn a_handshake_failure_is_reported_with_the_id_welcome_assigned() {
    // Before `Welcome` no id is assigned: the report carries zero.
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(server_end);
    let mut client = ClientSession::new(identity());
    let (result, report) = thread::scope(|scope| {
        let server = scope.spawn(|| {
            let hello = peer.recv();
            peer.send(0, hello.request_id, Message::Pong);
            peer.recv()
        });
        (client.connect(client_end), server.join().unwrap())
    });
    assert_eq!(result, Err(IpcError::UnexpectedMessage(MessageKind::Pong)));
    assert_eq!(report.client_id, 0);

    // After a valid `Welcome`, a broken handshake (here a duplicate snapshot) is reported
    // with the assigned id.
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(server_end);
    let (result, report) = thread::scope(|scope| {
        let server = scope.spawn(|| {
            let hello = peer.recv();
            peer.send(
                SERVER_ID,
                hello.request_id,
                Message::Welcome {
                    client_id: SERVER_ID,
                    server_revision: 1,
                },
            );
            peer.send(SERVER_ID, 1, Message::RuntimeSnapshot(runtime(1)));
            peer.send(SERVER_ID, 2, Message::RuntimeSnapshot(runtime(1)));
            peer.recv()
        });
        (client.connect(client_end), server.join().unwrap())
    });
    assert_eq!(
        result,
        Err(IpcError::UnexpectedMessage(MessageKind::RuntimeSnapshot))
    );
    assert!(!client.is_connected());
    assert_eq!(report.client_id, SERVER_ID);
    assert_eq!(
        report.message,
        Message::Error {
            code: ErrorCode::UNEXPECTED_MESSAGE
        }
    );

    // A server session therefore surfaces the client's code instead of a client id mismatch.
    let (client_end, server_end) = duplex();
    let mut raw_client = RawPeer::new(client_end);
    raw_client.send(
        0,
        1,
        Message::Hello {
            client_kind: ClientKind::Tip,
            pid: 1,
            arch: Arch::X64,
            protocol: PROTOCOL_VERSION,
        },
    );
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    server.handshake(&published(1, 1, 1)).unwrap();
    raw_client.send(report.client_id, report.request_id, report.message);
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    assert_eq!(
        server.serve_one(&mut handler),
        Err(IpcError::Remote(ErrorCode::UNEXPECTED_MESSAGE))
    );
    assert!(server.is_closed());
}

#[test]
fn client_handshake_times_out_when_the_server_is_silent() {
    let (mut client_end, _server_end) = duplex();
    client_end.set_read_timeout(Some(ms(20)));
    let mut client = ClientSession::new(identity());
    assert_eq!(
        client.connect(client_end).err(),
        Some(IpcError::Io(std::io::ErrorKind::TimedOut))
    );
    assert!(!client.is_connected());
}

#[test]
fn publish_sends_only_new_revisions() {
    let state = published(1, 10, 20);
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), None);
    assert_eq!(
        server.publish(&state),
        Ok(PublishOutcome {
            runtime_sent: false,
            macros_sent: false
        })
    );
    assert_eq!(client.recv(), Ok(None));

    let config_changed = published(2, 11, 20);
    assert_eq!(
        server.publish(&config_changed),
        Ok(PublishOutcome {
            runtime_sent: true,
            macros_sent: false
        })
    );
    match client.recv() {
        Ok(Some(ClientEvent::RuntimeSnapshot(snapshot))) => {
            assert_eq!(snapshot.config_revision, 11);
        }
        other => panic!("expected a runtime snapshot, got {other:?}"),
    }
    assert_eq!(client.runtime().unwrap().config_revision, 11);

    let macros_changed = published(3, 11, 21);
    assert_eq!(
        server.publish(&macros_changed),
        Ok(PublishOutcome {
            runtime_sent: false,
            macros_sent: true
        })
    );
    match client.recv() {
        Ok(Some(ClientEvent::MacroSnapshot(snapshot))) => {
            assert_eq!(snapshot.macro_revision, 21);
        }
        other => panic!("expected a macro snapshot, got {other:?}"),
    }
    assert_eq!(client.macros().unwrap().macro_revision, 21);
    assert_eq!(client.recv(), Ok(None));
}

#[test]
fn commands_are_dispatched_and_answered_with_the_same_request_id() {
    let state = published(1, 1, 1);
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), None);
    let mut seen = Vec::new();
    let mut handler = |client_id: NonZeroU64, command: &Command| {
        seen.push((client_id, command.clone()));
        match command {
            Command::HardReset => ErrorCode::COMMAND_FAILED,
            _ => ErrorCode::OK,
        }
    };

    let first = client.send_command(Command::SetMode(false)).unwrap();
    let second = client.send_command(Command::HardReset).unwrap();
    assert_ne!(first, second);
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Command {
            request_id: first,
            command: Command::SetMode(false),
            code: ErrorCode::OK
        }))
    );
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Command {
            request_id: second,
            command: Command::HardReset,
            code: ErrorCode::COMMAND_FAILED
        }))
    );
    assert_eq!(
        seen,
        [
            (id(SERVER_ID), Command::SetMode(false)),
            (id(SERVER_ID), Command::HardReset)
        ]
    );
    assert_eq!(
        client.recv(),
        Ok(Some(ClientEvent::CommandResult {
            request_id: first,
            ok: true,
            code: ErrorCode::OK
        }))
    );
    assert_eq!(
        client.recv(),
        Ok(Some(ClientEvent::CommandResult {
            request_id: second,
            ok: false,
            code: ErrorCode::COMMAND_FAILED
        }))
    );
}

#[test]
fn ping_and_pong_work_in_both_directions() {
    let state = published(1, 1, 1);
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), Some(ms(50)));
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;

    let client_ping = client.ping().unwrap();
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Ping {
            request_id: client_ping
        }))
    );
    assert_eq!(
        client.recv(),
        Ok(Some(ClientEvent::Pong {
            request_id: client_ping
        }))
    );

    let server_ping = server.ping().unwrap();
    // The client answers automatically and then reports that nothing else arrived.
    assert_eq!(client.recv(), Ok(None));
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Pong {
            request_id: server_ping
        }))
    );
    assert_eq!(server.serve_one(&mut handler), Ok(None));
}

#[test]
fn mode_changes_and_reset_all_reach_the_client() {
    let state = published(1, 1, 1);
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), None);
    server.send_mode_changed(false, 9).unwrap();
    server.send_reset_all().unwrap();
    assert_eq!(
        client.recv(),
        Ok(Some(ClientEvent::ModeChanged {
            vietnamese_mode: false,
            revision: 9
        }))
    );
    assert_eq!(client.vietnamese_mode(), Some(false));
    assert_eq!(client.recv(), Ok(Some(ClientEvent::ResetAll)));
}

#[test]
fn diagnostics_are_sent_only_after_the_user_opted_in() {
    let state = published(1, 1, 1);
    let (mut client, mut server) = connect_pair(&state, Some(ms(50)), Some(ms(50)));
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    assert_eq!(client.send_diagnostic(&diag()), Ok(false));
    assert_eq!(server.serve_one(&mut handler), Ok(None));

    let mut opted_in = runtime(2);
    opted_in.diagnostics_enabled = true;
    let mut state = state;
    state.set_runtime(Arc::new(opted_in)).unwrap();
    server.publish(&state).unwrap();
    assert!(matches!(
        client.recv(),
        Ok(Some(ClientEvent::RuntimeSnapshot(_)))
    ));
    assert_eq!(client.send_diagnostic(&diag()), Ok(true));
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Diagnostic(diag())))
    );
}

#[test]
fn the_server_drops_diagnostics_unless_the_user_opted_in() {
    let mut state = published(1, 1, 1);
    assert!(!state.runtime().diagnostics_enabled);
    let (client_end, mut server_end) = duplex();
    server_end.set_read_timeout(Some(ms(50)));
    let mut peer = RawPeer::new(client_end);
    peer.send(
        0,
        1,
        Message::Hello {
            client_kind: ClientKind::Tip,
            pid: 1,
            arch: Arch::X64,
            protocol: PROTOCOL_VERSION,
        },
    );
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    server.handshake(&state).unwrap();
    for _ in 0..3 {
        peer.recv();
    }
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;

    // A client that ignores the opt-in: its event is dropped and the session goes on.
    peer.send(SERVER_ID, 2, Message::Diagnostic(diag()));
    peer.send(SERVER_ID, 3, Message::Ping);
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Ping { request_id: 3 }))
    );
    assert!(server.is_ready());
    assert_eq!(peer.recv().message, Message::Pong);

    // Once the published snapshot opts in, events reach the controller.
    let mut opted_in = runtime(2);
    opted_in.diagnostics_enabled = true;
    state.set_runtime(Arc::new(opted_in)).unwrap();
    server.publish(&state).unwrap();
    peer.recv();
    peer.send(SERVER_ID, 4, Message::Diagnostic(diag()));
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Diagnostic(diag())))
    );

    // Opting out again drops them again.
    state.set_runtime(Arc::new(runtime(3))).unwrap();
    server.publish(&state).unwrap();
    peer.recv();
    peer.send(SERVER_ID, 5, Message::Diagnostic(diag()));
    assert_eq!(server.serve_one(&mut handler), Ok(None));
    assert!(server.is_ready());
}

#[test]
fn goodbye_from_the_client_closes_the_server_session() {
    let state = published(1, 1, 1);
    let (mut client, mut server) = connect_pair(&state, None, None);
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    client.close().unwrap();
    assert!(!client.is_connected());
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Goodbye))
    );
    assert!(server.is_closed());
    assert_eq!(server.serve_one(&mut handler), Err(IpcError::NotConnected));
    assert_eq!(
        client.send_command(Command::ToggleMode),
        Err(IpcError::NotConnected)
    );
}

#[test]
fn goodbye_from_the_server_disconnects_the_client_but_keeps_snapshots() {
    let state = published(1, 4, 5);
    let (mut client, mut server) = connect_pair(&state, None, None);
    server.close().unwrap();
    assert!(server.is_closed());
    assert_eq!(client.recv(), Ok(Some(ClientEvent::Goodbye)));
    assert!(!client.is_connected());
    assert_eq!(client.client_id(), None);
    assert_eq!(client.runtime().unwrap().config_revision, 4);
    assert_eq!(client.macros().unwrap().macro_revision, 5);
    assert_eq!(client.recv(), Err(IpcError::NotConnected));
}

#[test]
fn client_rejects_a_second_welcome_after_the_handshake() {
    let (mut client_end, server_end) = duplex();
    client_end.set_read_timeout(Some(ms(1_000)));
    let mut peer = RawPeer::new(server_end);
    let mut client = thread::scope(|scope| {
        let server = scope.spawn(|| raw_server_handshake(&mut peer, 9));
        let mut client = ClientSession::new(identity());
        client.connect(client_end).unwrap();
        server.join().unwrap();
        client
    });
    peer.send(
        9,
        3,
        Message::Welcome {
            client_id: 9,
            server_revision: 2,
        },
    );
    assert_eq!(
        client.recv(),
        Err(IpcError::UnexpectedMessage(MessageKind::Welcome))
    );
    assert!(!client.is_connected());
    assert_eq!(client.server_revision(), None);
    assert_eq!(client.runtime().unwrap().config_revision, 1);
    let report = peer.recv();
    assert_eq!(report.client_id, 9);
    assert_eq!(
        report.message,
        Message::Error {
            code: ErrorCode::UNEXPECTED_MESSAGE
        }
    );
}

#[test]
fn server_errors_after_the_handshake_disconnect_the_client() {
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(server_end);
    let mut client = thread::scope(|scope| {
        let server = scope.spawn(|| raw_server_handshake(&mut peer, 3));
        let mut client = ClientSession::new(identity());
        client.connect(client_end).unwrap();
        server.join().unwrap();
        client
    });
    peer.send(
        3,
        9,
        Message::Error {
            code: ErrorCode::SHUTTING_DOWN,
        },
    );
    assert_eq!(
        client.recv(),
        Err(IpcError::Remote(ErrorCode::SHUTTING_DOWN))
    );
    assert!(!client.is_connected());
}

#[test]
fn client_keeps_its_last_snapshot_and_backs_off_across_reconnects() {
    let state = published(1, 6, 7);
    let (mut client, server) = connect_pair(&state, None, None);
    assert_eq!(client.backoff().failures(), 0);
    drop(server);
    assert_eq!(client.recv(), Err(IpcError::Closed));
    assert!(!client.is_connected());
    assert_eq!(client.runtime().unwrap().config_revision, 6);
    assert_eq!(client.next_reconnect_delay(), ms(100));

    let (dead_end, peer) = duplex();
    drop(peer);
    assert!(matches!(client.connect(dead_end), Err(IpcError::Io(_))));
    assert_eq!(client.next_reconnect_delay(), ms(200));
    assert_eq!(client.runtime().unwrap().config_revision, 6);

    let newer = published(2, 8, 9);
    let (client_end, server_end) = duplex();
    thread::scope(|scope| {
        let server = scope.spawn(|| {
            let mut server = ServerSession::new(server_end, id(11));
            server.handshake(&newer).unwrap();
            server
        });
        client.connect(client_end).unwrap();
        drop(server.join().unwrap());
    });
    assert_eq!(client.client_id(), Some(id(11)));
    assert_eq!(client.runtime().unwrap().config_revision, 8);
    assert_eq!(client.backoff().failures(), 0);
    assert_eq!(client.backoff().peek(), ms(100));
}

#[test]
fn partial_frames_survive_read_timeouts() {
    let (mut client_end, server_end) = duplex();
    client_end.set_read_timeout(Some(ms(30)));
    let mut peer = RawPeer::new(server_end);
    let mut client = thread::scope(|scope| {
        let server = scope.spawn(|| raw_server_handshake(&mut peer, 2));
        let mut client = ClientSession::new(identity());
        client.connect(client_end).unwrap();
        server.join().unwrap();
        client
    });
    let bytes = encode_frame(&Frame::new(
        2,
        5,
        Message::ModeChanged {
            vietnamese_mode: false,
            revision: 4,
        },
    ))
    .unwrap();
    let (head, tail) = bytes.split_at(17);
    peer.pipe.write_all(head).unwrap();
    assert_eq!(client.recv(), Ok(None));
    assert!(client.is_connected());
    peer.pipe.write_all(tail).unwrap();
    assert_eq!(
        client.recv(),
        Ok(Some(ClientEvent::ModeChanged {
            vietnamese_mode: false,
            revision: 4
        }))
    );
}

#[test]
fn server_rejects_server_only_kinds_from_clients() {
    let state = published(1, 1, 1);
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(client_end);
    peer.send(
        0,
        1,
        Message::Hello {
            client_kind: ClientKind::Tip,
            pid: 1,
            arch: Arch::X64,
            protocol: PROTOCOL_VERSION,
        },
    );
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    server.handshake(&state).unwrap();
    peer.send(SERVER_ID, 2, Message::ResetAll);
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    assert_eq!(
        server.serve_one(&mut handler),
        Err(IpcError::UnexpectedMessage(MessageKind::ResetAll))
    );
}

#[test]
fn server_rejects_a_second_hello() {
    let state = published(1, 1, 1);
    let (client_end, server_end) = duplex();
    let mut peer = RawPeer::new(client_end);
    let hello = Message::Hello {
        client_kind: ClientKind::Tip,
        pid: 1,
        arch: Arch::X64,
        protocol: PROTOCOL_VERSION,
    };
    peer.send(0, 1, hello.clone());
    let mut server = ServerSession::new(server_end, id(SERVER_ID));
    server.handshake(&state).unwrap();
    peer.send(SERVER_ID, 2, hello);
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    assert_eq!(
        server.serve_one(&mut handler),
        Err(IpcError::UnexpectedMessage(MessageKind::Hello))
    );
}

/// A transport that replays fixed input, then reports end-of-stream, and records every write.
struct Scripted {
    input: Cursor<Vec<u8>>,
    output: Arc<Mutex<Vec<u8>>>,
}

impl Scripted {
    fn new(input: Vec<u8>) -> (Self, Arc<Mutex<Vec<u8>>>) {
        let output = Arc::new(Mutex::new(Vec::new()));
        let stream = Self {
            input: Cursor::new(input),
            output: Arc::clone(&output),
        };
        (stream, output)
    }
}

impl Read for Scripted {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl Write for Scripted {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.output.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// The kinds of the complete frames in `bytes`.
fn kinds_written(bytes: &Mutex<Vec<u8>>) -> Vec<MessageKind> {
    let mut decoder = FrameDecoder::new();
    decoder.feed(&bytes.lock().unwrap());
    let mut kinds = Vec::new();
    while let Some(frame) = decoder.next_frame() {
        kinds.push(frame.unwrap().message.kind());
    }
    assert!(decoder.is_idle());
    kinds
}

/// What a server sends for a handshake that assigns `client_id`.
fn server_handshake_bytes(client_id: u64) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (request_id, message) in [
        Message::Welcome {
            client_id,
            server_revision: 1,
        },
        Message::RuntimeSnapshot(runtime(1)),
        Message::MacroSnapshot(macros(1)),
    ]
    .into_iter()
    .enumerate()
    {
        let frame = Frame::new(client_id, request_id as u64, message);
        bytes.extend(encode_frame(&frame).unwrap());
    }
    bytes
}

fn hello_bytes() -> Vec<u8> {
    let hello = Message::Hello {
        client_kind: ClientKind::Tip,
        pid: 1,
        arch: Arch::X64,
        protocol: PROTOCOL_VERSION,
    };
    encode_frame(&Frame::new(0, 1, hello)).unwrap()
}

/// A transport that replays fixed input and accepts `writes` write calls; every later write
/// fails with `BrokenPipe`, as when the client vanished.
struct FailingWrites {
    input: Cursor<Vec<u8>>,
    writes: usize,
}

impl Read for FailingWrites {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.input.read(buf)
    }
}

impl Write for FailingWrites {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.writes == 0 {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        self.writes -= 1;
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn a_command_whose_result_cannot_be_written_is_still_reported() {
    let mut input = hello_bytes();
    let command = Message::Command {
        command: Command::ToggleMode,
    };
    input.extend(encode_frame(&Frame::new(SERVER_ID, 5, command)).unwrap());
    // `Welcome` and both snapshots go through; the `CommandResult` does not.
    let stream = FailingWrites {
        input: Cursor::new(input),
        writes: 3,
    };
    let mut server = ServerSession::new(stream, id(SERVER_ID));
    server.handshake(&published(1, 1, 1)).unwrap();
    let mut executed = Vec::new();
    let mut handler = |_: NonZeroU64, command: &Command| {
        executed.push(command.clone());
        ErrorCode::OK
    };
    // The mode already changed, so the controller must still learn about the command (to
    // broadcast the change to the other clients).
    assert_eq!(
        server.serve_one(&mut handler),
        Ok(Some(ServerEvent::Command {
            request_id: 5,
            command: Command::ToggleMode,
            code: ErrorCode::OK
        }))
    );
    assert!(server.is_closed());
    assert_eq!(server.serve_one(&mut handler), Err(IpcError::NotConnected));
    assert_eq!(executed, [Command::ToggleMode]);
}

#[test]
fn a_stream_that_ends_inside_a_frame_is_truncated_and_nothing_is_reported() {
    // Client side: the server vanishes in the middle of a frame.
    let mut input = server_handshake_bytes(2);
    let mode = encode_frame(&Frame::new(
        2,
        9,
        Message::ModeChanged {
            vietnamese_mode: false,
            revision: 2,
        },
    ))
    .unwrap();
    input.extend_from_slice(&mode[..mode.len() / 2]);
    let (stream, written) = Scripted::new(input);
    let mut client = ClientSession::new(identity());
    client.connect(stream).unwrap();
    assert_eq!(client.recv(), Err(IpcError::Truncated));
    assert!(!client.is_connected());
    assert_eq!(kinds_written(&written), [MessageKind::Hello]);

    // Server side: the client vanishes inside a header.
    let mut input = hello_bytes();
    input.extend_from_slice(&encode_frame(&Frame::new(SERVER_ID, 2, Message::Ping)).unwrap()[..17]);
    let (stream, written) = Scripted::new(input);
    let mut server = ServerSession::new(stream, id(SERVER_ID));
    server.handshake(&published(1, 1, 1)).unwrap();
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    assert_eq!(server.serve_one(&mut handler), Err(IpcError::Truncated));
    assert!(server.is_closed());
    assert_eq!(
        kinds_written(&written),
        [
            MessageKind::Welcome,
            MessageKind::RuntimeSnapshot,
            MessageKind::MacroSnapshot
        ]
    );
}

#[test]
fn a_stream_that_ends_between_frames_is_closed() {
    let (stream, written) = Scripted::new(server_handshake_bytes(2));
    let mut client = ClientSession::new(identity());
    client.connect(stream).unwrap();
    assert_eq!(client.recv(), Err(IpcError::Closed));
    assert!(!client.is_connected());
    assert_eq!(kinds_written(&written), [MessageKind::Hello]);

    let (stream, written) = Scripted::new(hello_bytes());
    let mut server = ServerSession::new(stream, id(SERVER_ID));
    server.handshake(&published(1, 1, 1)).unwrap();
    let mut handler = |_: NonZeroU64, _: &Command| ErrorCode::OK;
    assert_eq!(server.serve_one(&mut handler), Err(IpcError::Closed));
    assert!(server.is_closed());
    assert_eq!(kinds_written(&written).len(), 3);
}

#[test]
fn client_id_allocator_hands_out_every_id_once_then_reports_exhaustion() {
    let ids = ClientIdAllocator::starting_at(id(u64::MAX - 1));
    assert_eq!(ids.allocate(), Ok(id(u64::MAX - 1)));
    assert_eq!(ids.allocate(), Ok(id(u64::MAX)));
    assert_eq!(ids.allocate(), Err(IpcError::ClientIdsExhausted));
    assert_eq!(ids.allocate(), Err(IpcError::ClientIdsExhausted));
}

#[test]
fn client_id_allocator_counts_up_from_one() {
    let ids = ClientIdAllocator::new();
    assert_eq!(ids.allocate(), Ok(id(1)));
    assert_eq!(ids.allocate(), Ok(id(2)));
    let shared = Arc::new(ClientIdAllocator::default());
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let shared = Arc::clone(&shared);
            thread::spawn(move || {
                (0..100)
                    .map(|_| shared.allocate().unwrap())
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut all: Vec<NonZeroU64> = handles
        .into_iter()
        .flat_map(|h| h.join().unwrap())
        .collect();
    all.sort_unstable();
    all.dedup();
    assert_eq!(all.len(), 400);
}

#[test]
fn current_identity_uses_this_process() {
    let current = ClientIdentity::current(ClientKind::Tool);
    assert_eq!(current.client_kind, ClientKind::Tool);
    assert_eq!(current.pid, std::process::id());
    assert_eq!(current.arch, Arch::current());
}
