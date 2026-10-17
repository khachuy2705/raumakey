//! Typed policy errors. No variant carries user content.

use core::fmt;

/// Why a probe result could not be recorded.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyError {
    /// The probe cache already holds [`MAX_PROBE_RECORDS`](crate::MAX_PROBE_RECORDS) records and
    /// none of them may be evicted for the new one (see
    /// [`ProbeCache::upsert`](crate::ProbeCache::upsert)); for a current probe result this means
    /// every record comes from a newer probe version.
    ProbeCacheFull,
    /// The cache holds a record for the executable from a newer probe version; it is kept.
    NewerProbeVersion {
        /// Probe version of the stored record.
        stored: u32,
    },
}

impl fmt::Display for PolicyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ProbeCacheFull => f.write_str("probe cache is full"),
            Self::NewerProbeVersion { stored } => {
                write!(f, "a record from newer probe version {stored} is kept")
            }
        }
    }
}

impl std::error::Error for PolicyError {}
