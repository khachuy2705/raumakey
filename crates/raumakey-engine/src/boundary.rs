//! Word boundaries: Shift+Space, macro expansion, auto-restore (plan §3.8 "Word boundary"),
//! and tokens across punctuation (plan D28).

use raumakey_model::error::EngineError;
use raumakey_model::input::{InputEvent, ModifierSet, ReplayKey, vk};
use raumakey_model::macros::Boundary;
use raumakey_model::transaction::{EditTransaction, MAX_INSERT_UNITS};
use raumakey_model::utf16;
use unicode_normalization::UnicodeNormalization;

use crate::diff::diff;
use crate::process::{Request, owned_with, result, units};
use crate::render::normalize;
use crate::token::{Committed, Token};
use crate::word::Word;
use crate::{EngineResult, EngineState, Tracking};

/// Text that replaces the last `delete` owned units.
struct Replacement {
    text: String,
    delete: usize,
}

/// Commits the current word at a boundary.
///
/// Shift+Space commits unchanged (D7). Otherwise, at whitespace, an active macro replaces the
/// whole tracked token (D28), else the word; failing that, an invalid transformed word is
/// restored to its raw keys (matrix #15). Without a replacement the delimiter passes through.
/// Whitespace ends the token; punctuation keeps it while it can still match.
pub(crate) fn commit(
    request: &Request<'_>,
    boundary: Boundary,
) -> Result<EngineResult, EngineError> {
    match boundary {
        Boundary::Whitespace => whitespace(request),
        Boundary::Punctuation => punctuation(request),
    }
}

/// A whitespace boundary: ends the word and its token, and resets the state.
fn whitespace(request: &Request<'_>) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    if prior.overflow || prior.is_idle() {
        return Ok(result(
            prior,
            EditTransaction::pass_through(),
            EngineState::new(),
        ));
    }
    let replacement = if is_shift_space(request.event) {
        None
    } else {
        token_macro(request)
            .or_else(|| word_macro(request, Boundary::Whitespace))
            .or_else(|| restore(request))
    };
    let transaction = match replacement {
        Some(replacement) => {
            emit(request, &replacement)?.unwrap_or_else(EditTransaction::pass_through_reset)
        }
        None => EditTransaction::pass_through_reset(),
    };
    Ok(result(prior, transaction, EngineState::new()))
}

/// Boundary punctuation that no action consumed: ends the word. A macro or a restore replaces
/// the word as before (plan §3.8); otherwise the punctuation passes through, and the token
/// goes on while its text still starts a trigger (D28).
fn punctuation(request: &Request<'_>) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    let untracked = EngineState {
        token: request.untracked(),
        ..EngineState::new()
    };
    if prior.overflow {
        return Ok(result(prior, EditTransaction::pass_through(), untracked));
    }
    if let Some(replacement) =
        word_macro(request, Boundary::Punctuation).or_else(|| restore(request))
    {
        let transaction =
            emit(request, &replacement)?.unwrap_or_else(EditTransaction::pass_through_reset);
        return Ok(result(prior, transaction, untracked));
    }
    if let Some(next) = continue_token(request) {
        return Ok(result(prior, EditTransaction::pass_through(), next));
    }
    let transaction = if prior.is_idle() {
        EditTransaction::pass_through()
    } else {
        EditTransaction::pass_through_reset()
    };
    Ok(result(prior, transaction, untracked))
}

/// VIQR `\` followed by bound boundary punctuation: the punctuation replaces the backslash
/// and the word before it (`committed`, without the backslash) is committed unchanged, with
/// no macro and no restore (matrix #8, #49). The escape ends the token too.
pub(crate) fn escape(
    request: &Request<'_>,
    committed: &Word,
    key: char,
) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    let mut text = owned_with(prior, committed, request.normalization())?;
    let mut buffer = [0u16; 2];
    text.extend_from_slice(key.encode_utf16(&mut buffer));
    let edit = diff(&prior.owned, &text);
    let transaction = EditTransaction {
        delete_before: units(edit.delete)?,
        insert_utf16: edit.insert,
        consume_original: true,
        replay_key: None,
        reset_context: true,
    };
    let next = EngineState {
        token: request.untracked(),
        ..EngineState::new()
    };
    Ok(result(prior, transaction, next))
}

fn is_shift_space(event: &InputEvent) -> bool {
    event.modifiers.shift && event.text_utf16 == [u16::from(b' ')]
}

/// Units of the owned text that belong to the current word.
fn word_units(prior: &EngineState) -> usize {
    prior.owned.len().saturating_sub(prior.token.units())
}

/// Looks up `rendered`, then `raw` when it differs.
fn lookup(request: &Request<'_>, rendered: &str, raw: &str, boundary: Boundary) -> Option<String> {
    let macros = request.engine.macros();
    let case_mode = request.case_mode();
    if let Some(expansion) = macros.lookup(rendered, boundary, case_mode) {
        return Some(expansion.replacement);
    }
    if raw == rendered {
        return None;
    }
    macros
        .lookup(raw, boundary, case_mode)
        .map(|expansion| expansion.replacement)
}

