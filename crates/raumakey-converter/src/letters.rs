//! The Vietnamese letter model shared by every charset table.

use std::collections::HashMap;
use std::sync::OnceLock;

/// Vowel letters (base letter plus vowel mark) in the order used by every table in this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Vowel {
    A,
    ABreve,
    ACircumflex,
    E,
    ECircumflex,
    I,
    O,
    OCircumflex,
    OHorn,
    U,
    UHorn,
    Y,
}

/// Vowel marks that turn a plain vowel into another vowel letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Shape {
    Breve,
    Circumflex,
    Horn,
}

/// Tones in the order of the TCVN tables: level (no mark), grave, hook, tilde, acute, dot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Tone {
    Level,
    Grave,
    Hook,
    Tilde,
    Acute,
    Dot,
}

/// One Vietnamese letter: a vowel with its tone, or đ.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Letter {
    Vowel {
        vowel: Vowel,
        tone: Tone,
        upper: bool,
    },
    DStroke {
        upper: bool,
    },
}

/// Lowercase vowel letters, `[vowel][tone]` in [`Vowel`] and [`Tone`] order.
const LOWER: [[char; 6]; 12] = [
    ['a', 'à', 'ả', 'ã', 'á', 'ạ'],
    ['ă', 'ằ', 'ẳ', 'ẵ', 'ắ', 'ặ'],
    ['â', 'ầ', 'ẩ', 'ẫ', 'ấ', 'ậ'],
    ['e', 'è', 'ẻ', 'ẽ', 'é', 'ẹ'],
    ['ê', 'ề', 'ể', 'ễ', 'ế', 'ệ'],
    ['i', 'ì', 'ỉ', 'ĩ', 'í', 'ị'],
    ['o', 'ò', 'ỏ', 'õ', 'ó', 'ọ'],
    ['ô', 'ồ', 'ổ', 'ỗ', 'ố', 'ộ'],
    ['ơ', 'ờ', 'ở', 'ỡ', 'ớ', 'ợ'],
    ['u', 'ù', 'ủ', 'ũ', 'ú', 'ụ'],
    ['ư', 'ừ', 'ử', 'ữ', 'ứ', 'ự'],
    ['y', 'ỳ', 'ỷ', 'ỹ', 'ý', 'ỵ'],
];

/// Uppercase vowel letters, `[vowel][tone]`.
const UPPER: [[char; 6]; 12] = [
    ['A', 'À', 'Ả', 'Ã', 'Á', 'Ạ'],
    ['Ă', 'Ằ', 'Ẳ', 'Ẵ', 'Ắ', 'Ặ'],
    ['Â', 'Ầ', 'Ẩ', 'Ẫ', 'Ấ', 'Ậ'],
    ['E', 'È', 'Ẻ', 'Ẽ', 'É', 'Ẹ'],
    ['Ê', 'Ề', 'Ể', 'Ễ', 'Ế', 'Ệ'],
    ['I', 'Ì', 'Ỉ', 'Ĩ', 'Í', 'Ị'],
    ['O', 'Ò', 'Ỏ', 'Õ', 'Ó', 'Ọ'],
    ['Ô', 'Ồ', 'Ổ', 'Ỗ', 'Ố', 'Ộ'],
    ['Ơ', 'Ờ', 'Ở', 'Ỡ', 'Ớ', 'Ợ'],
    ['U', 'Ù', 'Ủ', 'Ũ', 'Ú', 'Ụ'],
    ['Ư', 'Ừ', 'Ử', 'Ữ', 'Ứ', 'Ự'],
    ['Y', 'Ỳ', 'Ỷ', 'Ỹ', 'Ý', 'Ỵ'],
];

impl Vowel {
    /// Every vowel, in table order.
    pub(crate) const ALL: [Self; 12] = [
        Self::A,
        Self::ABreve,
        Self::ACircumflex,
        Self::E,
        Self::ECircumflex,
        Self::I,
        Self::O,
        Self::OCircumflex,
        Self::OHorn,
        Self::U,
        Self::UHorn,
        Self::Y,
    ];

    /// Position in the tables.
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The ASCII letter this vowel is written with (lowercase).
    pub(crate) const fn ascii_base(self) -> u8 {
        match self {
            Self::A | Self::ABreve | Self::ACircumflex => b'a',
            Self::E | Self::ECircumflex => b'e',
            Self::I => b'i',
            Self::O | Self::OCircumflex | Self::OHorn => b'o',
            Self::U | Self::UHorn => b'u',
            Self::Y => b'y',
        }
    }

    /// The vowel produced by adding `shape` to this plain vowel, if Vietnamese has one.
    pub(crate) const fn with_shape(self, shape: Shape) -> Option<Self> {
        match (self, shape) {
            (Self::A, Shape::Breve) => Some(Self::ABreve),
            (Self::A, Shape::Circumflex) => Some(Self::ACircumflex),
            (Self::E, Shape::Circumflex) => Some(Self::ECircumflex),
            (Self::O, Shape::Circumflex) => Some(Self::OCircumflex),
            (Self::O, Shape::Horn) => Some(Self::OHorn),
            (Self::U, Shape::Horn) => Some(Self::UHorn),
            _ => None,
        }
    }

