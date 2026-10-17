//! TCVN 5712:1993 VN3 ("ABC"), the variant used with `.Vn*` fonts.
//!
//! VN3 has the lowercase letters with every tone, the untoned uppercase letters Ă Â Ê Ô Ơ Ư Đ,
//! and no uppercase letter with a tone mark (those were typed with a separate uppercase font
//! that draws capitals at the lowercase codes). Encoding an uppercase toned letter therefore
//! writes the lowercase code and reports a loss. The lowercase and untoned uppercase codes are
//! the same as in VN1 (`TCVN5712-1:1993`); the positions VN1 additionally uses (uppercase toned
//! letters in 0x01..=0x17, 0x80..=0x9F, 0xAF, 0xBA, 0xBF..=0xC5, 0xCD, 0xD9..=0xDB, 0xE0, 0xF0,
//! 0xFF and combining tone marks in 0xB0..=0xB4) keep their Windows-1252 meaning here.

use std::sync::OnceLock;

use super::Sink;
use crate::codepage::cp1252_to_char;
use crate::letters::{Letter, Tone, lookup};

/// Lowercase letters, `[vowel][tone]` (tones: level, grave, hook, tilde, acute, dot).
const LOWER: [[u8; 6]; 12] = [
    [b'a', 0xB5, 0xB6, 0xB7, 0xB8, 0xB9], // a à ả ã á ạ
    [0xA8, 0xBB, 0xBC, 0xBD, 0xBE, 0xC6], // ă ằ ẳ ẵ ắ ặ
    [0xA9, 0xC7, 0xC8, 0xC9, 0xCA, 0xCB], // â ầ ẩ ẫ ấ ậ
    [b'e', 0xCC, 0xCE, 0xCF, 0xD0, 0xD1], // e è ẻ ẽ é ẹ
    [0xAA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6], // ê ề ể ễ ế ệ
    [b'i', 0xD7, 0xD8, 0xDC, 0xDD, 0xDE], // i ì ỉ ĩ í ị
    [b'o', 0xDF, 0xE1, 0xE2, 0xE3, 0xE4], // o ò ỏ õ ó ọ
    [0xAB, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9], // ô ồ ổ ỗ ố ộ
    [0xAC, 0xEA, 0xEB, 0xEC, 0xED, 0xEE], // ơ ờ ở ỡ ớ ợ
    [b'u', 0xEF, 0xF1, 0xF2, 0xF3, 0xF4], // u ù ủ ũ ú ụ
    [0xAD, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9], // ư ừ ử ữ ứ ự
    [b'y', 0xFA, 0xFB, 0xFC, 0xFD, 0xFE], // y ỳ ỷ ỹ ý ỵ
];

/// Untoned uppercase letters, `[vowel]`: A Ă Â E Ê I O Ô Ơ U Ư Y.
const UPPER_LEVEL: [u8; 12] = [
    b'A', 0xA1, 0xA2, b'E', 0xA3, b'I', b'O', 0xA4, 0xA5, b'U', 0xA6, b'Y',
];

const D_LOWER: u8 = 0xAE;
const D_UPPER: u8 = 0xA7;

/// The code of a letter and whether it is exact.
fn code(letter: Letter) -> (u8, bool) {
    // The table indices come from the enums, so the `?` fallback is never written (tested).
    match letter {
        Letter::Vowel {
            vowel,
            tone,
            upper: false,
        } => (lookup(&LOWER, vowel, tone).unwrap_or(b'?'), true),
        Letter::Vowel {
            vowel,
            tone: Tone::Level,
            upper: true,
        } => (
            UPPER_LEVEL.get(vowel.index()).copied().unwrap_or(b'?'),
            true,
        ),
        Letter::Vowel { vowel, tone, .. } => (lookup(&LOWER, vowel, tone).unwrap_or(b'?'), false),
        Letter::DStroke { upper } => (if upper { D_UPPER } else { D_LOWER }, true),
    }
}

fn table() -> &'static [Option<char>; 256] {
    static TABLE: OnceLock<[Option<char>; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = [None; 256];
        for letter in Letter::all() {
            // ASCII letters keep their ASCII code and need no entry.
            if let (byte, true) = code(letter)
                && !letter.to_char().is_ascii()
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
    let (byte, exact) = code(Letter::from_char(c)?);
    sink.byte(byte);
    Some(exact)
}

pub(super) fn is_free(byte: u8) -> bool {
    entry(byte).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::letters::Vowel;

    #[test]
    fn vn3_assigns_74_codes_and_no_c0_control() {
        let assigned: Vec<u8> = (0..=255).filter(|&b| entry(b).is_some()).collect();
        assert_eq!(assigned.len(), 74);
        assert!(assigned.iter().all(|&b| b >= 0xA1));
        assert_eq!(Vowel::ALL.len(), UPPER_LEVEL.len());
    }

    #[test]
    fn every_letter_has_a_code_and_the_fallback_is_never_used() {
        for letter in Letter::all() {
            assert_ne!(code(letter).0, b'?', "{letter:?}");
        }
    }
}
