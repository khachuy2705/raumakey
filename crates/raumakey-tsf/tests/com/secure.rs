//! Secure fields (matrix #29, design §14): the input scope of an application's field, read
//! through TSF (`GUID_PROP_INPUTSCOPE`), decides whether keys there are transformed.

use windows::core::IUnknown;

use crate::store::AppStore;
use crate::support::{on_sta, serial};
use crate::tsf::{Document, Tip, type_text};

/// `IS_DEFAULT`, `IS_PASSWORD`, and `IS_NUMERIC_PIN`.
const IS_DEFAULT: i32 = 0;
const IS_PASSWORD: i32 = 31;
const IS_NUMERIC_PIN: i32 = 64;

#[test]
fn a_field_of_an_application_without_a_password_scope_is_transformed() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let store = AppStore::new(&[IS_DEFAULT]);
        let document = Document::with_store(&tip, &store.to_interface::<IUnknown>());
        tip.focus(Some(&document));
        type_text(&tip, &document, "tieengs vieetj");
        assert_eq!(document.text(), "tiếng việt");
        drop((document, store));
        tip.end();
    });
}

#[test]
fn matrix_29_keys_in_a_password_or_pin_field_reach_it_untouched() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        for scopes in [
            &[IS_PASSWORD][..],
            &[IS_NUMERIC_PIN],
            &[IS_DEFAULT, IS_PASSWORD],
        ] {
            let store = AppStore::new(scopes);
            let document = Document::with_store(&tip, &store.to_interface::<IUnknown>());
            tip.focus(Some(&document));
            type_text(&tip, &document, "aa ddas");
            assert_eq!(document.text(), "aa ddas", "{scopes:?}");
            drop((document, store));
        }
        tip.end();
    });
}

#[test]
fn a_field_that_changes_its_input_scope_is_read_again_at_the_next_focus() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let store = AppStore::new(&[IS_PASSWORD]);
        let document = Document::with_store(&tip, &store.to_interface::<IUnknown>());
        let other = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "aa");
        assert_eq!(document.text(), "aa");
        // The application turns the password field into a plain one (a "show password"
        // button), and the user comes back to it.
        store.set_scopes(&[IS_DEFAULT]);
        tip.focus(Some(&other));
        tip.focus(Some(&document));
        type_text(&tip, &document, " aa");
        assert_eq!(document.text(), "aa â");
        drop((document, other, store));
        tip.end();
    });
}

#[test]
fn a_field_that_became_a_password_field_is_secure_until_its_scope_is_read_again() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let store = AppStore::new(&[IS_DEFAULT]);
        let document = Document::with_store(&tip, &store.to_interface::<IUnknown>());
        let other = Document::new(&tip);
        tip.focus(Some(&document));
        type_text(&tip, &document, "aa");
        assert_eq!(document.text(), "â");
        // The application turns the plain field into a password field, and is busy when the
        // user comes back to it: the read of the input scope at the focus has to wait.
        store.set_scopes(&[IS_PASSWORD]);
        tip.focus(Some(&other));
        store.defer_async_locks();
        tip.focus(Some(&document));
        assert!(
            store.has_deferred(),
            "the read at the focus waits for the host"
        );
        type_text(&tip, &document, " aa");
        assert_eq!(
            document.text(),
            "â aa",
            "keys in the password field were not transformed"
        );
        store.grant_deferred();
        drop((document, other, store));
        tip.end();
    });
}

#[test]
fn a_field_whose_scope_read_waits_is_read_again_at_its_first_key() {
    let _serial = serial();
    on_sta(|| {
        let tip = Tip::activate();
        let store = AppStore::new(&[IS_PASSWORD]);
        let document = Document::with_store(&tip, &store.to_interface::<IUnknown>());
        let other = Document::new(&tip);
        tip.focus(Some(&document));
        // The password field turns into a plain one while the user is elsewhere, and the read
        // at the focus has to wait: the first key reads the scope synchronously.
        store.set_scopes(&[IS_DEFAULT]);
        tip.focus(Some(&other));
        store.defer_async_locks();
        tip.focus(Some(&document));
        assert!(
            store.has_deferred(),
            "the read at the focus waits for the host"
        );
        type_text(&tip, &document, "aa");
        assert_eq!(
            document.text(),
            "â",
            "keys in the plain field were transformed"
        );
        store.grant_deferred();
        drop((document, other, store));
        tip.end();
    });
}
