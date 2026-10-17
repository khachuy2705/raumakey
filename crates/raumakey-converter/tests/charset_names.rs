//! Stable charset and option names (used by `config.toml` and the `convert` subcommand).

use raumakey_converter::{CaseTransform, Charset};

const NAMES: [(Charset, &str); 10] = [
    (Charset::Unicode, "unicode"),
    (Charset::UnicodeNfd, "unicode-nfd"),
    (Charset::Viqr, "viqr"),
    (Charset::Tcvn3, "tcvn3"),
    (Charset::VniWindows, "vni-windows"),
    (Charset::Viscii, "viscii"),
    (Charset::Cp1258, "cp1258"),
    (Charset::NcrDecimal, "ncr-decimal"),
    (Charset::NcrHex, "ncr-hex"),
    (Charset::Utf8Literal, "utf8-literal"),
];

#[test]
fn every_charset_has_its_stable_name() {
    for (charset, name) in NAMES {
        assert_eq!(charset.name(), name);
        assert_eq!(charset.to_string(), name);
        assert_eq!(name.parse::<Charset>(), Ok(charset));
    }
}

#[test]
fn all_lists_every_charset_once_in_declaration_order() {
    let listed: Vec<Charset> = Charset::ALL.to_vec();
    let expected: Vec<Charset> = NAMES.iter().map(|(charset, _)| *charset).collect();
    assert_eq!(listed, expected);
}

#[test]
fn parsing_ignores_ascii_case_and_surrounding_whitespace() {
    assert_eq!("TCVN3".parse::<Charset>(), Ok(Charset::Tcvn3));
    assert_eq!(" Vni-Windows ".parse::<Charset>(), Ok(Charset::VniWindows));
}

#[test]
fn parsing_rejects_unknown_names() {
    for bad in ["", "utf8", "vni", "tcvn", "unicode nfd", "ncr", "x"] {
        let error = bad.parse::<Charset>().unwrap_err();
        assert_eq!(error.to_string(), "unknown charset name");
    }
}

#[test]
fn only_tcvn3_is_lossy_for_vietnamese_letters() {
    for (charset, _) in NAMES {
        assert_eq!(
            charset.is_lossless(),
            charset != Charset::Tcvn3,
            "{charset}"
        );
    }
}

#[test]
fn legacy_byte_charsets_are_flagged() {
    let legacy: Vec<Charset> = Charset::ALL
        .into_iter()
        .filter(|charset| charset.is_legacy_byte())
        .collect();
    assert_eq!(
        legacy,
        [
            Charset::Tcvn3,
            Charset::VniWindows,
            Charset::Viscii,
            Charset::Cp1258
        ]
    );
}

#[test]
fn case_transform_names_round_trip() {
    for (case, name) in [
        (CaseTransform::None, "none"),
        (CaseTransform::Upper, "upper"),
        (CaseTransform::Lower, "lower"),
        (CaseTransform::Title, "title"),
    ] {
        assert_eq!(case.name(), name);
        assert_eq!(case.to_string(), name);
        assert_eq!(name.parse::<CaseTransform>(), Ok(case));
        assert_eq!(name.to_uppercase().parse::<CaseTransform>(), Ok(case));
    }
    let error = "sentence".parse::<CaseTransform>().unwrap_err();
    assert_eq!(error.to_string(), "unknown case transform name");
}

#[test]
fn default_options_change_nothing() {
    let options = raumakey_converter::ConvertOptions::default();
    assert_eq!(options.case, CaseTransform::None);
    assert!(!options.remove_tones);
}
