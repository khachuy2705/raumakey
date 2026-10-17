//! What the edit record of a change shows (`ITfTextEditSink::OnEndEdit`), as the core's
//! [`SelectionChange`].

use raumakey_model::input::MAX_EVENT_TEXT_UNITS;
use raumakey_model::utf16;
use raumakey_tip::adapter::HostFailure;
use raumakey_tip::adapter::record::{EditRecordReader, Observed, observe};
use raumakey_tip::session::SelectionChange;

/// One text change of a fake record: the text its range holds now, and whether the
/// selection is a caret at the end of that range.
#[derive(Clone, Debug)]
struct Change {
    text: Vec<u16>,
    caret_at_end: bool,
}

fn change(text: &str, caret_at_end: bool) -> Change {
    Change {
        text: utf16::encode(text),
        caret_at_end,
    }
}

/// A read the fake record fails.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Selection,
    Changes,
    Text,
    Caret,
}

#[derive(Debug, Default)]
struct Record {
    moved: bool,
    changes: Vec<Change>,
    fail: Option<Step>,
    /// The limit of every text read.
    text_limits: Vec<usize>,
    /// The limit of every read of the change list.
    change_limits: Vec<usize>,
}

impl Record {
    fn new(moved: bool, changes: Vec<Change>) -> Self {
        Self {
            moved,
            changes,
            ..Self::default()
        }
    }

    fn fails(&self, step: Step) -> Result<(), HostFailure> {
        if self.fail == Some(step) {
            Err(HostFailure)
        } else {
            Ok(())
        }
    }
}

impl EditRecordReader for Record {
    type Range = usize;

    fn selection_moved(&mut self) -> Result<bool, HostFailure> {
        self.fails(Step::Selection)?;
        Ok(self.moved)
    }

    fn text_changes(&mut self, max: usize) -> Result<Vec<usize>, HostFailure> {
        self.change_limits.push(max);
        self.fails(Step::Changes)?;
        Ok((0..self.changes.len()).take(max).collect())
    }

    fn text(&mut self, range: &usize, max_units: usize) -> Result<Vec<u16>, HostFailure> {
        self.text_limits.push(max_units);
        self.fails(Step::Text)?;
        Ok(self.changes[*range]
            .text
            .iter()
            .take(max_units)
            .copied()
            .collect())
    }

    fn caret_at_end_of(&mut self, range: &usize) -> Result<bool, HostFailure> {
        self.fails(Step::Caret)?;
        Ok(self.changes[*range].caret_at_end)
    }
}

#[test]
fn a_typed_key_is_its_text_inserted_before_the_caret() {
    let mut record = Record::new(true, vec![change("a", true)]);
    let observed = observe(&mut record);
    assert_eq!(observed, Observed::Inserted(utf16::encode("a")));
    assert_eq!(
        observed.change(),
        Some(SelectionChange::Inserted(&utf16::encode("a")))
    );
}

#[test]
fn a_key_that_types_a_surrogate_pair_is_inserted_whole() {
    let mut record = Record::new(true, vec![change("😀", true)]);
    assert_eq!(
        observe(&mut record),
        Observed::Inserted(utf16::encode("😀"))
    );
}

#[test]
fn the_selection_status_does_not_decide_an_insert() {
    // Hosts that move the caret with the text may or may not report the selection as
    // changed; the caret position decides.
    let mut record = Record::new(false, vec![change("a", true)]);
    assert_eq!(observe(&mut record), Observed::Inserted(utf16::encode("a")));
}

#[test]
fn backspace_is_text_deleted_at_the_caret() {
    let mut record = Record::new(true, vec![change("", true)]);
    let observed = observe(&mut record);
    assert_eq!(observed, Observed::Deleted);
    assert_eq!(observed.change(), Some(SelectionChange::Deleted));
}

#[test]
fn a_change_away_from_the_caret_is_another_change() {
    for text in ["a", ""] {
        let mut record = Record::new(true, vec![change(text, false)]);
        let observed = observe(&mut record);
        assert_eq!(observed, Observed::Other, "{text:?}");
        assert_eq!(observed.change(), Some(SelectionChange::Other));
    }
}

#[test]
fn several_text_changes_are_another_change() {
    let mut record = Record::new(true, vec![change("a", true), change("b", true)]);
    assert_eq!(observe(&mut record), Observed::Other);
    assert_eq!(
        record.change_limits,
        [2],
        "two ranges tell one change from several"
    );
}

#[test]
fn a_caret_move_without_text_is_another_change() {
    let mut record = Record::new(true, Vec::new());
    assert_eq!(observe(&mut record), Observed::Other);
}

#[test]
fn an_edit_that_changed_no_text_and_no_selection_is_no_change() {
    // Another text service's edit session that only changed properties, for example.
    let mut record = Record::new(false, Vec::new());
    let observed = observe(&mut record);
    assert_eq!(observed, Observed::Unchanged);
    assert_eq!(observed.change(), None);
}

#[test]
fn text_longer_than_a_key_types_is_another_change_without_reading_it_all() {
    let long = "a".repeat(MAX_EVENT_TEXT_UNITS + 5);
    let mut record = Record::new(true, vec![change(&long, true)]);
    assert_eq!(observe(&mut record), Observed::Other);
    assert_eq!(record.text_limits, [MAX_EVENT_TEXT_UNITS + 1]);

    let longest = "a".repeat(MAX_EVENT_TEXT_UNITS);
    let mut record = Record::new(true, vec![change(&longest, true)]);
    assert_eq!(
        observe(&mut record),
        Observed::Inserted(utf16::encode(&longest))
    );
}

#[test]
fn a_record_that_cannot_be_read_is_another_change() {
    for step in [Step::Selection, Step::Changes, Step::Text, Step::Caret] {
        let mut record = Record::new(true, vec![change("a", true)]);
        record.fail = Some(step);
        assert_eq!(observe(&mut record), Observed::Other, "{step:?}");
    }
}

#[test]
fn the_debug_output_hides_the_inserted_text() {
    let shown = format!("{:?}", Observed::Inserted(utf16::encode("xyz")));
    assert!(!shown.contains("xyz") && !shown.contains("120"), "{shown}");
    assert!(shown.contains('3'), "the unit count is shown: {shown}");
}
