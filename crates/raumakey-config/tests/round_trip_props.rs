//! Property tests.
//!
//! - Every generated document, including unknown keys inside entries and entries that could not
//!   be decoded, survives encode -> decode unchanged.
//! - Whatever the lenient decoder makes of a well-formed file re-encodes to a fixed point: the
//!   second decode yields the same document and the same text, and reports issues only for
//!   entries that are still set aside.
//! - A decoded configuration always yields valid engine options and key maps (matrix #45).
//! - The header shortcut of `poll_revision` never disagrees with a full decode.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

use proptest::collection::{btree_map, btree_set, vec};
use proptest::prelude::*;
use proptest::sample::{Index, select};

use raumakey_config::{
    AppRulesDocument, CaseTransform, CommitResult, ConfigDocument, ConfigError, ConversionSection,
    EntryExtras, ImportedCounts, InputSection, MacroDocument, MacroSection, MigrationRecord,
    MigrationStatus, QuarantinedField, RuleIdentity, SourceFingerprint, StartupMode, StoreDocument,
    SystemSection,
};
use raumakey_model::host::ExeName;
use raumakey_model::hotkey::{Hotkey, HotkeyConfig, HotkeyKey};
use raumakey_model::input::vk;
use raumakey_model::keymap::{BaseVowel, KeyAction, KeyBinding, KeyMap, Tone};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::{InputMethod, Normalization};
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, NamedSet, ProbeRecord, RuleAction, SecureFieldMode, Transport,
};

/// Largest TOML integer (macro ids).
const TOML_INT_MAX: u64 = 0x7FFF_FFFF_FFFF_FFFF;
/// Largest revision a TOML document reads (one below `TOML_INT_MAX`).
const MAX_TOML_REVISION: u64 = TOML_INT_MAX - 1;

type JsonMap = serde_json::Map<String, serde_json::Value>;

fn text() -> impl Strategy<Value = String> {
    vec(any::<char>(), 0..12).prop_map(String::from_iter)
}

/// Unknown keys never collide with schema keys because of the `x_` prefix.
fn ext_key() -> impl Strategy<Value = String> {
    "x_[a-z0-9_. -]{0,6}|x_ê[a-zA-Z\"']{0,3}"
}

fn toml_leaf() -> impl Strategy<Value = toml::Value> {
    prop_oneof![
        any::<bool>().prop_map(toml::Value::Boolean),
        any::<i64>().prop_map(toml::Value::Integer),
        any::<f64>()
            .prop_filter("finite", |f| f.is_finite())
            .prop_map(toml::Value::Float),
        text().prop_map(toml::Value::String),
    ]
}

fn toml_tree(leaf: BoxedStrategy<toml::Value>) -> impl Strategy<Value = toml::Value> {
    leaf.prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            vec(inner.clone(), 0..4).prop_map(toml::Value::Array),
            btree_map(ext_key(), inner, 0..4)
                .prop_map(|map| toml::Value::Table(map.into_iter().collect())),
        ]
    })
}

fn toml_extensions(min: usize) -> impl Strategy<Value = toml::Table> {
    btree_map(ext_key(), toml_tree(toml_leaf().boxed()), min..min + 3)
        .prop_map(|map| map.into_iter().collect())
}

/// Unknown keys of one entry (never empty).
fn unknown_keys() -> impl Strategy<Value = toml::Table> {
    toml_extensions(1)
}

/// An entry table without `missing`, so that it is set aside verbatim.
fn rejected_tables(missing: &'static str) -> impl Strategy<Value = Vec<toml::Table>> {
    vec(
        toml_extensions(1).prop_filter("no identity", move |table| !table.contains_key(missing)),
        0..2,
    )
}

fn hotkey() -> impl Strategy<Value = Hotkey> {
    let keys = vec![
        0x41,
        0x5A,
        0x30,
        0x39,
        0x01,
        0x5D,
        0xFE,
        vk::F1,
        vk::F12,
        vk::F24,
        vk::SPACE,
        vk::RETURN,
        vk::PRIOR,
        vk::DELETE,
    ];
    (any::<[bool; 4]>(), prop::option::of(select(keys)))
        .prop_map(|([ctrl, shift, alt, win], key)| Hotkey {
            ctrl,
            shift,
            alt,
            win,
            key: key.map_or(HotkeyKey::ModifiersOnly, HotkeyKey::VirtualKey),
        })
        .prop_filter("valid hotkey", |hotkey| {
            hotkey.to_string().parse::<Hotkey>().ok() == Some(*hotkey)
        })
}

fn key_action() -> impl Strategy<Value = KeyAction> {
    select(vec![
        KeyAction::Tone(Tone::Acute),
        KeyAction::Tone(Tone::Grave),
        KeyAction::Tone(Tone::Hook),
        KeyAction::Tone(Tone::Tilde),
        KeyAction::Tone(Tone::Dot),
        KeyAction::RemoveTone,
        KeyAction::Circumflex,
        KeyAction::CircumflexOn(BaseVowel::A),
        KeyAction::CircumflexOn(BaseVowel::E),
        KeyAction::CircumflexOn(BaseVowel::O),
        KeyAction::Breve,
        KeyAction::Horn,
        KeyAction::HornOrBreve,
        KeyAction::Stroke,
        KeyAction::Escape,
    ])
}

fn bindings() -> impl Strategy<Value = Vec<KeyBinding>> {
    vec((0x21u8..=0x7E, vec(key_action(), 1..=4)), 0..6).prop_map(|raw| {
        let mut seen = Vec::new();
        raw.into_iter()
            .filter_map(|(byte, actions)| {
                let folded = byte.to_ascii_lowercase();
                if seen.contains(&folded) {
                    return None;
                }
                seen.push(folded);
                Some(KeyBinding {
                    key: char::from(byte),
                    actions,
                })
            })
            .collect()
    })
}

