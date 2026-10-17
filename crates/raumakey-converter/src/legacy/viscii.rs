//! VISCII (RFC 1456): 134 Vietnamese letters in 0x80..=0xFF and six C0 positions (0x02, 0x05,
//! 0x06, 0x14, 0x19, 0x1E); every other byte is ASCII.

use std::sync::OnceLock;

use super::Sink;
use crate::codepage::cp1252_to_char;
use crate::letters::{Letter, lookup};

/// Lowercase letters, `[vowel][tone]` (tones: level, grave, hook, tilde, acute, dot).
const LOWER: [[u8; 6]; 12] = [
    [b'a', 0xE0, 0xE4, 0xE3, 0xE1, 0xD5], // a à ả ã á ạ
    [0xE5, 0xA2, 0xC6, 0xC7, 0xA1, 0xA3], // ă ằ ẳ ẵ ắ ặ
    [0xE2, 0xA5, 0xA6, 0xE7, 0xA4, 0xA7], // â ầ ẩ ẫ ấ ậ
    [b'e', 0xE8, 0xEB, 0xA8, 0xE9, 0xA9], // e è ẻ ẽ é ẹ
    [0xEA, 0xAB, 0xAC, 0xAD, 0xAA, 0xAE], // ê ề ể ễ ế ệ
    [b'i', 0xEC, 0xEF, 0xEE, 0xED, 0xB8], // i ì ỉ ĩ í ị
    [b'o', 0xF2, 0xF6, 0xF5, 0xF3, 0xF7], // o ò ỏ õ ó ọ
    [0xF4, 0xB0, 0xB1, 0xB2, 0xAF, 0xB5], // ô ồ ổ ỗ ố ộ
    [0xBD, 0xB6, 0xB7, 0xDE, 0xBE, 0xFE], // ơ ờ ở ỡ ớ ợ
    [b'u', 0xF9, 0xFC, 0xFB, 0xFA, 0xF8], // u ù ủ ũ ú ụ
    [0xDF, 0xD7, 0xD8, 0xE6, 0xD1, 0xF1], // ư ừ ử ữ ứ ự
    [b'y', 0xCF, 0xD6, 0xDB, 0xFD, 0xDC], // y ỳ ỷ ỹ ý ỵ
];

/// Uppercase letters, `[vowel][tone]`.
const UPPER: [[u8; 6]; 12] = [
    [b'A', 0xC0, 0xC4, 0xC3, 0xC1, 0x80], // A À Ả Ã Á Ạ
    [0xC5, 0x82, 0x02, 0x05, 0x81, 0x83], // Ă Ằ Ẳ Ẵ Ắ Ặ
    [0xC2, 0x85, 0x86, 0x06, 0x84, 0x87], // Â Ầ Ẩ Ẫ Ấ Ậ
    [b'E', 0xC8, 0xCB, 0x88, 0xC9, 0x89], // E È Ẻ Ẽ É Ẹ
    [0xCA, 0x8B, 0x8C, 0x8D, 0x8A, 0x8E], // Ê Ề Ể Ễ Ế Ệ
    [b'I', 0xCC, 0x9B, 0xCE, 0xCD, 0x98], // I Ì Ỉ Ĩ Í Ị
    [b'O', 0xD2, 0x99, 0xA0, 0xD3, 0x9A], // O Ò Ỏ Õ Ó Ọ
    [0xD4, 0x90, 0x91, 0x92, 0x8F, 0x93], // Ô Ồ Ổ Ỗ Ố Ộ
    [0xB4, 0x96, 0x97, 0xB3, 0x95, 0x94], // Ơ Ờ Ở Ỡ Ớ Ợ
    [b'U', 0xD9, 0x9C, 0x9D, 0xDA, 0x9E], // U Ù Ủ Ũ Ú Ụ
    [0xBF, 0xBB, 0xBC, 0xFF, 0xBA, 0xB9], // Ư Ừ Ử Ữ Ứ Ự
    [b'Y', 0x9F, 0x14, 0x19, 0xDD, 0x1E], // Y Ỳ Ỷ Ỹ Ý Ỵ
];

const D_LOWER: u8 = 0xF0;
const D_UPPER: u8 = 0xD0;

fn code(letter: Letter) -> u8 {
    match letter {
        Letter::Vowel { vowel, tone, upper } => {
            let table = if upper { &UPPER } else { &LOWER };
            // The indices come from the enums, so the `?` fallback is never written (tested).
            lookup(table, vowel, tone).unwrap_or(b'?')
        }
        Letter::DStroke { upper } => {
            if upper {
                D_UPPER
            } else {
                D_LOWER
            }
        }
    }
}

fn table() -> &'static [Option<char>; 256] {
    static TABLE: OnceLock<[Option<char>; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [None; 256];
        for letter in Letter::all() {
            let byte = code(letter);
            // ASCII letters keep their ASCII code and need no entry.
            if !letter.to_char().is_ascii()
                && let Some(slot) = table.get_mut(usize::from(byte))
            {
                *slot = Some(letter.to_char());
            }
        }
        table
    })
}

fn entry(byte: u8) -> Option<char> {
    table().get(usize::from(byte)).copied().flatten()
}

pub(super) fn decode_byte(byte: u8) -> char {
    entry(byte).unwrap_or_else(|| cp1252_to_char(byte))
}

pub(super) fn encode_letter(c: char, sink: &mut impl Sink) -> Option<bool> {
    sink.byte(code(Letter::from_char(c)?));
    Some(true)
}

pub(super) fn is_free(byte: u8) -> bool {
    entry(byte).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn viscii_assigns_every_high_byte_and_six_c0_bytes() {
        let assigned: Vec<u8> = (0..=255).filter(|&b| entry(b).is_some()).collect();
        assert_eq!(assigned.len(), 134);
        let low: Vec<u8> = assigned.iter().copied().filter(|&b| b < 0x80).collect();
        assert_eq!(low, [0x02, 0x05, 0x06, 0x14, 0x19, 0x1E]);
    }

    #[test]
    fn every_letter_has_a_code_and_the_fallback_is_never_used() {
        for letter in Letter::all() {
            assert_ne!(code(letter), b'?', "{letter:?}");
        }
    }
}
