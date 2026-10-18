//! Host identity (clean-room spec §3.5).

use core::fmt;

use serde::{Deserialize, Serialize};

/// Opaque identifier that partitions engine state per TSF activation/document context.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Debug, Serialize, Deserialize,
)]
pub struct SessionId(pub [u8; 16]);

impl SessionId {
    /// Creates an identifier from a 128-bit value.
    pub const fn from_u128(value: u128) -> Self {
        Self(value.to_be_bytes())
    }

    /// Returns the 128-bit value.
    pub const fn to_u128(self) -> u128 {
        u128::from_be_bytes(self.0)
    }
}

/// Opaque identifier of a document within a host, when the host exposes one.
#[derive(
    Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Debug, Serialize, Deserialize,
)]
pub struct DocumentId(pub [u8; 16]);

impl DocumentId {
    /// Creates an identifier from a 128-bit value.
    pub const fn from_u128(value: u128) -> Self {
        Self(value.to_be_bytes())
    }

    /// Returns the 128-bit value.
    pub const fn to_u128(self) -> u128 {
        u128::from_be_bytes(self.0)
    }
}

/// Why an executable name was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExeNameError {
    /// Empty after trimming.
    Empty,
    /// Longer than [`ExeName::MAX_LEN`] characters once lowercased.
    TooLong,
    /// Contains a path separator, drive separator, NUL, or control character.
    InvalidChar,
}

impl fmt::Display for ExeNameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Empty => "executable name is empty",
            Self::TooLong => "executable name is too long",
            Self::InvalidChar => "executable name contains an invalid character",
        })
    }
}

impl std::error::Error for ExeNameError {}

/// Lowercase executable basename such as `notepad.exe`.
///
/// Every name reads back as itself: [`ExeName::new`] of [`ExeName::as_str`] (which is how the
/// IPC channel and the configuration files read a name) gives the same name.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ExeName(String);

impl ExeName {
    /// Maximum length in characters, of the lowercase name.
    pub const MAX_LEN: usize = 255;

    /// Validates and lowercases a basename. The length limit applies to the lowercase name,
    /// since lowercasing can lengthen it (`İ`, U+0130, becomes two characters).
    pub fn new(raw: &str) -> Result<Self, ExeNameError> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return Err(ExeNameError::Empty);
        }
        // A cheap bound first: lowercasing never shortens a name.
        if trimmed.chars().count() > Self::MAX_LEN {
            return Err(ExeNameError::TooLong);
        }
        if trimmed
            .chars()
            .any(|c| matches!(c, '\\' | '/' | ':') || c.is_control())
        {
            return Err(ExeNameError::InvalidChar);
        }
        let lower = trimmed.to_lowercase();
        if lower.chars().count() > Self::MAX_LEN {
            return Err(ExeNameError::TooLong);
        }
        Ok(Self(lower))
    }

    /// Takes the basename of a path (either separator) and validates it as [`ExeName::new`]
    /// does.
    pub fn from_path(path: &str) -> Result<Self, ExeNameError> {
        let base = path.rsplit(['\\', '/']).next().unwrap_or(path);
        Self::new(base)
    }

    /// The lowercase basename.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ExeName {
    type Error = ExeNameError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<ExeName> for String {
    fn from(value: ExeName) -> Self {
        value.0
    }
}

impl fmt::Display for ExeName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Describes the host that produced an input event.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HostContext {
    /// Partition key of engine state (TSF activation/document context or fallback session).
    pub session_id: SessionId,
    /// Host process identifier.
    pub process_id: u64,
    /// Host thread identifier.
    pub thread_id: u64,
    /// Lowercase executable basename.
    pub executable: ExeName,
    /// Window class of the focused window, when known.
    pub window_class: Option<String>,
    /// Document identifier, when the host exposes one.
    pub document_id: Option<DocumentId>,
    /// Whether the host supports TSF range edits.
    pub supports_tsf_range_edit: bool,
    /// Whether the focused field is a password/secure field.
    pub is_secure_field: bool,
    /// Whether the host process is elevated.
    pub is_elevated: bool,
    /// Active keyboard layout identifier (KLID form such as `00000409`).
    pub keyboard_layout: String,
    /// Foreground change counter maintained by the adapter.
    pub foreground_epoch: u64,
}

impl HostContext {
    /// Creates a context with conservative defaults for everything but the identity.
    pub fn new(session_id: SessionId, executable: ExeName) -> Self {
        Self {
            session_id,
            process_id: 0,
            thread_id: 0,
            executable,
            window_class: None,
            document_id: None,
            supports_tsf_range_edit: true,
            is_secure_field: false,
            is_elevated: false,
            keyboard_layout: "00000409".to_owned(),
            foreground_epoch: 0,
        }
    }

    /// The key that owns engine state for this host.
    pub fn owner_key(&self) -> (SessionId, Option<DocumentId>) {
        (self.session_id, self.document_id)
    }
}
