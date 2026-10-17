use proptest::prelude::*;
use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::input::{InputEvent, ModifierSet, ReplayKey, vk};
use raumakey_model::transaction::{
    EditTransaction, MAX_INSERT_UNITS, TextSimulator, TransactionError,
};
use raumakey_model::utf16;

fn host() -> HostContext {
    HostContext::new(SessionId::from_u128(9), ExeName::new("app.exe").unwrap())
}

#[test]
fn constructors() {
    let p = EditTransaction::pass_through();
    assert_eq!(p.delete_before, 0);
    assert!(p.insert_utf16.is_empty());
    assert!(!p.consume_original);
    assert!(p.replay_key.is_none());
    assert!(!p.reset_context);
    assert!(p.is_pass_through());
    assert!(EditTransaction::pass_through_reset().reset_context);
    let r = EditTransaction::replace(1, "â");
    assert_eq!(r.delete_before, 1);
    assert_eq!(r.insert_utf16, utf16::encode("â"));
    assert!(r.consume_original);
    assert!(!r.is_pass_through());
    assert_eq!(r.insert_string(), "â");
}

#[test]
fn validation_checks_bounds_and_well_formedness() {
    let ok = EditTransaction::replace(2, "ế");
    assert!(ok.validate(2, 64).is_ok());
    assert_eq!(
        ok.validate(1, 64),
        Err(TransactionError::DeleteExceedsContext {
            requested: 2,
            available: 1
        })
    );
    assert_eq!(
        ok.validate(5, 1),
        Err(TransactionError::DeleteExceedsPolicy {
            requested: 2,
            max: 1
        })
    );
    let mut bad = EditTransaction::replace(0, "");
    bad.insert_utf16 = vec![0xD800];
    assert_eq!(bad.validate(0, 64), Err(TransactionError::MalformedInsert));
    let mut long = EditTransaction::replace(0, "");
    long.insert_utf16 = vec![0x61; MAX_INSERT_UNITS + 1];
    assert_eq!(long.validate(0, 64), Err(TransactionError::InsertTooLong));
    let mut replay = EditTransaction::pass_through();
    replay.replay_key = Some(ReplayKey {
        virtual_key: vk::RETURN,
        scan_code: None,
        modifiers: ModifierSet::default(),
    });
    assert_eq!(
        replay.validate(0, 64),
        Err(TransactionError::ReplayWithoutConsume)
    );
}

#[test]
fn simulator_applies_matrix_style_steps() {
    let mut sim = TextSimulator::new();
    let a = InputEvent::character('a', host());
    sim.apply(&a, &EditTransaction::pass_through()).unwrap();
    assert_eq!(sim.as_string(), "a");
    sim.apply(&a, &EditTransaction::replace(1, "â")).unwrap();
    assert_eq!(sim.as_string(), "â");
    let space = InputEvent::separator(' ', vk::SPACE, host());
    sim.apply(&space, &EditTransaction::pass_through_reset())
        .unwrap();
    assert_eq!(sim.as_string(), "â ");
}

#[test]
fn simulator_replays_enter_after_replacement() {
    let mut sim = TextSimulator::with_text("ko");
    let enter = InputEvent::separator('\r', vk::RETURN, host());
    let mut tx = EditTransaction::replace(2, "không");
    tx.replay_key = Some(ReplayKey {
        virtual_key: vk::RETURN,
        scan_code: None,
        modifiers: ModifierSet::default(),
    });
    sim.apply(&enter, &tx).unwrap();
    assert_eq!(sim.as_string(), "không\r");
}

#[test]
fn simulator_backspace_pass_through_removes_last_scalar() {
    let mut sim = TextSimulator::with_text("a😀");
    let bs = InputEvent::backspace(host());
    sim.apply(&bs, &EditTransaction::pass_through()).unwrap();
    assert_eq!(sim.as_string(), "a");
    sim.apply(&bs, &EditTransaction::pass_through()).unwrap();
    assert_eq!(sim.as_string(), "");
    sim.apply(&bs, &EditTransaction::pass_through()).unwrap();
    assert_eq!(sim.as_string(), "");
}

#[test]
fn simulator_rejects_unsafe_deletes_without_changing_text() {
    let mut sim = TextSimulator::with_text("😀");
    let a = InputEvent::character('a', host());
    assert_eq!(
        sim.apply(&a, &EditTransaction::replace(1, "x")),
        Err(TransactionError::SplitsSurrogate)
    );
    assert_eq!(sim.as_string(), "😀");
    assert_eq!(
        sim.apply(&a, &EditTransaction::replace(3, "x")),
        Err(TransactionError::DeleteExceedsContext {
            requested: 3,
            available: 2
        })
    );
    assert_eq!(sim.as_string(), "😀");
}

proptest! {
    #[test]
    fn valid_transactions_keep_text_well_formed(
        start in "[a-zđâêôơưă😀]{0,8}",
        insert in "[a-zđâêôơưăế😀]{0,4}",
        delete in 0u32..12,
    ) {
        let mut sim = TextSimulator::with_text(&start);
        let event = InputEvent::character('x', host());
        let tx = EditTransaction::replace(delete, &insert);
        let before = sim.as_string();
        match sim.apply(&event, &tx) {
            Ok(()) => prop_assert!(utf16::is_well_formed(sim.units())),
            Err(_) => prop_assert_eq!(sim.as_string(), before),
        }
    }
}
