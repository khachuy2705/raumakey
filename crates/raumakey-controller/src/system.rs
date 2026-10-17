//! The real side effects of the commands ([`PlatformSystem`]): thin wrappers around
//! `raumakey-windows`, which owns every Win32 and COM call.
//!
//! On Windows, [`WinSystem`] maps each side effect to one wrapper:
//!
//! | Side effect | Wrapper |
//! |---|---|
//! | elevation check, elevated restart | `sid::current_process_is_elevated`, `process::relaunch_elevated` (`runas`) |
//! | regsvr32 of the TIP's bitness | `process::run_regsvr32` (`/s`, `/u`), [`REGSVR32_TIMEOUT`] |
//! | registration read-back | `tsf_registration::WinRegistrationReader` (read-only, both views) |
//! | installation record | [`RegistryRecordStore`] over the registry write guard |
//! | Startup-folder shortcut | `known_folder::startup_folder`, `shell::create_shortcut`, `shell::remove_shortcut` |
//! | the account of this process | `sid::current_user_sid_hash` |
//! | elevated logon task of an account | `task_scheduler::ElevatedLogonTask::for_user` |
//! | folders only administrators can change | `known_folder::program_files`, `known_folder::program_files_x86`, [`is_inside_folder`] |
//! | clipboard | `clipboard::WinClipboard` |
//!
//! A file counts as protected from the account's own programs when it lies inside
//! `Program Files` or `Program Files (x86)`, whose default permissions let only administrators,
//! the system, and the installer service change them. The check is lexical
//! ([`is_inside_folder`]): it opens nothing and reads no permissions, so a folder whose
//! permissions an administrator widened still counts as protected, and a path in another form
//! (a short `PROGRA~1` name, `\\?\`) counts as unprotected. A 32-bit `raumakey.exe` on 64-bit
//! Windows sees only `Program Files (x86)`.
//!
//! Only the record store and [`is_inside_folder`] are tested here, the store against a test key
//! of its own below `HKCU\Software\Raumakey`. The rest changes the machine (HKLM, COM/TSF
//! registration, UAC prompts, the user's Startup folder, scheduled tasks, the real clipboard)
//! and is checked by hand: see the manual checks of [`crate::install`] and [`crate::startup`],
//! and the ignored manual tests of `raumakey-windows`. A successful build is no evidence that
//! this glue works on Windows.
//!
//! Elsewhere, [`PlatformSystem`] fails every side effect with `E_NOTIMPL`, so the commands
//! that need Windows fail cleanly while dry runs, `convert --in/--out`, and the messages work.

#[cfg(not(windows))]
pub use other::UnsupportedSystem as PlatformSystem;
#[cfg(windows)]
pub use win::{
    ELEVATED_TIMEOUT, REGSVR32_TIMEOUT, RegistryRecordStore, SHORTCUT_DESCRIPTION,
    SHORTCUT_FILE_NAME, WinSystem, WinSystem as PlatformSystem,
};

/// The side effects of this platform for the `raumakey.exe` at `exe`.
pub fn platform(exe: std::path::PathBuf) -> PlatformSystem {
    PlatformSystem::new(exe)
}

/// Whether `path` names an entry inside `folder` (not `folder` itself). Both must be absolute
/// paths on a drive (`X:\…`); they are compared as text, the way Windows compares them: ASCII
/// case and `/` against `\` do not matter. A path with an empty, `.`, or `..` component below
/// `folder`, or a component that Windows reduces to one of these (`...`, `. `), is never
/// inside, nor is a path in another form (`\\?\`, UNC, a short name). Pure: nothing is
/// opened.
pub fn is_inside_folder(path: &std::path::Path, folder: &std::path::Path) -> bool {
    let (Some(path), Some(folder)) = (path.to_str(), folder.to_str()) else {
        return false;
    };
    let normalize = |text: &str| text.replace('/', "\\").to_ascii_lowercase();
    let path = normalize(path);
    let folder = normalize(folder);
    let folder = folder.trim_end_matches('\\');
    if !is_drive_path(folder) {
        return false;
    }
    let Some(rest) = path
        .strip_prefix(folder)
        .and_then(|rest| rest.strip_prefix('\\'))
    else {
        return false;
    };
    !rest.is_empty()
        && rest
            .split('\\')
            .all(|component| !component.trim_end_matches(['.', ' ']).is_empty())
}

