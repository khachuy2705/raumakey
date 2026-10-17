//! The service core: key handling, document states, and configuration.

use core::fmt;
use std::sync::Arc;

use raumakey_engine::{EngineResult, EngineState, VietnameseEngine};
use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_macro::MacroTable;
use raumakey_model::error::ResetReason;
use raumakey_model::host::{DocumentId, HostContext, SessionId};
use raumakey_model::hotkey::HotkeyAction;
use raumakey_model::input::{InputEvent, InputKind, ReplayKey, vk};
use raumakey_model::macros::MacroLookup;
use raumakey_model::options::EngineOptions;
use raumakey_model::policy::{HostPolicy, ProbeResult, Transport};
use raumakey_model::transaction::EditTransaction;
use raumakey_model::utf16;
use raumakey_policy::{CompatibilityPolicy, HotkeyClassifier, KeyVerdict, PolicyError};

use crate::config::{RuntimeConfig, SnapshotError, build_macro_table, check_runtime};
use crate::document::{DocumentPort, EditRequest, SelectionChange};
use crate::keystroke::KeyStroke;
use crate::states::DocumentStates;

/// What `OnKeyDown` reports and what the COM layer must do afterwards.
///
/// `Debug` says whether a key is replayed but not which one.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyDownOutcome {
    /// The key must not reach the application (`*pfEaten = TRUE`).
    pub eaten: bool,
    /// A key to replay with `SendInput`, tagged with
    /// [`RAUMAKEY_INJECTED_TAG`](raumakey_model::identity::RAUMAKEY_INJECTED_TAG), after
    /// `OnKeyDown` returns. Only set when the key was eaten.
    pub replay: Option<ReplayKey>,
}

impl KeyDownOutcome {
    /// The key reaches the application and nothing is replayed.
    pub const fn pass_through() -> Self {
        Self {
            eaten: false,
            replay: None,
        }
    }
}

impl fmt::Debug for KeyDownOutcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyDownOutcome")
            .field("eaten", &self.eaten)
            .field("replay", &self.replay.is_some())
            .finish()
    }
}

/// Most probe results the core keeps for snapshots that carry no record of their executable
/// yet. A text service runs in one host process and probes its executable; the bound only
/// keeps a misbehaving caller from growing the list.
const MAX_RECORDED_PROBES: usize = 8;

/// What a key let through to the application changes when it arrives, as in a plain text
/// control. `Debug` is not implemented: it holds the typed text.
#[derive(Clone)]
enum Arrival {
    /// The key types its text at the caret.
    Typed(Vec<u16>),
    /// Backspace deletes before the caret.
    Deleted,
}

impl Arrival {
    /// The arrival of the key of `event` when the key changes the document: it types text or
    /// deletes, and it is not a shortcut. Other keys move the caret or change nothing.
    fn of(event: &InputEvent) -> Option<Self> {
        if event.is_shortcut() {
            return None;
        }
        match event.kind {
            InputKind::Character | InputKind::Separator if !event.text_utf16.is_empty() => {
                Some(Self::Typed(event.text_utf16.clone()))
            }
            InputKind::Backspace => Some(Self::Deleted),
            _ => None,
        }
    }

    /// True when `change` is exactly this arrival.
    fn is(&self, change: SelectionChange<'_>) -> bool {
        match (self, change) {
            (Self::Typed(typed), SelectionChange::Inserted(inserted)) => {
                typed.as_slice() == inserted
            }
            (Self::Deleted, SelectionChange::Deleted) => true,
            _ => false,
        }
    }
}

/// A key-down let through to the application while its document had a word state.
struct Echo {
    document: DocumentId,
    arrival: Arrival,
}