    /// Splits a shaped vowel into its plain vowel and shape; `None` for plain vowels.
    pub(crate) const fn split_shape(self) -> Option<(Self, Shape)> {
        match self {
            Self::ABreve => Some((Self::A, Shape::Breve)),
            Self::ACircumflex => Some((Self::A, Shape::Circumflex)),
            Self::ECircumflex => Some((Self::E, Shape::Circumflex)),
            Self::OCircumflex => Some((Self::O, Shape::Circumflex)),
            Self::OHorn => Some((Self::O, Shape::Horn)),
            Self::UHorn => Some((Self::U, Shape::Horn)),
            _ => None,
        }
    }
}

impl Tone {
    /// Every tone, in table order.
    pub(crate) const ALL: [Self; 6] = [
        Self::Level,
        Self::Grave,
        Self::Hook,
        Self::Tilde,
        Self::Acute,
        Self::Dot,
    ];

    /// Position in the tables.
    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

impl Letter {
    /// Every letter: 12 vowels x 6 tones x 2 cases, then đ and Đ (146 letters, 12 of them ASCII).
    pub(crate) fn all() -> impl Iterator<Item = Self> {
        [false, true]
            .into_iter()
            .flat_map(|upper| {
                Vowel::ALL.into_iter().flat_map(move |vowel| {
                    Tone::ALL
                        .into_iter()
                        .map(move |tone| Self::Vowel { vowel, tone, upper })
                })
            })
            .chain([
                Self::DStroke { upper: false },
                Self::DStroke { upper: true },
            ])
    }

    /// The letter a character spells, if it is a Vietnamese letter (ASCII vowels included,
    /// consonants other than đ excluded).
    pub(crate) fn from_char(c: char) -> Option<Self> {
        static INDEX: OnceLock<HashMap<char, Letter>> = OnceLock::new();
        INDEX
            .get_or_init(|| {
                Self::all()
                    .map(|letter| (letter.to_char(), letter))
                    .collect()
            })
            .get(&c)
            .copied()
    }

    /// The precomposed character.
    pub(crate) fn to_char(self) -> char {
        match self {
            Self::Vowel { vowel, tone, upper } => {
                let table = if upper { &UPPER } else { &LOWER };
                // The indices come from the enums, so the entry always exists.
                lookup(table, vowel, tone).unwrap_or(char::REPLACEMENT_CHARACTER)
            }
            Self::DStroke { upper: false } => '\u{111}',
            Self::DStroke { upper: true } => '\u{110}',
        }
    }

    /// True for uppercase letters.
    pub(crate) const fn is_upper(self) -> bool {
        match self {
            Self::Vowel { upper, .. } | Self::DStroke { upper } => upper,
        }
    }

    /// The same letter in the requested case.
    pub(crate) const fn with_upper(self, upper: bool) -> Self {
        match self {
            Self::Vowel { vowel, tone, .. } => Self::Vowel { vowel, tone, upper },
            Self::DStroke { .. } => Self::DStroke { upper },
        }
    }

    /// The ASCII letter left after removing every mark (đ becomes d), in the same case.
    pub(crate) fn without_marks(self) -> char {
        let base = match self {
            Self::Vowel { vowel, .. } => vowel.ascii_base(),
            Self::DStroke { .. } => b'd',
        };
        char::from(if self.is_upper() {
            base.to_ascii_uppercase()
        } else {
            base
        })
    }
}

/// The entry of a `[vowel][tone]` table.
pub(crate) fn lookup<T: Copy>(table: &[[T; 6]; 12], vowel: Vowel, tone: Tone) -> Option<T> {
    table.get(vowel.index())?.get(tone.index()).copied()
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use unicode_normalization::UnicodeNormalization;

    use super::*;

    fn shape_mark(vowel: Vowel) -> &'static str {
        match vowel {
            Vowel::ABreve => "\u{306}",
            Vowel::ACircumflex | Vowel::ECircumflex | Vowel::OCircumflex => "\u{302}",
            Vowel::OHorn | Vowel::UHorn => "\u{31B}",
            _ => "",
        }
    }

