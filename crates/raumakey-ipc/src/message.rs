//! Control-channel messages (plan §7.2).
//!
//! The JSON body of every frame is the internally tagged [`Message`] enum (`"type"` tag). No
//! message carries keystrokes or document text: the channel transports configuration, macro
//! snapshots, control commands, and opt-in typed diagnostics only.

use core::fmt;
use std::io;

use raumakey_model::diag::DiagEvent;
use raumakey_model::host::ExeName;
use raumakey_model::hotkey::{Hotkey, HotkeyConfig};
use raumakey_model::keymap::KeyMap;
use raumakey_model::macros::MacroEntry;
use raumakey_model::options::EngineOptions;
use raumakey_model::policy::{CompatibilityConfig, ProbeRecord};
use serde::{Deserialize, Serialize};

use crate::error::{ErrorCode, IpcError, Limit, MalformedReason};
use crate::frame::{MAX_CONTROL_PAYLOAD, MAX_PAYLOAD};

/// Most macro entries in one [`MacroSnapshot`].
pub const MAX_MACROS: usize = 20_000;
/// Most key maps in one [`RuntimeSnapshot`].
pub const MAX_KEYMAPS: usize = 64;
/// Most hotkeys bound to one action in a [`RuntimeSnapshot`].
pub const MAX_HOTKEYS: usize = 16;
/// Most application rules in a [`RuntimeSnapshot`].
pub const MAX_APP_RULES: usize = 4_096;
/// Most named sets in a [`RuntimeSnapshot`].
pub const MAX_NAMED_SETS: usize = 256;
/// Most members of one named set.
pub const MAX_NAMED_SET_MEMBERS: usize = 4_096;
/// Most probe records in a [`RuntimeSnapshot`].
pub const MAX_PROBES: usize = 4_096;

/// Wire number of a message kind (frame header offset 6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum MessageKind {
    /// Client greeting.
    Hello = 1,
    /// Server reply to `Hello` with the assigned client id.
    Welcome = 2,
    /// Configuration snapshot.
    RuntimeSnapshot = 3,
    /// Macro table snapshot.
    MacroSnapshot = 4,
    /// Client command.
    Command = 5,
    /// Server reply to a command.
    CommandResult = 6,
    /// Vietnamese/English mode changed.
    ModeChanged = 7,
    /// Reset every engine session.
    ResetAll = 8,
    /// Opt-in typed diagnostic event.
    Diagnostic = 9,
    /// Liveness probe.
    Ping = 10,
    /// Reply to `Ping`.
    Pong = 11,
    /// Protocol error preceding a close.
    Error = 12,
    /// Orderly close.
    Goodbye = 13,
}

impl MessageKind {
    /// Every kind, in wire order.
    pub const ALL: [Self; 13] = [
        Self::Hello,
        Self::Welcome,
        Self::RuntimeSnapshot,
        Self::MacroSnapshot,
        Self::Command,
        Self::CommandResult,
        Self::ModeChanged,
        Self::ResetAll,
        Self::Diagnostic,
        Self::Ping,
        Self::Pong,
        Self::Error,
        Self::Goodbye,
    ];

    /// Wire number.
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Parses a wire number.
    pub const fn from_u16(raw: u16) -> Option<Self> {
        Some(match raw {
            1 => Self::Hello,
            2 => Self::Welcome,
            3 => Self::RuntimeSnapshot,
            4 => Self::MacroSnapshot,
            5 => Self::Command,
            6 => Self::CommandResult,
            7 => Self::ModeChanged,
            8 => Self::ResetAll,
            9 => Self::Diagnostic,
            10 => Self::Ping,
            11 => Self::Pong,
            12 => Self::Error,
            13 => Self::Goodbye,
            _ => return None,
        })
    }

    /// Largest payload a frame of this kind may carry: [`MAX_PAYLOAD`] for the two snapshots,
    /// [`MAX_CONTROL_PAYLOAD`] for every other kind. Encoder and decoder both enforce it; the
    /// decoder does so from the header, before it reserves memory.
    pub const fn max_payload(self) -> u32 {
        match self {
            Self::RuntimeSnapshot | Self::MacroSnapshot => MAX_PAYLOAD,
            Self::Hello
            | Self::Welcome
            | Self::Command
            | Self::CommandResult
            | Self::ModeChanged
            | Self::ResetAll
            | Self::Diagnostic
            | Self::Ping
            | Self::Pong
            | Self::Error
            | Self::Goodbye => MAX_CONTROL_PAYLOAD,
        }
    }