/// What a key transition needs from `key_down` or `key_up`.
enum Decision {
    /// Nothing: the key reaches the application and no state changes.
    Pass,
    /// Like `Pass`, but the key changes `document` when it arrives while the document has a
    /// word state, so its `arrival` is expected as the next change of that document (see
    /// [`ServiceCore::selection_changed`]).
    PassEcho {
        document: DocumentId,
        arrival: Arrival,
    },
    /// A hotkey verdict while disconnected (D24): run `action`, if any, and eat the key when
    /// `eat` is set.
    Hotkey {
        action: Option<HotkeyAction>,
        eat: bool,
    },
    /// Reset the word of `document`; the key reaches the application.
    Reset { document: DocumentId },
    /// Apply the engine result to the document and commit its next state on success. When
    /// the key is let through and the next state is a word state, its `arrival` is expected.
    Apply {
        document: DocumentId,
        policy: HostPolicy,
        result: Box<EngineResult>,
        arrival: Option<Arrival>,
    },
}

impl Decision {
    /// Whether the test call must claim the key so that TSF calls `key_down`/`key_up`.
    fn claims(&self) -> bool {
        !matches!(self, Self::Pass | Self::PassEcho { .. })
    }

    /// The decision of a hotkey classifier verdict; `None` lets the key go on to the engine.
    fn of_verdict(verdict: KeyVerdict) -> Option<Self> {
        match verdict {
            KeyVerdict::Pass => None,
            KeyVerdict::Swallow => Some(Self::Hotkey {
                action: None,
                eat: true,
            }),
            KeyVerdict::Hotkey(found) => Some(Self::Hotkey {
                action: Some(found.action),
                eat: found.consume,
            }),
        }
    }
}

/// What an engine decision was computed against: the generation of the document's state
/// and the revision of the configuration.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Stamp {
    generation: u64,
    revision: u64,
}

/// An engine decision computed by `OnTestKeyDown` for [`Tested::key`].
struct Prepared {
    stamp: Stamp,
    decision: Decision,
}

/// The key transition of the last test call (`OnTestKeyDown` or `OnTestKeyUp`).
///
/// - Its hotkey `verdict` is reused by the matching `key_down`/`key_up` and by a repeated
///   test of the same transition, so that a transition reaches the classifier once however
///   often TSF tests it (D24). A repeated test cannot be told from an auto-repeat of a key
///   that was not claimed: identical transitions with no `key_down`/`key_up` in between are
///   fed once. Feeding the repeats would change nothing: the verdict of a key that was not
///   claimed is `Pass`, and an auto-repeat never fires a hotkey.
/// - Its `prepared` engine decision is reused by `key_down` only for the identical key while
///   the document state and the configuration are those it was computed against;
///   otherwise `key_down` computes the key again.
///
/// Any other key transition, focus change, document closure, or connection change replaces
/// or drops it, and `key_down`/`key_up` use it up.
struct Tested {
    down: bool,
    key: KeyStroke,
    verdict: Option<KeyVerdict>,
    prepared: Option<Prepared>,
}

impl Tested {
    fn fresh(key: &KeyStroke, down: bool) -> Self {
        Self {
            down,
            key: key.clone(),
            verdict: None,
            prepared: None,
        }
    }

    /// The same physical transition: direction, virtual key, scan code, and document.
    fn matches(&self, key: &KeyStroke, down: bool) -> bool {
        self.down == down
            && self.key.virtual_key == key.virtual_key
            && self.key.scan_code == key.scan_code
            && self.key.host.document_id == key.host.document_id
    }
}

/// A transaction the core will not apply.
struct Rejected;

/// The platform-independent core of the TSF text service.
///
/// It owns the engine, the immutable runtime configuration, the engine state of each
/// document, whether the controller connection is up, the hotkey classifier used while it
/// is down (D24), and the decision of the last test call. The [crate
/// documentation](crate) describes the rules.
///
/// One core serves one text service activation and is called from the thread that owns
/// it; it is `Send`, so the COM layer may move it there. It is written not to panic on any
/// input, which a property test checks with arbitrary keys and events; the COM layer still
/// guards every entry point. `Debug` shows revisions, the mode, and counts only.
pub struct ServiceCore {
    config: Arc<RuntimeConfig>,
    engine: VietnameseEngine,
    policy: CompatibilityPolicy,
    classifier: HotkeyClassifier,
    tested: Option<Tested>,
    states: DocumentStates,
    /// The document the last key came from, with its session.
    active: Option<(SessionId, DocumentId)>,
    /// The last key-down, when it was let through to the application while its document had
    /// a word state and no key-down came since: the next change of that document that is
    /// exactly its arrival keeps the word.
    echo: Option<Echo>,
    /// Probe results recorded through the core, applied again to the rules of each snapshot
    /// until one carries a record of their executable (see `record_probe`).
    probes: Vec<(HostContext, ProbeResult)>,
    connected: bool,
}

