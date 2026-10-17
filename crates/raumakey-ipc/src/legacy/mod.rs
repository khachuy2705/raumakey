//! EVKey legacy shared-memory wire codec (clean-room spec §6, plan §7.4).
//!
//! Compiled only with the `evkey-compat` Cargo feature, which is off by default and in release
//! builds. It exists for interoperability with legacy components; Raumakey's own processes use
//! the typed channel. The EVKey kernel object names appear only in this module.
//!
//! The shared block is `0x1038` bytes. Every field is little-endian; 4-byte fields are kept as
//! raw `u32` values and range-checked, never reinterpreted as signed integers.
//!
//! | Offset | Type | Use |
//! |---:|---|---|
//! | `0x00` | u32 | opcode (1 process char, 2 backspace, 3 reset, 5 connect/sync) |
//! | `0x04` | u32 | one UTF-16 code unit for op 1 |
//! | `0x08` | u32 | Shift flag |
//! | `0x0c` | u32 | Caps flag |
//! | `0x10` | u32 | request sequence (incremented under the mutex, wraps) |
//! | `0x14` | u32 | response sequence (must equal the request sequence) |
//! | `0x18` | u32 | UTF-16 units to delete before the caret |
//! | `0x1c` | u32 | output length in bytes (even, `<= 0x1000`) |
//! | `0x20` | u32 | unknown flag, preserved |
//! | `0x24` | u32 | reset gate |
//! | `0x28` | u32 | unknown flag, preserved |
//! | `0x2c` | u32 | unknown flag, preserved |
//! | `0x30` | u64 | controller tick of the response (valid for 2000 ms) |
//! | `0x38` | `[u8; 0x1000]` | UTF-16LE replacement |

use core::fmt;
use std::time::Duration;

use raumakey_model::diag::DiagCode;
use raumakey_model::transaction::EditTransaction;
use raumakey_model::utf16;

mod channel;
mod client;
pub mod fake;
mod server;

pub use channel::{
    LegacyChannel, LegacyEvent, LegacyObject, LockResult, MutexOrigin, WaitResult,
    activate_controller,
};
pub use client::{LegacyClient, MAX_CONSECUTIVE_FAILURES, ResetOutcome};
pub use server::{LegacyServer, LegacyServerConfig, ServeOutcome, ShutdownOutcome};

/// The transaction for the outcome of a legacy request: a decoded edit is applied once
/// ([`LegacyEdit::to_transaction`]); any failure passes the original key through and resets the
/// context, so no shared output is ever applied after a timeout, mismatch, or invalid response.
pub fn transaction_for(result: &Result<LegacyEdit, LegacyError>) -> EditTransaction {
    match result {
        Ok(edit) => edit.to_transaction(),
        Err(_) => EditTransaction::pass_through_reset(),
    }
}

/// Size of the shared mapping.
pub const BLOCK_SIZE: usize = 0x1038;
/// Opcode field.
pub const OFF_OPCODE: usize = 0x00;
/// UTF-16 code unit of op 1.
pub const OFF_CHAR: usize = 0x04;
/// Shift flag.
pub const OFF_SHIFT: usize = 0x08;
/// Caps flag.
pub const OFF_CAPS: usize = 0x0C;
/// Request sequence.
pub const OFF_REQUEST_SEQ: usize = 0x10;
/// Response sequence.
pub const OFF_RESPONSE_SEQ: usize = 0x14;
/// Units to delete.
pub const OFF_DELETE_COUNT: usize = 0x18;
/// Output length in bytes.
pub const OFF_OUTPUT_LEN: usize = 0x1C;
/// Unknown flag at `0x20`; preserved, never interpreted.
pub const OFF_FLAG_20: usize = 0x20;
/// Reset gate.
pub const OFF_RESET_GATE: usize = 0x24;
/// Unknown flag at `0x28`; preserved, never interpreted.
pub const OFF_FLAG_28: usize = 0x28;
/// Unknown flag at `0x2c`; preserved, never interpreted.
pub const OFF_FLAG_2C: usize = 0x2C;
/// Controller tick (u64).
pub const OFF_EPOCH: usize = 0x30;
/// UTF-16LE output.
pub const OFF_OUTPUT: usize = 0x38;
/// Output capacity in bytes.
pub const OUTPUT_CAPACITY: usize = 0x1000;
/// Output capacity in UTF-16 units.
pub const MAX_OUTPUT_UNITS: usize = OUTPUT_CAPACITY / 2;
/// Largest delete count accepted (a Raumakey bound: one full output buffer of units).
pub const MAX_DELETE_UNITS: u32 = 0x800;

