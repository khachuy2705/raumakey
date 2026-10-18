use raumakey_ipc::error::{ErrorCode, IpcError, Limit, MalformedReason};
use raumakey_ipc::frame::MAX_PAYLOAD;
use raumakey_ipc::message::{
    Arch, ClientKind, Command, MAX_APP_RULES, MAX_HOTKEYS, MAX_KEYMAPS, MAX_MACROS,
    MAX_NAMED_SET_MEMBERS, MAX_NAMED_SETS, MAX_PROBES, MacroSnapshot, Message, MessageKind,
    RuntimeSnapshot,
};
use raumakey_model::diag::{Component, DiagCode, DiagEvent};
use raumakey_model::host::ExeName;
use raumakey_model::hotkey::{Hotkey, HotkeyConfig, HotkeyKey};
use raumakey_model::input::vk;
use raumakey_model::keymap::{KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, Tone};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_model::policy::{AppRule, CompatibilityConfig, NamedSet, ProbeRecord, RuleAction};
use serde_json::json;

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn macro_entry(id: u64, trigger: &str, replacement: &str) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    }
}

fn keymap(id: &str) -> KeyMap {
    KeyMap {
        id: id.to_owned(),
        version: KEYMAP_SCHEMA_VERSION,
        name: "Test map".to_owned(),
        bindings: vec![KeyBinding {
            key: 's',
            actions: vec![KeyAction::Tone(Tone::Acute)],
        }],
    }
}

fn probe(name: &str) -> ProbeRecord {
    ProbeRecord {
        exe: exe(name),
        probe_version: 1,
        tsf_range_edit: false,
    }
}

fn diag() -> DiagEvent {
    DiagEvent {
        timestamp_ms: 12,
        component: Component::Ipc,
        code: DiagCode(7),
        revision: Some(3),
        host_exe: Some(exe("notepad.exe")),
        vector_id: None,
    }
}

fn sample_messages() -> Vec<(Message, MessageKind)> {
    vec![
        (
            Message::Hello {
                client_kind: ClientKind::Tip,
                pid: 42,
                arch: Arch::X64,
                protocol: 1,
            },
            MessageKind::Hello,
        ),
        (
            Message::Welcome {
                client_id: 7,
                server_revision: 3,
            },
            MessageKind::Welcome,
        ),
        (
            Message::RuntimeSnapshot(RuntimeSnapshot::defaults_v1()),
            MessageKind::RuntimeSnapshot,
        ),
        (
            Message::MacroSnapshot(MacroSnapshot {
                macro_revision: 2,
                entries: vec![macro_entry(1, "ko", "không")],
            }),
            MessageKind::MacroSnapshot,
        ),
        (
            Message::Command {
                command: Command::ToggleMode,
            },
            MessageKind::Command,
        ),
        (
            Message::CommandResult {
                ok: true,
                code: ErrorCode::OK,
            },
            MessageKind::CommandResult,
        ),
        (
            Message::ModeChanged {
                vietnamese_mode: false,
                revision: 9,
            },
            MessageKind::ModeChanged,
        ),
        (Message::ResetAll, MessageKind::ResetAll),
        (Message::Diagnostic(diag()), MessageKind::Diagnostic),
        (Message::Ping, MessageKind::Ping),
        (Message::Pong, MessageKind::Pong),
        (
            Message::Error {
                code: ErrorCode::UNEXPECTED_MESSAGE,
            },
            MessageKind::Error,
        ),
        (Message::Goodbye, MessageKind::Goodbye),
    ]
}

#[test]
fn message_kinds_have_stable_wire_numbers() {
    let expected = [
        (MessageKind::Hello, 1),
        (MessageKind::Welcome, 2),
        (MessageKind::RuntimeSnapshot, 3),
        (MessageKind::MacroSnapshot, 4),
        (MessageKind::Command, 5),
        (MessageKind::CommandResult, 6),
        (MessageKind::ModeChanged, 7),
        (MessageKind::ResetAll, 8),
        (MessageKind::Diagnostic, 9),
        (MessageKind::Ping, 10),
        (MessageKind::Pong, 11),
        (MessageKind::Error, 12),
        (MessageKind::Goodbye, 13),
    ];
    assert_eq!(MessageKind::ALL.len(), expected.len());
    for (kind, number) in expected {
        assert_eq!(kind.as_u16(), number);
        assert_eq!(MessageKind::from_u16(number), Some(kind));
        assert!(MessageKind::ALL.contains(&kind));
    }
    for raw in [0_u16, 14, 255, u16::MAX] {
        assert_eq!(MessageKind::from_u16(raw), None);
    }
}

