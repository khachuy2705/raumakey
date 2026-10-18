//! D47: in an application of `[exclusions]` the hook types English, whatever the mode: nothing
//! is transformed and no macro expands, while the mode hotkey still switches the mode. The
//! snapshots come from `Settings` opened on a hand-written `raumakey.toml`, so the chain from
//! the file through the runtime snapshot to the hook is covered.

use std::fs;
use std::path::Path;

use raumakey_controller::settings::{DocumentStatus, Settings};
use raumakey_controller::typing::{Foreground, TypingUpdate};
use raumakey_windows::hooks::HookDecision;

use crate::support::{FakePlatform, NOTEPAD, Sim, notepad, window};

/// The window of a Remote Desktop client.
const REMOTE: isize = 0x0004_0040;

/// `raumakey-macros.toml` with the macro `ko` -> `không`.
const MACROS: &str =
    "version = 1\nrevision = 1\n[[macro]]\nid = 1\ntrigger = \"ko\"\nreplacement = \"không\"\n";

/// Settings opened on `dir`, whose `raumakey.toml` holds `config`, with the macros of
/// [`MACROS`].
fn open(dir: &Path, config: &str) -> Settings {
    fs::write(dir.join("raumakey.toml"), config).expect("written");
    fs::write(dir.join("raumakey-macros.toml"), MACROS).expect("written");
    let (settings, report) = Settings::open(dir, None, &|_| true, &|_| false);
    assert_eq!(
        report.status.config,
        DocumentStatus::Loaded { issues: Vec::new() }
    );
    assert_eq!(settings.macro_snapshot().entries.len(), 1);
    settings
}

/// A handler with `foreground` in front and the snapshots of `settings` posted, as the tray
/// hands them to the hook.
fn hook_with(settings: &Settings, foreground: Foreground) -> Sim {
    let sim = Sim::new(FakePlatform {
        foreground: Some(foreground),
        ..FakePlatform::default()
    });
    sim.mailbox
        .post(TypingUpdate::Runtime(settings.runtime_snapshot().clone()))
        .expect("the session core takes the runtime snapshot");
    sim.mailbox
        .post(TypingUpdate::Macros(
            settings.macro_snapshot().as_ref().clone(),
        ))
        .expect("the session core takes the macros");
    sim
}

/// Types `text` and checks that every key passed untouched and nothing was sent.
fn types_english(sim: &mut Sim, text: &str) {
    let decisions = sim.type_text(text);
    assert!(
        decisions
            .iter()
            .all(|decision| *decision == HookDecision::PassThrough),
        "{decisions:?}"
    );
    assert!(sim.platform().sends.is_empty(), "nothing is sent");
}

/// With the default list in effect and Vietnamese mode on, Remote Desktop Connection gets the
/// keys as typed, whatever the case of its executable's name, while Notepad gets Vietnamese.
#[test]
fn remote_desktop_types_english_by_default_while_notepad_types_vietnamese() {
    let dir = tempfile::tempdir().expect("a folder");
    let settings = open(dir.path(), "version = 1\nrevision = 1\n");
    assert!(settings.vietnamese_mode());
    for exe in ["mstsc.exe", "MSTSC.EXE"] {
        let mut sim = hook_with(&settings, window(REMOTE, exe));
        types_english(&mut sim, "tieengs vieejt aa");
        assert_eq!(sim.text(REMOTE), "tieengs vieejt aa", "{exe}");
    }
    let mut sim = hook_with(&settings, notepad());
    sim.type_text("tieengs vieejt aa");
    assert_eq!(sim.text(NOTEPAD), "tiếng việt â");
}

/// No macro expands in an excluded application, not even with `[macros] when_disabled`, in
/// either mode; Notepad expands it in both.
#[test]
fn macros_do_not_expand_in_an_excluded_application_in_either_mode() {
    let dir = tempfile::tempdir().expect("a folder");
    let settings = open(
        dir.path(),
        "version = 1\nrevision = 1\n[macros]\nenabled = true\nwhen_disabled = true\n",
    );
    let mut sim = hook_with(&settings, window(REMOTE, "msrdc.exe"));
    types_english(&mut sim, "ko ");
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [false], "English mode");
    types_english(&mut sim, "ko ");
    assert_eq!(sim.text(REMOTE), "ko ko ");

    sim.focus(Some(notepad()));
    sim.type_text("ko ");
    assert_eq!(
        sim.text(NOTEPAD),
        "không ",
        "English mode expands with when_disabled"
    );
    sim.tap_ctrl_shift();
    sim.type_text("ko ");
    assert_eq!(sim.text(NOTEPAD), "không không ");
}

/// The mode hotkey switches the global mode from inside an excluded application.
#[test]
fn the_mode_hotkey_still_switches_the_mode_in_an_excluded_application() {
    let dir = tempfile::tempdir().expect("a folder");
    let settings = open(dir.path(), "version = 1\nrevision = 1\n");
    let mut sim = hook_with(&settings, window(REMOTE, "mstsc.exe"));
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [false]);
    sim.focus(Some(notepad()));
    sim.type_text("aa ");
    sim.focus(Some(window(REMOTE, "mstsc.exe")));
    sim.tap_ctrl_shift();
    assert_eq!(sim.modes(), [false, true]);
    sim.focus(Some(notepad()));
    sim.type_text("aa");
    assert_eq!(sim.text(NOTEPAD), "aa â");
}

/// Once the user empties `apps` by hand, the poll brings a snapshot without exclusions, and
/// Remote Desktop Connection gets Vietnamese again.
#[test]
fn an_emptied_list_lets_remote_desktop_type_vietnamese_again() {
    let dir = tempfile::tempdir().expect("a folder");
    let mut settings = open(dir.path(), "version = 1\nrevision = 1\n");
    let mut sim = hook_with(&settings, window(REMOTE, "mstsc.exe"));
    types_english(&mut sim, "aa ");
    fs::write(
        dir.path().join("raumakey.toml"),
        "version = 1\nrevision = 1\n[exclusions]\napps = []\n",
    )
    .expect("written");
    let runtime = settings
        .poll()
        .expect("reloaded")
        .runtime
        .expect("new rules");
    sim.mailbox
        .post(TypingUpdate::Runtime(runtime))
        .expect("the session core takes the runtime snapshot");
    sim.type_text("aa");
    assert_eq!(sim.text(REMOTE), "aa â");
}
