//! The settings worker (`worker`): the tray's commands run on the worker's `Settings` (D40),
//! off the tray's window thread, and come back as numbered answers that carry the settings in
//! effect; the macro snapshots of a reload go to the typing hook from the worker; changes that
//! could not be saved are retried; a half-written hand edit is read once more; a hand edit of
//! `[system] startup` creates or removes the Startup shortcut (D41). Real files in temporary
//! folders; the shortcut, Notepad, and the hook are fakes.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use raumakey_config::{
    ConfigDocument, ConfigError, ExclusionsSection, IssueReason, StartupMode, StoreDocument,
};
use raumakey_controller::exclusions::ExclusionsRequest;
use raumakey_controller::run::ShortcutState;
use raumakey_controller::settings::{Change, DocumentStatus, Recovery, Settings};
use raumakey_controller::worker::{
    Answer, OpenError, Outcome, PollReport, Saved, SettingsState, SettingsWorker, ShortcutSync,
    StartupError, WorkerCommand, WorkerPorts, WorkerThread,
};
use raumakey_ipc::message::MacroSnapshot;
use raumakey_model::host::ExeName;
use raumakey_model::options::InputMethod;
use raumakey_windows::error::{E_ACCESSDENIED, E_FAIL, WinError, WinResult};

/// What the worker asked of its ports.
#[derive(Clone, Debug, PartialEq, Eq)]
enum PortCall {
    /// A macro snapshot for the hook: its revision and entry count (never its text).
    Macros {
        revision: u64,
        entries: usize,
    },
    ReadShortcut,
    CreateShortcut,
    RemoveShortcut,
    Notepad(PathBuf),
}

type Log = Arc<Mutex<Vec<PortCall>>>;

struct FakePorts {
    log: Log,
    shortcut: WinResult<ShortcutState>,
    create: WinResult<()>,
    remove: WinResult<bool>,
    notepad: WinResult<()>,
    /// Notepad panics instead of answering.
    notepad_panics: bool,
}

impl FakePorts {
    fn new(log: &Log) -> Self {
        Self {
            log: Arc::clone(log),
            shortcut: Ok(ShortcutState::Missing),
            create: Ok(()),
            remove: Ok(true),
            notepad: Ok(()),
            notepad_panics: false,
        }
    }

    fn with_shortcut(mut self, shortcut: WinResult<ShortcutState>) -> Self {
        self.shortcut = shortcut;
        self
    }

    fn push(&self, call: PortCall) {
        self.log.lock().unwrap().push(call);
    }
}

impl WorkerPorts for FakePorts {
    fn post_macros(&mut self, macros: &MacroSnapshot) {
        self.push(PortCall::Macros {
            revision: macros.macro_revision,
            entries: macros.entries.len(),
        });
    }

    fn shortcut_state(&mut self) -> WinResult<ShortcutState> {
        self.push(PortCall::ReadShortcut);
        self.shortcut
    }

    fn create_shortcut(&mut self) -> WinResult<()> {
        self.push(PortCall::CreateShortcut);
        self.create
    }

    fn remove_shortcut(&mut self) -> WinResult<bool> {
        self.push(PortCall::RemoveShortcut);
        self.remove
    }

    fn open_in_notepad(&mut self, file: &Path) -> WinResult<()> {
        self.push(PortCall::Notepad(file.to_path_buf()));
        assert!(!self.notepad_panics, "Notepad panics in this test");
        self.notepad
    }
}

/// A folder for `raumakey.exe` that holds the configuration.
struct Folder(tempfile::TempDir);

impl Folder {
    fn new() -> Self {
        Self(tempfile::tempdir().unwrap())
    }

    fn open(&self) -> Settings {
        Settings::open(self.0.path(), None, &|_| true, &|_| false).0
    }

    fn config(&self) -> PathBuf {
        self.0.path().join("raumakey.toml")
    }

    fn macros(&self) -> PathBuf {
        self.0.path().join("raumakey-macros.toml")
    }

    fn stored(&self) -> ConfigDocument {
        ConfigDocument::decode(&fs::read_to_string(self.config()).unwrap())
            .unwrap()
            .document
    }
}

fn worker(folder: &Folder, ports: FakePorts) -> SettingsWorker<FakePorts> {
    SettingsWorker::new(folder.open(), ports)
}

