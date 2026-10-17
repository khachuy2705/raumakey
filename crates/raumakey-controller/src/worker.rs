//! The settings worker of the run mode (D40, D41, D42): the thread that owns
//! [`Settings`](crate::settings::Settings) and does the tray's file work.
//!
//! [`Settings::apply`], [`Settings::poll`], and [`Settings::recover`] read and write files and
//! may wait up to two seconds for a store lock, and creating the Startup-folder shortcut or
//! starting Notepad is I/O as well, so none of it runs on the tray's window thread. The tray
//! sends numbered [`WorkerCommand`]s; the worker runs them one at a time, in the order they
//! were sent, and delivers one [`Answer`] for each:
//!
//! | Command | What the worker does | Answer |
//! |---|---|---|
//! | [`WorkerCommand::Apply`] | [`Settings::apply`]: the change takes effect and is committed | [`Outcome::Applied`] with the commit's result and whether it wrote anything ([`Saved`]) |
//! | [`WorkerCommand::Poll`] (the tray's timer, every 2 s) | [`Settings::poll`], once more after [`REREAD_DELAY`] when a reloaded file looks half-written (it cannot be decoded, or has no `version`); new macros go to the typing hook ([`WorkerPorts::post_macros`]); a hand edit of `[system] startup` creates or removes the shortcut ([`ShortcutSync`]); changes not saved yet are retried ([`Settings::save_unsaved`]) | [`Outcome::Polled`] with a [`PollReport`] |
//! | [`WorkerCommand::Startup`] `(true)` | creates `Raumakey.lnk` in the Startup folder (never for an exe in the temporary folder, [`StartupError::Temporary`]), then saves `startup = "shortcut"` | [`Outcome::Startup`] |
//! | [`WorkerCommand::Startup`] `(false)` | removes the shortcut, then saves `startup = "none"` | [`Outcome::Startup`] |
//! | [`WorkerCommand::Recover`] (the user said yes) | [`Settings::recover`] | [`Outcome::Recovered`] |
//! | [`WorkerCommand::OpenConfig`] | opens `raumakey.toml` in Notepad when the file exists | [`Outcome::Opened`] |
//!
//! A shortcut that cannot be created leaves `startup` as it was (nothing would start Raumakey
//! at logon), and one that cannot be removed leaves `shortcut` saved (it still starts
//! Raumakey). A hand edit to `shortcut` creates the shortcut only when there is none or it
//! starts an exe that no longer exists (another copy that still exists keeps its shortcut);
//! a hand edit to `none` removes only the shortcut of this exe; `elevated-task` touches no
//! shortcut.
//!
//! Every answer carries the number of its command ([`Answer::seq`]) and the settings in effect
//! afterwards ([`SettingsState`]: the runtime snapshot, with the mode the worker last saved,
//! and the startup mode). The tray takes them over only from the answer to the last command
//! it sent (`crate::tray`): an older answer does not know of a change still on its way, and
//! the answer to the last command, whatever it was, brings everything the worker has, such
//! as a reload answered before it.
//!
//! The macro snapshot of a reload goes to the typing hook from the worker thread
//! ([`WorkerPorts::post_macros`]): the hook's mailbox builds the macro table on the posting
//! thread, and a table can hold thousands of entries. Runtime snapshots go back to the tray,
//! which hands them to the hook with the mode it shows, so a mode that the hotkey changed
//! meanwhile is never undone.
//!
//! [`WorkerThread`] runs a [`SettingsWorker`] on its own thread. A command that panics is
//! contained there ([`Outcome::Failed`]) and the worker goes on. [`WorkerThread::finish`]
//! closes the queue and waits a bounded time for the commands already sent and for a last
//! retry of what is unsaved ([`SettingsWorker::finish`]), so a change made just before Exit
//! is still saved.
//!
//! Nothing here logs. `Debug` of an answer shows revisions, states, and paths, never macro
//! text: macros never travel in an answer.

use core::fmt;
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use raumakey_config::{ConfigError, IssueReason, StartupMode};
use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_windows::error::{WinError, WinResult};
use raumakey_windows::panic::guard_value;

use crate::run::ShortcutState;
use crate::settings::{Change, DocumentStatus, Recovery, Reloaded, Settings, Status};

/// Name of the worker thread.
pub const THREAD_NAME: &str = "raumakey-settings";

/// A command of the tray for the settings worker.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WorkerCommand {
    /// Apply a change from the tray and commit it ([`Settings::apply`]).
    Apply(Change),
    /// Look for files changed on disk ([`Settings::poll`]).
    Poll,
    /// Start with Windows through the Startup-folder shortcut (`true`), or not (`false`).
    Startup(bool),
    /// Move a damaged `raumakey.toml` aside and write the settings in effect
    /// ([`Settings::recover`]); only after the user said yes.
    Recover,
    /// Open `raumakey.toml` in Notepad.
    OpenConfig,
}