#[test]
fn every_message_reports_its_kind() {
    let samples = sample_messages();
    assert_eq!(samples.len(), MessageKind::ALL.len());
    for (message, kind) in samples {
        assert_eq!(message.kind(), kind);
        assert_eq!(message.validate(), Ok(()));
    }
}

#[test]
fn json_body_is_an_internally_tagged_enum() {
    let value = |m: &Message| serde_json::to_value(m).unwrap();
    assert_eq!(
        serde_json::to_string(&Message::Ping).unwrap(),
        r#"{"type":"ping"}"#
    );
    assert_eq!(value(&Message::Pong), json!({"type": "pong"}));
    assert_eq!(value(&Message::ResetAll), json!({"type": "reset_all"}));
    assert_eq!(value(&Message::Goodbye), json!({"type": "goodbye"}));
    assert_eq!(
        value(&Message::Hello {
            client_kind: ClientKind::Tip,
            pid: 42,
            arch: Arch::X64,
            protocol: 1
        }),
        json!({"type": "hello", "client_kind": "tip", "pid": 42, "arch": "x64", "protocol": 1})
    );
    assert_eq!(
        value(&Message::Welcome {
            client_id: 7,
            server_revision: 3
        }),
        json!({"type": "welcome", "client_id": 7, "server_revision": 3})
    );
    assert_eq!(
        value(&Message::Command {
            command: Command::ToggleMode
        }),
        json!({"type": "command", "command": "toggle_mode"})
    );
    assert_eq!(
        value(&Message::Command {
            command: Command::SetMode(true)
        }),
        json!({"type": "command", "command": {"set_mode": true}})
    );
    assert_eq!(
        value(&Message::Command {
            command: Command::HardReset
        }),
        json!({"type": "command", "command": "hard_reset"})
    );
    assert_eq!(
        value(&Message::Command {
            command: Command::ReportProbe(probe("legacy.exe"))
        }),
        json!({"type": "command", "command": {"report_probe":
            {"exe": "legacy.exe", "probe_version": 1, "tsf_range_edit": false}}})
    );
    assert_eq!(
        value(&Message::CommandResult {
            ok: false,
            code: ErrorCode::COMMAND_FAILED
        }),
        json!({"type": "command_result", "ok": false, "code": ErrorCode::COMMAND_FAILED.0})
    );
    assert_eq!(
        value(&Message::ModeChanged {
            vietnamese_mode: false,
            revision: 9
        }),
        json!({"type": "mode_changed", "vietnamese_mode": false, "revision": 9})
    );
    assert_eq!(
        value(&Message::Error { code: ErrorCode(3) }),
        json!({"type": "error", "code": 3})
    );
    assert_eq!(
        value(&Message::Diagnostic(diag())),
        json!({"type": "diagnostic", "timestamp_ms": 12, "component": "ipc", "code": 7,
               "revision": 3, "host_exe": "notepad.exe", "vector_id": null})
    );
    let runtime = value(&Message::RuntimeSnapshot(RuntimeSnapshot::defaults_v1()));
    assert_eq!(runtime["type"], "runtime_snapshot");
    assert_eq!(runtime["config_revision"], 0);
    assert_eq!(runtime["vietnamese_mode"], true);
    let macros = value(&Message::MacroSnapshot(MacroSnapshot::empty(5)));
    assert_eq!(
        macros,
        json!({"type": "macro_snapshot", "macro_revision": 5, "entries": []})
    );
}

#[test]
fn every_message_round_trips_through_json() {
    for (message, _) in sample_messages() {
        let text = serde_json::to_vec(&message).unwrap();
        let back: Message = serde_json::from_slice(&text).unwrap();
        assert_eq!(back, message);
    }
}