impl ServiceCore {
    /// A core on defaults v1, disconnected from the controller, with no documents.
    pub fn new() -> Self {
        let config = Arc::new(RuntimeConfig::defaults_v1());
        Self {
            engine: build_engine(&config),
            policy: CompatibilityPolicy::new(Arc::clone(config.compatibility())),
            classifier: HotkeyClassifier::new(config.hotkeys()),
            tested: None,
            config,
            states: DocumentStates::new(),
            active: None,
            echo: None,
            probes: Vec::new(),
            connected: false,
        }
    }

    /// The configuration in effect.
    pub fn config(&self) -> &Arc<RuntimeConfig> {
        &self.config
    }

    /// Live Vietnamese/English mode.
    pub fn vietnamese_mode(&self) -> bool {
        self.config.vietnamese_mode()
    }

    /// Whether the controller connection is up.
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    /// Number of documents whose engine state the core holds (at most
    /// [`MAX_DOCUMENTS`](crate::MAX_DOCUMENTS)).
    pub fn document_count(&self) -> usize {
        self.states.len()
    }

    /// `OnTestKeyDown`: whether TSF must call [`key_down`](Self::key_down) for `key` (the
    /// rules are in the [crate documentation](crate)). It never edits the document
    /// and commits no engine result; the decision is kept for `key_down`. A key from another
    /// document than the last one is a document change, though: the words of both documents
    /// end and the hotkey classifier starts again already here.
    pub fn test_key_down(&mut self, key: &KeyStroke) -> bool {
        if key.is_injected() {
            return false;
        }
        self.echo = None;
        let mut record = self.take_tested(key, true);
        let (decision, stamp) = self.decide_key_down(key, &mut record);
        let claims = decision.claims();
        if let Decision::PassEcho { document, arrival } = &decision {
            // TSF will not call `key_down`: the key goes straight to the application.
            self.echo = Some(Echo {
                document: *document,
                arrival: arrival.clone(),
            });
        }
        if let Some(stamp) = stamp {
            if record.key != *key {
                record.key = key.clone();
            }
            record.prepared = Some(Prepared { stamp, decision });
        }
        self.tested = Some(record);
        claims
    }

    /// `OnKeyDown`: handles `key` in `document`, the document of `key.host`, with at most one
    /// edit. Uses the decision of the matching test while it is current and computes the key
    /// otherwise. The next engine state is committed only after the edit succeeded; any
    /// failure resets the word and lets the key through.
    pub fn key_down(&mut self, key: &KeyStroke, document: &mut dyn DocumentPort) -> KeyDownOutcome {
        if key.is_injected() {
            return KeyDownOutcome::pass_through();
        }
        self.echo = None;
        let mut record = self.take_tested(key, true);
        match self.decide_key_down(key, &mut record).0 {
            Decision::Pass => KeyDownOutcome::pass_through(),
            Decision::PassEcho {
                document: id,
                arrival,
            } => {
                self.echo = Some(Echo {
                    document: id,
                    arrival,
                });
                KeyDownOutcome::pass_through()
            }
            Decision::Hotkey { action, eat } => {
                self.run_hotkey(action);
                KeyDownOutcome {
                    eaten: eat,
                    replay: None,
                }
            }
            Decision::Reset { document: id } => {
                self.states
                    .reset(&self.engine, id, ResetReason::ContextChanged);
                KeyDownOutcome::pass_through()
            }
            Decision::Apply {
                document: id,
                policy,
                result,
                arrival,
            } => self.apply(id, &policy, *result, arrival, document),
        }
    }

    /// `OnTestKeyUp`: whether TSF must call [`key_up`](Self::key_up) for `key`; only a
    /// hotkey or a swallowed key-up while disconnected is claimed (D24).
    pub fn test_key_up(&mut self, key: &KeyStroke) -> bool {
        if key.is_injected() {
            return false;
        }
        let mut record = self.take_tested(key, false);
        let claims = self.decide_key_up(key, &mut record).claims();
        self.tested = Some(record);
        claims
    }

