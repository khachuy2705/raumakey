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
//! | [`WorkerCommand::Apply`] | [`Settings::apply`]: the change takes effect and is committed | [`Outcome::Applied`] with the commit's result |
//! | [`WorkerCommand::Poll`] (the tray's timer, every 2 s) | [`Settings::poll`]; new macros go to the typing hook ([`WorkerPorts::post_macros`]) | [`Outcome::Polled`]: `None` when nothing changed, else the state of the files |
//! | [`WorkerCommand::Startup`] `(true)` | creates `Raumakey.lnk` in the Startup folder, then saves `startup = "shortcut"` | [`Outcome::Startup`] |
//! | [`WorkerCommand::Startup`] `(false)` | removes the shortcut, then saves `startup = "none"` | [`Outcome::Startup`] |
//! | [`WorkerCommand::Recover`] (the user said yes) | [`Settings::recover`] | [`Outcome::Recovered`] |
//! | [`WorkerCommand::OpenConfig`] | opens `raumakey.toml` in Notepad when the file exists | [`Outcome::Opened`] |
//!
//! A shortcut that cannot be created leaves `startup` as it was (nothing would start Raumakey
//! at logon), and one that cannot be removed leaves `shortcut` saved (it still starts
//! Raumakey).
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
//! closes the queue and waits a bounded time for the commands already sent, so a change made
//! just before Exit is still saved.
//!
//! Nothing here logs. `Debug` of an answer shows revisions, states, and paths, never macro
//! text: macros never travel in an answer.

use core::fmt;
use std::io;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::time::Duration;

use raumakey_config::{ConfigError, StartupMode};
use raumakey_ipc::message::{MacroSnapshot, RuntimeSnapshot};
use raumakey_windows::error::{WinError, WinResult};
use raumakey_windows::panic::guard_value;

use crate::settings::{Change, Recovery, Settings, Status};

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

/// Why the Startup-folder shortcut could not be set as asked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupError {
    /// The shortcut could not be created or removed; the saved choice is unchanged.
    Shortcut(WinError),
    /// The shortcut was created or removed, but the choice could not be saved; it is in
    /// effect until Raumakey exits.
    Save(ConfigError),
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
        /// The commit's result ([`Settings::apply`]).
        result: Result<(), ConfigError>,
    },
    /// [`WorkerCommand::Poll`]: `None` when nothing changed; otherwise the state of every file
    /// after the reload ([`Settings::status`]).
    Polled(Option<Status>),
    /// [`WorkerCommand::Startup`].
    Startup {
        /// Start with Windows (`true`) or not.
        on: bool,
        /// Whether the shortcut and the choice were both set.
        result: Result<(), StartupError>,
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
}

impl<P> fmt::Debug for SettingsWorker<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SettingsWorker")
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

impl<P: WorkerPorts> SettingsWorker<P> {
    /// The worker of `settings`, with `ports`.
    pub fn new(settings: Settings, ports: P) -> Self {
        Self { settings, ports }
    }

    /// The settings it owns.
    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Runs `command`, numbered `seq` by the tray, and answers it.
    pub fn handle(&mut self, seq: u64, command: WorkerCommand) -> Answer {
        let outcome = match command {
            WorkerCommand::Apply(change) => {
                let result = self.settings.apply(change);
                Outcome::Applied { change, result }
            }
            WorkerCommand::Poll => match self.settings.poll() {
                None => Outcome::Polled(None),
                Some(reloaded) => {
                    if let Some(macros) = &reloaded.macros {
                        self.ports.post_macros(macros);
                    }
                    Outcome::Polled(Some(self.settings.status().clone()))
                }
            },
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

    fn state(&self) -> SettingsState {
        SettingsState {
            runtime: self.settings.runtime_snapshot().clone(),
            startup: self.settings.document().system.startup,
        }
    }

    /// The shortcut first, then the choice: a choice is saved only when the shortcut matches
    /// it.
    fn startup(&mut self, on: bool) -> Result<(), StartupError> {
        let mode = if on {
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
        self.settings
            .apply(Change::Startup(mode))
            .map_err(StartupError::Save)
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
