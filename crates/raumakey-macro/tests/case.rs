//! Case pattern detection and adaptation (plan §4, matrix D50).

use raumakey_macro::{adapt_case, case_pattern};
use raumakey_model::macros::CasePattern;

#[test]
fn all_lowercase_is_lower() {
    assert_eq!(case_pattern("ko"), CasePattern::Lower);
    assert_eq!(case_pattern("đc"), CasePattern::Lower);
    assert_eq!(case_pattern("việt"), CasePattern::Lower);
}

#[test]
fn text_without_cased_letters_is_lower() {
    assert_eq!(case_pattern(""), CasePattern::Lower);
    assert_eq!(case_pattern("123"), CasePattern::Lower);
    assert_eq!(case_pattern("@#"), CasePattern::Lower);
}

#[test]
fn first_upper_rest_lower_is_title() {
    assert_eq!(case_pattern("Ko"), CasePattern::Title);
    assert_eq!(case_pattern("Đc"), CasePattern::Title);
    assert_eq!(case_pattern("Việt"), CasePattern::Title);
}

#[test]
fn single_upper_letter_is_title() {
    assert_eq!(case_pattern("K"), CasePattern::Title);
    assert_eq!(case_pattern("Đ"), CasePattern::Title);
}

#[test]
fn several_upper_letters_are_upper() {
    assert_eq!(case_pattern("KO"), CasePattern::Upper);
    assert_eq!(case_pattern("ĐC"), CasePattern::Upper);
    assert_eq!(case_pattern("VIỆT"), CasePattern::Upper);
}

#[test]
fn other_shapes_are_mixed() {
    assert_eq!(case_pattern("kO"), CasePattern::Mixed);
    assert_eq!(case_pattern("KoN"), CasePattern::Mixed);
    assert_eq!(case_pattern("đI"), CasePattern::Mixed);
}

#[test]
fn uncased_characters_do_not_count_as_letters() {
    assert_eq!(case_pattern("2K"), CasePattern::Title);
    assert_eq!(case_pattern("k2"), CasePattern::Lower);
    assert_eq!(case_pattern("K2O"), CasePattern::Upper);
    assert_eq!(case_pattern("K-o"), CasePattern::Title);
}

#[test]
fn lower_and_mixed_keep_the_replacement_as_stored() {
    assert_eq!(adapt_case("không", CasePattern::Lower), "không");
    assert_eq!(adapt_case("Việt Nam", CasePattern::Lower), "Việt Nam");
    assert_eq!(adapt_case("không", CasePattern::Mixed), "không");
    assert_eq!(adapt_case("Việt Nam", CasePattern::Mixed), "Việt Nam");
}

#[test]
fn title_uppercases_only_the_first_letter() {
    assert_eq!(adapt_case("không", CasePattern::Title), "Không");
    assert_eq!(adapt_case("đường", CasePattern::Title), "Đường");
    assert_eq!(adapt_case("ở đâu", CasePattern::Title), "Ở đâu");
    assert_eq!(adapt_case("việt Nam", CasePattern::Title), "Việt Nam");
    assert_eq!(adapt_case("Không", CasePattern::Title), "Không");
}

#[test]
fn title_skips_leading_non_letters() {
    assert_eq!(
        adapt_case("\u{201C}không\u{201D}", CasePattern::Title),
        "\u{201C}Không\u{201D}"
    );
    assert_eq!(adapt_case("(ơn)", CasePattern::Title), "(Ơn)");
}

#[test]
fn upper_uppercases_every_letter() {
    assert_eq!(adapt_case("không", CasePattern::Upper), "KHÔNG");
    assert_eq!(adapt_case("Việt Nam", CasePattern::Upper), "VIỆT NAM");
    assert_eq!(
        adapt_case("đường ưu tiên", CasePattern::Upper),
        "ĐƯỜNG ƯU TIÊN"
    );
    assert_eq!(adapt_case("a1-b", CasePattern::Upper), "A1-B");
}

#[test]
fn adaptation_output_is_nfc() {
    // "ệ" written decomposed must come out precomposed.
    assert_eq!(
        adapt_case("vie\u{0302}\u{0323}t", CasePattern::Lower),
        "việt"
    );
    assert_eq!(
        adapt_case("vie\u{0302}\u{0323}t", CasePattern::Title),
        "Việt"
    );
    assert_eq!(
        adapt_case("vie\u{0302}\u{0323}t", CasePattern::Upper),
        "VIỆT"
    );
}

#[test]
fn upper_uses_full_case_mapping() {
    assert_eq!(adapt_case("straße", CasePattern::Upper), "STRASSE");
}
