//! TIP-side legacy bridge over the deterministic fake (matrix #30–#38, #42).

#![cfg(feature = "evkey-compat")]

use raumakey_ipc::legacy::fake::{FakeEpoch, FakeLegacy, FakeOp, FakeReply};
use raumakey_ipc::legacy::{
    BLOCK_SIZE, LegacyClient, LegacyEdit, LegacyError, LegacyEvent, LegacyObject, LockResult,
    MAX_DELETE_UNITS, MutexOrigin, OFF_FLAG_2C, OFF_FLAG_20, OFF_FLAG_28, OFF_OPCODE,
    OFF_REQUEST_SEQ, OFF_RESET_GATE, ResetOutcome, WaitResult,
};
use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::input::InputEvent;
use raumakey_model::transaction::TextSimulator;
use raumakey_model::utf16;

fn host() -> HostContext {
    HostContext::new(
        SessionId::from_u128(9),
        ExeName::new("notepad.exe").unwrap(),
    )
}

fn edit(delete_before: u32, text: &str) -> LegacyEdit {
    LegacyEdit {
        delete_before,
        insert_utf16: utf16::encode(text),
    }
}

fn request_bytes(opcode: u32, unit: u32, shift: u32, caps: u32) -> Vec<u8> {
    [opcode, unit, shift, caps]
        .iter()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

/// The exact channel operations of one successful request with sequence `sequence`.
fn exchange_ops(opcode: u32, unit: u32, shift: u32, caps: u32, sequence: u32) -> Vec<FakeOp> {
    vec![
        FakeOp::Lock {
            timeout_ms: 100,
            result: LockResult::Acquired,
        },
        FakeOp::Wait {
            event: LegacyEvent::Response,
            timeout_ms: 0,
            result: WaitResult::TimedOut,
        },
        FakeOp::Read {
            offset: OFF_REQUEST_SEQ,
            len: 4,
        },
        FakeOp::Write {
            offset: OFF_OPCODE,
            bytes: request_bytes(opcode, unit, shift, caps),
        },
        FakeOp::Write {
            offset: OFF_REQUEST_SEQ,
            bytes: sequence.to_le_bytes().to_vec(),
        },
        FakeOp::Signal(LegacyEvent::Request),
        FakeOp::Wait {
            event: LegacyEvent::Response,
            timeout_ms: 50,
            result: WaitResult::Signaled,
        },
        FakeOp::Read {
            offset: 0,
            len: BLOCK_SIZE,
        },
        FakeOp::Unlock,
    ]
}

fn connected() -> (FakeLegacy, LegacyClient<FakeLegacy>) {
    let fake = FakeLegacy::with_controller_objects();
    fake.push_reply(FakeReply::empty());
    let mut client = LegacyClient::new(fake.clone());
    client.connect().unwrap();
    fake.take_journal();
    (fake, client)
}

fn signals(journal: &[FakeOp]) -> usize {
    journal
        .iter()
        .filter(|op| **op == FakeOp::Signal(LegacyEvent::Request))
        .count()
}

fn assert_reads_in_bounds(journal: &[FakeOp]) {
    for op in journal {
        if let FakeOp::Read { offset, len } = op {
            assert!(offset + len <= BLOCK_SIZE, "{op:?}");
        }
    }
}

#[test]
fn matrix_38_op_5_sync_must_succeed_before_characters_are_processed() {
    let fake = FakeLegacy::with_controller_objects();
    let mut client = LegacyClient::new(fake.clone());
    assert!(!client.is_connected());
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::NotConnected)
    );
    assert_eq!(client.backspace(), Err(LegacyError::NotConnected));
    assert!(fake.journal().is_empty());

    fake.push_reply(FakeReply::empty());
    client.connect().unwrap();
    assert!(client.is_connected());
    let mut expected = vec![
        FakeOp::OpenOrCreateMutex(MutexOrigin::Created),
        FakeOp::OpenControllerObjects,
    ];
    expected.extend(exchange_ops(5, 0, 0, 0, 1));
    assert_eq!(fake.take_journal(), expected);

    fake.push_reply(FakeReply::text(0, "a"));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "a")));
}

