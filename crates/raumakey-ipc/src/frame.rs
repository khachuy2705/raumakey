//! Frame layout (plan §7.1), encoder, and incremental decoder.
//!
//! ```text
//! offset size field
//! 0      4    magic "RMK1"
//! 4      2    protocol_version (1)
//! 6      2    kind
//! 8      2    flags (0)
//! 10     2    header_len (40)
//! 12     4    payload_len (<= MAX_PAYLOAD; control kinds <= MAX_CONTROL_PAYLOAD)
//! 16     8    client_id
//! 24     8    request_id
//! 32     4    checksum (CRC-32 over bytes 0..32 and the payload)
//! 36     4    reserved (0)
//! 40     ..   payload (UTF-8 JSON of the message body)
//! ```
//!
//! Every integer is little-endian.

use std::borrow::Cow;
use std::collections::VecDeque;
use std::{fmt, io};

use serde::{Deserialize, Serialize};

use crate::error::{IpcError, MalformedReason};
use crate::message::{MacroSnapshot, Message, MessageKind, MessageRef, RuntimeSnapshot};

/// Frame magic.
pub const MAGIC: [u8; 4] = *b"RMK1";
/// Protocol version carried by every frame and by `Hello`.
pub const PROTOCOL_VERSION: u16 = 1;
/// Fixed header length.
pub const HEADER_LEN: usize = 40;
/// Largest payload of any frame (8 MiB); only snapshots may use it.
pub const MAX_PAYLOAD: u32 = 8 * 1024 * 1024;
/// Largest payload of every kind other than the two snapshots (4 KiB).
///
/// Control messages have a fixed shape whose largest legitimate body (a diagnostic or probe
/// report naming a 255-character executable) is about 1.2 KiB. The cap matters because serde
/// buffers unknown fields of an internally tagged body in its generic content tree, which
/// costs a few dozen times the body length.
pub const MAX_CONTROL_PAYLOAD: u32 = 4 * 1024;

const HEADER_LEN_FIELD: u16 = 40;
const CHECKSUMMED_HEADER: usize = 32;
const OFF_VERSION: usize = 4;
const OFF_KIND: usize = 6;
const OFF_FLAGS: usize = 8;
const OFF_HEADER_LEN: usize = 10;
const OFF_PAYLOAD_LEN: usize = 12;
const OFF_CLIENT_ID: usize = 16;
const OFF_REQUEST_ID: usize = 24;
const OFF_CHECKSUM: usize = 32;
const OFF_RESERVED: usize = 36;

/// Validated fixed header of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FrameHeader {
    /// Message kind.
    pub kind: MessageKind,
    /// Payload length in bytes.
    pub payload_len: u32,
    /// Client identifier (zero in `Hello`).
    pub client_id: u64,
    /// Request identifier; responses echo the request's identifier.
    pub request_id: u64,
    /// CRC-32 over header bytes `0..32` and the payload.
    pub checksum: u32,
}

