//! `legacy_sequence_wrap`: the EVKey legacy codec over its deterministic fake (clean-room spec
//! §6, plan §7.4, matrix #30–#38).

use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use raumakey_ipc::legacy::fake::{FakeEpoch, FakeLegacy, FakeOp, FakeReply};
use raumakey_ipc::legacy::{
    BLOCK_SIZE, LegacyClient, LegacyEdit, LegacyError, LegacyObject, LegacyRequest, LegacyServer,
    LegacyServerConfig, MAX_CONSECUTIVE_FAILURES, MAX_DELETE_UNITS, MAX_OUTPUT_UNITS, OFF_FLAG_2C,
    OFF_FLAG_20, OFF_FLAG_28, OFF_OPCODE, OFF_REQUEST_SEQ, OFF_RESET_GATE, ResetOutcome,
    ServeOutcome, decode_request, next_sequence, transaction_for, validate_response,
};
use raumakey_model::transaction::EditTransaction;

use super::reader::Reader;

/// Operations decoded from one input.
const MAX_OPS: usize = 64;
/// Request sequences near the `u32` wrap and the sign boundary.
const SEQUENCES: [u32; 8] = [
    u32::MAX,
    u32::MAX - 1,
    u32::MAX - 2,
    0,
    1,
    2,
    0x7FFF_FFFF,
    0x8000_0000,
];
const FLAG_VALUES: [u32; 4] = [0, 1, 0xDEAD_BEEF, u32::MAX];

/// A delete limit for the client or the server: the full bound by default.
fn max_delete(reader: &mut Reader<'_>) -> u32 {
    match reader.below(8) {
        0..=4 => MAX_DELETE_UNITS,
        5 => 64,
        6 => 1,
        _ => u32::MAX,
    }
}

fn sequence(reader: &mut Reader<'_>) -> u32 {
    let choice = reader.below(SEQUENCES.len() + 1);
    SEQUENCES
        .get(choice)
        .copied()
        .unwrap_or_else(|| reader.u32())
}

/// Short, well-formed output.
fn short_output(reader: &mut Reader<'_>) -> Vec<u16> {
    (0..reader.below(4))
        .map(|_| reader.pick(&[0x0061, 0x00E2, 0x1EBF, 0x0301, 0x0020]))
        .collect()
}

/// Any output: short, at the buffer size, ill-formed, or too long.
fn output(reader: &mut Reader<'_>) -> Vec<u16> {
    match reader.below(4) {
        0 => short_output(reader),
        1 => vec![0x0062; MAX_OUTPUT_UNITS],
        2 => vec![0xD800, 0x0061],
        _ => vec![0x0063; MAX_OUTPUT_UNITS + 1],
    }
}

/// Any delete count: small, at the bound, or past it.
fn delete_count(reader: &mut Reader<'_>) -> u32 {
    reader.pick(&[0, 1, 3, MAX_DELETE_UNITS, MAX_DELETE_UNITS + 1, u32::MAX])
}

/// A scripted controller reply. Uniform input bytes must still produce mostly valid exchanges,
/// or most operations would only meet a dropped bridge, so a reply is fresh, matching, and
/// immediate with a small edit unless one choice in four breaks one rule of the response.
fn reply(reader: &mut Reader<'_>, now: u64) -> FakeReply {
    let delete = u32::from(reader.byte() % 3);
    let benign = FakeReply::units(delete, short_output(reader));
    match reader.below(16) {
        0..=11 => benign,
        12 => match reader.below(5) {
            0 => benign.after_ms(reader.pick(&[10, 49, 50, 51, 100])),
            1 => benign.with_sequence(sequence(reader)),
            2 => benign.with_output_len(reader.pick(&[1, 3, 0x1000, 0x1002, u32::MAX])),
            3 => benign.with_epoch(FakeEpoch::AgeMs(reader.pick(&[2_000, 2_001, 5_000]))),
            _ => benign.with_epoch(FakeEpoch::Absolute(reader.pick(&[0, u64::MAX, now + 1]))),
        },
        13 => FakeReply::units(delete_count(reader), short_output(reader)),
        14 => FakeReply::units(delete, output(reader)),
        _ => FakeReply::units(delete_count(reader), output(reader))
            .after_ms(reader.pick(&[0, 49, 51]))
            .with_epoch(FakeEpoch::AgeMs(reader.pick(&[0, 2_000, 2_001]))),
    }
}

