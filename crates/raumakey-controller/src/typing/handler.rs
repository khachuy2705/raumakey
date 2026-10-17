//! The handler of the low-level keyboard and mouse hooks.

use core::fmt;
use std::sync::Arc;

use raumakey_model::host::{DocumentId, ExeName, HostContext, SessionId};
use raumakey_model::input::{MAX_EVENT_TEXT_UNITS, ReplayKey, vk};
use raumakey_session::{KeyDownOutcome, KeyStroke, Role, SelectionChange, ServiceCore};
use raumakey_windows::hooks::{HookDecision, HookHandler, KeyboardEvent, MouseEvent};
use raumakey_windows::input::{
    InputError, SyntheticKey, is_extended_key, is_raumakey_extra_info, key_down_input,
};
use raumakey_windows::panic::guard_value;

use super::blind::BlindDocument;
use super::mailbox::{Mailbox, Prepared};
use super::platform::{Foreground, LayoutInfo, TypingPlatform};
use super::tracker::{KeyTracker, Transition};

/// The executable name of a process whose image name cannot be read, and of the keys typed
/// without a foreground window. No file can have this name, so no rule written for a real
/// executable applies to it.
pub const UNKNOWN_EXECUTABLE: &str = "<unknown>";

/// Num Lock and Scroll Lock: lock keys that type nothing.
const VK_NUMLOCK: u32 = 0x90;
const VK_SCROLL: u32 = 0x91;

/// Called on the hook thread when the session core changed the Vietnamese mode itself (the
/// toggle hotkey), with the new mode (true for Vietnamese). It must return at once: post a
/// message to the tray window, for example. A panic in it is contained.
pub type ModeObserver = Box<dyn FnMut(bool) + Send>;

/// What to do with a key-down after the core has handled it.
#[derive(Clone, PartialEq, Eq)]
enum Plan {
    /// Let the key through; nothing is sent.
    Pass,
    /// Block the key; nothing is sent. Only a hotkey that consumes its key does this.
    Block,
    /// Send these keys, then block the key they replace.
    Replace(Vec<SyntheticKey>),
    /// The core edited, but what replaces the key cannot be built: send nothing, end the word,
    /// and let the key through.
    Unsendable,
}

/// The plan for a key-down whose core outcome is `outcome` and whose edit, if the core made
/// one, is `edit`.
///
/// `SendInput` appends to the input stream: what the hook sends arrives after the event it is
/// handling, so a key let through would reach the application ahead of the edit. An edit
/// therefore always blocks the key, and when the core lets the key through all the same, the
/// key is sent again after the edit. A key the core eats is replaced by its edit and the
/// core's replay key. A key sent again is its key-down alone ([`key_down_input`]): its
/// physical key-up follows, and the modifiers the user holds are down already.
fn plan(outcome: KeyDownOutcome, edit: Option<Vec<SyntheticKey>>, key: &KeyboardEvent) -> Plan {
    let replay = match (outcome.eaten, outcome.replay) {
        (true, Some(replay)) => Some(replay_key_down(replay, key)),
        (true, None) => None,
        (false, _) => edit
            .is_some()
            .then(|| key_down_input(key.vk, key.scan, key.extended)),
    };
    let mut keys = match edit {
        Some(keys) => keys,
        None if !outcome.eaten => return Plan::Pass,
        None if replay.is_none() => return Plan::Block,
        None => Vec::new(),
    };
    match replay {
        Some(Ok(replay)) => keys.push(replay),
        Some(Err(_)) => return Plan::Unsendable,
        None => {}
    }
    Plan::Replace(keys)
}

