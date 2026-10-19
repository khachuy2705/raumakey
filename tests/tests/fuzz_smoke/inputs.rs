//! Seeded input generators, one per target.
//!
//! Every target accepts any byte string, so plain random bytes are valid inputs. These
//! generators bias the bytes towards the shapes each target can do the most with: key scripts,
//! UTF-16 rich in surrogates, TOML and JSON documents made of schema keys, macro files in every
//! encoding, and well-framed IPC streams whose bodies come from real messages.

use raumakey_ipc::frame::{Frame, HEADER_LEN, encode_frame};
use raumakey_ipc::message::{Arch, ClientKind, Command, MacroSnapshot, Message, RuntimeSnapshot};
use raumakey_model::diag::{Component, DiagCode, DiagEvent, VectorId};
use raumakey_model::host::ExeName;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::policy::ProbeRecord;

use crate::corpus;
use crate::rng::SplitMix64;

/// Random bytes of a length in `0..=max`.
fn random_bytes(rng: &mut SplitMix64, max: usize) -> Vec<u8> {
    let len = rng.below(max + 1);
    rng.bytes(len)
}

/// Applies a few byte-level edits: flips, deletions, insertions, duplications, truncation.
fn mutate(rng: &mut SplitMix64, mut bytes: Vec<u8>) -> Vec<u8> {
    for _ in 0..rng.range(1, 4) {
        let len = bytes.len();
        match rng.below(6) {
            0 if len > 0 => {
                let at = rng.below(len);
                bytes[at] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let at = rng.below(len);
                bytes.remove(at);
            }
            2 => {
                let at = rng.below(len + 1);
                let value = rng.pick(&[0x00, b'"', b'\\', b'[', b'{', b'=', b'\n', 0xC3, 0xFF]);
                bytes.insert(at, value);
            }
            3 if len > 1 => {
                let start = rng.below(len);
                let end = rng.range(start, len.min(start + 32));
                let copy = bytes[start..end].to_vec();
                let at = rng.below(len + 1);
                bytes.splice(at..at, copy);
            }
            4 if len > 0 => {
                let keep = rng.below(len);
                bytes.truncate(keep);
            }
            _ => {
                let at = rng.below(len + 1);
                bytes.insert(at, rng.byte());
            }
        }
    }
    bytes
}

// --- engine_sequences --------------------------------------------------------------------

const SCRIPT_OPTIONS: &[&str] = &[
    "telex",
    "telex",
    "simple-telex",
    "vni",
    "viqr",
    "user",
    "nfc",
    "nfd",
    "preserve",
    "modern",
    "nofree",
    "nospell",
    "norestore",
    "nomacros",
    "always-macro",
    "exact-case",
    "disabled",
    "limit=8",
    "limit=12",
    "limit=128",
    "limit=0",
];

const SCRIPT_KEYS: &[&str] = &[
    "a",
    "a",
    "e",
    "o",
    "u",
    "i",
    "y",
    "d",
    "w",
    "s",
    "f",
    "r",
    "x",
    "j",
    "z",
    "n",
    "g",
    "h",
    "t",
    "c",
    "k",
    "q",
    "A",
    "D",
    "W",
    "S",
    "1",
    "2",
    "5",
    "6",
    "7",
    "8",
    "9",
    "0",
    "'",
    "`",
    "?",
    "~",
    ".",
    "-",
    "^",
    "(",
    "+",
    "*",
    "\\",
    ",",
    ";",
    "!",
    "@",
    "😀",
    "\u{302}",
    "\u{301}",
    "ê",
    "đ",
    " ",
    "{SPACE}",
    "{SHIFT+SPACE}",
    "{ENTER}",
    "{TAB}",
    "{BS}",
    "{BS}",
    "{NAV}",
    "{MOUSE}",
    "{FOCUS}",
    "{CTRL+c}",
    "{CTRL+SPACE}",
    "{ALTGR+a}",
    "{ALTGR+@}",
    "{SECURE}",
    "{/SECURE}",
    "{LBRACE}",
    "ko",
    "vn",
    "dc",
    "big",
    "after",
    "UBND",
    "tieengs",
    "nguwowif",
];

/// A `#!` key script or random bytes for the binary form.
pub fn engine_sequences(rng: &mut SplitMix64) -> Vec<u8> {
    if !rng.chance(30) {
        return random_bytes(rng, 520);
    }
    let mut script = String::from("#!");
    for _ in 0..rng.below(5) {
        script.push_str(rng.pick(SCRIPT_OPTIONS));
        script.push(' ');
    }
    script.push('\n');
    for _ in 0..rng.below(90) {
        script.push_str(rng.pick(SCRIPT_KEYS));
    }
    script.into_bytes()
}

