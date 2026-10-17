//! Legacy block layout, constants, and pure response/request validation
//! (clean-room spec §6; matrix #34, #35, #36, #42).

#![cfg(feature = "evkey-compat")]

use std::time::Duration;

use raumakey_ipc::legacy::{
    BLOCK_SIZE, EPOCH_VALIDITY_MS, LEGACY_INJECTED_SENTINEL, LegacyBlock, LegacyEdit, LegacyError,
    LegacyObject, LegacyOpcode, LegacyRequest, MAX_DELETE_UNITS, MAX_OUTPUT_UNITS, MUTEX_NAME,
    MUTEX_WAIT, OFF_CAPS, OFF_CHAR, OFF_DELETE_COUNT, OFF_EPOCH, OFF_FLAG_2C, OFF_FLAG_20,
    OFF_FLAG_28, OFF_OPCODE, OFF_OUTPUT, OFF_OUTPUT_LEN, OFF_REQUEST_SEQ, OFF_RESET_GATE,
    OFF_RESPONSE_SEQ, OFF_SHIFT, OUTPUT_CAPACITY, REQUEST_EVENT_NAME, RESPONSE_EVENT_NAME,
    RESPONSE_WAIT, RejectReason, SHARED_MEMORY_NAME, SHUTDOWN_WAIT, decode_request,
    is_legacy_injected, next_sequence, validate_response,
};
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::InputEvent;
use raumakey_model::transaction::{EditTransaction, TextSimulator};
use raumakey_model::utf16;

const NOW: u64 = 50_000;

fn response(sequence: u32, delete: u32, text: &str) -> LegacyBlock {
    let mut block = LegacyBlock::zeroed();
    block.set_response_sequence(sequence);
    block.set_delete_count(delete);
    block.set_output(&utf16::encode(text)).unwrap();
    block.set_epoch(NOW);
    block
}

fn u32_at(block: &LegacyBlock, offset: usize) -> u32 {
    u32::from_le_bytes(block.as_bytes()[offset..offset + 4].try_into().unwrap())
}

#[test]
fn matrix_42_layout_names_timeouts_and_sentinel_are_fixed_constants() {
    assert_eq!(BLOCK_SIZE, 0x1038);
    assert_eq!(
        [OFF_OPCODE, OFF_CHAR, OFF_SHIFT, OFF_CAPS],
        [0x00, 0x04, 0x08, 0x0C]
    );
    assert_eq!(
        [
            OFF_REQUEST_SEQ,
            OFF_RESPONSE_SEQ,
            OFF_DELETE_COUNT,
            OFF_OUTPUT_LEN
        ],
        [0x10, 0x14, 0x18, 0x1C]
    );
    assert_eq!(
        [OFF_FLAG_20, OFF_RESET_GATE, OFF_FLAG_28, OFF_FLAG_2C],
        [0x20, 0x24, 0x28, 0x2C]
    );
    assert_eq!([OFF_EPOCH, OFF_OUTPUT], [0x30, 0x38]);
    assert_eq!(OUTPUT_CAPACITY, 0x1000);
    assert_eq!(OFF_OUTPUT + OUTPUT_CAPACITY, BLOCK_SIZE);
    assert_eq!(MAX_OUTPUT_UNITS, 0x800);
    assert_eq!(MAX_DELETE_UNITS, 0x800);

    assert_eq!(SHARED_MEMORY_NAME, "EVKeyIME_SharedMem_v1");
    assert_eq!(REQUEST_EVENT_NAME, "EVKeyIME_Request_v1");
    assert_eq!(RESPONSE_EVENT_NAME, "EVKeyIME_Response_v1");
    assert_eq!(MUTEX_NAME, "EVKeyIME_Mutex_v1");
    assert_eq!(LegacyObject::SharedMemory.name(), SHARED_MEMORY_NAME);
    assert_eq!(LegacyObject::RequestEvent.name(), REQUEST_EVENT_NAME);
    assert_eq!(LegacyObject::ResponseEvent.name(), RESPONSE_EVENT_NAME);
    assert_eq!(LegacyObject::Mutex.name(), MUTEX_NAME);
    assert_eq!(LegacyObject::ALL.len(), 4);

    assert_eq!(MUTEX_WAIT, Duration::from_millis(100));
    assert_eq!(RESPONSE_WAIT, Duration::from_millis(50));
    assert_eq!(EPOCH_VALIDITY_MS, 2_000);
    assert_eq!(SHUTDOWN_WAIT, Duration::from_millis(2_000));
    assert_eq!(LEGACY_INJECTED_SENTINEL, 0xEBCA_1234);

    let opcodes = [
        (LegacyOpcode::ProcessChar, 1),
        (LegacyOpcode::Backspace, 2),
        (LegacyOpcode::Reset, 3),
        (LegacyOpcode::Sync, 5),
    ];
    for (opcode, raw) in opcodes {
        assert_eq!(opcode.as_u32(), raw);
        assert_eq!(LegacyOpcode::from_u32(raw), Some(opcode));
    }
    for raw in [0, 4, 6, 0x8000_0001, u32::MAX] {
        assert_eq!(LegacyOpcode::from_u32(raw), None);
    }
}

