//! Property tests: the decoders never panic and are insensitive to chunking; messages round
//! trip; legacy validation never panics and handles sequence wrap.

use proptest::prelude::*;
use raumakey_ipc::error::{ErrorCode, IpcError};
use raumakey_ipc::frame::{Frame, FrameDecoder, HEADER_LEN, Sender, encode_frame};
use raumakey_ipc::message::{Arch, ClientKind, Command, MacroSnapshot, Message, RuntimeSnapshot};
use raumakey_model::diag::{Component, DiagCode, DiagEvent};
use raumakey_model::host::ExeName;
use raumakey_model::hotkey::{Hotkey, HotkeyConfig, HotkeyKey};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::policy::{AppRule, ProbeRecord, RuleAction};

fn message() -> impl Strategy<Value = Message> {
    let entry = (any::<u64>(), "\\PC{1,8}", "\\PC{1,24}", any::<bool>()).prop_map(
        |(id, trigger, replacement, enabled)| MacroEntry {
            id: MacroId(id),
            trigger,
            replacement,
            enabled,
            boundary: BoundaryPolicy::Delimiter,
            case_policy: CasePolicy::Exact,
        },
    );
    prop_oneof![
        (
            any::<u32>(),
            any::<u16>(),
            prop::sample::select(vec![ClientKind::Tip, ClientKind::Tool]),
            prop::sample::select(vec![Arch::X86, Arch::X64, Arch::Arm64, Arch::Other]),
        )
            .prop_map(|(pid, protocol, client_kind, arch)| Message::Hello {
                client_kind,
                pid,
                arch,
                protocol,
            }),
        (1..=u64::MAX, any::<u64>()).prop_map(|(client_id, server_revision)| Message::Welcome {
            client_id,
            server_revision,
        }),
        (any::<u64>(), any::<bool>(), any::<bool>()).prop_map(|(revision, mode, diagnostics)| {
            let mut snapshot = RuntimeSnapshot::defaults_v1();
            snapshot.config_revision = revision;
            snapshot.vietnamese_mode = mode;
            snapshot.diagnostics_enabled = diagnostics;
            Message::RuntimeSnapshot(snapshot)
        }),
        (any::<u64>(), prop::collection::vec(entry, 0..6)).prop_map(|(macro_revision, entries)| {
            Message::MacroSnapshot(MacroSnapshot {
                macro_revision,
                entries,
            })
        }),
        prop::sample::select(vec![
            Command::ToggleMode,
            Command::HardReset,
            Command::SetMode(true),
            Command::SetMode(false),
        ])
        .prop_map(|command| Message::Command { command }),
        any::<u32>().prop_map(|code| Message::CommandResult {
            ok: code == 0,
            code: ErrorCode(code),
        }),
        (any::<bool>(), any::<u64>()).prop_map(|(vietnamese_mode, revision)| {
            Message::ModeChanged {
                vietnamese_mode,
                revision,
            }
        }),
        (any::<u64>(), any::<u32>(), any::<Option<u64>>()).prop_map(
            |(timestamp_ms, code, revision)| {
                Message::Diagnostic(DiagEvent {
                    timestamp_ms,
                    component: Component::Ipc,
                    code: DiagCode(code),
                    revision,
                    host_exe: None,
                    vector_id: None,
                })
            }
        ),
        any::<u32>().prop_map(|code| Message::Error {
            code: ErrorCode(code),
        }),
        Just(Message::ResetAll),
        Just(Message::Ping),
        Just(Message::Pong),
        Just(Message::Goodbye),
    ]
}

/// Hotkeys with arbitrary field values, many of which the text parser would reject.
fn any_hotkey() -> impl Strategy<Value = Hotkey> {
    let key = prop_oneof![
        Just(HotkeyKey::ModifiersOnly),
        (0_u32..=0x1FF).prop_map(HotkeyKey::VirtualKey),
        any::<u32>().prop_map(HotkeyKey::VirtualKey),
    ];
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        key,
    )
        .prop_map(|(ctrl, shift, alt, win, key)| Hotkey {
            ctrl,
            shift,
            alt,
            win,
            key,
        })
}