/// Name of the shared mapping.
pub const SHARED_MEMORY_NAME: &str = "EVKeyIME_SharedMem_v1";
/// Name of the auto-reset request event.
pub const REQUEST_EVENT_NAME: &str = "EVKeyIME_Request_v1";
/// Name of the auto-reset response event.
pub const RESPONSE_EVENT_NAME: &str = "EVKeyIME_Response_v1";
/// Name of the mutex that serializes the single request slot.
pub const MUTEX_NAME: &str = "EVKeyIME_Mutex_v1";

/// Longest wait for the mutex.
pub const MUTEX_WAIT: Duration = Duration::from_millis(100);
/// Longest wait for the response.
pub const RESPONSE_WAIT: Duration = Duration::from_millis(RESPONSE_WAIT_MS);
/// A response tick older than this is stale.
pub const EPOCH_VALIDITY_MS: u64 = 2_000;
/// Longest wait for the controller worker during shutdown.
pub const SHUTDOWN_WAIT: Duration = Duration::from_millis(2_000);
/// `dwExtraInfo` of keys replayed by a legacy TIP; such events must be bypassed.
pub const LEGACY_INJECTED_SENTINEL: u32 = 0xEBCA_1234;

const RESPONSE_WAIT_MS: u64 = 50;

/// True when an input event's extra info is the legacy replay sentinel.
pub fn is_legacy_injected(extra_info: u64) -> bool {
    extra_info == u64::from(LEGACY_INJECTED_SENTINEL)
}

/// Legacy request opcodes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegacyOpcode {
    /// Op 1: process one UTF-16 code unit.
    ProcessChar,
    /// Op 2: semantic backspace.
    Backspace,
    /// Op 3: reset.
    Reset,
    /// Op 5: connect/sync.
    Sync,
}

impl LegacyOpcode {
    /// Raw opcode value.
    pub const fn as_u32(self) -> u32 {
        match self {
            Self::ProcessChar => 1,
            Self::Backspace => 2,
            Self::Reset => 3,
            Self::Sync => 5,
        }
    }

    /// Parses a raw opcode value.
    pub const fn from_u32(raw: u32) -> Option<Self> {
        match raw {
            1 => Some(Self::ProcessChar),
            2 => Some(Self::Backspace),
            3 => Some(Self::Reset),
            5 => Some(Self::Sync),
            _ => None,
        }
    }
}

/// Typed legacy failure. No variant carries typed or document text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LegacyError {
    /// The bridge has not completed an op 5 sync (or was reset).
    NotConnected,
    /// A named object does not exist (or its handle was lost).
    ObjectMissing(LegacyObject),
    /// A named object already exists, so activation failed closed.
    ConflictingObject(LegacyObject),
    /// The mutex was not acquired within [`MUTEX_WAIT`]; nothing was written.
    MutexTimeout,
    /// No matching response within [`RESPONSE_WAIT`]; the outcome is ambiguous and the
    /// request is never retried.
    ResponseTimeout,
    /// The response sequence differs from the request sequence.
    SequenceMismatch {
        /// Request sequence.
        expected: u32,
        /// Response sequence found.
        actual: u32,
    },
    /// The output length is odd.
    OddOutputLength {
        /// Raw length field.
        len: u32,
    },
    /// The output length exceeds [`OUTPUT_CAPACITY`].
    OutputTooLong {
        /// Raw (or requested) length in bytes.
        len: u32,
    },
    /// The delete count exceeds the accepted bound.
    DeleteOutOfRange {
        /// Raw delete count.
        count: u32,
    },
    /// The output is not well-formed UTF-16.
    IllFormedOutput,
    /// The response tick is older than [`EPOCH_VALIDITY_MS`].
    StaleEpoch {
        /// Age of the tick in milliseconds.
        age_ms: u64,
    },
    /// The response tick lies in the future.
    EpochInFuture,
    /// The character cannot travel as one UTF-16 code unit (a lone surrogate).
    UnsupportedChar,
    /// A shared-memory access fell outside the block (adapter bug).
    OutOfBounds,
    /// The platform adapter failed with an OS error code.
    Channel {
        /// OS error code.
        code: u32,
    },
}