#[test]
fn matrix_42_block_fields_are_little_endian_at_the_documented_offsets() {
    let mut block = LegacyBlock::zeroed();
    block.set_opcode(1);
    block.set_char_unit(0x00E2);
    block.set_shift(1);
    block.set_caps(0);
    block.set_request_sequence(0x0102_0304);
    block.set_response_sequence(0xA0B0_C0D0);
    block.set_delete_count(2);
    block.set_unknown_flags([0x1111_1111, 0x2828_2828, 0x2C2C_2C2C]);
    block.set_reset_gate(1);
    block.set_epoch(0x1122_3344_5566_7788);
    block.set_output(&[0x1EBF, 0x0061]).unwrap();

    let bytes = block.as_bytes();
    assert_eq!(bytes.len(), BLOCK_SIZE);
    assert_eq!(bytes[0x00..0x04], [1, 0, 0, 0]);
    assert_eq!(bytes[0x04..0x08], [0xE2, 0, 0, 0]);
    assert_eq!(bytes[0x08..0x0C], [1, 0, 0, 0]);
    assert_eq!(bytes[0x0C..0x10], [0, 0, 0, 0]);
    assert_eq!(bytes[0x10..0x14], [4, 3, 2, 1]);
    assert_eq!(bytes[0x14..0x18], [0xD0, 0xC0, 0xB0, 0xA0]);
    assert_eq!(bytes[0x18..0x1C], [2, 0, 0, 0]);
    assert_eq!(bytes[0x1C..0x20], [4, 0, 0, 0]);
    assert_eq!(bytes[0x20..0x24], [0x11; 4]);
    assert_eq!(bytes[0x24..0x28], [1, 0, 0, 0]);
    assert_eq!(bytes[0x28..0x2C], [0x28; 4]);
    assert_eq!(bytes[0x2C..0x30], [0x2C; 4]);
    assert_eq!(
        bytes[0x30..0x38],
        [0x88, 0x77, 0x66, 0x55, 0x44, 0x33, 0x22, 0x11]
    );
    assert_eq!(bytes[0x38..0x3C], [0xBF, 0x1E, 0x61, 0x00]);
    assert!(bytes[0x3C..].iter().all(|b| *b == 0));

    assert_eq!(block.opcode(), 1);
    assert_eq!(block.char_unit(), 0xE2);
    assert_eq!(block.shift(), 1);
    assert_eq!(block.caps(), 0);
    assert_eq!(block.request_sequence(), 0x0102_0304);
    assert_eq!(block.response_sequence(), 0xA0B0_C0D0);
    assert_eq!(block.delete_count(), 2);
    assert_eq!(block.output_len(), 4);
    assert_eq!(
        block.unknown_flags(),
        [0x1111_1111, 0x2828_2828, 0x2C2C_2C2C]
    );
    assert_eq!(block.reset_gate(), 1);
    assert_eq!(block.epoch(), 0x1122_3344_5566_7788);
    assert_eq!(block.output_bytes().len(), OUTPUT_CAPACITY);

    let copy = LegacyBlock::from_bytes(bytes).unwrap();
    assert_eq!(copy, block);
    assert!(LegacyBlock::from_bytes(&bytes[..BLOCK_SIZE - 1]).is_none());
}

