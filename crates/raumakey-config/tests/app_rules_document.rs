use raumakey_config::{AppRulesDocument, ConfigError, FieldIssue, IssueReason, StoreDocument};
use raumakey_model::host::ExeName;
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, NamedSet, ProbeRecord, RuleAction, SecureFieldMode, Transport,
};

/// The example of plan §5.1 for `raumakey-app-rules.toml` (`app_rules.toml` in the plan),
/// verbatim.
const PLAN_EXAMPLE: &str = r#"version = 1
revision = 1
clipboard_enabled = false
default_reset_on_mouse = true
secure_field_mode = "bypass"     # bypass | restricted
default_max_delete_before = 64
[[rule]]
exe = "notepad.exe"
window_class = "Notepad"         # optional
transport = "synthetic-key"      # tsf | synthetic-key | clipboard (all action fields optional)
reset_on_mouse = true
macros = true
vietnamese = false
max_delete_before = 64
[[named_set]]
name = "chromium-family"
members = ["msedge.exe", "chrome.exe"]
action = { reset_on_mouse = true }
[[probe]]
exe = "legacy.exe"
probe_version = 1
tsf_range_edit = false
"#;

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn issue(path: &str, reason: IssueReason) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason,
    }
}

fn plan_example_compatibility() -> CompatibilityConfig {
    CompatibilityConfig {
        clipboard_enabled: false,
        default_reset_on_mouse: true,
        secure_field_mode: SecureFieldMode::Bypass,
        default_max_delete_before: 64,
        rules: vec![AppRule {
            exe: exe("notepad.exe"),
            window_class: Some("Notepad".into()),
            action: RuleAction {
                transport: Some(Transport::SyntheticKey),
                reset_on_mouse: Some(true),
                macros: Some(true),
                vietnamese: Some(false),
                max_delete_before: Some(64),
                autocomplete_guard: None,
            },
        }],
        named_sets: vec![NamedSet {
            name: "chromium-family".into(),
            members: vec![exe("msedge.exe"), exe("chrome.exe")],
            action: RuleAction {
                reset_on_mouse: Some(true),
                ..RuleAction::default()
            },
        }],
        probes: vec![ProbeRecord {
            exe: exe("legacy.exe"),
            probe_version: 1,
            tsf_range_edit: false,
        }],
    }
}

#[test]
fn defaults_wrap_compatibility_defaults_v1() {
    let doc = AppRulesDocument::default_document();
    assert_eq!(doc.version, 1);
    assert_eq!(doc.revision, 0);
    assert_eq!(doc.compatibility, CompatibilityConfig::defaults_v1());
    assert!(doc.extensions.is_empty());
    assert_eq!(AppRulesDocument::default(), doc);
    assert_eq!(AppRulesDocument::FILE_NAME, "raumakey-app-rules.toml");
    assert_eq!(AppRulesDocument::CURRENT_VERSION, 1);
}

#[test]
fn plan_example_decodes_flattened_at_top_level() {
    let loaded = AppRulesDocument::decode(PLAN_EXAMPLE).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    assert_eq!(loaded.document.revision, 1);
    assert_eq!(loaded.document.compatibility, plan_example_compatibility());
    assert!(loaded.document.extensions.is_empty());
}

#[test]
fn plan_example_round_trips() {
    let doc = AppRulesDocument::decode(PLAN_EXAMPLE).unwrap().document;
    let text = doc.encode().unwrap();
    assert!(text.starts_with("version = 1\nrevision = 1\n"), "{text}");
    for header in ["[[rule]]", "[[named_set]]", "[[probe]]"] {
        assert!(text.contains(header), "{text}");
    }
    let back = AppRulesDocument::decode(&text).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, doc);
}

#[test]
fn defaults_round_trip_including_an_empty_named_set() {
    let doc = AppRulesDocument::default_document();
    let back = AppRulesDocument::decode(&doc.encode().unwrap()).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, doc);
}

