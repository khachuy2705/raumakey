//! Supported charsets and their stable names (plan D19).

use core::fmt;
use core::str::FromStr;

/// A Vietnamese text encoding the converter can read and write.
///
/// Legacy 8-bit charsets ([`Charset::Tcvn3`], [`Charset::VniWindows`], [`Charset::Viscii`],
/// [`Charset::Cp1258`]) appear in Unicode text (clipboard, documents) as the Windows-1252
/// characters of their bytes; the text functions of this crate use that representation and the
/// byte functions use the raw bytes. The other charsets are conventions over Unicode text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Charset {
    /// Precomposed Unicode (NFC). Bytes: UTF-8 (UTF-8/UTF-16 byte order marks are honored when
    /// reading).
    Unicode,
    /// Decomposed Unicode (NFD). Bytes: UTF-8.
    UnicodeNfd,
    /// VIQR (RFC 1456): ASCII letters followed by a vowel-mark character (`^`, `(`, `+`) and then
    /// a tone character (`'`, `` ` ``, `?`, `~`, `.`), `dd`/`DD` for đ/Đ, and `\` to escape a
    /// character that would otherwise be read as a mark. Bytes: UTF-8.
    Viqr,
    /// TCVN 5712:1993 VN3 ("ABC", used with `.Vn*` fonts). It has no precomposed uppercase
    /// letters with tone marks, so encoding them is lossy (see [`crate::encode_with_report`]).
    Tcvn3,
    /// VNI Windows (VNI fonts): a base letter followed by a diacritic byte.
    VniWindows,
    /// VISCII (RFC 1456).
    Viscii,
    /// Windows-1258: precomposed letters where the code page has them, otherwise the letter
    /// without tone followed by a combining tone mark.
    Cp1258,
    /// Numeric character references in decimal (`&#7871;`). Bytes: UTF-8.
    NcrDecimal,
    /// Numeric character references in hexadecimal (`&#x1EBF;`). Bytes: UTF-8.
    NcrHex,
    /// UTF-8 bytes shown as Windows-1252 characters (`Tiáº¿ng`). Bytes: UTF-8 of that text.
    Utf8Literal,
}

impl Charset {
    /// Every charset, in declaration order.
    pub const ALL: [Self; 10] = [
        Self::Unicode,
        Self::UnicodeNfd,
        Self::Viqr,
        Self::Tcvn3,
        Self::VniWindows,
        Self::Viscii,
        Self::Cp1258,
        Self::NcrDecimal,
        Self::NcrHex,
        Self::Utf8Literal,
    ];

    /// Stable lowercase name used in configuration files and on the command line.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unicode => "unicode",
            Self::UnicodeNfd => "unicode-nfd",
            Self::Viqr => "viqr",
            Self::Tcvn3 => "tcvn3",
            Self::VniWindows => "vni-windows",
            Self::Viscii => "viscii",
            Self::Cp1258 => "cp1258",
            Self::NcrDecimal => "ncr-decimal",
            Self::NcrHex => "ncr-hex",
            Self::Utf8Literal => "utf8-literal",
        }
    }

    /// True when every Vietnamese letter (all 134 non-ASCII letters, both cases, every tone) can
    /// be encoded exactly. Only [`Charset::Tcvn3`] is lossy.
    pub const fn is_lossless(self) -> bool {
        !matches!(self, Self::Tcvn3)
    }

    /// True for the 8-bit charsets whose text form is the Windows-1252 view of their bytes.
    pub const fn is_legacy_byte(self) -> bool {
        matches!(
            self,
            Self::Tcvn3 | Self::VniWindows | Self::Viscii | Self::Cp1258
        )
    }
}

impl fmt::Display for Charset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Rejected charset name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParseCharsetError;

impl fmt::Display for ParseCharsetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown charset name")
    }
}

impl std::error::Error for ParseCharsetError {}

impl FromStr for Charset {
    type Err = ParseCharsetError;

    /// Parses a stable name, ignoring ASCII case and surrounding whitespace.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let wanted = text.trim();
        Self::ALL
            .into_iter()
            .find(|charset| charset.name().eq_ignore_ascii_case(wanted))
            .ok_or(ParseCharsetError)
    }
}
