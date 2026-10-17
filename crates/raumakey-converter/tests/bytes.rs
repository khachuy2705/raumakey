//! Byte-level conversion used for files.

mod common;

use common::*;
use raumakey_converter::{
    CaseTransform, Charset, ConvertError, ConvertOptions, MAX_INPUT_BYTES, convert_bytes,
    decode_bytes, encode_bytes, encode_bytes_with_report,
};

#[test]
fn legacy_bytes_match_the_oracles() {
    assert_eq!(
        decode_bytes(ICONV_VISCII_LETTERS, Charset::Viscii).unwrap(),
        LETTERS
    );
    assert_eq!(
        encode_bytes(LETTERS, Charset::Viscii).unwrap(),
        ICONV_VISCII_LETTERS
    );
    assert_eq!(
        decode_bytes(ICONV_TCVN_VN1_SENTENCE, Charset::Tcvn3).unwrap(),
        SENTENCE
    );
    assert_eq!(
        encode_bytes(SENTENCE, Charset::Tcvn3).unwrap(),
        ICONV_TCVN_VN1_SENTENCE
    );
    assert_eq!(
        decode_bytes(PY_CP1258_LETTERS, Charset::Cp1258).unwrap(),
        LETTERS
    );
    assert_eq!(
        encode_bytes(LETTERS, Charset::Cp1258).unwrap(),
        PY_CP1258_LETTERS
    );
    assert_eq!(
        encode_bytes("Tiếng Việt", Charset::VniWindows).unwrap(),
        cp1252_bytes("Tieáng Vieät")
    );
    assert_eq!(
        decode_bytes(&cp1252_bytes("Tieáng Vieät"), Charset::VniWindows).unwrap(),
        "Tiếng Việt"
    );
}

#[test]
fn tcvn3_bytes_only_vn1_assigns_keep_their_windows_1252_meaning() {
    // VN1 puts uppercase toned letters at 0x01, 0x80, 0xC0, 0xFF (among others) and combining
    // tone marks at 0xB0..=0xB4; Raumakey's TCVN3 is VN3, which leaves them unassigned.
    assert_eq!(
        decode_bytes(&[0x01, 0x80, 0xB0, 0xB4, 0xC0, 0xFF], Charset::Tcvn3).unwrap(),
        "\u{1}€°´Àÿ"
    );
}

#[test]
fn characters_a_legacy_charset_lacks_become_question_marks_in_bytes() {
    for charset in [
        Charset::Tcvn3,
        Charset::VniWindows,
        Charset::Viscii,
        Charset::Cp1258,
    ] {
        let report = encode_bytes_with_report("a中😀b", charset).unwrap();
        assert!(report.lossy, "{charset}");
        assert_eq!(report.bytes, b"a??b", "{charset}");
    }
}

#[test]
fn unicode_bytes_are_utf8_without_a_byte_order_mark() {
    assert_eq!(
        encode_bytes("Vie\u{323}\u{302}t", Charset::Unicode).unwrap(),
        "Việt".as_bytes()
    );
    assert_eq!(
        encode_bytes("Việt", Charset::UnicodeNfd).unwrap(),
        "Vie\u{323}\u{302}t".as_bytes()
    );
}

