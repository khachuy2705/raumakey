//! Typed IPC errors and wire error codes. No variant carries user text, so errors can be logged
//! through the typed diagnostics API without exposing keystrokes, documents, or macro content.

use core::fmt;
use std::io;

use raumakey_model::diag::DiagCode;
use serde::{Deserialize, Serialize};

use crate::message::{
    MAX_APP_RULES, MAX_HOTKEYS, MAX_KEYMAPS, MAX_MACROS, MAX_NAMED_SET_MEMBERS, MAX_NAMED_SETS,
    MAX_PROBES, MessageKind,
};

/// Stable numeric code carried by `Error` and `CommandResult` messages.
///
/// The value space is open so that a newer peer can report codes this build does not name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ErrorCode(pub u32);

impl ErrorCode {
    /// Success.
    pub const OK: Self = Self(0);
    /// The peer speaks another protocol version.
    pub const UNSUPPORTED_PROTOCOL: Self = Self(1);
    /// A frame failed validation; the stream cannot be trusted any more.
    pub const MALFORMED_FRAME: Self = Self(2);
    /// A message arrived in a state or direction where it is not allowed.
    pub const UNEXPECTED_MESSAGE: Self = Self(3);
    /// A frame carried another client's identifier.
    pub const CLIENT_ID_MISMATCH: Self = Self(4);
    /// A collection exceeded its entry limit.
    pub const LIMIT_EXCEEDED: Self = Self(5);
    /// The command was understood but could not be executed.
    pub const COMMAND_FAILED: Self = Self(6);
    /// The command is not supported by this server.
    pub const COMMAND_UNSUPPORTED: Self = Self(7);
    /// The server is shutting down.
    pub const SHUTTING_DOWN: Self = Self(8);
    /// Internal failure.
    pub const INTERNAL: Self = Self(9);

    /// True for [`ErrorCode::OK`].
    pub const fn is_ok(self) -> bool {
        self.0 == Self::OK.0
    }
}

/// Collections whose entry count is bounded on encode and decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Limit {
    /// Macro entries in a macro snapshot.
    Macros,
    /// Key maps in a runtime snapshot.
    Keymaps,
    /// Hotkeys per action in a runtime snapshot.
    Hotkeys,
    /// Application rules.
    AppRules,
    /// Named sets.
    NamedSets,
    /// Members of one named set.
    NamedSetMembers,
    /// Probe records.
    Probes,
}

impl Limit {
    /// Largest accepted entry count.
    pub const fn max(self) -> usize {
        match self {
            Self::Macros => MAX_MACROS,
            Self::Keymaps => MAX_KEYMAPS,
            Self::Hotkeys => MAX_HOTKEYS,
            Self::AppRules => MAX_APP_RULES,
            Self::NamedSets => MAX_NAMED_SETS,
            Self::NamedSetMembers => MAX_NAMED_SET_MEMBERS,
            Self::Probes => MAX_PROBES,
        }
    }

    /// Fails when `count` exceeds the limit.
    pub fn check(self, count: usize) -> Result<(), IpcError> {
        if count > self.max() {
            Err(IpcError::LimitExceeded {
                limit: self,
                count: u64::try_from(count).unwrap_or(u64::MAX),
            })
        } else {
            Ok(())
        }
    }
}

/// Why a frame body was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MalformedReason {
    /// The payload is not JSON of the declared message kind.
    Json,
    /// The JSON `type` tag differs from the header kind.
    KindMismatch,
    /// Engine options failed validation.
    InvalidOptions,
    /// A key map failed validation.
    InvalidKeymap,
    /// A hotkey does not survive its text form: the peer's parser would reject it (for example
    /// a modifier-only hotkey with one modifier, a key without a modifier, or a modifier as
    /// the key) or read another hotkey.
    InvalidHotkey,
    /// An executable name fails validation when it is read back (for example one that
    /// lowercasing made longer than `ExeName::MAX_LEN`).
    InvalidExeName,
    /// `CommandResult.ok` disagrees with its code.
    InconsistentResult,
    /// A client identifier is zero or disagrees with the frame header.
    InvalidClientId,
}

/// Typed IPC failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpcError {
    /// The frame does not start with the `RMK1` magic.
    BadMagic,
    /// The frame uses an unsupported protocol version.
    UnsupportedVersion {
        /// Version found in the header.
        version: u16,
    },
    /// The header length field is not the fixed header length.
    BadHeaderLength {
        /// Length found in the header.
        header_len: u16,
    },
    /// The header names an unknown message kind.
    UnknownKind {
        /// Raw kind found in the header.
        kind: u16,
    },
    /// The flags field is not zero.
    NonZeroFlags {
        /// Flags found in the header.
        flags: u16,
    },
    /// The reserved field is not zero.
    NonZeroReserved {
        /// Value found in the header.
        reserved: u32,
    },
    /// The payload is larger than the accepted maximum.
    Oversize {
        /// Declared (decode) or attempted (encode) payload length.
        payload_len: u64,
        /// Maximum accepted by the decoder or encoder.
        max: u32,
    },
    /// The CRC-32 does not match the header and payload.
    ChecksumMismatch {
        /// Checksum carried by the frame.
        expected: u32,
        /// Checksum computed over the received bytes.
        actual: u32,
    },
    /// The payload failed JSON decoding or semantic validation.
    Malformed(MalformedReason),
    /// A collection exceeded its entry limit.
    LimitExceeded {
        /// The collection.
        limit: Limit,
        /// Entries present.
        count: u64,
    },
    /// A message arrived in a state or direction where it is not allowed.
    UnexpectedMessage(MessageKind),
    /// A frame carried another client's identifier.
    ClientIdMismatch {
        /// Identifier assigned to this connection.
        expected: u64,
        /// Identifier found in the frame.
        actual: u64,
    },
    /// The client asked for an unsupported protocol in `Hello`.
    ProtocolMismatch {
        /// Protocol requested by the client.
        protocol: u16,
    },
    /// The peer reported an error and is closing the connection.
    Remote(ErrorCode),
    /// The peer closed the stream at a frame boundary.
    Closed,
    /// The stream ended inside a frame.
    Truncated,
    /// Transport failure.
    Io(io::ErrorKind),
    /// The session has no live connection.
    NotConnected,
    /// No client identifier is left to assign.
    ClientIdsExhausted,
}