    /// `OnKeyUp`: runs a hotkey that fires on this key-up and says whether the key-up is
    /// eaten (only the key-up of a consumed key hotkey is).
    pub fn key_up(&mut self, key: &KeyStroke) -> bool {
        if key.is_injected() {
            return false;
        }
        let mut record = self.take_tested(key, false);
        match self.decide_key_up(key, &mut record) {
            Decision::Hotkey { action, eat } => {
                self.run_hotkey(action);
                eat
            }
            Decision::Pass
            | Decision::PassEcho { .. }
            | Decision::Reset { .. }
            | Decision::Apply { .. } => false,
        }
    }

    /// A change of `document` that Raumakey's own edit sessions did not make, with what its
    /// edit record shows. It ends the word in progress of `document` (matrix #19), with one
    /// exception: when the last key-down was let through to the application while
    /// `document` had a word state, the first change of `document` that is exactly that key
    /// arriving keeps the word. Any other change, [`SelectionChange::Other`] above all (a
    /// click, a caret moved by the application), ends it; so does every later one.
    ///
    /// Such a change also interrupts the hotkey classifier (D24): a modifier-only hotkey
    /// held meanwhile (Ctrl+Shift+click extends the selection) does not fire.
    pub fn selection_changed(&mut self, document: DocumentId, change: SelectionChange<'_>) {
        if self
            .echo
            .take_if(|echo| echo.document == document)
            .is_some_and(|echo| echo.arrival.is(change))
        {
            return;
        }
        self.classifier.interrupt();
        self.states
            .reset(&self.engine, document, ResetReason::ContextChanged);
    }

    /// A mouse button went down in the host (matrix #19); `host` describes the context that
    /// has the focus. A click can move the caret before the host reports the move, or
    /// without the host ever reporting it, so the COM layer reports every press.
    ///
    /// The hotkey classifier is interrupted (D24): a modifier-only hotkey held meanwhile does
    /// not fire. When the policy of `host` sets
    /// [`reset_on_mouse`](raumakey_model::policy::HostPolicy::reset_on_mouse) (the default),
    /// the word in progress ends too.
    pub fn mouse_down(&mut self, host: &HostContext) {
        self.classifier.interrupt();
        if self.policy.resolve(host).reset_on_mouse {
            if let Some((_, document)) = self.active {
                self.states
                    .reset(&self.engine, document, ResetReason::Mouse);
            }
            self.echo = None;
        }
    }

    /// The focus moved to `document` (or to no document): the words of the document that
    /// lost the focus and of `document` end (matrix #20).
    pub fn set_focus(&mut self, document: Option<DocumentId>) {
        if let Some((_, previous)) = self.active.take() {
            self.states
                .reset(&self.engine, previous, ResetReason::Focus);
        }
        if let Some(document) = document {
            self.states
                .reset(&self.engine, document, ResetReason::Focus);
        }
        self.echo = None;
        self.forget_keys();
    }

    /// `document` was closed: its state is dropped.
    pub fn document_closed(&mut self, document: DocumentId) {
        self.states.remove(document);
        if self.active.is_some_and(|(_, active)| active == document) {
            self.active = None;
        }
        if self
            .echo
            .as_ref()
            .is_some_and(|echo| echo.document == document)
        {
            self.echo = None;
        }
        if self
            .tested
            .as_ref()
            .is_some_and(|tested| tested.key.host.document_id == Some(document))
        {
            self.tested = None;
        }
    }

