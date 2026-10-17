//! Windows-1252 and Windows-1258 single-byte tables.
//!
//! Bytes that a code page leaves undefined map to the C1 control with the same value (as Windows
//! and the WHATWG encoding standard do), so every byte has exactly one character and back.

/// Windows-1252 0x80..=0x9F (undefined bytes 0x81, 0x8D, 0x8F, 0x90, 0x9D are C1 controls).
const CP1252_80_9F: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{160}', '\u{2039}', '\u{152}', '\u{8D}', '\u{17D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{2DC}', '\u{2122}', '\u{161}', '\u{203A}', '\u{153}', '\u{9D}', '\u{17E}', '\u{178}',
];

/// Windows-1258 0x80..=0x9F (undefined bytes 0x81, 0x8A, 0x8D..=0x90, 0x9A, 0x9D, 0x9E are C1
/// controls).
const CP1258_80_9F: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{2C6}', '\u{2030}', '\u{8A}', '\u{2039}', '\u{152}', '\u{8D}', '\u{8E}', '\u{8F}', '\u{90}',
    '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}', '\u{2DC}',
    '\u{2122}', '\u{9A}', '\u{203A}', '\u{153}', '\u{9D}', '\u{9E}', '\u{178}',
];

/// Windows-1258 positions in 0xC0..=0xFF that differ from Latin-1: Vietnamese letters, the five
/// combining tone marks, and the dong sign.
const CP1258_OVERRIDES: [(u8, char); 14] = [
    (0xC3, '\u{102}'),  // Ă
    (0xCC, '\u{300}'),  // combining grave
    (0xD0, '\u{110}'),  // Đ
    (0xD2, '\u{309}'),  // combining hook above
    (0xD5, '\u{1A0}'),  // Ơ
    (0xDD, '\u{1AF}'),  // Ư
    (0xDE, '\u{303}'),  // combining tilde
    (0xE3, '\u{103}'),  // ă
    (0xEC, '\u{301}'),  // combining acute
    (0xF0, '\u{111}'),  // đ
    (0xF2, '\u{323}'),  // combining dot below
    (0xF5, '\u{1A1}'),  // ơ
    (0xFD, '\u{1B0}'),  // ư
    (0xFE, '\u{20AB}'), // ₫
];

fn high_to_char(table: &[char; 32], byte: u8) -> char {
    table
        .get(usize::from(byte.wrapping_sub(0x80)))
        .copied()
        .unwrap_or(char::from(byte))
}

fn high_to_byte(table: &[char; 32], c: char) -> Option<u8> {
    let offset = table.iter().position(|&entry| entry == c)?;
    u8::try_from(offset).ok().map(|offset| 0x80 | offset)
}

/// The character of a Windows-1252 byte.
pub(crate) fn cp1252_to_char(byte: u8) -> char {
    match byte {
        0x80..=0x9F => high_to_char(&CP1252_80_9F, byte),
        _ => char::from(byte),
    }
}

/// The Windows-1252 byte of a character, if it has one.
pub(crate) fn cp1252_to_byte(c: char) -> Option<u8> {
    match u32::from(c) {
        0x00..=0x7F | 0xA0..=0xFF => u8::try_from(c).ok(),
        _ => high_to_byte(&CP1252_80_9F, c),
    }
}

/// The character of a Windows-1258 byte.
pub(crate) fn cp1258_to_char(byte: u8) -> char {
    match byte {
        0x80..=0x9F => high_to_char(&CP1258_80_9F, byte),
        _ => CP1258_OVERRIDES
            .iter()
            .find(|(position, _)| *position == byte)
            .map_or(char::from(byte), |(_, c)| *c),
    }
}