fn state(answer: &Answer) -> &SettingsState {
    answer.state.as_deref().expect("the settings in effect")
}

fn calls(log: &Log) -> Vec<PortCall> {
    log.lock().unwrap().clone()
}

/// The report of a poll answer.
fn polled(answer: &Answer) -> &PollReport {
    match &answer.outcome {
        Outcome::Polled(report) => report,
        other => panic!("{other:?}"),
    }
}

#[test]
fn an_applied_change_is_committed_and_answered_with_the_settings_in_effect() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    let change = Change::InputMethod(InputMethod::Vni);
    let answer = worker.handle(1, WorkerCommand::Apply(change));
    assert_eq!(answer.seq, 1);
    assert_eq!(
        answer.outcome,
        Outcome::Applied {
            change,
            result: Ok(Saved::Written)
        }
    );
    let state = state(&answer);
    assert_eq!(state.runtime, *worker.settings().runtime_snapshot());
    assert_eq!(state.runtime.options.input_method, InputMethod::Vni);
    assert_eq!(state.startup, StartupMode::None);
    assert_eq!(folder.stored().input.method, InputMethod::Vni);
    assert_eq!(calls(&log), []);
}

/// "Mở bảng này khi Raumakey chạy" of the panel (D43): saved like any change, and every answer
/// carries it, so the tray (and the panel) show what the worker has, also after a hand edit.
#[test]
fn every_answer_carries_whether_the_panel_opens_at_start() {
    let folder = Folder::new();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()));
    let answer = worker.handle(1, WorkerCommand::Poll);
    assert!(state(&answer).show_panel_on_start, "the schema default");
    let change = Change::ShowPanelOnStart(false);
    let answer = worker.handle(2, WorkerCommand::Apply(change));
    assert_eq!(
        answer.outcome,
        Outcome::Applied {
            change,
            result: Ok(Saved::Written)
        }
    );
    assert!(!state(&answer).show_panel_on_start);
    assert!(!folder.stored().system.show_panel_on_start);
    // A hand edit turns it on again; the poll's answer says so.
    let mut edited = folder.stored();
    edited.system.show_panel_on_start = true;
    fs::write(folder.config(), edited.encode().unwrap()).unwrap();
    let answer = worker.handle(3, WorkerCommand::Poll);
    assert!(state(&answer).show_panel_on_start);
}

fn exe(name: &str) -> ExeName {
    ExeName::new(name).unwrap()
}

/// The default exclusions followed by `more`.
fn defaults_and(more: &[&str]) -> Vec<ExeName> {
    let mut apps = ExclusionsSection::default().apps;
    apps.extend(more.iter().map(|name| exe(name)));
    apps
}

/// An edit of the exclusions dialog (D48) is committed like a change and answered with whether
/// it wrote anything; the answer carries the list in effect and the runtime snapshot with its
/// rule.
#[test]
fn an_exclusion_edit_is_committed_and_answered_with_the_list_in_effect() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    let first = worker.handle(1, WorkerCommand::Poll);
    assert_eq!(state(&first).exclusions, defaults_and(&[]));

    let add = ExclusionsRequest::Add(exe("notepad.exe"));
    let answer = worker.handle(2, WorkerCommand::Exclusions(add.clone()));
    assert_eq!(
        answer.outcome,
        Outcome::Exclusions {
            request: add.clone(),
            result: Ok(Saved::Written)
        }
    );
    let state = state(&answer);
    assert_eq!(state.exclusions, defaults_and(&["notepad.exe"]));
    assert_eq!(state.runtime, *worker.settings().runtime_snapshot());
    assert!(
        state
            .runtime
            .compatibility
            .rules
            .iter()
            .any(|rule| rule.exe == exe("notepad.exe")),
        "{:?}",
        state.runtime.compatibility
    );
    assert_eq!(
        folder.stored().exclusions.apps,
        defaults_and(&["notepad.exe"])
    );

    // Listed already: nothing written, and the answer says so.
    assert_eq!(
        worker
            .handle(3, WorkerCommand::Exclusions(add.clone()))
            .outcome,
        Outcome::Exclusions {
            request: add,
            result: Ok(Saved::Unchanged)
        }
    );
    let answer = worker.handle(4, WorkerCommand::Exclusions(ExclusionsRequest::Defaults));
    assert_eq!(
        answer.outcome,
        Outcome::Exclusions {
            request: ExclusionsRequest::Defaults,
            result: Ok(Saved::Written)
        }
    );
    assert_eq!(answer.state.unwrap().exclusions, defaults_and(&[]));
    assert_eq!(calls(&log), []);
}

