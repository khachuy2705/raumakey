//! `malformed_utf16`: arbitrary code units against `raumakey_model::utf16`, `InputEvent`
//! validation, the engine's handling of event text, and every converter charset.

use std::sync::{Arc, OnceLock};

use raumakey_converter::{Charset, ConvertError, decode, decode_bytes, encode_with_report};
use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_model::error::EngineError;
use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::input::{InputEvent, InputKind, MAX_EVENT_TEXT_UNITS};
use raumakey_model::macros::NoMacros;
use raumakey_model::options::EngineOptions;
use raumakey_model::transaction::TextSimulator;
use raumakey_model::utf16;
use unicode_normalization::{UnicodeNormalization, is_nfc};

use super::oracle::{decode_items, well_formed};

/// Largest input in bytes (256 code units).
const MAX_BYTES: usize = 512;
/// Event text is taken from the first units, so that both short and overlong texts occur.
const EVENT_UNITS: usize = MAX_EVENT_TEXT_UNITS + 4;

const KINDS: [InputKind; 9] = [
    InputKind::Character,
    InputKind::Backspace,
    InputKind::Separator,
    InputKind::Navigation,
    InputKind::MouseReset,
    InputKind::FocusReset,
    InputKind::ModeToggle,
    InputKind::ConfigChanged,
    InputKind::Shutdown,
];

/// The model helpers agree with `std` on `units`.
fn check_model_helpers(units: &[u16]) {
    let items = decode_items(units);
    let first_bad = items.iter().find(|item| item.scalar.is_none());
    let std_text = String::from_utf16(units).ok();
    assert!(
        utf16::is_well_formed(units) == std_text.is_some(),
        "is_well_formed disagrees with std"
    );
    match (utf16::validate(units), first_bad) {
        (Ok(()), None) => {}
        (Err(error), Some(item)) => assert!(
            error.index == item.start,
            "validate reports unit {} instead of {}",
            error.index,
            item.start
        ),
        (Ok(()), Some(_)) => panic!("validate accepted ill-formed UTF-16"),
        (Err(_), None) => panic!("validate rejected well-formed UTF-16"),
    }
    match (utf16::decode(units), &std_text) {
        (Ok(text), Some(expected)) => assert!(text == *expected, "decode differs from std"),
        (Err(error), None) => assert!(
            utf16::validate(units) == Err(error),
            "decode and validate report different errors"
        ),
        _ => panic!("decode disagrees with std about well-formedness"),
    }
    assert!(
        utf16::scalar_count(units) == items.len(),
        "scalar_count differs from std's code point count"
    );
    for index in 0..=units.len() + 1 {
        let expected = index == units.len() || items.iter().any(|item| item.start == index);
        assert!(
            utf16::is_scalar_boundary(units, index) == expected,
            "is_scalar_boundary is wrong at unit {index}"
        );
    }
    assert!(
        utf16::last_scalar_len(units) == items.last().map_or(0, |item| item.len),
        "last_scalar_len differs from std"
    );
    let single = match items.as_slice() {
        [only] => only.scalar,
        _ => None,
    };
    assert!(
        utf16::single_scalar(units) == single,
        "single_scalar differs from std"
    );
    if let Some(text) = std_text {
        assert!(
            utf16::encode(&text) == units,
            "encode does not reverse decode"
        );
    }
}

fn host() -> HostContext {
    let exe = ExeName::new("fuzz-utf16.exe").expect("constant name is valid");
    HostContext::new(SessionId::from_u128(0x7516), exe)
}

/// The engine and a state that owns the word `tiêng` (Telex `tieeng`), built once.
fn engine_and_word() -> &'static (VietnameseEngine, EngineState, Vec<u16>) {
    static SETUP: OnceLock<(VietnameseEngine, EngineState, Vec<u16>)> = OnceLock::new();
    SETUP.get_or_init(|| {
        let engine = VietnameseEngine::new(Vec::new(), Arc::new(NoMacros));
        let options = EngineOptions::defaults_v1();
        let mut state = EngineState::new();
        let mut document = TextSimulator::new();
        for key in "tieeng".chars() {
            let event = InputEvent::character(key, host());
            let result = engine
                .process(&event, &options, &state)
                .expect("typing a Telex word succeeds");
            document
                .apply(&event, &result.transaction)
                .expect("the engine's edit applies");
            state = result.next;
        }
        (engine, state, document.units().to_vec())
    })
}

