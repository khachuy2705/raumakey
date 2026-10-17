//! The COM layer of `raumakey_tip.dll` (plan §10.3, §10.4; design §12), tested in-process.
//!
//! The tests call the four exports through the library, create the text service through its
//! class factory, and run activation against the real TSF thread manager of this process, on
//! fresh single-threaded apartment threads (`CoInitializeEx`/`CoUninitialize` through
//! `raumakey_windows::com`). `typing` and `focus` drive the key path, the edit sessions,
//! the focus, and the mode against documents of TSF's own text store (see `tsf`); `secure`
//! and `hosts` use a minimal text store of an application (see `store`) that declares input
//! scopes and, when a test asks, refuses or defers a lock, leaves the text of an edit
//! selected, or acts while TSF waits for a lock.
//! Nothing is registered: `DllRegisterServer` and `DllUnregisterServer`, which write
//! `HKEY_LOCAL_MACHINE`, run only in an `#[ignore]`d manual test. `artifact` loads the DLL
//! that Cargo built and calls its real exports. No test sends input to the desktop.
//!
//! Passing these tests says nothing about runtime behavior inside a real Windows host
//! application: TSF never activates the text service here (see `activation`), and the
//! documents belong to no application.

#![cfg(windows)]

mod shim;
mod store;
mod support;
mod tsf;

mod activation;
mod artifact;
mod exports;
mod focus;
mod hosts;
mod registration;
mod secure;
mod service;
mod typing;
