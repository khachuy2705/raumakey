//! `transaction_application`: arbitrary documents (lone surrogates and astral characters
//! included) and arbitrary transactions against `EditTransaction::validate` and
//! `TextSimulator::apply` (plan §2.7, design §5.1, matrix #40, #41, #54).

use raumakey_model::host::{ExeName, HostContext, SessionId};
use raumakey_model::input::{InputEvent, InputKind, ModifierSet, Modifiers, ReplayKey, vk};
use raumakey_model::transaction::{
    EditTransaction, MAX_INSERT_UNITS, TextSimulator, TransactionError,
};

use super::oracle::{is_high, is_low, splits_pair, well_formed};
use super::reader::Reader;

/// Most code points of a document; an astral pair counts once, so a document holds up to twice
/// as many UTF-16 units.
const MAX_DOCUMENT_CODE_POINTS: usize = 48;
/// Most code points of an insert (except the inserts sized at the limit), likewise.
const MAX_TEXT_CODE_POINTS: usize = 8;

const KINDS: [InputKind; 9] = [
    InputKind::Character,
    InputKind::Backspace,
    InputKind::Separator,
    InputKind::Navigation,
    InputKind::MouseReset,
    InputKind::FocusReset,
    InputKind::ModeToggle,
    InputKind::ConfigChanged,
    InputKind::Shutdown,
];

/// One code unit, biased towards surrogates, combining marks, and the replayed characters.
pub(crate) fn unit(reader: &mut Reader<'_>) -> u16 {
    match reader.below(10) {
        0..=2 => 0x20 + u16::from(reader.byte() % 0x5F),
        3 => 0xD800 + (u16::from(reader.byte()) << 2),
        4 => 0xDC00 + (u16::from(reader.byte()) << 2),
        5 => reader.pick(&[0x00E2, 0x1EBF, 0x0301, 0x0302, 0x0111]),
        6 => reader.pick(&[0x000D, 0x0009, 0x0020, 0x0008, 0xFEFF, 0x0000]),
        _ => reader.u16(),
    }
}

/// Up to `max` code points (lone surrogates count as one); about one in eight is a well-formed
/// astral pair, so the result holds up to `2 * max` units.
pub(crate) fn units(reader: &mut Reader<'_>, max: usize) -> Vec<u16> {
    let mut out = Vec::new();
    for _ in 0..reader.below(max + 1) {
        if reader.chance(32) {
            out.push(0xD83D);
            out.push(0xDE00 + u16::from(reader.byte() % 0x50));
        } else {
            out.push(unit(reader));
        }
    }
    out
}

fn host() -> HostContext {
    let exe = ExeName::new("fuzz-transaction.exe").expect("constant name is valid");
    HostContext::new(SessionId::from_u128(0x7458), exe)
}

fn decode_event(reader: &mut Reader<'_>) -> InputEvent {
    let mut event = InputEvent::simple(reader.pick(&KINDS), host());
    if reader.chance(200) {
        event.text_utf16 = units(reader, 4);
    }
    event.virtual_key = match reader.below(4) {
        0 => None,
        1 => Some(reader.pick(&[vk::RETURN, vk::TAB, vk::SPACE, vk::BACK])),
        2 => Some(u32::from(reader.byte())),
        _ => Some(reader.u32()),
    };
    let bits = reader.byte();
    event.modifiers = Modifiers {
        shift: bits & 1 != 0,
        caps_lock: bits & 2 != 0,
        ctrl: bits & 4 != 0,
        alt: bits & 8 != 0,
        alt_gr: bits & 16 != 0,
        win: bits & 32 != 0,
    };
    event
}

fn decode_transaction(reader: &mut Reader<'_>, document_len: usize) -> EditTransaction {
    let len = u32::try_from(document_len).unwrap_or(u32::MAX);
    let delete_before = match reader.below(8) {
        0..=3 => u32::try_from(reader.below(document_len + 1)).unwrap_or(0),
        4 => len.saturating_add(1),
        5 => u32::MAX,
        6 => 0,
        _ => u32::from(reader.u16()),
    };
    let mut insert_utf16 = units(reader, MAX_TEXT_CODE_POINTS);
    if reader.chance(4) {
        // At the insert limit and one unit beyond it.
        let size = MAX_INSERT_UNITS + reader.below(2);
        insert_utf16.resize(size, u16::from(b'a'));
    }
    let consume_original = reader.flag();
    let replay_key = match reader.below(6) {
        0..=2 => None,
        3 => Some(vk::RETURN),
        4 => Some(vk::TAB),
        _ => Some(reader.pick(&[vk::SPACE, vk::BACK, 0x41, 0])),
    }
    .map(|virtual_key| ReplayKey {
        virtual_key,
        scan_code: None,
        modifiers: ModifierSet::default(),
    });
    EditTransaction {
        delete_before,
        insert_utf16,
        consume_original,
        replay_key,
        reset_context: reader.flag(),
    }
}