    fn tone_mark(tone: Tone) -> &'static str {
        match tone {
            Tone::Level => "",
            Tone::Grave => "\u{300}",
            Tone::Hook => "\u{309}",
            Tone::Tilde => "\u{303}",
            Tone::Acute => "\u{301}",
            Tone::Dot => "\u{323}",
        }
    }

    fn ascii_base(vowel: Vowel) -> char {
        match vowel {
            Vowel::A | Vowel::ABreve | Vowel::ACircumflex => 'a',
            Vowel::E | Vowel::ECircumflex => 'e',
            Vowel::I => 'i',
            Vowel::O | Vowel::OCircumflex | Vowel::OHorn => 'o',
            Vowel::U | Vowel::UHorn => 'u',
            Vowel::Y => 'y',
        }
    }

    #[test]
    fn there_are_146_distinct_letters_and_134_are_not_ascii() {
        let all: Vec<Letter> = Letter::all().collect();
        assert_eq!(all.len(), 146);
        let chars: HashSet<char> = all.iter().map(|l| l.to_char()).collect();
        assert_eq!(chars.len(), 146);
        assert_eq!(chars.iter().filter(|c| !c.is_ascii()).count(), 134);
    }

    #[test]
    fn every_table_entry_is_the_composition_of_base_shape_and_tone() {
        // Cross-checks the hand-written tables against Unicode's canonical compositions.
        for vowel in Vowel::ALL {
            for tone in Tone::ALL {
                for upper in [false, true] {
                    let mut base = ascii_base(vowel).to_string();
                    if upper {
                        base = base.to_uppercase();
                    }
                    let composed: String =
                        format!("{base}{}{}", shape_mark(vowel), tone_mark(tone))
                            .nfc()
                            .collect();
                    let letter = Letter::Vowel { vowel, tone, upper };
                    assert_eq!(letter.to_char().to_string(), composed, "{letter:?}");
                }
            }
        }
        assert_eq!(Letter::DStroke { upper: false }.to_char(), '\u{111}');
        assert_eq!(Letter::DStroke { upper: true }.to_char(), '\u{110}');
    }

    #[test]
    fn from_char_inverts_to_char() {
        for letter in Letter::all() {
            assert_eq!(Letter::from_char(letter.to_char()), Some(letter));
        }
        for c in ['b', 'd', 'D', 'z', '1', ' ', 'ä', 'ç', 'ñ', '\u{301}', '中'] {
            assert_eq!(Letter::from_char(c), None, "{c:?}");
        }
    }

    #[test]
    fn uppercase_letters_are_the_unicode_uppercase_of_lowercase_ones() {
        for letter in Letter::all().filter(|l| !l.is_upper()) {
            let upper = letter.with_upper(true);
            assert!(upper.is_upper());
            assert_eq!(
                letter.to_char().to_uppercase().collect::<String>(),
                upper.to_char().to_string()
            );
        }
    }

    #[test]
    fn without_marks_keeps_only_the_ascii_letter() {
        let stripped: String = "àẰẩẼỆĩỌỐờỰỷđĐaY"
            .chars()
            .map(|c| Letter::from_char(c).map(Letter::without_marks).unwrap())
            .collect();
        assert_eq!(stripped, "aAaEEiOOoUydDaY");
    }

    #[test]
    fn shapes_combine_only_with_their_plain_vowels() {
        assert_eq!(Vowel::A.with_shape(Shape::Breve), Some(Vowel::ABreve));
        assert_eq!(
            Vowel::A.with_shape(Shape::Circumflex),
            Some(Vowel::ACircumflex)
        );
        assert_eq!(
            Vowel::E.with_shape(Shape::Circumflex),
            Some(Vowel::ECircumflex)
        );
        assert_eq!(
            Vowel::O.with_shape(Shape::Circumflex),
            Some(Vowel::OCircumflex)
        );
        assert_eq!(Vowel::O.with_shape(Shape::Horn), Some(Vowel::OHorn));
        assert_eq!(Vowel::U.with_shape(Shape::Horn), Some(Vowel::UHorn));
        for (vowel, shape) in [
            (Vowel::A, Shape::Horn),
            (Vowel::E, Shape::Breve),
            (Vowel::E, Shape::Horn),
            (Vowel::I, Shape::Circumflex),
            (Vowel::O, Shape::Breve),
            (Vowel::U, Shape::Circumflex),
            (Vowel::Y, Shape::Horn),
            (Vowel::ACircumflex, Shape::Breve),
            (Vowel::OHorn, Shape::Horn),
        ] {
            assert_eq!(vowel.with_shape(shape), None, "{vowel:?} {shape:?}");
        }
        for vowel in Vowel::ALL {
            let expected = match vowel {
                Vowel::ABreve => Some((Vowel::A, Shape::Breve)),
                Vowel::ACircumflex => Some((Vowel::A, Shape::Circumflex)),
                Vowel::ECircumflex => Some((Vowel::E, Shape::Circumflex)),
                Vowel::OCircumflex => Some((Vowel::O, Shape::Circumflex)),
                Vowel::OHorn => Some((Vowel::O, Shape::Horn)),
                Vowel::UHorn => Some((Vowel::U, Shape::Horn)),
                _ => None,
            };
            assert_eq!(vowel.split_shape(), expected, "{vowel:?}");
        }
    }
}