// --- malformed_utf16 ---------------------------------------------------------------------

fn interesting_unit(rng: &mut SplitMix64) -> u16 {
    match rng.below(14) {
        0..=2 => 0x20 + u16::from(rng.byte() % 0x5F),
        3 => rng.pick(&[
            0x00E0, 0x00E1, 0x00E2, 0x0103, 0x0111, 0x01A1, 0x01B0, 0x1EBF, 0x1EC7,
        ]),
        4 => 0x1EA0 + u16::from(rng.byte() % 0x5A),
        5 => 0x0300 + u16::from(rng.byte() % 0x70),
        6 => 0xD800 + (u16::from(rng.byte()) << 2),
        7 => 0xDC00 + (u16::from(rng.byte()) << 2),
        8 => rng.pick(&[0xFEFF, 0xFFFE, 0xFFFF, 0x0000, 0x000D, 0x000A, 0x0009]),
        9 => 0x0080 + u16::from(rng.byte() % 0x80),
        10 => u16::from(rng.pick(b"^(+'`?~.\\dD&#x;0123456789")),
        11 => rng.pick(&[
            0x20AC, 0x201A, 0x0192, 0x02C6, 0x2030, 0x0160, 0x0152, 0x017D,
        ]),
        _ => u16::from_le_bytes([rng.byte(), rng.byte()]),
    }
}

/// Little-endian UTF-16 rich in surrogates, astral pairs, and Vietnamese letters.
pub fn malformed_utf16(rng: &mut SplitMix64) -> Vec<u8> {
    let mut units = Vec::new();
    for _ in 0..rng.below(120) {
        if rng.chance(8) {
            // A well-formed astral pair.
            units.push(0xD800 + u16::from(rng.byte() % 4));
            units.push(0xDC00 + u16::from(rng.byte()));
        } else {
            units.push(interesting_unit(rng));
        }
    }
    let mut bytes: Vec<u8> = units.iter().flat_map(|unit| unit.to_le_bytes()).collect();
    if rng.chance(10) {
        bytes.push(rng.byte());
    }
    bytes
}

// --- config_documents --------------------------------------------------------------------

const WORDS: &[&str] = &[
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
    "lower",
    "title",
    "unicode",
    "tcvn3",
    "vni-windows",
    "delimiter",
    "whitespace",
    "adapt",
    "exact",
    "tsf",
    "synthetic-key",
    "clipboard",
    "bypass",
    "restricted",
    "Ctrl+Shift",
    "Ctrl+Shift+Alt+F12",
    "Alt+Z",
    "Shift",
    "notepad.exe",
    "Chrome.EXE",
    "C:\\x.exe",
    "tone:acute",
    "circumflex:a",
    "horn-or-breve",
    "bogus",
    "s",
    "ss",
    "ko",
    "không",
    "my-map",
    "m",
    "",
    "x\u{0}y",
    "\u{feff}",
    "😀",
    "completed",
    "rolled-back",
    "in-progress",
    "line 3",
];

const TOP_KEYS: &[&str] = &[
    "clipboard_enabled",
    "default_reset_on_mouse",
    "secure_field_mode",
    "default_max_delete_before",
    "x_unknown",
];

const TABLES: &[(&str, &[&str])] = &[
    (
        "input",
        &[
            "enabled",
            "method",
            "user_keymap",
            "normalization",
            "modern_tone",
            "free_marking",
            "spell_check",
            "auto_restore_invalid",
            "context_limit",
            "charset",
            "x_new",
        ],
    ),
    (
        "macros",
        &["enabled", "when_disabled", "auto_case", "x_new"],
    ),
    ("hotkeys", &["toggle_mode", "hard_reset", "x_new"]),
    ("system", &["startup", "show_panel_on_start", "diagnostics"]),
    ("conversion", &["source", "target", "case", "remove_tones"]),
];

const ARRAYS: &[(&str, &[&str])] = &[
    ("keymaps", &["id", "version", "name", "bindings", "x_extra"]),
    (
        "macro",
        &[
            "id",
            "trigger",
            "replacement",
            "enabled",
            "boundary",
            "case",
            "x_extra",
        ],
    ),
    (
        "rule",
        &[
            "exe",
            "window_class",
            "transport",
            "reset_on_mouse",
            "macros",
            "vietnamese",
            "max_delete_before",
            "autocomplete_guard",
            "x_extra",
        ],
    ),
    ("named_set", &["name", "members", "action", "x_extra"]),
    (
        "probe",
        &["exe", "probe_version", "tsf_range_edit", "x_extra"],
    ),
];