#[test]
fn missing_fields_take_model_defaults() {
    let loaded = AppRulesDocument::decode("version = 1\n").unwrap();
    assert!(loaded.issues.is_empty());
    let compatibility = loaded.document.compatibility;
    assert!(!compatibility.clipboard_enabled);
    assert!(compatibility.default_reset_on_mouse);
    assert_eq!(compatibility.secure_field_mode, SecureFieldMode::Bypass);
    assert_eq!(compatibility.default_max_delete_before, 64);
    assert!(compatibility.rules.is_empty());
    assert!(compatibility.named_sets.is_empty());
    assert!(compatibility.probes.is_empty());
}

/// Matrix #45 for application rules.
#[test]
fn matrix_45_corrupt_fields_and_entries_are_isolated() {
    let text = r#"version = 1
revision = 3
clipboard_enabled = true
secure_field_mode = "paranoid"
default_max_delete_before = -5
future_flag = "kept"
[[rule]]
exe = "word.exe"
transport = "tsf"
[[rule]]
exe = "C:/path/not-a-basename.exe"
transport = "clipboard"
[[rule]]
exe = "excel.exe"
transport = "teleport"
[[named_set]]
name = "editors"
members = ["code.exe"]
action = { macros = false }
[[probe]]
exe = "old.exe"
probe_version = "one"
tsf_range_edit = true
[future_table]
a = 1
"#;
    let loaded = AppRulesDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("secure_field_mode", "invalid-value"),
            ("default_max_delete_before", "out-of-range"),
            ("rule[1].exe", "invalid-value"),
            ("rule[2].transport", "invalid-value"),
            ("probe[0].probe_version", "invalid-type"),
        ])
    );
    let doc = loaded.document;
    assert_eq!(doc.revision, 3);
    let c = &doc.compatibility;
    assert!(c.clipboard_enabled);
    assert_eq!(c.secure_field_mode, SecureFieldMode::Bypass);
    assert_eq!(c.default_max_delete_before, 64);
    assert_eq!(c.rules.len(), 2);
    assert_eq!(c.rules[0].exe, exe("word.exe"));
    assert_eq!(c.rules[0].action.transport, Some(Transport::Tsf));
    assert_eq!(c.rules[1].exe, exe("excel.exe"));
    assert_eq!(c.rules[1].action, RuleAction::default());
    assert_eq!(c.named_sets.len(), 1);
    assert_eq!(c.named_sets[0].action.macros, Some(false));
    assert!(c.probes.is_empty());
    assert_eq!(
        doc.extensions
            .get("future_flag")
            .and_then(toml::Value::as_str),
        Some("kept")
    );
    assert!(doc.extensions.contains_key("future_table"));
    let rendered = format!("{:?}", loaded.issues);
    for secret in ["paranoid", "teleport", "not-a-basename"] {
        assert!(!rendered.contains(secret), "{rendered}");
    }

    // The rule and the probe that could not be decoded are written back unchanged.
    let encoded = doc.encode().unwrap();
    assert!(encoded.contains("C:/path/not-a-basename.exe"), "{encoded}");
    assert!(encoded.contains("probe_version = \"one\""), "{encoded}");
    let again = AppRulesDocument::decode(&encoded).unwrap();
    assert_eq!(again.document, doc);
    assert_eq!(
        codes(&again.issues),
        pairs(&[
            ("rule[2].exe", "invalid-value"),
            ("probe[0].probe_version", "invalid-type"),
        ])
    );
}

/// `(path, reason code)` pairs, which is how a caller would display issues.
fn codes(issues: &[FieldIssue]) -> Vec<(String, &'static str)> {
    issues
        .iter()
        .map(|issue| (issue.path.clone(), issue.reason.code()))
        .collect()
}

