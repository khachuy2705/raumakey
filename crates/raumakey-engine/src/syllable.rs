//! Vietnamese syllable grammar: onsets, rhymes, classification, split, and tone placement
//! (plan §3.4, §3.5).
//!
//! Letters are lowercase symbols with marks and without tone (`ư`, `đ`, `ê`, ...).

use raumakey_model::keymap::Tone;

/// Classification of a letter sequence against the syllable list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Class {
    /// Not the beginning of any syllable, even after adding marks.
    Invalid,
    /// The beginning of at least one syllable (possibly after adding marks).
    Prefix,
    /// A complete syllable.
    Complete,
}

impl Class {
    /// True for `Prefix` and `Complete`.
    pub(crate) fn is_valid(self) -> bool {
        self != Self::Invalid
    }
}

/// Onset/cluster/coda boundaries: onset `0..onset`, cluster `onset..cluster`, coda `cluster..`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Split {
    /// End of the onset.
    pub(crate) onset: usize,
    /// End of the vowel cluster.
    pub(crate) cluster: usize,
}

use std::sync::OnceLock;

/// Rhymes: vowel cluster and its allowed codas (`""` = open syllable).
const MAIN_RHYMES: &[(&str, &[&str])] = &[
    ("a", &["", "c", "ch", "m", "n", "ng", "nh", "p", "t"]),
    ("ă", &["c", "m", "n", "ng", "p", "t"]),
    ("â", &["c", "m", "n", "ng", "p", "t"]),
    ("e", &["", "c", "m", "n", "ng", "p", "t"]),
    ("ê", &["", "ch", "m", "n", "nh", "p", "t"]),
    ("i", &["", "ch", "m", "n", "nh", "p", "t"]),
    ("o", &["", "c", "m", "n", "ng", "p", "t"]),
    ("ô", &["", "c", "m", "n", "ng", "p", "t"]),
    ("ơ", &["", "m", "n", "p", "t"]),
    ("u", &["", "c", "m", "n", "ng", "p", "t"]),
    ("ư", &["", "c", "m", "n", "ng", "t"]),
    ("y", &[""]),
    ("ai", &[""]),
    ("ao", &[""]),
    ("au", &[""]),
    ("ay", &[""]),
    ("âu", &[""]),
    ("ây", &[""]),
    ("eo", &[""]),
    ("êu", &[""]),
    ("iu", &[""]),
    ("oi", &[""]),
    ("ôi", &[""]),
    ("ơi", &[""]),
    ("ui", &[""]),
    ("ưi", &[""]),
    ("ưu", &[""]),
    ("ia", &[""]),
    ("ua", &[""]),
    ("ưa", &[""]),
    ("uơ", &[""]),
    ("iê", &["c", "m", "n", "ng", "p", "t"]),
    ("yê", &["m", "n", "ng", "t"]),
    ("uâ", &["n", "ng", "t"]),
    ("uê", &["", "ch", "n", "nh"]),
    ("uy", &["", "ch", "n", "nh", "p", "t"]),
    ("uô", &["c", "m", "n", "ng", "t"]),
    ("ươ", &["c", "m", "n", "ng", "p", "t"]),
    ("oa", &["", "c", "ch", "m", "n", "ng", "nh", "p", "t"]),
    ("oă", &["c", "m", "n", "ng", "t"]),
    ("oe", &["", "n", "t"]),
    ("oo", &["c", "ng"]),
    ("uyê", &["n", "t"]),
    ("iêu", &[""]),
    ("yêu", &[""]),
    ("oai", &[""]),
    ("oay", &[""]),
    ("oao", &[""]),
    ("oeo", &[""]),
    ("uây", &[""]),
    ("uôi", &[""]),
    ("ươi", &[""]),
    ("ươu", &[""]),
    ("uyu", &[""]),
    ("uya", &[""]),
];

/// Rhymes after the `qu` onset (the `u` belongs to the onset).
///
/// Plan §3.4 plus real rhymes the plan's table omits: `ăp` (co quắp, quặp), `êt` (quết trầu,
/// quệt), `êu` (quều quào), and `au`, `o` (quạu quọ).
const AFTER_QU_RHYMES: &[(&str, &[&str])] = &[
    ("a", &["", "c", "ch", "m", "n", "ng", "nh", "t"]),
    ("ă", &["c", "m", "n", "ng", "p", "t"]),
    ("â", &["n", "ng", "t"]),
    ("e", &["", "n", "t"]),
    ("ê", &["", "ch", "n", "nh", "t"]),
    ("i", &["", "ch", "n", "nh", "t"]),
    ("y", &["", "ch", "n", "nh", "t"]),
    ("yê", &["n", "t"]),
    ("o", &[""]),
    ("ơ", &[""]),
    ("ô", &["c"]),
    ("ai", &[""]),
    ("ay", &[""]),
    ("ao", &[""]),
    ("au", &[""]),
    ("ây", &[""]),
    ("eo", &[""]),
    ("êu", &[""]),
];

