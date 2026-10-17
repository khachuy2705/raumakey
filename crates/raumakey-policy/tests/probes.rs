//! Probe cache and `record_probe` (plan §6; matrix B27).

use std::sync::Arc;

use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::policy::{
    AppRule, CompatibilityConfig, PROBE_VERSION, ProbeRecord, ProbeResult, RuleAction, Transport,
};
use raumakey_policy::{CompatibilityPolicy, MAX_PROBE_RECORDS, PolicyError, ProbeCache};

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

fn host(name: &str) -> HostContext {
    HostContext::new(SessionId::from_u128(3), exe(name))
}

fn record(name: &str, probe_version: u32, tsf_range_edit: bool) -> ProbeRecord {
    ProbeRecord {
        exe: exe(name),
        probe_version,
        tsf_range_edit,
    }
}

fn numbered(i: usize, probe_version: u32) -> ProbeRecord {
    record(&format!("app{i}.exe"), probe_version, true)
}

fn policy(config: CompatibilityConfig) -> CompatibilityPolicy {
    CompatibilityPolicy::new(Arc::new(config))
}

fn full_cache(probe_version: u32) -> ProbeCache {
    let mut cache = ProbeCache::new();
    for i in 0..MAX_PROBE_RECORDS {
        cache.upsert(numbered(i, probe_version)).unwrap();
    }
    cache
}

#[test]
fn probe_cache_capacity_is_1024() {
    assert_eq!(MAX_PROBE_RECORDS, 1024);
}

#[test]
fn upsert_keeps_one_record_per_exe_in_first_insertion_order() {
    let mut cache = ProbeCache::new();
    assert!(cache.is_empty());
    cache.upsert(record("a.exe", PROBE_VERSION, true)).unwrap();
    cache.upsert(record("b.exe", PROBE_VERSION, true)).unwrap();
    cache.upsert(record("a.exe", PROBE_VERSION, false)).unwrap();
    assert_eq!(cache.len(), 2);
    assert_eq!(
        cache.records(),
        [
            record("a.exe", PROBE_VERSION, false),
            record("b.exe", PROBE_VERSION, true)
        ]
    );
    assert_eq!(
        cache.get(&exe("a.exe")),
        Some(&record("a.exe", PROBE_VERSION, false))
    );
    assert_eq!(cache.get(&exe("c.exe")), None);
}

#[test]
fn upsert_replaces_older_versions_and_keeps_newer_ones() {
    let mut cache = ProbeCache::new();
    cache
        .upsert(record("a.exe", PROBE_VERSION - 1, false))
        .unwrap();
    cache.upsert(record("a.exe", PROBE_VERSION, true)).unwrap();
    assert_eq!(cache.records(), [record("a.exe", PROBE_VERSION, true)]);

    cache
        .upsert(record("b.exe", PROBE_VERSION + 1, true))
        .unwrap();
    assert_eq!(
        cache.upsert(record("b.exe", PROBE_VERSION, false)),
        Err(PolicyError::NewerProbeVersion {
            stored: PROBE_VERSION + 1
        })
    );
    assert_eq!(
        cache.get(&exe("b.exe")),
        Some(&record("b.exe", PROBE_VERSION + 1, true))
    );
}

#[test]
fn current_returns_only_records_of_the_current_probe_version() {
    let cache = ProbeCache::from_records([
        record("old.exe", PROBE_VERSION - 1, false),
        record("now.exe", PROBE_VERSION, false),
        record("new.exe", PROBE_VERSION + 1, false),
    ]);
    assert_eq!(cache.current(&exe("old.exe")), None);
    assert_eq!(
        cache.current(&exe("now.exe")),
        Some(&record("now.exe", PROBE_VERSION, false))
    );
    assert_eq!(cache.current(&exe("new.exe")), None);
    assert_eq!(
        cache.len(),
        3,
        "records of other versions are kept, not used"
    );
}

#[test]
fn from_records_keeps_the_newest_version_per_exe() {
    let cache = ProbeCache::from_records([
        record("a.exe", PROBE_VERSION, false),
        record("b.exe", PROBE_VERSION + 1, true),
        record("a.exe", PROBE_VERSION - 1, true),
        record("b.exe", PROBE_VERSION, false),
        record("a.exe", PROBE_VERSION, true),
    ]);
    assert_eq!(
        cache.records(),
        [
            record("a.exe", PROBE_VERSION, true),
            record("b.exe", PROBE_VERSION + 1, true)
        ]
    );
    assert_eq!(
        cache.clone().into_records(),
        cache.records().to_vec(),
        "into_records yields the same list"
    );
}

