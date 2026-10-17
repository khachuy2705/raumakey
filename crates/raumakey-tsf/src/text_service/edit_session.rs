//! The edit of a key: [`DocumentPort`] over a TSF context, one synchronous read-write edit
//! session per call (matrix #40, #41).

use core::cell::Cell;

use raumakey_windows::com_server::ObjectGuard;
use raumakey_windows::error::{E_FAIL, E_UNEXPECTED};
use windows::Win32::UI::TextServices::{
    ITfContext, ITfEditSession, ITfEditSession_Impl, ITfRange, TF_ANCHOR_END, TF_ES_READWRITE,
    TF_ES_SYNC,
};
use windows::core::{ComObject, implement};

use super::context::{read_text, selection_range, set_selection};
use super::{error, guarded};
use crate::adapter::HostFailure;
use crate::adapter::edit::{EditRange, replace_before_caret};
use crate::session::{DocumentPort, EditError, EditRequest};

/// The document of a key: its context, the client id of the activation, and the counter of
/// the service's own edit sessions, which `OnEndEdit` reads to skip their changes.
pub(super) struct EditPort<'a> {
    context: Option<&'a ITfContext>,
    client_id: u32,
    own_edits: &'a Cell<u32>,
}

impl<'a> EditPort<'a> {
    pub(super) fn new(
        context: Option<&'a ITfContext>,
        client_id: u32,
        own_edits: &'a Cell<u32>,
    ) -> Self {
        Self {
            context,
            client_id,
            own_edits,
        }
    }
}

impl DocumentPort for EditPort<'_> {
    /// Requests one edit session with `TF_ES_SYNC | TF_ES_READWRITE`. When TSF ran it, its
    /// outcome decides; a request that TSF refused or could not run synchronously
    /// (`TF_E_SYNCHRONOUS`) changed nothing and is [`EditError::Denied`]. TSF reports the edit
    /// to `ITfTextEditSink::OnEndEdit` before the request returns, while the counter of own
    /// edits is raised.
    fn replace_before_caret(&mut self, request: &EditRequest<'_>) -> Result<(), EditError> {
        let Some(context) = self.context else {
            return Err(EditError::Denied);
        };
        let session = ComObject::new(ReplaceSession {
            context: context.clone(),
            expected: request.expected().to_vec(),
            insert: request.insert().to_vec(),
            armed: Cell::new(true),
            outcome: Cell::new(None),
            _counted: ObjectGuard::new(),
        });
        let interface: ITfEditSession = session.to_interface();
        self.own_edits.set(self.own_edits.get().saturating_add(1));
        // SAFETY: a live context and edit session of this thread, and the client id TSF gave
        // the activation.
        let requested = unsafe {
            context.RequestEditSession(self.client_id, &interface, TF_ES_SYNC | TF_ES_READWRITE)
        };
        self.own_edits.set(self.own_edits.get().saturating_sub(1));
        // A session that TSF still held could otherwise run later and edit after the key
        // went through.
        session.armed.set(false);
        drop(requested);
        session.outcome.take().unwrap_or(Err(EditError::Denied))
    }
}

/// The edit session of one [`EditRequest`].
#[implement(ITfEditSession, Agile = false)]
struct ReplaceSession {
    context: ITfContext,
    expected: Vec<u16>,
    insert: Vec<u16>,
    /// True while the request is in progress: the session edits only then, and once.
    armed: Cell<bool>,
    /// What the edit did, once it ran.
    outcome: Cell<Option<Result<(), EditError>>>,
    _counted: ObjectGuard,
}

impl ITfEditSession_Impl for ReplaceSession_Impl {
    fn DoEditSession(&self, ec: u32) -> windows::core::Result<()> {
        guarded(|| {
            if !self.armed.replace(false) {
                return Err(error(E_UNEXPECTED));
            }
            let outcome = match selection_range(&self.context, ec) {
                Some(range) => replace_before_caret(
                    &mut TsfRange {
                        context: &self.context,
                        range,
                        ec,
                    },
                    &self.expected,
                    &self.insert,
                ),
                None => Err(EditError::Denied),
            };
            self.outcome.set(Some(outcome));
            outcome.map_err(|_| error(E_FAIL))
        })
    }
}

/// [`EditRange`] over the selection range of an edit session.
struct TsfRange<'a> {
    context: &'a ITfContext,
    range: ITfRange,
    ec: u32,
}

impl EditRange for TsfRange<'_> {
    fn is_empty(&self) -> Result<bool, HostFailure> {
        // SAFETY: a range of this session's context and the session's cookie.
        unsafe { self.range.IsEmpty(self.ec) }
            .map(|empty| empty.as_bool())
            .map_err(|_| HostFailure)
    }

    fn extend_back(&mut self, units: u32) -> Result<u32, HostFailure> {
        let count = i32::try_from(units).map_err(|_| HostFailure)?;
        let mut moved = 0_i32;
        // SAFETY: as above, a writable count, and no halt condition.
        unsafe {
            self.range
                .ShiftStart(self.ec, -count, &mut moved, core::ptr::null())
        }
        .map_err(|_| HostFailure)?;
        // A backward shift reports a negative count.
        Ok(moved.unsigned_abs())
    }

    fn read(&self, max_units: usize) -> Result<Vec<u16>, HostFailure> {
        read_text(&self.range, self.ec, max_units)
    }

    fn write(&mut self, units: &[u16]) -> Result<(), HostFailure> {
        // SAFETY: a range of this session's context and the read-write cookie.
        unsafe { self.range.SetText(self.ec, 0, units) }.map_err(|_| HostFailure)
    }

    fn select_end(&mut self) -> Result<(), HostFailure> {
        // SAFETY: as above; the copy leaves the range itself as it is.
        let caret = unsafe { self.range.Clone() }.map_err(|_| HostFailure)?;
        // SAFETY: a range of this session's context and the read-write cookie.
        unsafe { caret.Collapse(self.ec, TF_ANCHOR_END) }.map_err(|_| HostFailure)?;
        set_selection(self.context, self.ec, caret).map_err(|_| HostFailure)
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use super::EditPort;
    use crate::session::{DocumentPort, EditError, EditRequest};

    #[test]
    fn a_key_without_a_context_is_denied_without_an_edit_session() {
        let own_edits = Cell::new(0);
        let mut port = EditPort::new(None, 1, &own_edits);
        let (expected, insert) = ([u16::from(b'a')], [0x00E2]);
        assert_eq!(
            port.replace_before_caret(&EditRequest::new(&expected, &insert)),
            Err(EditError::Denied)
        );
        assert_eq!(own_edits.get(), 0, "no edit session was counted");
    }
}