/// The key-down that replays `replay` for the blocked `key`. The engine replays the key it
/// was given (Enter or Tab after a macro), which keeps the scan code and extended flag of the
/// physical key; a key of its own would take the usual extended flag of its code.
fn replay_key_down(replay: ReplayKey, key: &KeyboardEvent) -> Result<SyntheticKey, InputError> {
    if replay.virtual_key == key.vk {
        key_down_input(key.vk, key.scan, key.extended)
    } else {
        key_down_input(
            replay.virtual_key,
            replay.scan_code.unwrap_or(0),
            is_extended_key(replay.virtual_key),
        )
    }
}

/// The document of a foreground: its foreground window and the window with keyboard focus.
fn document_of(foreground: &Foreground) -> DocumentId {
    let window = u128::from(foreground.window as usize as u64);
    let focus = u128::from(foreground.focus as usize as u64);
    DocumentId::from_u128((window << 64) | focus)
}

/// The executable of a foreground, or [`UNKNOWN_EXECUTABLE`].
fn executable_of(foreground: Option<&Foreground>) -> Option<ExeName> {
    foreground
        .and_then(|foreground| foreground.executable.clone())
        .or_else(|| ExeName::new(UNKNOWN_EXECUTABLE).ok())
}

/// The handler of the typing hook (plan D39, design §21.2): every key and mouse press of the
/// session reaches it on the hook thread, and it drives a [`ServiceCore`] in the
/// [keyboard-hook role](Role::KeyboardHook) over a blind [`BlindDocument`], sending what
/// replaces a key through the [`TypingPlatform`]. The [module documentation](super) lists
/// its rules.
///
/// It never panics on any input as far as its tests reach; the hook thread contains a panic
/// all the same (`raumakey_windows::hooks`). `Debug` shows the core's revisions and mode,
/// whether keys pass untouched, and counts, never keys or text.
pub struct TypingHandler<P> {
    platform: P,
    core: ServiceCore,
    tracker: KeyTracker,
    mailbox: Arc<Mailbox>,
    observer: ModeObserver,
    /// This process is elevated.
    elevated: bool,
    /// The document of the last key, `None` before the first and without a foreground.
    document: Option<DocumentId>,
    /// The foreground runs with more privileges than this process: keys pass untouched.
    passing: bool,
    /// The layout of the last key and its description.
    layout: Option<(isize, LayoutInfo)>,
    /// Counts document changes, for the host context.
    epoch: u64,
    /// The last key-down that typed text was a dead key: the application composes the next
    /// key with it, which the hook cannot see.
    dead_key: bool,
    /// The host context of the last key, for mouse presses.
    host: Option<HostContext>,
}

impl<P: TypingPlatform> TypingHandler<P> {
    /// A handler over `platform`: a core in the keyboard-hook role on defaults v1, the
    /// updates already posted to `mailbox` applied, the Caps Lock toggle read from the
    /// desktop, and `observer` told whenever the core toggles the mode itself.
    pub fn new(
        mut platform: P,
        mailbox: Arc<Mailbox>,
        observer: impl FnMut(bool) + Send + 'static,
    ) -> Self {
        let elevated = platform.process_is_elevated();
        let caps_lock = platform.caps_lock_on();
        let mut handler = Self {
            platform,
            core: ServiceCore::with_role(Role::KeyboardHook),
            tracker: KeyTracker::new(caps_lock),
            mailbox,
            observer: Box::new(observer),
            elevated,
            document: None,
            passing: false,
            layout: None,
            epoch: 0,
            dead_key: false,
            host: None,
        };
        handler.apply_updates();
        handler
    }

    /// The session core.
    pub fn core(&self) -> &ServiceCore {
        &self.core
    }

    /// The key tracker.
    pub fn tracker(&self) -> &KeyTracker {
        &self.tracker
    }

    /// The platform.
    pub fn platform(&self) -> &P {
        &self.platform
    }

    /// The platform, for tests that play the desktop around the handler.
    pub fn platform_mut(&mut self) -> &mut P {
        &mut self.platform
    }

