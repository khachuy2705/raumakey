//! `VS_VERSIONINFO` builder and version-string parsing.
//!
//! The version resource is a tree of nodes. Every node starts on a DWORD boundary and has this
//! layout:
//!
//! ```text
//! WORD  wLength        bytes from the node start to the end of its value or last child
//! WORD  wValueLength   value size: bytes for binary values, UTF-16 units (with NUL) for text
//! WORD  wType          0 = binary value, 1 = text value or block
//! WCHAR szKey[]        NUL-terminated key
//! padding              to a DWORD boundary
//! Value                optional
//! Children             each preceded by padding to a DWORD boundary
//! ```
//!
//! A node's `wLength` excludes padding after its own last byte; a parent's `wLength` includes the
//! padding that aligns each child.

use std::borrow::Cow;

use crate::error::ResourceError;
use crate::res::{LANG_EN_US, pad_to_dword};

/// Company name written into every Raumakey version resource.
pub const COMPANY_NAME: &str = "Raumakey contributors";
/// Product name written into every Raumakey version resource.
pub const PRODUCT_NAME: &str = "Raumakey";
/// Copyright notice written into every Raumakey version resource.
pub const LEGAL_COPYRIGHT: &str =
    "Copyright (C) 2026 Raumakey contributors. Licensed under GPL-3.0-or-later.";
/// Code page of version strings: UTF-16 little endian (1200).
pub const CODE_PAGE_UNICODE: u16 = 1200;
/// `dwFileFlags` bit marking a pre-release build (`VS_FF_PRERELEASE`).
pub const VS_FF_PRERELEASE: u32 = 0x0000_0002;

const VS_FFI_SIGNATURE: u32 = 0xFEEF_04BD;
const VS_FFI_STRUCVERSION: u32 = 0x0001_0000;
const VS_FFI_FILEFLAGSMASK: u32 = 0x0000_003F;
const VOS_NT_WINDOWS32: u32 = 0x0004_0004;
const VFT_APP: u32 = 0x0000_0001;
const VFT_DLL: u32 = 0x0000_0002;

/// Kind of binary described by version information (`dwFileType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileType {
    /// Executable (`VFT_APP`).
    App,
    /// Dynamic-link library (`VFT_DLL`).
    Dll,
}

impl FileType {
    const fn code(self) -> u32 {
        match self {
            Self::App => VFT_APP,
            Self::Dll => VFT_DLL,
        }
    }
}

/// A parsed version string.
struct ParsedVersion {
    quad: [u16; 4],
    prerelease: bool,
}

/// Parses `major.minor.patch[.build]`, optionally followed by a semantic-version pre-release
/// (`-alpha.1`) and/or build metadata (`+meta`), into the four 16-bit components of a Windows
/// file version. A missing build component is zero; the suffixes do not affect the result.
///
/// The syntax is Semantic Versioning 2.0.0 plus the optional fourth component:
///
/// - every numeric component is ASCII digits without a leading zero and at most 65,535;
/// - pre-release and build metadata are dot-separated, non-empty identifiers of ASCII letters,
///   digits, and `-`;
/// - an all-digit pre-release identifier has no leading zero (build metadata may have one).
pub fn version_quad(text: &str) -> Result<[u16; 4], ResourceError> {
    parse_version(text).map(|parsed| parsed.quad)
}

fn parse_version(text: &str) -> Result<ParsedVersion, ResourceError> {
    let split = text.find(['-', '+']).unwrap_or(text.len());
    let (core, suffix) = text.split_at(split);
    let prerelease = validate_suffix(suffix)?;
    let mut quad = [0_u16; 4];
    let mut count = 0_usize;
    for part in core.split('.') {
        let slot = quad.get_mut(count).ok_or(ResourceError::InvalidVersion)?;
        if !is_numeric_identifier(part) || has_leading_zero(part) {
            return Err(ResourceError::InvalidVersion);
        }
        *slot = part
            .parse::<u16>()
            .map_err(|_| ResourceError::InvalidVersion)?;
        count += 1;
    }
    if count < 3 {
        return Err(ResourceError::InvalidVersion);
    }
    Ok(ParsedVersion { quad, prerelease })
}

/// Whether `part` is one or more ASCII digits.
fn is_numeric_identifier(part: &str) -> bool {
    !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit())
}

/// Whether a number is written with a superfluous leading zero (`"01"`, but not `"0"`).
fn has_leading_zero(part: &str) -> bool {
    part.len() > 1 && part.starts_with('0')
}

/// Validates `-pre`, `+meta`, or `-pre+meta`; returns whether a pre-release part is present.
fn validate_suffix(suffix: &str) -> Result<bool, ResourceError> {
    if suffix.is_empty() {
        return Ok(false);
    }
    let (prerelease, metadata) = match suffix.strip_prefix('-') {
        Some(rest) => match rest.split_once('+') {
            Some((pre, meta)) => (Some(pre), Some(meta)),
            None => (Some(rest), None),
        },
        None => (None, suffix.strip_prefix('+')),
    };
    let valid_prerelease = prerelease.is_none_or(|part| valid_identifiers(part, true));
    let valid_metadata = metadata.is_none_or(|part| valid_identifiers(part, false));
    if !(valid_prerelease && valid_metadata) {
        return Err(ResourceError::InvalidVersion);
    }
    Ok(prerelease.is_some())
}

