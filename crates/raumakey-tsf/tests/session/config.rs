//! Runtime configuration: defaults v1 until the first snapshot, runtime and macro snapshots
//! (an invalid one keeps the last good configuration), the live mode, reset-all, and the
//! host rules a snapshot carries.

use std::sync::Arc;

use raumakey_ipc::error::{IpcError, Limit, MalformedReason};
use raumakey_ipc::message::{MAX_MACROS, MacroSnapshot, RuntimeSnapshot};
use raumakey_macro::{EntryError, MacroError};
use raumakey_model::host::ExeName;
use raumakey_model::hotkey::HotkeyConfig;
use raumakey_model::input::vk;
use raumakey_model::keymap::{KEYMAP_SCHEMA_VERSION, KeyAction, KeyBinding, KeyMap, Tone};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_model::policy::{AppRule, CompatibilityConfig, RuleAction, SecureFieldMode};
use raumakey_tip::session::{ServiceCore, SnapshotError, build_macro_table};

use crate::support::{Behavior, FakeDocument, edit, host};

/// Defaults v1 with `config_revision` and the changes of `change`.
fn snapshot(revision: u64, change: impl FnOnce(&mut RuntimeSnapshot)) -> RuntimeSnapshot {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = revision;
    change(&mut snapshot);
    snapshot
}

/// A macro snapshot of `(id, trigger, replacement)` entries.
fn macros(revision: u64, entries: &[(u64, &str, &str)]) -> MacroSnapshot {
    MacroSnapshot {
        macro_revision: revision,
        entries: entries
            .iter()
            .map(|&(id, trigger, replacement)| MacroEntry {
                id: MacroId(id),
                trigger: trigger.to_owned(),
                replacement: replacement.to_owned(),
                enabled: true,
                boundary: BoundaryPolicy::Delimiter,
                case_policy: CasePolicy::Adapt,
            })
            .collect(),
    }
}

/// A rule for `notepad.exe`, the executable of every fake document.
fn notepad_rule(action: RuleAction) -> AppRule {
    AppRule {
        exe: ExeName::new("notepad.exe").expect("valid name"),
        window_class: None,
        action,
    }
}

#[test]
fn defaults_v1_apply_before_the_first_snapshot() {
    let core = ServiceCore::new();
    let config = core.config();
    assert_eq!(config.options(), &EngineOptions::defaults_v1());
    assert!(config.vietnamese_mode());
    assert!(core.vietnamese_mode());
    assert!(config.keymaps().is_empty());
    assert_eq!(config.hotkeys(), &HotkeyConfig::default());
    assert_eq!(**config.compatibility(), CompatibilityConfig::defaults_v1());
    assert!(config.macros().is_empty());
    assert!(!config.diagnostics_enabled());
    assert_eq!(config.config_revision(), 0);
    assert_eq!(config.macro_revision(), 0);
    // Exactly what other clients use before their first snapshot.
    let defaults = RuntimeSnapshot::defaults_v1();
    assert_eq!(config.options(), &defaults.options);
    assert_eq!(config.keymaps(), defaults.keymaps.as_slice());
    assert_eq!(config.hotkeys(), &defaults.hotkeys);
    assert_eq!(**config.compatibility(), defaults.compatibility);
    assert_eq!(config.vietnamese_mode(), defaults.vietnamese_mode);
    assert_eq!(config.diagnostics_enabled(), defaults.diagnostics_enabled);
    assert_eq!(config.config_revision(), defaults.config_revision);
    assert!(
        !core.is_connected(),
        "no controller connection until one is reported"
    );
}

#[test]
fn the_core_and_its_configuration_can_move_between_threads() {
    // The COM layer receives snapshots on its IPC thread and hands them to the thread that
    // owns the core.
    fn sendable<T: Send>() {}
    fn shareable<T: Send + Sync>() {}
    sendable::<ServiceCore>();
    shareable::<raumakey_tip::session::RuntimeConfig>();
    sendable::<SnapshotError>();
}

#[test]
fn defaults_v1_type_telex() {
    let mut core = ServiceCore::default();
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "Ddaay laf tieengs Vieetj");
    assert_eq!(doc.text(), "Đây là tiếng Việt");
    assert!(doc.edits.contains(&edit(1, "Đ")));
}

