//! Controller-side legacy bridge, activation, and lifecycle (matrix #30, #38, #55).

#![cfg(feature = "evkey-compat")]

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use raumakey_ipc::legacy::fake::{FakeLegacy, FakeOp};
use raumakey_ipc::legacy::{
    BLOCK_SIZE, LegacyChannel, LegacyClient, LegacyEdit, LegacyError, LegacyEvent, LegacyObject,
    LegacyOpcode, LegacyRequest, LegacyServer, LegacyServerConfig, MAX_CONSECUTIVE_FAILURES,
    MAX_OUTPUT_UNITS, MutexOrigin, OFF_DELETE_COUNT, OFF_EPOCH, OFF_FLAG_2C, OFF_FLAG_20,
    OFF_FLAG_28, OFF_OUTPUT_LEN, OFF_RESET_GATE, OFF_RESPONSE_SEQ, RejectReason, ResetOutcome,
    SHUTDOWN_WAIT, ServeOutcome, ShutdownOutcome, activate_controller,
};
use raumakey_model::utf16;

fn edit(delete_before: u32, text: &str) -> LegacyEdit {
    LegacyEdit {
        delete_before,
        insert_utf16: utf16::encode(text),
    }
}

fn started() -> (FakeLegacy, LegacyServer<FakeLegacy>) {
    let fake = FakeLegacy::new();
    let server = LegacyServer::start(fake.clone(), LegacyServerConfig::default()).unwrap();
    fake.take_journal();
    (fake, server)
}

/// A tiny stand-in engine: `a` after `a` becomes `â`.
fn engine() -> impl FnMut(LegacyRequest) -> LegacyEdit + Send + 'static {
    let mut last = None;
    move |request| match request {
        LegacyRequest::ProcessChar { unit, .. } => {
            let out = if last == Some(0x61) && unit == 0x61 {
                edit(1, "â")
            } else {
                LegacyEdit {
                    delete_before: 0,
                    insert_utf16: vec![unit],
                }
            };
            last = Some(unit);
            out
        }
        LegacyRequest::Backspace => {
            last = None;
            edit(1, "")
        }
        LegacyRequest::Reset | LegacyRequest::Sync => {
            last = None;
            LegacyEdit::empty()
        }
    }
}

#[test]
fn matrix_55_startup_creates_the_mapping_and_both_events_before_serving() {
    let fake = FakeLegacy::new();
    let config = LegacyServerConfig::default();
    assert!(config.reset_gate);
    let server = LegacyServer::start(fake.clone(), config).unwrap();
    for object in [
        LegacyObject::SharedMemory,
        LegacyObject::RequestEvent,
        LegacyObject::ResponseEvent,
    ] {
        assert!(fake.exists(object), "{object:?}");
    }
    assert!(
        !fake.exists(LegacyObject::Mutex),
        "the controller never creates the mutex"
    );
    assert_eq!(fake.block().unwrap().as_bytes().len(), BLOCK_SIZE);
    assert_eq!(fake.u32_at(OFF_RESET_GATE), 1);
    assert_eq!(fake.u64_at(OFF_EPOCH), fake.now());
    assert!(!fake.request_signaled());
    assert!(!fake.response_signaled());
    assert_eq!(fake.journal()[0], FakeOp::CreateControllerObjects);
    assert!(server.channel().exists(LegacyObject::SharedMemory));

    let gate_off = FakeLegacy::new();
    LegacyServer::start(
        gate_off.clone(),
        LegacyServerConfig {
            reset_gate: false,
            ..LegacyServerConfig::default()
        },
    )
    .unwrap();
    assert_eq!(gate_off.u32_at(OFF_RESET_GATE), 0);
}

/// Lets `server` answer every request a TIP signals in the shared namespace.
fn serve_with_engine(namespace: &FakeLegacy, server: &Arc<LegacyServer<FakeLegacy>>) {
    let serving = Arc::clone(server);
    let mut handler = engine();
    namespace.set_peer(move || {
        serving.serve_one(Duration::ZERO, &mut handler).unwrap();
    });
}

