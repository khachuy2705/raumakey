//! Typed failures of the resource writer.

use core::fmt;

/// A resource could not be represented in the Windows resource formats.
///
/// None of the variants carries caller text, so the error can be logged safely.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResourceError {
    /// A numeric resource type or name was zero.
    ZeroOrdinal,
    /// A string resource type or name was empty, contained NUL, exceeded 65,535 UTF-16 units,
    /// started with `#` or U+FFFF, or was not its own Unicode upper-case form (see
    /// [`ResourceId::named`](crate::ResourceId::named)).
    InvalidName,
    /// Another resource already has the same type, name, and language.
    Duplicate,
    /// Data exceeds a length field of the format (32-bit sizes, 16-bit version-block lengths,
    /// 16-bit string-table lengths, or 16-bit entry counts).
    TooLarge,
    /// A version string is not `major.minor.patch[.build][-pre][+meta]` in Semantic Versioning
    /// 2.0.0 syntax with 16-bit components.
    InvalidVersion,
    /// A key was empty, or a key or value contained NUL or an unsupported character.
    InvalidText,
    /// An icon size is outside `1..=256` pixels.
    InvalidIconSize,
    /// An icon group or icon file has no images.
    EmptyIconGroup,
    /// Every `RT_ICON` ordinal is already in use.
    IconIdsExhausted,
}

impl fmt::Display for ResourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ZeroOrdinal => "resource ordinal must not be zero",
            Self::InvalidName => {
                "resource name is empty, too long, not upper case, starts with '#' or U+FFFF, \
                 or contains NUL"
            }
            Self::Duplicate => "resource type, name, and language are already used",
            Self::TooLarge => "resource data exceeds a length field of the format",
            Self::InvalidVersion => "version is not a semantic version with 16-bit components",
            Self::InvalidText => "resource text is empty or contains an unsupported character",
            Self::InvalidIconSize => "icon size must be between 1 and 256 pixels",
            Self::EmptyIconGroup => "icon group has no images",
            Self::IconIdsExhausted => "no unused RT_ICON ordinal is left",
        })
    }
}

impl std::error::Error for ResourceError {}
