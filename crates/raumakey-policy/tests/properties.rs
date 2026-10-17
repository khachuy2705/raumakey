//! Property tests: policy resolution is total and deterministic; the probe cache stays bounded,
//! a full cache turns a record away only when no cached record may make room for it, and
//! `needs_probe` asks exactly for results that can be stored; the hotkey classifier never panics,
//! gives the verdict of an independent model of the documented rules for every key transition (so
//! a modifier-only hotkey fires exactly when its tap is complete and unspoiled), never withholds a
//! modifier, and never withholds a key-up whose key-down reached the application.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use proptest::collection::vec;
use proptest::option;
use proptest::prelude::*;
use raumakey_model::host::{DocumentId, ExeName, HostContext, SessionId};
use raumakey_model::hotkey::{Hotkey, HotkeyAction, HotkeyConfig, HotkeyKey};
use raumakey_model::input::vk;
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, NamedSet, PROBE_VERSION, ProbeRecord, ProbeResult, RuleAction,
    SecureFieldMode, Transport,
};
use raumakey_policy::hotkey::{HotkeyClassifier, HotkeyMatch, KeyVerdict};
use raumakey_policy::{CompatibilityPolicy, MAX_PROBE_RECORDS, PolicyError, ProbeCache};

const EXES: [&str; 6] = [
    "notepad.exe",
    "chrome.exe",
    "wt.exe",
    "game.exe",
    "legacy.exe",
    "Paste.EXE",
];
const CLASSES: [&str; 4] = ["Edit", "EDIT", "RichEdit20W", ""];

fn exe_name() -> impl Strategy<Value = ExeName> {
    prop::sample::select(&EXES[..]).prop_map(|n| ExeName::new(n).unwrap())
}

fn class() -> impl Strategy<Value = Option<String>> {
    option::of(prop::sample::select(&CLASSES[..]).prop_map(str::to_owned))
}

fn transport() -> impl Strategy<Value = Transport> {
    prop_oneof![
        Just(Transport::Tsf),
        Just(Transport::SyntheticKey),
        Just(Transport::Clipboard),
    ]
}

fn secure_mode() -> impl Strategy<Value = SecureFieldMode> {
    prop_oneof![
        Just(SecureFieldMode::Bypass),
        Just(SecureFieldMode::Restricted)
    ]
}

fn action() -> impl Strategy<Value = RuleAction> {
    (
        option::of(transport()),
        option::of(any::<bool>()),
        option::of(any::<bool>()),
        option::of(any::<bool>()),
        option::of(any::<u32>()),
    )
        .prop_map(
            |(transport, reset_on_mouse, macros, vietnamese, max_delete_before)| RuleAction {
                transport,
                reset_on_mouse,
                macros,
                vietnamese,
                max_delete_before,
            },
        )
}

fn app_rule() -> impl Strategy<Value = AppRule> {
    (exe_name(), class(), action()).prop_map(|(exe, window_class, action)| AppRule {
        exe,
        window_class,
        action,
    })
}

fn named_set() -> impl Strategy<Value = NamedSet> {
    ("[a-z-]{1,12}", vec(exe_name(), 0..4), action()).prop_map(|(name, members, action)| NamedSet {
        name,
        members,
        action,
    })
}

fn probe_record() -> impl Strategy<Value = ProbeRecord> {
    (exe_name(), 0..=PROBE_VERSION + 2, any::<bool>()).prop_map(
        |(exe, probe_version, tsf_range_edit)| ProbeRecord {
            exe,
            probe_version,
            tsf_range_edit,
        },
    )
}