/// An edit that cannot be saved stays in effect, and the answer carries the error; a hand edit
/// of `[exclusions]` reaches the next answer.
#[test]
fn an_exclusion_edit_that_cannot_be_saved_is_answered_with_the_error() {
    let folder = Folder::new();
    fs::write(folder.config(), "[[[ damaged").unwrap();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()));
    let remove = ExclusionsRequest::Remove(exe("mstsc.exe"));
    let answer = worker.handle(1, WorkerCommand::Exclusions(remove.clone()));
    assert_eq!(
        answer.outcome,
        Outcome::Exclusions {
            request: remove,
            result: Err(ConfigError::Corrupt)
        }
    );
    assert_eq!(
        state(&answer).exclusions,
        [exe("msrdc.exe"), exe("rdcman.exe"), exe("vmconnect.exe")]
    );
    assert_eq!(fs::read(folder.config()).unwrap(), b"[[[ damaged");

    let edited = Folder::new();
    let mut other = SettingsWorker::new(edited.open(), FakePorts::new(&Log::default()));
    fs::write(
        edited.config(),
        "version = 1\nrevision = 1\n[exclusions]\napps = [\"putty\"]\n",
    )
    .unwrap();
    let answer = other.handle(1, WorkerCommand::Poll);
    assert_eq!(state(&answer).exclusions, [exe("putty.exe")]);
}

/// A change to the value in effect writes nothing, and says so: the tray does not take it for
/// a save that worked.
#[test]
fn a_change_to_the_value_in_effect_is_answered_as_unchanged() {
    let folder = Folder::new();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()));
    let change = Change::InputMethod(InputMethod::Telex);
    assert_eq!(
        worker.handle(1, WorkerCommand::Apply(change)).outcome,
        Outcome::Applied {
            change,
            result: Ok(Saved::Unchanged)
        }
    );
}

/// A damaged `raumakey.toml` is never overwritten: the change takes effect, and the answer
/// carries the error.
#[test]
fn a_change_that_cannot_be_saved_stays_in_effect_and_is_answered_with_the_error() {
    let folder = Folder::new();
    fs::write(folder.config(), "[[[ damaged").unwrap();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()));
    let change = Change::SpellCheck(false);
    let answer = worker.handle(4, WorkerCommand::Apply(change));
    assert_eq!(
        answer.outcome,
        Outcome::Applied {
            change,
            result: Err(ConfigError::Corrupt)
        }
    );
    assert!(!state(&answer).runtime.options.spell_check);
    assert_eq!(fs::read(folder.config()).unwrap(), b"[[[ damaged");
}

/// Every answer carries the settings in effect, so the answer to whatever command the tray
/// sent last brings everything the worker has (a reload answered before it included): the
/// tray takes settings over only from that answer.
#[test]
fn a_poll_that_finds_nothing_new_still_answers_with_the_settings_in_effect() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    let answer = worker.handle(2, WorkerCommand::Poll);
    assert_eq!(answer.seq, 2);
    assert_eq!(answer.outcome, Outcome::Polled(PollReport::default()));
    assert_eq!(
        state(&answer).runtime,
        *worker.settings().runtime_snapshot()
    );
    assert_eq!(calls(&log), []);
}

/// A hand edit reloaded by a poll, then "Mở file cấu hình": the answer to the later command
/// carries the reloaded settings as well.
#[test]
fn the_answer_to_open_config_carries_a_reload_answered_before_it() {
    let folder = Folder::new();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()));
    fs::write(
        folder.config(),
        "version = 1\nrevision = 1\n[input]\nmethod = \"vni\"\n",
    )
    .unwrap();
    worker.handle(1, WorkerCommand::Poll);
    let answer = worker.handle(2, WorkerCommand::OpenConfig);
    assert_eq!(
        state(&answer).runtime.options.input_method,
        InputMethod::Vni
    );
}

