//! 32-bit Windows `.res` container writer.
//!
//! A `.res` file is a sequence of DWORD-aligned entries. It starts with an empty entry (type and
//! name ordinal 0) that identifies the 32-bit format. Every entry has this layout:
//!
//! ```text
//! DWORD DataSize          size of the data, without trailing padding
//! DWORD HeaderSize        size of this header, including TYPE, NAME and padding
//! TYPE                    0xFFFF + 16-bit ordinal, or NUL-terminated UTF-16 string
//! NAME                    0xFFFF + 16-bit ordinal, or NUL-terminated UTF-16 string
//! padding                 zero bytes up to a DWORD boundary
//! DWORD DataVersion
//! WORD  MemoryFlags
//! WORD  LanguageId
//! DWORD Version
//! DWORD Characteristics
//! data                    DataSize bytes, then zero bytes up to a DWORD boundary
//! ```

use core::num::NonZeroU16;
use std::collections::BTreeSet;

use crate::error::ResourceError;
use crate::icon::{IconImage, dimension_byte};
use crate::manifest::{MANIFEST_ID, Manifest};
use crate::strings::StringTable;
use crate::version::VersionInfo;

/// Resource type of one icon image (`RT_ICON`).
pub const RT_ICON: u16 = 3;
/// Resource type of a string-table block (`RT_STRING`).
pub const RT_STRING: u16 = 6;
/// Resource type of an icon group directory (`RT_GROUP_ICON`).
pub const RT_GROUP_ICON: u16 = 14;
/// Resource type of version information (`RT_VERSION`).
pub const RT_VERSION: u16 = 16;
/// Resource type of a side-by-side manifest (`RT_MANIFEST`).
pub const RT_MANIFEST: u16 = 24;

/// Memory flag `MOVEABLE` (ignored by 32-bit Windows, kept for format compatibility).
pub const MEMORY_MOVEABLE: u16 = 0x0010;
/// Memory flag `PURE` (ignored by 32-bit Windows, kept for format compatibility).
pub const MEMORY_PURE: u16 = 0x0020;
/// Memory flag `PRELOAD` (ignored by 32-bit Windows, kept for format compatibility).
pub const MEMORY_PRELOAD: u16 = 0x0040;
/// Memory flag `DISCARDABLE` (ignored by 32-bit Windows, kept for format compatibility).
pub const MEMORY_DISCARDABLE: u16 = 0x1000;

/// Language-neutral resource language identifier.
pub const LANG_NEUTRAL: u16 = 0x0000;
/// English (United States) language identifier.
pub const LANG_EN_US: u16 = 0x0409;
/// Vietnamese language identifier.
pub const LANG_VIETNAMESE: u16 = 0x042A;

/// Name of the version resource (`VS_VERSION_INFO` is always resource 1).
pub const VERSION_INFO_ID: u16 = 1;

/// The empty entry that starts every 32-bit `.res` file.
const NULL_ENTRY: [u8; 32] = [
    0, 0, 0, 0, 0x20, 0, 0, 0, 0xFF, 0xFF, 0, 0, 0xFF, 0xFF, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0, 0, 0, 0, 0,
];

/// Size of the fixed header part after TYPE, NAME and padding.
const HEADER_SUFFIX_SIZE: usize = 16;

/// Longest string type or name, in UTF-16 code units (a PE directory string has a 16-bit length).
const MAX_NAME_UNITS: usize = u16::MAX as usize;

/// First unit of an ordinal type or name in a `.res` header; no string id may start with it.
const ORDINAL_MARKER: u16 = 0xFFFF;

/// A resource type or name: a non-zero 16-bit ordinal or a non-empty, upper-case UTF-16 string.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ResourceId(IdRepr);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum IdRepr {
    /// Never zero: every constructor rejects or cannot produce zero.
    Ordinal(u16),
    Name(String),
}

impl ResourceId {
    /// Creates a numeric id. Ordinal 0 is reserved for the leading null entry.
    pub fn ordinal(id: u16) -> Result<Self, ResourceError> {
        NonZeroU16::new(id)
            .map(Self::from)
            .ok_or(ResourceError::ZeroOrdinal)
    }

    /// Creates a numeric id from a crate constant that is known to be non-zero.
    pub(crate) const fn known(id: u16) -> Self {
        Self(IdRepr::Ordinal(id))
    }

