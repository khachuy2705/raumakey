//! `.res` container layout: null entry, headers, ordinal and string ids, alignment.

use crate::support::{Id, NULL_ENTRY, Rng, parse_res};
use raumakey_resources::{
    LANG_EN_US, LANG_VIETNAMESE, MEMORY_DISCARDABLE, MEMORY_MOVEABLE, MEMORY_PURE, ResWriter,
    ResourceError, ResourceId,
};

fn ordinal(id: u16) -> ResourceId {
    ResourceId::ordinal(id).unwrap()
}

fn named(name: &str) -> ResourceId {
    ResourceId::named(name).unwrap()
}

#[test]
fn empty_writer_emits_only_the_null_entry() {
    let writer = ResWriter::new();
    assert!(writer.is_empty());
    assert_eq!(writer.to_bytes(), NULL_ENTRY);
}

#[test]
fn ordinal_resource_has_every_header_field() {
    let mut writer = ResWriter::new();
    writer
        .add_raw(
            ordinal(10),
            ordinal(7),
            LANG_EN_US,
            MEMORY_MOVEABLE | MEMORY_PURE,
            vec![1, 2, 3, 4, 5],
        )
        .unwrap();
    assert_eq!(writer.len(), 1);
    let bytes = writer.to_bytes();
    let expected_header: [u8; 32] = [
        5, 0, 0, 0, // DataSize
        32, 0, 0, 0, // HeaderSize
        0xFF, 0xFF, 10, 0, // TYPE
        0xFF, 0xFF, 7, 0, // NAME
        0, 0, 0, 0, // DataVersion
        0x30, 0x00, // MemoryFlags
        0x09, 0x04, // LanguageId
        0, 0, 0, 0, // Version
        0, 0, 0, 0, // Characteristics
    ];
    assert_eq!(bytes[32..64], expected_header);
    assert_eq!(bytes[64..69], [1, 2, 3, 4, 5]);
    assert_eq!(bytes[69..72], [0, 0, 0], "data padded to a DWORD");
    assert_eq!(bytes.len(), 72);

    let entries = parse_res(&bytes);
    assert_eq!(entries.len(), 1);
    let entry = &entries[0];
    assert_eq!(entry.offset, 32);
    assert_eq!(entry.data_size, 5);
    assert_eq!(entry.header_size, 32);
    assert_eq!(entry.kind, Id::Ordinal(10));
    assert_eq!(entry.name, Id::Ordinal(7));
    assert_eq!(entry.data_version, 0);
    assert_eq!(entry.memory_flags, 0x0030);
    assert_eq!(entry.language, 0x0409);
    assert_eq!(entry.version, 0);
    assert_eq!(entry.characteristics, 0);
    assert_eq!(entry.data, [1, 2, 3, 4, 5]);
}

#[test]
fn string_type_and_name_are_utf16_with_header_padding() {
    let mut writer = ResWriter::new();
    // 8 + "AB\0" (6) + "XYZ\0" (8) = 22 -> padded to 24, + 16 = 40.
    writer
        .add_raw(named("AB"), named("XYZ"), LANG_EN_US, 0, vec![0xAA])
        .unwrap();
    // 8 + ordinal (4) + "N\0" (4) = 16, no padding, + 16 = 32.
    writer
        .add_raw(ordinal(5), named("N"), LANG_EN_US, 0, vec![0xBB; 4])
        .unwrap();
    // 8 + "R2\0" (6) + ordinal (4) = 18 -> padded to 20, + 16 = 36.
    writer
        .add_raw(named("R2"), ordinal(3), LANG_VIETNAMESE, 0, Vec::new())
        .unwrap();
    // Non-ASCII names are stored as UTF-16.
    writer
        .add_raw(named("RAUMÁ"), ordinal(1), LANG_EN_US, 0, vec![1, 2])
        .unwrap();

    let bytes = writer.to_bytes();
    let entries = parse_res(&bytes);
    assert_eq!(entries.len(), 4);

    assert_eq!(entries[0].kind, Id::Name("AB".into()));
    assert_eq!(entries[0].name, Id::Name("XYZ".into()));
    assert_eq!(entries[0].header_size, 40);
    assert_eq!(entries[0].data, [0xAA]);

    assert_eq!(entries[1].kind, Id::Ordinal(5));
    assert_eq!(entries[1].name, Id::Name("N".into()));
    assert_eq!(entries[1].header_size, 32);

    assert_eq!(entries[2].kind, Id::Name("R2".into()));
    assert_eq!(entries[2].name, Id::Ordinal(3));
    assert_eq!(entries[2].header_size, 36);
    assert_eq!(entries[2].data_size, 0);
    assert_eq!(entries[2].language, LANG_VIETNAMESE);

    assert_eq!(entries[3].kind, Id::Name("RAUMÁ".into()));
    // 8 + 12 + 4 = 24, + 16 = 40.
    assert_eq!(entries[3].header_size, 40);
}

