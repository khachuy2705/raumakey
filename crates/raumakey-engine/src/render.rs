//! Output normalization (plan §3.7).

use raumakey_model::options::Normalization;
use raumakey_model::utf16;
use unicode_normalization::UnicodeNormalization;

use crate::word::Word;

/// Applies `normalization` to `text`.
pub(crate) fn normalize(text: &str, normalization: Normalization) -> String {
    match normalization {
        Normalization::Nfc => text.nfc().collect(),
        Normalization::Nfd => text.nfd().collect(),
        Normalization::Preserve => text.to_owned(),
    }
}

/// Renders `word` as UTF-16 with `normalization`.
pub(crate) fn render(word: &Word, normalization: Normalization) -> Vec<u16> {
    utf16::encode(&normalize(&word.render_text(), normalization))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nfc_composes_nfd_decomposes_preserve_keeps() {
        let decomposed = "vie\u{302}\u{323}t";
        assert_eq!(normalize(decomposed, Normalization::Nfc), "việt");
        assert_eq!(normalize("việt", Normalization::Nfd), "vie\u{323}\u{302}t");
        assert_eq!(normalize(decomposed, Normalization::Preserve), decomposed);
        assert_eq!(normalize("việt", Normalization::Preserve), "việt");
    }
}
