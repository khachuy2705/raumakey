//! The immutable runtime configuration of the text service.

use core::fmt;
use std::sync::Arc;

use raumakey_ipc::error::{IpcError, MalformedReason};
use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_macro::{MacroError, MacroTable};
use raumakey_model::hotkey::HotkeyConfig;
use raumakey_model::keymap::KeyMap;
use raumakey_model::options::{EngineOptions, InputMethod};
use raumakey_model::policy::CompatibilityConfig;

/// Why a snapshot was refused. The core keeps running with the last good configuration.
/// No variant carries configuration or macro text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotError {
    /// The runtime snapshot failed
    /// [`RuntimeSnapshot::validate`](raumakey_ipc::message::RuntimeSnapshot::validate), or its
    /// options select [`InputMethod::UserDefined`] with a key map that it does not carry
    /// (reported as [`MalformedReason::InvalidKeymap`]): the engine would refuse every key.
    Runtime(IpcError),
    /// The macro snapshot failed
    /// [`MacroSnapshot::validate`](raumakey_ipc::message::MacroSnapshot::validate).
    MacroSnapshot(IpcError),
    /// An entry of the macro snapshot failed macro validation.
    MacroTable(MacroError),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Runtime(error) => write!(f, "runtime snapshot refused: {error}"),
            Self::MacroSnapshot(error) => write!(f, "macro snapshot refused: {error}"),
            Self::MacroTable(error) => write!(f, "macro snapshot refused: {error}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

/// Validates a macro snapshot and indexes its entries into a [`MacroTable`], for
/// [`ServiceCore::install_macro_table`](crate::ServiceCore::install_macro_table).
///
/// It touches no core, so the COM layer runs it on its IPC thread: a snapshot can hold
/// [`MAX_MACROS`](raumakey_ipc::message::MAX_MACROS) entries, too many to index on the host
/// application's UI thread.
///
/// # Errors
///
/// [`SnapshotError::MacroSnapshot`] when the snapshot fails
/// [`MacroSnapshot::validate`], [`SnapshotError::MacroTable`] when an entry fails macro
/// validation.
pub fn build_macro_table(snapshot: &MacroSnapshot) -> Result<Arc<MacroTable>, SnapshotError> {
    snapshot.validate().map_err(SnapshotError::MacroSnapshot)?;
    let table =
        MacroTable::new(snapshot.entries.iter().cloned()).map_err(SnapshotError::MacroTable)?;
    Ok(Arc::new(table))
}

/// Checks a runtime snapshot the way
/// [`ServiceCore::apply_runtime_snapshot`](crate::ServiceCore::apply_runtime_snapshot) does
/// before it installs one, without a core: a front end that hands snapshots to the thread of
/// its core runs it on its own thread, so that it can report a refused snapshot to the
/// caller at once.
///
/// # Errors
///
/// [`SnapshotError::Runtime`] when the core would refuse the snapshot.
pub fn check_runtime_snapshot(snapshot: &RuntimeSnapshot) -> Result<(), SnapshotError> {
    check_runtime(snapshot)
}

/// Checks a runtime snapshot before the core installs it:
/// [`RuntimeSnapshot::validate`], then that a user-defined input method names one of the
/// snapshot's key maps. `validate` only checks that an id is present.
pub(crate) fn check_runtime(snapshot: &RuntimeSnapshot) -> Result<(), SnapshotError> {
    snapshot.validate().map_err(SnapshotError::Runtime)?;
    let options = &snapshot.options;
    let carried = options
        .user_keymap_id
        .as_deref()
        .is_some_and(|id| snapshot.keymaps.iter().any(|keymap| keymap.id == id));
    if options.input_method == InputMethod::UserDefined && !carried {
        return Err(SnapshotError::Runtime(IpcError::Malformed(
            MalformedReason::InvalidKeymap,
        )));
    }
    Ok(())
}

/// The configuration one request runs with. It never changes; a new snapshot, mode change,
/// or macro table installs a new `Arc<RuntimeConfig>` with a higher
/// [`revision`](Self::revision).
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    revision: u64,
    config_revision: u64,
    macro_revision: u64,
    options: EngineOptions,
    keymaps: Vec<KeyMap>,
    hotkeys: HotkeyConfig,
    compatibility: Arc<CompatibilityConfig>,
    vietnamese_mode: bool,
    diagnostics_enabled: bool,
    macros: Arc<MacroTable>,
}

