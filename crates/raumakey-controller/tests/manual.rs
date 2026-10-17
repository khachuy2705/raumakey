//! Manual checks of the real side effects (`system::WinSystem`) through the built
//! `raumakey.exe`. Each test changes the machine or the user's desktop, so each is ignored.
//! Run them one at a time on a test machine, elevated where noted:
//!
//! ```text
//! cargo test -p raumakey-controller --test manual -- --ignored --exact <name>
//! ```
//!
//! A successful build or a passing fake-based suite is no evidence that this glue works on
//! Windows; these tests (and the manual checks in the `install`, `startup`, and `desktop`
//! module docs) are.

#![cfg(windows)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use raumakey_controller::install::PACKAGE_ARCHS;
use raumakey_controller::system::{RegistryRecordStore, SHORTCUT_FILE_NAME};
use raumakey_converter::ClipboardPort;
use raumakey_windows::clipboard::WinClipboard;
use raumakey_windows::known_folder::startup_folder;
use raumakey_windows::shell::read_shortcut;
use raumakey_windows::sid::current_user_sid_hash;
use raumakey_windows::task_scheduler::ElevatedLogonTask;
use raumakey_windows::tsf_registration::{verify_installation, verify_removal};

fn controller() -> &'static Path {
    Path::new(env!("CARGO_BIN_EXE_raumakey"))
}

fn run(exe: &Path, args: &[&str]) -> Output {
    Command::new(exe)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("raumakey.exe must start: {error}"))
}

