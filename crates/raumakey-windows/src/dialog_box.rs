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
//!   for its controls (check boxes, texts, drop-down lists and editable combo boxes, list
//!   views, the icons of its title bar and taskbar button, focus, message boxes and the Open
//!   dialog owned by the dialog, ending it), and its position ([`Dialog::move_to`]). Texts of several lines go through
//!   [`Dialog::set_text_wrapped`], which wraps them at spaces with the control's own font
//!   (and again after a change of DPI), since Windows would cut a Vietnamese word.
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
//! [`DialogHandler::notify`] with the id and code of its `NMHDR`, and a key pressed in a list
//! view (`LVN_KEYDOWN`) then reaches [`DialogHandler::list_key`] too. Private messages
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
//! dismissed or brought to the front, and its wrapped texts wrapped again after a change of
//! DPI ([`Dialog::set_text_wrapped`]), while its handler waits for a message box.
//!
//! **Dismissing.** [`DetachedDialog::dismiss`] posts a reserved message that the dialog
//! procedure answers itself with `EndDialog(IDCANCEL)`, without asking the handler (and
//! nothing else can send it; see "Forged messages"). It also dismisses the dialogs that the
//! handler runs with this dialog as owner, and closes the message box shown through
//! [`Dialog::message_box`] with the command that closes a box of its kind
//! ([`dismiss_command`](crate::dialog::MessageKind::dismiss_command); `message_box` then fails
//! with `E_ABORT`), and the Open dialog of [`Dialog::choose_file`] with `IDCANCEL` (which then
//! fails the same way). A dialog dismissed before its window handle is published (that happens
//! once `init` returned) ends during `WM_INITDIALOG` and is never shown. Until then nothing
//! reaches the dialog's thread: a message box that `init` shows stays open until it is
//! answered (and [`Dialog::message_box`] then fails with `E_ABORT`), while one asked after the
//! dismiss fails at once without being shown.
//!
//! **Forged messages.** Any process of the desktop at the same or a higher integrity level may
//! send or post messages to a dialog, as to any window. The private messages carry no
//! pointers, and a forged `WM_INITDIALOG` is ignored (see above).
//!
//! - The reserved messages count only with the dialog's cookie in `wParam`, a random value
//!   that only its [`DetachedDialog`] and this module know, so another process cannot simply
//!   dismiss a dialog (skipping the question its handler asks before closing) or bring it to
//!   the front. That is hardening, not a boundary: a process that may read this process's
//!   memory can find the cookie.
//! - `WM_DESTROY` carries nothing, and only `DestroyWindow` sends the genuine one, after
//!   hiding the window. One that reaches the shown dialog is ignored; one that reaches it
//!   before Windows first shows it looks genuine, and only keeps the handler's callbacks away
//!   until the dialog is shown. The window stays published and on the thread's list of
//!   dialogs until `DialogBoxIndirectParamW` returns, so dismissing it, or its owner, still
//!   closes it.
//! - `WM_NOTIFY` carries a pointer to an `NMHDR` that Windows neither marshals between
//!   processes nor refuses to post, so a forged one may point anywhere: the dialog procedure
//!   copies the header with `ReadProcessMemory`, which fails instead of faulting on memory that
//!   cannot be read, and hands it to the handler only when it names a control of the dialog by
//!   its handle and id; the key of an `LVN_KEYDOWN` is copied the same way. The two requests
//!   of a virtual list view that the procedure answers itself
//!   ([`Dialog::list_set_virtual_rows`]) pass the same check and are copied the same way, and
//!   so is the text of a search, block by block within pages, up to its NUL and at most 1,024
//!   units: a text that cannot be read finds nothing. A search (`LVN_ODFINDITEMW`)
//!   is answered with its result alone (`DWLP_MSGRESULT`). A cell request (`LVN_GETDISPINFOW`)
//!   is answered by setting the request's own `pszText` field to a text the dialog keeps, with
//!   `WriteProcessMemory`, and only when `VirtualQuery` reports that field in committed
//!   read-write memory: `WriteProcessMemory` fails instead of faulting where it cannot write,
//!   but it makes a read-only executable page writable for the write (and restores it), so a
//!   forged request could otherwise patch code. Nothing is written through a pointer a request
//!   carries.
//!
//! **Panics.** The dialog procedure runs inside [`guard_value`](crate::panic::guard_value): a
//! panicking callback leaves its message unhandled (`FALSE`), the handler is not left borrowed,
//! and no unwind reaches Windows. A panic in `init` still publishes the window handle, so the
//! dialog can be dismissed. A panic while the handler is dropped, after the dialog ended, is
//! contained too: the result stays what the dialog ended with.

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
/// Wraps the texts set with `Dialog::set_text_wrapped` again, after a change of DPI.
#[cfg(windows)]
const REWRAP: u32 = WM_APP_LAST - 2;