#[test]
fn message_direction_is_fixed_per_kind() {
    use MessageKind as K;
    let from_client = [
        K::Hello,
        K::Command,
        K::Diagnostic,
        K::Ping,
        K::Pong,
        K::Error,
        K::Goodbye,
    ];
    let from_server = [
        K::Welcome,
        K::RuntimeSnapshot,
        K::MacroSnapshot,
        K::CommandResult,
        K::ModeChanged,
        K::ResetAll,
        K::Ping,
        K::Pong,
        K::Error,
        K::Goodbye,
    ];
    for kind in MessageKind::ALL {
        assert_eq!(
            kind.sent_by_client(),
            from_client.contains(&kind),
            "{kind:?}"
        );
        assert_eq!(
            kind.sent_by_server(),
            from_server.contains(&kind),
            "{kind:?}"
        );
    }
}

#[test]
fn runtime_snapshot_defaults_v1_are_valid() {
    let snapshot = RuntimeSnapshot::defaults_v1();
    assert_eq!(snapshot.config_revision, 0);
    assert_eq!(snapshot.options, EngineOptions::defaults_v1());
    assert!(snapshot.keymaps.is_empty());
    assert_eq!(snapshot.hotkeys, HotkeyConfig::default());
    assert_eq!(snapshot.compatibility, CompatibilityConfig::defaults_v1());
    assert!(snapshot.vietnamese_mode);
    assert!(!snapshot.diagnostics_enabled);
    assert_eq!(snapshot.validate(), Ok(()));
}

#[test]
fn runtime_snapshot_without_diagnostics_field_defaults_to_disabled() {
    let mut value =
        serde_json::to_value(Message::RuntimeSnapshot(RuntimeSnapshot::defaults_v1())).unwrap();
    value.as_object_mut().unwrap().remove("diagnostics_enabled");
    let back: Message = serde_json::from_value(value).unwrap();
    let Message::RuntimeSnapshot(snapshot) = back else {
        panic!("expected a runtime snapshot");
    };
    assert!(!snapshot.diagnostics_enabled);
}

#[test]
fn entry_count_limits_have_the_planned_values() {
    assert_eq!(MAX_MACROS, 20_000);
    assert_eq!(MAX_KEYMAPS, 64);
    assert_eq!(Limit::Macros.max(), MAX_MACROS);
    assert_eq!(Limit::Keymaps.max(), MAX_KEYMAPS);
    assert_eq!(Limit::Hotkeys.max(), MAX_HOTKEYS);
    assert_eq!(Limit::AppRules.max(), MAX_APP_RULES);
    assert_eq!(Limit::NamedSets.max(), MAX_NAMED_SETS);
    assert_eq!(Limit::NamedSetMembers.max(), MAX_NAMED_SET_MEMBERS);
    assert_eq!(Limit::Probes.max(), MAX_PROBES);
}

#[test]
fn macro_snapshot_limit_is_enforced() {
    let at_limit = MacroSnapshot {
        macro_revision: 1,
        entries: (0..MAX_MACROS as u64)
            .map(|i| macro_entry(i, "a", "b"))
            .collect(),
    };
    assert_eq!(at_limit.validate(), Ok(()));
    let mut over = at_limit;
    over.entries.push(macro_entry(99_999, "a", "b"));
    assert_eq!(
        over.validate(),
        Err(IpcError::LimitExceeded {
            limit: Limit::Macros,
            count: MAX_MACROS as u64 + 1
        })
    );
}

