//! Configuration updates reach the hook thread through the mailbox: they are checked (and a
//! macro table is built) on the posting thread, and applied at the hook's next event, before
//! the key, in the order they were posted.

use std::sync::Arc;

use raumakey_controller::typing::{Mailbox, TypingUpdate};
use raumakey_ipc::error::{IpcError, MalformedReason};
use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_macro::{EntryError, MacroError};
use raumakey_model::input::vk;
use raumakey_model::macros::{BoundaryPolicy, CasePolicy, MacroEntry, MacroId};
use raumakey_model::options::InputMethod;
use raumakey_session::SnapshotError;
use raumakey_windows::hooks::HookDecision;

use crate::support::{FakePlatform, NOTEPAD, Sim, scan_of};

/// Defaults v1 at `revision` with the changes of `change`.
fn snapshot(revision: u64, change: impl FnOnce(&mut RuntimeSnapshot)) -> RuntimeSnapshot {
    let mut snapshot = RuntimeSnapshot::defaults_v1();
    snapshot.config_revision = revision;
    change(&mut snapshot);
    snapshot
}

/// A macro snapshot of `(id, trigger, replacement)` entries.
fn macros(revision: u64, entries: &[(u64, &str, &str)]) -> MacroSnapshot {
    MacroSnapshot {
        macro_revision: revision,
        entries: entries
            .iter()
            .map(|&(id, trigger, replacement)| MacroEntry {
                id: MacroId(id),
                trigger: trigger.to_owned(),
                replacement: replacement.to_owned(),
                enabled: true,
                boundary: BoundaryPolicy::Delimiter,
                case_policy: CasePolicy::Adapt,
            })
            .collect(),
    }
}

fn vni(revision: u64) -> RuntimeSnapshot {
    snapshot(revision, |snapshot| {
        snapshot.options.input_method = InputMethod::Vni;
    })
}

#[test]
fn a_runtime_snapshot_is_applied_at_the_next_key() {
    let mut sim = Sim::notepad();
    sim.mailbox
        .post(TypingUpdate::Runtime(vni(1)))
        .expect("a valid snapshot");
    assert_eq!(
        sim.handler.core().config().options().input_method,
        InputMethod::Telex,
        "the hook thread applies it, at its next event"
    );
    sim.type_text("tie6ng1 vie65t");
    assert_eq!(sim.handler.core().config().config_revision(), 1);
    assert_eq!(sim.text(NOTEPAD), "tiếng việt");
}

#[test]
fn a_snapshot_posted_mid_word_applies_before_the_next_key_and_ends_the_word() {
    let mut sim = Sim::notepad();
    sim.type_text("tiee");
    sim.mailbox
        .post(TypingUpdate::Runtime(snapshot(1, |snapshot| {
            snapshot.options.modern_tone = true;
        })))
        .expect("a valid snapshot");
    sim.type_char('s');
    assert_eq!(
        sim.text(NOTEPAD),
        "tiês",
        "new options end the word in progress"
    );
}

#[test]
fn a_mouse_press_applies_what_was_posted_too() {
    let mut sim = Sim::notepad();
    sim.mailbox
        .post(TypingUpdate::Runtime(vni(4)))
        .expect("a valid snapshot");
    sim.click();
    assert_eq!(sim.handler.core().config().config_revision(), 4);
}

#[test]
fn an_invalid_snapshot_is_refused_when_posted_and_changes_nothing() {
    let mut sim = Sim::notepad();
    let refused = snapshot(2, |snapshot| {
        snapshot.options.input_method = InputMethod::UserDefined;
        snapshot.options.user_keymap_id = Some("missing".to_owned());
    });
    assert_eq!(
        sim.mailbox.post(TypingUpdate::Runtime(refused)),
        Err(SnapshotError::Runtime(IpcError::Malformed(
            MalformedReason::InvalidKeymap
        )))
    );
    assert_eq!(
        sim.mailbox
            .post(TypingUpdate::Macros(macros(2, &[(1, "", "x")]))),
        Err(SnapshotError::MacroTable(MacroError::InvalidEntry {
            id: MacroId(1),
            reason: EntryError::EmptyTrigger,
        }))
    );
    sim.type_text("aa");
    assert_eq!(sim.handler.core().config().config_revision(), 0);
    assert_eq!(sim.handler.core().config().macro_revision(), 0);
    assert_eq!(sim.text(NOTEPAD), "â");
}

#[test]
fn macros_expand_and_enter_is_sent_after_the_expansion() {
    let mut sim = Sim::notepad();
    sim.mailbox
        .post(TypingUpdate::Macros(macros(3, &[(1, "ko", "không")])))
        .expect("valid macros");
    sim.type_text("ko ko");
    assert_eq!(sim.handler.core().config().macro_revision(), 3);
    assert_eq!(sim.text(NOTEPAD), "không ko");
    // Enter is replayed after the expansion: blocked, then sent again as its key-down.
    assert_eq!(sim.down(vk::RETURN), HookDecision::Swallow);
    assert_eq!(sim.text(NOTEPAD), "không không\r");
    let last = sim.platform().sends.last().cloned().expect("a send");
    let enter = last.last().copied().expect("the replay");
    assert_eq!(u32::from(enter.virtual_key), vk::RETURN);
    assert_eq!(u32::from(enter.scan_code), scan_of(vk::RETURN));
    assert!(!enter.key_up, "the physical key-up follows on its own");
    assert_eq!(sim.up(vk::RETURN), HookDecision::PassThrough);
}

#[test]
fn updates_apply_in_the_order_they_were_posted() {
    let mut sim = Sim::notepad();
    sim.mailbox
        .post(TypingUpdate::Mode(false))
        .expect("accepted");
    sim.mailbox
        .post(TypingUpdate::Runtime(snapshot(1, |snapshot| {
            snapshot.vietnamese_mode = true;
        })))
        .expect("a valid snapshot");
    sim.type_char('x');
    assert!(
        sim.handler.core().vietnamese_mode(),
        "the snapshot came last"
    );
    sim.mailbox
        .post(TypingUpdate::Runtime(snapshot(2, |_| {})))
        .expect("a valid snapshot");
    sim.mailbox
        .post(TypingUpdate::Mode(false))
        .expect("accepted");
    sim.type_char('x');
    assert!(!sim.handler.core().vietnamese_mode(), "the mode came last");
    assert_eq!(sim.handler.core().config().config_revision(), 2);
    assert!(sim.modes().is_empty(), "no update is reported back");
}

#[test]
fn only_the_last_update_of_each_kind_is_kept() {
    let mut sim = Sim::notepad();
    for revision in 1..=5 {
        sim.mailbox
            .post(TypingUpdate::Runtime(vni(revision)))
            .expect("a valid snapshot");
    }
    sim.type_char('x');
    assert_eq!(sim.handler.core().config().config_revision(), 5);
}

#[test]
fn the_initial_updates_are_applied_when_the_handler_is_built() {
    let mailbox = Arc::new(Mailbox::new());
    mailbox
        .post(TypingUpdate::Runtime(vni(7)))
        .expect("a valid snapshot");
    mailbox.post(TypingUpdate::Mode(false)).expect("accepted");
    let sim = Sim::with_mailbox(FakePlatform::notepad(), mailbox);
    assert_eq!(sim.handler.core().config().config_revision(), 7);
    assert!(!sim.handler.core().vietnamese_mode());
    assert!(sim.modes().is_empty());
}