    /// A key event of the hook: whether the key reaches the application. Every key that is
    /// blocked is replaced by what was sent, unless a hotkey consumes it.
    pub fn on_key(&mut self, event: &KeyboardEvent) -> HookDecision {
        if is_raumakey_extra_info(event.extra_info) {
            return HookDecision::PassThrough;
        }
        self.apply_updates();
        let down = !event.key_up;
        if down
            && !vk::is_modifier(event.vk)
            && self
                .tracker
                .forget_missed_releases(event.vk, |key| self.platform.key_is_down(key))
        {
            // Releases went missing, so Caps Lock presses may have too.
            let caps_lock = self.platform.caps_lock_on();
            self.tracker.set_caps_lock(caps_lock);
        }
        let repeat = match self.tracker.observe(event) {
            Transition::SyntheticCtrl => return HookDecision::PassThrough,
            Transition::Down { repeat } => repeat,
            Transition::Up => true,
        };
        let foreground = self.platform.foreground();
        if self.passes_untouched(foreground.as_ref()) {
            if !self.passing {
                self.passing = true;
                self.enter_document(None);
            }
            return HookDecision::PassThrough;
        }
        if self.passing {
            // The hook may not have seen the keys typed in that window.
            self.passing = false;
            let caps_lock = self.platform.caps_lock_on();
            self.tracker.set_caps_lock(caps_lock);
        }
        let document = foreground.as_ref().map(document_of);
        if document != self.document {
            self.enter_document(document);
        }
        let (layout, altgr) = self.layout_of(foreground.as_ref());
        let Some(host) = self.host_context(foreground.as_ref(), document) else {
            return HookDecision::PassThrough;
        };
        let mode = self.core.vietnamese_mode();
        let decision = if down {
            let text = match (&foreground, layout) {
                (Some(_), Some(layout)) => self.key_text(event, layout, altgr),
                _ => Vec::new(),
            };
            let stroke = self.stroke(event, repeat, altgr, text, host);
            let mut port = BlindDocument::new();
            let outcome = self.core.key_down(&stroke, &mut port);
            let edit = port.edited().then(|| port.into_keys());
            self.carry_out(plan(outcome, edit, event), document)
        } else {
            let stroke = self.stroke(event, repeat, altgr, Vec::new(), host);
            if self.core.key_up(&stroke) {
                HookDecision::Swallow
            } else {
                HookDecision::PassThrough
            }
        };
        self.report_mode(mode);
        decision
    }

    /// A mouse button press: the word ends (matrix #19), unless the host's policy turns
    /// `reset_on_mouse` off, and a modifier-only hotkey held meanwhile does not fire.
    pub fn on_mouse(&mut self, event: &MouseEvent) {
        if is_raumakey_extra_info(event.extra_info) {
            return;
        }
        self.apply_updates();
        let host = self
            .host
            .clone()
            .or_else(|| unknown_host(None, None, String::new(), self.epoch));
        if let Some(host) = host {
            self.core.mouse_down(&host);
        }
    }

    /// Applies what was posted to the mailbox, in posting order. A refused snapshot (the
    /// mailbox checked it already) leaves the last good configuration.
    fn apply_updates(&mut self) {
        for update in self.mailbox.take() {
            match update {
                Prepared::Runtime(snapshot) => {
                    let _ = self.core.apply_runtime_snapshot(&snapshot);
                }
                Prepared::Macros { revision, table } => {
                    self.core.install_macro_table(revision, table);
                }
                Prepared::Mode(mode) => self.core.set_vietnamese_mode(mode),
            }
        }
    }

    /// Whether keys must pass untouched: the foreground runs with more privileges than this
    /// process, or its privileges cannot be read (User Interface Privilege Isolation would
    /// drop what the hook sends, and a blocked key would be lost).
    fn passes_untouched(&self, foreground: Option<&Foreground>) -> bool {
        !self.elevated && foreground.is_some_and(|foreground| foreground.elevated != Some(false))
    }

