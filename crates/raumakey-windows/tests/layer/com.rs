//! `com`: apartment guards (run on fresh threads so the test harness threads stay clean).

use std::thread;

use raumakey_windows::com::{ApartmentKind, ComApartment};

#[test]
fn sta_initializes_and_uninitializes_on_its_thread() {
    thread::spawn(|| {
        let apartment = ComApartment::sta().unwrap();
        assert_eq!(apartment.kind(), ApartmentKind::SingleThreaded);
        assert!(apartment.owns_initialization());
        // Nested initialization in the same mode is counted and balanced too.
        let nested = ComApartment::sta().unwrap();
        assert!(nested.owns_initialization());
        drop(nested);
        drop(apartment);
    })
    .join()
    .unwrap();
}

#[test]
fn a_conflicting_mode_keeps_the_existing_apartment() {
    thread::spawn(|| {
        let sta = ComApartment::sta().unwrap();
        let mta = ComApartment::mta().unwrap();
        assert!(
            !mta.owns_initialization(),
            "RPC_E_CHANGED_MODE must not uninitialize"
        );
        assert_eq!(mta.kind(), ApartmentKind::MultiThreaded);
        drop(mta);
        drop(sta);
    })
    .join()
    .unwrap();
    thread::spawn(|| {
        let mta = ComApartment::mta().unwrap();
        assert!(mta.owns_initialization());
        let sta = ComApartment::sta().unwrap();
        assert!(!sta.owns_initialization());
    })
    .join()
    .unwrap();
}