#[test]
fn every_data_length_is_padded_to_a_dword() {
    for len in 0..=13usize {
        let mut writer = ResWriter::new();
        let first: Vec<u8> = (0..len).map(|value| value as u8 + 1).collect();
        writer
            .add_raw(ordinal(10), ordinal(1), LANG_EN_US, 0, first.clone())
            .unwrap();
        writer
            .add_raw(ordinal(10), ordinal(2), LANG_EN_US, 0, vec![0xEE; len + 1])
            .unwrap();
        let bytes = writer.to_bytes();
        let entries = parse_res(&bytes);
        assert_eq!(entries.len(), 2, "length {len}");
        assert_eq!(entries[0].data, first, "length {len}");
        assert_eq!(entries[1].data, vec![0xEE; len + 1], "length {len}");
        assert_eq!(entries[1].offset, 32 + 32 + len.div_ceil(4) * 4);
    }
}

#[test]
fn memory_flags_and_language_are_stored_verbatim() {
    let mut writer = ResWriter::new();
    writer
        .add_raw(
            ordinal(6),
            ordinal(1),
            LANG_VIETNAMESE,
            MEMORY_MOVEABLE | MEMORY_PURE | MEMORY_DISCARDABLE,
            vec![0; 2],
        )
        .unwrap();
    let entries = parse_res(&writer.to_bytes());
    assert_eq!(entries[0].memory_flags, 0x1030);
    assert_eq!(entries[0].language, 0x042A);
}

#[test]
fn rejects_zero_ordinals_and_malformed_names() {
    assert_eq!(ResourceId::ordinal(0), Err(ResourceError::ZeroOrdinal));
    assert_eq!(ResourceId::named(""), Err(ResourceError::InvalidName));
    assert_eq!(ResourceId::named("A\0B"), Err(ResourceError::InvalidName));
    let too_long = "X".repeat(usize::from(u16::MAX) + 1);
    assert_eq!(
        ResourceId::named(&too_long),
        Err(ResourceError::InvalidName)
    );
    let longest = "X".repeat(usize::from(u16::MAX));
    assert!(ResourceId::named(&longest).is_ok());
}

/// A `.res` reader takes a leading `0xFFFF` unit as the ordinal marker, so a string id that
/// starts with U+FFFF would be read back as an ordinal and misalign the rest of the header.
#[test]
fn rejects_names_that_start_with_the_ordinal_marker() {
    for name in ["\u{FFFF}", "\u{FFFF}A", "\u{FFFF}\u{FFFF}", "\u{FFFF}#7"] {
        assert_eq!(
            ResourceId::named(name),
            Err(ResourceError::InvalidName),
            "{name:?}"
        );
    }
}