    /// The keys now go to `document`: the words of the last document and of `document` end
    /// (matrix #20), and the hotkey classifier starts again.
    fn enter_document(&mut self, document: Option<DocumentId>) {
        self.core.set_focus(document);
        self.document = document;
        self.epoch = self.epoch.wrapping_add(1);
        self.dead_key = false;
    }

    /// The layout of the foreground thread and whether it has AltGr, described once per
    /// change; the core follows the AltGr flag. Without a foreground, the last layout.
    fn layout_of(&mut self, foreground: Option<&Foreground>) -> (Option<isize>, bool) {
        let Some(foreground) = foreground else {
            return (
                self.layout.as_ref().map(|(handle, _)| *handle),
                self.layout.as_ref().is_some_and(|(_, info)| info.altgr),
            );
        };
        let handle = self.platform.keyboard_layout(foreground.thread_id);
        let current = self.layout.as_ref().map(|(current, _)| *current);
        if current != Some(handle) {
            let info = self.platform.describe_layout(handle);
            self.core.set_altgr_layout(info.altgr);
            self.layout = Some((handle, info));
        }
        let altgr = self.layout.as_ref().is_some_and(|(_, info)| info.altgr);
        (Some(handle), altgr)
    }

    /// The host context of a key in `document`, kept for mouse presses; `None` only in the
    /// impossible case of [`unknown_host`].
    fn host_context(
        &mut self,
        foreground: Option<&Foreground>,
        document: Option<DocumentId>,
    ) -> Option<HostContext> {
        let layout = self
            .layout
            .as_ref()
            .map(|(_, info)| info.name.clone())
            .unwrap_or_default();
        let host = unknown_host(foreground, document, layout, self.epoch);
        self.host.clone_from(&host);
        host
    }

    /// The text of a key-down for the core: what the layout types with the tracker's key
    /// state, or nothing for modifiers, lock keys, a dead key, the key after a dead key (the
    /// application composes the two), and a key that would type more than one event holds.
    fn key_text(&mut self, event: &KeyboardEvent, layout: isize, altgr: bool) -> Vec<u16> {
        if vk::is_modifier(event.vk) || matches!(event.vk, vk::CAPITAL | VK_NUMLOCK | VK_SCROLL) {
            return Vec::new();
        }
        let translation =
            self.platform
                .translate(event.vk, event.scan, &self.tracker.snapshot(altgr), layout);
        let after_dead_key = std::mem::replace(&mut self.dead_key, translation.dead_key);
        if after_dead_key || translation.dead_key || translation.units.len() > MAX_EVENT_TEXT_UNITS
        {
            Vec::new()
        } else {
            translation.units
        }
    }

    /// The key stroke of `event` for the core.
    fn stroke(
        &self,
        event: &KeyboardEvent,
        repeat: bool,
        altgr: bool,
        text: Vec<u16>,
        host: HostContext,
    ) -> KeyStroke {
        KeyStroke {
            virtual_key: event.vk,
            scan_code: event.scan,
            extended: event.extended,
            repeat,
            modifiers: self.tracker.modifiers(altgr),
            text,
            extra_info: event.extra_info as u64,
            host,
        }
    }

    /// Carries out `plan` for a key-down in `document`. A send that injects nothing lets the
    /// key through; a send that injects only part leaves the key blocked (part of what
    /// replaces it is on its way, and the key would arrive ahead of it). Either way the text
    /// of the document is not what the engine believes, so the word ends; nothing is sent
    /// again.
    fn carry_out(&mut self, plan: Plan, document: Option<DocumentId>) -> HookDecision {
        match plan {
            Plan::Pass => HookDecision::PassThrough,
            Plan::Block => HookDecision::Swallow,
            Plan::Unsendable => {
                self.end_word(document);
                HookDecision::PassThrough
            }
            Plan::Replace(keys) => {
                let report = self.platform.send(&keys);
                if report.is_complete() {
                    return HookDecision::Swallow;
                }
                self.end_word(document);
                if report.sent == 0 {
                    HookDecision::PassThrough
                } else {
                    HookDecision::Swallow
                }
            }
        }
    }

