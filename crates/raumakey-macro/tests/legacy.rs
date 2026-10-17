//! Legacy `trigger:replacement` text import and export (plan §4).

use raumakey_macro::{
    LegacyEncoding, LegacyExport, LegacyIssue, LegacyIssueReason, MAX_LEGACY_MACRO_LINES,
    MAX_LEGACY_TEXT_BYTES, MAX_REPLACEMENT_SCALARS, MAX_TRIGGER_SCALARS, detect_encoding,
    export_text, export_text_checked, is_legacy_exportable, parse_legacy_text,
};
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};

fn entry(id: u64, trigger: &str, replacement: &str) -> MacroEntry {
    MacroEntry {
        id: MacroId(id),
        trigger: trigger.to_owned(),
        replacement: replacement.to_owned(),
        enabled: true,
        boundary: BoundaryPolicy::Delimiter,
        case_policy: CasePolicy::Adapt,
    }
}

fn pairs(entries: &[MacroEntry]) -> Vec<(String, String)> {
    entries
        .iter()
        .map(|e| (e.trigger.clone(), e.replacement.clone()))
        .collect()
}

fn pair(trigger: &str, replacement: &str) -> (String, String) {
    (trigger.to_owned(), replacement.to_owned())
}

fn utf8_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(text.as_bytes());
    bytes
}

fn utf16le_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes
}

fn utf16be_bom(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    bytes
}

fn issue(line: usize, reason: LegacyIssueReason) -> LegacyIssue {
    LegacyIssue { line, reason }
}

const SAMPLE: &str = "ko:không\nvn:Việt Nam\n";

#[test]
fn parses_utf8_without_bom_with_default_entry_settings() {
    let import = parse_legacy_text(SAMPLE.as_bytes());
    assert_eq!(
        import.entries,
        [entry(1, "ko", "không"), entry(2, "vn", "Việt Nam")]
    );
    assert!(import.issues.is_empty());
}

#[test]
fn parses_every_supported_encoding_identically() {
    for (bytes, encoding) in [
        (SAMPLE.as_bytes().to_vec(), LegacyEncoding::Utf8),
        (utf8_bom(SAMPLE), LegacyEncoding::Utf8Bom),
        (utf16le_bom(SAMPLE), LegacyEncoding::Utf16Le),
        (utf16be_bom(SAMPLE), LegacyEncoding::Utf16Be),
    ] {
        assert_eq!(detect_encoding(&bytes), encoding);
        let import = parse_legacy_text(&bytes);
        assert_eq!(
            pairs(&import.entries),
            [pair("ko", "không"), pair("vn", "Việt Nam")],
            "{encoding:?}"
        );
        assert!(import.issues.is_empty(), "{encoding:?}");
    }
}

#[test]
fn supplementary_characters_survive_utf16() {
    let text = "cuoi:😀 vui\r\n";
    for bytes in [utf16le_bom(text), utf16be_bom(text)] {
        assert_eq!(
            pairs(&parse_legacy_text(&bytes).entries),
            [pair("cuoi", "😀 vui")]
        );
    }
}

#[test]
fn accepts_crlf_lf_and_cr_line_breaks_and_counts_physical_lines() {
    let import = parse_legacy_text(b"a:x\r\nb:y\rc:z\nd\n\ne:w");
    assert_eq!(
        pairs(&import.entries),
        [
            pair("a", "x"),
            pair("b", "y"),
            pair("c", "z"),
            pair("e", "w")
        ]
    );
    assert_eq!(
        import.issues,
        [issue(4, LegacyIssueReason::MissingSeparator)]
    );
}

#[test]
fn skips_blank_comment_and_header_lines() {
    let text = ";DO NOT DELETE: header of the legacy file\n\
                # comment: hash\n\
                // comment: slashes\n\
                ; comment: semicolon\n\
                \n\
                \u{20}\u{20}\t\n\
                \u{20}\u{20}# indented comment: x\n\
                ko:không\n\
                note:keep this line, DO NOT DELETE it\n";
    let import = parse_legacy_text(text.as_bytes());
    assert_eq!(pairs(&import.entries), [pair("ko", "không")]);
    assert!(import.issues.is_empty(), "{:?}", import.issues);
}

