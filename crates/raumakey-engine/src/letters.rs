//! Letters, marks, and precomposed Vietnamese characters (plan §3.3, §3.7).

use raumakey_model::keymap::Tone;

/// A diacritic carried by a letter. Tones are word-level and are not marks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Mark {
    /// No mark.
    #[default]
    None,
    /// â ê ô
    Circumflex,
    /// ă
    Breve,
    /// ơ ư
    Horn,
    /// đ
    Stroke,
}

/// An ASCII letter with case and mark. The base is always `'a'..='z'`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Letter {
    base: char,
    /// Rendered in uppercase.
    pub(crate) upper: bool,
    /// Diacritic mark.
    pub(crate) mark: Mark,
    /// Standalone `ư` produced by Telex `w`: the key that typed it.
    pub(crate) from_w: Option<char>,
}

impl Letter {
    /// The letter typed as `key`, when `key` is an ASCII letter.
    pub(crate) fn from_key(key: char) -> Option<Self> {
        key.is_ascii_alphabetic().then(|| Self {
            base: key.to_ascii_lowercase(),
            upper: key.is_ascii_uppercase(),
            mark: Mark::None,
            from_w: None,
        })
    }

    /// Standalone `ư` produced by Telex `w`, in the case of `key`.
    pub(crate) fn standalone_u_horn(key: char) -> Self {
        Self {
            base: 'u',
            upper: key.is_uppercase(),
            mark: Mark::Horn,
            from_w: Some(key),
        }
    }

    /// Lowercase ASCII base letter.
    pub(crate) fn base(&self) -> char {
        self.base
    }

    /// Lowercase letter with its mark and without tone (`ư`, `đ`, `a`, ...).
    /// A mark that does not apply to the base is ignored.
    pub(crate) fn symbol(&self) -> char {
        match (self.base, self.mark) {
            ('a', Mark::Breve) => 'ă',
            ('a', Mark::Circumflex) => 'â',
            ('e', Mark::Circumflex) => 'ê',
            ('o', Mark::Circumflex) => 'ô',
            ('o', Mark::Horn) => 'ơ',
            ('u', Mark::Horn) => 'ư',
            ('d', Mark::Stroke) => 'đ',
            (base, _) => base,
        }
    }

    /// True for the twelve Vietnamese vowels.
    pub(crate) fn is_vowel(&self) -> bool {
        vowel_row(self.symbol()).is_some()
    }

    /// Precomposed character with `tone` applied, in the letter's case.
    /// Consonants ignore the tone.
    pub(crate) fn render(&self, tone: Option<Tone>) -> char {
        let symbol = self.symbol();
        let table = if self.upper { &UPPER } else { &LOWER };
        if let Some(&composed) = vowel_row(symbol)
            .and_then(|row| table.get(row))
            .and_then(|row| row.get(tone_column(tone)))
        {
            return composed;
        }
        match (symbol, self.upper) {
            ('đ', true) => 'Đ',
            ('đ', false) => 'đ',
            (other, true) => other.to_ascii_uppercase(),
            (other, false) => other,
        }
    }
}

/// Vowel rows of [`LOWER`] and [`UPPER`], in this order.
const VOWEL_SYMBOLS: [char; 12] = ['a', 'ă', 'â', 'e', 'ê', 'i', 'o', 'ô', 'ơ', 'u', 'ư', 'y'];

/// Columns: no tone, sắc, huyền, hỏi, ngã, nặng.
const LOWER: [[char; 6]; 12] = [
    ['a', 'á', 'à', 'ả', 'ã', 'ạ'],
    ['ă', 'ắ', 'ằ', 'ẳ', 'ẵ', 'ặ'],
    ['â', 'ấ', 'ầ', 'ẩ', 'ẫ', 'ậ'],
    ['e', 'é', 'è', 'ẻ', 'ẽ', 'ẹ'],
    ['ê', 'ế', 'ề', 'ể', 'ễ', 'ệ'],
    ['i', 'í', 'ì', 'ỉ', 'ĩ', 'ị'],
    ['o', 'ó', 'ò', 'ỏ', 'õ', 'ọ'],
    ['ô', 'ố', 'ồ', 'ổ', 'ỗ', 'ộ'],
    ['ơ', 'ớ', 'ờ', 'ở', 'ỡ', 'ợ'],
    ['u', 'ú', 'ù', 'ủ', 'ũ', 'ụ'],
    ['ư', 'ứ', 'ừ', 'ử', 'ữ', 'ự'],
    ['y', 'ý', 'ỳ', 'ỷ', 'ỹ', 'ỵ'],
];

