use raumakey_ipc::error::{ErrorCode, IpcError, Limit, MalformedReason};
use raumakey_ipc::frame::{
    EncodedPayload, Frame, FrameDecoder, FrameHeader, HEADER_LEN, MAGIC, MAX_CONTROL_PAYLOAD,
    MAX_PAYLOAD, PROTOCOL_VERSION, Sender, encode_frame,
};
use raumakey_ipc::message::{
    Arch, ClientKind, Command, MAX_MACROS, MacroSnapshot, Message, MessageKind, RuntimeSnapshot,
};
use raumakey_model::diag::{Component, DiagCode, DiagEvent, VectorId};
use raumakey_model::host::ExeName;
use raumakey_model::keymap::{KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, Tone};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::policy::{AppRule, ProbeRecord, RuleAction, Transport};

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn macro_entry(id: u64, trigger: &str, replacement: &str) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Whitespace,
        case_policy: CasePolicy::Exact,
    }
}

fn rich_runtime() -> RuntimeSnapshot {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 41;
    snapshot.vietnamese_mode = false;
    snapshot.diagnostics_enabled = true;
    snapshot.keymaps.push(KeyMap {
        id: "my-map".to_owned(),
        version: KEYMAP_SCHEMA_VERSION,
        name: "Bản đồ phím".to_owned(),
        bindings: vec![
            KeyBinding {
                key: 's',
                actions: vec![KeyAction::Tone(Tone::Acute)],
            },
            KeyBinding {
                key: 'w',
                actions: vec![KeyAction::HornOrBreve, KeyAction::Horn],
            },
        ],
    });
    snapshot.compatibility.rules.push(AppRule {
        exe: exe("notepad.exe"),
        window_class: Some("Notepad".to_owned()),
        action: RuleAction {
            transport: Some(Transport::SyntheticKey),
            reset_on_mouse: Some(false),
            macros: None,
            vietnamese: Some(true),
            max_delete_before: Some(32),
        },
    });
    snapshot.compatibility.probes.push(ProbeRecord {
        exe: exe("legacy.exe"),
        probe_version: 1,
        tsf_range_edit: false,
    });
    snapshot
}

fn sample_messages() -> Vec<Message> {
    vec![
        Message::Hello {
            client_kind: ClientKind::Tool,
            pid: u32::MAX,
            arch: Arch::Arm64,
            protocol: PROTOCOL_VERSION,
        },
        Message::Welcome {
            client_id: u64::MAX,
            server_revision: 0,
        },
        Message::RuntimeSnapshot(rich_runtime()),
        Message::MacroSnapshot(MacroSnapshot {
            macro_revision: 7,
            entries: vec![
                macro_entry(1, "ko", "không"),
                macro_entry(2, "vn", "Việt Nam 🇻🇳"),
            ],
        }),
        Message::Command {
            command: Command::SetMode(false),
        },
        Message::Command {
            command: Command::ReportProbe(ProbeRecord {
                exe: exe("winword.exe"),
                probe_version: 1,
                tsf_range_edit: true,
            }),
        },
        Message::CommandResult {
            ok: false,
            code: ErrorCode::COMMAND_UNSUPPORTED,
        },
        Message::ModeChanged {
            vietnamese_mode: true,
            revision: u64::MAX,
        },
        Message::ResetAll,
        Message::Diagnostic(DiagEvent {
            timestamp_ms: 1,
            component: Component::Tsf,
            code: DiagCode(9),
            revision: None,
            host_exe: None,
            vector_id: Some(VectorId::new("A1").unwrap()),
        }),
        Message::Ping,
        Message::Pong,
        Message::Error {
            code: ErrorCode(4_000_000_000),
        },
        Message::Goodbye,
    ]
}