fn toml_string(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Up to `count` distinct keys of `keys` (TOML rejects a key defined twice in one table).
fn distinct<'a>(rng: &mut SplitMix64, keys: &[&'a str], count: usize) -> Vec<&'a str> {
    let mut chosen: Vec<&'a str> = Vec::new();
    for _ in 0..count {
        let key = rng.pick(keys);
        if !chosen.contains(&key) {
            chosen.push(key);
        }
    }
    chosen
}

fn toml_value(rng: &mut SplitMix64, depth: usize) -> String {
    match rng.below(if depth > 2 { 5 } else { 8 }) {
        0 => rng.pick(&["true", "false"]).to_owned(),
        1 => rng
            .pick(&[
                "0",
                "1",
                "-1",
                "8",
                "32",
                "128",
                "129",
                "64",
                "9223372036854775807",
                "-9223372036854775808",
                "4294967296",
            ])
            .to_owned(),
        2 => rng
            .pick(&["1.5", "nan", "-inf", "-0.0", "1e300"])
            .to_owned(),
        3 | 4 => toml_string(rng.pick(WORDS)),
        5 => {
            let items: Vec<String> = (0..rng.below(4))
                .map(|_| toml_value(rng, depth + 1))
                .collect();
            format!("[{}]", items.join(", "))
        }
        6 => {
            let count = rng.below(3);
            let keys = distinct(
                rng,
                &["key", "actions", "reset_on_mouse", "transport", "x"],
                count,
            );
            let items: Vec<String> = keys
                .into_iter()
                .map(|key| format!("{key} = {}", toml_value(rng, depth + 1)))
                .collect();
            format!("{{ {} }}", items.join(", "))
        }
        _ => {
            // A key-map binding list, the most structured value of the schema.
            let bindings: Vec<String> = (0..rng.below(4))
                .map(|_| {
                    format!(
                        "{{ key = {}, actions = [{}] }}",
                        toml_string(rng.pick(&["s", "f", "6", ";", "ss", "", "A", "a"])),
                        toml_string(rng.pick(WORDS))
                    )
                })
                .collect();
            format!("[{}]", bindings.join(", "))
        }
    }
}

fn toml_document(rng: &mut SplitMix64) -> String {
    let mut out = String::new();
    if rng.chance(85) {
        out.push_str("version = 1\n");
    } else {
        out.push_str(rng.pick(&["version = 2\n", "version = \"1\"\n", "version = 0\n", ""]));
    }
    if rng.chance(80) {
        let revision = rng.pick(&[
            "0",
            "1",
            "42",
            "9223372036854775806",
            "9223372036854775807",
            "-3",
        ]);
        out.push_str(&format!("revision = {revision}\n"));
    }
    let count = rng.below(3);
    for key in distinct(rng, TOP_KEYS, count) {
        out.push_str(&format!("{key} = {}\n", toml_value(rng, 0)));
    }
    for (name, keys) in TABLES {
        if rng.chance(40) {
            out.push_str(&format!("[{name}]\n"));
            let count = rng.below(6);
            for key in distinct(rng, keys, count) {
                out.push_str(&format!("{key} = {}\n", toml_value(rng, 0)));
            }
        }
    }
    for _ in 0..rng.below(5) {
        let (name, keys) = rng.pick(ARRAYS);
        out.push_str(&format!("[[{name}]]\n"));
        let count = rng.below(6);
        for key in distinct(rng, keys, count) {
            out.push_str(&format!("{key} = {}\n", toml_value(rng, 0)));
        }
    }
    out
}

fn json_value(rng: &mut SplitMix64, depth: usize) -> serde_json::Value {
    use serde_json::Value;
    match rng.below(if depth > 2 { 5 } else { 7 }) {
        0 => Value::Bool(rng.chance(50)),
        1 => Value::from(rng.pick(&[0_u64, 1, 2, u64::MAX, u64::MAX - 1, 1 << 53])),
        2 => Value::from(rng.pick(&[-1_i64, i64::MIN])),
        3 => serde_json::Number::from_f64(rng.pick(&[
            0.1,
            -0.0,
            1e300,
            5e-324,
            2.5,
            1.7976931348623157e308,
        ]))
        .map_or(Value::Null, Value::Number),
        4 => Value::String(rng.pick(WORDS).to_owned()),
        5 => Value::Array(
            (0..rng.below(4))
                .map(|_| json_value(rng, depth + 1))
                .collect(),
        ),
        _ => {
            let mut map = serde_json::Map::new();
            for _ in 0..rng.below(4) {
                let key = rng.pick(&["kind", "location", "sha256", "size", "source", "field", "x"]);
                map.insert(key.to_owned(), json_value(rng, depth + 1));
            }
            Value::Object(map)
        }
    }
}