/// Only the first unit can be the marker: a later U+FFFF is stored and read back as an
/// ordinary unit, and `FindResourceW` finds such a name.
#[test]
fn u_ffff_after_the_first_unit_is_an_ordinary_unit() {
    let mut writer = ResWriter::new();
    writer
        .add_raw(
            named("A\u{FFFF}"),
            named("B\u{FFFF}\u{FFFF}"),
            LANG_EN_US,
            0,
            vec![7],
        )
        .unwrap();
    let entries = parse_res(&writer.to_bytes());
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind, Id::Name("A\u{FFFF}".into()));
    assert_eq!(entries[0].name, Id::Name("B\u{FFFF}\u{FFFF}".into()));
    // 8 + "A\u{FFFF}\0" (6) + "B\u{FFFF}\u{FFFF}\0" (8) = 22 -> padded to 24, + 16 = 40.
    assert_eq!(entries[0].header_size, 40);
    assert_eq!(entries[0].data, [7]);
}

/// `FindResourceW`, and every loader built on it, reads a leading `#` as a decimal ordinal; it
/// upper-cases any other query one UTF-16 unit at a time and then compares exactly. A name that
/// starts with `#`, or holds a unit that Windows upper-cases, can therefore never be found
/// (observed on Windows 11).
#[test]
fn rejects_names_that_windows_cannot_find() {
    for name in [
        "abc", "Abc", "ABc", "lower", "RAUMá", "đ",
        "\u{1F80}", // Windows upper-cases it to U+1F88, Unicode to U+1F08 U+0399
        "#7", "#", "#ABC",
    ] {
        assert_eq!(
            ResourceId::named(name),
            Err(ResourceError::InvalidName),
            "{name:?}"
        );
    }
}

/// The upper-case rule is stricter than Windows. Its case table leaves these letters
/// unchanged, so `FindResourceW` finds each of these names by its exact spelling (observed on
/// Windows 11), but Unicode upper-casing changes them, so they are rejected.
#[test]
fn rejects_some_names_that_windows_could_find() {
    for (name, unicode_upper) in [
        ("\u{00DF}", "SS"),               // sharp s
        ("\u{01C5}", "\u{01C4}"),         // titlecase "Dž"
        ("\u{00B5}", "\u{039C}"),         // micro sign
        ("\u{0131}", "I"),                // dotless i
        ("\u{1F88}", "\u{1F08}\u{0399}"), // titlecase Greek alpha with psili and prosgegrammeni
        ("\u{10D0}", "\u{1C90}"),         // Georgian "an", whose capital Unicode 11 added
        ("\u{10428}", "\u{10400}"),       // Deseret: Windows leaves units outside the BMP alone
    ] {
        assert_eq!(name.to_uppercase(), unicode_upper, "{name:?}");
        assert_eq!(
            ResourceId::named(name),
            Err(ResourceError::InvalidName),
            "{name:?}"
        );
    }
}

#[test]
fn accepts_upper_case_names_verbatim() {
    for name in [
        "ABC",
        "A#7",
        "123",
        "RAUMÁ",
        "Đ",
        "\u{00AA}",  // no upper-case form
        "\u{10400}", // Deseret capital letter
        "_X-1.Y Z",
    ] {
        let id = ResourceId::named(name).unwrap();
        assert_eq!(id.as_name(), Some(name));
        assert_eq!(id.as_ordinal(), None);
    }
}

#[test]
fn case_variants_of_a_name_cannot_coexist() {
    let mut writer = ResWriter::new();
    writer
        .add_raw(named("ABC"), ordinal(1), LANG_EN_US, 0, vec![1])
        .unwrap();
    for variant in ["abc", "Abc", "aBC"] {
        assert_eq!(
            ResourceId::named(variant),
            Err(ResourceError::InvalidName),
            "{variant:?}"
        );
    }
    assert_eq!(
        writer.add_raw(named("ABC"), ordinal(1), LANG_EN_US, 0, vec![2]),
        Err(ResourceError::Duplicate)
    );
    assert_eq!(writer.len(), 1);
}

