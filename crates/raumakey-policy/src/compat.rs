//! Host policy resolution (plan §6, clean-room spec §4, design §5.3).

use std::sync::Arc;

use raumakey_model::host::HostContext;
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, HostPolicy, PROBE_VERSION, ProbeRecord, ProbeResult, RuleAction,
    SecureFieldMode, Transport,
};

use crate::{PolicyError, ProbeCache};

/// Resolves the [`HostPolicy`] of a host from an immutable [`CompatibilityConfig`] snapshot and
/// the in-memory probe cache.
///
/// Resolution is a pure function of the configuration, the probe cache, and the host's
/// executable, window class, `supports_tsf_range_edit`, and `is_secure_field`; it never panics.
/// It does not consult `is_elevated`: whether synthetic keys or the clipboard can reach an
/// elevated host depends on the integrity level of the component that injects them, which only
/// that adapter knows. The steps, in order:
///
/// 1. **Base defaults:** TSF, `default_reset_on_mouse`, replay permitted,
///    `default_max_delete_before`, macros and Vietnamese allowed, the configured secure-field
///    mode.
/// 2. **Overrides,** most specific first, where the first source that sets a field decides it
///    and later sources only fill fields that are still unset: app rules for the executable
///    whose `window_class` equals the host's (case-insensitive), then app rules for the
///    executable without a window class, then named sets that list the executable, in declared
///    order. Rules of equal specificity apply in declared order. A rule with a window class
///    never matches a host whose class is unknown.
/// 3. **Capability:** when no rule or named set selected a transport explicitly, a host without
///    TSF range edits, or whose current-version probe failed, gets
///    [`Transport::SyntheticKey`].
/// 4. **Clipboard opt-in:** [`Transport::Clipboard`] becomes [`Transport::SyntheticKey`] unless
///    `clipboard_enabled`.
/// 5. **Secure field:** macros are disabled and clipboard becomes synthetic keys; in
///    [`SecureFieldMode::Bypass`] Vietnamese transformation is disabled as well, while
///    [`SecureFieldMode::Restricted`] keeps it.
#[derive(Clone, Debug)]
pub struct CompatibilityPolicy {
    config: Arc<CompatibilityConfig>,
    probes: ProbeCache,
}

impl CompatibilityPolicy {
    /// Creates a policy over a configuration snapshot. The probe cache starts from
    /// `config.probes`, normalized by [`ProbeCache::from_records`].
    pub fn new(config: Arc<CompatibilityConfig>) -> Self {
        let probes = ProbeCache::from_records(config.probes.iter().cloned());
        Self { config, probes }
    }

    /// The configuration snapshot. Its `probes` field is not updated by
    /// [`record_probe`](Self::record_probe); see [`probes`](Self::probes).
    pub fn config(&self) -> &Arc<CompatibilityConfig> {
        &self.config
    }

    /// The probe records in effect, which the controller persists in `app_rules.toml`.
    pub fn probes(&self) -> &[ProbeRecord] {
        self.probes.records()
    }

    /// The probe cache in effect.
    pub fn probe_cache(&self) -> &ProbeCache {
        &self.probes
    }

    /// A copy of the configuration whose `probes` are the records in effect, ready to persist.
    pub fn to_config(&self) -> CompatibilityConfig {
        CompatibilityConfig {
            probes: self.probes.records().to_vec(),
            ..(*self.config).clone()
        }
    }

    /// Records the outcome of a TSF range-edit probe of `host`'s executable as a
    /// [`PROBE_VERSION`] record, replacing any older record of that executable. Later
    /// resolutions use it immediately. When the cache is full, the record takes the place of
    /// the least useful one (see [`ProbeCache::upsert`]). Records of a newer probe version are
    /// never evicted, so the cap of [`MAX_PROBE_RECORDS`](crate::MAX_PROBE_RECORDS) records
    /// keeps a result from taking effect only when every cached record comes from a newer probe
    /// version; [`needs_probe`](Self::needs_probe) does not ask for a probe then.
    ///
    /// # Errors
    ///
    /// Only for hosts that [`needs_probe`](Self::needs_probe) does not ask for; nothing changes
    /// in either case, and the host keeps resolving as if it had not been probed:
    ///
    /// - [`PolicyError::NewerProbeVersion`] when the executable's record comes from a newer
    ///   probe version, which is kept.
    /// - [`PolicyError::ProbeCacheFull`] when the executable is new and every cached record comes
    ///   from a newer probe version.
    pub fn record_probe(
        &mut self,
        host: &HostContext,
        probe: ProbeResult,
    ) -> Result<(), PolicyError> {
        self.probes.upsert(ProbeRecord {
            exe: host.executable.clone(),
            probe_version: PROBE_VERSION,
            tsf_range_edit: probe == ProbeResult::TsfRangeEditOk,
        })
    }

