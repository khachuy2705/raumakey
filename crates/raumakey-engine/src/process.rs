//! Event dispatch (plan §3.8), semantic backspace, and overflow (plan §3.9).

use raumakey_model::error::EngineError;
use raumakey_model::input::{InputEvent, InputKind};
use raumakey_model::macros::{Boundary, CaseMode};
use raumakey_model::options::{EngineOptions, Normalization};
use raumakey_model::transaction::EditTransaction;
use raumakey_model::utf16;

use crate::boundary;
use crate::diff::diff;
use crate::render::render;
use crate::token::Token;
use crate::word::{self, Ctx, KeyOutcome, Word};
use crate::{EngineResult, EngineState, STATE_VERSION, Tracking, VietnameseEngine};

/// One `process` call: the immutable inputs.
pub(crate) struct Request<'a> {
    /// Key maps and macro lookup.
    pub(crate) engine: &'a VietnameseEngine,
    /// The event.
    pub(crate) event: &'a InputEvent,
    /// Options for this request.
    pub(crate) options: &'a EngineOptions,
    /// State before the event.
    pub(crate) prior: &'a EngineState,
}

impl Request<'_> {
    /// Rules that track a word started now.
    pub(crate) fn tracking(&self) -> Tracking {
        if self.options.enabled {
            Tracking::Vietnamese
        } else {
            Tracking::Literal
        }
    }

    /// Normalization of the rendered current word: literal tracking reproduces the typed text
    /// exactly. Inserted replacements (macros, restores) use `options.normalization`.
    pub(crate) fn normalization(&self) -> Normalization {
        match self.tracking() {
            Tracking::Vietnamese => self.options.normalization,
            Tracking::Literal => Normalization::Preserve,
        }
    }

    /// `macros_enabled && (enabled || macros_when_disabled)`, and never for an event in a
    /// secure field (D25, matrix #29).
    pub(crate) fn macros_active(&self) -> bool {
        self.options.macros_enabled
            && (self.options.enabled || self.options.macros_when_disabled)
            && !self.event.host.is_secure_field
    }

    /// Token triggers take part (plan D28): macros are active and the lookup has triggers
    /// with boundary punctuation. Otherwise punctuation commits words exactly as plan §3.8
    /// has it, and no token state ever arises.
    pub(crate) fn tokens_active(&self) -> bool {
        self.macros_active() && self.engine.macros().has_token_triggers()
    }

    /// The token after a word that ends inside its token without being tracked further: lost
    /// while token triggers take part, so that no token starts before the next whitespace.
    pub(crate) fn untracked(&self) -> Token {
        if self.tokens_active() {
            Token::Lost
        } else {
            Token::Start
        }
    }

    /// Case handling of macro lookups.
    pub(crate) fn case_mode(&self) -> CaseMode {
        if self.options.auto_case_macro {
            CaseMode::Adaptive
        } else {
            CaseMode::Exact
        }
    }

    /// Keys tracked for one word, clamped into the accepted option range.
    fn context_limit(&self) -> usize {
        let limit = self
            .options
            .context_limit_codepoints
            .clamp(EngineOptions::MIN_CONTEXT, EngineOptions::MAX_CONTEXT);
        usize::try_from(limit).unwrap_or(usize::MAX)
    }
}

/// Gives `next` its generation: an unchanged state keeps the prior one, any change advances it.
pub(crate) fn finish(prior: &EngineState, mut next: EngineState) -> EngineState {
    next.version = STATE_VERSION;
    next.generation = prior.generation;
    if next != *prior {
        next.generation = prior.generation.wrapping_add(1);
    }
    next
}

/// The idle state that follows `prior`.
pub(crate) fn idle_after(prior: &EngineState) -> EngineState {
    finish(prior, EngineState::new())
}

/// Builds a result whose next state follows `prior`.
pub(crate) fn result(
    prior: &EngineState,
    transaction: EditTransaction,
    next: EngineState,
) -> EngineResult {
    EngineResult {
        transaction,
        next: finish(prior, next),
    }
}

/// Pass-through with the given `reset_context`; the state resets.
fn reset_with(prior: &EngineState, reset_context: bool) -> EngineResult {
    let transaction = EditTransaction {
        reset_context,
        ..EditTransaction::pass_through()
    };
    result(prior, transaction, EngineState::new())
}

