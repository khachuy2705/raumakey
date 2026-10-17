//! The built `raumakey.exe` (`CARGO_BIN_EXE_raumakey`): its PE image and its command line as a
//! process (exit codes, standard output, and standard error).
//!
//! These tests run only commands that change nothing on the machine: `--version`, `help`,
//! `update`, `migrate` (not available), dry runs of `install` and `uninstall`, usage errors,
//! file conversion inside a temporary directory, and a second start of the controller while
//! the test holds the single-instance mutex: the single instance is checked first, so that
//! start shows no tray icon, installs no keyboard hook, and writes no `raumakey.toml`.

mod support;

use std::fs;
#[cfg(windows)]
use std::io::Read;
use std::path::Path;
#[cfg(windows)]
use std::process::Stdio;
use std::process::{Command, Output};
#[cfg(windows)]
use std::thread;
#[cfg(windows)]
use std::time::{Duration, Instant};

use raumakey_model::identity::{Guid, TIP_CLSID, TIP_PROFILE_GUID};
#[cfg(windows)]
use raumakey_windows::error::WinErrorKind;
#[cfg(windows)]
use raumakey_windows::names::CONTROLLER_INSTANCE;
#[cfg(windows)]
use raumakey_windows::single_instance::SingleInstance;
use raumakey_windows::tsf_registration::TIP_CATEGORIES;

use support::guids_in;

fn controller_exe() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_raumakey"))
}