    /// True when an adapter should probe `host`'s executable and report the result with
    /// [`record_probe`](Self::record_probe): the executable has no record of the current
    /// [`PROBE_VERSION`] (matrix #27: it is not cached yet, or only under an older probe
    /// version), and a result can be stored.
    ///
    /// False when a result could not be stored, so that the host is not probed again on every
    /// activation: the executable's record comes from a newer probe version (after a downgrade),
    /// or the cache is full of such records. This build then resolves the host without probe
    /// data: TSF, unless the host reports no TSF range edits or a rule selects a transport.
    pub fn needs_probe(&self, host: &HostContext) -> bool {
        self.probes.current(&host.executable).is_none()
            && self.probes.can_store_current(&host.executable)
    }

    /// Resolves the policy for `host` (see the type documentation for the order of steps).
    pub fn resolve(&self, host: &HostContext) -> HostPolicy {
        let config = &*self.config;
        let overrides = self.overrides(host);
        let mut policy = HostPolicy {
            transport: overrides.transport.unwrap_or(Transport::Tsf),
            reset_on_mouse: overrides
                .reset_on_mouse
                .unwrap_or(config.default_reset_on_mouse),
            permit_replay: true,
            max_delete_before: overrides
                .max_delete_before
                .unwrap_or(config.default_max_delete_before),
            macro_allowed: overrides.macros.unwrap_or(true),
            secure_field_mode: config.secure_field_mode,
            vietnamese_allowed: overrides.vietnamese.unwrap_or(true),
        };
        if overrides.transport.is_none() && !self.tsf_range_edit_works(host) {
            policy.transport = Transport::SyntheticKey;
        }
        if policy.transport == Transport::Clipboard && !config.clipboard_enabled {
            policy.transport = Transport::SyntheticKey;
        }
        if host.is_secure_field {
            policy.macro_allowed = false;
            if policy.transport == Transport::Clipboard {
                policy.transport = Transport::SyntheticKey;
            }
            if config.secure_field_mode == SecureFieldMode::Bypass {
                policy.vietnamese_allowed = false;
            }
        }
        policy
    }

    /// False when the host reports no TSF range edits or its current-version probe failed.
    fn tsf_range_edit_works(&self, host: &HostContext) -> bool {
        let probe_failed = self
            .probes
            .current(&host.executable)
            .is_some_and(|record| !record.tsf_range_edit);
        host.supports_tsf_range_edit && !probe_failed
    }

    /// Merges every action that applies to `host`, most specific first: rules narrowed to the
    /// host's window class, then exe-only rules, then named sets in declared order. The first
    /// source that sets a field explicitly decides it.
    fn overrides(&self, host: &HostContext) -> RuleAction {
        let config = &*self.config;
        let exe_rules = || config.rules.iter().filter(|r| r.exe == host.executable);
        let class_rules = exe_rules()
            .filter(|r| class_matches(r, host))
            .map(|r| &r.action);
        let exe_only_rules = exe_rules()
            .filter(|r| r.window_class.is_none())
            .map(|r| &r.action);
        let sets = config
            .named_sets
            .iter()
            .filter(|s| s.members.contains(&host.executable))
            .map(|s| &s.action);
        class_rules
            .chain(exe_only_rules)
            .chain(sets)
            .fold(RuleAction::default(), fill_unset)
    }
}

/// True when `rule` is narrowed to a window class and the host reports that class. Windows
/// class names are case-insensitive.
fn class_matches(rule: &AppRule, host: &HostContext) -> bool {
    match (&rule.window_class, &host.window_class) {
        (Some(wanted), Some(actual)) => wanted
            .chars()
            .flat_map(char::to_lowercase)
            .eq(actual.chars().flat_map(char::to_lowercase)),
        _ => false,
    }
}

/// Copies the fields of `from` that `merged` does not set yet.
fn fill_unset(mut merged: RuleAction, from: &RuleAction) -> RuleAction {
    merged.transport = merged.transport.or(from.transport);
    merged.reset_on_mouse = merged.reset_on_mouse.or(from.reset_on_mouse);
    merged.macros = merged.macros.or(from.macros);
    merged.vietnamese = merged.vietnamese.or(from.vietnamese);
    merged.max_delete_before = merged.max_delete_before.or(from.max_delete_before);
    merged
}
