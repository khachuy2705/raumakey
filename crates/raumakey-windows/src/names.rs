//! User- and session-qualified names of kernel objects and pipes (pure).
//!
//! Every Raumakey named kernel object lives in the session-local namespace and carries a hash of
//! the user's SID, so two users of one session (or a user and a sandbox running as another
//! user) never share an object: `Local\Raumakey.<sidhash>.<name>`. The control pipe, which is
//! machine-global by nature, additionally carries the session id:
//! `\\.\pipe\Raumakey.<session>.<sidhash>.control`.
//!
//! The SID hash is FNV-1a-64 of the canonical SID string (as returned by
//! `ConvertSidToStringSidW`), written as 16 lowercase hex digits. It only partitions names; it is
//! not a secret. Security comes from the descriptors of `security` (user-only for every kernel
//! object; the control pipe also admits AppContainer clients of the user, plan D27) and from
//! peer verification (`pipe`).
//!
//! An AppContainer process's `Local\` is its own namespace, so it cannot open these kernel
//! objects by name. That is why the IPC path uses the pipe only: `\\.\pipe\` names are the same
//! for every process.

use core::fmt;

use raumakey_model::identity::NAMED_OBJECT_PREFIX;

/// Longest accepted name component.
pub const MAX_COMPONENT_LEN: usize = 64;

/// Component of the controller's single-instance mutex.
pub const CONTROLLER_INSTANCE: &str = "controller";

/// FNV-1a 64-bit offset basis.
const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64-bit prime.
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Why a name could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NameError {
    /// The SID hash is not 16 lowercase hexadecimal digits.
    InvalidSidHash,
    /// A component is empty, longer than [`MAX_COMPONENT_LEN`], or contains a character outside
    /// ASCII letters, digits, `_`, and `-`.
    InvalidComponent,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidSidHash => "SID hash is not 16 lowercase hex digits",
            Self::InvalidComponent => "object name component is not [A-Za-z0-9_-]{1,64}",
        })
    }
}

impl std::error::Error for NameError {}

/// A validated SID hash: exactly 16 lowercase hexadecimal digits.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SidHash(String);

impl SidHash {
    /// Accepts exactly 16 lowercase hexadecimal digits.
    pub fn parse(text: &str) -> Result<Self, NameError> {
        let valid = text.len() == 16
            && text
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(NameError::InvalidSidHash)
        }
    }

    /// The hash digits.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SidHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// FNV-1a 64-bit hash of `bytes`.
pub const fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET_BASIS;
    let mut index = 0;
    while index < bytes.len() {
        hash ^= bytes[index] as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        index += 1;
    }
    hash
}

/// Hash of a SID string: FNV-1a-64 of its UTF-8 bytes as 16 lowercase hex digits.
pub fn sid_hash16(sid: &str) -> SidHash {
    SidHash(format!("{:016x}", fnv1a_64(sid.as_bytes())))
}

/// True for `[A-Za-z0-9_-]{1,64}`.
pub fn is_valid_component(component: &str) -> bool {
    !component.is_empty()
        && component.len() <= MAX_COMPONENT_LEN
        && component
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

/// `Local\Raumakey.<sidhash>.<name>`.
pub fn object_name(sid_hash: &SidHash, name: &str) -> Result<String, NameError> {
    if !is_valid_component(name) {
        return Err(NameError::InvalidComponent);
    }
    Ok(format!("{NAMED_OBJECT_PREFIX}{}.{name}", sid_hash.as_str()))
}

/// `\\.\pipe\Raumakey.<session>.<sidhash>.control`.
pub fn pipe_name(session_id: u32, sid_hash: &SidHash) -> String {
    format!(
        "\\\\.\\pipe\\Raumakey.{session_id}.{}.control",
        sid_hash.as_str()
    )
}