/// A hand edit is loaded again by the poll: the answer carries the new settings and the state
/// of the files, and the new macros go to the hook from the worker thread.
#[test]
fn a_hand_edit_is_reloaded_by_a_poll_and_its_macros_go_to_the_hook() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    fs::write(
        folder.config(),
        "version = 1\nrevision = 1\n[input]\nmethod = \"viqr\"\nenabled = false\n",
    )
    .unwrap();
    fs::write(
        folder.macros(),
        "version = 1\nrevision = 3\n[[macro]]\nid = 1\ntrigger = \"vn\"\nreplacement = \"Việt Nam\"\n",
    )
    .unwrap();
    let answer = worker.handle(5, WorkerCommand::Poll);
    let report = polled(&answer);
    let status = report.status.as_ref().expect("the files changed");
    assert_eq!(status, worker.settings().status());
    assert!(matches!(status.config, DocumentStatus::Loaded { .. }));
    assert_eq!(report.shortcut, None);
    assert_eq!(report.retry, None);
    let state = state(&answer);
    assert_eq!(state.runtime.options.input_method, InputMethod::Viqr);
    assert!(!state.runtime.vietnamese_mode);
    assert_eq!(
        calls(&log),
        [PortCall::Macros {
            revision: 3,
            entries: 1
        }]
    );
    // Nothing new since.
    let answer = worker.handle(6, WorkerCommand::Poll);
    assert_eq!(*polled(&answer), PollReport::default());
}

/// A hand edit that breaks the file keeps the last good settings and is answered with the new
/// state of the files, so the tray can tell the user. It is read once more first, in case the
/// editor was still writing it.
#[test]
fn a_hand_edit_that_breaks_the_file_is_answered_with_its_state() {
    let folder = Folder::new();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()))
        .with_reread_delay(Duration::from_millis(10));
    worker.handle(1, WorkerCommand::Apply(Change::ModernTone(true)));
    fs::write(folder.config(), "[input\nmethod = ").unwrap();
    let answer = worker.handle(2, WorkerCommand::Poll);
    let status = polled(&answer).status.as_ref().unwrap();
    assert_eq!(
        status.config,
        DocumentStatus::Unreadable(ConfigError::Corrupt)
    );
    assert!(state(&answer).runtime.options.modern_tone);
}

/// An editor caught half-way through saving: the worker reads the file once more after a
/// moment, and takes what the editor finished writing, without a warning.
#[test]
fn a_half_written_hand_edit_is_read_again_before_it_is_taken() {
    let folder = Folder::new();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()))
        .with_reread_delay(Duration::from_millis(400));
    fs::write(folder.config(), "version = 1\nrevision = 1\n[inp").unwrap();
    let config = folder.config();
    let editor = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        fs::write(
            config,
            "version = 1\nrevision = 1\n[input]\nmethod = \"viqr\"\n",
        )
        .unwrap();
    });
    let answer = worker.handle(1, WorkerCommand::Poll);
    editor.join().unwrap();
    let status = polled(&answer).status.as_ref().unwrap();
    assert_eq!(status.config, DocumentStatus::Loaded { issues: Vec::new() });
    assert_eq!(
        state(&answer).runtime.options.input_method,
        InputMethod::Viqr
    );
}

/// A file without `version` that stays so after the second read is what the user wrote: it
/// is taken, with `version` reported.
#[test]
fn a_file_without_a_version_is_taken_after_the_second_read() {
    let folder = Folder::new();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()))
        .with_reread_delay(Duration::from_millis(10));
    fs::write(folder.config(), "[input]\nmethod = \"vni\"\n").unwrap();
    let answer = worker.handle(1, WorkerCommand::Poll);
    let status = polled(&answer).status.as_ref().unwrap();
    let DocumentStatus::Loaded { issues } = &status.config else {
        panic!("{status:?}");
    };
    assert!(
        issues
            .iter()
            .any(|issue| issue.path == "version" && issue.reason == IssueReason::Missing),
        "{issues:?}"
    );
    assert_eq!(
        state(&answer).runtime.options.input_method,
        InputMethod::Vni
    );
}

