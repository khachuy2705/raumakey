//! Raumakey COM in-process TSF text input processor (`raumakey_tip.dll`, plan §10.3).
//!
//! - [`session`]: the platform-independent core of the text service. It owns the engine, the
//!   runtime configuration, the per-document engine states, and the `OnTestKeyDown` /
//!   `OnKeyDown` decision logic, and it reaches the document only through the
//!   [`session::DocumentPort`] edit-session abstraction. It contains no Win32 or COM types and
//!   no `unsafe` code, so its behavior is tested with fakes on any host (design §15.1).
//! - [`activation`]: the sinks a text service activation advises, in order, and how they are
//!   undone after a failure and at `Deactivate`, apart from COM so that the rules are tested
//!   with fakes on any host.
//! - [`adapter`]: the rules of the COM layer apart from COM: key decoding, modifiers, AltGr,
//!   and the keyboard layout; the edit of one edit session over a range port; what an edit
//!   record shows; document identities; password input scopes; the open/close compartment;
//!   mouse presses; the order of the core call and the replay; and the `HostContext` of a key.
//!   No Win32 or COM types and no `unsafe` code; tested with fakes on any host.
//! - `server` (Windows only): the in-process COM server. The DLL exports `DllGetClassObject`,
//!   `DllCanUnloadNow`, `DllRegisterServer`, and `DllUnregisterServer`, nothing else; the
//!   class factory creates the text service, one COM object that is the text input processor
//!   and every sink of plan §10.3. It translates the TSF callbacks into calls on
//!   [`session::ServiceCore`]: keys, edit sessions, replays, document identities, focus,
//!   changes, mouse presses, secure fields, and the mode. Every entry point runs inside a
//!   `raumakey_windows::panic` guard. The module documentation lists what is done, what is
//!   not (the IPC snapshot client, the probe, diagnostics, the `evkey-compat` path), the
//!   threading rules, and the manual check list.
//! - `mouse_hook` (Windows only): the `WH_MOUSE` hook of the TSF thread.
//!
//! The in-process tests run against the real TSF thread manager of the test process, with
//! documents of TSF's own text store and of a minimal application text store that the tests
//! implement. Nothing here has been run inside a Windows host application.

#![warn(missing_docs)]

pub mod activation;
pub mod adapter;
#[cfg(windows)]
pub mod mouse_hook;
#[cfg(windows)]
pub mod server;
pub mod session;
#[cfg(windows)]
mod text_service;

#[cfg(windows)]
pub use server::{DllCanUnloadNow, DllGetClassObject, DllRegisterServer, DllUnregisterServer};