/// The Windows-1258 byte of a character, if it has one.
pub(crate) fn cp1258_to_byte(c: char) -> Option<u8> {
    if let Some((byte, _)) = CP1258_OVERRIDES.iter().find(|(_, entry)| *entry == c) {
        return Some(*byte);
    }
    match u32::from(c) {
        0x00..=0x7F | 0xA0..=0xFF => {
            let byte = u8::try_from(c).ok()?;
            // A Latin-1 character whose position Windows-1258 reuses is not in the code page.
            (cp1258_to_char(byte) == c).then_some(byte)
        }
        _ => high_to_byte(&CP1258_80_9F, c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Python 3.14 `bytes([b]).decode("cp1252")` for 0x80..=0xFF; 0 marks an undefined byte.
    const PY_CP1252_HIGH: [u32; 128] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
        0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178, 0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A4,
        0x00A5, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF,
        0x00B0, 0x00B1, 0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x00BA,
        0x00BB, 0x00BC, 0x00BD, 0x00BE, 0x00BF, 0x00C0, 0x00C1, 0x00C2, 0x00C3, 0x00C4, 0x00C5,
        0x00C6, 0x00C7, 0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x00CC, 0x00CD, 0x00CE, 0x00CF, 0x00D0,
        0x00D1, 0x00D2, 0x00D3, 0x00D4, 0x00D5, 0x00D6, 0x00D7, 0x00D8, 0x00D9, 0x00DA, 0x00DB,
        0x00DC, 0x00DD, 0x00DE, 0x00DF, 0x00E0, 0x00E1, 0x00E2, 0x00E3, 0x00E4, 0x00E5, 0x00E6,
        0x00E7, 0x00E8, 0x00E9, 0x00EA, 0x00EB, 0x00EC, 0x00ED, 0x00EE, 0x00EF, 0x00F0, 0x00F1,
        0x00F2, 0x00F3, 0x00F4, 0x00F5, 0x00F6, 0x00F7, 0x00F8, 0x00F9, 0x00FA, 0x00FB, 0x00FC,
        0x00FD, 0x00FE, 0x00FF,
    ];

    /// Python 3.14 `bytes([b]).decode("cp1258")` for 0x80..=0xFF; 0 marks an undefined byte.
    /// GNU libiconv 1.19 `CP1258` agrees on every byte.
    const PY_CP1258_HIGH: [u32; 128] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0, 0x2039,
        0x0152, 0, 0, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122,
        0, 0x203A, 0x0153, 0, 0, 0x0178, 0x00A0, 0x00A1, 0x00A2, 0x00A3, 0x00A4, 0x00A5, 0x00A6,
        0x00A7, 0x00A8, 0x00A9, 0x00AA, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x00AF, 0x00B0, 0x00B1,
        0x00B2, 0x00B3, 0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8, 0x00B9, 0x00BA, 0x00BB, 0x00BC,
        0x00BD, 0x00BE, 0x00BF, 0x00C0, 0x00C1, 0x00C2, 0x0102, 0x00C4, 0x00C5, 0x00C6, 0x00C7,
        0x00C8, 0x00C9, 0x00CA, 0x00CB, 0x0300, 0x00CD, 0x00CE, 0x00CF, 0x0110, 0x00D1, 0x0309,
        0x00D3, 0x00D4, 0x01A0, 0x00D6, 0x00D7, 0x00D8, 0x00D9, 0x00DA, 0x00DB, 0x00DC, 0x01AF,
        0x0303, 0x00DF, 0x00E0, 0x00E1, 0x00E2, 0x0103, 0x00E4, 0x00E5, 0x00E6, 0x00E7, 0x00E8,
        0x00E9, 0x00EA, 0x00EB, 0x0301, 0x00ED, 0x00EE, 0x00EF, 0x0111, 0x00F1, 0x0323, 0x00F3,
        0x00F4, 0x01A1, 0x00F6, 0x00F7, 0x00F8, 0x00F9, 0x00FA, 0x00FB, 0x00FC, 0x01B0, 0x20AB,
        0x00FF,
    ];

    fn check_against_oracle(
        oracle: &[u32; 128],
        to_char: fn(u8) -> char,
        to_byte: fn(char) -> Option<u8>,
    ) {
        for byte in 0..=0x7F_u8 {
            assert_eq!(to_char(byte), char::from(byte), "ASCII {byte:#04X}");
        }
        for (offset, &expected) in oracle.iter().enumerate() {
            let byte = 0x80 + u8::try_from(offset).unwrap();
            // Undefined bytes map to the C1 control of the same value.
            let expected = if expected == 0 {
                u32::from(byte)
            } else {
                expected
            };
            assert_eq!(u32::from(to_char(byte)), expected, "byte {byte:#04X}");
        }
        for byte in 0..=0xFF_u8 {
            assert_eq!(to_byte(to_char(byte)), Some(byte), "byte {byte:#04X}");
        }
    }

    #[test]
    fn cp1252_matches_the_python_codec_and_is_a_bijection() {
        check_against_oracle(&PY_CP1252_HIGH, cp1252_to_char, cp1252_to_byte);
    }

    #[test]
    fn cp1258_matches_the_python_codec_and_is_a_bijection() {
        check_against_oracle(&PY_CP1258_HIGH, cp1258_to_char, cp1258_to_byte);
    }

    #[test]
    fn characters_outside_the_code_page_have_no_byte() {
        for c in ['\u{80}', '\u{82}', '\u{100}', 'ế', '中', '\u{1F600}'] {
            assert_eq!(cp1252_to_byte(c), None, "{c:?}");
        }
        // Windows-1258 replaces these Windows-1252 characters.
        for c in [
            'Š', 'Ž', 'š', 'ž', 'Ã', 'Ì', 'Ð', 'Ò', 'Õ', 'Ý', 'Þ', 'ã', 'ì', 'ð', 'ò', 'õ',
        ] {
            assert_eq!(cp1258_to_byte(c), None, "{c:?}");
        }
    }
}