/// Executable names that start with a run of `İ` (U+0130), which lowercases to two
/// characters, so that some of them outgrow `ExeName::MAX_LEN` once stored.
fn any_exe_name() -> impl Strategy<Value = ExeName> {
    let dotted = prop_oneof![0_usize..=4, 120_usize..=136, 0_usize..=255];
    (dotted, "[a-z0-9._-]{0,8}").prop_filter_map("not a valid name", |(dotted, rest)| {
        ExeName::new(&format!("{}{rest}", "\u{130}".repeat(dotted))).ok()
    })
}

/// Runtime snapshots whose hotkeys and executable names take arbitrary values.
fn runtime_snapshot() -> impl Strategy<Value = RuntimeSnapshot> {
    let hotkeys = || prop::collection::vec(any_hotkey(), 0..3);
    let names = || prop::collection::vec(any_exe_name(), 0..3);
    (hotkeys(), hotkeys(), names(), names(), names()).prop_map(
        |(toggle_mode, hard_reset, rules, members, probes)| {
            let mut snapshot = RuntimeSnapshot::defaults_v1();
            snapshot.hotkeys = HotkeyConfig {
                toggle_mode,
                hard_reset,
            };
            let compatibility = &mut snapshot.compatibility;
            compatibility.rules = rules
                .into_iter()
                .map(|exe| AppRule {
                    exe,
                    window_class: None,
                    action: RuleAction::default(),
                })
                .collect();
            compatibility.named_sets[0].members.extend(members);
            compatibility.probes = probes
                .into_iter()
                .map(|exe| ProbeRecord {
                    exe,
                    probe_version: 1,
                    tsf_range_edit: true,
                })
                .collect();
            snapshot
        },
    )
}

/// Builds a frame by hand with a valid checksum around an arbitrary payload.
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

/// Feeds `bytes` in chunks whose sizes cycle through `sizes` and collects every result up to
/// and including the first error.
fn decode_chunked(
    sender: Sender,
    bytes: &[u8],
    sizes: &[usize],
) -> (Vec<Result<Frame, IpcError>>, bool) {
    let mut decoder = FrameDecoder::for_sender(sender);
    let mut out = Vec::new();
    let mut rest = bytes;
    let mut index = 0;
    while !rest.is_empty() {
        let size = sizes
            .get(index % sizes.len().max(1))
            .copied()
            .unwrap_or(1)
            .max(1);
        index += 1;
        let (chunk, tail) = rest.split_at(size.min(rest.len()));
        rest = tail;
        decoder.feed(chunk);
        while let Some(item) = decoder.next_frame() {
            let failed = item.is_err();
            out.push(item);
            if failed {
                return (out, false);
            }
        }
    }
    (out, decoder.is_idle())
}

/// 512 cases per property; setting `PROPTEST_CASES` runs a longer campaign instead.
fn config() -> ProptestConfig {
    let mut config = ProptestConfig::default();
    if std::env::var_os("PROPTEST_CASES").is_none() {
        config.cases = 512;
    }
    config
}

