//! Platform-independent key scripts for tests, the `type` example, and the acceptance runner.
//!
//! A script is plain text: every character is typed as itself, and `{NAME}` tokens stand for
//! special keys: `{BS}`, `{SP}`/`{SPACE}`, `{SHIFT+SPACE}`, `{ENTER}`, `{TAB}`, `{NAV}`,
//! `{MOUSE}`, `{FOCUS}`, `{CTRL+x}`, `{ALTGR+x}`, `{SECURE}` (the following keys are typed in a
//! secure field), `{/SECURE}`, `{LBRACE}`, and `{RBRACE}`. Token names ignore ASCII case.
//!
//! The `x` of a chord is one character or a key name: `SPACE`, `ENTER`, `TAB`, `LBRACE`,
//! `RBRACE`. `{CTRL+SPACE}`, `{CTRL+ENTER}`, and `{CTRL+TAB}` are the separator events of
//! `{SPACE}`, `{ENTER}`, and `{TAB}` with Ctrl held. Every token prints in a form that parses
//! back to the same token.
//!
//! [`run_script`] feeds the events through the engine, applies every transaction to
//! [`TextSimulator`], and commits the next state only when the simulator accepted the
//! transaction, exactly as an adapter must.

use core::fmt;

use raumakey_model::error::{EngineError, ResetReason};
use raumakey_model::host::{ExeName, ExeNameError, HostContext, SessionId};
use raumakey_model::input::{InputEvent, InputKind, vk};
use raumakey_model::options::EngineOptions;
use raumakey_model::transaction::{EditTransaction, TextSimulator};

use crate::{EngineState, VietnameseEngine};

/// Executable name of the host used by [`run_script`].
pub const SCRIPT_EXE: &str = "raumakey-script.exe";

/// One scripted key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum KeyToken {
    /// A typed character.
    Char(char),
    /// `{BS}`
    Backspace,
    /// `{SP}` / `{SPACE}`
    Space,
    /// `{SHIFT+SPACE}`
    ShiftSpace,
    /// `{ENTER}`
    Enter,
    /// `{TAB}`
    Tab,
    /// `{NAV}`: a caret movement key.
    Navigation,
    /// `{MOUSE}`: a click in the document.
    Mouse,
    /// `{FOCUS}`: focus change.
    Focus,
    /// `{CTRL+x}`: a Ctrl shortcut.
    Ctrl(char),
    /// `{ALTGR+x}`: a character produced with AltGr.
    AltGr(char),
    /// `{SECURE}`: focus moves into a secure field.
    SecureOn,
    /// `{/SECURE}`: focus leaves the secure field.
    SecureOff,
}

impl fmt::Display for KeyToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Char('{') => f.write_str("{LBRACE}"),
            Self::Char('}') => f.write_str("{RBRACE}"),
            Self::Char(c) => write!(f, "{c}"),
            Self::Backspace => f.write_str("{BS}"),
            Self::Space => f.write_str("{SPACE}"),
            Self::ShiftSpace => f.write_str("{SHIFT+SPACE}"),
            Self::Enter => f.write_str("{ENTER}"),
            Self::Tab => f.write_str("{TAB}"),
            Self::Navigation => f.write_str("{NAV}"),
            Self::Mouse => f.write_str("{MOUSE}"),
            Self::Focus => f.write_str("{FOCUS}"),
            Self::Ctrl(c) => write_chord(f, "CTRL", *c),
            Self::AltGr(c) => write_chord(f, "ALTGR", *c),
            Self::SecureOn => f.write_str("{SECURE}"),
            Self::SecureOff => f.write_str("{/SECURE}"),
        }
    }
}

/// Chord keys written by name: braces would end or open a token, and whitespace is invisible.
const CHORD_KEY_NAMES: [(char, &str); 5] = [
    ('{', "LBRACE"),
    ('}', "RBRACE"),
    (' ', "SPACE"),
    ('\r', "ENTER"),
    ('\t', "TAB"),
];

