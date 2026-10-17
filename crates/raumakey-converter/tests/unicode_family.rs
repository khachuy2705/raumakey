//! Unicode, NFD, numeric character references, and UTF-8 literal text conversion.

use raumakey_converter::{
    Charset, ConvertError, MAX_INPUT_BYTES, decode, encode, encode_with_report,
};

const NFC: &str = "Tiếng Việt";
const NFD: &str = "Tie\u{302}\u{301}ng Vie\u{323}\u{302}t";

#[test]
fn unicode_decodes_and_encodes_to_nfc() {
    assert_eq!(decode(NFD, Charset::Unicode).unwrap(), NFC);
    assert_eq!(encode(NFD, Charset::Unicode).unwrap(), NFC);
    assert_eq!(encode(NFC, Charset::Unicode).unwrap(), NFC);
}

#[test]
fn unicode_nfd_encodes_decomposed_and_decodes_to_nfc() {
    assert_eq!(encode(NFC, Charset::UnicodeNfd).unwrap(), NFD);
    assert_eq!(decode(NFD, Charset::UnicodeNfd).unwrap(), NFC);
}

#[test]
fn ncr_decimal_encodes_every_non_ascii_scalar() {
    assert_eq!(
        encode("Tiếng Việt", Charset::NcrDecimal).unwrap(),
        "Ti&#7871;ng Vi&#7879;t"
    );
    assert_eq!(
        encode("Đ😀", Charset::NcrDecimal).unwrap(),
        "&#272;&#128512;"
    );
}

#[test]
fn ncr_hex_encodes_every_non_ascii_scalar_in_uppercase_hex() {
    assert_eq!(
        encode("Tiếng Việt", Charset::NcrHex).unwrap(),
        "Ti&#x1EBF;ng Vi&#x1EC7;t"
    );
    assert_eq!(encode("Đ😀", Charset::NcrHex).unwrap(), "&#x110;&#x1F600;");
}

#[test]
fn ncr_decoding_accepts_both_forms_in_either_charset() {
    for charset in [Charset::NcrDecimal, Charset::NcrHex] {
        assert_eq!(
            decode("Ti&#7871;ng Vi&#x1ec7;t &#X1EBF;", charset).unwrap(),
            "Tiếng Việt ế"
        );
        assert_eq!(decode("&#0000065;&#x0041;", charset).unwrap(), "AA");
    }
}

#[test]
fn ncr_encoding_escapes_an_ampersand_that_starts_a_reference() {
    let text = "a&#233; & b &";
    let encoded = encode(text, Charset::NcrDecimal).unwrap();
    assert_eq!(encoded, "a&#38;#233; & b &");
    assert_eq!(decode(&encoded, Charset::NcrDecimal).unwrap(), text);
    let encoded = encode(text, Charset::NcrHex).unwrap();
    assert_eq!(encoded, "a&#x26;#233; & b &");
    assert_eq!(decode(&encoded, Charset::NcrHex).unwrap(), text);
}

#[test]
fn malformed_or_unsafe_references_stay_literal() {
    for text in [
        "&#;",
        "&#x;",
        "&#65",
        "&#x41 ",
        "&#xG1;",
        "&#0;",
        "&#x0;",
        "&#xD800;",
        "&#55296;",
        "&#x110000;",
        "&#1114112;",
        "&#000000065;",
        "&#99999999999999999999;",
        "&",
        "&#",
        "&#x",
        "&amp;",
    ] {
        assert_eq!(decode(text, Charset::NcrDecimal).unwrap(), text, "{text:?}");
    }
}

#[test]
fn utf8_literal_shows_utf8_bytes_as_windows_1252_characters() {
    assert_eq!(
        encode("Tiếng Việt", Charset::Utf8Literal).unwrap(),
        "Tiáº¿ng Viá»‡t"
    );
    assert_eq!(
        decode("Tiáº¿ng Viá»‡t", Charset::Utf8Literal).unwrap(),
        "Tiếng Việt"
    );
    // U+00C1 is C3 81; 0x81 is undefined in Windows-1252 and maps to U+0081.
    assert_eq!(encode("Á", Charset::Utf8Literal).unwrap(), "Ã\u{81}");
    assert_eq!(decode("Ã\u{81}", Charset::Utf8Literal).unwrap(), "Á");
}

#[test]
fn utf8_literal_decoding_keeps_text_that_is_not_utf8() {
    // A lone lead byte, a lone continuation byte, a truncated sequence, and characters that are
    // not Windows-1252 at all are kept as they are.
    assert_eq!(
        decode("café ¿ Ã 中 áº", Charset::Utf8Literal).unwrap(),
        "café ¿ Ã 中 áº"
    );
    assert_eq!(
        decode("Viá»‡t中Viá»‡t", Charset::Utf8Literal).unwrap(),
        "Việt中Việt"
    );
}

#[test]
fn unicode_family_encoding_is_never_lossy() {
    for charset in [
        Charset::Unicode,
        Charset::UnicodeNfd,
        Charset::NcrDecimal,
        Charset::NcrHex,
        Charset::Utf8Literal,
    ] {
        let report = encode_with_report("Đà Nẵng 中文 😀 &#", charset).unwrap();
        assert!(!report.lossy, "{charset}");
        assert_eq!(
            decode(&report.text, charset).unwrap(),
            "Đà Nẵng 中文 😀 &#",
            "{charset}"
        );
    }
}

#[test]
fn oversized_input_is_rejected_before_conversion() {
    let big = "a".repeat(MAX_INPUT_BYTES + 1);
    for charset in Charset::ALL {
        assert_eq!(decode(&big, charset), Err(ConvertError::InputTooLarge));
        assert_eq!(encode(&big, charset), Err(ConvertError::InputTooLarge));
    }
    let limit = "a".repeat(MAX_INPUT_BYTES);
    assert_eq!(
        decode(&limit, Charset::Unicode).unwrap().len(),
        MAX_INPUT_BYTES
    );
}
