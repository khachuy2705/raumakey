use raumakey_model::diag::{Component, DiagCode, DiagEvent, VectorId};
use raumakey_model::error::{EngineError, ResetReason};
use raumakey_model::host::ExeName;
use raumakey_model::hotkey::{Hotkey, HotkeyConfig, HotkeyKey};
use raumakey_model::input::vk;
use raumakey_model::macros::{
    Boundary, BoundaryPolicy, CaseMode, CasePolicy, MacroEntry, MacroId, MacroLookup, NoMacros,
};
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, HostPolicy, PROBE_VERSION, ProbeRecord, RuleAction,
    SecureFieldMode, Transport,
};

#[test]
fn policy_enums_have_stable_names() {
    assert_eq!(
        serde_json::to_string(&Transport::SyntheticKey).unwrap(),
        "\"synthetic-key\""
    );
    assert_eq!(serde_json::to_string(&Transport::Tsf).unwrap(), "\"tsf\"");
    assert_eq!(
        serde_json::to_string(&SecureFieldMode::Restricted).unwrap(),
        "\"restricted\""
    );
}

#[test]
fn compatibility_defaults_v1_keep_named_sets_separate() {
    let c = CompatibilityConfig::defaults_v1();
    assert!(!c.clipboard_enabled);
    assert!(c.default_reset_on_mouse);
    assert_eq!(c.secure_field_mode, SecureFieldMode::Bypass);
    assert_eq!(c.default_max_delete_before, 64);
    let names: Vec<&str> = c.named_sets.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["chromium-family", "terminals", "games"]);
    let chromium = &c.named_sets[0];
    assert!(
        chromium
            .members
            .contains(&ExeName::new("msedge.exe").unwrap())
    );
    assert!(
        chromium
            .members
            .contains(&ExeName::new("chrome.exe").unwrap())
    );
    assert_eq!(chromium.action.reset_on_mouse, Some(true));
    assert_eq!(chromium.action.transport, None);
    let games = &c.named_sets[2];
    assert!(games.members.is_empty());
    assert_eq!(games.action, RuleAction::default());
    assert!(c.rules.is_empty());
    assert!(c.probes.is_empty());
    assert_eq!(PROBE_VERSION, 1);
}

#[test]
fn compatibility_config_toml_round_trip() {
    let mut c = CompatibilityConfig::defaults_v1();
    c.rules.push(AppRule {
        exe: ExeName::new("notepad.exe").unwrap(),
        window_class: Some("Notepad".into()),
        action: RuleAction {
            transport: Some(Transport::SyntheticKey),
            vietnamese: Some(false),
            ..RuleAction::default()
        },
    });
    c.probes.push(ProbeRecord {
        exe: ExeName::new("legacy.exe").unwrap(),
        probe_version: PROBE_VERSION,
        tsf_range_edit: false,
    });
    let text = toml::to_string(&c).unwrap();
    let back: CompatibilityConfig = toml::from_str(&text).unwrap();
    assert_eq!(back, c);
}

#[test]
fn host_policy_default_is_tsf() {
    let p = HostPolicy::default_tsf();
    assert_eq!(p.transport, Transport::Tsf);
    assert!(p.reset_on_mouse);
    assert!(p.permit_replay);
    assert_eq!(p.max_delete_before, 64);
    assert!(p.macro_allowed);
    assert_eq!(p.secure_field_mode, SecureFieldMode::Bypass);
    assert!(p.vietnamese_allowed);
}

#[test]
fn macro_entry_defaults_when_reading() {
    let text = "id = 3\ntrigger = \"ko\"\nreplacement = \"không\"\n";
    let e: MacroEntry = toml::from_str(text).unwrap();
    assert_eq!(e.id, MacroId(3));
    assert!(e.enabled);
    assert_eq!(e.boundary, BoundaryPolicy::Delimiter);
    assert_eq!(e.case_policy, CasePolicy::Adapt);
    let out = toml::to_string(&e).unwrap();
    assert!(out.contains("case = \"adapt\""));
    assert!(out.contains("boundary = \"delimiter\""));
}

