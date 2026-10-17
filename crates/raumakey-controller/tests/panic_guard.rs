//! A panic inside a command is contained: the command fails with exit code 1 and reports where
//! the panic happened, never its message (which may have been formatted from user content).
//!
//! This binary installs the quiet panic hook of `raumakey-windows`, as `main` does, so it holds
//! only these tests: the hook would silence the failure messages of other suites.

mod support;

use raumakey_controller::cli::EXIT_FAILED;
use raumakey_controller::run_guarded;
use raumakey_windows::panic::install_quiet_panic_hook;

use support::capture;

#[test]
fn a_panicking_command_reports_only_where_it_panicked() {
    install_quiet_panic_hook();
    let secret = "xin chào, mật khẩu là 1234";
    let result = capture(|output| {
        run_guarded(output, |output| {
            output.line("started");
            panic!("{secret}")
        })
    });
    assert_eq!(result.code, EXIT_FAILED, "{result:?}");
    assert_eq!(result.out, "started\n");
    assert!(
        result.err.starts_with("raumakey: internal error at "),
        "{}",
        result.err
    );
    assert!(result.err.contains("panic_guard.rs:"), "{}", result.err);
    assert!(!result.err.contains(secret), "{}", result.err);
}

#[test]
fn a_command_that_returns_keeps_its_exit_code_and_output() {
    install_quiet_panic_hook();
    let result = capture(|output| {
        run_guarded(output, |output| {
            output.line("done");
            3
        })
    });
    assert_eq!(result.code, 3);
    assert_eq!(result.out, "done\n");
    assert!(result.err.is_empty(), "{}", result.err);
}