impl LegacyError {
    /// True when the bridge must be re-synchronized (op 5) before the next request:
    /// malformed or stale responses and lost handles.
    pub const fn resets_bridge(&self) -> bool {
        matches!(
            self,
            Self::OddOutputLength { .. }
                | Self::OutputTooLong { .. }
                | Self::DeleteOutOfRange { .. }
                | Self::IllFormedOutput
                | Self::StaleEpoch { .. }
                | Self::EpochInFuture
                | Self::ObjectMissing(_)
                | Self::OutOfBounds
                | Self::Channel { .. }
        )
    }

    /// Stable diagnostic code (`7100..=7199`).
    pub const fn diag_code(&self) -> DiagCode {
        DiagCode(match self {
            Self::NotConnected => 7101,
            Self::ObjectMissing(_) => 7102,
            Self::ConflictingObject(_) => 7103,
            Self::MutexTimeout => 7104,
            Self::ResponseTimeout => 7105,
            Self::SequenceMismatch { .. } => 7106,
            Self::OddOutputLength { .. } => 7107,
            Self::OutputTooLong { .. } => 7108,
            Self::DeleteOutOfRange { .. } => 7109,
            Self::IllFormedOutput => 7110,
            Self::StaleEpoch { .. } => 7111,
            Self::EpochInFuture => 7112,
            Self::UnsupportedChar => 7113,
            Self::OutOfBounds => 7114,
            Self::Channel { .. } => 7115,
        })
    }
}

impl fmt::Display for LegacyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotConnected => f.write_str("legacy bridge not connected"),
            Self::ObjectMissing(object) => write!(f, "legacy object {object:?} is missing"),
            Self::ConflictingObject(object) => {
                write!(f, "legacy object {object:?} already exists")
            }
            Self::MutexTimeout => f.write_str("legacy mutex wait timed out"),
            Self::ResponseTimeout => f.write_str("legacy response wait timed out"),
            Self::SequenceMismatch { expected, actual } => {
                write!(f, "legacy response sequence {actual:#x} != {expected:#x}")
            }
            Self::OddOutputLength { len } => write!(f, "legacy output length {len:#x} is odd"),
            Self::OutputTooLong { len } => write!(f, "legacy output length {len:#x} too long"),
            Self::DeleteOutOfRange { count } => {
                write!(f, "legacy delete count {count:#x} out of range")
            }
            Self::IllFormedOutput => f.write_str("legacy output is not well-formed UTF-16"),
            Self::StaleEpoch { age_ms } => write!(f, "legacy response is {age_ms} ms old"),
            Self::EpochInFuture => f.write_str("legacy response tick is in the future"),
            Self::UnsupportedChar => f.write_str("character is not one UTF-16 code unit"),
            Self::OutOfBounds => f.write_str("legacy shared memory access out of bounds"),
            Self::Channel { code } => write!(f, "legacy channel error {code}"),
        }
    }
}

impl std::error::Error for LegacyError {}

/// A decoded response: units to delete before the caret, then units to insert.
///
/// `Debug` shows only the counts, never the inserted text.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct LegacyEdit {
    /// UTF-16 units to delete before the caret.
    pub delete_before: u32,
    /// UTF-16 units to insert.
    pub insert_utf16: Vec<u16>,
}

impl LegacyEdit {
    /// No deletion and no insertion.
    pub const fn empty() -> Self {
        Self {
            delete_before: 0,
            insert_utf16: Vec::new(),
        }
    }

    /// True when the edit changes nothing.
    pub fn is_empty(&self) -> bool {
        self.delete_before == 0 && self.insert_utf16.is_empty()
    }

    /// Checks the insert length and form, then `delete_before <= min(max_delete,
    /// MAX_DELETE_UNITS)`.
    pub fn validate(&self, max_delete: u32) -> Result<(), LegacyError> {
        if self.insert_utf16.len() > MAX_OUTPUT_UNITS {
            return Err(LegacyError::OutputTooLong {
                len: byte_len(self.insert_utf16.len()),
            });
        }
        if !utf16::is_well_formed(&self.insert_utf16) {
            return Err(LegacyError::IllFormedOutput);
        }
        if self.delete_before > max_delete.min(MAX_DELETE_UNITS) {
            return Err(LegacyError::DeleteOutOfRange {
                count: self.delete_before,
            });
        }
        Ok(())
    }