/// Valid key maps with unique ids; names stay within the 128-character limit.
fn keymaps() -> impl Strategy<Value = Vec<KeyMap>> {
    btree_set("[A-Za-z0-9._-]{1,12}", 0..3).prop_flat_map(|ids| {
        ids.into_iter()
            .map(|id| {
                (Just(id), text(), bindings()).prop_map(|(id, name, bindings)| KeyMap {
                    id,
                    version: 1,
                    name,
                    bindings,
                })
            })
            .collect::<Vec<_>>()
    })
}

/// Key maps plus their extras: unknown keys of key maps and of bindings (nested under
/// `bindings` by key), and key maps without an id, which are set aside.
fn keymaps_with_extras() -> impl Strategy<Value = (Vec<KeyMap>, EntryExtras<String>)> {
    (
        keymaps(),
        vec(prop::option::of(unknown_keys()), 3),
        vec(vec(prop::option::of(unknown_keys()), 6), 3),
        rejected_tables("id"),
    )
        .prop_map(|(maps, map_unknown, binding_unknown, rejected)| {
            let mut extras = EntryExtras::default();
            for (index, map) in maps.iter().enumerate() {
                let mut record = map_unknown
                    .get(index)
                    .cloned()
                    .flatten()
                    .unwrap_or_default();
                let mut nested = toml::Table::new();
                for (position, binding) in map.bindings.iter().enumerate() {
                    if let Some(Some(keys)) = binding_unknown
                        .get(index)
                        .and_then(|bindings| bindings.get(position))
                    {
                        nested.insert(binding.key.to_string(), toml::Value::Table(keys.clone()));
                    }
                }
                if !nested.is_empty() {
                    record.insert("bindings".to_owned(), toml::Value::Table(nested));
                }
                if !record.is_empty() {
                    extras.unknown_keys.push((map.id.clone(), record));
                }
            }
            extras.rejected = rejected;
            (maps, extras)
        })
}

fn input_section() -> impl Strategy<Value = InputSection> {
    let methods = vec![
        InputMethod::Telex,
        InputMethod::Vni,
        InputMethod::Viqr,
        InputMethod::UserDefined,
    ];
    let forms = vec![
        Normalization::Nfc,
        Normalization::Nfd,
        Normalization::Preserve,
    ];
    (
        any::<[bool; 5]>(),
        select(methods),
        prop::option::of("[A-Za-z0-9._-]{1,64}"),
        select(forms),
        8u32..=128,
        toml_extensions(0),
    )
        .prop_map(
            |(flags, method, user_keymap, normalization, context_limit, extensions)| InputSection {
                enabled: flags[0],
                method,
                user_keymap,
                normalization,
                modern_tone: flags[1],
                free_marking: flags[2],
                spell_check: flags[3],
                auto_restore_invalid: flags[4],
                context_limit,
                extensions,
            },
        )
}

fn macro_section() -> impl Strategy<Value = MacroSection> {
    (any::<[bool; 3]>(), toml_extensions(0)).prop_map(|(flags, extensions)| MacroSection {
        enabled: flags[0],
        when_disabled: flags[1],
        auto_case: flags[2],
        extensions,
    })
}

fn system_section() -> impl Strategy<Value = SystemSection> {
    let modes = vec![
        StartupMode::None,
        StartupMode::Shortcut,
        StartupMode::ElevatedTask,
    ];
    (select(modes), any::<[bool; 2]>(), toml_extensions(0)).prop_map(
        |(startup, flags, extensions)| SystemSection {
            startup,
            show_panel_on_start: flags[0],
            diagnostics: flags[1],
            extensions,
        },
    )
}

fn conversion_section() -> impl Strategy<Value = ConversionSection> {
    let cases = vec![
        CaseTransform::None,
        CaseTransform::Upper,
        CaseTransform::Lower,
        CaseTransform::Title,
    ];
    (
        text(),
        text(),
        select(cases),
        any::<bool>(),
        toml_extensions(0),
    )
        .prop_map(
            |(source, target, case, remove_tones, extensions)| ConversionSection {
                source,
                target,
                case,
                remove_tones,
                extensions,
            },
        )
}

fn config_document() -> impl Strategy<Value = ConfigDocument> {
    (
        0..=MAX_TOML_REVISION,
        input_section(),
        macro_section(),
        (vec(hotkey(), 0..3), vec(hotkey(), 0..3)),
        system_section(),
        conversion_section(),
        keymaps_with_extras(),
        toml_extensions(0),
        prop::option::of(toml_extensions(1)),
        (any::<Index>(), any::<bool>()),
    )
        .prop_map(
            |(
                revision,
                mut input,
                macros,
                (toggle_mode, hard_reset),
                system,
                conversion,
                (keymaps, keymap_extras),
                mut extensions,
                hotkey_extensions,
                (pick, resolve),
            )| {
                // Mostly let the user method name a key map that decodes; otherwise it stays
                // unresolved (kept, and reported on every load).
                if input.method == InputMethod::UserDefined && resolve && !keymaps.is_empty() {
                    input.user_keymap = Some(keymaps[pick.index(keymaps.len())].id.clone());
                }
                if let Some(table) = hotkey_extensions {
                    extensions.insert("hotkeys".into(), toml::Value::Table(table));
                }
                ConfigDocument {
                    version: 1,
                    revision,
                    input,
                    macros,
                    hotkeys: HotkeyConfig {
                        toggle_mode,
                        hard_reset,
                    },
                    system,
                    conversion,
                    keymaps,
                    keymap_extras,
                    extensions,
                }
            },
        )
}