/// Onsets combined with [`MAIN_RHYMES`]; `qu` and `gi` have their own rules.
const PLAIN_ONSETS: &[&str] = &[
    "", "b", "c", "ch", "d", "đ", "g", "gh", "h", "k", "kh", "l", "m", "n", "ng", "ngh", "nh", "p",
    "ph", "r", "s", "t", "th", "tr", "v", "x",
];

/// Onsets tried by [`split`], longest first.
const SPLIT_ONSETS: &[&str] = &[
    "ngh", "ch", "gh", "gi", "kh", "ng", "nh", "ph", "qu", "th", "tr", "b", "c", "d", "đ", "g",
    "h", "k", "l", "m", "n", "p", "r", "s", "t", "v", "x",
];

/// True for the twelve Vietnamese vowels.
pub(crate) fn is_vowel(c: char) -> bool {
    matches!(
        c,
        'a' | 'ă' | 'â' | 'e' | 'ê' | 'i' | 'o' | 'ô' | 'ơ' | 'u' | 'ư' | 'y'
    )
}

/// True for vowels that carry a mark (`ă â ê ô ơ ư`).
pub(crate) fn is_marked_vowel(c: char) -> bool {
    matches!(c, 'ă' | 'â' | 'ê' | 'ô' | 'ơ' | 'ư')
}

struct Table {
    list: Vec<Vec<char>>,
    max_len: usize,
}

fn table() -> &'static Table {
    static TABLE: OnceLock<Table> = OnceLock::new();
    TABLE.get_or_init(build_table)
}

/// Orthographic onset constraints (plan §3.4): `k gh ngh` only before `e ê i y`; `c ng`
/// never before them; plain `g` never before `e ê y` (written `gh`), while `g` + `i` is the
/// merged `gi` onset.
fn onset_allows(onset: &str, cluster: &str) -> bool {
    let front = cluster.starts_with(['e', 'ê', 'i', 'y']);
    match onset {
        "k" | "gh" | "ngh" => front,
        "c" | "ng" => !front,
        "g" => !cluster.starts_with(['e', 'ê', 'y']),
        _ => true,
    }
}

fn build_table() -> Table {
    let mut list: Vec<Vec<char>> = Vec::new();
    let mut add = |onset: &str, cluster: &str, coda: &str| {
        list.push(
            onset
                .chars()
                .chain(cluster.chars())
                .chain(coda.chars())
                .collect(),
        );
    };
    for onset in PLAIN_ONSETS {
        for (cluster, codas) in MAIN_RHYMES {
            if onset_allows(onset, cluster) {
                for coda in *codas {
                    add(onset, cluster, coda);
                }
            }
        }
    }
    for (cluster, codas) in AFTER_QU_RHYMES {
        for coda in *codas {
            add("qu", cluster, coda);
        }
    }
    for (cluster, codas) in MAIN_RHYMES {
        if !cluster.starts_with(['i', 'y']) {
            for coda in *codas {
                add("gi", cluster, coda);
            }
        }
    }
    list.sort();
    list.dedup();
    let max_len = list.iter().map(Vec::len).max().unwrap_or(0);
    Table { list, max_len }
}

/// Every syllable, sorted and unique.
#[cfg(test)]
pub(crate) fn syllables() -> &'static [Vec<char>] {
    &table().list
}

/// Classifies `letters`.
///
/// `Complete` iff `letters` is a syllable. `Prefix` iff some syllable starts with `letters`
/// after adding marks that later keys may still type: an unmarked `a e o u d` also matches
/// `ă â`, `ê`, `ô ơ`, `ư`, `đ` (so `ưo` → `ươ`, `tieng` → `tiêng`). Existing marks must match.
pub(crate) fn classify(letters: &[char]) -> Class {
    let table = table();
    if table
        .list
        .binary_search_by(|syllable| syllable.as_slice().cmp(letters))
        .is_ok()
    {
        return Class::Complete;
    }
    if letters.len() <= table.max_len && prefix_matches(&table.list, letters, 0) {
        Class::Prefix
    } else {
        Class::Invalid
    }
}