/// Uppercase counterpart of [`LOWER`].
const UPPER: [[char; 6]; 12] = [
    ['A', 'Á', 'À', 'Ả', 'Ã', 'Ạ'],
    ['Ă', 'Ắ', 'Ằ', 'Ẳ', 'Ẵ', 'Ặ'],
    ['Â', 'Ấ', 'Ầ', 'Ẩ', 'Ẫ', 'Ậ'],
    ['E', 'É', 'È', 'Ẻ', 'Ẽ', 'Ẹ'],
    ['Ê', 'Ế', 'Ề', 'Ể', 'Ễ', 'Ệ'],
    ['I', 'Í', 'Ì', 'Ỉ', 'Ĩ', 'Ị'],
    ['O', 'Ó', 'Ò', 'Ỏ', 'Õ', 'Ọ'],
    ['Ô', 'Ố', 'Ồ', 'Ổ', 'Ỗ', 'Ộ'],
    ['Ơ', 'Ớ', 'Ờ', 'Ở', 'Ỡ', 'Ợ'],
    ['U', 'Ú', 'Ù', 'Ủ', 'Ũ', 'Ụ'],
    ['Ư', 'Ứ', 'Ừ', 'Ử', 'Ữ', 'Ự'],
    ['Y', 'Ý', 'Ỳ', 'Ỷ', 'Ỹ', 'Ỵ'],
];

fn vowel_row(symbol: char) -> Option<usize> {
    VOWEL_SYMBOLS.iter().position(|&v| v == symbol)
}

fn tone_column(tone: Option<Tone>) -> usize {
    match tone {
        None => 0,
        Some(Tone::Acute) => 1,
        Some(Tone::Grave) => 2,
        Some(Tone::Hook) => 3,
        Some(Tone::Tilde) => 4,
        Some(Tone::Dot) => 5,
    }
}

/// One element of a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Item {
    /// An ASCII letter (possibly marked).
    Letter(Letter),
    /// Any other character; makes the word non-Vietnamese.
    Other(char),
}

impl Item {
    /// The literal item for a typed key: ASCII letters become letters, anything else `Other`.
    pub(crate) fn literal(key: char) -> Self {
        Letter::from_key(key).map_or(Self::Other(key), Self::Letter)
    }

    /// The letter, if this item is one.
    pub(crate) fn letter(&self) -> Option<&Letter> {
        match self {
            Self::Letter(letter) => Some(letter),
            Self::Other(_) => None,
        }
    }

    /// Mutable access to the letter, if this item is one.
    pub(crate) fn letter_mut(&mut self) -> Option<&mut Letter> {
        match self {
            Self::Letter(letter) => Some(letter),
            Self::Other(_) => None,
        }
    }

    /// The Vietnamese letter symbol, or `None` for `Other` items and the non-Vietnamese
    /// letters `f j w z`.
    pub(crate) fn viet_symbol(&self) -> Option<char> {
        let symbol = self.letter()?.symbol();
        (!matches!(symbol, 'f' | 'j' | 'w' | 'z')).then_some(symbol)
    }

    /// True when the item is a vowel letter.
    pub(crate) fn is_vowel(&self) -> bool {
        self.letter().is_some_and(Letter::is_vowel)
    }
}

#[cfg(test)]
mod tests {
    use unicode_normalization::UnicodeNormalization;

    use super::*;

    const TONES: [Option<Tone>; 6] = [
        None,
        Some(Tone::Acute),
        Some(Tone::Grave),
        Some(Tone::Hook),
        Some(Tone::Tilde),
        Some(Tone::Dot),
    ];

