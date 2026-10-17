//! The order of a key-down: the core call with its edit first, the replay of
//! [`KeyDownOutcome::replay`](raumakey_tip::session::KeyDownOutcome::replay) only after the
//! core returned and the edit session ended.

use core::cell::{Cell, RefCell};

use raumakey_ipc::message::MacroSnapshot;
use raumakey_model::host::{DocumentId, ExeName, HostContext, SessionId};
use raumakey_model::input::{Modifiers, ReplayKey, vk};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::utf16;
use raumakey_tip::adapter::keypath::{key_down, with_core};
use raumakey_tip::session::{DocumentPort, EditError, EditRequest, KeyStroke, ServiceCore};

/// A document that applies every edit and records whether an edit is in progress.
struct Document<'a> {
    text: Vec<u16>,
    editing: &'a Cell<bool>,
}

impl DocumentPort for Document<'_> {
    fn replace_before_caret(&mut self, request: &EditRequest<'_>) -> Result<(), EditError> {
        self.editing.set(true);
        let start = self.text.len() - request.expected().len();
        assert_eq!(&self.text[start..], request.expected());
        self.text.truncate(start);
        self.text.extend_from_slice(request.insert());
        self.editing.set(false);
        Ok(())
    }
}

fn host() -> HostContext {
    let mut host = HostContext::new(
        SessionId::from_u128(7),
        ExeName::new("notepad.exe").expect("valid name"),
    );
    host.document_id = Some(DocumentId::from_u128(1));
    host
}

fn stroke(virtual_key: u32, text: &str) -> KeyStroke {
    KeyStroke {
        virtual_key,
        scan_code: virtual_key + 0x100,
        extended: false,
        repeat: false,
        modifiers: Modifiers::default(),
        text: utf16::encode(text),
        extra_info: 0,
        host: host(),
    }
}

/// A core with the macro `ko` → `không`.
fn core_with_macro() -> RefCell<ServiceCore> {
    let mut core = ServiceCore::new();
    core.apply_macro_snapshot(&MacroSnapshot {
        macro_revision: 1,
        entries: vec![MacroEntry {
            id: MacroId(1),
            trigger: "ko".to_owned(),
            replacement: "không".to_owned(),
            enabled: true,
            boundary: BoundaryPolicy::Delimiter,
            case_policy: CasePolicy::Adapt,
        }],
    })
    .expect("valid macros");
    RefCell::new(core)
}

/// Presses a letter that the application types itself.
fn type_letter(core: &RefCell<ServiceCore>, document: &mut Document<'_>, letter: char) {
    let key = stroke(u32::from(letter.to_ascii_uppercase()), &letter.to_string());
    if with_core(core, |core| core.test_key_down(&key)) == Some(true) {
        let eaten = key_down(core, &key, document, |_| panic!("no replay for a letter"));
        assert_eq!(eaten, Some(false), "the application types the letter");
    }
    document.text.extend(utf16::encode(&letter.to_string()));
}

#[test]
fn the_replay_is_sent_after_the_core_returned_and_outside_the_edit() {
    let core = core_with_macro();
    let editing = Cell::new(false);
    let mut document = Document {
        text: Vec::new(),
        editing: &editing,
    };
    type_letter(&core, &mut document, 'k');
    type_letter(&core, &mut document, 'o');

    let enter = stroke(vk::RETURN, "\r");
    assert_eq!(
        with_core(&core, |core| core.test_key_down(&enter)),
        Some(true)
    );
    let mut replays: Vec<ReplayKey> = Vec::new();
    let eaten = key_down(&core, &enter, &mut document, |replay| {
        assert!(core.try_borrow_mut().is_ok(), "the core call has returned");
        assert!(!editing.get(), "the edit session has ended");
        replays.push(replay);
    });
    assert_eq!(eaten, Some(true), "Enter is eaten and replayed");
    assert_eq!(String::from_utf16_lossy(&document.text), "không");
    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].virtual_key, vk::RETURN);
    assert_eq!(replays[0].scan_code, Some(enter.scan_code));
}

#[test]
fn a_busy_core_lets_the_key_through_without_a_replay() {
    let core = core_with_macro();
    let editing = Cell::new(false);
    let mut document = Document {
        text: Vec::new(),
        editing: &editing,
    };
    let held = core.borrow_mut();
    let enter = stroke(vk::RETURN, "\r");
    assert_eq!(
        key_down(&core, &enter, &mut document, |_| panic!("replayed")),
        None
    );
    assert_eq!(with_core(&core, |core| core.vietnamese_mode()), None);
    drop(held);
    assert_eq!(with_core(&core, |core| core.vietnamese_mode()), Some(true));
}

#[test]
fn a_key_that_reaches_the_application_replays_nothing() {
    let core = core_with_macro();
    let editing = Cell::new(false);
    let mut document = Document {
        text: Vec::new(),
        editing: &editing,
    };
    type_letter(&core, &mut document, 'k');
    let space = stroke(vk::SPACE, " ");
    let eaten = key_down(&core, &space, &mut document, |_| panic!("replayed"));
    assert_eq!(eaten, Some(false));
}
