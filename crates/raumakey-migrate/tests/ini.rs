//! Tolerant INI parsing of `setting.ini`.

use raumakey_migrate::{
    IniDocument, IniEncoding, IniEntry, IniIssue, IniIssueKind, MAX_INI_ITEMS, parse_ini,
};

fn entry(line: usize, section: &str, key: &str, value: &str) -> IniEntry {
    IniEntry {
        line,
        section: section.to_owned(),
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

fn issue(line: usize, kind: IniIssueKind) -> IniIssue {
    IniIssue { line, kind }
}

fn utf16(text: &str, little_endian: bool) -> Vec<u8> {
    let mut bytes = if little_endian {
        vec![0xFF, 0xFE]
    } else {
        vec![0xFE, 0xFF]
    };
    for unit in text.encode_utf16() {
        if little_endian {
            bytes.extend(unit.to_le_bytes());
        } else {
            bytes.extend(unit.to_be_bytes());
        }
    }
    bytes
}

#[test]
fn sections_keys_and_values_are_read() {
    let doc = parse_ini(b"[General]\nA=1\nB = two  words \n[Other]\nC=\n");
    assert_eq!(doc.encoding, IniEncoding::Utf8);
    assert_eq!(
        doc.entries,
        [
            entry(2, "General", "A", "1"),
            entry(3, "General", "B", "two  words"),
            entry(5, "Other", "C", ""),
        ]
    );
    assert!(doc.issues.is_empty());
}

#[test]
fn keys_before_any_section_belong_to_the_root_section() {
    let doc = parse_ini(b"A=1\n[S]\nB=2\n");
    assert_eq!(
        doc.entries,
        [entry(1, "", "A", "1"), entry(3, "S", "B", "2")]
    );
}

#[test]
fn comments_and_blank_lines_are_skipped() {
    let doc = parse_ini(b"; comment\n# comment\n\n   \n  ; indented\nA=1\n");
    assert_eq!(doc.entries, [entry(6, "", "A", "1")]);
    assert!(doc.issues.is_empty());
}

#[test]
fn values_are_kept_raw_apart_from_surrounding_whitespace() {
    let doc = parse_ini(b"A=b=c ; not a comment\nB=\"quoted value\"\nC=\t tabbed\t\n");
    assert_eq!(
        doc.entries,
        [
            entry(1, "", "A", "b=c ; not a comment"),
            entry(2, "", "B", "\"quoted value\""),
            entry(3, "", "C", "tabbed"),
        ]
    );
}

#[test]
fn every_line_ending_counts_one_line() {
    let doc = parse_ini(b"A=1\r\nB=2\nC=3\rD=4");
    let lines: Vec<usize> = doc.entries.iter().map(|entry| entry.line).collect();
    assert_eq!(lines, [1, 2, 3, 4]);
}

#[test]
fn a_utf8_byte_order_mark_is_detected_and_stripped() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("[S]\nA=Tiếng Việt\n".as_bytes());
    let doc = parse_ini(&bytes);
    assert_eq!(doc.encoding, IniEncoding::Utf8Bom);
    assert_eq!(doc.entries, [entry(2, "S", "A", "Tiếng Việt")]);
}

#[test]
fn repeated_byte_order_marks_are_ignored() {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice("\u{feff}\u{feff}A=1\n".as_bytes());
    let doc = parse_ini(&bytes);
    assert_eq!(doc.entries, [entry(1, "", "A", "1")]);
    assert!(doc.issues.is_empty());
}

#[test]
fn utf16_with_a_byte_order_mark_is_decoded() {
    let text = "[S]\r\nA=Tiếng\r\n; c\r\nB=x\r\n";
    for (little_endian, encoding) in [(true, IniEncoding::Utf16Le), (false, IniEncoding::Utf16Be)] {
        let doc = parse_ini(&utf16(text, little_endian));
        assert_eq!(doc.encoding, encoding);
        assert_eq!(
            doc.entries,
            [entry(2, "S", "A", "Tiếng"), entry(4, "S", "B", "x")]
        );
        assert!(doc.issues.is_empty());
    }
}

#[test]
fn duplicate_keys_keep_the_last_value_and_report_the_earlier_line() {
    let doc = parse_ini(b"[S]\nA=1\nB=2\na=3\n[s]\nA=4\n");
    assert_eq!(
        doc.entries,
        [entry(3, "S", "B", "2"), entry(6, "S", "A", "4")]
    );
    assert_eq!(
        doc.issues,
        [
            issue(2, IniIssueKind::DuplicateKey),
            issue(4, IniIssueKind::DuplicateKey)
        ]
    );
}

#[test]
fn equal_keys_in_different_sections_are_not_duplicates() {
    let doc = parse_ini(b"A=0\n[S]\nA=1\n[T]\nA=2\n");
    assert_eq!(doc.entries.len(), 3);
    assert!(doc.issues.is_empty());
}

#[test]
fn malformed_lines_are_reported_by_line_number() {
    let doc = parse_ini(b"[S]\nno separator\n=value\nA=1\n");
    assert_eq!(doc.entries, [entry(4, "S", "A", "1")]);
    assert_eq!(
        doc.issues,
        [
            issue(2, IniIssueKind::Malformed),
            issue(3, IniIssueKind::Malformed)
        ]
    );
}

#[test]
fn a_malformed_section_header_orphans_the_keys_below_it() {
    let doc =
        parse_ini(b"[S]\nA=1\n[Broken\nB=2\n[]\nC=3\n[T] trailing\nD=4\n[U] ; comment\nE=5\n");
    assert_eq!(
        doc.entries,
        [entry(2, "S", "A", "1"), entry(10, "U", "E", "5")]
    );
    let lines: Vec<usize> = doc.issues.iter().map(|issue| issue.line).collect();
    assert_eq!(lines, [3, 4, 5, 6, 7, 8]);
    assert!(
        doc.issues
            .iter()
            .all(|issue| issue.kind == IniIssueKind::Malformed)
    );
}

#[test]
fn names_are_trimmed() {
    let doc = parse_ini(b"[  S  ]\n  K  =  v  \n");
    assert_eq!(doc.entries, [entry(2, "S", "K", "v")]);
}

#[test]
fn undecodable_lines_are_isolated() {
    let doc = parse_ini(b"A=1\nB=\xff\xfe\nC=3\n");
    assert_eq!(
        doc.entries,
        [entry(1, "", "A", "1"), entry(3, "", "C", "3")]
    );
    assert_eq!(doc.issues, [issue(2, IniIssueKind::Undecodable)]);
}

#[test]
fn undecodable_utf16_lines_are_isolated() {
    let mut bytes = utf16("A=1\r\n", true);
    bytes.extend(0xD800u16.to_le_bytes()); // unpaired surrogate
    bytes.extend(utf16("=x\r\nC=3\r\n", true).split_off(2));
    bytes.push(b'Z'); // dangling odd byte on the last line
    let doc = parse_ini(&bytes);
    assert_eq!(
        doc.entries,
        [entry(1, "", "A", "1"), entry(3, "", "C", "3")]
    );
    assert_eq!(
        doc.issues,
        [
            issue(2, IniIssueKind::Undecodable),
            issue(4, IniIssueKind::Undecodable)
        ]
    );
}

#[test]
fn empty_input_yields_nothing() {
    let doc = parse_ini(b"");
    assert_eq!(doc.encoding, IniEncoding::Utf8);
    assert!(doc.entries.is_empty());
    assert!(doc.issues.is_empty());
}

#[test]
fn parsing_stops_at_the_item_limit_with_one_issue() {
    let mut text = String::new();
    for index in 0..MAX_INI_ITEMS + 10 {
        text.push_str(&format!("k{index}=v\n"));
    }
    let doc = parse_ini(text.as_bytes());
    assert_eq!(doc.entries.len(), MAX_INI_ITEMS);
    assert_eq!(
        doc.issues,
        [issue(MAX_INI_ITEMS + 1, IniIssueKind::LimitExceeded)]
    );
}

#[test]
fn encodings_and_issue_kinds_have_stable_names() {
    let encodings = [
        (IniEncoding::Utf8, "utf-8"),
        (IniEncoding::Utf8Bom, "utf-8-bom"),
        (IniEncoding::Utf16Le, "utf-16le"),
        (IniEncoding::Utf16Be, "utf-16be"),
    ];
    for (encoding, name) in encodings {
        assert_eq!(encoding.as_str(), name);
    }
}

#[test]
fn debug_output_shows_no_values() {
    let doc: IniDocument = parse_ini(b"[S]\nK=private value\n");
    let text = format!("{doc:?}");
    assert!(!text.contains("private"), "{text}");
}