/// Every rule of plan §2.4 that an event of `kind` with `text` breaks.
fn violated_rules(kind: InputKind, text: &[u16]) -> Vec<EngineError> {
    let mut violated = Vec::new();
    if !matches!(kind, InputKind::Character | InputKind::Separator) && !text.is_empty() {
        violated.push(EngineError::InternalInvariant);
    }
    if text.len() > MAX_EVENT_TEXT_UNITS {
        violated.push(EngineError::CapacityExceeded);
    }
    if !well_formed(text) {
        violated.push(EngineError::InvalidUtf16);
    }
    violated
}

/// `InputEvent::validate` and the engine accept exactly the events that break no rule, and
/// otherwise report a broken rule as a typed error.
fn check_events(units: &[u16]) {
    let text = &units[..units.len().min(EVENT_UNITS)];
    let (engine, word_state, word_text) = engine_and_word();
    let options = EngineOptions::defaults_v1();
    for kind in KINDS {
        let mut event = InputEvent::simple(kind, host());
        event.text_utf16 = text.to_vec();
        let violated = violated_rules(kind, text);
        match event.validate() {
            Ok(()) => assert!(violated.is_empty(), "validate accepted a malformed event"),
            Err(error) => assert!(
                violated.contains(&error),
                "validate reported {error:?}, which the event does not break"
            ),
        }
        for (prior, document) in [
            (&EngineState::new(), &[][..]),
            (word_state, word_text.as_slice()),
        ] {
            match engine.process(&event, &options, prior) {
                Ok(result) => {
                    assert!(
                        violated.is_empty(),
                        "the engine processed a malformed event"
                    );
                    let mut simulator = TextSimulator::new();
                    let loader = InputEvent::text(document.to_vec(), host());
                    let loaded = simulator.apply(
                        &loader,
                        &raumakey_model::transaction::EditTransaction::pass_through(),
                    );
                    assert!(loaded.is_ok(), "a pass-through edit was rejected");
                    assert!(
                        simulator.apply(&event, &result.transaction).is_ok(),
                        "the engine's transaction does not apply"
                    );
                    assert!(
                        well_formed(simulator.units()),
                        "the document became ill-formed UTF-16"
                    );
                    assert!(
                        simulator.units().ends_with(result.next.owned_utf16()),
                        "the owned text is not the document suffix"
                    );
                }
                Err(error) => assert!(
                    violated.contains(&error),
                    "the engine reported {error:?} for an event that does not break it"
                ),
            }
        }
    }
}

/// Decoding and encoding arbitrary text never fails or panics; decoding yields NFC; a lossless
/// encoding decodes back to the NFC input; byte decoding fails only for malformed Unicode.
fn check_converter(units: &[u16], bytes: &[u8]) {
    let text = String::from_utf16_lossy(units);
    let nfc: String = text.nfc().collect();
    for charset in Charset::ALL {
        match decode(&text, charset) {
            Ok(decoded) => assert!(is_nfc(&decoded), "{charset}: decode is not NFC"),
            Err(error) => panic!("{charset}: decode failed on small text: {error}"),
        }
        let report = match encode_with_report(&text, charset) {
            Ok(report) => report,
            Err(error) => panic!("{charset}: encode failed on small text: {error}"),
        };
        if !report.lossy {
            let back = decode(&report.text, charset);
            assert!(
                back.as_ref() == Ok(&nfc),
                "{charset}: a lossless encoding does not decode back"
            );
        }
        match decode_bytes(bytes, charset) {
            Ok(decoded) => assert!(is_nfc(&decoded), "{charset}: decode_bytes is not NFC"),
            Err(error) => assert!(
                !charset.is_legacy_byte() && error == ConvertError::MalformedInput,
                "{charset}: decode_bytes failed with {error}"
            ),
        }
    }
}

pub(crate) fn run(data: &[u8]) {
    let bytes = &data[..data.len().min(MAX_BYTES)];
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    check_model_helpers(&units);
    if let Some(rest) = units.get(1..) {
        check_model_helpers(rest);
    }
    if let Some(head) = units.len().checked_sub(1).and_then(|end| units.get(..end)) {
        check_model_helpers(head);
    }
    check_events(&units);
    check_converter(&units, bytes);
}
