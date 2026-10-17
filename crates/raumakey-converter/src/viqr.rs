//! VIQR (RFC 1456).
//!
//! A Vietnamese letter is written as its ASCII base letter, then the vowel mark (`(` breve,
//! `^` circumflex, `+` horn), then the tone (`` ` `` grave, `?` hook, `~` tilde, `'` acute,
//! `.` dot below); đ/Đ is `dd`/`DD`. A backslash makes the next mark character, `d`/`D`, or
//! backslash literal.
//!
//! Decoding rules (the encoder escapes exactly where these would misread a literal):
//! - a vowel mark applies to the untoned plain vowel just written, if Vietnamese has that
//!   letter (`a(`, `a^`, `e^`, `o^`, `o+`, `u+`);
//! - a tone applies to the untoned vowel just written (precomposed ă â ê ô ơ ư included);
//! - `d` or `D` right after a plain `d`/`D` makes đ/Đ in the case of the first one;
//! - escaped characters never combine with what follows.

use crate::letters::{Letter, Shape, Tone, Vowel};

const fn shape_of(c: char) -> Option<Shape> {
    match c {
        '(' => Some(Shape::Breve),
        '^' => Some(Shape::Circumflex),
        '+' => Some(Shape::Horn),
        _ => None,
    }
}

const fn shape_char(shape: Shape) -> char {
    match shape {
        Shape::Breve => '(',
        Shape::Circumflex => '^',
        Shape::Horn => '+',
    }
}

const fn tone_of(c: char) -> Option<Tone> {
    match c {
        '`' => Some(Tone::Grave),
        '?' => Some(Tone::Hook),
        '~' => Some(Tone::Tilde),
        '\'' => Some(Tone::Acute),
        '.' => Some(Tone::Dot),
        _ => None,
    }
}

const fn tone_char(tone: Tone) -> Option<char> {
    match tone {
        Tone::Level => None,
        Tone::Grave => Some('`'),
        Tone::Hook => Some('?'),
        Tone::Tilde => Some('~'),
        Tone::Acute => Some('\''),
        Tone::Dot => Some('.'),
    }
}

/// Characters a backslash makes literal.
const fn is_escapable(c: char) -> bool {
    shape_of(c).is_some() || tone_of(c).is_some() || matches!(c, 'd' | 'D' | '\\')
}

/// What the decoder remembers about the last character it wrote.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    /// Nothing combines with the next character.
    Idle,
    /// An untoned vowel that may still take a tone (and a vowel mark when `can_shape`).
    Vowel {
        vowel: Vowel,
        upper: bool,
        can_shape: bool,
    },
    /// A plain d/D that a following d/D turns into đ/Đ.
    PlainD { upper: bool },
}

impl State {
    /// The state after writing `c` literally.
    fn after_literal(c: char) -> Self {
        match Letter::from_char(c) {
            Some(Letter::Vowel {
                vowel,
                tone: Tone::Level,
                upper,
            }) => Self::Vowel {
                vowel,
                upper,
                can_shape: vowel.split_shape().is_none(),
            },
            _ => match c {
                'd' => Self::PlainD { upper: false },
                'D' => Self::PlainD { upper: true },
                _ => Self::Idle,
            },
        }
    }

    /// If the decoder reads `c` as a mark on the last character, the replacement character
    /// and the next state.
    fn apply(self, c: char) -> Option<(char, Self)> {
        match self {
            Self::Vowel {
                vowel,
                upper,
                can_shape,
            } => {
                if can_shape
                    && let Some(shape) = shape_of(c)
                    && let Some(shaped) = vowel.with_shape(shape)
                {
                    let letter = Letter::Vowel {
                        vowel: shaped,
                        tone: Tone::Level,
                        upper,
                    };
                    let next = Self::Vowel {
                        vowel: shaped,
                        upper,
                        can_shape: false,
                    };
                    return Some((letter.to_char(), next));
                }
                let tone = tone_of(c)?;
                Some((Letter::Vowel { vowel, tone, upper }.to_char(), Self::Idle))
            }
            Self::PlainD { upper } if matches!(c, 'd' | 'D') => {
                Some((Letter::DStroke { upper }.to_char(), Self::Idle))
            }
            Self::PlainD { .. } | Self::Idle => None,
        }
    }
}

/// Decodes VIQR text (not yet normalized).
pub(crate) fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut state = State::Idle;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&next) if is_escapable(next) => {
                    chars.next();
                    out.push(next);
                }
                _ => out.push('\\'),
            }
            state = State::Idle;
            continue;
        }
        if let Some((replacement, next)) = state.apply(c) {
            // The state guarantees the last character written is the one being marked.
            out.pop();
            out.push(replacement);
            state = next;
            continue;
        }
        out.push(c);
        state = State::after_literal(c);
    }
    out
}

/// Encodes NFC text as VIQR.
pub(crate) fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len().saturating_add(text.len() / 2));
    let mut state = State::Idle;
    // The last character written is a literal backslash that was not escaped.
    let mut pending_backslash = false;
    for c in text.chars() {
        match Letter::from_char(c) {
            Some(Letter::Vowel { vowel, tone, upper }) => {
                let (plain, shape) = vowel
                    .split_shape()
                    .map_or((vowel, None), |(plain, shape)| (plain, Some(shape)));
                let base = char::from(plain.ascii_base());
                out.push(if upper {
                    base.to_ascii_uppercase()
                } else {
                    base
                });
                if let Some(shape) = shape {
                    out.push(shape_char(shape));
                }
                if let Some(tone) = tone_char(tone) {
                    out.push(tone);
                    state = State::Idle;
                } else {
                    state = State::Vowel {
                        vowel,
                        upper,
                        can_shape: shape.is_none(),
                    };
                }
                pending_backslash = false;
            }
            Some(Letter::DStroke { upper }) => {
                if pending_backslash || matches!(state, State::PlainD { .. }) {
                    escape_last(&mut out);
                }
                out.push_str(if upper { "DD" } else { "dd" });
                state = State::Idle;
                pending_backslash = false;
            }
            None if state.apply(c).is_some() => {
                out.push('\\');
                out.push(c);
                state = State::Idle;
                pending_backslash = false;
            }
            None => {
                if pending_backslash && is_escapable(c) {
                    escape_last(&mut out);
                }
                out.push(c);
                state = State::after_literal(c);
                pending_backslash = c == '\\';
            }
        }
    }
    out
}

/// Puts a backslash before the last character written.
fn escape_last(out: &mut String) {
    if let Some(last) = out.pop() {
        out.push('\\');
        out.push(last);
    }
}
