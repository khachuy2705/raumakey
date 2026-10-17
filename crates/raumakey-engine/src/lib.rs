//! Raumakey platform-independent Vietnamese input state machine.
//!
//! [`VietnameseEngine::process`] is a pure function of the engine (key maps and macro snapshot),
//! one [`InputEvent`], the [`EngineOptions`], and the prior [`EngineState`]. It returns the
//! [`EditTransaction`] the adapter must apply and the state to commit only after a successful
//! apply. The engine never mutates its inputs, holds no platform resources, and never logs.
//!
//! Behavior follows the implementation plan §3:
//!
//! - Telex, VNI, VIQR, and validated user key maps map keys to semantic actions; a key whose
//!   actions do not apply is typed literally (never swallowed).
//! - The word model keeps letters with marks plus one word-level tone placed by the old or
//!   modern rule; a repeated action undoes itself and makes the rest of the word literal.
//! - With spell checking, transformations stay inside the syllable grammar. A prefix may still
//!   lack marks that later keys add (`ưo` → `ươ`, `tieng` → `tiêng`).
//! - Boundaries expand macros, restore invalid transformed words to their raw keys, and reset
//!   the state; Shift+Space and VIQR-escaped punctuation commit the word unchanged.
//! - A whitespace boundary also looks up the token typed since the previous whitespace, so a
//!   trigger with boundary punctuation (`v.v`) expands (plan D28). The engine tracks such a
//!   token only while the macro lookup has triggers with boundary punctuation and the text
//!   committed so far starts one of them; it then owns the whole token's rendered text, so
//!   the replacement deletes only text the adapter can check.
//! - Backspace restores up to [`HISTORY_CAPACITY`] snapshots; a word longer than the context
//!   limit stops being owned until the next boundary.
//! - A key in a secure field never expands a macro and is otherwise processed like any other
//!   key (plan D25). Whether a secure field is transformed at all is the adapter's decision,
//!   taken from the host policy's secure-field mode before the engine: the default `Bypass`
//!   passes such keys through untouched, `Restricted` lets the engine transform them.
//!   Ctrl/Alt/Win shortcuts still bypass the engine.
//!
//! [`script`] drives the engine from key scripts against the model's `TextSimulator`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod boundary;
mod diff;
mod history;
mod keymaps;
mod letters;
mod process;
mod render;
pub mod script;
mod syllable;
mod token;
mod word;

use std::fmt;
use std::sync::Arc;

use raumakey_model::error::{EngineError, ResetReason};
use raumakey_model::host::{DocumentId, SessionId};
use raumakey_model::input::InputEvent;
use raumakey_model::keymap::KeyMap;
use raumakey_model::macros::MacroLookup;
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_model::transaction::EditTransaction;

use crate::history::History;
use crate::keymaps::KeymapSet;
use crate::token::Token;
use crate::word::Word;

/// Version of [`EngineState`]; a state with another version is rejected.
pub const STATE_VERSION: u32 = 1;

/// Number of word snapshots kept for semantic backspace.
pub const HISTORY_CAPACITY: usize = 32;

/// Structural form of the current word: C is a consonant onset or coda, V the vowel cluster.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WordForm {
    /// No word in progress.
    Empty,
    /// Onset only (`ng`, `qu`).
    C,
    /// Vowel cluster only (`a`, `ươ`).
    V,
    /// Onset and vowel cluster (`hoa`).
    CV,
    /// Vowel cluster and coda (`anh`).
    VC,
    /// Onset, vowel cluster, and coda (`tiếng`).
    CVC,
    /// Not a Vietnamese syllable or prefix.
    NonVietnamese,
}

/// Which rules track the current word.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Tracking {
    /// Vietnamese mode.
    #[default]
    Vietnamese,
    /// Disabled mode with macros: the word is tracked literally.
    Literal,
}

/// Session-scoped engine state. It holds no operating-system resources.
///
/// `Debug` shows the word in progress; the engine never logs it and callers must not either.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineState {
    pub(crate) version: u32,
    pub(crate) generation: u64,
    pub(crate) owner: Option<(SessionId, Option<DocumentId>)>,
    pub(crate) word: Word,
    pub(crate) owned: Vec<u16>,
    pub(crate) history: History<Word>,
    pub(crate) overflow: bool,
    pub(crate) tracking: Tracking,
    /// The token of the current word (plan D28); its committed part starts `owned`.
    pub(crate) token: Token,
}

