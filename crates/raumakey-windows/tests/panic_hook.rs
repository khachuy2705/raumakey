//! `panic`: no unwind crosses an FFI boundary; the hook records only `file:line`.
//!
//! This is a test binary of its own because the quiet hook is process-wide: installed in the
//! main suite, it would also silence the assertion messages of unrelated failing tests.

use raumakey_windows::panic::{
    PanicLocation, guard_value, install_quiet_panic_hook, last_panic_location, panic_count,
    thread_last_panic_location,
};

#[test]
fn guard_value_returns_the_closure_result_or_the_fallback() {
    install_quiet_panic_hook();
    assert_eq!(guard_value(5, || 7), 7);
    let before = panic_count();
    let line = line!() + 1;
    let value = guard_value(5, || -> i32 { panic!("secret user text {}", 42) });
    assert_eq!(value, 5);
    assert!(panic_count() > before);
    // Other tests of this binary panic concurrently, so the exact check uses this thread's record.
    let location: PanicLocation = thread_last_panic_location().expect("a location was recorded");
    assert!(
        location.file.ends_with("panic_hook.rs"),
        "{}",
        location.file
    );
    assert_eq!(location.line, line);
    let display = location.to_string();
    assert!(!display.contains("secret"), "{display}");
    assert!(
        display.ends_with(&format!("panic_hook.rs:{line}")),
        "{display}"
    );
    let global = last_panic_location().expect("a process-wide location was recorded");
    assert!(global.line > 0);
}

#[test]
fn installing_the_hook_twice_is_harmless() {
    install_quiet_panic_hook();
    install_quiet_panic_hook();
    assert!(!guard_value(false, || -> bool { panic!("boom") }));
}

#[cfg(windows)]
mod windows_guards {
    use raumakey_windows::error::{E_INVALIDARG, E_UNEXPECTED};
    use raumakey_windows::panic::{
        guard_bool, guard_hresult, guard_hresult_raw, guard_lresult, install_quiet_panic_hook,
    };
    use windows::Win32::Foundation::LRESULT;
    use windows::core::{Error, HRESULT};

    #[test]
    fn guard_hresult_maps_success_errors_and_panics() {
        install_quiet_panic_hook();
        assert_eq!(guard_hresult(|| Ok(())), HRESULT(0));
        assert_eq!(
            guard_hresult(|| Err(Error::from_hresult(HRESULT(E_INVALIDARG)))),
            HRESULT(E_INVALIDARG)
        );
        assert_eq!(
            guard_hresult(|| -> windows::core::Result<()> { panic!("x") }),
            HRESULT(E_UNEXPECTED)
        );
        // An empty error object never reads as success.
        assert!(guard_hresult(|| Err(Error::empty())).is_err());
    }

    #[test]
    fn guard_hresult_raw_passes_codes_through() {
        install_quiet_panic_hook();
        assert_eq!(guard_hresult_raw(|| HRESULT(1)), HRESULT(1));
        assert_eq!(
            guard_hresult_raw(|| -> HRESULT { panic!("x") }),
            HRESULT(E_UNEXPECTED)
        );
    }

    #[test]
    fn guard_lresult_and_guard_bool_use_fallbacks_on_panic() {
        install_quiet_panic_hook();
        assert_eq!(guard_lresult(LRESULT(3), || LRESULT(9)), LRESULT(9));
        assert_eq!(
            guard_lresult(LRESULT(3), || -> LRESULT { panic!("x") }),
            LRESULT(3)
        );
        assert!(guard_bool(|| true).as_bool());
        assert!(!guard_bool(|| -> bool { panic!("x") }).as_bool());
    }
}