#[test]
fn splits_each_line_at_the_first_colon() {
    let import = parse_legacy_text("tg:10:30 sáng\nweb:https://example.invalid/a:b\n".as_bytes());
    assert_eq!(
        pairs(&import.entries),
        [
            pair("tg", "10:30 sáng"),
            pair("web", "https://example.invalid/a:b")
        ]
    );
}

#[test]
fn trims_whitespace_around_trigger_and_replacement() {
    let import = parse_legacy_text("  ko : không  \t\r\n".as_bytes());
    assert_eq!(pairs(&import.entries), [pair("ko", "không")]);
}

#[test]
fn whitespace_only_replacements_are_kept_verbatim() {
    // Raumakey decision: edge whitespace is trimmed for readability, but a replacement made only
    // of whitespace is kept as written (NFC) instead of being reported as empty.
    let import = parse_legacy_text(b"sp:\t\nnb:\xC2\xA0\nok:x\nsp2:  \t \r\n");
    assert_eq!(
        pairs(&import.entries),
        [
            pair("sp", "\t"),
            pair("nb", "\u{A0}"),
            pair("ok", "x"),
            pair("sp2", "  \t ")
        ]
    );
    assert!(import.issues.is_empty(), "{:?}", import.issues);
}

#[test]
fn whitespace_only_replacements_are_exportable_and_round_trip() {
    let entries = vec![
        entry(1, "sp", "\t"),
        entry(2, "nb", "\u{A0}"),
        entry(3, "ko", "không"),
        entry(4, "sp2", " \u{3000} "),
    ];
    for e in &entries {
        assert!(is_legacy_exportable(e), "{:?}", e.id);
    }
    let text = export_text(&entries);
    assert_eq!(text, "sp:\t\r\nnb:\u{A0}\r\nko:không\r\nsp2: \u{3000} \r\n");
    let import = parse_legacy_text(text.as_bytes());
    assert!(import.issues.is_empty(), "{:?}", import.issues);
    assert_eq!(import.entries, entries);
}

#[test]
fn invalid_lines_become_issues_with_line_numbers_and_reason_codes() {
    let long_trigger = "a".repeat(MAX_TRIGGER_SCALARS + 1);
    let long_replacement = "x".repeat(MAX_REPLACEMENT_SCALARS + 1);
    let text = format!(
        "nocolon\n:x\nko:\nk o:x\nk\u{0001}:x\n{long_trigger}:x\na:{long_replacement}\na:x\u{0000}y\nok:fine\n"
    );
    let import = parse_legacy_text(text.as_bytes());
    assert_eq!(import.entries, [entry(1, "ok", "fine")]);
    assert_eq!(
        import.issues,
        [
            issue(1, LegacyIssueReason::MissingSeparator),
            issue(2, LegacyIssueReason::EmptyTrigger),
            issue(3, LegacyIssueReason::EmptyReplacement),
            issue(4, LegacyIssueReason::TriggerWhitespace),
            issue(5, LegacyIssueReason::TriggerControl),
            issue(6, LegacyIssueReason::TriggerTooLong),
            issue(7, LegacyIssueReason::ReplacementTooLong),
            issue(8, LegacyIssueReason::ReplacementNul),
        ]
    );
}

#[test]
fn issues_carry_no_line_content() {
    let import = parse_legacy_text("bimat matkhau\nbi mat:noidung\n".as_bytes());
    assert_eq!(import.issues.len(), 2);
    let debug = format!("{:?}", import.issues);
    for secret in ["bimat", "matkhau", "noidung", "bi mat"] {
        assert!(!debug.contains(secret), "{debug}");
    }
    let record = toml::to_string(&import.issues[0]).unwrap();
    assert_eq!(record.trim(), "line = 1\nreason = \"missing-separator\"");
}