#[test]
fn output_longer_than_the_block_cannot_be_stored() {
    let mut block = LegacyBlock::zeroed();
    assert!(block.set_output(&vec![0x61; MAX_OUTPUT_UNITS]).is_ok());
    assert_eq!(block.output_len(), 0x1000);
    assert_eq!(
        block.set_output(&vec![0x61; MAX_OUTPUT_UNITS + 1]),
        Err(LegacyError::OutputTooLong { len: 0x1002 })
    );
    assert_eq!(block.output_len(), 0x1000);
}

#[test]
fn sequence_increments_as_raw_u32_with_wrap_and_never_yields_zero() {
    assert_eq!(next_sequence(0), 1);
    assert_eq!(next_sequence(41), 42);
    assert_eq!(next_sequence(0x7FFF_FFFF), 0x8000_0000);
    assert_eq!(next_sequence(0xFFFF_FFFE), 0xFFFF_FFFF);
    assert_eq!(next_sequence(u32::MAX), 1);
}

#[test]
fn legacy_sentinel_is_recognised_exactly() {
    assert!(is_legacy_injected(0xEBCA_1234));
    assert!(!is_legacy_injected(0));
    assert!(!is_legacy_injected(0x1_EBCA_1234));
    assert!(!is_legacy_injected(RAUMAKEY_INJECTED_TAG));
}

#[test]
fn a_matching_fresh_response_is_accepted() {
    let edit = validate_response(&response(7, 2, "ế"), 7, NOW, MAX_DELETE_UNITS).unwrap();
    assert_eq!(edit.delete_before, 2);
    assert_eq!(edit.insert_utf16, utf16::encode("ế"));
    let full = "a".repeat(MAX_OUTPUT_UNITS);
    let edit = validate_response(&response(7, 0, &full), 7, NOW, MAX_DELETE_UNITS).unwrap();
    assert_eq!(edit.insert_utf16.len(), MAX_OUTPUT_UNITS);
}

#[test]
fn matrix_34_another_sequence_is_rejected() {
    assert_eq!(
        validate_response(&response(8, 1, "x"), 7, NOW, MAX_DELETE_UNITS),
        Err(LegacyError::SequenceMismatch {
            expected: 7,
            actual: 8
        })
    );
    // Raw comparison: no signed interpretation of the high bit.
    assert_eq!(
        validate_response(&response(0x8000_0007, 1, "x"), 7, NOW, MAX_DELETE_UNITS),
        Err(LegacyError::SequenceMismatch {
            expected: 7,
            actual: 0x8000_0007
        })
    );
    assert!(validate_response(&response(0x8000_0007, 1, "x"), 0x8000_0007, NOW, 64).is_ok());
}

#[test]
fn matrix_35_odd_oversized_and_high_bit_lengths_and_counts_are_rejected() {
    for (len, expected) in [
        (3, LegacyError::OddOutputLength { len: 3 }),
        (0x1002, LegacyError::OutputTooLong { len: 0x1002 }),
        (0x8000_0002, LegacyError::OutputTooLong { len: 0x8000_0002 }),
        (
            0x8000_0001,
            LegacyError::OddOutputLength { len: 0x8000_0001 },
        ),
        (u32::MAX, LegacyError::OddOutputLength { len: u32::MAX }),
    ] {
        let mut block = response(1, 0, "");
        block.set_output_len(len);
        assert_eq!(
            validate_response(&block, 1, NOW, MAX_DELETE_UNITS),
            Err(expected),
            "length {len:#x}"
        );
    }
    for count in [0x8000_0000, u32::MAX, MAX_DELETE_UNITS + 1] {
        assert_eq!(
            validate_response(&response(1, count, ""), 1, NOW, u32::MAX),
            Err(LegacyError::DeleteOutOfRange { count })
        );
    }
    assert_eq!(
        validate_response(&response(1, 65, ""), 1, NOW, 64),
        Err(LegacyError::DeleteOutOfRange { count: 65 })
    );
    let mut block = response(1, 0, "");
    block.set_output(&[0xD800]).unwrap();
    assert_eq!(
        validate_response(&block, 1, NOW, MAX_DELETE_UNITS),
        Err(LegacyError::IllFormedOutput)
    );
}

