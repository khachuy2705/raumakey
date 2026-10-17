//! The key path in hosts that behave differently from TSF's own text store (plan §10.3;
//! matrix #39, #40, #41): the fields belong to the test store of an application (see
//! `store`), which a test makes refuse a lock, leave the new text selected after an edit, or
//! act while TSF waits for a lock.
//!
//! Like `typing`, these tests show the text service working with TSF in this process; they
//! are not evidence of how a real Windows host application grants locks.

use raumakey_model::identity::RAUMAKEY_INJECTED_TAG;
use windows::Win32::Foundation::LPARAM;
use windows::Win32::UI::WindowsAndMessaging::SetMessageExtraInfo;
use windows::core::{ComObject, IUnknown};

use crate::store::AppStore;
use crate::support::{on_sta, serial};
use crate::tsf::{Document, Tip, hold_ctrl, key_message, press, release, type_text};

/// Sets the extra information of the message this thread processes, as `SendInput` and the
/// retrieval of a message do.
fn set_extra_info(extra_info: u64) {
    // SAFETY: sets the extra information of this thread's current message only.
    unsafe { SetMessageExtraInfo(LPARAM(extra_info as isize)) };
}

/// `IS_DEFAULT`: a field without a special input scope.
const IS_DEFAULT: i32 = 0;

/// A plain field of an application, focused.
fn plain_field(tip: &Tip) -> (ComObject<AppStore>, Document) {
    let store = AppStore::new(&[IS_DEFAULT]);
    let document = Document::with_store(tip, &store.to_interface::<IUnknown>());
    tip.focus(Some(&document));
    (store, document)
}

#[test]
fn matrix_41_a_refused_edit_session_lets_the_key_through_and_ends_the_word() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let (store, document) = plain_field(&tip);
        type_text(&tip, &document, "a");
        // The host cannot grant the synchronous read-write lock of the next edit: TSF answers
        // the edit session with TF_E_SYNCHRONOUS.
        store.refuse_next_sync_write();
        let letter_a = u32::from(b'A');
        let (claimed, eaten) = press(&tip, Some(&document.context), letter_a);
        assert!(claimed, "the key would edit the word");
        assert!(!eaten, "the key reaches the application");
        assert_eq!(
            store.refused(),
            1,
            "the edit session of the key was refused"
        );
        document.app_types("a");
        assert!(!release(&tip, Some(&document.context), letter_a));
        assert_eq!(
            document.text(),
            "aa",
            "nothing was edited, and no key was lost"
        );
        // The word ended: the tone key starts a new word instead of reaching back.
        type_text(&tip, &document, "s");
        assert_eq!(document.text(), "aas");
        drop((document, store));
        tip.end();
    });
}

#[test]
fn matrix_40_the_edit_leaves_a_caret_after_the_new_text_where_the_host_selects_it() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let (store, document) = plain_field(&tip);
        // The host leaves the text of every replacement selected: only the text service's own
        // selection puts the caret after its edits.
        store.select_inserted_text();
        type_text(&tip, &document, "tieengs");
        assert_eq!(document.text(), "tiếng");
        assert_eq!(store.selection(), (5, 5), "a caret after the word");
        type_text(&tip, &document, " x");
        assert_eq!(document.text(), "tiếng x");
        drop((document, store));
        tip.end();
    });
}

#[test]
fn matrix_39_the_injection_tag_of_a_key_is_read_before_tsf_runs_for_it() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        // A field that never had the focus: its first key reads the input scope in a
        // synchronous edit session, inside the key event.
        let store = AppStore::new(&[IS_DEFAULT]);
        let document = Document::with_store(&tip, &store.to_interface::<IUnknown>());
        // Raumakey injected the key; while TSF waits for the lock of the read, the thread
        // retrieves another message, which carries no extra information.
        set_extra_info(RAUMAKEY_INJECTED_TAG);
        store.on_next_lock(|| set_extra_info(0));
        let letter_a = u32::from(b'A');
        assert_eq!(
            press(&tip, Some(&document.context), letter_a),
            (false, false),
            "the injected key is not even claimed"
        );
        document.app_types("a");
        assert!(!release(&tip, Some(&document.context), letter_a));
        set_extra_info(0);
        assert_eq!(document.text(), "a");
        drop((document, store));
        tip.end();
    });
}

#[test]
fn the_modifiers_of_a_key_are_read_before_tsf_runs_for_it() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        // As above, the first key of the field reads its input scope inside the key event.
        let store = AppStore::new(&[IS_DEFAULT]);
        let document = Document::with_store(&tip, &store.to_interface::<IUnknown>());
        // While TSF waits for the lock of the read, the thread retrieves a message after which
        // its key state shows Ctrl down; it is up again when TSF calls `OnKeyDown`.
        store.on_next_lock(|| hold_ctrl(true));
        let keys = tip.keys();
        let letter_a = u32::from(b'A');
        let (wparam, lparam) = key_message(letter_a, false);
        // SAFETY: calls on a live sink of this thread with a live context.
        let claimed = unsafe { keys.OnTestKeyDown(&document.context, wparam, lparam) }
            .expect("OnTestKeyDown")
            .as_bool();
        hold_ctrl(false);
        assert!(claimed, "the key was tested as the letter that was typed");
        // SAFETY: as above.
        let eaten = unsafe { keys.OnKeyDown(&document.context, wparam, lparam) }
            .expect("OnKeyDown")
            .as_bool();
        assert!(
            !eaten,
            "a letter that starts a word reaches the application"
        );
        document.app_types("a");
        assert!(!release(&tip, Some(&document.context), letter_a));
        assert_eq!(document.text(), "a");
        drop((keys, document, store));
        tip.end();
    });
}
