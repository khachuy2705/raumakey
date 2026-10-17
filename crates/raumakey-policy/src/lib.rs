//! Raumakey host capability, transport selection, and hotkey classification (plan §6).
//!
//! - [`CompatibilityPolicy`] resolves the [`HostPolicy`](raumakey_model::policy::HostPolicy) of
//!   a host from an immutable [`CompatibilityConfig`](raumakey_model::policy::CompatibilityConfig)
//!   snapshot and keeps TSF capability probe results in a bounded [`ProbeCache`].
//! - [`hotkey::HotkeyClassifier`] recognizes the configured toggle and hard-reset hotkeys before
//!   key events reach the engine, and tells adapters which key events to withhold
//!   ([`KeyVerdict`]).
//!
//! The crate is platform-independent, never logs, and does not panic on configuration or input
//! data.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod compat;
mod error;
pub mod hotkey;
pub mod probe;

pub use compat::CompatibilityPolicy;
pub use error::PolicyError;
pub use hotkey::{HotkeyClassifier, HotkeyMatch, KeyVerdict};
pub use probe::{MAX_PROBE_RECORDS, ProbeCache};