impl IpcError {
    /// True when the peer broke the protocol (bad frame, unexpected message, wrong client id,
    /// unsupported protocol). Sessions report such errors to the peer before closing.
    pub const fn is_protocol_violation(&self) -> bool {
        matches!(
            self,
            Self::BadMagic
                | Self::UnsupportedVersion { .. }
                | Self::BadHeaderLength { .. }
                | Self::UnknownKind { .. }
                | Self::NonZeroFlags { .. }
                | Self::NonZeroReserved { .. }
                | Self::Oversize { .. }
                | Self::ChecksumMismatch { .. }
                | Self::Malformed(_)
                | Self::LimitExceeded { .. }
                | Self::UnexpectedMessage(_)
                | Self::ClientIdMismatch { .. }
                | Self::ProtocolMismatch { .. }
        )
    }

    /// Code sent to the peer in an `Error` message before closing.
    pub const fn wire_code(&self) -> ErrorCode {
        match self {
            Self::UnsupportedVersion { .. } | Self::ProtocolMismatch { .. } => {
                ErrorCode::UNSUPPORTED_PROTOCOL
            }
            Self::BadMagic
            | Self::BadHeaderLength { .. }
            | Self::UnknownKind { .. }
            | Self::NonZeroFlags { .. }
            | Self::NonZeroReserved { .. }
            | Self::Oversize { .. }
            | Self::ChecksumMismatch { .. }
            | Self::Malformed(_)
            | Self::Truncated => ErrorCode::MALFORMED_FRAME,
            Self::LimitExceeded { .. } => ErrorCode::LIMIT_EXCEEDED,
            Self::UnexpectedMessage(_) => ErrorCode::UNEXPECTED_MESSAGE,
            Self::ClientIdMismatch { .. } => ErrorCode::CLIENT_ID_MISMATCH,
            Self::Remote(code) => *code,
            Self::Closed | Self::Io(_) | Self::NotConnected | Self::ClientIdsExhausted => {
                ErrorCode::INTERNAL
            }
        }
    }

    /// Stable diagnostic code (`7000..=7099`) for the typed diagnostics log.
    pub const fn diag_code(&self) -> DiagCode {
        DiagCode(match self {
            Self::BadMagic => 7001,
            Self::UnsupportedVersion { .. } => 7002,
            Self::BadHeaderLength { .. } => 7003,
            Self::UnknownKind { .. } => 7004,
            Self::NonZeroFlags { .. } => 7005,
            Self::NonZeroReserved { .. } => 7006,
            Self::Oversize { .. } => 7007,
            Self::ChecksumMismatch { .. } => 7008,
            Self::Malformed(_) => 7009,
            Self::LimitExceeded { .. } => 7010,
            Self::UnexpectedMessage(_) => 7011,
            Self::ClientIdMismatch { .. } => 7012,
            Self::ProtocolMismatch { .. } => 7013,
            Self::Remote(_) => 7014,
            Self::Closed => 7015,
            Self::Truncated => 7016,
            Self::Io(_) => 7017,
            Self::NotConnected => 7018,
            Self::ClientIdsExhausted => 7019,
        })
    }
}

impl fmt::Display for IpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BadMagic => f.write_str("frame magic is not RMK1"),
            Self::UnsupportedVersion { version } => {
                write!(f, "unsupported protocol version {version}")
            }
            Self::BadHeaderLength { header_len } => {
                write!(f, "unexpected frame header length {header_len}")
            }
            Self::UnknownKind { kind } => write!(f, "unknown message kind {kind}"),
            Self::NonZeroFlags { .. } => f.write_str("frame flags must be zero"),
            Self::NonZeroReserved { .. } => f.write_str("frame reserved field must be zero"),
            Self::Oversize { payload_len, max } => {
                write!(f, "payload of {payload_len} bytes exceeds {max} bytes")
            }
            Self::ChecksumMismatch { .. } => f.write_str("frame checksum mismatch"),
            Self::Malformed(reason) => write!(f, "malformed message body ({reason:?})"),
            Self::LimitExceeded { limit, count } => {
                write!(f, "{count} entries exceed the {limit:?} limit")
            }
            Self::UnexpectedMessage(kind) => write!(f, "unexpected {kind:?} message"),
            Self::ClientIdMismatch { .. } => f.write_str("frame carries another client id"),
            Self::ProtocolMismatch { protocol } => {
                write!(f, "client requested unsupported protocol {protocol}")
            }
            Self::Remote(code) => write!(f, "peer reported error code {}", code.0),
            Self::Closed => f.write_str("connection closed by peer"),
            Self::Truncated => f.write_str("connection closed inside a frame"),
            Self::Io(kind) => write!(f, "transport error ({kind:?})"),
            Self::NotConnected => f.write_str("not connected"),
            Self::ClientIdsExhausted => f.write_str("client identifiers exhausted"),
        }
    }
}

impl std::error::Error for IpcError {}

impl From<io::Error> for IpcError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}