fn write_chord(f: &mut fmt::Formatter<'_>, modifier: &str, key: char) -> fmt::Result {
    match CHORD_KEY_NAMES.iter().find(|(named, _)| *named == key) {
        Some((_, name)) => write!(f, "{{{modifier}+{name}}}"),
        None => write!(f, "{{{modifier}+{key}}}"),
    }
}

/// The key a chord names: one character, or a key name in any ASCII case.
fn chord_key(text: &str) -> Option<char> {
    let mut chars = text.chars();
    if let (Some(key), None) = (chars.next(), chars.next()) {
        return Some(key);
    }
    let key = match text.to_ascii_uppercase().as_str() {
        "SP" => ' ',
        "RETURN" => '\r',
        name => CHORD_KEY_NAMES
            .iter()
            .find(|(_, known)| *known == name)
            .map(|(key, _)| *key)?,
    };
    Some(key)
}

/// Why a script could not be parsed. Offsets are byte offsets of the offending `{`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScriptError {
    /// `{` without a closing `}`.
    Unterminated {
        /// Byte offset.
        offset: usize,
    },
    /// Unknown `{NAME}`.
    UnknownToken {
        /// Byte offset.
        offset: usize,
    },
    /// `{CTRL+x}` / `{ALTGR+x}` must name exactly one character or a known key name.
    InvalidChord {
        /// Byte offset.
        offset: usize,
    },
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unterminated { offset } => write!(f, "unterminated token at byte {offset}"),
            Self::UnknownToken { offset } => write!(f, "unknown token at byte {offset}"),
            Self::InvalidChord { offset } => write!(
                f,
                "chord at byte {offset} must name one character or SPACE, ENTER, TAB, LBRACE, RBRACE"
            ),
        }
    }
}

impl std::error::Error for ScriptError {}

/// Parses a key script.
pub fn parse_keys(script: &str) -> Result<Vec<KeyToken>, ScriptError> {
    let mut tokens = Vec::new();
    let mut rest = script.char_indices();
    while let Some((offset, c)) = rest.next() {
        if c != '{' {
            tokens.push(KeyToken::Char(c));
            continue;
        }
        let body_start = offset + 1;
        let Some(length) = script.get(body_start..).and_then(|tail| tail.find('}')) else {
            return Err(ScriptError::Unterminated { offset });
        };
        let body = script
            .get(body_start..body_start + length)
            .ok_or(ScriptError::Unterminated { offset })?;
        tokens.push(parse_token(body, offset)?);
        for _ in 0..=body.chars().count() {
            rest.next();
        }
    }
    Ok(tokens)
}

fn parse_token(body: &str, offset: usize) -> Result<KeyToken, ScriptError> {
    let upper = body.to_ascii_uppercase();
    let token = match upper.as_str() {
        "BS" | "BACKSPACE" => KeyToken::Backspace,
        "SP" | "SPACE" => KeyToken::Space,
        "SHIFT+SPACE" => KeyToken::ShiftSpace,
        "ENTER" | "RETURN" => KeyToken::Enter,
        "TAB" => KeyToken::Tab,
        "NAV" => KeyToken::Navigation,
        "MOUSE" => KeyToken::Mouse,
        "FOCUS" => KeyToken::Focus,
        "SECURE" => KeyToken::SecureOn,
        "/SECURE" => KeyToken::SecureOff,
        "LBRACE" => KeyToken::Char('{'),
        "RBRACE" => KeyToken::Char('}'),
        _ => {
            if let Some(key) = chord(body, "CTRL+", offset)? {
                KeyToken::Ctrl(key)
            } else if let Some(key) = chord(body, "ALTGR+", offset)? {
                KeyToken::AltGr(key)
            } else {
                return Err(ScriptError::UnknownToken { offset });
            }
        }
    };
    Ok(token)
}

/// `Some(key)` when `body` is `<prefix><key>` (prefix in any ASCII case).
fn chord(body: &str, prefix: &str, offset: usize) -> Result<Option<char>, ScriptError> {
    let Some(head) = body.get(..prefix.len()) else {
        return Ok(None);
    };
    if !head.eq_ignore_ascii_case(prefix) {
        return Ok(None);
    }
    chord_key(body.get(prefix.len()..).unwrap_or_default())
        .map(Some)
        .ok_or(ScriptError::InvalidChord { offset })
}

