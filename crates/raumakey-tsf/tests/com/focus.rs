//! Documents, focus, the mouse hook, and the mode, against the real TSF thread manager of
//! this process (plan §10.3; matrix #19, #20, #24; D24).
//!
//! Reference counts show which sinks TSF holds: activation advises four sinks and installs
//! the mouse hook, which holds the service too, and the text edit sink is advised on the
//! focused context only.

use raumakey_tip::mouse_hook;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_CONTROL, VK_SHIFT};
use windows::Win32::UI::TextServices::{ITfDocumentMgr, ITfThreadFocusSink};
use windows::core::Interface;

use crate::support::{on_sta, serial};
use crate::tsf::{Document, Tip, press, release, type_text};

/// Taps Ctrl+Shift in `document`: the default toggle of the Vietnamese mode (D5).
fn tap_ctrl_shift(tip: &Tip, document: &Document) {
    let context = Some(&document.context);
    let (ctrl, shift) = (u32::from(VK_CONTROL.0), u32::from(VK_SHIFT.0));
    assert_eq!(press(tip, context, ctrl), (false, false));
    assert_eq!(press(tip, context, shift), (false, false));
    assert!(!release(tip, context, shift));
    assert!(
        !release(tip, context, ctrl),
        "the key-up of a modifier-only hotkey reaches the application"
    );
}

#[test]
fn activation_installs_the_mouse_hook_of_the_thread_and_deactivation_removes_it() {
    let _serial = serial();
    on_sta(|| {
        assert!(!mouse_hook::is_installed());
        let tip = Tip::activate();
        assert!(mouse_hook::is_installed());
        let other_thread = std::thread::spawn(mouse_hook::is_installed)
            .join()
            .expect("the thread ran");
        assert!(!other_thread, "the hook belongs to the TSF thread only");
        tip.end();
        assert!(!mouse_hook::is_installed());
    });
}

#[test]
fn the_text_edit_sink_follows_the_focus() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let unfocused = tip.held();
        let first = Document::new(&tip);
        let second = Document::new(&tip);
        tip.focus(Some(&first));
        assert_eq!(
            tip.held(),
            unfocused + 1,
            "the edit sink of the first document"
        );
        tip.focus(Some(&second));
        assert_eq!(tip.held(), unfocused + 1, "moved to the second document");
        tip.focus(None);
        assert_eq!(tip.held(), unfocused, "no document, no edit sink");
        tip.focus(Some(&first));
        assert_eq!(tip.held(), unfocused + 1);
        drop((first, second));
        tip.end();
    });
}

#[test]
fn a_focus_without_a_document_manager_is_accepted() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let unfocused = tip.held();
        // SAFETY: a live sink of this thread; both document managers are null.
        unsafe {
            tip.events()
                .OnSetFocus(None::<&ITfDocumentMgr>, None::<&ITfDocumentMgr>)
        }
        .expect("OnSetFocus without a document manager");
        assert_eq!(tip.held(), unfocused);
        tip.end();
    });
}

#[test]
fn activation_advises_the_edit_sink_of_the_document_that_has_the_focus() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let unfocused = tip.held();
        tip.end();

        let (tip, document) = Tip::activate_after(|real, client_id| {
            let document = Document::create(real, client_id);
            // SAFETY: a live thread manager and one of its document managers.
            unsafe { real.SetFocus(&document.manager) }.expect("SetFocus");
            document
        });
        assert_eq!(tip.held(), unfocused + 1, "advised during activation");
        type_text(&tip, &document, "ba ba");
        document.app_moves_caret(2);
        type_text(&tip, &document, "s");
        assert_eq!(document.text(), "bas ba", "the edit sink reports the move");
        drop(document);
        tip.end();
    });
}

#[test]
fn matrix_20_a_focus_change_never_edits_across_documents() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let first = Document::new(&tip);
        let second = Document::new(&tip);
        tip.focus(Some(&first));
        type_text(&tip, &first, "a");
        tip.focus(Some(&second));
        type_text(&tip, &second, "a");
        assert_eq!(second.text(), "a", "the word of the first document ended");
        tip.focus(Some(&first));
        type_text(&tip, &first, "a");
        assert_eq!(first.text(), "aa", "the focus change ended its word");
        // Away and back without a key in between: only the focus reports end the word.
        type_text(&tip, &first, " ca");
        tip.focus(Some(&second));
        tip.focus(Some(&first));
        type_text(&tip, &first, "s");
        assert_eq!(first.text(), "aa cas");
        // Without a focus change the tone reaches the word.
        type_text(&tip, &first, " ca");
        type_text(&tip, &first, "s");
        assert_eq!(first.text(), "aa cas cá");
        drop((first, second));
        tip.end();
    });
}