impl FrameHeader {
    /// Parses and validates magic, version, header length, kind, flags, reserved bits, and
    /// `payload_len <= kind.max_payload()` ([`MAX_PAYLOAD`] for snapshots,
    /// [`MAX_CONTROL_PAYLOAD`] otherwise). The checksum is verified once the payload has
    /// arrived.
    pub fn parse(bytes: &[u8; HEADER_LEN]) -> Result<Self, IpcError> {
        if field::<4>(bytes, 0) != MAGIC {
            return Err(IpcError::BadMagic);
        }
        let version = u16::from_le_bytes(field(bytes, OFF_VERSION));
        if version != PROTOCOL_VERSION {
            return Err(IpcError::UnsupportedVersion { version });
        }
        let header_len = u16::from_le_bytes(field(bytes, OFF_HEADER_LEN));
        if header_len != HEADER_LEN_FIELD {
            return Err(IpcError::BadHeaderLength { header_len });
        }
        let raw_kind = u16::from_le_bytes(field(bytes, OFF_KIND));
        let kind =
            MessageKind::from_u16(raw_kind).ok_or(IpcError::UnknownKind { kind: raw_kind })?;
        let flags = u16::from_le_bytes(field(bytes, OFF_FLAGS));
        if flags != 0 {
            return Err(IpcError::NonZeroFlags { flags });
        }
        let reserved = u32::from_le_bytes(field(bytes, OFF_RESERVED));
        if reserved != 0 {
            return Err(IpcError::NonZeroReserved { reserved });
        }
        let payload_len = u32::from_le_bytes(field(bytes, OFF_PAYLOAD_LEN));
        let max = kind.max_payload();
        if payload_len > max {
            return Err(IpcError::Oversize {
                payload_len: u64::from(payload_len),
                max,
            });
        }
        Ok(Self {
            kind,
            payload_len,
            client_id: u64::from_le_bytes(field(bytes, OFF_CLIENT_ID)),
            request_id: u64::from_le_bytes(field(bytes, OFF_REQUEST_ID)),
            checksum: u32::from_le_bytes(field(bytes, OFF_CHECKSUM)),
        })
    }

    /// Serializes the header, including the stored checksum.
    pub fn to_bytes(&self) -> [u8; HEADER_LEN] {
        let mut bytes = self.checksummed_prefix();
        put(&mut bytes, OFF_CHECKSUM, &self.checksum.to_le_bytes());
        bytes
    }

    /// Header bytes with the checksum and reserved fields zero.
    fn checksummed_prefix(&self) -> [u8; HEADER_LEN] {
        let mut bytes = [0_u8; HEADER_LEN];
        put(&mut bytes, 0, &MAGIC);
        put(&mut bytes, OFF_VERSION, &PROTOCOL_VERSION.to_le_bytes());
        put(&mut bytes, OFF_KIND, &self.kind.as_u16().to_le_bytes());
        put(&mut bytes, OFF_HEADER_LEN, &HEADER_LEN_FIELD.to_le_bytes());
        put(&mut bytes, OFF_PAYLOAD_LEN, &self.payload_len.to_le_bytes());
        put(&mut bytes, OFF_CLIENT_ID, &self.client_id.to_le_bytes());
        put(&mut bytes, OFF_REQUEST_ID, &self.request_id.to_le_bytes());
        bytes
    }
}

/// A decoded frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    /// Client identifier (zero in `Hello`).
    pub client_id: u64,
    /// Request identifier.
    pub request_id: u64,
    /// Body.
    pub message: Message,
}

impl Frame {
    /// Bundles a message with its frame identifiers.
    pub const fn new(client_id: u64, request_id: u64, message: Message) -> Self {
        Self {
            client_id,
            request_id,
            message,
        }
    }
}

/// Validates and encodes `frame`.
pub fn encode_frame(frame: &Frame) -> Result<Vec<u8>, IpcError> {
    let payload = EncodedPayload::encode(&frame.message)?;
    Ok(payload.frame(frame.client_id, frame.request_id))
}

/// A validated, serialized message body that can be framed for many clients without being
/// serialized again. `Debug` shows only the kind and length because bodies may hold macro text.
#[derive(Clone, PartialEq, Eq)]
pub struct EncodedPayload {
    kind: MessageKind,
    bytes: Vec<u8>,
    len: u32,
}

impl EncodedPayload {
    /// Validates and serializes `message`.
    pub fn encode(message: &Message) -> Result<Self, IpcError> {
        message.validate()?;
        Self::serialize(message.kind(), message)
    }

    /// Validates and serializes a runtime snapshot without cloning it.
    pub fn runtime_snapshot(snapshot: &RuntimeSnapshot) -> Result<Self, IpcError> {
        snapshot.validate()?;
        Self::serialize(
            MessageKind::RuntimeSnapshot,
            &MessageRef::RuntimeSnapshot(snapshot),
        )
    }