/// The host used by [`run_script`].
pub fn script_host() -> Result<HostContext, ExeNameError> {
    Ok(HostContext::new(
        SessionId::from_u128(1),
        ExeName::new(SCRIPT_EXE)?,
    ))
}

/// The input event a token produces in `host`.
///
/// ASCII uppercase characters carry Shift; Space/Enter/Tab are `Separator` events with their
/// virtual keys; `{CTRL+SPACE}`/`{CTRL+ENTER}`/`{CTRL+TAB}` are those separator events with
/// Ctrl held; any other `{CTRL+x}` is a `Character` event with Ctrl, no text, and the letter's
/// virtual key; `{ALTGR+x}` types `x` with Ctrl+Alt+AltGr; `{SECURE}`/`{/SECURE}` are focus
/// changes into and out of a secure field.
pub fn event_for(token: &KeyToken, host: &HostContext, secure: bool) -> InputEvent {
    let mut host = host.clone();
    host.is_secure_field = match token {
        KeyToken::SecureOn => true,
        KeyToken::SecureOff => false,
        _ => secure,
    };
    match *token {
        KeyToken::Char(c) => {
            let mut event = InputEvent::character(c, host);
            event.modifiers.shift = c.is_ascii_uppercase();
            event
        }
        KeyToken::Backspace => InputEvent::backspace(host),
        KeyToken::Space => InputEvent::separator(' ', vk::SPACE, host),
        KeyToken::ShiftSpace => {
            let mut event = InputEvent::separator(' ', vk::SPACE, host);
            event.modifiers.shift = true;
            event
        }
        KeyToken::Enter => InputEvent::separator('\r', vk::RETURN, host),
        KeyToken::Tab => InputEvent::separator('\t', vk::TAB, host),
        KeyToken::Navigation => InputEvent::navigation(vk::LEFT, host),
        KeyToken::Mouse => InputEvent::simple(InputKind::MouseReset, host),
        KeyToken::Focus | KeyToken::SecureOn | KeyToken::SecureOff => {
            InputEvent::simple(InputKind::FocusReset, host)
        }
        KeyToken::Ctrl(separator @ (' ' | '\r' | '\t')) => {
            let virtual_key = match separator {
                ' ' => vk::SPACE,
                '\r' => vk::RETURN,
                _ => vk::TAB,
            };
            let mut event = InputEvent::separator(separator, virtual_key, host);
            event.modifiers.ctrl = true;
            event
        }
        KeyToken::Ctrl(c) => {
            // A shortcut types no text; the key is identified by its virtual key.
            let mut event = InputEvent::text(Vec::new(), host);
            event.virtual_key = c
                .is_ascii_alphanumeric()
                .then(|| u32::from(c.to_ascii_uppercase()));
            event.modifiers.ctrl = true;
            event.modifiers.shift = c.is_ascii_uppercase();
            event
        }
        KeyToken::AltGr(c) => {
            let mut event = InputEvent::character(c, host);
            event.modifiers.ctrl = true;
            event.modifiers.alt = true;
            event.modifiers.alt_gr = true;
            event
        }
    }
}

/// Formats a transaction in matrix notation: `Tx(delete,"insert",consume,replay,reset)`.
pub fn format_transaction(tx: &EditTransaction) -> String {
    let replay = match tx.replay_key {
        None => "None".to_owned(),
        Some(key) => format!("Some({})", key_name(key.virtual_key)),
    };
    format!(
        "Tx({},{},{},{},{})",
        tx.delete_before,
        quote(&tx.insert_string()),
        tx.consume_original,
        replay,
        tx.reset_context
    )
}

/// `text` in double quotes; `"`, `\`, and control characters are escaped, everything else
/// (including combining marks) is kept as is.
pub fn quote(text: &str) -> String {
    format!("\"{}\"", escape(text))
}

