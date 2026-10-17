//! The word model and key action application (plan §3.3, §3.5, §3.6).

use std::ops::Range;

use raumakey_model::keymap::{BaseVowel, KeyAction, Tone};

use crate::WordForm;
use crate::letters::{Item, Letter, Mark};
use crate::syllable::{self, Class};

/// Placeholder symbol for `Other` items: neither a vowel nor an onset letter.
const OTHER_SYMBOL: char = '\u{FFFC}';

/// Options that shape action application.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Ctx {
    /// Gate transformations on Vietnamese spelling.
    pub(crate) spell_check: bool,
    /// Allow marks anywhere after their target vowel.
    pub(crate) free_marking: bool,
    /// New tone placement.
    pub(crate) modern_tone: bool,
}

/// The current word: items, one word-level tone, and typing state.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Word {
    /// Letters and other characters in typing order.
    pub(crate) items: Vec<Item>,
    /// Word tone and the index of the item that carries it.
    pub(crate) tone: Option<(Tone, usize)>,
    /// After an undo: every further key of the word is literal.
    pub(crate) literal_mode: bool,
    /// VIQR escape pending: the last item is the escape key.
    pub(crate) escape_armed: bool,
    /// Raw keys typed for this word, as typed.
    pub(crate) raw: Vec<char>,
}

/// What a key did to the word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum KeyOutcome {
    /// The key changed the word.
    Changed(Word),
    /// The key is boundary punctuation that no action consumed.
    Boundary,
    /// An escaped key that is boundary punctuation: it replaces the escape and ends the word.
    /// The payload is the word without the escape, committed unchanged (no macro, no
    /// restore; matrix #8, #49).
    EscapedBoundary(Word),
}

/// Result of one candidate action.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    /// The action transformed the word.
    Applied(Word),
    /// The action reverted an earlier transformation and appended the key literally.
    Undo(Word),
    /// The action does not apply.
    NotApplicable,
}

/// Boundary punctuation (plan §3.8).
pub(crate) fn is_boundary_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | ','
            | ';'
            | ':'
            | '!'
            | '?'
            | '"'
            | '\''
            | '('
            | ')'
            | '['
            | ']'
            | '{'
            | '}'
            | '<'
            | '>'
            | '/'
            | '\\'
            | '|'
    )
}

impl Word {
    /// True when the word has no items.
    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Lowercase symbols of every item; `Other` items map to a non-letter placeholder.
    pub(crate) fn symbols(&self) -> Vec<char> {
        self.items
            .iter()
            .map(|item| item.letter().map_or(OTHER_SYMBOL, Letter::symbol))
            .collect()
    }

    /// Vietnamese letters of the word, or `None` when it contains anything else.
    fn viet_letters(&self) -> Option<Vec<char>> {
        self.items.iter().map(Item::viet_symbol).collect()
    }

    /// Classification of the word (`Invalid` when it contains non-Vietnamese items).
    pub(crate) fn class(&self) -> Class {
        self.viet_letters()
            .map_or(Class::Invalid, |letters| syllable::classify(&letters))
    }

    /// True when the rendering carries a tone or a mark.
    pub(crate) fn transformed(&self) -> bool {
        self.tone.is_some()
            || self
                .items
                .iter()
                .any(|item| item.letter().is_some_and(|l| l.mark != Mark::None))
    }

    /// True for a complete Vietnamese syllable that obeys the tone/coda rule.
    pub(crate) fn is_complete_syllable(&self) -> bool {
        self.viet_letters().is_some_and(|letters| {
            syllable::classify(&letters) == Class::Complete
                && syllable::tone_fits_coda(&letters, self.tone.map(|(tone, _)| tone))
        })
    }

