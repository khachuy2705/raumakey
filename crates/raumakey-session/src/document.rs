//! The edit-session port: the only way the core changes a document.

use core::fmt;

/// One synchronous edit of the text before the caret (matrix #40, #41).
///
/// The port must, inside a single read-write edit session:
///
/// 1. start from the selection, which must be empty (a caret);
/// 2. move the start of the selection back by [`delete_before`](Self::delete_before) UTF-16
///    units and fail with [`EditError::RangeShorter`] unless exactly that many were covered;
/// 3. fail with [`EditError::TextMismatch`] unless the covered units equal
///    [`expected`](Self::expected), the tail of the text the engine believes is before the
///    caret;
/// 4. replace the covered units with [`insert`](Self::insert) and collapse the selection to
///    the end of the inserted text.
///
/// Nothing may change in the document when the edit fails. A port receives requests from the
/// core only, and in those `expected` always holds exactly `delete_before` units, the cut
/// never splits a surrogate pair, and `insert` is well-formed UTF-16 within
/// [`MAX_INSERT_UNITS`](raumakey_model::transaction::MAX_INSERT_UNITS).
/// ([`new`](Self::new) is public so that tests of a port can build requests of their own.)
///
/// `Debug` shows only the unit counts: the request carries document text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct EditRequest<'a> {
    delete_before: u32,
    expected: &'a [u16],
    insert: &'a [u16],
}

impl<'a> EditRequest<'a> {
    /// A request that replaces `expected` (the units right before the caret) by `insert`.
    /// The caller guarantees that `expected` holds at most `u32::MAX` units.
    ///
    /// The core builds every request it hands to a port with this function. It is public
    /// only for the tests of a port, and it checks nothing else: a request built elsewhere
    /// keeps the promises listed on [`EditRequest`] only as far as its caller does.
    pub fn new(expected: &'a [u16], insert: &'a [u16]) -> Self {
        Self {
            delete_before: u32::try_from(expected.len()).unwrap_or(u32::MAX),
            expected,
            insert,
        }
    }

    /// UTF-16 units to remove before the caret.
    pub fn delete_before(&self) -> u32 {
        self.delete_before
    }

    /// The units that must be found right before the caret; exactly
    /// [`delete_before`](Self::delete_before) of them.
    pub fn expected(&self) -> &'a [u16] {
        self.expected
    }

    /// The units to insert in their place.
    pub fn insert(&self) -> &'a [u16] {
        self.insert
    }
}

impl fmt::Debug for EditRequest<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditRequest")
            .field("delete_before", &self.delete_before)
            .field("insert_units", &self.insert.len())
            .finish()
    }
}

/// Why the document refused an [`EditRequest`]. No variant carries document text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EditError {
    /// The selection covers text instead of being a caret.
    SelectionNotEmpty,
    /// The start of the selection could move back only `covered` units, fewer than
    /// requested (the text before the caret is shorter than the engine believes).
    RangeShorter {
        /// Units the range could cover.
        covered: u32,
    },
    /// The covered units differ from [`EditRequest::expected`].
    TextMismatch,
    /// The host refused the edit: a read-only or locked document, no synchronous read-write
    /// edit session, or any other failure of the edit session.
    Denied,
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::SelectionNotEmpty => "the selection is not a caret",
            Self::RangeShorter { .. } => "the text before the caret is shorter than expected",
            Self::TextMismatch => "the text before the caret differs from the expected text",
            Self::Denied => "the document refused the edit",
        })
    }
}

impl std::error::Error for EditError {}

/// A change of a document that Raumakey's own edit sessions did not make, as the COM layer
/// reads it from the edit record of `ITfTextEditSink::OnEndEdit` and reports it to
/// [`ServiceCore::selection_changed`](crate::ServiceCore::selection_changed).
///
/// [`Inserted`](Self::Inserted) and [`Deleted`](Self::Deleted) describe what a key let
/// through to the application does when it arrives in a plain text control; the core compares
/// them with that key. A record that shows any other change, or cannot be read, is
/// [`Other`](Self::Other), which always ends the word in progress. A record that shows
/// neither a text change nor a selection change (such as an edit session of another text
/// service that only sets properties) is no change of the document: the COM layer does not
/// report it.
///
/// `Debug` shows only the number of inserted units: they are document text.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SelectionChange<'a> {
    /// The record holds exactly one text change, its range now holds these units, and the
    /// selection is a caret at the end of that range. No key types more than
    /// [`MAX_EVENT_TEXT_UNITS`](raumakey_model::input::MAX_EVENT_TEXT_UNITS) units, so the COM
    /// layer may report a longer range as [`Other`](Self::Other) without reading it.
    Inserted(&'a [u16]),
    /// The record holds exactly one text change, its range is now empty (text was removed and
    /// none inserted), and the selection is a caret at that range.
    Deleted,
    /// Any other change: a selection that moved without a text change; several text
    /// changes, or one away from the caret; a selection that covers text; or a record that
    /// could not be read.
    Other,
}

impl fmt::Debug for SelectionChange<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inserted(units) => f
                .debug_struct("Inserted")
                .field("units", &units.len())
                .finish(),
            Self::Deleted => f.write_str("Deleted"),
            Self::Other => f.write_str("Other"),
        }
    }
}

/// The edit-session abstraction of one document.
///
/// The TSF implementation of the COM layer requests one synchronous read-write edit session
/// per call and performs the steps described on [`EditRequest`] inside it (the
/// `adapter::edit` module of `raumakey_tip`). Tests use a fake document. Implementations must
/// not call back into the core.
pub trait DocumentPort {
    /// Performs `request` in one edit session, or changes nothing and says why not.
    fn replace_before_caret(&mut self, request: &EditRequest<'_>) -> Result<(), EditError>;
}