/// The settings in effect after a command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SettingsState {
    /// [`Settings::runtime_snapshot`].
    pub runtime: RuntimeSnapshot,
    /// `[system] startup` in effect.
    pub startup: StartupMode,
}

/// Whether a change that worked wrote `raumakey.toml`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Saved {
    /// The file now holds the change.
    Written,
    /// Nothing was written: the value was in effect already.
    Unchanged,
}

/// Why the Startup-folder shortcut could not be set as asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupError {
    /// `raumakey.exe` runs from the temporary folder (from inside a ZIP, which Explorer
    /// deletes later): no shortcut is made to it, and nothing is changed.
    Temporary,
    /// The shortcut could not be created or removed; the saved choice is unchanged.
    Shortcut(WinError),
    /// The shortcut was created or removed, but the choice could not be saved; it is in
    /// effect, and retried, until Raumakey exits.
    Save(ConfigError),
}

/// What a hand edit of `[system] startup` between `none` and `shortcut` did to the shortcut
/// (D41). An edit to or from `elevated-task` touches no shortcut.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShortcutSync {
    /// `shortcut`: the shortcut was missing, or started an exe that no longer exists (the exe
    /// was moved), so a shortcut to this `raumakey.exe` was created.
    Created,
    /// `none`: the shortcut to this `raumakey.exe` was removed.
    Removed,
    /// Nothing to do: the shortcut is right already, starts another copy that still exists
    /// (never re-pointed), or is not this exe's to remove.
    Unchanged,
    /// `shortcut`, but `raumakey.exe` runs from the temporary folder: nothing was created.
    RefusedTemporary,
    /// The shortcut could not be read, created, or removed.
    Failed {
        /// Whether it was to be created (`true`) or removed.
        on: bool,
        /// Why.
        error: WinError,
    },
}

/// What a poll found and did.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PollReport {
    /// The state of every file after a reload ([`Settings::status`]); `None` when nothing
    /// changed on disk.
    pub status: Option<Status>,
    /// What a hand edit of `[system] startup` did to the shortcut.
    pub shortcut: Option<ShortcutSync>,
    /// The retry of changes not saved yet ([`Settings::save_unsaved`]); `None` when there
    /// were none.
    pub retry: Option<Result<(), ConfigError>>,
}

/// Why `raumakey.toml` was not opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenError {
    /// There is no `raumakey.toml` (deleted, never written, or no folder at all).
    NoFile,
    /// Notepad could not be started.
    Failed(WinError),
}

/// What a command did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// [`WorkerCommand::Apply`]: the change is in effect; `result` says whether it was saved.
    Applied {
        /// The change.
        change: Change,
        /// The commit's result ([`Settings::apply`]), and whether it wrote anything.
        result: Result<Saved, ConfigError>,
    },
    /// [`WorkerCommand::Poll`].
    Polled(PollReport),
    /// [`WorkerCommand::Startup`].
    Startup {
        /// Start with Windows (`true`) or not.
        on: bool,
        /// Whether the shortcut and the choice were both set, and whether the choice was
        /// written.
        result: Result<Saved, StartupError>,
    },
    /// [`WorkerCommand::Recover`]: [`Settings::recover`]'s result.
    Recovered(Result<Recovery, ConfigError>),
    /// [`WorkerCommand::OpenConfig`].
    Opened(Result<(), OpenError>),
    /// The command panicked; the worker went on with the next one.
    Failed,
}

/// The answer to one command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Answer {
    /// The number the tray gave the command.
    pub seq: u64,
    /// What the command did.
    pub outcome: Outcome,
    /// The settings in effect after the command; `None` only after a panic
    /// ([`Outcome::Failed`]).
    pub state: Option<Box<SettingsState>>,
}

/// The side effects of the worker besides the configuration files.
pub trait WorkerPorts {
    /// Hands a new macro snapshot to the typing hook (its mailbox builds the table on this
    /// thread).
    fn post_macros(&mut self, macros: &MacroSnapshot);
    /// What `Raumakey.lnk` in the user's Startup folder starts.
    fn shortcut_state(&mut self) -> WinResult<ShortcutState>;
    /// Creates (or replaces) `Raumakey.lnk` in the user's Startup folder, pointing at the
    /// running `raumakey.exe`.
    fn create_shortcut(&mut self) -> WinResult<()>;
    /// Removes that shortcut; whether it existed.
    fn remove_shortcut(&mut self) -> WinResult<bool>;
    /// Opens `file` (absolute, existing) in Notepad without waiting for it.
    fn open_in_notepad(&mut self, file: &Path) -> WinResult<()>;
}

