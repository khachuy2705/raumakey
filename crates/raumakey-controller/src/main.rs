//! Raumakey controller executable (`raumakey.exe`, plan §10.2).
//!
//! A Windows GUI application: started without arguments it runs the controller; its
//! subcommands attach to the console of the caller (see `raumakey_controller::run_process`).

#![cfg_attr(windows, windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    // First thing: a panic message may have been formatted from user content, so the quiet hook
    // records only where a panic happened and prints nothing.
    raumakey_windows::panic::install_quiet_panic_hook();
    ExitCode::from(raumakey_controller::run_process(
        std::env::args_os().skip(1),
    ))
}

/// `build.rs` links the controller's resources into this binary and into this unit-test
/// harness of it, so the tray's icons are tested here: the integration tests are other
/// executables, which carry no icons.
#[cfg(all(test, windows, target_env = "msvc"))]
mod tests {
    use raumakey_controller::desktop::ModeIcons;
    use raumakey_controller::tray::InputMode;
    use raumakey_windows::icon::{IconKind, IconSize, icon_size};
    use raumakey_windows::module::Module;

    /// Both mode icons load. Which resource each mode uses is checked by `tests/tray.rs`
    /// (`InputMode::icon_id`), and that the V and E images differ by the tests of
    /// `raumakey-resources`; two loaded handles always differ, so they prove nothing here.
    #[test]
    fn the_v_and_e_icons_load_from_this_executable_at_the_small_icon_size() {
        let module = Module::executable().unwrap();
        let small = icon_size(IconKind::Small).unwrap();
        let icons = ModeIcons::load(module, small).unwrap();
        for mode in [InputMode::Vietnamese, InputMode::English] {
            assert!(!icons.icon(mode).handle().is_invalid(), "{mode:?}");
        }
        // Every size the shell may ask for at another DPI loads as well.
        for size in [16, 20, 24, 32, 48] {
            let size = IconSize::square(size).unwrap();
            ModeIcons::load(module, size).unwrap();
        }
    }
}
