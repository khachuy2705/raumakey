//! PE image parsing and byte-level helpers used by `cargo xtask inspect`.
//!
//! Parsing is built on the `object` crate. Every function returns an error for malformed input;
//! none of them panics.

use std::fmt;

use object::read::pe::{
    ExportTarget, ImageNtHeaders, ImageOptionalHeader, PeFile, ResourceDirectory,
    ResourceDirectoryEntryData, ResourceDirectoryTable, ResourceNameOrId,
};
use object::{FileKind, LittleEndian as LE, pe};
use raumakey_model::identity::Guid;

#[cfg(test)]
pub(crate) mod synthetic;

/// Resource type `RT_ICON`.
pub const RT_ICON: u16 = 3;
/// Resource type `RT_GROUP_ICON`.
pub const RT_GROUP_ICON: u16 = 14;
/// Resource type `RT_VERSION`.
pub const RT_VERSION: u16 = 16;
/// Resource type `RT_MANIFEST`.
pub const RT_MANIFEST: u16 = 24;
/// Optional header subsystem `IMAGE_SUBSYSTEM_WINDOWS_GUI`.
pub const SUBSYSTEM_WINDOWS_GUI: u16 = 2;

/// Upper bound on resource directory entries visited, so crafted trees cannot exhaust memory or
/// time.
const MAX_RESOURCE_ENTRIES: usize = 16_384;

/// Error returned for malformed or unsupported PE input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeError(pub String);

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Identifier of a resource type or name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResourceId {
    /// Numeric identifier.
    Id(u16),
    /// String identifier.
    Name(String),
}

/// One leaf (type/name/language) of the resource tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource<'a> {
    /// Resource type, e.g. `Id(RT_MANIFEST)`.
    pub kind: ResourceId,
    /// Resource name.
    pub name: ResourceId,
    /// Raw resource bytes.
    pub data: &'a [u8],
}

/// Facts about a PE image that the artifact checks use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeInfo<'a> {
    /// COFF machine.
    pub machine: u16,
    /// Whether the optional header is PE32+.
    pub pe32_plus: bool,
    /// Whether `IMAGE_FILE_DLL` is set.
    pub is_dll: bool,
    /// Optional header subsystem.
    pub subsystem: u16,
    /// Names of named exports.
    pub exports: Vec<String>,
    /// Number of exports that have only an ordinal.
    pub unnamed_exports: usize,
    /// Imported and delay-loaded DLL names.
    pub imports: Vec<String>,
    /// Resource leaves in directory order.
    pub resources: Vec<Resource<'a>>,
}

impl<'a> PeInfo<'a> {
    /// Resources of the numeric type `kind`.
    pub fn resources_of(&self, kind: u16) -> impl Iterator<Item = &Resource<'a>> {
        self.resources
            .iter()
            .filter(move |resource| resource.kind == ResourceId::Id(kind))
    }
}

/// Parses a PE32 or PE32+ image.
pub fn parse(bytes: &[u8]) -> Result<PeInfo<'_>, PeError> {
    match FileKind::parse(bytes) {
        Ok(FileKind::Pe32) => parse_image::<pe::ImageNtHeaders32>(bytes),
        Ok(FileKind::Pe64) => parse_image::<pe::ImageNtHeaders64>(bytes),
        Ok(kind) => Err(PeError(format!("not a PE image ({kind:?})"))),
        Err(error) => Err(PeError(format!("not a PE image: {error}"))),
    }
}

