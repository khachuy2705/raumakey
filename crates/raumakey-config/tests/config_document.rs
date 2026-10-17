use raumakey_config::{
    CaseTransform, ConfigDocument, ConfigError, FieldIssue, IssueReason, StartupMode, StoreDocument,
};
use raumakey_model::hotkey::{Hotkey, HotkeyConfig};
use raumakey_model::keymap::{KeyAction, KeyBinding, KeyMap, Tone};
use raumakey_model::options::{EngineOptions, InputMethod, Normalization};

/// The example of plan §5.1 for `raumakey.toml` (`config.toml` in the plan), verbatim.
const PLAN_EXAMPLE: &str = r#"version = 1
revision = 1
[input]
enabled = true
method = "telex"            # telex | vni | viqr | user
user_keymap = "my-map"      # optional
normalization = "nfc"       # nfc | nfd | preserve
modern_tone = false
free_marking = true
spell_check = true
auto_restore_invalid = true
context_limit = 32
[macros]
enabled = true
when_disabled = false
auto_case = true
[hotkeys]
toggle_mode = ["Ctrl+Shift"]
hard_reset = ["Ctrl+Shift+Alt+F12", "Ctrl+Shift+Alt+Space"]
[system]
startup = "none"            # none | shortcut | elevated-task
show_panel_on_start = true
diagnostics = false
[conversion]
source = "unicode"
target = "unicode"
case = "none"               # none | upper | lower | title
remove_tones = false
[[keymaps]]
id = "my-map"
version = 1
name = "My map"
bindings = [ { key = "s", actions = ["tone:acute"] } ]
"#;

fn issue(path: &str, reason: IssueReason) -> FieldIssue {
    FieldIssue {
        path: path.to_owned(),
        reason,
    }
}

#[test]
fn defaults_mirror_engine_defaults_v1_and_default_hotkeys() {
    let doc = ConfigDocument::defaults();
    assert_eq!(doc.version, 1);
    assert_eq!(doc.revision, 0);
    assert_eq!(doc.engine_options(), EngineOptions::defaults_v1());
    assert_eq!(doc.hotkeys, HotkeyConfig::default());
    assert_eq!(doc.system.startup, StartupMode::None);
    assert!(doc.system.show_panel_on_start);
    assert!(!doc.system.diagnostics);
    assert_eq!(doc.conversion.source, "unicode");
    assert_eq!(doc.conversion.target, "unicode");
    assert_eq!(doc.conversion.case, CaseTransform::None);
    assert!(!doc.conversion.remove_tones);
    assert!(doc.keymaps.is_empty());
    assert!(doc.extensions.is_empty());
    assert!(doc.input.extensions.is_empty());
    assert_eq!(ConfigDocument::default_document(), doc);
    assert_eq!(ConfigDocument::default(), doc);
    assert_eq!(ConfigDocument::FILE_NAME, "raumakey.toml");
    assert_eq!(ConfigDocument::CURRENT_VERSION, 1);
}

#[test]
fn engine_options_convert_both_ways() {
    let options = EngineOptions {
        enabled: false,
        input_method: InputMethod::UserDefined,
        normalization: Normalization::Nfd,
        modern_tone: true,
        free_marking: false,
        spell_check: false,
        auto_restore_invalid: false,
        macros_enabled: false,
        macros_when_disabled: true,
        auto_case_macro: false,
        user_keymap_id: Some("mine".into()),
        context_limit_codepoints: 100,
    };
    let mut doc = ConfigDocument::defaults();
    doc.keymaps.push(KeyMap {
        id: "mine".into(),
        version: 1,
        name: "Mine".into(),
        bindings: Vec::new(),
    });
    doc.apply_engine_options(&options);
    assert_eq!(doc.engine_options(), options);
    assert_eq!(doc.input.method, InputMethod::UserDefined);
    assert_eq!(doc.input.user_keymap.as_deref(), Some("mine"));
    assert_eq!(doc.input.context_limit, 100);
    assert!(!doc.macros.enabled);
    assert!(doc.macros.when_disabled);
    assert!(!doc.macros.auto_case);
    assert_eq!(doc.hotkeys, HotkeyConfig::default());
    assert_eq!(doc.system, ConfigDocument::defaults().system);
}