#[test]
fn snapshots_report_their_body_length_and_whether_one_frame_can_carry_them() {
    let snapshot = MacroSnapshot {
        macro_revision: 2,
        entries: vec![
            macro_entry(1, "ko", "không"),
            macro_entry(2, "q\"", "a\\b\u{1}"),
        ],
    };
    let body = serde_json::to_vec(&Message::MacroSnapshot(snapshot.clone())).unwrap();
    assert_eq!(snapshot.encoded_len(), Ok(body.len() as u64));
    assert_eq!(snapshot.check_publishable(), Ok(()));
    let over = MacroSnapshot {
        macro_revision: 1,
        entries: (0..=MAX_MACROS as u64)
            .map(|i| macro_entry(i, "a", "b"))
            .collect(),
    };
    assert_eq!(
        over.check_publishable(),
        Err(IpcError::LimitExceeded {
            limit: Limit::Macros,
            count: MAX_MACROS as u64 + 1
        })
    );

    let runtime = RuntimeSnapshot::defaults_v1();
    let body = serde_json::to_vec(&Message::RuntimeSnapshot(runtime.clone())).unwrap();
    assert_eq!(runtime.encoded_len(), Ok(body.len() as u64));
    assert_eq!(runtime.check_publishable(), Ok(()));
    let mut invalid = runtime.clone();
    invalid.options.context_limit_codepoints = 0;
    assert_eq!(
        invalid.check_publishable(),
        Err(IpcError::Malformed(MalformedReason::InvalidOptions))
    );
    // Within every entry limit, but over the frame limit.
    let mut huge = runtime;
    huge.compatibility.rules = vec![
        AppRule {
            exe: exe("notepad.exe"),
            window_class: Some("w".repeat(2_100)),
            action: RuleAction::default(),
        };
        MAX_APP_RULES
    ];
    let len = huge.encoded_len().unwrap();
    assert!(len > u64::from(MAX_PAYLOAD), "{len}");
    assert_eq!(
        huge.check_publishable(),
        Err(IpcError::Oversize {
            payload_len: len,
            max: MAX_PAYLOAD
        })
    );
}

#[test]
fn runtime_snapshot_limits_are_enforced() {
    let base = RuntimeSnapshot::defaults_v1();
    let exceeded = |limit: Limit, count: usize| {
        Err(IpcError::LimitExceeded {
            limit,
            count: count as u64,
        })
    };

    let mut s = base.clone();
    s.keymaps = (0..MAX_KEYMAPS)
        .map(|i| keymap(&format!("map-{i}")))
        .collect();
    assert_eq!(s.validate(), Ok(()));
    s.keymaps.push(keymap("one-more"));
    assert_eq!(s.validate(), exceeded(Limit::Keymaps, MAX_KEYMAPS + 1));

    let mut s = base.clone();
    let toggle = s.hotkeys.toggle_mode[0];
    s.hotkeys.toggle_mode = vec![toggle; MAX_HOTKEYS + 1];
    assert_eq!(s.validate(), exceeded(Limit::Hotkeys, MAX_HOTKEYS + 1));

    let mut s = base.clone();
    let reset = s.hotkeys.hard_reset[0];
    s.hotkeys.hard_reset = vec![reset; MAX_HOTKEYS + 1];
    assert_eq!(s.validate(), exceeded(Limit::Hotkeys, MAX_HOTKEYS + 1));

    let mut s = base.clone();
    let rule = AppRule {
        exe: exe("notepad.exe"),
        window_class: None,
        action: RuleAction::default(),
    };
    s.compatibility.rules = vec![rule; MAX_APP_RULES + 1];
    assert_eq!(s.validate(), exceeded(Limit::AppRules, MAX_APP_RULES + 1));

    let mut s = base.clone();
    let set = NamedSet {
        name: "set".to_owned(),
        members: Vec::new(),
        action: RuleAction::default(),
    };
    s.compatibility.named_sets = vec![set; MAX_NAMED_SETS + 1];
    assert_eq!(s.validate(), exceeded(Limit::NamedSets, MAX_NAMED_SETS + 1));

    let mut s = base.clone();
    s.compatibility.named_sets[0].members = vec![exe("a.exe"); MAX_NAMED_SET_MEMBERS + 1];
    assert_eq!(
        s.validate(),
        exceeded(Limit::NamedSetMembers, MAX_NAMED_SET_MEMBERS + 1)
    );

    let mut s = base;
    s.compatibility.probes = vec![probe("a.exe"); MAX_PROBES + 1];
    assert_eq!(s.validate(), exceeded(Limit::Probes, MAX_PROBES + 1));
}

#[test]
fn invalid_options_and_keymaps_are_malformed() {
    let mut s = RuntimeSnapshot::defaults_v1();
    s.options.context_limit_codepoints = 4;
    assert_eq!(
        s.validate(),
        Err(IpcError::Malformed(MalformedReason::InvalidOptions))
    );

    let mut s = RuntimeSnapshot::defaults_v1();
    s.keymaps.push(keymap("bad id with spaces"));
    assert_eq!(
        s.validate(),
        Err(IpcError::Malformed(MalformedReason::InvalidKeymap))
    );
}