fn json_document(rng: &mut SplitMix64) -> String {
    use serde_json::Value;
    let mut map = serde_json::Map::new();
    let keys = [
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
        "x_top",
    ];
    if rng.chance(85) {
        map.insert("version".to_owned(), Value::from(1));
    }
    for _ in 0..rng.below(8) {
        let key = rng.pick(&keys);
        let value = match key {
            "sources" | "quarantined" => {
                Value::Array((0..rng.below(3)).map(|_| json_value(rng, 2)).collect())
            }
            "status" => Value::String(rng.pick(WORDS).to_owned()),
            "result" => {
                let mut result = serde_json::Map::new();
                for field in ["config_revision", "macros_revision", "app_rules_revision"] {
                    if rng.chance(80) {
                        result.insert(field.to_owned(), json_value(rng, 3));
                    }
                }
                result.insert("checksums".to_owned(), json_value(rng, 1));
                Value::Object(result)
            }
            _ => json_value(rng, 0),
        };
        map.insert(key.to_owned(), value);
    }
    serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_default()
}

/// TOML and JSON documents built from schema keys, mutated corpus documents, or noise.
pub fn config_documents(rng: &mut SplitMix64) -> Vec<u8> {
    match rng.below(10) {
        0..=4 => toml_document(rng).into_bytes(),
        5..=6 => json_document(rng).into_bytes(),
        7 | 8 => {
            let seeds = corpus::config_documents();
            let seed = seeds[rng.below(seeds.len())].clone();
            mutate(rng, seed)
        }
        _ => {
            let document = toml_document(rng).into_bytes();
            mutate(rng, document)
        }
    }
}

// --- config_concurrency ------------------------------------------------------------------

/// Random schedule bytes: the target decodes every byte string, and about one operation in
/// four is a race.
pub fn config_concurrency(rng: &mut SplitMix64) -> Vec<u8> {
    random_bytes(rng, 64)
}

// --- macro_files -------------------------------------------------------------------------

const TRIGGER_PARTS: &[&str] = &[
    "ko",
    "vn",
    "dc",
    "KO",
    "Ko",
    "tphcm",
    "a",
    "đ",
    "việt",
    "vie\u{302}\u{323}t",
    "😀",
    "k o",
    "#x",
    ";x",
    "//x",
    "\u{feff}x",
    "x\u{1}",
    "",
    "  ",
    "x:y",
    "İ",
    "ß",
    "\u{0}",
];

const REPLACEMENT_PARTS: &[&str] = &[
    "không",
    "Việt Nam",
    "được",
    "thành phố Hồ Chí Minh",
    "\t",
    "\u{a0}",
    " padded ",
    "a:b",
    "DO NOT DELETE",
    "x\u{0}y",
    "😀",
    "e\u{301}",
    "",
    "  ",
    "dòng 1",
    "http://x.vn",
];

/// Legacy macro text in UTF-8 (with or without BOM) or UTF-16 (LE or BE with BOM).
pub fn macro_files(rng: &mut SplitMix64) -> Vec<u8> {
    let mut text = String::new();
    // Replacements at the length limit are costly to case-adapt; a few inputs carry one.
    let long_lines = rng.chance(3);
    for _ in 0..rng.below(24) {
        match rng.below(12) {
            0 => {
                text.push_str(rng.pick(&["; comment", "# comment", "// comment", "DO NOT DELETE"]))
            }
            1 => {}
            2 => text.push_str(&"t".repeat(rng.pick(&[63, 64, 65]))),
            3 if long_lines => {
                text.push_str("long:");
                text.push_str(&"ệ".repeat(rng.pick(&[4095, 4096, 4097])));
            }
            4 => text.push_str("no separator"),
            _ => {
                for _ in 0..rng.range(1, 2) {
                    text.push_str(rng.pick(TRIGGER_PARTS));
                }
                text.push_str(rng.pick(&[":", ":", " : ", "::"]));
                for _ in 0..rng.range(1, 2) {
                    text.push_str(rng.pick(REPLACEMENT_PARTS));
                }
            }
        }
        text.push_str(rng.pick(&["\r\n", "\n", "\r"]));
    }
    let mut bytes = match rng.below(10) {
        0..=4 => text.into_bytes(),
        5 => [b"\xEF\xBB\xBF".as_slice(), text.as_bytes()].concat(),
        6 | 7 => {
            let mut bytes = vec![0xFF, 0xFE];
            bytes.extend(text.encode_utf16().flat_map(u16::to_le_bytes));
            bytes
        }
        8 => {
            let mut bytes = vec![0xFE, 0xFF];
            bytes.extend(text.encode_utf16().flat_map(u16::to_be_bytes));
            bytes
        }
        _ => mutate(rng, text.into_bytes()),
    };
    if rng.chance(10) {
        bytes = mutate(rng, bytes);
    }
    bytes
}

