//! Peak heap use of the frame decoder, measured with a counting global allocator (design §10:
//! lengths are validated before allocation).
//!
//! The allocator only counts allocations made by a thread while it is inside
//! [`peak_allocation`], so tests running in parallel and the test harness do not disturb the
//! measurement. The library itself keeps `#![forbid(unsafe_code)]`; this separate test binary
//! needs `unsafe` only to implement `GlobalAlloc` by forwarding to `System`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use raumakey_ipc::error::IpcError;
use raumakey_ipc::frame::{FrameDecoder, HEADER_LEN, MAX_CONTROL_PAYLOAD, MAX_PAYLOAD, Sender};
use raumakey_ipc::message::{Message, MessageKind, RuntimeSnapshot};
use raumakey_model::host::ExeName;
use raumakey_model::policy::{AppRule, RuleAction};

thread_local! {
    static TRACKING: Cell<bool> = const { Cell::new(false) };
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
}

fn record_alloc(size: usize) {
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            let live = LIVE.with(|live| {
                let now = live.get().saturating_add(size);
                live.set(now);
                now
            });
            PEAK.with(|peak| peak.set(peak.get().max(live)));
        }
    });
}

fn record_free(size: usize) {
    let _ = TRACKING.try_with(|tracking| {
        if tracking.get() {
            LIVE.with(|live| live.set(live.get().saturating_sub(size)));
        }
    });
}

struct Counting;

// SAFETY: every method forwards to `System` with the caller's arguments unchanged, so `System`
// upholds the `GlobalAlloc` contract. The bookkeeping only touches const-initialized
// thread-local `Cell`s, which never allocate and never unwind.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds the contract of `alloc`.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds the contract of `alloc_zeroed`.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        record_free(layout.size());
        // SAFETY: forwarded unchanged; `ptr` came from this allocator (that is, from `System`)
        // with `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: forwarded unchanged; the caller upholds the contract of `realloc`.
        let new = unsafe { System.realloc(ptr, layout, new_size) };
        if !new.is_null() {
            // Count the new block before freeing the old one: a moving realloc holds both.
            record_alloc(new_size);
            record_free(layout.size());
        }
        new
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Runs `f` and returns its result with the largest number of bytes this thread held
/// allocated (beyond what it held before) while `f` ran.
fn peak_allocation<T>(f: impl FnOnce() -> T) -> (T, usize) {
    LIVE.with(|live| live.set(0));
    PEAK.with(|peak| peak.set(0));
    TRACKING.with(|tracking| tracking.set(true));
    let out = f();
    TRACKING.with(|tracking| tracking.set(false));
    (out, PEAK.with(Cell::get))
}

/// A frame with a valid checksum around `payload`, declaring `declared` payload bytes.
fn frame(kind: MessageKind, declared: u32, payload: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(HEADER_LEN + payload.len());
    bytes.extend_from_slice(b"RMK1");
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&kind.as_u16().to_le_bytes());
    bytes.extend_from_slice(&0_u16.to_le_bytes());
    bytes.extend_from_slice(&40_u16.to_le_bytes());
    bytes.extend_from_slice(&declared.to_le_bytes());
    bytes.extend_from_slice(&1_u64.to_le_bytes());
    bytes.extend_from_slice(&1_u64.to_le_bytes());
    let mut crc = crc32fast::Hasher::new();
    crc.update(&bytes[..32]);
    crc.update(payload);
    bytes.extend_from_slice(&crc.finalize().to_le_bytes());
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}

/// `{"type":"pong","pad":[0,0,…]}` of at least `len` bytes: valid JSON whose unknown field
/// an internally tagged enum buffers element by element.
fn padded_pong(len: usize) -> Vec<u8> {
    let mut body = br#"{"type":"pong","pad":[0"#.to_vec();
    while body.len() + 2 < len {
        body.extend_from_slice(b",0");
    }
    body.extend_from_slice(b"]}");
    body
}

fn declared_len(body: &[u8]) -> u32 {
    u32::try_from(body.len()).unwrap()
}

#[test]
fn the_counting_allocator_sees_allocations() {
    let (bytes, peak) = peak_allocation(|| vec![0_u8; 100_000]);
    assert_eq!(bytes.len(), 100_000);
    assert!(peak >= 100_000, "{peak}");
}

#[test]
fn an_oversize_header_reserves_nothing() {
    for (kind, declared) in [
        (MessageKind::MacroSnapshot, MAX_PAYLOAD + 1),
        (MessageKind::RuntimeSnapshot, u32::MAX),
        (MessageKind::Pong, MAX_CONTROL_PAYLOAD + 1),
    ] {
        let header = frame(kind, declared, b"");
        let mut decoder = FrameDecoder::new();
        let (result, peak) = peak_allocation(|| {
            decoder.feed(&header);
            // The sender keeps streaming the body it announced.
            decoder.feed(&[b' '; 4096]);
            decoder.next_frame()
        });
        assert!(
            matches!(result, Some(Err(IpcError::Oversize { .. }))),
            "{result:?}"
        );
        assert!(peak < 1024, "{kind:?}: {peak} bytes");
    }
}

