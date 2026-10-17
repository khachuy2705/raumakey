//! An in-memory fake of every side effect of the controller's commands, and output capture.
//!
//! The fake keeps the Raumakey registrations of the 64-bit and 32-bit registry views, the TIP
//! files that exist, the installation record, the startup shortcut, the elevated logon task of
//! each account (by SID hash; this process runs as [`me`] unless a test says otherwise), and
//! the clipboard.
//! Its regsvr32 behaves like the TIP's `DllRegisterServer`/`DllUnregisterServer`: a run that
//! succeeds registers or removes every entry of the DLL's view, a run that fails changes
//! nothing (the DLL rolls itself back), and a DLL that does not exist fails to load (exit 3).
//! Every call is logged in [`FakeSystem::calls`]. The desktop of the run mode is
//! [`FakeDesktop`], which logs its own calls.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use raumakey_controller::commands::System;
use raumakey_controller::elevation::Elevation;
use raumakey_controller::install::{InstallRecord, InstallSystem};
use raumakey_controller::output::Output;
use raumakey_controller::run::Desktop;
use raumakey_controller::startup::StartupSystem;
use raumakey_controller::tray::InputMode;
use raumakey_converter::{ClipboardError, ClipboardPort, ClipboardText};
use raumakey_model::identity::Guid;
use raumakey_windows::error::{E_ACCESSDENIED, WinError, WinResult, win32};
use raumakey_windows::names::SidHash;
use raumakey_windows::registry::RegView;
use raumakey_windows::tsf_registration::{
    COM_CLASS_NAME, ComClassState, RegistrationReader, THREADING_MODEL, TIP_CATEGORIES, TipArch,
    TipBinary,
};

/// An access-denied failure.
pub fn denied(context: &'static str) -> WinError {
    WinError::new(E_ACCESSDENIED, context)
}

/// The user declined the UAC prompt.
pub fn cancelled() -> WinError {
    WinError::from_win32(win32::ERROR_CANCELLED, "fake elevation")
}

/// A child process that did not finish in time.
pub fn timed_out() -> WinError {
    WinError::from_win32(win32::ERROR_TIMEOUT, "fake wait")
}

/// The account (SID hash) this process runs as, unless a test says otherwise.
pub fn me() -> SidHash {
    SidHash::parse("0123456789abcdef").unwrap()
}

/// Another account of the same machine.
pub fn someone_else() -> SidHash {
    SidHash::parse("fedcba9876543210").unwrap()
}

/// The Raumakey entries of one registry view.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ViewState {
    /// The COM class key.
    pub com: Option<ComClassState>,
    /// The language profile.
    pub profile: bool,
    /// The categories the TIP is registered in.
    pub categories: BTreeSet<Guid>,
}

impl ViewState {
    /// Everything `DllRegisterServer` of `dll` writes.
    pub fn registered(dll: &Path) -> Self {
        Self {
            com: Some(ComClassState {
                class_name: Some(COM_CLASS_NAME.to_owned()),
                inproc_server: Some(dll.to_string_lossy().into_owned()),
                threading_model: Some(THREADING_MODEL.to_owned()),
            }),
            profile: true,
            categories: TIP_CATEGORIES
                .iter()
                .map(|category| category.guid)
                .collect(),
        }
    }

    /// No Raumakey entry.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The registered DLL path.
    pub fn server(&self) -> Option<&str> {
        self.com.as_ref()?.inproc_server.as_deref()
    }
}

/// What a regsvr32 run returns instead of the default behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// regsvr32 exits with this code and changes nothing (also with 0: an exit code is no
    /// proof).
    Exit(u32),
    /// regsvr32 exits with this code after a registration that failed half-way and whose own
    /// rollback failed too: the COM class of the DLL's view is left behind.
    Partial(u32),
    /// regsvr32 exits with this code after a registration that failed and whose own rollback
    /// removed every Raumakey entry of the DLL's view, also those of an earlier registration.
    Removed(u32),
    /// regsvr32 cannot be started, and changes nothing.
    Error(WinError),
    /// regsvr32 runs as usual (the default behavior), but waiting for it or reading its exit
    /// code then fails with this error.
    Unread(WinError),
}