    /// Structural form of the word.
    pub(crate) fn word_form(&self) -> WordForm {
        if self.items.is_empty() {
            return WordForm::Empty;
        }
        let Some(letters) = self.viet_letters() else {
            return WordForm::NonVietnamese;
        };
        if !syllable::classify(&letters).is_valid() {
            return WordForm::NonVietnamese;
        }
        let split = syllable::split(&letters);
        let onset = split.onset > 0;
        let vowel = split.cluster > split.onset;
        let coda = letters.len() > split.cluster;
        match (onset, vowel, coda) {
            (_, false, _) => WordForm::C,
            (false, true, false) => WordForm::V,
            (true, true, false) => WordForm::CV,
            (false, true, true) => WordForm::VC,
            (true, true, true) => WordForm::CVC,
        }
    }

    /// Precomposed rendering (before output normalization).
    pub(crate) fn render_text(&self) -> String {
        self.items
            .iter()
            .enumerate()
            .map(|(index, item)| match item {
                Item::Letter(letter) => letter.render(
                    self.tone
                        .and_then(|(tone, at)| (at == index).then_some(tone)),
                ),
                Item::Other(c) => *c,
            })
            .collect()
    }

    /// The raw keys as text.
    pub(crate) fn raw_text(&self) -> String {
        self.raw.iter().collect()
    }

    /// Appends `key` literally without any transformation (disabled-mode tracking).
    pub(crate) fn push_plain(&mut self, key: char) {
        self.items.push(Item::literal(key));
        self.raw.push(key);
    }

    /// Index range of the vowel cluster.
    fn cluster(&self) -> Range<usize> {
        let split = syllable::split(&self.symbols());
        split.onset..split.cluster
    }

    fn letter_at(&self, index: usize) -> Option<&Letter> {
        self.items.get(index).and_then(Item::letter)
    }

    fn base_at(&self, index: usize) -> Option<char> {
        self.letter_at(index).map(Letter::base)
    }

    fn mark_at(&self, index: usize) -> Mark {
        self.letter_at(index).map_or(Mark::None, |l| l.mark)
    }

    /// A copy with `mark` set on the letter at `index`.
    fn with_mark(&self, index: usize, mark: Mark) -> Self {
        let mut next = self.clone();
        next.set_mark(index, mark);
        next
    }

    fn set_mark(&mut self, index: usize, mark: Mark) {
        if let Some(letter) = self.items.get_mut(index).and_then(Item::letter_mut) {
            letter.mark = mark;
            if mark != Mark::Horn {
                letter.from_w = false;
            }
        }
    }

    /// Reverting step of an undo: append the key literally and enter literal mode.
    fn undo_with(mut self, key: char) -> Self {
        self.items.push(Item::literal(key));
        self.literal_mode = true;
        self
    }

    /// Appends a literal key; in normal processing applies the `uơ` fix-up and moves the tone.
    fn append(&mut self, key: char, transform: bool, modern: bool) {
        let item = Item::literal(key);
        if transform
            && let Item::Letter(letter) = item
            && matches!(letter.base(), 'c' | 'm' | 'n' | 'p' | 't' | 'i' | 'u')
        {
            self.fix_u_horn();
        }
        self.items.push(item);
        if transform {
            self.reposition_tone(modern);
        }
    }

    /// `uơ` fix-up: plain `u` + horned `o` at the end (outside `qu`) → `ươ`.
    fn fix_u_horn(&mut self) {
        let len = self.items.len();
        if len < 2 {
            return;
        }
        let (u, o) = (len - 2, len - 1);
        let in_qu = u == 1 && self.base_at(0) == Some('q');
        let plain_u = self.base_at(u) == Some('u') && self.mark_at(u) == Mark::None;
        let horned_o = self.base_at(o) == Some('o') && self.mark_at(o) == Mark::Horn;
        if !in_qu && plain_u && horned_o {
            self.set_mark(u, Mark::Horn);
        }
    }