/// Finding: the user method needs a key map of the document. While `input.user_keymap` names
/// none, the engine options use the default method, but the document keeps the user's choice,
/// so it takes effect again as soon as the key map can be used.
#[test]
fn engine_options_fall_back_while_the_user_keymap_cannot_be_used() {
    let mut doc = ConfigDocument::defaults();
    doc.input.method = InputMethod::UserDefined;
    doc.input.user_keymap = Some("mine".into());
    let options = doc.engine_options();
    assert_eq!(options.input_method, InputMethod::Telex);
    assert_eq!(options.user_keymap_id.as_deref(), Some("mine"));
    assert!(options.validate().is_ok());
    assert_eq!(doc.input.method, InputMethod::UserDefined);

    doc.keymaps.push(KeyMap {
        id: "mine".into(),
        version: 1,
        name: "Mine".into(),
        bindings: Vec::new(),
    });
    let options = doc.engine_options();
    assert_eq!(options.input_method, InputMethod::UserDefined);
    assert_eq!(options.user_keymap_id.as_deref(), Some("mine"));

    doc.input.user_keymap = None;
    let options = doc.engine_options();
    assert_eq!(options.input_method, InputMethod::Telex);
    assert!(options.validate().is_ok());
    // Other methods never depend on a key map.
    doc.input.method = InputMethod::Vni;
    assert_eq!(doc.engine_options().input_method, InputMethod::Vni);
}

#[test]
fn keyword_enums_use_documented_names() {
    let names = [
        (StartupMode::None, "none"),
        (StartupMode::Shortcut, "shortcut"),
        (StartupMode::ElevatedTask, "elevated-task"),
    ];
    for (mode, name) in names {
        assert_eq!(mode.as_str(), name);
        assert_eq!(serde_json::to_string(&mode).unwrap(), format!("\"{name}\""));
    }
    let cases = [
        (CaseTransform::None, "none"),
        (CaseTransform::Upper, "upper"),
        (CaseTransform::Lower, "lower"),
        (CaseTransform::Title, "title"),
    ];
    for (case, name) in cases {
        assert_eq!(case.as_str(), name);
        assert_eq!(serde_json::to_string(&case).unwrap(), format!("\"{name}\""));
    }
}

#[test]
fn plan_example_decodes_without_issues() {
    let loaded = ConfigDocument::decode(PLAN_EXAMPLE).unwrap();
    assert_eq!(loaded.issues, Vec::<FieldIssue>::new());
    assert!(loaded.is_clean());
    let doc = loaded.document;
    assert_eq!(doc.version, 1);
    assert_eq!(doc.revision, 1);
    let mut expected_options = EngineOptions::defaults_v1();
    expected_options.user_keymap_id = Some("my-map".into());
    assert_eq!(doc.engine_options(), expected_options);
    assert_eq!(doc.hotkeys, HotkeyConfig::default());
    assert_eq!(doc.system, ConfigDocument::defaults().system);
    assert_eq!(doc.conversion, ConfigDocument::defaults().conversion);
    assert_eq!(
        doc.keymaps,
        vec![KeyMap {
            id: "my-map".into(),
            version: 1,
            name: "My map".into(),
            bindings: vec![KeyBinding {
                key: 's',
                actions: vec![KeyAction::Tone(Tone::Acute)],
            }],
        }]
    );
    assert!(doc.extensions.is_empty());
}

#[test]
fn plan_example_round_trips_through_encode() {
    let first = ConfigDocument::decode(PLAN_EXAMPLE).unwrap().document;
    let text = first.encode().unwrap();
    let second = ConfigDocument::decode(&text).unwrap();
    assert!(second.issues.is_empty());
    assert_eq!(second.document, first);
}

#[test]
fn encoded_defaults_round_trip_and_start_with_version() {
    let doc = ConfigDocument::defaults();
    let text = doc.encode().unwrap();
    assert!(text.starts_with("version = 1\nrevision = 0\n"), "{text}");
    let back = ConfigDocument::decode(&text).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, doc);
}

#[test]
fn missing_sections_and_fields_take_defaults_silently() {
    let loaded = ConfigDocument::decode("version = 1\n[input]\nmethod = \"vni\"\n").unwrap();
    assert!(loaded.issues.is_empty());
    let mut expected = ConfigDocument::defaults();
    expected.input.method = InputMethod::Vni;
    assert_eq!(loaded.document, expected);
}

