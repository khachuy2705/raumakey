//! Document identities of TSF contexts.

use raumakey_tip::adapter::documents::{Contexts, MAX_CONTEXTS};
use raumakey_tip::session::MAX_DOCUMENTS;

/// The identity of a context or document manager in these tests.
const fn identity(value: usize) -> usize {
    0x1000 + value * 0x10
}

#[test]
fn a_context_keeps_its_document_id_and_every_context_gets_its_own() {
    let mut contexts = Contexts::default();
    let first = contexts.enter(identity(1), Some(identity(100)));
    assert!(first.new);
    assert_eq!(first.evicted, None);
    let again = contexts.enter(identity(1), Some(identity(100)));
    assert!(!again.new);
    assert_eq!(again.id, first.id);
    let second = contexts.enter(identity(2), Some(identity(100)));
    assert!(second.new);
    assert_ne!(second.id, first.id);
    assert_eq!(contexts.len(), 2);
    assert_eq!(
        contexts.get(identity(1)).map(|info| info.id),
        Some(first.id)
    );
    assert_eq!(
        contexts.get(identity(1)).and_then(|info| info.manager),
        Some(identity(100))
    );
    assert!(contexts.get(identity(3)).is_none());
}

#[test]
fn a_context_that_comes_back_after_its_removal_is_a_new_document() {
    let mut contexts = Contexts::default();
    let first = contexts.enter(identity(1), None);
    assert_eq!(contexts.remove(identity(1)), Some(first.id));
    assert_eq!(contexts.remove(identity(1)), None);
    assert!(contexts.is_empty());
    // A new context may reuse the address of one that is gone; its words are its own.
    let reused = contexts.enter(identity(1), None);
    assert!(reused.new);
    assert_ne!(reused.id, first.id);
}

#[test]
fn removing_a_document_manager_removes_its_contexts() {
    let mut contexts = Contexts::default();
    let a = contexts.enter(identity(1), Some(identity(100))).id;
    let b = contexts.enter(identity(2), Some(identity(100))).id;
    let other = contexts.enter(identity(3), Some(identity(200))).id;
    let mut removed = contexts.remove_manager(identity(100));
    removed.sort();
    let mut expected = vec![a, b];
    expected.sort();
    assert_eq!(removed, expected);
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts.get(identity(3)).map(|info| info.id), Some(other));
    assert!(contexts.remove_manager(identity(100)).is_empty());
}

#[test]
fn the_least_recently_used_context_makes_room_and_is_reported() {
    assert_eq!(
        MAX_CONTEXTS, MAX_DOCUMENTS,
        "one document state per context"
    );
    let mut contexts = Contexts::default();
    let oldest = contexts.enter(identity(0), None).id;
    for n in 1..MAX_CONTEXTS {
        assert_eq!(contexts.enter(identity(n), None).evicted, None);
    }
    // Using the oldest context makes the second one the least recently used.
    contexts.enter(identity(0), None);
    let second = contexts.get(identity(1)).map(|info| info.id);
    let entered = contexts.enter(identity(MAX_CONTEXTS), None);
    assert!(entered.new);
    assert_eq!(entered.evicted, second);
    assert_eq!(contexts.len(), MAX_CONTEXTS);
    assert!(contexts.get(identity(1)).is_none());
    assert_eq!(contexts.get(identity(0)).map(|info| info.id), Some(oldest));
}

#[test]
fn an_input_scope_that_was_not_read_yet_counts_as_a_secure_field() {
    let mut contexts = Contexts::default();
    contexts.enter(identity(1), None);
    let info = contexts.get(identity(1)).expect("entered");
    assert_eq!(info.secure, None);
    assert!(info.is_secure_field(), "matrix #29: fail safe");
    for (secure, expected) in [(Some(false), false), (Some(true), true)] {
        contexts.get_mut(identity(1)).expect("entered").secure = secure;
        assert_eq!(
            contexts.get(identity(1)).map(|info| info.is_secure_field()),
            Some(expected)
        );
    }
}

#[test]
fn the_window_class_is_kept_per_context() {
    let mut contexts = Contexts::default();
    contexts.enter(identity(1), None);
    contexts.enter(identity(2), None);
    contexts.get_mut(identity(1)).expect("entered").window_class = Some("Edit".to_owned());
    assert_eq!(
        contexts
            .get(identity(1))
            .and_then(|info| info.window_class.as_deref()),
        Some("Edit")
    );
    assert_eq!(
        contexts
            .get(identity(2))
            .and_then(|info| info.window_class.as_deref()),
        None
    );
}
