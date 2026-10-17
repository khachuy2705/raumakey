//! The keyboard-hook role (plan D39, design §21.2): the core of the portable controller,
//! which types every host through its global keyboard hook and is the only hotkey
//! classifier. There is no TIP in the portable product, so no host is left to another
//! transport, no TSF probe is asked for, and the hotkeys are classified whatever
//! `set_connected` says.
//!
//! A hook has no test-key-down step and cannot read the application's text: the drivers
//! here call `key_down` and `key_up` alone and never report a change of the document.

use proptest::collection::vec;
use proptest::prelude::*;
use raumakey_ipc::error::{IpcError, MalformedReason};
use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::host::ExeName;
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::vk;
use raumakey_model::options::InputMethod;
use raumakey_model::policy::{AppRule, RuleAction, Transport};
use raumakey_session::{KeyStroke, Role, ServiceCore, SnapshotError, check_runtime_snapshot};

use crate::support::{Behavior, FakeDocument, host, replay_stroke, stroke};

/// One key press as the hook front end drives the core: `key_down` (no test), the
/// application's own handling when the key was not eaten (never reported back), the tagged
/// replay, and `key_up`. Returns whether the key-down was eaten.
fn hook_press(core: &mut ServiceCore, doc: &mut FakeDocument, key: &KeyStroke) -> bool {
    let outcome = core.key_down(key, doc);
    if !outcome.eaten {
        doc.app_key(key);
    }
    if let Some(replay) = outcome.replay {
        let injected = replay_stroke(replay, &doc.host);
        assert!(!core.key_down(&injected, doc).eaten);
        doc.app_key(&injected);
        assert!(!core.key_up(&injected));
    }
    assert!(!core.key_up(key), "no key-up of a typed key is eaten");
    outcome.eaten
}

/// Types every character of `text` the hook's way.
fn hook_type(core: &mut ServiceCore, doc: &mut FakeDocument, text: &str) {
    for c in text.chars() {
        let key = doc.char_key(c);
        hook_press(core, doc, &key);
    }
}

/// Taps Ctrl+Shift the hook's way: Ctrl down, Shift down, Shift up, Ctrl up.
fn hook_tap_ctrl_shift(core: &mut ServiceCore, doc: &FakeDocument) {
    let ctrl = stroke(vk::LCONTROL, "", &doc.host);
    let mut shift = stroke(vk::LSHIFT, "", &doc.host);
    shift.modifiers.ctrl = true;
    let mut document = FakeDocument::with_host(doc.host.clone());
    assert!(!core.key_down(&ctrl, &mut document).eaten);
    assert!(!core.key_down(&shift, &mut document).eaten);
    assert!(!core.key_up(&shift));
    assert!(
        !core.key_up(&ctrl),
        "the release of a modifier-only hotkey reaches the application"
    );
    assert_eq!(document.attempts, 0);
}

/// A rule for `notepad.exe`, the executable of every fake document.
fn notepad_rule(transport: Transport) -> AppRule {
    AppRule {
        exe: ExeName::new("notepad.exe").expect("valid name"),
        window_class: None,
        action: RuleAction {
            transport: Some(transport),
            ..RuleAction::default()
        },
    }
}

#[test]
fn the_text_service_role_is_the_default() {
    assert_eq!(Role::default(), Role::TextService);
    assert_eq!(ServiceCore::new().role(), Role::TextService);
    assert_eq!(ServiceCore::default().role(), Role::TextService);
    assert_eq!(
        ServiceCore::with_role(Role::KeyboardHook).role(),
        Role::KeyboardHook
    );
}

#[test]
fn d39_the_hook_types_telex_with_key_down_alone() {
    let mut core = ServiceCore::with_role(Role::KeyboardHook);
    let mut doc = FakeDocument::new(1);
    hook_type(&mut core, &mut doc, "tieengs vieetj ");
    assert_eq!(doc.text(), "tiếng việt ");
}

#[test]
fn d39_the_hook_types_in_hosts_that_the_text_service_leaves_to_the_controller() {
    // Without TSF range edits the policy selects the controller's fallback transport. The
    // text service lets such a host through; the hook is the only transport and types it.
    let mut fallback = host(1);
    fallback.supports_tsf_range_edit = false;
    let mut tip = ServiceCore::new();
    let mut doc = FakeDocument::with_host(fallback.clone());
    hook_type(&mut tip, &mut doc, "tieengs");
    assert_eq!(
        doc.text(),
        "tieengs",
        "the text service leaves the host alone"
    );
    let mut core = ServiceCore::with_role(Role::KeyboardHook);
    let mut doc = FakeDocument::with_host(fallback);
    hook_type(&mut core, &mut doc, "tieengs");
    assert_eq!(doc.text(), "tiếng");
}

