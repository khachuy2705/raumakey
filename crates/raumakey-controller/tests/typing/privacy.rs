//! Nothing of the typing front end shows what the user typed: `Debug` hides keys, text,
//! words, and macro text.

use raumakey_controller::typing::{BlindDocument, KeyTracker, TypingUpdate};
use raumakey_ipc::message::MacroSnapshot;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_session::{DocumentPort, EditRequest};
use raumakey_windows::hooks::KeyboardEvent;

use crate::support::{NOTEPAD, Sim};

fn letter(virtual_key: u32) -> KeyboardEvent {
    KeyboardEvent {
        vk: virtual_key,
        scan: 0x1E,
        extended: false,
        injected: false,
        injected_lower_il: false,
        alt_down: false,
        key_up: false,
        time: 0,
        extra_info: 0,
    }
}

#[test]
fn debug_output_never_shows_keys_text_or_macros() {
    let mut sim = Sim::notepad();
    sim.type_text("tieengs");
    assert_eq!(sim.text(NOTEPAD), "tiếng");
    let handler = format!("{:?}", sim.handler);
    for secret in ["tiếng", "tieeng", "iếng", "ếng"] {
        assert!(!handler.contains(secret), "{handler}");
    }

    let mut tracker = KeyTracker::new(false);
    tracker.observe(&letter(0x51));
    let tracker = format!("{tracker:?}");
    assert!(
        !tracker.contains("81") && !tracker.contains("0x51"),
        "{tracker}"
    );

    let mut document = BlindDocument::new();
    let units: Vec<u16> = "bí mật".encode_utf16().collect();
    document
        .replace_before_caret(&EditRequest::new(&units, &units))
        .expect("fits");
    let document = format!("{document:?}");
    assert!(
        !document.contains("bí") && !document.contains("237"),
        "{document}"
    );

    let update = TypingUpdate::Macros(MacroSnapshot {
        macro_revision: 1,
        entries: vec![MacroEntry {
            id: MacroId(1),
            trigger: "mk".to_owned(),
            replacement: "mật khẩu bí mật".to_owned(),
            enabled: true,
            boundary: BoundaryPolicy::Delimiter,
            case_policy: CasePolicy::Adapt,
        }],
    });
    let update = format!("{update:?}");
    assert!(
        !update.contains("mk") && !update.contains("khẩu"),
        "{update}"
    );
    let mailbox = format!("{:?}", sim.mailbox);
    assert!(!mailbox.contains("khẩu"), "{mailbox}");
}