proptest! {
    #![proptest_config(config())]

    #[test]
    fn decoder_never_panics_and_ignores_chunking_on_arbitrary_bytes(
        bytes in prop::collection::vec(any::<u8>(), 0..2048),
        sizes in prop::collection::vec(1_usize..97, 1..16),
        sender in prop::sample::select(vec![Sender::Any, Sender::Client, Sender::Server]),
    ) {
        let one_shot = decode_chunked(sender, &bytes, &[usize::MAX]);
        let chunked = decode_chunked(sender, &bytes, &sizes);
        prop_assert_eq!(one_shot, chunked);
    }

    #[test]
    fn decoder_never_panics_on_corrupted_frames(
        messages in prop::collection::vec(message(), 1..4),
        flips in prop::collection::vec((any::<prop::sample::Index>(), 1_u8..=255), 0..4),
        sizes in prop::collection::vec(1_usize..64, 1..8),
    ) {
        let mut bytes = Vec::new();
        for (index, message) in messages.iter().enumerate() {
            let frame = Frame::new(1, index as u64, message.clone());
            bytes.extend(encode_frame(&frame).unwrap());
        }
        for (position, mask) in &flips {
            let at = position.index(bytes.len());
            bytes[at] ^= mask;
        }
        let (results, _) = decode_chunked(Sender::Any, &bytes, &sizes);
        if flips.is_empty() {
            prop_assert_eq!(results.len(), messages.len());
            for (result, message) in results.iter().zip(&messages) {
                prop_assert_eq!(&result.as_ref().unwrap().message, message);
            }
        } else {
            // A corrupted stream may still yield the frames before the damage, never more.
            prop_assert!(results.iter().filter(|r| r.is_ok()).count() <= messages.len());
        }
    }

    #[test]
    fn messages_round_trip_through_frames_in_any_chunking(
        message in message(),
        client_id in any::<u64>(),
        request_id in any::<u64>(),
        sizes in prop::collection::vec(1_usize..50, 1..8),
    ) {
        let frame = Frame::new(client_id, request_id, message);
        let bytes = encode_frame(&frame).unwrap();
        prop_assert!(bytes.len() > HEADER_LEN);
        let (results, idle) = decode_chunked(Sender::Any, &bytes, &sizes);
        prop_assert!(idle);
        prop_assert_eq!(results, vec![Ok(frame)]);
    }

    #[test]
    fn mutated_bodies_with_valid_checksums_never_panic(
        message in message(),
        kind in 1_u16..=13,
        flips in prop::collection::vec((any::<prop::sample::Index>(), any::<u8>()), 0..6),
        truncate in any::<Option<prop::sample::Index>>(),
    ) {
        let mut payload = serde_json::to_vec(&message).unwrap();
        for (position, value) in &flips {
            let at = position.index(payload.len());
            payload[at] = *value;
        }
        if let Some(cut) = truncate {
            payload.truncate(cut.index(payload.len()));
        }
        let bytes = raw_frame(kind, 3, 4, &payload);
        let mut decoder = FrameDecoder::new();
        decoder.feed(&bytes);
        match decoder.next_frame() {
            Some(Ok(frame)) => {
                prop_assert_eq!(frame.message.kind().as_u16(), kind);
                prop_assert_eq!(frame.message.validate(), Ok(()));
            }
            Some(Err(error)) => prop_assert!(
                matches!(error, IpcError::Malformed(_) | IpcError::LimitExceeded { .. }),
                "{error:?}"
            ),
            None => prop_assert!(false, "a complete frame must produce a result"),
        }
    }

    #[test]
    fn a_runtime_snapshot_is_publishable_exactly_when_clients_decode_it_back(
        snapshot in runtime_snapshot(),
    ) {
        let message = Message::RuntimeSnapshot(snapshot.clone());
        // What a client reads, serialized without the encoder's checks.
        let body = serde_json::to_vec(&message).unwrap();
        let mut decoder = FrameDecoder::for_sender(Sender::Server);
        decoder.feed(&raw_frame(3, 1, 2, &body));
        let decoded = decoder.next_frame().map(|result| result.map(|frame| frame.message));
        let round_trips = decoded.as_ref() == Some(&Ok(message.clone()));
        let publishable = snapshot.check_publishable();
        prop_assert_eq!(publishable.is_ok(), round_trips, "{:?} -> {:?}", publishable, decoded);
        if publishable.is_ok() {
            let frame = Frame::new(1, 2, message);
            let bytes = encode_frame(&frame).unwrap();
            let (results, idle) = decode_chunked(Sender::Server, &bytes, &[usize::MAX]);
            prop_assert!(idle);
            prop_assert_eq!(results, vec![Ok(frame)]);
        }
    }

    #[test]
    fn a_control_message_naming_an_executable_is_sent_exactly_when_the_peer_decodes_it_back(
        exe in any_exe_name(),
        diagnostic in any::<bool>(),
    ) {
        let (message, kind) = if diagnostic {
            let event = DiagEvent {
                timestamp_ms: 1,
                component: Component::Tsf,
                code: DiagCode(2),
                revision: None,
                host_exe: Some(exe),
                vector_id: None,
            };
            (Message::Diagnostic(event), 9)
        } else {
            let probe = ProbeRecord { exe, probe_version: 1, tsf_range_edit: false };
            (Message::Command { command: Command::ReportProbe(probe) }, 5)
        };
        // What the server reads, serialized without the encoder's checks.
        let body = serde_json::to_vec(&message).unwrap();
        let mut decoder = FrameDecoder::for_sender(Sender::Client);
        decoder.feed(&raw_frame(kind, 1, 2, &body));
        let decoded = decoder.next_frame().map(|result| result.map(|frame| frame.message));
        let round_trips = decoded.as_ref() == Some(&Ok(message.clone()));
        let encoded = encode_frame(&Frame::new(1, 2, message));
        prop_assert_eq!(encoded.is_ok(), round_trips, "{:?} -> {:?}", encoded.err(), decoded);
    }

    #[test]
    fn truncated_frames_never_complete(message in message(), cut in any::<prop::sample::Index>()) {
        let bytes = encode_frame(&Frame::new(1, 2, message)).unwrap();
        let cut = cut.index(bytes.len());
        let mut decoder = FrameDecoder::new();
        decoder.feed(&bytes[..cut]);
        prop_assert_eq!(decoder.next_frame(), None);
        prop_assert_eq!(decoder.is_idle(), cut == 0);
    }
}

