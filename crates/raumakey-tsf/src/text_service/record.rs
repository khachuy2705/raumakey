//! [`EditRecordReader`] over the `ITfEditRecord` of `ITfTextEditSink::OnEndEdit`.

use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditRecord, ITfRange, TF_ANCHOR_END, TF_GTP_INCL_TEXT,
};

use super::context::{read_text, selection_range};
use crate::adapter::HostFailure;
use crate::adapter::record::EditRecordReader;

/// The record of one `OnEndEdit` call, read with its read-only cookie.
pub(super) struct TsfRecord<'a> {
    context: &'a ITfContext,
    record: &'a ITfEditRecord,
    ec: u32,
}

impl<'a> TsfRecord<'a> {
    pub(super) fn new(context: &'a ITfContext, record: &'a ITfEditRecord, ec: u32) -> Self {
        Self {
            context,
            record,
            ec,
        }
    }
}

impl EditRecordReader for TsfRecord<'_> {
    type Range = ITfRange;

    fn selection_moved(&mut self) -> Result<bool, HostFailure> {
        // SAFETY: the record of this OnEndEdit call.
        unsafe { self.record.GetSelectionStatus() }
            .map(|moved| moved.as_bool())
            .map_err(|_| HostFailure)
    }

    fn text_changes(&mut self, max: usize) -> Result<Vec<ITfRange>, HostFailure> {
        // SAFETY: as above; text changes only, no properties.
        let ranges = unsafe { self.record.GetTextAndPropertyUpdates(TF_GTP_INCL_TEXT, &[]) }
            .map_err(|_| HostFailure)?;
        let mut changes = Vec::new();
        while changes.len() < max {
            let mut next = [None];
            let mut fetched = 0_u32;
            // SAFETY: a live enumerator and writable out parameters for one range.
            unsafe { ranges.Next(&mut next, &mut fetched) }.map_err(|_| HostFailure)?;
            let [range] = next;
            match range {
                Some(range) if fetched == 1 => changes.push(range),
                _ => break,
            }
        }
        Ok(changes)
    }

    fn text(&mut self, range: &ITfRange, max_units: usize) -> Result<Vec<u16>, HostFailure> {
        read_text(range, self.ec, max_units)
    }

    fn caret_at_end_of(&mut self, range: &ITfRange) -> Result<bool, HostFailure> {
        let selection = selection_range(self.context, self.ec).ok_or(HostFailure)?;
        // SAFETY: a range of this context and the read-only cookie of OnEndEdit.
        let empty = unsafe { selection.IsEmpty(self.ec) }.map_err(|_| HostFailure)?;
        if !empty.as_bool() {
            return Ok(false);
        }
        // SAFETY: two ranges of this context and the read-only cookie.
        let order = unsafe { selection.CompareStart(self.ec, range, TF_ANCHOR_END) }
            .map_err(|_| HostFailure)?;
        Ok(order == 0)
    }
}
