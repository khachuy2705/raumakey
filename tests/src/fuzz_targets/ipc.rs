//! `ipc_frames`: byte streams against the control-channel frame decoder (plan §7.1, design
//! §10: lengths are validated before allocation).

use raumakey_ipc::error::IpcError;
use raumakey_ipc::frame::{
    Frame, FrameDecoder, FrameHeader, HEADER_LEN, MAX_CONTROL_PAYLOAD, MAX_PAYLOAD, Sender,
    encode_frame,
};
use raumakey_ipc::message::MessageKind;

use super::reader::Reader;

/// Largest input in bytes.
const MAX_BYTES: usize = 64 * 1024;
/// Frames built in assemble form.
const MAX_ASSEMBLED_FRAMES: usize = 8;
/// Largest payload taken from the input for one assembled frame.
const MAX_ASSEMBLED_PAYLOAD: usize = 9_000;
/// Chunk sizes the stream is fed in, cycled from an offset chosen by the input.
const CHUNK_SIZES: [usize; 9] = [1, 7, 39, 40, 41, 3, 1_000, 2, 4_096];

/// What one decoder made of a stream.
#[derive(PartialEq)]
struct Outcome {
    frames: Vec<Frame>,
    failure: Option<IpcError>,
    idle: bool,
}

/// One frame position in the stream, read from the raw header bytes without validation.
struct Span {
    payload_start: usize,
    payload_end: usize,
    declared: usize,
    kind: Option<MessageKind>,
}

/// The frame positions a valid stream would have: each header's raw `payload_len` decides
/// where the next frame starts. Returns the spans and whether the stream ends on a boundary.
fn layout(stream: &[u8]) -> (Vec<Span>, bool) {
    let mut spans = Vec::new();
    let mut position = 0_usize;
    while let Some(header) = position
        .checked_add(HEADER_LEN)
        .and_then(|end| stream.get(position..end))
    {
        let declared = usize::try_from(u32::from_le_bytes([
            header[12], header[13], header[14], header[15],
        ]))
        .unwrap_or(usize::MAX);
        let kind = MessageKind::from_u16(u16::from_le_bytes([header[6], header[7]]));
        let payload_start = position + HEADER_LEN;
        let payload_end = payload_start.saturating_add(declared);
        spans.push(Span {
            payload_start,
            payload_end,
            declared,
            kind,
        });
        position = payload_end;
    }
    (spans, position == stream.len())
}

/// The decoder never reserves more payload memory than the frame in progress declares, than
/// its kind allows, or than its sender allows, and holds none outside a payload or after a
/// failure.
fn check_reservation(decoder: &FrameDecoder, spans: &[Span], fed: usize) {
    let reserved = decoder.reserved_payload_bytes();
    let sender_limit = usize::try_from(decoder.max_payload()).unwrap_or(usize::MAX);
    assert!(
        reserved <= sender_limit,
        "reserved past the sender's payload limit"
    );
    if decoder.failure().is_some() {
        assert!(reserved == 0, "a failed decoder still holds payload memory");
        return;
    }
    match spans
        .iter()
        .find(|span| span.payload_start <= fed && fed < span.payload_end)
    {
        Some(span) => {
            let kind_limit = span.kind.map_or(0, |kind| {
                usize::try_from(kind.max_payload()).unwrap_or(usize::MAX)
            });
            assert!(
                reserved <= span.declared && reserved <= kind_limit,
                "reserved {reserved} bytes for a payload declared as {} bytes",
                span.declared
            );
        }
        None => assert!(reserved == 0, "payload memory held between frames"),
    }
}

/// Feeds `stream` in chunks of `sizes` (cycled) and drains the decoder after every chunk.
fn decode(stream: &[u8], sender: Sender, sizes: &[usize], spans: &[Span]) -> Outcome {
    let mut decoder = FrameDecoder::for_sender(sender);
    let mut frames = Vec::new();
    let mut failure = None;
    let mut fed = 0;
    let mut rest = stream;
    for size in sizes.iter().cycle() {
        if rest.is_empty() {
            break;
        }
        let (chunk, tail) = rest.split_at((*size).clamp(1, rest.len()));
        rest = tail;
        decoder.feed(chunk);
        fed += chunk.len();
        check_reservation(&decoder, spans, fed);
        while let Some(next) = decoder.next_frame() {
            match next {
                Ok(frame) => {
                    assert!(failure.is_none(), "a frame was delivered after the failure");
                    frames.push(frame);
                }
                Err(error) => {
                    assert!(
                        failure.is_none_or(|known| known == error),
                        "the terminal error changed"
                    );
                    failure = Some(error);
                    break;
                }
            }
        }
    }
    if let Some(error) = failure {
        // The failure is terminal and repeats.
        assert!(
            decoder.next_frame() == Some(Err(error)),
            "the failure did not repeat"
        );
        assert!(decoder.failure() == Some(error), "failure() lost the error");
    }
    Outcome {
        frames,
        failure,
        idle: decoder.is_idle(),
    }
}