fn macro_document() -> impl Strategy<Value = MacroDocument> {
    let entry = (
        text(),
        text(),
        any::<bool>(),
        select(vec![BoundaryPolicy::Delimiter, BoundaryPolicy::Whitespace]),
        select(vec![CasePolicy::Adapt, CasePolicy::Exact]),
        prop::option::of(unknown_keys()),
    );
    (
        0..=MAX_TOML_REVISION,
        btree_set(0..=TOML_INT_MAX, 0..5),
        vec(entry, 5),
        rejected_tables("id"),
        toml_extensions(0),
    )
        .prop_map(|(revision, ids, parts, rejected, extensions)| {
            let mut extras = EntryExtras::default();
            let entries = ids
                .into_iter()
                .zip(parts)
                .map(
                    |(id, (trigger, replacement, enabled, boundary, case_policy, unknown))| {
                        if let Some(keys) = unknown {
                            extras
                                .unknown_keys
                                .push(((MacroId(id), trigger.clone()), keys));
                        }
                        MacroEntry {
                            id: MacroId(id),
                            trigger,
                            replacement,
                            enabled,
                            boundary,
                            case_policy,
                        }
                    },
                )
                .collect();
            extras.rejected = rejected;
            MacroDocument {
                version: 1,
                revision,
                entries,
                entry_extras: extras,
                extensions,
            }
        })
}

fn exe_text() -> impl Strategy<Value = String> {
    "[a-z0-9_.-]{1,12}"
}