fn run_exe(args: &[&str]) -> Output {
    Command::new(controller_exe())
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("the controller must start: {error}"))
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
fn version_prints_the_product_name_and_version() {
    let output = run_exe(&["--version"]);
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    assert_eq!(
        text(&output.stdout).trim(),
        format!("Raumakey {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(output.stderr.is_empty(), "{}", text(&output.stderr));
}

#[test]
fn help_and_update_exit_with_0() {
    let help = run_exe(&["help"]);
    assert_eq!(help.status.code(), Some(0));
    assert!(text(&help.stdout).contains("install"));
    let update = run_exe(&["update"]);
    assert_eq!(update.status.code(), Some(0));
    assert!(text(&update.stdout).contains("not available"));
}

#[test]
fn migrate_is_not_available_yet() {
    let output = run_exe(&["migrate", "--dry-run"]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(
        text(&output.stderr).contains("not available"),
        "{}",
        text(&output.stderr)
    );
}

/// Holds the controller's single-instance mutex of this user and session. Another test
/// process (the run of another target) may hold it for a moment, so this waits up to 30 s.
#[cfg(windows)]
fn hold_the_controller_instance() -> SingleInstance {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let busy = match SingleInstance::acquire(CONTROLLER_INSTANCE) {
            Ok(Some(instance)) => return instance,
            Ok(None) => "held by another process".to_owned(),
            Err(error) if error.kind() == WinErrorKind::AccessDenied => error.to_string(),
            Err(error) => panic!("the single-instance mutex cannot be created: {error}"),
        };
        assert!(
            Instant::now() < deadline,
            "a Raumakey controller is running ({busy}); close it to run this test"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

/// Without arguments `raumakey.exe` runs the controller. A second start (the Startup
/// shortcut while the controller runs, a double click) finds the single-instance mutex held,
/// ends at once with 0, and prints nothing. This test holds the mutex itself, so the process
/// it starts can never become the first instance and show a tray icon.
#[cfg(windows)]
#[test]
fn a_second_start_exits_at_once_with_0_and_says_nothing() {
    let _first = hold_the_controller_instance();
    let mut child = Command::new(controller_exe())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("the controller must start: {error}"));
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the second instance did not end within 30 s");
        }
        thread::sleep(Duration::from_millis(20));
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_end(&mut stderr)
        .unwrap();
    assert_eq!(status.code(), Some(0), "{}", text(&stderr));
    assert!(stdout.is_empty(), "{}", text(&stdout));
    assert!(stderr.is_empty(), "{}", text(&stderr));
}

#[test]
fn usage_errors_exit_with_2_and_print_the_usage() {
    for args in [
        &["bogus"][..],
        &["install", "--dry-run", "--dry-run"],
        &["install", "--tip-x64"],
        &["install", "--bogus"],
        &["startup", "always"],
        &[
            "convert",
            "--from",
            "klingon",
            "--to",
            "unicode",
            "--clipboard",
        ],
        &["update", "now"],
    ] {
        let output = run_exe(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(output.stdout.is_empty(), "{args:?}");
        let stderr = text(&output.stderr);
        assert!(stderr.starts_with("raumakey: "), "{args:?}: {stderr}");
        assert!(stderr.contains("Usage:"), "{args:?}: {stderr}");
    }
}

/// The contract of the TSF dry runs (the TIP stays in the repository but is not shipped, and
/// the release gate checks the portable package instead, D38): exit code 0, both Raumakey
/// identities, and no other GUID than the Windows SDK categories of plan §10.4.
fn assert_dry_run_contract(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", text(&output.stderr));
    let all = format!("{}{}", text(&output.stdout), text(&output.stderr));
    for guid in [TIP_CLSID, TIP_PROFILE_GUID] {
        assert!(all.contains(&guid.to_string()), "{guid} missing:\n{all}");
    }
    let mut allowed: Vec<Guid> = vec![TIP_CLSID, TIP_PROFILE_GUID];
    allowed.extend(TIP_CATEGORIES.iter().map(|category| category.guid));
    for guid in guids_in(&all) {
        assert!(allowed.contains(&guid), "unexpected {guid}:\n{all}");
    }
}

/// `--dry-run` may come before or after the TIP paths; both orders print the same plan.
#[test]
fn the_install_dry_run_accepts_its_options_in_any_order() {
    let x64 = "C:\\Raumakey Package\\x64\\raumakey_tip.dll";
    let x86 = "C:\\Raumakey Package\\x86\\raumakey_tip.dll";
    let first = run_exe(&["install", "--dry-run", "--tip-x64", x64, "--tip-x86", x86]);
    let last = run_exe(&["install", "--tip-x64", x64, "--tip-x86", x86, "--dry-run"]);
    assert_dry_run_contract(&first);
    assert_dry_run_contract(&last);
    assert_eq!(text(&first.stdout), text(&last.stdout));
    let plan = text(&first.stdout);
    assert!(plan.contains(x64) && plan.contains(x86), "{plan}");
}

#[test]
fn the_uninstall_dry_run_meets_the_dry_run_contract() {
    let output = run_exe(&["uninstall", "--dry-run"]);
    assert_dry_run_contract(&output);
}

#[test]
fn convert_writes_a_new_file_and_refuses_to_overwrite_it() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("vni.txt");
    // "Tiếng Việt" in TCVN3 bytes.
    fs::write(&input, b"Ti\xD5ng Vi\xD6t").unwrap();
    let output_path = dir.path().join("unicode.txt");
    let args = |overwrite: bool| {
        let mut args = vec![
            "convert".to_owned(),
            "--from".to_owned(),
            "tcvn3".to_owned(),
            "--to".to_owned(),
            "unicode".to_owned(),
            "--in".to_owned(),
            input.display().to_string(),
            "--out".to_owned(),
            output_path.display().to_string(),
        ];
        if overwrite {
            args.push("--overwrite".to_owned());
        }
        args
    };
    let first = Command::new(controller_exe())
        .args(args(false))
        .output()
        .unwrap();
    assert_eq!(first.status.code(), Some(0), "{}", text(&first.stderr));
    assert_eq!(fs::read_to_string(&output_path).unwrap(), "Tiếng Việt");

    let again = Command::new(controller_exe())
        .args(args(false))
        .output()
        .unwrap();
    assert_eq!(again.status.code(), Some(1));
    assert!(text(&again.stderr).contains("already exists"));

    let replaced = Command::new(controller_exe())
        .args(args(true))
        .output()
        .unwrap();
    assert_eq!(
        replaced.status.code(),
        Some(0),
        "{}",
        text(&replaced.stderr)
    );
}

/// `text` as UTF-16LE bytes, the encoding of version-resource strings.
fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let pair = bytes.get(offset..offset + 2)?;
    Some(u16::from_le_bytes([pair[0], pair[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Option<u32> {
    let quad = bytes.get(offset..offset + 4)?;
    Some(u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]))
}

/// The `Subsystem` field of a PE image: `e_lfanew` at 0x3C points at the `PE\0\0` signature,
/// which is followed by the 20-byte COFF header and the optional header, whose `Subsystem` is at
/// offset 68 in both PE32 and PE32+.
fn pe_subsystem(image: &[u8]) -> Option<u16> {
    let pe = u32_at(image, 0x3C)? as usize;
    if image.get(pe..pe + 4)? != b"PE\0\0" {
        return None;
    }
    u16_at(image, pe + 4 + 20 + 68)
}

/// `IMAGE_SUBSYSTEM_WINDOWS_GUI`.
const SUBSYSTEM_WINDOWS_GUI: u16 = 2;

/// `build.rs` links the controller's `.res` (manifest, version information, icons) into the
/// `raumakey` binary only (Windows MSVC). This checks the bytes of the files;
/// `(Get-Item raumakey.exe).VersionInfo` and `cargo xtask inspect` are the Windows views.
#[cfg(all(windows, target_env = "msvc"))]
#[test]
fn only_the_controller_binary_carries_its_windows_resources() {
    let controller = fs::read(controller_exe()).unwrap_or_default();
    assert!(!controller.is_empty(), "the controller binary must exist");
    // VS_VERSIONINFO strings: FileDescription and OriginalFilename.
    let description = utf16le("Raumakey Vietnamese input method");
    let original_filename = utf16le("raumakey.exe");
    // The manifest's assembly identity, built at run time so that this test binary does not
    // hold it as a literal.
    let identity: String = "rellortnoC.yekamuaR".chars().rev().collect();
    assert!(contains(&controller, &description), "no FileDescription");
    assert!(
        contains(&controller, &original_filename),
        "no OriginalFilename"
    );
    assert!(contains(&controller, identity.as_bytes()), "no manifest");
    let this_test = std::env::current_exe()
        .and_then(fs::read)
        .unwrap_or_default();
    assert!(!this_test.is_empty());
    assert!(
        !contains(&this_test, identity.as_bytes()),
        "a test binary has the controller's manifest"
    );
}

/// The controller is a Windows GUI application (plan §10.2); its subcommands attach to the
/// console of the caller instead.
#[cfg(windows)]
#[test]
fn the_controller_is_a_windows_gui_application() {
    let controller = fs::read(controller_exe()).unwrap_or_default();
    assert_eq!(pe_subsystem(&controller), Some(SUBSYSTEM_WINDOWS_GUI));
    let this_test = std::env::current_exe()
        .and_then(fs::read)
        .unwrap_or_default();
    assert_ne!(
        pe_subsystem(&this_test),
        Some(SUBSYSTEM_WINDOWS_GUI),
        "the subsystem check must tell console and GUI images apart"
    );
}
