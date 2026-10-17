//! TCVN3, VNI Windows, VISCII, and Windows-1258 in text form (Windows-1252 characters of the
//! bytes), checked against independent oracles where one exists (see `common/mod.rs`).

mod common;

use common::*;
use raumakey_converter::{Charset, decode, encode, encode_with_report};

fn assert_exact(text: &str, charset: Charset, expected: &str) {
    let report = encode_with_report(text, charset).unwrap();
    assert_eq!(report.text, expected, "encode {charset}");
    assert!(!report.lossy, "encode {charset} must be exact");
    assert_eq!(decode(expected, charset).unwrap(), text, "decode {charset}");
}

#[test]
fn viscii_matches_iconv_for_every_letter_and_a_sentence() {
    assert_exact(LETTERS, Charset::Viscii, &cp1252(ICONV_VISCII_LETTERS));
    assert_exact(SENTENCE, Charset::Viscii, &cp1252(ICONV_VISCII_SENTENCE));
    assert_exact("Tiếng Việt", Charset::Viscii, "Tiªng Vi®t");
}

#[test]
fn tcvn3_matches_vn1_for_lowercase_and_untoned_uppercase_letters() {
    // The sentence has no uppercase toned letter, so VN3 and VN1 agree on every byte.
    assert_exact(SENTENCE, Charset::Tcvn3, &cp1252(ICONV_TCVN_VN1_SENTENCE));
    assert_exact("Tiếng Việt", Charset::Tcvn3, "TiÕng ViÖt");
    assert_exact(
        "Cộng hòa xã hội chủ nghĩa Việt Nam",
        Charset::Tcvn3,
        "Céng hßa x· héi chñ nghÜa ViÖt Nam",
    );
    let mut exact = 0;
    for (letter, &vn1) in LETTERS.chars().zip(ICONV_TCVN_VN1_LETTERS) {
        let toned_upper = letter.is_uppercase() && !"ĂÂÊÔƠƯĐ".contains(letter);
        if toned_upper {
            continue;
        }
        assert_exact(&letter.to_string(), Charset::Tcvn3, &cp1252(&[vn1]));
        exact += 1;
    }
    assert_eq!(exact, 74);
}

#[test]
fn tcvn3_writes_uppercase_toned_letters_in_lowercase_and_reports_loss() {
    let mut lossy = 0;
    for letter in LETTERS.chars() {
        let toned_upper = letter.is_uppercase() && !"ĂÂÊÔƠƯĐ".contains(letter);
        if !toned_upper {
            continue;
        }
        let lower: String = letter.to_lowercase().collect();
        let report = encode_with_report(&letter.to_string(), Charset::Tcvn3).unwrap();
        assert!(report.lossy, "{letter}");
        assert_eq!(
            report.text,
            encode(&lower, Charset::Tcvn3).unwrap(),
            "{letter}"
        );
        assert_eq!(decode(&report.text, Charset::Tcvn3).unwrap(), lower);
        lossy += 1;
    }
    assert_eq!(lossy, 60);
    let report = encode_with_report("TIẾNG VIỆT", Charset::Tcvn3).unwrap();
    assert!(report.lossy);
    assert_eq!(report.text, "TIÕNG VIÖT");
}

#[test]
fn tcvn3_keeps_vn1_only_positions_as_windows_1252_characters() {
    // VN1 puts uppercase toned letters at positions VN3 leaves unassigned and combining tone
    // marks at 0xB0..=0xB4; Raumakey's TCVN3 is VN3, so those bytes keep their 1252 meaning.
    let vn1_only = [
        0x01, 0x80, 0x9F, 0xAF, 0xB0, 0xB4, 0xBA, 0xC0, 0xCD, 0xE0, 0xF0, 0xFF,
    ];
    let text = cp1252(&vn1_only);
    assert_eq!(decode(&text, Charset::Tcvn3).unwrap(), text);
    assert_eq!(decode("25°C €", Charset::Tcvn3).unwrap(), "25°C €");
}

#[test]
fn windows_1258_matches_python_for_every_letter_and_a_sentence() {
    assert_exact(LETTERS, Charset::Cp1258, &cp1252(PY_CP1258_LETTERS));
    assert_exact(SENTENCE, Charset::Cp1258, &cp1252(PY_CP1258_SENTENCE));
    assert_exact("Tiếng Việt", Charset::Cp1258, "Tiêìng Viêòt");
}