/// A full cache of current records does not freeze: a current record of a new executable takes
/// the place of the earliest successful probe, which only costs a re-probe of that host.
#[test]
fn a_full_cache_evicts_the_earliest_successful_probe_for_a_new_executable() {
    let mut cache = full_cache(PROBE_VERSION);
    cache
        .upsert(record("extra.exe", PROBE_VERSION, false))
        .unwrap();
    assert_eq!(cache.len(), MAX_PROBE_RECORDS);
    assert_eq!(cache.get(&exe("app0.exe")), None, "earliest evicted");
    assert_eq!(
        cache.records().last(),
        Some(&record("extra.exe", PROBE_VERSION, false))
    );

    cache
        .upsert(record("app5.exe", PROBE_VERSION, false))
        .unwrap();
    assert_eq!(cache.len(), MAX_PROBE_RECORDS);
    assert!(
        cache.get(&exe("app1.exe")).is_some(),
        "updating a known executable evicts nothing"
    );
    assert_eq!(
        cache.current(&exe("app5.exe")),
        Some(&record("app5.exe", PROBE_VERSION, false))
    );
}

/// Eviction order of a full cache: older probe versions, then successful current probes, then
/// failed current probes, each earliest first. Records of newer probe versions stay.
#[test]
fn a_full_cache_evicts_stale_then_successful_then_failed_records_but_never_newer_ones() {
    let mut records = vec![
        record("new0.exe", PROBE_VERSION + 1, true),
        record("fail0.exe", PROBE_VERSION, false),
        record("ok0.exe", PROBE_VERSION, true),
        record("old0.exe", PROBE_VERSION - 1, false),
        record("fail1.exe", PROBE_VERSION, false),
        record("ok1.exe", PROBE_VERSION, true),
        record("old1.exe", PROBE_VERSION - 1, true),
    ];
    let newer = MAX_PROBE_RECORDS - records.len() + 1;
    records.extend((records.len()..MAX_PROBE_RECORDS).map(|i| numbered(i, PROBE_VERSION + 1)));
    let mut cache = ProbeCache::from_records(records);
    assert_eq!(cache.len(), MAX_PROBE_RECORDS);

    let victims = [
        "old0.exe",
        "old1.exe",
        "ok0.exe",
        "ok1.exe",
        "fail0.exe",
        "fail1.exe",
        "probe0.exe",
    ];
    for (n, victim) in victims.iter().enumerate() {
        assert!(cache.get(&exe(victim)).is_some(), "{victim} still cached");
        let newcomer = record(&format!("probe{n}.exe"), PROBE_VERSION, false);
        cache.upsert(newcomer.clone()).unwrap();
        assert_eq!(cache.get(&exe(victim)), None, "{victim} evicted");
        assert_eq!(cache.records().last(), Some(&newcomer));
        assert_eq!(cache.len(), MAX_PROBE_RECORDS);
        assert_eq!(
            cache
                .records()
                .iter()
                .filter(|r| r.probe_version > PROBE_VERSION)
                .count(),
            newer,
            "records of a newer probe version are kept"
        );
    }
}

#[test]
fn a_cache_full_of_newer_records_cannot_store_a_current_probe() {
    let mut cache = full_cache(PROBE_VERSION + 1);
    assert_eq!(
        cache.upsert(record("extra.exe", PROBE_VERSION, false)),
        Err(PolicyError::ProbeCacheFull)
    );
    assert_eq!(cache, full_cache(PROBE_VERSION + 1), "nothing changed");
}

/// Records this build cannot use only take the place of stale records.
#[test]
fn a_full_cache_admits_records_of_other_versions_only_in_place_of_stale_ones() {
    let mut cache = full_cache(PROBE_VERSION);
    for version in [PROBE_VERSION - 1, PROBE_VERSION + 1] {
        assert_eq!(
            cache.upsert(record("other.exe", version, false)),
            Err(PolicyError::ProbeCacheFull)
        );
    }
    assert_eq!(cache, full_cache(PROBE_VERSION));

    let mut cache = ProbeCache::from_records((0..MAX_PROBE_RECORDS).map(|i| {
        numbered(
            i,
            if i == 9 {
                PROBE_VERSION - 1
            } else {
                PROBE_VERSION
            },
        )
    }));
    cache
        .upsert(record("future.exe", PROBE_VERSION + 1, false))
        .unwrap();
    assert_eq!(cache.get(&exe("app9.exe")), None);
    assert!(cache.get(&exe("app0.exe")).is_some());
}