#[test]
fn resource_id_reports_its_form() {
    assert_eq!(ordinal(24).as_ordinal(), Some(24));
    assert_eq!(ordinal(24).as_name(), None);
    assert_eq!(named("MUI").as_name(), Some("MUI"));
    assert_eq!(named("MUI").as_ordinal(), None);
}

#[test]
fn rejects_duplicate_type_name_and_language() {
    let mut writer = ResWriter::new();
    writer
        .add_raw(ordinal(10), ordinal(1), LANG_EN_US, 0, vec![1])
        .unwrap();
    assert_eq!(
        writer.add_raw(ordinal(10), ordinal(1), LANG_EN_US, 0, vec![2]),
        Err(ResourceError::Duplicate)
    );
    writer
        .add_raw(named("T"), named("N"), LANG_EN_US, 0, vec![1])
        .unwrap();
    assert_eq!(
        writer.add_raw(named("T"), named("N"), LANG_EN_US, 0, vec![1]),
        Err(ResourceError::Duplicate)
    );
    // The same type and name in another language is a distinct resource.
    writer
        .add_raw(ordinal(10), ordinal(1), LANG_VIETNAMESE, 0, vec![3])
        .unwrap();
    assert_eq!(writer.len(), 3);
    assert_eq!(parse_res(&writer.to_bytes()).len(), 3);
}

#[test]
fn writer_language_defaults_to_english_us() {
    assert_eq!(ResWriter::new().language(), LANG_EN_US);
    assert_eq!(ResWriter::default().language(), LANG_EN_US);
    assert_eq!(
        ResWriter::with_language(LANG_VIETNAMESE).language(),
        LANG_VIETNAMESE
    );
}

#[test]
fn random_entries_round_trip_field_by_field() {
    let mut rng = Rng::new(0x5241_554D_414B_4559);
    for round in 0..40 {
        let mut writer = ResWriter::new();
        let mut expected = Vec::new();
        for index in 0..rng.below(12) {
            // A unique ordinal name per entry keeps (type, name, language) distinct.
            let unique = u16::try_from(index + 1).unwrap();
            // String ids must be upper case (Windows upper-cases names before lookup).
            let (kind, kind_id) = if rng.below(2) == 0 {
                let id = u16::try_from(rng.below(u64::from(u16::MAX)) + 1).unwrap();
                (ordinal(id), Id::Ordinal(id))
            } else {
                let text = format!("T{}", rng.text(9)).to_uppercase();
                (named(&text), Id::Name(text))
            };
            let (name, name_id) = if rng.below(2) == 0 {
                (ordinal(unique), Id::Ordinal(unique))
            } else {
                let text = format!("N{unique}{}", rng.text(7)).to_uppercase();
                (named(&text), Id::Name(text))
            };
            let language = u16::try_from(rng.below(0x10000)).unwrap();
            let flags = u16::try_from(rng.below(0x10000)).unwrap();
            let data: Vec<u8> = (0..rng.below(41)).map(|_| rng.below(256) as u8).collect();
            writer
                .add_raw(kind, name, language, flags, data.clone())
                .unwrap();
            expected.push((kind_id, name_id, language, flags, data));
        }
        let entries = parse_res(&writer.to_bytes());
        assert_eq!(entries.len(), expected.len(), "round {round}");
        for (entry, (kind, name, language, flags, data)) in entries.iter().zip(expected) {
            assert_eq!(entry.kind, kind, "round {round}");
            assert_eq!(entry.name, name, "round {round}");
            assert_eq!(entry.language, language, "round {round}");
            assert_eq!(entry.memory_flags, flags, "round {round}");
            assert_eq!(entry.data_size as usize, data.len(), "round {round}");
            assert_eq!(entry.data, data, "round {round}");
            assert_eq!(entry.data_version, 0);
            assert_eq!(entry.version, 0);
            assert_eq!(entry.characteristics, 0);
        }
    }
}
