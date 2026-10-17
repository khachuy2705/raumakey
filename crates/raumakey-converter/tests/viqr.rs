//! VIQR (RFC 1456): diacritics `^ ( +` follow the base letter, tones `' ` ? ~ .` follow those,
//! `dd`/`DD` spell đ/Đ, and `\` escapes a character that would otherwise be read as a mark.

use raumakey_converter::{Charset, decode, encode, encode_with_report};

fn assert_both_ways(unicode: &str, viqr: &str) {
    let report = encode_with_report(unicode, Charset::Viqr).unwrap();
    assert_eq!(report.text, viqr, "encode {unicode:?}");
    assert!(!report.lossy);
    assert_eq!(
        decode(viqr, Charset::Viqr).unwrap(),
        unicode,
        "decode {viqr:?}"
    );
}

#[test]
fn words_are_written_with_marks_after_the_letter() {
    assert_both_ways("Tiếng Việt", "Tie^'ng Vie^.t");
    assert_both_ways("người", "ngu+o+`i");
    assert_both_ways("Đà Nẵng", "DDa` Na(~ng");
    assert_both_ways("thuở", "thuo+?");
    assert_both_ways("rượu", "ru+o+.u");
    assert_both_ways("VIỆT NAM", "VIE^.T NAM");
    assert_both_ways("ĐẠI HỌC", "DDA.I HO.C");
    assert_both_ways("khuyến", "khuye^'n");
    assert_both_ways("ỹ Ỹ ỵ", "y~ Y~ y.");
}

#[test]
fn a_mark_character_after_an_untoned_vowel_is_escaped() {
    assert_both_ways("Anh đi.", "Anh ddi\\.");
    assert_both_ways("Ai?", "Ai\\?");
    assert_both_ways("a^ o+ a( o(", "a\\^ o\\+ a\\( o(");
    assert_both_ways("u^ e( e+", "u^ e( e+");
    // A vowel that already has its shape takes no second one, but still takes a tone.
    assert_both_ways("â^ ơ+ ê'", "a^^ o++ e^\\'");
}

#[test]
fn a_mark_character_after_a_toned_vowel_or_a_consonant_needs_no_escape() {
    assert_both_ways("Hà.", "Ha`.");
    assert_both_ways("Cái gì?", "Ca'i gi`?");
    assert_both_ways("x. y'", "x. y\\'");
    assert_both_ways("ấy.", "a^'y\\.");
}

#[test]
fn a_literal_dd_is_escaped() {
    assert_both_ways("add", "ad\\d");
    assert_both_ways("ADD Dd", "AD\\D D\\d");
    assert_both_ways("ddd", "d\\dd");
    // A plain d before đ is escaped itself, so the pair stays apart.
    assert_both_ways("dđ", "\\ddd");
    assert_both_ways("đd", "ddd");
}

#[test]
fn a_literal_backslash_is_escaped_only_before_an_escapable_character() {
    assert_both_ways("C:\\Users", "C:\\Users");
    assert_both_ways("a\\.", "a\\\\.");
    assert_both_ways("x\\d", "x\\\\d");
    assert_both_ways("x\\đ", "x\\\\dd");
    assert_both_ways("\\\\", "\\\\\\");
    assert_both_ways("end\\", "end\\");
}

#[test]
fn decoding_accepts_rfc_1456_text() {
    assert_eq!(
        decode(
            "Vie^.t Nam, DDo^.c la^.p - Tu+. do - Ha.nh phu'c",
            Charset::Viqr
        )
        .unwrap(),
        "Việt Nam, Độc lập - Tự do - Hạnh phúc"
    );
    // Matrix #8: a backslash then an apostrophe is a literal apostrophe.
    assert_eq!(decode("\\'", Charset::Viqr).unwrap(), "'");
    assert_eq!(decode("Dd dD", Charset::Viqr).unwrap(), "Đ đ");
}

#[test]
fn decoding_is_lenient_with_marks_that_do_not_apply() {
    // A shape after a tone, a second tone, a mark after a consonant, a shape on the wrong
    // vowel, and an escape of a non-mark character are kept literally.
    assert_eq!(
        decode("a'^ a'' b' u^ \\x", Charset::Viqr).unwrap(),
        "á^ á' b' u^ \\x"
    );
    // A precomposed untoned vowel takes a following tone.
    assert_eq!(decode("Vie^.t vê'", Charset::Viqr).unwrap(), "Việt vế");
    // A trailing backslash stays.
    assert_eq!(decode("x\\", Charset::Viqr).unwrap(), "x\\");
}

#[test]
fn other_characters_pass_through() {
    assert_both_ways("中文 😀 * - ä", "中文 😀 * - ä");
    assert_eq!(encode("e\u{301}", Charset::Viqr).unwrap(), "e'");
}
