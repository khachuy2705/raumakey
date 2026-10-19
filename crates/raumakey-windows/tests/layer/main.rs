//! Integration tests of `raumakey-windows`.
//!
//! Pure modules are tested on every host. Win32 modules are tested only on Windows, and only
//! with operations that are safe on a user's desktop: temporary files, uniquely named pipes and
//! kernel objects, registry keys under `HKCU\Software\Raumakey\Tests-<pid>-<n>`, process-local
//! windows that are never shown (created on a fresh thread and destroyed before it ends; nothing
//! is broadcast), `WH_CBT` hooks of such a thread or of a test dialog's thread only (they see
//! only that thread's windows), out-of-context WinEvent hooks of the foreground window, which
//! only observe (installed and removed on a test thread without waiting for an event),
//! dialog boxes without a taskbar button (`WS_EX_NOACTIVATE`) that are moved far off-screen
//! before Windows shows them (some under an activation context of Common Controls 6 made from a
//! temporary manifest, active on the test's own threads only), whose controls may paint
//! themselves into memory bitmaps (`WM_PRINTCLIENT`), message boxes that hooks of their own
//! thread (`WH_CALLWNDPROCRET`, `WH_CBT`) read once they are laid out, move off-screen without
//! a taskbar button, never let activate, and end before Windows first shows them or keep
//! off-screen until the test dismisses them, menus that are never shown,
//! in-process COM objects that are never registered, system DLLs
//! mapped as data, a uniquely named temporary copy of a data-only DLL (no code runs),
//! impersonation on a fresh test thread (which reverts before it ends) of tokens that exist only
//! in this process (a low-integrity copy of the process token, an AppContainer "lowbox" token
//! made with `NtCreateLowBoxToken` for a made-up package SID, with no AppContainer profile, or
//! the anonymous logon), a child `cmd.exe` without a console window that waits on its standard
//! input until the test ends it, and read-only queries (the top-level windows of the desktop,
//! the running processes, version resources). Open dialogs are asked for only after a dismiss
//! or under a `WH_CBT` hook of the dialog's thread that refuses every window, so none is
//! shown. Anything that sends input, hooks the keyboard or
//! mouse, touches the real clipboard, writes `HKEY_LOCAL_MACHINE`, registers COM/TSF, schedules
//! tasks, changes which processes may take the foreground, or shows a menu, a message box, or a
//! notification icon is `#[ignore]`d and run manually.
//!
//! The quiet panic hook is tested in its own binary (`tests/panic_hook.rs`) because installing
//! it would silence the failure messages of this suite.

#[cfg(windows)]
mod common_controls_v6;
mod support;

// Pure modules (every platform); Windows-only cases inside are gated individually.
mod clipboard;
mod com_server;
mod dialog;
mod dialog_box;
mod dialog_template;
mod error;
mod file_dialog;
mod foreground;
mod hooks;
mod icon;
mod input;
mod keyboard;
mod menu;
mod names;
mod process;
mod programs;
mod registry_model;
mod security;
mod shell;
mod strings;
mod tsf_registration;
mod window;
mod word_wrap;

// Win32 modules.
#[cfg(windows)]
mod com;
#[cfg(windows)]
mod dialog_box_choices;
#[cfg(windows)]
mod dialog_box_editor;
#[cfg(windows)]
mod dialog_box_picker;
#[cfg(windows)]
mod file_replace;
#[cfg(windows)]
mod known_folder;
#[cfg(all(windows, feature = "evkey-compat"))]
mod legacy;
#[cfg(windows)]
mod module;
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