fn parse_image<Pe: ImageNtHeaders>(bytes: &[u8]) -> Result<PeInfo<'_>, PeError> {
    let file = PeFile::<Pe>::parse(bytes).map_err(context("PE headers"))?;
    let nt_headers = file.nt_headers();
    let file_header = nt_headers.file_header();
    let (exports, unnamed_exports) = exports(&file)?;
    let imports = imports(&file)?;
    let sections = file.section_table();
    let resources = match file
        .data_directories()
        .resource_directory(bytes, &sections)
        .map_err(context("resource directory"))?
    {
        Some(directory) => walk_resources(directory, |rva, size| {
            sections.pe_data_at(bytes, rva)?.get(..size as usize)
        })?,
        None => Vec::new(),
    };
    Ok(PeInfo {
        machine: file_header.machine.get(LE).0,
        pe32_plus: nt_headers.is_type_64(),
        is_dll: file_header.characteristics.get(LE).0 & pe::IMAGE_FILE_DLL.0 != 0,
        subsystem: nt_headers.optional_header().subsystem().0,
        exports,
        unnamed_exports,
        imports,
        resources,
    })
}

fn exports<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>) -> Result<(Vec<String>, usize), PeError> {
    let mut named = Vec::new();
    let mut unnamed = 0;
    let Some(table) = file.export_table().map_err(context("export table"))? else {
        return Ok((named, unnamed));
    };
    for export in table.exports().map_err(context("export table"))? {
        if matches!(export.target, ExportTarget::Address(0)) {
            continue; // unused ordinal slot
        }
        match export.name {
            Some(name) => named.push(String::from_utf8_lossy(name).into_owned()),
            None => unnamed += 1,
        }
    }
    Ok((named, unnamed))
}

fn imports<Pe: ImageNtHeaders>(file: &PeFile<'_, Pe>) -> Result<Vec<String>, PeError> {
    let mut names = Vec::new();
    if let Some(table) = file.import_table().map_err(context("import table"))? {
        let mut descriptors = table.descriptors().map_err(context("import table"))?;
        while let Some(descriptor) = descriptors.next().map_err(context("import table"))? {
            let name = table
                .name(descriptor.name.get(LE))
                .map_err(context("import table"))?;
            names.push(String::from_utf8_lossy(name).into_owned());
        }
    }
    if let Some(table) = file
        .delay_load_import_table()
        .map_err(context("delay-load import table"))?
    {
        let mut descriptors = table
            .descriptors()
            .map_err(context("delay-load import table"))?;
        while let Some(descriptor) = descriptors
            .next()
            .map_err(context("delay-load import table"))?
        {
            let name = table
                .name(descriptor.dll_name_rva.get(LE))
                .map_err(context("delay-load import table"))?;
            names.push(String::from_utf8_lossy(name).into_owned());
        }
    }
    Ok(names)
}

/// Walks the type/name/language resource tree. `data_at(rva, size)` maps a data entry to bytes.
fn walk_resources<'a>(
    directory: ResourceDirectory<'a>,
    data_at: impl Fn(u32, u32) -> Option<&'a [u8]>,
) -> Result<Vec<Resource<'a>>, PeError> {
    let mut budget = MAX_RESOURCE_ENTRIES;
    let mut spend = |count: usize| {
        budget = budget.checked_sub(count).ok_or_else(|| {
            PeError(format!(
                "too many resource directory entries (limit {MAX_RESOURCE_ENTRIES})"
            ))
        })?;
        Ok::<(), PeError>(())
    };
    let mut resources = Vec::new();
    let root = directory.root().map_err(context("resource directory"))?;
    spend(root.entries.len())?;
    for type_entry in root.entries {
        let kind = resource_id(directory, type_entry)?;
        let names = subdirectory(directory, type_entry)?;
        spend(names.entries.len())?;
        for name_entry in names.entries {
            let name = resource_id(directory, name_entry)?;
            let languages = subdirectory(directory, name_entry)?;
            spend(languages.entries.len())?;
            for language_entry in languages.entries {
                let data_entry = match language_entry
                    .data(directory)
                    .map_err(context("resource directory"))?
                {
                    ResourceDirectoryEntryData::Data(entry) => entry,
                    ResourceDirectoryEntryData::Table(_) => {
                        return Err(PeError(
                            "resource tree is nested deeper than type/name/language".to_owned(),
                        ));
                    }
                };
                let rva = data_entry.offset_to_data.get(LE);
                let size = data_entry.size.get(LE);
                let data = data_at(rva, size).ok_or_else(|| {
                    PeError(format!(
                        "resource data at RVA {rva:#x} (+{size:#x}) lies outside the image"
                    ))
                })?;
                resources.push(Resource {
                    kind: kind.clone(),
                    name: name.clone(),
                    data,
                });
            }
        }
    }
    Ok(resources)
}