/// Notification codes that dialog handlers match on: the high word of `wParam` of a
/// `WM_COMMAND` for the `u16` ones, `NMHDR::code` of a `WM_NOTIFY` for the `i32` ones.
pub mod notifications {
    /// `BN_CLICKED`: a button was clicked (also a check box, which toggled itself).
    pub const BN_CLICKED: u16 = 0;
    /// `CBN_SELCHANGE`: the user selected another item of a drop-down list. The edit of an
    /// editable combo box (`CBS_DROPDOWN`) shows the item's text only after this
    /// notification.
    pub const CBN_SELCHANGE: u16 = 1;
    /// `CBN_EDITCHANGE`: the user changed the text of the edit of an editable combo box
    /// (`CBS_DROPDOWN`), by typing or pasting; a choice from its list sends none.
    pub const CBN_EDITCHANGE: u16 = 5;
    /// `EN_CHANGE`: the text of an edit changed.
    pub const EN_CHANGE: u16 = 0x0300;
    /// `LVN_ITEMCHANGED`: an item of a list view changed, such as its selection; one change of
    /// the selection reports each item that lost or gained it.
    pub const LVN_ITEMCHANGED: i32 = -101;
    /// `LVN_KEYDOWN`: the user pressed a key while a list view had the focus; the key also
    /// reaches [`DialogHandler::list_key`](super::DialogHandler::list_key).
    pub const LVN_KEYDOWN: i32 = -155;
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
    use core::hash::BuildHasher;
    use std::collections::hash_map::RandomState;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicUsize, Ordering};
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
        LVS_EX_FULLROWSELECT, NMHDR, NMLVKEYDOWN,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetFocus};
    use windows::Win32::UI::WindowsAndMessaging::{
        BM_GETCHECK, BM_SETCHECK, CB_ADDSTRING, CB_GETCURSEL, CB_GETLBTEXT, CB_GETLBTEXTLEN,
        CB_RESETCONTENT, CB_SETCURSEL, DLGTEMPLATE, DialogBoxIndirectParamW, EndDialog,
        EnumThreadWindows, GW_OWNER, GetClassNameW, GetDlgCtrlID, GetDlgItem, GetLastActivePopup,
        GetWindow, ICON_BIG, ICON_SMALL, IsChild, IsIconic, IsWindow, IsWindowVisible,
        MapDialogRect, PostMessageW, SW_RESTORE, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
        SendDlgItemMessageW, SendMessageW, SetDlgItemTextW, SetForegroundWindow, SetWindowPos,
        ShowWindow, WINDOW_LONG_PTR_INDEX, WM_CLOSE, WM_COMMAND, WM_DESTROY, WM_DPICHANGED,
        WM_GETTEXT, WM_GETTEXTLENGTH, WM_INITDIALOG, WM_NEXTDLGCTL, WM_NOTIFY, WM_SETICON,
    };
    use windows::core::{BOOL, PWSTR};
    // The helpers of the macro list editor: virtual list views, text limits, posting.
    use windows::Win32::Graphics::Gdi::InvalidateRect;
    use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows::Win32::System::Memory::{
        MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_READWRITE, PAGE_WRITECOPY, VirtualQuery,
    };
    use windows::Win32::UI::Controls::{
        EM_SETLIMITTEXT, LVFI_PARTIAL, LVFI_STRING, LVFI_SUBSTRING, LVFI_WRAP, LVM_SETITEMCOUNT,
        LVN_GETDISPINFOW, LVN_ODFINDITEMW, LVSICF_NOSCROLL, NMLVDISPINFOW, NMLVFINDITEMW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GWL_STYLE, GetWindowLongW};

    use super::notifications::{BN_CLICKED, LVN_KEYDOWN};
    use super::{BRING_TO_FRONT, DISMISS, REWRAP, is_reserved};
    use crate::dialog::{MessageKind, MessageResponse, message_box};
    use crate::dialog_template::styles::LVS_OWNERDATA;
    use crate::dialog_template::{IDCANCEL, TemplateBytes};
    use crate::error::{E_ABORT, E_FAIL, E_INVALIDARG, ResultExt, WinError, WinResult};
    use crate::file_dialog::{FileFilter, open_file};
    use crate::icon::{Icon, IconKind, icon_size, icon_size_for_dpi, icon_size_for_window};
    use crate::module::Module;
    use crate::panic::{guard_bool, guard_value};
    use crate::strings::WideString;
    use crate::window::AppMessage;
    use crate::word_wrap::wrap_for_control;

    /// What a dialog procedure answers for a message it handled.
    const TRUE: isize = 1;
    /// What a dialog procedure answers for a message left to the default dialog processing.
    const FALSE: isize = 0;

    /// `DWLP_MSGRESULT`: the slot of the dialog's extra bytes that holds the result of a
    /// message its procedure handled (returned `TRUE` for), such as a `WM_NOTIFY`.
    const DWLP_MSGRESULT: WINDOW_LONG_PTR_INDEX = WINDOW_LONG_PTR_INDEX(0);

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
    /// The dialog's thread is a UI thread: while a callback runs, the dialog neither repaints
    /// nor answers the user, so a handler returns quickly. Slow work belongs to another thread,
    /// which posts its result back to the dialog ([`DetachedDialog::post`]). A dialog that
    /// shares its thread with other windows (a modal [`run_dialog`] runs on its owner's
    /// thread) never blocks on files, the network, or IPC, which would stop those windows too
    /// (design §11). A detached dialog ([`spawn_dialog`]) has its thread to itself and may do
    /// bounded work there, at the cost of its own responsiveness only: the macro list editor
    /// reads and writes its file on its thread, and may stop answering for a second or two
    /// (design §22.2).
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
        /// for every notification. The requests of a virtual list view that the dialog answers
        /// itself (`LVN_GETDISPINFOW`, `LVN_ODFINDITEMW`; see
        /// [`Dialog::list_set_virtual_rows`]) never arrive here.
        fn notify(&mut self, dialog: &Dialog<'_>, id: u16, code: i32) {
            let _ = (dialog, id, code);
        }

        /// List view `id` reported a key that the user pressed while it had the focus
        /// ([`LVN_KEYDOWN`](super::notifications::LVN_KEYDOWN), after
        /// [`notify`](Self::notify) got it), with its virtual-key code, such as `VK_DELETE`
        /// (0x2E). Any process of the desktop may send a list view keys, as any input: the
        /// key is a plain value to act on as a key pressed by the user.
        fn list_key(&mut self, dialog: &Dialog<'_>, id: u16, key: u16) {
            let _ = (dialog, id, key);
        }

        /// The private `message` arrived with `wparam` and `lparam` (posted with
        /// [`DetachedDialog::post`] or by other code of the process). Reserved messages
        /// ([`super::is_reserved`]) never arrive here.
        ///
        /// Any process of the desktop at the same or a higher integrity level may post such a
        /// message too, with any values, and a posted message may be lost (see "Re-entrancy"
        /// in the module documentation): `wparam` and `lparam` are untrusted plain values that
        /// never carry pointers, and the data they announce travels through state the handler
        /// shares with the sender.
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
        ///
        /// A label or a button (whose text USER keeps) holds at most 65,535 UTF-16 units: USER
        /// replaces a longer text with an empty one and the call still succeeds. An edit takes
        /// longer texts.
        pub fn set_text(&self, id: u16, text: &str) -> WinResult<()> {
            let text = WideString::new(text)?;
            // SAFETY: the text is NUL-terminated and outlives the call; the control is a
            // window of this thread.
            unsafe { SetDlgItemTextW(self.hwnd, i32::from(id), text.as_pcwstr()) }
                .context("SetDlgItemTextW")?;
            // A text set as it is replaces one that was wrapped.
            self.state.forget_wrapped(id);
            Ok(())
        }

        /// Sets the text of label, check box, or radio button `id` wrapped at spaces to the
        /// control's width with its own font
        /// ([`wrap_for_control`](crate::word_wrap::wrap_for_control)), so that the control
        /// shows each line as it is: wrapping by itself, Windows would cut a Vietnamese word.
        /// The control needs the height for the lines; a check box or radio button needs
        /// `BS_MULTILINE` for more than one.
        ///
        /// The dialog keeps `text` and wraps it again for the controls' new sizes and fonts
        /// once the dialog moved to a monitor of another DPI (`WM_DPICHANGED`), until
        /// [`set_text`](Self::set_text) or this sets another text for the control. Fails with
        /// `E_INVALIDARG` for a text with a NUL and for a control of another kind (a push
        /// button, an edit), and when there is no such control; the text stays as it was.
        pub fn set_text_wrapped(&self, id: u16, text: &str) -> WinResult<()> {
            // A text with a NUL fails before anything is measured.
            WideString::new(text)?;
            let wrapped = wrap_for_control(self.item(id)?, text)?;
            self.set_text(id, &wrapped)?;
            self.state.keep_wrapped(id, text);
            Ok(())
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

        /// Whether control `id` has the keyboard focus of the dialog's thread (`GetFocus`),
        /// itself or through a window inside it (the edit of an editable combo box); false when
        /// there is no such control. A handler that disables a control asks this first and
        /// moves the focus elsewhere ([`focus`](Self::focus)): a disabled control keeps
        /// nothing focused, and the keyboard would reach no control of the dialog.
        pub fn has_focus(&self, id: u16) -> bool {
            let Ok(control) = self.item(id) else {
                return false;
            };
            // SAFETY: no preconditions; reads the focus of the calling thread, the dialog's.
            let focus = unsafe { GetFocus() };
            // SAFETY: a query about window handles, which USER validates.
            !focus.is_invalid()
                && (focus == control || unsafe { IsChild(control, focus) }.as_bool())
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
        /// (`CB_SETCURSEL`, which sends no notification). An index past the last item (such as
        /// any index above `i32::MAX`) fails with `E_INVALIDARG` and leaves nothing selected.
        pub fn combo_select(&self, id: u16, index: Option<usize>) -> WinResult<()> {
            let control = self.item(id)?;
            // USER reads the index as an `int`, so one above `i32::MAX` would select the item
            // that its low bits name: it selects nothing instead, as -1 (all bits set) does.
            let wparam = index
                .filter(|&index| i32::try_from(index).is_ok())
                .unwrap_or(usize::MAX);
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

        /// The text of item `index` of combo box `id` (`CB_GETLBTEXTLEN`, `CB_GETLBTEXT`).
        /// An editable combo box (`CBS_DROPDOWN`) reports a choice from its list
        /// ([`notifications::CBN_SELCHANGE`](super::notifications::CBN_SELCHANGE)) before its
        /// edit shows the item's text, so the handler reads the choice here; the edit itself
        /// is read and set with [`text`](Self::text) and [`set_text`](Self::set_text). Fails
        /// with `E_INVALIDARG` for an index past the last item (such as any above
        /// `i32::MAX`), and when there is no such control. Unpaired surrogates become U+FFFD.
        pub fn combo_item_text(&self, id: u16, index: usize) -> WinResult<String> {
            let combo = self.item(id)?;
            // USER reads the index as an `int`: one above `i32::MAX` would name another item.
            if i32::try_from(index).is_err() {
                return Err(WinError::new(E_INVALIDARG, "CB_GETLBTEXTLEN"));
            }
            // SAFETY: a control of this thread; no pointers.
            let length =
                unsafe { SendMessageW(combo, CB_GETLBTEXTLEN, Some(WPARAM(index)), None) }.0;
            let length = usize::try_from(length)
                .map_err(|_| WinError::new(E_INVALIDARG, "CB_GETLBTEXTLEN"))?;
            let mut buffer = vec![0_u16; length + 1];
            // SAFETY: the buffer holds the item's length and its NUL, as CB_GETLBTEXT writes,
            // and outlives the call, which the control answers on this thread.
            let copied = unsafe {
                SendMessageW(
                    combo,
                    CB_GETLBTEXT,
                    Some(WPARAM(index)),
                    Some(LPARAM(buffer.as_mut_ptr() as isize)),
                )
            }
            .0;
            let copied =
                usize::try_from(copied).map_err(|_| WinError::new(E_INVALIDARG, "CB_GETLBTEXT"))?;
            Ok(String::from_utf16_lossy(
                buffer.get(..copied.min(length)).unwrap_or(&[]),
            ))
        }

        /// Replaces the columns of list view `id` with `columns`: each a header text and a
        /// width in dialog units, converted to pixels for the dialog's font and DPI
        /// (`MapDialogRect`). Also turns on full-row selection (`LVS_EX_FULLROWSELECT`). Fails
        /// with `E_INVALIDARG`, before anything changes, for a text with a NUL or a negative
        /// width; fails when there is no such control or a column cannot be inserted. The
        /// widths are converted here, once: they are not scaled again when the dialog moves to
        /// a monitor of another DPI (`WM_DPICHANGED`).
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
        /// the busy handler. A row past the last (such as any row above `i32::MAX`) fails with
        /// `E_INVALIDARG` and leaves nothing selected.
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
            // USER reads the row as an `int`, so one above `i32::MAX` would select the row that
            // its low bits name.
            if i32::try_from(index).is_err() {
                return Err(WinError::new(E_INVALIDARG, "LVM_SETITEMSTATE"));
            }
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

        /// Moves the dialog's upper-left corner to (`x`, `y`) in screen coordinates without
        /// changing its size, its place in the Z order, or the active window
        /// (`SetWindowPos`). Called from [`DialogHandler::init`], it places the dialog before
        /// Windows shows it: Windows moves a dialog whose template starts off every monitor
        /// back onto one when it creates it, so tests keep their dialogs off the screen this
        /// way.
        pub fn move_to(&self, x: i32, y: i32) -> WinResult<()> {
            // SAFETY: the dialog is a window of this thread; no pointers.
            unsafe {
                SetWindowPos(
                    self.hwnd,
                    None,
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                )
            }
            .context("SetWindowPos")
        }

        /// Ends the dialog with `result` (`EndDialog`) once the current callback returned:
        /// [`run_dialog`] returns it, and a detached dialog hands it to `on_close`.
        pub fn end(&self, result: isize) {
            // SAFETY: the dialog is a window of this thread run by DialogBoxIndirectParamW.
            let _ = unsafe { EndDialog(self.hwnd, result) };
        }

        /// Shows a modal message box owned by the dialog, captioned with the product name, and
        /// returns the answer ([`message_box`](crate::dialog::message_box), which wraps the
        /// text at spaces). Its modal loop runs inside the callback (see "Re-entrancy" in the
        /// module documentation). Fails with `E_INVALIDARG` for a text with a NUL, and with
        /// `E_ABORT` ([`WinErrorKind::Cancelled`](crate::error::WinErrorKind)) when the dialog
        /// was dismissed, before or while the box was open: the box is closed with the dialog,
        /// or not shown at all. A box shown during `init` of a detached dialog is the
        /// exception: a dismiss cannot reach it before `init` returns (see "Dismissing" in the
        /// module documentation), so it stays open until it is answered, and the call then
        /// fails.
        pub fn message_box(&self, text: &str, kind: MessageKind) -> WinResult<MessageResponse> {
            if self.state.is_dismissed() {
                return Err(WinError::new(E_ABORT, "MessageBoxW"));
            }
            let previous = self.state.open_box.replace(Some(kind.dismiss_command()));
            let answer = message_box(Some(self.hwnd), text, PRODUCT_NAME, kind);
            self.state.open_box.set(previous);
            if self.state.is_dismissed() {
                return Err(WinError::new(E_ABORT, "MessageBoxW"));
            }
            answer
        }

        /// Shows the Open dialog for one existing file of the types `filters`, owned by the
        /// dialog ([`open_file`](crate::file_dialog::open_file)), and returns the path the
        /// user chose, or `None` when the user cancelled. Its modal loop runs inside the
        /// callback, as a message box's does (see "Re-entrancy" in the module documentation),
        /// and a dismiss of the dialog closes it with `IDCANCEL`, as Esc does; the call then
        /// fails with `E_ABORT`, as it does at once, without showing anything, once the
        /// dialog was dismissed. Also fails for a filter that
        /// [`filter_units`](crate::file_dialog::filter_units) refuses, and when the dialog
        /// cannot be shown. The path is user content: never log it.
        pub fn choose_file(&self, filters: &[FileFilter<'_>]) -> WinResult<Option<PathBuf>> {
            if self.state.is_dismissed() {
                return Err(WinError::new(E_ABORT, "GetOpenFileNameW"));
            }
            let previous = self.state.open_box.replace(Some(IDCANCEL));
            let chosen = open_file(Some(self.hwnd), filters);
            self.state.open_box.set(previous);
            if self.state.is_dismissed() {
                return Err(WinError::new(E_ABORT, "GetOpenFileNameW"));
            }
            chosen
        }

        /// Posts the private `message` to the dialog itself, for the handler's
        /// [`app_message`](DialogHandler::app_message) after the current callback. Posted from
        /// [`DialogHandler::init`], it arrives before the dialog is shown: a dialog box shows
        /// itself once its message queue first runs empty. False, and nothing is posted, for a
        /// reserved message ([`super::is_reserved`]) and when posting fails. Like any private
        /// message, one that arrives while the handler runs a modal loop does not reach it.
        pub fn post(&self, message: AppMessage, wparam: usize, lparam: isize) -> bool {
            if is_reserved(message.id()) {
                return false;
            }
            // SAFETY: posts plain values to this thread's dialog.
            unsafe {
                PostMessageW(
                    Some(self.hwnd),
                    message.id(),
                    WPARAM(wparam),
                    LPARAM(lparam),
                )
            }
            .is_ok()
        }

        /// Limits what the user can type or paste into edit `id` to `units` UTF-16 code units
        /// (`EM_SETLIMITTEXT`; 0 lifts the limit to the edit's maximum). The text already in
        /// the edit, and a text set with [`set_text`](Self::set_text), are not shortened. Does
        /// nothing when there is no such control.
        pub fn set_text_limit(&self, id: u16, units: usize) {
            self.send_item(id, EM_SETLIMITTEXT, units, 0);
        }

        /// Replaces the rows of the virtual list view `id`
        /// ([`Control::virtual_list_view`](crate::dialog_template::Control::virtual_list_view))
        /// with `rows`: each row's cells go to the columns in order, and a missing cell is
        /// empty. The dialog keeps the cells and hands each one to the list view itself when
        /// the list view shows or reports it (`LVN_GETDISPINFOW`, which never reaches the
        /// handler), also while the handler is busy. So a list of any length is replaced at once
        /// (`LVM_SETITEMCOUNT`, then the list view redraws what it shows); the scroll position
        /// is kept where it can be, and nothing is selected afterwards (`LVN_ITEMCHANGED`
        /// does not reach the busy handler).
        ///
        /// The dialog also answers the list view's searches (`LVN_ODFINDITEMW`, which never
        /// reaches the handler either), so that typing into the list view selects a row as in
        /// any list view: the first row from the one the search names (or, with `LVFI_WRAP`,
        /// from the first row on) whose first cell equals the text, or starts with it for
        /// `LVFI_PARTIAL` (`LVFI_SUBSTRING`), ignoring case; a search by `lParam` or by
        /// position finds nothing. Common Controls 6 (the version `raumakey.exe` uses) searches
        /// that way as the user types, from the row after the focused one, and selects the row
        /// found, which the handler learns from `LVN_ITEMCHANGED`, or keeps the selection when
        /// nothing matches; it also reports `LVN_INCREMENTALSEARCHW` first, which reaches the
        /// handler and changes nothing.
        ///
        /// Fails with `E_INVALIDARG`, before anything changes, for a cell with a NUL, for more
        /// than `i32::MAX` rows, and for a list view without `LVS_OWNERDATA`; fails when there
        /// is no such control. The cells may be user content: never log them.
        pub fn list_set_virtual_rows(&self, id: u16, rows: &[Vec<String>]) -> WinResult<()> {
            let rows = rows
                .iter()
                .map(|row| {
                    row.iter()
                        .map(|cell| WideString::new(cell))
                        .collect::<WinResult<Vec<_>>>()
                })
                .collect::<WinResult<Vec<_>>>()?;
            let count = i32::try_from(rows.len())
                .map_err(|_| WinError::new(E_INVALIDARG, "list view row count"))?;
            let list = self.item(id)?;
            // SAFETY: only reads the style of a control of this thread.
            let style = unsafe { GetWindowLongW(list, GWL_STYLE) } as u32;
            if style & LVS_OWNERDATA == 0 {
                return Err(WinError::new(E_INVALIDARG, "virtual list view"));
            }
            self.list_select(id, None)?;
            // Never borrowed while a helper runs; fail rather than panic should it be.
            self.state
                .virtual_rows
                .try_borrow_mut()
                .map_err(|_| WinError::new(E_FAIL, "virtual list view rows"))?
                .set(id, rows);
            // SAFETY: a control of this thread; no pointers. The list view may ask for cells
            // during the call; the dialog procedure answers from the rows stored above.
            let set = unsafe {
                SendMessageW(
                    list,
                    LVM_SETITEMCOUNT,
                    Some(WPARAM(count as usize)),
                    Some(LPARAM(LVSICF_NOSCROLL as isize)),
                )
            }
            .0;
            if set == 0 {
                return Err(WinError::new(E_FAIL, "LVM_SETITEMCOUNT"));
            }
            // SAFETY: a control of this thread; the whole client area is redrawn.
            let _ = unsafe { InvalidateRect(Some(list), None, false) };
            Ok(())
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
        /// The `wParam` that the reserved messages of this dialog carry ([`new_cookie`]).
        cookie: usize,
        /// The dialog window, once `WM_INITDIALOG` installed this state (0 before).
        hwnd: Cell<isize>,
        /// `WM_DESTROY` reached the hidden dialog: it is being destroyed, and the handler gets
        /// no more callbacks while it stays hidden (see [`dispatch`]).
        destroyed: Cell<bool>,
        /// The dialog was dismissed (see also [`is_dismissed`](Self::is_dismissed)).
        dismissed: Cell<bool>,
        /// The dialog ended during `WM_INITDIALOG` because it was dismissed: never shown.
        ended_unshown: Cell<bool>,
        /// [`Dialog::focus`] moved the focus (during `init`, the default focus is skipped).
        focus_moved: Cell<bool>,
        /// While [`Dialog::message_box`] or [`Dialog::choose_file`] shows its box, the command
        /// that closes it without an answer ([`MessageKind::dismiss_command`], `IDCANCEL` for
        /// the Open dialog).
        open_box: Cell<Option<u16>>,
        /// The icons set with [`Dialog::set_icons`]; the window uses them until it is
        /// destroyed, before this state is dropped.
        icons: RefCell<Option<Icons>>,
        /// The rows of the virtual list views ([`Dialog::list_set_virtual_rows`]).
        virtual_rows: RefCell<VirtualRows>,
        /// The texts set with [`Dialog::set_text_wrapped`], by control, as they were given:
        /// wrapped again after a change of DPI ([`rewrap`]).
        wrapped: RefCell<Vec<(u16, String)>>,
    }

    impl DialogState {
        fn new(handler: Box<dyn DialogHandler>, shared: Option<Arc<Shared>>) -> Self {
            Self {
                handler: RefCell::new(handler),
                cookie: shared
                    .as_ref()
                    .map_or_else(new_cookie, |shared| shared.cookie),
                shared,
                hwnd: Cell::new(0),
                destroyed: Cell::new(false),
                dismissed: Cell::new(false),
                ended_unshown: Cell::new(false),
                focus_moved: Cell::new(false),
                open_box: Cell::new(None),
                icons: RefCell::new(None),
                virtual_rows: RefCell::new(VirtualRows::default()),
                wrapped: RefCell::new(Vec::new()),
            }
        }

        /// Keeps `text` as the wrapped text of control `id` (see [`rewrap`]).
        fn keep_wrapped(&self, id: u16, text: &str) {
            // Never borrowed while a helper runs; a text not kept is only not wrapped again.
            if let Ok(mut wrapped) = self.wrapped.try_borrow_mut() {
                wrapped.retain(|(known, _)| *known != id);
                wrapped.push((id, text.to_owned()));
            }
        }

        /// Control `id` has no wrapped text any more.
        fn forget_wrapped(&self, id: u16) {
            if let Ok(mut wrapped) = self.wrapped.try_borrow_mut() {
                wrapped.retain(|(known, _)| *known != id);
            }
        }

        /// True once the dialog was dismissed, including by its [`DetachedDialog`] before the
        /// window handle was published (during `init`).
        fn is_dismissed(&self) -> bool {
            self.dismissed.get()
                || self
                    .shared
                    .as_ref()
                    .is_some_and(|shared| shared.dismissed.load(Ordering::SeqCst))
        }

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
    struct Shared {
        /// The `wParam` of the dialog's reserved messages ([`new_cookie`]).
        cookie: usize,
        /// The dialog window once its `init` ran; 0 before, and again once
        /// `DialogBoxIndirectParamW` returned.
        hwnd: AtomicIsize,
        /// The owner dismissed the dialog: a dialog not shown yet is never shown.
        dismissed: AtomicBool,
        /// The dialog was closed (or never shown).
        closed: AtomicBool,
    }

    impl Shared {
        fn new() -> Self {
            Self {
                cookie: new_cookie(),
                hwnd: AtomicIsize::new(0),
                dismissed: AtomicBool::new(false),
                closed: AtomicBool::new(false),
            }
        }
    }

    impl fmt::Debug for Shared {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            // The cookie stays out of logs.
            f.debug_struct("Shared")
                .field("hwnd", &self.hwnd)
                .field("dismissed", &self.dismissed)
                .field("closed", &self.closed)
                .finish_non_exhaustive()
        }
    }

    /// A new `wParam` for the reserved messages of one dialog (see "Forged messages" in the
    /// module documentation): SipHash under keys drawn from the system's random generator
    /// ([`RandomState`]) of a process-wide counter and the thread id, so other processes cannot
    /// predict it; never 0.
    fn new_cookie() -> usize {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        // SAFETY: GetCurrentThreadId has no preconditions.
        let thread = unsafe { GetCurrentThreadId() };
        // Keeps the low bits of the hash on 32-bit targets, as many as `wParam` holds.
        RandomState::new().hash_one((count, thread)) as usize | 1
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
        ///
        /// `wparam` and `lparam` are plain values, never pointers: other processes may post
        /// the same message with any values, so the handler cannot trust them, and the message
        /// may be lost. Data travels through state the handler shares with the caller.
        pub fn post(&self, message: AppMessage, wparam: usize, lparam: isize) -> bool {
            !is_reserved(message.id()) && self.post_raw(message.id(), wparam, lparam)
        }

        /// Asks the dialog's thread to restore the dialog if it is minimized and bring it to
        /// the front ([`Dialog::bring_to_front`]), without waiting. False when the dialog is
        /// not up (see [`post`](Self::post)).
        pub fn bring_to_front(&self) -> bool {
            self.post_raw(BRING_TO_FRONT, self.shared.cookie, 0)
        }

        /// Closes the dialog without waiting and without asking its handler: the dialog ends
        /// with `IDCANCEL`, together with the dialogs and the message box it shows (see
        /// "Dismissing" in the module documentation). A dialog that is not shown yet is never
        /// shown, and its `on_close` gets `None`.
        pub fn dismiss(&self) {
            // Pairs with the publication in `initialize`: either the dialog's thread sees the
            // flag before it shows the dialog, or this sees the published handle and posts.
            self.shared.dismissed.store(true, Ordering::SeqCst);
            let _ = self.post_raw(DISMISS, self.shared.cookie, 0);
        }

        fn post_raw(&self, message: u32, wparam: usize, lparam: isize) -> bool {
            let hwnd = self.shared.hwnd.load(Ordering::SeqCst);
            if hwnd == 0 {
                return false;
            }
            let window = HWND(hwnd as *mut c_void);
            // The handle stays published until `DialogBoxIndirectParamW` returns, a while after
            // the window was destroyed when the dialog's thread is held up (a busy desktop), and
            // `PostMessageW` to such a destroyed window may still succeed: a destroyed window
            // takes no message, so it was not reached.
            // SAFETY: only queries whether the handle names a window.
            if !unsafe { IsWindow(Some(window)) }.as_bool() {
                return false;
            }
            // SAFETY: posts plain values; a handle that no longer names a window makes the call
            // fail.
            unsafe { PostMessageW(Some(window), message, WPARAM(wparam), LPARAM(lparam)) }.is_ok()
        }
    }

    /// Runs `template` without owner on a new thread (named `raumakey-dialog`) and returns at
    /// once; when the dialog closes, `on_close` gets its result on that thread, after the
    /// handler was dropped (`None` only when the dialog was dismissed before it was shown, or
    /// could not be shown; a panic while the handler is dropped is contained and leaves the
    /// result as it was). Fails when the thread cannot start. A panic in `on_close` is
    /// contained.
    pub fn spawn_dialog(
        template: TemplateBytes,
        handler: impl DialogHandler + Send + 'static,
        on_close: impl FnOnce(Option<isize>) + Send + 'static,
    ) -> WinResult<DetachedDialog> {
        let shared = Arc::new(Shared::new());
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
    /// dropped before this returns, after the dialog is destroyed; a panic while it is dropped
    /// is contained and leaves the result as it was.
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
    /// dialog was dismissed before it was shown. The handler is dropped last, and a panic
    /// meanwhile (it is the caller's code) does not lose the outcome.
    fn run(
        template: &TemplateBytes,
        owner: Option<HWND>,
        handler: Box<dyn DialogHandler>,
        shared: Option<Arc<Shared>>,
    ) -> WinResult<Option<isize>> {
        let state = DialogState::new(handler, shared);
        let outcome = show(template, owner, &state);
        guard_value((), move || drop(state));
        outcome
    }

    /// Shows the dialog of `state` and returns once it ended (see [`run`]); afterwards nothing
    /// reaches `state` any more: the window is no longer published, nor in the thread's list
    /// of dialogs, and its slot no longer points to `state`.
    fn show(
        template: &TemplateBytes,
        owner: Option<HWND>,
        state: &DialogState,
    ) -> WinResult<Option<isize>> {
        register_common_controls()?;
        let module = Module::containing_this_crate()?;
        let pointer: *const DialogState = state;
        let result = {
            let _starting = Starting::mark(pointer);
            // SAFETY: the template is a DWORD-aligned extended template that lives during the
            // call, and `DialogTemplate::build` refused everything that would make USER read
            // it otherwise than it was written (a text that starts with the ordinal marker, the
            // message box point size), so USER reads its bytes only. `dialog_proc` reads
            // `state` through the init parameter only during `WM_INITDIALOG` of this call (see
            // `Starting`), and through the dialog's slot until the call returns (the slot is
            // cleared below should the window outlive it). USER validates the owner handle.
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
        // Done here rather than at `WM_DESTROY`, which anyone may forge (see "Forged
        // messages" in the module documentation).
        if let Some(shared) = &state.shared {
            shared.hwnd.store(0, Ordering::SeqCst);
        }
        OPEN.with(|open| {
            if let Ok(mut open) = open.try_borrow_mut() {
                open.retain(|&(dialog, _)| dialog != hwnd);
            }
        });
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
        /// The dialogs of this thread that this module runs, with their cookies, from
        /// `WM_INITDIALOG` until `DialogBoxIndirectParamW` returns (so that dismissing a dialog
        /// reaches the dialogs it owns).
        static OPEN: RefCell<Vec<(isize, usize)>> = const { RefCell::new(Vec::new()) };
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
                open.push((hwnd.0 as isize, state.cookie));
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
    ///
    /// A reserved message counts only with the dialog's cookie in `wParam`, and `WM_DESTROY`
    /// only when the dialog is hidden, as `DestroyWindow` hides a window before it sends that
    /// message: a forged one that reaches the shown dialog is ignored (see "Forged messages"
    /// in the module documentation). Should a forged one reach the dialog before it is first
    /// shown, the handler gets its callbacks again once the dialog is visible.
    fn dispatch(
        state: &DialogState,
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> isize {
        match message {
            DISMISS | BRING_TO_FRONT | REWRAP if wparam.0 != state.cookie => return FALSE,
            DISMISS => {
                dismiss(state, hwnd);
                return TRUE;
            }
            BRING_TO_FRONT => {
                bring_to_front(hwnd);
                return TRUE;
            }
            REWRAP => {
                rewrap(state, hwnd);
                return TRUE;
            }
            WM_DESTROY => {
                if !is_visible(hwnd) {
                    state.destroyed.set(true);
                }
                return FALSE;
            }
            WM_DPICHANGED => {
                // The low word is the new horizontal DPI (both are equal).
                state.reload_icons(hwnd, u32::from(low_word(wparam.0)));
                if state
                    .wrapped
                    .try_borrow()
                    .is_ok_and(|wrapped| !wrapped.is_empty())
                {
                    // The default processing scales the controls and their fonts once this
                    // returns; the texts are wrapped again after it.
                    // SAFETY: posts plain values to this thread's dialog.
                    let _ = unsafe {
                        PostMessageW(Some(hwnd), REWRAP, WPARAM(state.cookie), LPARAM(0))
                    };
                }
                return FALSE;
            }
            WM_NOTIFY => match answer_virtual_list(state, hwnd, lparam) {
                Some(VirtualAnswer::InPlace) => return FALSE,
                Some(VirtualAnswer::Result(result)) => {
                    write_long(hwnd, DWLP_MSGRESULT, result);
                    return TRUE;
                }
                None => {}
            },
            _ => {}
        }
        if state.destroyed.get() {
            if !is_visible(hwnd) {
                return FALSE;
            }
            // A destroyed dialog never shows again: that `WM_DESTROY` was forged.
            state.destroyed.set(false);
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
                    let (id, code) = (low_word(header.idFrom), header.code as i32);
                    handler.notify(&dialog, id, code);
                    if code == LVN_KEYDOWN
                        && let Some(key) = list_key(lparam)
                    {
                        handler.list_key(&dialog, id, key);
                    }
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
        // SAFETY: `NMHDR` holds a window handle and integers: any bytes make a valid one.
        let header = unsafe { read_memory::<NMHDR>(lparam.0 as usize) }?;
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

    /// The virtual-key code of the `NMLVKEYDOWN` that the `WM_NOTIFY` argument `lparam` points
    /// to (its header checked by [`notify_header`] already), copied; `None` when it cannot be
    /// read (see "Forged messages" in the module documentation).
    fn list_key(lparam: LPARAM) -> Option<u16> {
        // SAFETY: `NMLVKEYDOWN` holds a header of a window handle and integers, then two
        // integers: any bytes make a valid one.
        let down = unsafe { read_memory::<NMLVKEYDOWN>(lparam.0 as usize) }?;
        Some(down.wVKey)
    }

    /// How many lent texts each virtual list view keeps ([`VirtualList`]): a list view may use
    /// the text of an answer to `LVN_GETDISPINFOW` until it has sent two more requests for
    /// itself (those of another list view of the dialog do not count).
    const LENT_TEXTS: usize = 3;

    /// The most UTF-16 units of a search text (`LVN_ODFINDITEMW`) that the dialog reads; a
    /// longer text finds nothing.
    const SEARCH_TEXT_UNITS: usize = 1024;

    /// The block size in which a search text is read: a block never crosses a page boundary
    /// (pages are 4 KiB or a multiple of it), so a text that ends before memory that cannot be
    /// read is still read whole.
    const READ_BLOCK: usize = 4096;

    /// The `LVFINDINFOW` flags of the searches the dialog answers: by text (`LVFI_STRING`), for
    /// a prefix (`LVFI_PARTIAL`, `LVFI_SUBSTRING`), going on from the first row (`LVFI_WRAP`,
    /// which alone asks for a search by text too).
    const TEXT_SEARCH: u32 = LVFI_STRING.0 | LVFI_PARTIAL.0 | LVFI_SUBSTRING.0 | LVFI_WRAP.0;

    /// The rows of a dialog's virtual list views ([`Dialog::list_set_virtual_rows`]).
    #[derive(Default)]
    struct VirtualRows {
        /// Each virtual list view, in the order its rows were first set.
        lists: Vec<VirtualList>,
    }

    impl VirtualRows {
        fn set(&mut self, id: u16, rows: Vec<Vec<WideString>>) {
            match self.list_mut(id) {
                Some(list) => list.rows = rows,
                None => self.lists.push(VirtualList {
                    id,
                    rows,
                    lent: Default::default(),
                    next: 0,
                }),
            }
        }

        /// The virtual list view `id`; `None` when `id` names no virtual list view of the
        /// dialog.
        fn list_mut(&mut self, id: u16) -> Option<&mut VirtualList> {
            self.lists.iter_mut().find(|list| list.id == id)
        }

        /// [`VirtualList::lend`] of list `id`.
        #[cfg(test)]
        fn lend(&mut self, id: u16, row: i32, column: i32) -> Option<PWSTR> {
            self.list_mut(id)?.lend(row, column)
        }

        /// True while `text`, lent for list `id`, is still kept.
        #[cfg(test)]
        fn keeps(&self, id: u16, text: PWSTR) -> bool {
            self.lists.iter().any(|list| {
                list.id == id
                    && list
                        .lent
                        .iter()
                        .flatten()
                        .any(|kept| kept.as_pcwstr().0 == text.0.cast_const())
            })
        }
    }

    /// The rows of one virtual list view, and the copies of cells lent to it.
    struct VirtualList {
        /// The list view's control id.
        id: u16,
        rows: Vec<Vec<WideString>>,
        /// The texts of the last answers, which the list view may still read: an answer points
        /// the request at one of these instead of writing through the request's buffer, so
        /// rows may be replaced at any time.
        lent: [Option<WideString>; LENT_TEXTS],
        /// The slot of `lent` that the next answer takes.
        next: usize,
    }

    impl VirtualList {
        /// A copy of the cell at `row` and `column` (empty when the row is shorter or there is
        /// no such row), kept until this list view got [`LENT_TEXTS`] more answers, and the
        /// pointer to lend.
        fn lend(&mut self, row: i32, column: i32) -> Option<PWSTR> {
            let cell = usize::try_from(row)
                .ok()
                .zip(usize::try_from(column).ok())
                .and_then(|(row, column)| self.rows.get(row)?.get(column))
                .cloned()
                .or_else(|| WideString::new("").ok())?;
            let slot = self.lent.get_mut(self.next % LENT_TEXTS)?;
            self.next = (self.next + 1) % LENT_TEXTS;
            let text = slot.insert(cell);
            Some(PWSTR(text.as_pcwstr().0.cast_mut()))
        }

        /// The first row from row `start` on (from the first row when `start` is negative, from
        /// none when it is past the last row) whose first cell equals `text`, or starts with it
        /// for a `prefix` search, ignoring case ([`matches_text`]); with `wrap`, the search goes
        /// on from the first row up to `start`. Common Controls 6 searches from the row after
        /// the focused one, which may be one past the last, with `wrap`, as the user types.
        fn find(&self, start: i32, text: &[u16], prefix: bool, wrap: bool) -> Option<usize> {
            let count = self.rows.len();
            let start = usize::try_from(start).unwrap_or(0).min(count);
            let wrapped = if wrap { 0..start } else { 0..0 };
            (start..count).chain(wrapped).find(|&row| {
                let cell = self
                    .rows
                    .get(row)
                    .and_then(|cells| cells.first())
                    .map_or(&[][..], WideString::as_units);
                matches_text(cell, text, prefix)
            })
        }
    }

    /// `units` as the search compares them: decoded (ill-formed UTF-16 as U+FFFD) and in lower
    /// case (`char::to_lowercase`).
    fn folded(units: &[u16]) -> impl Iterator<Item = char> + '_ {
        char::decode_utf16(units.iter().copied())
            .map(|decoded| decoded.unwrap_or(char::REPLACEMENT_CHARACTER))
            .flat_map(char::to_lowercase)
    }

    /// True when `cell` equals `text`, or starts with it for a `prefix` search, ignoring case
    /// ([`folded`]).
    fn matches_text(cell: &[u16], text: &[u16], prefix: bool) -> bool {
        let (mut cell, mut text) = (folded(cell), folded(text));
        loop {
            match (text.next(), cell.next()) {
                (None, None) => return true,
                (None, Some(_)) => return prefix,
                (Some(wanted), Some(found)) if wanted == found => {}
                _ => return false,
            }
        }
    }

    /// What the dialog procedure answers itself to a request of a virtual list view.
    enum VirtualAnswer {
        /// The request was answered in place (or could not be read): the dialog answers 0, as
        /// for any notification left to the default processing.
        InPlace,
        /// The notification's result (`DWLP_MSGRESULT`).
        Result(isize),
    }

    /// Answers the request of a virtual list view of `dialog`
    /// ([`Dialog::list_set_virtual_rows`]) that the `WM_NOTIFY` argument `lparam` points to,
    /// when it names that list view by its handle and id ([`notify_header`]):
    /// `LVN_GETDISPINFOW` ([`lend_cell`]) and `LVN_ODFINDITEMW` ([`find_row`]). `None` for
    /// anything else, which goes to the handler.
    fn answer_virtual_list(
        state: &DialogState,
        dialog: HWND,
        lparam: LPARAM,
    ) -> Option<VirtualAnswer> {
        let header = notify_header(dialog, lparam)?;
        if header.code != LVN_GETDISPINFOW && header.code != LVN_ODFINDITEMW {
            return None;
        }
        // Never borrowed while a helper runs, which is when list views ask.
        let mut rows = state.virtual_rows.try_borrow_mut().ok()?;
        let list = rows.list_mut(low_word(header.idFrom))?;
        Some(if header.code == LVN_GETDISPINFOW {
            lend_cell(list, lparam);
            VirtualAnswer::InPlace
        } else {
            let found = find_row(list, lparam).and_then(|row| isize::try_from(row).ok());
            VirtualAnswer::Result(found.unwrap_or(-1))
        })
    }

    /// Answers the `LVN_GETDISPINFOW` request at `lparam` for `list`: lends the cell asked for
    /// ([`VirtualList::lend`]) by setting the request's `pszText`.
    ///
    /// Only that one field of the request is written, with `WriteProcessMemory`, and only when
    /// it lies in committed read-write memory ([`is_read_write`]): nothing is written through a
    /// pointer the request carries, so a forged request (see "Forged messages" in the module
    /// documentation) can at most change a pointer-sized field of writable memory of this
    /// process that holds a notification of this list.
    fn lend_cell(list: &mut VirtualList, lparam: LPARAM) {
        // SAFETY: `NMLVDISPINFOW` holds integers, handles, and pointers that are never
        // followed: any bytes make a valid one.
        let Some(request) = (unsafe { read_memory::<NMLVDISPINFOW>(lparam.0 as usize) }) else {
            return;
        };
        let offset =
            core::mem::offset_of!(NMLVDISPINFOW, item) + core::mem::offset_of!(LVITEMW, pszText);
        let field = (lparam.0 as usize).wrapping_add(offset);
        if !request.item.mask.contains(LVIF_TEXT) || !is_read_write(field, size_of::<PWSTR>()) {
            return;
        }
        let Some(lent) = list.lend(request.item.iItem, request.item.iSubItem) else {
            return;
        };
        // SAFETY: writes the pointer-sized `pszText` field of the request, which lies in
        // committed read-write memory of this process (checked just above); the call fails
        // instead of faulting should another thread have changed that meanwhile.
        let _ = unsafe {
            WriteProcessMemory(
                GetCurrentProcess(),
                field as *const c_void,
                (&raw const lent).cast(),
                size_of::<PWSTR>(),
                None,
            )
        };
    }

    /// Answers the `LVN_ODFINDITEMW` request at `lparam` for `list`: the row that
    /// [`VirtualList::find`] finds for its `iStart`, text, and flags, or `None` (the answer
    /// -1) when nothing matches, when the request or its text cannot be read ([`read_text`]),
    /// and for flags other than [`TEXT_SEARCH`] (searches by `lParam` or by position).
    fn find_row(list: &VirtualList, lparam: LPARAM) -> Option<usize> {
        // SAFETY: `NMLVFINDITEMW` holds integers, handles, and pointers that are never
        // followed: any bytes make a valid one.
        let request = unsafe { read_memory::<NMLVFINDITEMW>(lparam.0 as usize) }?;
        let flags = request.lvfi.flags.0;
        if flags == 0 || flags & !TEXT_SEARCH != 0 {
            return None;
        }
        let text = read_text(request.lvfi.psz.0 as usize)?;
        let prefix = flags & (LVFI_PARTIAL.0 | LVFI_SUBSTRING.0) != 0;
        list.find(request.iStart, &text, prefix, flags & LVFI_WRAP.0 != 0)
    }

    /// A copy of the `T` at `address` in this process's memory, or `None` when any of its
    /// bytes cannot be read: `ReadProcessMemory` fails instead of faulting, and no reference to
    /// the source is ever made (see "Forged messages" in the module documentation).
    ///
    /// # Safety
    ///
    /// Any bytes must make a valid `T` (a C structure of integers, handles, and pointers).
    unsafe fn read_memory<T: Default>(address: usize) -> Option<T> {
        let mut value = T::default();
        let mut copied = 0_usize;
        // SAFETY: copies from this process's own memory into `value`, which is writable for the
        // size passed, and which any bytes leave valid (the caller's promise); the call fails
        // instead of faulting where the source cannot be read.
        unsafe {
            ReadProcessMemory(
                GetCurrentProcess(),
                address as *const c_void,
                (&raw mut value).cast(),
                size_of::<T>(),
                Some(&mut copied),
            )
        }
        .ok()?;
        (copied == size_of::<T>()).then_some(value)
    }

    /// The NUL-terminated UTF-16 text at `address` in this process's memory, without its NUL,
    /// read without trusting the pointer: [`READ_BLOCK`] by block with `ReadProcessMemory`, at
    /// most [`SEARCH_TEXT_UNITS`] units. `None` when the text runs into memory that cannot be
    /// read before its NUL, or has no NUL within the bound.
    fn read_text(address: usize) -> Option<Vec<u16>> {
        let limit = (SEARCH_TEXT_UNITS + 1) * size_of::<u16>();
        let mut bytes: Vec<u8> = Vec::with_capacity(limit);
        while bytes.len() < limit {
            let start = bytes.len();
            let at = address.checked_add(start)?;
            let block = (READ_BLOCK - at % READ_BLOCK).min(limit - start);
            bytes.resize(start + block, 0);
            let target = bytes.get_mut(start..)?;
            let mut copied = 0_usize;
            // SAFETY: copies from this process's own memory into the new tail of `bytes`,
            // which is writable for the size passed; the call fails instead of faulting where
            // the source cannot be read.
            unsafe {
                ReadProcessMemory(
                    GetCurrentProcess(),
                    at as *const c_void,
                    target.as_mut_ptr().cast(),
                    block,
                    Some(&mut copied),
                )
            }
            .ok()?;
            if copied != block {
                return None;
            }
            let (pairs, _) = bytes.as_chunks::<2>();
            let mut units: Vec<u16> = pairs.iter().map(|&pair| u16::from_ne_bytes(pair)).collect();
            if let Some(end) = units.iter().position(|&unit| unit == 0) {
                units.truncate(end);
                return Some(units);
            }
        }
        None
    }

    /// True when the `size` bytes at `address` lie in committed memory of this process that
    /// is plainly readable and writable (`PAGE_READWRITE` or `PAGE_WRITECOPY`, without
    /// `PAGE_GUARD` or any other modifier), as `VirtualQuery` reports it.
    /// `WriteProcessMemory` fails instead of faulting on memory it cannot write, but makes a
    /// read-only executable page writable for the write (and restores it afterwards), so a
    /// forged request could otherwise patch code. Hardening, not a boundary: another thread
    /// may change the protection between the query and the write.
    fn is_read_write(address: usize, size: usize) -> bool {
        let mut info = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: queries this process's address space (any address may be asked about) into
        // a valid out buffer of the size passed.
        let answered = unsafe {
            VirtualQuery(
                Some(address as *const c_void),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        let base = info.BaseAddress as usize;
        let end = base.saturating_add(info.RegionSize);
        answered == size_of::<MEMORY_BASIC_INFORMATION>()
            && info.State == MEM_COMMIT
            && (info.Protect == PAGE_READWRITE || info.Protect == PAGE_WRITECOPY)
            && address >= base
            && address.checked_add(size).is_some_and(|after| after <= end)
    }

    /// Asks the handler whether the dialog may close, and ends it with `IDCANCEL` if so.
    fn close(handler: &mut dyn DialogHandler, dialog: &Dialog<'_>) {
        if handler.close_requested(dialog) {
            dialog.end(CANCELLED);
        }
    }

    /// True when `hwnd` has `WS_VISIBLE` (and so do its parents; a dialog has none).
    fn is_visible(hwnd: HWND) -> bool {
        // SAFETY: only reads the window's state; USER validates `hwnd`.
        unsafe { IsWindowVisible(hwnd) }.as_bool()
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

    /// Wraps the texts set with [`Dialog::set_text_wrapped`] again for the controls' sizes and
    /// fonts now, after a change of DPI. Labels and buttons notify nobody of a new text, so
    /// this never reaches the handler, and it runs while the handler is busy too.
    fn rewrap(state: &DialogState, hwnd: HWND) {
        let texts = match state.wrapped.try_borrow() {
            Ok(wrapped) => wrapped.clone(),
            Err(_) => return,
        };
        let dialog = Dialog { hwnd, state };
        for (id, text) in texts {
            // A control that cannot be measured keeps the lines it shows.
            let _ = dialog.set_text_wrapped(id, &text);
        }
    }

    /// Ends the dialog with `IDCANCEL` without asking its handler, and closes what it shows:
    /// the dialogs of this module that it owns get the same reserved message, and the message
    /// box of [`Dialog::message_box`] the command that closes it
    /// ([`MessageKind::dismiss_command`]), as the Open dialog of [`Dialog::choose_file`] does
    /// (`IDCANCEL`).
    fn dismiss(state: &DialogState, hwnd: HWND) {
        state.dismissed.set(true);
        // SAFETY: the dialog is a window of this thread run by DialogBoxIndirectParamW.
        let _ = unsafe { EndDialog(hwnd, CANCELLED) };
        let mut closing = Closing {
            owner: hwnd,
            box_command: state.open_box.get(),
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
        /// The command that closes the message box or the Open dialog it shows, if it shows
        /// one ([`MessageKind::dismiss_command`], `IDCANCEL`).
        box_command: Option<u16>,
        /// The dialogs of this module on this thread, with their cookies.
        open: Vec<(isize, usize)>,
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
            let open = closing
                .open
                .iter()
                .find(|&&(dialog, _)| dialog == hwnd.0 as isize);
            let (message, wparam) = if let Some(&(_, cookie)) = open {
                (DISMISS, cookie)
            } else if let Some(command) = closing.box_command
                && is_dialog_class(hwnd)
            {
                (WM_COMMAND, usize::from(command))
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

    fn write_slot(hwnd: HWND, value: isize) {
        write_long(hwnd, DWLP_USER, value);
    }

    /// Writes slot `index` of the extra bytes of a dialog of this thread: `DWLP_USER`, or
    /// `DWLP_MSGRESULT` (what the dialog answers when its procedure returns `TRUE`).
    #[cfg(target_pointer_width = "64")]
    fn write_long(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) {
        use windows::Win32::UI::WindowsAndMessaging::SetWindowLongPtrW;
        // SAFETY: writes a slot of the extra bytes that the dialog class reserves.
        unsafe { SetWindowLongPtrW(hwnd, index, value) };
    }

    #[cfg(target_pointer_width = "32")]
    fn write_long(hwnd: HWND, index: WINDOW_LONG_PTR_INDEX, value: isize) {
        use windows::Win32::UI::WindowsAndMessaging::SetWindowLongW;
        // SAFETY: as above; on 32-bit Windows `isize` is 32 bits wide.
        unsafe { SetWindowLongW(hwnd, index, value as i32) };
    }

    #[cfg(test)]
    mod tests {
        use super::{LENT_TEXTS, VirtualRows};
        use crate::strings::WideString;

        fn one_cell(text: &str) -> Vec<Vec<WideString>> {
            vec![vec![WideString::new(text).unwrap()]]
        }

        /// A list view may use a lent text until it asked for `LENT_TEXTS - 1` more cells: the
        /// cells another list view of the dialog asks for meanwhile do not count.
        #[test]
        fn each_virtual_list_keeps_the_texts_lent_to_it() {
            let mut rows = VirtualRows::default();
            rows.set(1, one_cell("một"));
            rows.set(2, one_cell("hai"));
            let first = rows.lend(1, 0, 0).unwrap();
            for _ in 0..2 * LENT_TEXTS {
                rows.lend(2, 0, 0).unwrap();
            }
            assert!(rows.keeps(1, first));
            // Replacing the rows keeps what was lent.
            rows.set(1, one_cell("ba"));
            assert!(rows.keeps(1, first));
            for _ in 1..LENT_TEXTS {
                rows.lend(1, 0, 0).unwrap();
            }
            assert!(rows.keeps(1, first));
            rows.lend(1, 0, 0).unwrap();
            assert!(!rows.keeps(1, first));
        }
    }
}