#[test]
fn a_runtime_snapshot_replaces_the_configuration() {
    let mut core = ServiceCore::new();
    let before = core.config().revision();
    let vni = snapshot(7, |s| s.options.input_method = InputMethod::Vni);
    core.apply_runtime_snapshot(&vni).expect("valid snapshot");
    assert_eq!(core.config().config_revision(), 7);
    assert_eq!(core.config().options().input_method, InputMethod::Vni);
    assert_ne!(core.config().revision(), before);
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "to6i aa");
    assert_eq!(doc.text(), "tôi aa");
}

#[test]
fn matrix_16_without_auto_restore_the_rendered_word_is_committed() {
    let mut core = ServiceCore::new();
    core.apply_runtime_snapshot(&snapshot(1, |s| s.options.auto_restore_invalid = false))
        .expect("valid snapshot");
    let mut doc = FakeDocument::new(1);
    let presses = doc.type_text(&mut core, "after ");
    assert_eq!(doc.text(), "àter ");
    assert_eq!(doc.edits, [edit(1, "à")]);
    assert!(presses.last().expect("space").reached_app());
}

#[test]
fn an_invalid_runtime_snapshot_keeps_the_last_good_configuration() {
    let mut core = ServiceCore::new();
    core.apply_runtime_snapshot(&snapshot(1, |s| s.options.input_method = InputMethod::Vni))
        .expect("valid snapshot");
    let good = std::sync::Arc::clone(core.config());
    let invalid = snapshot(2, |s| s.options.context_limit_codepoints = 0);
    assert_eq!(
        core.apply_runtime_snapshot(&invalid),
        Err(SnapshotError::Runtime(IpcError::Malformed(
            MalformedReason::InvalidOptions
        )))
    );
    assert!(std::sync::Arc::ptr_eq(&good, core.config()));
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "a6");
    assert_eq!(doc.text(), "â");
}

/// A user key map `mine` where `q` sets the acute tone.
fn user_keymap() -> KeyMap {
    KeyMap {
        id: "mine".to_owned(),
        version: KEYMAP_SCHEMA_VERSION,
        name: "Mine".to_owned(),
        bindings: vec![KeyBinding {
            key: 'q',
            actions: vec![KeyAction::Tone(Tone::Acute)],
        }],
    }
}

/// Selects the user key map `mine` in `snapshot`.
fn select_user_keymap(snapshot: &mut RuntimeSnapshot) {
    snapshot.options.input_method = InputMethod::UserDefined;
    snapshot.options.user_keymap_id = Some("mine".to_owned());
}

#[test]
fn a_user_method_types_with_the_key_map_the_snapshot_carries() {
    let mut core = ServiceCore::new();
    core.apply_runtime_snapshot(&snapshot(1, |s| {
        select_user_keymap(s);
        s.keymaps.push(user_keymap());
    }))
    .expect("valid snapshot");
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "aq ");
    assert_eq!(doc.text(), "á ");
}

#[test]
fn a_user_method_without_its_key_map_keeps_the_last_good_configuration() {
    // The engine would refuse every key of such a configuration (`UnsupportedInputMethod`),
    // and the core would let them all through without a word.
    let mut core = ServiceCore::new();
    let good = std::sync::Arc::clone(core.config());
    let mut other = user_keymap();
    other.id = "other".to_owned();
    for keymaps in [Vec::new(), vec![other]] {
        let missing = snapshot(1, |s| {
            select_user_keymap(s);
            s.keymaps = keymaps;
        });
        assert_eq!(
            core.apply_runtime_snapshot(&missing),
            Err(SnapshotError::Runtime(IpcError::Malformed(
                MalformedReason::InvalidKeymap
            )))
        );
        assert!(std::sync::Arc::ptr_eq(&good, core.config()));
    }
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "tieengs ");
    assert_eq!(doc.text(), "tiếng ", "Telex of defaults v1 still types");
}