    /// JSON `type` tag of the kind.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::Hello => "hello",
            Self::Welcome => "welcome",
            Self::RuntimeSnapshot => "runtime_snapshot",
            Self::MacroSnapshot => "macro_snapshot",
            Self::Command => "command",
            Self::CommandResult => "command_result",
            Self::ModeChanged => "mode_changed",
            Self::ResetAll => "reset_all",
            Self::Diagnostic => "diagnostic",
            Self::Ping => "ping",
            Self::Pong => "pong",
            Self::Error => "error",
            Self::Goodbye => "goodbye",
        }
    }

    /// True when a client (TIP or tool) may send this kind.
    pub const fn sent_by_client(self) -> bool {
        matches!(
            self,
            Self::Hello
                | Self::Command
                | Self::Diagnostic
                | Self::Ping
                | Self::Pong
                | Self::Error
                | Self::Goodbye
        )
    }

    /// True when the server (controller) may send this kind.
    pub const fn sent_by_server(self) -> bool {
        matches!(
            self,
            Self::Welcome
                | Self::RuntimeSnapshot
                | Self::MacroSnapshot
                | Self::CommandResult
                | Self::ModeChanged
                | Self::ResetAll
                | Self::Ping
                | Self::Pong
                | Self::Error
                | Self::Goodbye
        )
    }
}

/// Kind of client process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClientKind {
    /// The TSF text input processor loaded into an application.
    Tip,
    /// A command-line or test tool.
    Tool,
}

/// Processor architecture of the client binary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Arch {
    /// 32-bit x86.
    X86,
    /// x86-64.
    X64,
    /// ARM64.
    Arm64,
    /// Any other architecture (tests on non-Windows hosts).
    Other,
}

impl Arch {
    /// Architecture of the running binary.
    pub const fn current() -> Self {
        if cfg!(target_arch = "x86") {
            Self::X86
        } else if cfg!(target_arch = "x86_64") {
            Self::X64
        } else if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else {
            Self::Other
        }
    }
}

/// A control command sent by a client.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// Toggle Vietnamese/English mode.
    ToggleMode,
    /// Set Vietnamese mode on (`true`) or off.
    SetMode(bool),
    /// Reset every engine session without changing committed text.
    HardReset,
    /// Report a TSF capability probe for persistence.
    ReportProbe(ProbeRecord),
}

/// Immutable configuration published by the controller.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeSnapshot {
    /// Revision of `config.toml` that produced this snapshot.
    pub config_revision: u64,
    /// Engine options.
    pub options: EngineOptions,
    /// User-defined key maps.
    pub keymaps: Vec<KeyMap>,
    /// Hotkeys.
    pub hotkeys: HotkeyConfig,
    /// Application compatibility rules. Decoded through a wire form that does not buffer the
    /// keys of each rule (the model's `AppRule` flattens its overrides).
    #[serde(deserialize_with = "crate::wire::compatibility")]
    pub compatibility: CompatibilityConfig,
    /// Live Vietnamese mode.
    pub vietnamese_mode: bool,
    /// The user opted into diagnostics; clients send `Diagnostic` messages only when set.
    #[serde(default)]
    pub diagnostics_enabled: bool,
}

impl RuntimeSnapshot {
    /// The snapshot a client uses before it receives one: versioned defaults v1, revision 0.
    pub fn defaults_v1() -> Self {
        let options = EngineOptions::defaults_v1();
        Self {
            config_revision: 0,
            vietnamese_mode: options.enabled,
            options,
            keymaps: Vec::new(),
            hotkeys: HotkeyConfig::default(),
            compatibility: CompatibilityConfig::defaults_v1(),
            diagnostics_enabled: false,
        }
    }

    /// Checks entry-count limits first, then option and key map invariants, then that every
    /// hotkey and executable name reads back as itself
    /// ([`MalformedReason::InvalidHotkey`], [`MalformedReason::InvalidExeName`]), so that a
    /// snapshot that passes also decodes on every client.
    pub fn validate(&self) -> Result<(), IpcError> {
        Limit::Keymaps.check(self.keymaps.len())?;
        Limit::Hotkeys.check(self.hotkeys.toggle_mode.len())?;
        Limit::Hotkeys.check(self.hotkeys.hard_reset.len())?;
        let compatibility = &self.compatibility;
        Limit::AppRules.check(compatibility.rules.len())?;
        Limit::NamedSets.check(compatibility.named_sets.len())?;
        for set in &compatibility.named_sets {
            Limit::NamedSetMembers.check(set.members.len())?;
        }
        Limit::Probes.check(compatibility.probes.len())?;
        self.options
            .validate()
            .map_err(|_| IpcError::Malformed(MalformedReason::InvalidOptions))?;
        for keymap in &self.keymaps {
            keymap
                .validate()
                .map_err(|_| IpcError::Malformed(MalformedReason::InvalidKeymap))?;
        }
        let hotkeys = &self.hotkeys;
        for hotkey in hotkeys.toggle_mode.iter().chain(&hotkeys.hard_reset) {
            check_hotkey(hotkey)?;
        }
        for rule in &compatibility.rules {
            check_exe(&rule.exe)?;
        }
        for member in compatibility.named_sets.iter().flat_map(|set| &set.members) {
            check_exe(member)?;
        }
        for probe in &compatibility.probes {
            check_exe(&probe.exe)?;
        }
        Ok(())
    }