fn rule_action() -> impl Strategy<Value = RuleAction> {
    let transports = vec![
        Transport::Tsf,
        Transport::SyntheticKey,
        Transport::Clipboard,
    ];
    (
        prop::option::of(select(transports)),
        prop::option::of(any::<bool>()),
        prop::option::of(any::<bool>()),
        prop::option::of(any::<bool>()),
        prop::option::of(any::<u32>()),
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

/// One entry of each array with its optional unknown keys.
type Extra = Option<toml::Table>;

fn app_rules_document() -> impl Strategy<Value = AppRulesDocument> {
    let rules = btree_set((exe_text(), prop::option::of(text())), 0..3).prop_flat_map(|ids| {
        ids.into_iter()
            .map(|(exe, class)| {
                (
                    Just(exe),
                    Just(class),
                    rule_action(),
                    prop::option::of(unknown_keys()),
                )
            })
            .collect::<Vec<_>>()
    });
    let named_sets = btree_set(text(), 0..3).prop_flat_map(|names| {
        names
            .into_iter()
            .map(|name| {
                (
                    Just(name),
                    vec(exe_text(), 0..4),
                    rule_action(),
                    prop::option::of(unknown_keys()),
                    prop::option::of(unknown_keys()),
                )
            })
            .collect::<Vec<_>>()
    });
    let probes = btree_set(exe_text(), 0..3).prop_flat_map(|exes| {
        exes.into_iter()
            .map(|exe| {
                (
                    Just(exe),
                    any::<u32>(),
                    any::<bool>(),
                    prop::option::of(unknown_keys()),
                )
            })
            .collect::<Vec<_>>()
    });
    (
        0..=MAX_TOML_REVISION,
        any::<[bool; 2]>(),
        select(vec![SecureFieldMode::Bypass, SecureFieldMode::Restricted]),
        any::<u32>(),
        rules,
        named_sets,
        probes,
        (
            rejected_tables("exe"),
            rejected_tables("name"),
            rejected_tables("exe"),
        ),
        toml_extensions(0),
    )
        .prop_map(
            |(
                revision,
                flags,
                secure_field_mode,
                default_max_delete_before,
                rules,
                named_sets,
                probes,
                (rejected_rules, rejected_sets, rejected_probes),
                extensions,
            )| {
                let exe = |name: &str| ExeName::new(name).unwrap();
                let mut rule_extras: EntryExtras<RuleIdentity> = EntryExtras::default();
                let rules = rules
                    .into_iter()
                    .map(
                        |(name, window_class, action, unknown): (
                            String,
                            Option<String>,
                            RuleAction,
                            Extra,
                        )| {
                            if let Some(keys) = unknown {
                                rule_extras
                                    .unknown_keys
                                    .push(((exe(&name), window_class.clone()), keys));
                            }
                            AppRule {
                                exe: exe(&name),
                                window_class,
                                action,
                            }
                        },
                    )
                    .collect();
                rule_extras.rejected = rejected_rules;
                let mut named_set_extras: EntryExtras<String> = EntryExtras::default();
                let named_sets = named_sets
                    .into_iter()
                    .map(
                        |(name, members, action, unknown, action_unknown): (
                            String,
                            Vec<String>,
                            RuleAction,
                            Extra,
                            Extra,
                        )| {
                            let mut record = unknown.unwrap_or_default();
                            if let Some(keys) = action_unknown {
                                record.insert("action".to_owned(), toml::Value::Table(keys));
                            }
                            if !record.is_empty() {
                                named_set_extras.unknown_keys.push((name.clone(), record));
                            }
                            NamedSet {
                                name,
                                members: members.iter().map(|member| exe(member)).collect(),
                                action,
                            }
                        },
                    )
                    .collect();
                named_set_extras.rejected = rejected_sets;
                let mut probe_extras: EntryExtras<ExeName> = EntryExtras::default();
                let probes = probes
                    .into_iter()
                    .map(
                        |(name, probe_version, tsf_range_edit, unknown): (
                            String,
                            u32,
                            bool,
                            Extra,
                        )| {
                            if let Some(keys) = unknown {
                                probe_extras.unknown_keys.push((exe(&name), keys));
                            }
                            ProbeRecord {
                                exe: exe(&name),
                                probe_version,
                                tsf_range_edit,
                            }
                        },
                    )
                    .collect();
                probe_extras.rejected = rejected_probes;
                AppRulesDocument {
                    version: 1,
                    revision,
                    compatibility: CompatibilityConfig {
                        clipboard_enabled: flags[0],
                        default_reset_on_mouse: flags[1],
                        secure_field_mode,
                        default_max_delete_before,
                        rules,
                        named_sets,
                        probes,
                    },
                    rule_extras,
                    named_set_extras,
                    probe_extras,
                    extensions,
                }
            },
        )
}

fn json_leaf() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![
        Just(serde_json::Value::Null),
        any::<bool>().prop_map(serde_json::Value::Bool),
        any::<i64>().prop_map(serde_json::Value::from),
        any::<u64>().prop_map(serde_json::Value::from),
        text().prop_map(serde_json::Value::String),
    ]
}

fn json_value() -> impl Strategy<Value = serde_json::Value> {
    json_leaf().prop_recursive(3, 24, 4, |inner| {
        prop_oneof![
            vec(inner.clone(), 0..4).prop_map(serde_json::Value::Array),
            btree_map(text(), inner, 0..4)
                .prop_map(|map| serde_json::Value::Object(map.into_iter().collect())),
        ]
    })
}

fn json_unknown_keys() -> impl Strategy<Value = JsonMap> {
    btree_map(ext_key(), json_value(), 1..3).prop_map(|map| map.into_iter().collect())
}

/// A record of `MigrationRecord::extensions` for an entry with unknown keys: its identity
/// fields plus those keys.
fn identity_record(names: [&str; 2], values: [&str; 2], mut keys: JsonMap) -> serde_json::Value {
    for (name, value) in names.into_iter().zip(values) {
        keys.insert(name.to_owned(), serde_json::Value::String(value.to_owned()));
    }
    serde_json::Value::Object(keys)
}

fn migration_record() -> impl Strategy<Value = MigrationRecord> {
    let statuses = vec![
        MigrationStatus::NotStarted,
        MigrationStatus::InProgress,
        MigrationStatus::Completed,
        MigrationStatus::RolledBack,
        MigrationStatus::Failed,
    ];
    let sources = btree_map(
        (text(), text()),
        (
            "[0-9a-f]{64}",
            any::<u64>(),
            prop::option::of(json_unknown_keys()),
        ),
        0..3,
    );
    let quarantined = btree_map(
        (text(), text()),
        (text(), prop::option::of(json_unknown_keys())),
        0..3,
    );
    let imported = (any::<[u64; 4]>(), prop::option::of(json_unknown_keys()));
    let result = prop::option::of((
        any::<[u64; 3]>(),
        btree_map(text(), "[0-9a-f]{64}", 0..3),
        prop::option::of(json_unknown_keys()),
    ));
    let extensions =
        btree_map(ext_key(), json_value(), 0..3).prop_map(|map| map.into_iter().collect());
    (
        0..u64::MAX,
        select(statuses),
        any::<[u64; 2]>(),
        sources,
        imported,
        quarantined,
        json_value(),
        result,
        extensions,
    )
        .prop_map(
            |(
                revision,
                status,
                times,
                sources,
                (counts, imported_unknown),
                quarantined,
                preserved,
                result,
                mut extensions,
            ): (_, _, _, _, _, _, _, _, JsonMap)| {
                let mut source_records = Vec::new();
                let sources = sources
                    .into_iter()
                    .map(|((kind, location), (sha256, size, unknown))| {
                        if let Some(keys) = unknown {
                            source_records.push(identity_record(
                                ["kind", "location"],
                                [&kind, &location],
                                keys,
                            ));
                        }
                        SourceFingerprint {
                            kind,
                            location,
                            sha256,
                            size,
                        }
                    })
                    .collect();
                let mut quarantined_records = Vec::new();
                let quarantined = quarantined
                    .into_iter()
                    .map(|((source, field), (reason_code, unknown))| {
                        if let Some(keys) = unknown {
                            quarantined_records.push(identity_record(
                                ["source", "field"],
                                [&source, &field],
                                keys,
                            ));
                        }
                        QuarantinedField {
                            source,
                            field,
                            reason_code,
                        }
                    })
                    .collect();
                if !source_records.is_empty() {
                    extensions.insert(
                        "sources".to_owned(),
                        serde_json::Value::Array(source_records),
                    );
                }
                if !quarantined_records.is_empty() {
                    extensions.insert(
                        "quarantined".to_owned(),
                        serde_json::Value::Array(quarantined_records),
                    );
                }
                if let Some(keys) = imported_unknown {
                    extensions.insert("imported".to_owned(), serde_json::Value::Object(keys));
                }
                let result = result.map(|(revisions, checksums, unknown)| {
                    if let Some(keys) = unknown {
                        extensions.insert("result".to_owned(), serde_json::Value::Object(keys));
                    }
                    CommitResult {
                        config_revision: revisions[0],
                        macros_revision: revisions[1],
                        app_rules_revision: revisions[2],
                        checksums: checksums.into_iter().collect::<BTreeMap<_, _>>(),
                    }
                });
                MigrationRecord {
                    version: 1,
                    revision,
                    status,
                    started_at_ms: times[0],
                    completed_at_ms: times[1],
                    sources,
                    imported: ImportedCounts {
                        settings: counts[0],
                        macros: counts[1],
                        named_sets: counts[2],
                        probes: counts[3],
                    },
                    quarantined,
                    preserved,
                    result,
                    extensions,
                }
            },
        )
}

/// How many problems of a document are reported on every load: entries that were set aside
/// and, for the configuration, a user method without a usable key map.
trait LastingIssues {
    fn lasting_issues(&self) -> usize;
}

impl LastingIssues for ConfigDocument {
    fn lasting_issues(&self) -> usize {
        let unresolved_user_method = self.input.method == InputMethod::UserDefined
            && !self
                .keymaps
                .iter()
                .any(|map| Some(map.id.as_str()) == self.input.user_keymap.as_deref());
        self.keymap_extras.rejected.len() + usize::from(unresolved_user_method)
    }
}

impl LastingIssues for MacroDocument {
    fn lasting_issues(&self) -> usize {
        self.entry_extras.rejected.len()
    }
}

impl LastingIssues for AppRulesDocument {
    fn lasting_issues(&self) -> usize {
        self.rule_extras.rejected.len()
            + self.named_set_extras.rejected.len()
            + self.probe_extras.rejected.len()
    }
}

impl LastingIssues for MigrationRecord {
    fn lasting_issues(&self) -> usize {
        0
    }
}

fn fail(error: impl Debug) -> TestCaseError {
    TestCaseError::fail(format!("{error:?}"))
}

/// encode -> decode returns the document; only lasting problems are reported; a TOML file
/// written by the encoder is recognised by the header shortcut.
fn assert_round_trip<D: StoreDocument + LastingIssues + PartialEq + Debug>(
    document: &D,
    toml_header: bool,
) -> Result<(), TestCaseError> {
    let text = document.encode().map_err(fail)?;
    let loaded = D::decode(&text).map_err(|error| fail((error, &text)))?;
    prop_assert_eq!(
        loaded.issues.is_empty(),
        document.lasting_issues() == 0,
        "{:?}\n{}",
        loaded.issues,
        text
    );
    prop_assert_eq!(&loaded.document, document, "{}", text);
    let expected = toml_header.then_some(document.revision());
    prop_assert_eq!(D::revision_from_head(text.as_bytes()), expected, "{}", text);
    Ok(())
}

/// Whatever decode makes of `text` re-encodes to a fixed point.
fn assert_fixed_point<D: StoreDocument + LastingIssues + PartialEq + Debug>(
    text: &str,
) -> Result<Option<D>, TestCaseError> {
    let first = match D::decode(text) {
        Ok(first) => first,
        Err(error) => {
            prop_assert_eq!(error, ConfigError::UnsupportedVersion, "{}", text);
            return Ok(None);
        }
    };
    let encoded = first.document.encode().map_err(fail)?;
    let second = D::decode(&encoded).map_err(|error| fail((error, &encoded)))?;
    prop_assert_eq!(&second.document, &first.document, "{}", encoded);
    prop_assert_eq!(
        second.issues.is_empty(),
        first.document.lasting_issues() == 0,
        "{:?}\n{}",
        second.issues,
        encoded
    );
    prop_assert_eq!(&second.document.encode().map_err(fail)?, &encoded);
    if let Some(revision) = D::revision_from_head(encoded.as_bytes()) {
        prop_assert_eq!(revision, first.document.revision());
    }
    Ok(Some(first.document))
}

/// The header shortcut agrees with decode whenever it answers.
fn assert_header_agrees<D: StoreDocument>(text: &str) -> Result<(), TestCaseError> {
    if let Some(revision) = D::revision_from_head(text.as_bytes()) {
        match D::decode(text) {
            Ok(loaded) => prop_assert_eq!(loaded.document.revision(), revision),
            Err(error) => prop_assert_eq!(error, ConfigError::Corrupt),
        }
    }
    Ok(())
}

/// Leaves biased toward values the schemas accept, so decoding takes both paths.
///
/// JSON leaves leave out floats: `serde_json` (without its `float_roundtrip` feature) may
/// read a printed float back one ULP off, which is unrelated to the decoder under test.
fn schema_leaf(words: Vec<&'static str>, floats: bool) -> BoxedStrategy<toml::Value> {
    let leaf = toml_leaf().prop_filter("JSON leaves have no floats", move |value| {
        floats || !value.is_float()
    });
    prop_oneof![
        3 => leaf,
        3 => select(words).prop_map(|w| toml::Value::String(w.to_owned())),
        2 => (-3i64..200).prop_map(toml::Value::Integer),
        1 => any::<bool>().prop_map(toml::Value::Boolean),
    ]
    .boxed()
}

/// A random table whose keys come from a schema (at every level) or are unknown.
fn schema_table(
    top_keys: Vec<&'static str>,
    inner_keys: Vec<&'static str>,
    leaf: BoxedStrategy<toml::Value>,
) -> impl Strategy<Value = toml::Table> {
    let value = leaf.prop_recursive(3, 40, 6, move |inner| {
        let key = select(inner_keys.clone()).prop_map(str::to_owned);
        prop_oneof![
            vec(inner.clone(), 0..5).prop_map(toml::Value::Array),
            btree_map(key, inner, 0..6)
                .prop_map(|map| toml::Value::Table(map.into_iter().collect())),
        ]
    });
    btree_map(select(top_keys).prop_map(str::to_owned), value, 0..8)
        .prop_map(|map| map.into_iter().collect())
}

fn config_random_table() -> impl Strategy<Value = toml::Table> {
    let keys = vec![
        "version",
        "revision",
        "input",
        "macros",
        "hotkeys",
        "system",
        "conversion",
        "keymaps",
        "enabled",
        "method",
        "user_keymap",
        "normalization",
        "modern_tone",
        "context_limit",
        "when_disabled",
        "toggle_mode",
        "hard_reset",
        "startup",
        "diagnostics",
        "source",
        "case",
        "id",
        "name",
        "bindings",
        "key",
        "actions",
        "x_unknown",
    ];
    let words = vec![
        "telex",
        "vni",
        "viqr",
        "user",
        "nfc",
        "nfd",
        "preserve",
        "none",
        "shortcut",
        "elevated-task",
        "upper",
        "title",
        "unicode",
        "Ctrl+Shift",
        "Alt+Z",
        "Hyper+Q",
        "tone:acute",
        "horn",
        "bogus",
        "s",
        "S",
        "my-map",
        "bad id",
        "",
    ];
    schema_table(keys.clone(), keys, schema_leaf(words, true))
}

fn macro_random_table() -> impl Strategy<Value = toml::Table> {
    let keys = vec![
        "version",
        "revision",
        "macro",
        "id",
        "trigger",
        "replacement",
        "enabled",
        "boundary",
        "case",
        "x_unknown",
    ];
    let words = vec![
        "delimiter",
        "whitespace",
        "Whitespace",
        "sentence",
        "adapt",
        "exact",
        "vn",
        "Việt Nam",
        "",
    ];
    schema_table(keys.clone(), keys, schema_leaf(words, true))
}

fn app_rules_random_table() -> impl Strategy<Value = toml::Table> {
    let keys = vec![
        "version",
        "revision",
        "clipboard_enabled",
        "default_reset_on_mouse",
        "secure_field_mode",
        "default_max_delete_before",
        "rule",
        "named_set",
        "probe",
        "exe",
        "window_class",
        "transport",
        "reset_on_mouse",
        "macros",
        "vietnamese",
        "max_delete_before",
        "name",
        "members",
        "action",
        "probe_version",
        "tsf_range_edit",
        "x_unknown",
    ];
    let words = vec![
        "notepad.exe",
        "Chrome.EXE",
        " wt.exe ",
        "C:/apps/x.exe",
        "tsf",
        "synthetic-key",
        "clipboard",
        "warp",
        "bypass",
        "restricted",
        "chromium-family",
        "",
    ];
    schema_table(keys.clone(), keys, schema_leaf(words, true))
}

fn migration_random_table() -> impl Strategy<Value = toml::Table> {
    let keys = vec![
        "version",
        "revision",
        "status",
        "started_at_ms",
        "completed_at_ms",
        "sources",
        "imported",
        "quarantined",
        "preserved",
        "result",
        "kind",
        "location",
        "sha256",
        "size",
        "settings",
        "macros",
        "named_sets",
        "probes",
        "source",
        "field",
        "reason_code",
        "config_revision",
        "macros_revision",
        "app_rules_revision",
        "checksums",
        "x_unknown",
    ];
    let words = vec![
        "completed",
        "in-progress",
        "half-done",
        "registry",
        "macro-text",
        "config.toml",
        "",
    ];
    schema_table(keys.clone(), keys, schema_leaf(words, false))
}

// Biased generators: entries whose fields are usually valid, sometimes absent, and sometimes
// of any type, with unknown keys, so that the field-level paths are exercised.

/// A field that is usually valid, sometimes absent, and sometimes anything.
fn mostly(valid: BoxedStrategy<toml::Value>) -> BoxedStrategy<Option<toml::Value>> {
    prop_oneof![
        6 => valid.prop_map(Some),
        1 => Just(None),
        1 => toml_leaf().prop_map(Some),
    ]
    .boxed()
}

/// A field that is absent half of the time, else as [`mostly`].
fn optional(valid: BoxedStrategy<toml::Value>) -> BoxedStrategy<Option<toml::Value>> {
    prop_oneof![Just(None), mostly(valid)].boxed()
}

fn word(words: Vec<&'static str>) -> BoxedStrategy<toml::Value> {
    select(words)
        .prop_map(|word| toml::Value::String(word.to_owned()))
        .boxed()
}

fn int(range: core::ops::Range<i64>) -> BoxedStrategy<toml::Value> {
    range.prop_map(toml::Value::Integer).boxed()
}

fn boolean() -> BoxedStrategy<toml::Value> {
    any::<bool>().prop_map(toml::Value::Boolean).boxed()
}

fn string() -> BoxedStrategy<toml::Value> {
    text().prop_map(toml::Value::String).boxed()
}

/// A table of the given fields (each present or not) plus unknown keys.
fn fields_table(
    fields: Vec<(&'static str, BoxedStrategy<Option<toml::Value>>)>,
) -> BoxedStrategy<toml::Table> {
    let (keys, values): (Vec<&'static str>, Vec<_>) = fields.into_iter().unzip();
    (values, toml_extensions(0))
        .prop_map(move |(values, mut table)| {
            for (key, value) in keys.iter().zip(values) {
                if let Some(value) = value {
                    table.insert((*key).to_owned(), value);
                }
            }
            table
        })
        .boxed()
}

fn table_value(
    fields: Vec<(&'static str, BoxedStrategy<Option<toml::Value>>)>,
) -> BoxedStrategy<toml::Value> {
    fields_table(fields).prop_map(toml::Value::Table).boxed()
}

/// An array of `element`s with an occasional element that is not a table.
fn array_of(element: BoxedStrategy<toml::Value>, max: usize) -> BoxedStrategy<toml::Value> {
    vec(prop_oneof![9 => element, 1 => toml_leaf()], 0..max)
        .prop_map(toml::Value::Array)
        .boxed()
}

fn header_fields() -> Vec<(&'static str, BoxedStrategy<Option<toml::Value>>)> {
    vec![
        ("version", mostly(Just(toml::Value::Integer(1)).boxed())),
        ("revision", optional(int(0..1000))),
    ]
}

fn binding_value() -> BoxedStrategy<toml::Value> {
    let actions = vec![
        "tone:acute",
        "tone:grave",
        "horn",
        "breve",
        "stroke",
        "circumflex:a",
        "remove-tone",
        "escape",
        "bogus",
    ];
    table_value(vec![
        (
            "key",
            mostly(word(vec!["s", "S", "f", "1", "[", "ab", "", " "])),
        ),
        ("actions", mostly(array_of(word(actions), 6))),
    ])
}

fn keymap_value() -> BoxedStrategy<toml::Value> {
    table_value(vec![
        ("id", mostly(word(vec!["a", "b", "my-map", "bad id", ""]))),
        (
            "version",
            optional(prop_oneof![4 => Just(toml::Value::Integer(1)), 1 => int(0..3)].boxed()),
        ),
        ("name", mostly(string())),
        ("bindings", mostly(array_of(binding_value(), 5))),
    ])
}

fn config_file() -> BoxedStrategy<toml::Table> {
    let hotkeys = vec!["Ctrl+Shift", "Alt+Z", "Ctrl+Shift+Alt+F12", "Hyper+Q", ""];
    let input = table_value(vec![
        ("method", mostly(word(vec!["telex", "vni", "user", "User"]))),
        (
            "user_keymap",
            optional(word(vec!["a", "b", "my-map", "bad id", ""])),
        ),
        ("context_limit", optional(int(0..200))),
        ("spell_check", optional(boolean())),
    ]);
    let hotkey_table = table_value(vec![
        ("toggle_mode", optional(array_of(word(hotkeys.clone()), 3))),
        ("hard_reset", optional(array_of(word(hotkeys), 3))),
    ]);
    let mut fields = header_fields();
    fields.extend([
        ("input", optional(input)),
        ("hotkeys", optional(hotkey_table)),
        ("keymaps", mostly(array_of(keymap_value(), 4))),
    ]);
    fields_table(fields)
}

fn macro_file() -> BoxedStrategy<toml::Table> {
    let entry = table_value(vec![
        ("id", mostly(int(0..20))),
        ("trigger", mostly(string())),
        ("replacement", mostly(string())),
        ("enabled", optional(boolean())),
        (
            "boundary",
            optional(word(vec![
                "delimiter",
                "whitespace",
                "Whitespace",
                "sentence",
            ])),
        ),
        ("case", optional(word(vec!["adapt", "exact", "Exact"]))),
    ]);
    let mut fields = header_fields();
    fields.push(("macro", mostly(array_of(entry, 6))));
    fields_table(fields)
}

fn action_fields() -> Vec<(&'static str, BoxedStrategy<Option<toml::Value>>)> {
    vec![
        (
            "transport",
            optional(word(vec!["tsf", "synthetic-key", "clipboard", "warp"])),
        ),
        ("reset_on_mouse", optional(boolean())),
        ("macros", optional(boolean())),
        ("vietnamese", optional(boolean())),
        (
            "max_delete_before",
            optional(prop_oneof![int(0..100), Just(toml::Value::Integer(-1))].boxed()),
        ),
    ]
}

fn app_rules_file() -> BoxedStrategy<toml::Table> {
    let exes = || {
        word(vec![
            "notepad.exe",
            "Chrome.EXE",
            " wt.exe ",
            "C:/apps/x.exe",
            "",
        ])
    };
    let mut rule = vec![
        ("exe", mostly(exes())),
        (
            "window_class",
            optional(word(vec!["Edit", "Chrome_WidgetWin_1"])),
        ),
    ];
    rule.extend(action_fields());
    let named_set = table_value(vec![
        (
            "name",
            mostly(word(vec!["chromium-family", "terminals", ""])),
        ),
        ("members", optional(array_of(exes(), 4))),
        ("action", optional(table_value(action_fields()))),
    ]);
    let probe = table_value(vec![
        ("exe", mostly(exes())),
        ("probe_version", mostly(int(0..3))),
        ("tsf_range_edit", mostly(boolean())),
    ]);
    let mut fields = header_fields();
    fields.extend([
        ("clipboard_enabled", optional(boolean())),
        (
            "secure_field_mode",
            optional(word(vec!["bypass", "restricted", "paranoid"])),
        ),
        ("default_max_delete_before", optional(int(-1..100))),
        ("rule", mostly(array_of(table_value(rule), 4))),
        ("named_set", mostly(array_of(named_set, 3))),
        ("probe", mostly(array_of(probe, 3))),
    ]);
    fields_table(fields)
}

fn migration_file() -> BoxedStrategy<toml::Table> {
    let names = || word(vec!["registry", "macro-text", "settings-ini"]);
    let source = table_value(vec![
        ("kind", mostly(names())),
        (
            "location",
            mostly(word(vec!["HKCU\\Software\\MyQ\\EVKey", "evkmacro.txt"])),
        ),
        ("sha256", mostly(string())),
        ("size", mostly(int(0..100))),
    ]);
    let quarantined = table_value(vec![
        ("source", mostly(names())),
        ("field", mostly(word(vec!["Hotkey", "line 7"]))),
        (
            "reason_code",
            mostly(word(vec!["unmapped", "invalid-line"])),
        ),
    ]);
    let imported = table_value(vec![
        ("settings", optional(int(0..5))),
        ("macros", optional(int(-1..5))),
    ]);
    let result = table_value(vec![
        ("config_revision", mostly(int(0..5))),
        ("macros_revision", mostly(int(0..5))),
        ("app_rules_revision", mostly(int(0..5))),
        (
            "checksums",
            optional(table_value(vec![("config.toml", mostly(string()))])),
        ),
    ]);
    let mut fields = header_fields();
    fields.extend([
        (
            "status",
            optional(word(vec!["completed", "in-progress", "half-done"])),
        ),
        ("sources", mostly(array_of(source, 3))),
        ("quarantined", optional(array_of(quarantined, 3))),
        ("imported", optional(imported)),
        ("result", optional(result)),
    ]);
    fields_table(fields)
}

/// JSON without floats (see [`schema_leaf`]).
fn json_without_floats(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Number(number) if number.is_f64() => {
            serde_json::Value::String(number.to_string())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(json_without_floats).collect())
        }
        serde_json::Value::Object(map) => serde_json::Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, json_without_floats(value)))
                .collect(),
        ),
        other => other,
    }
}