/// A controller handler's reply: a small valid edit unless one choice in four makes it too
/// long, ill-formed, or delete too much (the server must then send no response).
fn edit(reader: &mut Reader<'_>) -> LegacyEdit {
    if reader.below(4) < 3 {
        LegacyEdit {
            delete_before: u32::from(reader.byte() % 3),
            insert_utf16: short_output(reader),
        }
    } else {
        LegacyEdit {
            delete_before: delete_count(reader),
            insert_utf16: output(reader),
        }
    }
}

/// The handler of the real server: replays decoded edits and records each request sequence.
struct Handler {
    namespace: FakeLegacy,
    edits: Vec<LegacyEdit>,
    calls: usize,
    sequences: Vec<u32>,
}

impl Handler {
    fn handle(&mut self, request: LegacyRequest) -> LegacyEdit {
        let _ = request;
        self.sequences.push(self.namespace.u32_at(OFF_REQUEST_SEQ));
        let edit = self
            .edits
            .get(self.calls % self.edits.len().max(1))
            .cloned()
            .unwrap_or_default();
        self.calls += 1;
        edit
    }
}

fn lock(handler: &Mutex<Handler>) -> std::sync::MutexGuard<'_, Handler> {
    handler.lock().unwrap_or_else(PoisonError::into_inner)
}

/// What the client side has seen, to check the documented bridge rules.
struct Bridge {
    client: LegacyClient<FakeLegacy>,
    /// Exchanges in a row that produced no usable response.
    failures: u8,
}

impl Bridge {
    /// Applies the bridge rules of plan §7.4 to one exchange result.
    fn settle<T>(&mut self, result: &Result<T, LegacyError>) {
        match result {
            Ok(_) => self.failures = 0,
            Err(error) if error.resets_bridge() => {
                assert!(
                    !self.client.is_connected(),
                    "{error:?} must drop the bridge"
                );
                self.failures = 0;
            }
            Err(
                LegacyError::MutexTimeout
                | LegacyError::ResponseTimeout
                | LegacyError::SequenceMismatch { .. },
            ) => {
                self.failures = self.failures.saturating_add(1);
                if self.failures >= MAX_CONSECUTIVE_FAILURES {
                    assert!(
                        !self.client.is_connected(),
                        "the bridge survived {MAX_CONSECUTIVE_FAILURES} failed exchanges"
                    );
                    self.failures = 0;
                } else {
                    assert!(
                        self.client.is_connected(),
                        "one failed exchange dropped the bridge"
                    );
                }
            }
            Err(LegacyError::NotConnected) => {
                assert!(!self.client.is_connected(), "NotConnected while connected");
            }
            Err(_) => {}
        }
    }
}

/// Channel operations of one client call: in bounds, mutex released, request sequence advanced
/// as a raw `u32` that skips 0 (matrix #30, #35).
fn check_journal(journal: &[FakeOp], sequence_before: u32, client_only: bool) {
    let mut sequence = sequence_before;
    let mut locks = 0_usize;
    let mut unlocks = 0_usize;
    for op in journal {
        match op {
            FakeOp::Read { offset, len } => assert!(
                offset
                    .checked_add(*len)
                    .is_some_and(|end| end <= BLOCK_SIZE),
                "a read outside the shared block"
            ),
            FakeOp::Write { offset, bytes } => {
                assert!(
                    offset
                        .checked_add(bytes.len())
                        .is_some_and(|end| end <= BLOCK_SIZE),
                    "a write outside the shared block"
                );
                if *offset == OFF_REQUEST_SEQ && bytes.len() == 4 {
                    let written = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                    assert!(
                        written == next_sequence(sequence) && written != 0,
                        "the request sequence did not advance as a raw u32 skipping 0"
                    );
                    sequence = written;
                } else if client_only {
                    assert!(
                        *offset == OFF_OPCODE && bytes.len() == 16,
                        "the client wrote outside the request fields"
                    );
                }
            }
            FakeOp::Lock { result, .. } => {
                if !matches!(result, raumakey_ipc::legacy::LockResult::TimedOut) {
                    locks += 1;
                }
            }
            FakeOp::Unlock => unlocks += 1,
            _ => {}
        }
    }
    assert!(locks == unlocks, "the client kept the mutex after a call");
}

