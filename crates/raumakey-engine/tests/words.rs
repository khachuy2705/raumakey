//! Real Vietnamese words and sentences typed in Telex, VNI, and VIQR, in lower, title, and upper
//! case, with both tone styles. Key sequences are derived from the expected text by a small
//! test-side encoder (marks right after their letter, the tone key at the end of the word).

mod common;

use common::{modern, telex, type_keys, viqr, vni};
use raumakey_model::options::EngineOptions;
use unicode_normalization::UnicodeNormalization;

#[derive(Clone, Copy, Debug)]
enum Method {
    Telex,
    Vni,
    Viqr,
}

const METHODS: [Method; 3] = [Method::Telex, Method::Vni, Method::Viqr];

fn options(method: Method) -> EngineOptions {
    match method {
        Method::Telex => telex(),
        Method::Vni => vni(),
        Method::Viqr => viqr(),
    }
}

/// Keys that type `word` (one syllable, NFC) in `method`.
fn encode(word: &str, method: Method) -> String {
    let all_upper = word.chars().filter(|c| c.is_alphabetic()).count() > 1
        && word.chars().all(|c| !c.is_lowercase());
    let mut keys = String::new();
    let mut tone_key = None;
    for c in word.chars() {
        let upper = c.is_uppercase();
        if c == 'đ' || c == 'Đ' {
            let d = if upper { "DD" } else { "dd" };
            keys.push_str(match method {
                Method::Vni => {
                    if upper {
                        "D9"
                    } else {
                        "d9"
                    }
                }
                _ => d,
            });
            continue;
        }
        let mut parts = c.nfd();
        let base = parts.next().expect("non-empty");
        keys.push(base);
        for mark in parts {
            let (telex_key, vni_key, viqr_key, is_tone) = match mark {
                '\u{302}' => (base, '6', '^', false),
                '\u{306}' => ('w', '8', '(', false),
                '\u{31B}' => ('w', '7', '+', false),
                '\u{301}' => ('s', '1', '\'', true),
                '\u{300}' => ('f', '2', '`', true),
                '\u{309}' => ('r', '3', '?', true),
                '\u{303}' => ('x', '4', '~', true),
                '\u{323}' => ('j', '5', '.', true),
                other => panic!("unexpected mark {other:?} in {word}"),
            };
            let key = match method {
                Method::Telex => telex_key,
                Method::Vni => vni_key,
                Method::Viqr => viqr_key,
            };
            let key = if upper && key.is_ascii_alphabetic() {
                key.to_ascii_uppercase()
            } else {
                key
            };
            if is_tone {
                tone_key = Some(key);
            } else {
                keys.push(key);
            }
        }
    }
    if let Some(key) = tone_key {
        keys.push(if all_upper {
            key.to_ascii_uppercase()
        } else {
            key
        });
    }
    keys
}