/// Pass-through with the given `reset_context`; the word ends inside its token, which is not
/// tracked further ([`Request::untracked`]).
fn drop_word(request: &Request<'_>, reset_context: bool) -> EngineResult {
    let transaction = EditTransaction {
        reset_context,
        ..EditTransaction::pass_through()
    };
    let next = EngineState {
        token: request.untracked(),
        ..EngineState::new()
    };
    result(request.prior, transaction, next)
}

/// Commits the current word unchanged and lets the event through.
fn commit_pass_through(request: &Request<'_>) -> EngineResult {
    drop_word(request, !request.prior.is_idle())
}

/// The owned text with `word` as the current word: the committed part of the token, then the
/// word rendered with `normalization`.
pub(crate) fn owned_with(
    prior: &EngineState,
    word: &Word,
    normalization: Normalization,
) -> Result<Vec<u16>, EngineError> {
    let mut owned = prior
        .owned
        .get(..prior.token.units())
        .ok_or(EngineError::InternalInvariant)?
        .to_vec();
    owned.extend(render(word, normalization));
    Ok(owned)
}

/// Lets the event through without touching the state.
fn unchanged(prior: &EngineState) -> EngineResult {
    EngineResult {
        transaction: EditTransaction::pass_through(),
        next: prior.clone(),
    }
}

/// Converts a length to the transaction field type.
pub(crate) fn units(len: usize) -> Result<u32, EngineError> {
    u32::try_from(len).map_err(|_| EngineError::InternalInvariant)
}

/// Processes one event.
pub(crate) fn process(
    engine: &VietnameseEngine,
    event: &InputEvent,
    options: &EngineOptions,
    prior: &EngineState,
) -> Result<EngineResult, EngineError> {
    if prior.version != STATE_VERSION {
        return Err(EngineError::InvalidStateVersion);
    }
    if event.is_raumakey_injected() {
        return Ok(unchanged(prior));
    }
    event.validate()?;
    if !prior.is_idle() && prior.owner != Some(event.host.owner_key()) {
        return Err(EngineError::ContextMismatch);
    }
    // Ctrl/Alt/Win shortcuts of every kind, including Ctrl+Space and Alt+Enter separators
    // (matrix #21), bypass the engine: never consumed, no macro, no restore. A key in a secure
    // field is processed, but never expands a macro (`Request::macros_active`): whether such
    // a field is transformed at all is the adapter's call, made from the policy's
    // secure-field mode before the engine (D25, matrix #29).
    if event.is_shortcut() {
        return Ok(reset_with(prior, true));
    }
    let request = Request {
        engine,
        event,
        options,
        prior,
    };
    if prior.token.is_tracked() && !request.tokens_active() {
        // The options changed under a tracked token without an event that says so: end it,
        // as a change of tracking rules ends a word.
        return Ok(reset_with(prior, true));
    }
    match event.kind {
        InputKind::Navigation
        | InputKind::MouseReset
        | InputKind::FocusReset
        | InputKind::ModeToggle
        | InputKind::ConfigChanged
        | InputKind::Shutdown => Ok(reset_with(prior, true)),
        InputKind::Backspace => backspace(&request),
        InputKind::Separator => boundary::commit(&request, Boundary::Whitespace),
        InputKind::Character => character(&request),
    }
}

/// Space, tab, and line breaks typed as characters are separators.
fn is_whitespace_key(key: char) -> bool {
    matches!(key, ' ' | '\t' | '\r' | '\n')
}

