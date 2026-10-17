//! Macro types and the lookup port used by the engine (clean-room spec §4).

use serde::{Deserialize, Serialize};

/// Stable macro identifier.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MacroId(pub u64);

/// Which boundaries expand an entry.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BoundaryPolicy {
    /// Whitespace and punctuation.
    #[default]
    Delimiter,
    /// Whitespace only.
    Whitespace,
}

/// Whether the replacement follows the typed trigger's case.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CasePolicy {
    /// Adapt to the typed trigger when automatic case is enabled.
    #[default]
    Adapt,
    /// Always insert as stored.
    Exact,
}

/// A macro entry. Trigger and replacement are stored NFC.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MacroEntry {
    /// Stable identifier.
    pub id: MacroId,
    /// Trigger text (NFC).
    pub trigger: String,
    /// Replacement text (NFC).
    pub replacement: String,
    /// Entry enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Boundary policy.
    #[serde(default)]
    pub boundary: BoundaryPolicy,
    /// Case policy.
    #[serde(default, rename = "case")]
    pub case_policy: CasePolicy,
}

fn default_true() -> bool {
    true
}

/// The boundary that ended the typed word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Boundary {
    /// Space, tab, or enter.
    Whitespace,
    /// Punctuation.
    Punctuation,
}

/// Case handling requested by the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CaseMode {
    /// Match the trigger exactly and insert as stored.
    Exact,
    /// Match case-insensitively and adapt the replacement case.
    Adaptive,
}

/// Case shape of a typed trigger.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CasePattern {
    /// all lowercase
    Lower,
    /// First letter uppercase, rest lowercase
    Title,
    /// ALL UPPERCASE
    Upper,
    /// Anything else
    Mixed,
}

/// A resolved expansion (NFC, case already adapted).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MacroExpansion {
    /// Matched entry.
    pub id: MacroId,
    /// Text to insert.
    pub replacement: String,
}

/// Immutable macro lookup used by the engine for one request.
pub trait MacroLookup: Send + Sync {
    /// Looks up the typed word (NFC) at a boundary.
    fn lookup(
        &self,
        typed: &str,
        boundary: Boundary,
        case_mode: CaseMode,
    ) -> Option<MacroExpansion>;
}

/// A lookup with no entries.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoMacros;

impl MacroLookup for NoMacros {
    fn lookup(
        &self,
        _typed: &str,
        _boundary: Boundary,
        _case_mode: CaseMode,
    ) -> Option<MacroExpansion> {
        None
    }
}