fn report(output: &Output) -> String {
    format!(
        "exit {:?}\n{}{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Installs and uninstalls the package in `RAUMAKEY_PACKAGE_DIR` (a TIP with its COM layer is
/// needed for the install to succeed).
#[test]
#[ignore = "manual: registers the TIPs machine-wide (HKLM, COM, TSF); run elevated with RAUMAKEY_PACKAGE_DIR set"]
fn manual_install_and_uninstall_a_package() {
    let dir =
        PathBuf::from(std::env::var_os("RAUMAKEY_PACKAGE_DIR").expect("RAUMAKEY_PACKAGE_DIR"));
    let exe = dir.join("raumakey.exe");
    let install = run(&exe, &["install"]);
    assert_eq!(install.status.code(), Some(0), "{}", report(&install));
    let store = RegistryRecordStore::machine();
    let record = store.read().unwrap().expect("the installation record");
    assert_eq!(record.tips.len(), PACKAGE_ARCHS.len());
    let installed = verify_installation(&record.tips);
    assert!(installed.all_passed(), "{:#?}", installed.describe());

    let uninstall = run(&exe, &["uninstall"]);
    assert_eq!(uninstall.status.code(), Some(0), "{}", report(&uninstall));
    let removed = verify_removal(&PACKAGE_ARCHS);
    assert!(removed.all_passed(), "{:#?}", removed.describe());
    assert_eq!(store.read().unwrap(), None);
}

/// Creates the shortcut in the user's Startup folder and removes it again.
#[test]
#[ignore = "manual: creates and removes Raumakey.lnk in the user's Startup folder"]
fn manual_startup_shortcut() {
    let exe = controller();
    let created = run(exe, &["startup", "shortcut"]);
    assert_eq!(created.status.code(), Some(0), "{}", report(&created));
    let lnk = startup_folder().unwrap().join(SHORTCUT_FILE_NAME);
    let spec = read_shortcut(&lnk).unwrap();
    assert!(
        spec.target
            .to_string_lossy()
            .eq_ignore_ascii_case(&exe.to_string_lossy()),
        "{}",
        spec.target.display()
    );
    assert!(spec.arguments.is_empty());

    let removed = run(exe, &["startup", "none"]);
    assert_eq!(removed.status.code(), Some(0), "{}", report(&removed));
    assert!(!lnk.exists());
}

/// Creates the elevated logon task of this account (the explicit choice of matrix #56) and
/// deletes it again. The task needs a `raumakey.exe` inside Program Files, so
/// `RAUMAKEY_PACKAGE_DIR` must name a package copied there; the build's own `raumakey.exe` is
/// refused. The two-account checks are in the manual check list of `startup`.
#[test]
#[ignore = "manual: creates and deletes the elevated logon task; run elevated with RAUMAKEY_PACKAGE_DIR set to a package inside Program Files"]
fn manual_startup_elevated_task() {
    let refused = run(controller(), &["startup", "elevated-task"]);
    assert_eq!(refused.status.code(), Some(1), "{}", report(&refused));

    let dir =
        PathBuf::from(std::env::var_os("RAUMAKEY_PACKAGE_DIR").expect("RAUMAKEY_PACKAGE_DIR"));
    let exe = dir.join("raumakey.exe");
    let created = run(&exe, &["startup", "elevated-task"]);
    assert_eq!(created.status.code(), Some(0), "{}", report(&created));
    let task = ElevatedLogonTask::for_user(&current_user_sid_hash().unwrap());
    assert!(task.exists().unwrap());

    let removed = run(&exe, &["startup", "none"]);
    assert_eq!(removed.status.code(), Some(0), "{}", report(&removed));
    assert!(!task.exists().unwrap());
}

/// Runs the controller with its real notification-area icon until "Thoát" (Exit) is chosen in
/// the icon's menu. Meanwhile, work through the manual check list in the documentation of
/// `raumakey_controller::desktop` (click, menu, a second start, an Explorer restart, Exit);
/// run it again from an elevated prompt for the elevated items.
#[test]
#[ignore = "manual: shows the Raumakey notification-area icon until you choose Thoát (Exit) in its menu"]
fn manual_tray_icon() {
    let status = Command::new(controller())
        .status()
        .unwrap_or_else(|error| panic!("raumakey.exe must start: {error}"));
    assert_eq!(status.code(), Some(0));
}

/// Converts the text on the real clipboard from TCVN3 to Unicode.
#[test]
#[ignore = "manual: replaces the text on the user's clipboard"]
fn manual_clipboard_conversion() {
    let mut clipboard = WinClipboard::new();
    let tcvn3: Vec<u16> = "TiÕng ViÖt".encode_utf16().collect();
    clipboard.write_transient_units(&tcvn3).unwrap();
    let converted = run(
        controller(),
        &[
            "convert",
            "--from",
            "tcvn3",
            "--to",
            "unicode",
            "--clipboard",
        ],
    );
    assert_eq!(converted.status.code(), Some(0), "{}", report(&converted));
    let text = clipboard.read_text().unwrap().expect("clipboard text");
    assert_eq!(String::from_utf16_lossy(&text.units), "Tiếng Việt");
}

/// Runs the real typing hook (plan D39) in this test process for `RAUMAKEY_TYPING_SECONDS`
/// seconds (60 by default), with defaults v1 (Telex, Vietnamese mode on, no macros). Work
/// through the manual check list of `raumakey_controller::typing` meanwhile; the mode
/// changes the observer saw are printed at the end. Do not run `raumakey.exe` or another
/// Vietnamese input method at the same time.
#[test]
#[ignore = "manual: installs a global keyboard and mouse hook and types into the foreground window"]
fn manual_typing_hook() {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use raumakey_controller::typing::TypingHook;
    use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};

    let seconds = std::env::var("RAUMAKEY_TYPING_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);
    let modes = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&modes);
    let hook = TypingHook::start(
        RuntimeSnapshot::defaults_v1(),
        MacroSnapshot::empty(0),
        move |mode| seen.lock().unwrap().push(mode),
    )
    .unwrap();
    std::thread::sleep(Duration::from_secs(seconds));
    hook.stop().unwrap();
    eprintln!("mode changes: {:?}", modes.lock().unwrap());
}