fn key_name(key: u32) -> String {
    match key {
        vk::RETURN => "RETURN".to_owned(),
        vk::TAB => "TAB".to_owned(),
        vk::SPACE => "SPACE".to_owned(),
        vk::BACK => "BACK".to_owned(),
        other => format!("0x{other:02X}"),
    }
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\r' => out.push_str("\\r"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:X}}}", u32::from(c))),
            c => out.push(c),
        }
    }
    out
}

/// One processed token.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptStep {
    /// The token.
    pub token: KeyToken,
    /// The event fed to the engine.
    pub event: InputEvent,
    /// The engine's transaction.
    pub transaction: EditTransaction,
    /// Whether the simulator accepted the transaction (the next state was committed).
    pub accepted: bool,
}

/// Result of a script run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptOutcome {
    /// Simulated document text after the last step.
    pub final_text: String,
    /// Every step in order.
    pub steps: Vec<ScriptStep>,
    /// The committed engine state after the last step.
    pub final_state: EngineState,
}

/// The engine rejected an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScriptRunError {
    /// Zero-based index of the failing token.
    pub step: usize,
    /// Engine error.
    pub error: EngineError,
}

impl fmt::Display for ScriptRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "step {}: {}", self.step, self.error)
    }
}

impl std::error::Error for ScriptRunError {}

/// Runs `tokens` in the [`script_host`] starting from an empty document.
pub fn run_script(
    engine: &VietnameseEngine,
    options: &EngineOptions,
    tokens: &[KeyToken],
) -> Result<ScriptOutcome, ScriptRunError> {
    let host = script_host().map_err(|_| ScriptRunError {
        step: 0,
        error: EngineError::InternalInvariant,
    })?;
    run_script_with(engine, options, tokens, &host, "")
}

/// Runs `tokens` in `host` starting from a document that holds `initial_text`.
///
/// When the simulator rejects a transaction the state is not committed: the engine is reset
/// ([`ResetReason::ApplyFailed`]) and the key passes through, as the adapter contract requires.
pub fn run_script_with(
    engine: &VietnameseEngine,
    options: &EngineOptions,
    tokens: &[KeyToken],
    host: &HostContext,
    initial_text: &str,
) -> Result<ScriptOutcome, ScriptRunError> {
    run_script_from(
        engine,
        options,
        tokens,
        host,
        initial_text,
        EngineState::new(),
    )
}

/// Runs `tokens` in `host` from `initial_state` against a document that holds `initial_text`.
///
/// The state may be stale (built against other text, as when the host edited the document
/// behind the engine): a transaction the simulator rejects is not committed; the engine is
/// reset ([`ResetReason::ApplyFailed`]) and the key passes through, as the adapter contract
/// requires. [`ScriptStep::accepted`] records the simulator's verdict.
pub fn run_script_from(
    engine: &VietnameseEngine,
    options: &EngineOptions,
    tokens: &[KeyToken],
    host: &HostContext,
    initial_text: &str,
    initial_state: EngineState,
) -> Result<ScriptOutcome, ScriptRunError> {
    let mut simulator = TextSimulator::with_text(initial_text);
    let mut state = initial_state;
    let mut secure = false;
    let mut steps = Vec::with_capacity(tokens.len());
    for (step, token) in tokens.iter().enumerate() {
        match token {
            KeyToken::SecureOn => secure = true,
            KeyToken::SecureOff => secure = false,
            _ => {}
        }
        let event = event_for(token, host, secure);
        let result = engine
            .process(&event, options, &state)
            .map_err(|error| ScriptRunError { step, error })?;
        let accepted = simulator.apply(&event, &result.transaction).is_ok();
        if accepted {
            state = result.next;
        } else {
            state = engine.reset(&state, ResetReason::ApplyFailed);
            // The key passes through; a pass-through edit always applies.
            let _ = simulator.apply(&event, &EditTransaction::pass_through_reset());
        }
        steps.push(ScriptStep {
            token: *token,
            event,
            transaction: result.transaction,
            accepted,
        });
    }
    Ok(ScriptOutcome {
        final_text: simulator.as_string(),
        steps,
        final_state: state,
    })
}