/// One call to the fake.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Call {
    IsElevated,
    RunElevated(Vec<String>),
    Regsvr32 {
        arch: TipArch,
        path: PathBuf,
        unregister: bool,
    },
    ReadRecord,
    WriteRecord(InstallRecord),
    RemoveRecord,
    CreateShortcut,
    RemoveShortcut,
    TaskExists(SidHash),
    CreateTask(SidHash),
    DeleteTask(SidHash),
}

/// An in-memory clipboard whose sequence number advances with every change, as on Windows.
#[derive(Clone, Debug, Default)]
pub struct FakeClipboard {
    /// The text on the clipboard (`None`: no text format).
    pub text: Option<Vec<u16>>,
    /// The sequence number.
    pub sequence: u32,
    /// Reading fails with this error.
    pub read_error: Option<ClipboardError>,
    /// Another application changes the clipboard right after it is read.
    pub changed_by_another_application: bool,
    /// Successful writes.
    pub writes: usize,
}

impl FakeClipboard {
    /// A clipboard holding `text`.
    pub fn with_text(text: &str) -> Self {
        Self {
            text: Some(text.encode_utf16().collect()),
            sequence: 7,
            ..Self::default()
        }
    }

    /// The text on the clipboard.
    pub fn text(&self) -> Option<String> {
        self.text
            .as_ref()
            .map(|units| String::from_utf16_lossy(units))
    }
}

impl ClipboardPort for FakeClipboard {
    fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError> {
        if let Some(error) = self.read_error {
            return Err(error);
        }
        let text = self.text.clone().map(|units| ClipboardText {
            units,
            sequence: self.sequence,
        });
        if self.changed_by_another_application {
            self.sequence = self.sequence.wrapping_add(1);
        }
        Ok(text)
    }

    fn write_text_if_unchanged(
        &mut self,
        expected_sequence: u32,
        units: &[u16],
    ) -> Result<bool, ClipboardError> {
        if self.sequence != expected_sequence {
            return Ok(false);
        }
        self.text = Some(units.to_vec());
        self.sequence = self.sequence.wrapping_add(1);
        self.writes += 1;
        Ok(true)
    }
}

/// One call to the fake desktop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DesktopCall {
    AcquireInstance,
    RunTray(InputMode),
    ReleaseInstance,
    ReportError(String),
}

/// The desktop of the run mode: the single-instance mutex, the tray, and error reports.
#[derive(Debug)]
pub struct FakeDesktop {
    /// Answer of the single-instance check.
    pub instance: WinResult<bool>,
    /// Result of the tray: the exit code of its message loop.
    pub tray: WinResult<i32>,
    /// The mutex is held.
    pub held: bool,
    /// Every call, in order.
    pub calls: Vec<DesktopCall>,
}

impl Default for FakeDesktop {
    fn default() -> Self {
        Self {
            instance: Ok(true),
            tray: Ok(0),
            held: false,
            calls: Vec::new(),
        }
    }
}

impl Desktop for FakeDesktop {
    fn acquire_instance(&mut self) -> WinResult<bool> {
        self.calls.push(DesktopCall::AcquireInstance);
        assert!(!self.held, "the instance is acquired once");
        self.held = self.instance == Ok(true);
        self.instance
    }

    fn run_tray(&mut self, mode: InputMode) -> WinResult<i32> {
        self.calls.push(DesktopCall::RunTray(mode));
        assert!(self.held, "the tray runs only while the instance is held");
        self.tray
    }

    fn release_instance(&mut self) {
        self.calls.push(DesktopCall::ReleaseInstance);
        assert!(self.held, "only a held instance is released");
        self.held = false;
    }

    fn report_error(&mut self, text: &str) {
        self.calls.push(DesktopCall::ReportError(text.to_owned()));
    }
}