    /// The edit as a transaction: an empty edit passes the original key through; any other
    /// edit consumes it and replaces `delete_before` units with the output.
    pub fn to_transaction(&self) -> EditTransaction {
        if self.is_empty() {
            EditTransaction::pass_through()
        } else {
            EditTransaction {
                delete_before: self.delete_before,
                insert_utf16: self.insert_utf16.clone(),
                consume_original: true,
                replay_key: None,
                reset_context: false,
            }
        }
    }
}

impl fmt::Debug for LegacyEdit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyEdit")
            .field("delete_before", &self.delete_before)
            .field("insert_units", &self.insert_utf16.len())
            .finish()
    }
}

/// A decoded request, as served by a controller.
///
/// `Debug` never shows the typed code unit.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum LegacyRequest {
    /// Op 1.
    ProcessChar {
        /// The typed UTF-16 code unit (never a surrogate).
        unit: u16,
        /// Shift held.
        shift: bool,
        /// Caps Lock on.
        caps: bool,
    },
    /// Op 2.
    Backspace,
    /// Op 3.
    Reset,
    /// Op 5.
    Sync,
}

impl LegacyRequest {
    /// The request's opcode.
    pub const fn opcode(&self) -> LegacyOpcode {
        match self {
            Self::ProcessChar { .. } => LegacyOpcode::ProcessChar,
            Self::Backspace => LegacyOpcode::Backspace,
            Self::Reset => LegacyOpcode::Reset,
            Self::Sync => LegacyOpcode::Sync,
        }
    }
}

impl fmt::Debug for LegacyRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProcessChar { shift, caps, .. } => f
                .debug_struct("ProcessChar")
                .field("unit", &format_args!("<redacted>"))
                .field("shift", shift)
                .field("caps", caps)
                .finish(),
            Self::Backspace => f.write_str("Backspace"),
            Self::Reset => f.write_str("Reset"),
            Self::Sync => f.write_str("Sync"),
        }
    }
}

/// Why a controller refused a request (no response is written).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RejectReason {
    /// Opcode other than 1, 2, 3, 5.
    UnknownOpcode(u32),
    /// The code unit of op 1 is above `0xFFFF` or a surrogate.
    InvalidChar,
    /// A Shift or Caps flag of op 1 is neither 0 nor 1.
    InvalidFlag,
    /// The handler's edit exceeds the output buffer, is ill-formed, or deletes too much.
    InvalidReply,
}

/// Raw copy of the shared block with typed accessors at the documented offsets.
///
/// Setters change only this copy; adapters decide which fields reach shared memory. `Debug`
/// omits the typed code unit and the output text.
#[derive(Clone, PartialEq, Eq)]
pub struct LegacyBlock {
    bytes: Box<[u8; BLOCK_SIZE]>,
}

impl LegacyBlock {
    /// An all-zero block (a freshly created mapping).
    pub fn zeroed() -> Self {
        Self {
            bytes: Box::new([0; BLOCK_SIZE]),
        }
    }

