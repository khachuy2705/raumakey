//! Platform-independent session core of Raumakey (plan §10.3, design §5.2).
//!
//! The TSF text service (`raumakey_tip.dll`, crate `raumakey-tsf`) runs on it, and so does
//! the keyboard-hook transport of the portable controller (plan D39, module `typing` of
//! `raumakey-controller`). It lives in its own crate so that the controller executable can
//! link it without the DLL's exports and resources. The rest of this documentation describes
//! the core as the text service drives it; [Roles](#roles) says what differs for the hook.
//!
//! [`ServiceCore`] makes every decision of the text input processor. The COM layer (the
//! `server` module of `raumakey_tip` describes it) translates TSF callbacks into calls on it
//! and carries out what it returns; it reaches a document only through the [`DocumentPort`]
//! edit-session abstraction. Nothing in this crate uses Win32 or COM types, and it has no
//! `unsafe` code, so it is tested with fakes on any host (design §15.1). Those tests say
//! nothing about behavior inside a real Windows application.
//!
//! | TSF callback, host event, or controller message | Core call |
//! |---|---|
//! | `ITfKeyEventSink::OnTestKeyDown` / `OnKeyDown` | [`ServiceCore::test_key_down`] / [`ServiceCore::key_down`] |
//! | `ITfKeyEventSink::OnTestKeyUp` / `OnKeyUp` | [`ServiceCore::test_key_up`] / [`ServiceCore::key_up`] |
//! | `ITfThreadMgrEventSink::OnSetFocus`, `ITfThreadFocusSink` | [`ServiceCore::set_focus`] |
//! | `ITfTextEditSink::OnEndEdit` for a change that Raumakey's edit sessions did not make | [`ServiceCore::selection_changed`] with its [`SelectionChange`] |
//! | a mouse button press in the host | [`ServiceCore::mouse_down`] |
//! | a context or document manager going away | [`ServiceCore::document_closed`] |
//! | `RuntimeSnapshot` | [`ServiceCore::apply_runtime_snapshot`] |
//! | `MacroSnapshot` | [`build_macro_table`] on the IPC thread, then [`ServiceCore::install_macro_table`] ([`ServiceCore::apply_macro_snapshot`] does both) |
//! | `ModeChanged`, keyboard open/close compartment | [`ServiceCore::set_vietnamese_mode`] |
//! | `ResetAll` | [`ServiceCore::reset_all`] |
//! | control connection up or down | [`ServiceCore::set_connected`] |
//! | keyboard layout change | [`ServiceCore::set_altgr_layout`] |
//! | TSF range-edit probe of the host (matrix #27) | [`ServiceCore::needs_probe`], [`ServiceCore::record_probe`] |
//!
//! # When `test_key_down` claims a key
//!
//! TSF calls `OnKeyDown` only for a key that `OnTestKeyDown` claimed, and `OnKeyDown` may
//! still report the key as not eaten so that the application receives it. The core therefore
//! claims every key for which `key_down` has something to do (an edit, a replay, eating the
//! key, a hotkey action, or committing a state change) and nothing else:
//!
//! | Key-down | Claimed | `key_down` |
//! |---|---|---|
//! | Injected: extra info `RAUMAKEY_INJECTED_TAG` (matrix #39) | no | not eaten; nothing is touched, not even the hotkey classifier |
//! | Text-service role only: its host's policy selects a transport other than TSF | no | not eaten; the controller's hook handles that host, so no key is handled twice |
//! | Disconnected, and the hotkey classifier reports a hotkey (D24) | yes | runs the action (toggle the mode, or reset every document); eaten when the hotkey consumes its key, which key hotkeys do |
//! | Disconnected, and the classifier swallows the key (auto-repeat of a consumed hotkey key) | yes | eaten |
//! | No document, a modifier, or a lock key | no | not eaten |
//! | A secure field whose policy does not allow Vietnamese (`Bypass`, D25) while its document has a word state | yes | ends the word, without the engine; not eaten |
//! | The same without a word state | no | (not called) |
//! | The engine transaction edits, consumes, or replays | yes | one edit through the port; on success the next state is committed, the key is eaten when the transaction consumes it, and its replay key is returned |
//! | The engine lets the key through but changes the state: a letter extends the word, a boundary or navigation key ends it | yes | commits the state; not eaten |
//! | The engine lets the key through and the state stays as it is | no | (not called) |
//! | Engine error while the document has a word state | yes | resets the word; not eaten |
//! | Engine error without one | no | (not called) |
//!
//! When the edit fails ([`EditError`]), or the transaction breaks the host policy (more units
//! than `max_delete_before`, a replay where replays are not permitted) or the bounds of the
//! word the engine owns, nothing is committed: the word resets and the key reaches the
//! application, without a replay (matrix #41). A replay key is returned only with an eaten
//! key; the COM layer sends it with `SendInput`, tagged `RAUMAKEY_INJECTED_TAG`, after
//! `OnKeyDown` returns, and the tag makes the core let it through.
//!
//! Key-ups are claimed only while disconnected (or in the [hook role](#roles)), when the
//! classifier reports a hotkey or a key-up to swallow. A modifier-only hotkey (the default
//! toggle `Ctrl+Shift`, D5) fires on the release of its last modifier: `key_up` runs the
//! action and lets the key-up through. The key-up of a consumed key hotkey is eaten.
//!
//! `test_key_down` never edits the document and commits no engine result: it runs the engine
//! on the current state and keeps the decision. (A key from another document than the last
//! one is a document change, which ends words and restarts the hotkey classifier already in
//! the test; see [Documents](#documents).) `key_down` reuses the decision only for the
//! identical key while the document's state generation and the configuration revision are
//! those it was computed against: a key-down without a test, another key, or anything in
//! between that changes the configuration or ends the word (a snapshot, a mode change, a
//! reset, a focus, selection, or mouse event) makes `key_down` compute the key again.
//!
//! Each key transition reaches the hotkey classifier once: `key_down` and `key_up` reuse the
//! verdict of their test, and a repeated test reuses the first. The core cannot tell a
//! repeated test from an auto-repeat of a key that it did not claim, so identical transitions
//! with no `key_down` or `key_up` in between are fed once; feeding the repeats would change
//! nothing, because such a key's verdict is `Pass` and an auto-repeat never fires a hotkey.
//!
//! # Documents
//!
//! Engine states are kept per document, for at most [`MAX_DOCUMENTS`] documents (the least
//! recently used is dropped beyond that). A focus change, or a key from another document
//! (or session) than the last one, ends the words of both documents: an edit never reaches
//! across documents (matrix #20), and no state is used for another owner, which the engine
//! would refuse with `ContextMismatch`. Only the active document can therefore have a word
//! in progress.
//!
//! A change of a document that Raumakey did not make ends the word of that document (matrix
//! #19). So does a mouse press in the host, unless the host's policy turns `reset_on_mouse`
//! off: a click can move the caret before the host reports the move, or without the host
//! ever reporting it.
//!
//! There is one exception, a decision of this crate that plan §10.3 does not record yet. A
//! key that the core lets through to the application while its document has a word state (a
//! letter that extends the word, or any key typed while an overlong word is skipped) changes
//! the document when it arrives, and the host reports that change too. The COM layer reports
//! what the edit record of every change shows ([`SelectionChange`]), and the core keeps the
//! word only when the first change of that document after the key is exactly the key
//! arriving, before the next key-down: its text inserted with the caret right after it, or,
//! for Backspace, text deleted at the caret. A click is never such a change, so it ends the
//! word even when the host has not reported the key yet. A report that comes after the next
//! key-down, that covers more than the key, or that shows other text ends the word as well.
//!
//! The edit checks the text before the caret, but that catches a caret move that was never
//! reported only when the text before the new caret differs from the word: after a click
//! behind the same letters the edit would change the wrong place. The change and mouse
//! reports are what prevent that.
//!
//! # Configuration
//!
//! Until the first snapshot the core runs on defaults v1 (`RuntimeSnapshot::defaults_v1`:
//! plan D4, D5, D13; Vietnamese mode on, no macros). The configuration is an immutable
//! [`RuntimeConfig`] behind an `Arc`; each snapshot, mode change, or macro table installs a
//! new one. A snapshot is checked before anything changes, and an invalid one leaves the
//! last good configuration in place: a runtime snapshot must pass `RuntimeSnapshot::validate`
//! and carry the key map that a user-defined input method names, and a macro snapshot must
//! pass `MacroSnapshot::validate` and macro validation. A runtime snapshot that changes the
//! options, key maps, rules, or mode ends every word in progress; the same settings again (a
//! reconnect) keep them. A macro table replaces the old one without ending words. A probe
//! result recorded through the core stands in only until a snapshot carries a record of its
//! executable (of the current probe version, or of a newer one that this build cannot
//! replace): the controller resolves the host with the record it publishes, so from then on
//! that record decides in the core too, and both pick the same transport for the host
//! ([`ServiceCore::record_probe`] says when they can briefly disagree).
//!
//! Per key, the engine runs with the snapshot's options, except that `enabled` is the live
//! Vietnamese mode and the host policy (`raumakey_policy::CompatibilityPolicy`) can turn
//! Vietnamese and macros off for the host.
//!
//! # Secure fields (D25, matrix #29)
//!
//! The policy's secure-field mode decides, and the core acts on it before the engine. In a
//! secure field the policy never allows macros, and in `Bypass` mode, the default, it does
//! not allow Vietnamese either: every key of such a field passes through untouched and ends
//! the word of its document, without reaching the engine, so nothing there is transformed,
//! expanded, edited, or tracked. In `Restricted` mode (`secure_field_mode = "restricted"` in
//! `raumakey-app-rules.toml`) the engine transforms the keys of a secure field as usual, with macros
//! off; the engine itself never expands a macro, word or token, for a secure-field key
//! either. A rule that turns Vietnamese off for the host stays in force in both modes, and
//! the field then passes its keys through as in `Bypass`. The clipboard transport is never
//! selected for a secure field, and the core never asks for a range-edit probe in one.
//!
//! # Hotkeys (D24)
//!
//! While [connected](ServiceCore::set_connected) to the controller, its global hook is the
//! only hotkey classifier: the core of the text service classifies nothing, and the
//! controller's `ModeChanged` and `ResetAll` reach the core as calls. (The core of the hook
//! is that classifier: see [Roles](#roles).) While disconnected, the core feeds every key
//! transition of a TSF host, except injected ones, to a `raumakey_policy::HotkeyClassifier`
//! built from the configured hotkeys: the toggle flips the local mode and ends every word,
//! the hard reset ends every word, and swallowed keys are eaten. Hotkeys are classified in
//! secure fields too (classifying is not a transformation) and without a focused document.
//! A connection change, a focus change, or a document change starts the classifier again
//! from no held keys. A mouse press, and a change of a document that Raumakey did not make
//! (other than the arrival of a key it let through), interrupt it: a modifier-only hotkey
//! held meanwhile, as in Ctrl+Shift+click, does not fire.
//!
//! # Roles
//!
//! [`ServiceCore::new`] builds the core of the text service ([`Role::TextService`]), and
//! [`ServiceCore::with_role`] the core of another front end. The portable controller types
//! every host through a global low-level keyboard hook and `SendInput` (plan D39, design
//! §21.2) and builds its core with [`Role::KeyboardHook`]. There is no TIP in that product,
//! so three rules change, and only these:
//!
//! - **Every host is the hook's.** The core handles a host whatever transport its policy
//!   selects (TSF, synthetic keys, or the clipboard, which the hook front end does not
//!   implement and replaces with synthetic keys). The text service lets such hosts through.
//! - **The core always classifies hotkeys.** It is the controller, so it feeds every key
//!   transition that is not injected to its classifier, whatever
//!   [`set_connected`](ServiceCore::set_connected) records, and a connection change does not
//!   restart the classifier.
//! - **No TSF probe.** [`ServiceCore::needs_probe`] never asks for one.
//!
//! A hook has no test-key-down step: its front end calls [`ServiceCore::key_down`] and
//! [`ServiceCore::key_up`] alone, which compute every key themselves. It cannot read the
//! application's text either, so it never reports a [`SelectionChange`]; it reports the
//! foreground and focus windows as documents ([`ServiceCore::set_focus`]), mouse presses
//! ([`ServiceCore::mouse_down`]), and an edit it could not send
//! ([`SelectionChange::Other`]). The rest of this documentation holds for both roles.
//!
//! # What the COM layer must do
//!
//! - Call the core from the thread that owns it, never from inside a [`DocumentPort`] call,
//!   and wrap every COM entry point and hook procedure in a panic guard
//!   (`raumakey_windows::panic`). The core is written not to panic on any input, and a
//!   property test drives it with arbitrary keys and events, but the guard is the boundary
//!   rule.
//! - Implement [`DocumentPort`] with one synchronous read-write edit session per call.
//! - Report every change of a document that its own edit sessions did not make through
//!   [`ServiceCore::selection_changed`], with the [`SelectionChange`] that the edit record
//!   shows: its text-change ranges (`ITfEditRecord::GetTextAndPropertyUpdates`) and the
//!   selection (`ITfEditRecord::GetSelectionStatus`), read with the read-only cookie of
//!   `OnEndEdit`. Whenever the record shows anything but exactly one of the other variants,
//!   or cannot be read, report [`SelectionChange::Other`]. A record that shows neither a text
//!   change nor a selection change is no change of the document and is not reported (a
//!   decision of this crate that plan §10.3 does not record yet): the edit sessions of other
//!   text services that only set properties, such as display attributes, leave the text and
//!   the caret as they were, and reporting them would end the word in progress.
//! - Report every mouse button press in the host's windows through
//!   [`ServiceCore::mouse_down`], for example from a `WH_MOUSE` hook of the thread that owns
//!   the core.
//! - Give each context a stable `DocumentId`, and each activation a `SessionId`, in the
//!   `HostContext` of every key.
//! - Translate keys with `ToUnicodeEx` without changing the keyboard state; report right Alt
//!   with its extended flag, drop the synthetic left Ctrl that Windows sends with AltGr, and
//!   keep [`ServiceCore::set_altgr_layout`] in step with the layout (the
//!   `raumakey_policy::hotkey` adapter contract). Pass the message extra info zero-extended
//!   ([`KeyStroke::extra_info`]).
//! - Send the replay key of [`KeyDownOutcome::replay`] with `SendInput`, tagged
//!   `RAUMAKEY_INJECTED_TAG`, after `OnKeyDown` returns.
//! - Build macro tables with [`build_macro_table`] on the IPC thread, not on the host
//!   application's UI thread, and hand them over with [`ServiceCore::install_macro_table`].
//! - Probe a host's TSF range edits when [`ServiceCore::needs_probe`] asks for it, record the
//!   result with [`ServiceCore::record_probe`], and report it to the controller
//!   (`Command::ReportProbe`). It never asks in a secure field or without a document: the
//!   probe edits the document, so a later ordinary document of the executable is probed.
//! - After a key call while disconnected, compare [`ServiceCore::vietnamese_mode`] with the
//!   keyboard open/close compartment and the language bar: a local toggle changes the mode
//!   without any other signal. After a reconnect the controller's snapshot sets the mode.
//! - Never log keys or text; the `Debug` output of this crate's types hides them.
//!
//! # Not covered here
//!
//! The probe edit itself, opt-in diagnostics, and the `evkey-compat` legacy path are not part
//! of this crate yet; that path must bypass keys tagged with the legacy sentinel
//! `0xEBCA1234` (clean-room spec §3.1), which the core still treats as ordinary input. How
//! real TSF hosts report a key that was let through must be checked on Windows: the word
//! survives only when the edit record shows exactly that key arriving before the next
//! key-down, and hosts that update their text asynchronously (such as browsers), or that
//! report the text change and the caret move separately, may not do that.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod config;
mod document;
mod keystroke;
mod service;
mod states;

pub use config::{RuntimeConfig, SnapshotError, build_macro_table, check_runtime_snapshot};
pub use document::{DocumentPort, EditError, EditRequest, SelectionChange};
pub use keystroke::KeyStroke;
pub use service::{KeyDownOutcome, Role, ServiceCore};
pub use states::MAX_DOCUMENTS;