    /// Recomputes the tone position of a valid word; invalid words keep the frozen position.
    fn reposition_tone(&mut self, modern: bool) {
        let Some((tone, at)) = self.tone else {
            return;
        };
        let symbols = self.symbols();
        if self.class().is_valid()
            && let Some(target) = syllable::tone_position(&symbols, modern)
        {
            self.tone = Some((tone, target));
            return;
        }
        if !self.items.get(at).is_some_and(Item::is_vowel) {
            self.tone = syllable::tone_position(&symbols, modern).map(|target| (tone, target));
        }
    }

    /// True when the word ends inside its vowel cluster (no coda typed yet).
    fn ends_in_cluster(&self) -> bool {
        let cluster = self.cluster();
        !cluster.is_empty() && cluster.end == self.items.len()
    }

    /// True when the word is exactly the onset `d`.
    fn is_bare_d(&self) -> bool {
        self.items.len() == 1 && self.base_at(0) == Some('d')
    }

    /// True when Telex `w` has a vowel to modify. In a word that is exactly `gi` the `i`
    /// belongs to the `gi` onset (`giư`, `giữa`), so there is none.
    fn has_vowel(&self) -> bool {
        let bare_gi =
            self.items.len() == 2 && self.base_at(0) == Some('g') && self.base_at(1) == Some('i');
        !bare_gi && self.items.iter().any(Item::is_vowel)
    }
}

/// Processes one key typed in Vietnamese mode.
///
/// VIQR escape (plan §3.6): after `\`, a key with an action binding is typed as a literal in
/// place of the backslash. An escaped key behaves like the same key without its binding:
/// boundary punctuation (`\.`, `\?`, `\'`, `\(`, `\\`) ends the word, which is committed
/// unchanged; any other key (`\^`, `\-`, `\d`, ...) stays in the word as a literal `Other`.
pub(crate) fn process_key(
    word: &Word,
    key: char,
    actions: Option<&[KeyAction]>,
    ctx: &Ctx,
) -> KeyOutcome {
    let actions = actions.unwrap_or_default();
    if word.escape_armed && !actions.is_empty() {
        let mut next = word.clone();
        next.escape_armed = false;
        if is_boundary_punctuation(key) {
            // The armed escape is the last item and the last raw key.
            next.items.pop();
            next.raw.pop();
            return KeyOutcome::EscapedBoundary(next);
        }
        if let Some(last) = next.items.last_mut() {
            *last = Item::Other(key);
        }
        next.raw.push(key);
        return KeyOutcome::Changed(next);
    }
    let mut base = word.clone();
    base.escape_armed = false;
    if actions.contains(&KeyAction::Escape) {
        base.items.push(Item::Other(key));
        base.escape_armed = true;
        base.raw.push(key);
        return KeyOutcome::Changed(base);
    }
    let literal = base.literal_mode || (ctx.spell_check && !base.class().is_valid());
    if !literal {
        for &action in actions {
            match try_apply(&base, action, key, ctx) {
                Outcome::Applied(mut next) | Outcome::Undo(mut next) => {
                    next.raw.push(key);
                    return KeyOutcome::Changed(next);
                }
                Outcome::NotApplicable => {}
            }
        }
    }
    if is_boundary_punctuation(key) {
        return KeyOutcome::Boundary;
    }
    base.append(key, !literal, ctx.modern_tone);
    base.raw.push(key);
    KeyOutcome::Changed(base)
}

