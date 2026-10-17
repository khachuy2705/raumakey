//! Property test: no sequence of keys, document events, mouse presses, probe results, failing
//! edits, and configuration changes makes the core panic, and every step keeps the core's
//! guarantees: a replay comes only with an eaten key, injected keys and hosts left to the
//! controller are never claimed, secure fields are never edited, and the number of document
//! states stays bounded. (The fake document also checks every edit request it receives.)

use proptest::collection::vec;
use proptest::prelude::*;
use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_model::host::{ExeName, SessionId};
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::vk;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::{InputMethod, Normalization};
use raumakey_model::policy::{AppRule, ProbeResult, RuleAction};
use raumakey_model::utf16;
use raumakey_session::{KeyDownOutcome, KeyStroke, MAX_DOCUMENTS, SelectionChange, ServiceCore};

use crate::support::{Behavior, FakeDocument, release, replay_stroke};

const DOCUMENTS: usize = 3;

/// Characters that exercise Telex transformations, boundaries, and odd input.
const CHARS: &[char] = &[
    'a', 'a', 'e', 'o', 'u', 'w', 'd', 's', 'f', 'r', 'x', 'j', 'z', 't', 'n', 'g', 'h', 'i', 'k',
    'A', 'D', 'W', '6', '1', '.', '\'', '\\', '^', 'ữ', '😀',
];

#[derive(Clone, Copy, Debug)]
enum Path {
    TestThenDown,
    TestTwiceThenDown,
    DownWithoutTest,
    TestWithoutDown,
}

#[derive(Clone, Debug)]
struct Key {
    doc: usize,
    virtual_key: u32,
    scan_code: u32,
    extended: bool,
    modifiers: u8,
    text: Vec<u16>,
    extra: u8,
    secure: bool,
    controller_host: bool,
    no_document: bool,
    other_session: bool,
    path: Path,
    /// The host reports what the application did with the key (otherwise never).
    report: bool,
    release: bool,
}

#[derive(Clone, Debug)]
enum Op {
    Key(Key),
    Focus(Option<usize>),
    Selection(usize, u8),
    Close(usize),
    Connect(bool),
    Mode(bool),
    ResetAll,
    Snapshot(u8),
    Macros(u8),
    Behavior(usize, u8),
    MoveCaret(usize, usize),
    AltGr(bool),
    Mouse(usize),
    Probe(usize, bool),
}

fn text() -> impl Strategy<Value = Vec<u16>> {
    prop_oneof![
        5 => prop::sample::select(CHARS).prop_map(|c| c.encode_utf16(&mut [0; 2]).to_vec()),
        2 => vec(any::<u16>(), 0..4),
        1 => Just(Vec::new()),
    ]
}

fn virtual_key() -> impl Strategy<Value = u32> {
    prop_oneof![
        4 => 0x41_u32..=0x5A,
        3 => prop::sample::select(vec![
            vk::BACK, vk::SPACE, vk::RETURN, vk::TAB, vk::LEFT, vk::RIGHT, vk::DELETE,
            vk::ESCAPE, vk::F12, vk::SHIFT, vk::CONTROL, vk::MENU, vk::CAPITAL,
        ]),
        1 => 0_u32..=0x1FF,
    ]
}

fn path() -> impl Strategy<Value = Path> {
    prop_oneof![
        6 => Just(Path::TestThenDown),
        1 => Just(Path::TestTwiceThenDown),
        1 => Just(Path::DownWithoutTest),
        1 => Just(Path::TestWithoutDown),
    ]
}

prop_compose! {
    fn key()(
        doc in 0..DOCUMENTS,
        virtual_key in virtual_key(),
        scan_code in 0_u32..4,
        extended in any::<bool>(),
        modifiers in prop_oneof![4 => Just(0_u8), 1 => any::<u8>()],
        text in text(),
        extra in prop_oneof![8 => Just(0_u8), 1 => 1_u8..=3],
        secure in prop::bool::weighted(0.1),
        controller_host in prop::bool::weighted(0.05),
        no_document in prop::bool::weighted(0.05),
        other_session in prop::bool::weighted(0.05),
        path in path(),
        report in prop::bool::weighted(0.85),
        release in prop::bool::weighted(0.8),
    ) -> Key {
        Key {
            doc, virtual_key, scan_code, extended, modifiers, text, extra, secure,
            controller_host, no_document, other_session, path, report, release,
        }
    }
}

