//! VNI Windows (VNI fonts).
//!
//! A letter is written as an ASCII base letter followed by one diacritic byte that carries the
//! vowel mark and/or the tone. ơ, ư, đ, the toned i letters, and ỵ have single-byte forms; ơ and
//! ư also serve as bases for their toned forms. Uppercase letters use the uppercase base and the
//! diacritic byte 0x20 lower (the Latin-1 uppercase position).
//!
//! Lowercase layout (bytes shown as Windows-1252 characters):
//!
//! | mark \ tone   | level | grave | hook | tilde | acute | dot |
//! |---------------|-------|-------|------|-------|-------|-----|
//! | none          |       | ø F8  | û FB | õ F5  | ù F9  | ï EF |
//! | circumflex    | â E2  | à E0  | å E5 | ã E3  | á E1  | ä E4 |
//! | breve         | ê EA  | è E8  | ú FA | ü FC  | é E9  | ë EB |
//!
//! Single bytes: đ ñ F1, ơ ô F4, ư ö F6, ì EC, ỉ æ E6, ĩ ó F3, í ED, ị ò F2, ỵ î EE.
//!
//! Decoding also accepts pairs the encoder never writes: a base and diacritic in different case
//! (the base decides the case), and i or y followed by a tone byte.

use std::sync::OnceLock;

use super::{Sink, Unit};
use crate::codepage::cp1252_to_char;
use crate::letters::{Letter, Shape, Tone, Vowel};

/// Lowercase diacritic bytes and what they add to the base letter.
const MARKS: [(u8, Option<Shape>, Tone); 17] = [
    (0xF8, None, Tone::Grave),
    (0xFB, None, Tone::Hook),
    (0xF5, None, Tone::Tilde),
    (0xF9, None, Tone::Acute),
    (0xEF, None, Tone::Dot),
    (0xE2, Some(Shape::Circumflex), Tone::Level),
    (0xE0, Some(Shape::Circumflex), Tone::Grave),
    (0xE5, Some(Shape::Circumflex), Tone::Hook),
    (0xE3, Some(Shape::Circumflex), Tone::Tilde),
    (0xE1, Some(Shape::Circumflex), Tone::Acute),
    (0xE4, Some(Shape::Circumflex), Tone::Dot),
    (0xEA, Some(Shape::Breve), Tone::Level),
    (0xE8, Some(Shape::Breve), Tone::Grave),
    (0xFA, Some(Shape::Breve), Tone::Hook),
    (0xFC, Some(Shape::Breve), Tone::Tilde),
    (0xE9, Some(Shape::Breve), Tone::Acute),
    (0xEB, Some(Shape::Breve), Tone::Dot),
];

/// Lowercase single-byte letters.
const SINGLES: [(u8, Letter); 9] = [
    (0xF1, Letter::DStroke { upper: false }),
    (0xF4, vowel(Vowel::OHorn, Tone::Level)),
    (0xF6, vowel(Vowel::UHorn, Tone::Level)),
    (0xEC, vowel(Vowel::I, Tone::Grave)),
    (0xE6, vowel(Vowel::I, Tone::Hook)),
    (0xF3, vowel(Vowel::I, Tone::Tilde)),
    (0xED, vowel(Vowel::I, Tone::Acute)),
    (0xF2, vowel(Vowel::I, Tone::Dot)),
    (0xEE, vowel(Vowel::Y, Tone::Dot)),
];

/// Lowercase base bytes that a diacritic byte may follow.
const BASES: [(u8, Vowel); 8] = [
    (b'a', Vowel::A),
    (b'e', Vowel::E),
    (b'i', Vowel::I),
    (b'o', Vowel::O),
    (b'u', Vowel::U),
    (b'y', Vowel::Y),
    (0xF4, Vowel::OHorn),
    (0xF6, Vowel::UHorn),
];

const fn vowel(vowel: Vowel, tone: Tone) -> Letter {
    Letter::Vowel {
        vowel,
        tone,
        upper: false,
    }
}

/// The uppercase form of a lowercase VNI byte.
const fn to_upper(byte: u8) -> u8 {
    match byte {
        b'a'..=b'z' => byte.to_ascii_uppercase(),
        0xE0..=0xFE => byte.wrapping_sub(0x20),
        _ => byte,
    }
}

/// The lowercase form of a byte and whether it was uppercase.
const fn to_lower(byte: u8) -> (u8, bool) {
    match byte {
        b'A'..=b'Z' => (byte.to_ascii_lowercase(), true),
        0xC0..=0xDE if byte != 0xD7 => (byte.wrapping_add(0x20), true),
        _ => (byte, false),
    }
}

#[derive(Clone, Copy)]
enum Role {
    Mark(Option<Shape>, Tone),
    Single(Letter),
}