/// Matrix #45: a file without `version` (empty, comment-only, or truncated by an external
/// tool) still loads on defaults, but never silently: the caller can tell it from a healthy
/// file.
#[test]
fn a_file_without_version_is_reported() {
    for text in [
        "",
        "   \n",
        "# only a comment\n",
        "\u{feff}",
        "[input]\nmethod = \"vni\"\n",
    ] {
        let loaded = ConfigDocument::decode(text).unwrap();
        assert_eq!(
            loaded
                .issues
                .iter()
                .map(|issue| (issue.path.as_str(), issue.reason.code()))
                .collect::<Vec<_>>(),
            vec![("version", "missing")],
            "{text:?}"
        );
        assert!(!loaded.is_clean());
        assert_eq!(loaded.document.version, 1);
    }
    assert_eq!(
        ConfigDocument::decode("").unwrap().document,
        ConfigDocument::defaults()
    );
}

#[test]
fn a_leading_byte_order_mark_is_accepted() {
    let loaded = ConfigDocument::decode("\u{feff}version = 1\nrevision = 3\n").unwrap();
    assert_eq!(loaded.document.revision, 3);
}

/// Matrix #45: one corrupt field is defaulted and reported; everything else, including
/// unknown fields, survives and is re-emitted.
#[test]
fn matrix_45_one_corrupt_field_only_defaults_that_field() {
    let text = r#"version = 1
revision = 7
future_top = "keep me"
[input]
enabled = false
method = "vni"
normalization = "bogus-normalization"
modern_tone = true
context_limit = 64
future_input = [1, 2, 3]
[macros]
enabled = false
[system]
startup = "shortcut"
diagnostics = true
[future_section]
x = 1
nested = { deep = "value" }
"#;
    let loaded = ConfigDocument::decode(text).unwrap();
    assert_eq!(
        loaded.issues,
        vec![issue("input.normalization", IssueReason::InvalidValue)]
    );
    let doc = loaded.document;
    assert_eq!(doc.revision, 7);
    assert!(!doc.input.enabled);
    assert_eq!(doc.input.method, InputMethod::Vni);
    assert_eq!(doc.input.normalization, Normalization::Nfc);
    assert!(doc.input.modern_tone);
    assert_eq!(doc.input.context_limit, 64);
    assert!(!doc.macros.enabled);
    assert_eq!(doc.system.startup, StartupMode::Shortcut);
    assert!(doc.system.diagnostics);
    assert_eq!(
        doc.extensions
            .get("future_top")
            .and_then(toml::Value::as_str),
        Some("keep me")
    );
    assert!(doc.extensions.contains_key("future_section"));
    assert!(doc.input.extensions.contains_key("future_input"));

    let encoded = doc.encode().unwrap();
    let again = ConfigDocument::decode(&encoded).unwrap();
    assert!(again.issues.is_empty(), "{:?}", again.issues);
    assert_eq!(again.document, doc);
    let reparsed: toml::Table = toml::from_str(&encoded).unwrap();
    assert_eq!(reparsed["future_top"].as_str(), Some("keep me"));
    assert_eq!(
        reparsed["future_section"]["nested"]["deep"].as_str(),
        Some("value")
    );
    assert_eq!(
        reparsed["input"]["future_input"].as_array().map(Vec::len),
        Some(3)
    );
}

/// Matrix #45 for `[system]`: a startup mode this build does not know (from a newer build or a
/// typo) and values of the wrong type take their defaults one by one and are reported; the
/// other fields and unknown keys of the section survive.
#[test]
fn matrix_45_system_fields_are_defaulted_one_by_one() {
    let text = r#"version = 1
[system]
startup = "service"
show_panel_on_start = "no"
diagnostics = 1
future_system = true
"#;
    let loaded = ConfigDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("system.startup", "invalid-value"),
            ("system.show_panel_on_start", "invalid-type"),
            ("system.diagnostics", "invalid-type"),
        ])
    );
    let system = &loaded.document.system;
    assert_eq!(system.startup, StartupMode::None);
    assert!(system.show_panel_on_start);
    assert!(!system.diagnostics);
    assert!(system.extensions.contains_key("future_system"));
    assert!(!format!("{:?}", loaded.issues).contains("service"));

    for (startup, reason) in [("7", "invalid-type"), ("\"Shortcut\"", "invalid-value")] {
        let text = format!(
            "version = 1\n[system]\nstartup = {startup}\nshow_panel_on_start = false\ndiagnostics = true\n"
        );
        let loaded = ConfigDocument::decode(&text).unwrap();
        assert_eq!(
            codes(&loaded.issues),
            pairs(&[("system.startup", reason)]),
            "{text}"
        );
        let system = &loaded.document.system;
        assert_eq!(system.startup, StartupMode::None);
        assert!(!system.show_panel_on_start);
        assert!(system.diagnostics);
    }
}

