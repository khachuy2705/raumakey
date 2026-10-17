//! The typing front end of the portable controller (`raumakey_controller::typing`, plan
//! D39, design §21.2): the keyboard-hook handler over a fake Windows.
//!
//! Every test runs on any host against fakes (design §15.1): no hook is installed and no
//! input reaches the desktop. The simulator in `support` delivers the keys that the handler
//! sends after the event it handled, as `SendInput` does, and lets each window's application
//! type the keys that pass with the real key state. Passing these tests says nothing about
//! runtime behavior on a Windows desktop; the manual check list of the `typing` module does.

mod support;

mod failures;
mod gates;
mod health;
mod hotkeys;
mod keys;
mod privacy;
mod telex;
mod updates;
mod words;