#[cfg(feature = "evkey-compat")]
mod legacy {
    use proptest::prelude::*;
    use raumakey_ipc::legacy::fake::{FakeEpoch, FakeLegacy, FakeOp, FakeReply};
    use raumakey_ipc::legacy::{
        BLOCK_SIZE, LegacyBlock, LegacyClient, LegacyError, LegacyObject, LegacyOpcode,
        MAX_CONSECUTIVE_FAILURES, MAX_DELETE_UNITS, OFF_REQUEST_SEQ, OFF_RESET_GATE,
        OUTPUT_CAPACITY, ResetOutcome, decode_request, next_sequence, validate_response,
    };
    use raumakey_model::utf16;

    use super::config;

    /// How the (scripted) controller handles one exchange.
    #[derive(Clone, Copy, Debug)]
    enum Outcome {
        Answered,
        Silent,
        WrongSequence,
        MutexBusy,
        Malformed,
    }

    #[derive(Clone, Copy, Debug)]
    enum Step {
        Key(Outcome),
        Reset { gate: u32, outcome: Outcome },
        Disconnect,
        Connect { answered: bool },
    }

    fn outcome() -> impl Strategy<Value = Outcome> {
        prop_oneof![
            Just(Outcome::Answered),
            Just(Outcome::Silent),
            Just(Outcome::WrongSequence),
            Just(Outcome::MutexBusy),
            Just(Outcome::Malformed),
        ]
    }

    fn step() -> impl Strategy<Value = Step> {
        prop_oneof![
            4 => outcome().prop_map(Step::Key),
            3 => (0_u32..3, outcome()).prop_map(|(gate, outcome)| Step::Reset { gate, outcome }),
            1 => Just(Step::Disconnect),
            1 => any::<bool>().prop_map(|answered| Step::Connect { answered }),
        ]
    }

    /// Scripts the controller for the next exchange.
    fn arrange(fake: &FakeLegacy, outcome: Outcome) {
        match outcome {
            Outcome::Answered => fake.push_reply(FakeReply::empty()),
            Outcome::Silent => {}
            // Sequence zero is never requested, so it never matches.
            Outcome::WrongSequence => fake.push_reply(FakeReply::empty().with_sequence(0)),
            // Held past the 100 ms wait, released before the next attempt.
            Outcome::MutexBusy => fake.hold_mutex_elsewhere(Some(150)),
            Outcome::Malformed => {
                fake.push_reply(FakeReply::text(0, "x").with_output_len(3));
            }
        }
    }