#[test]
fn matrix_36_epochs_older_than_2000_ms_or_in_the_future_are_stale() {
    let mut block = response(1, 0, "a");
    block.set_epoch(NOW - 2_000);
    assert!(validate_response(&block, 1, NOW, MAX_DELETE_UNITS).is_ok());
    block.set_epoch(NOW - 2_001);
    assert_eq!(
        validate_response(&block, 1, NOW, MAX_DELETE_UNITS),
        Err(LegacyError::StaleEpoch { age_ms: 2_001 })
    );
    block.set_epoch(NOW + 1);
    assert_eq!(
        validate_response(&block, 1, NOW, MAX_DELETE_UNITS),
        Err(LegacyError::EpochInFuture)
    );
}

#[test]
fn bridge_resetting_errors_are_classified() {
    for error in [
        LegacyError::OddOutputLength { len: 1 },
        LegacyError::OutputTooLong { len: 0x1002 },
        LegacyError::DeleteOutOfRange { count: 0x8000_0000 },
        LegacyError::IllFormedOutput,
        LegacyError::StaleEpoch { age_ms: 2_001 },
        LegacyError::EpochInFuture,
        LegacyError::ObjectMissing(LegacyObject::SharedMemory),
        LegacyError::OutOfBounds,
        LegacyError::Channel { code: 6 },
    ] {
        assert!(error.resets_bridge(), "{error:?}");
    }
    for error in [
        LegacyError::NotConnected,
        LegacyError::MutexTimeout,
        LegacyError::ResponseTimeout,
        LegacyError::SequenceMismatch {
            expected: 1,
            actual: 2,
        },
        LegacyError::UnsupportedChar,
        LegacyError::ConflictingObject(LegacyObject::SharedMemory),
    ] {
        assert!(!error.resets_bridge(), "{error:?}");
    }
}

#[test]
fn requests_decode_with_range_checked_raw_fields() {
    let request = |opcode: u32, unit: u32, shift: u32, caps: u32| {
        let mut block = LegacyBlock::zeroed();
        block.set_opcode(opcode);
        block.set_char_unit(unit);
        block.set_shift(shift);
        block.set_caps(caps);
        block.set_request_sequence(0xFFFF_FFF0);
        decode_request(&block)
    };
    assert_eq!(
        request(1, 0x61, 0, 1),
        Ok((
            0xFFFF_FFF0,
            LegacyRequest::ProcessChar {
                unit: 0x61,
                shift: false,
                caps: true
            }
        ))
    );
    assert_eq!(
        request(2, 0xFFFF_FFFF, 9, 9),
        Ok((0xFFFF_FFF0, LegacyRequest::Backspace))
    );
    assert_eq!(request(3, 0, 0, 0), Ok((0xFFFF_FFF0, LegacyRequest::Reset)));
    assert_eq!(request(5, 0, 0, 0), Ok((0xFFFF_FFF0, LegacyRequest::Sync)));
    for opcode in [0, 4, 6, 0x8000_0001, u32::MAX] {
        assert_eq!(
            request(opcode, 0x61, 0, 0),
            Err(RejectReason::UnknownOpcode(opcode))
        );
    }
    for unit in [0xD800, 0xDFFF, 0x1_0000, 0x8000_0061, u32::MAX] {
        assert_eq!(request(1, unit, 0, 0), Err(RejectReason::InvalidChar));
    }
    assert_eq!(request(1, 0x61, 2, 0), Err(RejectReason::InvalidFlag));
    assert_eq!(
        request(1, 0x61, 0, 0x8000_0000),
        Err(RejectReason::InvalidFlag)
    );
    assert_eq!(
        LegacyRequest::ProcessChar {
            unit: 1,
            shift: false,
            caps: false
        }
        .opcode(),
        LegacyOpcode::ProcessChar
    );
    assert_eq!(LegacyRequest::Sync.opcode(), LegacyOpcode::Sync);
}