/// Applies one action with the free-marking and spell-check gates.
fn try_apply(word: &Word, action: KeyAction, key: char, ctx: &Ctx) -> Outcome {
    let marks_allowed = ctx.free_marking || word.ends_in_cluster();
    let outcome = match action {
        KeyAction::Tone(tone) if marks_allowed => tone_action(word, tone, key, ctx),
        KeyAction::RemoveTone if marks_allowed => remove_tone(word),
        KeyAction::CircumflexOn(vowel) if marks_allowed => circumflex_on(word, vowel, key),
        KeyAction::Circumflex if marks_allowed => circumflex_any(word, key),
        KeyAction::Breve if marks_allowed => breve(word, key),
        KeyAction::Horn if marks_allowed => horn(word, key),
        KeyAction::HornOrBreve if !word.has_vowel() => standalone_u_horn(word, key),
        KeyAction::HornOrBreve if marks_allowed => horn_or_breve(word, key),
        KeyAction::Stroke if ctx.free_marking || word.is_bare_d() => stroke(word, key),
        _ => Outcome::NotApplicable,
    };
    match outcome {
        Outcome::Applied(mut next) => {
            next.reposition_tone(ctx.modern_tone);
            if ctx.spell_check && !next.class().is_valid() {
                Outcome::NotApplicable
            } else {
                Outcome::Applied(next)
            }
        }
        other => other,
    }
}

fn tone_action(word: &Word, tone: Tone, key: char, ctx: &Ctx) -> Outcome {
    let Some(position) = syllable::tone_position(&word.symbols(), ctx.modern_tone) else {
        return Outcome::NotApplicable;
    };
    match word.tone {
        Some((current, _)) if current == tone => {
            let mut next = word.clone();
            next.tone = None;
            Outcome::Undo(next.undo_with(key))
        }
        Some((_, frozen)) if !word.class().is_valid() => {
            let mut next = word.clone();
            next.tone = Some((tone, frozen));
            Outcome::Applied(next)
        }
        _ => {
            let mut next = word.clone();
            next.tone = Some((tone, position));
            Outcome::Applied(next)
        }
    }
}

fn remove_tone(word: &Word) -> Outcome {
    if word.tone.is_none() {
        return Outcome::NotApplicable;
    }
    let mut next = word.clone();
    next.tone = None;
    Outcome::Applied(next)
}

fn circumflex_on(word: &Word, vowel: BaseVowel, key: char) -> Outcome {
    let base = match vowel {
        BaseVowel::A => 'a',
        BaseVowel::E => 'e',
        BaseVowel::O => 'o',
    };
    let Some(index) = word
        .cluster()
        .rev()
        .find(|&i| word.base_at(i) == Some(base))
    else {
        return Outcome::NotApplicable;
    };
    if word.mark_at(index) == Mark::Circumflex {
        Outcome::Undo(word.with_mark(index, Mark::None).undo_with(key))
    } else {
        Outcome::Applied(word.with_mark(index, Mark::Circumflex))
    }
}

fn circumflex_any(word: &Word, key: char) -> Outcome {
    let candidates: Vec<usize> = word
        .cluster()
        .rev()
        .filter(|&i| matches!(word.base_at(i), Some('a' | 'e' | 'o')))
        .collect();
    if let Some(&index) = candidates
        .iter()
        .find(|&&i| word.mark_at(i) == Mark::Circumflex)
    {
        return Outcome::Undo(word.with_mark(index, Mark::None).undo_with(key));
    }
    pick(
        candidates
            .iter()
            .map(|&i| word.with_mark(i, Mark::Circumflex)),
    )
}

/// The first candidate that is a valid prefix, else the first candidate (the spell-check gate
/// rejects it when spell checking is on).
fn pick(candidates: impl Iterator<Item = Word>) -> Outcome {
    let mut fallback = None;
    for candidate in candidates {
        if candidate.class().is_valid() {
            return Outcome::Applied(candidate);
        }
        fallback.get_or_insert(candidate);
    }
    fallback.map_or(Outcome::NotApplicable, Outcome::Applied)
}

fn breve(word: &Word, key: char) -> Outcome {
    let Some(index) = word.cluster().rev().find(|&i| word.base_at(i) == Some('a')) else {
        return Outcome::NotApplicable;
    };
    if word.mark_at(index) == Mark::Breve {
        Outcome::Undo(word.with_mark(index, Mark::None).undo_with(key))
    } else {
        Outcome::Applied(word.with_mark(index, Mark::Breve))
    }
}