#[test]
fn matrix_55_controller_first_and_tip_first_orders_both_work() {
    // Controller first.
    let (fake, server) = started();
    let server = Arc::new(server);
    serve_with_engine(&fake, &server);
    let mut client = LegacyClient::new(fake.endpoint());
    client.connect().unwrap();
    fake.clear_peer();

    // TIP first: the TIP creates the mutex, fails (closing what it opened), and connects once
    // the controller exists.
    let fake = FakeLegacy::new();
    let mut client = LegacyClient::new(fake.endpoint());
    assert_eq!(
        client.connect(),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );
    assert_eq!(
        fake.take_journal()[0],
        FakeOp::OpenOrCreateMutex(MutexOrigin::Created)
    );
    let server =
        Arc::new(LegacyServer::start(fake.clone(), LegacyServerConfig::default()).unwrap());
    serve_with_engine(&fake, &server);
    client.connect().unwrap();
    assert!(client.is_connected());
    assert!(fake.exists(LegacyObject::Mutex));
    fake.clear_peer();
}

#[test]
fn a_tip_keeps_an_exited_controllers_objects_alive_only_until_its_bridge_drops() {
    let namespace = FakeLegacy::new();
    let controller =
        Arc::new(LegacyServer::start(namespace.endpoint(), LegacyServerConfig::default()).unwrap());
    serve_with_engine(&namespace, &controller);
    let tip = namespace.endpoint();
    let mut client = LegacyClient::new(tip.clone());
    client.connect().unwrap();
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "a")));

    // The controller exits. The TIP's handles keep the objects alive, so a restarted
    // controller fails closed and has to retry.
    namespace.clear_peer();
    assert_eq!(controller.shutdown(), ShutdownOutcome::Completed);
    assert!(namespace.exists(LegacyObject::SharedMemory));
    assert_eq!(
        LegacyServer::start(namespace.endpoint(), LegacyServerConfig::default()).err(),
        Some(LegacyError::ConflictingObject(LegacyObject::SharedMemory))
    );

    // Nobody answers any more: the TIP drops the bridge and closes its handles, and the
    // orphaned objects disappear with them.
    for _ in 0..MAX_CONSECUTIVE_FAILURES {
        assert_eq!(
            client.process_char(0x61, false, false),
            Err(LegacyError::ResponseTimeout)
        );
    }
    assert!(!client.is_connected());
    for object in LegacyObject::ALL {
        assert!(!tip.holds(object), "{object:?}");
        assert!(!namespace.exists(object), "{object:?}");
    }

    // The retry succeeds and the TIP re-synchronizes with the new controller.
    let restarted =
        Arc::new(LegacyServer::start(namespace.endpoint(), LegacyServerConfig::default()).unwrap());
    serve_with_engine(&namespace, &restarted);
    client.connect().unwrap();
    assert_eq!(client.process_char(0x62, false, false), Ok(edit(0, "b")));
    namespace.clear_peer();
}

#[test]
fn after_close_every_call_on_the_channel_fails_with_object_missing() {
    let controller = FakeLegacy::new();
    let tip = controller.endpoint();
    activate_controller(&controller).unwrap();
    assert_eq!(tip.open_or_create_mutex(), Ok(MutexOrigin::Created));
    tip.open_controller_objects().unwrap();
    for (channel, objects_survive) in [(&controller, true), (&tip, false)] {
        channel.close();
        // Idempotent: closing again is harmless.
        channel.close();
        let missing = |object| Err(LegacyError::ObjectMissing(object));
        assert_eq!(
            channel.read_shared(0, &mut [0; 4]),
            missing(LegacyObject::SharedMemory)
        );
        assert_eq!(
            channel.write_shared(0, &[0; 4]),
            missing(LegacyObject::SharedMemory)
        );
        assert_eq!(
            channel.signal(LegacyEvent::Request),
            missing(LegacyObject::RequestEvent)
        );
        assert_eq!(
            channel.signal(LegacyEvent::Response),
            missing(LegacyObject::ResponseEvent)
        );
        assert_eq!(
            channel.wait(LegacyEvent::Request, Duration::ZERO),
            Err(LegacyError::ObjectMissing(LegacyObject::RequestEvent))
        );
        assert_eq!(
            channel.wait(LegacyEvent::Response, Duration::ZERO),
            Err(LegacyError::ObjectMissing(LegacyObject::ResponseEvent))
        );
        assert_eq!(
            channel.lock_mutex(Duration::ZERO),
            Err(LegacyError::ObjectMissing(LegacyObject::Mutex))
        );
        assert_eq!(channel.unlock_mutex(), missing(LegacyObject::Mutex));
        // Objects that another process still holds stay alive.
        assert_eq!(
            controller.exists(LegacyObject::SharedMemory),
            objects_survive
        );
    }
}

