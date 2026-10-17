//! Fakes and drivers shared by the session tests.
//!
//! [`FakeDocument`] is a text field over a UTF-16 buffer with a caret. It implements
//! [`DocumentPort`] the way the TSF edit session will: move the start of the selection back,
//! require exactly the requested units with the expected text, replace them, and collapse the
//! caret to the end. It can be told to deny edits, to reach fewer units than requested, or to
//! have its text and caret changed behind the core's back. It also plays the application: a
//! key that was not eaten reaches [`FakeDocument::app_key`], which edits the buffer as a
//! simple text control would.
//!
//! [`FakeDocument::press`] drives one key press exactly as TSF and the future COM layer will:
//! `OnTestKeyDown`, `OnKeyDown` only when the test claimed the key, the application's own
//! handling when the key was not eaten (reported back to the core as a [`Change`] that
//! Raumakey did not make, as `ITfTextEditSink::OnEndEdit` would, right away), the replay key
//! sent with `RAUMAKEY_INJECTED_TAG`, and the key-up. Tests that need a host reporting late,
//! never, or in one piece call the core and [`FakeDocument::app_key`] themselves.

#![allow(dead_code)]

use raumakey_model::host::{DocumentId, ExeName, HostContext, SessionId};
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::{Modifiers, ReplayKey, vk};
use raumakey_model::utf16;
use raumakey_session::{
    DocumentPort, EditError, EditRequest, KeyDownOutcome, KeyStroke, SelectionChange, ServiceCore,
};

/// Session id of every fake host.
pub const SESSION: SessionId = SessionId::from_u128(0x5E55_1011);

/// One edit the fake document applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    /// Units removed before the caret.
    pub delete_before: u32,
    /// Text inserted.
    pub insert: String,
}

/// Shorthand for an expected [`Edit`].
pub fn edit(delete_before: u32, insert: &str) -> Edit {
    Edit {
        delete_before,
        insert: insert.to_owned(),
    }
}

/// A change the application made to the fake document, as the COM layer reads it from the
/// edit record and reports it to [`ServiceCore::selection_changed`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// Text typed at the caret, which is now right after it.
    Inserted(Vec<u16>),
    /// Text deleted right before the caret.
    Deleted,
    /// The caret moved.
    Moved,
}

impl Change {
    /// What the COM layer reports for this change.
    pub fn report(&self) -> SelectionChange<'_> {
        match self {
            Self::Inserted(text) => SelectionChange::Inserted(text),
            Self::Deleted => SelectionChange::Deleted,
            Self::Moved => SelectionChange::Other,
        }
    }
}

/// How the fake document answers edit requests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Behavior {
    /// Apply edits that meet the contract.
    #[default]
    Accept,
    /// Refuse every edit, as a read-only or locked document would.
    Deny,
    /// The start of the selection can move back at most this many units.
    Shorter(u32),
}

/// What one key press did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Press {
    /// `test_key_down` claimed the key, so TSF called `key_down`.
    pub claimed: bool,
    /// The outcome of `key_down` (a pass-through when the key was not claimed).
    pub outcome: KeyDownOutcome,
    /// `key_up` ate the key-up.
    pub up_eaten: bool,
}

impl Press {
    /// The key reached the application.
    pub fn reached_app(&self) -> bool {
        !self.outcome.eaten
    }
}

/// What a key-down did (the key stays down).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Down {
    /// `test_key_down` claimed the key.
    pub claimed: bool,
    /// The outcome of `key_down` (a pass-through when the key was not claimed).
    pub outcome: KeyDownOutcome,
}

/// A host context for document `id` in `notepad.exe`.
pub fn host(id: u128) -> HostContext {
    let mut host = HostContext::new(SESSION, ExeName::new("notepad.exe").expect("valid name"));
    host.document_id = Some(DocumentId::from_u128(id));
    host
}

/// Virtual key of a typed character in these tests.
fn virtual_key_of(c: char) -> u32 {
    match c {
        ' ' => vk::SPACE,
        '\r' => vk::RETURN,
        '\t' => vk::TAB,
        c if c.is_ascii_alphanumeric() => u32::from(c.to_ascii_uppercase()),
        _ => 0xBA,
    }
}

/// A key stroke in `host` with no modifiers.
pub fn stroke(virtual_key: u32, text: &str, host: &HostContext) -> KeyStroke {
    KeyStroke {
        virtual_key,
        scan_code: virtual_key.wrapping_add(0x100),
        extended: false,
        repeat: false,
        modifiers: Modifiers::default(),
        text: utf16::encode(text),
        extra_info: 0,
        host: host.clone(),
    }
}

