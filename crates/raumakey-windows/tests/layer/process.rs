//! `process`: command lines and regsvr32 selection (pure), plus read-only Windows lookups.

use raumakey_windows::process::{
    ConsolePlan, Machine, Regsvr32Dir, command_line, console_plan, quote_arg, regsvr32_arguments,
    regsvr32_dir,
};
use raumakey_windows::tsf_registration::TipArch;

#[test]
fn arguments_are_quoted_for_command_line_to_argv() {
    assert_eq!(quote_arg("abc"), "abc");
    assert_eq!(quote_arg("/s"), "/s");
    assert_eq!(quote_arg(""), "\"\"");
    assert_eq!(quote_arg("a b"), "\"a b\"");
    assert_eq!(quote_arg("a\tb"), "\"a\tb\"");
    assert_eq!(quote_arg("a\"b"), "\"a\\\"b\"");
    assert_eq!(
        quote_arg("C:\\Program Files\\x\\"),
        "\"C:\\Program Files\\x\\\\\""
    );
    assert_eq!(quote_arg("a\\\\\"b"), "\"a\\\\\\\\\\\"b\"");
    assert_eq!(quote_arg("C:\\path\\no-spaces"), "C:\\path\\no-spaces");
    assert_eq!(quote_arg("Tiếng Việt"), "\"Tiếng Việt\"");
}

#[test]
fn command_lines_quote_the_program_and_each_argument() {
    assert_eq!(
        command_line(
            "C:\\Windows\\System32\\regsvr32.exe",
            &["/s", "C:\\Program Files\\Raumakey\\raumakey_tip.dll"]
        ),
        "\"C:\\Windows\\System32\\regsvr32.exe\" /s \"C:\\Program Files\\Raumakey\\raumakey_tip.dll\""
    );
    assert_eq!(command_line("a.exe", &[]), "\"a.exe\"");
}

#[test]
fn regsvr32_arguments_are_silent_and_optionally_unregister() {
    assert_eq!(regsvr32_arguments("C:\\x.dll", false), ["/s", "C:\\x.dll"]);
    assert_eq!(
        regsvr32_arguments("C:\\x.dll", true),
        ["/s", "/u", "C:\\x.dll"]
    );
}

#[test]
fn regsvr32_directory_follows_tip_bitness() {
    use Machine::{Arm64, X64, X86};
    use Regsvr32Dir::{SysWow64, Sysnative, System32};
    let cases = [
        // native, process, tip, expected
        (X64, X64, TipArch::X64, Some(System32)),
        (X64, X64, TipArch::X86, Some(SysWow64)),
        (X64, X64, TipArch::Arm64, None),
        (X64, X86, TipArch::X64, Some(Sysnative)),
        (X64, X86, TipArch::X86, Some(SysWow64)),
        (X86, X86, TipArch::X86, Some(System32)),
        (X86, X86, TipArch::X64, None),
        (X86, X86, TipArch::Arm64, None),
        (Arm64, Arm64, TipArch::Arm64, Some(System32)),
        (Arm64, Arm64, TipArch::X86, Some(SysWow64)),
        (Arm64, Arm64, TipArch::X64, None),
        (Arm64, X86, TipArch::Arm64, Some(Sysnative)),
        (Arm64, X64, TipArch::Arm64, Some(System32)),
    ];
    for (native, process, tip, expected) in cases {
        assert_eq!(
            regsvr32_dir(native, process, tip).ok(),
            expected,
            "{native:?} {process:?} {tip:?}"
        );
    }
}

#[test]
fn machines_decode_from_image_file_machine_values() {
    assert_eq!(Machine::from_image_file_machine(0x8664), Some(Machine::X64));
    assert_eq!(Machine::from_image_file_machine(0x014C), Some(Machine::X86));
    assert_eq!(
        Machine::from_image_file_machine(0xAA64),
        Some(Machine::Arm64)
    );
    assert_eq!(Machine::from_image_file_machine(0x01C4), None);
    assert_eq!(Machine::from_image_file_machine(0), None);
}

#[test]
fn console_plan_only_replaces_missing_handles() {
    assert_eq!(
        console_plan(true, true),
        ConsolePlan {
            attach: false,
            replace_stdout: false,
            replace_stderr: false
        }
    );
    assert_eq!(
        console_plan(false, false),
        ConsolePlan {
            attach: true,
            replace_stdout: true,
            replace_stderr: true
        }
    );
    assert_eq!(
        console_plan(true, false),
        ConsolePlan {
            attach: true,
            replace_stdout: false,
            replace_stderr: true
        }
    );
}

#[cfg(windows)]
mod win {
    use std::path::Path;

    use raumakey_windows::error::WinErrorKind;
    use raumakey_windows::process::{
        Machine, attach_parent_console, current_machines, regsvr32_path,
    };
    use raumakey_windows::tsf_registration::TipArch;

    fn ends_with(path: &Path, suffix: &str) -> bool {
        path.to_string_lossy()
            .to_ascii_lowercase()
            .ends_with(&suffix.to_ascii_lowercase())
    }

    #[test]
    fn the_test_process_runs_natively() {
        let (native, process) = current_machines().unwrap();
        let expected = if cfg!(target_arch = "x86_64") {
            Machine::X64
        } else if cfg!(target_arch = "aarch64") {
            Machine::Arm64
        } else {
            Machine::X86
        };
        assert_eq!(process, expected);
        if process == Machine::X64 {
            assert!(matches!(native, Machine::X64 | Machine::Arm64));
        }
    }

    #[test]
    fn regsvr32_paths_exist_for_supported_architectures() {
        let (native, _) = current_machines().unwrap();
        if native != Machine::X64 || !cfg!(target_arch = "x86_64") {
            return;
        }
        let x64 = regsvr32_path(TipArch::X64).unwrap();
        assert!(
            ends_with(&x64, "System32\\regsvr32.exe"),
            "{}",
            x64.display()
        );
        assert!(x64.is_file());
        let x86 = regsvr32_path(TipArch::X86).unwrap();
        assert!(
            ends_with(&x86, "SysWOW64\\regsvr32.exe"),
            "{}",
            x86.display()
        );
        assert!(x86.is_file());
        assert_eq!(
            regsvr32_path(TipArch::Arm64).unwrap_err().kind(),
            WinErrorKind::NotSupported
        );
    }

    #[test]
    fn a_process_with_standard_handles_does_not_attach_a_console() {
        // Test processes always have stdout/stderr (a console or cargo's pipes).
        assert!(!attach_parent_console().unwrap());
    }

    /// Registers a DLL with regsvr32 (writes HKLM). Manual test only.
    #[test]
    #[ignore = "manual: runs regsvr32, which registers COM classes machine-wide"]
    fn manual_run_regsvr32() {
        let dll = std::env::var_os("RAUMAKEY_TIP_DLL").expect("set RAUMAKEY_TIP_DLL");
        let code = raumakey_windows::process::run_regsvr32(
            Path::new(&dll),
            TipArch::X64,
            false,
            std::time::Duration::from_secs(30),
        )
        .unwrap();
        assert_eq!(code, 0);
    }

    /// Shows a UAC prompt. Manual test only.
    #[test]
    #[ignore = "manual: shows a UAC prompt and starts an elevated process"]
    fn manual_relaunch_elevated() {
        let exe = std::env::current_exe().unwrap();
        let _ = raumakey_windows::process::relaunch_elevated(
            &exe,
            &["--list"],
            std::time::Duration::from_secs(60),
        );
    }
}