#[test]
fn a_full_cache_evicts_the_earliest_stale_record_first() {
    let mut cache = ProbeCache::new();
    cache.upsert(numbered(0, PROBE_VERSION)).unwrap();
    cache.upsert(numbered(1, PROBE_VERSION - 1)).unwrap();
    cache.upsert(numbered(2, PROBE_VERSION - 1)).unwrap();
    for i in 3..MAX_PROBE_RECORDS {
        cache.upsert(numbered(i, PROBE_VERSION)).unwrap();
    }
    cache
        .upsert(record("extra.exe", PROBE_VERSION, false))
        .unwrap();
    assert_eq!(cache.len(), MAX_PROBE_RECORDS);
    assert_eq!(cache.get(&exe("app1.exe")), None, "earliest stale evicted");
    assert!(cache.get(&exe("app2.exe")).is_some());
    assert!(cache.get(&exe("app0.exe")).is_some());
    assert_eq!(
        cache.records().last(),
        Some(&record("extra.exe", PROBE_VERSION, false))
    );
}

#[test]
fn from_records_caps_the_list() {
    let cache =
        ProbeCache::from_records((0..MAX_PROBE_RECORDS + 10).map(|i| numbered(i, PROBE_VERSION)));
    assert_eq!(cache.len(), MAX_PROBE_RECORDS);
    assert!(
        cache.get(&exe("app9.exe")).is_none(),
        "later records displace the earliest successful probes"
    );
    assert!(cache.get(&exe("app10.exe")).is_some());
    assert!(
        cache
            .get(&exe(&format!("app{}.exe", MAX_PROBE_RECORDS + 9)))
            .is_some()
    );
}

#[test]
fn from_records_keeps_failed_probes_of_an_oversized_list() {
    let failed = (0..10).map(|i| record(&format!("legacy{i}.exe"), PROBE_VERSION, false));
    let cache = ProbeCache::from_records(
        (0..MAX_PROBE_RECORDS)
            .map(|i| numbered(i, PROBE_VERSION))
            .chain(failed),
    );
    assert_eq!(cache.len(), MAX_PROBE_RECORDS);
    for i in 0..10 {
        assert_eq!(
            cache.current(&exe(&format!("legacy{i}.exe"))),
            Some(&record(&format!("legacy{i}.exe"), PROBE_VERSION, false))
        );
    }
}

/// Matrix #27: a successful probe of an uncached host is stored with the current version.
#[test]
fn b27_record_probe_stores_a_current_version_record() {
    let mut p = policy(CompatibilityConfig::defaults_v1());
    assert!(p.probes().is_empty());
    p.record_probe(&host("Generic.exe"), ProbeResult::TsfRangeEditOk)
        .unwrap();
    assert_eq!(p.probes(), [record("generic.exe", PROBE_VERSION, true)]);
    assert_eq!(p.resolve(&host("generic.exe")).transport, Transport::Tsf);
}

/// Matrix #27: a failed probe switches the host to synthetic keys, and only that host.
#[test]
fn b27_failed_probe_selects_synthetic_key_for_that_host_only() {
    let mut p = policy(CompatibilityConfig::defaults_v1());
    p.record_probe(&host("legacy.exe"), ProbeResult::TsfRangeEditOk)
        .unwrap();
    p.record_probe(&host("legacy.exe"), ProbeResult::TsfRangeEditFailed)
        .unwrap();
    assert_eq!(p.probes(), [record("legacy.exe", PROBE_VERSION, false)]);
    assert_eq!(
        p.resolve(&host("legacy.exe")).transport,
        Transport::SyntheticKey
    );
    assert_eq!(p.resolve(&host("notepad.exe")).transport, Transport::Tsf);

    p.record_probe(&host("legacy.exe"), ProbeResult::TsfRangeEditOk)
        .unwrap();
    assert_eq!(p.resolve(&host("legacy.exe")).transport, Transport::Tsf);
}

/// Matrix #27: a stale record is ignored until a current probe replaces it.
#[test]
fn b27_current_probe_replaces_a_stale_record() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = vec![record("legacy.exe", PROBE_VERSION - 1, false)];
    let mut p = policy(config);
    assert_eq!(p.resolve(&host("legacy.exe")).transport, Transport::Tsf);
    p.record_probe(&host("legacy.exe"), ProbeResult::TsfRangeEditFailed)
        .unwrap();
    assert_eq!(p.probes(), [record("legacy.exe", PROBE_VERSION, false)]);
    assert_eq!(
        p.resolve(&host("legacy.exe")).transport,
        Transport::SyntheticKey
    );
}