    fn matches_outcome<T>(result: &Result<T, LegacyError>, outcome: Outcome) -> bool {
        matches!(
            (outcome, result),
            (Outcome::Answered, Ok(_))
                | (Outcome::Silent, Err(LegacyError::ResponseTimeout))
                | (
                    Outcome::WrongSequence,
                    Err(LegacyError::SequenceMismatch { .. })
                )
                | (Outcome::MutexBusy, Err(LegacyError::MutexTimeout))
                | (
                    Outcome::Malformed,
                    Err(LegacyError::OddOutputLength { len: 3 })
                )
        )
    }

    /// The bridge state the client must reach.
    #[derive(Debug)]
    struct Model {
        connected: bool,
        failures: u8,
    }

    impl Model {
        fn drop_bridge(&mut self) {
            self.connected = false;
            self.failures = 0;
        }

        fn exchange(&mut self, outcome: Outcome) {
            match outcome {
                Outcome::Answered => self.failures = 0,
                Outcome::Silent | Outcome::WrongSequence | Outcome::MutexBusy => {
                    self.failures += 1;
                    if self.failures >= MAX_CONSECUTIVE_FAILURES {
                        self.drop_bridge();
                    }
                }
                Outcome::Malformed => self.drop_bridge(),
            }
        }
    }