/// Every rule of plan §2.7 that `tx` breaks against `available` units and `max_delete`.
fn violated_rules(tx: &EditTransaction, available: u32, max_delete: u32) -> Vec<TransactionError> {
    let mut violated = Vec::new();
    if !well_formed(&tx.insert_utf16) {
        violated.push(TransactionError::MalformedInsert);
    }
    if tx.insert_utf16.len() > MAX_INSERT_UNITS {
        violated.push(TransactionError::InsertTooLong);
    }
    if tx.delete_before > available {
        violated.push(TransactionError::DeleteExceedsContext {
            requested: tx.delete_before,
            available,
        });
    }
    if tx.delete_before > max_delete {
        violated.push(TransactionError::DeleteExceedsPolicy {
            requested: tx.delete_before,
            max: max_delete,
        });
    }
    if tx.replay_key.is_some() && !tx.consume_original {
        violated.push(TransactionError::ReplayWithoutConsume);
    }
    violated
}

/// `validate` accepts exactly the transactions that break no rule and otherwise reports one
/// of the broken rules.
fn check_validate(tx: &EditTransaction, available: u32, max_delete: u32) {
    let violated = violated_rules(tx, available, max_delete);
    match tx.validate(available, max_delete) {
        Ok(()) => assert!(
            violated.is_empty(),
            "validate accepted a transaction that breaks {} rule(s)",
            violated.len()
        ),
        Err(error) => assert!(
            violated.contains(&error),
            "validate reported {error:?}, which the transaction does not break"
        ),
    }
}

/// Removes the last code point (a lone surrogate counts as one), as Backspace does.
fn delete_last_scalar(text: &mut Vec<u16>) {
    let len = match text.as_slice() {
        [] => 0,
        [.., high, low] if is_high(*high) && is_low(*low) => 2,
        _ => 1,
    };
    text.truncate(text.len() - len);
}

/// The effect of an event that reaches the application (plan §2.7).
fn natural_effect(text: &mut Vec<u16>, event: &InputEvent) {
    match event.kind {
        InputKind::Character | InputKind::Separator => text.extend_from_slice(&event.text_utf16),
        InputKind::Backspace => delete_last_scalar(text),
        _ => {}
    }
}

/// The text plan §2.7 gives a replayed key: Enter, Tab, and Space type `\r`, `\t`, and a
/// space. The plan names no other key; such a key types nothing here.
fn plan_replay_text(virtual_key: u32) -> &'static [u16] {
    match virtual_key {
        vk::RETURN => &[0x0D],
        vk::TAB => &[0x09],
        vk::SPACE => &[0x20],
        _ => &[],
    }
}

/// The replay rules that `TextSimulator::apply` adds to plan §2.7, which the plan does not
/// state: a replay of the event's own key has the event's natural effect instead of the
/// replayed key's text, and a replayed Backspace deletes the last code point. Returns the text
/// after the replay, or `None` where plan §2.7 alone decides.
///
/// This mirrors the model, so for these two cases the target is a regression check, not an
/// independent oracle. The rules agree with plan §2.7 for an Enter, Tab, or Space event that
/// types its plan text; they differ for others (an Enter event typing `\n` replays `\n`, where
/// the plan gives `\r`), which is reported for a decision.
fn model_replay_rule(text: &[u16], event: &InputEvent, virtual_key: u32) -> Option<Vec<u16>> {
    let mut next = text.to_vec();
    if event.virtual_key == Some(virtual_key) {
        natural_effect(&mut next, event);
    } else if virtual_key == vk::BACK {
        delete_last_scalar(&mut next);
    } else {
        return None;
    }
    Some(next)
}

/// The text `apply` must produce, or `None` when the transaction must be rejected: plan §2.7,
/// with the model's own replay rules ([`model_replay_rule`]) where they apply.
fn oracle(document: &[u16], event: &InputEvent, tx: &EditTransaction) -> Option<Vec<u16>> {
    let available = u32::try_from(document.len()).unwrap_or(u32::MAX);
    if !violated_rules(tx, available, u32::MAX).is_empty() {
        return None;
    }
    let cut = document.len() - usize::try_from(tx.delete_before).ok()?;
    if splits_pair(document, cut) {
        return None;
    }
    let mut next = document[..cut].to_vec();
    next.extend_from_slice(&tx.insert_utf16);
    if !tx.consume_original {
        natural_effect(&mut next, event);
    }
    if let Some(replay) = tx.replay_key {
        match model_replay_rule(&next, event, replay.virtual_key) {
            Some(replayed) => next = replayed,
            None => next.extend_from_slice(plan_replay_text(replay.virtual_key)),
        }
    }
    Some(next)
}