#[test]
fn losing_the_thread_focus_ends_the_word() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "ca");
        // Another thread takes the focus and gives it back, as TSF reports it to the
        // thread focus sink: keys may have been missed meanwhile.
        let thread_focus: ITfThreadFocusSink = tip.service.cast().expect("ITfThreadFocusSink");
        // SAFETY: calls on a live sink of this thread.
        unsafe { thread_focus.OnKillThreadFocus() }.expect("OnKillThreadFocus");
        // SAFETY: as above.
        unsafe { thread_focus.OnSetThreadFocus() }.expect("OnSetThreadFocus");
        type_text(&tip, &document, "s");
        assert_eq!(
            document.text(),
            "cas",
            "the tone did not reach back over the lost focus"
        );
        // `Tip::end` counts the references to the service: this one goes first.
        drop((thread_focus, document));
        tip.end();
    });
}

#[test]
fn losing_the_keyboard_focus_of_the_key_event_sink_ends_the_word() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "ca");
        // Another text service takes the keyboard (the foreground key event sink) and
        // gives it back.
        let keys = tip.keys();
        // SAFETY: calls on a live sink of this thread.
        unsafe { keys.OnSetFocus(false) }.expect("OnSetFocus(FALSE)");
        // SAFETY: as above.
        unsafe { keys.OnSetFocus(true) }.expect("OnSetFocus(TRUE)");
        type_text(&tip, &document, "s");
        assert_eq!(
            document.text(),
            "cas",
            "the tone did not reach back over the lost keyboard"
        );
        drop((keys, document));
        tip.end();
    });
}

#[test]
fn a_pushed_context_takes_the_focus_and_ends_the_word_of_the_one_below() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let base = Document::new(&tip);
        tip.focus(Some(&base));
        type_text(&tip, &base, "ca");
        // TSF reports the push and the pop to the thread manager event sink; no key reaches
        // the pushed context.
        let pushed = Document::push_on(&base);
        // SAFETY: a live document manager with two contexts.
        unsafe { base.manager.Pop(0) }.expect("Pop");
        type_text(&tip, &base, "s");
        assert_eq!(base.text(), "cas", "the push ended the word");
        drop((pushed, base));
        tip.end();
    });
}

#[test]
fn a_pushed_context_takes_the_focus_and_its_pop_gives_it_back() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let unfocused = tip.held();
        let base = Document::new(&tip);
        tip.focus(Some(&base));
        // TSF reports the push and the pop to the thread manager event sink.
        let pushed = Document::push_on(&base);
        assert_eq!(
            tip.held(),
            unfocused + 1,
            "one edit sink, on the pushed context"
        );
        type_text(&tip, &pushed, "aa");
        assert_eq!(pushed.text(), "â");
        // SAFETY: a live document manager with two contexts.
        unsafe { base.manager.Pop(0) }.expect("Pop");
        assert_eq!(
            tip.held(),
            unfocused + 1,
            "the edit sink is back on the base context"
        );
        type_text(&tip, &base, "ba ba");
        base.app_moves_caret(2);
        type_text(&tip, &base, "s");
        assert_eq!(
            base.text(),
            "bas ba",
            "the base context reports its changes"
        );
        drop((pushed, base));
        tip.end();
    });
}

#[test]
fn an_uninitialized_document_manager_releases_the_edit_sink_of_its_context() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let unfocused = tip.held();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        assert_eq!(tip.held(), unfocused + 1);
        // As TSF reports a document manager whose contexts are gone.
        // SAFETY: a live sink of this thread and a live document manager.
        unsafe { tip.events().OnUninitDocumentMgr(&document.manager) }
            .expect("OnUninitDocumentMgr");
        assert_eq!(tip.held(), unfocused, "the edit sink was unadvised");
        drop(document);
        tip.end();
    });
}

#[test]
fn closing_the_focused_document_releases_its_edit_sink() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let unfocused = tip.held();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        assert_eq!(tip.held(), unfocused + 1);
        // TSF reports `OnPopContext` and `OnUninitDocumentMgr`.
        document.close();
        assert_eq!(tip.held(), unfocused);
        drop(document);
        tip.end();
    });
}

#[test]
fn activation_shows_the_vietnamese_mode_in_the_open_close_compartment() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        assert_eq!(tip.open_close(), Some(1), "defaults v1: Vietnamese on");
        tip.end();
    });
}

#[test]
fn d24_ctrl_shift_toggles_the_mode_and_the_compartment_follows() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        tap_ctrl_shift(&tip, &document);
        assert_eq!(tip.open_close(), Some(0), "English");
        type_text(&tip, &document, "aa");
        // Checked here: the space below would restore a stray "â" to "aa".
        assert_eq!(document.text(), "aa");
        tap_ctrl_shift(&tip, &document);
        assert_eq!(tip.open_close(), Some(1), "Vietnamese");
        type_text(&tip, &document, " aa");
        assert_eq!(document.text(), "aa â");
        drop(document);
        tip.end();
    });
}

#[test]
fn a_change_of_the_compartment_sets_the_mode() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let document = Document::new(&tip);
        tip.focus(Some(&document));
        // The input indicator or the language bar closes the keyboard.
        tip.set_open_close(0);
        type_text(&tip, &document, "aa");
        // Checked here: the space below would restore a stray "â" to "aa".
        assert_eq!(document.text(), "aa");
        tip.set_open_close(1);
        type_text(&tip, &document, " aa");
        assert_eq!(document.text(), "aa â");
        drop(document);
        tip.end();
    });
}
