//! Tokens: the text typed since the previous whitespace (plan D28, §3.8).
//!
//! A macro trigger that contains boundary punctuation (`v.v`) spans several words, because
//! the punctuation commits the word before it. While token triggers take part
//! ([`Request::tokens_active`](crate::process::Request::tokens_active)), the engine keeps the
//! words and punctuation committed since the previous whitespace as long as that text starts
//! a trigger, and owns their rendered text in front of the current word. A whitespace
//! boundary then looks the whole token up and can replace it. Anything else ends the token:
//! a reset event, a replacement at punctuation, an overflowing word, or text that no trigger
//! starts with. Until the next whitespace or reset no token starts again, so a trigger never
//! matches the end of a longer token (matrix #48).
//!
//! Backspace inside the current word keeps the token. Once the current word is empty, a
//! Backspace deletes the punctuation that ended the committed part and returns to the state
//! before it ([`Committed::before`]): the word before the punctuation, with its history, and
//! the token around it.
//!
//! Limitation: punctuation that ended a token which could no longer match (the token is
//! [`Token::Lost`]) was never owned, so a Backspace over it lets the application delete it and
//! restores nothing. The engine stays idle with the token lost until the next whitespace or
//! reset: after `v,` Backspace `.v` Space the text is `v.v `, unexpanded, and nothing is
//! edited.

use crate::EngineState;

/// Where the current word stands in its token.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Token {
    /// The current or next word starts a token: after whitespace, after a reset, or at the
    /// start. Also the only value while token triggers do not take part.
    #[default]
    Start,
    /// The token began before the current word and can no longer match: no token starts
    /// before the next whitespace or reset.
    Lost,
    /// The token began before the current word and can still match.
    Tracked(Committed),
}

/// The committed part of a tracked token: its words and the punctuation that committed them.
///
/// `Debug` shows typed text, like the rest of the engine state; nothing logs it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Committed {
    /// NFC rendering, as typed.
    pub(crate) rendered: String,
    /// Raw keys (NFC).
    pub(crate) raw: String,
    /// UTF-16 units at the start of the state's owned text that hold this part.
    pub(crate) units: usize,
    /// The state just before the punctuation that ends this part. Its owned text is this
    /// part without that punctuation, so a Backspace over the punctuation returns to it. The
    /// chain is as deep as the token has punctuation, which the trigger length bounds.
    pub(crate) before: Box<EngineState>,
}

impl Token {
    /// Units of the owned text that belong to the committed part (0 unless tracked).
    pub(crate) fn units(&self) -> usize {
        match self {
            Self::Tracked(committed) => committed.units,
            Self::Start | Self::Lost => 0,
        }
    }

    /// True for a tracked token.
    pub(crate) fn is_tracked(&self) -> bool {
        matches!(self, Self::Tracked(_))
    }
}
