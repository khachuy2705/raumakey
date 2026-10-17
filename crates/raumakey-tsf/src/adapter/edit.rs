//! The edit of one synchronous read-write edit session (matrix #40, #41).

use crate::adapter::HostFailure;
use crate::session::EditError;

/// The range operations one edit needs: `ITfRange` and `ITfContext::SetSelection` in the COM
/// layer, a fake in tests. The range starts as the selection of the document.
pub trait EditRange {
    /// True when the range is empty: the selection is a caret.
    fn is_empty(&self) -> Result<bool, HostFailure>;

    /// Moves the start of the range back by at most `units` UTF-16 units and returns how
    /// many it moved (`ITfRange::ShiftStart` with a negative count and no halt condition).
    fn extend_back(&mut self, units: u32) -> Result<u32, HostFailure>;

    /// At most `max_units` units of the range's text, from its start (`ITfRange::GetText`).
    fn read(&self, max_units: usize) -> Result<Vec<u16>, HostFailure>;

    /// Replaces the text of the range with `units` (`ITfRange::SetText`); the range then
    /// covers exactly the new units.
    fn write(&mut self, units: &[u16]) -> Result<(), HostFailure>;

    /// Makes the selection a caret at the end of the range, leaving the range as it is
    /// (`ITfContext::SetSelection` with a collapsed copy).
    fn select_end(&mut self) -> Result<(), HostFailure>;
}

/// Replaces the units `expected` right before the caret with `insert` and puts the caret at
/// the end of the insert, as [`EditRequest`](crate::session::EditRequest) describes:
///
/// 1. the selection must be a caret ([`EditError::SelectionNotEmpty`]);
/// 2. its start moves back by `expected.len()` units, and exactly that many must be covered
///    ([`EditError::RangeShorter`]);
/// 3. the covered units must equal `expected` ([`EditError::TextMismatch`]); a cut through a
///    surrogate pair never does, since `expected` never starts with a low surrogate;
/// 4. the covered units are replaced with `insert` and the selection becomes a caret at the
///    end of the insert.
///
/// Any failure of the host is [`EditError::Denied`]. Nothing changes in the document when the
/// edit fails: a failed write changes nothing, and when the caret cannot be placed after a
/// write, the covered units are written back.
///
/// # Errors
///
/// The [`EditError`] of the first step that failed.
pub fn replace_before_caret(
    range: &mut impl EditRange,
    expected: &[u16],
    insert: &[u16],
) -> Result<(), EditError> {
    let denied = |_: HostFailure| EditError::Denied;
    if !range.is_empty().map_err(denied)? {
        return Err(EditError::SelectionNotEmpty);
    }
    if !expected.is_empty() {
        let wanted = u32::try_from(expected.len()).map_err(|_| EditError::Denied)?;
        let covered = range.extend_back(wanted).map_err(denied)?;
        if covered != wanted {
            return Err(EditError::RangeShorter { covered });
        }
        if range.read(expected.len()).map_err(denied)? != expected {
            return Err(EditError::TextMismatch);
        }
    }
    range.write(insert).map_err(denied)?;
    if range.select_end().is_err() {
        // The caret cannot follow the insert: put the covered units back, so that the
        // failed edit changes nothing, and let the key through.
        let _ = range.write(expected);
        return Err(EditError::Denied);
    }
    Ok(())
}