    proptest! {
        #![proptest_config(config())]

        #[test]
        fn the_bridge_drops_exactly_after_consecutive_failed_exchanges_and_holds_no_handle_while_down(
            steps in prop::collection::vec(step(), 1..40),
        ) {
            let fake = FakeLegacy::with_controller_objects();
            let mut client = LegacyClient::new(fake.clone());
            let mut model = Model { connected: false, failures: 0 };
            for step in steps {
                fake.take_journal();
                let was_connected = model.connected;
                match step {
                    Step::Key(outcome) => {
                        if model.connected {
                            arrange(&fake, outcome);
                        }
                        let result = client.process_char(0x61, false, false);
                        if model.connected {
                            prop_assert!(matches_outcome(&result, outcome), "{result:?}");
                            model.exchange(outcome);
                        } else {
                            prop_assert_eq!(result, Err(LegacyError::NotConnected));
                        }
                    }
                    Step::Reset { gate, outcome } => {
                        fake.set_u32(OFF_RESET_GATE, gate);
                        let remote = model.connected && gate == 1;
                        if remote {
                            arrange(&fake, outcome);
                        }
                        let result = client.reset();
                        if remote {
                            prop_assert!(matches_outcome(&result, outcome), "{result:?}");
                            if result.is_ok() {
                                prop_assert_eq!(result, Ok(ResetOutcome::Remote));
                            }
                            model.exchange(outcome);
                        } else {
                            // Local resets are not exchanges: the count is untouched.
                            prop_assert_eq!(result, Ok(ResetOutcome::LocalOnly));
                        }
                    }
                    Step::Disconnect => {
                        client.disconnect();
                        model.drop_bridge();
                    }
                    Step::Connect { answered } => {
                        if answered {
                            fake.push_reply(FakeReply::empty());
                        }
                        prop_assert_eq!(client.connect().is_ok(), answered);
                        model.connected = answered;
                        model.failures = 0;
                    }
                }
                prop_assert_eq!(client.is_connected(), model.connected, "{:?}", model);
                let holds = LegacyObject::ALL.iter().any(|object| fake.holds(*object));
                prop_assert_eq!(holds, model.connected);
                if was_connected && !model.connected {
                    let journal = fake.journal();
                    prop_assert_eq!(journal.last(), Some(&FakeOp::Close));
                }
            }
        }

        #[test]
        fn response_validation_never_panics_and_requires_the_sequence(
            bytes in prop::collection::vec(any::<u8>(), BLOCK_SIZE),
            sequence in any::<u32>(),
            now in any::<u64>(),
            max_delete in any::<u32>(),
        ) {
            let block = LegacyBlock::from_bytes(&bytes).unwrap();
            match validate_response(&block, sequence, now, max_delete) {
                Ok(edit) => {
                    prop_assert_eq!(block.response_sequence(), sequence);
                    prop_assert!(edit.delete_before <= max_delete.min(MAX_DELETE_UNITS));
                    prop_assert!(edit.insert_utf16.len() * 2 <= OUTPUT_CAPACITY);
                    prop_assert!(utf16::is_well_formed(&edit.insert_utf16));
                }
                Err(LegacyError::SequenceMismatch { expected, actual }) => {
                    prop_assert_eq!(expected, sequence);
                    prop_assert_ne!(actual, sequence);
                }
                Err(_) => prop_assert_eq!(block.response_sequence(), sequence),
            }
        }

        #[test]
        fn request_decoding_never_panics(bytes in prop::collection::vec(any::<u8>(), BLOCK_SIZE)) {
            let block = LegacyBlock::from_bytes(&bytes).unwrap();
            if let Ok((sequence, request)) = decode_request(&block) {
                prop_assert_eq!(sequence, block.request_sequence());
                prop_assert_eq!(LegacyOpcode::from_u32(block.opcode()), Some(request.opcode()));
            }
        }

        #[test]
        fn sequence_wrap_is_handled(current in any::<u32>(), other in any::<u32>()) {
            let next = next_sequence(current);
            prop_assert_ne!(next, 0);
            if current == u32::MAX {
                prop_assert_eq!(next, 1);
            } else {
                prop_assert_eq!(next, current + 1);
            }
            let mut block = LegacyBlock::zeroed();
            block.set_response_sequence(other);
            block.set_epoch(10);
            let result = validate_response(&block, next, 10, MAX_DELETE_UNITS);
            prop_assert_eq!(result.is_ok(), other == next);
        }

        #[test]
        fn the_client_never_panics_or_reads_out_of_bounds_on_arbitrary_replies(
            start in any::<u32>(),
            delete in any::<u32>(),
            output in prop::collection::vec(any::<u16>(), 0..12),
            output_len in any::<Option<u32>>(),
            sequence in any::<Option<u32>>(),
            age in 0_u64..4_000,
            delay in 0_u64..120,
        ) {
            let fake = FakeLegacy::with_controller_objects();
            let mut client = LegacyClient::new(fake.clone());
            fake.push_reply(FakeReply::empty());
            client.connect().unwrap();
            fake.set_u32(OFF_REQUEST_SEQ, start);
            let mut reply = FakeReply::units(delete, output.clone())
                .after_ms(delay)
                .with_epoch(FakeEpoch::AgeMs(age));
            reply.output_len = output_len;
            reply.sequence = sequence;
            fake.push_reply(reply);
            let result = client.process_char(0x61, false, false);
            let expected = next_sequence(start);
            prop_assert_eq!(fake.u32_at(OFF_REQUEST_SEQ), expected);
            if let Ok(edit) = &result {
                prop_assert!(sequence.is_none() || sequence == Some(expected));
                prop_assert!(delay <= 50 && age <= 2_000);
                prop_assert_eq!(edit.delete_before, delete);
                prop_assert!(utf16::is_well_formed(&edit.insert_utf16));
            }
            let journal = fake.journal();
            for op in &journal {
                if let FakeOp::Read { offset, len } = op {
                    prop_assert!(offset + len <= BLOCK_SIZE);
                }
            }
            // The mutex is always released; a dropped bridge also closes every handle.
            let tail: &[FakeOp] = if client.is_connected() {
                &[FakeOp::Unlock]
            } else {
                &[FakeOp::Unlock, FakeOp::Close]
            };
            prop_assert!(journal.ends_with(tail), "{:?}", journal.last());
            let open = LegacyObject::ALL.iter().any(|object| fake.holds(*object));
            prop_assert_eq!(open, client.is_connected());
        }
    }
}