/// `X:` alone or followed by `\…`: an absolute path on a drive.
fn is_drive_path(text: &str) -> bool {
    match text.as_bytes() {
        [drive, b':'] | [drive, b':', b'\\', ..] => drive.is_ascii_alphabetic(),
        _ => false,
    }
}

/// For a subcommand of the GUI-subsystem `raumakey.exe`: when standard output or standard
/// error is missing, attaches to the console of the parent process, so that the output and the
/// exit code reach the caller. Redirected output keeps its target. Does nothing elsewhere.
pub fn attach_console() {
    #[cfg(windows)]
    {
        // Without a console the output is lost, which does not change what the command does.
        let _ = raumakey_windows::process::attach_parent_console();
    }
}

#[cfg(windows)]
mod win {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use raumakey_converter::ClipboardPort;
    use raumakey_model::identity::Guid;
    use raumakey_windows::clipboard::WinClipboard;
    use raumakey_windows::error::{E_ACCESSDENIED, WinError, WinResult};
    use raumakey_windows::known_folder::{program_files, program_files_x86, startup_folder};
    use raumakey_windows::names::SidHash;
    use raumakey_windows::process::{relaunch_elevated, run_regsvr32};
    use raumakey_windows::registry::{
        OwnedRegKey, RegKey, RegRoot, RegValue, RegView, delete_empty_key, delete_tree,
        is_owned_path,
    };
    use raumakey_windows::shell::{ShortcutSpec, create_shortcut, remove_shortcut};
    use raumakey_windows::sid::{current_process_is_elevated, current_user_sid_hash};
    use raumakey_windows::task_scheduler::ElevatedLogonTask;
    use raumakey_windows::tsf_registration::{
        ComClassState, RegistrationReader, TipArch, TipBinary, WinRegistrationReader,
    };

    use super::is_inside_folder;
    use crate::commands::System;
    use crate::desktop::WinDesktop;
    use crate::elevation::Elevation;
    use crate::install::{INSTALL_RECORD_KEY, InstallRecord, InstallSystem, tip_value_name};
    use crate::run::Desktop;
    use crate::startup::StartupSystem;

    /// Time limit of one regsvr32 run. A run that takes longer keeps running (it is never
    /// killed mid-registration) and counts as failed with an unknown outcome.
    pub const REGSVR32_TIMEOUT: Duration = Duration::from_secs(60);
    /// Time limit of an elevated restart, counted from the answer to the UAC prompt.
    pub const ELEVATED_TIMEOUT: Duration = Duration::from_secs(600);
    /// File name of the shortcut in the user's Startup folder.
    pub const SHORTCUT_FILE_NAME: &str = "Raumakey.lnk";
    /// Description of the shortcut (its Explorer tooltip): "Raumakey - Vietnamese input method".
    pub const SHORTCUT_DESCRIPTION: &str = "Raumakey - bộ gõ tiếng Việt";