impl EngineState {
    /// An idle state of the current [`STATE_VERSION`].
    pub fn new() -> Self {
        Self {
            version: STATE_VERSION,
            generation: 0,
            owner: None,
            word: Word::default(),
            owned: Vec::new(),
            history: History::default(),
            overflow: false,
            tracking: Tracking::default(),
            token: Token::Start,
        }
    }

    /// State format version.
    pub fn version(&self) -> u32 {
        self.version
    }

    /// Counter that changes whenever the state changes.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// True when the engine owns no text before the caret.
    pub fn is_idle(&self) -> bool {
        self.owned.is_empty()
    }

    /// The text the engine believes is immediately before the caret and may replace: the
    /// rendered current word, preceded by the committed part of its token while the engine
    /// tracks one (plan D28).
    pub fn owned_utf16(&self) -> &[u16] {
        &self.owned
    }

    /// Structural form of the current word (`NonVietnamese` while an overlong word is skipped).
    pub fn word_form(&self) -> WordForm {
        if self.overflow {
            WordForm::NonVietnamese
        } else {
            self.word.word_form()
        }
    }
}

impl Default for EngineState {
    fn default() -> Self {
        Self::new()
    }
}

/// The transaction to apply and the state to commit after a successful apply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineResult {
    /// Edit for the adapter.
    pub transaction: EditTransaction,
    /// State to commit only after the transaction was applied.
    pub next: EngineState,
}

/// The Vietnamese input engine: immutable key maps plus a macro lookup snapshot.
#[derive(Clone)]
pub struct VietnameseEngine {
    keymaps: Arc<KeymapSet>,
    macros: Arc<dyn MacroLookup>,
}

impl fmt::Debug for VietnameseEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VietnameseEngine").finish_non_exhaustive()
    }
}

impl VietnameseEngine {
    /// Creates an engine. Invalid user key maps are ignored; for duplicate ids the first wins.
    pub fn new(user_keymaps: Vec<KeyMap>, macros: Arc<dyn MacroLookup>) -> Self {
        Self {
            keymaps: Arc::new(KeymapSet::new(user_keymaps)),
            macros,
        }
    }

    /// The built-in key map of `method`; `UserDefined` has none.
    pub fn builtin_keymap(method: InputMethod) -> Option<&'static KeyMap> {
        keymaps::builtin(method)
    }

    /// Processes one event against `prior` without mutating it.
    pub fn process(
        &self,
        event: &InputEvent,
        options: &EngineOptions,
        prior: &EngineState,
    ) -> Result<EngineResult, EngineError> {
        process::process(self, event, options, prior)
    }

    /// An idle state that follows `prior` (the reason is informational).
    pub fn reset(&self, prior: &EngineState, reason: ResetReason) -> EngineState {
        let _ = reason;
        process::idle_after(prior)
    }

    pub(crate) fn keymaps(&self) -> &KeymapSet {
        &self.keymaps
    }

    pub(crate) fn macros(&self) -> &dyn MacroLookup {
        self.macros.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use raumakey_model::host::{ExeName, HostContext};
    use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
    use raumakey_model::macros::NoMacros;

    use super::*;

    #[test]
    fn rejects_states_of_another_version() {
        let engine = VietnameseEngine::new(Vec::new(), Arc::new(NoMacros));
        let host = HostContext::new(
            SessionId::from_u128(1),
            ExeName::new("app.exe").expect("valid name"),
        );
        let event = InputEvent::character('a', host);
        let options = EngineOptions::defaults_v1();
        let mut prior = EngineState::new();
        prior.version = STATE_VERSION + 1;
        assert_eq!(
            engine.process(&event, &options, &prior),
            Err(EngineError::InvalidStateVersion)
        );
        let injected = event.clone().with_injected_tag(RAUMAKEY_INJECTED_TAG);
        assert_eq!(
            engine.process(&injected, &options, &prior),
            Err(EngineError::InvalidStateVersion)
        );
        let reset = engine.reset(&prior, ResetReason::HardReset);
        assert_eq!(reset.version(), STATE_VERSION);
        assert!(engine.process(&event, &options, &reset).is_ok());
    }
}