fn subdirectory<'a>(
    directory: ResourceDirectory<'a>,
    entry: &pe::ImageResourceDirectoryEntry,
) -> Result<ResourceDirectoryTable<'a>, PeError> {
    match entry
        .data(directory)
        .map_err(context("resource directory"))?
    {
        ResourceDirectoryEntryData::Table(table) => Ok(table),
        ResourceDirectoryEntryData::Data(_) => Err(PeError(
            "resource directory entry holds data where a subdirectory is expected".to_owned(),
        )),
    }
}

fn resource_id(
    directory: ResourceDirectory<'_>,
    entry: &pe::ImageResourceDirectoryEntry,
) -> Result<ResourceId, PeError> {
    match entry.name_or_id() {
        ResourceNameOrId::Id(id) => Ok(ResourceId::Id(id)),
        ResourceNameOrId::Name(name) => name
            .to_string_lossy(directory)
            .map(ResourceId::Name)
            .map_err(context("resource name")),
    }
}

fn context(what: &'static str) -> impl Fn(object::read::Error) -> PeError {
    move |error| PeError(format!("invalid {what}: {error}"))
}

/// GUIDs of the legacy EVKey TIP (clean-room spec §6.1). They are used only for the negative
/// check that no Raumakey artifact carries them; Raumakey never registers them.
pub const LEGACY_EVKEY_GUIDS: [(&str, Guid); 2] = [
    (
        "legacy EVKey CLSID",
        Guid::from_u128(0x0BAD_015B_A9AA_41E4_B817_803D_D9E8_1A2A),
    ),
    (
        "legacy EVKey profile GUID",
        Guid::from_u128(0xF74F_81AF_6F78_42C7_B006_2630_C950_B9A2),
    ),
];

/// Encodes a GUID as the 16-byte in-memory `GUID` struct: `Data1`, `Data2`, `Data3` little-endian
/// followed by the eight `Data4` bytes.
pub fn guid_le_bytes(guid: Guid) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[..4].copy_from_slice(&guid.data1().to_le_bytes());
    bytes[4..6].copy_from_slice(&guid.data2().to_le_bytes());
    bytes[6..8].copy_from_slice(&guid.data3().to_le_bytes());
    bytes[8..].copy_from_slice(&guid.data4());
    bytes
}

/// Canonical upper-case text of a GUID without braces, e.g. `0FC48F2B-4F91-44A0-B500-DE512E801432`.
pub fn guid_text(guid: Guid) -> String {
    let braced = guid.to_string();
    braced
        .trim_start_matches('{')
        .trim_end_matches('}')
        .to_owned()
}

/// Whether `needle` occurs in `haystack`. An empty needle never matches.
pub fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// Whether the binary `GUID` struct of `guid` occurs in `haystack`.
pub fn contains_guid_binary(haystack: &[u8], guid: Guid) -> bool {
    contains_bytes(haystack, &guid_le_bytes(guid))
}

/// Whether the text form of `guid` occurs in `haystack` as ASCII/UTF-8 or UTF-16LE, in any letter
/// case, with or without braces.
pub fn contains_guid_text(haystack: &[u8], guid: Guid) -> bool {
    let text = guid_text(guid).to_ascii_lowercase();
    let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
    // ASCII lower-casing keeps UTF-16LE code units of ASCII characters aligned.
    let lowered = haystack.to_ascii_lowercase();
    contains_bytes(&lowered, text.as_bytes()) || contains_bytes(&lowered, &utf16)
}