/// Mark variants an unmarked letter may still become.
fn variants(c: char) -> ([char; 3], usize) {
    match c {
        'a' => (['a', 'ă', 'â'], 3),
        'e' => (['e', 'ê', 'e'], 2),
        'o' => (['o', 'ô', 'ơ'], 3),
        'u' => (['u', 'ư', 'u'], 2),
        'd' => (['d', 'đ', 'd'], 2),
        other => ([other; 3], 1),
    }
}

/// `table` holds the syllables whose first `depth` letters match `word`; checks the rest.
fn prefix_matches(table: &[Vec<char>], word: &[char], depth: usize) -> bool {
    let Some(&letter) = word.get(depth) else {
        return !table.is_empty();
    };
    let (candidates, count) = variants(letter);
    candidates.iter().take(count).any(|&candidate| {
        let narrowed = narrow(table, depth, candidate);
        !narrowed.is_empty() && prefix_matches(narrowed, word, depth + 1)
    })
}

/// The sub-slice of sorted `table` whose letter at `depth` is `letter`.
fn narrow(table: &[Vec<char>], depth: usize, letter: char) -> &[Vec<char>] {
    let start = table.partition_point(|s| s.get(depth).is_none_or(|&c| c < letter));
    let end = table.partition_point(|s| s.get(depth).is_none_or(|&c| c <= letter));
    table.get(start..end).unwrap_or_default()
}

/// Length of the longest onset at the start of `symbols` (`qu`/`gi` only before a vowel).
fn onset_len(symbols: &[char]) -> usize {
    for onset in SPLIT_ONSETS {
        let len = onset.chars().count();
        let matches = symbols.len() >= len && onset.chars().zip(symbols).all(|(a, &b)| a == b);
        if !matches {
            continue;
        }
        let needs_vowel = matches!(*onset, "qu" | "gi");
        if needs_vowel && !symbols.get(len).is_some_and(|&c| is_vowel(c)) {
            continue;
        }
        return len;
    }
    0
}

/// Splits `symbols` into onset, vowel cluster, and coda.
pub(crate) fn split(symbols: &[char]) -> Split {
    let onset = onset_len(symbols);
    let vowels = symbols
        .get(onset..)
        .unwrap_or_default()
        .iter()
        .take_while(|&&c| is_vowel(c))
        .count();
    Split {
        onset,
        cluster: onset + vowels,
    }
}

/// The codas a syllable may end with, and their prefixes (`""`: an open syllable).
const SHAPE_CODAS: &[&[char]] = &[
    &[],
    &['c'],
    &['c', 'h'],
    &['m'],
    &['n'],
    &['n', 'g'],
    &['n', 'h'],
    &['p'],
    &['t'],
];

/// The structure check (D52), which applies with or without spell checking: `letters` (all
/// Vietnamese letters) are consonants, then at most one group of vowels, then one of the
/// codas `c ch m n ng nh p t` or none. Neither the consonants before the vowels nor the
/// vowels of the group are checked here (spell checking checks both); a mark still needs an
/// onset that [`split`] knows, since it looks for its vowel there.
pub(crate) fn has_shape(letters: &[char]) -> bool {
    let onset = letters.iter().take_while(|&&c| !is_vowel(c)).count();
    let rest = letters.get(onset..).unwrap_or_default();
    let vowels = rest.iter().take_while(|&&c| is_vowel(c)).count();
    let coda = rest.get(vowels..).unwrap_or_default();
    SHAPE_CODAS.contains(&coda)
}

/// Every rhyme of the syllable list (a vowel cluster and a coda that goes with it, after any
/// onset or after `qu`), sorted and unique.
fn rhymes() -> &'static [Vec<char>] {
    static RHYMES: OnceLock<Vec<Vec<char>>> = OnceLock::new();
    RHYMES.get_or_init(|| {
        let mut list: Vec<Vec<char>> = MAIN_RHYMES
            .iter()
            .chain(AFTER_QU_RHYMES)
            .flat_map(|(cluster, codas)| {
                codas
                    .iter()
                    .map(move |coda| cluster.chars().chain(coda.chars()).collect())
            })
            .collect();
        list.sort();
        list.dedup();
        list
    })
}

/// The end of a word without spell checking (D52): after its onset (the consonants it starts
/// with, or `gi` or `qu` before a vowel) comes a rhyme of the syllable list, and the tone fits
/// the coda ([`tone_fits_coda`]). The onset itself is not checked: spell checking does that.
pub(crate) fn has_complete_rhyme(letters: &[char], tone: Option<Tone>) -> bool {
    let leading = letters.iter().take_while(|&&c| !is_vowel(c)).count();
    let merged = matches!(
        letters,
        ['g', 'i', next, ..] | ['q', 'u', next, ..] if is_vowel(*next)
    );
    let starts = [Some(leading), merged.then_some(2)];
    let rhymes = rhymes();
    let rhyme_found = starts.into_iter().flatten().any(|start| {
        letters.get(start..).is_some_and(|rhyme| {
            !rhyme.is_empty()
                && rhymes
                    .binary_search_by(|known| known.as_slice().cmp(rhyme))
                    .is_ok()
        })
    });
    rhyme_found && tone_fits_coda(letters, tone)
}

