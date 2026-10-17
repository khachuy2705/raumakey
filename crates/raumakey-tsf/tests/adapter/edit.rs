//! The edit of one edit session, over a fake range (matrix #40, #41): move the start of the
//! selection back, check the covered text, replace it, and put the caret after the insert.

use raumakey_model::utf16;
use raumakey_tip::adapter::HostFailure;
use raumakey_tip::adapter::edit::{EditRange, replace_before_caret};
use raumakey_tip::session::EditError;

/// The calls a fake host refuses.
#[derive(Clone, Copy, Debug, Default)]
struct Refusals {
    is_empty: bool,
    shift: bool,
    read: bool,
    write: bool,
    select: bool,
}

/// A document and its selection, as a host keeps them.
#[derive(Debug)]
struct Host {
    text: Vec<u16>,
    selection: (usize, usize),
    /// The start of a range moves back at most this many units.
    shift_limit: Option<usize>,
    /// The start of a range stops before a surrogate pair instead of splitting it.
    whole_scalars: bool,
    /// A write leaves the inserted text selected instead of moving the caret after it.
    selects_insert: bool,
    refuse: Refusals,
    /// Every `write`, applied or refused.
    writes: usize,
    /// Every `extend_back` and `read`.
    shifts: usize,
    reads: usize,
}

impl Host {
    /// `text` with the caret at its end.
    fn new(text: &str) -> Self {
        let text = utf16::encode(text);
        let end = text.len();
        Self {
            text,
            selection: (end, end),
            shift_limit: None,
            whole_scalars: false,
            selects_insert: false,
            refuse: Refusals::default(),
            writes: 0,
            shifts: 0,
            reads: 0,
        }
    }

    fn text(&self) -> String {
        String::from_utf16_lossy(&self.text)
    }

    /// Runs the edit on the selection of this host.
    fn replace(&mut self, expected: &[u16], insert: &str) -> Result<(), EditError> {
        let (start, end) = self.selection;
        let mut range = Range {
            host: self,
            start,
            end,
        };
        replace_before_caret(&mut range, expected, &utf16::encode(insert))
    }
}

/// A range over a [`Host`], as `ITfRange` is over a document.
struct Range<'a> {
    host: &'a mut Host,
    start: usize,
    end: usize,
}

impl EditRange for Range<'_> {
    fn is_empty(&self) -> Result<bool, HostFailure> {
        if self.host.refuse.is_empty {
            return Err(HostFailure);
        }
        Ok(self.start == self.end)
    }

    fn extend_back(&mut self, units: u32) -> Result<u32, HostFailure> {
        self.host.shifts += 1;
        if self.host.refuse.shift {
            return Err(HostFailure);
        }
        let mut moved = (units as usize)
            .min(self.start)
            .min(self.host.shift_limit.unwrap_or(usize::MAX));
        if self.host.whole_scalars
            && moved > 0
            && !utf16::is_scalar_boundary(&self.host.text, self.start - moved)
        {
            moved -= 1;
        }
        self.start -= moved;
        Ok(u32::try_from(moved).expect("small"))
    }

    fn read(&self, max_units: usize) -> Result<Vec<u16>, HostFailure> {
        if self.host.refuse.read {
            return Err(HostFailure);
        }
        Ok(self.host.text[self.start..self.end]
            .iter()
            .take(max_units)
            .copied()
            .collect())
    }

    fn write(&mut self, units: &[u16]) -> Result<(), HostFailure> {
        self.host.writes += 1;
        if self.host.refuse.write {
            return Err(HostFailure);
        }
        let removed = self.end - self.start;
        self.host
            .text
            .splice(self.start..self.end, units.iter().copied());
        let end = self.start + units.len();
        // The selection follows the text the way a host moves it: a position after the
        // replaced units shifts, one inside them ends up after the insert.
        let adjust = |position: usize| {
            if position >= self.start + removed {
                position - removed + units.len()
            } else if position > self.start {
                end
            } else {
                position
            }
        };
        self.host.selection = if self.host.selects_insert {
            (self.start, end)
        } else {
            (adjust(self.host.selection.0), adjust(self.host.selection.1))
        };
        self.end = end;
        Ok(())
    }

    fn select_end(&mut self) -> Result<(), HostFailure> {
        if self.host.refuse.select {
            return Err(HostFailure);
        }
        self.host.selection = (self.end, self.end);
        Ok(())
    }
}

fn units(text: &str) -> Vec<u16> {
    utf16::encode(text)
}

#[test]
fn the_units_before_the_caret_are_replaced_and_the_caret_follows_the_insert() {
    let mut host = Host::new("ta");
    assert_eq!(host.replace(&units("a"), "â"), Ok(()));
    assert_eq!(host.text(), "tâ");
    assert_eq!(host.selection, (2, 2));
}

#[test]
fn matrix_40_two_units_are_replaced_by_one() {
    let mut host = Host::new("tiee");
    assert_eq!(host.replace(&units("ee"), "ế"), Ok(()));
    assert_eq!(host.text(), "tiế");
    assert_eq!(host.selection, (3, 3));
    assert_eq!(host.writes, 1);
}