/// A change that could not be saved is retried at the next poll: once the damaged file was
/// fixed, the retry writes it, and the answer says so.
#[test]
fn a_poll_retries_a_change_that_could_not_be_saved() {
    let folder = Folder::new();
    fs::write(folder.config(), "[[[ damaged").unwrap();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()))
        .with_reread_delay(Duration::from_millis(10));
    worker.handle(1, WorkerCommand::Apply(Change::SpellCheck(false)));
    let answer = worker.handle(2, WorkerCommand::Poll);
    assert_eq!(polled(&answer).retry, Some(Err(ConfigError::Corrupt)));
    fs::write(
        folder.config(),
        "version = 1\nrevision = 1\n[input]\nmodern_tone = true\n",
    )
    .unwrap();
    let answer = worker.handle(3, WorkerCommand::Poll);
    assert_eq!(polled(&answer).retry, Some(Ok(())));
    let stored = folder.stored();
    assert!(!stored.input.spell_check);
    assert!(stored.input.modern_tone);
    assert_eq!(
        polled(&worker.handle(4, WorkerCommand::Poll)).retry,
        None,
        "nothing left to retry"
    );
}

/// The worker saves what is still unsaved before it ends, so a change made just before Exit
/// is not lost when the file was blocked for a moment.
#[test]
fn the_worker_thread_saves_unsaved_changes_before_it_ends() {
    let folder = Folder::new();
    fs::write(folder.config(), "[[[ damaged").unwrap();
    let (deliver, answers) = mpsc::channel();
    let thread = WorkerThread::spawn(
        worker(&folder, FakePorts::new(&Log::default())),
        move |answer| {
            deliver.send(answer).unwrap();
        },
    )
    .unwrap();
    thread.send(1, WorkerCommand::Apply(Change::Macros(false)));
    let answer = answers.recv_timeout(Duration::from_secs(20)).unwrap();
    assert!(matches!(
        answer.outcome,
        Outcome::Applied {
            result: Err(ConfigError::Corrupt),
            ..
        }
    ));
    fs::write(folder.config(), "version = 1\nrevision = 1\n").unwrap();
    assert!(thread.finish(Duration::from_secs(20)));
    assert!(!folder.stored().macros.enabled);
}

#[test]
fn turning_startup_on_creates_the_shortcut_then_saves_shortcut() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    let answer = worker.handle(3, WorkerCommand::Startup(true));
    assert_eq!(
        answer.outcome,
        Outcome::Startup {
            on: true,
            result: Ok(Saved::Written)
        }
    );
    assert_eq!(state(&answer).startup, StartupMode::Shortcut);
    assert_eq!(folder.stored().system.startup, StartupMode::Shortcut);
    assert_eq!(calls(&log), [PortCall::CreateShortcut]);
}

/// Explorer deletes the temporary folder of a ZIP later: no shortcut is made to an exe there.
#[test]
fn turning_startup_on_from_the_temporary_folder_is_refused() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log)).with_temporary_exe(true);
    let answer = worker.handle(1, WorkerCommand::Startup(true));
    assert_eq!(
        answer.outcome,
        Outcome::Startup {
            on: true,
            result: Err(StartupError::Temporary)
        }
    );
    assert_eq!(state(&answer).startup, StartupMode::None);
    assert_eq!(calls(&log), []);
}

/// Without the shortcut, nothing starts Raumakey at logon, so `shortcut` is not saved.
#[test]
fn a_shortcut_that_cannot_be_created_leaves_startup_off() {
    let folder = Folder::new();
    let log = Log::default();
    let error = WinError::new(E_ACCESSDENIED, "IPersistFile::Save");
    let mut ports = FakePorts::new(&log);
    ports.create = Err(error);
    let mut worker = worker(&folder, ports);
    let answer = worker.handle(1, WorkerCommand::Startup(true));
    assert_eq!(
        answer.outcome,
        Outcome::Startup {
            on: true,
            result: Err(StartupError::Shortcut(error))
        }
    );
    assert_eq!(state(&answer).startup, StartupMode::None);
    assert_eq!(folder.stored().system.startup, StartupMode::None);
}