/// Builds a frame by hand so that tests can send bodies the encoder would refuse.
fn raw_frame(kind: u16, client_id: u64, request_id: u64, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(b"RMK1");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&40_u16.to_le_bytes());
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(&client_id.to_le_bytes());
    bytes.extend_from_slice(&request_id.to_le_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&bytes[..32]);
    crc.update(payload);
    bytes.extend_from_slice(&crc.finalize().to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

fn ping_frame() -> Vec<u8> {
    encode_frame(&Frame::new(5, 6, Message::Ping)).unwrap()
}

fn decode_all(decoder: &mut FrameDecoder) -> Vec<Result<Frame, IpcError>> {
    let mut out = Vec::new();
    while let Some(item) = decoder.next_frame() {
        let failed = item.is_err();
        out.push(item);
        if failed {
            break;
        }
    }
    out
}

fn decode_one(bytes: &[u8]) -> Option<Result<Frame, IpcError>> {
    let mut decoder = FrameDecoder::new();
    decoder.feed(bytes);
    decoder.next_frame()
}

fn first_error(bytes: &[u8]) -> IpcError {
    match decode_one(bytes) {
        Some(Err(error)) => error,
        other => panic!("expected an error, got {other:?}"),
    }
}

#[test]
fn constants_match_the_plan() {
    assert_eq!(MAGIC, *b"RMK1");
    assert_eq!(PROTOCOL_VERSION, 1);
    assert_eq!(HEADER_LEN, 40);
    assert_eq!(MAX_PAYLOAD, 8 * 1024 * 1024);
    assert_eq!(MAX_CONTROL_PAYLOAD, 4 * 1024);
}

#[test]
fn only_snapshots_may_use_the_full_payload_limit() {
    for kind in MessageKind::ALL {
        let snapshot = matches!(
            kind,
            MessageKind::RuntimeSnapshot | MessageKind::MacroSnapshot
        );
        let max = kind.max_payload();
        let expected = if snapshot {
            MAX_PAYLOAD
        } else {
            MAX_CONTROL_PAYLOAD
        };
        assert_eq!(max, expected, "{kind:?}");
        for sender in [Sender::Any, Sender::Client, Sender::Server] {
            // At the cap the header is accepted and the decoder waits for the body.
            let mut header = raw_frame(kind.as_u16(), 0, 0, b"");
            header[12..16].copy_from_slice(&max.to_le_bytes());
            let mut decoder = FrameDecoder::for_sender(sender);
            decoder.feed(&header);
            if decoder.next_frame().is_some() {
                // The sender filter refused the kind; the cap is checked for allowed kinds.
                continue;
            }
            assert!(!decoder.is_idle(), "{kind:?}");
            // One byte over is refused from the header alone.
            header[12..16].copy_from_slice(&(max + 1).to_le_bytes());
            let mut decoder = FrameDecoder::for_sender(sender);
            decoder.feed(&header);
            assert_eq!(
                decoder.next_frame(),
                Some(Err(IpcError::Oversize {
                    payload_len: u64::from(max) + 1,
                    max
                })),
                "{kind:?} from {sender:?}"
            );
            assert_eq!(decoder.reserved_payload_bytes(), 0);
        }
    }
}

/// `{"type":"pong","pad":[0,0,…]}` padded to at least `len` bytes: valid JSON whose unknown
/// field serde would buffer in its generic content tree.
fn padded_pong(len: usize) -> Vec<u8> {
    let mut body = br#"{"type":"pong","pad":[0"#.to_vec();
    while body.len() + 2 < len {
        body.extend_from_slice(b",0");
    }
    body.extend_from_slice(b"]}");
    body
}

#[test]
fn a_padded_control_body_is_refused_from_its_header() {
    let body = padded_pong(2 * 1024 * 1024);
    let mut decoder = FrameDecoder::for_sender(Sender::Server);
    decoder.feed(&raw_frame(11, 1, 1, &body));
    assert_eq!(
        decoder.next_frame(),
        Some(Err(IpcError::Oversize {
            payload_len: body.len() as u64,
            max: MAX_CONTROL_PAYLOAD
        }))
    );

    // Within the cap unknown fields are still ignored (forward compatibility).
    let body = padded_pong(MAX_CONTROL_PAYLOAD as usize - 1);
    assert!(body.len() <= MAX_CONTROL_PAYLOAD as usize);
    let mut decoder = FrameDecoder::for_sender(Sender::Server);
    decoder.feed(&raw_frame(11, 1, 1, &body));
    assert_eq!(
        decoder.next_frame(),
        Some(Ok(Frame::new(1, 1, Message::Pong)))
    );
}

#[test]
fn the_largest_control_messages_fit_their_cap_with_room_to_spare() {
    // Exe names are at most 255 characters of at most 4 UTF-8 bytes; JSON does not escape
    // them (control characters and path separators are refused).
    let widest_exe = || ExeName::new(&"\u{1D54F}".repeat(ExeName::MAX_LEN)).unwrap();
    let messages = [
        Message::Diagnostic(DiagEvent {
            timestamp_ms: u64::MAX,
            component: Component::Controller,
            code: DiagCode(u32::MAX),
            revision: Some(u64::MAX),
            host_exe: Some(widest_exe()),
            vector_id: Some(VectorId::new(&"v".repeat(32)).unwrap()),
        }),
        Message::Command {
            command: Command::ReportProbe(ProbeRecord {
                exe: widest_exe(),
                probe_version: u32::MAX,
                tsf_range_edit: true,
            }),
        },
        Message::Hello {
            client_kind: ClientKind::Tool,
            pid: u32::MAX,
            arch: Arch::Arm64,
            protocol: u16::MAX,
        },
        Message::Welcome {
            client_id: u64::MAX,
            server_revision: u64::MAX,
        },
        Message::ModeChanged {
            vietnamese_mode: false,
            revision: u64::MAX,
        },
        Message::CommandResult {
            ok: false,
            code: ErrorCode(u32::MAX),
        },
    ];
    for message in messages {
        let payload = EncodedPayload::encode(&message).unwrap();
        assert!(
            payload.len() * 2 <= MAX_CONTROL_PAYLOAD as usize,
            "{:?} needs {} bytes",
            message.kind(),
            payload.len()
        );
    }
}

#[test]
fn header_is_little_endian_at_the_planned_offsets() {
    let frame = Frame::new(0x0102_0304_0506_0708, 0x1112_1314_1516_1718, Message::Ping);
    let bytes = encode_frame(&frame).unwrap();
    let payload = br#"{"type":"ping"}"#;
    assert_eq!(bytes.len(), 40 + payload.len());
    assert_eq!(&bytes[0..4], b"RMK1");
    assert_eq!(bytes[4..6], [1, 0]);
    assert_eq!(bytes[6..8], [10, 0]);
    assert_eq!(bytes[8..10], [0, 0]);
    assert_eq!(bytes[10..12], [40, 0]);
    assert_eq!(bytes[12..16], (payload.len() as u32).to_le_bytes());
    assert_eq!(bytes[16..24], [8, 7, 6, 5, 4, 3, 2, 1]);
    assert_eq!(
        bytes[24..32],
        [0x18, 0x17, 0x16, 0x15, 0x14, 0x13, 0x12, 0x11]
    );
    let mut crc = crc32fast::Hasher::new();
    crc.update(&bytes[0..32]);
    crc.update(payload);
    assert_eq!(bytes[32..36], crc.finalize().to_le_bytes());
    assert_eq!(bytes[36..40], [0, 0, 0, 0]);
    assert_eq!(&bytes[40..], payload);
    assert_eq!(
        bytes,
        raw_frame(10, frame.client_id, frame.request_id, payload)
    );
}

#[test]
fn header_parses_back_to_its_fields() {
    let bytes = encode_frame(&Frame::new(3, 4, Message::Goodbye)).unwrap();
    let header_bytes: [u8; HEADER_LEN] = bytes[..HEADER_LEN].try_into().unwrap();
    let header = FrameHeader::parse(&header_bytes).unwrap();
    assert_eq!(header.kind, MessageKind::Goodbye);
    assert_eq!(header.payload_len as usize, bytes.len() - HEADER_LEN);
    assert_eq!(header.client_id, 3);
    assert_eq!(header.request_id, 4);
    assert_eq!(header.checksum.to_le_bytes(), bytes[32..36]);
    assert_eq!(header.to_bytes(), header_bytes);
}

#[test]
fn every_message_round_trips_through_a_frame() {
    for (index, message) in sample_messages().into_iter().enumerate() {
        let frame = Frame::new(index as u64 + 1, u64::MAX - index as u64, message);
        let bytes = encode_frame(&frame).unwrap();
        let mut decoder = FrameDecoder::new();
        decoder.feed(&bytes);
        assert_eq!(decoder.next_frame(), Some(Ok(frame)));
        assert_eq!(decoder.next_frame(), None);
        assert!(decoder.is_idle());
    }
}

#[test]
fn encoded_payload_frames_equal_encode_frame() {
    for message in sample_messages() {
        let payload = EncodedPayload::encode(&message).unwrap();
        assert_eq!(payload.kind(), message.kind());
        let direct = encode_frame(&Frame::new(11, 12, message)).unwrap();
        assert_eq!(payload.len(), direct.len() - HEADER_LEN);
        assert_eq!(payload.frame(11, 12), direct);
    }
}

#[test]
fn bad_magic_is_rejected_at_the_first_wrong_byte() {
    assert_eq!(first_error(b"X"), IpcError::BadMagic);
    assert_eq!(first_error(b"RMX"), IpcError::BadMagic);
    let mut bytes = ping_frame();
    bytes[3] = b'2';
    assert_eq!(first_error(&bytes), IpcError::BadMagic);
}

#[test]
fn unsupported_version_is_rejected() {
    let mut bytes = ping_frame();
    bytes[4..6].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(
        first_error(&bytes),
        IpcError::UnsupportedVersion { version: 2 }
    );
    bytes[4..6].copy_from_slice(&0_u16.to_le_bytes());
    assert_eq!(
        first_error(&bytes),
        IpcError::UnsupportedVersion { version: 0 }
    );
}

#[test]
fn bad_header_length_is_rejected() {
    let mut bytes = ping_frame();
    bytes[10..12].copy_from_slice(&48_u16.to_le_bytes());
    assert_eq!(
        first_error(&bytes),
        IpcError::BadHeaderLength { header_len: 48 }
    );
}

#[test]
fn unknown_kind_is_rejected() {
    for kind in [0_u16, 14, u16::MAX] {
        let mut bytes = ping_frame();
        bytes[6..8].copy_from_slice(&kind.to_le_bytes());
        assert_eq!(first_error(&bytes), IpcError::UnknownKind { kind });
    }
}

#[test]
fn nonzero_flags_and_reserved_are_rejected() {
    let mut bytes = ping_frame();
    bytes[8..10].copy_from_slice(&1_u16.to_le_bytes());
    assert_eq!(first_error(&bytes), IpcError::NonZeroFlags { flags: 1 });
    let mut bytes = ping_frame();
    bytes[36..40].copy_from_slice(&0x8000_0000_u32.to_le_bytes());
    assert_eq!(
        first_error(&bytes),
        IpcError::NonZeroReserved {
            reserved: 0x8000_0000
        }
    );
}

#[test]
fn oversize_is_rejected_from_the_header_alone_without_reserving_payload() {
    // Peak allocation is measured with a counting allocator in tests/alloc_bounds.rs.
    let cases = [
        (MessageKind::MacroSnapshot, MAX_PAYLOAD + 1, MAX_PAYLOAD),
        (MessageKind::RuntimeSnapshot, u32::MAX, MAX_PAYLOAD),
        (
            MessageKind::Ping,
            MAX_CONTROL_PAYLOAD + 1,
            MAX_CONTROL_PAYLOAD,
        ),
        (MessageKind::Pong, MAX_PAYLOAD, MAX_CONTROL_PAYLOAD),
    ];
    for (kind, declared, max) in cases {
        let mut header = raw_frame(kind.as_u16(), 0, 0, b"");
        header[12..16].copy_from_slice(&declared.to_le_bytes());
        assert_eq!(header.len(), HEADER_LEN);
        let mut decoder = FrameDecoder::new();
        decoder.feed(&header);
        assert_eq!(
            decoder.next_frame(),
            Some(Err(IpcError::Oversize {
                payload_len: u64::from(declared),
                max
            })),
            "{kind:?}"
        );
        assert_eq!(decoder.reserved_payload_bytes(), 0);
    }
}

#[test]
fn payload_buffer_grows_with_received_bytes_not_with_the_declared_length() {
    let mut header = raw_frame(4, 0, 0, b"");
    header[12..16].copy_from_slice(&MAX_PAYLOAD.to_le_bytes());
    let mut decoder = FrameDecoder::new();
    decoder.feed(&header);
    assert_eq!(decoder.next_frame(), None);
    assert!(!decoder.is_idle());
    assert_eq!(decoder.reserved_payload_bytes(), 0);
    decoder.feed(&[b' '; 1000]);
    assert_eq!(decoder.next_frame(), None);
    assert!(decoder.reserved_payload_bytes() >= 1000);
    assert!(decoder.reserved_payload_bytes() <= 2000);
}

#[test]
fn checksum_mismatch_is_detected_in_payload_and_header() {
    let mut bytes = ping_frame();
    let last = bytes.len() - 3;
    // `{"type":"ping"}` becomes `{"type":"pong"}`: valid JSON, but not what was checksummed.
    bytes[last] = b'o';
    assert!(matches!(
        first_error(&bytes),
        IpcError::ChecksumMismatch { .. }
    ));

    let mut bytes = ping_frame();
    bytes[16] ^= 0x01;
    assert!(matches!(
        first_error(&bytes),
        IpcError::ChecksumMismatch { .. }
    ));

    let mut bytes = ping_frame();
    bytes[33] ^= 0x80;
    let IpcError::ChecksumMismatch { expected, actual } = first_error(&bytes) else {
        panic!("expected a checksum mismatch");
    };
    assert_ne!(expected, actual);
}

#[test]
fn malformed_bodies_with_valid_checksums_are_rejected() {
    let error = first_error(&raw_frame(10, 0, 0, b"not json secret"));
    assert_eq!(error, IpcError::Malformed(MalformedReason::Json));
    assert!(!error.to_string().contains("secret"));
    assert_eq!(
        first_error(&raw_frame(10, 0, 0, b"")),
        IpcError::Malformed(MalformedReason::Json)
    );
    assert_eq!(
        first_error(&raw_frame(10, 0, 0, br#"{"type":"bogus"}"#)),
        IpcError::Malformed(MalformedReason::Json)
    );
    assert_eq!(
        first_error(&raw_frame(10, 0, 0, br#"{"type":"pong"}"#)),
        IpcError::Malformed(MalformedReason::KindMismatch)
    );
    assert_eq!(
        first_error(&raw_frame(
            4,
            0,
            0,
            br#"{"type":"pong","macro_revision":1,"entries":[]}"#
        )),
        IpcError::Malformed(MalformedReason::KindMismatch)
    );
    assert_eq!(
        first_error(&raw_frame(4, 0, 0, br#"{"macro_revision":1,"entries":[]}"#)),
        IpcError::Malformed(MalformedReason::Json)
    );
    assert_eq!(
        first_error(&raw_frame(
            6,
            0,
            0,
            br#"{"type":"command_result","ok":true,"code":6}"#
        )),
        IpcError::Malformed(MalformedReason::InconsistentResult)
    );
}

#[test]
fn entry_limits_and_invalid_options_are_rejected_on_decode() {
    let over = MacroSnapshot {
        macro_revision: 1,
        entries: (0..=MAX_MACROS as u64)
            .map(|i| macro_entry(i, "a", "b"))
            .collect(),
    };
    let payload = serde_json::to_vec(&Message::MacroSnapshot(over)).unwrap();
    assert_eq!(
        first_error(&raw_frame(4, 0, 0, &payload)),
        IpcError::LimitExceeded {
            limit: Limit::Macros,
            count: MAX_MACROS as u64 + 1
        }
    );

    let mut runtime = RuntimeSnapshot::defaults_v1();
    runtime.options.context_limit_codepoints = 1_000;
    let payload = serde_json::to_vec(&Message::RuntimeSnapshot(runtime)).unwrap();
    assert_eq!(
        first_error(&raw_frame(3, 0, 0, &payload)),
        IpcError::Malformed(MalformedReason::InvalidOptions)
    );
}

#[test]
fn decoder_stays_failed_after_an_error() {
    let mut decoder = FrameDecoder::new();
    decoder.feed(b"garbage");
    assert_eq!(decoder.next_frame(), Some(Err(IpcError::BadMagic)));
    decoder.feed(&ping_frame());
    assert_eq!(decoder.next_frame(), Some(Err(IpcError::BadMagic)));
    assert_eq!(decoder.next_frame(), Some(Err(IpcError::BadMagic)));
    assert_eq!(decoder.failure(), Some(IpcError::BadMagic));
}

#[test]
fn frames_completed_before_an_error_are_delivered_first() {
    let mut bytes = ping_frame();
    bytes.extend_from_slice(&encode_frame(&Frame::new(5, 7, Message::Pong)).unwrap());
    bytes.extend_from_slice(b"junk");
    let mut decoder = FrameDecoder::new();
    decoder.feed(&bytes);
    assert_eq!(
        decode_all(&mut decoder),
        [
            Ok(Frame::new(5, 6, Message::Ping)),
            Ok(Frame::new(5, 7, Message::Pong)),
            Err(IpcError::BadMagic),
        ]
    );
}

#[test]
fn chunked_feeding_yields_the_same_frames_as_one_shot_feeding() {
    let frames: Vec<Frame> = sample_messages()
        .into_iter()
        .enumerate()
        .map(|(i, m)| Frame::new(1, i as u64, m))
        .collect();
    let mut stream = Vec::new();
    for frame in &frames {
        stream.extend_from_slice(&encode_frame(frame).unwrap());
    }

    let expected: Vec<Result<Frame, IpcError>> = frames.iter().cloned().map(Ok).collect();
    for chunk_size in [1, 2, 3, 7, 39, 40, 41, 64, 1000, stream.len()] {
        let mut decoder = FrameDecoder::new();
        let mut out = Vec::new();
        for chunk in stream.chunks(chunk_size) {
            decoder.feed(chunk);
            out.extend(decode_all(&mut decoder));
        }
        assert_eq!(out, expected, "chunk size {chunk_size}");
        assert!(decoder.is_idle());
    }

    let mut decoder = FrameDecoder::new();
    let mut out = Vec::new();
    let mut rest = stream.as_slice();
    let mut size = 1;
    while !rest.is_empty() {
        let take = size.min(rest.len());
        let (chunk, tail) = rest.split_at(take);
        decoder.feed(chunk);
        out.extend(decode_all(&mut decoder));
        rest = tail;
        size = size * 3 % 97 + 1;
    }
    assert_eq!(out, expected);
}

#[test]
fn idle_state_tracks_partial_frames() {
    let bytes = ping_frame();
    let mut decoder = FrameDecoder::new();
    assert!(decoder.is_idle());
    decoder.feed(&bytes[..10]);
    assert!(!decoder.is_idle());
    decoder.feed(&bytes[10..HEADER_LEN + 2]);
    assert!(!decoder.is_idle());
    decoder.feed(&bytes[HEADER_LEN + 2..]);
    assert!(decoder.is_idle());
    assert!(matches!(decoder.next_frame(), Some(Ok(_))));
}

#[test]
fn encoder_enforces_limits_and_payload_size() {
    let over = MacroSnapshot {
        macro_revision: 1,
        entries: (0..=MAX_MACROS as u64)
            .map(|i| macro_entry(i, "a", "b"))
            .collect(),
    };
    assert_eq!(
        encode_frame(&Frame::new(1, 1, Message::MacroSnapshot(over))),
        Err(IpcError::LimitExceeded {
            limit: Limit::Macros,
            count: MAX_MACROS as u64 + 1
        })
    );

    let long = "x".repeat(4_000);
    let huge = MacroSnapshot {
        macro_revision: 1,
        entries: (0..3_000).map(|i| macro_entry(i, "k", &long)).collect(),
    };
    let result = encode_frame(&Frame::new(1, 1, Message::MacroSnapshot(huge)));
    let Err(IpcError::Oversize { payload_len, max }) = result else {
        panic!("expected oversize, got {result:?}");
    };
    assert_eq!(max, MAX_PAYLOAD);
    assert!(payload_len > u64::from(MAX_PAYLOAD));

    assert_eq!(
        encode_frame(&Frame::new(
            1,
            1,
            Message::Welcome {
                client_id: 0,
                server_revision: 0
            }
        )),
        Err(IpcError::Malformed(MalformedReason::InvalidClientId))
    );
}

#[test]
fn client_frame_decoder_rejects_server_kinds_and_large_payloads_from_the_header() {
    let mut header = raw_frame(3, 0, 0, b"");
    header[12..16].copy_from_slice(&100_u32.to_le_bytes());
    let mut decoder = FrameDecoder::for_sender(Sender::Client);
    decoder.feed(&header);
    assert_eq!(
        decoder.next_frame(),
        Some(Err(IpcError::UnexpectedMessage(
            MessageKind::RuntimeSnapshot
        )))
    );

    let mut header = raw_frame(5, 0, 0, b"");
    header[12..16].copy_from_slice(&(MAX_CONTROL_PAYLOAD + 1).to_le_bytes());
    let mut decoder = FrameDecoder::for_sender(Sender::Client);
    decoder.feed(&header);
    assert_eq!(
        decoder.next_frame(),
        Some(Err(IpcError::Oversize {
            payload_len: u64::from(MAX_CONTROL_PAYLOAD) + 1,
            max: MAX_CONTROL_PAYLOAD
        }))
    );
    // Every kind a client may send is a control kind.
    assert_eq!(decoder.max_payload(), MAX_CONTROL_PAYLOAD);

    let hello = Message::Hello {
        client_kind: ClientKind::Tip,
        pid: 1,
        arch: Arch::X86,
        protocol: 1,
    };
    let mut decoder = FrameDecoder::for_sender(Sender::Server);
    decoder.feed(&encode_frame(&Frame::new(0, 1, hello.clone())).unwrap());
    assert_eq!(
        decoder.next_frame(),
        Some(Err(IpcError::UnexpectedMessage(MessageKind::Hello)))
    );
    assert_eq!(decoder.max_payload(), MAX_PAYLOAD);

    let mut decoder = FrameDecoder::for_sender(Sender::Client);
    decoder.feed(&encode_frame(&Frame::new(0, 1, hello.clone())).unwrap());
    assert_eq!(decoder.next_frame(), Some(Ok(Frame::new(0, 1, hello))));
}

#[test]
fn error_codes_and_diagnostic_codes_are_stable() {
    assert_eq!(IpcError::BadMagic.wire_code(), ErrorCode::MALFORMED_FRAME);
    assert_eq!(
        IpcError::UnsupportedVersion { version: 2 }.wire_code(),
        ErrorCode::UNSUPPORTED_PROTOCOL
    );
    assert_eq!(
        IpcError::ProtocolMismatch { protocol: 9 }.wire_code(),
        ErrorCode::UNSUPPORTED_PROTOCOL
    );
    assert_eq!(
        IpcError::ClientIdMismatch {
            expected: 1,
            actual: 2
        }
        .wire_code(),
        ErrorCode::CLIENT_ID_MISMATCH
    );
    assert_eq!(
        IpcError::UnexpectedMessage(MessageKind::Hello).wire_code(),
        ErrorCode::UNEXPECTED_MESSAGE
    );
    assert_eq!(
        IpcError::LimitExceeded {
            limit: Limit::Keymaps,
            count: 65
        }
        .wire_code(),
        ErrorCode::LIMIT_EXCEEDED
    );
    let all = [
        IpcError::BadMagic,
        IpcError::UnsupportedVersion { version: 0 },
        IpcError::BadHeaderLength { header_len: 0 },
        IpcError::UnknownKind { kind: 0 },
        IpcError::NonZeroFlags { flags: 1 },
        IpcError::NonZeroReserved { reserved: 1 },
        IpcError::Oversize {
            payload_len: 0,
            max: 0,
        },
        IpcError::ChecksumMismatch {
            expected: 0,
            actual: 1,
        },
        IpcError::Malformed(MalformedReason::Json),
        IpcError::LimitExceeded {
            limit: Limit::Macros,
            count: 0,
        },
        IpcError::UnexpectedMessage(MessageKind::Ping),
        IpcError::ClientIdMismatch {
            expected: 0,
            actual: 1,
        },
        IpcError::ProtocolMismatch { protocol: 0 },
        IpcError::Remote(ErrorCode::OK),
        IpcError::Closed,
        IpcError::Truncated,
        IpcError::Io(std::io::ErrorKind::Other),
        IpcError::NotConnected,
        IpcError::ClientIdsExhausted,
    ];
    let mut codes: Vec<u32> = all.iter().map(|e| e.diag_code().0).collect();
    assert!(codes.iter().all(|c| (7000..7100).contains(c)));
    codes.sort_unstable();
    codes.dedup();
    assert_eq!(codes.len(), all.len());
}