#[test]
fn matrix_38_tip_first_creates_the_mutex_and_fails_until_the_controller_exists() {
    let fake = FakeLegacy::new();
    let mut client = LegacyClient::new(fake.clone());
    assert_eq!(
        client.connect(),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );
    assert!(!client.is_connected());
    assert_eq!(
        fake.take_journal()[0],
        FakeOp::OpenOrCreateMutex(MutexOrigin::Created)
    );
    assert!(!fake.exists(LegacyObject::SharedMemory));

    // Once the controller has created its objects, the next attempt creates the mutex again
    // (the failed attempt closed it) and synchronizes.
    for object in LegacyObject::CONTROLLER_OWNED {
        fake.create_object(object);
    }
    fake.push_reply(FakeReply::empty());
    client.connect().unwrap();
    assert_eq!(
        fake.take_journal()[0],
        FakeOp::OpenOrCreateMutex(MutexOrigin::Created)
    );
    assert!(fake.exists(LegacyObject::Mutex));
}

#[test]
fn matrix_38_a_failed_sync_leaves_the_client_disconnected() {
    let fake = FakeLegacy::with_controller_objects();
    let mut client = LegacyClient::new(fake.clone());
    assert_eq!(client.connect(), Err(LegacyError::ResponseTimeout));
    assert!(!client.is_connected());
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::NotConnected)
    );
}

#[test]
fn matrix_30_op_1_reads_count_length_and_output_and_applies_once() {
    let (fake, mut client) = connected();
    fake.set_u32(OFF_REQUEST_SEQ, 41);
    fake.push_reply(FakeReply::text(1, "â"));
    let result = client.process_char(0x61, false, false).unwrap();
    assert_eq!(result, edit(1, "â"));
    let journal = fake.take_journal();
    assert_eq!(journal, exchange_ops(1, 0x61, 0, 0, 42));
    assert_eq!(signals(&journal), 1);
    assert_eq!(fake.u32_at(OFF_REQUEST_SEQ), 42);

    let mut text = TextSimulator::with_text("a");
    text.apply(
        &InputEvent::character('a', host()),
        &result.to_transaction(),
    )
    .unwrap();
    assert_eq!(text.as_string(), "â");
}

#[test]
fn matrix_23_shift_and_caps_travel_as_separate_flags() {
    let (fake, mut client) = connected();
    fake.push_reply(FakeReply::text(0, "A"));
    client.process_char(0x41, true, false).unwrap();
    fake.push_reply(FakeReply::text(0, "A"));
    client.process_char(0x41, false, true).unwrap();
    let writes: Vec<Vec<u8>> = fake
        .take_journal()
        .into_iter()
        .filter_map(|op| match op {
            FakeOp::Write { offset, bytes } if offset == OFF_OPCODE => Some(bytes),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes,
        [request_bytes(1, 0x41, 1, 0), request_bytes(1, 0x41, 0, 1)]
    );
}

#[test]
fn op_2_backspace_round_trip() {
    let (fake, mut client) = connected();
    fake.push_reply(FakeReply::text(1, ""));
    assert_eq!(client.backspace(), Ok(edit(1, "")));
    assert_eq!(fake.take_journal(), exchange_ops(2, 0, 0, 0, 2));
}

#[test]
fn matrix_31_mutex_timeout_writes_and_applies_nothing() {
    let (fake, mut client) = connected();
    fake.hold_mutex_elsewhere(None);
    let before = fake.block().unwrap();
    let started = fake.now();
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::MutexTimeout)
    );
    assert_eq!(
        fake.take_journal(),
        [FakeOp::Lock {
            timeout_ms: 100,
            result: LockResult::TimedOut
        }]
    );
    assert_eq!(fake.block().unwrap(), before);
    assert_eq!(fake.now() - started, 100);
    assert!(client.is_connected());

    // The other holder releases within the window: the request then proceeds.
    fake.hold_mutex_elsewhere(Some(30));
    fake.push_reply(FakeReply::text(0, "a"));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "a")));
}

