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
//! host: [`KeyTracker`], the [`TypingPlatform`] port, [`BlindDocument`], [`Mailbox`],
//! [`TypingHandler`], and the health decision [`hook_lost`]. On Windows, `TypingHook` (module
//! `hook`) runs the handler on a hook thread over the real platform (`raumakey-windows`
//! wrappers). Passing the tests says nothing about runtime behavior on a Windows desktop;
//! the [manual check list](#manual-check-list) does.
//!
//! # What the handler does with each event
//!
//! | Event | What happens |
//! |---|---|
//! | A key Raumakey injected (`dwExtraInfo` = `RAUMAKEY_INJECTED_TAG`, matrix #39) | passes untouched; nothing is read, tracked, or applied |
//! | Any other key or mouse event | first applies the updates posted to the [`Mailbox`] |
//! | Any other key | the [`KeyTracker`] records it (after the checks of [Key state](#key-state)); the synthetic left Ctrl of AltGr passes untouched and goes no further |
//! | The foreground runs with more privileges than `raumakey.exe`, or its privileges cannot be read | passes untouched (UIPI drops what a lower process sends, so a blocked key would be lost); on entering such a window the words end |
//! | The foreground thread's layout is a Chinese, Japanese, or Korean input method (primary language `0x04`, `0x11`, `0x12`) | passes untouched (the input method composes the keys); on entering it the words end |
//! | The foreground or focus window changed since the last key | the words of both documents end ([`ServiceCore::set_focus`](raumakey_session::ServiceCore::set_focus), matrix #20) |
//! | A key-down | [`ServiceCore::key_down`](raumakey_session::ServiceCore::key_down) (a hook has no test-key-down step) over a [`BlindDocument`]; then the plan below |
//! | A key-up | [`ServiceCore::key_up`](raumakey_session::ServiceCore::key_up); blocked only when a hotkey consumed its key |
//! | The release of a lone Alt tap (Alt down with no other modifier held, up before any other key went down; right Alt on an AltGr layout is AltGr and excluded) | the word ends: the window's menu bar is in menu mode now. F10, which does the same, types nothing and ends the word anyway |
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
//! pressed or released. A replay of another key than the blocked one is sent down and up. No
//! key is ever blocked without what replaces it being sent, except by a hotkey.
//!
//! Nothing is sent while Ctrl, Alt, or Win is physically down outside AltGr: the word ends
//! and the key passes, so that no Backspace ever arrives as Ctrl+Backspace (which deletes a
//! word in many editors) or as part of a shortcut. With the modifiers synced from the
//! physical state the core already treats such a key as a shortcut; this is the last guard.
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
//! # Key state
//!
//! Windows does not call the hook for every key the user presses: not while the secure
//! desktop has the input (Ctrl+Alt+Del, the lock screen, a UAC prompt), and not for the
//! input of a window that runs with more privileges than `raumakey.exe`. The thread key
//! state of a hook thread (`GetKeyState` for down keys) is stale besides, because the thread
//! receives no keyboard messages. So the [`KeyTracker`] keeps the state from the hook's own
//! events, and two reads of the desktop correct it, the only two the hook trusts:
//!
//! - **`GetAsyncKeyState`, for whether a key is down.** Before every key-down, each key the
//!   tracker holds (except the key being handled) is checked, and those that are up are
//!   dropped: their release was missed. The hotkey classifier of the core forgets its held
//!   keys then too ([`ServiceCore::transitions_missed`](raumakey_session::ServiceCore::transitions_missed)),
//!   so that the first Ctrl+Shift after unlocking toggles and Ctrl+Shift+F12 does not turn
//!   into the hard reset with a stale Alt. Before a key-down that is not a modifier, the eight
//!   side-specific modifier codes are then set from it both ways, so a modifier pressed where
//!   the hook did not see it counts too; while right Alt is down on an AltGr layout, left Ctrl
//!   is left alone (Windows holds a synthetic one with AltGr). A key-down that the hook
//!   blocked never reaches the system, so the physical state shows that key up: the tracker
//!   remembers such keys ([`KeyTracker::settle`]) and does not take them for missed releases,
//!   as in rollover, when the next key goes down while a replaced letter is still held.
//! - **`GetKeyState(VK_CAPITAL)`, for the Caps Lock toggle.** It is read when the handler is
//!   built, at every document change (foreground or focus window), after an idle gap of
//!   [`IDLE_RESYNC_MS`] or more since the last key event the hook saw, and after missed
//!   releases; in between, each Caps Lock key-down that is not a repeat flips it. The read
//!   happens before the key is handled, so a Caps Lock key-down at that moment still flips
//!   it. Without these reads, Caps Lock toggled where the hook was not called would give
//!   mixed case such as `xin TIếng` (the application types capitals, the edits do not).
//!
//! Low-level hooks have no repeat bit: a key-down of a key already down is a repeat. The text
//! of a key is what `ToUnicodeEx` gives with the layout of the foreground window's thread
//! and the tracker's key state, without changing the keyboard state. Right Alt is AltGr on
//! an AltGr layout (matrix #22), which is described once per layout change.
//!
//! A dead key types nothing yet; the application composes it with the next key, which the
//! hook cannot see, so that next key reaches the core without text and ends the word.
//!
//! # Documents
//!
//! A document is the pair (foreground window, focused window of its thread). Each has its
//! engine state, and only the active one can have a word in progress, so an edit never
//! reaches into another window. The hook notices a change of foreground or focus at the
//! next key it sees; a switch away and back between two keys goes unnoticed (Alt+Tab and
//! clicks end the word anyway), as does a caret moved by the application itself (the hook
//! cannot read the text). Clicks, navigation keys, and chords end the word.
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
//! reported back. Keys typed where the hook is not called never reach the classifier.
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
//! # Hook health
//!
//! Windows removes a low-level hook that timed out without telling its thread, and typing
//! turns plain. `TypingHook::ensure_alive`, called from the tray's 2-second timer, compares
//! the last event either hook saw (`raumakey_windows::hooks::Heartbeat`, mouse moves included)
//! with the last input of the session (`GetLastInputInfo`): when the session saw input more
//! than [`HOOK_SILENCE_LIMIT_MS`] later, while the foreground is a window the hook types in
//! ([`hook_lost`]), it stops the hook thread and starts a new one with the runtime snapshot
//! and macro table in effect, the mode the old one had, the updates posted since
//! ([`Mailbox::rearm`]), and the same observer. A restart after input that the hook was not
//! meant to see (the lock screen, a more privileged window) is harmless: only the words in
//! progress end.
//!
//! # API of the run mode (Windows)
//!
//! ```text
//! let mut hook = TypingHook::start(runtime_snapshot, macro_snapshot, move |vietnamese| {
//!     // hook thread, inside the callback: post a message to the tray window and return
//! })?;                                  // Err(TypingError::Snapshot | TypingError::Hook)
//! hook.update(TypingUpdate::Mode(false))?;            // the tray chose English
//! hook.update(TypingUpdate::Runtime(new_snapshot))?;  // raumakey.toml changed
//! hook.update(TypingUpdate::Macros(new_macros))?;     // raumakey-macros.toml changed
//! let mailbox = hook.mailbox();         // Arc<Mailbox>: post from another thread
//! let restarted = hook.ensure_alive()?; // every 2 s, from the tray's timer
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
//! - The Chinese, Japanese, and Korean input methods get every key untouched: no Vietnamese
//!   there, by design.
//! - Antivirus or anti-cheat software may flag a global keyboard hook.
//! - Decomposed output (NFD) sends one Backspace per combining mark, which applications that
//!   delete a whole grapheme cluster do not match.
//! - The synthetic left Ctrl of AltGr is recognized by its scan code `0x21D`.
//! - An edit under AltGr (rare: AltGr types characters that end the word) sends Backspaces
//!   while the synthetic left Ctrl is down.
//! - The menu-activation mask of the hard reset (a lone Alt release after its key was
//!   swallowed) is not sent.
//! - Whether `GetKeyState(VK_CAPITAL)` on the hook thread is current at the moments it is
//!   read is an open question (check list item 9).
//!
//! # Manual check list
//!
//! On a test desktop, with no other Vietnamese input method running, start the hook with the
//! ignored test `manual_typing_hook` (`tests/manual.rs`; `RAUMAKEY_TYPING_SECONDS` sets how
//! long it runs; it checks the hook's health every 2 seconds) or with the portable
//! `raumakey.exe` started without arguments (the run mode starts the hook, `crate::desktop`).
//! Record each result.
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
//! 9. **Caps Lock (open question: is `GetKeyState(VK_CAPITAL)` current on the hook thread
//!    when it is read?):** start with Caps Lock on: `tieengs` gives `TIẾNG`; toggle it while
//!    the hook runs and repeat both ways; Shift with Caps Lock on gives lowercase. Then:
//!    type `xin `, lock with Win+L, toggle Caps Lock on the lock screen, unlock, type
//!    `tieengs`: `xin TIẾNG`, never `xin TIếng`. The same with an elevated window in front
//!    (toggle Caps Lock there, come back after a second, and also after less than a second),
//!    and with a UAC prompt.
//! 10. **Typing fast:** type a paragraph of Telex as fast as possible, with overlapping keys
//!     and auto-repeat (hold `a`): no lost, doubled, or reordered characters; Windows does not
//!     remove the hook (keys keep being transformed afterwards).
//! 11. **Rollover over a replaced letter (open question: does `GetAsyncKeyState` show a key
//!     whose key-down the hook blocked as up, as the hook assumes?):** type `a`, then press
//!     the second `a` (replaced by `â`) and, still holding it, press `s`, then release both:
//!     `ấ`; repeat with Shift held throughout, and with Ctrl+Shift tapped right after: the
//!     toggle still works.
//! 12. **Missed releases:** press Ctrl+Alt+Del, cancel, then type `aa`: `â` (not a shortcut),
//!     and tap Ctrl+Shift: the mode switches at the first tap. Lock with Win+L, unlock, and
//!     check the same; then Ctrl+Shift+F12 types nothing special (it is no hard reset).
//! 13. **Modifiers the hook did not see:** with an elevated window in front, press and hold
//!     Ctrl, click Notepad while still holding it, and type `aa`: nothing is edited (it is
//!     Ctrl+A twice), and no word is deleted as by Ctrl+Backspace.
//! 14. **Shortcuts and AltGr:** Ctrl+C, Ctrl+V, Alt+F4 work and end the word; on a German
//!     layout AltGr+Q types `@` and AltGr+E types `€`, and AltGr never toggles the mode.
//! 15. **Menu mode:** type `tiee`, tap Alt alone (the menu bar of Notepad is selected), press
//!     Esc, type `s`: `tiês`. The same with F10.
//! 16. **Hard reset:** Ctrl+Shift+Alt+F12 swallows F12, ends the word, and leaves the mode;
//!     note whether releasing Alt last opens the menu bar of Notepad.
//! 17. **Macros and Enter:** with a macro `ko` -> `không`, type `ko` then Enter: `không` and a
//!     new line, in Notepad and in a single-line field that submits on Enter.
//! 18. **Chinese, Japanese, or Korean input method:** add one, switch Notepad to it, type:
//!     the input method composes as usual and Raumakey sends nothing; switch back: Telex
//!     works again, and a word typed before the switch is not continued.
//! 19. **Hook health:** make the hook time out (for example attach a debugger to the test
//!     process and break for a few seconds while typing, or lower `LowLevelHooksTimeout`
//!     under `HKCU\Control Panel\Desktop` and stress the machine): typing turns plain, then
//!     within about 2 seconds of the next input Vietnamese works again, with the same mode;
//!     the test prints the number of restarts. Also check that idle periods, the lock screen,
//!     and typing in an elevated window cause no restart, or only one harmless restart.
//! 20. **Browsers and Office:** type in Edge or Chrome (a page text area and the address
//!     bar), Word, and Excel; record where Backspaces go wrong.
//! 21. **Stop:** stop the hook (end of the test, or Exit in the tray): typing is plain again
//!     at once, nothing is stuck down.

mod blind;
mod handler;
mod health;
#[cfg(windows)]
mod hook;
mod mailbox;
mod platform;
mod tracker;

pub use blind::BlindDocument;
pub use handler::{IDLE_RESYNC_MS, ModeObserver, TypingHandler, UNKNOWN_EXECUTABLE};
pub use health::{HOOK_SILENCE_LIMIT_MS, hook_lost};
#[cfg(windows)]
pub use hook::{TypingError, TypingHook};
pub use mailbox::{Mailbox, TypingUpdate};
pub use platform::{Foreground, LayoutInfo, TypingPlatform};
pub use tracker::{KeyTracker, Transition};
