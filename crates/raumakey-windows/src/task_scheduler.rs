//! The optional elevated logon task (design §13, matrix #56).
//!
//! The task is created only after an explicit user choice and removed idempotently. It starts
//! the controller at the current user's logon with the highest run level, an interactive token
//! (no stored password), and no execution time limit. Creating or deleting it requires an
//! elevated caller; [`ElevatedLogonTask::exists`] is a read-only query.
//!
//! The task belongs to one account: its principal and logon trigger are the SID of the account
//! that creates it. [`ElevatedLogonTask::for_user`] names it after the SID hash of that account
//! (`names::sid_hash16`), so each account has its own task and one account's choice never
//! replaces or deletes another's; such a task must be created by its own account.

use std::path::Path;

use windows::Win32::Foundation::{VARIANT_FALSE, VARIANT_TRUE};
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance};
use windows::Win32::System::TaskScheduler::{
    IExecAction, ILogonTrigger, ITaskFolder, ITaskService, TASK_ACTION_EXEC, TASK_CREATE_OR_UPDATE,
    TASK_INSTANCES_IGNORE_NEW, TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_HIGHEST,
    TASK_TRIGGER_LOGON, TaskScheduler,
};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, Interface};

use crate::com::ComApartment;
use crate::error::{E_INVALIDARG, ResultExt, WinError, WinResult, hresult_from_win32, win32};
use crate::names::SidHash;
use crate::sid::current_user_sid_string;

/// Longest accepted task name.
const MAX_NAME_LEN: usize = 200;

/// The elevated startup task of the current user.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ElevatedLogonTask {
    name: String,
}

impl Default for ElevatedLogonTask {
    fn default() -> Self {
        Self::new()
    }
}

/// A connected Task Scheduler. Field order matters: the interfaces are released before the
/// apartment guard uninitializes COM.
struct Scheduler {
    root: ITaskFolder,
    service: ITaskService,
    _apartment: ComApartment,
}

impl Scheduler {
    fn connect() -> WinResult<Self> {
        let apartment = ComApartment::sta()?;
        // SAFETY: COM is initialized on this thread for the lifetime of the result.
        let service: ITaskService =
            unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }
                .context("CoCreateInstance(TaskScheduler)")?;
        let empty = VARIANT::default();
        // SAFETY: empty VARIANTs connect to the local service as the current user.
        unsafe { service.Connect(&empty, &empty, &empty, &empty) }
            .context("ITaskService::Connect")?;
        // SAFETY: plain COM call.
        let root =
            unsafe { service.GetFolder(&BSTR::from("\\")) }.context("ITaskService::GetFolder")?;
        Ok(Self {
            root,
            service,
            _apartment: apartment,
        })
    }
}

fn is_missing(error: &windows::core::Error) -> bool {
    let code = error.code().0;
    code == hresult_from_win32(win32::ERROR_FILE_NOT_FOUND)
        || code == hresult_from_win32(win32::ERROR_PATH_NOT_FOUND)
}

impl ElevatedLogonTask {
    /// Name of the product's task.
    pub const NAME: &'static str = "Raumakey Elevated Startup";

    /// The product's task.
    pub fn new() -> Self {
        Self {
            name: Self::NAME.to_owned(),
        }
    }

    /// The product's task of the account whose SID hash is `user`:
    /// `Raumakey Elevated Startup <user>` in the root folder. [`Self::create`] registers it for
    /// the account of the calling process, so only that account may create its task.
    pub fn for_user(user: &SidHash) -> Self {
        Self {
            name: format!("{} {}", Self::NAME, user.as_str()),
        }
    }

    /// A task with another name in the root folder (tests and diagnostics). Names are 1..=200
    /// characters without `\ / : * ? " < > |` or control characters.
    pub fn with_name(name: &str) -> WinResult<Self> {
        let valid = !name.is_empty()
            && name.chars().count() <= MAX_NAME_LEN
            && !name.chars().any(|c| {
                c.is_control() || matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
            });
        if valid {
            Ok(Self {
                name: name.to_owned(),
            })
        } else {
            Err(WinError::new(E_INVALIDARG, "task name"))
        }
    }

    /// The task name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Whether the task is registered (read-only).
    pub fn exists(&self) -> WinResult<bool> {
        let scheduler = Scheduler::connect()?;
        // SAFETY: plain COM call.
        match unsafe { scheduler.root.GetTask(&BSTR::from(self.name.as_str())) } {
            Ok(_) => Ok(true),
            Err(error) if is_missing(&error) => Ok(false),
            Err(error) => Err(WinError::new(error.code().0, "ITaskFolder::GetTask")),
        }
    }

