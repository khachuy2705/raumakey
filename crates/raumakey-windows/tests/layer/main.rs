//! Integration tests of `raumakey-windows`.
//!
//! Pure modules are tested on every host. Win32 modules are tested only on Windows, and only
//! with operations that are safe on a user's desktop: temporary files, uniquely named pipes and
//! kernel objects, registry keys under `HKCU\Software\Raumakey\Tests-<pid>-<n>`, and read-only
//! queries. Anything that sends input, hooks the keyboard or mouse, touches the real clipboard,
//! writes `HKEY_LOCAL_MACHINE`, registers COM/TSF, or schedules tasks is `#[ignore]`d and run
//! manually.
//!
//! The quiet panic hook is tested in its own binary (`tests/panic_hook.rs`) because installing
//! it would silence the failure messages of this suite.

mod support;

// Pure modules (every platform); Windows-only cases inside are gated individually.
mod clipboard;
mod error;
mod foreground;
mod hooks;
mod input;
mod keyboard;
mod names;
mod process;
mod registry_model;
mod security;
mod shell;
mod strings;
mod tsf_registration;

// Win32 modules.
#[cfg(windows)]
mod com;
#[cfg(windows)]
mod file_replace;
#[cfg(windows)]
mod known_folder;
#[cfg(all(windows, feature = "evkey-compat"))]
mod legacy;
#[cfg(windows)]
mod pipe;
#[cfg(windows)]
mod registry;
#[cfg(windows)]
mod sid;
#[cfg(windows)]
mod single_instance;
#[cfg(windows)]
mod task_scheduler;