/// Whether `part` is dot-separated, non-empty identifiers of ASCII letters, digits, and `-`.
/// In a pre-release, all-digit identifiers may not have a leading zero.
fn valid_identifiers(part: &str, prerelease: bool) -> bool {
    part.split('.').all(|identifier| {
        let charset = !identifier.is_empty()
            && identifier
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
        let padded_number =
            prerelease && is_numeric_identifier(identifier) && has_leading_zero(identifier);
        charset && !padded_number
    })
}

/// Formats a version quad as `a.b.c.d`.
pub(crate) fn dotted(quad: [u16; 4]) -> String {
    let [a, b, c, d] = quad;
    format!("{a}.{b}.{c}.{d}")
}

/// Version information for one binary (`RT_VERSION` resource data).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VersionInfo {
    file_type: FileType,
    file_version: [u16; 4],
    product_version: [u16; 4],
    file_flags: u32,
    language: u16,
    code_page: u16,
    strings: Vec<(String, String)>,
}

impl VersionInfo {
    /// Creates version information without strings. The string table and translation use
    /// English (United States) and the UTF-16 code page.
    pub fn new(file_type: FileType, file_version: [u16; 4], product_version: [u16; 4]) -> Self {
        Self {
            file_type,
            file_version,
            product_version,
            file_flags: 0,
            language: LANG_EN_US,
            code_page: CODE_PAGE_UNICODE,
            strings: Vec::new(),
        }
    }

    /// Creates the standard Raumakey version information.
    ///
    /// `version` is parsed with [`version_quad`] and used for both the file and the product
    /// version. `FileVersion` is the dotted quad, `ProductVersion` is `version` as given, and a
    /// pre-release suffix sets `VS_FF_PRERELEASE`. Company, product, and copyright strings are
    /// the Raumakey constants.
    pub fn raumakey(
        file_type: FileType,
        version: &str,
        file_description: &str,
        internal_name: &str,
        original_filename: &str,
    ) -> Result<Self, ResourceError> {
        let parsed = parse_version(version)?;
        let mut info = Self::new(file_type, parsed.quad, parsed.quad);
        if parsed.prerelease {
            info.set_file_flags(VS_FF_PRERELEASE);
        }
        let file_version = dotted(parsed.quad);
        for (key, value) in [
            ("CompanyName", COMPANY_NAME),
            ("FileDescription", file_description),
            ("FileVersion", file_version.as_str()),
            ("InternalName", internal_name),
            ("LegalCopyright", LEGAL_COPYRIGHT),
            ("OriginalFilename", original_filename),
            ("ProductName", PRODUCT_NAME),
            ("ProductVersion", version),
        ] {
            info.set_string(key, value)?;
        }
        Ok(info)
    }

    /// Sets a string-table value. An existing key keeps its position; a new key is appended.
    /// Keys must be non-empty; neither keys nor values may contain NUL.
    pub fn set_string(&mut self, key: &str, value: &str) -> Result<(), ResourceError> {
        if key.is_empty() || key.contains('\0') || value.contains('\0') {
            return Err(ResourceError::InvalidText);
        }
        match self
            .strings
            .iter_mut()
            .find(|(existing, _)| existing == key)
        {
            Some((_, slot)) => value.clone_into(slot),
            None => self.strings.push((key.to_owned(), value.to_owned())),
        }
        Ok(())
    }