#[test]
fn activation_fails_closed_when_an_evkey_object_already_exists() {
    for object in [
        LegacyObject::SharedMemory,
        LegacyObject::RequestEvent,
        LegacyObject::ResponseEvent,
    ] {
        let fake = FakeLegacy::new();
        fake.create_object(object);
        assert_eq!(
            LegacyServer::start(fake.clone(), LegacyServerConfig::default()).err(),
            Some(LegacyError::ConflictingObject(object))
        );
        assert!(
            fake.journal().is_empty(),
            "nothing may be created or written"
        );
        assert_eq!(
            activate_controller(&fake),
            Err(LegacyError::ConflictingObject(object))
        );
        for other in LegacyObject::ALL {
            assert_eq!(fake.exists(other), other == object, "{other:?}");
        }
    }

    // A mutex created by a TIP that started first is not a conflict.
    let fake = FakeLegacy::new();
    fake.create_object(LegacyObject::Mutex);
    assert!(LegacyServer::start(fake, LegacyServerConfig::default()).is_ok());
}

#[test]
fn serve_one_answers_a_character_request() {
    let (fake, server) = started();
    fake.set_u32(OFF_FLAG_20, 0x2020_2020);
    fake.set_u32(OFF_FLAG_28, 0x2828_2828);
    fake.set_u32(OFF_FLAG_2C, 0x2C2C_2C2C);
    fake.submit_request(1, 0x61, 0, 1, 77);
    let mut seen = Vec::new();
    let mut handler = |request: LegacyRequest| {
        seen.push(request);
        edit(1, "á")
    };
    assert_eq!(
        server.serve_one(Duration::from_millis(10), &mut handler),
        Ok(ServeOutcome::Served(LegacyOpcode::ProcessChar))
    );
    assert_eq!(
        seen,
        [LegacyRequest::ProcessChar {
            unit: 0x61,
            shift: false,
            caps: true
        }]
    );
    let block = fake.block().unwrap();
    assert_eq!(block.response_sequence(), 77);
    assert_eq!(block.delete_count(), 1);
    assert_eq!(block.output_len(), 2);
    assert_eq!(&block.output_bytes()[..2], &0x00E1_u16.to_le_bytes());
    assert_eq!(block.epoch(), fake.now());
    assert_eq!(
        block.unknown_flags(),
        [0x2020_2020, 0x2828_2828, 0x2C2C_2C2C]
    );
    assert!(fake.response_signaled());
    let journal = fake.journal();
    let last_write = journal
        .iter()
        .rev()
        .find_map(|op| match op {
            FakeOp::Write { offset, .. } => Some(*offset),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        last_write, OFF_RESPONSE_SEQ,
        "the sequence publishes the response"
    );
    assert_eq!(journal.last(), Some(&FakeOp::Signal(LegacyEvent::Response)));
    for op in journal {
        if let FakeOp::Write { offset, bytes } = op {
            let end = offset + bytes.len();
            assert!(
                end <= OFF_FLAG_20 || offset >= OFF_EPOCH,
                "server wrote {offset:#x}..{end:#x}"
            );
        }
    }
}

#[test]
fn a_repeated_signal_for_a_request_already_handled_does_not_run_the_handler_again() {
    // The worker's wait returns for request 5, then the worker is delayed past the TIP's 50 ms
    // response wait. The TIP gives up, writes request 6 and signals again; the worker answers
    // 6 (the TIP applies it), and the second signal still finds request 6 in the block.
    let (fake, server) = started();
    let mut seen = Vec::new();
    let mut handler = |request: LegacyRequest| {
        seen.push(request);
        edit(0, "b")
    };
    fake.submit_request(1, 0x62, 0, 0, 6);
    assert_eq!(
        server.serve_one(Duration::ZERO, &mut handler),
        Ok(ServeOutcome::Served(LegacyOpcode::ProcessChar))
    );
    let answered = fake.block().unwrap();
    // The TIP takes the response.
    assert_eq!(
        fake.wait(LegacyEvent::Response, Duration::ZERO),
        Ok(raumakey_ipc::legacy::WaitResult::Signaled)
    );
    fake.submit_request(1, 0x62, 0, 0, 6);
    fake.take_journal();
    assert_eq!(
        server.serve_one(Duration::ZERO, &mut handler),
        Ok(ServeOutcome::Duplicate),
        "the key must not reach the engine twice"
    );
    // Nothing is written or signaled for the repeated signal.
    assert!(
        fake.journal()
            .iter()
            .all(|op| !matches!(op, FakeOp::Write { .. } | FakeOp::Signal(_))),
        "{:?}",
        fake.journal()
    );
    assert_eq!(fake.block().unwrap(), answered);
    assert!(!fake.response_signaled());

    // The next request carries a new sequence and is served.
    fake.submit_request(1, 0x63, 0, 0, 7);
    assert_eq!(
        server.serve_one(Duration::ZERO, &mut handler),
        Ok(ServeOutcome::Served(LegacyOpcode::ProcessChar))
    );
    assert_eq!(fake.u32_at(OFF_RESPONSE_SEQ), 7);
    let units: Vec<u16> = seen
        .iter()
        .map(|request| match request {
            LegacyRequest::ProcessChar { unit, .. } => *unit,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(units, [0x62, 0x63]);
}

#[test]
fn serve_one_is_idle_without_a_request() {
    let (fake, server) = started();
    let mut handler = |_: LegacyRequest| LegacyEdit::empty();
    assert_eq!(
        server.serve_one(Duration::from_millis(25), &mut handler),
        Ok(ServeOutcome::Idle)
    );
    assert_eq!(
        fake.take_journal(),
        [FakeOp::Wait {
            event: LegacyEvent::Request,
            timeout_ms: 25,
            result: raumakey_ipc::legacy::WaitResult::TimedOut
        }]
    );
}

#[test]
fn invalid_requests_and_replies_get_no_response() {
    let cases: Vec<(u32, u32, u32, u32, RejectReason)> = vec![
        (4, 0x61, 0, 0, RejectReason::UnknownOpcode(4)),
        (0, 0x61, 0, 0, RejectReason::UnknownOpcode(0)),
        (u32::MAX, 0x61, 0, 0, RejectReason::UnknownOpcode(u32::MAX)),
        (1, 0xD800, 0, 0, RejectReason::InvalidChar),
        (1, 0x1_0061, 0, 0, RejectReason::InvalidChar),
        (1, 0x61, 7, 0, RejectReason::InvalidFlag),
        (1, 0x61, 0, 0x8000_0000, RejectReason::InvalidFlag),
    ];
    for (opcode, unit, shift, caps, reason) in cases {
        let (fake, server) = started();
        fake.submit_request(opcode, unit, shift, caps, 5);
        let mut called = false;
        let mut handler = |_: LegacyRequest| {
            called = true;
            LegacyEdit::empty()
        };
        assert_eq!(
            server.serve_one(Duration::ZERO, &mut handler),
            Ok(ServeOutcome::Rejected(reason))
        );
        assert!(!called);
        assert!(!fake.response_signaled());
        assert_eq!(fake.u32_at(OFF_RESPONSE_SEQ), 0);
    }

    let replies = [
        LegacyEdit {
            delete_before: 0,
            insert_utf16: vec![0x61; MAX_OUTPUT_UNITS + 1],
        },
        LegacyEdit {
            delete_before: 0,
            insert_utf16: vec![0xDC00],
        },
        edit(65, ""),
    ];
    for reply in replies {
        let fake = FakeLegacy::new();
        let server = LegacyServer::start(
            fake.clone(),
            LegacyServerConfig {
                max_delete: 64,
                ..LegacyServerConfig::default()
            },
        )
        .unwrap();
        fake.submit_request(1, 0x61, 0, 0, 5);
        let mut handler = move |_: LegacyRequest| reply.clone();
        assert_eq!(
            server.serve_one(Duration::ZERO, &mut handler),
            Ok(ServeOutcome::Rejected(RejectReason::InvalidReply))
        );
        assert!(!fake.response_signaled());
        assert_eq!(fake.u32_at(OFF_DELETE_COUNT), 0);
        assert_eq!(fake.u32_at(OFF_OUTPUT_LEN), 0);
    }
}

#[test]
fn matrix_30_and_38_client_and_server_interoperate() {
    let (fake, server) = started();
    let server = Arc::new(server);
    serve_with_engine(&fake, &server);
    let mut client = LegacyClient::new(fake.endpoint());
    client.connect().unwrap();
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "a")));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(1, "â")));
    assert_eq!(client.backspace(), Ok(edit(1, "")));
    assert_eq!(client.reset(), Ok(ResetOutcome::Remote));
    fake.clear_peer();
}