    /// Ends the word of `document`: its text is not what the engine believes.
    fn end_word(&mut self, document: Option<DocumentId>) {
        if let Some(document) = document {
            self.core
                .selection_changed(document, SelectionChange::Other);
        }
    }

    /// Tells the observer when the core changed the mode from `before` itself.
    fn report_mode(&mut self, before: bool) {
        let mode = self.core.vietnamese_mode();
        if mode != before {
            let observer = &mut self.observer;
            guard_value((), || observer(mode));
        }
    }
}

/// The host context of a key in `document` of `foreground`, with KLID `layout`; with no
/// foreground, a context of [`UNKNOWN_EXECUTABLE`] without a document. `None` only if
/// [`UNKNOWN_EXECUTABLE`] were not a valid name, which a test rules out.
fn unknown_host(
    foreground: Option<&Foreground>,
    document: Option<DocumentId>,
    layout: String,
    epoch: u64,
) -> Option<HostContext> {
    let executable = executable_of(foreground)?;
    let process_id = foreground.map_or(0, |foreground| foreground.process_id);
    let mut host = HostContext::new(SessionId::from_u128(u128::from(process_id)), executable);
    host.process_id = u64::from(process_id);
    if let Some(foreground) = foreground {
        host.thread_id = u64::from(foreground.thread_id);
        host.window_class.clone_from(&foreground.window_class);
        host.is_secure_field = foreground.secure_field;
        host.is_elevated = foreground.elevated.unwrap_or(true);
    }
    host.document_id = document;
    // The hook has no TSF; the transport the policy selects is ignored in the hook role.
    host.supports_tsf_range_edit = false;
    if !layout.is_empty() {
        host.keyboard_layout = layout;
    }
    host.foreground_epoch = epoch;
    Some(host)
}

impl<P: TypingPlatform> HookHandler for TypingHandler<P> {
    fn on_key(&mut self, event: &KeyboardEvent) -> HookDecision {
        TypingHandler::on_key(self, event)
    }

    fn on_mouse(&mut self, event: &MouseEvent) {
        TypingHandler::on_mouse(self, event);
    }
}