impl RuntimeConfig {
    /// The configuration used until the first snapshot arrives:
    /// [`RuntimeSnapshot::defaults_v1`] (versioned defaults v1, plan D4, D5, D13; Vietnamese
    /// mode on, no user key maps), no macros, revision 0.
    pub fn defaults_v1() -> Self {
        Self::from_snapshot(
            &RuntimeSnapshot::defaults_v1(),
            0,
            0,
            Arc::new(MacroTable::default()),
        )
    }

    /// The settings of `snapshot` at local `revision`, with the macro table of
    /// `macro_revision`.
    fn from_snapshot(
        snapshot: &RuntimeSnapshot,
        revision: u64,
        macro_revision: u64,
        macros: Arc<MacroTable>,
    ) -> Self {
        Self {
            revision,
            config_revision: snapshot.config_revision,
            macro_revision,
            options: snapshot.options.clone(),
            keymaps: snapshot.keymaps.clone(),
            hotkeys: snapshot.hotkeys.clone(),
            compatibility: Arc::new(snapshot.compatibility.clone()),
            vietnamese_mode: snapshot.vietnamese_mode,
            diagnostics_enabled: snapshot.diagnostics_enabled,
            macros,
        }
    }

    /// Local revision: it changes whenever the core installs another configuration, so a
    /// result computed with an older configuration is never applied.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// `config_revision` of the runtime snapshot in effect (0 for defaults v1).
    pub fn config_revision(&self) -> u64 {
        self.config_revision
    }

    /// `macro_revision` of the macro snapshot in effect (0 before the first one).
    pub fn macro_revision(&self) -> u64 {
        self.macro_revision
    }

    /// Engine options of the snapshot. `enabled` is replaced per request by
    /// [`vietnamese_mode`](Self::vietnamese_mode) and the host policy.
    pub fn options(&self) -> &EngineOptions {
        &self.options
    }

    /// User-defined key maps.
    pub fn keymaps(&self) -> &[KeyMap] {
        &self.keymaps
    }

    /// Configured hotkeys; the core classifies them only while disconnected (D24).
    pub fn hotkeys(&self) -> &HotkeyConfig {
        &self.hotkeys
    }

    /// Application compatibility rules.
    pub fn compatibility(&self) -> &Arc<CompatibilityConfig> {
        &self.compatibility
    }

    /// Live Vietnamese/English mode.
    pub fn vietnamese_mode(&self) -> bool {
        self.vietnamese_mode
    }

    /// The user opted into diagnostics.
    pub fn diagnostics_enabled(&self) -> bool {
        self.diagnostics_enabled
    }

    /// The macro table in effect.
    pub fn macros(&self) -> &Arc<MacroTable> {
        &self.macros
    }

    /// This configuration with the settings of a validated runtime snapshot, at the next
    /// revision; the macro table stays.
    pub(crate) fn with_runtime(&self, snapshot: &RuntimeSnapshot) -> Self {
        Self::from_snapshot(
            snapshot,
            self.revision.wrapping_add(1),
            self.macro_revision,
            Arc::clone(&self.macros),
        )
    }

    /// This configuration with another macro table, at the next revision.
    pub(crate) fn with_macros(&self, macro_revision: u64, macros: Arc<MacroTable>) -> Self {
        Self {
            revision: self.revision.wrapping_add(1),
            macro_revision,
            macros,
            ..self.clone()
        }
    }

    /// This configuration with another live mode, at the next revision.
    pub(crate) fn with_mode(&self, vietnamese_mode: bool) -> Self {
        Self {
            revision: self.revision.wrapping_add(1),
            vietnamese_mode,
            ..self.clone()
        }
    }

    /// True when words typed under `self` may continue under `other`: the engine options,
    /// the key maps, the compatibility rules, and the mode are the same.
    pub(crate) fn keeps_words_of(&self, other: &Self) -> bool {
        self.options == other.options
            && self.keymaps == other.keymaps
            && self.compatibility == other.compatibility
            && self.vietnamese_mode == other.vietnamese_mode
    }
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self::defaults_v1()
    }
}