fn config_like_table() -> impl Strategy<Value = toml::Table> {
    prop_oneof![config_file(), config_random_table().boxed()]
}

fn macro_like_table() -> impl Strategy<Value = toml::Table> {
    prop_oneof![macro_file(), macro_random_table().boxed()]
}

fn app_rules_like_table() -> impl Strategy<Value = toml::Table> {
    prop_oneof![app_rules_file(), app_rules_random_table().boxed()]
}

fn migration_like_json() -> impl Strategy<Value = serde_json::Value> {
    prop_oneof![migration_file(), migration_random_table().boxed()].prop_map(|table| {
        json_without_floats(serde_json::to_value(table).unwrap_or(serde_json::Value::Null))
    })
}

/// A canonical header followed by any of `tails`.
fn headed_text(tails: BoxedStrategy<String>) -> impl Strategy<Value = String> {
    (prop_oneof![0..=MAX_TOML_REVISION, any::<u64>()], tails)
        .prop_map(|(revision, tail)| format!("version = 1\nrevision = {revision}\n{tail}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn config_documents_round_trip(document in config_document()) {
        assert_round_trip(&document, true)?;
    }

    #[test]
    fn macro_documents_round_trip(document in macro_document()) {
        assert_round_trip(&document, true)?;
    }

    #[test]
    fn app_rules_documents_round_trip(document in app_rules_document()) {
        assert_round_trip(&document, true)?;
    }

    #[test]
    fn migration_records_round_trip(record in migration_record()) {
        assert_round_trip(&record, false)?;
    }

    /// Whatever the lenient decoder makes of a well-formed file re-encodes to a fixed point,
    /// and a decoded configuration is always usable as is (matrix #45).
    #[test]
    fn lenient_config_decoding_reaches_a_fixed_point(table in config_like_table()) {
        let text = toml::to_string(&table).unwrap();
        if let Some(doc) = assert_fixed_point::<ConfigDocument>(&text)? {
            let options = doc.engine_options();
            prop_assert!(options.validate().is_ok(), "{}", text);
            if options.input_method == InputMethod::UserDefined {
                let named = options.user_keymap_id.as_deref();
                prop_assert!(doc.keymaps.iter().any(|map| Some(map.id.as_str()) == named));
            } else if doc.input.method != InputMethod::UserDefined {
                prop_assert_eq!(options.input_method, doc.input.method);
            }
            let mut ids = BTreeSet::new();
            for map in &doc.keymaps {
                prop_assert!(map.validate().is_ok(), "{:?}", map);
                prop_assert!(ids.insert(map.id.clone()), "duplicate key map id");
            }
            prop_assert!(!doc.hotkeys.toggle_mode.is_empty() || text.contains("toggle_mode"));
        }
    }

    #[test]
    fn lenient_macro_decoding_reaches_a_fixed_point(table in macro_like_table()) {
        let text = toml::to_string(&table).unwrap();
        assert_fixed_point::<MacroDocument>(&text)?;
    }

    #[test]
    fn lenient_app_rules_decoding_reaches_a_fixed_point(table in app_rules_like_table()) {
        let text = toml::to_string(&table).unwrap();
        assert_fixed_point::<AppRulesDocument>(&text)?;
    }

    #[test]
    fn lenient_migration_decoding_reaches_a_fixed_point(value in migration_like_json()) {
        let text = serde_json::to_string(&value).unwrap();
        if value.is_object() {
            assert_fixed_point::<MigrationRecord>(&text)?;
        } else {
            prop_assert_eq!(MigrationRecord::decode(&text).unwrap_err(), ConfigError::Corrupt);
        }
    }

    /// `poll_revision` may answer from the header alone; that answer never contradicts a full
    /// decode of the same text.
    #[test]
    fn the_header_shortcut_never_disagrees_with_decode(
        text in prop_oneof![
            headed_text(any::<String>().boxed()),
            headed_text(
                config_like_table()
                    .prop_map(|table| toml::to_string(&table).unwrap())
                    .boxed()
            ),
            headed_text(
                macro_like_table()
                    .prop_map(|table| toml::to_string(&table).unwrap())
                    .boxed()
            ),
            any::<String>(),
        ]
    ) {
        assert_header_agrees::<ConfigDocument>(&text)?;
        assert_header_agrees::<MacroDocument>(&text)?;
        assert_header_agrees::<AppRulesDocument>(&text)?;
        assert_header_agrees::<MigrationRecord>(&text)?;
    }

    #[test]
    fn decoding_arbitrary_text_never_panics(text in any::<String>()) {
        let _ = ConfigDocument::decode(&text);
        let _ = MacroDocument::decode(&text);
        let _ = AppRulesDocument::decode(&text);
        let _ = MigrationRecord::decode(&text);
    }
}