    /// Length in bytes of the JSON body that carries this snapshot, counted without building
    /// it.
    pub fn encoded_len(&self) -> Result<u64, IpcError> {
        body_len(&MessageRef::RuntimeSnapshot(self))
    }

    /// Checks that one frame can carry the snapshot and that clients can decode it:
    /// [`RuntimeSnapshot::validate`], then a body of at most [`MAX_PAYLOAD`] bytes. The
    /// controller runs this before it commits a configuration change, so that a change the
    /// channel cannot publish is refused with a typed error instead of leaving clients on an
    /// older snapshot.
    pub fn check_publishable(&self) -> Result<(), IpcError> {
        self.validate()?;
        check_body_len(self.encoded_len()?)
    }
}

/// Immutable macro table published by the controller.
///
/// `Debug` prints only the revision and the entry count so that macro triggers and
/// replacements never reach a log through formatting.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroSnapshot {
    /// Revision of `macros.toml` that produced this snapshot.
    pub macro_revision: u64,
    /// Entries (NFC). Semantic validation happens when `raumakey-macro` builds a table.
    pub entries: Vec<MacroEntry>,
}

impl MacroSnapshot {
    /// A snapshot with no entries.
    pub const fn empty(macro_revision: u64) -> Self {
        Self {
            macro_revision,
            entries: Vec::new(),
        }
    }

    /// Checks the entry-count limit.
    pub fn validate(&self) -> Result<(), IpcError> {
        Limit::Macros.check(self.entries.len())
    }

    /// Length in bytes of the JSON body that carries this snapshot, counted without building
    /// it. Replacements may hold up to 4,096 scalars each, so the entry limit alone does not
    /// keep a table within one frame.
    pub fn encoded_len(&self) -> Result<u64, IpcError> {
        body_len(&MessageRef::MacroSnapshot(self))
    }

    /// Checks that one frame can carry the table: at most [`MAX_MACROS`] entries and a body
    /// of at most [`MAX_PAYLOAD`] bytes. The controller runs this before it commits a macro
    /// import or edit and shows the typed error ([`IpcError::LimitExceeded`] or
    /// [`IpcError::Oversize`] with the full body length) instead of storing a table that
    /// cannot be published.
    pub fn check_publishable(&self) -> Result<(), IpcError> {
        self.validate()?;
        check_body_len(self.encoded_len()?)
    }
}

impl fmt::Debug for MacroSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MacroSnapshot")
            .field("macro_revision", &self.macro_revision)
            .field("entries", &format_args!("<{} entries>", self.entries.len()))
            .finish()
    }
}

/// Body of one frame.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Message {
    /// Client greeting; the frame's client id is zero.
    Hello {
        /// Client process kind.
        client_kind: ClientKind,
        /// Client process id.
        pid: u32,
        /// Client binary architecture.
        arch: Arch,
        /// Protocol version the client speaks.
        protocol: u16,
    },
    /// Server reply to `Hello`.
    Welcome {
        /// Identifier every later frame of this connection carries.
        client_id: u64,
        /// Server state revision at the time of the handshake.
        server_revision: u64,
    },
    /// Configuration snapshot.
    RuntimeSnapshot(RuntimeSnapshot),
    /// Macro snapshot.
    MacroSnapshot(MacroSnapshot),
    /// Client command; answered by `CommandResult` with the same request id.
    Command {
        /// The command.
        command: Command,
    },
    /// Outcome of a command.
    CommandResult {
        /// The command succeeded (exactly when `code` is [`ErrorCode::OK`]).
        ok: bool,
        /// Result code.
        code: ErrorCode,
    },
    /// Vietnamese/English mode changed.
    ModeChanged {
        /// New mode.
        vietnamese_mode: bool,
        /// Server state revision of the change.
        revision: u64,
    },
    /// Reset every engine session without changing committed text.
    ResetAll,
    /// Opt-in typed diagnostic event (no free-form text).
    Diagnostic(DiagEvent),
    /// Liveness probe; answered by `Pong` with the same request id.
    Ping,
    /// Reply to `Ping`.
    Pong,
    /// Protocol error; the sender closes the connection afterwards.
    Error {
        /// Error code.
        code: ErrorCode,
    },
    /// Orderly close.
    Goodbye,
}