fn pairs(expected: &[(&str, &'static str)]) -> Vec<(String, &'static str)> {
    expected
        .iter()
        .map(|(path, code)| ((*path).to_owned(), *code))
        .collect()
}

/// Matrix #45 with the D13 data: one member written as a path costs only that member, so the
/// policy of every other member of the set survives the next save.
#[test]
fn matrix_45_one_bad_member_keeps_the_named_set() {
    let text = r#"version = 1
[[named_set]]
name = "chromium-family"
members = ["msedge.exe", "chrome.exe", "C:/Apps/brave.exe", 7, "vivaldi.exe"]
action = { reset_on_mouse = true, transport = "warp", max_delete_before = -1 }
[[named_set]]
name = "terminals"
members = "wt.exe"
action = "reset"
[[named_set]]
members = ["orphan.exe"]
"#;
    let loaded = AppRulesDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("named_set[0].members[2]", "invalid-value"),
            ("named_set[0].members[3]", "invalid-type"),
            ("named_set[0].action.transport", "invalid-value"),
            ("named_set[0].action.max_delete_before", "out-of-range"),
            ("named_set[1].members", "invalid-type"),
            ("named_set[1].action", "invalid-type"),
            ("named_set[2].name", "missing"),
        ])
    );
    let sets = &loaded.document.compatibility.named_sets;
    assert_eq!(sets.len(), 2);
    assert_eq!(sets[0].name, "chromium-family");
    assert_eq!(
        sets[0].members,
        vec![exe("msedge.exe"), exe("chrome.exe"), exe("vivaldi.exe")]
    );
    assert_eq!(
        sets[0].action,
        RuleAction {
            reset_on_mouse: Some(true),
            ..RuleAction::default()
        }
    );
    assert_eq!(sets[1].name, "terminals");
    assert!(sets[1].members.is_empty());
    assert_eq!(sets[1].action, RuleAction::default());
    let encoded = loaded.document.encode().unwrap();
    assert!(encoded.contains("orphan.exe"), "{encoded}");
}

/// Rule fields other than the executable take their "inherit" default when they are bad; a
/// rule whose executable or window class cannot be read is set aside, because dropping only
/// the window class would widen the rule to every window of the executable.
#[test]
fn rule_fields_are_isolated_and_scope_problems_set_the_rule_aside() {
    let text = r#"version = 1
[[rule]]
exe = "Notepad.EXE"
transport = "Clipboard"
reset_on_mouse = "yes"
macros = false
vietnamese = 0
max_delete_before = 4294967296
[[rule]]
exe = "code.exe"
window_class = 5
vietnamese = false
[[rule]]
window_class = "Chrome_WidgetWin_1"
[[probe]]
exe = "legacy.exe"
probe_version = 1
[[probe]]
exe = "fine.exe"
probe_version = 1
tsf_range_edit = false
"#;
    let loaded = AppRulesDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("rule[0].transport", "invalid-value"),
            ("rule[0].reset_on_mouse", "invalid-type"),
            ("rule[0].vietnamese", "invalid-type"),
            ("rule[0].max_delete_before", "out-of-range"),
            ("rule[1].window_class", "invalid-type"),
            ("rule[2].exe", "missing"),
            ("probe[0].tsf_range_edit", "missing"),
        ])
    );
    assert!(
        loaded
            .issues
            .contains(&issue("rule[2].exe", IssueReason::Missing))
    );
    let c = &loaded.document.compatibility;
    assert_eq!(
        c.rules,
        vec![AppRule {
            exe: exe("notepad.exe"),
            window_class: None,
            action: RuleAction {
                macros: Some(false),
                ..RuleAction::default()
            },
        }]
    );
    assert_eq!(c.probes.len(), 1);
    assert_eq!(c.probes[0].exe, exe("fine.exe"));
    let encoded = loaded.document.encode().unwrap();
    for kept in ["code.exe", "Chrome_WidgetWin_1", "legacy.exe"] {
        assert!(encoded.contains(kept), "{kept} missing from\n{encoded}");
    }
}

