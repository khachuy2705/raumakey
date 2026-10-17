//! Case and tone transforms applied between decoding and encoding.

use core::fmt;
use core::str::FromStr;

use unicode_normalization::char::is_combining_mark;

use crate::codec::nfc;
use crate::letters::Letter;

/// Letter case applied to the decoded text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum CaseTransform {
    /// Keep the case of the source.
    #[default]
    None,
    /// Every letter uppercase (`TIẾNG VIỆT`).
    Upper,
    /// Every letter lowercase (`tiếng việt`).
    Lower,
    /// First letter of every word uppercase, the rest lowercase (`Tiếng Việt`). A word is a run
    /// of letters, digits, and combining marks; an apostrophe between two letters stays inside
    /// the word.
    Title,
}

impl CaseTransform {
    /// Every transform, in declaration order.
    pub const ALL: [Self; 4] = [Self::None, Self::Upper, Self::Lower, Self::Title];

    /// Stable lowercase name (`none`, `upper`, `lower`, `title`).
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Upper => "upper",
            Self::Lower => "lower",
            Self::Title => "title",
        }
    }
}

impl fmt::Display for CaseTransform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Rejected case transform name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseCaseTransformError;

impl fmt::Display for ParseCaseTransformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown case transform name")
    }
}

impl std::error::Error for ParseCaseTransformError {}

impl FromStr for CaseTransform {
    type Err = ParseCaseTransformError;

    /// Parses a stable name, ignoring ASCII case and surrounding whitespace.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let wanted = text.trim();
        Self::ALL
            .into_iter()
            .find(|case| case.name().eq_ignore_ascii_case(wanted))
            .ok_or(ParseCaseTransformError)
    }
}

/// Transforms applied to the decoded Unicode text before it is encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ConvertOptions {
    /// Letter case of the result.
    pub case: CaseTransform,
    /// Remove every Vietnamese diacritic: tone marks and the vowel marks of ă â ê ô ơ ư, and đ/Đ
    /// become d/D ("không dấu" text such as `Tieng Viet`). Other characters are unchanged.
    pub remove_tones: bool,
}

impl ConvertOptions {
    /// Applies the options to NFC text and returns NFC text.
    pub(crate) fn apply(self, text: String) -> String {
        let text = if self.remove_tones {
            nfc(&remove_tones(&text))
        } else {
            text
        };
        match self.case {
            CaseTransform::None => text,
            CaseTransform::Upper => nfc(&text.to_uppercase()),
            CaseTransform::Lower => nfc(&text.to_lowercase()),
            CaseTransform::Title => nfc(&title_case(&text)),
        }
    }
}

/// Combining marks Vietnamese uses: the five tones, breve, circumflex, and horn.
const fn is_vietnamese_mark(c: char) -> bool {
    matches!(
        c,
        '\u{300}'
            | '\u{301}'
            | '\u{302}'
            | '\u{303}'
            | '\u{306}'
            | '\u{309}'
            | '\u{31B}'
            | '\u{323}'
    )
}

/// Replaces Vietnamese letters by their ASCII letter and drops the Vietnamese combining marks in
/// the combining sequence that follows them (NFC keeps such a mark separate when no precomposed
/// letter exists, and it would otherwise recombine with the bare letter).
fn remove_tones(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut after_letter = false;
    for c in text.chars() {
        if after_letter && is_combining_mark(c) {
            if !is_vietnamese_mark(c) {
                out.push(c);
            }
            continue;
        }
        match Letter::from_char(c) {
            Some(letter) => {
                out.push(letter.without_marks());
                after_letter = true;
            }
            None => {
                out.push(c);
                after_letter = false;
            }
        }
    }
    out
}

const fn is_apostrophe(c: char) -> bool {
    matches!(c, '\'' | '\u{2019}')
}

fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_word = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_alphanumeric() || (in_word && is_combining_mark(c)) {
            if in_word {
                out.extend(c.to_lowercase());
            } else {
                out.extend(c.to_uppercase());
                in_word = true;
            }
        } else if in_word
            && is_apostrophe(c)
            && chars.peek().is_some_and(|next| next.is_alphabetic())
        {
            out.push(c);
        } else {
            out.push(c);
            in_word = false;
        }
    }
    out
}