fn op() -> impl Strategy<Value = Op> {
    prop_oneof![
        30 => key().prop_map(Op::Key),
        1 => prop::option::of(0..DOCUMENTS).prop_map(Op::Focus),
        2 => (0..DOCUMENTS, any::<u8>()).prop_map(|(doc, kind)| Op::Selection(doc, kind)),
        1 => (0..DOCUMENTS).prop_map(Op::Close),
        1 => any::<bool>().prop_map(Op::Connect),
        1 => any::<bool>().prop_map(Op::Mode),
        1 => Just(Op::ResetAll),
        1 => any::<u8>().prop_map(Op::Snapshot),
        1 => any::<u8>().prop_map(Op::Macros),
        2 => (0..DOCUMENTS, any::<u8>()).prop_map(|(doc, b)| Op::Behavior(doc, b)),
        1 => (0..DOCUMENTS, 0_usize..8).prop_map(|(doc, at)| Op::MoveCaret(doc, at)),
        1 => any::<bool>().prop_map(Op::AltGr),
        1 => (0..DOCUMENTS).prop_map(Op::Mouse),
        1 => (0..DOCUMENTS, any::<bool>()).prop_map(|(doc, ok)| Op::Probe(doc, ok)),
    ]
}

fn runtime_snapshot(choice: u8) -> RuntimeSnapshot {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = u64::from(choice);
    match choice % 6 {
        0 => {}
        1 => snapshot.options.input_method = InputMethod::Vni,
        2 => {
            snapshot.options.modern_tone = true;
            snapshot.options.normalization = Normalization::Nfd;
        }
        3 => snapshot.options.context_limit_codepoints = 0,
        4 => snapshot.compatibility.rules.push(AppRule {
            exe: ExeName::new("notepad.exe").expect("valid name"),
            window_class: None,
            action: RuleAction {
                max_delete_before: Some(1),
                ..RuleAction::default()
            },
        }),
        _ => {
            snapshot.options.spell_check = false;
            snapshot.hotkeys.toggle_mode = vec!["Alt+Z".parse().expect("hotkey")];
        }
    }
    snapshot
}

fn macro_snapshot(choice: u8) -> MacroSnapshot {
    let entry = |id, trigger: &str, replacement: &str| MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    };
    let entries = match choice % 3 {
        0 => vec![entry(1, "ko", "không"), entry(2, "a", "😀 ữ")],
        1 => Vec::new(),
        _ => vec![entry(1, "", "x")],
    };
    MacroSnapshot {
        macro_revision: u64::from(choice),
        entries,
    }
}

/// The stroke `spec` describes in its document.
fn stroke(spec: &Key, doc: &FakeDocument) -> KeyStroke {
    let mut host = doc.host.clone();
    host.is_secure_field = spec.secure;
    host.supports_tsf_range_edit = !spec.controller_host;
    if spec.no_document {
        host.document_id = None;
    }
    if spec.other_session {
        host.session_id = SessionId::from_u128(77);
    }
    let bit = |n: u8| spec.modifiers & (1 << n) != 0;
    let mut key = crate::support::stroke(spec.virtual_key, "", &host);
    key.scan_code = spec.scan_code;
    key.extended = spec.extended;
    key.text = spec.text.clone();
    key.modifiers.shift = bit(0);
    key.modifiers.caps_lock = bit(1);
    key.modifiers.ctrl = bit(2);
    key.modifiers.alt = bit(3);
    key.modifiers.alt_gr = bit(4);
    key.modifiers.win = bit(5);
    key.extra_info = match spec.extra {
        0 => 0,
        1 => RAUMAKEY_INJECTED_TAG,
        2 => 0xEBCA_1234,
        _ => u64::MAX,
    };
    key
}