#[test]
fn every_kind_of_corrupt_field_is_reported_with_its_path_only() {
    let text = r#"version = 1
revision = "seven"
[input]
spell_check = 1
context_limit = 4096
user_keymap = "bad id with spaces"
method = 42
[macros]
auto_case = "sometimes"
[hotkeys]
toggle_mode = ["Ctrl+Shift", "Hyper+Q", 5]
hard_reset = "Ctrl+Alt+Delete"
[conversion]
source = 1
case = "sideways"
system = 3
[[keymaps]]
id = "ok-map"
version = 1
name = "Fine"
bindings = [ { key = "s", actions = ["tone:acute"] } ]
[[keymaps]]
id = "broken map"
version = 1
name = "Broken"
bindings = []
[[keymaps]]
id = "ok-map"
version = 1
name = "Duplicate"
bindings = []
[[keymaps]]
id = "not-a-table-next"
version = "one"
name = "Wrong"
bindings = []
"#;
    let loaded = ConfigDocument::decode(text).unwrap();
    let mut expected = vec![
        issue("revision", IssueReason::InvalidType),
        issue("input.method", IssueReason::InvalidType),
        issue("input.user_keymap", IssueReason::InvalidValue),
        issue("input.spell_check", IssueReason::InvalidType),
        issue("input.context_limit", IssueReason::OutOfRange),
        issue("macros.auto_case", IssueReason::InvalidType),
        issue("hotkeys.toggle_mode[1]", IssueReason::InvalidValue),
        issue("hotkeys.toggle_mode[2]", IssueReason::InvalidType),
        issue("hotkeys.hard_reset", IssueReason::InvalidType),
        issue("conversion.source", IssueReason::InvalidType),
        issue("conversion.case", IssueReason::InvalidValue),
        issue("keymaps[1].id", IssueReason::InvalidValue),
        issue("keymaps[2].id", IssueReason::DuplicateEntry),
        issue("keymaps[3].version", IssueReason::InvalidType),
    ];
    let mut actual = loaded.issues.clone();
    expected.sort_by(|a, b| a.path.cmp(&b.path));
    actual.sort_by(|a, b| a.path.cmp(&b.path));
    assert_eq!(actual, expected);

    let doc = loaded.document;
    let defaults = ConfigDocument::defaults();
    assert_eq!(doc.revision, 0);
    assert_eq!(doc.input.method, defaults.input.method);
    assert_eq!(doc.input.user_keymap, None);
    assert_eq!(doc.input.spell_check, defaults.input.spell_check);
    assert_eq!(doc.input.context_limit, defaults.input.context_limit);
    assert_eq!(doc.macros.auto_case, defaults.macros.auto_case);
    assert_eq!(
        doc.hotkeys.toggle_mode,
        vec!["Ctrl+Shift".parse::<Hotkey>().unwrap()]
    );
    assert_eq!(doc.hotkeys.hard_reset, defaults.hotkeys.hard_reset);
    assert_eq!(doc.conversion.source, "unicode");
    assert_eq!(doc.conversion.case, CaseTransform::None);
    // `system = 3` sits inside [conversion], so it is an unknown conversion key, not a section.
    assert!(doc.conversion.extensions.contains_key("system"));
    assert_eq!(doc.keymaps.len(), 1);
    assert_eq!(doc.keymaps[0].name, "Fine");

    // Issues carry paths and reason codes only, never the rejected values.
    let rendered = format!("{:?}", loaded.issues);
    for secret in [
        "seven",
        "bad id",
        "Hyper",
        "sideways",
        "sometimes",
        "Broken",
    ] {
        assert!(!rendered.contains(secret), "{rendered}");
    }
    let codes: Vec<&str> = loaded.issues.iter().map(|i| i.reason.code()).collect();
    assert!(codes.contains(&"out-of-range"));
    assert!(codes.contains(&"duplicate-entry"));
}

#[test]
fn a_section_of_the_wrong_type_is_defaulted_and_reported() {
    let loaded =
        ConfigDocument::decode("version = 1\ninput = 5\n[system]\nstartup = \"elevated-task\"\n")
            .unwrap();
    assert_eq!(
        loaded.issues,
        vec![issue("input", IssueReason::InvalidType)]
    );
    assert_eq!(loaded.document.input, ConfigDocument::defaults().input);
    assert_eq!(loaded.document.system.startup, StartupMode::ElevatedTask);
}