#[test]
fn matrix_32_a_late_response_is_dropped_and_never_applied_twice() {
    let (fake, mut client) = connected();
    fake.push_reply(FakeReply::text(0, "LATE").after_ms(80));
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::ResponseTimeout)
    );
    let journal = fake.take_journal();
    assert_eq!(
        signals(&journal),
        1,
        "an ambiguous timeout is never retried"
    );
    assert!(journal.contains(&FakeOp::Wait {
        event: LegacyEvent::Response,
        timeout_ms: 50,
        result: WaitResult::TimedOut
    }));
    assert_eq!(journal.last(), Some(&FakeOp::Unlock));
    assert!(client.is_connected());

    // The next key gets its own response even though the late one is still in flight.
    fake.push_reply(FakeReply::text(0, "b"));
    assert_eq!(client.process_char(0x62, false, false), Ok(edit(0, "b")));

    // The late response lands afterwards; the next request drains it and ignores its text.
    fake.advance(100);
    assert!(fake.response_signaled());
    fake.take_journal();
    fake.push_reply(FakeReply::text(0, "c"));
    assert_eq!(client.process_char(0x63, false, false), Ok(edit(0, "c")));
    assert_eq!(
        fake.take_journal()[1],
        FakeOp::Wait {
            event: LegacyEvent::Response,
            timeout_ms: 0,
            result: WaitResult::Signaled
        }
    );
}

#[test]
fn matrix_33_a_stale_response_signal_is_drained_before_the_request() {
    let (fake, mut client) = connected();
    // A stale response that would match the next sequence is already signaled.
    let next = fake.u32_at(OFF_REQUEST_SEQ) + 1;
    fake.write_stale_response(next, &utf16::encode("STALE"));
    fake.push_reply(FakeReply::text(0, "fresh").after_ms(10));
    assert_eq!(
        client.process_char(0x61, false, false),
        Ok(edit(0, "fresh"))
    );
    let journal = fake.take_journal();
    assert_eq!(
        journal[1],
        FakeOp::Wait {
            event: LegacyEvent::Response,
            timeout_ms: 0,
            result: WaitResult::Signaled
        }
    );
    assert_eq!(
        journal[2],
        FakeOp::Wait {
            event: LegacyEvent::Response,
            timeout_ms: 0,
            result: WaitResult::TimedOut
        }
    );
    let first_write = journal
        .iter()
        .position(|op| matches!(op, FakeOp::Write { .. }))
        .unwrap();
    assert!(first_write > 2);
}

#[test]
fn matrix_34_a_response_with_another_sequence_is_rejected_without_output() {
    let (fake, mut client) = connected();
    fake.push_reply(FakeReply::text(1, "WRONG").with_sequence(999));
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::SequenceMismatch {
            expected: 2,
            actual: 999
        })
    );
    assert!(client.is_connected());
    assert_eq!(fake.take_journal().last(), Some(&FakeOp::Unlock));

    // A mismatched response does not end the wait: the matching one is still accepted.
    fake.schedule_reply(FakeReply::text(1, "WRONG").with_sequence(7).after_ms(5));
    fake.push_reply(FakeReply::text(0, "ok").after_ms(10));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "ok")));
}