#[test]
fn record_probe_keeps_a_record_from_a_newer_probe_version() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = vec![record("app.exe", PROBE_VERSION + 1, true)];
    let mut p = policy(config);
    assert_eq!(
        p.record_probe(&host("app.exe"), ProbeResult::TsfRangeEditFailed),
        Err(PolicyError::NewerProbeVersion {
            stored: PROBE_VERSION + 1
        })
    );
    assert_eq!(p.probes(), [record("app.exe", PROBE_VERSION + 1, true)]);
    assert_eq!(p.resolve(&host("app.exe")).transport, Transport::Tsf);
}

/// Matrix #27 on a long-lived install: after 1024 successful probes a newly installed host whose
/// probe fails still falls back to synthetic keys.
#[test]
fn record_probe_is_capped_at_1024_records_and_keeps_working_when_full() {
    let mut p = policy(CompatibilityConfig::defaults_v1());
    for i in 0..MAX_PROBE_RECORDS {
        p.record_probe(&host(&format!("app{i}.exe")), ProbeResult::TsfRangeEditOk)
            .unwrap();
    }
    assert!(p.needs_probe(&host("extra.exe")));
    p.record_probe(&host("extra.exe"), ProbeResult::TsfRangeEditFailed)
        .unwrap();
    assert_eq!(p.probes().len(), MAX_PROBE_RECORDS);
    assert_eq!(
        p.resolve(&host("extra.exe")).transport,
        Transport::SyntheticKey
    );
    assert!(!p.needs_probe(&host("extra.exe")));
    assert!(
        p.needs_probe(&host("app0.exe")),
        "the evicted host is probed again"
    );
    assert_eq!(p.resolve(&host("app0.exe")).transport, Transport::Tsf);

    p.record_probe(&host("app7.exe"), ProbeResult::TsfRangeEditFailed)
        .unwrap();
    assert_eq!(p.probes().len(), MAX_PROBE_RECORDS);
    assert_eq!(
        p.resolve(&host("app7.exe")).transport,
        Transport::SyntheticKey
    );
}

#[test]
fn policy_normalizes_the_configured_probe_list() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = vec![
        record("app3.exe", PROBE_VERSION, false),
        record("a.exe", PROBE_VERSION, true),
        record("app3.exe", PROBE_VERSION, true),
    ];
    assert_eq!(
        policy(config).probes(),
        [
            record("app3.exe", PROBE_VERSION, true),
            record("a.exe", PROBE_VERSION, true)
        ],
        "a later duplicate of the same version replaces the earlier one in place"
    );

    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = (0..MAX_PROBE_RECORDS + 5)
        .map(|i| numbered(i, PROBE_VERSION))
        .collect();
    let p = policy(config);
    assert_eq!(p.probes().len(), MAX_PROBE_RECORDS);
    assert_eq!(p.probes().first(), Some(&numbered(5, PROBE_VERSION)));
}

/// A host whose result cannot be stored is not probed again and again.
#[test]
fn no_probe_is_requested_when_a_cache_full_of_newer_records_cannot_store_it() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = full_cache(PROBE_VERSION + 1).into_records();
    let mut p = policy(config);
    assert!(!p.needs_probe(&host("extra.exe")));
    assert_eq!(
        p.record_probe(&host("extra.exe"), ProbeResult::TsfRangeEditFailed),
        Err(PolicyError::ProbeCacheFull)
    );
    assert_eq!(p.resolve(&host("extra.exe")).transport, Transport::Tsf);
}

