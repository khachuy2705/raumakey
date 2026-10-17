//! Every Vietnamese letter survives a round trip through every lossless charset, in text form
//! and in byte form; TCVN3 keeps 74 letters exactly and lowercases the 60 uppercase toned ones.

mod common;

use common::*;
use raumakey_converter::{
    Charset, decode, decode_bytes, encode_bytes_with_report, encode_with_report,
};

fn lossless() -> impl Iterator<Item = Charset> {
    Charset::ALL
        .into_iter()
        .filter(|charset| charset.is_lossless())
}

#[test]
fn there_are_134_distinct_letters_under_test() {
    let mut letters = letters();
    assert_eq!(letters.len(), 134);
    letters.sort_unstable();
    letters.dedup();
    assert_eq!(letters.len(), 134);
}

#[test]
fn every_letter_round_trips_in_text_form_for_lossless_charsets() {
    for charset in lossless() {
        for letter in letters() {
            let letter = letter.to_string();
            let report = encode_with_report(&letter, charset).unwrap();
            assert!(!report.lossy, "{charset} {letter}");
            assert_eq!(decode(&report.text, charset).unwrap(), letter, "{charset}");
        }
        let report = encode_with_report(LETTERS, charset).unwrap();
        assert!(!report.lossy, "{charset}");
        assert_eq!(decode(&report.text, charset).unwrap(), LETTERS, "{charset}");
    }
}

#[test]
fn every_letter_round_trips_in_byte_form_for_lossless_charsets() {
    for charset in lossless() {
        for letter in letters() {
            let letter = letter.to_string();
            let report = encode_bytes_with_report(&letter, charset).unwrap();
            assert!(!report.lossy, "{charset} {letter}");
            assert_eq!(
                decode_bytes(&report.bytes, charset).unwrap(),
                letter,
                "{charset}"
            );
        }
        let report = encode_bytes_with_report(SENTENCE, charset).unwrap();
        assert!(!report.lossy, "{charset}");
        assert_eq!(
            decode_bytes(&report.bytes, charset).unwrap(),
            SENTENCE,
            "{charset}"
        );
    }
}

#[test]
fn tcvn3_round_trips_74_letters_and_lowercases_60() {
    let (mut exact, mut lowered) = (0, 0);
    for letter in letters() {
        let text = letter.to_string();
        let report = encode_bytes_with_report(&text, Charset::Tcvn3).unwrap();
        let back = decode_bytes(&report.bytes, Charset::Tcvn3).unwrap();
        if is_toned_upper(letter) {
            assert!(report.lossy, "{letter}");
            assert_eq!(back, letter.to_lowercase().to_string());
            lowered += 1;
        } else {
            assert!(!report.lossy, "{letter}");
            assert_eq!(back, text);
            exact += 1;
        }
    }
    assert_eq!((exact, lowered), (74, 60));
}

#[test]
fn byte_form_and_text_form_agree_for_legacy_charsets() {
    for charset in Charset::ALL.into_iter().filter(|c| c.is_legacy_byte()) {
        let text = encode_with_report(SENTENCE, charset).unwrap();
        let bytes = encode_bytes_with_report(SENTENCE, charset).unwrap();
        assert_eq!(cp1252(&bytes.bytes), text.text, "{charset}");
        assert_eq!(bytes.lossy, text.lossy, "{charset}");
    }
}