// --- ipc_frames --------------------------------------------------------------------------

fn exe(rng: &mut SplitMix64) -> ExeName {
    let name = rng.pick(&["notepad.exe", "legacy.exe", "a", "x.y.z"]);
    ExeName::new(name).expect("constant names are valid")
}

fn message(rng: &mut SplitMix64) -> Message {
    match rng.below(13) {
        0 => Message::Hello {
            client_kind: rng.pick(&[ClientKind::Tip, ClientKind::Tool]),
            pid: u32::from(rng.byte()),
            arch: rng.pick(&[Arch::X86, Arch::X64, Arch::Arm64, Arch::Other]),
            protocol: rng.pick(&[0, 1, 2]),
        },
        1 => Message::Welcome {
            client_id: rng.pick(&[0, 1, u64::MAX]),
            server_revision: rng.next_u64(),
        },
        2 => {
            let mut snapshot = RuntimeSnapshot::defaults_v1();
            snapshot.config_revision = rng.next_u64();
            snapshot.vietnamese_mode = rng.chance(50);
            snapshot.diagnostics_enabled = rng.chance(50);
            snapshot.options.context_limit_codepoints = rng.pick(&[8, 32, 128, 0, 999]);
            Message::RuntimeSnapshot(snapshot)
        }
        3 => Message::MacroSnapshot(MacroSnapshot {
            macro_revision: rng.next_u64(),
            entries: (0..rng.below(4))
                .map(|index| MacroEntry {
                    id: MacroId(u64::try_from(index).unwrap_or(0)),
                    trigger: rng.pick(TRIGGER_PARTS).to_owned(),
                    replacement: rng.pick(REPLACEMENT_PARTS).to_owned(),
                    enabled: rng.chance(80),
                    boundary: rng.pick(&[BoundaryPolicy::Delimiter, BoundaryPolicy::Whitespace]),
                    case_policy: rng.pick(&[CasePolicy::Adapt, CasePolicy::Exact]),
                })
                .collect(),
        }),
        4 => Message::Command {
            command: match rng.below(4) {
                0 => Command::ToggleMode,
                1 => Command::SetMode(rng.chance(50)),
                2 => Command::HardReset,
                _ => Command::ReportProbe(ProbeRecord {
                    exe: exe(rng),
                    probe_version: rng.pick(&[0, 1, u32::MAX]),
                    tsf_range_edit: rng.chance(50),
                }),
            },
        },
        5 => {
            let code = rng.pick(&[0, 1, 9, u32::MAX]);
            Message::CommandResult {
                ok: if rng.chance(90) { code == 0 } else { code != 0 },
                code: raumakey_ipc::error::ErrorCode(code),
            }
        }
        6 => Message::ModeChanged {
            vietnamese_mode: rng.chance(50),
            revision: rng.next_u64(),
        },
        7 => Message::ResetAll,
        8 => Message::Diagnostic(DiagEvent {
            timestamp_ms: rng.next_u64(),
            component: rng.pick(&[Component::Engine, Component::Ipc, Component::Installer]),
            code: DiagCode(u32::from(rng.byte())),
            revision: rng.chance(50).then(|| rng.next_u64()),
            host_exe: rng.chance(50).then(|| exe(rng)),
            vector_id: rng
                .chance(30)
                .then(|| VectorId::new("A1.x-y").expect("constant id is valid")),
        }),
        9 => Message::Ping,
        10 => Message::Pong,
        11 => Message::Error {
            code: raumakey_ipc::error::ErrorCode(u32::from(rng.byte())),
        },
        _ => Message::Goodbye,
    }
}