    /// Installs a runtime snapshot from the controller (plan §7.2): options, user key maps,
    /// hotkeys, compatibility rules, and the live mode.
    ///
    /// The snapshot is checked first (see [`SnapshotError::Runtime`]); an invalid one changes
    /// nothing and the core keeps the last good configuration. When the snapshot changes how
    /// words are typed (options, key maps, rules, or mode), every word in progress ends; the
    /// same settings again, as after a reconnect, keep them. A probe result recorded through
    /// [`record_probe`](Self::record_probe) stays in effect until a snapshot carries a record
    /// of its executable, as `record_probe` describes.
    pub fn apply_runtime_snapshot(
        &mut self,
        snapshot: &RuntimeSnapshot,
    ) -> Result<(), SnapshotError> {
        check_runtime(snapshot)?;
        let next = self.config.with_runtime(snapshot);
        let keeps_words = self.config.keeps_words_of(&next);
        self.install(next);
        if !keeps_words {
            self.states
                .reset_all(&self.engine, ResetReason::ConfigChanged);
        }
        Ok(())
    }

    /// Installs a macro snapshot from the controller: [`build_macro_table`], then
    /// [`install_macro_table`](Self::install_macro_table), on the calling thread. When the
    /// snapshot is refused, nothing changes and the last good table stays.
    pub fn apply_macro_snapshot(&mut self, snapshot: &MacroSnapshot) -> Result<(), SnapshotError> {
        let table = build_macro_table(snapshot)?;
        self.install_macro_table(snapshot.macro_revision, table);
        Ok(())
    }

    /// Installs a macro table that [`build_macro_table`] made, on any thread, from the macro
    /// snapshot with `macro_revision`. Words in progress continue: only their boundary
    /// expansion uses the new table.
    pub fn install_macro_table(&mut self, macro_revision: u64, table: Arc<MacroTable>) {
        let next = self.config.with_macros(macro_revision, table);
        self.install(next);
    }

    /// Whether the COM layer should run the TSF range-edit probe in `host` and report the
    /// result with [`record_probe`](Self::record_probe) (matrix #27): `host` is a document
    /// that is not a secure field, its executable has no result of the current probe version
    /// yet, and a result could be stored.
    ///
    /// The probe edits the document, and a secure field must not be edited for anything but
    /// the typing its secure-field mode allows (D25, matrix #29), so a secure field is never
    /// probed, nor a context without a document; the next ordinary document of the same
    /// executable is.
    pub fn needs_probe(&self, host: &HostContext) -> bool {
        !host.is_secure_field && host.document_id.is_some() && self.policy.needs_probe(host)
    }

    /// Records the result of a TSF range-edit probe of `host`'s executable. Keys of that
    /// executable resolve with it at once: after a failed probe the controller's fallback
    /// transport handles the host, unless a rule selects a transport. A result that changes
    /// the resolved policy of `host` ends every word in progress, as a change of the rules
    /// does.
    ///
    /// The COM layer also reports the result to the controller (`Command::ReportProbe`),
    /// which persists it and publishes it in a later runtime snapshot. The controller
    /// resolves the host with the record it publishes, and the core must pick the same
    /// transport, so the result only stands in until a snapshot carries a record of the
    /// executable:
    ///
    /// - The core applies it again to the rules of each snapshot that carries no record of
    ///   the executable, or only one of an older probe version.
    /// - It drops the result at the first snapshot that carries a record of the current
    ///   probe version, whatever that record says (another process of the executable may
    ///   have reported it), or of a newer version, which this build ignores and cannot
    ///   replace. From then on the snapshots decide: when a later one drops the record,
    ///   [`needs_probe`](Self::needs_probe) asks for a new probe.
    ///
    /// The core and the controller can still disagree briefly while a result is on its way:
    /// until the controller has stored it, and while a snapshot published before then carries
    /// the record that the controller held until then.
    ///
    /// # Errors
    ///
    /// Those of [`CompatibilityPolicy::record_probe`], only for hosts that
    /// [`needs_probe`](Self::needs_probe) does not ask for; nothing changes then.
    pub fn record_probe(
        &mut self,
        host: &HostContext,
        probe: ProbeResult,
    ) -> Result<(), PolicyError> {
        let before = self.policy.resolve(host);
        self.policy.record_probe(host, probe)?;
        self.probes
            .retain(|(probed, _)| probed.executable != host.executable);
        if self.probes.len() >= MAX_RECORDED_PROBES {
            self.probes.remove(0);
        }
        self.probes.push((host.clone(), probe));
        if self.policy.resolve(host) != before {
            self.states
                .reset_all(&self.engine, ResetReason::ConfigChanged);
            self.echo = None;
        }
        Ok(())
    }