    fn tone_mark(tone: Option<Tone>) -> &'static str {
        match tone {
            None => "",
            Some(Tone::Acute) => "\u{301}",
            Some(Tone::Grave) => "\u{300}",
            Some(Tone::Hook) => "\u{309}",
            Some(Tone::Tilde) => "\u{303}",
            Some(Tone::Dot) => "\u{323}",
        }
    }

    fn letter(base: char, mark: Mark, upper: bool) -> Letter {
        let mut l = Letter::from_key(base).expect("ascii letter");
        l.mark = mark;
        l.upper = upper;
        l
    }

    const VOWELS: [(char, Mark, &str); 12] = [
        ('a', Mark::None, ""),
        ('a', Mark::Breve, "\u{306}"),
        ('a', Mark::Circumflex, "\u{302}"),
        ('e', Mark::None, ""),
        ('e', Mark::Circumflex, "\u{302}"),
        ('i', Mark::None, ""),
        ('o', Mark::None, ""),
        ('o', Mark::Circumflex, "\u{302}"),
        ('o', Mark::Horn, "\u{31B}"),
        ('u', Mark::None, ""),
        ('u', Mark::Horn, "\u{31B}"),
        ('y', Mark::None, ""),
    ];

    #[test]
    fn every_vowel_mark_tone_is_one_canonical_precomposed_char() {
        for (base, mark, mark_text) in VOWELS {
            for upper in [false, true] {
                for tone in TONES {
                    let rendered = letter(base, mark, upper).render(tone);
                    let base_text = if upper {
                        base.to_ascii_uppercase()
                    } else {
                        base
                    };
                    let decomposed = format!("{base_text}{mark_text}{}", tone_mark(tone));
                    let composed: String = decomposed.nfc().collect();
                    assert_eq!(
                        composed.chars().count(),
                        1,
                        "{decomposed:?} composes to one char"
                    );
                    assert_eq!(rendered.to_string(), composed, "{base} {mark:?} {tone:?}");
                }
            }
        }
    }

    #[test]
    fn symbols_and_vowels() {
        let cases = [
            ('a', Mark::None, 'a', true),
            ('a', Mark::Breve, 'ă', true),
            ('a', Mark::Circumflex, 'â', true),
            ('e', Mark::Circumflex, 'ê', true),
            ('o', Mark::Circumflex, 'ô', true),
            ('o', Mark::Horn, 'ơ', true),
            ('u', Mark::Horn, 'ư', true),
            ('i', Mark::None, 'i', true),
            ('y', Mark::None, 'y', true),
            ('d', Mark::Stroke, 'đ', false),
            ('d', Mark::None, 'd', false),
            ('n', Mark::None, 'n', false),
            ('w', Mark::None, 'w', false),
        ];
        for (base, mark, symbol, vowel) in cases {
            let l = letter(base, mark, false);
            assert_eq!(l.symbol(), symbol);
            assert_eq!(l.is_vowel(), vowel, "{symbol}");
        }
    }

    #[test]
    fn stroke_and_consonants_render_in_case() {
        assert_eq!(letter('d', Mark::Stroke, false).render(None), 'đ');
        assert_eq!(letter('d', Mark::Stroke, true).render(None), 'Đ');
        assert_eq!(letter('n', Mark::None, true).render(None), 'N');
        assert_eq!(
            letter('n', Mark::None, false).render(Some(Tone::Acute)),
            'n'
        );
        assert_eq!(Letter::standalone_u_horn('W').render(None), 'Ư');
        assert_eq!(Letter::standalone_u_horn('w').render(Some(Tone::Dot)), 'ự');
    }

    #[test]
    fn literal_items_and_vietnamese_symbols() {
        assert_eq!(
            Item::literal('A').letter().map(|l| (l.base(), l.upper)),
            Some(('a', true))
        );
        assert_eq!(Item::literal('7'), Item::Other('7'));
        assert_eq!(Item::literal('é'), Item::Other('é'));
        assert_eq!(Item::literal('a').viet_symbol(), Some('a'));
        for key in ['f', 'j', 'w', 'z', 'F'] {
            assert_eq!(Item::literal(key).viet_symbol(), None, "{key}");
        }
        assert_eq!(Item::Other('1').viet_symbol(), None);
        assert_eq!(
            Item::Letter(Letter::standalone_u_horn('w')).viet_symbol(),
            Some('ư')
        );
    }
}