/// A decoded frame is valid for its sender, re-encodes, and decodes back to itself.
fn check_frame(frame: &Frame, sender: Sender) {
    let kind = frame.message.kind();
    let allowed = match sender {
        Sender::Any => true,
        Sender::Client => kind.sent_by_client(),
        Sender::Server => kind.sent_by_server(),
    };
    assert!(allowed, "{kind:?} was accepted from the wrong sender");
    assert!(
        frame.message.validate().is_ok(),
        "{kind:?}: a decoded message does not validate"
    );
    let bytes = match encode_frame(frame) {
        Ok(bytes) => bytes,
        Err(error) => panic!("{kind:?}: a decoded message does not re-encode: {error:?}"),
    };
    let header: [u8; HEADER_LEN] = match bytes.get(..HEADER_LEN).map(<[u8; HEADER_LEN]>::try_from) {
        Some(Ok(header)) => header,
        _ => panic!("{kind:?}: an encoded frame is shorter than its header"),
    };
    match FrameHeader::parse(&header) {
        Ok(parsed) => assert!(
            parsed.kind == kind
                && usize::try_from(parsed.payload_len).ok() == Some(bytes.len() - HEADER_LEN)
                && parsed.client_id == frame.client_id
                && parsed.request_id == frame.request_id,
            "{kind:?}: the re-encoded header does not describe the frame"
        ),
        Err(error) => panic!("{kind:?}: a re-encoded header is invalid: {error:?}"),
    }
    let mut decoder = FrameDecoder::new();
    decoder.feed(&bytes);
    assert!(
        decoder.next_frame() == Some(Ok(frame.clone())),
        "{kind:?}: the re-encoded frame decodes to another message"
    );
    assert!(
        decoder.next_frame().is_none() && decoder.is_idle(),
        "{kind:?}: the re-encoded frame left bytes behind"
    );
}

/// Builds frames with valid checksums from the assemble form (so that a byte-level fuzzer
/// reaches the body decoder): per frame a raw kind (u16), a client-id choice, a request id, a
/// corruption code, a length mode, a payload length (u16), and the payload bytes.
fn assemble(reader: &mut Reader<'_>) -> Vec<u8> {
    let mut stream = Vec::new();
    for _ in 0..MAX_ASSEMBLED_FRAMES {
        if reader.is_empty() {
            break;
        }
        let kind = reader.u16();
        let client_id = reader.pick(&[0, 1, 7, u64::MAX]);
        let request_id = u64::from(reader.byte());
        let corruption = reader.byte() % 16;
        let length_mode = reader.byte() % 8;
        let len = usize::from(reader.u16()).min(MAX_ASSEMBLED_PAYLOAD);
        let payload = reader.take(len);
        let true_len = u32::try_from(payload.len()).unwrap_or(u32::MAX);
        let declared = match length_mode {
            6 => true_len.saturating_add(1),
            7 => reader.pick(&[MAX_CONTROL_PAYLOAD + 1, MAX_PAYLOAD + 1, u32::MAX]),
            _ => true_len,
        };
        let mut header = [0_u8; HEADER_LEN];
        header[..4].copy_from_slice(b"RMK1");
        header[4..6].copy_from_slice(&1_u16.to_le_bytes());
        header[6..8].copy_from_slice(&kind.to_le_bytes());
        header[10..12].copy_from_slice(&40_u16.to_le_bytes());
        header[12..16].copy_from_slice(&declared.to_le_bytes());
        header[16..24].copy_from_slice(&client_id.to_le_bytes());
        header[24..32].copy_from_slice(&request_id.to_le_bytes());
        let mut crc = crc32fast::Hasher::new();
        crc.update(&header[..32]);
        crc.update(payload);
        let mut checksum = crc.finalize();
        match corruption {
            12 => checksum ^= 1,
            13 => header[4] = 2,
            14 => header[8] = 1,
            15 => header[36] = 1,
            _ => {}
        }
        header[32..36].copy_from_slice(&checksum.to_le_bytes());
        stream.extend_from_slice(&header);
        stream.extend_from_slice(payload);
    }
    stream
}

pub(crate) fn run(data: &[u8]) {
    let data = &data[..data.len().min(MAX_BYTES)];
    let (stream, offset) = match data.split_first() {
        Some((&control, rest)) if control & 0x80 != 0 => (
            assemble(&mut Reader::new(rest)),
            usize::from(control & 0x3F),
        ),
        _ => (data.to_vec(), data.len()),
    };
    let sizes: Vec<usize> = (0..CHUNK_SIZES.len())
        .map(|index| CHUNK_SIZES[(offset + index) % CHUNK_SIZES.len()])
        .collect();
    let (spans, ends_on_boundary) = layout(&stream);
    for sender in [Sender::Any, Sender::Client, Sender::Server] {
        let whole = decode(&stream, sender, &[usize::MAX], &spans);
        let chunked = decode(&stream, sender, &sizes, &spans);
        assert!(
            whole == chunked,
            "chunking changed what the decoder produced"
        );
        if chunked.failure.is_none() {
            assert!(
                chunked.idle == ends_on_boundary,
                "the decoder's idle state disagrees with the frame boundaries"
            );
        }
        for frame in &chunked.frames {
            check_frame(frame, sender);
        }
    }
}
