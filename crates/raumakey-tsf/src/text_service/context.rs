//! Small reads of TSF objects: identities, the focused and the base context, selections,
//! text, and the window of a view.

use core::mem::ManuallyDrop;

use raumakey_windows::foreground::window_class_name;
use windows::Win32::UI::TextServices::{
    ITfContext, ITfDocumentMgr, ITfRange, ITfThreadMgr, TF_AE_NONE, TF_DEFAULT_SELECTION,
    TF_SELECTION, TF_SELECTIONSTYLE,
};
use windows::core::{BOOL, IUnknown, Interface};

use crate::adapter::HostFailure;

/// The COM identity of `object`: the address of its `IUnknown`, which stays the same for
/// as long as the object lives.
pub(super) fn identity(object: &impl Interface) -> Option<usize> {
    object
        .cast::<IUnknown>()
        .ok()
        .map(|unknown| unknown.as_raw().addr())
}

/// The top context of `manager`, the one that receives keys. A null result is an error
/// that carries `S_OK` in the `windows` crate; both read as `None`.
pub(super) fn top_context(manager: &ITfDocumentMgr) -> Option<ITfContext> {
    // SAFETY: a live document manager of this thread.
    unsafe { manager.GetTop() }.ok()
}

/// The base context of `manager`, the bottom of its stack.
pub(super) fn base_context(manager: &ITfDocumentMgr) -> Option<ITfContext> {
    // SAFETY: a live document manager of this thread.
    unsafe { manager.GetBase() }.ok()
}

/// The top context of the document manager that has the focus of `thread_mgr`.
pub(super) fn focused_context(thread_mgr: &ITfThreadMgr) -> Option<ITfContext> {
    // SAFETY: a live thread manager of this thread.
    let manager = unsafe { thread_mgr.GetFocus() }.ok()?;
    top_context(&manager)
}

/// The identity of the document manager of `context`.
pub(super) fn manager_of(context: &ITfContext) -> Option<usize> {
    // SAFETY: a live context of this thread.
    let manager = unsafe { context.GetDocumentMgr() }.ok()?;
    identity(&manager)
}

/// The class of the window of the active view of `context`, when it has one.
pub(super) fn window_class(context: &ITfContext) -> Option<String> {
    // SAFETY: a live context of this thread.
    let view = unsafe { context.GetActiveView() }.ok()?;
    // SAFETY: a live view of this thread.
    let window = unsafe { view.GetWnd() }.ok()?;
    if window.is_invalid() {
        return None;
    }
    window_class_name(window.0.addr().cast_signed())
}

/// The default selection of `context` as a range, read with the cookie `ec`.
pub(super) fn selection_range(context: &ITfContext, ec: u32) -> Option<ITfRange> {
    let mut selection = [TF_SELECTION::default()];
    let mut fetched = 0;
    // SAFETY: a valid cookie of an edit session or edit notification, and writable out
    // parameters.
    let result =
        unsafe { context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selection, &mut fetched) };
    let [selection] = selection;
    // Whatever GetSelection stored is owned here, so that it is released.
    let range = ManuallyDrop::into_inner(selection.range);
    if result.is_ok() && fetched == 1 {
        range
    } else {
        None
    }
}

/// Makes `caret` the selection of `context` (cookie `ec` of a read-write session).
pub(super) fn set_selection(
    context: &ITfContext,
    ec: u32,
    caret: ITfRange,
) -> windows::core::Result<()> {
    let selection = [TF_SELECTION {
        range: ManuallyDrop::new(Some(caret)),
        style: TF_SELECTIONSTYLE {
            ase: TF_AE_NONE,
            fInterimChar: BOOL::from(false),
        },
    }];
    // SAFETY: a valid read-write cookie and a selection whose range lives for the call.
    let result = unsafe { context.SetSelection(ec, &selection) };
    let [selection] = selection;
    drop(ManuallyDrop::into_inner(selection.range));
    result
}

/// At most `max_units` units of the text of `range`, from its start.
pub(super) fn read_text(
    range: &ITfRange,
    ec: u32,
    max_units: usize,
) -> Result<Vec<u16>, HostFailure> {
    let mut buffer = vec![0_u16; max_units];
    let mut count = 0_u32;
    // SAFETY: a range of a context of this thread, a valid cookie, and a writable buffer of
    // `max_units` units; GetText writes at most that many.
    unsafe { range.GetText(ec, 0, &mut buffer, &mut count) }.map_err(|_| HostFailure)?;
    let count = usize::try_from(count).unwrap_or(usize::MAX).min(max_units);
    buffer.truncate(count);
    Ok(buffer)
}