#[test]
fn matrix_55_shutdown_signals_the_worker_and_waits_for_it() {
    let (fake, server) = started();
    let server = Arc::new(server);
    let worker = {
        let server = Arc::clone(&server);
        thread::spawn(move || server.run(engine()))
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while !server.is_running() {
        assert!(Instant::now() < deadline, "worker did not start");
        thread::yield_now();
    }
    let started_at = Instant::now();
    assert_eq!(server.shutdown(), ShutdownOutcome::Completed);
    assert!(started_at.elapsed() < SHUTDOWN_WAIT);
    assert!(!server.is_running());
    assert_eq!(worker.join().unwrap(), Ok(()));
    let journal = fake.journal();
    assert!(journal.contains(&FakeOp::Signal(LegacyEvent::Request)));
    assert_eq!(journal.last(), Some(&FakeOp::Close));
    assert!(!fake.exists(LegacyObject::SharedMemory));

    let mut handler = |_: LegacyRequest| LegacyEdit::empty();
    assert_eq!(
        server.serve_one(Duration::ZERO, &mut handler),
        Ok(ServeOutcome::Stopped)
    );
}

#[test]
fn matrix_55_shutdown_gives_up_on_a_stuck_worker_after_the_limit() {
    let (fake, server) = started();
    let server = Arc::new(server);
    let (release, blocked) = mpsc::channel::<()>();
    let blocked = Mutex::new(blocked);
    let (entered_tx, entered) = mpsc::channel::<()>();
    fake.submit_request(1, 0x61, 0, 0, 1);
    let worker = {
        let server = Arc::clone(&server);
        thread::spawn(move || {
            server.run(move |_| {
                let _ = entered_tx.send(());
                let _ = blocked.lock().unwrap().recv();
                LegacyEdit::empty()
            })
        })
    };
    entered.recv().unwrap();
    let started_at = Instant::now();
    assert_eq!(
        server.shutdown_within(Duration::from_millis(100)),
        ShutdownOutcome::TimedOut
    );
    let waited = started_at.elapsed();
    assert!(waited >= Duration::from_millis(90), "{waited:?}");
    assert!(waited < SHUTDOWN_WAIT, "{waited:?}");
    assert!(server.is_running());
    release.send(()).unwrap();
    // The channel is closed, so the worker's late response fails cleanly and the worker stops.
    assert_eq!(
        worker.join().unwrap(),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );
    assert!(!server.is_running());
}

#[test]
fn matrix_55_default_shutdown_waits_at_most_2000_ms() {
    let (fake, server) = started();
    let server = Arc::new(server);
    let (release, blocked) = mpsc::channel::<()>();
    let blocked = Mutex::new(blocked);
    let (entered_tx, entered) = mpsc::channel::<()>();
    fake.submit_request(2, 0, 0, 0, 1);
    let worker = {
        let server = Arc::clone(&server);
        thread::spawn(move || {
            server.run(move |_| {
                let _ = entered_tx.send(());
                let _ = blocked.lock().unwrap().recv();
                LegacyEdit::empty()
            })
        })
    };
    entered.recv().unwrap();
    let started_at = Instant::now();
    assert_eq!(server.shutdown(), ShutdownOutcome::TimedOut);
    let waited = started_at.elapsed();
    assert!(waited >= Duration::from_millis(1_900), "{waited:?}");
    assert!(waited < Duration::from_millis(3_000), "{waited:?}");
    release.send(()).unwrap();
    let _ = worker.join().unwrap();
}

#[test]
fn a_stop_requested_before_run_ends_it_immediately() {
    let (_fake, server) = started();
    assert_eq!(server.shutdown(), ShutdownOutcome::Completed);
    assert_eq!(server.run(engine()), Ok(()));
    assert!(!server.is_running());
}