    /// Sets the live Vietnamese/English mode, as `ModeChanged` from the controller or a
    /// change of the keyboard open/close compartment reports it. A change ends every word in
    /// progress first (matrix #24).
    pub fn set_vietnamese_mode(&mut self, vietnamese_mode: bool) {
        if vietnamese_mode == self.config.vietnamese_mode() {
            return;
        }
        self.states.reset_all(&self.engine, ResetReason::ModeToggle);
        let next = self.config.with_mode(vietnamese_mode);
        self.install(next);
    }

    /// Records whether the controller connection is up. While it is, the controller's hook
    /// is the only hotkey classifier and the core classifies nothing (D24); a change hands
    /// classification over, so the classifier starts again from no held keys.
    pub fn set_connected(&mut self, connected: bool) {
        if self.connected != connected {
            self.connected = connected;
            self.forget_keys();
        }
    }

    /// Says whether the active keyboard layout has AltGr: right Alt then types characters
    /// and is not Alt for hotkeys (matrix #22). Keep it in step with the active layout.
    pub fn set_altgr_layout(&mut self, altgr_layout: bool) {
        self.classifier.set_altgr_layout(altgr_layout);
    }

    /// Ends the word of every document without changing committed text, as `ResetAll` from
    /// the controller or the hard-reset hotkey asks (matrix #25).
    pub fn reset_all(&mut self) {
        self.states.reset_all(&self.engine, ResetReason::HardReset);
        self.echo = None;
    }

    /// Makes `config` the configuration in effect and rebuilds what derives from the parts
    /// that changed.
    fn install(&mut self, config: RuntimeConfig) {
        if config.keymaps() != self.config.keymaps()
            || !Arc::ptr_eq(config.macros(), self.config.macros())
        {
            self.engine = build_engine(&config);
        }
        if !Arc::ptr_eq(config.compatibility(), self.config.compatibility()) {
            let mut policy = CompatibilityPolicy::new(Arc::clone(config.compatibility()));
            // A local result stands in only while the rules ask for a probe of its
            // executable: once they carry a record of it, the controller resolves the host
            // with that record, and so must the core (see `record_probe`).
            self.probes.retain(|(host, probe)| {
                policy.needs_probe(host) && policy.record_probe(host, *probe).is_ok()
            });
            self.policy = policy;
        }
        if config.hotkeys() != self.config.hotkeys() {
            let altgr_layout = self.classifier.altgr_layout();
            self.classifier = HotkeyClassifier::new(config.hotkeys());
            self.classifier.set_altgr_layout(altgr_layout);
            self.tested = None;
        }
        self.config = Arc::new(config);
    }

    /// Forgets every key transition seen so far: held modifiers and the last test.
    fn forget_keys(&mut self) {
        self.classifier.reset();
        self.tested = None;
    }

    /// The record of the last test when it is the same transition as `key`, or a fresh one.
    /// Either way the stored record is gone afterwards.
    fn take_tested(&mut self, key: &KeyStroke, down: bool) -> Tested {
        match self.tested.take() {
            Some(tested) if tested.matches(key, down) => tested,
            _ => Tested::fresh(key, down),
        }
    }

    /// Runs a hotkey action (D24, disconnected only).
    fn run_hotkey(&mut self, action: Option<HotkeyAction>) {
        match action {
            Some(HotkeyAction::ToggleMode) => {
                let toggled = !self.config.vietnamese_mode();
                self.set_vietnamese_mode(toggled);
            }
            Some(HotkeyAction::HardReset) => self.reset_all(),
            None => {}
        }
    }

    /// Makes `document` of `session` the active document and says whether it changed. A key
    /// from another document than the last one is a document change: both words end, so no
    /// edit ever reaches across documents (matrix #20) and no state is used by another owner.
    fn enter_document(&mut self, session: SessionId, document: DocumentId) -> bool {
        let owner = (session, document);
        if self.active == Some(owner) {
            return false;
        }
        if let Some((_, previous)) = self.active {
            self.states
                .reset(&self.engine, previous, ResetReason::ContextChanged);
        }
        self.states
            .reset(&self.engine, document, ResetReason::ContextChanged);
        self.active = Some(owner);
        self.echo = None;
        self.forget_keys();
        true
    }

