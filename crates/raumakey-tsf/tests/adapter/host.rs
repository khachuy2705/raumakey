//! The `HostContext` of a key: the session of the activation, the document of the context,
//! its input scope, and the process.

use raumakey_model::host::ExeName;
use raumakey_tip::adapter::documents::Contexts;
use raumakey_tip::adapter::host::{HostFacts, UNKNOWN_EXECUTABLE, executable_name, session_id};

fn facts() -> HostFacts {
    HostFacts {
        session_id: session_id(4, 1),
        process_id: 4,
        thread_id: 8,
        executable: ExeName::new("notepad.exe").expect("valid name"),
        is_elevated: false,
    }
}

#[test]
fn every_activation_and_every_process_gets_its_own_session() {
    let sessions = [session_id(4, 1), session_id(4, 2), session_id(5, 1)];
    assert_ne!(sessions[0], sessions[1]);
    assert_ne!(sessions[0], sessions[2]);
    assert_ne!(sessions[1], sessions[2]);
    assert_eq!(session_id(4, 1), session_id(4, 1));
}

#[test]
fn a_key_in_a_context_carries_its_document_and_input_scope() {
    let facts = facts();
    let mut contexts = Contexts::default();
    let entered = contexts.enter(0x1000, Some(0x2000));
    let host = facts.context(contexts.get(0x1000), "00000409");
    assert_eq!(host.session_id, facts.session_id);
    assert_eq!(host.document_id, Some(entered.id));
    assert_eq!((host.process_id, host.thread_id), (4, 8));
    assert_eq!(host.executable.as_str(), "notepad.exe");
    assert!(host.supports_tsf_range_edit);
    assert!(!host.is_elevated);
    assert_eq!(host.keyboard_layout, "00000409");
    assert_eq!(host.window_class, None);
    assert!(
        host.is_secure_field,
        "an input scope that was not read yet counts as secure"
    );

    let info = contexts.get_mut(0x1000).expect("entered");
    info.secure = Some(false);
    info.window_class = Some("Edit".to_owned());
    let host = facts.context(contexts.get(0x1000), "0000042A");
    assert!(!host.is_secure_field);
    assert_eq!(host.window_class.as_deref(), Some("Edit"));
    assert_eq!(host.keyboard_layout, "0000042A");
}

#[test]
fn a_key_without_a_context_has_no_document() {
    let host = facts().context(None, "00000409");
    assert_eq!(host.document_id, None);
    assert!(!host.is_secure_field);
    assert!(host.supports_tsf_range_edit);
}

#[test]
fn the_executable_is_the_lowercase_file_name_of_the_process() {
    let name = |path| executable_name(path).map(|name| name.as_str().to_owned());
    assert_eq!(
        name(Some(r"C:\Windows\System32\NOTEPAD.EXE")).as_deref(),
        Some("notepad.exe")
    );
    assert_eq!(name(Some("relative/Word.exe")).as_deref(), Some("word.exe"));
    // A name that no file can have, so that no rule of a real executable applies.
    assert_eq!(UNKNOWN_EXECUTABLE, "<unknown>");
    assert_eq!(name(None).as_deref(), Some(UNKNOWN_EXECUTABLE));
    assert_eq!(
        name(Some(r"C:\folder\")).as_deref(),
        Some(UNKNOWN_EXECUTABLE)
    );
}