/// Tone/coda rule for complete words: codas `c ch p t` allow only acute, dot, or no tone.
pub(crate) fn tone_fits_coda(letters: &[char], tone: Option<Tone>) -> bool {
    let coda = letters.get(split(letters).cluster..).unwrap_or_default();
    let stop = matches!(coda, ['c'] | ['c', 'h'] | ['p'] | ['t']);
    !stop || matches!(tone, None | Some(Tone::Acute | Tone::Dot))
}

/// Index of the vowel that carries the tone (plan §3.5 rules 1-3). Clusters longer than three
/// vowels fall back to their last vowel; `None` when there is no vowel cluster.
pub(crate) fn tone_position(symbols: &[char], modern: bool) -> Option<usize> {
    let Split { onset, cluster } = split(symbols);
    let vowels = symbols.get(onset..cluster).unwrap_or_default();
    if vowels.is_empty() {
        return None;
    }
    if let Some(marked) = vowels.iter().rposition(|&c| is_marked_vowel(c)) {
        return Some(onset + marked);
    }
    if cluster < symbols.len() {
        return Some(cluster - 1);
    }
    match vowels {
        [_] => Some(onset),
        ['o', 'a'] | ['o', 'e'] | ['u', 'y'] if modern => Some(onset + 1),
        [_, _] => Some(onset),
        [_, _, _] => Some(onset + 1),
        _ => Some(cluster - 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chars(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn class(text: &str) -> Class {
        classify(&chars(text))
    }

    #[test]
    fn syllable_list_is_sorted_unique_and_contains_known_words() {
        let list = syllables();
        assert!(list.len() > 2000, "{} syllables", list.len());
        assert!(list.windows(2).all(|w| w[0] < w[1]), "sorted and unique");
        for word in [
            "tiêng", "viêt", "ngươi", "thuơ", "huơ", "trương", "rươu", "khuyên", "qua", "quy",
            "quyêt", "gia", "gi", "gin", "giưa", "giương", "giêng", "đi", "đây", "hoa", "thuy",
            "khoe", "hoan", "chao", "cua", "muôn", "nghiêng", "ư", "tư", "như", "a", "anh", "oanh",
            "khuya", "khuyu", "ngoai", "xoay", "khuây", "quôc", "ke", "ky", "ghi", "nghe", "xoong",
            "yêu", "uyên", "ươn", "oăn", "quên", "quynh",
        ] {
            assert_eq!(class(word), Class::Complete, "{word}");
        }
    }

    #[test]
    fn classify_distinguishes_prefixes_and_invalid_words() {
        for prefix in [
            "", "n", "ng", "ngh", "q", "qu", "tiê", "đ", "uyê", "kh", "gh", "ă", "â", "quô",
        ] {
            assert_eq!(class(prefix), Class::Prefix, "{prefix:?}");
        }
        for invalid in [
            "cl", "ate", "ưin", "đđ", "ka", "ko", "ce", "ci", "nge", "ngi", "gha", "ngha", "qa",
            "quâc", "queng", "quu", "quoa", "bb", "ăi", "ơa",
        ] {
            assert_eq!(class(invalid), Class::Invalid, "{invalid:?}");
        }
        assert!(Class::Prefix.is_valid() && Class::Complete.is_valid());
        assert!(!Class::Invalid.is_valid());
    }

    #[test]
    fn prefixes_may_still_receive_marks() {
        // Marks are typed after the letters (free marking, `nguwowif`, `tieengs`).
        for text in [
            "ưo", "tieng", "duong", "nguo", "truo", "khuye", "quye", "giuo",
        ] {
            assert_eq!(class(text), Class::Prefix, "{text:?}");
        }
        // Existing marks must match exactly.
        assert_eq!(class("ăo"), Class::Invalid);
    }

    #[test]
    fn after_qu_table() {
        for word in [
            "qua", "quac", "quach", "quan", "quang", "quanh", "quat", "quăc", "quăm", "quăn",
            "quăng", "quăp", "quăt", "quân", "quâng", "quât", "que", "quen", "quet", "quê",
            "quêch", "quên", "quênh", "quêu", "qui", "quich", "quin", "quinh", "quit", "quy",
            "quych", "quyn", "quynh", "quyt", "quyên", "quyêt", "quơ", "quôc", "quai", "quay",
            "quao", "quây", "queo",
        ] {
            assert_eq!(class(word), Class::Complete, "{word}");
        }
        // Real words the plan's table omits: quết trầu, quệt, quạu quọ.
        for word in ["quêt", "quau", "quo"] {
            assert_eq!(class(word), Class::Complete, "{word}");
        }
        for word in [
            "quap", "quânh", "quem", "quêm", "quim", "quơn", "quôn", "quêun", "quău", "quêp",
            "quoc", "quot", "quaun",
        ] {
            assert_ne!(class(word), Class::Complete, "{word}");
        }
    }

    #[test]
    fn plain_g_never_comes_before_e_ê_or_y() {
        // `gh` is written before e ê i; `g` + i is the merged `gi` onset (gì, giếng, giẻ).
        for invalid in ["ge", "gê", "gy", "gen", "gêm", "gyêu", "get"] {
            assert_eq!(class(invalid), Class::Invalid, "{invalid}");
        }
        for word in [
            "ghe", "ghê", "ghi", "gi", "gin", "gie", "giêng", "giêt", "ga", "go", "gơi", "gưi",
        ] {
            assert_eq!(class(word), Class::Complete, "{word}");
        }
        assert_eq!(class("g"), Class::Prefix);
        assert_eq!(class("gh"), Class::Prefix);
    }

    #[test]
    fn split_rules() {
        let cases = [
            ("tiêng", 1, 3),
            ("nghiêng", 3, 5),
            ("qua", 2, 3),
            ("quyêt", 2, 4),
            ("qu", 0, 0),
            ("gia", 2, 3),
            ("gi", 1, 2),
            ("gin", 1, 2),
            ("giêng", 2, 3),
            ("giưa", 2, 4),
            ("a", 0, 1),
            ("anh", 0, 1),
            ("ươi", 0, 3),
            ("ngươi", 2, 5),
            ("cla", 1, 1),
            ("ate", 0, 1),
            ("đi", 1, 2),
            ("khuyên", 2, 5),
            ("ngh", 3, 3),
            ("", 0, 0),
        ];
        for (text, onset, cluster) in cases {
            assert_eq!(split(&chars(text)), Split { onset, cluster }, "{text:?}");
        }
    }

    #[test]
    fn tone_coda_rule() {
        let letters = chars("tac");
        assert!(tone_fits_coda(&letters, None));
        assert!(tone_fits_coda(&letters, Some(Tone::Acute)));
        assert!(tone_fits_coda(&letters, Some(Tone::Dot)));
        assert!(!tone_fits_coda(&letters, Some(Tone::Grave)));
        assert!(!tone_fits_coda(&chars("sach"), Some(Tone::Hook)));
        assert!(!tone_fits_coda(&chars("tap"), Some(Tone::Tilde)));
        assert!(!tone_fits_coda(&chars("viêt"), Some(Tone::Grave)));
        assert!(tone_fits_coda(&chars("tan"), Some(Tone::Grave)));
        assert!(tone_fits_coda(&chars("hoa"), Some(Tone::Tilde)));
    }

    #[test]
    fn tone_placement_rules() {
        let both = [
            ("hoan", 2),
            ("chao", 2),
            ("cua", 1),
            ("mia", 1),
            ("mai", 1),
            ("ngoai", 3),
            ("tiêng", 2),
            ("ngươi", 3),
            ("thuơ", 3),
            ("rươu", 2),
            ("khuyên", 4),
            ("qua", 2),
            ("quy", 2),
            ("quyêt", 3),
            ("gia", 2),
            ("gi", 1),
            ("giưa", 2),
            ("giương", 3),
            ("muôn", 2),
            ("toan", 2),
            ("khuya", 3),
            ("khuây", 3),
            ("uơ", 1),
            ("ưo", 0),
            ("xoong", 2),
            ("a", 0),
            ("oaoa", 3),
        ];
        for (text, position) in both {
            for modern in [false, true] {
                assert_eq!(
                    tone_position(&chars(text), modern),
                    Some(position),
                    "{text} modern={modern}"
                );
            }
        }
        for (text, old, new) in [("hoa", 1, 2), ("thuy", 2, 3), ("khoe", 2, 3)] {
            assert_eq!(tone_position(&chars(text), false), Some(old), "{text}");
            assert_eq!(tone_position(&chars(text), true), Some(new), "{text}");
        }
        for text in ["qu", "ngh", "", "b"] {
            assert_eq!(tone_position(&chars(text), false), None, "{text:?}");
        }
    }
}
