//! `RT_STRING` blocks of sixteen counted UTF-16 strings.

use crate::support::{Id, find, of_type, parse_res, u16_at};
use raumakey_resources::{
    LANG_EN_US, LANG_VIETNAMESE, RT_STRING, ResWriter, ResourceError, StringTable,
};

/// Decodes one block and checks it holds exactly sixteen counted strings.
fn decode_block(data: &[u8]) -> Vec<String> {
    let mut pos = 0;
    let mut strings = Vec::new();
    for _ in 0..16 {
        let len = usize::from(u16_at(data, pos));
        pos += 2;
        let units: Vec<u16> = (0..len)
            .map(|index| u16_at(data, pos + 2 * index))
            .collect();
        pos += 2 * len;
        strings.push(String::from_utf16(&units).unwrap());
    }
    assert_eq!(pos, data.len(), "a block holds exactly sixteen strings");
    strings
}

#[test]
fn strings_are_grouped_in_blocks_of_sixteen() {
    let mut table = StringTable::new();
    table.insert(1, "Bật tiếng Việt").unwrap();
    table.insert(15, "x").unwrap();
    table.insert(16, "Tắt").unwrap();
    table.insert(u16::MAX, "last").unwrap();
    let mut writer = ResWriter::new();
    writer.add_string_table(&table, LANG_VIETNAMESE).unwrap();
    let entries = parse_res(&writer.to_bytes());

    let blocks = of_type(&entries, RT_STRING);
    let names: Vec<&Id> = blocks.iter().map(|entry| &entry.name).collect();
    assert_eq!(
        names,
        [&Id::Ordinal(1), &Id::Ordinal(2), &Id::Ordinal(4096)],
        "block id = string id / 16 + 1"
    );
    for block in &blocks {
        assert_eq!(block.language, LANG_VIETNAMESE);
        assert_eq!(block.memory_flags, 0x1030, "MOVEABLE | PURE | DISCARDABLE");
    }

    let first = decode_block(&find(&entries, RT_STRING, 1).data);
    assert_eq!(first[0], "");
    assert_eq!(first[1], "Bật tiếng Việt");
    assert!(first[2..15].iter().all(String::is_empty));
    assert_eq!(first[15], "x");

    let second = decode_block(&find(&entries, RT_STRING, 2).data);
    assert_eq!(second[0], "Tắt");
    assert!(second[1..].iter().all(String::is_empty));

    let last = decode_block(&find(&entries, RT_STRING, 4096).data);
    assert_eq!(last[15], "last");
}

#[test]
fn string_lengths_count_utf16_units() {
    let mut table = StringTable::new();
    table.insert(0, "a\u{1D519}").unwrap();
    let mut writer = ResWriter::new();
    writer.add_string_table(&table, LANG_EN_US).unwrap();
    let entries = parse_res(&writer.to_bytes());
    let data = &find(&entries, RT_STRING, 1).data;
    assert_eq!(u16_at(data, 0), 3, "one BMP unit plus a surrogate pair");
    assert_eq!(decode_block(data)[0], "a\u{1D519}");
}

#[test]
fn table_rejects_duplicates_and_overlong_strings() {
    let mut table = StringTable::new();
    table.insert(5, "a").unwrap();
    assert_eq!(table.insert(5, "b"), Err(ResourceError::Duplicate));
    assert_eq!(table.get(5), Some("a"));
    let too_long = "x".repeat(usize::from(u16::MAX) + 1);
    assert_eq!(table.insert(6, &too_long), Err(ResourceError::TooLarge));
    assert_eq!(table.get(6), None);
    let longest = "x".repeat(usize::from(u16::MAX));
    table.insert(7, &longest).unwrap();
    assert_eq!(table.len(), 2);
    assert!(!table.is_empty());

    let mut writer = ResWriter::new();
    writer.add_string_table(&table, LANG_EN_US).unwrap();
    let entries = parse_res(&writer.to_bytes());
    let strings = decode_block(&find(&entries, RT_STRING, 1).data);
    assert_eq!(strings[7].len(), usize::from(u16::MAX));
}

#[test]
fn empty_tables_add_nothing() {
    let table = StringTable::new();
    assert!(table.is_empty());
    assert_eq!(table.len(), 0);
    let mut writer = ResWriter::new();
    writer.add_string_table(&table, LANG_EN_US).unwrap();
    assert!(writer.is_empty());
}

#[test]
fn duplicate_blocks_are_rejected_without_partial_writes() {
    let mut table = StringTable::new();
    table.insert(1, "a").unwrap();
    table.insert(40, "b").unwrap();
    let mut writer = ResWriter::new();
    writer.add_string_table(&table, LANG_EN_US).unwrap();
    assert_eq!(writer.len(), 2);

    // Block 7 would be new, but block 1 already exists in this language.
    let mut other = StringTable::new();
    other.insert(100, "c").unwrap();
    other.insert(2, "d").unwrap();
    assert_eq!(
        writer.add_string_table(&other, LANG_EN_US),
        Err(ResourceError::Duplicate)
    );
    assert_eq!(writer.len(), 2, "nothing from the failed table was added");

    writer.add_string_table(&other, LANG_VIETNAMESE).unwrap();
    assert_eq!(writer.len(), 4);
}

#[test]
fn string_tables_use_their_explicit_language_not_the_writer_default() {
    let mut table = StringTable::new();
    table.insert(3, "Raumakey").unwrap();
    let mut writer = ResWriter::with_language(LANG_VIETNAMESE);
    writer.add_string_table(&table, LANG_EN_US).unwrap();
    writer.add_string_table(&table, LANG_VIETNAMESE).unwrap();
    let entries = parse_res(&writer.to_bytes());
    let languages: Vec<u16> = of_type(&entries, RT_STRING)
        .iter()
        .map(|entry| entry.language)
        .collect();
    assert_eq!(languages, [LANG_EN_US, LANG_VIETNAMESE]);
}