    /// Copies exactly [`BLOCK_SIZE`] bytes.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let array: [u8; BLOCK_SIZE] = bytes.try_into().ok()?;
        Some(Self {
            bytes: Box::new(array),
        })
    }

    /// The raw bytes.
    pub fn as_bytes(&self) -> &[u8; BLOCK_SIZE] {
        &self.bytes
    }

    /// The raw bytes, for adapters that copy shared memory into the block.
    pub fn as_bytes_mut(&mut self) -> &mut [u8; BLOCK_SIZE] {
        &mut self.bytes
    }

    /// Opcode (`0x00`).
    pub fn opcode(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_OPCODE)
    }

    /// Sets the opcode.
    pub fn set_opcode(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_OPCODE, &value.to_le_bytes());
    }

    /// Raw code unit field (`0x04`).
    pub fn char_unit(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_CHAR)
    }

    /// Sets the code unit field.
    pub fn set_char_unit(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_CHAR, &value.to_le_bytes());
    }

    /// Raw Shift flag (`0x08`).
    pub fn shift(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_SHIFT)
    }

    /// Sets the Shift flag.
    pub fn set_shift(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_SHIFT, &value.to_le_bytes());
    }

    /// Raw Caps flag (`0x0c`).
    pub fn caps(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_CAPS)
    }

    /// Sets the Caps flag.
    pub fn set_caps(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_CAPS, &value.to_le_bytes());
    }

    /// Request sequence (`0x10`).
    pub fn request_sequence(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_REQUEST_SEQ)
    }

    /// Sets the request sequence.
    pub fn set_request_sequence(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_REQUEST_SEQ, &value.to_le_bytes());
    }

    /// Response sequence (`0x14`).
    pub fn response_sequence(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_RESPONSE_SEQ)
    }

    /// Sets the response sequence.
    pub fn set_response_sequence(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_RESPONSE_SEQ, &value.to_le_bytes());
    }

    /// Raw delete count (`0x18`).
    pub fn delete_count(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_DELETE_COUNT)
    }

    /// Sets the delete count.
    pub fn set_delete_count(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_DELETE_COUNT, &value.to_le_bytes());
    }

    /// Raw output length in bytes (`0x1c`).
    pub fn output_len(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_OUTPUT_LEN)
    }

    /// Sets the raw output length without touching the output bytes.
    pub fn set_output_len(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_OUTPUT_LEN, &value.to_le_bytes());
    }

    /// The unknown flags at `0x20`, `0x28`, and `0x2c`, as stored.
    pub fn unknown_flags(&self) -> [u32; 3] {
        [
            le_u32(&self.bytes[..], OFF_FLAG_20),
            le_u32(&self.bytes[..], OFF_FLAG_28),
            le_u32(&self.bytes[..], OFF_FLAG_2C),
        ]
    }

    /// Sets the unknown flags (tests and fakes; the codec never writes them).
    pub fn set_unknown_flags(&mut self, flags: [u32; 3]) {
        put(&mut self.bytes[..], OFF_FLAG_20, &flags[0].to_le_bytes());
        put(&mut self.bytes[..], OFF_FLAG_28, &flags[1].to_le_bytes());
        put(&mut self.bytes[..], OFF_FLAG_2C, &flags[2].to_le_bytes());
    }

    /// Raw reset gate (`0x24`).
    pub fn reset_gate(&self) -> u32 {
        le_u32(&self.bytes[..], OFF_RESET_GATE)
    }

    /// Sets the reset gate.
    pub fn set_reset_gate(&mut self, value: u32) {
        put(&mut self.bytes[..], OFF_RESET_GATE, &value.to_le_bytes());
    }

    /// Controller tick (`0x30`).
    pub fn epoch(&self) -> u64 {
        le_u64(&self.bytes[..], OFF_EPOCH)
    }

    /// Sets the controller tick.
    pub fn set_epoch(&mut self, value: u64) {
        put(&mut self.bytes[..], OFF_EPOCH, &value.to_le_bytes());
    }

    /// The whole output buffer (`0x38..0x1038`).
    pub fn output_bytes(&self) -> &[u8] {
        self.bytes.get(OFF_OUTPUT..).unwrap_or_default()
    }

    /// Writes `units` as UTF-16LE output, zeroes the rest of the buffer, and sets the length.
    pub fn set_output(&mut self, units: &[u16]) -> Result<(), LegacyError> {
        if units.len() > MAX_OUTPUT_UNITS {
            return Err(LegacyError::OutputTooLong {
                len: byte_len(units.len()),
            });
        }
        if let Some(buffer) = self.bytes.get_mut(OFF_OUTPUT..) {
            buffer.fill(0);
            for (pair, unit) in buffer.as_chunks_mut::<2>().0.iter_mut().zip(units) {
                *pair = unit.to_le_bytes();
            }
        }
        self.set_output_len(byte_len(units.len()));
        Ok(())
    }

    /// Decodes the output after checking that its length is even and within the buffer.
    pub fn output_units(&self) -> Result<Vec<u16>, LegacyError> {
        let len = self.output_len();
        if !len.is_multiple_of(2) {
            return Err(LegacyError::OddOutputLength { len });
        }
        let bytes = usize::try_from(len)
            .ok()
            .filter(|bytes| *bytes <= OUTPUT_CAPACITY)
            .and_then(|bytes| self.output_bytes().get(..bytes))
            .ok_or(LegacyError::OutputTooLong { len })?;
        Ok(bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect())
    }
}