#[test]
fn matrix_35_malformed_responses_are_rejected_and_reset_the_bridge() {
    let cases = [
        (
            FakeReply::text(0, "x").with_output_len(3),
            LegacyError::OddOutputLength { len: 3 },
        ),
        (
            FakeReply::text(0, "x").with_output_len(0x1002),
            LegacyError::OutputTooLong { len: 0x1002 },
        ),
        (
            FakeReply::text(0, "x").with_output_len(0x8000_0002),
            LegacyError::OutputTooLong { len: 0x8000_0002 },
        ),
        (
            FakeReply::text(0, "x").with_output_len(0x8000_0001),
            LegacyError::OddOutputLength { len: 0x8000_0001 },
        ),
        (
            FakeReply::text(0x8000_0000, "x"),
            LegacyError::DeleteOutOfRange { count: 0x8000_0000 },
        ),
        (
            FakeReply::text(MAX_DELETE_UNITS + 1, "x"),
            LegacyError::DeleteOutOfRange {
                count: MAX_DELETE_UNITS + 1,
            },
        ),
        (
            FakeReply::units(0, vec![0x61, 0xD800]),
            LegacyError::IllFormedOutput,
        ),
    ];
    for (reply, expected) in cases {
        let (fake, mut client) = connected();
        fake.push_reply(reply);
        assert_eq!(client.process_char(0x61, false, false), Err(expected));
        assert!(!client.is_connected(), "{expected:?} must reset the bridge");
        let journal = fake.take_journal();
        assert_reads_in_bounds(&journal);
        assert!(
            journal.ends_with(&[FakeOp::Unlock, FakeOp::Close]),
            "{expected:?}: the mutex is released, then the handles are closed"
        );
        assert_eq!(
            client.process_char(0x61, false, false),
            Err(LegacyError::NotConnected)
        );
        // Reconnecting with op 5 restores the bridge.
        fake.push_reply(FakeReply::empty());
        client.connect().unwrap();
        assert!(client.is_connected());
    }
}

#[test]
fn matrix_35_the_delete_bound_can_be_narrowed_to_the_host_policy() {
    let (fake, mut client) = connected();
    client.set_max_delete(64);
    assert_eq!(client.max_delete(), 64);
    fake.push_reply(FakeReply::text(65, "x"));
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::DeleteOutOfRange { count: 65 })
    );
    client.set_max_delete(u32::MAX);
    assert_eq!(client.max_delete(), MAX_DELETE_UNITS);
}

#[test]
fn matrix_36_a_stale_epoch_resets_the_bridge_without_output() {
    let (fake, mut client) = connected();
    fake.push_reply(FakeReply::text(0, "x").with_epoch(FakeEpoch::AgeMs(2_000)));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "x")));

    fake.push_reply(FakeReply::text(0, "x").with_epoch(FakeEpoch::AgeMs(2_001)));
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::StaleEpoch { age_ms: 2_001 })
    );
    assert!(!client.is_connected());

    fake.push_reply(FakeReply::empty());
    client.connect().unwrap();
    let future = fake.now() + 10;
    fake.push_reply(FakeReply::text(0, "x").with_epoch(FakeEpoch::Absolute(future)));
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::EpochInFuture)
    );
    assert!(!client.is_connected());
}

#[test]
fn matrix_37_reset_gate_on_requests_op_3_and_off_resets_locally() {
    let (fake, mut client) = connected();
    fake.set_u32(OFF_FLAG_20, 0xFFFF_FFFF);
    fake.set_u32(OFF_FLAG_28, 0xDEAD_BEEF);
    fake.set_u32(OFF_FLAG_2C, 1);

    fake.set_u32(OFF_RESET_GATE, 1);
    fake.push_reply(FakeReply::empty());
    assert_eq!(client.reset(), Ok(ResetOutcome::Remote));
    let journal = fake.take_journal();
    assert_eq!(
        journal[0],
        FakeOp::Read {
            offset: OFF_RESET_GATE,
            len: 4
        }
    );
    assert_eq!(journal[1..], exchange_ops(3, 0, 0, 0, 2));

    fake.set_u32(OFF_RESET_GATE, 0);
    assert_eq!(client.reset(), Ok(ResetOutcome::LocalOnly));
    assert_eq!(
        fake.take_journal(),
        [FakeOp::Read {
            offset: OFF_RESET_GATE,
            len: 4
        }]
    );

    // Values other than 0/1 are not interpreted: fail closed to a local reset.
    fake.set_u32(OFF_RESET_GATE, 2);
    assert_eq!(client.reset(), Ok(ResetOutcome::LocalOnly));
    assert_eq!(signals(&fake.take_journal()), 0);

    client.disconnect();
    assert_eq!(fake.take_journal(), [FakeOp::Close]);
    assert_eq!(client.reset(), Ok(ResetOutcome::LocalOnly));
    assert!(fake.take_journal().is_empty());
}

