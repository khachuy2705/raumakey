//! Word boundaries: Shift+Space, macro expansion, auto-restore (plan §3.8 "Word boundary").

use raumakey_model::error::EngineError;
use raumakey_model::input::{InputEvent, ModifierSet, ReplayKey, vk};
use raumakey_model::macros::{Boundary, CaseMode};
use raumakey_model::transaction::{EditTransaction, MAX_INSERT_UNITS};
use raumakey_model::utf16;
use unicode_normalization::UnicodeNormalization;

use crate::diff::diff;
use crate::process::{Request, result, units};
use crate::render::{normalize, render};
use crate::word::Word;
use crate::{EngineResult, EngineState, Tracking};

/// Commits the current word at a boundary and resets the state.
///
/// Shift+Space commits unchanged (D7). Otherwise an active macro replaces the word; failing
/// that, an invalid transformed word is restored to its raw keys (matrix #15). Without a
/// replacement the delimiter passes through.
pub(crate) fn commit(
    request: &Request<'_>,
    boundary: Boundary,
) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    if prior.overflow || prior.word.is_empty() {
        return Ok(result(
            prior,
            EditTransaction::pass_through(),
            EngineState::new(),
        ));
    }
    let replacement = if is_shift_space(request.event, boundary) {
        None
    } else {
        macro_replacement(request, boundary).or_else(|| restore(request))
    };
    let transaction = match replacement {
        Some(text) => emit(request, &text)?.unwrap_or_else(EditTransaction::pass_through_reset),
        None => EditTransaction::pass_through_reset(),
    };
    Ok(result(prior, transaction, EngineState::new()))
}

/// VIQR `\` followed by bound boundary punctuation: the punctuation replaces the backslash
/// and the word before it (`committed`, without the backslash) is committed unchanged, with
/// no macro and no restore (matrix #8, #49).
pub(crate) fn escape(
    request: &Request<'_>,
    committed: &Word,
    key: char,
) -> Result<EngineResult, EngineError> {
    let prior = request.prior;
    let mut text = render(committed, request.normalization());
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
    Ok(result(prior, transaction, EngineState::new()))
}

fn is_shift_space(event: &InputEvent, boundary: Boundary) -> bool {
    boundary == Boundary::Whitespace
        && event.modifiers.shift
        && event.text_utf16 == [u16::from(b' ')]
}

/// Looks up the rendered NFC word, then the raw keys when they differ.
fn macro_replacement(request: &Request<'_>, boundary: Boundary) -> Option<String> {
    if !request.macros_active() {
        return None;
    }
    let word = &request.prior.word;
    let case_mode = if request.options.auto_case_macro {
        CaseMode::Adaptive
    } else {
        CaseMode::Exact
    };
    let macros = request.engine.macros();
    let rendered: String = word.render_text().nfc().collect();
    if let Some(expansion) = macros.lookup(&rendered, boundary, case_mode) {
        return Some(expansion.replacement);
    }
    let raw: String = word.raw_text().nfc().collect();
    if raw == rendered {
        return None;
    }
    macros
        .lookup(&raw, boundary, case_mode)
        .map(|expansion| expansion.replacement)
}

/// The raw keys of an invalid transformed word, when auto-restore applies.
fn restore(request: &Request<'_>) -> Option<String> {
    let options = request.options;
    let prior = request.prior;
    let word = &prior.word;
    let applies = options.enabled
        && options.spell_check
        && options.auto_restore_invalid
        && prior.tracking == Tracking::Vietnamese
        && word.transformed()
        && !word.is_complete_syllable();
    applies.then(|| word.raw_text())
}

/// Replaces the owned word with `text` and the delimiter (or replays Enter/Tab).
/// `None` when the result would exceed [`MAX_INSERT_UNITS`].
///
/// The replacement is engine output, so it follows the configured normalization in every
/// mode; only the literally tracked word of disabled mode is rendered as typed.
fn emit(request: &Request<'_>, text: &str) -> Result<Option<EditTransaction>, EngineError> {
    let event = request.event;
    let mut insert = utf16::encode(&normalize(text, request.options.normalization));
    let replay_key = replay_key(event);
    if replay_key.is_none() {
        insert.extend_from_slice(&event.text_utf16);
    }
    if insert.len() > MAX_INSERT_UNITS {
        return Ok(None);
    }
    Ok(Some(EditTransaction {
        delete_before: units(request.prior.owned.len())?,
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