    /// Validates and serializes a macro snapshot without cloning it.
    pub fn macro_snapshot(snapshot: &MacroSnapshot) -> Result<Self, IpcError> {
        snapshot.validate()?;
        Self::serialize(
            MessageKind::MacroSnapshot,
            &MessageRef::MacroSnapshot(snapshot),
        )
    }

    /// Serializes `body` within the payload limit of `kind`, so that everything the encoder
    /// produces passes the decoder's header checks.
    fn serialize<T: Serialize + ?Sized>(kind: MessageKind, body: &T) -> Result<Self, IpcError> {
        let max = kind.max_payload();
        let mut writer = LimitedWriter::new(max);
        if serde_json::to_writer(&mut writer, body).is_err() {
            return Err(match writer.attempted {
                Some(payload_len) => IpcError::Oversize { payload_len, max },
                None => IpcError::Malformed(MalformedReason::Json),
            });
        }
        let len = u32::try_from(writer.bytes.len()).map_err(|_| IpcError::Oversize {
            payload_len: u64::try_from(writer.bytes.len()).unwrap_or(u64::MAX),
            max,
        })?;
        Ok(Self {
            kind,
            bytes: writer.bytes,
            len,
        })
    }

    /// Message kind.
    pub const fn kind(&self) -> MessageKind {
        self.kind
    }

    /// Payload length in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// True for an empty payload (never produced by the encoder).
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Frames the payload for one client.
    pub fn frame(&self, client_id: u64, request_id: u64) -> Vec<u8> {
        let mut header = FrameHeader {
            kind: self.kind,
            payload_len: self.len,
            client_id,
            request_id,
            checksum: 0,
        };
        let prefix = header.checksummed_prefix();
        header.checksum = checksum(&field::<CHECKSUMMED_HEADER>(&prefix, 0), &self.bytes);
        let mut out = Vec::with_capacity(HEADER_LEN.saturating_add(self.bytes.len()));
        out.extend_from_slice(&header.to_bytes());
        out.extend_from_slice(&self.bytes);
        out
    }
}

impl fmt::Debug for EncodedPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncodedPayload")
            .field("kind", &self.kind)
            .field("len", &self.len)
            .finish()
    }
}

/// Who produced the frames a decoder reads. Every kind keeps its own payload limit
/// ([`MessageKind::max_payload`]) whatever the sender.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sender {
    /// Any kind.
    Any,
    /// A client: only client kinds, all of them control kinds (used by the server).
    Client,
    /// The server: only server kinds (used by clients).
    Server,
}

impl Sender {
    const fn allows(self, kind: MessageKind) -> bool {
        match self {
            Self::Any => true,
            Self::Client => kind.sent_by_client(),
            Self::Server => kind.sent_by_server(),
        }
    }

    /// The largest payload limit among the kinds this sender may send.
    const fn max_payload(self) -> u32 {
        match self {
            Self::Client => MAX_CONTROL_PAYLOAD,
            Self::Any | Self::Server => MAX_PAYLOAD,
        }
    }
}

struct InProgress {
    header: FrameHeader,
    crc: crc32fast::Hasher,
}

/// Incremental frame decoder.
///
/// Bytes may arrive in chunks of any size. The header is validated as soon as its 40 bytes are
/// present, before any payload memory is reserved; the payload buffer then grows only with
/// the bytes actually received. The first error is terminal: frames completed before it are
/// still delivered, then [`FrameDecoder::next_frame`] keeps returning the error.
pub struct FrameDecoder {
    sender: Sender,
    header: [u8; HEADER_LEN],
    filled: usize,
    current: Option<InProgress>,
    payload: Vec<u8>,
    ready: VecDeque<Frame>,
    failure: Option<IpcError>,
}

impl FrameDecoder {
    /// A decoder that accepts every message kind.
    pub fn new() -> Self {
        Self::for_sender(Sender::Any)
    }

    /// A decoder for frames produced by `sender`.
    pub fn for_sender(sender: Sender) -> Self {
        Self {
            sender,
            header: [0; HEADER_LEN],
            filled: 0,
            current: None,
            payload: Vec::new(),
            ready: VecDeque::new(),
            failure: None,
        }
    }