#[test]
fn turning_startup_off_removes_the_shortcut_then_saves_none() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    worker.handle(1, WorkerCommand::Startup(true));
    let answer = worker.handle(2, WorkerCommand::Startup(false));
    assert_eq!(
        answer.outcome,
        Outcome::Startup {
            on: false,
            result: Ok(Saved::Written)
        }
    );
    assert_eq!(state(&answer).startup, StartupMode::None);
    assert_eq!(folder.stored().system.startup, StartupMode::None);
    assert_eq!(
        calls(&log),
        [PortCall::CreateShortcut, PortCall::RemoveShortcut]
    );
    // A shortcut that was gone already is no failure.
    let mut ports = FakePorts::new(&log);
    ports.remove = Ok(false);
    let mut worker = SettingsWorker::new(folder.open(), ports);
    let answer = worker.handle(1, WorkerCommand::Startup(false));
    assert_eq!(
        answer.outcome,
        Outcome::Startup {
            on: false,
            result: Ok(Saved::Unchanged)
        }
    );
}

/// The shortcut that is still there still starts Raumakey, so `shortcut` stays saved.
#[test]
fn a_shortcut_that_cannot_be_removed_keeps_startup_on() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    worker.handle(1, WorkerCommand::Startup(true));
    let error = WinError::new(E_FAIL, "remove shortcut");
    let mut ports = FakePorts::new(&log);
    ports.remove = Err(error);
    let mut worker = SettingsWorker::new(folder.open(), ports);
    let answer = worker.handle(2, WorkerCommand::Startup(false));
    assert_eq!(
        answer.outcome,
        Outcome::Startup {
            on: false,
            result: Err(StartupError::Shortcut(error))
        }
    );
    assert_eq!(state(&answer).startup, StartupMode::Shortcut);
    assert_eq!(folder.stored().system.startup, StartupMode::Shortcut);
}

/// The shortcut was made, but the choice could not be saved: the shortcut stays (it starts
/// Raumakey at logon), and the save error is answered.
#[test]
fn a_startup_choice_that_cannot_be_saved_is_answered_with_the_save_error() {
    let folder = Folder::new();
    fs::write(folder.config(), "[[[ damaged").unwrap();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    let answer = worker.handle(1, WorkerCommand::Startup(true));
    assert_eq!(
        answer.outcome,
        Outcome::Startup {
            on: true,
            result: Err(StartupError::Save(ConfigError::Corrupt))
        }
    );
    assert_eq!(state(&answer).startup, StartupMode::Shortcut);
    assert_eq!(calls(&log), [PortCall::CreateShortcut]);
}

/// Writes `startup = "<mode>"` into `raumakey.toml` by hand.
fn edit_startup(folder: &Folder, mode: &str) {
    fs::write(
        folder.config(),
        format!("version = 1\nrevision = 1\n[system]\nstartup = \"{mode}\"\n"),
    )
    .unwrap();
}

/// A hand edit that turns the start with Windows on creates the shortcut when there is none,
/// or when the one there starts an exe that no longer exists (it was moved).
#[test]
fn a_hand_edit_that_turns_startup_on_creates_a_missing_or_dead_shortcut() {
    for shortcut in [ShortcutState::Missing, ShortcutState::PointsToMissing] {
        let folder = Folder::new();
        let log = Log::default();
        let mut worker = worker(&folder, FakePorts::new(&log).with_shortcut(Ok(shortcut)));
        edit_startup(&folder, "shortcut");
        let answer = worker.handle(1, WorkerCommand::Poll);
        assert_eq!(
            polled(&answer).shortcut,
            Some(ShortcutSync::Created),
            "{shortcut:?}"
        );
        assert_eq!(
            calls(&log),
            [PortCall::ReadShortcut, PortCall::CreateShortcut],
            "{shortcut:?}"
        );
        assert_eq!(state(&answer).startup, StartupMode::Shortcut);
    }
}

/// A shortcut that starts this exe, or another copy that still exists, is left as it is.
#[test]
fn a_hand_edit_that_turns_startup_on_never_repoints_another_copy() {
    for shortcut in [ShortcutState::PointsHere, ShortcutState::PointsToOther] {
        let folder = Folder::new();
        let log = Log::default();
        let mut worker = worker(&folder, FakePorts::new(&log).with_shortcut(Ok(shortcut)));
        edit_startup(&folder, "shortcut");
        let answer = worker.handle(1, WorkerCommand::Poll);
        assert_eq!(polled(&answer).shortcut, Some(ShortcutSync::Unchanged));
        assert_eq!(calls(&log), [PortCall::ReadShortcut], "{shortcut:?}");
    }
}