#[test]
fn unknown_hotkey_keys_are_preserved_under_the_hotkeys_extension() {
    let text = "version = 1\n[hotkeys]\ntoggle_mode = [\"Alt+Z\"]\nfuture_hotkey = [\"Ctrl+F9\"]\n";
    let doc = ConfigDocument::decode(text).unwrap().document;
    assert_eq!(
        doc.hotkeys.toggle_mode,
        vec!["Alt+Z".parse::<Hotkey>().unwrap()]
    );
    assert_eq!(doc.hotkeys.hard_reset, HotkeyConfig::default().hard_reset);
    let hotkey_extensions = doc.extensions["hotkeys"].as_table().unwrap();
    assert!(hotkey_extensions.contains_key("future_hotkey"));
    let encoded = doc.encode().unwrap();
    let reparsed: toml::Table = toml::from_str(&encoded).unwrap();
    assert!(
        reparsed["hotkeys"]
            .as_table()
            .unwrap()
            .contains_key("future_hotkey")
    );
    assert_eq!(ConfigDocument::decode(&encoded).unwrap().document, doc);
}

#[test]
fn extension_keys_that_collide_with_known_keys_are_not_emitted_twice() {
    let mut doc = ConfigDocument::defaults();
    doc.extensions
        .insert("revision".into(), toml::Value::Integer(99));
    doc.input
        .extensions
        .insert("enabled".into(), toml::Value::Boolean(false));
    let text = doc.encode().unwrap();
    let back = ConfigDocument::decode(&text).unwrap().document;
    assert_eq!(back.revision, 0);
    assert!(back.input.enabled);
}

#[test]
fn empty_and_nested_unknown_tables_survive_a_round_trip() {
    let text = r#"version = 1
x_empty = {}
x_array_of_empty = [{}, {}]
x_mixed = [1, "two", { three = 3 }]
when = 1979-05-27T07:32:00Z
[input]
x_empty_section_table = {}
[x_section]
[x_section.deeper]
leaf = 1.5
"#;
    let doc = ConfigDocument::decode(text).unwrap().document;
    assert_eq!(doc.extensions.len(), 5);
    assert_eq!(
        doc.extensions["x_empty"].as_table().map(toml::Table::len),
        Some(0)
    );
    assert_eq!(
        doc.extensions["x_array_of_empty"].as_array().map(Vec::len),
        Some(2)
    );
    assert!(doc.extensions["when"].is_datetime());
    assert!(doc.input.extensions.contains_key("x_empty_section_table"));
    let back = ConfigDocument::decode(&doc.encode().unwrap()).unwrap();
    assert!(back.issues.is_empty());
    assert_eq!(back.document, doc);
}