fn horn(word: &Word, key: char) -> Outcome {
    let cluster = word.cluster();
    let pair = cluster.clone().find(|&u| {
        u + 1 < cluster.end && word.base_at(u) == Some('u') && word.base_at(u + 1) == Some('o')
    });
    if let Some(u) = pair {
        let o = u + 1;
        let u_horned = word.mark_at(u) == Mark::Horn;
        let o_horned = word.mark_at(o) == Mark::Horn;
        let followed = o + 1 < word.items.len();
        if (u_horned && o_horned) || (o_horned && !followed) {
            let mut next = word.clone();
            for index in [u, o] {
                if next.mark_at(index) == Mark::Horn {
                    next.set_mark(index, Mark::None);
                }
            }
            return Outcome::Undo(next.undo_with(key));
        }
        let mut next = word.with_mark(o, Mark::Horn);
        if followed {
            next.set_mark(u, Mark::Horn);
        }
        return Outcome::Applied(next);
    }
    let mut fallback = None;
    for index in cluster.rev() {
        if !matches!(word.base_at(index), Some('u' | 'o')) {
            continue;
        }
        if word.mark_at(index) == Mark::Horn {
            return Outcome::Undo(remove_horn(word, index).undo_with(key));
        }
        let candidate = word.with_mark(index, Mark::Horn);
        if candidate.class().is_valid() {
            return Outcome::Applied(candidate);
        }
        fallback.get_or_insert(candidate);
    }
    fallback.map_or(Outcome::NotApplicable, Outcome::Applied)
}

/// Reverts the horn at `index`; a standalone `ư` (Telex `w`) is removed entirely.
fn remove_horn(word: &Word, index: usize) -> Word {
    let standalone = word.letter_at(index).is_some_and(|l| l.from_w);
    if !standalone {
        return word.with_mark(index, Mark::None);
    }
    let mut next = word.clone();
    if index < next.items.len() {
        next.items.remove(index);
    }
    next.tone = match next.tone {
        Some((_, at)) if at == index => None,
        Some((tone, at)) if at > index => Some((tone, at - 1)),
        other => other,
    };
    next
}

fn standalone_u_horn(word: &Word, key: char) -> Outcome {
    let mut next = word.clone();
    next.items
        .push(Item::Letter(Letter::standalone_u_horn(key)));
    Outcome::Applied(next)
}

fn horn_or_breve(word: &Word, key: char) -> Outcome {
    let horned = horn(word, key);
    match &horned {
        Outcome::Undo(_) => return horned,
        Outcome::Applied(next) if next.class().is_valid() => return horned,
        _ => {}
    }
    let breved = breve(word, key);
    match &breved {
        Outcome::Undo(_) => return breved,
        Outcome::Applied(next) if next.class().is_valid() => return breved,
        _ => {}
    }
    if matches!(horned, Outcome::Applied(_)) {
        horned
    } else {
        breved
    }
}

fn stroke(word: &Word, key: char) -> Outcome {
    if word.base_at(0) != Some('d') {
        return Outcome::NotApplicable;
    }
    if word.mark_at(0) == Mark::Stroke {
        Outcome::Undo(word.with_mark(0, Mark::None).undo_with(key))
    } else {
        Outcome::Applied(word.with_mark(0, Mark::Stroke))
    }
}

#[cfg(test)]
mod tests {
    use raumakey_model::options::InputMethod;

    use super::*;
    use crate::keymaps::builtin;

    const DEFAULT: Ctx = Ctx {
        spell_check: true,
        free_marking: true,
        modern_tone: false,
    };