#[test]
fn d39_the_hook_types_hosts_whatever_transport_a_rule_selects() {
    for transport in [
        Transport::SyntheticKey,
        Transport::Clipboard,
        Transport::Tsf,
    ] {
        let mut snapshot = RuntimeSnapshot::defaults_v1();
        snapshot.config_revision = 1;
        snapshot.compatibility.clipboard_enabled = true;
        snapshot.compatibility.rules = vec![notepad_rule(transport)];
        let mut core = ServiceCore::with_role(Role::KeyboardHook);
        core.apply_runtime_snapshot(&snapshot)
            .expect("valid snapshot");
        let mut doc = FakeDocument::new(1);
        hook_type(&mut core, &mut doc, "vieetj");
        assert_eq!(doc.text(), "việt", "{transport:?}");
    }
}

#[test]
fn d39_the_hook_classifies_hotkeys_itself_even_when_marked_connected() {
    for connected in [false, true] {
        let mut core = ServiceCore::with_role(Role::KeyboardHook);
        core.set_connected(connected);
        let mut doc = FakeDocument::new(1);
        hook_tap_ctrl_shift(&mut core, &doc);
        assert!(!core.vietnamese_mode(), "connected: {connected}");
        hook_type(&mut core, &mut doc, "aa");
        assert_eq!(doc.text(), "aa");
        hook_tap_ctrl_shift(&mut core, &doc);
        assert!(core.vietnamese_mode(), "connected: {connected}");
    }
}

#[test]
fn d39_the_hook_classifies_hotkeys_in_hosts_that_the_text_service_leaves_alone() {
    let mut fallback = host(1);
    fallback.supports_tsf_range_edit = false;
    let doc = FakeDocument::with_host(fallback);
    let mut core = ServiceCore::with_role(Role::KeyboardHook);
    hook_tap_ctrl_shift(&mut core, &doc);
    assert!(!core.vietnamese_mode());
    // The text service, disconnected, leaves the keys of such a host to the controller.
    let mut tip = ServiceCore::new();
    hook_tap_ctrl_shift(&mut tip, &doc);
    assert!(tip.vietnamese_mode());
}

#[test]
fn d39_a_connection_change_does_not_restart_the_hooks_classifier() {
    // The hook owns the hotkeys in every state, so a connection change is no hand-over:
    // Ctrl and Shift held across it still make a toggle.
    let mut core = ServiceCore::with_role(Role::KeyboardHook);
    let mut doc = FakeDocument::new(1);
    let ctrl = stroke(vk::LCONTROL, "", &doc.host);
    let mut shift = stroke(vk::LSHIFT, "", &doc.host);
    shift.modifiers.ctrl = true;
    core.key_down(&ctrl, &mut doc);
    core.key_down(&shift, &mut doc);
    core.set_connected(true);
    core.key_up(&shift);
    core.key_up(&ctrl);
    assert!(!core.vietnamese_mode());
}

#[test]
fn d39_the_hook_never_asks_for_a_tsf_probe() {
    let core = ServiceCore::with_role(Role::KeyboardHook);
    assert!(!core.needs_probe(&host(1)));
    assert!(ServiceCore::new().needs_probe(&host(1)));
}

#[test]
fn d39_backspace_restores_the_state_before_with_key_down_alone() {
    // Matrix #17: `a a Backspace` gives `a`.
    let mut core = ServiceCore::with_role(Role::KeyboardHook);
    let mut doc = FakeDocument::new(1);
    hook_type(&mut core, &mut doc, "aa");
    assert_eq!(doc.text(), "â");
    let backspace = doc.key(vk::BACK);
    assert!(hook_press(&mut core, &mut doc, &backspace));
    assert_eq!(doc.text(), "a");
}

/// One step of the hook-role property test.
#[derive(Clone, Debug)]
enum HookOp {
    /// A key-down (and its key-up when the flag is set) in document `0..3`: a character, or a
    /// virtual key without text, with modifier bits, possibly tagged as injected, in a host
    /// that the text service would leave to the controller, or in a secure field.
    Key {
        doc: usize,
        key: Result<char, u32>,
        modifiers: u8,
        injected: bool,
        fallback_host: bool,
        secure: bool,
        release: bool,
    },
    Focus(Option<usize>),
    Mouse(usize),
    Mode(bool),
    Connected(bool),
    Vni(bool),
    Deny(usize, bool),
}