fn title(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// Common words; every one is a complete syllable and survives the boundary unchanged.
const WORDS: &[&str] = &[
    "a",
    "á",
    "ai",
    "anh",
    "ăn",
    "ấy",
    "bà",
    "bạn",
    "bán",
    "bàn",
    "bảo",
    "bây",
    "bé",
    "bên",
    "bệnh",
    "biết",
    "biển",
    "bình",
    "bỏ",
    "bố",
    "bởi",
    "bức",
    "bước",
    "cả",
    "các",
    "cách",
    "cái",
    "cảm",
    "cần",
    "cao",
    "cây",
    "chả",
    "chào",
    "chạy",
    "chân",
    "chè",
    "chỉ",
    "chiều",
    "chín",
    "chính",
    "cho",
    "chó",
    "chơi",
    "chồng",
    "chúng",
    "chưa",
    "chữ",
    "có",
    "con",
    "còn",
    "cơm",
    "cũng",
    "của",
    "cười",
    "cửa",
    "dài",
    "dân",
    "dạy",
    "dễ",
    "đã",
    "đang",
    "đánh",
    "đâu",
    "đây",
    "để",
    "đẹp",
    "đến",
    "đều",
    "đi",
    "điện",
    "đó",
    "đọc",
    "đôi",
    "đồng",
    "đúng",
    "được",
    "đường",
    "em",
    "gà",
    "gặp",
    "gì",
    "già",
    "giá",
    "giấy",
    "giờ",
    "giữa",
    "giường",
    "giúp",
    "hai",
    "hay",
    "hãy",
    "hiểu",
    "hoa",
    "học",
    "hỏi",
    "hơn",
    "hương",
    "khi",
    "khó",
    "không",
    "khuya",
    "khuyên",
    "kia",
    "kìa",
    "kỹ",
    "là",
    "làm",
    "lại",
    "lắm",
    "lên",
    "lớn",
    "lời",
    "lúc",
    "lửa",
    "ly",
    "mà",
    "mai",
    "màu",
    "mắt",
    "mặt",
    "mấy",
    "mẹ",
    "mình",
    "mới",
    "mỗi",
    "một",
    "mua",
    "mùa",
    "muốn",
    "mưa",
    "mười",
    "năm",
    "nào",
    "này",
    "nên",
    "ngày",
    "nghe",
    "nghĩ",
    "ngoài",
    "ngủ",
    "người",
    "nhà",
    "nhanh",
    "nhiều",
    "như",
    "nhưng",
    "những",
    "nói",
    "nơi",
    "nước",
    "nữa",
    "ở",
    "ông",
    "phải",
    "phố",
    "quá",
    "quả",
    "quen",
    "quê",
    "quyết",
    "ra",
    "rằng",
    "rất",
    "rồi",
    "rượu",
    "sách",
    "sao",
    "sau",
    "sẽ",
    "sinh",
    "sông",
    "sớm",
    "sống",
    "sữa",
    "tai",
    "tay",
    "tên",
    "thấy",
    "thế",
    "thì",
    "thích",
    "thôi",
    "thời",
    "thuyền",
    "thương",
    "tiền",
    "tiếng",
    "tìm",
    "tôi",
    "trà",
    "trăng",
    "trên",
    "trời",
    "trong",
    "trường",
    "từ",
    "tuổi",
    "tuyết",
    "uống",
    "vào",
    "vẫn",
    "về",
    "việc",
    "viết",
    "vui",
    "vừa",
    "xa",
    "xanh",
    "xem",
    "xe",
    "xin",
    "xuân",
    "yêu",
    "yên",
];

/// Words that exercise rarer rhymes and the `qu`/`gi`/`uơ` rules.
const SPECIAL_WORDS: &[&str] = &[
    "thuở", "huơ", "khuỷu", "khuấy", "ngoẻo", "xoáy", "quặng", "quốc", "quần", "quyển", "quý",
    "quỳnh", "giếng", "gìn", "giặt", "nghiêng", "nguyễn", "khoanh", "hoạch", "xoắn", "ngoằn",
    "khoét", "huệ", "thuế", "tuần", "uyên", "oán", "ước", "ưu", "ửng", "hươu", "tươi", "ướt",
    "khuyến", "giữ", "gió", "quạt", "quét", "quên", "huỳnh", "toán", "hoàng", "nguệch", "oai",
    "quắp", "quặp", "quều", "quăng", "quắt", "quặc", "quẹo", "giẻ", "ghế", "gửi", "gợi", "quết",
    "quệt", "quạu", "quọ",
];

/// Old style (tone on the first vowel of oa/oe/uy) and modern style.
const TONE_STYLE_PAIRS: &[(&str, &str)] = &[
    ("hòa", "hoà"),
    ("khỏe", "khoẻ"),
    ("thủy", "thuỷ"),
    ("họa", "hoạ"),
    ("xóa", "xoá"),
    ("tòa", "toà"),
    ("hủy", "huỷ"),
    ("lũy", "luỹ"),
    ("thúy", "thuý"),
    ("khóa", "khoá"),
    ("đóa", "đoá"),
    ("nhòe", "nhoè"),
    ("lòe", "loè"),
    ("tủy", "tuỷ"),
    ("góa", "goá"),
];

fn check_word(word: &str, options: &EngineOptions, method: Method) {
    let keys = encode(word, method);
    assert_eq!(
        type_keys(options, &keys),
        word,
        "{method:?} keys {keys:?} -> {word}"
    );
    // A valid word is committed unchanged at a boundary (never auto-restored).
    assert_eq!(
        type_keys(options, &format!("{keys}{{SPACE}}")),
        format!("{word} "),
        "{method:?} keys {keys:?} + space"
    );
}

#[test]
fn corpus_is_large_enough() {
    let distinct: std::collections::BTreeSet<&str> = WORDS
        .iter()
        .chain(SPECIAL_WORDS)
        .copied()
        .chain(TONE_STYLE_PAIRS.iter().map(|(old, _)| *old))
        .collect();
    assert!(distinct.len() >= 250, "{} words", distinct.len());
}

#[test]
fn common_words_in_every_method() {
    for method in METHODS {
        for word in WORDS.iter().chain(SPECIAL_WORDS) {
            check_word(word, &options(method), method);
        }
    }
}

#[test]
fn title_and_upper_case_words() {
    for method in METHODS {
        for word in WORDS.iter().chain(SPECIAL_WORDS) {
            check_word(&title(word), &options(method), method);
            if word.chars().count() > 1 {
                check_word(&word.to_uppercase(), &options(method), method);
            }
        }
    }
}

#[test]
fn both_tone_styles() {
    for method in METHODS {
        for (old, new) in TONE_STYLE_PAIRS {
            check_word(old, &options(method), method);
            check_word(new, &modern(options(method)), method);
            // The same keys produce the other style when the option flips.
            let keys = encode(old, method);
            assert_eq!(type_keys(&modern(options(method)), &keys), *new, "{keys}");
            assert_eq!(type_keys(&options(method), &encode(new, method)), *old);
        }
        // Words with a coda or a marked vowel do not depend on the style.
        for word in [
            "hoàn", "toán", "huỳnh", "khoét", "thuyền", "quý", "của", "mía",
        ] {
            check_word(word, &options(method), method);
            check_word(word, &modern(options(method)), method);
        }
    }
}

#[test]
fn u_horn_and_o_horn_rules() {
    // `uơ` stays when the `ơ` ends the word; anything after it horns the `u` too.
    for (keys, expected) in [
        ("thuowr", "thuở"),
        ("huow", "huơ"),
        ("huowu", "hươu"),
        ("huowng", "hương"),
        ("tuowi", "tươi"),
        ("uowts", "ướt"),
        ("nguowif", "người"),
        ("nguwowif", "người"),
        ("nguoiwf", "người"),
        ("truongwf", "trường"),
        ("duowngfd", "đường"),
        ("muwowfi", "mười"),
        ("quow", "quơ"),
        ("huowf", "huờ"),
    ] {
        assert_eq!(type_keys(&telex(), keys), expected, "telex {keys}");
    }
    for (keys, expected) in [
        ("thuo73", "thuở"),
        ("huo7", "huơ"),
        ("huo7u", "hươu"),
        ("tuo7i", "tươi"),
        ("nguo72i", "người"),
        ("u7o7c1", "ước"),
        ("duo7ng29", "đường"),
    ] {
        assert_eq!(type_keys(&vni(), keys), expected, "vni {keys}");
    }
    for (keys, expected) in [
        ("thuo+?", "thuở"),
        ("huo+", "huơ"),
        ("huo+u", "hươu"),
        ("ngu+o+`i", "người"),
        ("u*o*c'", "ước"),
    ] {
        assert_eq!(type_keys(&viqr(), keys), expected, "viqr {keys}");
    }
}

#[test]
fn alternative_key_orders() {
    for (keys, expected) in [
        ("tieesng", "tiếng"),
        ("tiengse", "tiếng"),
        ("vietej", "việt"),
        ("hoangf", "hoàng"),
        ("thuowngr", "thưởng"),
        ("ddeens", "đến"),
        ("deensd", "đến"),
        ("nguyeenx", "nguyễn"),
        ("nguyenxe", "nguyễn"),
        ("quoocs", "quốc"),
        ("gioongs", "giống"),
        ("asf", "à"),
        ("asz", "a"),
    ] {
        assert_eq!(type_keys(&telex(), keys), expected, "telex {keys}");
    }
    for (keys, expected) in [
        ("tie1ng6", "tiếng"),
        ("vie5t6", "việt"),
        ("d9e6n1", "đến"),
        ("de6n19", "đến"),
        ("a12", "à"),
        ("a10", "a"),
    ] {
        assert_eq!(type_keys(&vni(), keys), expected, "vni {keys}");
    }
    for (keys, expected) in [
        ("tie'^ng", "tiếng"),
        ("vie.^t", "việt"),
        ("dde^'n", "đến"),
        ("a'`", "à"),
        ("a'-", "a"),
    ] {
        assert_eq!(type_keys(&viqr(), keys), expected, "viqr {keys}");
    }
}

const SENTENCES: &[&str] = &[
    "Tiếng Việt là ngôn ngữ chính thức của Việt Nam.",
    "Người dân ở đây rất thân thiện và hiếu khách.",
    "Chúng tôi đang học cách gõ tiếng Việt trên máy tính.",
    "Trường học của tôi nằm gần một con sông lớn.",
    "Quyển sách này giúp bạn hiểu thêm về lịch sử nước ta.",
    "Hôm nay, trời rất đẹp và mát.",
];

/// Keys for a sentence: words separated by `{SPACE}`; VIQR escapes bound punctuation.
fn encode_sentence(sentence: &str, method: Method) -> String {
    let mut keys = Vec::new();
    for token in sentence.split(' ') {
        let word: String = token.chars().filter(|c| c.is_alphabetic()).collect();
        let punctuation: String = token.chars().filter(|c| !c.is_alphabetic()).collect();
        let mut typed = encode(&word, method);
        for p in punctuation.chars() {
            if matches!(method, Method::Viqr) && matches!(p, '.' | '?' | '\'') {
                typed.push('\\');
            }
            typed.push(p);
        }
        keys.push(typed);
    }
    keys.join("{SPACE}")
}

#[test]
fn full_sentences_in_every_method() {
    for method in METHODS {
        for sentence in SENTENCES {
            let keys = encode_sentence(sentence, method);
            assert_eq!(
                type_keys(&options(method), &keys),
                *sentence,
                "{method:?}: {keys}"
            );
        }
    }
}

#[test]
fn full_sentences_in_nfd_output() {
    for method in METHODS {
        for sentence in SENTENCES {
            let keys = encode_sentence(sentence, method);
            let typed = type_keys(&common::nfd(options(method)), &keys);
            assert_eq!(
                typed,
                sentence.nfd().collect::<String>(),
                "{method:?}: {keys}"
            );
        }
    }
}

#[test]
fn sentences_with_mixed_case_and_old_new_style() {
    let sentence = "Hòa bình và sức khỏe cho mọi người.";
    let modern_sentence = "Hoà bình và sức khoẻ cho mọi người.";
    for method in METHODS {
        let keys = encode_sentence(sentence, method);
        assert_eq!(type_keys(&options(method), &keys), sentence, "{method:?}");
        assert_eq!(
            type_keys(&modern(options(method)), &keys),
            modern_sentence,
            "{method:?}"
        );
        let upper = sentence.to_uppercase();
        let keys = encode_sentence(&upper, method);
        assert_eq!(type_keys(&options(method), &keys), upper, "{method:?}");
    }
}
