//! `convert` with case and tone options.

use raumakey_converter::{
    CaseTransform, Charset, ConvertError, ConvertOptions, MAX_INPUT_BYTES, convert,
    convert_with_report, decode, encode,
};

fn options(case: CaseTransform, remove_tones: bool) -> ConvertOptions {
    ConvertOptions { case, remove_tones }
}

fn unicode(text: &str, case: CaseTransform, remove_tones: bool) -> String {
    convert(
        text,
        Charset::Unicode,
        Charset::Unicode,
        options(case, remove_tones),
    )
    .unwrap()
}

#[test]
fn upper_and_lower_case_cover_vietnamese_letters() {
    assert_eq!(
        unicode("tiếng việt đường ơ ư ă", CaseTransform::Upper, false),
        "TIẾNG VIỆT ĐƯỜNG Ơ Ư Ă"
    );
    assert_eq!(
        unicode("TIẾNG VIỆT ĐƯỜNG Ơ Ư Ă", CaseTransform::Lower, false),
        "tiếng việt đường ơ ư ă"
    );
}

#[test]
fn title_case_capitalizes_the_first_letter_of_each_word() {
    for (input, expected) in [
        ("tiếng việt", "Tiếng Việt"),
        ("ĐÀ NẴNG", "Đà Nẵng"),
        ("người đi đường", "Người Đi Đường"),
        ("don't stop o'clock", "Don't Stop O'clock"),
        ("3d model, hello-world", "3d Model, Hello-World"),
        ("  ấy  ", "  Ấy  "),
        ("ưu tiên", "Ưu Tiên"),
    ] {
        assert_eq!(
            unicode(input, CaseTransform::Title, false),
            expected,
            "{input}"
        );
    }
}

#[test]
fn removing_tones_strips_every_vietnamese_mark() {
    assert_eq!(
        unicode(
            "Cộng hòa xã hội chủ nghĩa Việt Nam",
            CaseTransform::None,
            true
        ),
        "Cong hoa xa hoi chu nghia Viet Nam"
    );
    assert_eq!(
        unicode("Đường Ăn Ở ữ", CaseTransform::None, true),
        "Duong An O u"
    );
    // Letters that are not Vietnamese keep their marks.
    assert_eq!(
        unicode("café naïve ñ ç", CaseTransform::None, true),
        "cafe naïve ñ ç"
    );
    assert_eq!(
        unicode("Tiếng Việt", CaseTransform::Upper, true),
        "TIENG VIET"
    );
}

#[test]
fn removing_tones_also_drops_stacked_combining_marks() {
    // NFC keeps a second mark combining when no precomposed letter exists; it must not
    // recombine with the stripped base letter.
    assert_eq!(unicode("ạ\u{301}", CaseTransform::None, true), "a");
    assert_eq!(unicode("Ê\u{323}\u{301}x", CaseTransform::None, true), "Ex");
    assert_eq!(unicode("đ\u{309}", CaseTransform::Upper, true), "D");
    // Found by the property test: another mark between the letter and a tone.
    assert_eq!(
        unicode("à\u{316}\u{340}", CaseTransform::None, true),
        "a\u{316}"
    );
    // Marks after letters that are not Vietnamese are kept.
    assert_eq!(
        unicode("ǜ q\u{301}", CaseTransform::None, true),
        "ǜ q\u{301}"
    );
}

#[test]
fn removing_tones_reads_decomposed_input() {
    assert_eq!(
        convert(
            "Vie\u{323}\u{302}t",
            Charset::UnicodeNfd,
            Charset::Unicode,
            options(CaseTransform::None, true)
        )
        .unwrap(),
        "Viet"
    );
}

#[test]
fn default_options_equal_decode_then_encode() {
    let tcvn3 = "Céng hßa x· héi chñ nghÜa ViÖt Nam";
    let unicode_text = decode(tcvn3, Charset::Tcvn3).unwrap();
    for to in Charset::ALL {
        assert_eq!(
            convert(tcvn3, Charset::Tcvn3, to, ConvertOptions::default()).unwrap(),
            encode(&unicode_text, to).unwrap(),
            "{to}"
        );
    }
}

#[test]
fn conversion_between_legacy_charsets_goes_through_unicode() {
    let options = ConvertOptions::default();
    assert_eq!(
        convert("Tieáng Vieät", Charset::VniWindows, Charset::Viqr, options).unwrap(),
        "Tie^'ng Vie^.t"
    );
    assert_eq!(
        convert("TiÕng ViÖt", Charset::Tcvn3, Charset::VniWindows, options).unwrap(),
        "Tieáng Vieät"
    );
    assert_eq!(
        convert("Tiªng Vi®t", Charset::Viscii, Charset::Utf8Literal, options).unwrap(),
        "Tiáº¿ng Viá»‡t"
    );
}

#[test]
fn case_options_apply_before_encoding_and_loss_is_reported() {
    let report = convert_with_report(
        "TiÕng ViÖt",
        Charset::Tcvn3,
        Charset::Unicode,
        options(CaseTransform::Upper, false),
    )
    .unwrap();
    assert_eq!(report.text, "TIẾNG VIỆT");
    assert!(!report.lossy);

    let report = convert_with_report(
        "Tiếng Việt",
        Charset::Unicode,
        Charset::Tcvn3,
        options(CaseTransform::Upper, false),
    )
    .unwrap();
    assert_eq!(report.text, "TIÕNG VIÖT");
    assert!(report.lossy);

    let report = convert_with_report(
        "Tiếng Việt",
        Charset::Unicode,
        Charset::Tcvn3,
        options(CaseTransform::Upper, true),
    )
    .unwrap();
    assert_eq!(report.text, "TIENG VIET");
    assert!(!report.lossy);
}

#[test]
fn oversized_input_is_rejected() {
    let big = "a".repeat(MAX_INPUT_BYTES + 1);
    assert_eq!(
        convert(
            &big,
            Charset::Unicode,
            Charset::Unicode,
            ConvertOptions::default()
        ),
        Err(ConvertError::InputTooLarge)
    );
}