/// The fake machine.
#[derive(Debug)]
pub struct FakeSystem {
    /// Answer of the elevation check.
    pub elevated: Result<bool, WinError>,
    /// Result of an elevated restart.
    pub elevated_run: Result<u32, WinError>,
    /// Files that exist.
    pub files: BTreeSet<PathBuf>,
    /// The 64-bit registry view.
    pub view64: ViewState,
    /// The 32-bit registry view.
    pub view32: ViewState,
    /// Reading the registrations fails.
    pub registration_read_fails: bool,
    /// regsvr32 results that replace the default behavior, by architecture and `/u`.
    pub regsvr32: BTreeMap<(TipArch, bool), Outcome>,
    /// regsvr32 results for one DLL and `/u`; they take precedence over [`Self::regsvr32`].
    pub regsvr32_dll: BTreeMap<(PathBuf, bool), Outcome>,
    /// Architectures whose DLL registers without its language profile.
    pub without_profile: BTreeSet<TipArch>,
    /// The installation record.
    pub record: Option<InstallRecord>,
    /// Reading the record fails.
    pub record_read_fails: bool,
    /// The next this many record writes fail.
    pub record_write_failures: usize,
    /// Removing the record fails.
    pub record_remove_fails: bool,
    /// Files outside the folders that only administrators can change (every other path is
    /// inside one).
    pub unprotected: BTreeSet<PathBuf>,
    /// The account this process runs as, or the error of that lookup.
    pub user: Result<SidHash, WinError>,
    /// `raumakey.exe` lies in a folder that only administrators can change.
    pub program_protected: bool,
    /// The startup shortcut (of this process's account) exists.
    pub shortcut: bool,
    /// The accounts whose elevated logon task exists.
    pub tasks: BTreeSet<SidHash>,
    /// Creating the shortcut fails.
    pub shortcut_create_fails: bool,
    /// Removing the shortcut fails.
    pub shortcut_remove_fails: bool,
    /// Checking for the task fails with this error.
    pub task_check_error: Option<WinError>,
    /// Creating the task fails.
    pub task_create_fails: bool,
    /// Deleting the task fails.
    pub task_delete_fails: bool,
    /// The clipboard.
    pub clipboard: FakeClipboard,
    /// The desktop of the run mode.
    pub desktop: FakeDesktop,
    /// Every call, in order (the desktop logs its own).
    pub calls: Vec<Call>,
}

impl Default for FakeSystem {
    fn default() -> Self {
        Self {
            elevated: Ok(true),
            elevated_run: Ok(0),
            files: BTreeSet::new(),
            view64: ViewState::default(),
            view32: ViewState::default(),
            registration_read_fails: false,
            regsvr32: BTreeMap::new(),
            regsvr32_dll: BTreeMap::new(),
            without_profile: BTreeSet::new(),
            record: None,
            record_read_fails: false,
            record_write_failures: 0,
            record_remove_fails: false,
            unprotected: BTreeSet::new(),
            user: Ok(me()),
            program_protected: true,
            shortcut: false,
            tasks: BTreeSet::new(),
            shortcut_create_fails: false,
            shortcut_remove_fails: false,
            task_check_error: None,
            task_create_fails: false,
            task_delete_fails: false,
            clipboard: FakeClipboard::default(),
            desktop: FakeDesktop::default(),
            calls: Vec::new(),
        }
    }
}

impl FakeSystem {
    /// An elevated machine on which `files` exist.
    pub fn with_files(files: &[&Path]) -> Self {
        Self {
            files: files.iter().map(|path| path.to_path_buf()).collect(),
            ..Self::default()
        }
    }

    /// The view of `arch`.
    pub fn view(&self, arch: TipArch) -> &ViewState {
        match arch.registry_view() {
            RegView::Wow64_32 => &self.view32,
            RegView::Wow64_64 | RegView::Default => &self.view64,
        }
    }

    fn view_mut(&mut self, arch: TipArch) -> &mut ViewState {
        match arch.registry_view() {
            RegView::Wow64_32 => &mut self.view32,
            RegView::Wow64_64 | RegView::Default => &mut self.view64,
        }
    }

    fn view_of(&self, view: RegView) -> &ViewState {
        match view {
            RegView::Wow64_32 => &self.view32,
            RegView::Wow64_64 | RegView::Default => &self.view64,
        }
    }