fn hook_op() -> impl Strategy<Value = HookOp> {
    let chars = prop::sample::select(vec![
        'a', 'e', 'o', 'w', 'd', 's', 'f', 'r', 'x', 'j', 'z', 't', 'n', 'g', '6', '1', '.', ' ',
        '😀',
    ]);
    let keys = prop_oneof![
        chars.prop_map(Ok),
        prop::sample::select(vec![
            vk::BACK,
            vk::LEFT,
            vk::DELETE,
            vk::RETURN,
            vk::TAB,
            vk::LCONTROL,
            vk::LSHIFT,
            vk::RMENU,
            vk::CAPITAL,
            vk::F12,
        ])
        .prop_map(Err),
    ];
    prop_oneof![
        8 => (0..3_usize, keys, any::<u8>(), any::<u8>(), any::<bool>()).prop_map(
            |(doc, key, modifiers, flags, release)| HookOp::Key {
                doc,
                key,
                modifiers,
                injected: flags % 11 == 0,
                fallback_host: flags % 3 == 0,
                secure: flags % 13 == 0,
                release,
            }
        ),
        1 => prop::option::of(0..3_usize).prop_map(HookOp::Focus),
        1 => (0..3_usize).prop_map(HookOp::Mouse),
        1 => any::<bool>().prop_map(HookOp::Mode),
        1 => any::<bool>().prop_map(HookOp::Connected),
        1 => any::<bool>().prop_map(HookOp::Vni),
        1 => (0..3_usize, any::<bool>()).prop_map(|(doc, deny)| HookOp::Deny(doc, deny)),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// The hook drives the core with `key_down` and `key_up` alone and reports no document
    /// change; no sequence makes the core panic, a replay comes only with an eaten key,
    /// injected keys are never handled, and secure fields are never edited.
    #[test]
    fn the_hook_role_never_panics_and_keeps_its_guarantees(ops in vec(hook_op(), 0..120)) {
        let mut core = ServiceCore::with_role(Role::KeyboardHook);
        let mut docs: Vec<FakeDocument> = (1..=3_u128).map(FakeDocument::new).collect();
        for op in ops {
            match op {
                HookOp::Key { doc, key, modifiers, injected, fallback_host, secure, release } => {
                    let document = &mut docs[doc];
                    let mut stroke = match key {
                        Ok(c) => document.char_key(c),
                        Err(virtual_key) => document.key(virtual_key),
                    };
                    let bit = |n: u8| modifiers & (1 << n) != 0;
                    stroke.modifiers.shift |= bit(0);
                    stroke.modifiers.caps_lock = bit(1);
                    stroke.modifiers.ctrl = bit(2) && bit(3);
                    stroke.modifiers.alt = bit(4) && bit(5);
                    stroke.modifiers.alt_gr = bit(6);
                    stroke.repeat = bit(7);
                    stroke.host.supports_tsf_range_edit = !fallback_host;
                    stroke.host.is_secure_field = secure;
                    if injected {
                        stroke.extra_info = RAUMAKEY_INJECTED_TAG;
                    }
                    let attempts = document.attempts;
                    let outcome = core.key_down(&stroke, document);
                    prop_assert!(outcome.eaten || outcome.replay.is_none());
                    if injected {
                        prop_assert!(!outcome.eaten);
                        prop_assert_eq!(document.attempts, attempts);
                    }
                    if secure {
                        prop_assert_eq!(document.attempts, attempts);
                    }
                    if !outcome.eaten {
                        let _ = document.app_key(&stroke);
                    }
                    if release {
                        let eaten = core.key_up(&stroke);
                        prop_assert!(!(injected && eaten));
                    }
                }
                HookOp::Focus(doc) => core.set_focus(doc.map(|i| docs[i].id)),
                HookOp::Mouse(doc) => core.mouse_down(&docs[doc].host),
                HookOp::Mode(mode) => core.set_vietnamese_mode(mode),
                HookOp::Connected(connected) => core.set_connected(connected),
                HookOp::Vni(vni) => {
                    let mut snapshot = RuntimeSnapshot::defaults_v1();
                    snapshot.config_revision = 1;
                    if vni {
                        snapshot.options.input_method = InputMethod::Vni;
                    }
                    prop_assert!(core.apply_runtime_snapshot(&snapshot).is_ok());
                }
                HookOp::Deny(doc, deny) => {
                    docs[doc].behavior = if deny { Behavior::Deny } else { Behavior::Accept };
                }
            }
            prop_assert_eq!(core.role(), Role::KeyboardHook);
        }
    }
}

#[test]
fn a_runtime_snapshot_is_checked_without_a_core() {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    assert_eq!(check_runtime_snapshot(&snapshot), Ok(()));
    snapshot.options.input_method = InputMethod::UserDefined;
    snapshot.options.user_keymap_id = Some("missing".to_owned());
    let refused = check_runtime_snapshot(&snapshot);
    assert_eq!(
        refused,
        Err(SnapshotError::Runtime(IpcError::Malformed(
            MalformedReason::InvalidKeymap
        )))
    );
    // The core refuses it alike.
    let mut core = ServiceCore::with_role(Role::KeyboardHook);
    assert_eq!(core.apply_runtime_snapshot(&snapshot), refused);
}