#[test]
fn a_new_configuration_ends_the_word_in_progress() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    core.apply_runtime_snapshot(&snapshot(1, |s| s.options.modern_tone = true))
        .expect("valid snapshot");
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "aa");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn the_same_configuration_again_keeps_the_word_in_progress() {
    // A reconnect delivers the configuration the core already runs with.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    core.apply_runtime_snapshot(&RuntimeSnapshot::defaults_v1())
        .expect("valid snapshot");
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "â");
}

#[test]
fn macros_expand_at_a_boundary() {
    let mut core = ServiceCore::new();
    core.apply_macro_snapshot(&macros(3, &[(1, "ko", "không")]))
        .expect("valid macros");
    assert_eq!(core.config().macro_revision(), 3);
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko ");
    assert_eq!(doc.text(), "không ");
    assert_eq!(doc.edits, [edit(2, "không ")]);
}

#[test]
fn enter_after_a_macro_is_replayed_with_the_injected_tag() {
    let mut core = ServiceCore::new();
    core.apply_macro_snapshot(&macros(1, &[(1, "ko", "không")]))
        .expect("valid macros");
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko");
    let enter = doc.key(vk::RETURN);
    // `press` sends the replay back tagged and checks that the core does not claim it.
    let press = doc.press(&mut core, &enter);
    assert!(press.outcome.eaten);
    let replay = press.outcome.replay.expect("Enter is replayed");
    assert_eq!(replay.virtual_key, vk::RETURN);
    assert_eq!(replay.scan_code, Some(enter.scan_code));
    assert_eq!(doc.edits, [edit(2, "không")]);
    assert_eq!(doc.text(), "không\r");
    // The boundary ended the word.
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "không\ra");
    assert_eq!(doc.attempts, 1);
}

#[test]
fn a_failed_macro_edit_never_replays() {
    let mut core = ServiceCore::new();
    core.apply_macro_snapshot(&macros(1, &[(1, "ko", "không")]))
        .expect("valid macros");
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko");
    doc.behavior = Behavior::Deny;
    let press = doc.type_key(&mut core, vk::RETURN);
    assert!(press.claimed);
    assert!(!press.outcome.eaten);
    assert_eq!(press.outcome.replay, None);
    assert_eq!(doc.text(), "ko\r");
}

#[test]
fn an_invalid_macro_snapshot_keeps_the_last_good_table() {
    let mut core = ServiceCore::new();
    core.apply_macro_snapshot(&macros(1, &[(1, "ko", "không")]))
        .expect("valid macros");
    assert_eq!(
        core.apply_macro_snapshot(&macros(2, &[(1, "ko", "không"), (2, "", "x")])),
        Err(SnapshotError::MacroTable(MacroError::InvalidEntry {
            id: MacroId(2),
            reason: EntryError::EmptyTrigger,
        }))
    );
    assert_eq!(core.config().macro_revision(), 1);
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko ");
    assert_eq!(doc.text(), "không ");
}

#[test]
fn a_macro_table_built_on_another_thread_is_installed_as_it_is() {
    // The IPC thread validates and indexes up to `MAX_MACROS` entries; the thread that owns
    // the core only swaps the table in.
    let snapshot = macros(4, &[(1, "ko", "không")]);
    let table = std::thread::spawn(move || build_macro_table(&snapshot))
        .join()
        .expect("the thread finishes")
        .expect("valid macros");
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'k');
    core.install_macro_table(4, Arc::clone(&table));
    assert!(Arc::ptr_eq(core.config().macros(), &table));
    assert_eq!(core.config().macro_revision(), 4);
    doc.type_text(&mut core, "o ");
    assert_eq!(doc.text(), "không ", "the word in progress went on");
}

#[test]
fn building_a_macro_table_reports_why_it_fails() {
    assert_eq!(
        build_macro_table(&macros(2, &[(1, "", "x")])).map(|_| ()),
        Err(SnapshotError::MacroTable(MacroError::InvalidEntry {
            id: MacroId(1),
            reason: EntryError::EmptyTrigger,
        }))
    );
}