#[test]
fn newer_or_unknown_versions_are_unsupported() {
    assert_eq!(
        ConfigDocument::decode("version = 2\n").unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    assert_eq!(
        ConfigDocument::decode("version = 0\n").unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    assert_eq!(
        ConfigDocument::decode("version = -1\n").unwrap_err(),
        ConfigError::UnsupportedVersion
    );
    // `version` decides how everything else is read, so a version that is not an integer is
    // never guessed: such a file is not decoded (and a store never overwrites it).
    for text in [
        "version = \"1\"\n",
        "version = \"2\"\n",
        "version = 1.0\n",
        "version = 2.5\n",
        "version = true\n",
        "version = [1]\n",
        "version = { major = 1 }\n",
        "version = 1979-05-27\n",
    ] {
        assert_eq!(
            ConfigDocument::decode(text).unwrap_err(),
            ConfigError::UnsupportedVersion,
            "{text}"
        );
    }
}

#[test]
fn syntax_errors_are_corrupt() {
    for text in [
        "version = 1\n[input\n",
        "version = = 1",
        "[a]\nx = 1\n[a]\nx = 2\n",
    ] {
        assert_eq!(
            ConfigDocument::decode(text).unwrap_err(),
            ConfigError::Corrupt,
            "{text}"
        );
    }
}

#[test]
fn encoding_a_document_with_a_foreign_version_is_refused() {
    let mut doc = ConfigDocument::defaults();
    doc.version = 2;
    assert_eq!(doc.encode().unwrap_err(), ConfigError::UnsupportedVersion);
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

fn binding(key: char, actions: &[KeyAction]) -> KeyBinding {
    KeyBinding {
        key,
        actions: actions.to_vec(),
    }
}

/// Matrix #45 inside `[[keymaps]]`: a bad action, binding, or optional field costs only that
/// element, never the key map that `input.user_keymap` points at.
#[test]
fn matrix_45_keymap_problems_are_isolated_per_element() {
    let text = r#"version = 1
[input]
method = "user"
user_keymap = "my-map"
[[keymaps]]
id = "my-map"
version = 1
name = "My map"
bindings = [
    { key = "s", actions = ["tone:acute"] },
    { key = "f", actions = ["tone:graveX"] },
    { key = "r", actions = ["tone:hook", "bogus", "remove-tone"] },
    { key = "x", actions = ["tone:tilde", "breve", "horn", "stroke", "escape"] },
    { key = "S", actions = ["tone:acute"] },
    { key = "ab", actions = ["stroke"] },
    { actions = ["stroke"] },
    { key = "d" },
    { key = "j", actions = [] },
    { key = "w", actions = "horn" },
    5,
]
"#;
    let loaded = ConfigDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("keymaps[0].bindings[1].actions[0]", "invalid-value"),
            ("keymaps[0].bindings[1].actions", "out-of-range"),
            ("keymaps[0].bindings[2].actions[1]", "invalid-value"),
            ("keymaps[0].bindings[3].actions[4]", "out-of-range"),
            ("keymaps[0].bindings[4].key", "duplicate-entry"),
            ("keymaps[0].bindings[5].key", "invalid-value"),
            ("keymaps[0].bindings[6].key", "missing"),
            ("keymaps[0].bindings[7].actions", "missing"),
            ("keymaps[0].bindings[8].actions", "out-of-range"),
            ("keymaps[0].bindings[9].actions", "invalid-type"),
            ("keymaps[0].bindings[10]", "invalid-type"),
        ])
    );
    let doc = loaded.document;
    assert_eq!(doc.input.method, InputMethod::UserDefined);
    assert_eq!(doc.input.user_keymap.as_deref(), Some("my-map"));
    assert_eq!(doc.keymaps.len(), 1);
    let map = &doc.keymaps[0];
    assert!(map.validate().is_ok());
    assert_eq!(map.name, "My map");
    assert_eq!(
        map.bindings,
        vec![
            binding('s', &[KeyAction::Tone(Tone::Acute)]),
            binding('r', &[KeyAction::Tone(Tone::Hook), KeyAction::RemoveTone]),
            binding(
                'x',
                &[
                    KeyAction::Tone(Tone::Tilde),
                    KeyAction::Breve,
                    KeyAction::Horn,
                    KeyAction::Stroke,
                ]
            ),
        ]
    );
    assert!(doc.engine_options().validate().is_ok());
}

/// A key map whose optional fields are bad keeps its bindings; a missing name or version is
/// reported but does not cost the key map.
#[test]
fn keymap_name_and_version_problems_keep_the_key_map() {
    let long_name = "n".repeat(129);
    let text = format!(
        r#"version = 1
[[keymaps]]
id = "no-version"
name = "No version"
bindings = [ {{ key = "s", actions = ["tone:acute"] }} ]
[[keymaps]]
id = "no-name"
version = 1
bindings = []
[[keymaps]]
id = "long-name"
version = 1
name = "{long_name}"
[[keymaps]]
id = "typed-name"
version = 1
name = 7
bindings = "none"
"#
    );
    let loaded = ConfigDocument::decode(&text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("keymaps[0].version", "missing"),
            ("keymaps[1].name", "missing"),
            ("keymaps[2].name", "out-of-range"),
            ("keymaps[2].bindings", "missing"),
            ("keymaps[3].name", "invalid-type"),
            ("keymaps[3].bindings", "invalid-type"),
        ])
    );
    let maps = &loaded.document.keymaps;
    let ids: Vec<&str> = maps.iter().map(|map| map.id.as_str()).collect();
    assert_eq!(ids, ["no-version", "no-name", "long-name", "typed-name"]);
    assert!(maps.iter().all(|map| map.validate().is_ok()));
    assert_eq!(maps[0].version, 1);
    assert_eq!(maps[0].bindings.len(), 1);
    assert_eq!(maps[1].name, "");
    assert_eq!(maps[2].name, "");
    assert!(!format!("{:?}", loaded.issues).contains("nnnn"));
}