/// Decodes an `RT_MANIFEST` payload (UTF-8 with or without BOM, or UTF-16LE with BOM).
pub fn decode_manifest(data: &[u8]) -> String {
    if let Some(body) = data.strip_prefix(&[0xFF, 0xFE]) {
        let (pairs, _odd_byte) = body.as_chunks::<2>();
        let units: Vec<u16> = pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        return String::from_utf16_lossy(&units);
    }
    let body = data.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(data);
    String::from_utf8_lossy(body).into_owned()
}

/// Attributes of the manifest's `requestedExecutionLevel` element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionLevel {
    /// Value of the `level` attribute.
    pub level: Option<String>,
    /// Value of the `uiAccess` attribute.
    pub ui_access: Option<String>,
}

/// Finds the single `requestedExecutionLevel` element (any namespace prefix) and returns its
/// `level` and `uiAccess` attributes.
pub fn requested_execution_level(xml: &str) -> Result<ExecutionLevel, String> {
    const ELEMENT: &str = "requestedExecutionLevel";
    let mut found = None;
    let mut offset = 0;
    while let Some(position) = xml[offset..].find('<') {
        let tag = &xml[offset + position + 1..];
        offset += position + 1;
        if !tag.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_') {
            continue; // closing tag, comment, declaration, or processing instruction
        }
        let name_len = tag
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(tag.len());
        let name = &tag[..name_len];
        let local_name = name.rsplit(':').next().unwrap_or(name);
        if local_name != ELEMENT {
            continue;
        }
        if found.is_some() {
            return Err(format!("manifest has more than one {ELEMENT} element"));
        }
        let attributes = parse_attributes(&tag[name_len..])?;
        let value = |wanted: &str| {
            attributes
                .iter()
                .find(|(name, _)| name == wanted)
                .map(|(_, value)| value.clone())
        };
        found = Some(ExecutionLevel {
            level: value("level"),
            ui_access: value("uiAccess"),
        });
    }
    found.ok_or_else(|| format!("manifest has no {ELEMENT} element"))
}