#[test]
fn edits_convert_to_transactions() {
    assert_eq!(
        LegacyEdit::empty().to_transaction(),
        EditTransaction::pass_through()
    );
    assert!(LegacyEdit::empty().is_empty());
    let edit = LegacyEdit {
        delete_before: 1,
        insert_utf16: utf16::encode("â"),
    };
    assert_eq!(edit.to_transaction(), EditTransaction::replace(1, "â"));
    let host = raumakey_model::host::HostContext::new(
        raumakey_model::host::SessionId::from_u128(1),
        raumakey_model::host::ExeName::new("notepad.exe").unwrap(),
    );
    let mut text = TextSimulator::with_text("a");
    text.apply(&InputEvent::character('a', host), &edit.to_transaction())
        .unwrap();
    assert_eq!(text.as_string(), "â");
}

#[test]
fn edits_validate_their_bounds() {
    let ok = LegacyEdit {
        delete_before: 64,
        insert_utf16: vec![0x61; MAX_OUTPUT_UNITS],
    };
    assert_eq!(ok.validate(64), Ok(()));
    assert_eq!(
        ok.validate(63),
        Err(LegacyError::DeleteOutOfRange { count: 64 })
    );
    let long = LegacyEdit {
        delete_before: 0,
        insert_utf16: vec![0x61; MAX_OUTPUT_UNITS + 1],
    };
    assert_eq!(
        long.validate(64),
        Err(LegacyError::OutputTooLong { len: 0x1002 })
    );
    let broken = LegacyEdit {
        delete_before: 0,
        insert_utf16: vec![0xDC00],
    };
    assert_eq!(broken.validate(64), Err(LegacyError::IllFormedOutput));
}

#[test]
fn debug_output_never_contains_typed_or_replacement_text() {
    let edit = LegacyEdit {
        delete_before: 1,
        insert_utf16: utf16::encode("SECRET"),
    };
    let text = format!("{edit:?}");
    assert!(!text.contains("SECRET") && !text.contains("83"), "{text}");
    let request = LegacyRequest::ProcessChar {
        unit: 0x2603,
        shift: true,
        caps: false,
    };
    let text = format!("{request:?}");
    assert!(
        !text.contains("2603") && !text.contains("9731") && !text.contains('☃'),
        "{text}"
    );
    let mut block = response(1, 0, "SECRET");
    block.set_char_unit(0x2603);
    let text = format!("{block:?}");
    assert!(!text.contains("SECRET") && !text.contains("9731"), "{text}");
    assert_eq!(u32_at(&block, OFF_CHAR), 0x2603);
}

#[test]
fn failures_pass_the_key_through_and_reset_the_context() {
    use raumakey_ipc::legacy::transaction_for;
    assert_eq!(
        transaction_for(&Ok(LegacyEdit {
            delete_before: 1,
            insert_utf16: utf16::encode("â"),
        })),
        EditTransaction::replace(1, "â")
    );
    assert_eq!(
        transaction_for(&Ok(LegacyEdit::empty())),
        EditTransaction::pass_through()
    );
    for error in [
        LegacyError::NotConnected,
        LegacyError::MutexTimeout,
        LegacyError::ResponseTimeout,
        LegacyError::SequenceMismatch {
            expected: 1,
            actual: 2,
        },
        LegacyError::StaleEpoch { age_ms: 2_001 },
        LegacyError::OddOutputLength { len: 3 },
    ] {
        assert_eq!(
            transaction_for(&Err(error)),
            EditTransaction::pass_through_reset()
        );
    }
}
