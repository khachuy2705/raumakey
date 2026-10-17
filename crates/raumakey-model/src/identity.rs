//! Fixed Raumakey product identities (design §3).

use core::fmt;

/// A COM/TSF GUID stored as its canonical 128-bit value
/// (`data1` in the top 32 bits, `data4` in the low 64 bits, big-endian order).
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Guid(pub u128);

/// Error returned when a GUID string is not in `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GuidParseError;

impl fmt::Display for GuidParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("malformed GUID")
    }
}

impl std::error::Error for GuidParseError {}

impl Guid {
    /// Creates a GUID from its 128-bit value.
    pub const fn from_u128(value: u128) -> Self {
        Self(value)
    }

    /// Returns the 128-bit value.
    pub const fn to_u128(self) -> u128 {
        self.0
    }

    /// COM `Data1`.
    pub const fn data1(self) -> u32 {
        (self.0 >> 96) as u32
    }

    /// COM `Data2`.
    pub const fn data2(self) -> u16 {
        (self.0 >> 80) as u16
    }

    /// COM `Data3`.
    pub const fn data3(self) -> u16 {
        (self.0 >> 64) as u16
    }

    /// COM `Data4`.
    pub const fn data4(self) -> [u8; 8] {
        (self.0 as u64).to_be_bytes()
    }

    /// Parses `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` with or without braces, any hex case.
    pub fn parse(text: &str) -> Result<Self, GuidParseError> {
        let inner = match text.strip_prefix('{') {
            Some(rest) => rest.strip_suffix('}').ok_or(GuidParseError)?,
            None => text,
        };
        let bytes = inner.as_bytes();
        if bytes.len() != 36 {
            return Err(GuidParseError);
        }
        let mut value: u128 = 0;
        for (index, &byte) in bytes.iter().enumerate() {
            if matches!(index, 8 | 13 | 18 | 23) {
                if byte != b'-' {
                    return Err(GuidParseError);
                }
                continue;
            }
            let digit = (byte as char).to_digit(16).ok_or(GuidParseError)?;
            value = (value << 4) | u128::from(digit);
        }
        Ok(Self(value))
    }
}

impl fmt::Display for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let d4 = self.data4();
        write!(
            f,
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
            self.data1(),
            self.data2(),
            self.data3(),
            d4[0],
            d4[1],
            d4[2],
            d4[3],
            d4[4],
            d4[5],
            d4[6],
            d4[7]
        )
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Product name used in user-visible strings.
pub const PRODUCT_NAME: &str = "Raumakey";

/// TSF COM CLSID of the Raumakey text input processor.
pub const TIP_CLSID: Guid = Guid::from_u128(0x0FC4_8F2B_4F91_44A0_B500_DE51_2E80_1432);

/// TSF language profile GUID.
pub const TIP_PROFILE_GUID: Guid = Guid::from_u128(0xDB87_5058_BBF9_407C_85A8_950B_E7D1_4C22);

/// Installer upgrade code.
pub const INSTALLER_UPGRADE_CODE: Guid = Guid::from_u128(0xBF73_893F_FCEB_4BAE_AE1C_E81D_7107_ECE5);

/// Prefix of every Raumakey named kernel object; it is followed by a user-qualified name.
pub const NAMED_OBJECT_PREFIX: &str = "Local\\Raumakey.";

/// Product-owned registry root under `HKEY_CURRENT_USER`.
pub const REGISTRY_ROOT: &str = "Software\\Raumakey";

/// Language identifier of the TSF profile (Vietnamese).
pub const LANGID_VIETNAMESE: u16 = 0x042A;

/// Sentinel placed in `dwExtraInfo` of input injected by Raumakey ("RKY1").
/// It fits in 32 bits so that i686 builds carry it unchanged.
pub const RAUMAKEY_INJECTED_TAG: u64 = 0x524B_5931;
