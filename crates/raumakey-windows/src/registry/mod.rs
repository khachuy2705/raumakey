//! Registry access with a write guard (plan §10.1).
//!
//! Reading is allowed anywhere, always with `KEY_READ` only. Writing (create, set, delete) is
//! possible only through [`OwnedRegKey`] and [`delete_tree`], which accept exclusively paths for
//! which [`is_owned_path`] holds:
//!
//! - `Software\Raumakey` and everything below it (under `HKCU` or `HKLM`);
//! - `Software\Classes\CLSID\{TIP_CLSID}` and everything below it (the Raumakey TIP class).
//!
//! Everything else, including the legacy EVKey roots, other CLSIDs, explicit `WOW6432Node`
//! paths (use [`RegView`] instead), `.`/`..` components, empty components, and prefix tricks
//! such as `Software\RaumakeyEvil`, fails with `AccessDenied` before any registry call.
//!
//! The pure part of this module (path guard, value encoding) compiles on every platform.

use core::fmt;

use raumakey_model::identity::TIP_CLSID;

/// Registry value type numbers (`REG_*`).
pub mod value_type {
    /// `REG_NONE`.
    pub const REG_NONE: u32 = 0;
    /// `REG_SZ`.
    pub const REG_SZ: u32 = 1;
    /// `REG_EXPAND_SZ`.
    pub const REG_EXPAND_SZ: u32 = 2;
    /// `REG_BINARY`.
    pub const REG_BINARY: u32 = 3;
    /// `REG_DWORD` (little-endian).
    pub const REG_DWORD: u32 = 4;
    /// `REG_DWORD_BIG_ENDIAN`.
    pub const REG_DWORD_BIG_ENDIAN: u32 = 5;
    /// `REG_LINK`.
    pub const REG_LINK: u32 = 6;
    /// `REG_MULTI_SZ`.
    pub const REG_MULTI_SZ: u32 = 7;
    /// `REG_QWORD` (little-endian).
    pub const REG_QWORD: u32 = 11;
}

use value_type::{REG_BINARY, REG_DWORD, REG_EXPAND_SZ, REG_MULTI_SZ, REG_QWORD, REG_SZ};

/// A predefined root key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegRoot {
    /// `HKEY_CURRENT_USER`.
    CurrentUser,
    /// `HKEY_LOCAL_MACHINE`.
    LocalMachine,
}

/// Which WOW64 registry view to use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegView {
    /// The view of the calling process.
    Default,
    /// The 64-bit view (`KEY_WOW64_64KEY`).
    Wow64_64,
    /// The 32-bit view (`KEY_WOW64_32KEY`).
    Wow64_32,
}

impl RegView {
    /// The `KEY_WOW64_*` access flag of the view (0 for [`RegView::Default`]).
    pub const fn sam_flags(self) -> u32 {
        match self {
            Self::Default => 0,
            Self::Wow64_64 => 0x0100,
            Self::Wow64_32 => 0x0200,
        }
    }
}

/// A decoded registry value.
///
/// `Debug` shows only the type and size, never string content or bytes.
#[derive(Clone, PartialEq, Eq)]
pub enum RegValue {
    /// `REG_SZ`.
    String(String),
    /// `REG_EXPAND_SZ` (not expanded).
    ExpandString(String),
    /// `REG_MULTI_SZ`.
    MultiString(Vec<String>),
    /// `REG_DWORD`.
    Dword(u32),
    /// `REG_QWORD`.
    Qword(u64),
    /// `REG_BINARY`.
    Binary(Vec<u8>),
    /// Any other type, or data that does not decode exactly as its type (odd-length or
    /// ill-formed UTF-16 strings, numbers of the wrong size): type number and raw bytes.
    Other(u32, Vec<u8>),
}

impl fmt::Debug for RegValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::String(text) => write!(f, "String(<{} chars>)", text.chars().count()),
            Self::ExpandString(text) => {
                write!(f, "ExpandString(<{} chars>)", text.chars().count())
            }
            Self::MultiString(items) => write!(f, "MultiString(<{} entries>)", items.len()),
            Self::Dword(value) => write!(f, "Dword({value})"),
            Self::Qword(value) => write!(f, "Qword({value})"),
            Self::Binary(bytes) => write!(f, "Binary(<{} bytes>)", bytes.len()),
            Self::Other(kind, bytes) => write!(f, "Other({kind}, <{} bytes>)", bytes.len()),
        }
    }
}

/// Why a value cannot be written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RegValueError {
    /// A string contains NUL, which the registry string types cannot represent.
    InteriorNul,
    /// A multi-string entry is empty; it would end the list early.
    EmptyMultiStringEntry,
}

impl fmt::Display for RegValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InteriorNul => "registry string contains NUL",
            Self::EmptyMultiStringEntry => "registry multi-string contains an empty entry",
        })
    }
}

impl std::error::Error for RegValueError {}

