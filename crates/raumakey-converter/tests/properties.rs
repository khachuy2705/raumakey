//! Property tests: decoding never panics, encoding is decodable whenever it reports no loss,
//! and decode-then-encode is idempotent for every charset.

use proptest::prelude::*;
use raumakey_converter::{
    CaseTransform, Charset, ConvertOptions, convert, decode, decode_bytes, encode,
    encode_bytes_with_report, encode_with_report,
};
use unicode_normalization::UnicodeNormalization;
use unicode_normalization::is_nfc;

const VIETNAMESE: &str = "aàảãáạăằẳẵắặâầẩẫấậeèẻẽéẹêềểễếệiìỉĩíịoòỏõóọôồổỗốộơờởỡớợuùủũúụưừửữứựyỳỷỹýỵđ\
AÀẢÃÁẠĂẰẲẴẮẶÂẦẨẪẤẬEÈẺẼÉẸÊỀỂỄẾỆIÌỈĨÍỊOÒỎÕÓỌÔỒỔỖỐỘƠỜỞỠỚỢUÙỦŨÚỤƯỪỬỮỨỰYỲỶỸÝỴĐ";

/// Characters that exercise every codec: Vietnamese letters, VIQR marks and escapes, reference
/// syntax, Windows-1252 high characters, combining marks, and anything else.
fn interesting_char() -> impl Strategy<Value = char> {
    let vietnamese: Vec<char> = VIETNAMESE.chars().collect();
    prop_oneof![
        4 => prop::sample::select(vietnamese),
        3 => prop::sample::select("^(+'`?~.\\dD&#x;0123456789*- ".chars().collect::<Vec<_>>()),
        2 => (0x80_u32..=0xFF).prop_map(|c| char::from_u32(c).unwrap_or('?')),
        2 => prop::sample::select("€‚ƒ„…†‡ˆ‰Š‹ŒŽ‘’“”•–—˜™š›œžŸ\u{81}\u{8D}\u{8F}\u{90}\u{9D}".chars().collect::<Vec<_>>()),
        1 => (0x300_u32..=0x36F).prop_map(|c| char::from_u32(c).unwrap_or('?')),
        1 => Just('\u{FEFF}'),
        2 => any::<char>(),
        2 => (0x20_u8..0x7F).prop_map(char::from),
    ]
}

fn interesting_text() -> impl Strategy<Value = String> {
    prop::collection::vec(interesting_char(), 0..48).prop_map(|chars| chars.into_iter().collect())
}

fn nfc(text: &str) -> String {
    text.nfc().collect()
}

proptest! {
    #[test]
    fn decoding_any_text_never_panics_and_yields_nfc(text in interesting_text()) {
        for charset in Charset::ALL {
            let decoded = decode(&text, charset).unwrap();
            prop_assert!(is_nfc(&decoded), "{charset}");
        }
    }

    #[test]
    fn decoding_arbitrary_strings_never_panics(text in any::<String>()) {
        for charset in Charset::ALL {
            prop_assert!(decode(&text, charset).is_ok());
        }
    }

    #[test]
    fn decoding_any_bytes_never_panics(bytes in prop::collection::vec(any::<u8>(), 0..96)) {
        for charset in Charset::ALL {
            match decode_bytes(&bytes, charset) {
                Ok(decoded) => prop_assert!(is_nfc(&decoded), "{charset}"),
                Err(error) => {
                    prop_assert!(!charset.is_legacy_byte(), "{charset}: {error}");
                }
            }
        }
    }

    #[test]
    fn exact_text_encoding_decodes_back(text in interesting_text()) {
        for charset in Charset::ALL {
            let report = encode_with_report(&text, charset).unwrap();
            if !report.lossy {
                prop_assert_eq!(decode(&report.text, charset).unwrap(), nfc(&text), "{}", charset);
            }
        }
    }

    #[test]
    fn exact_byte_encoding_decodes_back(text in interesting_text()) {
        for charset in Charset::ALL {
            let report = encode_bytes_with_report(&text, charset).unwrap();
            if !report.lossy {
                prop_assert_eq!(
                    decode_bytes(&report.bytes, charset).unwrap(),
                    nfc(&text),
                    "{}",
                    charset
                );
            }
        }
    }

    #[test]
    fn vietnamese_text_is_never_lossy_in_lossless_charsets(
        text in prop::collection::vec(prop::sample::select(VIETNAMESE.chars().chain(" .,?".chars()).collect::<Vec<_>>()), 0..48)
            .prop_map(|chars| chars.into_iter().collect::<String>())
    ) {
        for charset in Charset::ALL.into_iter().filter(|c| c.is_lossless()) {
            prop_assert!(!encode_with_report(&text, charset).unwrap().lossy, "{}", charset);
            prop_assert!(!encode_bytes_with_report(&text, charset).unwrap().lossy, "{}", charset);
        }
    }

    #[test]
    fn options_always_produce_nfc(
        text in interesting_text(),
        case in prop::sample::select(CaseTransform::ALL.to_vec()),
        remove_tones in any::<bool>(),
    ) {
        let options = ConvertOptions { case, remove_tones };
        let converted = convert(&text, Charset::Unicode, Charset::Unicode, options).unwrap();
        prop_assert!(is_nfc(&converted));
    }

    #[test]
    fn removing_tones_leaves_no_vietnamese_diacritic(text in interesting_text()) {
        let options = ConvertOptions { case: CaseTransform::None, remove_tones: true };
        let converted = convert(&text, Charset::Unicode, Charset::Unicode, options).unwrap();
        prop_assert!(
            !converted.chars().any(|c| !c.is_ascii() && VIETNAMESE.contains(c)),
            "{:?}",
            converted
        );
    }

    #[test]
    fn decode_then_encode_is_idempotent(text in interesting_text()) {
        for charset in Charset::ALL {
            let once = encode(&decode(&text, charset).unwrap(), charset).unwrap();
            let twice = encode(&decode(&once, charset).unwrap(), charset).unwrap();
            prop_assert_eq!(&twice, &once, "{}", charset);
        }
    }

    #[test]
    fn encoded_text_is_a_fixed_point(text in interesting_text()) {
        for charset in Charset::ALL {
            let encoded = encode(&text, charset).unwrap();
            let again = encode(&decode(&encoded, charset).unwrap(), charset).unwrap();
            prop_assert_eq!(&again, &encoded, "{}", charset);
        }
    }
}
