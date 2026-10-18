//! Raumakey narrow safe wrappers around Win32 and COM (plan §10.1).
//!
//! Every Win32 or COM call of the controller and the TIP goes through this crate. Each wrapper
//! owns its handles (RAII), checks every result, and reports failures as
//! [`error::WinError`] values that carry an HRESULT and a static operation name only.
//!
//! Modules that hold pure logic (planning, naming, decoding, encoding, validation) compile on
//! every platform: [`error`], [`panic`](mod@panic), [`strings`] (including growing-buffer reads),
//! [`names`], [`security`] (SDDL), [`registry`] (write guard, value encoding), [`clipboard`]
//! (snapshot logic), [`input`] (synthetic input builders), [`hooks`] (record decoders),
//! [`foreground`], [`keyboard`], [`process`] (command lines, regsvr32 selection), [`shell`]
//! (tooltips, notification-icon callbacks), [`tsf_registration`] (planner, rollback,
//! verification), [`window`] (private, registered, broadcastable, and closing message numbers,
//! class names), [`menu`] (menu items and their flags), [`icon`] (icon sizes), [`dialog`]
//! (message box styles, texts, and answers), [`dialog_template`] (in-memory dialog templates:
//! layout, styles, validation), [`dialog_box`] (the private messages dialogs reserve, the
//! notification codes of their controls), [`word_wrap`] (wrapping at spaces only, the width of
//! a message box line), and [`com_server`] (COM object and server-lock counts). Their Win32
//! parts and the purely Win32 modules (`com`, `file_replace`,
//! `known_folder`, `module`, `pipe`, `sid`, `single_instance`, `task_scheduler`, and the
//! `evkey-compat` module `legacy`) exist only on Windows.
//!
//! The controller's building blocks are [`window`] (window classes, the hidden top-level
//! window that owns the notification icon, message-only windows, the message loop, posting
//! from other threads, letting the shell's messages through UIPI when elevated, registered
//! messages of the user and their broadcast, and passing on the right to take the
//! foreground), [`shell`] (the notification icon and its callbacks, the double-click among
//! them), [`input`] (the clock and the user's double-click time), [`foreground`] (the
//! application in front and the watcher that reports each change of it), [`menu`], [`icon`],
//! [`dialog`], [`dialog_template`] and [`dialog_box`] (the settings panel and the macro list:
//! modal dialog boxes from in-memory templates, run on a thread of their own and driven by a
//! handler),
//! [`word_wrap`] (Vietnamese texts of labels, check boxes, and message boxes wrapped at spaces
//! with the real fonts, since Windows' own word wrap cuts Vietnamese words), `single_instance`,
//! and `module` (the module that links this crate and its path). The TIP's
//! are [`com_server`] (class factory, `DllGetClassObject`, `DllCanUnloadNow`), `module` (the
//! DLL's own path for `DllRegisterServer`), and [`input`] (`GetMessageExtraInfo`, to bypass
//! Raumakey's own injected keys).
//!
//! Nothing here logs. Values that may hold user content (paths, registry strings, clipboard
//! bytes, the texts of dialog controls) are not shown by `Debug` or `Display`.

#![warn(missing_docs)]

pub mod clipboard;
pub mod com_server;
pub mod dialog;
pub mod dialog_box;
pub mod dialog_template;
pub mod error;
pub mod foreground;
pub mod hooks;
pub mod icon;
pub mod input;
pub mod keyboard;
pub mod menu;
pub mod names;
pub mod panic;
pub mod process;
pub mod registry;
pub mod security;
pub mod shell;
pub mod strings;
pub mod tsf_registration;
pub mod window;
pub mod word_wrap;

#[cfg(windows)]
pub mod com;
#[cfg(windows)]
pub mod file_replace;
#[cfg(windows)]
mod handle;
#[cfg(windows)]
pub mod known_folder;
#[cfg(all(windows, feature = "evkey-compat"))]
pub mod legacy;
#[cfg(windows)]
pub mod module;
#[cfg(windows)]
pub mod pipe;
#[cfg(windows)]
pub mod sid;
#[cfg(windows)]
pub mod single_instance;
#[cfg(windows)]
pub mod task_scheduler;
