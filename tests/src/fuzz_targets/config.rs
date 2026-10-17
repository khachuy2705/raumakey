//! `config_documents`: text decoded as each `raumakey-config` document (plan §5, matrix #45).

use std::sync::Arc;

use raumakey_config::{
    AppRulesDocument, ConfigDocument, ConfigError, FieldIssue, MacroDocument, MigrationRecord,
    StoreDocument,
};
use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_macro::MacroTable;
use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::input::InputEvent;
use raumakey_model::macros::NoMacros;
use raumakey_model::policy::{SecureFieldMode, Transport};
use raumakey_policy::CompatibilityPolicy;

/// Largest input in bytes.
const MAX_BYTES: usize = 64 * 1024;
/// Hosts resolved against a decoded `raumakey-app-rules.toml`.
const MAX_POLICY_HOSTS: usize = 6;
/// Largest distance, in units in the last place, between two readings of one JSON float:
/// `serde_json` without its `float_roundtrip` feature may read a printed float one ULP off.
const JSON_FLOAT_ULPS: u64 = 4;

/// A store document plus the checks its consumers rely on.
trait Fuzzed: StoreDocument + PartialEq {
    /// The document is JSON (floats may move by a few ULPs per round trip).
    const JSON: bool;

    /// Checks that hold for every decoded document of this type.
    fn check_consumers(&self);
}

impl Fuzzed for ConfigDocument {
    const JSON: bool = false;

    /// A decoded configuration is always usable as is (matrix #45): its options validate, its
    /// key maps validate, and the engine runs with them.
    fn check_consumers(&self) {
        let options = self.engine_options();
        assert!(
            options.validate().is_ok(),
            "a decoded configuration yields invalid engine options"
        );
        for keymap in &self.keymaps {
            assert!(keymap.validate().is_ok(), "a decoded key map is invalid");
        }
        let engine = VietnameseEngine::new(self.keymaps.clone(), Arc::new(NoMacros));
        let event = InputEvent::character('a', host("fuzz-config.exe", false));
        assert!(
            engine
                .process(&event, &options, &EngineState::new())
                .is_ok(),
            "the engine rejects the options of a decoded configuration"
        );
    }
}

impl Fuzzed for MacroDocument {
    const JSON: bool = false;

    /// Decoding is structural; building a table validates and fails with a typed error.
    fn check_consumers(&self) {
        let _ = MacroTable::new(self.entries.clone());
    }
}

impl Fuzzed for AppRulesDocument {
    const JSON: bool = false;

    /// Resolution never selects the clipboard unless it is enabled, and secure fields never get
    /// macros or the clipboard, nor Vietnamese in `Bypass` mode (plan §6, design §14).
    fn check_consumers(&self) {
        let config = Arc::new(self.compatibility.clone());
        let policy = CompatibilityPolicy::new(Arc::clone(&config));
        let exes = config
            .rules
            .iter()
            .map(|rule| (rule.exe.as_str(), rule.window_class.as_deref()))
            .chain(
                config
                    .named_sets
                    .iter()
                    .flat_map(|set| set.members.iter().map(|member| (member.as_str(), None))),
            )
            .chain(config.probes.iter().map(|probe| (probe.exe.as_str(), None)))
            .chain([("generic.exe", None)])
            .take(MAX_POLICY_HOSTS);
        for (exe, window_class) in exes {
            for secure in [false, true] {
                let mut host = host(exe, secure);
                host.window_class = window_class.map(str::to_owned);
                host.supports_tsf_range_edit = !secure;
                let resolved = policy.resolve(&host);
                if !config.clipboard_enabled || secure {
                    assert!(
                        resolved.transport != Transport::Clipboard,
                        "the clipboard was selected while disabled or in a secure field"
                    );
                }
                if secure {
                    assert!(!resolved.macro_allowed, "macros allowed in a secure field");
                    if config.secure_field_mode == SecureFieldMode::Bypass {
                        assert!(
                            !resolved.vietnamese_allowed,
                            "Vietnamese allowed in a secure field in bypass mode"
                        );
                    }
                }
            }
        }
    }
}

impl Fuzzed for MigrationRecord {
    const JSON: bool = true;

    fn check_consumers(&self) {}
}

fn host(exe: &str, secure: bool) -> HostContext {
    let executable = ExeName::new(exe)
        .or_else(|_| ExeName::new("fallback.exe"))
        .expect("constant name is valid");
    let mut host = HostContext::new(SessionId::from_u128(0xC0F1), executable);
    host.is_secure_field = secure;
    host
}

/// Issue paths name schema fields and indices only, never content (`FieldIssue` docs).
fn check_issue_paths(issues: &[FieldIssue]) {
    for issue in issues {
        assert!(
            !issue.path.is_empty()
                && issue.path.chars().all(|c| {
                    c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || matches!(c, '_' | '.' | '[' | ']')
                }),
            "an issue path holds something other than schema names and indices"
        );
    }
}