    /// Creates a string id, stored exactly as given.
    ///
    /// Fails with [`ResourceError::InvalidName`] unless the name is 1 to 65,535 UTF-16 code
    /// units long, contains no NUL, and
    ///
    /// - does not start with U+FFFF, which a `.res` reader takes as the ordinal marker;
    /// - does not start with `#`, which `FindResourceW`, and every loader built on it, reads as
    ///   a decimal ordinal;
    /// - is its own Unicode upper-case form ([`str::to_uppercase`]).
    ///
    /// `FindResourceW` upper-cases the query one UTF-16 unit at a time with the Windows case
    /// table and then compares exactly, so a name holding a unit that the table changes
    /// (`"abc"`, `"đ"`) can never be found. The last rule is a stricter stand-in for that table,
    /// which this crate does not carry: on Windows 11 every unit the table changes is also
    /// changed by Unicode upper-casing, but the table leaves some letters alone, among them
    /// `"ß"`, titlecase `"ǅ"`, `"µ"`, and every letter outside the Basic Multilingual Plane
    /// (`"𐐨"`). Windows would find such names by their exact spelling; they are rejected anyway.
    /// In return an accepted name is found by itself and by any query Windows upper-cases to it
    /// (for example `"raumá"` finds `"RAUMÁ"`), and two accepted names never differ only in case.
    pub fn named(name: &str) -> Result<Self, ResourceError> {
        let valid = !name.is_empty()
            && !name.contains('\0')
            && name.encode_utf16().next() != Some(ORDINAL_MARKER)
            && !name.starts_with('#')
            && name.encode_utf16().count() <= MAX_NAME_UNITS
            && name.to_uppercase() == name;
        if !valid {
            return Err(ResourceError::InvalidName);
        }
        Ok(Self(IdRepr::Name(name.to_owned())))
    }

    /// Returns the ordinal, if this id is numeric.
    pub fn as_ordinal(&self) -> Option<u16> {
        match &self.0 {
            IdRepr::Ordinal(id) => Some(*id),
            IdRepr::Name(_) => None,
        }
    }

    /// Returns the name, if this id is a string.
    pub fn as_name(&self) -> Option<&str> {
        match &self.0 {
            IdRepr::Ordinal(_) => None,
            IdRepr::Name(name) => Some(name),
        }
    }

    /// Number of bytes the id occupies in a `.res` header.
    fn encoded_len(&self) -> usize {
        match &self.0 {
            IdRepr::Ordinal(_) => 4,
            // At most 65,536 units including the terminator, so this cannot overflow.
            IdRepr::Name(name) => (name.encode_utf16().count() + 1) * 2,
        }
    }

    fn encode(&self, out: &mut Vec<u8>) {
        match &self.0 {
            IdRepr::Ordinal(id) => {
                out.extend_from_slice(&ORDINAL_MARKER.to_le_bytes());
                out.extend_from_slice(&id.to_le_bytes());
            }
            IdRepr::Name(name) => {
                for unit in name.encode_utf16().chain(core::iter::once(0)) {
                    out.extend_from_slice(&unit.to_le_bytes());
                }
            }
        }
    }
}

impl From<NonZeroU16> for ResourceId {
    fn from(id: NonZeroU16) -> Self {
        Self(IdRepr::Ordinal(id.get()))
    }
}

/// One resource waiting to be written.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Entry {
    kind: ResourceId,
    name: ResourceId,
    language: u16,
    memory_flags: u16,
    data_size: u32,
    header_size: u32,
    data: Vec<u8>,
}

impl Entry {
    /// Validates sizes and computes the header size; does not check for duplicates.
    fn new(
        kind: ResourceId,
        name: ResourceId,
        language: u16,
        memory_flags: u16,
        data: Vec<u8>,
    ) -> Result<Self, ResourceError> {
        let data_size = u32::try_from(data.len()).map_err(|_| ResourceError::TooLarge)?;
        let ids = kind.encoded_len() + name.encoded_len();
        let header = (8 + ids).next_multiple_of(4) + HEADER_SUFFIX_SIZE;
        let header_size = u32::try_from(header).map_err(|_| ResourceError::TooLarge)?;
        Ok(Self {
            kind,
            name,
            language,
            memory_flags,
            data_size,
            header_size,
            data,
        })
    }

