//! Host policy resolution (plan §6; matrix B26, B27, B28, B29).

use std::sync::Arc;

use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, HostPolicy, NamedSet, PROBE_VERSION, ProbeRecord, RuleAction,
    SecureFieldMode, Transport,
};
use raumakey_policy::CompatibilityPolicy;

const CHROMIUM_FAMILY: [&str; 9] = [
    "msedge.exe",
    "chrome.exe",
    "brave.exe",
    "vivaldi.exe",
    "opera.exe",
    "ms-teams.exe",
    "teams.exe",
    "slack.exe",
    "discord.exe",
];

const TERMINALS: [&str; 4] = [
    "windowsterminal.exe",
    "wt.exe",
    "openconsole.exe",
    "conhost.exe",
];

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn host(name: &str) -> HostContext {
    HostContext::new(SessionId::from_u128(7), exe(name))
}

fn host_with_class(name: &str, class: &str) -> HostContext {
    let mut h = host(name);
    h.window_class = Some(class.to_owned());
    h
}

fn policy(config: CompatibilityConfig) -> CompatibilityPolicy {
    CompatibilityPolicy::new(Arc::new(config))
}

fn rule(name: &str, class: Option<&str>, action: RuleAction) -> AppRule {
    AppRule {
        exe: exe(name),
        window_class: class.map(str::to_owned),
        action,
    }
}

fn set(name: &str, members: &[&str], action: RuleAction) -> NamedSet {
    NamedSet {
        name: name.to_owned(),
        members: members.iter().map(|m| exe(m)).collect(),
        action,
    }
}

fn named_set_mut<'a>(config: &'a mut CompatibilityConfig, name: &str) -> &'a mut NamedSet {
    config
        .named_sets
        .iter_mut()
        .find(|s| s.name == name)
        .unwrap()
}

#[test]
fn generic_host_gets_tsf_with_the_configured_defaults() {
    let p = policy(CompatibilityConfig::defaults_v1());
    assert_eq!(p.resolve(&host("notepad.exe")), HostPolicy::default_tsf());

    let mut config = CompatibilityConfig::defaults_v1();
    config.default_reset_on_mouse = false;
    config.default_max_delete_before = 100;
    config.secure_field_mode = SecureFieldMode::Restricted;
    let resolved = policy(config).resolve(&host("notepad.exe"));
    assert_eq!(
        resolved,
        HostPolicy {
            transport: Transport::Tsf,
            reset_on_mouse: false,
            permit_replay: true,
            max_delete_before: 100,
            macro_allowed: true,
            secure_field_mode: SecureFieldMode::Restricted,
            vietnamese_allowed: true,
        }
    );
}

/// Matrix #26: every chromium-family member gets the mouse reset and otherwise the generic policy.
#[test]
fn b26_each_chromium_family_member_gets_mouse_reset_and_otherwise_generic_policy() {
    for default_reset in [true, false] {
        let mut config = CompatibilityConfig::defaults_v1();
        config.default_reset_on_mouse = default_reset;
        let p = policy(config);
        let generic = p.resolve(&host("notepad.exe"));
        assert_eq!(generic.reset_on_mouse, default_reset);
        for member in CHROMIUM_FAMILY.iter().chain(TERMINALS.iter()) {
            let expected = HostPolicy {
                reset_on_mouse: true,
                ..generic
            };
            assert_eq!(p.resolve(&host(member)), expected, "{member}");
        }
    }
}

#[test]
fn b26_member_names_match_case_insensitively() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.default_reset_on_mouse = false;
    let p = policy(config);
    assert!(p.resolve(&host("MsEdge.EXE")).reset_on_mouse);
    assert!(!p.resolve(&host("msedge2.exe")).reset_on_mouse);
}

/// Matrix #28: the games set is empty by default and does not act as a blacklist.
#[test]
fn b28_games_set_is_empty_and_inert_by_default() {
    let p = policy(CompatibilityConfig::defaults_v1());
    for name in ["game.exe", "steam.exe", "notepad.exe"] {
        assert_eq!(p.resolve(&host(name)), HostPolicy::default_tsf(), "{name}");
    }
}