#[test]
fn unknown_flags_are_preserved_and_never_written() {
    let fake = FakeLegacy::with_controller_objects();
    let flags = [
        (OFF_FLAG_20, 0x8000_0001),
        (OFF_FLAG_28, 7),
        (OFF_FLAG_2C, u32::MAX),
    ];
    for (offset, value) in flags {
        fake.set_u32(offset, value);
    }
    fake.set_u32(OFF_RESET_GATE, 1);
    let mut client = LegacyClient::new(fake.clone());
    fake.push_reply(FakeReply::empty());
    client.connect().unwrap();
    fake.push_reply(FakeReply::text(0, "a"));
    client.process_char(0x61, false, false).unwrap();
    fake.push_reply(FakeReply::text(1, ""));
    client.backspace().unwrap();
    fake.push_reply(FakeReply::empty());
    client.reset().unwrap();
    for (offset, value) in flags {
        assert_eq!(fake.u32_at(offset), value);
    }
    for op in fake.journal() {
        if let FakeOp::Write { offset, bytes } = op {
            assert!(
                offset + bytes.len() <= OFF_FLAG_20,
                "client wrote {offset:#x}"
            );
        }
    }
}

#[test]
fn sequence_wrap_is_handled_with_raw_u32_arithmetic() {
    let (fake, mut client) = connected();
    fake.set_u32(OFF_REQUEST_SEQ, u32::MAX);
    fake.push_reply(FakeReply::text(0, "a"));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "a")));
    assert_eq!(fake.u32_at(OFF_REQUEST_SEQ), 1);
    fake.set_u32(OFF_REQUEST_SEQ, 0x7FFF_FFFF);
    fake.push_reply(FakeReply::text(0, "b"));
    assert_eq!(client.process_char(0x62, false, false), Ok(edit(0, "b")));
    assert_eq!(fake.u32_at(OFF_REQUEST_SEQ), 0x8000_0000);
}

#[test]
fn lone_surrogates_are_never_sent() {
    let (fake, mut client) = connected();
    for unit in [0xD83D, 0xDE00] {
        assert_eq!(
            client.process_char(unit, false, false),
            Err(LegacyError::UnsupportedChar)
        );
    }
    assert!(fake.journal().is_empty());
    assert!(client.is_connected());
}

#[test]
fn an_abandoned_mutex_is_owned_and_released() {
    let (fake, mut client) = connected();
    fake.abandon_mutex_next();
    fake.push_reply(FakeReply::text(0, "a"));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "a")));
    let journal = fake.take_journal();
    assert_eq!(
        journal[0],
        FakeOp::Lock {
            timeout_ms: 100,
            result: LockResult::Abandoned
        }
    );
    assert_eq!(journal.last(), Some(&FakeOp::Unlock));
}

#[test]
fn lost_handles_reset_the_bridge() {
    let (fake, mut client) = connected();
    fake.close_all();
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::ObjectMissing(LegacyObject::Mutex))
    );
    assert!(!client.is_connected());
    assert_eq!(fake.take_journal(), [FakeOp::Close]);
}