/// The text a non-character key produces, as `ToUnicodeEx` reports it.
pub fn key_text(virtual_key: u32) -> &'static str {
    match virtual_key {
        vk::SPACE => " ",
        vk::RETURN => "\r",
        vk::TAB => "\t",
        vk::BACK => "\u{8}",
        vk::ESCAPE => "\u{1b}",
        _ => "",
    }
}

/// The stroke of a replayed key as `SendInput` delivers it: tagged with
/// `RAUMAKEY_INJECTED_TAG`.
pub fn replay_stroke(replay: ReplayKey, host: &HostContext) -> KeyStroke {
    let mut key = stroke(replay.virtual_key, key_text(replay.virtual_key), host);
    if let Some(scan) = replay.scan_code {
        key.scan_code = scan;
    }
    key.modifiers.shift = replay.modifiers.shift;
    key.modifiers.ctrl = replay.modifiers.ctrl;
    key.modifiers.alt = replay.modifiers.alt;
    key.modifiers.win = replay.modifiers.win;
    key.extra_info = RAUMAKEY_INJECTED_TAG;
    key
}

/// Releases `key`: `OnTestKeyUp`, then `OnKeyUp` when claimed. Returns whether it was eaten.
pub fn release(core: &mut ServiceCore, key: &KeyStroke) -> bool {
    if core.test_key_up(key) {
        core.key_up(key)
    } else {
        false
    }
}

/// A text field with a caret over a UTF-16 buffer.
#[derive(Debug)]
pub struct FakeDocument {
    /// Document id.
    pub id: DocumentId,
    /// Host context the COM layer reports for this document.
    pub host: HostContext,
    text: Vec<u16>,
    caret: usize,
    /// How edit requests are answered.
    pub behavior: Behavior,
    /// Edits applied, in order.
    pub edits: Vec<Edit>,
    /// Edit requests received (applied or not).
    pub attempts: usize,
    /// Failures returned, in order.
    pub failures: Vec<EditError>,
    /// `Debug` output of every edit request received.
    pub request_debug: Vec<String>,
}

impl FakeDocument {
    /// An empty document `id` in `notepad.exe`.
    pub fn new(id: u128) -> Self {
        Self::with_host(host(id))
    }

    /// An empty document with the given host context.
    pub fn with_host(host: HostContext) -> Self {
        Self {
            id: host.document_id.unwrap_or_default(),
            host,
            text: Vec::new(),
            caret: 0,
            behavior: Behavior::Accept,
            edits: Vec::new(),
            attempts: 0,
            failures: Vec::new(),
            request_debug: Vec::new(),
        }
    }