    /// Whether the elevated logon task of this process's account exists.
    pub fn my_task(&self) -> bool {
        self.user
            .as_ref()
            .is_ok_and(|user| self.tasks.contains(user))
    }

    /// The regsvr32 runs, in order: (architecture, DLL, unregister).
    pub fn regsvr32_runs(&self) -> Vec<(TipArch, PathBuf, bool)> {
        self.calls
            .iter()
            .filter_map(|call| match call {
                Call::Regsvr32 {
                    arch,
                    path,
                    unregister,
                } => Some((*arch, path.clone(), *unregister)),
                _ => None,
            })
            .collect()
    }

    /// Whether any call changed the machine (regsvr32, record, shortcut, task).
    pub fn changed_anything(&self) -> bool {
        self.calls.iter().any(|call| {
            matches!(
                call,
                Call::Regsvr32 { .. }
                    | Call::WriteRecord(_)
                    | Call::RemoveRecord
                    | Call::CreateShortcut
                    | Call::RemoveShortcut
                    | Call::CreateTask(_)
                    | Call::DeleteTask(_)
            )
        })
    }
}

impl Elevation for FakeSystem {
    fn is_elevated(&mut self) -> WinResult<bool> {
        self.calls.push(Call::IsElevated);
        self.elevated
    }

    fn run_elevated(&mut self, args: &[String]) -> WinResult<u32> {
        self.calls.push(Call::RunElevated(args.to_vec()));
        self.elevated_run
    }
}

impl RegistrationReader for FakeSystem {
    fn com_class(&mut self, view: RegView) -> Result<Option<ComClassState>, WinError> {
        if self.registration_read_fails {
            return Err(denied("fake registry read"));
        }
        Ok(self.view_of(view).com.clone())
    }

    fn profile_present(&mut self, view: RegView) -> Result<bool, WinError> {
        if self.registration_read_fails {
            return Err(denied("fake registry read"));
        }
        Ok(self.view_of(view).profile)
    }

    fn category_present(&mut self, view: RegView, category: Guid) -> Result<bool, WinError> {
        if self.registration_read_fails {
            return Err(denied("fake registry read"));
        }
        Ok(self.view_of(view).categories.contains(&category))
    }
}

impl InstallSystem for FakeSystem {
    fn file_exists(&mut self, path: &Path) -> bool {
        self.files.contains(path)
    }

    fn regsvr32(&mut self, tip: &TipBinary, unregister: bool) -> WinResult<u32> {
        self.calls.push(Call::Regsvr32 {
            arch: tip.arch,
            path: tip.path.clone(),
            unregister,
        });
        let outcome = self
            .regsvr32_dll
            .get(&(tip.path.clone(), unregister))
            .or_else(|| self.regsvr32.get(&(tip.arch, unregister)))
            .copied();
        if let Some(outcome) = outcome {
            return match outcome {
                Outcome::Exit(code) => Ok(code),
                Outcome::Partial(code) => {
                    let view = self.view_mut(tip.arch);
                    view.com = ViewState::registered(&tip.path).com;
                    Ok(code)
                }
                Outcome::Removed(code) => {
                    *self.view_mut(tip.arch) = ViewState::default();
                    Ok(code)
                }
                Outcome::Error(error) => Err(error),
                Outcome::Unread(error) => {
                    self.regsvr32_default(tip, unregister);
                    Err(error)
                }
            };
        }
        Ok(self.regsvr32_default(tip, unregister))
    }

    fn read_record(&mut self) -> WinResult<Option<InstallRecord>> {
        self.calls.push(Call::ReadRecord);
        if self.record_read_fails {
            return Err(denied("fake record read"));
        }
        Ok(self.record.clone())
    }

    fn write_record(&mut self, record: &InstallRecord) -> WinResult<()> {
        self.calls.push(Call::WriteRecord(record.clone()));
        if self.record_write_failures > 0 {
            self.record_write_failures -= 1;
            return Err(denied("fake record write"));
        }
        self.record = Some(record.clone());
        Ok(())
    }

