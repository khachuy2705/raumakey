//! Modal dialog boxes built from in-memory templates: the controller's settings panel and
//! macro list (plan §10.2). [`dialog_template`](crate::dialog_template) lays a dialog out; this
//! module shows it and routes its messages to a handler.
//!
//! [`RESERVED_FIRST`] and [`is_reserved`] (the private messages the dialog procedure keeps for
//! itself) and [`notifications`] (the notification codes handlers match on) are pure. On
//! Windows:
//!
//! - [`run_dialog`] shows a modal dialog (`DialogBoxIndirectParamW`) owned by an optional
//!   window of the calling thread, and returns once it ended, with the result the dialog ended
//!   with. The first call registers the common controls the templates use
//!   (`InitCommonControlsEx`: the standard classes and the list view).
//! - Each dialog routes its messages to its own [`DialogHandler`], a trait object that travels
//!   to the dialog procedure in the `LPARAM` of `WM_INITDIALOG`, is reachable from the dialog's
//!   `DWLP_USER` slot from then on, and is dropped after the dialog is destroyed, when
//!   [`run_dialog`] returns. Only the `WM_INITDIALOG` that `DialogBoxIndirectParamW` sends for
//!   the [`run_dialog`] call running on the thread installs a handler; a forged one (another
//!   thread or process cannot know the pointer) is ignored.
//! - The handler gets a [`Dialog`] in every callback: a view of the dialog window with helpers
//!   for its controls (check boxes, texts, drop-down lists, list views, the icons of its title
//!   bar and taskbar button, focus, message boxes owned by the dialog, ending it).
//! - [`spawn_dialog`] runs a dialog without owner on a thread of its own, the way
//!   [`spawn_message_box`](crate::dialog::spawn_message_box) runs message boxes: the caller's
//!   thread goes on handling its own messages, and a [`DetachedDialog`] posts private messages
//!   to the dialog, brings it to the front, or dismisses it from any thread. An unowned dialog
//!   has its own taskbar button, so it cannot get lost behind other windows.
//!
//! **Messages.** `WM_COMMAND` reaches [`DialogHandler::command`] with the control id and the
//! notification code, except `IDCANCEL` with `BN_CLICKED` (the Cancel button, Esc, and the
//! close box through `WM_CLOSE`) and `WM_CLOSE` itself: they ask
//! [`DialogHandler::close_requested`], which ends the dialog with `IDCANCEL` unless the handler
//! vetoes. `IDOK` (the default button, Enter) is an ordinary command: the handler ends the
//! dialog with [`Dialog::end`] once it accepted the input. `WM_NOTIFY` reaches
//! [`DialogHandler::notify`] with the id and code of its `NMHDR`. Private messages
//! ([`AppMessage`](crate::window::AppMessage)) below [`RESERVED_FIRST`] reach
//! [`DialogHandler::app_message`]. Everything else gets the default dialog processing.
//!
//! **Threads.** A dialog belongs to the thread that runs it. Its handler runs only there, and
//! [`Dialog`] (valid during one callback) is neither `Send` nor `Sync`. Other threads talk to a
//! detached dialog through its [`DetachedDialog`] only.
//!
//! **Re-entrancy.** While a handler callback runs, messages that reach the dialog procedure
//! re-entrantly do not reach the handler: they get the default dialog processing (the
//! procedure answers `FALSE`), as [`window`](crate::window) does for windows. That covers the
//! notifications the handler's own changes cause (`EN_CHANGE` after [`Dialog::set_text`],
//! `LVN_ITEMCHANGED` after [`Dialog::list_select`]; `BM_SETCHECK` and `CB_SETCURSEL` send
//! none), so programmatic updates never loop back into the handler, and everything a modal
//! loop run by the handler dispatches ([`Dialog::message_box`], a nested [`run_dialog`]). A
//! private message posted to the dialog during such a modal loop is therefore lost to the
//! handler: callers keep shared state and re-read it once their message box returns. The
//! dialog procedure itself handles its reserved messages even then, so a dialog can be
//! dismissed or brought to the front while its handler waits for a message box.
//!
//! **Dismissing.** [`DetachedDialog::dismiss`] posts a reserved message that the dialog
//! procedure answers itself with `EndDialog(IDCANCEL)`, without asking the handler. It also
//! dismisses the dialogs that the handler runs with this dialog as owner, and closes the
//! message box shown through [`Dialog::message_box`] with its
//! [`dismiss_answer`](crate::dialog::MessageKind::dismiss_answer) (`message_box` then fails
//! with `E_ABORT`). A dialog dismissed before its window handle is published (that happens
//! once `init` returned) ends during `WM_INITDIALOG` and is never shown.
//!
//! **Forged messages.** Any process of the desktop at the same or a higher integrity level may
//! send or post messages to a dialog, as to any window. The reserved and private messages carry
//! no pointers, and a forged `WM_INITDIALOG` is ignored (see above). `WM_NOTIFY` carries a
//! pointer to an `NMHDR` that Windows neither marshals between processes nor refuses to post,
//! so a forged one may point anywhere: the dialog procedure copies the header with
//! `ReadProcessMemory`, which fails instead of faulting on memory that cannot be read, and
//! hands it to the handler only when it names a control of the dialog by its handle and id.
//!
//! **Panics.** The dialog procedure runs inside [`guard_value`](crate::panic::guard_value): a
//! panicking callback leaves its message unhandled (`FALSE`), the handler is not left borrowed,
//! and no unwind reaches Windows. A panic in `init` still publishes the window handle, so the
//! dialog can be dismissed.

use crate::window::WM_APP_LAST;

/// The first private message number that the dialog procedure keeps for itself: the top 16
/// numbers of the `WM_APP` range, `0xBFF0..=0xBFFF`. [`DetachedDialog::post`] refuses them, and
/// [`DialogHandler::app_message`] never receives them, so callers number their private messages
/// below this.
pub const RESERVED_FIRST: u32 = WM_APP_LAST - 0x0F;

/// True for the private messages the dialog procedure keeps for itself
/// ([`RESERVED_FIRST`]`..=`[`WM_APP_LAST`]).
pub const fn is_reserved(message: u32) -> bool {
    matches!(message, RESERVED_FIRST..=WM_APP_LAST)
}

/// Ends the dialog without asking its handler (see "Dismissing" in the module documentation).
#[cfg(windows)]
const DISMISS: u32 = WM_APP_LAST;
/// Restores the dialog if it is minimized and makes it the foreground window.
#[cfg(windows)]
const BRING_TO_FRONT: u32 = WM_APP_LAST - 1;

/// Notification codes that dialog handlers match on: the high word of `wParam` of a
/// `WM_COMMAND` for the `u16` ones, `NMHDR::code` of a `WM_NOTIFY` for the `i32` ones.
pub mod notifications {
    /// `BN_CLICKED`: a button was clicked (also a check box, which toggled itself).
    pub const BN_CLICKED: u16 = 0;
    /// `CBN_SELCHANGE`: the user selected another item of a drop-down list.
    pub const CBN_SELCHANGE: u16 = 1;
    /// `EN_CHANGE`: the text of an edit changed.
    pub const EN_CHANGE: u16 = 0x0300;
    /// `LVN_ITEMCHANGED`: an item of a list view changed, such as its selection; one change of
    /// the selection reports each item that lost or gained it.
    pub const LVN_ITEMCHANGED: i32 = -101;
    /// `NM_DBLCLK`: the user double-clicked the control.
    pub const NM_DBLCLK: i32 = -3;
}