/// Matrix #28: an action given to one named set affects only that set's members.
#[test]
fn b28_named_set_action_affects_only_its_members_and_sets_stay_separate() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.default_reset_on_mouse = false;
    let games = named_set_mut(&mut config, "games");
    games.members = vec![exe("game.exe")];
    games.action = RuleAction {
        vietnamese: Some(false),
        macros: Some(false),
        ..RuleAction::default()
    };
    let p = policy(config);

    let game = p.resolve(&host("game.exe"));
    assert!(!game.vietnamese_allowed);
    assert!(!game.macro_allowed);
    assert!(
        !game.reset_on_mouse,
        "games does not inherit chromium's action"
    );

    let chrome = p.resolve(&host("chrome.exe"));
    assert!(chrome.vietnamese_allowed);
    assert!(chrome.macro_allowed);
    assert!(chrome.reset_on_mouse);

    let terminal = p.resolve(&host("wt.exe"));
    assert!(terminal.vietnamese_allowed);
    assert!(terminal.reset_on_mouse);

    assert_eq!(
        p.resolve(&host("notepad.exe")),
        HostPolicy {
            reset_on_mouse: false,
            ..HostPolicy::default_tsf()
        }
    );
}

#[test]
fn b28_changing_one_set_does_not_change_another() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.default_reset_on_mouse = false;
    named_set_mut(&mut config, "terminals").action = RuleAction {
        reset_on_mouse: Some(false),
        ..RuleAction::default()
    };
    let p = policy(config);
    assert!(!p.resolve(&host("wt.exe")).reset_on_mouse);
    assert!(p.resolve(&host("chrome.exe")).reset_on_mouse);
}

#[test]
fn later_named_sets_do_not_override_earlier_explicit_values() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.named_sets = vec![
        set(
            "first",
            &["x.exe"],
            RuleAction {
                reset_on_mouse: Some(false),
                max_delete_before: Some(5),
                ..RuleAction::default()
            },
        ),
        set(
            "second",
            &["x.exe"],
            RuleAction {
                reset_on_mouse: Some(true),
                max_delete_before: Some(50),
                vietnamese: Some(false),
                ..RuleAction::default()
            },
        ),
    ];
    let resolved = policy(config).resolve(&host("x.exe"));
    assert!(!resolved.reset_on_mouse);
    assert_eq!(resolved.max_delete_before, 5);
    assert!(
        !resolved.vietnamese_allowed,
        "a later set still fills fields that earlier sets leave unset"
    );
}

#[test]
fn transport_precedence_is_class_rule_then_exe_rule_then_named_set() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.clipboard_enabled = true;
    config.rules = vec![
        rule(
            "editor.exe",
            None,
            RuleAction {
                transport: Some(Transport::SyntheticKey),
                ..RuleAction::default()
            },
        ),
        rule(
            "editor.exe",
            Some("RichEdit"),
            RuleAction {
                transport: Some(Transport::Tsf),
                ..RuleAction::default()
            },
        ),
    ];
    config.named_sets.push(set(
        "pasters",
        &["editor.exe"],
        RuleAction {
            transport: Some(Transport::Clipboard),
            ..RuleAction::default()
        },
    ));
    let p = policy(config);
    assert_eq!(
        p.resolve(&host_with_class("editor.exe", "RichEdit"))
            .transport,
        Transport::Tsf
    );
    assert_eq!(
        p.resolve(&host_with_class("editor.exe", "Edit")).transport,
        Transport::SyntheticKey
    );
    assert_eq!(
        p.resolve(&host("editor.exe")).transport,
        Transport::SyntheticKey
    );
}

#[test]
fn an_earlier_named_set_decides_the_transport() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.clipboard_enabled = true;
    config.named_sets = vec![
        set(
            "first",
            &["x.exe"],
            RuleAction {
                transport: Some(Transport::SyntheticKey),
                ..RuleAction::default()
            },
        ),
        set(
            "second",
            &["x.exe"],
            RuleAction {
                transport: Some(Transport::Clipboard),
                ..RuleAction::default()
            },
        ),
    ];
    assert_eq!(
        policy(config).resolve(&host("x.exe")).transport,
        Transport::SyntheticKey
    );
}

#[test]
fn macro_precedence_follows_rule_specificity_and_set_order() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.rules.push(rule(
        "writer.exe",
        None,
        RuleAction {
            macros: Some(true),
            ..RuleAction::default()
        },
    ));
    config.named_sets = vec![
        set(
            "no-macros",
            &["writer.exe", "both.exe"],
            RuleAction {
                macros: Some(false),
                ..RuleAction::default()
            },
        ),
        set(
            "macros",
            &["writer.exe", "both.exe"],
            RuleAction {
                macros: Some(true),
                ..RuleAction::default()
            },
        ),
    ];
    let p = policy(config);
    assert!(
        p.resolve(&host("writer.exe")).macro_allowed,
        "an app rule beats named sets"
    );
    assert!(
        !p.resolve(&host("both.exe")).macro_allowed,
        "an earlier named set beats a later one"
    );
}