    fn remove_record(&mut self) -> WinResult<bool> {
        self.calls.push(Call::RemoveRecord);
        if self.record_remove_fails {
            return Err(denied("fake record removal"));
        }
        Ok(self.record.take().is_some())
    }

    fn is_protected_location(&mut self, path: &Path) -> bool {
        !self.unprotected.contains(path)
    }
}

impl FakeSystem {
    /// What the TIP's `DllRegisterServer`/`DllUnregisterServer` does when regsvr32 of `tip`
    /// runs; returns regsvr32's exit code.
    fn regsvr32_default(&mut self, tip: &TipBinary, unregister: bool) -> u32 {
        if !self.files.contains(&tip.path) {
            // LoadLibrary failed.
            return 3;
        }
        let without_profile = self.without_profile.contains(&tip.arch);
        let view = self.view_mut(tip.arch);
        if unregister {
            *view = ViewState::default();
        } else {
            *view = ViewState::registered(&tip.path);
            if without_profile {
                view.profile = false;
            }
        }
        0
    }
}

impl StartupSystem for FakeSystem {
    fn current_user(&mut self) -> WinResult<SidHash> {
        self.user.clone()
    }

    fn create_shortcut(&mut self) -> WinResult<()> {
        self.calls.push(Call::CreateShortcut);
        if self.shortcut_create_fails {
            return Err(denied("fake shortcut creation"));
        }
        self.shortcut = true;
        Ok(())
    }

    fn remove_shortcut(&mut self) -> WinResult<bool> {
        self.calls.push(Call::RemoveShortcut);
        if self.shortcut_remove_fails {
            return Err(denied("fake shortcut removal"));
        }
        Ok(core::mem::take(&mut self.shortcut))
    }

    fn task_exists(&mut self, user: &SidHash) -> WinResult<bool> {
        self.calls.push(Call::TaskExists(user.clone()));
        match self.task_check_error {
            Some(error) => Err(error),
            None => Ok(self.tasks.contains(user)),
        }
    }

    fn create_task(&mut self, user: &SidHash) -> WinResult<()> {
        self.calls.push(Call::CreateTask(user.clone()));
        // As on Windows: the task runs as the account of this process, so it is created only
        // for that account.
        if self.task_create_fails || self.user.as_ref() != Ok(user) {
            return Err(denied("fake task creation"));
        }
        self.tasks.insert(user.clone());
        Ok(())
    }

    fn delete_task(&mut self, user: &SidHash) -> WinResult<bool> {
        self.calls.push(Call::DeleteTask(user.clone()));
        if self.task_delete_fails {
            return Err(denied("fake task deletion"));
        }
        Ok(self.tasks.remove(user))
    }

    fn program_is_protected(&mut self) -> bool {
        self.program_protected
    }
}

impl System for FakeSystem {
    fn clipboard(&mut self) -> &mut dyn ClipboardPort {
        &mut self.clipboard
    }

    fn desktop(&mut self) -> &mut dyn Desktop {
        &mut self.desktop
    }
}

/// What a command printed and returned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Captured {
    /// The exit code.
    pub code: u8,
    /// Standard output.
    pub out: String,
    /// Standard error.
    pub err: String,
}

/// Runs `command` with an [`Output`] that captures both streams.
pub fn capture(command: impl FnOnce(&mut Output<'_>) -> u8) -> Captured {
    let mut out = Vec::new();
    let mut err = Vec::new();
    let code = {
        let mut output = Output::new(&mut out, &mut err);
        command(&mut output)
    };
    Captured {
        code,
        out: String::from_utf8_lossy(&out).into_owned(),
        err: String::from_utf8_lossy(&err).into_owned(),
    }
}

/// A TIP of `arch` at `path`.
pub fn tip(arch: TipArch, path: &str) -> TipBinary {
    TipBinary {
        path: PathBuf::from(path),
        arch,
    }
}

/// Every `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}` in `text`.
pub fn guids_in(text: &str) -> Vec<Guid> {
    let mut found = Vec::new();
    for (start, _) in text.match_indices('{') {
        if let Some(candidate) = text.get(start..start + 38)
            && let Ok(guid) = Guid::parse(candidate)
        {
            found.push(guid);
        }
    }
    found
}