    fn write(&self, out: &mut Vec<u8>) {
        out.extend_from_slice(&self.data_size.to_le_bytes());
        out.extend_from_slice(&self.header_size.to_le_bytes());
        self.kind.encode(out);
        self.name.encode(out);
        pad_to_dword(out);
        out.extend_from_slice(&0_u32.to_le_bytes()); // DataVersion
        out.extend_from_slice(&self.memory_flags.to_le_bytes());
        out.extend_from_slice(&self.language.to_le_bytes());
        out.extend_from_slice(&0_u32.to_le_bytes()); // Version
        out.extend_from_slice(&0_u32.to_le_bytes()); // Characteristics
        out.extend_from_slice(&self.data);
        pad_to_dword(out);
    }
}

/// Appends zero bytes until the length is a multiple of four.
pub(crate) fn pad_to_dword(out: &mut Vec<u8>) {
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}

/// Builds a 32-bit `.res` file in memory.
///
/// Resources keep their insertion order. [`add_manifest`](Self::add_manifest),
/// [`add_version_info`](Self::add_version_info), and [`add_icon_group`](Self::add_icon_group)
/// use the writer's default language. [`add_string_table`](Self::add_string_table) and
/// [`add_raw`](Self::add_raw) take the language explicitly, because a string table is usually
/// added once per user-interface language.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResWriter {
    language: u16,
    entries: Vec<Entry>,
}

impl Default for ResWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl ResWriter {
    /// Creates an empty writer whose default language is English (United States).
    pub fn new() -> Self {
        Self::with_language(LANG_EN_US)
    }

    /// Creates an empty writer whose default language is `language`.
    pub fn with_language(language: u16) -> Self {
        Self {
            language,
            entries: Vec::new(),
        }
    }

    /// Default language of [`add_manifest`](Self::add_manifest),
    /// [`add_version_info`](Self::add_version_info), and
    /// [`add_icon_group`](Self::add_icon_group). [`add_string_table`](Self::add_string_table)
    /// does not use it.
    pub fn language(&self) -> u16 {
        self.language
    }

    /// Number of resources added so far.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no resource has been added.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Adds one resource with explicit type, name, language, and memory flags.
    ///
    /// `DataVersion`, `Version`, and `Characteristics` are written as zero.
    pub fn add_raw(
        &mut self,
        kind: ResourceId,
        name: ResourceId,
        language: u16,
        memory_flags: u16,
        data: Vec<u8>,
    ) -> Result<(), ResourceError> {
        if self.contains(&kind, &name, language) {
            return Err(ResourceError::Duplicate);
        }
        let entry = Entry::new(kind, name, language, memory_flags, data)?;
        self.entries.push(entry);
        Ok(())
    }

    /// Adds an executable manifest as `RT_MANIFEST` [`MANIFEST_ID`] in the writer's language.
    pub fn add_manifest(&mut self, manifest: &Manifest) -> Result<(), ResourceError> {
        self.add_raw(
            ResourceId::known(RT_MANIFEST),
            ResourceId::known(MANIFEST_ID),
            self.language,
            MEMORY_MOVEABLE | MEMORY_PURE,
            manifest.to_xml().into_bytes(),
        )
    }

    /// Adds version information as `RT_VERSION` [`VERSION_INFO_ID`] in the writer's language.
    pub fn add_version_info(&mut self, info: &VersionInfo) -> Result<(), ResourceError> {
        let data = info.to_bytes()?;
        self.add_raw(
            ResourceId::known(RT_VERSION),
            ResourceId::known(VERSION_INFO_ID),
            self.language,
            MEMORY_MOVEABLE | MEMORY_PURE,
            data,
        )
    }

