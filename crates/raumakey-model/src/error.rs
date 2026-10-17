//! Engine error and reset reason types (clean-room spec §4).

use core::fmt;

/// Typed engine failure. None of the variants carries user text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EngineError {
    /// Event text is not well-formed UTF-16.
    InvalidUtf16,
    /// The prior state was produced by an incompatible engine version.
    InvalidStateVersion,
    /// The prior state belongs to a different session or document.
    ContextMismatch,
    /// Input exceeded a fixed capacity.
    CapacityExceeded,
    /// The requested input method or user key map is unavailable.
    UnsupportedInputMethod,
    /// An internal invariant was violated (adapter or engine bug).
    InternalInvariant,
}

impl fmt::Display for EngineError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidUtf16 => "event text is not well-formed UTF-16",
            Self::InvalidStateVersion => "engine state version is not supported",
            Self::ContextMismatch => "engine state belongs to another context",
            Self::CapacityExceeded => "engine capacity exceeded",
            Self::UnsupportedInputMethod => "input method is not available",
            Self::InternalInvariant => "internal invariant violated",
        })
    }
}

impl std::error::Error for EngineError {}

/// Why engine state was reset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResetReason {
    /// Caret moved by a navigation key.
    Navigation,
    /// Mouse click in the document.
    Mouse,
    /// Focus moved to another field or window.
    Focus,
    /// Vietnamese/English mode toggled.
    ModeToggle,
    /// Configuration changed.
    ConfigChanged,
    /// Component shutting down.
    Shutdown,
    /// Hard reset hotkey.
    HardReset,
    /// The adapter could not apply a transaction.
    ApplyFailed,
    /// The document or context changed under the composition.
    ContextChanged,
    /// The current word was committed at a boundary.
    WordCommitted,
}
