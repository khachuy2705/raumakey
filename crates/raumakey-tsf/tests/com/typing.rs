//! The key path through real TSF ranges (plan §10.3; matrix #17, #19, #21, #39, #40): keys go
//! through the text service's key event sink, its edits go through synchronous edit sessions
//! on TSF's own text store, and the test plays the application (see `tsf`).
//!
//! These tests show the text service working with TSF's range implementation in this
//! process. They are not evidence of behavior inside a Windows host application, whose text
//! store may grant, report, and lay out text differently; the manual check list in the
//! `raumakey_tip::server` documentation covers that.

use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use raumakey_tip::mouse_hook;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_BACK, VK_CONTROL};
use windows::Win32::UI::WindowsAndMessaging::{
    HC_ACTION, MOUSEHOOKSTRUCT, SetMessageExtraInfo, WM_LBUTTONDOWN,
};

use crate::support::{on_sta, serial};
use crate::tsf::{Document, Tip, hold_ctrl, press, release, type_text};

/// A left click in the host's window, as Windows delivers it to the thread's `WH_MOUSE`
/// hook.
fn click() {
    let mut info = MOUSEHOOKSTRUCT::default();
    // SAFETY: the hook procedure, called as Windows calls it: HC_ACTION with a mouse message
    // and a record that lives for the call.
    unsafe {
        mouse_hook::procedure(
            HC_ACTION as i32,
            WPARAM(WM_LBUTTONDOWN as usize),
            LPARAM(&raw mut info as isize),
        )
    };
}

#[test]
fn telex_words_are_typed_through_real_tsf_ranges() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "tieengs vieetj nguwowif");
        assert_eq!(document.text(), "tiếng việt người");
        drop(document);
        tip.end();
    });
}

#[test]
fn a_document_that_never_had_the_focus_reads_its_input_scope_at_its_first_key() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        // No focus report: until its input scope is read, a document counts as a secure
        // field, so the first key reads it.
        type_text(&tip, &document, "aa");
        assert_eq!(document.text(), "â");
        drop(document);
        tip.end();
    });
}

#[test]
fn matrix_17_backspace_restores_the_previous_state() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "aa");
        assert_eq!(document.text(), "â");
        let back = u32::from(VK_BACK.0);
        assert_eq!(
            press(&tip, Some(&document.context), back),
            (true, true),
            "the text service edits and eats Backspace"
        );
        assert!(!release(&tip, Some(&document.context), back));
        assert_eq!(document.text(), "a");
        type_text(&tip, &document, "a");
        assert_eq!(
            document.text(),
            "â",
            "the word goes on from its earlier state"
        );
        drop(document);
        tip.end();
    });
}

#[test]
fn matrix_19_a_change_the_service_did_not_make_ends_the_word() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "ba ba");
        // The caret moves behind the first "ba": the same letters lie before it, so only the
        // report of the move keeps the tone off the wrong word.
        document.app_moves_caret(2);
        type_text(&tip, &document, "s");
        assert_eq!(document.text(), "bas ba");
        drop(document);
        tip.end();
    });
}

#[test]
fn matrix_19_a_mouse_press_ends_the_word() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "ba");
        click();
        type_text(&tip, &document, "s");
        assert_eq!(
            document.text(),
            "bas",
            "the tone did not reach back over the click"
        );
        drop(document);
        tip.end();
    });
}

#[test]
fn matrix_39_keys_injected_by_raumakey_reach_the_application_untouched() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        // SAFETY: sets the extra information of this thread's current message only.
        unsafe { SetMessageExtraInfo(LPARAM(RAUMAKEY_INJECTED_TAG as isize)) };
        assert_eq!(
            press(&tip, Some(&document.context), u32::from(b'A')),
            (false, false),
            "an injected key is not even claimed"
        );
        document.app_types("a");
        assert!(!release(&tip, Some(&document.context), u32::from(b'A')));
        type_text(&tip, &document, "a");
        // SAFETY: as above.
        unsafe { SetMessageExtraInfo(LPARAM(0)) };
        assert_eq!(document.text(), "aa");
        type_text(&tip, &document, " aa");
        assert_eq!(
            document.text(),
            "aa â",
            "ordinary keys are transformed again"
        );
        drop(document);
        tip.end();
    });
}

#[test]
fn matrix_21_a_letter_typed_with_ctrl_held_is_a_shortcut_that_ends_the_word() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "ca");
        let context = Some(&document.context);
        let (ctrl, letter_s) = (u32::from(VK_CONTROL.0), u32::from(b'S'));
        // Ctrl+S, with Ctrl down in the key state while TSF delivers the keys.
        hold_ctrl(true);
        assert_eq!(press(&tip, context, ctrl), (false, false));
        let (_, eaten) = press(&tip, context, letter_s);
        assert!(!release(&tip, context, letter_s));
        hold_ctrl(false);
        assert!(!release(&tip, context, ctrl));
        assert!(!eaten, "Ctrl+S reaches the application");
        // The application runs its shortcut and types nothing.
        assert_eq!(
            document.text(),
            "ca",
            "the shortcut put no tone on the word"
        );
        type_text(&tip, &document, "s");
        assert_eq!(document.text(), "cas", "the shortcut ended the word");
        drop(document);
        tip.end();
    });
}

#[test]
fn keys_without_a_document_reach_the_application() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        for _ in 0..2 {
            assert_eq!(press(&tip, None, u32::from(b'A')), (false, false));
            assert!(!release(&tip, None, u32::from(b'A')));
        }
        tip.end();
    });
}