#[test]
fn a_large_declared_payload_grows_only_with_the_bytes_received() {
    let header = frame(MessageKind::MacroSnapshot, MAX_PAYLOAD, b"");
    let mut decoder = FrameDecoder::new();
    let (result, peak) = peak_allocation(|| {
        decoder.feed(&header);
        decoder.feed(&[b' '; 1000]);
        decoder.next_frame()
    });
    assert_eq!(result, None);
    assert!(peak >= 1000, "{peak}");
    assert!(peak <= 4 * 1024, "{peak} bytes for 1000 received");
}

#[test]
fn a_padded_control_frame_is_refused_before_its_body_is_buffered() {
    let body = padded_pong(2 * 1024 * 1024);
    let bytes = frame(MessageKind::Pong, declared_len(&body), &body);
    let mut decoder = FrameDecoder::for_sender(Sender::Server);
    let (result, peak) = peak_allocation(|| {
        decoder.feed(&bytes);
        decoder.next_frame()
    });
    assert!(peak < 64 * 1024, "{peak} bytes");
    assert_eq!(
        result,
        Some(Err(IpcError::Oversize {
            payload_len: body.len() as u64,
            max: MAX_CONTROL_PAYLOAD
        }))
    );
}

/// Where [`padded_runtime_snapshot`] puts its padding.
#[derive(Clone, Copy, Debug)]
enum Padding {
    /// An unknown field of the snapshot.
    TopLevel,
    /// An unknown field of the first compatibility rule.
    InRule,
}

/// A runtime snapshot body of at least `len` bytes whose padding is an unknown field holding
/// `[0,0,…]`.
fn padded_runtime_snapshot(len: usize, padding: Padding) -> Vec<u8> {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.compatibility.rules.push(AppRule {
        exe: ExeName::new("notepad.exe").unwrap(),
        window_class: None,
        action: RuleAction::default(),
    });
    let body = serde_json::to_string(&Message::RuntimeSnapshot(snapshot)).unwrap();
    // The padding goes right after the first key of the object it pads.
    let anchor = match padding {
        Padding::TopLevel => r#"{"type":"runtime_snapshot""#,
        Padding::InRule => r#"{"exe":"notepad.exe""#,
    };
    let start = body.find(anchor).unwrap();
    let (head, tail) = body.split_at(start + anchor.len());
    let mut padded = head.as_bytes().to_vec();
    padded.extend_from_slice(br#","pad":[0"#);
    while padded.len() + tail.len() + 1 < len {
        padded.extend_from_slice(b",0");
    }
    padded.push(b']');
    padded.extend_from_slice(tail.as_bytes());
    padded
}

#[test]
fn a_padded_runtime_snapshot_costs_about_its_body_length() {
    // Snapshot bodies may be up to 8 MiB long; decoding one must not cost a multiple of that,
    // wherever unknown fields sit.
    for padding in [Padding::TopLevel, Padding::InRule] {
        let body = padded_runtime_snapshot(2 * 1024 * 1024, padding);
        let bytes = frame(MessageKind::RuntimeSnapshot, declared_len(&body), &body);
        let mut decoder = FrameDecoder::for_sender(Sender::Server);
        let (result, peak) = peak_allocation(|| {
            decoder.feed(&bytes);
            decoder.next_frame()
        });
        let message = result.unwrap().unwrap().message;
        let Message::RuntimeSnapshot(snapshot) = message else {
            panic!("expected a runtime snapshot, got {:?}", message.kind());
        };
        assert_eq!(snapshot.compatibility.rules.len(), 1);
        assert!(
            peak < 3 * body.len(),
            "{padding:?}: {peak} bytes for a {}-byte body",
            body.len()
        );
    }
}

#[test]
fn a_padded_control_frame_within_its_cap_costs_a_bounded_amount() {
    let body = padded_pong(MAX_CONTROL_PAYLOAD as usize - 1);
    let bytes = frame(MessageKind::Pong, declared_len(&body), &body);
    let mut decoder = FrameDecoder::for_sender(Sender::Server);
    let (result, peak) = peak_allocation(|| {
        decoder.feed(&bytes);
        decoder.next_frame()
    });
    assert_eq!(result.unwrap().unwrap().message, Message::Pong);
    // serde buffers the unknown array before it can skip it: a few dozen times the body.
    assert!(peak < 256 * 1024, "{peak} bytes");
}
