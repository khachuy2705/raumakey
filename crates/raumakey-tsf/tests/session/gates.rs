//! Gates in front of the engine: input Raumakey injected (matrix #39), the transport the
//! host policy selects and the probe results it rests on (#27), and secure fields (#29).

use raumakey_ipc::message::RuntimeSnapshot;
use raumakey_model::host::ExeName;
use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_model::input::vk;
use raumakey_model::policy::{PROBE_VERSION, ProbeRecord, ProbeResult};
use raumakey_tip::session::{KeyDownOutcome, ServiceCore};

use crate::support::{FakeDocument, host};

/// A probe record of `notepad.exe`, the executable of every fake document.
fn notepad_probe(probe_version: u32, tsf_range_edit: bool) -> ProbeRecord {
    ProbeRecord {
        exe: ExeName::new("notepad.exe").expect("valid name"),
        probe_version,
        tsf_range_edit,
    }
}

/// Defaults v1 at `revision` whose compatibility rules carry `probes`, as the controller
/// publishes the records it persists.
fn carrying(revision: u64, probes: Vec<ProbeRecord>) -> RuntimeSnapshot {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = revision;
    snapshot.compatibility.probes = probes;
    snapshot
}

#[test]
fn matrix_39_injected_keys_pass_through_untouched() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    let mut injected = doc.char_key('a');
    injected.extra_info = RAUMAKEY_INJECTED_TAG;
    assert!(!core.test_key_down(&injected));
    assert_eq!(
        core.key_down(&injected, &mut doc),
        KeyDownOutcome::pass_through()
    );
    assert!(!core.test_key_up(&injected));
    assert_eq!(doc.attempts, 0);
    // The word in progress never saw the injected key.
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "â");
}

#[test]
fn matrix_39_an_injected_key_from_another_document_changes_nothing() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let mut other = FakeDocument::new(2);
    doc.type_char(&mut core, 'a');
    let mut injected = other.key(raumakey_model::input::vk::RETURN);
    injected.extra_info = RAUMAKEY_INJECTED_TAG;
    assert!(!core.test_key_down(&injected));
    assert!(!core.key_down(&injected, &mut other).eaten);
    doc.type_char(&mut core, 'a');
    assert_eq!(
        doc.text(),
        "â",
        "the injected key was not a document change"
    );
}

#[test]
fn other_extra_info_values_are_ordinary_input() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let mut first = doc.char_key('a');
    first.extra_info = 0x1234_5678;
    doc.press(&mut core, &first);
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "â");
}

/// Without `evkey-compat` the legacy replay sentinel means nothing to Raumakey, which tags
/// its own replays with `RAUMAKEY_INJECTED_TAG` (D6). The `evkey-compat` TSF path must bypass
/// it instead (clean-room spec §3.1); that path is not written yet.
#[cfg(not(feature = "evkey-compat"))]
#[test]
fn the_legacy_sentinel_is_ordinary_input_without_evkey_compat() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    let mut first = doc.char_key('a');
    first.extra_info = 0xEBCA_1234;
    doc.press(&mut core, &first);
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "â");
}

#[test]
fn hosts_without_tsf_range_edits_are_left_to_the_controller() {
    let mut core = ServiceCore::new();
    let mut fallback = host(1);
    fallback.supports_tsf_range_edit = false;
    let mut doc = FakeDocument::with_host(fallback);
    let presses = doc.type_text(&mut core, "aas ");
    assert!(
        presses.iter().all(|press| !press.claimed),
        "no key is handled by two transports"
    );
    assert_eq!(doc.text(), "aas ");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn matrix_27_a_host_is_probed_once_and_a_failed_probe_leaves_it_to_the_controller() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    assert!(core.needs_probe(&doc.host));
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditFailed)
        .expect("recorded");
    assert!(!core.needs_probe(&doc.host));
    let presses = doc.type_text(&mut core, "aas ");
    assert!(
        presses.iter().all(|press| !press.claimed),
        "no key is handled by two transports"
    );
    assert_eq!(doc.text(), "aas ");
}

#[test]
fn matrix_27_a_recorded_probe_outlives_snapshots_that_do_not_carry_it_yet() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditFailed)
        .expect("recorded");
    // The controller has not published the reported result yet.
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 1;
    snapshot.options.modern_tone = true;
    core.apply_runtime_snapshot(&snapshot)
        .expect("valid snapshot");
    assert!(!core.needs_probe(&doc.host));
    let presses = doc.type_text(&mut core, "aa");
    assert!(presses.iter().all(|press| !press.claimed));
    assert_eq!(doc.text(), "aa");
}

#[test]
fn matrix_27_a_snapshot_that_carries_a_working_probe_overrides_a_failed_local_one() {
    // The controller resolves the host with the record it publishes (here another process of
    // the executable reported last), so its hook leaves these keys to the TIP.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditFailed)
        .expect("recorded");
    core.apply_runtime_snapshot(&carrying(1, vec![notepad_probe(PROBE_VERSION, true)]))
        .expect("valid snapshot");
    assert!(!core.needs_probe(&doc.host));
    doc.type_text(&mut core, "tieengs ");
    assert_eq!(doc.text(), "tiếng ", "the TIP handles the host");
}