/// Edits a JSON body: drops or adds a key, retypes a value, or swaps the type tag.
fn mutate_body(rng: &mut SplitMix64, body: &[u8]) -> Vec<u8> {
    let Ok(mut value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return body.to_vec();
    };
    if let Some(map) = value.as_object_mut() {
        let keys: Vec<String> = map.keys().cloned().collect();
        match rng.below(5) {
            0 if !keys.is_empty() => {
                map.remove(&keys[rng.below(keys.len())]);
            }
            1 => {
                map.insert("x_unknown".to_owned(), serde_json::json!([1, {"a": null}]));
            }
            2 if !keys.is_empty() => {
                let key = keys[rng.below(keys.len())].clone();
                let replacement = match rng.below(5) {
                    0 => serde_json::json!(null),
                    1 => serde_json::json!(-1),
                    2 => serde_json::json!("x"),
                    3 => serde_json::json!(1.5),
                    _ => serde_json::json!(u64::MAX),
                };
                map.insert(key, replacement);
            }
            3 => {
                let tag = rng.pick(&["ping", "hello", "runtime_snapshot", "bogus", ""]);
                map.insert("type".to_owned(), serde_json::json!(tag));
            }
            _ => {}
        }
    }
    serde_json::to_vec(&value).unwrap_or_default()
}

/// A frame around `payload` with a valid checksum.
fn frame_with_payload(kind: u16, client_id: u64, request_id: u64, payload: &[u8]) -> Vec<u8> {
    let declared = u32::try_from(payload.len()).unwrap_or(u32::MAX);
    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(b"RMK1");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&40_u16.to_le_bytes());
    bytes.extend_from_slice(&declared.to_le_bytes());
    bytes.extend_from_slice(&client_id.to_le_bytes());
    bytes.extend_from_slice(&request_id.to_le_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&bytes);
    crc.update(payload);
    bytes.extend_from_slice(&crc.finalize().to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

/// A stream of frames: encoded real messages, bodies edited with a fixed-up checksum, kinds
/// swapped, and byte-level damage; or random bytes in the assemble form.
pub fn ipc_frames(rng: &mut SplitMix64) -> Vec<u8> {
    if rng.chance(15) {
        let mut bytes = vec![0x80 | (rng.byte() & 0x3F)];
        bytes.extend(random_bytes(rng, 400));
        return bytes;
    }
    let mut stream = Vec::new();
    for _ in 0..rng.range(1, 4) {
        let message = message(rng);
        let kind = message.kind().as_u16();
        let client_id = rng.pick(&[0, 1, 7, u64::MAX]);
        let request_id = u64::from(rng.byte());
        let frame = Frame::new(client_id, request_id, message.clone());
        match rng.below(5) {
            0 | 1 => match encode_frame(&frame) {
                Ok(bytes) => stream.extend(bytes),
                Err(_) => {
                    let body = serde_json::to_vec(&message).unwrap_or_default();
                    stream.extend(frame_with_payload(kind, client_id, request_id, &body));
                }
            },
            2 => {
                let body = serde_json::to_vec(&message).unwrap_or_default();
                let body = mutate_body(rng, &body);
                stream.extend(frame_with_payload(kind, client_id, request_id, &body));
            }
            3 => {
                let body = serde_json::to_vec(&message).unwrap_or_default();
                let other = rng.pick(&[1, 2, 3, 4, 5, 6, 9, 10, 13]);
                stream.extend(frame_with_payload(other, client_id, request_id, &body));
            }
            _ => {
                let body = serde_json::to_vec(&message).unwrap_or_default();
                let bytes = frame_with_payload(kind, client_id, request_id, &body);
                stream.extend(mutate(rng, bytes));
            }
        }
    }
    stream
}

// --- legacy_sequence_wrap and transaction_application ------------------------------------

/// Random scenario bytes (the target decodes every byte string).
#[cfg(feature = "evkey-compat")]
pub fn legacy_sequence_wrap(rng: &mut SplitMix64) -> Vec<u8> {
    random_bytes(rng, 360)
}

/// Random bytes, sometimes with the control bytes pushed to their extremes.
pub fn transaction_application(rng: &mut SplitMix64) -> Vec<u8> {
    let mut bytes = random_bytes(rng, 200);
    if rng.chance(20) {
        for byte in bytes.iter_mut().take(8) {
            *byte = rng.pick(&[0, 1, 0x7F, 0x80, 0xFE, 0xFF]);
        }
    }
    bytes
}