/// D49: `autocomplete_guard` is an action field of a rule and of a named set: it is read,
/// written back, and a bad value is reported and inherits like any other action field.
#[test]
fn d49_the_autocomplete_guard_is_an_action_field() {
    let text = r#"version = 1
[[rule]]
exe = "winword.exe"
autocomplete_guard = true
[[rule]]
exe = "chrome.exe"
window_class = "Chrome_WidgetWin_1"
autocomplete_guard = false
[[named_set]]
name = "autocomplete"
members = ["excel.exe"]
action = { autocomplete_guard = true }
"#;
    let loaded = AppRulesDocument::decode(text).unwrap();
    assert!(loaded.issues.is_empty(), "{:?}", loaded.issues);
    let c = &loaded.document.compatibility;
    let guard = |on| RuleAction {
        autocomplete_guard: Some(on),
        ..RuleAction::default()
    };
    assert_eq!(c.rules[0].action, guard(true));
    assert_eq!(c.rules[1].action, guard(false));
    assert_eq!(c.named_sets[0].action, guard(true));
    let encoded = loaded.document.encode().unwrap();
    assert!(encoded.contains("autocomplete_guard = true"), "{encoded}");
    assert!(encoded.contains("autocomplete_guard = false"), "{encoded}");
    let back = AppRulesDocument::decode(&encoded).unwrap();
    assert!(back.issues.is_empty(), "{:?}", back.issues);
    assert_eq!(back.document, loaded.document);

    let bad = r#"version = 1
[[rule]]
exe = "chrome.exe"
autocomplete_guard = "yes"
macros = false
[[named_set]]
name = "autocomplete"
members = ["excel.exe"]
action = { autocomplete_guard = 1 }
"#;
    let loaded = AppRulesDocument::decode(bad).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("rule[0].autocomplete_guard", "invalid-type"),
            ("named_set[0].action.autocomplete_guard", "invalid-type"),
        ])
    );
    let c = &loaded.document.compatibility;
    assert_eq!(
        c.rules[0].action,
        RuleAction {
            macros: Some(false),
            ..RuleAction::default()
        }
    );
    assert_eq!(c.named_sets[0].action, RuleAction::default());
}

/// Unknown keys inside `[[rule]]`, `[[named_set]]` (and its `action`), and `[[probe]]` survive
/// a rewrite and follow their entry.
#[test]
fn unknown_keys_inside_entries_survive() {
    let text = r#"version = 1
[[rule]]
exe = "notepad.exe"
macros = false
future_rule_key = "r"
[[rule]]
exe = "notepad.exe"
window_class = "Edit"
future_rule_key = "narrow"
[[named_set]]
name = "editors"
members = ["code.exe"]
future_set_key = ["s"]
action = { macros = false, future_action_key = 3 }
[[probe]]
exe = "legacy.exe"
probe_version = 1
tsf_range_edit = false
future_probe_key = { at = 1 }
"#;
    let loaded = AppRulesDocument::decode(text).unwrap();
    assert!(loaded.is_clean(), "{:?}", loaded.issues);
    let mut doc = loaded.document;
    let check = |doc: &AppRulesDocument| {
        let encoded = doc.encode().unwrap();
        let parsed: toml::Table = toml::from_str(&encoded).unwrap();
        let rules = parsed["rule"].as_array().unwrap().clone();
        let rule = |class: Option<&str>| {
            rules
                .iter()
                .find(|rule| rule.get("window_class").and_then(toml::Value::as_str) == class)
                .unwrap()
                .clone()
        };
        assert_eq!(rule(None)["future_rule_key"].as_str(), Some("r"));
        assert_eq!(
            rule(Some("Edit"))["future_rule_key"].as_str(),
            Some("narrow")
        );
        let set = &parsed["named_set"][0];
        assert_eq!(set["future_set_key"][0].as_str(), Some("s"));
        assert_eq!(set["action"]["future_action_key"].as_integer(), Some(3));
        assert_eq!(set["action"]["macros"].as_bool(), Some(false));
        assert_eq!(
            parsed["probe"][0]["future_probe_key"]["at"].as_integer(),
            Some(1)
        );
        encoded
    };
    let encoded = check(&doc);
    assert_eq!(AppRulesDocument::decode(&encoded).unwrap().document, doc);
    doc.compatibility.rules.reverse();
    check(&doc);
}

#[test]
fn version_and_syntax_errors() {
    assert_eq!(
        AppRulesDocument::decode("version = 2\n").unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    assert_eq!(
        AppRulesDocument::decode("clipboard_enabled = tru\n").unwrap_err(),
        ConfigError::Corrupt
    );
}