/// Parses `name="value"` pairs up to the end of a start tag.
fn parse_attributes(mut text: &str) -> Result<Vec<(String, String)>, String> {
    let mut attributes = Vec::new();
    loop {
        text = text.trim_start();
        if text.starts_with('>') || text.starts_with("/>") {
            return Ok(attributes);
        }
        if text.is_empty() {
            return Err("unterminated manifest element".to_owned());
        }
        let name_len = text
            .find(|c: char| c == '=' || c.is_whitespace() || c == '>' || c == '/')
            .unwrap_or(text.len());
        if name_len == 0 {
            return Err("malformed manifest attribute".to_owned());
        }
        let name = &text[..name_len];
        text = text[name_len..].trim_start();
        text = text
            .strip_prefix('=')
            .ok_or_else(|| format!("manifest attribute `{name}` has no value"))?
            .trim_start();
        let quote = match text.chars().next() {
            Some(quote @ ('"' | '\'')) => quote,
            _ => return Err(format!("manifest attribute `{name}` is not quoted")),
        };
        text = &text[1..];
        let value_len = text
            .find(quote)
            .ok_or_else(|| format!("manifest attribute `{name}` is not terminated"))?;
        attributes.push((name.to_owned(), text[..value_len].to_owned()));
        text = &text[value_len + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::synthetic::PeBuilder;
    use super::*;
    use object::read::pe::ResourceDirectory;
    use raumakey_model::identity::{TIP_CLSID, TIP_PROFILE_GUID};

    fn full_dll() -> PeBuilder {
        let mut builder = PeBuilder::new(0x8664, true, true);
        builder.exports = vec!["DllCanUnloadNow".to_owned(), "DllGetClassObject".to_owned()];
        builder.ordinal_only_exports = 1;
        builder.imports = vec!["KERNEL32.dll".to_owned(), "VCRUNTIME140.dll".to_owned()];
        builder.delay_imports = vec!["USER32.dll".to_owned()];
        builder.resources = vec![
            (RT_VERSION, 1, b"version".to_vec()),
            (RT_GROUP_ICON, 1, b"group".to_vec()),
            (RT_ICON, 1, b"icon-1".to_vec()),
            (RT_ICON, 2, b"icon-2".to_vec()),
            (RT_MANIFEST, 2, b"<assembly/>".to_vec()),
        ];
        builder.payload = guid_le_bytes(TIP_CLSID).to_vec();
        builder
    }

    #[test]
    fn parses_a_64_bit_dll_with_every_directory() {
        let image = full_dll().build();
        let info = parse(&image).expect("synthetic DLL parses");
        assert_eq!(info.machine, 0x8664);
        assert!(info.pe32_plus);
        assert!(info.is_dll);
        assert_eq!(info.subsystem, SUBSYSTEM_WINDOWS_GUI);
        assert_eq!(info.exports, ["DllCanUnloadNow", "DllGetClassObject"]);
        assert_eq!(info.unnamed_exports, 1);
        assert_eq!(
            info.imports,
            ["KERNEL32.dll", "VCRUNTIME140.dll", "USER32.dll"]
        );
        let kinds: Vec<&ResourceId> = info.resources.iter().map(|r| &r.kind).collect();
        assert_eq!(
            kinds,
            [
                &ResourceId::Id(RT_ICON),
                &ResourceId::Id(RT_ICON),
                &ResourceId::Id(RT_GROUP_ICON),
                &ResourceId::Id(RT_VERSION),
                &ResourceId::Id(RT_MANIFEST),
            ]
        );
        let manifest: Vec<&Resource<'_>> = info.resources_of(RT_MANIFEST).collect();
        assert_eq!(manifest.len(), 1);
        assert_eq!(manifest[0].name, ResourceId::Id(2));
        assert_eq!(manifest[0].data, b"<assembly/>");
        assert_eq!(info.resources_of(RT_ICON).count(), 2);
        assert!(contains_guid_binary(&image, TIP_CLSID));
    }

    #[test]
    fn parses_a_32_bit_console_exe_without_directories() {
        let mut builder = PeBuilder::new(0x014C, false, false);
        builder.subsystem = 3;
        let image = builder.build();
        let info = parse(&image).expect("synthetic EXE parses");
        assert_eq!(info.machine, 0x014C);
        assert!(!info.pe32_plus);
        assert!(!info.is_dll);
        assert_eq!(info.subsystem, 3);
        assert!(info.exports.is_empty());
        assert_eq!(info.unnamed_exports, 0);
        assert!(info.imports.is_empty());
        assert!(info.resources.is_empty());
    }

    #[test]
    fn rejects_input_that_is_not_a_pe_image() {
        let elf = b"\x7fELF\x02\x01\x01\0\0\0\0\0\0\0\0\0";
        let zeros = [0u8; 512];
        for input in [
            &b""[..],
            b"MZ",
            b"not a binary at all",
            &elf[..],
            &zeros[..],
        ] {
            assert!(parse(input).is_err(), "{input:?}");
        }
    }

    #[test]
    fn truncated_images_are_errors() {
        let mut image = full_dll().build();
        image.truncate(0x200 + 0x30);
        assert!(parse(&image).is_err());
    }

    #[test]
    fn corrupted_images_never_panic() {
        let image = full_dll().build();
        for len in 0..image.len() {
            let _ = parse(&image[..len]);
        }
        for index in 0..image.len() {
            for mask in [0xFF, 0x80, 0x01] {
                let mut corrupted = image.clone();
                corrupted[index] ^= mask;
                let _ = parse(&corrupted);
            }
        }
    }

    /// Builds a resource directory whose three levels each consist of one table with `fanout`
    /// entries that all point at the next level's table.
    fn self_similar_resource_tree(fanout: u16) -> Vec<u8> {
        let table_len = 16 + 8 * usize::from(fanout);
        let mut blob = vec![0u8; 3 * table_len + 16];
        for level in 0..3 {
            let table = level * table_len;
            blob[table + 14..table + 16].copy_from_slice(&fanout.to_le_bytes());
            for index in 0..usize::from(fanout) {
                let entry = table + 16 + 8 * index;
                blob[entry..entry + 4].copy_from_slice(&(index as u32 + 1).to_le_bytes());
                let target = if level < 2 {
                    0x8000_0000 | ((level + 1) * table_len) as u32
                } else {
                    (3 * table_len) as u32
                };
                blob[entry + 4..entry + 8].copy_from_slice(&target.to_le_bytes());
            }
        }
        blob
    }

    #[test]
    fn resource_walk_reports_leaves_and_bounds_work() {
        let payload = b"data";
        let small = self_similar_resource_tree(2);
        let leaves = walk_resources(ResourceDirectory::new(&small), |_, _| Some(&payload[..]))
            .expect("bounded tree walks");
        assert_eq!(leaves.len(), 8);
        assert_eq!(leaves[0].kind, ResourceId::Id(1));
        assert_eq!(leaves[7].name, ResourceId::Id(2));
        assert_eq!(leaves[7].data, b"data");

        let huge = self_similar_resource_tree(26);
        let error = walk_resources(ResourceDirectory::new(&huge), |_, _| Some(&payload[..]))
            .expect_err("26^3 leaves exceed the budget");
        assert!(error.0.contains("too many"), "{error}");
    }

    #[test]
    fn resource_cycles_and_bad_data_are_errors() {
        // A root entry that points back at the root nests deeper than three levels.
        let mut cyclic = vec![0u8; 24];
        cyclic[14] = 1;
        cyclic[16..20].copy_from_slice(&1u32.to_le_bytes());
        cyclic[20..24].copy_from_slice(&0x8000_0000u32.to_le_bytes());
        assert!(walk_resources(ResourceDirectory::new(&cyclic), |_, _| None).is_err());

        let tree = self_similar_resource_tree(1);
        assert!(
            walk_resources(ResourceDirectory::new(&tree), |_, _| None).is_err(),
            "data outside the image is an error"
        );
    }

    fn utf16le(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_le_bytes).collect()
    }

    #[test]
    fn guid_struct_bytes_are_little_endian_fields() {
        assert_eq!(
            guid_le_bytes(TIP_CLSID),
            [
                0x2B, 0x8F, 0xC4, 0x0F, 0x91, 0x4F, 0xA0, 0x44, 0xB5, 0x00, 0xDE, 0x51, 0x2E, 0x80,
                0x14, 0x32
            ]
        );
        assert_eq!(
            guid_le_bytes(TIP_PROFILE_GUID),
            [
                0x58, 0x50, 0x87, 0xDB, 0xF9, 0xBB, 0x7C, 0x40, 0x85, 0xA8, 0x95, 0x0B, 0xE7, 0xD1,
                0x4C, 0x22
            ]
        );
    }

    #[test]
    fn guid_text_is_canonical_without_braces() {
        assert_eq!(guid_text(TIP_CLSID), "0FC48F2B-4F91-44A0-B500-DE512E801432");
        assert_eq!(
            guid_text(LEGACY_EVKEY_GUIDS[0].1),
            "0BAD015B-A9AA-41E4-B817-803DD9E81A2A"
        );
        assert_eq!(
            guid_text(LEGACY_EVKEY_GUIDS[1].1),
            "F74F81AF-6F78-42C7-B006-2630C950B9A2"
        );
    }

    #[test]
    fn byte_search() {
        assert!(contains_bytes(b"abcdef", b"cde"));
        assert!(contains_bytes(b"abcdef", b"abcdef"));
        assert!(!contains_bytes(b"abcdef", b"abcdefg"));
        assert!(!contains_bytes(b"abcdef", b"x"));
        assert!(!contains_bytes(b"abc", b""));
    }

    #[test]
    fn binary_guid_search() {
        let mut image = vec![0xCC; 64];
        image.extend_from_slice(&guid_le_bytes(TIP_CLSID));
        image.extend_from_slice(&[0xCC; 7]);
        assert!(contains_guid_binary(&image, TIP_CLSID));
        assert!(!contains_guid_binary(&image, TIP_PROFILE_GUID));
        // The big-endian (canonical u128) byte order is not the in-memory struct.
        let big_endian = TIP_PROFILE_GUID.to_u128().to_be_bytes();
        assert!(!contains_guid_binary(&big_endian, TIP_PROFILE_GUID));
    }

    #[test]
    fn text_guid_search_covers_case_braces_and_utf16() {
        let legacy = LEGACY_EVKEY_GUIDS[0].1;
        for text in [
            "{0BAD015B-A9AA-41E4-B817-803DD9E81A2A}",
            "0bad015b-a9aa-41e4-b817-803dd9e81a2a",
            "prefix 0Bad015b-A9aa-41E4-b817-803DD9E81a2a suffix",
        ] {
            assert!(contains_guid_text(text.as_bytes(), legacy), "{text}");
            assert!(contains_guid_text(&utf16le(text), legacy), "utf-16 {text}");
        }
        assert!(!contains_guid_text(
            b"0BAD015B-A9AA-41E4-B817-803DD9E81A2B",
            legacy
        ));
        assert!(!contains_guid_text(&guid_le_bytes(legacy), legacy));
    }

    #[test]
    fn manifest_decoding_handles_boms() {
        assert_eq!(decode_manifest(b"<a/>"), "<a/>");
        assert_eq!(decode_manifest(b"\xEF\xBB\xBF<a/>"), "<a/>");
        let mut utf16 = vec![0xFF, 0xFE];
        utf16.extend(utf16le("<a/>"));
        assert_eq!(decode_manifest(&utf16), "<a/>");
        assert_eq!(decode_manifest(b"<a>\xFF</a>"), "<a>\u{FFFD}</a>");
    }

    const MANIFEST: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="asInvoker" uiAccess="false"/>
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>"#;

    #[test]
    fn execution_level_is_read_from_the_element() {
        assert_eq!(
            requested_execution_level(MANIFEST),
            Ok(ExecutionLevel {
                level: Some("asInvoker".to_owned()),
                ui_access: Some("false".to_owned()),
            })
        );
    }

    #[test]
    fn execution_level_tolerates_quotes_whitespace_and_prefixes() {
        let xml = "<asmv3:requestedExecutionLevel\n uiAccess = 'true'\tlevel='requireAdministrator' >\
                   </asmv3:requestedExecutionLevel>";
        assert_eq!(
            requested_execution_level(xml),
            Ok(ExecutionLevel {
                level: Some("requireAdministrator".to_owned()),
                ui_access: Some("true".to_owned()),
            })
        );
        let no_ui_access = r#"<requestedExecutionLevel level="asInvoker"/>"#;
        assert_eq!(
            requested_execution_level(no_ui_access),
            Ok(ExecutionLevel {
                level: Some("asInvoker".to_owned()),
                ui_access: None,
            })
        );
    }

    #[test]
    fn execution_level_errors() {
        assert!(requested_execution_level("<assembly/>").is_err());
        let twice = format!("{MANIFEST}{MANIFEST}");
        assert!(requested_execution_level(&twice).is_err());
        assert!(requested_execution_level(r#"<requestedExecutionLevel level="asInvoker"#).is_err());
        assert!(
            requested_execution_level(r#"<requestedExecutionLevel level=asInvoker/>"#).is_err()
        );
        // An element whose name merely starts with the wanted name does not count.
        assert!(
            requested_execution_level(r#"<requestedExecutionLevelX level="asInvoker"/>"#).is_err()
        );
    }
}