    /// Feeds `keys` and returns the rendered word; `|` marks a boundary in the output.
    fn typed_with(keys: &str, method: InputMethod, ctx: Ctx) -> String {
        let map = builtin(method).expect("built-in method");
        let mut word = Word::default();
        let mut out = String::new();
        for key in keys.chars() {
            match process_key(&word, key, map.lookup(key), &ctx) {
                KeyOutcome::Changed(next) => word = next,
                KeyOutcome::Boundary => {
                    out.push_str(&word.render_text());
                    out.push('|');
                    out.push(key);
                    word = Word::default();
                }
                KeyOutcome::EscapedBoundary(committed) => {
                    out.push_str(&committed.render_text());
                    out.push('|');
                    out.push(key);
                    word = Word::default();
                }
            }
        }
        out.push_str(&word.render_text());
        out
    }

    fn telex(keys: &str) -> String {
        typed_with(keys, InputMethod::Telex, DEFAULT)
    }

    #[test]
    fn telex_marks_tones_and_undo() {
        let cases = [
            ("aa", "â"),
            ("dd", "đ"),
            ("as", "á"),
            ("aw", "ă"),
            ("ow", "ơ"),
            ("uw", "ư"),
            ("w", "ư"),
            ("W", "Ư"),
            ("ww", "w"),
            ("tw", "tư"),
            ("nhw", "như"),
            ("ass", "as"),
            ("asss", "ass"),
            ("aaa", "aa"),
            ("ddd", "dd"),
            ("awa", "â"),
            ("aww", "aw"),
            ("oww", "ow"),
            ("uww", "uw"),
            ("asz", "a"),
            ("az", "az"),
            ("Aa", "Â"),
            ("aA", "â"),
            ("DD", "Đ"),
            ("Dd", "Đ"),
            ("AS", "Á"),
        ];
        for (keys, expected) in cases {
            assert_eq!(telex(keys), expected, "{keys}");
        }
    }

    #[test]
    fn horn_pairs_and_uo_fix_up() {
        let cases = [
            ("uow", "uơ"),
            ("huow", "huơ"),
            ("thuowr", "thuở"),
            ("huowng", "hương"),
            ("huowi", "hươi"),
            ("ruowuj", "rượu"),
            ("uoiw", "ươi"),
            ("uonw", "ươn"),
            ("uwow", "ươ"),
            ("huoww", "huow"),
            ("nguwowif", "người"),
            ("nguoiwf", "người"),
            ("truowngf", "trường"),
            ("quow", "quơ"),
            ("quown", "quơn"),
        ];
        for (keys, expected) in cases {
            assert_eq!(telex(keys), expected, "{keys}");
        }
    }

    #[test]
    fn w_after_the_gi_onset_is_a_standalone_u_horn() {
        // In `giư` the `i` belongs to the `gi` onset, so `w` has no vowel to modify.
        assert_eq!(telex("giw"), "giư");
        assert_eq!(telex("giwx"), "giữ");
        assert_eq!(telex("Giwax"), "Giữa");
        assert_eq!(telex("GIWX"), "GIỮ");
        assert_eq!(telex("gifw"), "giừ");
        assert_eq!(telex("kiw"), "kiw");
        assert_eq!(telex("giaw"), "giă");
    }

    #[test]
    fn spell_check_gates_transformations() {
        assert_eq!(telex("kw"), "kw");
        assert_eq!(telex("cls"), "cls");
        assert_eq!(telex("after"), "àter");
        let off = Ctx {
            spell_check: false,
            ..DEFAULT
        };
        assert_eq!(typed_with("kw", InputMethod::Telex, off), "kư");
        assert_eq!(typed_with("oaw", InputMethod::Telex, off), "oă");
    }

    #[test]
    fn free_marking_off_requires_marks_inside_the_cluster() {
        let off = Ctx {
            free_marking: false,
            ..DEFAULT
        };
        assert_eq!(typed_with("did", InputMethod::Telex, off), "did");
        assert_eq!(typed_with("ddi", InputMethod::Telex, off), "đi");
        assert_eq!(typed_with("tiengs", InputMethod::Telex, off), "tiengs");
        assert_eq!(typed_with("tieesng", InputMethod::Telex, off), "tiếng");
        assert_eq!(telex("did"), "đi");
        assert_eq!(telex("tiengse"), "tiếng");
    }