/// The whole tracked token (D28): its committed part and the current word, rendered (NFC)
/// and as raw keys.
fn token_macro(request: &Request<'_>) -> Option<Replacement> {
    let prior = request.prior;
    let Token::Tracked(committed) = &prior.token else {
        return None;
    };
    if !request.macros_active() {
        return None;
    }
    let word = &prior.word;
    let rendered: String = format!("{}{}", committed.rendered, word.render_text())
        .nfc()
        .collect();
    let raw: String = format!("{}{}", committed.raw, word.raw_text())
        .nfc()
        .collect();
    let text = lookup(request, &rendered, &raw, Boundary::Whitespace)?;
    Some(Replacement {
        text,
        delete: prior.owned.len(),
    })
}

/// The current word: rendered (NFC) first, then the raw keys when they differ.
fn word_macro(request: &Request<'_>, boundary: Boundary) -> Option<Replacement> {
    let prior = request.prior;
    let word = &prior.word;
    if !request.macros_active() || word.is_empty() {
        return None;
    }
    let rendered: String = word.render_text().nfc().collect();
    let raw: String = word.raw_text().nfc().collect();
    let text = lookup(request, &rendered, &raw, boundary)?;
    Some(Replacement {
        text,
        delete: word_units(prior),
    })
}

/// The raw keys of an invalid transformed word, when auto-restore applies.
fn restore(request: &Request<'_>) -> Option<Replacement> {
    let options = request.options;
    let prior = request.prior;
    let word = &prior.word;
    let applies = options.enabled
        && options.spell_check
        && options.auto_restore_invalid
        && prior.tracking == Tracking::Vietnamese
        && word.transformed()
        && !word.is_complete_syllable();
    applies.then(|| Replacement {
        text: word.raw_text(),
        delete: word_units(prior),
    })
}

/// The state that keeps the token after the punctuation key of `request` passed through,
/// when token triggers take part, the word did not start inside a lost token, and the
/// committed text (rendered or raw, with the punctuation) still starts a trigger (D28).
fn continue_token(request: &Request<'_>) -> Option<EngineState> {
    if !request.tokens_active() {
        return None;
    }
    let prior = request.prior;
    let (rendered, raw) = match &prior.token {
        Token::Start => ("", ""),
        Token::Tracked(committed) => (committed.rendered.as_str(), committed.raw.as_str()),
        Token::Lost => return None,
    };
    let event = request.event;
    let key = String::from_utf16(&event.text_utf16).ok()?;
    let word = &prior.word;
    let rendered: String = format!("{rendered}{}{key}", word.render_text())
        .nfc()
        .collect();
    let raw: String = format!("{raw}{}{key}", word.raw_text()).nfc().collect();
    let macros = request.engine.macros();
    let case_mode = request.case_mode();
    let starts_a_trigger = macros.is_trigger_prefix(&rendered, case_mode)
        || (raw != rendered && macros.is_trigger_prefix(&raw, case_mode));
    if !starts_a_trigger {
        return None;
    }
    let mut owned = prior.owned.clone();
    owned.extend_from_slice(&event.text_utf16);
    let units = owned.len();
    Some(EngineState {
        owner: Some(event.host.owner_key()),
        owned,
        tracking: request.tracking(),
        token: Token::Tracked(Committed {
            rendered,
            raw,
            units,
            before: Box::new(prior.clone()),
        }),
        ..EngineState::new()
    })
}

/// Replaces the last `replacement.delete` owned units with its text and the delimiter (or
/// replays Enter/Tab). `None` when the result would exceed [`MAX_INSERT_UNITS`], which a
/// valid macro never does (plan D30).
///
/// The replacement is engine output, so it follows the configured normalization in every
/// mode; only the literally tracked word of disabled mode is rendered as typed.
fn emit(
    request: &Request<'_>,
    replacement: &Replacement,
) -> Result<Option<EditTransaction>, EngineError> {
    let event = request.event;
    let mut insert = utf16::encode(&normalize(&replacement.text, request.options.normalization));
    let replay_key = replay_key(event);
    if replay_key.is_none() {
        insert.extend_from_slice(&event.text_utf16);
    }
    if insert.len() > MAX_INSERT_UNITS {
        return Ok(None);
    }
    Ok(Some(EditTransaction {
        delete_before: units(replacement.delete)?,
        insert_utf16: insert,
        consume_original: true,
        replay_key,
        reset_context: true,
    }))
}

/// Enter and Tab are replayed rather than inserted as text.
fn replay_key(event: &InputEvent) -> Option<ReplayKey> {
    let virtual_key = event
        .virtual_key
        .filter(|&k| k == vk::RETURN || k == vk::TAB)?;
    let modifiers = event.modifiers;
    Some(ReplayKey {
        virtual_key,
        scan_code: event.scan_code,
        modifiers: ModifierSet {
            shift: modifiers.shift,
            ctrl: modifiers.ctrl,
            alt: modifiers.alt,
            win: modifiers.win,
        },
    })
}