fn config() -> impl Strategy<Value = CompatibilityConfig> {
    (
        any::<bool>(),
        any::<bool>(),
        secure_mode(),
        any::<u32>(),
        vec(app_rule(), 0..8),
        vec(named_set(), 0..5),
        vec(probe_record(), 0..8),
    )
        .prop_map(
            |(
                clipboard_enabled,
                default_reset_on_mouse,
                secure_field_mode,
                default_max_delete_before,
                rules,
                named_sets,
                probes,
            )| CompatibilityConfig {
                clipboard_enabled,
                default_reset_on_mouse,
                secure_field_mode,
                default_max_delete_before,
                rules,
                named_sets,
                probes,
            },
        )
}

fn host() -> impl Strategy<Value = HostContext> {
    (
        (
            exe_name(),
            class(),
            any::<bool>(),
            any::<bool>(),
            any::<bool>(),
        ),
        (
            any::<u128>(),
            option::of(any::<u128>()),
            any::<u64>(),
            any::<u64>(),
            any::<u64>(),
            "[0-9A-F]{8}",
        ),
    )
        .prop_map(
            |(
                (executable, window_class, supports_tsf_range_edit, is_secure_field, is_elevated),
                (session, document, process_id, thread_id, foreground_epoch, keyboard_layout),
            )| {
                let mut h = HostContext::new(SessionId::from_u128(session), executable);
                h.window_class = window_class;
                h.supports_tsf_range_edit = supports_tsf_range_edit;
                h.is_secure_field = is_secure_field;
                h.is_elevated = is_elevated;
                h.document_id = document.map(DocumentId::from_u128);
                h.process_id = process_id;
                h.thread_id = thread_id;
                h.foreground_epoch = foreground_epoch;
                h.keyboard_layout = keyboard_layout;
                h
            },
        )
}

fn probe_result(ok: bool) -> ProbeResult {
    if ok {
        ProbeResult::TsfRangeEditOk
    } else {
        ProbeResult::TsfRangeEditFailed
    }
}

proptest! {
    #[test]
    fn resolve_is_total_deterministic_and_keeps_the_safety_invariants(
        config in config(),
        host in host(),
    ) {
        let shared = Arc::new(config);
        let policy = CompatibilityPolicy::new(Arc::clone(&shared));
        let resolved = policy.resolve(&host);
        prop_assert_eq!(resolved, policy.resolve(&host));
        prop_assert_eq!(
            resolved,
            CompatibilityPolicy::new(Arc::new((*shared).clone())).resolve(&host)
        );
        prop_assert_eq!(resolved.secure_field_mode, shared.secure_field_mode);
        prop_assert!(resolved.permit_replay);
        if resolved.transport == Transport::Clipboard {
            prop_assert!(shared.clipboard_enabled);
            prop_assert!(!host.is_secure_field);
        }
        if host.is_secure_field {
            prop_assert!(!resolved.macro_allowed);
            if shared.secure_field_mode == SecureFieldMode::Bypass {
                prop_assert!(!resolved.vietnamese_allowed);
            }
        }
    }

    #[test]
    fn resolve_depends_only_on_the_identifying_host_fields(
        config in config(),
        host in host(),
        other in host(),
    ) {
        let policy = CompatibilityPolicy::new(Arc::new(config));
        let mut variant = other;
        variant.executable = host.executable.clone();
        variant.window_class = host.window_class.clone();
        variant.supports_tsf_range_edit = host.supports_tsf_range_edit;
        variant.is_secure_field = host.is_secure_field;
        prop_assert_eq!(policy.resolve(&host), policy.resolve(&variant));
    }

    #[test]
    fn probe_cache_keeps_the_newest_version_per_exe(records in vec(probe_record(), 0..32)) {
        let cache = ProbeCache::from_records(records.clone());
        let mut newest: BTreeMap<&ExeName, u32> = BTreeMap::new();
        for r in &records {
            let v = newest.entry(&r.exe).or_insert(r.probe_version);
            *v = (*v).max(r.probe_version);
        }
        prop_assert_eq!(cache.len(), newest.len());
        for (exe, version) in newest {
            prop_assert_eq!(cache.get(exe).map(|r| r.probe_version), Some(version));
        }
    }

    #[test]
    fn record_probe_keeps_one_bounded_record_per_exe(
        config in config(),
        steps in vec((exe_name(), any::<bool>()), 0..48),
    ) {
        let mut policy = CompatibilityPolicy::new(Arc::new(config));
        for (exe, ok) in steps {
            let host = HostContext::new(SessionId::default(), exe.clone());
            match policy.record_probe(&host, probe_result(ok)) {
                Ok(()) => {
                    prop_assert_eq!(
                        policy.probe_cache().current(&exe).map(|r| r.tsf_range_edit),
                        Some(ok)
                    );
                    prop_assert!(!policy.needs_probe(&host));
                }
                Err(PolicyError::NewerProbeVersion { stored }) => {
                    prop_assert!(stored > PROBE_VERSION);
                }
                Err(PolicyError::ProbeCacheFull) => {
                    prop_assert!(false, "a cache with few executables never fills");
                }
            }
            let names: BTreeSet<&ExeName> = policy.probes().iter().map(|r| &r.exe).collect();
            prop_assert_eq!(names.len(), policy.probes().len());
            prop_assert!(policy.probes().len() <= MAX_PROBE_RECORDS);
        }
    }

    /// No re-probe loop: a host is asked for exactly when its result can be stored and the
    /// cache has no current record of it.
    #[test]
    fn needs_probe_asks_exactly_for_results_that_can_be_stored(
        config in config(),
        exe in exe_name(),
        ok in any::<bool>(),
    ) {
        let mut policy = CompatibilityPolicy::new(Arc::new(config));
        let host = HostContext::new(SessionId::default(), exe.clone());
        let wanted = policy.needs_probe(&host);
        let cached = policy.probe_cache().current(&exe).is_some();
        let stored = policy.record_probe(&host, probe_result(ok)).is_ok();
        prop_assert_eq!(wanted, !cached && stored);
        if stored {
            prop_assert!(!policy.needs_probe(&host));
        }
    }
}