/// The settings and the side effects of the worker; [`handle`](Self::handle) runs one
/// command (see the [module documentation](self)).
pub struct SettingsWorker<P> {
    settings: Settings,
    ports: P,
    /// `raumakey.exe` runs from the temporary folder: no shortcut is made to it.
    temporary_exe: bool,
    /// How long a poll waits before it reads a file that looks half-written once more.
    reread_delay: Duration,
}

impl<P> fmt::Debug for SettingsWorker<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsWorker")
            .field("settings", &self.settings)
            .field("temporary_exe", &self.temporary_exe)
            .finish_non_exhaustive()
    }
}

/// How long a poll waits before it reads a file that looks half-written once more.
pub const REREAD_DELAY: Duration = Duration::from_millis(300);

/// Whether a reloaded file looks like an editor caught half-way through saving it: it cannot
/// be decoded, or it has no `version` (an empty or cut file).
fn looks_half_written(status: &Status) -> bool {
    [&status.config, &status.macros, &status.app_rules]
        .into_iter()
        .any(|file| match file {
            DocumentStatus::Unreadable(ConfigError::Corrupt) => true,
            DocumentStatus::Loaded { issues } => issues
                .iter()
                .any(|issue| issue.path == "version" && issue.reason == IssueReason::Missing),
            DocumentStatus::Missing
            | DocumentStatus::Unreadable(_)
            | DocumentStatus::Unusable(_) => false,
        })
}

impl<P: WorkerPorts> SettingsWorker<P> {
    /// The worker of `settings`, with `ports`, for a `raumakey.exe` outside the temporary
    /// folder, reading a half-written file again after [`REREAD_DELAY`].
    pub fn new(settings: Settings, ports: P) -> Self {
        Self {
            settings,
            ports,
            temporary_exe: false,
            reread_delay: REREAD_DELAY,
        }
    }

    /// The same worker for a `raumakey.exe` that runs from the temporary folder (`true`):
    /// it makes no shortcut ([`crate::settings::is_temporary_path`]).
    #[must_use]
    pub fn with_temporary_exe(mut self, temporary: bool) -> Self {
        self.temporary_exe = temporary;
        self
    }

    /// The same worker, waiting `delay` before it reads a half-written file again.
    #[must_use]
    pub fn with_reread_delay(mut self, delay: Duration) -> Self {
        self.reread_delay = delay;
        self
    }

    /// The settings it owns.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Runs `command`, numbered `seq` by the tray, and answers it.
    pub fn handle(&mut self, seq: u64, command: WorkerCommand) -> Answer {
        let outcome = match command {
            WorkerCommand::Apply(change) => Outcome::Applied {
                change,
                result: self.save(change),
            },
            WorkerCommand::Poll => Outcome::Polled(self.poll()),
            WorkerCommand::Startup(on) => Outcome::Startup {
                on,
                result: self.startup(on),
            },
            WorkerCommand::Recover => Outcome::Recovered(self.settings.recover()),
            WorkerCommand::OpenConfig => Outcome::Opened(self.open_config()),
        };
        Answer {
            seq,
            outcome,
            state: Some(Box::new(self.state())),
        }
    }

    /// Saves what is still unsaved, as the worker's last act before it ends.
    pub fn finish(&mut self) {
        let _ = self.settings.save_unsaved();
    }

    fn state(&self) -> SettingsState {
        SettingsState {
            runtime: self.settings.runtime_snapshot().clone(),
            startup: self.settings.document().system.startup,
        }
    }

    /// [`Settings::apply`], telling whether the file was written (its revision moved).
    fn save(&mut self, change: Change) -> Result<Saved, ConfigError> {
        let revision = self.settings.document().revision;
        self.settings.apply(change)?;
        Ok(if self.settings.document().revision == revision {
            Saved::Unchanged
        } else {
            Saved::Written
        })
    }

    /// Reloads changed files (once more after [`REREAD_DELAY`] when one looks half-written),
    /// hands new macros to the hook, follows a hand edit of `[system] startup`, and retries
    /// the changes not saved yet.
    fn poll(&mut self) -> PollReport {
        let before = self.settings.document().system.startup;
        let mut reloaded = self.settings.poll();
        if reloaded.is_some() && looks_half_written(self.settings.status()) {
            std::thread::sleep(self.reread_delay);
            if let Some(again) = self.settings.poll() {
                let first = reloaded.take();
                let first_macros = first.and_then(|first| first.macros);
                reloaded = Some(Reloaded {
                    runtime: again.runtime,
                    macros: again.macros.or(first_macros),
                });
            }
        }
        let mut report = PollReport::default();
        if let Some(reloaded) = reloaded {
            if let Some(macros) = &reloaded.macros {
                self.ports.post_macros(macros);
            }
            report.status = Some(self.settings.status().clone());
            let after = self.settings.document().system.startup;
            report.shortcut = self.follow_startup(before, after);
        }
        report.retry = match self.settings.save_unsaved() {
            Ok(false) => None,
            Ok(true) => Some(Ok(())),
            Err(error) => Some(Err(error)),
        };
        report
    }