impl<P> fmt::Debug for TypingHandler<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TypingHandler")
            .field("core", &self.core)
            .field("tracker", &self.tracker)
            .field("elevated", &self.elevated)
            .field("passing", &self.passing)
            .field("epoch", &self.epoch)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    //! The plan of a key-down: what is sent and whether the key is blocked, for every
    //! outcome, including one the current engine never produces (an edit that lets its key
    //! through).

    use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
    use raumakey_model::input::{ModifierSet, ReplayKey, vk};
    use raumakey_session::KeyDownOutcome;
    use raumakey_windows::hooks::KeyboardEvent;
    use raumakey_windows::input::{
        SyntheticKey, key_down_input, keyboard_inputs_for_backspaces, unicode_text_inputs,
    };

    use super::{Plan, UNKNOWN_EXECUTABLE, plan};

    fn key(virtual_key: u32, scan: u32) -> KeyboardEvent {
        KeyboardEvent {
            vk: virtual_key,
            scan,
            extended: false,
            injected: false,
            injected_lower_il: false,
            alt_down: false,
            key_up: false,
            time: 0,
            extra_info: 0,
        }
    }

    /// The edit that replaces the `e` before the caret with `ê`.
    fn edit() -> Vec<SyntheticKey> {
        let mut keys = keyboard_inputs_for_backspaces(1).unwrap();
        keys.extend(unicode_text_inputs(&[0xEA]).unwrap());
        keys
    }

    /// What a plain text field holds after `text` receives the key-downs of `keys` in order:
    /// Backspace deletes, a Unicode key types its unit, and a letter key types its letter.
    fn apply(text: &str, keys: &[SyntheticKey]) -> String {
        let mut text: Vec<u16> = text.encode_utf16().collect();
        for key in keys.iter().filter(|key| !key.key_up) {
            if key.unicode {
                text.push(key.scan_code);
            } else if key.virtual_key == 0x08 {
                text.pop();
            } else if (0x41..=0x5A).contains(&key.virtual_key) {
                text.push(key.virtual_key + 0x20);
            }
        }
        String::from_utf16_lossy(&text)
    }

    #[test]
    fn a_key_without_an_edit_passes_or_is_blocked_by_a_hotkey() {
        let a = key(0x41, 0x1E);
        assert!(plan(KeyDownOutcome::pass_through(), None, &a) == Plan::Pass);
        let eaten = KeyDownOutcome {
            eaten: true,
            replay: None,
        };
        assert!(plan(eaten, None, &a) == Plan::Block);
    }

    #[test]
    fn an_eaten_key_is_replaced_by_its_edit() {
        let e = key(0x45, 0x12);
        let eaten = KeyDownOutcome {
            eaten: true,
            replay: None,
        };
        assert!(plan(eaten, Some(edit()), &e) == Plan::Replace(edit()));
    }

    #[test]
    fn an_edit_that_lets_its_key_through_blocks_it_and_sends_it_after_the_edit() {
        // `SendInput` appends: a key let through would reach the application ahead of the
        // edit. The plan blocks the key and sends it again, tagged, after the edit.
        let x = key(0x58, 0x2D);
        let Plan::Replace(keys) = plan(KeyDownOutcome::pass_through(), Some(edit()), &x) else {
            panic!("the key must be replaced");
        };
        let mut expected = edit();
        expected.push(key_down_input(0x58, 0x2D, false).unwrap());
        assert_eq!(keys, expected);
        assert!(
            keys.iter()
                .all(|key| key.extra_info as u64 == RAUMAKEY_INJECTED_TAG)
        );
        assert_eq!(apply("tie", &keys), "tiêx");
        // Letting the key through would have put it before the edit.
        let mut wrong = vec![key_down_input(0x58, 0x2D, false).unwrap()];
        wrong.extend(edit());
        assert_eq!(apply("tie", &wrong), "tieê");
    }

    #[test]
    fn the_replay_of_an_eaten_key_follows_its_edit_as_a_key_down() {
        let mut enter = key(vk::RETURN, 0x1C);
        enter.extended = true;
        let replay = ReplayKey {
            virtual_key: vk::RETURN,
            scan_code: Some(0x1C),
            modifiers: ModifierSet {
                shift: true,
                ..ModifierSet::default()
            },
        };
        let outcome = KeyDownOutcome {
            eaten: true,
            replay: Some(replay),
        };
        let Plan::Replace(keys) = plan(outcome, Some(edit()), &enter) else {
            panic!("the key must be replaced");
        };
        assert_eq!(
            keys.last().copied(),
            Some(key_down_input(vk::RETURN, 0x1C, true).unwrap())
        );
        assert_eq!(
            keys.len(),
            edit().len() + 1,
            "no modifier is pressed or released"
        );
        // A replay without an edit is sent alone.
        assert!(
            plan(outcome, None, &enter)
                == Plan::Replace(vec![key_down_input(vk::RETURN, 0x1C, true).unwrap()])
        );
    }

    #[test]
    fn an_edit_whose_key_cannot_be_sent_again_is_not_sent_at_all() {
        let odd = key(0x58, 0x1_0000);
        assert!(plan(KeyDownOutcome::pass_through(), Some(edit()), &odd) == Plan::Unsendable);
    }

    #[test]
    fn the_unknown_executable_is_a_valid_name() {
        assert!(raumakey_model::host::ExeName::new(UNKNOWN_EXECUTABLE).is_ok());
    }
}
