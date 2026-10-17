//! Raumakey shared domain types and invariants.
//!
//! This crate defines the contracts shared by the engine, the Windows adapters, the controller,
//! and the IPC protocol. It contains no platform code, no raw pointers, and no mutable global
//! state.

#![forbid(unsafe_code)]

pub mod diag;
pub mod error;
pub mod host;
pub mod hotkey;
pub mod identity;
pub mod input;
pub mod keymap;
pub mod macros;
pub mod options;
pub mod policy;
pub mod transaction;
pub mod utf16;
