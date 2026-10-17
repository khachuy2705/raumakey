use raumakey_model::error::EngineError;
use raumakey_model::host::{DocumentId, ExeName, ExeNameError, HostContext, SessionId};
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::{
    InputEvent, InputKind, MAX_EVENT_TEXT_UNITS, Modifiers, is_boundary_punctuation, vk,
};

fn host() -> HostContext {
    HostContext::new(
        SessionId::from_u128(1),
        ExeName::new("notepad.exe").unwrap(),
    )
}

#[test]
fn exe_name_is_lowercase_basename() {
    assert_eq!(ExeName::new("Notepad.EXE").unwrap().as_str(), "notepad.exe");
    assert_eq!(
        ExeName::from_path("C:\\Windows\\System32\\NOTEPAD.exe")
            .unwrap()
            .as_str(),
        "notepad.exe"
    );
    assert_eq!(
        ExeName::from_path("/usr/bin/code").unwrap().as_str(),
        "code"
    );
    assert_eq!(ExeName::new("  "), Err(ExeNameError::Empty));
    assert_eq!(ExeName::new("a\\b.exe"), Err(ExeNameError::InvalidChar));
    assert_eq!(ExeName::new("c:x.exe"), Err(ExeNameError::InvalidChar));
    assert_eq!(ExeName::new("a\u{0}.exe"), Err(ExeNameError::InvalidChar));
    assert_eq!(ExeName::new(&"x".repeat(256)), Err(ExeNameError::TooLong));
    assert!(ExeName::new(&"x".repeat(255)).is_ok());
}

#[test]
fn exe_name_serializes_as_string_and_validates_on_read() {
    let name = ExeName::new("MsEdge.exe").unwrap();
    let json = serde_json::to_string(&name).unwrap();
    assert_eq!(json, "\"msedge.exe\"");
    let back: ExeName = serde_json::from_str("\"Chrome.EXE\"").unwrap();
    assert_eq!(back.as_str(), "chrome.exe");
    assert!(serde_json::from_str::<ExeName>("\"a/b\"").is_err());
}

#[test]
fn host_context_defaults_and_owner() {
    let h = host();
    assert!(h.supports_tsf_range_edit);
    assert!(!h.is_secure_field);
    assert!(!h.is_elevated);
    assert_eq!(h.document_id, None);
    assert_eq!(h.owner_key(), (SessionId::from_u128(1), None));
    let mut d = h.clone();
    d.document_id = Some(DocumentId::from_u128(7));
    assert_eq!(
        d.owner_key(),
        (SessionId::from_u128(1), Some(DocumentId::from_u128(7)))
    );
    assert_eq!(SessionId::from_u128(0x0102).to_u128(), 0x0102);
}

/// Plan §3.8: the boundary punctuation that ends a word when no action consumes it. The
/// engine and the macro table share this definition (D28).
#[test]
fn boundary_punctuation_is_the_plan_list() {
    let listed = ".,;:!?\"'()[]{}<>/\\|";
    for c in listed.chars() {
        assert!(is_boundary_punctuation(c), "{c}");
    }
    for c in "-_+=~`^@#$%&*0aZ ê😀\t".chars() {
        assert!(!is_boundary_punctuation(c), "{c:?}");
    }
}

#[test]
fn character_event_carries_utf16_text() {
    let e = InputEvent::character('ế', host());
    assert_eq!(e.kind, InputKind::Character);
    assert_eq!(e.text_utf16, vec![0x1EBF]);
    assert!(e.validate().is_ok());
    let emoji = InputEvent::character('😀', host());
    assert_eq!(emoji.text_utf16.len(), 2);
    assert!(emoji.validate().is_ok());
}

#[test]
fn event_validation() {
    let lone = InputEvent::text(vec![0xD800], host());
    assert_eq!(lone.validate(), Err(EngineError::InvalidUtf16));
    let long = InputEvent::text(vec![0x61; MAX_EVENT_TEXT_UNITS + 1], host());
    assert_eq!(long.validate(), Err(EngineError::CapacityExceeded));
    let mut bs = InputEvent::backspace(host());
    assert!(bs.validate().is_ok());
    bs.text_utf16 = vec![0x61];
    assert_eq!(bs.validate(), Err(EngineError::InternalInvariant));
    let sep = InputEvent::separator(' ', vk::SPACE, host());
    assert_eq!(sep.kind, InputKind::Separator);
    assert_eq!(sep.virtual_key, Some(vk::SPACE));
    assert!(sep.validate().is_ok());
}

#[test]
fn shortcut_classification_distinguishes_altgr() {
    let ctrl_c = InputEvent::character('c', host()).with_modifiers(Modifiers {
        ctrl: true,
        ..Modifiers::default()
    });
    assert!(ctrl_c.is_shortcut());
    let alt = InputEvent::character('f', host()).with_modifiers(Modifiers {
        alt: true,
        ..Modifiers::default()
    });
    assert!(alt.is_shortcut());
    let altgr = InputEvent::character('€', host()).with_modifiers(Modifiers {
        ctrl: true,
        alt: true,
        alt_gr: true,
        ..Modifiers::default()
    });
    assert!(!altgr.is_shortcut());
    let shift = InputEvent::character('A', host()).with_modifiers(Modifiers {
        shift: true,
        ..Modifiers::default()
    });
    assert!(!shift.is_shortcut());
}

#[test]
fn injected_events_are_detected() {
    let e = InputEvent::character('a', host()).with_injected_tag(RAUMAKEY_INJECTED_TAG);
    assert!(e.is_raumakey_injected());
    let other = InputEvent::character('a', host()).with_injected_tag(0xEBCA_1234);
    assert!(!other.is_raumakey_injected());
    assert!(!InputEvent::character('a', host()).is_raumakey_injected());
}

#[test]
fn navigation_keys() {
    assert!(vk::is_navigation(vk::LEFT));
    assert!(vk::is_navigation(vk::HOME));
    assert!(vk::is_navigation(vk::DELETE));
    assert!(!vk::is_navigation(vk::SPACE));
    let nav = InputEvent::navigation(vk::LEFT, host());
    assert_eq!(nav.kind, InputKind::Navigation);
    assert!(nav.text_utf16.is_empty());
}