/// A key map whose identity or schema version cannot be read is set aside verbatim and
/// written back. Finding: `method = "user"` is a valid choice and must not be reset because of
/// another field's problem (matrix #45). It is kept and written back; the engine options use
/// the default method meanwhile, every load reports `input.user_keymap`, and the choice takes
/// effect again once the key map is fixed.
#[test]
fn an_unreadable_keymap_is_kept_verbatim_and_the_user_method_survives() {
    let text = r#"version = 1
[input]
method = "user"
user_keymap = "future-map"
[[keymaps]]
id = "future-map"
version = 2
name = "From a newer build"
bindings = [ { key = "q", actions = ["tone:future"] } ]
"#;
    let expected = pairs(&[
        ("keymaps[0].version", "out-of-range"),
        ("input.user_keymap", "unresolved"),
    ]);
    let loaded = ConfigDocument::decode(text).unwrap();
    assert_eq!(codes(&loaded.issues), expected);
    let doc = &loaded.document;
    assert!(doc.keymaps.is_empty());
    assert_eq!(doc.input.method, InputMethod::UserDefined);
    assert_eq!(doc.input.user_keymap.as_deref(), Some("future-map"));
    let options = doc.engine_options();
    assert_eq!(options.input_method, InputMethod::Telex);
    assert!(options.validate().is_ok());

    let encoded = doc.encode().unwrap();
    for kept in [
        "method = \"user\"",
        "user_keymap = \"future-map\"",
        "From a newer build",
        "tone:future",
        "version = 2",
    ] {
        assert!(encoded.contains(kept), "{kept} missing from\n{encoded}");
    }
    let again = ConfigDocument::decode(&encoded).unwrap();
    assert_eq!(&again.document, doc);
    assert_eq!(codes(&again.issues), expected, "reported on every load");

    // Fixing the key map by hand brings the user method back.
    let fixed = encoded
        .replace("version = 2", "version = 1")
        .replace("tone:future", "tone:acute");
    let fixed = ConfigDocument::decode(&fixed).unwrap();
    assert!(fixed.is_clean(), "{:?}", fixed.issues);
    assert_eq!(
        fixed.document.engine_options().input_method,
        InputMethod::UserDefined
    );
}

/// `method = "user"` without a usable key map would make every engine request fail
/// (`OptionsError::MissingUserKeymap` or an unknown key map). The engine options use the
/// default method instead, while the document keeps `method = "user"` and every load reports
/// `input.user_keymap` until it names a key map that decodes.
#[test]
fn user_method_without_a_usable_keymap_is_kept_and_reported_on_every_load() {
    let keymap = "[[keymaps]]\nid = \"mine\"\nversion = 1\nname = \"Mine\"\nbindings = []\n";
    let missing = ("input.user_keymap", "missing");
    // (`user_keymap` line, issues of the first load, issues once the document was written back)
    let cases = [
        ("", vec![missing], vec![missing]),
        ("user_keymap = \"\"\n", vec![missing], vec![missing]),
        (
            "user_keymap = \"bad id\"\n",
            vec![("input.user_keymap", "invalid-value")],
            vec![missing],
        ),
        (
            "user_keymap = 5\n",
            vec![("input.user_keymap", "invalid-type")],
            vec![missing],
        ),
        (
            "user_keymap = \"absent\"\n",
            vec![("input.user_keymap", "unresolved")],
            vec![("input.user_keymap", "unresolved")],
        ),
    ];
    for (user_keymap, first, later) in cases {
        let text = format!("version = 1\n[input]\nmethod = \"user\"\n{user_keymap}{keymap}");
        let loaded = ConfigDocument::decode(&text).unwrap();
        assert_eq!(codes(&loaded.issues), pairs(&first), "{text}");
        let doc = &loaded.document;
        assert_eq!(doc.input.method, InputMethod::UserDefined, "{text}");
        let options = doc.engine_options();
        assert_eq!(options.input_method, InputMethod::Telex, "{text}");
        assert!(options.validate().is_ok(), "{text}");

        let encoded = doc.encode().unwrap();
        assert!(encoded.contains("method = \"user\""), "{encoded}");
        let again = ConfigDocument::decode(&encoded).unwrap();
        assert_eq!(codes(&again.issues), pairs(&later), "{encoded}");
        assert_eq!(again.document.input.method, InputMethod::UserDefined);
    }
    let valid =
        format!("version = 1\n[input]\nmethod = \"user\"\nuser_keymap = \"mine\"\n{keymap}");
    let loaded = ConfigDocument::decode(&valid).unwrap();
    assert!(loaded.is_clean(), "{:?}", loaded.issues);
    assert_eq!(loaded.document.input.method, InputMethod::UserDefined);
    assert_eq!(
        loaded.document.engine_options().input_method,
        InputMethod::UserDefined
    );
}