#[test]
fn app_rules_override_named_sets() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.rules.push(rule(
        "chrome.exe",
        None,
        RuleAction {
            reset_on_mouse: Some(false),
            ..RuleAction::default()
        },
    ));
    let p = policy(config);
    assert!(!p.resolve(&host("chrome.exe")).reset_on_mouse);
    assert!(p.resolve(&host("msedge.exe")).reset_on_mouse);
}

#[test]
fn app_rule_fields_apply_only_to_their_executable() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.rules.push(rule(
        "notepad.exe",
        None,
        RuleAction {
            transport: Some(Transport::SyntheticKey),
            reset_on_mouse: Some(false),
            macros: Some(false),
            vietnamese: Some(false),
            max_delete_before: Some(9),
        },
    ));
    let p = policy(config);
    assert_eq!(
        p.resolve(&host("notepad.exe")),
        HostPolicy {
            transport: Transport::SyntheticKey,
            reset_on_mouse: false,
            permit_replay: true,
            max_delete_before: 9,
            macro_allowed: false,
            secure_field_mode: SecureFieldMode::Bypass,
            vietnamese_allowed: false,
        }
    );
    assert_eq!(p.resolve(&host("wordpad.exe")), HostPolicy::default_tsf());
}

#[test]
fn max_delete_before_override() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.default_max_delete_before = 100;
    config.rules.push(rule(
        "legacy.exe",
        None,
        RuleAction {
            max_delete_before: Some(8),
            ..RuleAction::default()
        },
    ));
    config.rules.push(rule(
        "zero.exe",
        None,
        RuleAction {
            max_delete_before: Some(0),
            ..RuleAction::default()
        },
    ));
    let p = policy(config);
    assert_eq!(p.resolve(&host("legacy.exe")).max_delete_before, 8);
    assert_eq!(p.resolve(&host("zero.exe")).max_delete_before, 0);
    assert_eq!(p.resolve(&host("notepad.exe")).max_delete_before, 100);
}

#[test]
fn window_class_rule_beats_exe_only_rule_in_either_declaration_order() {
    let exe_only = rule(
        "notepad.exe",
        None,
        RuleAction {
            vietnamese: Some(false),
            max_delete_before: Some(10),
            ..RuleAction::default()
        },
    );
    let class_rule = rule(
        "notepad.exe",
        Some("RichEditD2DPT"),
        RuleAction {
            vietnamese: Some(true),
            ..RuleAction::default()
        },
    );
    for rules in [
        vec![exe_only.clone(), class_rule.clone()],
        vec![class_rule, exe_only],
    ] {
        let mut config = CompatibilityConfig::defaults_v1();
        config.rules = rules;
        let p = policy(config);

        let in_class = p.resolve(&host_with_class("notepad.exe", "RichEditD2DPT"));
        assert!(in_class.vietnamese_allowed);
        assert_eq!(
            in_class.max_delete_before, 10,
            "unset class-rule fields inherit from the exe-only rule"
        );

        let other_class = p.resolve(&host_with_class("notepad.exe", "Edit"));
        assert!(!other_class.vietnamese_allowed);

        let unknown_class = p.resolve(&host("notepad.exe"));
        assert!(!unknown_class.vietnamese_allowed);
    }
}

#[test]
fn window_class_matches_case_insensitively_and_needs_the_exe() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.rules.push(rule(
        "notepad.exe",
        Some("Edit"),
        RuleAction {
            macros: Some(false),
            ..RuleAction::default()
        },
    ));
    let p = policy(config);
    assert!(
        !p.resolve(&host_with_class("notepad.exe", "EDIT"))
            .macro_allowed
    );
    assert!(
        !p.resolve(&host_with_class("notepad.exe", "edit"))
            .macro_allowed
    );
    assert!(
        p.resolve(&host_with_class("wordpad.exe", "Edit"))
            .macro_allowed
    );
    assert!(
        p.resolve(&host_with_class("notepad.exe", "Edit2"))
            .macro_allowed
    );
}

#[test]
fn among_rules_of_equal_specificity_the_first_declared_wins() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.rules = vec![
        rule(
            "notepad.exe",
            None,
            RuleAction {
                max_delete_before: Some(3),
                ..RuleAction::default()
            },
        ),
        rule(
            "notepad.exe",
            None,
            RuleAction {
                max_delete_before: Some(30),
                macros: Some(false),
                ..RuleAction::default()
            },
        ),
    ];
    let resolved = policy(config).resolve(&host("notepad.exe"));
    assert_eq!(resolved.max_delete_before, 3);
    assert!(!resolved.macro_allowed);
}