#[test]
fn reason_codes_are_stable() {
    let all = [
        (LegacyIssueReason::InvalidEncoding, "invalid-encoding"),
        (LegacyIssueReason::MissingSeparator, "missing-separator"),
        (LegacyIssueReason::EmptyTrigger, "empty-trigger"),
        (LegacyIssueReason::TriggerTooLong, "trigger-too-long"),
        (LegacyIssueReason::TriggerWhitespace, "trigger-whitespace"),
        (LegacyIssueReason::TriggerControl, "trigger-control"),
        (LegacyIssueReason::EmptyReplacement, "empty-replacement"),
        (
            LegacyIssueReason::ReplacementTooLong,
            "replacement-too-long",
        ),
        (LegacyIssueReason::ReplacementNul, "replacement-nul"),
        (LegacyIssueReason::TooLarge, "too-large"),
    ];
    for (reason, code) in all {
        assert_eq!(reason.code(), code);
        let record = toml::to_string(&LegacyIssue { line: 0, reason }).unwrap();
        assert_eq!(record.trim(), format!("line = 0\nreason = \"{code}\""));
    }
}

#[test]
fn an_invalid_utf8_line_does_not_affect_other_lines() {
    let mut bytes = "ko:không\nbad:".as_bytes().to_vec();
    bytes.extend_from_slice(&[0xFF, 0xC3]);
    bytes.extend_from_slice("\nvn:Việt\n".as_bytes());
    let import = parse_legacy_text(&bytes);
    assert_eq!(
        import.entries,
        [entry(1, "ko", "không"), entry(2, "vn", "Việt")]
    );
    assert_eq!(
        import.issues,
        [issue(2, LegacyIssueReason::InvalidEncoding)]
    );
}

#[test]
fn comment_lines_in_a_foreign_encoding_are_still_skipped() {
    let mut bytes = b"; ghi ch\xFA\n# \xE0\xE1\n".to_vec();
    bytes.extend_from_slice("ko:không\n".as_bytes());
    let import = parse_legacy_text(&bytes);
    assert_eq!(pairs(&import.entries), [pair("ko", "không")]);
    assert!(import.issues.is_empty(), "{:?}", import.issues);
}