#[test]
fn matrix_27_a_snapshot_that_carries_a_failed_probe_overrides_a_working_local_one() {
    // The controller's hook handles the host that the published record hands it, so the TIP
    // must leave the keys alone.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditOk)
        .expect("recorded");
    core.apply_runtime_snapshot(&carrying(1, vec![notepad_probe(PROBE_VERSION, false)]))
        .expect("valid snapshot");
    assert!(!core.needs_probe(&doc.host));
    let presses = doc.type_text(&mut core, "tieengs ");
    assert!(
        presses.iter().all(|press| !press.claimed),
        "no key is handled by two transports"
    );
    assert_eq!(doc.text(), "tieengs ");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn matrix_27_a_host_is_probed_again_once_a_snapshot_drops_the_record_it_carried() {
    // The record a snapshot carries for the executable replaces the local result for good,
    // also one of a newer probe version, which this build ignores and cannot replace. When a
    // later snapshot drops it (the controller wants the host probed again), the host is
    // unprobed, as the controller now resolves it, and the core asks for a new probe.
    for carried in [
        notepad_probe(PROBE_VERSION, false),
        notepad_probe(PROBE_VERSION + 1, false),
    ] {
        let mut core = ServiceCore::new();
        let mut doc = FakeDocument::new(1);
        core.record_probe(&doc.host, ProbeResult::TsfRangeEditFailed)
            .expect("recorded");
        core.apply_runtime_snapshot(&carrying(1, vec![carried]))
            .expect("valid snapshot");
        assert!(!core.needs_probe(&doc.host));
        core.apply_runtime_snapshot(&carrying(2, Vec::new()))
            .expect("valid snapshot");
        assert!(core.needs_probe(&doc.host));
        doc.type_text(&mut core, "tieengs ");
        assert_eq!(doc.text(), "tiếng ", "an unprobed host uses TSF");
    }
}

#[test]
fn matrix_27_a_record_of_an_older_probe_version_leaves_the_local_result_in_effect() {
    // Resolution ignores such a record, in the controller too, until the controller stores
    // the reported result in its place. Dropping the local result here would let the TIP
    // handle the host as soon as the controller's hook does.
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditFailed)
        .expect("recorded");
    core.apply_runtime_snapshot(&carrying(1, vec![notepad_probe(PROBE_VERSION - 1, true)]))
        .expect("valid snapshot");
    assert!(!core.needs_probe(&doc.host));
    let presses = doc.type_text(&mut core, "tieengs ");
    assert!(presses.iter().all(|press| !press.claimed));
    assert_eq!(doc.text(), "tieengs ");
}

#[test]
fn matrix_29_a_secure_field_is_never_probed() {
    // The probe edits the document, and a secure field bypasses every transformation; the
    // next ordinary context of the executable is probed instead.
    let core = ServiceCore::new();
    let mut secure = host(1);
    secure.is_secure_field = true;
    assert!(!core.needs_probe(&secure));
    assert!(core.needs_probe(&host(2)));
}

#[test]
fn matrix_27_a_context_without_a_document_is_never_probed() {
    let core = ServiceCore::new();
    let mut no_document = host(1);
    no_document.document_id = None;
    assert!(!core.needs_probe(&no_document));
    assert!(core.needs_probe(&host(1)));
}

#[test]
fn a_probe_that_changes_how_the_host_is_handled_ends_the_word() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    // A successful probe changes nothing for a host that already uses TSF.
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditOk)
        .expect("recorded");
    doc.type_char(&mut core, 'a');
    assert_eq!(doc.text(), "â");
    // A failed one hands the host to the controller; a later success hands it back, without
    // the word that was in progress before.
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditFailed)
        .expect("recorded");
    core.record_probe(&doc.host, ProbeResult::TsfRangeEditOk)
        .expect("recorded");
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "âs");
    assert_eq!(doc.attempts, 1);
}

#[test]
fn matrix_29_a_secure_field_passes_everything_through() {
    let mut core = ServiceCore::new();
    let mut secure = host(1);
    secure.is_secure_field = true;
    let mut doc = FakeDocument::with_host(secure);
    let presses = doc.type_text(&mut core, "aas after ");
    assert!(presses.iter().all(|press| !press.claimed));
    assert_eq!(doc.text(), "aas after ");
    assert_eq!(doc.attempts, 0);
}

/// D25: in `Bypass`, the default, the core passes every key of a secure field through
/// before the engine, so the field is never edited. Without that gate the engine, which now
/// processes secure-field keys, would undo the last key of a word typed just before the field
/// turned secure: in English mode with `macros_when_disabled` such a word is tracked
/// literally, and Backspace steps back through its history.
#[test]
fn d25_bypass_never_lets_the_engine_edit_a_secure_field() {
    let mut core = ServiceCore::new();
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = 1;
    snapshot.options.macros_when_disabled = true;
    core.apply_runtime_snapshot(&snapshot)
        .expect("valid snapshot");
    core.set_vietnamese_mode(false);
    let mut doc = FakeDocument::new(1);
    doc.type_text(&mut core, "ko");
    doc.host.is_secure_field = true;
    let press = doc.type_key(&mut core, vk::BACK);
    assert!(press.reached_app());
    assert_eq!(doc.text(), "k");
    assert_eq!(
        doc.attempts, 0,
        "a secure field is never edited in Bypass mode"
    );
    // The word ended: back in a plain field, nothing is left to undo.
    doc.host.is_secure_field = false;
    doc.type_key(&mut core, vk::BACK);
    assert_eq!(doc.text(), "");
    assert_eq!(doc.attempts, 0);
}

#[test]
fn matrix_29_a_field_that_turns_secure_ends_the_word() {
    let mut core = ServiceCore::new();
    let mut doc = FakeDocument::new(1);
    doc.type_char(&mut core, 'a');
    doc.host.is_secure_field = true;
    let press = doc.type_char(&mut core, 'a');
    assert!(press.reached_app());
    doc.host.is_secure_field = false;
    doc.type_char(&mut core, 's');
    assert_eq!(doc.text(), "aas");
    assert_eq!(doc.attempts, 0);
}