impl RegValue {
    /// Decodes raw value data of type `kind`. String types may or may not carry terminators;
    /// a multi-string ends at its first empty entry.
    pub fn from_raw(kind: u32, data: &[u8]) -> Self {
        let decoded = match kind {
            REG_SZ => decode_string(data).map(Self::String),
            REG_EXPAND_SZ => decode_string(data).map(Self::ExpandString),
            REG_MULTI_SZ => decode_multi_string(data).map(Self::MultiString),
            REG_DWORD => <[u8; 4]>::try_from(data)
                .ok()
                .map(|bytes| Self::Dword(u32::from_le_bytes(bytes))),
            REG_QWORD => <[u8; 8]>::try_from(data)
                .ok()
                .map(|bytes| Self::Qword(u64::from_le_bytes(bytes))),
            REG_BINARY => Some(Self::Binary(data.to_vec())),
            _ => None,
        };
        decoded.unwrap_or_else(|| Self::Other(kind, data.to_vec()))
    }

    /// Encodes the value as its type number and raw data (strings with terminators).
    pub fn to_raw(&self) -> Result<(u32, Vec<u8>), RegValueError> {
        let data = match self {
            Self::String(text) | Self::ExpandString(text) => {
                if text.contains('\0') {
                    return Err(RegValueError::InteriorNul);
                }
                utf16le(text.encode_utf16().chain(Some(0)))
            }
            Self::MultiString(items) => {
                if items.iter().any(String::is_empty) {
                    return Err(RegValueError::EmptyMultiStringEntry);
                }
                if items.iter().any(|item| item.contains('\0')) {
                    return Err(RegValueError::InteriorNul);
                }
                let mut units: Vec<u16> = Vec::new();
                for item in items {
                    units.extend(item.encode_utf16());
                    units.push(0);
                }
                units.push(0);
                if items.is_empty() {
                    units.push(0);
                }
                utf16le(units)
            }
            Self::Dword(value) => value.to_le_bytes().to_vec(),
            Self::Qword(value) => value.to_le_bytes().to_vec(),
            Self::Binary(bytes) | Self::Other(_, bytes) => bytes.clone(),
        };
        Ok((self.value_type(), data))
    }

    /// The `REG_*` type number.
    pub const fn value_type(&self) -> u32 {
        match self {
            Self::String(_) => REG_SZ,
            Self::ExpandString(_) => REG_EXPAND_SZ,
            Self::MultiString(_) => REG_MULTI_SZ,
            Self::Dword(_) => REG_DWORD,
            Self::Qword(_) => REG_QWORD,
            Self::Binary(_) => REG_BINARY,
            Self::Other(kind, _) => *kind,
        }
    }
}

fn utf16le(units: impl IntoIterator<Item = u16>) -> Vec<u8> {
    units.into_iter().flat_map(u16::to_le_bytes).collect()
}

fn units_of(data: &[u8]) -> Option<Vec<u16>> {
    let (pairs, rest) = data.as_chunks::<2>();
    rest.is_empty()
        .then(|| pairs.iter().map(|pair| u16::from_le_bytes(*pair)).collect())
}

fn decode_string(data: &[u8]) -> Option<String> {
    let units = units_of(data)?;
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    String::from_utf16(units.get(..end)?).ok()
}

fn decode_multi_string(data: &[u8]) -> Option<Vec<String>> {
    let units = units_of(data)?;
    let mut items = Vec::new();
    for item in units.split(|&unit| unit == 0) {
        if item.is_empty() {
            break;
        }
        items.push(String::from_utf16(item).ok()?);
    }
    Some(items)
}

/// True when `path` (relative to `root`) is a key Raumakey may write: `Software\Raumakey[\…]`
/// or `Software\Classes\CLSID\{TIP_CLSID}[\…]`, compared component-wise and ASCII
/// case-insensitively. Empty, `.`, and `..` components, `/`, and NUL are rejected. Both roots
/// are accepted for both prefixes.
pub fn is_owned_path(root: RegRoot, path: &str) -> bool {
    // Both roots share the same rules; the parameter keeps call sites explicit.
    let _ = root;
    if path.contains(['\0', '/']) {
        return false;
    }
    let components: Vec<&str> = path.split('\\').collect();
    if components
        .iter()
        .any(|component| component.is_empty() || *component == "." || *component == "..")
    {
        return false;
    }
    let starts_with = |prefix: &[&str]| {
        components.len() >= prefix.len()
            && prefix
                .iter()
                .zip(&components)
                .all(|(expected, actual)| actual.eq_ignore_ascii_case(expected))
    };
    let clsid = TIP_CLSID.to_string();
    starts_with(&["Software", "Raumakey"]) || starts_with(&["Software", "Classes", "CLSID", &clsid])
}

#[cfg(windows)]
mod win;

#[cfg(windows)]
pub use win::{OwnedRegKey, RegKey, delete_empty_key, delete_tree, key_exists};