#[test]
fn the_caret_follows_the_insert_even_where_the_host_selects_it() {
    let mut host = Host::new("tiee");
    host.selects_insert = true;
    assert_eq!(host.replace(&units("ee"), "ế"), Ok(()));
    assert_eq!(host.text(), "tiế");
    assert_eq!(host.selection, (3, 3), "a caret after the insert");
}

#[test]
fn text_after_the_caret_stays_where_it_is() {
    let mut host = Host::new("ta xyz");
    host.selection = (2, 2);
    assert_eq!(host.replace(&units("a"), "â"), Ok(()));
    assert_eq!(host.text(), "tâ xyz");
    assert_eq!(host.selection, (2, 2));
}

#[test]
fn an_insert_without_a_delete_never_moves_the_range() {
    let mut host = Host::new("t");
    host.refuse.shift = true;
    host.refuse.read = true;
    assert_eq!(host.replace(&[], "ư"), Ok(()));
    assert_eq!(host.text(), "tư");
    assert_eq!((host.shifts, host.reads), (0, 0));
}

#[test]
fn a_delete_without_an_insert_removes_the_units() {
    let mut host = Host::new("ab");
    assert_eq!(host.replace(&units("b"), ""), Ok(()));
    assert_eq!(host.text(), "a");
    assert_eq!(host.selection, (1, 1));
}

#[test]
fn matrix_41_a_shorter_text_before_the_caret_changes_nothing() {
    let mut host = Host::new("a");
    assert_eq!(
        host.replace(&units("ba"), "bâ"),
        Err(EditError::RangeShorter { covered: 1 })
    );
    assert_eq!(host.text(), "a");
    assert_eq!(host.selection, (1, 1));
    assert_eq!(host.writes, 0);
}

#[test]
fn a_host_that_moves_the_start_less_far_changes_nothing() {
    let mut host = Host::new("xab");
    host.shift_limit = Some(1);
    assert_eq!(
        host.replace(&units("ab"), "á"),
        Err(EditError::RangeShorter { covered: 1 })
    );
    assert_eq!(host.text(), "xab");
    assert_eq!(host.writes, 0);
}

#[test]
fn other_text_before_the_caret_changes_nothing() {
    let mut host = Host::new("xa");
    assert_eq!(
        host.replace(&units("ba"), "bâ"),
        Err(EditError::TextMismatch)
    );
    assert_eq!(host.text(), "xa");
    assert_eq!(host.writes, 0);
}

#[test]
fn a_surrogate_pair_before_the_caret_is_replaced_whole() {
    let mut host = Host::new("a😀");
    assert_eq!(host.replace(&units("😀"), "b"), Ok(()));
    assert_eq!(host.text(), "ab");
    assert_eq!(host.selection, (2, 2));
}

#[test]
fn a_cut_through_a_surrogate_pair_never_matches() {
    // The document ends in a pair where the core believes one unit lies before the caret.
    // A host that splits the pair covers its low surrogate, which differs from any text.
    let mut splitting = Host::new("a😀");
    assert_eq!(
        splitting.replace(&units("x"), "y"),
        Err(EditError::TextMismatch)
    );
    assert_eq!(splitting.text(), "a😀");
    // A host that keeps pairs whole moves no unit at all.
    let mut whole = Host::new("a😀");
    whole.whole_scalars = true;
    assert_eq!(
        whole.replace(&units("x"), "y"),
        Err(EditError::RangeShorter { covered: 0 })
    );
    assert_eq!(whole.text(), "a😀");
}

#[test]
fn a_selection_that_covers_text_is_not_replaced() {
    let mut host = Host::new("ab");
    host.selection = (0, 2);
    assert_eq!(
        host.replace(&units("b"), "c"),
        Err(EditError::SelectionNotEmpty)
    );
    assert_eq!(host.text(), "ab");
    assert_eq!((host.shifts, host.writes), (0, 0));
}

#[test]
fn a_refused_set_text_changes_nothing() {
    let mut host = Host::new("ta");
    host.refuse.write = true;
    assert_eq!(host.replace(&units("a"), "â"), Err(EditError::Denied));
    assert_eq!(host.text(), "ta");
    assert_eq!(host.selection, (2, 2));
}

#[test]
fn a_refused_selection_puts_the_old_text_back() {
    let mut host = Host::new("tiee");
    host.refuse.select = true;
    assert_eq!(host.replace(&units("ee"), "ế"), Err(EditError::Denied));
    assert_eq!(host.text(), "tiee", "nothing changed");
    assert_eq!(host.writes, 2, "the insert, then the old text");
}

#[test]
fn every_other_failure_of_the_host_is_a_refusal() {
    for refuse in [
        Refusals {
            is_empty: true,
            ..Refusals::default()
        },
        Refusals {
            shift: true,
            ..Refusals::default()
        },
        Refusals {
            read: true,
            ..Refusals::default()
        },
    ] {
        let mut host = Host::new("ta");
        host.refuse = refuse;
        assert_eq!(
            host.replace(&units("a"), "â"),
            Err(EditError::Denied),
            "{refuse:?}"
        );
        assert_eq!(host.text(), "ta", "{refuse:?}");
        assert_eq!(host.writes, 0, "{refuse:?}");
    }
}