    /// Document text.
    pub fn text(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    /// Caret position in UTF-16 units.
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Moves the caret behind the core's back (a click, or a change made by the application).
    pub fn move_caret(&mut self, caret: usize) {
        self.caret = caret.min(self.text.len());
    }

    /// Replaces the units before the caret behind the core's back.
    pub fn replace_before_caret_behind(&mut self, delete: usize, insert: &str) {
        let start = self.caret.saturating_sub(delete);
        let insert = utf16::encode(insert);
        self.text.splice(start..self.caret, insert.iter().copied());
        self.caret = start + insert.len();
    }

    /// A key that types `c` (Shift held for ASCII uppercase letters).
    pub fn char_key(&self, c: char) -> KeyStroke {
        let mut buffer = [0u16; 2];
        let text = c.encode_utf16(&mut buffer);
        let mut key = stroke(virtual_key_of(c), "", &self.host);
        key.text = text.to_vec();
        key.modifiers.shift = c.is_ascii_uppercase();
        key
    }

    /// A non-character key (Backspace, arrows, Enter, Space, …) with its usual text.
    pub fn key(&self, virtual_key: u32) -> KeyStroke {
        stroke(virtual_key, key_text(virtual_key), &self.host)
    }

    /// Presses `key` and keeps it down: `OnTestKeyDown`, `OnKeyDown` when claimed, the
    /// application when the key was not eaten, and the tagged replay.
    pub fn hold(&mut self, core: &mut ServiceCore, key: &KeyStroke) -> Down {
        let claimed = core.test_key_down(key);
        let outcome = if claimed {
            core.key_down(key, self)
        } else {
            KeyDownOutcome::pass_through()
        };
        if !outcome.eaten {
            self.deliver(core, key);
        }
        if let Some(replay) = outcome.replay {
            let injected = replay_stroke(replay, &self.host);
            assert!(
                !core.test_key_down(&injected),
                "an injected replay must not be claimed"
            );
            self.deliver(core, &injected);
            assert!(!release(core, &injected), "an injected key-up is not eaten");
        }
        Down { claimed, outcome }
    }

    /// The application handles `key`, and the change it makes is reported to the core.
    pub fn deliver(&mut self, core: &mut ServiceCore, key: &KeyStroke) {
        if let Some(change) = self.app_key(key) {
            core.selection_changed(self.id, change.report());
        }
    }

    /// Presses and releases `key` the way TSF and the COM layer drive the core.
    pub fn press(&mut self, core: &mut ServiceCore, key: &KeyStroke) -> Press {
        let Down { claimed, outcome } = self.hold(core, key);
        let up_eaten = release(core, key);
        Press {
            claimed,
            outcome,
            up_eaten,
        }
    }

    /// Types one character.
    pub fn type_char(&mut self, core: &mut ServiceCore, c: char) -> Press {
        let key = self.char_key(c);
        self.press(core, &key)
    }

    /// Types every character of `text` (space, `\r`, and `\t` as their keys).
    pub fn type_text(&mut self, core: &mut ServiceCore, text: &str) -> Vec<Press> {
        text.chars().map(|c| self.type_char(core, c)).collect()
    }

    /// Presses a non-character key.
    pub fn type_key(&mut self, core: &mut ServiceCore, virtual_key: u32) -> Press {
        let key = self.key(virtual_key);
        self.press(core, &key)
    }

    /// The application's own handling of a key that reached it. Returns what changed, if
    /// anything.
    pub fn app_key(&mut self, key: &KeyStroke) -> Option<Change> {
        let m = key.modifiers;
        if (m.ctrl || m.alt || m.win) && !m.alt_gr {
            return None;
        }
        match key.virtual_key {
            vk::BACK => {
                let before = &self.text[..self.caret];
                let len = utf16::last_scalar_len(before);
                if len == 0 {
                    return None;
                }
                self.text.drain(self.caret - len..self.caret);
                self.caret -= len;
                Some(Change::Deleted)
            }
            vk::LEFT => {
                let len = utf16::last_scalar_len(&self.text[..self.caret]);
                self.caret -= len;
                (len > 0).then_some(Change::Moved)
            }
            vk::RIGHT | vk::END => {
                let moved = self.caret < self.text.len();
                self.caret = self.text.len();
                moved.then_some(Change::Moved)
            }
            vk::HOME => {
                let moved = self.caret > 0;
                self.caret = 0;
                moved.then_some(Change::Moved)
            }
            _ if vk::is_navigation(key.virtual_key) || key.text.is_empty() => None,
            _ => {
                let text = key.text.clone();
                self.text
                    .splice(self.caret..self.caret, text.iter().copied());
                self.caret += text.len();
                Some(Change::Inserted(text))
            }
        }
    }

    fn try_replace(&mut self, request: &EditRequest<'_>) -> Result<(), EditError> {
        if self.behavior == Behavior::Deny {
            return Err(EditError::Denied);
        }
        let requested = request.delete_before() as usize;
        let reachable = match self.behavior {
            Behavior::Shorter(limit) => self.caret.min(limit as usize),
            Behavior::Accept | Behavior::Deny => self.caret,
        };
        if requested > reachable {
            return Err(EditError::RangeShorter {
                covered: u32::try_from(reachable).expect("small"),
            });
        }
        let start = self.caret - requested;
        if self.text[start..self.caret] != *request.expected() {
            return Err(EditError::TextMismatch);
        }
        self.text
            .splice(start..self.caret, request.insert().iter().copied());
        self.caret = start + request.insert().len();
        Ok(())
    }
}

impl DocumentPort for FakeDocument {
    fn replace_before_caret(&mut self, request: &EditRequest<'_>) -> Result<(), EditError> {
        self.attempts += 1;
        self.request_debug.push(format!("{request:?}"));
        assert_eq!(
            request.expected().len(),
            request.delete_before() as usize,
            "the expected text covers exactly the units to delete"
        );
        assert!(
            utf16::is_well_formed(request.insert()),
            "the insert is well-formed UTF-16"
        );
        let result = self.try_replace(request);
        match result {
            Ok(()) => self.edits.push(Edit {
                delete_before: request.delete_before(),
                insert: String::from_utf16_lossy(request.insert()),
            }),
            Err(error) => self.failures.push(error),
        }
        result
    }
}
