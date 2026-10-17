//! The `HostContext` of a key: the activation's session, the process, the document of the
//! context, its input scope, and the keyboard layout.

use raumakey_model::host::{ExeName, HostContext, SessionId};

use crate::adapter::documents::ContextInfo;

/// The executable name of a process whose image name cannot be read. No file can have this
/// name, so no rule written for a real executable applies to it.
pub const UNKNOWN_EXECUTABLE: &str = "<unknown>";

/// The session id of `activation` (a counter of the process) in process `process_id`: every
/// activation partitions the engine states of its documents anew.
pub fn session_id(process_id: u32, activation: u64) -> SessionId {
    SessionId::from_u128((u128::from(process_id) << 64) | u128::from(activation))
}

/// The lowercase file name of the process image at `path` (`ExeName`), or
/// [`UNKNOWN_EXECUTABLE`] when there is no path or no valid file name. `None` only if
/// [`UNKNOWN_EXECUTABLE`] were not a valid name, which a test rules out.
pub fn executable_name(path: Option<&str>) -> Option<ExeName> {
    path.and_then(|path| ExeName::from_path(path).ok())
        .or_else(|| ExeName::new(UNKNOWN_EXECUTABLE).ok())
}

/// What an activation knows about its process and thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFacts {
    /// The session of the activation ([`session_id`]).
    pub session_id: SessionId,
    /// The process id.
    pub process_id: u64,
    /// The id of the TSF thread.
    pub thread_id: u64,
    /// The executable of the process ([`executable_name`]).
    pub executable: ExeName,
    /// Whether the process is elevated.
    pub is_elevated: bool,
}

impl HostFacts {
    /// The host context of a key in `document` (the context of the key, `None` without one),
    /// with the KLID of the active keyboard layout. The host supports TSF range edits: the
    /// edit session is the transport of a TSF text service, and a host whose edits fail is
    /// to be handed to the controller through the policy's probe records, which the probe
    /// (not done yet) reports.
    pub fn context(&self, document: Option<&ContextInfo>, keyboard_layout: &str) -> HostContext {
        let mut host = HostContext::new(self.session_id, self.executable.clone());
        host.process_id = self.process_id;
        host.thread_id = self.thread_id;
        host.is_elevated = self.is_elevated;
        host.supports_tsf_range_edit = true;
        keyboard_layout.clone_into(&mut host.keyboard_layout);
        if let Some(document) = document {
            host.document_id = Some(document.id);
            host.is_secure_field = document.is_secure_field();
            host.window_class.clone_from(&document.window_class);
        }
        host
    }
}