    /// The installation record in the registry: one string value per [`InstallRecord`] field
    /// (`Version`, `TipX64`, `TipX86`, `TipArm64`), written only through the registry write
    /// guard, so only a key below `Software\Raumakey` can be written.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct RegistryRecordStore {
        root: RegRoot,
        view: RegView,
        path: String,
    }

    impl RegistryRecordStore {
        /// `HKLM\Software\Raumakey\Install` in the 64-bit view (on 32-bit Windows, which has
        /// one view, the flag is ignored).
        pub fn machine() -> Self {
            Self::at(RegRoot::LocalMachine, RegView::Wow64_64, INSTALL_RECORD_KEY)
        }

        /// A record at another key (tests use one below `HKCU\Software\Raumakey`).
        pub fn at(root: RegRoot, view: RegView, path: &str) -> Self {
            Self {
                root,
                view,
                path: path.to_owned(),
            }
        }

        /// Reads the record; `Ok(None)` when the key does not exist. Values of another type
        /// than `REG_SZ` are ignored.
        pub fn read(&self) -> WinResult<Option<InstallRecord>> {
            let Some(key) = RegKey::open_read(self.root, self.view, &self.path)? else {
                return Ok(None);
            };
            let mut failure: Option<WinError> = None;
            let record = InstallRecord::from_values(|name| match key.get_value(name) {
                Ok(Some(RegValue::String(text))) => Some(text),
                Ok(_) => None,
                Err(error) => {
                    failure.get_or_insert(error);
                    None
                }
            });
            match failure {
                Some(error) => Err(error),
                None => Ok(Some(record)),
            }
        }

        /// Writes the record: creates the key, sets every value of the record, and deletes the
        /// path values of the architectures the record does not have.
        pub fn write(&self, record: &InstallRecord) -> WinResult<()> {
            let key = OwnedRegKey::create(self.root, self.view, &self.path)?;
            for (name, value) in record.to_values() {
                key.set_string(name, &value)?;
            }
            for arch in TipArch::ALL {
                if record.tip_path(arch).is_none() {
                    key.delete_value(tip_value_name(arch))?;
                }
            }
            Ok(())
        }

        /// Deletes the record key, then its parent when that is an owned key left empty
        /// (`HKLM\Software\Raumakey` after an uninstall). Returns whether the record existed.
        pub fn remove(&self) -> WinResult<bool> {
            let existed = delete_tree(self.root, self.view, &self.path)?;
            if let Some((parent, _)) = self.path.rsplit_once('\\')
                && is_owned_path(self.root, parent)
            {
                delete_empty_key(self.root, self.view, parent)?;
            }
            Ok(existed)
        }
    }

    /// Whether `path` lies inside `Program Files` or `Program Files (x86)` (see the module
    /// documentation). A folder that cannot be looked up protects nothing.
    fn is_protected(path: &Path) -> bool {
        [program_files(), program_files_x86()]
            .into_iter()
            .flatten()
            .any(|folder| is_inside_folder(path, &folder))
    }

    /// The side effects on this Windows machine, for the `raumakey.exe` at `exe`.
    #[derive(Debug)]
    pub struct WinSystem {
        exe: PathBuf,
        record: RegistryRecordStore,
        clipboard: WinClipboard,
        reader: WinRegistrationReader,
        desktop: WinDesktop,
    }

    impl WinSystem {
        /// The side effects for the `raumakey.exe` at `exe` (an absolute path).
        pub fn new(exe: PathBuf) -> Self {
            Self {
                exe,
                record: RegistryRecordStore::machine(),
                clipboard: WinClipboard::new(),
                reader: WinRegistrationReader,
                desktop: WinDesktop::new(),
            }
        }

        fn shortcut_path() -> WinResult<PathBuf> {
            Ok(startup_folder()?.join(SHORTCUT_FILE_NAME))
        }

        /// The shortcut starts `raumakey.exe` without arguments (the controller) in its own
        /// folder, with its own icon.
        fn shortcut_spec(&self) -> ShortcutSpec {
            ShortcutSpec {
                target: self.exe.clone(),
                arguments: String::new(),
                working_dir: self.exe.parent().map(Path::to_path_buf),
                description: SHORTCUT_DESCRIPTION.to_owned(),
                icon: Some((self.exe.clone(), 0)),
            }
        }
    }

    impl Elevation for WinSystem {
        fn is_elevated(&mut self) -> WinResult<bool> {
            current_process_is_elevated()
        }

        fn run_elevated(&mut self, args: &[String]) -> WinResult<u32> {
            let args: Vec<&str> = args.iter().map(String::as_str).collect();
            relaunch_elevated(&self.exe, &args, ELEVATED_TIMEOUT)
        }
    }

    impl RegistrationReader for WinSystem {
        fn com_class(&mut self, view: RegView) -> Result<Option<ComClassState>, WinError> {
            self.reader.com_class(view)
        }

        fn profile_present(&mut self, view: RegView) -> Result<bool, WinError> {
            self.reader.profile_present(view)
        }

        fn category_present(&mut self, view: RegView, category: Guid) -> Result<bool, WinError> {
            self.reader.category_present(view, category)
        }
    }

    impl InstallSystem for WinSystem {
        fn file_exists(&mut self, path: &Path) -> bool {
            path.is_file()
        }

        fn regsvr32(&mut self, tip: &TipBinary, unregister: bool) -> WinResult<u32> {
            run_regsvr32(&tip.path, tip.arch, unregister, REGSVR32_TIMEOUT)
        }

        fn read_record(&mut self) -> WinResult<Option<InstallRecord>> {
            self.record.read()
        }

        fn write_record(&mut self, record: &InstallRecord) -> WinResult<()> {
            self.record.write(record)
        }

        fn remove_record(&mut self) -> WinResult<bool> {
            self.record.remove()
        }

        fn is_protected_location(&mut self, path: &Path) -> bool {
            is_protected(path)
        }
    }

    impl StartupSystem for WinSystem {
        fn current_user(&mut self) -> WinResult<SidHash> {
            current_user_sid_hash()
        }

        fn create_shortcut(&mut self) -> WinResult<()> {
            create_shortcut(&Self::shortcut_path()?, &self.shortcut_spec())
        }

        fn remove_shortcut(&mut self) -> WinResult<bool> {
            remove_shortcut(&Self::shortcut_path()?)
        }

        fn task_exists(&mut self, user: &SidHash) -> WinResult<bool> {
            ElevatedLogonTask::for_user(user).exists()
        }

        fn create_task(&mut self, user: &SidHash) -> WinResult<()> {
            // The task runs as the account of this process (its principal and logon trigger),
            // so it may only carry the name of that account.
            if current_user_sid_hash()? != *user {
                return Err(WinError::new(
                    E_ACCESSDENIED,
                    "elevated startup task of another account",
                ));
            }
            ElevatedLogonTask::for_user(user).create(&self.exe, "")
        }

        fn delete_task(&mut self, user: &SidHash) -> WinResult<bool> {
            ElevatedLogonTask::for_user(user).delete()
        }

        fn program_is_protected(&mut self) -> bool {
            is_protected(&self.exe)
        }
    }

    impl System for WinSystem {
        fn clipboard(&mut self) -> &mut dyn ClipboardPort {
            &mut self.clipboard
        }

        fn desktop(&mut self) -> &mut dyn Desktop {
            &mut self.desktop
        }
    }
}

