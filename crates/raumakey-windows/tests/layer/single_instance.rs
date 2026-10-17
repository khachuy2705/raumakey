//! `single_instance`: named mutex with user-only security (unique test names).

use raumakey_model::identity::NAMED_OBJECT_PREFIX;
use raumakey_windows::error::WinErrorKind;
use raumakey_windows::sid::current_user_sid_hash;
use raumakey_windows::single_instance::SingleInstance;

use crate::support::unique_suffix;

#[test]
fn only_one_holder_at_a_time() {
    let component = format!("test-{}", unique_suffix());
    let first = SingleInstance::acquire(&component)
        .unwrap()
        .expect("first holder");
    assert!(first.object_name().starts_with(NAMED_OBJECT_PREFIX));
    assert!(
        first
            .object_name()
            .contains(current_user_sid_hash().unwrap().as_str())
    );
    assert!(first.object_name().ends_with(&component));
    assert!(SingleInstance::acquire(&component).unwrap().is_none());
    drop(first);
    assert!(SingleInstance::acquire(&component).unwrap().is_some());
}

#[test]
fn invalid_components_are_rejected_before_any_call() {
    let error = SingleInstance::acquire("a\\b").unwrap_err();
    assert_eq!(error.kind(), WinErrorKind::InvalidInput);
}