#[test]
fn no_macros_never_expands() {
    assert!(
        NoMacros
            .lookup("ko", Boundary::Whitespace, CaseMode::Adaptive)
            .is_none()
    );
}

/// D28: a lookup that does not implement the token queries has no token triggers, so the
/// engine never tracks tokens for it.
#[test]
fn lookups_have_no_token_triggers_by_default() {
    assert!(!NoMacros.has_token_triggers());
    assert!(!NoMacros.is_trigger_prefix("v.", CaseMode::Adaptive));
}

#[test]
fn hotkeys_parse_and_display_canonically() {
    let toggle: Hotkey = "Ctrl+Shift".parse().unwrap();
    assert_eq!(toggle.key, HotkeyKey::ModifiersOnly);
    assert!(toggle.ctrl && toggle.shift && !toggle.alt && !toggle.win);
    assert_eq!(toggle.to_string(), "Ctrl+Shift");

    let reset: Hotkey = "ctrl+alt+shift+f12".parse().unwrap();
    assert_eq!(reset.key, HotkeyKey::VirtualKey(vk::F12));
    assert_eq!(reset.to_string(), "Ctrl+Shift+Alt+F12");

    let space: Hotkey = "Control+Shift+Alt+Space".parse().unwrap();
    assert_eq!(space.key, HotkeyKey::VirtualKey(vk::SPACE));
    assert_eq!(space.to_string(), "Ctrl+Shift+Alt+Space");

    let alt_z: Hotkey = "Alt+Z".parse().unwrap();
    assert_eq!(alt_z.key, HotkeyKey::VirtualKey(0x5A));
    assert_eq!(alt_z.to_string(), "Alt+Z");
}

#[test]
fn hotkeys_reject_unsafe_combinations() {
    assert!("Shift".parse::<Hotkey>().is_err());
    assert!("Q".parse::<Hotkey>().is_err());
    assert!("Ctrl+Foo".parse::<Hotkey>().is_err());
    assert!("Ctrl+Ctrl".parse::<Hotkey>().is_err());
    assert!("".parse::<Hotkey>().is_err());
    assert!("Ctrl+A+B".parse::<Hotkey>().is_err());
}

#[test]
fn hotkey_defaults_d5() {
    let d = HotkeyConfig::default();
    let toggle: Vec<String> = d.toggle_mode.iter().map(Hotkey::to_string).collect();
    let reset: Vec<String> = d.hard_reset.iter().map(Hotkey::to_string).collect();
    assert_eq!(toggle, ["Ctrl+Shift"]);
    assert_eq!(reset, ["Ctrl+Shift+Alt+F12", "Ctrl+Shift+Alt+Space"]);
    let text = toml::to_string(&d).unwrap();
    assert_eq!(toml::from_str::<HotkeyConfig>(&text).unwrap(), d);
}

#[test]
fn diag_lines_contain_only_typed_fields() {
    let e = DiagEvent {
        timestamp_ms: 1234,
        component: Component::Tsf,
        code: DiagCode(42),
        revision: Some(7),
        host_exe: Some(ExeName::new("notepad.exe").unwrap()),
        vector_id: Some(VectorId::new("A01").unwrap()),
    };
    assert_eq!(e.to_log_line(), "1234 tsf 42 rev=7 exe=notepad.exe vec=A01");
    let bare = DiagEvent {
        timestamp_ms: 1,
        component: Component::Engine,
        code: DiagCode(1),
        revision: None,
        host_exe: None,
        vector_id: None,
    };
    assert_eq!(bare.to_log_line(), "1 engine 1 rev=- exe=- vec=-");
    assert!(VectorId::new("bad id").is_err());
    assert!(VectorId::new("").is_err());
    assert!(VectorId::new(&"a".repeat(33)).is_err());
}

#[test]
fn errors_display() {
    for e in [
        EngineError::InvalidUtf16,
        EngineError::InvalidStateVersion,
        EngineError::ContextMismatch,
        EngineError::CapacityExceeded,
        EngineError::UnsupportedInputMethod,
        EngineError::InternalInvariant,
    ] {
        assert!(!e.to_string().is_empty());
    }
    assert_ne!(ResetReason::Mouse, ResetReason::Focus);
}