#[test]
fn unicode_bytes_honor_byte_order_marks() {
    let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
    utf8_bom.extend_from_slice("Việt".as_bytes());
    assert_eq!(decode_bytes(&utf8_bom, Charset::Unicode).unwrap(), "Việt");

    let mut utf16le = vec![0xFF, 0xFE];
    utf16le.extend("Việt😀".encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(decode_bytes(&utf16le, Charset::Unicode).unwrap(), "Việt😀");

    let mut utf16be = vec![0xFE, 0xFF];
    utf16be.extend("Việt😀".encode_utf16().flat_map(u16::to_be_bytes));
    assert_eq!(decode_bytes(&utf16be, Charset::Unicode).unwrap(), "Việt😀");

    // The byte order mark is honored for the other Unicode-based charsets too.
    let mut viqr = vec![0xFF, 0xFE];
    viqr.extend("Vie^.t".encode_utf16().flat_map(u16::to_le_bytes));
    assert_eq!(decode_bytes(&viqr, Charset::Viqr).unwrap(), "Việt");
}

#[test]
fn a_leading_zero_width_no_break_space_survives_the_byte_form() {
    // Found by the byte round-trip property: U+FEFF at the start looks like a byte order mark.
    for charset in [Charset::Unicode, Charset::UnicodeNfd, Charset::Viqr] {
        let bytes = encode_bytes("\u{FEFF}Việt", charset).unwrap();
        assert!(
            bytes.starts_with(&[0xEF, 0xBB, 0xBF, 0xEF, 0xBB, 0xBF]),
            "{charset}"
        );
        assert_eq!(
            decode_bytes(&bytes, charset).unwrap(),
            "\u{FEFF}Việt",
            "{charset}"
        );
    }
    // Elsewhere it needs no help.
    assert_eq!(
        encode_bytes("a\u{FEFF}", Charset::Unicode).unwrap(),
        "a\u{FEFF}".as_bytes()
    );
}

#[test]
fn malformed_unicode_bytes_are_rejected() {
    for bytes in [
        &[0x56, 0xC3][..],
        &[0xFF][..],
        &[0xED, 0xA0, 0x80][..],
        &[0xFF, 0xFE, 0x56][..],
        &[0xFF, 0xFE, 0x00, 0xD8][..],
        &[0xFE, 0xFF, 0xDC, 0x00][..],
    ] {
        for charset in [Charset::Unicode, Charset::Viqr, Charset::Utf8Literal] {
            assert_eq!(
                decode_bytes(bytes, charset),
                Err(ConvertError::MalformedInput),
                "{charset} {bytes:02X?}"
            );
        }
    }
}

#[test]
fn text_based_charsets_are_stored_as_utf8_of_their_text_form() {
    assert_eq!(encode_bytes("Việt", Charset::Viqr).unwrap(), b"Vie^.t");
    assert_eq!(
        encode_bytes("Việt", Charset::NcrDecimal).unwrap(),
        b"Vi&#7879;t"
    );
    assert_eq!(
        encode_bytes("Việt", Charset::NcrHex).unwrap(),
        b"Vi&#x1EC7;t"
    );
    // UTF-8 literal files hold the UTF-8 of the Windows-1252 view ("double-encoded" text).
    let literal = encode_bytes("ế", Charset::Utf8Literal).unwrap();
    assert_eq!(literal, "áº¿".as_bytes());
    assert_eq!(decode_bytes(&literal, Charset::Utf8Literal).unwrap(), "ế");
}

#[test]
fn convert_bytes_applies_options_and_reports_loss() {
    let tcvn3 = encode_bytes("Tiếng Việt", Charset::Tcvn3).unwrap();
    let upper = ConvertOptions {
        case: CaseTransform::Upper,
        remove_tones: false,
    };
    let report = convert_bytes(&tcvn3, Charset::Tcvn3, Charset::Unicode, upper).unwrap();
    assert_eq!(report.bytes, "TIẾNG VIỆT".as_bytes());
    assert!(!report.lossy);
    let report = convert_bytes(
        "Tiếng Việt".as_bytes(),
        Charset::Unicode,
        Charset::Tcvn3,
        upper,
    )
    .unwrap();
    assert_eq!(report.bytes, cp1252_bytes("TIÕNG VIÖT"));
    assert!(report.lossy);
}

#[test]
fn oversized_byte_input_is_rejected() {
    let big = vec![b'a'; MAX_INPUT_BYTES + 1];
    assert_eq!(
        decode_bytes(&big, Charset::Tcvn3),
        Err(ConvertError::InputTooLarge)
    );
    assert_eq!(
        convert_bytes(
            &big,
            Charset::Tcvn3,
            Charset::Unicode,
            ConvertOptions::default()
        )
        .map(|report| report.lossy),
        Err(ConvertError::InputTooLarge)
    );
    let text = "a".repeat(MAX_INPUT_BYTES + 1);
    assert_eq!(
        encode_bytes(&text, Charset::Unicode),
        Err(ConvertError::InputTooLarge)
    );
}