/// Matrix #45 and #25: a hotkey list whose every element is invalid is a corrupt field, so it
/// takes its default instead of silently disabling the mode toggle or the hard reset.
#[test]
fn a_hotkey_list_with_only_invalid_elements_takes_its_default() {
    let text =
        "version = 1\n[hotkeys]\ntoggle_mode = [\"Ctrl+Bogus\"]\nhard_reset = [\"Hyper+F12\", 3]\n";
    let loaded = ConfigDocument::decode(text).unwrap();
    assert_eq!(
        codes(&loaded.issues),
        pairs(&[
            ("hotkeys.toggle_mode[0]", "invalid-value"),
            ("hotkeys.hard_reset[0]", "invalid-value"),
            ("hotkeys.hard_reset[1]", "invalid-type"),
        ])
    );
    assert_eq!(loaded.document.hotkeys, HotkeyConfig::default());

    // An explicitly empty list is a deliberate choice and is kept.
    let none = ConfigDocument::decode("version = 1\n[hotkeys]\ntoggle_mode = []\n").unwrap();
    assert!(none.is_clean());
    assert!(none.document.hotkeys.toggle_mode.is_empty());
    assert_eq!(
        none.document.hotkeys.hard_reset,
        HotkeyConfig::default().hard_reset
    );
}

/// Unknown keys inside `[[keymaps]]` and inside each binding survive a rewrite and follow
/// their key map (by id) and binding (by key).
#[test]
fn unknown_keys_inside_keymaps_and_bindings_survive() {
    let text = r#"version = 1
[[keymaps]]
id = "first"
version = 1
name = "First"
author = "someone"
bindings = [
    { key = "s", actions = ["tone:acute"], hint = "acute" },
    { key = "f", actions = ["tone:grave"] },
]
[[keymaps]]
id = "second"
version = 1
name = "Second"
bindings = [ { key = "j", actions = ["tone:dot"], hint = "dot" } ]
"#;
    let loaded = ConfigDocument::decode(text).unwrap();
    assert!(loaded.is_clean(), "{:?}", loaded.issues);
    let mut doc = loaded.document;
    let encoded = doc.encode().unwrap();
    let parsed: toml::Table = toml::from_str(&encoded).unwrap();
    let maps = parsed["keymaps"].as_array().unwrap();
    assert_eq!(maps[0]["author"].as_str(), Some("someone"));
    assert_eq!(maps[0]["bindings"][0]["hint"].as_str(), Some("acute"));
    assert!(maps[0]["bindings"][1].get("hint").is_none());
    assert_eq!(maps[1]["bindings"][0]["hint"].as_str(), Some("dot"));
    assert!(maps[1].get("author").is_none());
    assert_eq!(ConfigDocument::decode(&encoded).unwrap().document, doc);

    // Editing the lists keeps each key with its own key map and binding.
    doc.keymaps.reverse();
    doc.keymaps[1].bindings.reverse();
    let encoded = doc.encode().unwrap();
    let parsed: toml::Table = toml::from_str(&encoded).unwrap();
    let maps = parsed["keymaps"].as_array().unwrap();
    assert_eq!(maps[0]["id"].as_str(), Some("second"));
    assert!(maps[0].get("author").is_none());
    assert_eq!(maps[1]["author"].as_str(), Some("someone"));
    assert_eq!(maps[1]["bindings"][0]["key"].as_str(), Some("f"));
    assert!(maps[1]["bindings"][0].get("hint").is_none());
    assert_eq!(maps[1]["bindings"][1]["hint"].as_str(), Some("acute"));
}

#[test]
fn issue_reasons_have_stable_codes() {
    let reasons = [
        (IssueReason::Missing, "missing"),
        (IssueReason::InvalidType, "invalid-type"),
        (IssueReason::InvalidValue, "invalid-value"),
        (IssueReason::OutOfRange, "out-of-range"),
        (IssueReason::DuplicateEntry, "duplicate-entry"),
        (IssueReason::Unresolved, "unresolved"),
    ];
    for (reason, code) in reasons {
        assert_eq!(reason.code(), code);
        assert_eq!(reason.to_string(), code);
    }
    assert_eq!(
        issue(
            "keymaps[0].bindings[2].actions[1]",
            IssueReason::InvalidValue
        )
        .to_string(),
        "keymaps[0].bindings[2].actions[1]: invalid-value"
    );
}
