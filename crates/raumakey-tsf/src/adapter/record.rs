//! What the edit record of a change shows (`ITfTextEditSink::OnEndEdit`), for
//! [`ServiceCore::selection_changed`](crate::session::ServiceCore::selection_changed).

use core::fmt;

use raumakey_model::input::MAX_EVENT_TEXT_UNITS;

use crate::adapter::HostFailure;
use crate::session::SelectionChange;

/// The reads of an edit record, with the read-only cookie of `OnEndEdit`: `ITfEditRecord`
/// and `ITfContext::GetSelection` in the COM layer, a fake in tests.
pub trait EditRecordReader {
    /// A text-change range of the record.
    type Range;

    /// Whether the selection changed during the edit (`ITfEditRecord::GetSelectionStatus`).
    fn selection_moved(&mut self) -> Result<bool, HostFailure>;

    /// At most `max` ranges whose text changed
    /// (`ITfEditRecord::GetTextAndPropertyUpdates` with `TF_GTP_INCL_TEXT`).
    fn text_changes(&mut self, max: usize) -> Result<Vec<Self::Range>, HostFailure>;

    /// At most `max_units` units of the text `range` holds now.
    fn text(&mut self, range: &Self::Range, max_units: usize) -> Result<Vec<u16>, HostFailure>;

    /// Whether the selection is a caret at the end of `range`.
    fn caret_at_end_of(&mut self, range: &Self::Range) -> Result<bool, HostFailure>;
}

/// What an edit record shows.
///
/// `Debug` shows the number of inserted units only: they are document text.
#[derive(Clone, PartialEq, Eq)]
pub enum Observed {
    /// Neither the text nor the selection changed: no change to report.
    Unchanged,
    /// One text change whose range now holds these units, with the caret at its end.
    Inserted(Vec<u16>),
    /// One text change whose range is now empty, with the caret at it.
    Deleted,
    /// Anything else, or a record that could not be read.
    Other,
}

impl Observed {
    /// The change to report to the core; `None` for [`Observed::Unchanged`].
    pub fn change(&self) -> Option<SelectionChange<'_>> {
        match self {
            Self::Unchanged => None,
            Self::Inserted(units) => Some(SelectionChange::Inserted(units)),
            Self::Deleted => Some(SelectionChange::Deleted),
            Self::Other => Some(SelectionChange::Other),
        }
    }
}

impl fmt::Debug for Observed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unchanged => f.write_str("Unchanged"),
            Self::Inserted(units) => f
                .debug_struct("Inserted")
                .field("units", &units.len())
                .finish(),
            Self::Deleted => f.write_str("Deleted"),
            Self::Other => f.write_str("Other"),
        }
    }
}

/// Reads what `record` shows, as the core's contract requires: exactly one text change with
/// the caret at its end is the arrival of a key that was let through (its text, or a deletion
/// when the range is empty); no text change and no selection change is no change at all; a
/// record that shows anything else, or cannot be read, is [`Observed::Other`]. Text longer
/// than one key types ([`MAX_EVENT_TEXT_UNITS`])
/// is never read in full.
pub fn observe<R: EditRecordReader>(record: &mut R) -> Observed {
    read(record).unwrap_or(Observed::Other)
}

/// [`observe`], with every failed read as an error.
fn read<R: EditRecordReader>(record: &mut R) -> Result<Observed, HostFailure> {
    let moved = record.selection_moved()?;
    // Two ranges tell one change from several.
    let changes = record.text_changes(2)?;
    let [range] = changes.as_slice() else {
        return Ok(if changes.is_empty() && !moved {
            Observed::Unchanged
        } else {
            Observed::Other
        });
    };
    // One unit more than any key types tells a key's text from longer text.
    let text = record.text(range, MAX_EVENT_TEXT_UNITS + 1)?;
    if text.len() > MAX_EVENT_TEXT_UNITS || !record.caret_at_end_of(range)? {
        return Ok(Observed::Other);
    }
    Ok(if text.is_empty() {
        Observed::Deleted
    } else {
        Observed::Inserted(text)
    })
}