/// Drives one key the way TSF would for `spec.path` and checks the per-key guarantees.
fn drive_key(core: &mut ServiceCore, doc: &mut FakeDocument, spec: &Key) {
    let key = stroke(spec, doc);
    let attempts = doc.attempts;
    let (claimed, outcome) = match spec.path {
        Path::TestThenDown | Path::TestTwiceThenDown => {
            let claimed = core.test_key_down(&key);
            if matches!(spec.path, Path::TestTwiceThenDown) {
                assert_eq!(core.test_key_down(&key), claimed, "a repeated test agrees");
            }
            let outcome = if claimed {
                core.key_down(&key, doc)
            } else {
                KeyDownOutcome::pass_through()
            };
            (claimed, outcome)
        }
        Path::DownWithoutTest => (true, core.key_down(&key, doc)),
        Path::TestWithoutDown => {
            core.test_key_down(&key);
            (false, KeyDownOutcome::pass_through())
        }
    };
    assert!(
        outcome.eaten || outcome.replay.is_none(),
        "replay without eating"
    );
    if key.is_injected() || spec.controller_host {
        assert!(
            !outcome.eaten && (!claimed || matches!(spec.path, Path::DownWithoutTest)),
            "injected keys and controller hosts are never handled"
        );
    }
    if spec.secure {
        assert_eq!(doc.attempts, attempts, "a secure field is never edited");
    }
    if !outcome.eaten {
        if spec.report {
            doc.deliver(core, &key);
        } else {
            let _ = doc.app_key(&key);
        }
    }
    if let Some(replay) = outcome.replay {
        let injected = replay_stroke(replay, &doc.host);
        assert!(!core.test_key_down(&injected), "a replay is never claimed");
        doc.deliver(core, &injected);
    }
    if spec.release {
        release(core, &key);
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn the_core_never_panics_and_keeps_its_guarantees(ops in vec(op(), 0..160)) {
        let mut core = ServiceCore::new();
        let mut docs: Vec<FakeDocument> = (1..=DOCUMENTS)
            .map(|id| FakeDocument::new(u128::try_from(id).expect("small")))
            .collect();
        for op in &ops {
            match op {
                Op::Key(spec) => drive_key(&mut core, &mut docs[spec.doc], spec),
                Op::Focus(doc) => core.set_focus(doc.map(|i| docs[i].id)),
                Op::Selection(doc, kind) => {
                    let inserted = utf16::encode(if kind % 2 == 0 { "a" } else { "\u{1F600}" });
                    let change = match kind % 4 {
                        0 | 1 => SelectionChange::Inserted(&inserted),
                        2 => SelectionChange::Deleted,
                        _ => SelectionChange::Other,
                    };
                    core.selection_changed(docs[*doc].id, change);
                }
                Op::Close(doc) => core.document_closed(docs[*doc].id),
                Op::Connect(connected) => core.set_connected(*connected),
                Op::Mode(mode) => core.set_vietnamese_mode(*mode),
                Op::ResetAll => core.reset_all(),
                Op::Snapshot(choice) => {
                    let result = core.apply_runtime_snapshot(&runtime_snapshot(*choice));
                    prop_assert_eq!(result.is_err(), choice % 6 == 3);
                }
                Op::Macros(choice) => {
                    let result = core.apply_macro_snapshot(&macro_snapshot(*choice));
                    prop_assert_eq!(result.is_err(), choice % 3 == 2);
                }
                Op::Behavior(doc, behavior) => {
                    docs[*doc].behavior = match behavior % 4 {
                        0 => Behavior::Accept,
                        1 => Behavior::Deny,
                        2 => Behavior::Shorter(0),
                        _ => Behavior::Shorter(1),
                    };
                }
                Op::MoveCaret(doc, at) => docs[*doc].move_caret(*at),
                Op::AltGr(altgr) => core.set_altgr_layout(*altgr),
                Op::Mouse(doc) => core.mouse_down(&docs[*doc].host),
                Op::Probe(doc, ok) => {
                    let probe = if *ok {
                        ProbeResult::TsfRangeEditOk
                    } else {
                        ProbeResult::TsfRangeEditFailed
                    };
                    prop_assert!(core.record_probe(&docs[*doc].host, probe).is_ok());
                }
            }
            prop_assert!(core.document_count() <= MAX_DOCUMENTS);
        }
    }
}