impl Default for LegacyBlock {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl fmt::Debug for LegacyBlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LegacyBlock")
            .field("opcode", &self.opcode())
            .field("request_sequence", &self.request_sequence())
            .field("response_sequence", &self.response_sequence())
            .field("delete_count", &self.delete_count())
            .field("output_len", &self.output_len())
            .field("reset_gate", &self.reset_gate())
            .field("epoch", &self.epoch())
            .finish_non_exhaustive()
    }
}

/// The request sequence that follows `current`: raw `u32` increment with wrap-around. Zero is
/// skipped so that a response field that was never written can never match.
pub const fn next_sequence(current: u32) -> u32 {
    match current.wrapping_add(1) {
        0 => 1,
        next => next,
    }
}

/// Validates a copy of the block as the response to request `sequence`: the response
/// sequence must match, the tick must be at most [`EPOCH_VALIDITY_MS`] old and not in the
/// future, the delete count must be `<= min(max_delete, MAX_DELETE_UNITS)`, and the output
/// length must be even, `<= 0x1000`, and hold well-formed UTF-16.
pub fn validate_response(
    block: &LegacyBlock,
    sequence: u32,
    now_ms: u64,
    max_delete: u32,
) -> Result<LegacyEdit, LegacyError> {
    let actual = block.response_sequence();
    if actual != sequence {
        return Err(LegacyError::SequenceMismatch {
            expected: sequence,
            actual,
        });
    }
    match now_ms.checked_sub(block.epoch()) {
        None => return Err(LegacyError::EpochInFuture),
        Some(age_ms) if age_ms > EPOCH_VALIDITY_MS => {
            return Err(LegacyError::StaleEpoch { age_ms });
        }
        Some(_) => {}
    }
    let count = block.delete_count();
    if count > max_delete.min(MAX_DELETE_UNITS) {
        return Err(LegacyError::DeleteOutOfRange { count });
    }
    let insert_utf16 = block.output_units()?;
    if !utf16::is_well_formed(&insert_utf16) {
        return Err(LegacyError::IllFormedOutput);
    }
    Ok(LegacyEdit {
        delete_before: count,
        insert_utf16,
    })
}

/// Decodes the request fields of a block copy. Returns the raw request sequence (echoed in
/// the response) and the request. Fields other than the opcode are checked only for op 1.
pub fn decode_request(block: &LegacyBlock) -> Result<(u32, LegacyRequest), RejectReason> {
    let raw = block.opcode();
    let opcode = LegacyOpcode::from_u32(raw).ok_or(RejectReason::UnknownOpcode(raw))?;
    let request = match opcode {
        LegacyOpcode::ProcessChar => {
            let unit = u16::try_from(block.char_unit())
                .ok()
                .filter(|unit| !utf16::is_high_surrogate(*unit) && !utf16::is_low_surrogate(*unit))
                .ok_or(RejectReason::InvalidChar)?;
            LegacyRequest::ProcessChar {
                unit,
                shift: flag(block.shift()).ok_or(RejectReason::InvalidFlag)?,
                caps: flag(block.caps()).ok_or(RejectReason::InvalidFlag)?,
            }
        }
        LegacyOpcode::Backspace => LegacyRequest::Backspace,
        LegacyOpcode::Reset => LegacyRequest::Reset,
        LegacyOpcode::Sync => LegacyRequest::Sync,
    };
    Ok((block.request_sequence(), request))
}

/// A 0/1 flag; any other raw value is out of range.
const fn flag(raw: u32) -> Option<bool> {
    match raw {
        0 => Some(false),
        1 => Some(true),
        _ => None,
    }
}

/// Byte length of `units` UTF-16 units, saturating.
fn byte_len(units: usize) -> u32 {
    u32::try_from(units.saturating_mul(2)).unwrap_or(u32::MAX)
}

fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    let mut raw = [0_u8; 4];
    if let Some(source) = offset.checked_add(4).and_then(|end| bytes.get(offset..end)) {
        raw.copy_from_slice(source);
    }
    u32::from_le_bytes(raw)
}

fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    let mut raw = [0_u8; 8];
    if let Some(source) = offset.checked_add(8).and_then(|end| bytes.get(offset..end)) {
        raw.copy_from_slice(source);
    }
    u64::from_le_bytes(raw)
}

fn put(bytes: &mut [u8], offset: usize, data: &[u8]) {
    if let Some(target) = offset
        .checked_add(data.len())
        .and_then(|end| bytes.get_mut(offset..end))
    {
        target.copy_from_slice(data);
    }
}