    /// A hand edit of `[system] startup` from `before` to `after` (D41): turning the shortcut
    /// on creates a missing or dead one (never re-points another copy that still exists, and
    /// never from the temporary folder); turning it off removes only this exe's.
    fn follow_startup(&mut self, before: StartupMode, after: StartupMode) -> Option<ShortcutSync> {
        let on = match (before, after) {
            (StartupMode::None, StartupMode::Shortcut) => true,
            (StartupMode::Shortcut, StartupMode::None) => false,
            _ => return None,
        };
        if on && self.temporary_exe {
            return Some(ShortcutSync::RefusedTemporary);
        }
        let failed = |error| ShortcutSync::Failed { on, error };
        let state = match self.ports.shortcut_state() {
            Ok(state) => state,
            Err(error) => return Some(failed(error)),
        };
        let sync = match (on, state) {
            (true, ShortcutState::Missing | ShortcutState::PointsToMissing) => {
                match self.ports.create_shortcut() {
                    Ok(()) => ShortcutSync::Created,
                    Err(error) => failed(error),
                }
            }
            (false, ShortcutState::PointsHere) => match self.ports.remove_shortcut() {
                Ok(_) => ShortcutSync::Removed,
                Err(error) => failed(error),
            },
            _ => ShortcutSync::Unchanged,
        };
        Some(sync)
    }

    /// The shortcut first, then the choice: a choice is saved only when the shortcut matches
    /// it. No shortcut is made to an exe in the temporary folder.
    fn startup(&mut self, on: bool) -> Result<Saved, StartupError> {
        let mode = if on {
            if self.temporary_exe {
                return Err(StartupError::Temporary);
            }
            self.ports
                .create_shortcut()
                .map_err(StartupError::Shortcut)?;
            StartupMode::Shortcut
        } else {
            self.ports
                .remove_shortcut()
                .map_err(StartupError::Shortcut)?;
            StartupMode::None
        };
        self.save(Change::Startup(mode)).map_err(StartupError::Save)
    }

    fn open_config(&mut self) -> Result<(), OpenError> {
        let file = self.settings.config_path().ok_or(OpenError::NoFile)?;
        if !file.is_file() {
            return Err(OpenError::NoFile);
        }
        self.ports.open_in_notepad(file).map_err(OpenError::Failed)
    }
}

/// A [`SettingsWorker`] running on its own thread ([`THREAD_NAME`]).
///
/// Dropping it closes the queue without waiting: the thread runs what was sent and ends.
#[derive(Debug)]
pub struct WorkerThread {
    commands: Sender<(u64, WorkerCommand)>,
    /// Disconnected when the thread ends.
    ended: Receiver<()>,
}

impl WorkerThread {
    /// Starts the thread of `worker`; each answer is passed to `deliver` on that thread, which
    /// must hand it on at once (to the tray's inbox, with a message to the tray window).
    ///
    /// # Errors
    ///
    /// The thread could not be started.
    pub fn spawn<P: WorkerPorts + Send + 'static>(
        mut worker: SettingsWorker<P>,
        mut deliver: impl FnMut(Answer) + Send + 'static,
    ) -> io::Result<Self> {
        let (commands, queue) = mpsc::channel::<(u64, WorkerCommand)>();
        let (alive, ended) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name(THREAD_NAME.to_owned())
            .spawn(move || {
                let _alive = alive;
                for (seq, command) in queue {
                    let answer = guard_value(
                        Answer {
                            seq,
                            outcome: Outcome::Failed,
                            state: None,
                        },
                        || worker.handle(seq, command),
                    );
                    guard_value((), || deliver(answer));
                }
                // The queue is closed: save what is still unsaved before the thread ends.
                guard_value((), || worker.finish());
            })?;
        Ok(Self { commands, ended })
    }

    /// Queues `command`, numbered `seq`; false when the thread has ended.
    pub fn send(&self, seq: u64, command: WorkerCommand) -> bool {
        self.commands.send((seq, command)).is_ok()
    }

    /// Closes the queue and waits up to `timeout` until the thread has run every command sent
    /// and ended; whether it did.
    pub fn finish(self, timeout: Duration) -> bool {
        let Self { commands, ended } = self;
        drop(commands);
        matches!(
            ended.recv_timeout(timeout),
            Err(RecvTimeoutError::Disconnected)
        )
    }
}