    /// Largest payload this decoder accepts for any kind its sender may send.
    pub const fn max_payload(&self) -> u32 {
        self.sender.max_payload()
    }

    /// Feeds the next bytes of the stream. Input after a failure is ignored.
    pub fn feed(&mut self, mut input: &[u8]) {
        while !input.is_empty() && self.failure.is_none() {
            input = if self.current.is_some() {
                self.feed_payload(input)
            } else {
                self.feed_header(input)
            };
        }
    }

    /// The next decoded frame, then the terminal error (repeatedly), or `None` when more bytes
    /// are needed.
    pub fn next_frame(&mut self) -> Option<Result<Frame, IpcError>> {
        match self.ready.pop_front() {
            Some(frame) => Some(Ok(frame)),
            None => self.failure.map(Err),
        }
    }

    /// True when no partial frame is buffered (a stream may end cleanly here).
    pub fn is_idle(&self) -> bool {
        self.filled == 0 && self.current.is_none()
    }

    /// The terminal error, if any.
    pub const fn failure(&self) -> Option<IpcError> {
        self.failure
    }

    /// Bytes currently reserved for the payload of the frame being received.
    pub fn reserved_payload_bytes(&self) -> usize {
        self.payload.capacity()
    }

    fn feed_header<'a>(&mut self, input: &'a [u8]) -> &'a [u8] {
        let take = HEADER_LEN.saturating_sub(self.filled).min(input.len());
        let (chunk, rest) = input.split_at(take);
        put(&mut self.header, self.filled, chunk);
        self.filled = self.filled.saturating_add(take);
        let seen = self.filled.min(MAGIC.len());
        if self.header.get(..seen) != MAGIC.get(..seen) {
            self.fail(IpcError::BadMagic);
            return rest;
        }
        if self.filled == HEADER_LEN {
            self.filled = 0;
            match self.accept_header() {
                Ok(in_progress) => {
                    let empty = in_progress.header.payload_len == 0;
                    self.current = Some(in_progress);
                    if empty {
                        self.complete();
                    }
                }
                Err(error) => self.fail(error),
            }
        }
        rest
    }

    /// Validates a complete header (including the payload limit of its kind) and the sender
    /// filter before any payload memory is reserved.
    fn accept_header(&self) -> Result<InProgress, IpcError> {
        let header = FrameHeader::parse(&self.header)?;
        if !self.sender.allows(header.kind) {
            return Err(IpcError::UnexpectedMessage(header.kind));
        }
        let mut crc = crc32fast::Hasher::new();
        crc.update(&field::<CHECKSUMMED_HEADER>(&self.header, 0));
        Ok(InProgress { header, crc })
    }

    fn feed_payload<'a>(&mut self, input: &'a [u8]) -> &'a [u8] {
        let Some(in_progress) = self.current.as_mut() else {
            return input;
        };
        let total = to_usize(in_progress.header.payload_len);
        let remaining = total.saturating_sub(self.payload.len());
        let take = remaining.min(input.len());
        let (chunk, rest) = input.split_at(take);
        let spare = self.payload.capacity().saturating_sub(self.payload.len());
        if spare < take {
            // Grow geometrically with the bytes received, never past the declared length.
            self.payload
                .reserve_exact(take.max(self.payload.len()).min(remaining));
        }
        self.payload.extend_from_slice(chunk);
        in_progress.crc.update(chunk);
        if self.payload.len() >= total {
            self.complete();
        }
        rest
    }

    fn complete(&mut self) {
        let Some(in_progress) = self.current.take() else {
            return;
        };
        let payload = core::mem::take(&mut self.payload);
        let header = in_progress.header;
        let actual = in_progress.crc.finalize();
        if actual != header.checksum {
            self.fail(IpcError::ChecksumMismatch {
                expected: header.checksum,
                actual,
            });
            return;
        }
        match decode_message(header.kind, &payload) {
            Ok(message) => self.ready.push_back(Frame {
                client_id: header.client_id,
                request_id: header.request_id,
                message,
            }),
            Err(error) => self.fail(error),
        }
    }

    fn fail(&mut self, error: IpcError) {
        self.current = None;
        self.payload = Vec::new();
        self.failure = Some(error);
    }
}