/// Diacritic and single-letter bytes of both cases.
fn roles() -> &'static [Option<Role>; 256] {
    static ROLES: OnceLock<[Option<Role>; 256]> = OnceLock::new();
    ROLES.get_or_init(|| {
        let mut roles = [None; 256];
        let mut set = |byte: u8, role: Role| {
            if let Some(slot) = roles.get_mut(usize::from(byte)) {
                *slot = Some(role);
            }
        };
        for (byte, shape, tone) in MARKS {
            set(byte, Role::Mark(shape, tone));
            set(to_upper(byte), Role::Mark(shape, tone));
        }
        for (byte, letter) in SINGLES {
            set(byte, Role::Single(letter));
            set(to_upper(byte), Role::Single(letter.with_upper(true)));
        }
        roles
    })
}

fn base(byte: u8) -> Option<(Vowel, bool)> {
    let (lower, upper) = to_lower(byte);
    BASES
        .iter()
        .find(|(base, _)| *base == lower)
        .map(|(_, vowel)| (*vowel, upper))
}

fn role(byte: u8) -> Option<Role> {
    roles().get(usize::from(byte)).copied().flatten()
}

fn mark(byte: u8) -> Option<(Option<Shape>, Tone)> {
    match role(byte) {
        Some(Role::Mark(shape, tone)) => Some((shape, tone)),
        _ => None,
    }
}

/// The letter a base byte and a diacritic byte spell, if any.
fn pair(base_byte: u8, mark_byte: u8) -> Option<Letter> {
    let (base_vowel, upper) = base(base_byte)?;
    let (shape, tone) = mark(mark_byte)?;
    let vowel = match shape {
        None => base_vowel,
        Some(shape) => base_vowel.with_shape(shape)?,
    };
    Some(Letter::Vowel { vowel, tone, upper })
}

pub(super) fn decode(units: impl Iterator<Item = Unit>) -> String {
    let mut out = String::new();
    let mut units = units.peekable();
    while let Some(unit) = units.next() {
        let byte = match unit {
            Unit::Byte(byte) => byte,
            Unit::Char(c) => {
                out.push(c);
                continue;
            }
        };
        if let Some(&Unit::Byte(next)) = units.peek()
            && let Some(letter) = pair(byte, next)
        {
            units.next();
            out.push(letter.to_char());
            continue;
        }
        match role(byte) {
            Some(Role::Single(letter)) => out.push(letter.to_char()),
            _ => out.push(cp1252_to_char(byte)),
        }
    }
    out
}

/// The lowercase bytes of a lowercase letter (one or two).
fn lowercase_bytes(letter: Letter) -> (u8, Option<u8>) {
    if let Some((byte, _)) = SINGLES.iter().find(|(_, single)| *single == letter) {
        return (*byte, None);
    }
    let Letter::Vowel { vowel, tone, .. } = letter else {
        // đ is a single byte.
        return (0xF1, None);
    };
    let (base_vowel, shape) = match vowel {
        Vowel::OHorn | Vowel::UHorn => (vowel, None),
        _ => vowel
            .split_shape()
            .map_or((vowel, None), |(plain, shape)| (plain, Some(shape))),
    };
    let base_byte = BASES
        .iter()
        .find(|(_, base)| *base == base_vowel)
        .map_or(b'?', |(byte, _)| *byte);
    let mark_byte = MARKS
        .iter()
        .find(|(_, mark_shape, mark_tone)| *mark_shape == shape && *mark_tone == tone)
        .map(|(byte, _, _)| *byte);
    (base_byte, mark_byte)
}

pub(super) fn encode_letter(c: char, sink: &mut impl Sink) -> Option<bool> {
    let letter = Letter::from_char(c)?;
    let upper = letter.is_upper();
    let (base_byte, mark_byte) = lowercase_bytes(letter.with_upper(false));
    let case = |byte: u8| if upper { to_upper(byte) } else { byte };
    sink.byte(case(base_byte));
    if let Some(mark_byte) = mark_byte {
        sink.byte(case(mark_byte));
    }
    Some(true)
}

pub(super) fn is_free(byte: u8) -> bool {
    role(byte).is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn special_bytes_are_17_marks_and_9_singles_in_each_case() {
        let marks = (0..=255_u8).filter(|&b| mark(b).is_some()).count();
        let singles = (0..=255_u8)
            .filter(|&b| matches!(role(b), Some(Role::Single(_))))
            .count();
        assert_eq!((marks, singles), (34, 18));
        // No byte is both a base and a diacritic, so pairs are unambiguous.
        assert!((0..=255_u8).all(|b| !(base(b).is_some() && mark(b).is_some())));
    }

    #[test]
    fn case_mapping_round_trips_on_special_bytes() {
        for (byte, _, _) in MARKS {
            assert_eq!(to_lower(to_upper(byte)), (byte, true));
        }
        for (byte, _) in SINGLES {
            assert_eq!(to_lower(to_upper(byte)), (byte, true));
        }
    }
}
