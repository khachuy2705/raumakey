//! The rules of the TSF adapter, apart from COM (plan §10.3; design §15.1).
//!
//! The COM layer (`text_service`, Windows only) turns TSF callbacks into calls on
//! [`ServiceCore`](crate::session::ServiceCore). Whatever it decides on its own lives here,
//! in plain Rust without Win32 or COM types, so that it is tested with fakes on any host; the
//! COM layer only reads and writes TSF objects for these rules:
//!
//! - [`keys`]: decoding `wParam`/`lParam` of a key event, the modifier state, the text of a
//!   key, the synthetic left Ctrl that Windows sends with AltGr, and the keyboard layout.
//! - [`edit`]: the edit of one synchronous read-write edit session, over a port of the range
//!   operations (`ITfRange` in the COM layer).
//! - [`record`]: what the edit record of `ITfTextEditSink::OnEndEdit` shows, as the core's
//!   [`SelectionChange`](crate::session::SelectionChange).
//! - [`documents`]: the document identity of each TSF context, its input scope, and its
//!   window class.
//! - [`scope`]: the input scopes of password fields (matrix #29).
//! - [`mode`]: the keyboard open/close compartment kept in step with the Vietnamese mode.
//! - [`mouse`]: the mouse messages that are button presses (matrix #19).
//! - [`keypath`]: a key-down calls the core first and replays afterwards.
//! - [`host`]: session ids and the `HostContext` of a key.
//!
//! Passing their tests says nothing about runtime behavior inside a Windows host
//! application.

#![forbid(unsafe_code)]

pub mod documents;
pub mod edit;
pub mod host;
pub mod keypath;
pub mod keys;
pub mod mode;
pub mod mouse;
pub mod record;
pub mod scope;

/// A call of the host that failed (an `HRESULT` failure in the COM layer). Its code is
/// dropped: every failure means the same to these rules.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HostFailure;