#[cfg(not(windows))]
mod other {
    use std::path::{Path, PathBuf};

    use raumakey_converter::{ClipboardError, ClipboardPort, ClipboardText};
    use raumakey_model::identity::Guid;
    use raumakey_windows::error::{E_NOTIMPL, WinError, WinResult};
    use raumakey_windows::names::SidHash;
    use raumakey_windows::registry::RegView;
    use raumakey_windows::tsf_registration::{ComClassState, RegistrationReader, TipBinary};

    use crate::commands::System;
    use crate::elevation::Elevation;
    use crate::install::{InstallRecord, InstallSystem};
    use crate::run::Desktop;
    use crate::startup::StartupSystem;
    use crate::tray::InputMode;

    /// Every side effect fails: this is not Windows.
    const UNSUPPORTED: WinError = WinError::new(E_NOTIMPL, "Raumakey side effect (Windows only)");

    /// A desktop without a notification area: the controller cannot run, and says so on
    /// standard error.
    #[derive(Debug, Default)]
    struct NoDesktop;

    impl Desktop for NoDesktop {
        fn acquire_instance(&mut self) -> WinResult<bool> {
            Err(UNSUPPORTED)
        }

        fn run_tray(&mut self, _mode: InputMode) -> WinResult<i32> {
            Err(UNSUPPORTED)
        }

