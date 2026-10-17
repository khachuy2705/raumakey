//! Raumakey COM in-process TSF text input processor (`raumakey_tip.dll`, plan §10.3).
//!
//! - [`session`]: the platform-independent core of the text service. It owns the engine, the
//!   runtime configuration, the per-document engine states, and the `OnTestKeyDown` /
//!   `OnKeyDown` decision logic, and it reaches the document only through the
//!   [`session::DocumentPort`] edit-session abstraction. It contains no Win32 or COM types and
//!   no `unsafe` code, so its behavior is tested with fakes on any host (design §15.1).
//!
//! The COM layer (class factory, `ITfTextInputProcessorEx`, the sinks of plan §10.3, the four
//! DLL exports, registration, and the IPC client thread) is not written yet. It will live in
//! `cfg(windows)` modules next to [`session`], wrap every entry point in a panic guard from
//! `raumakey_windows::panic`, and translate TSF callbacks into calls on
//! [`session::ServiceCore`]. Until then this library exports no COM entry points, and nothing
//! here has been run inside a Windows host application.

#![warn(missing_docs)]

pub mod session;