    /// The classifier verdict of the transition in `record`, fed at most once.
    fn verdict(&mut self, key: &KeyStroke, record: &mut Tested) -> KeyVerdict {
        if let Some(verdict) = record.verdict {
            return verdict;
        }
        let verdict = self.classifier.classify(classifier_key(key), record.down);
        record.verdict = Some(verdict);
        verdict
    }

    /// The decision for a key-down, with the stamp of an engine decision (`None` for
    /// decisions made before the engine: pass-through gates and hotkeys).
    fn decide_key_down(
        &mut self,
        key: &KeyStroke,
        record: &mut Tested,
    ) -> (Decision, Option<Stamp>) {
        if let Some(document) = key.host.document_id
            && self.enter_document(key.host.session_id, document)
        {
            // The classifier started again: the transition is new to it.
            *record = Tested::fresh(key, true);
        }
        let policy = self.policy.resolve(&key.host);
        if policy.transport != Transport::Tsf {
            // The controller's hook handles this host; no key is handled twice.
            return (Decision::Pass, None);
        }
        if !self.connected
            && let Some(decision) = Decision::of_verdict(self.verdict(key, record))
        {
            return (decision, None);
        }
        let Some(document) = key.host.document_id else {
            return (Decision::Pass, None);
        };
        self.states.touch(document);
        let Some(generation) = self.states.get(document).map(EngineState::generation) else {
            return (Decision::Pass, None);
        };
        let stamp = Stamp {
            generation,
            revision: self.config.revision(),
        };
        if record.key == *key
            && let Some(prepared) = record.prepared.take_if(|prepared| prepared.stamp == stamp)
        {
            return (prepared.decision, Some(stamp));
        }
        (self.engine_decision(key, document, policy), Some(stamp))
    }

    /// Runs the engine for `key` in `document` without committing anything.
    ///
    /// A key of a secure field whose policy does not allow Vietnamese (`Bypass`, D25) never
    /// reaches the engine: it passes through untouched and ends the word, so nothing in such
    /// a field is transformed, expanded, edited, or tracked.
    fn engine_decision(
        &mut self,
        key: &KeyStroke,
        document: DocumentId,
        policy: HostPolicy,
    ) -> Decision {
        let Some(event) = key.input_event() else {
            return Decision::Pass;
        };
        let Some(prior) = self.states.get(document) else {
            return Decision::Pass;
        };
        if event.host.is_secure_field && !policy.vietnamese_allowed {
            return if is_fresh(&self.engine, prior) {
                Decision::Pass
            } else {
                Decision::Reset { document }
            };
        }
        let options = effective_options(&self.config, &policy);
        let arrival = Arrival::of(&event);
        match self.engine.process(&event, &options, prior) {
            Ok(result) if needs_key_down(&result, prior) => Decision::Apply {
                document,
                policy,
                result: Box::new(result),
                arrival,
            },
            Ok(_) if !is_fresh(&self.engine, prior) => match arrival {
                Some(arrival) => Decision::PassEcho { document, arrival },
                None => Decision::Pass,
            },
            Ok(_) => Decision::Pass,
            Err(_) if is_fresh(&self.engine, prior) => Decision::Pass,
            Err(_) => Decision::Reset { document },
        }
    }

    fn decide_key_up(&mut self, key: &KeyStroke, record: &mut Tested) -> Decision {
        if self.connected || self.policy.resolve(&key.host).transport != Transport::Tsf {
            return Decision::Pass;
        }
        Decision::of_verdict(self.verdict(key, record)).unwrap_or(Decision::Pass)
    }

