//! Typed diagnostics. There is deliberately no field that accepts free-form text, so user
//! keystrokes, document text, clipboard data, and macro content cannot be logged through it.

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::host::ExeName;

/// Component that produced a diagnostic.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Component {
    /// Engine.
    Engine,
    /// TSF text input processor.
    Tsf,
    /// Controller.
    Controller,
    /// Configuration store.
    Config,
    /// Macro store.
    Macro,
    /// Policy resolution.
    Policy,
    /// IPC.
    Ipc,
    /// Converter.
    Converter,
    /// Migration.
    Migrate,
    /// Installer.
    Installer,
}

impl Component {
    /// Stable lowercase name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Engine => "engine",
            Self::Tsf => "tsf",
            Self::Controller => "controller",
            Self::Config => "config",
            Self::Macro => "macro",
            Self::Policy => "policy",
            Self::Ipc => "ipc",
            Self::Converter => "converter",
            Self::Migrate => "migrate",
            Self::Installer => "installer",
        }
    }
}

/// Stable numeric diagnostic code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DiagCode(pub u32);

/// Test vector identifier `[A-Za-z0-9._-]{1,32}`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct VectorId(String);

/// Rejected vector identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VectorIdError;

impl fmt::Display for VectorIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid test vector id")
    }
}

impl std::error::Error for VectorIdError {}

impl VectorId {
    /// Validates an identifier.
    pub fn new(text: &str) -> Result<Self, VectorIdError> {
        let ok = !text.is_empty()
            && text.len() <= 32
            && text
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
        if ok {
            Ok(Self(text.to_owned()))
        } else {
            Err(VectorIdError)
        }
    }

    /// The identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for VectorId {
    type Error = VectorIdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(&value)
    }
}

impl From<VectorId> for String {
    fn from(value: VectorId) -> Self {
        value.0
    }
}

/// One diagnostic record.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagEvent {
    /// Milliseconds since the Unix epoch (or a monotonic origin for in-process logs).
    pub timestamp_ms: u64,
    /// Component.
    pub component: Component,
    /// Code.
    pub code: DiagCode,
    /// Configuration or macro revision, when relevant.
    pub revision: Option<u64>,
    /// Host executable basename, when relevant.
    pub host_exe: Option<ExeName>,
    /// Test vector, when running acceptance vectors.
    pub vector_id: Option<VectorId>,
}

impl DiagEvent {
    /// Stable single-line form: `<ts> <component> <code> rev=<n|-> exe=<name|-> vec=<id|->`.
    pub fn to_log_line(&self) -> String {
        let revision = self
            .revision
            .map_or_else(|| "-".to_owned(), |r| r.to_string());
        let exe = self.host_exe.as_ref().map_or("-", ExeName::as_str);
        let vector = self.vector_id.as_ref().map_or("-", VectorId::as_str);
        format!(
            "{} {} {} rev={} exe={} vec={}",
            self.timestamp_ms,
            self.component.as_str(),
            self.code.0,
            revision,
            exe,
            vector
        )
    }
}