impl Default for FrameDecoder {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for FrameDecoder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FrameDecoder")
            .field("sender", &self.sender)
            .field("idle", &self.is_idle())
            .field("ready", &self.ready.len())
            .field("failure", &self.failure)
            .finish()
    }
}

#[derive(Deserialize)]
struct TagOnly<'a> {
    #[serde(rename = "type", borrow)]
    tag: Cow<'a, str>,
}

/// Decodes a body. Snapshot kinds are decoded straight into their structs (the header already
/// names the kind) instead of through the internally tagged [`Message`], which serde would
/// buffer whole in its generic content tree; the `type` tag is still required to match.
/// Nothing inside a snapshot is buffered either: unknown keys are skipped, and the
/// compatibility rules, whose model type flattens its overrides (which serde buffers the same
/// way), are read through an explicit wire form (`crate::wire`).
fn decode_message(kind: MessageKind, payload: &[u8]) -> Result<Message, IpcError> {
    let message = match kind {
        MessageKind::RuntimeSnapshot => {
            expect_tag(kind, payload)?;
            Message::RuntimeSnapshot(from_json(payload)?)
        }
        MessageKind::MacroSnapshot => {
            expect_tag(kind, payload)?;
            Message::MacroSnapshot(from_json(payload)?)
        }
        _ => from_json::<Message>(payload)?,
    };
    if message.kind() != kind {
        return Err(IpcError::Malformed(MalformedReason::KindMismatch));
    }
    message.validate()?;
    Ok(message)
}

fn expect_tag(kind: MessageKind, payload: &[u8]) -> Result<(), IpcError> {
    let tag: TagOnly<'_> = from_json(payload)?;
    if tag.tag == kind.tag() {
        Ok(())
    } else {
        Err(IpcError::Malformed(MalformedReason::KindMismatch))
    }
}

fn from_json<'a, T: Deserialize<'a>>(payload: &'a [u8]) -> Result<T, IpcError> {
    serde_json::from_slice(payload).map_err(|_| IpcError::Malformed(MalformedReason::Json))
}

fn checksum(prefix: &[u8], payload: &[u8]) -> u32 {
    let mut crc = crc32fast::Hasher::new();
    crc.update(prefix);
    crc.update(payload);
    crc.finalize()
}

/// Copies `N` bytes at `at`; out-of-range reads yield zeros (offsets are constants).
fn field<const N: usize>(bytes: &[u8], at: usize) -> [u8; N] {
    let mut out = [0_u8; N];
    if let Some(source) = at.checked_add(N).and_then(|end| bytes.get(at..end)) {
        out.copy_from_slice(source);
    }
    out
}

/// Writes `data` at `at` when it fits.
fn put(bytes: &mut [u8], at: usize, data: &[u8]) {
    if let Some(target) = at
        .checked_add(data.len())
        .and_then(|end| bytes.get_mut(at..end))
    {
        target.copy_from_slice(data);
    }
}

fn to_usize(value: u32) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// A `Vec` writer that refuses to grow past a limit, so oversize bodies fail while serializing.
struct LimitedWriter {
    bytes: Vec<u8>,
    limit: usize,
    attempted: Option<u64>,
}

impl LimitedWriter {
    fn new(limit: u32) -> Self {
        Self {
            bytes: Vec::new(),
            limit: to_usize(limit),
            attempted: None,
        }
    }
}

impl io::Write for LimitedWriter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let total = self.bytes.len().saturating_add(data.len());
        if total > self.limit {
            self.attempted = Some(u64::try_from(total).unwrap_or(u64::MAX));
            return Err(io::Error::other("payload limit exceeded"));
        }
        self.bytes.extend_from_slice(data);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
