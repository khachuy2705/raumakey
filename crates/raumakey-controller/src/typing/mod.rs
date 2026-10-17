//! Typing through a global keyboard hook (plan D39, design §21.2).
//!
//! The portable `raumakey.exe` types without a text service: a low-level keyboard hook sees
//! every key of the session, the session core decides what each key does, and what replaces
//! a key is sent with `SendInput`.
//!
//! ```text
//! physical key
//!   -> WH_KEYBOARD_LL on the hook thread (raumakey_windows::hooks::HookThread)
//!   -> TypingHandler: KeyTracker (modifiers, held keys, Caps Lock), the foreground window
//!      (the document), the key's text with the foreground thread's layout
//!   -> KeyStroke -> ServiceCore (role KeyboardHook) -> VietnameseEngine -> EditTransaction
//!   -> BlindDocument: Backspace x characters to delete, then the text as KEYEVENTF_UNICODE
//!      keys, all tagged RAUMAKEY_INJECTED_TAG
//!   -> SendInput from inside the callback, then the hook blocks the key they replace
//! ```
//!
//! The pure part (everything but `hook`) has no Win32 type and is tested with fakes on every
//! host: [`KeyTracker`], the [`TypingPlatform`] port, [`BlindDocument`], [`Mailbox`], and
//! [`TypingHandler`]. On Windows, `TypingHook` (module `hook`) runs the handler on a hook
//! thread over the real platform (`raumakey-windows` wrappers). Passing the tests says
//! nothing about runtime behavior on a Windows desktop; the
//! [manual check list](#manual-check-list) does.
//!
//! # What the handler does with each event
//!
//! | Event | What happens |
//! |---|---|
//! | A key Raumakey injected (`dwExtraInfo` = `RAUMAKEY_INJECTED_TAG`, matrix #39) | passes untouched; nothing is read, tracked, or applied |
//! | Any other key or mouse event | first applies the updates posted to the [`Mailbox`] |
//! | Any other key | the [`KeyTracker`] records it; the synthetic left Ctrl of AltGr passes untouched and goes no further |
//! | The foreground runs with more privileges than `raumakey.exe`, or its privileges cannot be read | passes untouched (UIPI drops what a lower process sends, so a blocked key would be lost); on entering such a window the words end |
//! | The foreground or focus window changed since the last key | the words of both documents end ([`ServiceCore::set_focus`](raumakey_session::ServiceCore::set_focus), matrix #20) |
//! | A key-down | [`ServiceCore::key_down`](raumakey_session::ServiceCore::key_down) (a hook has no test-key-down step) over a [`BlindDocument`]; then the plan below |
//! | A key-up | [`ServiceCore::key_up`](raumakey_session::ServiceCore::key_up); blocked only when a hotkey consumed its key |
//! | A mouse button press | [`ServiceCore::mouse_down`](raumakey_session::ServiceCore::mouse_down): the word ends (matrix #19) unless the host's policy turns `reset_on_mouse` off |
//!
//! The core runs in the [keyboard-hook role](raumakey_session::Role::KeyboardHook): it types
//! every host whatever transport its policy selects, and it classifies the hotkeys itself.
//! Its crate documentation has the rules of `key_down`: secure fields, Ctrl, Alt, and Win
//! chords and navigation keys that end the word, macros, and Backspace that restores the
//! state before (matrix #17).
//!
//! ## The plan of a key-down
//!
//! `SendInput` appends to the input stream: what the hook sends from inside its callback
//! arrives after the event being handled. A key that the hook lets through would therefore
//! reach the application ahead of the edit sent for it, so an edit never travels with its
//! key:
//!
//! - no edit, not eaten: the key passes;
//! - no edit, eaten: the key is blocked (only a hotkey that consumes its key does this, such
//!   as the hard reset Ctrl+Shift+Alt+F12);
//! - an edit, eaten: the edit is sent, then the core's replay key if there is one (Enter or
//!   Tab after a macro), and the key is blocked;
//! - an edit, not eaten (the core lets the key through): the key is blocked, and the edit is
//!   sent followed by the key again.
//!
//! A key sent again is its key-down alone, with its scan code and extended flag
//! (`raumakey_windows::input::key_down_input`): its physical key-up follows, and the
//! modifiers the user holds are already down in the system's key state, so no modifier is
//! pressed or released. No key is ever blocked without what replaces it being sent, except
//! by a hotkey.
//!
//! ## Failures
//!
//! When `SendInput` injects nothing (the input desktop is locked, or UIPI dropped it), the key
//! passes as if nothing had happened. When it injects part of the keys, the key stays
//! blocked: part of what replaces it is on its way, and the key would arrive ahead of it.
//! Either way the text is not what the engine believes, so the word of the document ends
//! (reported to the core as a change it did not make, `SelectionChange::Other`), and nothing
//! is sent again. An edit that does not fit the input builders is refused by the
//! [`BlindDocument`], and the core lets the key through.
//!
//! # Keys and their text
//!
//! The thread key state (`GetKeyState`) of a hook thread is stale, so the [`KeyTracker`] is
//! the source of the modifiers, the repeat flag (low-level hooks have none: a key-down of a
//! key already down is a repeat), and the Caps Lock toggle (read from the desktop when the
//! handler is built, then flipped by each Caps Lock key-down that is not a repeat). The text
//! of a key is what `ToUnicodeEx` gives with the layout of the foreground window's thread
//! and the tracker's key state, without changing the keyboard state. Right Alt is AltGr on
//! an AltGr layout (matrix #22), which is described once per layout change.
//!
//! The hook can miss input: the secure desktop (Ctrl+Alt+Del, the lock screen, UAC) and
//! windows of a higher integrity level take it away. A key whose release went missing would
//! stay down in the tracker and turn every letter into a shortcut, so before each key-down
//! that is not a modifier, the keys the tracker holds are checked against the physical state
//! (`GetAsyncKeyState`) and those that are up are forgotten; Caps Lock is then read again from
//! the desktop, as it is after keys passed to a more privileged window.
//!
//! A dead key types nothing yet; the application composes it with the next key, which the
//! hook cannot see, so that next key reaches the core without text and ends the word.
//!
//! # Documents
//!
//! A document is the pair (foreground window, focused window of its thread). Each has its
//! engine state, and only the active one can have a word in progress, so an edit never
//! reaches into another window. The hook notices a change of foreground or focus at the
//! next key; a switch away and back between two keys goes unnoticed, as does a caret moved
//! by the application itself (the hook cannot read the text). Clicks, navigation keys, and
//! chords end the word.
//!
//! # Secure fields
//!
//! The hook recognizes only Edit controls with `ES_PASSWORD` as secure fields, and applies
//! the policy's secure-field mode to them (D25; `Bypass`, the default, lets every key
//! through). Password fields of browsers and other frameworks look like ordinary text.
//!
//! # Hotkeys and the mode
//!
//! The core classifies the hotkeys (D5, D24): the toggle Ctrl+Shift fires on the release of
//! its last modifier, and the hard reset swallows its key. When the core changes the mode
//! itself, the [`ModeObserver`] is told the new mode, from inside the hook callback, after
//! the key was handled; the tray updates its icon and saves the mode. A mode posted from the
//! tray ([`TypingUpdate::Mode`]) or carried by a runtime snapshot is applied and not
//! reported back. Keys typed in a more privileged window never reach the classifier.
//!
//! # Threads and speed
//!
//! Windows calls the hook synchronously for every key and click and removes a hook that
//! takes longer than `LowLevelHooksTimeout`, so the callback does no file I/O, no IPC, and
//! never waits for another thread: the engine and `SendInput` run inside it. Configuration
//! changes reach it through the [`Mailbox`]: [`Mailbox::post`] checks a runtime snapshot and
//! builds a macro table on the posting thread; the hook thread takes the updates at its next
//! event with `try_lock`, and applies them before that event, in posting order. The process
//! of the foreground window (executable name, elevation) is queried again only when the
//! foreground window changes, and a layout is described once.
//!
//! # API of the run mode (Windows)
//!
//! ```text
//! let hook = TypingHook::start(runtime_snapshot, macro_snapshot, move |vietnamese| {
//!     // hook thread, inside the callback: post a message to the tray window and return
//! })?;                                  // Err(TypingError::Snapshot | TypingError::Hook)
//! hook.update(TypingUpdate::Mode(false))?;            // the tray chose English
//! hook.update(TypingUpdate::Runtime(new_snapshot))?;  // raumakey.toml changed
//! hook.update(TypingUpdate::Macros(new_macros))?;     // raumakey-macros.toml changed
//! let mailbox = hook.mailbox();         // Arc<Mailbox>: post from another thread
//! hook.stop()?;                         // or drop it: both hooks are removed
//! ```
//!
//! # Privacy
//!
//! Nothing here logs. `Debug` of every type shows counts, flags, and revisions, never keys,
//! text, words, or macro text.
//!
//! # Known limits
//!
//! - Edits are blind: an application that completes text by itself (a browser's address
//!   bar, Excel) or changes the text between keys can make the Backspaces delete the wrong
//!   characters.
//! - Another input method that runs at the same time (UniKey) transforms the same keys;
//!   keys injected by other programs are typed like physical ones.
//! - Antivirus or anti-cheat software may flag a global keyboard hook.
//! - Decomposed output (NFD) sends one Backspace per combining mark, which applications that
//!   delete a whole grapheme cluster do not match.
//! - The synthetic left Ctrl of AltGr is recognized by its scan code `0x21D`.
//! - The menu-activation mask of the hard reset (a lone Alt release after its key was
//!   swallowed) is not sent.
//!
//! # Manual check list
//!
//! On a test desktop, with no other Vietnamese input method running, start the hook with the
//! ignored test `manual_typing_hook` (`tests/manual.rs`; `RAUMAKEY_TYPING_SECONDS` sets how
//! long it runs) or, once it is wired, with `raumakey.exe`. Record each result.
//!
//! 1. **Notepad, Telex:** type `tieengs vieetj` then Space: `tiếng việt `. Type
//!    `xin chaof` and `ddi hojc`: `xin chào`, `đi học`.
//! 2. **Backspace restore (matrix #17):** type `aa` (`â`), Backspace: `a`; type `vieetj`,
//!    Backspace: `viêt`, then `s`: `viết`.
//! 3. **Click ends the word (matrix #19):** type `tiee`, click elsewhere in the text, click
//!    back at the end, type `s`: `tiês`, and nothing before the click changed.
//! 4. **Alt+Tab (matrix #20):** type `tiee` in Notepad, Alt+Tab to WordPad, type `s`: WordPad
//!    gets `s`, Notepad still shows `tiê`; Alt+Tab back, type `s`: Notepad shows `tiês` (the
//!    word ended). Also switch with the mouse and with Win+number.
//! 5. **Ctrl+Shift toggle:** tap Ctrl+Shift: the mode switches to English (`aa` stays `aa`),
//!    tap again: Vietnamese; the observer (or the tray icon) follows each switch. Ctrl+Shift+T
//!    or Ctrl+Shift+click does not switch.
//! 6. **Elevated window:** open an elevated command prompt; with a non-elevated hook,
//!    `tieengs` stays `tieengs` and no key is lost or doubled. Run the hook elevated: the
//!    prompt types `tiếng`.
//! 7. **Password field:** in a classic Win32 password Edit (for example a Run-as or network
//!    credential dialog that uses one), `aas` stays `aas`. Note what browser password fields
//!    do (expected: transformed, a known limit).
//! 8. **VNI:** with a runtime snapshot that selects VNI, `tie6ng1 vie65t`: `tiếng việt`.
//! 9. **Caps Lock:** start with Caps Lock on: `tieengs` gives `TIẾNG`; toggle it while the
//!    hook runs and repeat both ways; Shift with Caps Lock on gives lowercase. Toggle Caps
//!    Lock in an elevated window or on the lock screen (Win+L), come back, and check the case
//!    of the next edited word.
//! 10. **Typing fast:** type a paragraph of Telex as fast as possible, with overlapping keys
//!     and auto-repeat (hold `a`): no lost, doubled, or reordered characters; Windows does not
//!     remove the hook (keys keep being transformed afterwards).
//! 11. **Missed releases:** press Ctrl+Alt+Del, cancel, then type `aa`: `â` (not a
//!     shortcut). Lock with Win+L, unlock, type `aa`: `â`.
//! 12. **Shortcuts and AltGr:** Ctrl+C, Ctrl+V, Alt+F4 work and end the word; on a German
//!     layout AltGr+Q types `@` and AltGr+E types `€`, and AltGr never toggles the mode.
//! 13. **Hard reset:** Ctrl+Shift+Alt+F12 swallows F12, ends the word, and leaves the mode;
//!     note whether releasing Alt last opens the menu bar of Notepad.
//! 14. **Macros and Enter:** with a macro `ko` -> `không`, type `ko` then Enter: `không` and a
//!     new line, in Notepad and in a single-line field that submits on Enter.
//! 15. **Browsers and Office:** type in Edge or Chrome (a page text area and the address
//!     bar), Word, and Excel; record where Backspaces go wrong.
//! 16. **Stop:** stop the hook (end of the test, or Exit in the tray): typing is plain again
//!     at once, nothing is stuck down.

mod blind;
mod handler;
#[cfg(windows)]
mod hook;
mod mailbox;
mod platform;
mod tracker;

pub use blind::BlindDocument;
pub use handler::{ModeObserver, TypingHandler, UNKNOWN_EXECUTABLE};
#[cfg(windows)]
pub use hook::{TypingError, TypingHook};
pub use mailbox::{Mailbox, TypingUpdate};
pub use platform::{Foreground, LayoutInfo, TypingPlatform};
pub use tracker::{KeyTracker, Transition};