fn probe(name: &str, probe_version: u32, tsf_range_edit: bool) -> ProbeRecord {
    ProbeRecord {
        exe: exe(name),
        probe_version,
        tsf_range_edit,
    }
}

fn transport_rule(name: &str, transport: Transport) -> AppRule {
    rule(
        name,
        None,
        RuleAction {
            transport: Some(transport),
            ..RuleAction::default()
        },
    )
}

#[test]
fn host_without_tsf_range_edit_falls_back_to_synthetic_key() {
    let p = policy(CompatibilityConfig::defaults_v1());
    let mut h = host("legacy.exe");
    h.supports_tsf_range_edit = false;
    let resolved = p.resolve(&h);
    assert_eq!(
        resolved,
        HostPolicy {
            transport: Transport::SyntheticKey,
            ..HostPolicy::default_tsf()
        }
    );
}

#[test]
fn explicit_transport_is_kept_when_the_host_lacks_tsf_range_edit() {
    let mut config = CompatibilityConfig::defaults_v1();
    config
        .rules
        .push(transport_rule("legacy.exe", Transport::Tsf));
    config.named_sets.push(set(
        "forced-tsf",
        &["other.exe"],
        RuleAction {
            transport: Some(Transport::Tsf),
            ..RuleAction::default()
        },
    ));
    let p = policy(config);
    for name in ["legacy.exe", "other.exe"] {
        let mut h = host(name);
        h.supports_tsf_range_edit = false;
        assert_eq!(p.resolve(&h).transport, Transport::Tsf, "{name}");
    }
}

/// Matrix #27: a failed probe with the current version selects the synthetic-key transport.
#[test]
fn b27_failed_current_probe_selects_synthetic_key() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = vec![
        probe("legacy.exe", PROBE_VERSION, false),
        probe("good.exe", PROBE_VERSION, true),
    ];
    let p = policy(config);
    assert_eq!(
        p.resolve(&host("legacy.exe")).transport,
        Transport::SyntheticKey
    );
    assert_eq!(p.resolve(&host("good.exe")).transport, Transport::Tsf);
    assert_eq!(p.resolve(&host("other.exe")).transport, Transport::Tsf);
}

#[test]
fn b27_successful_probe_does_not_override_missing_host_capability() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = vec![probe("good.exe", PROBE_VERSION, true)];
    let mut h = host("good.exe");
    h.supports_tsf_range_edit = false;
    assert_eq!(
        policy(config).resolve(&h).transport,
        Transport::SyntheticKey
    );
}

/// Matrix #27: probe records from another probe version are ignored.
#[test]
fn b27_stale_and_future_probe_versions_are_ignored() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = vec![
        probe("old.exe", PROBE_VERSION - 1, false),
        probe("future.exe", PROBE_VERSION + 1, false),
    ];
    let p = policy(config);
    assert_eq!(p.resolve(&host("old.exe")).transport, Transport::Tsf);
    assert_eq!(p.resolve(&host("future.exe")).transport, Transport::Tsf);
}

#[test]
fn explicit_transport_rule_is_not_overridden_by_a_failed_probe() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.clipboard_enabled = true;
    config
        .rules
        .push(transport_rule("forced.exe", Transport::Tsf));
    config
        .rules
        .push(transport_rule("paste.exe", Transport::Clipboard));
    config.probes = vec![
        probe("forced.exe", PROBE_VERSION, false),
        probe("paste.exe", PROBE_VERSION, false),
    ];
    let p = policy(config);
    assert_eq!(p.resolve(&host("forced.exe")).transport, Transport::Tsf);
    assert_eq!(
        p.resolve(&host("paste.exe")).transport,
        Transport::Clipboard
    );
}

#[test]
fn clipboard_transport_requires_clipboard_enabled() {
    let mut config = CompatibilityConfig::defaults_v1();
    config
        .rules
        .push(transport_rule("paste.exe", Transport::Clipboard));
    config.named_sets.push(set(
        "pasters",
        &["paste2.exe"],
        RuleAction {
            transport: Some(Transport::Clipboard),
            ..RuleAction::default()
        },
    ));
    assert!(!config.clipboard_enabled);
    let disabled = policy(config.clone());
    for name in ["paste.exe", "paste2.exe"] {
        assert_eq!(
            disabled.resolve(&host(name)).transport,
            Transport::SyntheticKey,
            "{name}"
        );
    }

    config.clipboard_enabled = true;
    let enabled = policy(config);
    for name in ["paste.exe", "paste2.exe"] {
        assert_eq!(
            enabled.resolve(&host(name)).transport,
            Transport::Clipboard,
            "{name}"
        );
    }
    assert_eq!(
        enabled.resolve(&host("notepad.exe")).transport,
        Transport::Tsf
    );
}