#[cfg(windows)]
pub use win::{DetachedDialog, Dialog, DialogHandler, run_dialog, spawn_dialog};

#[cfg(windows)]
mod win {
    use core::cell::{Cell, RefCell};
    use core::ffi::c_void;
    use core::fmt;
    use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
    use std::sync::{Arc, OnceLock};

    use raumakey_model::identity::PRODUCT_NAME;
    use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
    use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetCurrentThreadId};
    use windows::Win32::UI::Controls::{
        HDM_GETITEMCOUNT, ICC_LISTVIEW_CLASSES, ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX,
        InitCommonControlsEx, LIST_VIEW_ITEM_STATE_FLAGS, LVCF_TEXT, LVCF_WIDTH, LVCOLUMNW,
        LVIF_TEXT, LVIS_FOCUSED, LVIS_SELECTED, LVITEMW, LVM_DELETEALLITEMS, LVM_DELETECOLUMN,
        LVM_ENSUREVISIBLE, LVM_GETHEADER, LVM_GETNEXTITEM, LVM_INSERTCOLUMNW, LVM_INSERTITEMW,
        LVM_SETEXTENDEDLISTVIEWSTYLE, LVM_SETITEMSTATE, LVM_SETITEMTEXTW, LVNI_SELECTED,
        LVS_EX_FULLROWSELECT, NMHDR,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_GETCHECK, BM_SETCHECK, CB_ADDSTRING, CB_GETCURSEL, CB_RESETCONTENT, CB_SETCURSEL,
        DLGTEMPLATE, DialogBoxIndirectParamW, EndDialog, EnumThreadWindows, GW_OWNER,
        GetClassNameW, GetDlgCtrlID, GetDlgItem, GetLastActivePopup, GetWindow, ICON_BIG,
        ICON_SMALL, IsChild, IsIconic, IsWindow, MapDialogRect, PostMessageW, SW_RESTORE,
        SendDlgItemMessageW, SendMessageW, SetDlgItemTextW, SetForegroundWindow, ShowWindow,
        WINDOW_LONG_PTR_INDEX, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DPICHANGED, WM_GETTEXT,
        WM_GETTEXTLENGTH, WM_INITDIALOG, WM_NEXTDLGCTL, WM_NOTIFY, WM_SETICON,
    };
    use windows::core::{BOOL, PWSTR};

    use super::notifications::BN_CLICKED;
    use super::{BRING_TO_FRONT, DISMISS, is_reserved};
    use crate::dialog::{MessageKind, MessageResponse, message_box};
    use crate::dialog_template::{IDCANCEL, TemplateBytes};
    use crate::error::{E_ABORT, E_FAIL, E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::icon::{Icon, IconKind, icon_size, icon_size_for_dpi, icon_size_for_window};
    use crate::module::Module;
    use crate::panic::{guard_bool, guard_value};
    use crate::strings::WideString;
    use crate::window::AppMessage;

    /// What a dialog procedure answers for a message it handled.
    const TRUE: isize = 1;
    /// What a dialog procedure answers for a message left to the default dialog processing.
    const FALSE: isize = 0;

    /// `DWLP_USER`: the slot of the dialog's extra bytes that belongs to the application, after
    /// `DWLP_MSGRESULT` (an `LRESULT`) and `DWLP_DLGPROC` (a pointer).
    const DWLP_USER: WINDOW_LONG_PTR_INDEX = WINDOW_LONG_PTR_INDEX(2 * size_of::<isize>() as i32);

    /// The result of a dialog that was closed without the handler's own result: `IDCANCEL`.
    const CANCELLED: isize = IDCANCEL as isize;

    /// `BST_CHECKED`: the state of a checked button.
    const BST_CHECKED: isize = 1;

    /// The window class of dialog boxes, which message boxes are too.
    const DIALOG_CLASS: &str = "#32770";

    /// Name of the threads that run detached dialogs.
    const THREAD_NAME: &str = "raumakey-dialog";

    /// Receives the messages of one dialog, on the dialog's thread (see the module
    /// documentation for which messages reach which method). Every method does nothing by
    /// default, and [`close_requested`](Self::close_requested) agrees.
    ///
    /// The dialog's thread is a UI thread: a handler returns quickly and does not block on
    /// files, the network, or IPC (design §11). Slow work belongs to another thread, which
    /// posts its result back to the dialog ([`DetachedDialog::post`]).
    pub trait DialogHandler {
        /// The dialog was created (`WM_INITDIALOG`) and is not shown yet: fill the controls.
        /// The changes made here notify nobody.
        fn init(&mut self, dialog: &Dialog<'_>) {
            let _ = dialog;
        }

        /// Control `id` sent notification `code` (`WM_COMMAND`), such as
        /// [`BN_CLICKED`](super::notifications::BN_CLICKED) of a button or check box,
        /// [`CBN_SELCHANGE`](super::notifications::CBN_SELCHANGE), or
        /// [`EN_CHANGE`](super::notifications::EN_CHANGE). `IDCANCEL` clicks go to
        /// [`close_requested`](Self::close_requested) instead.
        fn command(&mut self, dialog: &Dialog<'_>, id: u16, code: u16) {
            let _ = (dialog, id, code);
        }

        /// Control `id` sent notification `code` (`WM_NOTIFY`), such as
        /// [`LVN_ITEMCHANGED`](super::notifications::LVN_ITEMCHANGED) or
        /// [`NM_DBLCLK`](super::notifications::NM_DBLCLK). The dialog answers 0, the default
        /// for every notification.
        fn notify(&mut self, dialog: &Dialog<'_>, id: u16, code: i32) {
            let _ = (dialog, id, code);
        }

        /// The private `message` arrived with `wparam` and `lparam` (posted with
        /// [`DetachedDialog::post`] or by other code of the process). Reserved messages
        /// ([`super::is_reserved`]) never arrive here.
        fn app_message(
            &mut self,
            dialog: &Dialog<'_>,
            message: AppMessage,
            wparam: usize,
            lparam: isize,
        ) {
            let _ = (dialog, message, wparam, lparam);
        }

        /// The user asked to close the dialog (Cancel, Esc, the close box) or something sent
        /// it `WM_CLOSE`. `true` ends the dialog with `IDCANCEL`; `false` keeps it open (for
        /// example after asking about unsaved changes with [`Dialog::message_box`]).
        fn close_requested(&mut self, dialog: &Dialog<'_>) -> bool {
            let _ = dialog;
            true
        }
    }

    /// The dialog window during one handler callback, with helpers for its controls. Every
    /// control is addressed by its id ([`dialog_template`](crate::dialog_template)); helpers
    /// that cannot fail visibly do nothing for an id that names no control.
    ///
    /// It is valid during the callback only and belongs to the dialog's thread, so it is
    /// neither `Send`:
    ///
    /// ```compile_fail
    /// fn assert_send<T: Send>() {}
    /// assert_send::<raumakey_windows::dialog_box::Dialog<'static>>();
    /// ```
    ///
    /// nor `Sync`:
    ///
    /// ```compile_fail
    /// fn assert_sync<T: Sync>() {}
    /// assert_sync::<raumakey_windows::dialog_box::Dialog<'static>>();
    /// ```
    pub struct Dialog<'a> {
        hwnd: HWND,
        state: &'a DialogState,
    }

    impl fmt::Debug for Dialog<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Dialog")
                .field("hwnd", &self.hwnd.0)
                .finish_non_exhaustive()
        }
    }

    impl Dialog<'_> {
        /// The dialog window, for calls on this thread during the callback.
        pub fn hwnd(&self) -> HWND {
            self.hwnd
        }

        /// The control `id` (`GetDlgItem`); fails when no control has that id.
        pub fn item(&self, id: u16) -> WinResult<HWND> {
            // SAFETY: only reads the dialog's list of children.
            unsafe { GetDlgItem(Some(self.hwnd), i32::from(id)) }.context("GetDlgItem")
        }

        /// Sends `message` to control `id` (`SendDlgItemMessageW`): 0 when there is no such
        /// control.
        fn send_item(&self, id: u16, message: u32, wparam: usize, lparam: isize) -> isize {
            // SAFETY: the callers pass plain values, or pointers to data that outlives the call
            // (the control is a window of this thread, so the call returns before they go).
            unsafe {
                SendDlgItemMessageW(
                    self.hwnd,
                    i32::from(id),
                    message,
                    WPARAM(wparam),
                    LPARAM(lparam),
                )
            }
            .0
        }

        /// True when check box `id` is checked (`BM_GETCHECK`).
        pub fn checked(&self, id: u16) -> bool {
            self.send_item(id, BM_GETCHECK, 0, 0) == BST_CHECKED
        }

        /// Checks or clears check box `id` (`BM_SETCHECK`, which sends no notification).
        pub fn set_checked(&self, id: u16, checked: bool) {
            self.send_item(id, BM_SETCHECK, usize::from(checked), 0);
        }

        /// The text of control `id` (`WM_GETTEXT`): an edit's content, a label's text, the
        /// selected item of a drop-down list. Unpaired surrogates become U+FFFD. It may be
        /// user content: never log it.
        pub fn text(&self, id: u16) -> WinResult<String> {
            let control = self.item(id)?;
            // SAFETY: a control of this thread; no pointers.
            let length = unsafe { SendMessageW(control, WM_GETTEXTLENGTH, None, None) }.0;
            let length = usize::try_from(length).unwrap_or(0);
            let mut buffer = vec![0_u16; length + 1];
            // SAFETY: the buffer is writable for the length passed and outlives the call, which
            // the control answers on this thread.
            let copied = unsafe {
                SendMessageW(
                    control,
                    WM_GETTEXT,
                    Some(WPARAM(buffer.len())),
                    Some(LPARAM(buffer.as_mut_ptr() as isize)),
                )
            }
            .0;
            let copied = usize::try_from(copied).unwrap_or(0).min(length);
            Ok(String::from_utf16_lossy(
                buffer.get(..copied).unwrap_or(&[]),
            ))
        }

        /// Sets the text of control `id` (`SetDlgItemTextW`). An edit reports `EN_CHANGE`,
        /// which does not reach the busy handler. Fails with `E_INVALIDARG` for a text with a
        /// NUL, and when there is no such control.
        pub fn set_text(&self, id: u16, text: &str) -> WinResult<()> {
            let text = WideString::new(text)?;
            // SAFETY: the text is NUL-terminated and outlives the call; the control is a
            // window of this thread.
            unsafe { SetDlgItemTextW(self.hwnd, i32::from(id), text.as_pcwstr()) }
                .context("SetDlgItemTextW")
        }

        /// Enables or disables control `id` (`EnableWindow`).
        pub fn set_enabled(&self, id: u16, enabled: bool) {
            if let Ok(control) = self.item(id) {
                // SAFETY: a control of this thread; the answer is the previous state.
                let _ = unsafe { EnableWindow(control, enabled) };
            }
        }

        /// Moves the keyboard focus to control `id` (`WM_NEXTDLGCTL`, which also moves the
        /// default-button highlight as Tab would). Called from
        /// [`DialogHandler::init`], it replaces the default focus (the first control with
        /// `WS_TABSTOP`).
        pub fn focus(&self, id: u16) {
            if let Ok(control) = self.item(id) {
                // SAFETY: a message to this thread's dialog; `lParam` 1 means that `wParam` is
                // the control's handle.
                unsafe {
                    SendMessageW(
                        self.hwnd,
                        WM_NEXTDLGCTL,
                        Some(WPARAM(control.0 as usize)),
                        Some(LPARAM(1)),
                    )
                };
                self.state.focus_moved.set(true);
            }
        }

        /// Replaces the items of drop-down list `id` with `items`, in order (`CB_RESETCONTENT`,
        /// `CB_ADDSTRING`); nothing is selected afterwards. Fails with `E_INVALIDARG`, before
        /// anything changes, when an item contains a NUL; fails when there is no such control
        /// or the list runs out of memory.
        pub fn combo_set_items(&self, id: u16, items: &[&str]) -> WinResult<()> {
            let items = items
                .iter()
                .map(|item| WideString::new(item))
                .collect::<WinResult<Vec<_>>>()?;
            let control = self.item(id)?;
            // SAFETY: a control of this thread; no pointers.
            unsafe { SendMessageW(control, CB_RESETCONTENT, None, None) };
            for item in &items {
                // SAFETY: the item is NUL-terminated and outlives the call; the combo box
                // copies it.
                let index = unsafe {
                    SendMessageW(
                        control,
                        CB_ADDSTRING,
                        None,
                        Some(LPARAM(item.as_pcwstr().0 as isize)),
                    )
                }
                .0;
                if index < 0 {
                    return Err(WinError::new(E_FAIL, "CB_ADDSTRING"));
                }
            }
            Ok(())
        }

        /// Selects item `index` of drop-down list `id`, or nothing for `None`
        /// (`CB_SETCURSEL`, which sends no notification). An index past the last item fails
        /// with `E_INVALIDARG` and leaves nothing selected.
        pub fn combo_select(&self, id: u16, index: Option<usize>) -> WinResult<()> {
            let control = self.item(id)?;
            // -1 (all bits set) selects nothing.
            let wparam = index.unwrap_or(usize::MAX);
            // SAFETY: a control of this thread; no pointers.
            let selected =
                unsafe { SendMessageW(control, CB_SETCURSEL, Some(WPARAM(wparam)), None) }.0;
            if index.is_some() && selected < 0 {
                return Err(WinError::new(E_INVALIDARG, "CB_SETCURSEL"));
            }
            Ok(())
        }

        /// The selected item of drop-down list `id` (`CB_GETCURSEL`); `None` when nothing is
        /// selected or there is no such control.
        pub fn combo_selected(&self, id: u16) -> Option<usize> {
            if self.item(id).is_err() {
                return None;
            }
            usize::try_from(self.send_item(id, CB_GETCURSEL, 0, 0)).ok()
        }

        /// Replaces the columns of list view `id` with `columns`: each a header text and a
        /// width in dialog units, converted to pixels for the dialog's font and DPI
        /// (`MapDialogRect`). Also turns on full-row selection (`LVS_EX_FULLROWSELECT`). Fails
        /// with `E_INVALIDARG`, before anything changes, for a text with a NUL or a negative
        /// width; fails when there is no such control or a column cannot be inserted.
        pub fn list_set_columns(&self, id: u16, columns: &[(&str, i16)]) -> WinResult<()> {
            let columns = columns
                .iter()
                .map(|&(text, width)| Ok((WideString::new(text)?, self.dlu_width(width)?)))
                .collect::<WinResult<Vec<_>>>()?;
            let list = self.item(id)?;
            let full_row = LVS_EX_FULLROWSELECT as usize;
            // SAFETY: a control of this thread; no pointers.
            unsafe {
                SendMessageW(
                    list,
                    LVM_SETEXTENDEDLISTVIEWSTYLE,
                    Some(WPARAM(full_row)),
                    Some(LPARAM(full_row as isize)),
                )
            };
            for column in (0..column_count(list)).rev() {
                // SAFETY: a control of this thread; no pointers.
                unsafe { SendMessageW(list, LVM_DELETECOLUMN, Some(WPARAM(column)), None) };
            }
            for (index, (text, width)) in columns.iter().enumerate() {
                let column = LVCOLUMNW {
                    mask: LVCF_TEXT | LVCF_WIDTH,
                    cx: *width,
                    pszText: PWSTR(text.as_pcwstr().0.cast_mut()),
                    ..Default::default()
                };
                // SAFETY: the column and its text outlive the call; the list view copies the
                // text and never writes through the pointer when inserting.
                let inserted = unsafe {
                    SendMessageW(
                        list,
                        LVM_INSERTCOLUMNW,
                        Some(WPARAM(index)),
                        Some(LPARAM(&raw const column as isize)),
                    )
                }
                .0;
                if inserted < 0 {
                    return Err(WinError::new(E_FAIL, "LVM_INSERTCOLUMNW"));
                }
            }
            Ok(())
        }

        /// `width` dialog units as pixels of this dialog; negative widths fail with
        /// `E_INVALIDARG`.
        fn dlu_width(&self, width: i16) -> WinResult<i32> {
            if width < 0 {
                return Err(WinError::new(E_INVALIDARG, "list view column width"));
            }
            let mut rect = RECT {
                right: i32::from(width),
                ..Default::default()
            };
            // SAFETY: `rect` is a valid in/out buffer; the dialog is a window of this thread.
            unsafe { MapDialogRect(self.hwnd, &mut rect) }.context("MapDialogRect")?;
            Ok(rect.right)
        }

        /// Replaces the rows of list view `id` with `rows`: each row's cells go to the columns
        /// in order (a row needs no more cells than there are columns). Nothing is selected
        /// afterwards. Fails with `E_INVALIDARG`, before anything changes, for a cell with a
        /// NUL; fails when there is no such control or a row or cell cannot be set (the rows
        /// before it stay). The cells may be user content: never log them.
        pub fn list_set_rows(&self, id: u16, rows: &[Vec<String>]) -> WinResult<()> {
            let rows = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| WideString::new(cell))
                        .collect::<WinResult<Vec<_>>>()
                })
                .collect::<WinResult<Vec<_>>>()?;
            let list = self.item(id)?;
            // SAFETY: a control of this thread; no pointers.
            unsafe { SendMessageW(list, LVM_DELETEALLITEMS, None, None) };
            let empty = WideString::new("")?;
            for (index, row) in rows.iter().enumerate() {
                let index = i32::try_from(index)
                    .map_err(|_| WinError::new(E_INVALIDARG, "list view row count"))?;
                let first = row.first().unwrap_or(&empty);
                let item = LVITEMW {
                    mask: LVIF_TEXT,
                    iItem: index,
                    pszText: PWSTR(first.as_pcwstr().0.cast_mut()),
                    ..Default::default()
                };
                // SAFETY: the item and its text outlive the call; the list view copies the
                // text and never writes through the pointer when inserting.
                let inserted = unsafe {
                    SendMessageW(
                        list,
                        LVM_INSERTITEMW,
                        None,
                        Some(LPARAM(&raw const item as isize)),
                    )
                }
                .0;
                let Ok(inserted) = usize::try_from(inserted) else {
                    return Err(WinError::new(E_FAIL, "LVM_INSERTITEMW"));
                };
                for (column, cell) in (1_i32..).zip(row.iter().skip(1)) {
                    let sub_item = LVITEMW {
                        iSubItem: column,
                        pszText: PWSTR(cell.as_pcwstr().0.cast_mut()),
                        ..Default::default()
                    };
                    // SAFETY: as above, for the text of one cell.
                    let set = unsafe {
                        SendMessageW(
                            list,
                            LVM_SETITEMTEXTW,
                            Some(WPARAM(inserted)),
                            Some(LPARAM(&raw const sub_item as isize)),
                        )
                    }
                    .0;
                    if set == 0 {
                        return Err(WinError::new(E_INVALIDARG, "LVM_SETITEMTEXTW"));
                    }
                }
            }
            Ok(())
        }

        /// The selected row of list view `id` (`LVM_GETNEXTITEM` with `LVNI_SELECTED`); `None`
        /// when no row is selected or there is no such control.
        pub fn list_selected(&self, id: u16) -> Option<usize> {
            if self.item(id).is_err() {
                return None;
            }
            // -1 (all bits set): search from the start.
            let selected = self.send_item(id, LVM_GETNEXTITEM, usize::MAX, LVNI_SELECTED as isize);
            usize::try_from(selected).ok()
        }

        /// Selects row `index` of list view `id` (and gives it the focus rectangle), scrolling
        /// it into view, or selects nothing for `None` (`LVM_SETITEMSTATE`,
        /// `LVM_ENSUREVISIBLE`). The list view reports `LVN_ITEMCHANGED`, which does not reach
        /// the busy handler. A row past the last fails with `E_INVALIDARG` and leaves nothing
        /// selected.
        pub fn list_select(&self, id: u16, index: Option<usize>) -> WinResult<()> {
            let list = self.item(id)?;
            let clear = LVITEMW {
                stateMask: LVIS_SELECTED,
                ..Default::default()
            };
            // SAFETY: the item outlives the call; -1 (all bits set) applies it to every row.
            unsafe {
                SendMessageW(
                    list,
                    LVM_SETITEMSTATE,
                    Some(WPARAM(usize::MAX)),
                    Some(LPARAM(&raw const clear as isize)),
                )
            };
            let Some(index) = index else {
                return Ok(());
            };
            let select = LVITEMW {
                state: LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0),
                stateMask: LIST_VIEW_ITEM_STATE_FLAGS(LVIS_SELECTED.0 | LVIS_FOCUSED.0),
                ..Default::default()
            };
            // SAFETY: the item outlives the call.
            let selected = unsafe {
                SendMessageW(
                    list,
                    LVM_SETITEMSTATE,
                    Some(WPARAM(index)),
                    Some(LPARAM(&raw const select as isize)),
                )
            }
            .0;
            if selected == 0 {
                return Err(WinError::new(E_INVALIDARG, "LVM_SETITEMSTATE"));
            }
            // SAFETY: a control of this thread; no pointers (0: the row must be fully visible).
            unsafe {
                SendMessageW(
                    list,
                    LVM_ENSUREVISIBLE,
                    Some(WPARAM(index)),
                    Some(LPARAM(0)),
                )
            };
            Ok(())
        }

        /// Sets the dialog's icons from the icon group `icon_group_id` of `module`: the large
        /// one (taskbar, Alt+Tab) and the small one (title bar), each at the size of the
        /// dialog's DPI ([`icon_size_for_window`]). They are loaded again at the new DPI when
        /// the dialog moves to a monitor of another DPI (`WM_DPICHANGED`), so `module` must
        /// stay loaded while the dialog is open. The dialog keeps the icons until it is
        /// destroyed.
        pub fn set_icons(&self, module: Module, icon_group_id: u16) -> WinResult<()> {
            let big = Icon::load(
                module,
                icon_group_id,
                icon_size_for_window(IconKind::Large, self.hwnd)?,
            )?;
            let small = Icon::load(
                module,
                icon_group_id,
                icon_size_for_window(IconKind::Small, self.hwnd)?,
            )?;
            self.state.set_icons(
                self.hwnd,
                Icons {
                    module,
                    id: icon_group_id,
                    big,
                    small,
                },
            );
            Ok(())
        }

        /// Restores the dialog if it is minimized and makes it, or the message box or dialog
        /// it owns that was active last, the foreground window. True when Windows let it come
        /// to the front: a process may do so only shortly after the user's input reached it,
        /// or when the process the user was working in allowed it
        /// ([`allow_any_foreground`](crate::window::allow_any_foreground)); otherwise Windows
        /// flashes its taskbar button.
        pub fn bring_to_front(&self) -> bool {
            bring_to_front(self.hwnd)
        }

        /// Ends the dialog with `result` (`EndDialog`) once the current callback returned:
        /// [`run_dialog`] returns it, and a detached dialog hands it to `on_close`.
        pub fn end(&self, result: isize) {
            // SAFETY: the dialog is a window of this thread run by DialogBoxIndirectParamW.
            let _ = unsafe { EndDialog(self.hwnd, result) };
        }

        /// Shows a modal message box owned by the dialog, captioned with the product name, and
        /// returns the answer ([`message_box`](crate::dialog::message_box)). Its modal loop
        /// runs inside the callback (see "Re-entrancy" in the module documentation). Fails with
        /// `E_INVALIDARG` for a text with a NUL, and with `E_ABORT`
        /// ([`WinErrorKind::Cancelled`](crate::error::WinErrorKind)) when the dialog was
        /// dismissed, before or while the box was open: the box is closed with the dialog.
        pub fn message_box(&self, text: &str, kind: MessageKind) -> WinResult<MessageResponse> {
            if self.state.dismissed.get() {
                return Err(WinError::new(E_ABORT, "MessageBoxW"));
            }
            let previous = self.state.open_box.replace(Some(kind));
            let answer = message_box(Some(self.hwnd), text, PRODUCT_NAME, kind);
            self.state.open_box.set(previous);
            if self.state.dismissed.get() {
                return Err(WinError::new(E_ABORT, "MessageBoxW"));
            }
            answer
        }
    }

    /// The number of columns of a list view (`HDM_GETITEMCOUNT` of its header).
    fn column_count(list: HWND) -> usize {
        // SAFETY: a control of this thread; no pointers.
        let header = unsafe { SendMessageW(list, LVM_GETHEADER, None, None) }.0;
        if header == 0 {
            return 0;
        }
        // SAFETY: the list view's header control, a window of this thread; no pointers.
        let count =
            unsafe { SendMessageW(HWND(header as *mut c_void), HDM_GETITEMCOUNT, None, None) }.0;
        usize::try_from(count).unwrap_or(0)
    }

    /// The icons a dialog shows, and where they come from (to load them again at another DPI).
    struct Icons {
        module: Module,
        id: u16,
        big: Icon,
        small: Icon,
    }

    /// What the dialog procedure keeps per dialog; [`run`] owns it, and the dialog's
    /// `DWLP_USER` slot points to it from `WM_INITDIALOG` on.
    struct DialogState {
        handler: RefCell<Box<dyn DialogHandler>>,
        /// What a detached dialog shares with its [`DetachedDialog`].
        shared: Option<Arc<Shared>>,
        /// The dialog window, once `WM_INITDIALOG` installed this state (0 before).
        hwnd: Cell<isize>,
        /// `WM_DESTROY` was seen: the handler gets no more callbacks.
        destroyed: Cell<bool>,
        /// The dialog was dismissed.
        dismissed: Cell<bool>,
        /// The dialog ended during `WM_INITDIALOG` because it was dismissed: never shown.
        ended_unshown: Cell<bool>,
        /// [`Dialog::focus`] moved the focus (during `init`, the default focus is skipped).
        focus_moved: Cell<bool>,
        /// The kind of the message box [`Dialog::message_box`] shows, while it is open.
        open_box: Cell<Option<MessageKind>>,
        /// The icons set with [`Dialog::set_icons`]; the window uses them until it is
        /// destroyed, before this state is dropped.
        icons: RefCell<Option<Icons>>,
    }

    impl DialogState {
        /// Shows `icons` in the dialog, then drops the icons shown before (the window no
        /// longer uses them).
        fn set_icons(&self, hwnd: HWND, icons: Icons) {
            for (which, icon) in [(ICON_BIG, &icons.big), (ICON_SMALL, &icons.small)] {
                // SAFETY: a message to this thread's dialog; the icon outlives its use, as the
                // state keeps it until the dialog is destroyed or the icon replaced.
                unsafe {
                    SendMessageW(
                        hwnd,
                        WM_SETICON,
                        Some(WPARAM(which as usize)),
                        Some(LPARAM(icon.handle().0 as isize)),
                    )
                };
            }
            if let Ok(mut slot) = self.icons.try_borrow_mut() {
                *slot = Some(icons);
            } else {
                // Never borrowed elsewhere while this runs; keep the icons alive regardless.
                core::mem::forget(icons);
            }
        }

        /// Loads the icons again for `dpi` after the dialog moved to another monitor.
        fn reload_icons(&self, hwnd: HWND, dpi: u32) {
            let source = match self.icons.try_borrow() {
                Ok(icons) => icons.as_ref().map(|icons| (icons.module, icons.id)),
                Err(_) => None,
            };
            let Some((module, id)) = source else {
                return;
            };
            let size = |kind| icon_size_for_dpi(kind, dpi).map_or_else(|| icon_size(kind), Ok);
            let load = || -> WinResult<Icons> {
                Ok(Icons {
                    module,
                    id,
                    big: Icon::load(module, id, size(IconKind::Large)?)?,
                    small: Icon::load(module, id, size(IconKind::Small)?)?,
                })
            };
            if let Ok(icons) = load() {
                self.set_icons(hwnd, icons);
            }
        }
    }

    /// What a detached dialog's thread and its [`DetachedDialog`]s share.
    #[derive(Debug, Default)]
    struct Shared {
        /// The dialog window once its `init` ran; 0 before, and again from `WM_DESTROY` on.
        hwnd: AtomicIsize,
        /// The owner dismissed the dialog: a dialog not shown yet is never shown.
        dismissed: AtomicBool,
        /// The dialog was closed (or never shown).
        closed: AtomicBool,
    }

    /// A dialog shown on a thread of its own ([`spawn_dialog`]). Clones share the dialog, and
    /// it may be used from any thread.
    ///
    /// Dropping it leaves the dialog open; [`dismiss`](Self::dismiss) closes it. Its window
    /// handle is published once the handler's `init` returned, so messages cannot reach the
    /// dialog before.
    #[derive(Clone, Debug)]
    pub struct DetachedDialog {
        shared: Arc<Shared>,
    }

    impl DetachedDialog {
        /// True once the dialog was closed (by the user, its handler, or
        /// [`dismiss`](Self::dismiss)), or when it could not be shown.
        pub fn is_closed(&self) -> bool {
            self.shared.closed.load(Ordering::Acquire)
        }

        /// Posts the private `message` to the dialog, for its handler's
        /// [`app_message`](DialogHandler::app_message). False, and nothing is posted, for a
        /// reserved message ([`super::is_reserved`]), before the dialog's `init` returned,
        /// after the dialog was destroyed, and when posting fails. A message that arrives while
        /// the handler runs a modal loop (a message box) does not reach it (see
        /// "Re-entrancy" in the module documentation).
        pub fn post(&self, message: AppMessage, wparam: usize, lparam: isize) -> bool {
            !is_reserved(message.id()) && self.post_raw(message.id(), wparam, lparam)
        }

        /// Asks the dialog's thread to restore the dialog if it is minimized and bring it to
        /// the front ([`Dialog::bring_to_front`]), without waiting. False when the dialog is
        /// not up (see [`post`](Self::post)).
        pub fn bring_to_front(&self) -> bool {
            self.post_raw(BRING_TO_FRONT, 0, 0)
        }

        /// Closes the dialog without waiting and without asking its handler: the dialog ends
        /// with `IDCANCEL`, together with the dialogs and the message box it shows (see
        /// "Dismissing" in the module documentation). A dialog that is not shown yet is never
        /// shown, and its `on_close` gets `None`.
        pub fn dismiss(&self) {
            // Pairs with the publication in `initialize`: either the dialog's thread sees the
            // flag before it shows the dialog, or this sees the published handle and posts.
            self.shared.dismissed.store(true, Ordering::SeqCst);
            let _ = self.post_raw(DISMISS, 0, 0);
        }

        fn post_raw(&self, message: u32, wparam: usize, lparam: isize) -> bool {
            let hwnd = self.shared.hwnd.load(Ordering::SeqCst);
            if hwnd == 0 {
                return false;
            }
            // SAFETY: posts plain values; a handle that no longer names a window makes the call
            // fail.
            unsafe {
                PostMessageW(
                    Some(HWND(hwnd as *mut c_void)),
                    message,
                    WPARAM(wparam),
                    LPARAM(lparam),
                )
            }
            .is_ok()
        }
    }

    /// Runs `template` without owner on a new thread (named `raumakey-dialog`) and returns at
    /// once; when the dialog closes, `on_close` gets its result on that thread (`None` when it
    /// was dismissed before it was shown, or could not be shown). Fails when the thread cannot
    /// start. A panic in `on_close` is contained.
    pub fn spawn_dialog(
        template: TemplateBytes,
        handler: impl DialogHandler + Send + 'static,
        on_close: impl FnOnce(Option<isize>) + Send + 'static,
    ) -> WinResult<DetachedDialog> {
        let shared = Arc::new(Shared::default());
        let on_thread = Arc::clone(&shared);
        std::thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                let result = if on_thread.dismissed.load(Ordering::SeqCst) {
                    None
                } else {
                    let shared = Some(Arc::clone(&on_thread));
                    guard_value(None, || {
                        run(&template, None, Box::new(handler), shared)
                            .ok()
                            .flatten()
                    })
                };
                on_thread.hwnd.store(0, Ordering::SeqCst);
                on_thread.closed.store(true, Ordering::Release);
                guard_value((), || on_close(result));
            })
            .map_err(|error| WinError::from_io(&error, "dialog thread"))?;
        Ok(DetachedDialog { shared })
    }

    /// Shows `template` as a modal dialog owned by `owner` and returns the result it ended
    /// with ([`Dialog::end`]; `IDCANCEL` when the handler agreed to close it or it was
    /// dismissed). `handler` receives its messages (see the module documentation) and is
    /// dropped before this returns, after the dialog is destroyed.
    ///
    /// `owner` must be `None` or a window of the calling thread, which the dialog disables
    /// while it is open; an owner whose thread waits for the caller would deadlock. Without an
    /// owner the dialog gets a taskbar button. Fails when the common controls cannot be
    /// registered or the dialog cannot be created (such as for an owner that is no window);
    /// `init` has not run then.
    pub fn run_dialog(
        template: &TemplateBytes,
        owner: Option<HWND>,
        handler: Box<dyn DialogHandler>,
    ) -> WinResult<isize> {
        run(template, owner, handler, None).map(|result| result.unwrap_or(CANCELLED))
    }

    /// [`run_dialog`], publishing the window to `shared` for a detached dialog. `Ok(None)`: the
    /// dialog was dismissed before it was shown.
    fn run(
        template: &TemplateBytes,
        owner: Option<HWND>,
        handler: Box<dyn DialogHandler>,
        shared: Option<Arc<Shared>>,
    ) -> WinResult<Option<isize>> {
        register_common_controls()?;
        let module = Module::containing_this_crate()?;
        let state = DialogState {
            handler: RefCell::new(handler),
            shared,
            hwnd: Cell::new(0),
            destroyed: Cell::new(false),
            dismissed: Cell::new(false),
            ended_unshown: Cell::new(false),
            focus_moved: Cell::new(false),
            open_box: Cell::new(None),
            icons: RefCell::new(None),
        };
        let pointer: *const DialogState = &state;
        let result = {
            let _starting = Starting::mark(pointer);
            // SAFETY: the template is a DWORD-aligned extended template that lives during the
            // call; `dialog_proc` reads `state` through the init parameter only during
            // `WM_INITDIALOG` of this call (see `Starting`), and through the dialog's slot
            // until the dialog is destroyed, which happens before the call returns. USER
            // validates the owner handle.
            unsafe {
                DialogBoxIndirectParamW(
                    Some(module.instance()),
                    template.as_ptr().cast::<DLGTEMPLATE>(),
                    owner,
                    Some(dialog_proc),
                    LPARAM(pointer as isize),
                )
            }
        };
        let hwnd = state.hwnd.get();
        if hwnd == 0 {
            // `init` never ran: the dialog could not be created.
            return Err(WinError::last("DialogBoxIndirectParamW"));
        }
        // The dialog is destroyed before DialogBoxIndirectParamW returns; should its window
        // still exist, it no longer reaches the state that is about to be dropped.
        let window = HWND(hwnd as *mut c_void);
        // SAFETY: only queries whether the handle names a window.
        if unsafe { IsWindow(Some(window)) }.as_bool() && read_slot(window) == pointer as isize {
            write_slot(window, 0);
        }
        if state.ended_unshown.get() {
            return Ok(None);
        }
        Ok(Some(result))
    }

    /// Registers the common control classes the templates use, once per process.
    fn register_common_controls() -> WinResult<()> {
        static REGISTERED: OnceLock<bool> = OnceLock::new();
        let registered = *REGISTERED.get_or_init(|| {
            let register = |classes| {
                let controls = INITCOMMONCONTROLSEX {
                    dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                    dwICC: classes,
                };
                // SAFETY: a fully initialized structure that outlives the call.
                unsafe { InitCommonControlsEx(&controls) }.as_bool()
            };
            // Common Controls 5 (without the application manifest, as in the tests) does not
            // know the standard classes, which it does not provide anyway.
            register(ICC_STANDARD_CLASSES | ICC_LISTVIEW_CLASSES) || register(ICC_LISTVIEW_CLASSES)
        });
        if registered {
            Ok(())
        } else {
            Err(WinError::new(E_FAIL, "InitCommonControlsEx"))
        }
    }

    thread_local! {
        /// The state of the `DialogBoxIndirectParamW` call that `run` is making on this thread,
        /// until its `WM_INITDIALOG` takes it; null otherwise.
        static STARTING: Cell<*const DialogState> = const { Cell::new(core::ptr::null()) };
        /// The dialogs of this thread that this module runs, from `WM_INITDIALOG` to
        /// `WM_DESTROY` (so that dismissing a dialog reaches the dialogs it owns).
        static OPEN: RefCell<Vec<isize>> = const { RefCell::new(Vec::new()) };
    }

    /// Marks a state as the one `WM_INITDIALOG` may take, for the duration of one
    /// `DialogBoxIndirectParamW` call. Calls nest (a handler may run a dialog of its own from
    /// any callback), so dropping the mark restores the previous one.
    struct Starting {
        previous: *const DialogState,
    }

    impl Starting {
        fn mark(state: *const DialogState) -> Self {
            Self {
                previous: STARTING.with(|starting| starting.replace(state)),
            }
        }
    }

    impl Drop for Starting {
        fn drop(&mut self) {
            STARTING.with(|starting| starting.set(self.previous));
        }
    }

    /// The dialog procedure of every dialog this module runs.
    unsafe extern "system" fn dialog_proc(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> isize {
        guard_value(FALSE, || {
            if message == WM_INITDIALOG {
                // SAFETY: for WM_INITDIALOG, `lparam` is the init parameter of the creating
                // call; `initialize` only compares it before trusting it.
                return unsafe { initialize(hwnd, lparam) };
            }
            let state = read_slot(hwnd) as *const DialogState;
            if state.is_null() {
                return FALSE;
            }
            // SAFETY: the slot holds the state that `initialize` installed. It belongs to the
            // `run` call whose DialogBoxIndirectParamW runs this dialog on this thread, and that
            // call returns (and drops the state) only after the dialog was destroyed.
            dispatch(unsafe { &*state }, hwnd, message, wparam, lparam)
        })
    }

    /// Installs the state of the running `run` call and calls the handler's `init`; then
    /// publishes the window of a detached dialog and ends it at once if it was dismissed
    /// meanwhile. Returns what `WM_INITDIALOG` answers: `TRUE` to let Windows focus the first
    /// control, `FALSE` when [`Dialog::focus`] already did (or nothing was installed).
    ///
    /// Only the state that `run` marked for its running call is taken, and only into an empty
    /// slot: a `WM_INITDIALOG` sent to a live dialog, or carrying any other value, is ignored.
    ///
    /// # Safety
    ///
    /// `lparam` must be the `WM_INITDIALOG` argument of a dialog of this thread.
    unsafe fn initialize(hwnd: HWND, lparam: LPARAM) -> isize {
        let pending = lparam.0 as *const DialogState;
        if pending.is_null() || pending != STARTING.with(Cell::get) || read_slot(hwnd) != 0 {
            return FALSE;
        }
        STARTING.with(|starting| starting.set(core::ptr::null()));
        // SAFETY: `pending` is the state that `run` marked on this thread; it lives during the
        // DialogBoxIndirectParamW call that sent this message.
        let state = unsafe { &*pending };
        write_slot(hwnd, pending as isize);
        state.hwnd.set(hwnd.0 as isize);
        OPEN.with(|open| {
            if let Ok(mut open) = open.try_borrow_mut() {
                open.push(hwnd.0 as isize);
            }
        });
        let dialog = Dialog { hwnd, state };
        guard_value((), || {
            if let Ok(mut handler) = state.handler.try_borrow_mut() {
                handler.init(&dialog);
            }
        });
        if let Some(shared) = &state.shared {
            // Pairs with `DetachedDialog::dismiss`: publish, then look at the flag.
            shared.hwnd.store(hwnd.0 as isize, Ordering::SeqCst);
            if shared.dismissed.load(Ordering::SeqCst) {
                state.dismissed.set(true);
                state.ended_unshown.set(true);
                dialog.end(CANCELLED);
                return FALSE;
            }
        }
        if state.focus_moved.get() { FALSE } else { TRUE }
    }

    /// Low word of `value` (`LOWORD`).
    const fn low_word(value: usize) -> u16 {
        (value & 0xFFFF) as u16
    }

    /// High word of the low 32 bits of `value` (`HIWORD`).
    const fn high_word(value: usize) -> u16 {
        ((value >> 16) & 0xFFFF) as u16
    }

    /// Handles one message of an installed dialog: the reserved ones and the bookkeeping here,
    /// the rest by the handler unless it is busy or the dialog is being destroyed.
    fn dispatch(
        state: &DialogState,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> isize {
        match message {
            DISMISS => {
                dismiss(state, hwnd);
                return TRUE;
            }
            BRING_TO_FRONT => {
                bring_to_front(hwnd);
                return TRUE;
            }
            WM_DESTROY => {
                forget(state, hwnd);
                return FALSE;
            }
            WM_DPICHANGED => {
                // The low word is the new horizontal DPI (both are equal).
                state.reload_icons(hwnd, u32::from(low_word(wparam.0)));
                return FALSE;
            }
            _ => {}
        }
        if state.destroyed.get() {
            return FALSE;
        }
        // A busy handler (a re-entrant message) leaves the message to the default processing.
        let Ok(mut handler) = state.handler.try_borrow_mut() else {
            return FALSE;
        };
        let dialog = Dialog { hwnd, state };
        match message {
            WM_COMMAND => {
                let (id, code) = (low_word(wparam.0), high_word(wparam.0));
                if id == IDCANCEL && code == BN_CLICKED {
                    close(handler.as_mut(), &dialog);
                } else {
                    handler.command(&dialog, id, code);
                }
                TRUE
            }
            WM_CLOSE => {
                close(handler.as_mut(), &dialog);
                TRUE
            }
            WM_NOTIFY => {
                if let Some(header) = notify_header(hwnd, lparam) {
                    handler.notify(&dialog, low_word(header.idFrom), header.code as i32);
                }
                // The default answer, 0, suits every notification.
                FALSE
            }
            _ => match AppMessage::from_id(message) {
                Some(app) if !is_reserved(message) => {
                    handler.app_message(&dialog, app, wparam.0, lparam.0);
                    TRUE
                }
                _ => FALSE,
            },
        }
    }

    /// The `NMHDR` that the `WM_NOTIFY` argument `lparam` points to, copied, when it names a
    /// control of `dialog`: its `hwndFrom` is a child of the dialog whose id is `idFrom`. `None`
    /// for anything else, including a pointer to memory that cannot be read (see "Forged
    /// messages" in the module documentation).
    fn notify_header(dialog: HWND, lparam: LPARAM) -> Option<NMHDR> {
        let mut header = NMHDR::default();
        let mut copied = 0_usize;
        // SAFETY: copies from this process's own memory into `header`, which is writable for
        // the size passed; the call fails instead of faulting where the source is not
        // readable, and no reference to the source is ever made.
        unsafe {
            ReadProcessMemory(
                GetCurrentProcess(),
                lparam.0 as *const c_void,
                (&raw mut header).cast(),
                size_of::<NMHDR>(),
                Some(&mut copied),
            )
        }
        .ok()?;
        if copied != size_of::<NMHDR>() {
            return None;
        }
        // SAFETY: queries about window handles, which USER validates.
        let (child, id) = unsafe {
            (
                IsChild(dialog, header.hwndFrom).as_bool(),
                GetDlgCtrlID(header.hwndFrom),
            )
        };
        // Controls report their id as `GetDlgCtrlID` gives it, widened with its sign.
        (child && id as isize as usize == header.idFrom).then_some(header)
    }

    /// Asks the handler whether the dialog may close, and ends it with `IDCANCEL` if so.
    fn close(handler: &mut dyn DialogHandler, dialog: &Dialog<'_>) {
        if handler.close_requested(dialog) {
            dialog.end(CANCELLED);
        }
    }

    /// `WM_DESTROY`: the handler gets no more callbacks, the window is no longer published,
    /// and it leaves the thread's list of dialogs.
    fn forget(state: &DialogState, hwnd: HWND) {
        state.destroyed.set(true);
        if let Some(shared) = &state.shared {
            let _ = shared.hwnd.compare_exchange(
                hwnd.0 as isize,
                0,
                Ordering::SeqCst,
                Ordering::SeqCst,
            );
        }
        OPEN.with(|open| {
            if let Ok(mut open) = open.try_borrow_mut() {
                open.retain(|&dialog| dialog != hwnd.0 as isize);
            }
        });
    }

    /// Restores `hwnd` if it is minimized and brings it, or its last active owned popup, to
    /// the front.
    fn bring_to_front(hwnd: HWND) -> bool {
        // SAFETY: calls about a window of this thread; no pointers.
        unsafe {
            if IsIconic(hwnd).as_bool() {
                let _ = ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(GetLastActivePopup(hwnd)).as_bool()
        }
    }

    /// Ends the dialog with `IDCANCEL` without asking its handler, and closes what it shows:
    /// the dialogs of this module that it owns get the same reserved message, and the message
    /// box of [`Dialog::message_box`] its dismiss answer.
    fn dismiss(state: &DialogState, hwnd: HWND) {
        state.dismissed.set(true);
        // SAFETY: the dialog is a window of this thread run by DialogBoxIndirectParamW.
        let _ = unsafe { EndDialog(hwnd, CANCELLED) };
        let mut closing = Closing {
            owner: hwnd,
            box_answer: state.open_box.get().map(|kind| kind.dismiss_answer()),
            open: OPEN.with(|open| {
                open.try_borrow()
                    .map(|open| open.clone())
                    .unwrap_or_default()
            }),
        };
        // SAFETY: GetCurrentThreadId has no preconditions; the callback runs during the call
        // only and receives a pointer to `closing`, which outlives the call; it is never kept.
        let _ = unsafe {
            EnumThreadWindows(
                GetCurrentThreadId(),
                Some(close_owned),
                LPARAM(&raw mut closing as isize),
            )
        };
    }

    /// What the enumeration of [`dismiss`] carries.
    struct Closing {
        /// The dismissed dialog.
        owner: HWND,
        /// The answer that closes the message box it shows, if it shows one.
        box_answer: Option<MessageResponse>,
        /// The dialogs of this module on this thread.
        open: Vec<isize>,
    }

    /// The callback of [`dismiss`]: closes a window that the dismissed dialog owns.
    unsafe extern "system" fn close_owned(hwnd: HWND, lparam: LPARAM) -> BOOL {
        guard_bool(|| {
            // SAFETY: `lparam` is the `Closing` of the enumeration that calls this.
            let closing = unsafe { &*(lparam.0 as *const Closing) };
            // SAFETY: only reads the window's owner; USER validates `hwnd`.
            let owner = unsafe { GetWindow(hwnd, GW_OWNER) }.unwrap_or_default();
            if owner != closing.owner {
                return true;
            }
            let (message, wparam) = if closing.open.contains(&(hwnd.0 as isize)) {
                (DISMISS, 0)
            } else if let Some(answer) = closing.box_answer
                && is_dialog_class(hwnd)
            {
                (WM_COMMAND, answer.id() as usize)
            } else {
                return true;
            };
            // SAFETY: posts plain values to a window of this thread.
            let _ = unsafe { PostMessageW(Some(hwnd), message, WPARAM(wparam), LPARAM(0)) };
            true
        })
    }

    /// True when `hwnd` is of the dialog class (`#32770`), as message boxes are.
    fn is_dialog_class(hwnd: HWND) -> bool {
        let mut class = [0_u16; 16];
        // SAFETY: the buffer is writable for its whole length; USER validates `hwnd`.
        let len = unsafe { GetClassNameW(hwnd, &mut class) };
        let len = usize::try_from(len).unwrap_or(0).min(class.len());
        String::from_utf16_lossy(class.get(..len).unwrap_or(&[])) == DIALOG_CLASS
    }

    #[cfg(target_pointer_width = "64")]
    fn read_slot(hwnd: HWND) -> isize {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowLongPtrW;
        // SAFETY: reads the DWLP_USER slot of a dialog (the dialog class reserves it); a stale
        // handle reads 0.
        unsafe { GetWindowLongPtrW(hwnd, DWLP_USER) }
    }

    #[cfg(target_pointer_width = "32")]
    fn read_slot(hwnd: HWND) -> isize {
        use windows::Win32::UI::WindowsAndMessaging::GetWindowLongW;
        // SAFETY: as above; on 32-bit Windows the slot is a 32-bit value.
        (unsafe { GetWindowLongW(hwnd, DWLP_USER) }) as isize
    }

    #[cfg(target_pointer_width = "64")]
    fn write_slot(hwnd: HWND, value: isize) {
        use windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW;
        // SAFETY: writes the DWLP_USER slot of a dialog of this thread.
        unsafe { SetWindowLongPtrW(hwnd, DWLP_USER, value) };
    }

    #[cfg(target_pointer_width = "32")]
    fn write_slot(hwnd: HWND, value: isize) {
        use windows::Win32::UI::WindowsAndMessaging::SetWindowLongW;
        // SAFETY: as above; on 32-bit Windows `isize` is 32 bits wide.
        unsafe { SetWindowLongW(hwnd, DWLP_USER, value as i32) };
    }
}