/// True when the TOML text holds a NaN float, which never compares equal to itself.
fn toml_has_nan(text: &str) -> bool {
    fn walk(value: &toml::Value) -> bool {
        match value {
            toml::Value::Float(float) => float.is_nan(),
            toml::Value::Array(items) => items.iter().any(walk),
            toml::Value::Table(table) => table.values().any(walk),
            _ => false,
        }
    }
    text.parse::<toml::Table>()
        .is_ok_and(|table| table.values().any(walk))
}

/// True when the JSON text holds a floating-point number.
fn json_has_float(text: &str) -> bool {
    fn walk(value: &serde_json::Value) -> bool {
        match value {
            serde_json::Value::Number(number) => number.is_f64(),
            serde_json::Value::Array(items) => items.iter().any(walk),
            serde_json::Value::Object(map) => map.values().any(walk),
            _ => false,
        }
    }
    serde_json::from_str::<serde_json::Value>(text).is_ok_and(|value| walk(&value))
}

/// Monotonic integer view of a float for ULP distances.
fn ordered_bits(value: f64) -> i64 {
    let bits = i64::from_ne_bytes(value.to_bits().to_ne_bytes());
    if bits < 0 {
        i64::MIN.wrapping_sub(bits)
    } else {
        bits
    }
}

/// JSON values equal up to a few ULPs per float.
fn json_close(left: &serde_json::Value, right: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (left, right) {
        (Value::Number(a), Value::Number(b)) if a.is_f64() || b.is_f64() => {
            match (a.as_f64(), b.as_f64()) {
                (Some(x), Some(y)) => ordered_bits(x).abs_diff(ordered_bits(y)) <= JSON_FLOAT_ULPS,
                _ => false,
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| json_close(x, y))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, x)| b.get(key).is_some_and(|y| json_close(x, y)))
        }
        _ => left == right,
    }
}

/// Two encodings of one document agree: exactly for TOML, up to float ULPs for JSON.
fn same_text<D: Fuzzed>(left: &str, right: &str) -> bool {
    if left == right {
        return true;
    }
    if !D::JSON {
        return false;
    }
    match (
        serde_json::from_str::<serde_json::Value>(left),
        serde_json::from_str::<serde_json::Value>(right),
    ) {
        (Ok(a), Ok(b)) => json_close(&a, &b),
        _ => false,
    }
}

/// Decode never panics; decoded documents re-encode to a fixed point (plan §5: the lenient
/// decoder's result survives encode → decode unchanged and encodes to the same text again);
/// the header shortcut agrees with a full decode; issue paths carry no content.
fn check<D: Fuzzed>(text: &str) {
    if let Some(revision) = D::revision_from_head(text.as_bytes()) {
        match D::decode(text) {
            Ok(loaded) => assert!(
                loaded.document.revision() == revision,
                "{}: the header revision disagrees with a full decode",
                D::FILE_NAME
            ),
            Err(error) => assert!(
                error == ConfigError::Corrupt,
                "{}: a header-recognised file failed with {error}",
                D::FILE_NAME
            ),
        }
    }
    let first = match D::decode(text) {
        Ok(loaded) => loaded,
        Err(error) => {
            assert!(
                matches!(
                    error,
                    ConfigError::Corrupt | ConfigError::UnsupportedVersion
                ),
                "{}: decode failed with {error}",
                D::FILE_NAME
            );
            return;
        }
    };
    check_issue_paths(&first.issues);
    let encoded = match first.document.encode() {
        Ok(encoded) => encoded,
        Err(error) => panic!(
            "{}: a decoded document does not encode: {error}",
            D::FILE_NAME
        ),
    };
    let second = match D::decode(&encoded) {
        Ok(loaded) => loaded,
        Err(error) => panic!(
            "{}: an encoded document does not decode: {error}",
            D::FILE_NAME
        ),
    };
    check_issue_paths(&second.issues);
    assert!(
        second.issues.len() <= first.issues.len(),
        "{}: decoding the encoded document reports new issues",
        D::FILE_NAME
    );
    let again = match second.document.encode() {
        Ok(again) => again,
        Err(error) => panic!(
            "{}: a re-decoded document does not encode: {error}",
            D::FILE_NAME
        ),
    };
    assert!(
        same_text::<D>(&again, &encoded),
        "{}: encoding is not a fixed point",
        D::FILE_NAME
    );
    let floats_may_differ = if D::JSON {
        json_has_float(&encoded)
    } else {
        toml_has_nan(&encoded)
    };
    assert!(
        second.document == first.document || floats_may_differ,
        "{}: encode → decode changed the document",
        D::FILE_NAME
    );
    if let Some(revision) = D::revision_from_head(encoded.as_bytes()) {
        assert!(
            revision == first.document.revision(),
            "{}: the header of an encoded document names another revision",
            D::FILE_NAME
        );
    }
    first.document.check_consumers();
}

pub(crate) fn run(data: &[u8]) {
    let text = String::from_utf8_lossy(&data[..data.len().min(MAX_BYTES)]);
    check::<ConfigDocument>(&text);
    check::<MacroDocument>(&text);
    check::<AppRulesDocument>(&text);
    check::<MigrationRecord>(&text);
}