    #[test]
    fn tone_moves_with_the_word() {
        assert_eq!(telex("hoaf"), "hòa");
        assert_eq!(telex("hoafn"), "hoàn");
        let modern = Ctx {
            modern_tone: true,
            ..DEFAULT
        };
        assert_eq!(typed_with("hoaf", InputMethod::Telex, modern), "hoà");
        assert_eq!(typed_with("thuyr", InputMethod::Telex, modern), "thuỷ");
        assert_eq!(telex("gifa"), "già");
    }

    #[test]
    fn vni_and_viqr_actions() {
        let vni = |keys| typed_with(keys, InputMethod::Vni, DEFAULT);
        assert_eq!(vni("a6"), "â");
        assert_eq!(vni("a66"), "a6");
        assert_eq!(vni("a8"), "ă");
        assert_eq!(vni("o7"), "ơ");
        assert_eq!(vni("uo7"), "uơ");
        assert_eq!(vni("nguo72i"), "người");
        assert_eq!(vni("d9"), "đ");
        assert_eq!(vni("a10"), "a");
        let viqr = |keys| typed_with(keys, InputMethod::Viqr, DEFAULT);
        assert_eq!(viqr("a^"), "â");
        assert_eq!(viqr("a("), "ă");
        assert_eq!(viqr("o*"), "ơ");
        assert_eq!(viqr("ngu+o+`i"), "người");
        assert_eq!(viqr("a.-"), "a");
        assert_eq!(viqr("dd"), "đ");
    }

    #[test]
    fn viqr_escape_and_punctuation_boundaries() {
        let viqr = |keys| typed_with(keys, InputMethod::Viqr, DEFAULT);
        // Escaped boundary punctuation replaces the backslash and ends the word.
        assert_eq!(viqr("\\'"), "|'");
        assert_eq!(viqr("a\\."), "a|.");
        assert_eq!(viqr("\\\\"), "|\\");
        assert_eq!(viqr("ba\\("), "ba|(");
        assert_eq!(viqr("\\'a'"), "|'á");
        assert_eq!(viqr("ho.c\\."), "học|.");
        // Any other escaped key replaces the backslash as a literal inside the word.
        assert_eq!(viqr("\\^a'"), "^a|'");
        assert_eq!(viqr("a\\^"), "a^");
        assert_eq!(viqr("\\dd"), "dd");
        assert_eq!(viqr("e\\-mail"), "e-mail");
        // An unbound key disarms the escape and is processed normally.
        assert_eq!(viqr("\\b"), "\\b");
        assert_eq!(viqr("b'"), "b|'");
        assert_eq!(viqr("a''"), "a'");
        assert_eq!(telex("a."), "a|.");
        assert!(is_boundary_punctuation('.'));
        assert!(is_boundary_punctuation('|'));
        assert!(!is_boundary_punctuation('-'));
        assert!(!is_boundary_punctuation('1'));
    }

    #[test]
    fn word_classification_helpers() {
        let map = builtin(InputMethod::Telex).expect("telex");
        let mut word = Word::default();
        for key in "tieengs".chars() {
            if let KeyOutcome::Changed(next) = process_key(&word, key, map.lookup(key), &DEFAULT) {
                word = next;
            }
        }
        assert_eq!(word.render_text(), "tiếng");
        assert_eq!(word.raw_text(), "tieengs");
        assert!(word.transformed());
        assert!(word.is_complete_syllable());
        assert_eq!(word.class(), Class::Complete);
        assert_eq!(word.word_form(), WordForm::CVC);
        let mut plain = Word::default();
        plain.push_plain('t');
        assert_eq!(plain.word_form(), WordForm::C);
        assert!(!plain.transformed());
        plain.push_plain('1');
        assert_eq!(plain.word_form(), WordForm::NonVietnamese);
        assert_eq!(Word::default().word_form(), WordForm::Empty);
    }
}