/// An edit the client accepted is valid and is exactly what the response validation makes of
/// the shared block (so malformed, stale, or unmatched responses are never accepted).
fn check_edit(fake: &FakeLegacy, client: &LegacyClient<FakeLegacy>, edit: &LegacyEdit) {
    assert!(
        edit.validate(client.max_delete()).is_ok(),
        "the client accepted an invalid edit"
    );
    let transaction = transaction_for(&Ok(edit.clone()));
    assert!(
        transaction.validate(u32::MAX, client.max_delete()).is_ok(),
        "an accepted edit yields an invalid transaction"
    );
    let Some(block) = fake.block() else {
        panic!("an edit was accepted without a shared block");
    };
    let sequence = fake.u32_at(OFF_REQUEST_SEQ);
    let revalidated = validate_response(&block, sequence, fake.now(), client.max_delete());
    assert!(
        revalidated.as_ref() == Ok(edit),
        "the accepted edit is not what the shared block holds for this request"
    );
}

pub(crate) fn run(data: &[u8]) {
    let mut reader = Reader::new(data);
    let with_server = reader.flag();
    let start = sequence(&mut reader);
    let gate = reader.pick(&[1, 1, 0, 2, u32::MAX]);
    let flags = [
        reader.pick(&FLAG_VALUES),
        reader.pick(&FLAG_VALUES),
        reader.pick(&FLAG_VALUES),
    ];

    let (fake, server, handler) = if with_server {
        let fake = FakeLegacy::new();
        let config = LegacyServerConfig {
            reset_gate: !reader.flag(),
            max_delete: max_delete(&mut reader),
        };
        let server = match LegacyServer::start(fake.endpoint(), config) {
            Ok(server) => Arc::new(server),
            Err(error) => panic!("the server did not start in an empty namespace: {error}"),
        };
        let edits = (0..=reader.below(4)).map(|_| edit(&mut reader)).collect();
        let handler = Arc::new(Mutex::new(Handler {
            namespace: fake.clone(),
            edits,
            calls: 0,
            sequences: Vec::new(),
        }));
        let serving = Arc::clone(&server);
        let shared = Arc::clone(&handler);
        fake.set_peer(move || {
            let mut call = |request| lock(&shared).handle(request);
            // Errors are typed; the client sees them as timeouts.
            let _ = serving.serve_one(Duration::ZERO, &mut call);
        });
        (fake, Some(server), Some(handler))
    } else {
        let fake = FakeLegacy::with_controller_objects();
        fake.set_u32(OFF_RESET_GATE, gate);
        (fake, None, None)
    };
    fake.set_u32(OFF_REQUEST_SEQ, start);
    for (offset, value) in [OFF_FLAG_20, OFF_FLAG_28, OFF_FLAG_2C]
        .into_iter()
        .zip(flags)
    {
        fake.set_u32(offset, value);
    }
    let mut flags_intact = true;
    let mut bridge = Bridge {
        client: LegacyClient::new(fake.endpoint()),
        failures: 0,
    };
    fake.take_journal();

    for _ in 0..MAX_OPS {
        if reader.is_empty() {
            break;
        }
        let before = fake.u32_at(OFF_REQUEST_SEQ);
        let scripted = server.is_none();
        match reader.below(20) {
            0..=2 => {
                if scripted && !reader.chance(32) {
                    fake.push_reply(reply(&mut reader, fake.now()));
                }
                let result = bridge.client.connect();
                assert!(
                    result.is_ok() == bridge.client.is_connected(),
                    "connect and is_connected disagree"
                );
                if let Err(error) = result {
                    assert!(
                        error != LegacyError::OutOfBounds,
                        "connect read out of bounds"
                    );
                }
                bridge.failures = 0;
            }
            op @ 3..=9 => {
                if scripted && !reader.chance(32) {
                    fake.push_reply(reply(&mut reader, fake.now()));
                }
                let result = if op == 9 {
                    bridge.client.backspace()
                } else {
                    let unit = reader.pick(&[0x0061, 0x0041, 0x0300, 0xD800, 0xDFFF, 0xFFFF]);
                    let result = bridge
                        .client
                        .process_char(unit, reader.flag(), reader.flag());
                    if (0xD800..=0xDFFF).contains(&unit) && bridge.client.is_connected() {
                        assert!(
                            result == Err(LegacyError::UnsupportedChar),
                            "a lone surrogate was sent"
                        );
                    }
                    result
                };
                match &result {
                    Ok(edit) => check_edit(&fake, &bridge.client, edit),
                    Err(error) => {
                        assert!(
                            *error != LegacyError::OutOfBounds,
                            "a client call read out of bounds"
                        );
                        assert!(
                            transaction_for(&result) == EditTransaction::pass_through_reset(),
                            "a failed request does not pass the key through with a reset"
                        );
                    }
                }
                bridge.settle(&result);
            }
            10 => {
                if scripted && !reader.chance(32) {
                    fake.push_reply(reply(&mut reader, fake.now()));
                }
                let result = bridge.client.reset();
                if result == Ok(ResetOutcome::LocalOnly) {
                    // No exchange took place.
                } else {
                    bridge.settle(&result);
                }
            }
            11 => {
                bridge.client.disconnect();
                assert!(
                    !bridge.client.is_connected(),
                    "disconnect left the bridge up"
                );
                bridge.failures = 0;
            }
            12 => bridge.client.set_max_delete(max_delete(&mut reader)),
            13 => fake.advance(reader.pick(&[0, 1, 49, 50, 51, 2_000, 2_001, 10_000])),
            14 => match (&server, &handler) {
                (Some(server), Some(handler)) => {
                    let opcode = reader.pick(&[1, 2, 3, 5, 0, 4, u32::MAX]);
                    let unit = reader.pick(&[0x61, 0xD800, 0x1_0000, u32::MAX]);
                    let shift = reader.pick(&[0, 1, 2]);
                    let caps = reader.pick(&[0, 1, u32::MAX]);
                    let request_sequence = sequence(&mut reader);
                    fake.submit_request(opcode, unit, shift, caps, request_sequence);
                    let decoded = fake.block().map(|block| decode_request(&block));
                    let mut call = |request| lock(handler).handle(request);
                    let outcome = server.serve_one(Duration::ZERO, &mut call);
                    if let (Ok(ServeOutcome::Rejected(reason)), Some(Err(expected))) =
                        (&outcome, decoded)
                    {
                        assert!(
                            *reason == expected,
                            "the server rejected a request for another reason"
                        );
                    }
                    if let (Ok(ServeOutcome::Served(_)), Some(Err(_))) = (&outcome, decoded) {
                        panic!("the server served a request that does not decode");
                    }
                }
                _ => fake.schedule_reply(reply(&mut reader, fake.now())),
            },
            15 => {
                let stale = reader.pick(&[before, before.wrapping_sub(1), next_sequence(before)]);
                let units = output(&mut reader);
                fake.write_stale_response(stale, &units);
            }
            16 if reader.flag() => fake.abandon_mutex_next(),
            16 => {
                let release = reader.pick(&[Some(0), Some(0), Some(50), Some(50), Some(150), None]);
                fake.hold_mutex_elsewhere(release);
            }
            17 => {
                let jump = sequence(&mut reader);
                fake.set_u32(OFF_REQUEST_SEQ, jump);
                fake.take_journal();
                continue;
            }
            18 => fake.set_u32(OFF_RESET_GATE, reader.pick(&[1, 0, 2, u32::MAX])),
            _ if reader.below(4) != 0 => {
                // A scripted controller restarts: it recreates whatever objects disappeared
                // (a recreated mapping is zeroed, so the flags are gone).
                if scripted {
                    for object in [
                        LegacyObject::SharedMemory,
                        LegacyObject::RequestEvent,
                        LegacyObject::ResponseEvent,
                    ] {
                        if !fake.exists(object) {
                            flags_intact = false;
                            fake.create_object(object);
                        }
                    }
                }
            }
            _ => {
                flags_intact = false;
                match &server {
                    Some(server) if reader.flag() => {
                        let _ = server.shutdown_within(Duration::ZERO);
                    }
                    _ => match reader.below(4) {
                        0 => fake.close_all(),
                        choice => fake.release_object(
                            [
                                LegacyObject::SharedMemory,
                                LegacyObject::RequestEvent,
                                LegacyObject::ResponseEvent,
                            ][choice - 1],
                        ),
                    },
                }
            }
        }
        let journal = fake.take_journal();
        check_journal(&journal, before, server.is_none());
        if flags_intact {
            let now = [OFF_FLAG_20, OFF_FLAG_28, OFF_FLAG_2C].map(|offset| fake.u32_at(offset));
            assert!(now == flags, "the unknown flags were not preserved");
        }
    }

    if let Some(handler) = &handler {
        let handled = lock(handler).sequences.clone();
        assert!(
            handled.windows(2).all(|pair| pair[0] != pair[1]),
            "the handler received one request sequence twice in a row"
        );
    }
    fake.clear_peer();
}