#[test]
fn a_user_defined_method_needs_its_key_map_in_the_snapshot() {
    let mut s = RuntimeSnapshot::defaults_v1();
    s.options.input_method = InputMethod::UserDefined;
    s.options.user_keymap_id = Some("mine".to_owned());
    let missing = Err(IpcError::Malformed(MalformedReason::InvalidKeymap));
    assert_eq!(s.validate(), missing, "no key maps at all");

    s.keymaps.push(keymap("other"));
    assert_eq!(s.validate(), missing, "only another key map");

    s.keymaps.push(keymap("mine"));
    assert_eq!(s.validate(), Ok(()));
    assert!(decodes_back(&Message::RuntimeSnapshot(s.clone())));

    s.options.input_method = InputMethod::Telex;
    s.keymaps.clear();
    assert_eq!(
        s.validate(),
        Ok(()),
        "built-in methods need no carried key map"
    );
}

fn hotkey(ctrl: bool, shift: bool, key: HotkeyKey) -> Hotkey {
    Hotkey {
        ctrl,
        shift,
        alt: false,
        win: false,
        key,
    }
}

/// True when the peer can read `message` back from the JSON body the encoder would write.
fn decodes_back(message: &Message) -> bool {
    let body = serde_json::to_vec(message).unwrap();
    matches!(serde_json::from_slice::<Message>(&body), Ok(back) if back == *message)
}

#[test]
fn hotkeys_that_do_not_parse_back_from_their_text_form_are_refused() {
    // Hotkeys travel as text and the peer parses them strictly.
    let unparseable = [
        // Written as "Ctrl" (a modifier-only hotkey needs two modifiers).
        hotkey(true, false, HotkeyKey::ModifiersOnly),
        // Written as "".
        hotkey(false, false, HotkeyKey::ModifiersOnly),
        // A modifier, zero, or a code above 0xFE as the key: written as "0xA0", "0x00", "0x100".
        hotkey(true, false, HotkeyKey::VirtualKey(vk::LSHIFT)),
        hotkey(true, false, HotkeyKey::VirtualKey(0)),
        hotkey(true, false, HotkeyKey::VirtualKey(0x100)),
        // A key without a modifier, written as "A".
        hotkey(false, false, HotkeyKey::VirtualKey(u32::from(b'A'))),
    ];
    for bad in unparseable {
        for hard_reset in [false, true] {
            let mut s = RuntimeSnapshot::defaults_v1();
            if hard_reset {
                s.hotkeys.hard_reset.push(bad);
            } else {
                s.hotkeys.toggle_mode.insert(0, bad);
            }
            let message = Message::RuntimeSnapshot(s.clone());
            assert!(!decodes_back(&message), "{bad:?} decodes back");
            let refused = Err(IpcError::Malformed(MalformedReason::InvalidHotkey));
            assert_eq!(s.validate(), refused, "{bad:?}");
            assert_eq!(s.check_publishable(), refused, "{bad:?}");
            assert_eq!(message.validate(), refused, "{bad:?}");
        }
    }

    // Every hotkey the parser accepts stays publishable.
    let parseable = [
        hotkey(true, true, HotkeyKey::ModifiersOnly),
        hotkey(false, true, HotkeyKey::VirtualKey(u32::from(b'Z'))),
        hotkey(true, false, HotkeyKey::VirtualKey(0x61)),
        hotkey(true, false, HotkeyKey::VirtualKey(0xFE)),
        hotkey(false, true, HotkeyKey::VirtualKey(vk::F24)),
        hotkey(true, false, HotkeyKey::VirtualKey(vk::RETURN)),
    ];
    let mut s = RuntimeSnapshot::defaults_v1();
    s.hotkeys.toggle_mode.extend(parseable);
    s.hotkeys.hard_reset.extend(parseable);
    assert_eq!(s.check_publishable(), Ok(()));
    assert!(decodes_back(&Message::RuntimeSnapshot(s)));
}