/// As `record_probe` documents, the cap keeps a failed probe from taking effect only when every
/// cached record comes from a newer probe version: a single record of any other kind makes room,
/// and `needs_probe` tells in advance when nothing can.
#[test]
fn a_full_cache_takes_a_failed_probe_unless_every_record_is_from_a_newer_probe_version() {
    let odd_ones = [
        Some(record("stale.exe", PROBE_VERSION - 1, true)),
        Some(record("ok.exe", PROBE_VERSION, true)),
        Some(record("failed.exe", PROBE_VERSION, false)),
        None,
    ];
    for odd_one in odd_ones {
        let newer = MAX_PROBE_RECORDS - usize::from(odd_one.is_some());
        let mut config = CompatibilityConfig::defaults_v1();
        config.probes = (0..newer)
            .map(|i| numbered(i, PROBE_VERSION + 1))
            .chain(odd_one.clone())
            .collect();
        let mut p = policy(config);
        assert_eq!(p.probes().len(), MAX_PROBE_RECORDS);
        let before = p.probes().to_vec();
        let extra = host("extra.exe");
        let asked = p.needs_probe(&extra);
        let outcome = p.record_probe(&extra, ProbeResult::TsfRangeEditFailed);
        let transport = p.resolve(&extra).transport;
        match &odd_one {
            Some(odd) => {
                assert!(asked, "{odd:?} makes room");
                assert_eq!(outcome, Ok(()), "{odd:?}");
                assert_eq!(transport, Transport::SyntheticKey, "{odd:?}");
                assert_eq!(p.probe_cache().get(&odd.exe), None, "{odd:?} is evicted");
            }
            None => {
                assert!(!asked, "a result that cannot be stored is not asked for");
                assert_eq!(outcome, Err(PolicyError::ProbeCacheFull));
                assert_eq!(transport, Transport::Tsf, "the failed probe has no effect");
                assert_eq!(p.probes(), before, "nothing changed");
            }
        }
        assert_eq!(
            p.probes()
                .iter()
                .filter(|r| r.probe_version > PROBE_VERSION)
                .count(),
            newer,
            "records of a newer probe version are kept"
        );
    }
}

#[test]
fn recorded_probe_does_not_override_an_explicit_transport_rule() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.rules.push(AppRule {
        exe: exe("forced.exe"),
        window_class: None,
        action: RuleAction {
            transport: Some(Transport::Tsf),
            ..RuleAction::default()
        },
    });
    let mut p = policy(config);
    p.record_probe(&host("forced.exe"), ProbeResult::TsfRangeEditFailed)
        .unwrap();
    assert_eq!(p.probes(), [record("forced.exe", PROBE_VERSION, false)]);
    assert_eq!(p.resolve(&host("forced.exe")).transport, Transport::Tsf);
}

#[test]
fn to_config_carries_the_recorded_probes_for_persistence() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.clipboard_enabled = true;
    let mut p = policy(config.clone());
    p.record_probe(&host("legacy.exe"), ProbeResult::TsfRangeEditFailed)
        .unwrap();
    let saved = p.to_config();
    assert_eq!(saved.probes, [record("legacy.exe", PROBE_VERSION, false)]);
    assert_eq!(
        CompatibilityConfig {
            probes: Vec::new(),
            ..saved
        },
        config
    );
    assert!(p.config().probes.is_empty(), "the snapshot is immutable");
}

#[test]
fn policy_errors_are_typed_and_displayable() {
    for e in [
        PolicyError::ProbeCacheFull,
        PolicyError::NewerProbeVersion { stored: 9 },
    ] {
        let as_error: &dyn std::error::Error = &e;
        assert!(!as_error.to_string().is_empty());
    }
}

/// Matrix #27: only hosts without a current-version record need a probe. A host whose record
/// comes from a newer probe version is not probed: this build cannot replace that record, so
/// probing it on every activation would never end.
#[test]
fn b27_needs_probe_until_a_current_record_exists() {
    let mut config = CompatibilityConfig::defaults_v1();
    config.probes = vec![
        record("old.exe", PROBE_VERSION - 1, true),
        record("future.exe", PROBE_VERSION + 1, true),
    ];
    let mut p = policy(config);
    assert!(p.needs_probe(&host("new.exe")));
    assert!(
        p.needs_probe(&host("old.exe")),
        "stale records are re-probed"
    );
    assert!(
        !p.needs_probe(&host("future.exe")),
        "this build can neither use nor replace a newer record"
    );
    assert_eq!(
        p.record_probe(&host("future.exe"), ProbeResult::TsfRangeEditFailed),
        Err(PolicyError::NewerProbeVersion {
            stored: PROBE_VERSION + 1
        })
    );
    p.record_probe(&host("new.exe"), ProbeResult::TsfRangeEditFailed)
        .unwrap();
    assert!(!p.needs_probe(&host("new.exe")));
    assert_eq!(
        p.probe_cache().current(&exe("new.exe")),
        Some(&record("new.exe", PROBE_VERSION, false))
    );
    assert_eq!(p.probe_cache().records(), p.probes());
}

#[test]
fn policy_types_can_move_to_worker_threads() {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<CompatibilityPolicy>();
    assert_send_sync::<ProbeCache>();
    assert_send_sync::<PolicyError>();
    assert_send_sync::<raumakey_policy::HotkeyClassifier>();
    assert_send_sync::<raumakey_policy::KeyVerdict>();
}