#[test]
fn a_hand_edit_that_turns_startup_on_in_the_temporary_folder_is_refused() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log)).with_temporary_exe(true);
    edit_startup(&folder, "shortcut");
    let answer = worker.handle(1, WorkerCommand::Poll);
    assert_eq!(
        polled(&answer).shortcut,
        Some(ShortcutSync::RefusedTemporary)
    );
    assert_eq!(calls(&log), []);
}

/// A hand edit that turns it off removes only the shortcut of this exe.
#[test]
fn a_hand_edit_that_turns_startup_off_removes_only_this_exes_shortcut() {
    let cases = [
        (ShortcutState::PointsHere, Some(ShortcutSync::Removed), true),
        (
            ShortcutState::PointsToOther,
            Some(ShortcutSync::Unchanged),
            false,
        ),
        (
            ShortcutState::PointsToMissing,
            Some(ShortcutSync::Unchanged),
            false,
        ),
        (ShortcutState::Missing, Some(ShortcutSync::Unchanged), false),
    ];
    for (shortcut, sync, removed) in cases {
        let folder = Folder::new();
        edit_startup(&folder, "shortcut");
        let log = Log::default();
        let mut worker = worker(&folder, FakePorts::new(&log).with_shortcut(Ok(shortcut)));
        edit_startup(&folder, "none");
        let answer = worker.handle(1, WorkerCommand::Poll);
        assert_eq!(polled(&answer).shortcut, sync, "{shortcut:?}");
        assert_eq!(
            calls(&log).contains(&PortCall::RemoveShortcut),
            removed,
            "{shortcut:?}"
        );
    }
}

/// A shortcut that cannot be read or made is reported; the elevated logon task is never
/// followed by a shortcut.
#[test]
fn shortcut_failures_are_reported_and_the_elevated_task_is_left_alone() {
    let folder = Folder::new();
    let log = Log::default();
    let error = WinError::new(E_FAIL, "IPersistFile::Load");
    let mut unreadable = worker(&folder, FakePorts::new(&log).with_shortcut(Err(error)));
    edit_startup(&folder, "shortcut");
    let answer = unreadable.handle(1, WorkerCommand::Poll);
    assert_eq!(
        polled(&answer).shortcut,
        Some(ShortcutSync::Failed { on: true, error })
    );
    let folder = Folder::new();
    let log = Log::default();
    let mut elevated = worker(&folder, FakePorts::new(&log));
    edit_startup(&folder, "elevated-task");
    let answer = elevated.handle(1, WorkerCommand::Poll);
    assert_eq!(polled(&answer).shortcut, None);
    assert_eq!(calls(&log), []);
}

#[test]
fn open_config_starts_notepad_with_the_configuration_file() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    let answer = worker.handle(1, WorkerCommand::OpenConfig);
    assert_eq!(answer.outcome, Outcome::Opened(Ok(())));
    assert!(answer.state.is_some());
    assert_eq!(calls(&log), [PortCall::Notepad(folder.config())]);
}

#[test]
fn open_config_without_a_file_says_so_and_starts_nothing() {
    let folder = Folder::new();
    let log = Log::default();
    let mut worker = worker(&folder, FakePorts::new(&log));
    fs::remove_file(folder.config()).unwrap();
    assert_eq!(
        worker.handle(1, WorkerCommand::OpenConfig).outcome,
        Outcome::Opened(Err(OpenError::NoFile))
    );
    // No folder at all (`Location::Unavailable`): no file either.
    let (settings, _) = Settings::open(Path::new(""), None, &|_| true, &|_| false);
    let mut worker = SettingsWorker::new(settings, FakePorts::new(&log));
    assert_eq!(
        worker.handle(2, WorkerCommand::OpenConfig).outcome,
        Outcome::Opened(Err(OpenError::NoFile))
    );
    assert_eq!(calls(&log), []);
}

#[test]
fn a_notepad_that_cannot_start_is_answered() {
    let folder = Folder::new();
    let log = Log::default();
    let error = WinError::new(E_FAIL, "CreateProcessW(notepad)");
    let mut ports = FakePorts::new(&log);
    ports.notepad = Err(error);
    let mut worker = worker(&folder, ports);
    assert_eq!(
        worker.handle(1, WorkerCommand::OpenConfig).outcome,
        Outcome::Opened(Err(OpenError::Failed(error)))
    );
}