    /// Creates the task, or updates it when it exists: run `exe arguments` (in `exe`'s
    /// directory) at the current user's logon, with the highest run level, an interactive token,
    /// no time limit, also on battery, never twice at once. Requires elevation.
    pub fn create(&self, exe: &Path, arguments: &str) -> WinResult<()> {
        let exe_text = exe
            .to_str()
            .filter(|_| exe.is_absolute())
            .ok_or_else(|| WinError::new(E_INVALIDARG, "task executable"))?;
        let user = current_user_sid_string()?;
        let scheduler = Scheduler::connect()?;
        // SAFETY: plain COM call on the connected service.
        let definition =
            unsafe { scheduler.service.NewTask(0) }.context("ITaskService::NewTask")?;
        // SAFETY: plain COM calls on interfaces obtained from the connected service; every BSTR
        // and VARIANT outlives its call.
        unsafe {
            let info = definition.RegistrationInfo().context("RegistrationInfo")?;
            info.SetAuthor(&BSTR::from("Raumakey"))
                .context("SetAuthor")?;
            info.SetDescription(&BSTR::from(
                "Starts Raumakey with administrator rights at logon (explicit user choice).",
            ))
            .context("SetDescription")?;

            let principal = definition.Principal().context("Principal")?;
            principal
                .SetUserId(&BSTR::from(user.as_str()))
                .context("IPrincipal::SetUserId")?;
            principal
                .SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)
                .context("IPrincipal::SetLogonType")?;
            principal
                .SetRunLevel(TASK_RUNLEVEL_HIGHEST)
                .context("IPrincipal::SetRunLevel")?;

            let settings = definition.Settings().context("Settings")?;
            settings
                .SetExecutionTimeLimit(&BSTR::from("PT0S"))
                .context("SetExecutionTimeLimit")?;
            settings
                .SetDisallowStartIfOnBatteries(VARIANT_FALSE)
                .context("SetDisallowStartIfOnBatteries")?;
            settings
                .SetStopIfGoingOnBatteries(VARIANT_FALSE)
                .context("SetStopIfGoingOnBatteries")?;
            settings
                .SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW)
                .context("SetMultipleInstances")?;
            settings
                .SetAllowDemandStart(VARIANT_TRUE)
                .context("SetAllowDemandStart")?;
            settings
                .SetEnabled(VARIANT_TRUE)
                .context("ITaskSettings::SetEnabled")?;

            let trigger = definition
                .Triggers()
                .context("Triggers")?
                .Create(TASK_TRIGGER_LOGON)
                .context("ITriggerCollection::Create")?;
            let logon: ILogonTrigger = trigger.cast().context("ITrigger as ILogonTrigger")?;
            logon
                .SetUserId(&BSTR::from(user.as_str()))
                .context("ILogonTrigger::SetUserId")?;
            logon
                .SetEnabled(VARIANT_TRUE)
                .context("ITrigger::SetEnabled")?;

            let action = definition
                .Actions()
                .context("Actions")?
                .Create(TASK_ACTION_EXEC)
                .context("IActionCollection::Create")?;
            let exec: IExecAction = action.cast().context("IAction as IExecAction")?;
            exec.SetPath(&BSTR::from(exe_text))
                .context("IExecAction::SetPath")?;
            exec.SetArguments(&BSTR::from(arguments))
                .context("IExecAction::SetArguments")?;
            if let Some(dir) = exe.parent().and_then(Path::to_str) {
                exec.SetWorkingDirectory(&BSTR::from(dir))
                    .context("IExecAction::SetWorkingDirectory")?;
            }

            scheduler
                .root
                .RegisterTaskDefinition(
                    &BSTR::from(self.name.as_str()),
                    &definition,
                    TASK_CREATE_OR_UPDATE.0,
                    &VARIANT::from(user.as_str()),
                    &VARIANT::default(),
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &VARIANT::default(),
                )
                .context("ITaskFolder::RegisterTaskDefinition")?;
        }
        Ok(())
    }

    /// Deletes the task; returns whether it existed. Deleting a missing task succeeds.
    pub fn delete(&self) -> WinResult<bool> {
        let scheduler = Scheduler::connect()?;
        // SAFETY: plain COM call.
        match unsafe {
            scheduler
                .root
                .DeleteTask(&BSTR::from(self.name.as_str()), 0)
        } {
            Ok(()) => Ok(true),
            Err(error) if is_missing(&error) => Ok(false),
            Err(error) => Err(WinError::new(error.code().0, "ITaskFolder::DeleteTask")),
        }
    }
}