fn check_apply(document: &[u16], event: &InputEvent, tx: &EditTransaction) {
    let mut simulator = TextSimulator::new();
    // The simulator does not validate the text of a passed-through event, so this loads the
    // document verbatim, lone surrogates included.
    let loader = InputEvent::text(document.to_vec(), host());
    assert!(
        simulator
            .apply(&loader, &EditTransaction::pass_through())
            .is_ok(),
        "a pass-through edit was rejected"
    );
    assert!(
        simulator.units() == document,
        "the document did not load verbatim"
    );
    let before = simulator.clone();
    let expected = oracle(document, event, tx);
    match simulator.apply(event, tx) {
        Ok(()) => {
            let Some(expected) = expected else {
                panic!("apply accepted a transaction that plan §2.7 rejects");
            };
            assert!(
                simulator.units() == expected.as_slice(),
                "apply produced other text than plan §2.7 and the model's replay rules give"
            );
            if well_formed(document) && well_formed(&event.text_utf16) {
                assert!(
                    well_formed(simulator.units()),
                    "apply turned well-formed input into ill-formed UTF-16"
                );
            }
        }
        Err(error) => {
            assert!(
                simulator == before,
                "a rejected transaction changed the text"
            );
            assert!(
                expected.is_none(),
                "apply rejected ({error:?}) a transaction that plan §2.7 accepts"
            );
            let available = u32::try_from(document.len()).unwrap_or(u32::MAX);
            let violated = violated_rules(tx, available, u32::MAX);
            assert!(
                violated.contains(&error)
                    || (error == TransactionError::SplitsSurrogate && violated.is_empty()),
                "apply reported {error:?}, which the transaction does not break"
            );
        }
    }
}

pub(crate) fn run(data: &[u8]) {
    let mut reader = Reader::new(data);
    let document = units(&mut reader, MAX_DOCUMENT_CODE_POINTS);
    let tx = decode_transaction(&mut reader, document.len());
    let event = decode_event(&mut reader);
    let len = u32::try_from(document.len()).unwrap_or(u32::MAX);
    let raw = u32::from(reader.u16());
    for (available, max_delete) in [
        (len, u32::MAX),
        (len, 0),
        (0, u32::MAX),
        (u32::MAX, raw),
        (raw, 64),
    ] {
        check_validate(&tx, available, max_delete);
    }
    check_apply(&document, &event, &tx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replay(virtual_key: u32) -> Option<ReplayKey> {
        Some(ReplayKey {
            virtual_key,
            scan_code: None,
            modifiers: ModifierSet::default(),
        })
    }

    #[test]
    fn plan_2_7_gives_enter_tab_and_space_their_text() {
        assert_eq!(plan_replay_text(vk::RETURN), [0x0D]);
        assert_eq!(plan_replay_text(vk::TAB), [0x09]);
        assert_eq!(plan_replay_text(vk::SPACE), [0x20]);
        assert!(plan_replay_text(vk::BACK).is_empty());
        assert!(plan_replay_text(0x41).is_empty());
    }

    #[test]
    fn the_model_rules_apply_only_where_plan_2_7_does_not_decide() {
        let enter = InputEvent::separator('\r', vk::RETURN, host());
        // Another key than the event's own, and not Backspace: plan §2.7 decides.
        assert_eq!(model_replay_rule(&[0x61], &enter, vk::TAB), None);
        assert_eq!(model_replay_rule(&[0x61], &enter, vk::SPACE), None);
        // A replayed Backspace deletes the last code point.
        assert_eq!(
            model_replay_rule(&[0x61, 0xD83D, 0xDE00], &enter, vk::BACK),
            Some(vec![0x61])
        );
        // The event's own key has the event's effect: the plan's text for an Enter typing `\r`.
        assert_eq!(
            model_replay_rule(&[0x61], &enter, vk::RETURN),
            Some(vec![0x61, 0x0D])
        );
    }

    /// The divergence reported for `raumakey-model`: plan §2.7 gives a replayed Enter `\r`,
    /// but `TextSimulator` replays the event's own key with the event's text.
    #[test]
    fn the_simulator_replays_the_events_own_key_with_its_text() {
        let newline = InputEvent::separator('\n', vk::RETURN, host());
        assert_eq!(
            model_replay_rule(&[0x61], &newline, vk::RETURN),
            Some(vec![0x61, 0x0A])
        );
        let tx = EditTransaction {
            consume_original: true,
            replay_key: replay(vk::RETURN),
            ..EditTransaction::default()
        };
        let mut simulator = TextSimulator::with_text("a");
        assert!(simulator.apply(&newline, &tx).is_ok());
        assert_eq!(simulator.as_string(), "a\n", "plan §2.7 would give a\r");
    }
}