#[test]
fn a_macro_snapshot_over_the_entry_limit_is_rejected() {
    let mut core = ServiceCore::new();
    let too_many = MacroSnapshot {
        macro_revision: 1,
        entries: (0..=MAX_MACROS)
            .map(|i| MacroEntry {
                id: MacroId(u64::try_from(i).expect("small")),
                trigger: format!("t{i}"),
                replacement: "x".to_owned(),
                enabled: true,
                boundary: BoundaryPolicy::Delimiter,
                case_policy: CasePolicy::Adapt,
            })
            .collect(),
    };
    assert!(matches!(
        core.apply_macro_snapshot(&too_many),
        Err(SnapshotError::MacroSnapshot(IpcError::LimitExceeded {
            limit: Limit::Macros,
            ..
        }))
    ));
    assert!(core.config().macros().is_empty());
}

#[test]
fn the_controller_turns_vietnamese_off_and_on() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    core.set_vietnamese_mode(false);
    assert!(!core.vietnamese_mode());
    doc.type_char(&mut core, 'a');
    assert_eq!(
        doc.text(),
        "aa",
        "matrix #24: the word ended before the mode changed"
    );
    doc.type_text(&mut core, "s aa");
    assert_eq!(doc.text(), "aas aa");
    core.set_vietnamese_mode(true);
    assert!(core.vietnamese_mode());
    doc.type_text(&mut core, " aa");
    assert_eq!(doc.text(), "aas aa â");
}

#[test]
fn reset_all_ends_every_word() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    core.reset_all();
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "aa", "matrix #25: committed text is unchanged");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn a_rule_that_forbids_vietnamese_leaves_its_host_untouched() {
    let mut core = ServiceCore::new();
    core.apply_runtime_snapshot(&snapshot(1, |s| {
        s.compatibility.rules.push(notepad_rule(RuleAction {
            vietnamese: Some(false),
            ..RuleAction::default()
        }));
    }))
    .expect("valid snapshot");
    let mut doc = FakeDocument::new(1);
    let presses = doc.type_text(&mut core, "aas ");
    assert!(presses.iter().all(|press| !press.claimed));
    assert_eq!(doc.text(), "aas ");
}

#[test]
fn a_rule_that_forbids_macros_keeps_the_trigger() {
    let mut core = ServiceCore::new();
    core.apply_runtime_snapshot(&snapshot(1, |s| {
        s.compatibility.rules.push(notepad_rule(RuleAction {
            macros: Some(false),
            ..RuleAction::default()
        }));
    }))
    .expect("valid snapshot");
    core.apply_macro_snapshot(&macros(1, &[(1, "ko", "không")]))
        .expect("valid macros");
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko ");
    assert_eq!(doc.text(), "ko ");
}

#[test]
fn a_deletion_beyond_the_host_bound_is_never_applied() {
    let mut core = ServiceCore::new();
    core.apply_runtime_snapshot(&snapshot(1, |s| {
        s.compatibility.rules.push(notepad_rule(RuleAction {
            max_delete_before: Some(0),
            ..RuleAction::default()
        }));
    }))
    .expect("valid snapshot");
    let mut doc = FakeDocument::new(1);
    let presses = doc.type_text(&mut core, "aa");
    assert!(presses[1].reached_app());
    assert_eq!(doc.attempts, 0);
    assert_eq!(doc.text(), "aa");
}

#[test]
fn matrix_29_restricted_mode_still_bypasses_secure_fields() {
    // Design §14 and matrix #29; plan §6 lets `Restricted` keep transformation, an open
    // question (design §20.8 item 1). The TIP bypasses secure fields in every mode.
    let mut core = ServiceCore::new();
    core.apply_runtime_snapshot(&snapshot(1, |s| {
        s.compatibility.secure_field_mode = SecureFieldMode::Restricted;
    }))
    .expect("valid snapshot");
    core.apply_macro_snapshot(&macros(1, &[(1, "ko", "không")]))
        .expect("valid macros");
    let mut secure = host(1);
    secure.is_secure_field = true;
    let mut doc = FakeDocument::with_host(secure);
    let presses = doc.type_text(&mut core, "aas ko ");
    assert!(presses.iter().all(|press| !press.claimed));
    assert_eq!(doc.text(), "aas ko ");
}