/// Borrowed form of the snapshot variants, serialized exactly like [`Message`] so that a
/// published snapshot is encoded once without being cloned.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum MessageRef<'a> {
    RuntimeSnapshot(&'a RuntimeSnapshot),
    MacroSnapshot(&'a MacroSnapshot),
}

/// Counts the bytes of the JSON body of `body` without storing them.
fn body_len(body: &MessageRef<'_>) -> Result<u64, IpcError> {
    let mut counter = ByteCounter(0);
    serde_json::to_writer(&mut counter, body)
        .map_err(|_| IpcError::Malformed(MalformedReason::Json))?;
    Ok(counter.0)
}

/// Hotkeys travel in their text form (`Display`), which the peer parses strictly. A hotkey the
/// parser would reject or read as another hotkey (a modifier-only hotkey with fewer than two
/// modifiers, a key without a modifier, a modifier or a code outside `1..=0xFE` as the key) is
/// refused.
fn check_hotkey(hotkey: &Hotkey) -> Result<(), IpcError> {
    match hotkey.to_string().parse::<Hotkey>() {
        Ok(parsed) if parsed == *hotkey => Ok(()),
        _ => Err(IpcError::Malformed(MalformedReason::InvalidHotkey)),
    }
}

/// Executable names travel as their string and are validated again when read.
/// `ExeName::new` checks the length before it lowercases, and lowercasing can lengthen a name
/// (`İ` becomes two characters), so a stored name may fail its own validation.
fn check_exe(exe: &ExeName) -> Result<(), IpcError> {
    match ExeName::new(exe.as_str()) {
        Ok(read_back) if read_back == *exe => Ok(()),
        _ => Err(IpcError::Malformed(MalformedReason::InvalidExeName)),
    }
}

fn check_body_len(len: u64) -> Result<(), IpcError> {
    if len > u64::from(MAX_PAYLOAD) {
        Err(IpcError::Oversize {
            payload_len: len,
            max: MAX_PAYLOAD,
        })
    } else {
        Ok(())
    }
}

/// An [`io::Write`] that only counts.
struct ByteCounter(u64);

impl io::Write for ByteCounter {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        let len = u64::try_from(data.len()).unwrap_or(u64::MAX);
        self.0 = self.0.saturating_add(len);
        Ok(data.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Message {
    /// The wire kind of this message.
    pub const fn kind(&self) -> MessageKind {
        match self {
            Self::Hello { .. } => MessageKind::Hello,
            Self::Welcome { .. } => MessageKind::Welcome,
            Self::RuntimeSnapshot(_) => MessageKind::RuntimeSnapshot,
            Self::MacroSnapshot(_) => MessageKind::MacroSnapshot,
            Self::Command { .. } => MessageKind::Command,
            Self::CommandResult { .. } => MessageKind::CommandResult,
            Self::ModeChanged { .. } => MessageKind::ModeChanged,
            Self::ResetAll => MessageKind::ResetAll,
            Self::Diagnostic(_) => MessageKind::Diagnostic,
            Self::Ping => MessageKind::Ping,
            Self::Pong => MessageKind::Pong,
            Self::Error { .. } => MessageKind::Error,
            Self::Goodbye => MessageKind::Goodbye,
        }
    }

    /// Validates entry-count limits and field invariants, including that hotkeys and
    /// executable names read back as themselves. Runs on both encode and decode, so that a
    /// message the encoder accepts also decodes on the peer.
    pub fn validate(&self) -> Result<(), IpcError> {
        match self {
            Self::Welcome { client_id: 0, .. } => {
                Err(IpcError::Malformed(MalformedReason::InvalidClientId))
            }
            Self::RuntimeSnapshot(snapshot) => snapshot.validate(),
            Self::MacroSnapshot(snapshot) => snapshot.validate(),
            Self::Command {
                command: Command::ReportProbe(probe),
            } => check_exe(&probe.exe),
            Self::CommandResult { ok, code } if *ok != code.is_ok() => {
                Err(IpcError::Malformed(MalformedReason::InconsistentResult))
            }
            Self::Diagnostic(DiagEvent {
                host_exe: Some(exe),
                ..
            }) => check_exe(exe),
            _ => Ok(()),
        }
    }
}