#[test]
fn windows_1258_decoding_composes_combining_tone_marks() {
    // e + combining acute, ê + combining acute, u + hook, ư + dot below.
    assert_eq!(
        decode(
            &cp1252(&[0x65, 0xEC, 0xEA, 0xEC, 0x75, 0xD2, 0xFD, 0xF2]),
            Charset::Cp1258
        )
        .unwrap(),
        "éếủự"
    );
}

#[test]
fn windows_1258_decomposes_other_letters_when_their_marks_exist() {
    assert_exact("ǹ Ǹ ḿ", Charset::Cp1258, "nÌ NÌ mì");
}

#[test]
fn vni_windows_matches_the_published_table() {
    for (letter, vni) in VNI_LETTERS {
        assert_exact(&letter.to_string(), Charset::VniWindows, vni);
    }
    let all: String = VNI_LETTERS.iter().map(|(_, vni)| *vni).collect();
    assert_exact(LETTERS, Charset::VniWindows, &all);
}

#[test]
fn vni_windows_well_known_words() {
    for (unicode, vni) in [
        ("Tiếng Việt", "Tieáng Vieät"),
        (
            "Cộng hòa xã hội chủ nghĩa Việt Nam",
            "Coäng hoøa xaõ hoäi chuû nghóa Vieät Nam",
        ),
        (
            "Độc lập - Tự do - Hạnh phúc",
            "Ñoäc laäp - Töï do - Haïnh phuùc",
        ),
        ("Người", "Ngöôøi"),
        ("Đà Nẵng", "Ñaø Naüng"),
        ("TRƯỜNG ĐẠI HỌC", "TRÖÔØNG ÑAÏI HOÏC"),
        ("năm", "naêm"),
        ("được", "ñöôïc"),
        ("những", "nhöõng"),
        ("chỉ", "chæ"),
        ("thị", "thò"),
        ("Mỹ", "Myõ"),
        ("quỵ", "quî"),
        ("sản phẩm", "saûn phaåm"),
        ("mặt", "maët"),
        ("chẳng", "chaúng"),
    ] {
        assert_exact(unicode, Charset::VniWindows, vni);
    }
}

#[test]
fn vni_windows_decoding_is_lenient() {
    // Mixed-case pairs take the case of the base letter; i/y take combining tones; a mark that
    // follows no vowel keeps its 1252 meaning.
    assert_eq!(decode("Aù aÙ", Charset::VniWindows).unwrap(), "Á á");
    assert_eq!(decode("iù Iø yï", Charset::VniWindows).unwrap(), "í Ì ỵ");
    assert_eq!(decode("nâ ù", Charset::VniWindows).unwrap(), "nâ ù");
}

#[test]
fn characters_without_a_windows_1252_byte_pass_through_text_form() {
    for charset in [
        Charset::Tcvn3,
        Charset::VniWindows,
        Charset::Viscii,
        Charset::Cp1258,
    ] {
        let report = encode_with_report("Việt 中文 😀", charset).unwrap();
        assert!(!report.lossy, "{charset}");
        assert!(report.text.ends_with(" 中文 😀"), "{charset}");
        assert_eq!(decode(&report.text, charset).unwrap(), "Việt 中文 😀");
    }
}

#[test]
fn windows_1252_characters_on_free_positions_pass_through() {
    assert_exact("25°C € ©", Charset::VniWindows, "25°C € ©");
    // 0xB1 is unassigned in VN3 (VN1 has a combining hook there).
    assert_exact("25°C € ±", Charset::Tcvn3, "25°C € ±");
    assert_exact("€ ©", Charset::Cp1258, "€ ©");
}

#[test]
fn windows_1252_characters_on_taken_positions_are_replaced_and_reported() {
    for (charset, text) in [
        (Charset::Tcvn3, "©"),
        (Charset::Tcvn3, "½"),
        (Charset::Tcvn3, "\u{AD}"),
        (Charset::Viscii, "©"),
        (Charset::Viscii, "\u{2}"),
        (Charset::VniWindows, "ä"),
        (Charset::VniWindows, "ñ"),
        (Charset::VniWindows, "Ø"),
        (Charset::Cp1258, "Š"),
        (Charset::Cp1258, "Ð"),
    ] {
        let report = encode_with_report(&format!("a{text}b"), charset).unwrap();
        assert!(report.lossy, "{charset} {text:?}");
        assert_eq!(report.text, "a?b", "{charset} {text:?}");
    }
}