#[test]
fn an_unpaired_utf16_surrogate_is_reported_for_its_line() {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in "ko:không\n".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    for unit in [u16::from(b'a'), u16::from(b':'), 0xD800, u16::from(b'\n')] {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    for unit in "vn:Việt".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    let import = parse_legacy_text(&bytes);
    assert_eq!(
        pairs(&import.entries),
        [pair("ko", "không"), pair("vn", "Việt")]
    );
    assert_eq!(
        import.issues,
        [issue(2, LegacyIssueReason::InvalidEncoding)]
    );
}

#[test]
fn an_odd_utf16_byte_count_invalidates_the_last_line_only() {
    let mut bytes = utf16le_bom("ko:không\r\nab:c");
    bytes.push(0x41);
    let import = parse_legacy_text(&bytes);
    assert_eq!(pairs(&import.entries), [pair("ko", "không")]);
    assert_eq!(
        import.issues,
        [issue(2, LegacyIssueReason::InvalidEncoding)]
    );
}

#[test]
fn decomposed_text_is_imported_as_nfc() {
    let import = parse_legacy_text("vie\u{0302}\u{0323}t:Vie\u{0302}\u{0323}t Nam\n".as_bytes());
    assert_eq!(pairs(&import.entries), [pair("việt", "Việt Nam")]);
}

#[test]
fn ids_follow_file_order_and_duplicate_triggers_are_kept() {
    let import = parse_legacy_text(b"a:1\nbad\na:2\nb:3\n");
    assert_eq!(
        import.entries,
        [entry(1, "a", "1"), entry(2, "a", "2"), entry(3, "b", "3")]
    );
    assert_eq!(
        import.issues,
        [issue(2, LegacyIssueReason::MissingSeparator)]
    );
}

#[test]
fn empty_and_bom_only_inputs_import_nothing() {
    let inputs: [&[u8]; 5] = [
        b"",
        &[0xEF, 0xBB, 0xBF],
        &[0xFF, 0xFE],
        &[0xFE, 0xFF],
        b"\r\n\n",
    ];
    for bytes in inputs {
        let import = parse_legacy_text(bytes);
        assert!(import.entries.is_empty(), "{bytes:?}");
        assert!(import.issues.is_empty(), "{bytes:?}");
    }
}

#[test]
fn utf16_without_a_bom_is_read_as_utf8() {
    let mut bytes = Vec::new();
    for unit in "ko:x".encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    assert_eq!(detect_encoding(&bytes), LegacyEncoding::Utf8);
    let import = parse_legacy_text(&bytes);
    assert!(import.entries.is_empty());
    assert_eq!(import.issues, [issue(1, LegacyIssueReason::TriggerControl)]);
}

#[test]
fn export_writes_trigger_colon_replacement_lines() {
    let text = export_text(&[entry(1, "ko", "không"), entry(2, "tg", "10:30")]);
    assert_eq!(text, "ko:không\r\ntg:10:30\r\n");
    assert_eq!(export_text(&[]), "");
}

#[test]
fn export_keeps_disabled_entries_and_ignores_policies() {
    let disabled = MacroEntry {
        enabled: false,
        boundary: BoundaryPolicy::Whitespace,
        case_policy: CasePolicy::Exact,
        ..entry(4, "ko", "không")
    };
    assert_eq!(export_text(&[disabled]), "ko:không\r\n");
}

#[test]
fn export_writes_nfc() {
    let text = export_text(&[entry(1, "vie\u{0302}\u{0323}t", "Vie\u{0302}\u{0323}t Nam")]);
    assert_eq!(text, "việt:Việt Nam\r\n");
}

#[test]
fn export_skips_entries_the_format_cannot_represent() {
    let unrepresentable = [
        entry(1, "a:b", "x"),
        entry(2, "#tag", "x"),
        entry(3, ";x", "x"),
        entry(4, "//x", "x"),
        entry(5, "ml", "line 1\nline 2"),
        entry(6, "cr", "a\rb"),
        entry(7, "lead", " x"),
        entry(8, "trail", "x\u{2028}"),
        entry(9, "hdr", "please DO NOT DELETE"),
        entry(10, "", "x"),
        entry(11, "k o", "x"),
        entry(12, "nul", "a\u{0000}"),
    ];
    for e in &unrepresentable {
        assert!(!is_legacy_exportable(e), "{:?}", e.id);
    }
    let mut all = unrepresentable.to_vec();
    all.push(entry(13, "ok", "fine"));
    assert_eq!(export_text(&all), "ok:fine\r\n");
}

#[test]
fn checked_export_reports_every_skipped_entry() {
    let entries = [
        entry(1, "dc", "dòng 1\r\ndòng 2"),
        entry(2, "ko", "không"),
        entry(3, "a:b", "x"),
        entry(4, "", "invalid"),
        entry(5, "vn", "Việt Nam"),
        entry(6, "lead", " x"),
    ];
    let export = export_text_checked(&entries);
    assert_eq!(export.text, "ko:không\r\nvn:Việt Nam\r\n");
    assert_eq!(
        export.skipped,
        [MacroId(1), MacroId(3), MacroId(4), MacroId(6)]
    );
    // The plain export writes the same text.
    assert_eq!(export_text(&entries), export.text);
}

#[test]
fn checked_export_of_representable_entries_skips_nothing() {
    let export = export_text_checked(&[entry(1, "ko", "không"), entry(2, "sp", "\t")]);
    assert_eq!(
        export,
        LegacyExport {
            text: "ko:không\r\nsp:\t\r\n".to_owned(),
            skipped: Vec::new(),
        }
    );
    assert_eq!(export_text_checked(&[]), LegacyExport::default());
}

#[test]
fn export_debug_output_does_not_reveal_macro_text() {
    let export = export_text_checked(&[entry(1, "bimat", "noidung"), entry(2, "ml", "a\nb")]);
    let text = format!("{export:?}");
    assert!(!text.contains("bimat"), "{text}");
    assert!(!text.contains("noidung"), "{text}");
    assert!(text.contains("MacroId(2)"), "{text}");
}

#[test]
fn representable_entries_are_exportable() {
    for e in [
        entry(1, "ko", "không"),
        entry(2, "a#", "x # y"),
        entry(3, "x/", "a // b ; c"),
        entry(4, "tab", "a\tb"),
        entry(5, "😀", "vui"),
        entry(6, "vie\u{0302}\u{0323}t", "x"),
    ] {
        assert!(is_legacy_exportable(&e), "{:?}", e.id);
    }
}

#[test]
fn export_then_parse_round_trips() {
    let entries = vec![
        entry(1, "ko", "không"),
        entry(2, "vn", "Việt Nam"),
        entry(3, "tg", "thời gian: 10:30"),
        entry(4, "VN", "VIỆT NAM"),
        entry(5, "a#", "x # y // z ; w"),
        entry(6, "😀", "mặt cười 😀"),
        entry(7, "tab", "cột 1\tcột 2"),
    ];
    let text = export_text(&entries);
    for bytes in [
        text.as_bytes().to_vec(),
        utf8_bom(&text),
        utf16le_bom(&text),
        utf16be_bom(&text),
    ] {
        let import = parse_legacy_text(&bytes);
        assert!(import.issues.is_empty(), "{:?}", import.issues);
        assert_eq!(import.entries, entries);
    }
}

#[test]
fn a_trigger_starting_with_a_byte_order_mark_is_not_exportable() {
    // As the first line of a UTF-8 file it would be indistinguishable from a byte order mark.
    let bom_trigger = entry(1, "\u{FEFF}", "a");
    assert!(!is_legacy_exportable(&bom_trigger));
    assert!(!is_legacy_exportable(&entry(2, "\u{FEFF}ko", "a")));
    assert!(is_legacy_exportable(&entry(3, "k\u{FEFF}o", "a")));
    assert_eq!(
        export_text(&[bom_trigger, entry(4, "ko", "không")]),
        "ko:không\r\n"
    );
}

#[test]
fn extra_byte_order_marks_at_the_start_are_ignored() {
    for bytes in [
        utf8_bom("\u{FEFF}ko:x\n"),
        "\u{FEFF}\u{FEFF}ko:x\n".as_bytes().to_vec(),
        utf16le_bom("\u{FEFF}ko:x\n"),
        utf16be_bom("\u{FEFF}ko:x\n"),
    ] {
        let import = parse_legacy_text(&bytes);
        assert_eq!(pairs(&import.entries), [pair("ko", "x")], "{bytes:?}");
        assert!(import.issues.is_empty());
    }
    // Only the start of the text is affected.
    let import = parse_legacy_text("a:b\n\u{FEFF}ko:x\n".as_bytes());
    assert_eq!(
        pairs(&import.entries),
        [pair("a", "b"), pair("\u{FEFF}ko", "x")]
    );
}

#[test]
fn a_dangling_utf16_byte_on_an_empty_last_line_is_reported() {
    let import = parse_legacy_text(&[0xFF, 0xFE, 0x41]);
    assert!(import.entries.is_empty());
    assert_eq!(
        import.issues,
        [issue(1, LegacyIssueReason::InvalidEncoding)]
    );

    let mut bytes = utf16be_bom("ko:không\n");
    bytes.push(0x00);
    let import = parse_legacy_text(&bytes);
    assert_eq!(pairs(&import.entries), [pair("ko", "không")]);
    assert_eq!(
        import.issues,
        [issue(2, LegacyIssueReason::InvalidEncoding)]
    );
}

/// "ko:không" plus a comment line that pads the input to exactly `len` bytes.
fn padded_input(prefix: &[u8], len: usize) -> Vec<u8> {
    let mut bytes = prefix.to_vec();
    bytes.extend_from_slice("ko:không\n".as_bytes());
    bytes.resize(len, b'#');
    bytes
}

#[test]
fn input_at_the_byte_limit_is_parsed() {
    for prefix in [&[][..], &[0xEF, 0xBB, 0xBF][..]] {
        let import = parse_legacy_text(&padded_input(prefix, MAX_LEGACY_TEXT_BYTES));
        assert_eq!(pairs(&import.entries), [pair("ko", "không")]);
        assert!(import.issues.is_empty(), "{:?}", import.issues);
    }
}

#[test]
fn input_over_the_byte_limit_is_rejected_without_parsing() {
    let mut utf16 = utf16le_bom("ko:không\n");
    utf16.resize(MAX_LEGACY_TEXT_BYTES + 1, 0);
    for bytes in [
        padded_input(&[], MAX_LEGACY_TEXT_BYTES + 1),
        padded_input(&[0xEF, 0xBB, 0xBF], MAX_LEGACY_TEXT_BYTES + 1),
        utf16,
    ] {
        let import = parse_legacy_text(&bytes);
        assert!(import.entries.is_empty());
        assert_eq!(import.issues, [issue(0, LegacyIssueReason::TooLarge)]);
    }
}

/// `count` macro lines `k<i>:v`, after comment and blank lines that do not count.
fn macro_lines(count: usize) -> String {
    let mut text = String::from("; header\n\n# comment\n");
    for index in 0..count {
        text.push_str(&format!("k{index}:v\n"));
    }
    text
}

#[test]
fn input_with_the_maximum_number_of_macro_lines_is_parsed() {
    let text = macro_lines(MAX_LEGACY_MACRO_LINES);
    for bytes in [text.as_bytes().to_vec(), utf16be_bom(&text)] {
        let import = parse_legacy_text(&bytes);
        assert_eq!(import.entries.len(), MAX_LEGACY_MACRO_LINES);
        assert!(import.issues.is_empty(), "{:?}", import.issues);
    }
}

#[test]
fn input_with_too_many_macro_lines_is_rejected() {
    // Lines that become issues count as macro lines too, which bounds the issue list.
    let mut with_bad_line = macro_lines(MAX_LEGACY_MACRO_LINES);
    with_bad_line.push_str("broken\n");
    let all_bad = "x\n".repeat(MAX_LEGACY_MACRO_LINES + 1);
    for text in [with_bad_line, all_bad] {
        for bytes in [text.as_bytes().to_vec(), utf16le_bom(&text)] {
            let import = parse_legacy_text(&bytes);
            assert!(import.entries.is_empty());
            assert_eq!(import.issues, [issue(0, LegacyIssueReason::TooLarge)]);
        }
    }
}

#[test]
fn many_blank_and_comment_lines_do_not_count_as_macro_lines() {
    let mut text = "\n; c\n".repeat(MAX_LEGACY_MACRO_LINES);
    text.push_str("ko:không\n");
    let import = parse_legacy_text(text.as_bytes());
    assert_eq!(pairs(&import.entries), [pair("ko", "không")]);
    assert!(import.issues.is_empty());
}

#[test]
fn import_debug_output_does_not_reveal_macro_text() {
    let import = parse_legacy_text("bimat:noidung\nbroken\n".as_bytes());
    let text = format!("{import:?}");
    assert!(!text.contains("bimat"), "{text}");
    assert!(!text.contains("noidung"), "{text}");
    assert!(text.contains("MissingSeparator"), "{text}");
}