/// Recovery is the user's explicit choice: the damaged file is moved aside and the settings
/// in effect are written in its place.
#[test]
fn recover_moves_a_damaged_file_aside_and_writes_the_settings_in_effect() {
    let folder = Folder::new();
    fs::write(folder.config(), "[[[ damaged").unwrap();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()));
    worker.handle(1, WorkerCommand::Apply(Change::Macros(false)));
    let answer = worker.handle(2, WorkerCommand::Recover);
    let Outcome::Recovered(Ok(Recovery::Replaced {
        backup: Some(backup),
        saved: Ok(()),
    })) = &answer.outcome
    else {
        panic!("{:?}", answer.outcome);
    };
    assert_eq!(fs::read(backup).unwrap(), b"[[[ damaged");
    assert!(!folder.stored().macros.enabled);
    assert!(!state(&answer).runtime.options.macros_enabled);
    // Nothing is left to recover.
    assert_eq!(
        worker.handle(3, WorkerCommand::Recover).outcome,
        Outcome::Recovered(Ok(Recovery::NotNeeded))
    );
}

/// The worker thread runs the commands in the order they were sent and delivers each answer;
/// `finish` closes the queue and waits until everything sent was done.
#[test]
fn the_worker_thread_answers_in_order_and_finishes_its_queue() {
    let folder = Folder::new();
    let log = Log::default();
    let (deliver, answers) = mpsc::channel();
    let thread = WorkerThread::spawn(worker(&folder, FakePorts::new(&log)), move |answer| {
        deliver.send(answer).unwrap();
    })
    .unwrap();
    assert!(thread.send(1, WorkerCommand::Apply(Change::ModernTone(true))));
    assert!(thread.send(2, WorkerCommand::Poll));
    assert!(thread.send(3, WorkerCommand::Apply(Change::SpellCheck(false))));
    assert!(thread.finish(Duration::from_secs(20)));
    let seqs: Vec<u64> = answers.try_iter().map(|answer| answer.seq).collect();
    assert_eq!(seqs, [1, 2, 3]);
    let stored = folder.stored();
    assert!(stored.input.modern_tone);
    assert!(!stored.input.spell_check);
}

/// A command that panics is contained on the worker thread: it is answered as failed (so the
/// tray never waits for it), and the worker goes on with the next command.
#[test]
fn a_command_that_panics_is_answered_as_failed_and_the_worker_goes_on() {
    let folder = Folder::new();
    let log = Log::default();
    let mut ports = FakePorts::new(&log);
    ports.notepad_panics = true;
    let (deliver, answers) = mpsc::channel();
    let thread = WorkerThread::spawn(worker(&folder, ports), move |answer| {
        deliver.send(answer).unwrap();
    })
    .unwrap();
    thread.send(1, WorkerCommand::OpenConfig);
    thread.send(2, WorkerCommand::Apply(Change::Macros(false)));
    assert!(thread.finish(Duration::from_secs(20)));
    let answers: Vec<Answer> = answers.try_iter().collect();
    assert_eq!(answers.len(), 2);
    assert_eq!((answers[0].seq, &answers[0].outcome), (1, &Outcome::Failed));
    assert_eq!(answers[1].seq, 2);
    assert!(!folder.stored().macros.enabled);
}

/// The answers travel to the tray's thread and the worker owns `Settings`.
#[test]
fn answers_and_workers_move_between_threads() {
    fn send<T: Send>() {}
    send::<Answer>();
    send::<SettingsWorker<FakePorts>>();
    send::<WorkerCommand>();
}

/// `Debug` of an answer shows revisions and states, never macro text (a poll's macros never
/// travel in an answer at all).
#[test]
fn answers_hold_no_macro_text() {
    let folder = Folder::new();
    let mut worker = worker(&folder, FakePorts::new(&Log::default()));
    fs::write(
        folder.macros(),
        "version = 1\nrevision = 1\n[[macro]]\nid = 1\ntrigger = \"secret-trigger\"\nreplacement = \"secret replacement\"\n",
    )
    .unwrap();
    let answer = worker.handle(1, WorkerCommand::Poll);
    let shown = format!("{answer:?}");
    assert!(!shown.contains("secret"), "{shown}");
}
