//! Integration tests of the platform-independent TSF service core (`raumakey_tip::session`).
//!
//! Everything runs against fakes on any host (design §15.1): a fake document stands in for
//! the TSF edit session and plays the application, and the drivers call the core in the
//! order TSF and the future COM layer will. Passing these tests says nothing about runtime
//! behavior inside a real Windows host application.

mod support;

mod cache;
mod config;
mod context;
mod failures;
mod gates;
mod hotkeys;
mod macros;
mod privacy;
mod properties;
mod typing;