/// `İ` (U+0130) lowercases to two characters. `ExeName::new` refuses a name that lowercasing
/// makes longer than `ExeName::MAX_LEN`, so every name a message can carry reads back on the
/// peer, and the longest one (127 × `İ` and one letter: 255 characters) is accepted
/// everywhere.
#[test]
fn every_exe_name_reads_back_and_the_longest_is_accepted_everywhere() {
    assert!(ExeName::new(&"\u{130}".repeat(200)).is_err());
    assert!(ExeName::new(&"\u{130}".repeat(128)).is_err());
    let longest = ExeName::new(&format!("{}x", "\u{130}".repeat(127))).unwrap();
    assert_eq!(longest.as_str().chars().count(), ExeName::MAX_LEN);
    let mut s = RuntimeSnapshot::defaults_v1();
    s.compatibility.rules.push(AppRule {
        exe: longest.clone(),
        window_class: None,
        action: RuleAction::default(),
    });
    s.compatibility.probes.push(probe(longest.as_str()));
    s.compatibility.named_sets[2].members.push(longest.clone());
    assert_eq!(s.check_publishable(), Ok(()));
    assert!(decodes_back(&Message::RuntimeSnapshot(s)));
    let messages = [
        Message::Command {
            command: Command::ReportProbe(probe(longest.as_str())),
        },
        Message::Diagnostic(DiagEvent {
            host_exe: Some(longest),
            ..diag()
        }),
    ];
    for message in messages {
        assert_eq!(message.validate(), Ok(()), "{:?}", message.kind());
        assert!(decodes_back(&message), "{:?}", message.kind());
    }
}

/// A runtime snapshot body whose `compatibility` value is the JSON text `compatibility`
/// (spliced as text, so that it may repeat keys).
fn runtime_body_with_compatibility(compatibility: &str) -> String {
    let mut value =
        serde_json::to_value(Message::RuntimeSnapshot(RuntimeSnapshot::defaults_v1())).unwrap();
    value.as_object_mut().unwrap().remove("compatibility");
    let mut body = serde_json::to_string(&value).unwrap();
    assert_eq!(body.pop(), Some('}'));
    body.push_str(r#","compatibility":"#);
    body.push_str(compatibility);
    body.push('}');
    body
}

#[test]
fn the_compatibility_configuration_decodes_exactly_like_the_model_types() {
    // Runtime snapshots read their compatibility configuration through a wire form of their
    // own (the model's `AppRule` flattens its overrides, which serde would buffer). It must
    // accept, produce, and refuse exactly what the model's `Deserialize` does.
    let rules = r#"[
        {"exe": "a.exe"},
        {"exe": "B.EXE", "window_class": "W", "transport": "synthetic-key",
         "reset_on_mouse": false, "macros": true, "vietnamese": false, "max_delete_before": 7},
        {"exe": "c.exe", "window_class": null, "transport": null,
         "future": {"nested": [1, "x"]}}
    ]"#;
    let accepted = [
        "{}".to_owned(),
        r#"{"rule": []}"#.to_owned(),
        [r#"{"rule": "#, rules, "}"].concat(),
        [
            r#"{"clipboard_enabled": true, "default_reset_on_mouse": false,
                "secure_field_mode": "restricted", "default_max_delete_before": 9,
                "rule": "#,
            rules,
            r#", "future": [1, 2, 3],
                "named_set": [{"name": "s", "members": ["x.exe"], "action": {"macros": false}}],
                "probe": [{"exe": "p.exe", "probe_version": 1, "tsf_range_edit": true}]}"#,
        ]
        .concat(),
    ];
    for compatibility in &accepted {
        let expected: CompatibilityConfig = serde_json::from_str(compatibility).unwrap();
        let body = runtime_body_with_compatibility(compatibility);
        let snapshot: RuntimeSnapshot = serde_json::from_str(&body).unwrap();
        assert_eq!(snapshot.compatibility, expected, "{compatibility}");
        let Message::RuntimeSnapshot(snapshot) = serde_json::from_str(&body).unwrap() else {
            panic!("expected a runtime snapshot");
        };
        assert_eq!(snapshot.compatibility, expected, "{compatibility}");
    }

    let refused = [
        r#"{"rule": [{"window_class": "W"}]}"#,
        r#"{"rule": [{"exe": "a/b.exe"}]}"#,
        r#"{"rule": [{"exe": "a.exe", "transport": "warp"}]}"#,
        r#"{"rule": [{"exe": "a.exe", "max_delete_before": -1}]}"#,
        r#"{"rule": [{"exe": "a.exe", "reset_on_mouse": 1}]}"#,
        r#"{"rule": [{"exe": "a.exe", "exe": "b.exe"}]}"#,
        r#"{"rule": [{"exe": "a.exe", "macros": true, "macros": false}]}"#,
        r#"{"rule": {"exe": "a.exe"}}"#,
        r#"{"rule": null}"#,
        r#"{"clipboard_enabled": 1}"#,
        r#"{"default_max_delete_before": null}"#,
    ];
    for compatibility in refused {
        assert!(
            serde_json::from_str::<CompatibilityConfig>(compatibility).is_err(),
            "{compatibility}"
        );
        let body = runtime_body_with_compatibility(compatibility);
        assert!(
            serde_json::from_str::<RuntimeSnapshot>(&body).is_err(),
            "{compatibility}"
        );
        assert!(
            serde_json::from_str::<Message>(&body).is_err(),
            "{compatibility}"
        );
    }
}