fn secure(mut h: HostContext) -> HostContext {
    h.is_secure_field = true;
    h
}

/// Matrix #29, bypass mode: no transformation, no macros, no clipboard in secure fields.
#[test]
fn b29_secure_field_in_bypass_mode_disables_transformation_macros_and_clipboard() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.clipboard_enabled = true;
    config
        .rules
        .push(transport_rule("paste.exe", Transport::Clipboard));
    let p = policy(config);

    assert_eq!(
        p.resolve(&secure(host("notepad.exe"))),
        HostPolicy {
            macro_allowed: false,
            vietnamese_allowed: false,
            ..HostPolicy::default_tsf()
        }
    );
    let paste = p.resolve(&secure(host("paste.exe")));
    assert_eq!(paste.transport, Transport::SyntheticKey);
    assert!(!paste.vietnamese_allowed);
    assert!(!paste.macro_allowed);
    assert_eq!(paste.secure_field_mode, SecureFieldMode::Bypass);
    assert_eq!(
        p.resolve(&host("paste.exe")).transport,
        Transport::Clipboard
    );
}

/// Matrix #29, restricted mode: transformation allowed, but no macros and no clipboard.
#[test]
fn b29_secure_field_in_restricted_mode_allows_transformation_only() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.secure_field_mode = SecureFieldMode::Restricted;
    config.clipboard_enabled = true;
    config
        .rules
        .push(transport_rule("paste.exe", Transport::Clipboard));
    config.rules.push(rule(
        "english.exe",
        None,
        RuleAction {
            vietnamese: Some(false),
            ..RuleAction::default()
        },
    ));
    let p = policy(config);

    assert_eq!(
        p.resolve(&secure(host("notepad.exe"))),
        HostPolicy {
            macro_allowed: false,
            secure_field_mode: SecureFieldMode::Restricted,
            ..HostPolicy::default_tsf()
        }
    );
    let paste = p.resolve(&secure(host("paste.exe")));
    assert_eq!(paste.transport, Transport::SyntheticKey);
    assert!(paste.vietnamese_allowed);
    assert!(!paste.macro_allowed);
    assert!(
        !p.resolve(&secure(host("english.exe"))).vietnamese_allowed,
        "restricted mode does not re-enable a host the user turned off"
    );
}

/// D25: the secure-field mode decides for secure fields only. `Bypass`, the default, allows
/// neither Vietnamese nor macros there; `Restricted` keeps Vietnamese and still never allows
/// macros; neither ever selects the clipboard. Outside secure fields both modes resolve alike.
#[test]
fn d25_the_secure_field_mode_decides_vietnamese_in_secure_fields_only() {
    assert_eq!(
        CompatibilityConfig::defaults_v1().secure_field_mode,
        SecureFieldMode::Bypass
    );
    for mode in [SecureFieldMode::Bypass, SecureFieldMode::Restricted] {
        let mut config = CompatibilityConfig::defaults_v1();
        config.secure_field_mode = mode;
        config.clipboard_enabled = true;
        config.rules.push(rule(
            "paste.exe",
            None,
            RuleAction {
                transport: Some(Transport::Clipboard),
                macros: Some(true),
                vietnamese: Some(true),
                ..RuleAction::default()
            },
        ));
        let p = policy(config);
        assert_eq!(
            p.resolve(&host("notepad.exe")),
            HostPolicy {
                secure_field_mode: mode,
                ..HostPolicy::default_tsf()
            },
            "{mode:?}"
        );
        for exe in ["notepad.exe", "paste.exe"] {
            let resolved = p.resolve(&secure(host(exe)));
            assert_eq!(
                resolved.vietnamese_allowed,
                mode == SecureFieldMode::Restricted,
                "{mode:?} {exe}"
            );
            assert!(!resolved.macro_allowed, "{mode:?} {exe}");
            assert_ne!(resolved.transport, Transport::Clipboard, "{mode:?} {exe}");
        }
    }
}

#[test]
fn secure_field_overrides_explicit_macro_and_vietnamese_rules() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.rules.push(rule(
        "notepad.exe",
        None,
        RuleAction {
            macros: Some(true),
            vietnamese: Some(true),
            ..RuleAction::default()
        },
    ));
    let resolved = policy(config).resolve(&secure(host("notepad.exe")));
    assert!(!resolved.macro_allowed);
    assert!(!resolved.vietnamese_allowed);
}