        fn release_instance(&mut self) {}

        fn report_error(&mut self, text: &str) {
            eprintln!("raumakey: {text}");
        }
    }

    /// A clipboard that is never available.
    #[derive(Debug, Default)]
    struct NoClipboard;

    impl ClipboardPort for NoClipboard {
        fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError> {
            Err(ClipboardError::Unavailable)
        }

        fn write_text_if_unchanged(
            &mut self,
            _expected_sequence: u32,
            _units: &[u16],
        ) -> Result<bool, ClipboardError> {
            Err(ClipboardError::Unavailable)
        }
    }

    /// The side effects outside Windows: none is supported.
    #[derive(Debug)]
    pub struct UnsupportedSystem {
        clipboard: NoClipboard,
        desktop: NoDesktop,
    }

    impl UnsupportedSystem {
        /// The (unsupported) side effects for the executable at `exe`.
        pub fn new(exe: PathBuf) -> Self {
            let _ = exe;
            Self {
                clipboard: NoClipboard,
                desktop: NoDesktop,
            }
        }
    }

    impl Elevation for UnsupportedSystem {
        fn is_elevated(&mut self) -> WinResult<bool> {
            Err(UNSUPPORTED)
        }

        fn run_elevated(&mut self, _args: &[String]) -> WinResult<u32> {
            Err(UNSUPPORTED)
        }
    }

    impl RegistrationReader for UnsupportedSystem {
        fn com_class(&mut self, _view: RegView) -> Result<Option<ComClassState>, WinError> {
            Err(UNSUPPORTED)
        }

        fn profile_present(&mut self, _view: RegView) -> Result<bool, WinError> {
            Err(UNSUPPORTED)
        }

        fn category_present(&mut self, _view: RegView, _category: Guid) -> Result<bool, WinError> {
            Err(UNSUPPORTED)
        }
    }

    impl InstallSystem for UnsupportedSystem {
        fn file_exists(&mut self, path: &Path) -> bool {
            path.is_file()
        }

        fn regsvr32(&mut self, _tip: &TipBinary, _unregister: bool) -> WinResult<u32> {
            Err(UNSUPPORTED)
        }

        fn read_record(&mut self) -> WinResult<Option<InstallRecord>> {
            Err(UNSUPPORTED)
        }

        fn write_record(&mut self, _record: &InstallRecord) -> WinResult<()> {
            Err(UNSUPPORTED)
        }

        fn remove_record(&mut self) -> WinResult<bool> {
            Err(UNSUPPORTED)
        }

        fn is_protected_location(&mut self, _path: &Path) -> bool {
            false
        }
    }

    impl StartupSystem for UnsupportedSystem {
        fn current_user(&mut self) -> WinResult<SidHash> {
            Err(UNSUPPORTED)
        }

        fn create_shortcut(&mut self) -> WinResult<()> {
            Err(UNSUPPORTED)
        }

        fn remove_shortcut(&mut self) -> WinResult<bool> {
            Err(UNSUPPORTED)
        }

        fn task_exists(&mut self, _user: &SidHash) -> WinResult<bool> {
            Err(UNSUPPORTED)
        }

        fn create_task(&mut self, _user: &SidHash) -> WinResult<()> {
            Err(UNSUPPORTED)
        }

        fn delete_task(&mut self, _user: &SidHash) -> WinResult<bool> {
            Err(UNSUPPORTED)
        }

        fn program_is_protected(&mut self) -> bool {
            false
        }
    }

    impl System for UnsupportedSystem {
        fn clipboard(&mut self) -> &mut dyn ClipboardPort {
            &mut self.clipboard
        }

        fn desktop(&mut self) -> &mut dyn Desktop {
            &mut self.desktop
        }
    }
}