fn character(request: &Request<'_>) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    let Some(key) = utf16::single_scalar(&request.event.text_utf16) else {
        return Ok(commit_pass_through(request));
    };
    if is_whitespace_key(key) {
        return boundary::commit(request, Boundary::Whitespace);
    }
    if key.is_control() {
        return Ok(commit_pass_through(request));
    }
    if !prior.is_idle() && prior.tracking != request.tracking() {
        return Ok(reset_with(prior, true));
    }
    if prior.overflow {
        return if word::is_boundary_punctuation(key) {
            boundary::commit(request, Boundary::Punctuation)
        } else {
            Ok(unchanged(prior))
        };
    }
    let options = request.options;
    if options.enabled {
        let keymap = request.engine.keymaps().resolve(options)?;
        let ctx = Ctx {
            spell_check: options.spell_check,
            free_marking: options.free_marking,
            modern_tone: options.modern_tone,
        };
        match word::process_key(&prior.word, key, keymap.lookup(key), &ctx) {
            KeyOutcome::Boundary => boundary::commit(request, Boundary::Punctuation),
            KeyOutcome::EscapedBoundary(committed) => boundary::escape(request, &committed, key),
            KeyOutcome::Changed(next) => change(request, next),
        }
    } else if request.macros_active() {
        if word::is_boundary_punctuation(key) {
            return boundary::commit(request, Boundary::Punctuation);
        }
        let mut next = prior.word.clone();
        next.push_plain(key);
        change(request, next)
    } else {
        Ok(commit_pass_through(request))
    }
}

/// Moves to `word`: snapshot the prior word, render, and diff (plan §3.7, §3.9). A tracked
/// token's committed part stays in front of the word (plan D28).
fn change(request: &Request<'_>, word: Word) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    if word.raw.len() > request.context_limit() {
        // Overflow: drop ownership, of the token too; further characters pass through until
        // a boundary.
        let next = EngineState {
            overflow: true,
            token: request.untracked(),
            ..EngineState::new()
        };
        return Ok(result(prior, EditTransaction::pass_through_reset(), next));
    }
    let owned = owned_with(prior, &word, request.normalization())?;
    let appended_own_text = owned.len() == prior.owned.len() + request.event.text_utf16.len()
        && owned.starts_with(&prior.owned)
        && owned.ends_with(&request.event.text_utf16);
    let transaction = if appended_own_text {
        EditTransaction::pass_through()
    } else {
        let edit = diff(&prior.owned, &owned);
        EditTransaction {
            delete_before: units(edit.delete)?,
            insert_utf16: edit.insert,
            consume_original: true,
            replay_key: None,
            reset_context: false,
        }
    };
    let mut history = prior.history.clone();
    history.push(prior.word.clone());
    let next = EngineState {
        owner: Some(request.event.host.owner_key()),
        word,
        owned,
        history,
        tracking: request.tracking(),
        token: prior.token.clone(),
        ..EngineState::new()
    };
    Ok(result(prior, transaction, next))
}

/// Semantic backspace (plan §3.9). Inside the current word it keeps the token; with an empty
/// word it deletes the punctuation that ended the token's committed part and returns to the
/// state before it (plan D28).
fn backspace(request: &Request<'_>) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    if prior.overflow || prior.is_idle() {
        return Ok(unchanged(prior));
    }
    if prior.tracking != request.tracking() {
        return Ok(reset_with(prior, true));
    }
    if prior.word.is_empty() {
        let Token::Tracked(committed) = &prior.token else {
            // Unreachable: only a tracked token owns text without a word.
            return Ok(drop_word(request, true));
        };
        // Only the committed part is owned, ending with its punctuation: delete that and
        // return to the state before it, whose owned text is the rest.
        let before = committed.before.as_ref().clone();
        let edit = diff(&prior.owned, &before.owned);
        let transaction = EditTransaction {
            delete_before: units(edit.delete)?,
            insert_utf16: edit.insert,
            consume_original: true,
            replay_key: None,
            reset_context: false,
        };
        return Ok(result(prior, transaction, before));
    }
    let mut history = prior.history.clone();
    let Some(word) = history.pop() else {
        // History exhausted: let the host delete and stop owning the word.
        return Ok(drop_word(request, true));
    };
    let owned = owned_with(prior, &word, request.normalization())?;
    let edit = diff(&prior.owned, &owned);
    let transaction = EditTransaction {
        delete_before: units(edit.delete)?,
        insert_utf16: edit.insert,
        consume_original: true,
        replay_key: None,
        reset_context: false,
    };
    let next = if word.is_empty() && !prior.token.is_tracked() {
        EngineState {
            token: prior.token.clone(),
            ..EngineState::new()
        }
    } else {
        EngineState {
            owner: prior.owner,
            word,
            owned,
            history,
            tracking: prior.tracking,
            token: prior.token.clone(),
            ..EngineState::new()
        }
    };
    Ok(result(prior, transaction, next))
}