    fn apply(
        &mut self,
        document: DocumentId,
        policy: &HostPolicy,
        result: EngineResult,
        arrival: Option<Arrival>,
        port: &mut dyn DocumentPort,
    ) -> KeyDownOutcome {
        let EngineResult { transaction, next } = result;
        let applied = match self.states.get(document) {
            Some(prior) => match checked_request(&transaction, prior.owned_utf16(), policy) {
                Ok(Some(request)) => port.replace_before_caret(&request).is_ok(),
                Ok(None) => true,
                Err(Rejected) => false,
            },
            None => false,
        };
        if !applied {
            self.states
                .reset(&self.engine, document, ResetReason::ApplyFailed);
            return KeyDownOutcome::pass_through();
        }
        let eaten = transaction.consume_original;
        // A key let through while the word goes on arrives as a change of the document.
        let echo = arrival.filter(|_| !eaten && !is_fresh(&self.engine, &next));
        self.states.commit(document, next);
        if let Some(arrival) = echo {
            self.echo = Some(Echo { document, arrival });
        }
        KeyDownOutcome {
            eaten,
            replay: transaction.replay_key.filter(|_| eaten),
        }
    }
}

impl Default for ServiceCore {
    fn default() -> Self {
        Self::new()
    }
}

/// Shows revisions, the mode, and counts only: the core holds the words in progress, which
/// are document text, and the last tested key.
impl fmt::Debug for ServiceCore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ServiceCore")
            .field("revision", &self.config.revision())
            .field("config_revision", &self.config.config_revision())
            .field("macro_revision", &self.config.macro_revision())
            .field("vietnamese_mode", &self.config.vietnamese_mode())
            .field("connected", &self.connected)
            .field("documents", &self.states.len())
            .finish_non_exhaustive()
    }
}

/// The code the hotkey classifier gets for a key: right Alt, reported by TSF as the generic
/// Alt code with the extended flag, becomes `RMENU`, so that AltGr is recognized on AltGr
/// layouts. Every other code is passed as it is; the classifier accepts generic codes.
fn classifier_key(key: &KeyStroke) -> u32 {
    if key.virtual_key == vk::MENU && key.extended {
        vk::RMENU
    } else {
        key.virtual_key
    }
}

/// The engine of `config`: its user key maps and macro table.
fn build_engine(config: &RuntimeConfig) -> VietnameseEngine {
    let macros: Arc<dyn MacroLookup> = Arc::clone(config.macros()) as Arc<dyn MacroLookup>;
    VietnameseEngine::new(config.keymaps().to_vec(), macros)
}

/// Options of one request: the snapshot's options with the live mode and the host policy.
fn effective_options(config: &RuntimeConfig, policy: &HostPolicy) -> EngineOptions {
    let mut options = config.options().clone();
    options.enabled = config.vietnamese_mode() && policy.vietnamese_allowed;
    options.macros_enabled = options.macros_enabled && policy.macro_allowed;
    options
}

/// True when the result edits, consumes, or replays anything, or changes the state.
fn needs_key_down(result: &EngineResult, prior: &EngineState) -> bool {
    !result.transaction.is_pass_through() || result.next.generation() != prior.generation()
}

/// True when `state` holds nothing a reset would end: no word in progress and no long word
/// being skipped until its boundary.
fn is_fresh(engine: &VietnameseEngine, state: &EngineState) -> bool {
    engine
        .reset(state, ResetReason::ContextChanged)
        .generation()
        == state.generation()
}

/// Validates `transaction` against the owned text and the host policy and builds the edit
/// request (`None` when there is nothing to edit).
fn checked_request<'a>(
    transaction: &'a EditTransaction,
    owned: &'a [u16],
    policy: &HostPolicy,
) -> Result<Option<EditRequest<'a>>, Rejected> {
    let available = u32::try_from(owned.len()).map_err(|_| Rejected)?;
    transaction
        .validate(available, policy.max_delete_before)
        .map_err(|_| Rejected)?;
    if transaction.replay_key.is_some() && !policy.permit_replay {
        return Err(Rejected);
    }
    let delete = usize::try_from(transaction.delete_before).map_err(|_| Rejected)?;
    let start = owned.len().checked_sub(delete).ok_or(Rejected)?;
    if !utf16::is_scalar_boundary(owned, start) {
        return Err(Rejected);
    }
    if delete == 0 && transaction.insert_utf16.is_empty() {
        return Ok(None);
    }
    let expected = owned.get(start..).ok_or(Rejected)?;
    Ok(Some(EditRequest::new(expected, &transaction.insert_utf16)))
}
