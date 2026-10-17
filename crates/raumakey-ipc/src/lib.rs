//! Raumakey typed control channel and gated legacy codec (plan §7, design §10).
//!
//! The control channel connects the controller (server) with TIP and tool clients over any
//! `Read + Write` transport; the named pipe itself lives in `raumakey-windows`.
//!
//! - [`frame`]: the 40-byte little-endian frame header with a CRC-32, the encoder, and the
//!   incremental [`frame::FrameDecoder`], which validates every header field and the payload
//!   length before reserving memory.
//! - [`message`]: the internally tagged JSON [`message::Message`] body with entry-count limits.
//!   No message carries keystrokes or document text.
//! - [`client`] and [`server`]: the session state machines (handshake, client id checks,
//!   snapshots, commands, ping/pong, goodbye); [`backoff`] paces reconnects.
//! - [`memory`]: an in-memory duplex pipe for tests and in-process wiring.
//! - `legacy` (feature `evkey-compat`, off by default): the EVKey shared-memory wire codec of
//!   clean-room spec §6, behind a platform port with a deterministic fake.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod backoff;
pub mod client;
pub mod error;
pub mod frame;
#[cfg(feature = "evkey-compat")]
pub mod legacy;
mod link;
pub mod memory;
pub mod message;
pub mod server;
mod wire;
