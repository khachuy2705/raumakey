//! Windows-1258.
//!
//! Encoding uses a precomposed character when the code page has one; otherwise the letter
//! without its tone followed by one of the five combining tone marks (0xCC grave, 0xD2 hook,
//! 0xDE tilde, 0xEC acute, 0xF2 dot below). Other characters are split by canonical
//! decomposition when the code page has the base and every mark. Decoding maps bytes to
//! characters; the caller composes the result to NFC.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::decompose_canonical;

use super::Sink;
use crate::codepage::{cp1252_to_char, cp1258_to_byte, cp1258_to_char};
use crate::letters::{Letter, Tone};

const fn tone_byte(tone: Tone) -> Option<u8> {
    match tone {
        Tone::Level => None,
        Tone::Grave => Some(0xCC),
        Tone::Hook => Some(0xD2),
        Tone::Tilde => Some(0xDE),
        Tone::Acute => Some(0xEC),
        Tone::Dot => Some(0xF2),
    }
}

pub(super) fn decode_byte(byte: u8) -> char {
    cp1258_to_char(byte)
}

pub(super) fn encode_char(c: char, sink: &mut impl Sink) -> Option<bool> {
    if let Some(byte) = cp1258_to_byte(c) {
        sink.byte(byte);
        return Some(true);
    }
    if let Some(Letter::Vowel { vowel, tone, upper }) = Letter::from_char(c) {
        let untoned = Letter::Vowel {
            vowel,
            tone: Tone::Level,
            upper,
        };
        if let (Some(base), Some(mark)) = (cp1258_to_byte(untoned.to_char()), tone_byte(tone)) {
            sink.byte(base);
            sink.byte(mark);
            return Some(true);
        }
    }
    let bytes = decomposed(c)?;
    for byte in bytes {
        sink.byte(byte);
    }
    Some(true)
}

/// Splits `c` into a precomposed head the code page has plus combining marks it has.
fn decomposed(c: char) -> Option<Vec<u8>> {
    let mut parts = Vec::new();
    decompose_canonical(c, |part| parts.push(part));
    for split in (1..parts.len()).rev() {
        let Some((head, marks)) = parts.split_at_checked(split) else {
            continue;
        };
        let head: String = head.iter().collect::<String>().nfc().collect();
        let mut chars = head.chars();
        let (Some(head_char), None) = (chars.next(), chars.next()) else {
            continue;
        };
        let Some(head_byte) = cp1258_to_byte(head_char) else {
            continue;
        };
        let mark_bytes: Option<Vec<u8>> = marks.iter().map(|&mark| cp1258_to_byte(mark)).collect();
        let Some(mark_bytes) = mark_bytes else {
            continue;
        };
        let recomposed: String = core::iter::once(head_char)
            .chain(marks.iter().copied())
            .collect::<String>()
            .nfc()
            .collect();
        let mut recomposed = recomposed.chars();
        if recomposed.next() == Some(c) && recomposed.next().is_none() {
            let mut bytes = vec![head_byte];
            bytes.extend(mark_bytes);
            return Some(bytes);
        }
    }
    None
}

pub(super) fn is_free(byte: u8) -> bool {
    cp1258_to_char(byte) == cp1252_to_char(byte)
}