// ----- full probe cache -----

/// A record of `exe` in one of four classes: older version, current success, current failure,
/// newer version.
fn class_record(exe: &str, class: u8) -> ProbeRecord {
    let (probe_version, tsf_range_edit) = match class % 4 {
        0 => (PROBE_VERSION - 1, false),
        1 => (PROBE_VERSION, true),
        2 => (PROBE_VERSION, false),
        _ => (PROBE_VERSION + 1, false),
    };
    ProbeRecord {
        exe: ExeName::new(exe).unwrap(),
        probe_version,
        tsf_range_edit,
    }
}

/// Classes of a full cache, drawn from a random palette so that caches without stale or without
/// successful records occur too.
fn full_cache_classes() -> impl Strategy<Value = Vec<u8>> {
    prop::sample::subsequence(vec![0_u8, 1, 2, 3], 1..=4)
        .prop_flat_map(|palette| vec(prop::sample::select(palette), MAX_PROBE_RECORDS))
}

/// The documented eviction order: lower is evicted first, `None` is never evicted.
fn usefulness(record: &ProbeRecord) -> Option<u8> {
    if record.probe_version < PROBE_VERSION {
        Some(0)
    } else if record.probe_version > PROBE_VERSION {
        None
    } else if record.tsf_range_edit {
        Some(1)
    } else {
        Some(2)
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn a_full_cache_evicts_the_least_useful_record_or_reports_full(
        classes in full_cache_classes(),
        incoming_class in 0_u8..4,
        known in option::of(0..MAX_PROBE_RECORDS),
    ) {
        let records: Vec<ProbeRecord> = classes
            .iter()
            .enumerate()
            .map(|(i, class)| class_record(&format!("app{i}.exe"), *class))
            .collect();
        let mut cache = ProbeCache::from_records(records.clone());
        prop_assert_eq!(cache.records(), &records[..]);
        let name = known.map_or_else(|| "newcomer.exe".to_owned(), |i| format!("app{i}.exe"));
        let incoming = class_record(&name, incoming_class);

        let policy = CompatibilityPolicy::new(Arc::new(CompatibilityConfig {
            probes: records.clone(),
            ..CompatibilityConfig::defaults_v1()
        }));
        let host = HostContext::new(SessionId::default(), incoming.exe.clone());
        let stored = known.and_then(|i| records.get(i));

        let mut expected = records.clone();
        let outcome = match stored {
            Some(old) if old.probe_version > incoming.probe_version => {
                Err(PolicyError::NewerProbeVersion { stored: old.probe_version })
            }
            Some(_) => {
                if let Some(i) = known
                    && let Some(slot) = expected.get_mut(i)
                {
                    *slot = incoming.clone();
                }
                Ok(())
            }
            None => {
                let limit = if incoming.probe_version == PROBE_VERSION { 2 } else { 0 };
                let victim = records
                    .iter()
                    .enumerate()
                    .filter_map(|(i, r)| usefulness(r).filter(|u| *u <= limit).map(|u| (u, i)))
                    .min()
                    .map(|(_, i)| i);
                match victim {
                    Some(i) => {
                        expected.remove(i);
                        expected.push(incoming.clone());
                        Ok(())
                    }
                    None => Err(PolicyError::ProbeCacheFull),
                }
            }
        };
        prop_assert_eq!(cache.upsert(incoming.clone()), outcome);
        prop_assert_eq!(cache.records(), &expected[..]);
        prop_assert!(cache.len() <= MAX_PROBE_RECORDS);

        let current_result_storable = match stored {
            Some(old) => old.probe_version <= PROBE_VERSION,
            None => records.iter().any(|r| r.probe_version <= PROBE_VERSION),
        };
        let has_current = stored.is_some_and(|r| r.probe_version == PROBE_VERSION);
        prop_assert_eq!(
            policy.needs_probe(&host),
            !has_current && current_result_storable
        );
    }
}

// ----- hotkey classifier -----

const MODIFIERS: [u32; 11] = [
    vk::SHIFT,
    vk::LSHIFT,
    vk::RSHIFT,
    vk::CONTROL,
    vk::LCONTROL,
    vk::RCONTROL,
    vk::MENU,
    vk::LMENU,
    vk::RMENU,
    vk::LWIN,
    vk::RWIN,
];
const KEYS: [u32; 6] = [vk::F12, vk::SPACE, 0x41, 0x5A, vk::F1, vk::RETURN];

#[derive(Clone, Copy, Debug)]
enum Step {
    Key(u32, bool),
    /// Another key-down of the last key that went down, like an auto-repeat.
    Repeat,
    Reset,
    Interrupt,
    AltGrLayout(bool),
}

fn key_code() -> impl Strategy<Value = u32> + Clone {
    prop_oneof![
        5 => prop::sample::select(&MODIFIERS[..]),
        3 => prop::sample::select(&KEYS[..]),
        1 => any::<u32>(),
    ]
}

/// The keys of the default hotkeys plus AltGr and Win, so that hotkey combinations are often held.
const DENSE_KEYS: [u32; 7] = [
    vk::LCONTROL,
    vk::LSHIFT,
    vk::LMENU,
    vk::RMENU,
    vk::LWIN,
    vk::F12,
    vk::SPACE,
];

fn dense_key_code() -> impl Strategy<Value = u32> + Clone {
    prop::sample::select(&DENSE_KEYS[..])
}

/// Steps over `keys`, with key-downs weighted `downs` against 10 for key-ups.
fn steps(
    keys: impl Strategy<Value = u32> + Clone,
    downs: u32,
    len: core::ops::Range<usize>,
) -> impl Strategy<Value = Vec<Step>> {
    let step = prop_oneof![
        downs => keys.clone().prop_map(|code| Step::Key(code, true)),
        10 => keys.prop_map(|code| Step::Key(code, false)),
        4 => Just(Step::Repeat),
        1 => Just(Step::Reset),
        1 => Just(Step::Interrupt),
        1 => any::<bool>().prop_map(Step::AltGrLayout),
    ];
    vec(step, len)
}

/// Modifier keys only, so that modifier-only hotkeys are often tapped without another key.
const MODIFIER_TAP_KEYS: [u32; 6] = [
    vk::LCONTROL,
    vk::LSHIFT,
    vk::LMENU,
    vk::RMENU,
    vk::CONTROL,
    vk::SHIFT,
];

/// Starts half of the streams on an AltGr layout.
fn with_altgr_start(steps: impl Strategy<Value = Vec<Step>>) -> impl Strategy<Value = Vec<Step>> {
    (any::<bool>(), steps).prop_map(|(altgr, mut steps)| {
        if altgr {
            steps.insert(0, Step::AltGrLayout(true));
        }
        steps
    })
}

/// The codes of each logical modifier, in `[ctrl, shift, alt, win]` order.
const FAMILY_CODES: [&[u32]; 4] = [
    &[vk::CONTROL, vk::LCONTROL, vk::RCONTROL],
    &[vk::SHIFT, vk::LSHIFT, vk::RSHIFT],
    &[vk::MENU, vk::LMENU, vk::RMENU],
    &[vk::LWIN, vk::RWIN],
];

/// A tap of two or more logical modifiers, each reported by one of its codes: the codes in the
/// order they go down, and in the order they come up.
fn modifier_tap() -> impl Strategy<Value = (Vec<u32>, Vec<u32>)> {
    prop::sample::subsequence(FAMILY_CODES.to_vec(), 2..=4)
        .prop_flat_map(|families| {
            families
                .into_iter()
                .map(prop::sample::select)
                .collect::<Vec<_>>()
        })
        .prop_flat_map(|codes| {
            (
                Just(codes.clone()).prop_shuffle(),
                Just(codes).prop_shuffle(),
            )
        })
}

fn any_hotkey() -> impl Strategy<Value = Hotkey> {
    (
        any::<[bool; 4]>(),
        prop_oneof![
            Just(HotkeyKey::ModifiersOnly),
            key_code().prop_map(HotkeyKey::VirtualKey),
        ],
    )
        .prop_map(|([ctrl, shift, alt, win], key)| Hotkey {
            ctrl,
            shift,
            alt,
            win,
            key,
        })
}

fn hotkey_config() -> impl Strategy<Value = HotkeyConfig> {
    prop_oneof![
        Just(HotkeyConfig::default()),
        (vec(any_hotkey(), 0..4), vec(any_hotkey(), 0..4)).prop_map(|(toggle_mode, hard_reset)| {
            HotkeyConfig {
                toggle_mode,
                hard_reset,
            }
        }),
    ]
}

/// Hotkeys over the dense keys: modifier-only, F12, or Space with any modifiers.
fn dense_hotkey_config() -> impl Strategy<Value = HotkeyConfig> {
    let hotkey = (
        any::<[bool; 4]>(),
        prop_oneof![
            Just(HotkeyKey::ModifiersOnly),
            Just(HotkeyKey::VirtualKey(vk::F12)),
            Just(HotkeyKey::VirtualKey(vk::SPACE)),
        ],
    )
        .prop_map(|([ctrl, shift, alt, win], key)| Hotkey {
            ctrl,
            shift,
            alt,
            win,
            key,
        });
    prop_oneof![
        Just(HotkeyConfig::default()),
        (vec(hotkey.clone(), 0..4), vec(hotkey, 0..4)).prop_map(|(toggle_mode, hard_reset)| {
            HotkeyConfig {
                toggle_mode,
                hard_reset,
            }
        }),
    ]
}

/// Logical modifier as `[ctrl, shift, alt, win]` index, and whether the code is generic.
fn family(code: u32) -> Option<(usize, bool)> {
    match code {
        vk::CONTROL => Some((0, true)),
        vk::LCONTROL | vk::RCONTROL => Some((0, false)),
        vk::SHIFT => Some((1, true)),
        vk::LSHIFT | vk::RSHIFT => Some((1, false)),
        vk::MENU => Some((2, true)),
        vk::LMENU | vk::RMENU => Some((2, false)),
        vk::LWIN | vk::RWIN => Some((3, false)),
        _ => None,
    }
}

const GENERIC: [Option<u32>; 4] = [Some(vk::CONTROL), Some(vk::SHIFT), Some(vk::MENU), None];

fn mods_of(hotkey: &Hotkey) -> [bool; 4] {
    [hotkey.ctrl, hotkey.shift, hotkey.alt, hotkey.win]
}

/// A hotkey the text parser would accept.
fn parser_valid(hotkey: &Hotkey) -> bool {
    let count = mods_of(hotkey).iter().filter(|m| **m).count();
    match hotkey.key {
        HotkeyKey::ModifiersOnly => count >= 2,
        HotkeyKey::VirtualKey(code) => {
            count >= 1 && (1..=0xFE).contains(&code) && !vk::is_modifier(code)
        }
    }
}

/// First configured action for `mods` + `key`, toggle list first.
fn configured(config: &HotkeyConfig, mods: [bool; 4], key: HotkeyKey) -> Option<HotkeyAction> {
    config
        .toggle_mode
        .iter()
        .map(|h| (h, HotkeyAction::ToggleMode))
        .chain(
            config
                .hard_reset
                .iter()
                .map(|h| (h, HotkeyAction::HardReset)),
        )
        .find(|(h, _)| parser_valid(h) && h.key == key && mods_of(h) == mods)
        .map(|(_, action)| action)
}

/// Independent model of the documented classification rules, keeping held keys as a set of
/// codes.
#[derive(Default)]
struct Model {
    altgr_layout: bool,
    held: BTreeSet<u32>,
    /// Logical modifiers held at any point of the current hold.
    seen: [bool; 4],
    /// Logical modifiers held right after each modifier key-down of the current hold.
    held_after_downs: Vec<[bool; 4]>,
    spoiled: bool,
    consumed: BTreeSet<u32>,
    last_down: Option<u32>,
}

impl Model {
    fn logical(&self) -> [bool; 4] {
        let mut mods = [false; 4];
        for code in &self.held {
            if self.altgr_layout && *code == vk::RMENU {
                continue;
            }
            if let Some((index, _)) = family(*code) {
                mods[index] = true;
            }
        }
        mods
    }

    fn altgr_held(&self) -> bool {
        self.altgr_layout && self.held.contains(&vk::RMENU)
    }

    /// The verdict for the key-up that releases the last held modifier: the modifier-only
    /// hotkey configured for exactly the modifiers seen, when all of them were down at the same
    /// time and nothing spoiled the hold.
    fn hold_end(&self, config: &HotkeyConfig) -> KeyVerdict {
        let together = self.held_after_downs.contains(&self.seen);
        if self.spoiled || !together {
            return KeyVerdict::Pass;
        }
        configured(config, self.seen, HotkeyKey::ModifiersOnly).map_or(KeyVerdict::Pass, |action| {
            KeyVerdict::Hotkey(HotkeyMatch {
                action,
                consume: false,
            })
        })
    }
}

/// Facts about the event stream, tracked independently of the classification rules.
#[derive(Default)]
struct Stream {
    /// Per key: whether its latest key-down was withheld from the application.
    latest_down_withheld: BTreeMap<u32, bool>,
    /// The last key that went down while it stays down, forgotten on reset like the classifier.
    last_down: Option<u32>,
    /// Right Alt is held; on an AltGr layout that makes every key-down an AltGr chord.
    right_alt_down: bool,
}

/// Feeds `steps` to a classifier and checks every verdict against the model and the stream
/// invariants.
fn check_classifier(config: &HotkeyConfig, steps: &[Step]) -> Result<(), TestCaseError> {
    let mut classifier = HotkeyClassifier::new(config);
    let mut model = Model::default();
    let mut stream = Stream::default();
    let mut last_fed: Option<u32> = None;
    for &step in steps {
        let (code, down) = match step {
            Step::Key(code, down) => (code, down),
            Step::Repeat => match last_fed {
                Some(code) => (code, true),
                None => continue,
            },
            Step::Reset => {
                classifier.reset();
                model = Model {
                    altgr_layout: model.altgr_layout,
                    ..Model::default()
                };
                stream.last_down = None;
                stream.right_alt_down = false;
                continue;
            }
            Step::Interrupt => {
                classifier.interrupt();
                if !model.held.is_empty() {
                    model.spoiled = true;
                }
                continue;
            }
            Step::AltGrLayout(on) => {
                classifier.set_altgr_layout(on);
                prop_assert_eq!(classifier.altgr_layout(), on);
                model.altgr_layout = on;
                continue;
            }
        };
        let altgr_chord = model.altgr_layout && stream.right_alt_down;
        let result = classifier.classify(code, down);

        // Stream invariants.
        if family(code).is_some() {
            prop_assert!(
                !result.consumes(),
                "modifier events always reach the application"
            );
        }
        if down {
            let repeat = stream.last_down == Some(code);
            if repeat {
                prop_assert_eq!(result.hotkey(), None, "an auto-repeat never fires");
            }
            if result == KeyVerdict::Swallow {
                prop_assert!(repeat);
                prop_assert_eq!(stream.latest_down_withheld.get(&code), Some(&true));
            }
            if altgr_chord && family(code).is_none() {
                prop_assert_eq!(result.hotkey(), None, "AltGr chords type characters");
            }
            stream.latest_down_withheld.insert(code, result.consumes());
            stream.last_down = Some(code);
            if code == vk::RMENU {
                stream.right_alt_down = true;
            }
            last_fed = Some(code);
        } else {
            if result.consumes() {
                prop_assert_eq!(
                    stream.latest_down_withheld.get(&code),
                    Some(&true),
                    "a key-up is withheld only after a withheld key-down"
                );
            }
            stream.latest_down_withheld.remove(&code);
            if stream.last_down == Some(code) {
                stream.last_down = None;
            }
            if code == vk::RMENU || code == vk::MENU {
                stream.right_alt_down = false;
            }
        }

        // Model.
        match (family(code), down) {
            (Some(_), true) => {
                prop_assert_eq!(result, KeyVerdict::Pass);
                if model.held.is_empty() {
                    model.seen = [false; 4];
                    model.held_after_downs.clear();
                    model.spoiled = false;
                }
                model.held.insert(code);
                if model.altgr_layout && code == vk::RMENU {
                    model.spoiled = true;
                }
                let now = model.logical();
                for (seen, held) in model.seen.iter_mut().zip(now) {
                    *seen |= held;
                }
                model.held_after_downs.push(now);
            }
            (Some((index, generic)), false) => {
                let was_held = !model.held.is_empty();
                if generic {
                    model
                        .held
                        .retain(|c| family(*c).map(|(i, _)| i) != Some(index));
                } else {
                    model.held.remove(&code);
                    if let Some(Some(generic_code)) = GENERIC.get(index) {
                        model.held.remove(generic_code);
                    }
                }
                let expected = if was_held && model.held.is_empty() {
                    model.hold_end(config)
                } else {
                    KeyVerdict::Pass
                };
                prop_assert_eq!(
                    result,
                    expected,
                    "a modifier-only hotkey fires exactly when the hold ends"
                );
            }
            (None, true) => {
                if !model.held.is_empty() {
                    model.spoiled = true;
                }
                let repeat = model.last_down == Some(code);
                let expected = if repeat && model.consumed.contains(&code) {
                    KeyVerdict::Swallow
                } else {
                    model.consumed.remove(&code);
                    let action = if repeat || model.altgr_held() {
                        None
                    } else {
                        configured(config, model.logical(), HotkeyKey::VirtualKey(code))
                    };
                    match action {
                        Some(action) => {
                            model.consumed.insert(code);
                            KeyVerdict::Hotkey(HotkeyMatch {
                                action,
                                consume: true,
                            })
                        }
                        None => KeyVerdict::Pass,
                    }
                };
                prop_assert_eq!(result, expected);
            }
            (None, false) => {
                let expected = if model.consumed.remove(&code) {
                    KeyVerdict::Swallow
                } else {
                    KeyVerdict::Pass
                };
                prop_assert_eq!(result, expected);
            }
        }
        if down {
            model.last_down = Some(code);
        } else if model.last_down == Some(code) {
            model.last_down = None;
        }
    }
    Ok(())
}

proptest! {
    #[test]
    fn classifier_follows_the_model_on_arbitrary_streams(
        config in hotkey_config(),
        steps in steps(key_code(), 14, 0..64),
    ) {
        check_classifier(&config, &steps)?;
    }

    #[test]
    fn classifier_follows_the_model_on_dense_hotkey_streams(
        config in dense_hotkey_config(),
        steps in with_altgr_start(steps(dense_key_code(), 14, 0..96)),
    ) {
        check_classifier(&config, &steps)?;
    }

    #[test]
    fn classifier_follows_the_model_on_modifier_taps(
        config in dense_hotkey_config(),
        steps in with_altgr_start(steps(prop::sample::select(&MODIFIER_TAP_KEYS[..]), 10, 0..48)),
    ) {
        check_classifier(&config, &steps)?;
    }

    /// Matrix #24 for every modifier-only hotkey: a tap of exactly its modifiers fires it on the
    /// last release, whichever generic or side codes report them and in whatever order they go
    /// down and come up. Only on an AltGr layout is a right Alt in the tap AltGr, which spoils
    /// the tap.
    #[test]
    fn a_tap_of_exactly_the_configured_modifiers_fires_on_the_last_release(
        (presses, releases) in modifier_tap(),
        altgr_layout in any::<bool>(),
    ) {
        let mut mods = [false; 4];
        for (index, _) in presses.iter().filter_map(|&code| family(code)) {
            mods[index] = true;
        }
        let [ctrl, shift, alt, win] = mods;
        let mut classifier = HotkeyClassifier::new(&HotkeyConfig {
            toggle_mode: vec![Hotkey { ctrl, shift, alt, win, key: HotkeyKey::ModifiersOnly }],
            hard_reset: Vec::new(),
        });
        classifier.set_altgr_layout(altgr_layout);
        let mut verdicts: Vec<KeyVerdict> =
            presses.iter().map(|&code| classifier.classify(code, true)).collect();
        verdicts.extend(releases.iter().map(|&code| classifier.classify(code, false)));

        let mut expected = vec![KeyVerdict::Pass; presses.len() + releases.len()];
        if !(altgr_layout && presses.contains(&vk::RMENU))
            && let Some(last) = expected.last_mut()
        {
            *last = KeyVerdict::Hotkey(HotkeyMatch {
                action: HotkeyAction::ToggleMode,
                consume: false,
            });
        }
        prop_assert_eq!(verdicts, expected);
    }

    #[test]
    fn on_key_reports_the_hotkey_of_classify(
        config in hotkey_config(),
        events in vec((key_code(), any::<bool>()), 0..48),
    ) {
        let mut by_verdict = HotkeyClassifier::new(&config);
        let mut by_match = HotkeyClassifier::new(&config);
        for (code, down) in events {
            prop_assert_eq!(
                by_match.on_key(code, down),
                by_verdict.classify(code, down).hotkey()
            );
        }
    }
}