    /// Returns a string-table value.
    pub fn string(&self, key: &str) -> Option<&str> {
        self.strings
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, value)| value.as_str())
    }

    /// Sets the language and code page of the string table and the `Translation` value.
    pub fn set_translation(&mut self, language: u16, code_page: u16) {
        self.language = language;
        self.code_page = code_page;
    }

    /// Sets `dwFileFlags`; bits outside `VS_FFI_FILEFLAGSMASK` are dropped.
    pub fn set_file_flags(&mut self, flags: u32) {
        self.file_flags = flags & VS_FFI_FILEFLAGSMASK;
    }

    /// Binary file version (`dwFileVersionMS`/`LS`).
    pub fn file_version(&self) -> [u16; 4] {
        self.file_version
    }

    /// Binary product version (`dwProductVersionMS`/`LS`).
    pub fn product_version(&self) -> [u16; 4] {
        self.product_version
    }

    /// Kind of binary.
    pub fn file_type(&self) -> FileType {
        self.file_type
    }

    /// `dwFileFlags`.
    pub fn file_flags(&self) -> u32 {
        self.file_flags
    }

    /// Serializes the `VS_VERSIONINFO` tree.
    ///
    /// Fails with [`ResourceError::TooLarge`] when a node would exceed the 16-bit `wLength`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ResourceError> {
        let fixed = self.fixed_file_info();
        let translation = {
            let mut value = [0_u8; 4];
            value[..2].copy_from_slice(&self.language.to_le_bytes());
            value[2..].copy_from_slice(&self.code_page.to_le_bytes());
            value
        };
        let strings = self
            .strings
            .iter()
            .map(|(key, value)| Node::leaf(Cow::Borrowed(key), Value::Text(value)))
            .collect();
        let table_key = format!("{:04X}{:04X}", self.language, self.code_page);
        let root = Node {
            key: Cow::Borrowed("VS_VERSION_INFO"),
            value: Value::Binary(&fixed),
            children: vec![
                Node::block(
                    "StringFileInfo",
                    vec![Node::block_owned(table_key, strings)],
                ),
                Node::block(
                    "VarFileInfo",
                    vec![Node::leaf(
                        Cow::Borrowed("Translation"),
                        Value::Binary(&translation),
                    )],
                ),
            ],
        };
        let mut out = Vec::with_capacity(512);
        write_node(&mut out, &root)?;
        Ok(out)
    }

    /// `VS_FIXEDFILEINFO` (52 bytes).
    fn fixed_file_info(&self) -> [u8; 52] {
        let [file_a, file_b, file_c, file_d] = self.file_version;
        let [product_a, product_b, product_c, product_d] = self.product_version;
        let fields: [u32; 13] = [
            VS_FFI_SIGNATURE,
            VS_FFI_STRUCVERSION,
            pack(file_a, file_b),
            pack(file_c, file_d),
            pack(product_a, product_b),
            pack(product_c, product_d),
            VS_FFI_FILEFLAGSMASK,
            self.file_flags,
            VOS_NT_WINDOWS32,
            self.file_type.code(),
            0, // dwFileSubtype
            0, // dwFileDateMS
            0, // dwFileDateLS
        ];
        let mut bytes = [0_u8; 52];
        for (chunk, field) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(fields) {
            *chunk = field.to_le_bytes();
        }
        bytes
    }
}

/// Packs two 16-bit components into the high and low words of a DWORD.
fn pack(high: u16, low: u16) -> u32 {
    (u32::from(high) << 16) | u32::from(low)
}

/// Value of a version node.
enum Value<'a> {
    /// A block without a value (`wType` 1, `wValueLength` 0).
    None,
    /// Binary value (`wType` 0, `wValueLength` in bytes).
    Binary(&'a [u8]),
    /// Text value (`wType` 1, `wValueLength` in UTF-16 units including the NUL).
    Text(&'a str),
}

struct Node<'a> {
    key: Cow<'a, str>,
    value: Value<'a>,
    children: Vec<Node<'a>>,
}

impl<'a> Node<'a> {
    fn leaf(key: Cow<'a, str>, value: Value<'a>) -> Self {
        Self {
            key,
            value,
            children: Vec::new(),
        }
    }

    fn block(key: &'a str, children: Vec<Node<'a>>) -> Self {
        Self {
            key: Cow::Borrowed(key),
            value: Value::None,
            children,
        }
    }

    fn block_owned(key: String, children: Vec<Node<'a>>) -> Self {
        Self {
            key: Cow::Owned(key),
            value: Value::None,
            children,
        }
    }
}

/// Appends `text` as UTF-16LE followed by a NUL unit.
fn write_utf16z(out: &mut Vec<u8>, text: &str) {
    for unit in text.encode_utf16().chain(core::iter::once(0)) {
        out.extend_from_slice(&unit.to_le_bytes());
    }
}

fn write_node(out: &mut Vec<u8>, node: &Node<'_>) -> Result<(), ResourceError> {
    pad_to_dword(out);
    let start = out.len();
    let (value_length, kind) = match node.value {
        Value::None => (0, 1_u16),
        Value::Binary(bytes) => (bytes.len(), 0),
        Value::Text(text) => (text.encode_utf16().count() + 1, 1),
    };
    let value_length = u16::try_from(value_length).map_err(|_| ResourceError::TooLarge)?;
    out.extend_from_slice(&0_u16.to_le_bytes()); // wLength, patched below
    out.extend_from_slice(&value_length.to_le_bytes());
    out.extend_from_slice(&kind.to_le_bytes());
    write_utf16z(out, &node.key);
    pad_to_dword(out);
    match node.value {
        Value::None => {}
        Value::Binary(bytes) => out.extend_from_slice(bytes),
        Value::Text(text) => write_utf16z(out, text),
    }
    for child in &node.children {
        write_node(out, child)?;
    }
    let length = u16::try_from(out.len() - start).map_err(|_| ResourceError::TooLarge)?;
    if let Some(slot) = out.get_mut(start..start + 2) {
        slot.copy_from_slice(&length.to_le_bytes());
    }
    Ok(())
}