#[test]
fn matrix_42_a_fixed_vector_writes_identical_request_bytes() {
    let fake = FakeLegacy::with_controller_objects();
    fake.set_u32(OFF_RESET_GATE, 1);
    let mut client = LegacyClient::new(fake.clone());
    fake.push_reply(FakeReply::empty());
    client.connect().unwrap();
    fake.push_reply(FakeReply::text(0, "a"));
    client.process_char(0x61, false, false).unwrap();
    fake.push_reply(FakeReply::text(1, ""));
    client.backspace().unwrap();
    fake.push_reply(FakeReply::empty());
    assert_eq!(client.reset(), Ok(ResetOutcome::Remote));

    let writes: Vec<(usize, Vec<u8>)> = fake
        .journal()
        .into_iter()
        .filter_map(|op| match op {
            FakeOp::Write { offset, bytes } => Some((offset, bytes)),
            _ => None,
        })
        .collect();
    let golden: Vec<(usize, Vec<u8>)> = vec![
        (0x00, vec![5, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (0x10, vec![1, 0, 0, 0]),
        (
            0x00,
            vec![1, 0, 0, 0, 0x61, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        ),
        (0x10, vec![2, 0, 0, 0]),
        (0x00, vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (0x10, vec![3, 0, 0, 0]),
        (0x00, vec![3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        (0x10, vec![4, 0, 0, 0]),
    ];
    assert_eq!(writes, golden);
    let timeouts: Vec<u64> = fake
        .journal()
        .into_iter()
        .filter_map(|op| match op {
            FakeOp::Lock { timeout_ms, .. } => Some(timeout_ms),
            FakeOp::Wait {
                timeout_ms,
                result: WaitResult::Signaled,
                ..
            } => Some(timeout_ms),
            _ => None,
        })
        .collect();
    assert_eq!(timeouts, [100, 50, 100, 50, 100, 50, 100, 50]);
}

fn assert_holds_nothing(fake: &FakeLegacy) {
    for object in LegacyObject::ALL {
        assert!(!fake.holds(object), "{object:?} is still open");
    }
}

fn assert_holds_everything(fake: &FakeLegacy) {
    for object in LegacyObject::ALL {
        assert!(fake.holds(object), "{object:?} is not open");
    }
}

#[test]
fn a_bridge_reset_closes_every_handle() {
    let (fake, mut client) = connected();
    assert_holds_everything(&fake);
    fake.push_reply(FakeReply::text(0, "x").with_output_len(3));
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::OddOutputLength { len: 3 })
    );
    assert!(!client.is_connected());
    let journal = fake.take_journal();
    assert_eq!(
        journal[journal.len() - 2..],
        [FakeOp::Unlock, FakeOp::Close]
    );
    assert_holds_nothing(&fake);
    // The controller still holds its objects, so the TIP can re-synchronize.
    assert!(fake.exists(LegacyObject::SharedMemory));
    fake.push_reply(FakeReply::empty());
    client.connect().unwrap();
    assert_holds_everything(&fake);
}

#[test]
fn every_way_the_bridge_goes_down_closes_the_handles() {
    // Three failed exchanges in a row.
    let (fake, mut client) = connected();
    for _ in 0..3 {
        let _ = client.process_char(0x61, false, false);
    }
    assert!(!client.is_connected());
    assert_eq!(fake.take_journal().last(), Some(&FakeOp::Close));
    assert_holds_nothing(&fake);

    // A stale epoch.
    let (fake, mut client) = connected();
    fake.push_reply(FakeReply::text(0, "x").with_epoch(FakeEpoch::AgeMs(2_001)));
    assert!(client.process_char(0x61, false, false).is_err());
    assert_eq!(fake.take_journal().last(), Some(&FakeOp::Close));
    assert_holds_nothing(&fake);

    // Lost handles, seen by the reset gate read.
    let (fake, mut client) = connected();
    fake.close_all();
    assert_eq!(
        client.reset(),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );
    assert!(!client.is_connected());
    assert_eq!(fake.take_journal().last(), Some(&FakeOp::Close));

    // An explicit disconnect, for example on TIP deactivation.
    let (fake, mut client) = connected();
    client.disconnect();
    assert_eq!(fake.take_journal(), [FakeOp::Close]);
    assert_holds_nothing(&fake);
    client.disconnect();
    assert!(fake.take_journal().is_empty(), "nothing is left to close");
}

#[test]
fn reconnecting_closes_the_previous_handles_before_reopening() {
    let (fake, mut client) = connected();
    fake.push_reply(FakeReply::empty());
    client.connect().unwrap();
    assert_eq!(
        fake.take_journal()[..3],
        [
            FakeOp::Close,
            // This TIP was the only holder of the mutex, so it is created again.
            FakeOp::OpenOrCreateMutex(MutexOrigin::Created),
            FakeOp::OpenControllerObjects,
        ]
    );
    assert_holds_everything(&fake);
}

#[test]
fn a_failed_connect_keeps_no_handle_open() {
    // The controller has not started: the mutex this TIP created is closed again.
    let fake = FakeLegacy::new();
    let mut client = LegacyClient::new(fake.clone());
    assert_eq!(
        client.connect(),
        Err(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    );
    assert_eq!(
        fake.take_journal(),
        [
            FakeOp::OpenOrCreateMutex(MutexOrigin::Created),
            FakeOp::OpenControllerObjects,
            FakeOp::Close,
        ]
    );
    assert_holds_nothing(&fake);
    assert!(!fake.exists(LegacyObject::Mutex));

    // The objects exist but nobody answers the op 5 sync (an exited controller whose objects
    // another TIP keeps alive): the TIP must not keep them alive between attempts.
    let fake = FakeLegacy::with_controller_objects();
    let mut client = LegacyClient::new(fake.clone());
    assert_eq!(client.connect(), Err(LegacyError::ResponseTimeout));
    assert_eq!(fake.take_journal().last(), Some(&FakeOp::Close));
    assert_holds_nothing(&fake);
}

#[test]
fn dropping_the_client_closes_its_handles() {
    let (fake, client) = connected();
    drop(client);
    assert_eq!(fake.take_journal(), [FakeOp::Close]);
    assert_holds_nothing(&fake);

    let fake = FakeLegacy::with_controller_objects();
    drop(LegacyClient::new(fake.clone()));
    assert!(
        fake.journal().is_empty(),
        "a client that never opened anything closes nothing"
    );
}

#[test]
fn local_resets_between_failed_keys_do_not_hide_consecutive_timeouts() {
    let (fake, mut client) = connected();
    fake.set_u32(OFF_RESET_GATE, 0);
    for attempt in 1..=2 {
        assert_eq!(
            client.process_char(0x61, false, false),
            Err(LegacyError::ResponseTimeout)
        );
        // Spec §6: the TIP passes the key through and resets its context. With the gate off
        // the reset is local only, which is not an exchange and must not clear the count.
        assert_eq!(client.reset(), Ok(ResetOutcome::LocalOnly));
        assert!(client.is_connected(), "attempt {attempt}");
    }
    let started = fake.now();
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::ResponseTimeout)
    );
    assert!(
        !client.is_connected(),
        "the third failed exchange in a row drops the bridge"
    );
    assert_eq!(fake.now() - started, 50);
    assert_eq!(fake.take_journal().last(), Some(&FakeOp::Close));
    // Later keys fail fast instead of each waiting for the silent controller.
    let started = fake.now();
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::NotConnected)
    );
    assert_eq!(client.reset(), Ok(ResetOutcome::LocalOnly));
    assert_eq!(fake.now(), started);
}

#[test]
fn repeated_response_timeouts_drop_the_bridge_so_the_tip_stops_waiting() {
    let (fake, mut client) = connected();
    for attempt in 1..=2 {
        assert_eq!(
            client.process_char(0x61, false, false),
            Err(LegacyError::ResponseTimeout)
        );
        assert!(client.is_connected(), "attempt {attempt}");
    }
    // A success in between resets the count.
    fake.push_reply(FakeReply::text(0, "a"));
    assert_eq!(client.process_char(0x61, false, false), Ok(edit(0, "a")));
    for _ in 0..2 {
        assert_eq!(
            client.process_char(0x61, false, false),
            Err(LegacyError::ResponseTimeout)
        );
    }
    assert!(client.is_connected());
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::ResponseTimeout)
    );
    assert!(!client.is_connected());
    assert_eq!(
        client.process_char(0x61, false, false),
        Err(LegacyError::NotConnected)
    );
}
