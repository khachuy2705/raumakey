//! Golden vectors of plan §3.10 (Telex unless stated), plus real words after the `qu` onset
//! that the plan's §3.4 table omits.

mod common;

use common::{modern, telex, type_keys, viqr, vni};

const TELEX: &[(&str, &str)] = &[
    ("aa", "â"),
    ("dd", "đ"),
    ("as", "á"),
    ("aw", "ă"),
    ("ow", "ơ"),
    ("uw", "ư"),
    ("w", "ư"),
    ("ww", "w"),
    ("tw", "tư"),
    ("nhw", "như"),
    ("ass", "as"),
    ("aaa", "aa"),
    ("ddd", "dd"),
    ("tieengs", "tiếng"),
    ("vieetj", "việt"),
    ("nguwowif", "người"),
    ("nguoiwf", "người"),
    ("thuowr", "thuở"),
    ("huow", "huơ"),
    ("truowngf", "trường"),
    ("ruowuj", "rượu"),
    ("khuyeens", "khuyến"),
    ("quas", "quá"),
    ("quys", "quý"),
    ("quyeets", "quyết"),
    ("gias", "giá"),
    ("gif", "gì"),
    ("giuwax", "giữa"),
    ("giuwowngf", "giường"),
    ("ddi", "đi"),
    ("did", "đi"),
    ("Ddaay", "Đây"),
    ("VIEETJ", "VIỆT"),
    ("hoafn", "hoàn"),
    ("chaof", "chào"),
    ("cuar", "của"),
    ("muoons", "muốn"),
    ("nghieeng", "nghiêng"),
    ("awa", "â"),
    ("tesst", "test"),
    ("class", "class"),
];

#[test]
fn telex_golden_vectors() {
    for (keys, expected) in TELEX {
        assert_eq!(type_keys(&telex(), keys), *expected, "telex {keys}");
    }
}

#[test]
fn tone_style_golden_vectors() {
    for (keys, old, new) in [
        ("hoaf", "hòa", "hoà"),
        ("thuyr", "thủy", "thuỷ"),
        ("khoer", "khỏe", "khoẻ"),
        ("hoafn", "hoàn", "hoàn"),
    ] {
        assert_eq!(type_keys(&telex(), keys), old, "old {keys}");
        assert_eq!(type_keys(&modern(telex()), keys), new, "modern {keys}");
    }
}

#[test]
fn boundary_golden_vectors() {
    assert_eq!(type_keys(&telex(), "after{SPACE}"), "after ");
    assert_eq!(type_keys(&telex(), "windows{SPACE}"), "windows ");
    assert_eq!(type_keys(&telex(), "class{SPACE}"), "class ");
    assert_eq!(type_keys(&telex(), "tesst{SPACE}"), "test ");
}

#[test]
fn vni_golden_vectors() {
    for (keys, expected) in [
        ("tie6ng1", "tiếng"),
        ("vie65t", "việt"),
        ("nguo72i", "người"),
        ("a66", "a6"),
        ("d9", "đ"),
    ] {
        assert_eq!(type_keys(&vni(), keys), expected, "vni {keys}");
    }
}

/// `ê` + `t`, `au`, and `o` after `qu` (quết trầu, quệt, quạu quọ) with default options in
/// every method; the words stay as typed at a boundary (no auto-restore).
#[test]
fn after_qu_golden_vectors() {
    for (telex_keys, vni_keys, viqr_keys, expected) in [
        ("queets", "que6t1", "que^t'", "quết"),
        ("queetj", "que6t5", "que^t.", "quệt"),
        ("quauj", "quau5", "quau.", "quạu"),
        ("quoj", "quo5", "quo.", "quọ"),
        ("Queets", "Que6t1", "Que^t'", "Quết"),
        ("QUAUJ", "QUAU5", "QUAU.", "QUẠU"),
    ] {
        for (method, options, keys) in [
            ("telex", telex(), telex_keys),
            ("vni", vni(), vni_keys),
            ("viqr", viqr(), viqr_keys),
        ] {
            assert_eq!(type_keys(&options, keys), expected, "{method} {keys}");
            assert_eq!(
                type_keys(&options, &format!("{keys}{{SPACE}}")),
                format!("{expected} "),
                "{method} {keys} + space"
            );
        }
    }
}

#[test]
fn viqr_golden_vectors() {
    for (keys, expected) in [
        ("tie^'ng", "tiếng"),
        ("vie^.t", "việt"),
        ("ngu+o+`i", "người"),
        ("\\'", "'"),
        ("dd", "đ"),
    ] {
        assert_eq!(type_keys(&viqr(), keys), expected, "viqr {keys}");
    }
}