    /// Adds an icon group as `RT_GROUP_ICON` `group` plus one `RT_ICON` per image, in the
    /// writer's language.
    ///
    /// `RT_ICON` ordinals are allocated from the smallest ordinals not yet used by any `RT_ICON`
    /// resource. On failure nothing is added.
    pub fn add_icon_group(
        &mut self,
        group: u16,
        images: &[IconImage],
    ) -> Result<(), ResourceError> {
        let group_name = ResourceId::ordinal(group)?;
        if images.is_empty() {
            return Err(ResourceError::EmptyIconGroup);
        }
        let count = u16::try_from(images.len()).map_err(|_| ResourceError::TooLarge)?;
        let group_type = ResourceId::known(RT_GROUP_ICON);
        if self.contains(&group_type, &group_name, self.language) {
            return Err(ResourceError::Duplicate);
        }
        let ids = self.unused_icon_ids(images.len())?;
        let mut prepared = Vec::with_capacity(images.len() + 1);
        let mut directory = Vec::with_capacity(6 + 14 * images.len());
        directory.extend_from_slice(&0_u16.to_le_bytes()); // idReserved
        directory.extend_from_slice(&1_u16.to_le_bytes()); // idType = icon
        directory.extend_from_slice(&count.to_le_bytes());
        for (image, id) in images.iter().zip(ids) {
            let dib = image.to_dib();
            let length = u32::try_from(dib.len()).map_err(|_| ResourceError::TooLarge)?;
            directory.push(dimension_byte(image.size()));
            directory.push(dimension_byte(image.size()));
            directory.push(0); // bColorCount
            directory.push(0); // bReserved
            directory.extend_from_slice(&1_u16.to_le_bytes()); // wPlanes
            directory.extend_from_slice(&32_u16.to_le_bytes()); // wBitCount
            directory.extend_from_slice(&length.to_le_bytes()); // dwBytesInRes
            directory.extend_from_slice(&id.to_le_bytes()); // nID
            prepared.push(Entry::new(
                ResourceId::known(RT_ICON),
                ResourceId::known(id),
                self.language,
                MEMORY_MOVEABLE | MEMORY_DISCARDABLE,
                dib,
            )?);
        }
        prepared.push(Entry::new(
            group_type,
            group_name,
            self.language,
            MEMORY_MOVEABLE | MEMORY_PURE | MEMORY_DISCARDABLE,
            directory,
        )?);
        self.entries.extend(prepared);
        Ok(())
    }

    /// Adds a string table as `RT_STRING` blocks in `language`, which need not be the writer's
    /// default language. On failure nothing is added; an empty table adds nothing.
    pub fn add_string_table(
        &mut self,
        table: &StringTable,
        language: u16,
    ) -> Result<(), ResourceError> {
        let kind = ResourceId::known(RT_STRING);
        let mut prepared = Vec::new();
        for (block, data) in table.blocks()? {
            // Block names start at 1.
            let name = ResourceId::known(block);
            if self.contains(&kind, &name, language) {
                return Err(ResourceError::Duplicate);
            }
            prepared.push(Entry::new(
                kind.clone(),
                name,
                language,
                MEMORY_MOVEABLE | MEMORY_PURE | MEMORY_DISCARDABLE,
                data,
            )?);
        }
        self.entries.extend(prepared);
        Ok(())
    }

    /// The `count` smallest non-zero ordinals not used by any `RT_ICON` resource.
    fn unused_icon_ids(&self, count: usize) -> Result<Vec<u16>, ResourceError> {
        let icon_type = ResourceId::known(RT_ICON);
        let used: BTreeSet<u16> = self
            .entries
            .iter()
            .filter(|entry| entry.kind == icon_type)
            .filter_map(|entry| entry.name.as_ordinal())
            .collect();
        let ids: Vec<u16> = (1..=u16::MAX)
            .filter(|id| !used.contains(id))
            .take(count)
            .collect();
        if ids.len() < count {
            return Err(ResourceError::IconIdsExhausted);
        }
        Ok(ids)
    }

    /// Whether a resource with this type, name, and language exists.
    ///
    /// Ids are compared exactly. For string ids that equals a case-insensitive comparison,
    /// because each is its own upper-case form; it also matches Windows, which upper-cases the
    /// query and then compares exactly.
    fn contains(&self, kind: &ResourceId, name: &ResourceId, language: u16) -> bool {
        self.entries
            .iter()
            .any(|entry| entry.kind == *kind && entry.name == *name && entry.language == language)
    }

    /// Serializes the `.res` file: the leading null entry followed by every resource.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = NULL_ENTRY.to_vec();
        for entry in &self.entries {
            entry.write(&mut out);
        }
        out
    }
}