#[test]
fn command_result_must_agree_with_its_code() {
    let result = |ok, code| Message::CommandResult { ok, code };
    assert_eq!(result(true, ErrorCode::OK).validate(), Ok(()));
    assert_eq!(result(false, ErrorCode::COMMAND_FAILED).validate(), Ok(()));
    assert_eq!(
        result(true, ErrorCode::COMMAND_FAILED).validate(),
        Err(IpcError::Malformed(MalformedReason::InconsistentResult))
    );
    assert_eq!(
        result(false, ErrorCode::OK).validate(),
        Err(IpcError::Malformed(MalformedReason::InconsistentResult))
    );
}

#[test]
fn welcome_requires_a_nonzero_client_id() {
    let welcome = Message::Welcome {
        client_id: 0,
        server_revision: 1,
    };
    assert_eq!(
        welcome.validate(),
        Err(IpcError::Malformed(MalformedReason::InvalidClientId))
    );
}

#[test]
fn macro_text_never_appears_in_debug_output() {
    let snapshot = MacroSnapshot {
        macro_revision: 4,
        entries: vec![macro_entry(1, "secrettrigger", "secretreplacement")],
    };
    let text = format!("{snapshot:?}");
    assert!(!text.contains("secret"), "{text}");
    assert!(text.contains("macro_revision: 4"), "{text}");
    let message = format!("{:?}", Message::MacroSnapshot(snapshot));
    assert!(!message.contains("secret"), "{message}");
}

#[test]
fn error_codes_are_transparent_numbers() {
    assert!(ErrorCode::OK.is_ok());
    assert!(!ErrorCode::COMMAND_FAILED.is_ok());
    assert_eq!(serde_json::to_string(&ErrorCode(5)).unwrap(), "5");
    let codes = [
        ErrorCode::OK,
        ErrorCode::UNSUPPORTED_PROTOCOL,
        ErrorCode::MALFORMED_FRAME,
        ErrorCode::UNEXPECTED_MESSAGE,
        ErrorCode::CLIENT_ID_MISMATCH,
        ErrorCode::LIMIT_EXCEEDED,
        ErrorCode::COMMAND_FAILED,
        ErrorCode::COMMAND_UNSUPPORTED,
        ErrorCode::SHUTTING_DOWN,
        ErrorCode::INTERNAL,
    ];
    for (index, code) in codes.iter().enumerate() {
        assert_eq!(code.0 as usize, index);
    }
}

#[test]
fn current_arch_matches_the_target() {
    let expected = if cfg!(target_arch = "x86") {
        Arch::X86
    } else if cfg!(target_arch = "x86_64") {
        Arch::X64
    } else if cfg!(target_arch = "aarch64") {
        Arch::Arm64
    } else {
        Arch::Other
    };
    assert_eq!(Arch::current(), expected);
}
