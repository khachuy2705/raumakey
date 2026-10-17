//! `install` and `uninstall` (plan §10.2, §10.4; design §12, §13; clean-room spec §6.1).
//!
//! The TIP registers itself: its `DllRegisterServer` writes the COM class, the language
//! profile, and the categories of the view that matches its bitness, and undoes its own changes
//! when a step fails (`raumakey_windows::tsf_registration::execute_registration`). The
//! controller runs it through the regsvr32 of that bitness and owns everything around it, behind
//! the [`InstallSystem`] side effects.
//!
//! # install
//!
//! 1. The TIP paths are checked by `plan_install` (absolute Windows paths of `.dll` files that
//!    COM can load). A path it rejects is a usage error.
//! 2. Every TIP file must exist. Checking needs no administrator rights, so a mistyped path or
//!    an incomplete package costs no UAC prompt; the elevated restart checks again.
//! 3. A TIP outside the folders that only administrators can change, such as Program Files
//!    ([`InstallSystem::is_protected_location`]), gets a warning before the UAC prompt (the
//!    elevated restart does not repeat it). Every application loads the TIP, also applications
//!    that run as administrator, so a TIP that the account's own programs can replace lets them
//!    run code there. Installing goes on: the package is installed from wherever it was
//!    extracted, and whether to require Program Files is a product decision not made yet.
//! 4. Without administrator rights the command restarts elevated ([`crate::elevation`]).
//! 5. Before anything changes, the installation record and the current registration of each
//!    view are read: the run restores them if it fails.
//! 6. Each TIP, x64 first, is registered with `regsvr32 /s` of its bitness. Its exit code is
//!    never taken as proof of registration.
//! 7. Every registration is read back (`verify_with`): COM class, `InprocServer32`,
//!    `ThreadingModel`, profile, and the seven categories of each view.
//! 8. The installation record ([`InstallRecord`]) is written to `HKLM\`[`INSTALL_RECORD_KEY`]
//!    in the 64-bit view, through the registry write guard of `raumakey-windows`.
//!
//! When step 6, 7, or 8 fails, the run undoes only its own changes, last change first. The
//! previous record is written back (or the new one deleted). Each TIP this run registered gets
//! back the registration its view had before the run: a view that had none is unregistered
//! (`regsvr32 /s /u`); a view registered from the same DLL is left as it is; a view registered
//! from another DLL is registered again from that DLL. Every view restored this way is read
//! back (`verify_removal_with`, or `verify_with` for the other DLL): regsvr32's exit code is no
//! proof here either. The TIP whose regsvr32 failed after it may have run (it exited with an
//! error, or waiting for it or reading its exit code failed) has already undone its own
//! changes, but its DLL ignores errors while doing so, so its view is read back and restored the
//! same way when it differs from before the run. The rollback is incomplete
//! ([`EXIT_INCOMPLETE`]) when one of these steps fails, when a restored view does not read back
//! as before, when that other DLL no longer exists, or when a regsvr32 run did not finish in
//! time: that regsvr32 keeps running, so its view is left alone and its outcome is unknown.
//!
//! # uninstall
//!
//! It works on both package architectures ([`PACKAGE_ARCHS`]). A view without any Raumakey
//! entry is left alone. Otherwise the TIP is unregistered with `regsvr32 /s /u` of its bitness,
//! using the DLL named in the installation record or, when that file is gone, the one next to
//! `raumakey.exe` ([`default_tip_path`]); any Raumakey TIP of that bitness removes the same
//! entries. The removal is read back (`verify_removal_with`), and only when every entry is gone
//! is the installation record deleted (with `HKLM\Software\Raumakey` when it is then empty).
//! An uninstall that fails part-way keeps the record and can be run again; it has nothing to
//! roll back. The startup shortcut and the elevated logon task are not part of the installation
//! and are left alone (`startup none` removes them).
//!
//! # Dry runs
//!
//! [`print_install_plan`] and [`print_uninstall_plan`] take no [`InstallSystem`]: they print
//! the plan from the arguments alone and cannot change anything. They do not check that the
//! TIP files exist, whether they lie in a protected folder (no warning), or that the process
//! could be elevated. Plan lines name Raumakey's CLSID and
//! profile GUID and the Windows SDK category GUIDs, never another product's identity.
//!
//! # Manual checks (Windows, elevated, on a test machine)
//!
//! The registry and process side effects (`crate::system`) are thin wrappers that only a real
//! machine can check; a successful build is no evidence that they work:
//!
//! 1. `raumakey.exe install` from a non-elevated console shows one UAC prompt, and declining it
//!    exits with 1 without changes.
//! 2. After an install, both views hold the CLSID and `InprocServer32` of their TIP, TSF lists
//!    the Raumakey profile for Vietnamese, and `HKLM\Software\Raumakey\Install` holds
//!    `Version`, `TipX64`, and `TipX86`.
//! 3. An install with a broken x86 DLL (for example a copy of a text file named
//!    `raumakey_tip.dll`) exits with 1 and leaves both views as they were.
//! 4. `raumakey.exe uninstall` removes every entry of both views and the record; running it
//!    again prints "not registered" for both views and "No installation record", and exits
//!    with 0.
//! 5. An install from a folder outside Program Files (such as `Downloads`) prints the warning
//!    for both TIPs before the UAC prompt; an install of a package copied into Program Files
//!    prints none.

use std::path::{Path, PathBuf};

use raumakey_windows::error::{WinErrorKind, WinResult};
use raumakey_windows::tsf_registration::{
    PlanError, RegistrationReader, TipArch, TipBinary, plan_install, plan_uninstall,
    verify_removal_with, verify_with,
};

use crate::cli::{EXIT_FAILED, EXIT_INCOMPLETE, EXIT_OK, EXIT_USAGE};
use crate::elevation::{Elevation, NOTHING_CHANGED, Privilege, ensure_elevated};
use crate::output::Output;

/// File name of the TIP DLL.
pub const TIP_FILE_NAME: &str = "raumakey_tip.dll";
/// Key of the installation record below `HKEY_LOCAL_MACHINE` (64-bit view).
pub const INSTALL_RECORD_KEY: &str = "Software\\Raumakey\\Install";
/// The architectures of the TIPs in the x64 package: `x64\raumakey_tip.dll` and
/// `x86\raumakey_tip.dll` next to `raumakey.exe`.
pub const PACKAGE_ARCHS: [TipArch; 2] = [TipArch::X64, TipArch::X86];
/// Name of the record value that holds the product version.
pub const VERSION_VALUE: &str = "Version";
/// Version written to the installation record.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The TIP of `arch` in the package layout: `<exe_dir>\<arch>\raumakey_tip.dll`.
pub fn default_tip_path(exe_dir: &Path, arch: TipArch) -> PathBuf {
    exe_dir.join(arch.name()).join(TIP_FILE_NAME)
}

/// Name of the record value that holds the path of the TIP of `arch`.
pub const fn tip_value_name(arch: TipArch) -> &'static str {
    match arch {
        TipArch::X64 => "TipX64",
        TipArch::X86 => "TipX86",
        TipArch::Arm64 => "TipArm64",
    }
}

/// The installation record: what `install` registered (`HKLM\Software\Raumakey\Install`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstallRecord {
    /// Version of the controller that wrote it (empty when the value is missing).
    pub version: String,
    /// The registered TIPs, at most one per architecture.
    pub tips: Vec<TipBinary>,
}

impl InstallRecord {
    /// The record of an installation of `tips` by this version.
    pub fn new(tips: &[TipBinary]) -> Self {
        Self {
            version: PRODUCT_VERSION.to_owned(),
            tips: tips.to_vec(),
        }
    }

    /// The recorded TIP of `arch`.
    pub fn tip_path(&self, arch: TipArch) -> Option<&Path> {
        self.tips
            .iter()
            .find(|tip| tip.arch == arch)
            .map(|tip| tip.path.as_path())
    }

    /// The registry values: [`VERSION_VALUE`], then one [`tip_value_name`] per recorded
    /// architecture (x64, x86, ARM64), all strings.
    pub fn to_values(&self) -> Vec<(&'static str, String)> {
        let mut values = vec![(VERSION_VALUE, self.version.clone())];
        for arch in TipArch::ALL {
            if let Some(path) = self.tip_path(arch) {
                values.push((tip_value_name(arch), path.to_string_lossy().into_owned()));
            }
        }
        values
    }

    /// The record read through `lookup`, which returns the string value of a name. Missing
    /// values are left out; an empty path counts as missing.
    pub fn from_values(mut lookup: impl FnMut(&str) -> Option<String>) -> Self {
        let version = lookup(VERSION_VALUE).unwrap_or_default();
        let tips = TipArch::ALL
            .into_iter()
            .filter_map(|arch| {
                lookup(tip_value_name(arch))
                    .filter(|path| !path.is_empty())
                    .map(|path| TipBinary {
                        path: PathBuf::from(path),
                        arch,
                    })
            })
            .collect();
        Self { version, tips }
    }
}

/// The side effects of `install` and `uninstall`. The registry reads of
/// [`RegistrationReader`] are read-only.
pub trait InstallSystem: Elevation + RegistrationReader {
    /// Whether `path` names an existing file.
    fn file_exists(&mut self, path: &Path) -> bool;
    /// Runs `regsvr32 /s [/u] <tip>` with the regsvr32 of the TIP's bitness, waits, and returns
    /// its exit code.
    fn regsvr32(&mut self, tip: &TipBinary, unregister: bool) -> WinResult<u32>;
    /// Reads the installation record; `Ok(None)` when there is none.
    fn read_record(&mut self) -> WinResult<Option<InstallRecord>>;
    /// Writes (replaces) the installation record.
    fn write_record(&mut self, record: &InstallRecord) -> WinResult<()>;
    /// Deletes the installation record; returns whether it existed.
    fn remove_record(&mut self) -> WinResult<bool>;
    /// Whether `path` lies in a folder that only administrators and the system can change,
    /// such as Program Files. A path that cannot be checked counts as outside.
    fn is_protected_location(&mut self, path: &Path) -> bool;
}

/// Compares two DLL paths ignoring case and separator style, as Windows does.
fn same_path(first: &Path, second: &Path) -> bool {
    let normalize = |path: &Path| path.to_string_lossy().replace('/', "\\").to_lowercase();
    normalize(first) == normalize(second)
}

/// Reports a TIP path that `plan_install` rejected.
fn plan_error(tips: &[TipBinary], error: PlanError, output: &mut Output<'_>) -> u8 {
    let arch = match error {
        PlanError::NoBinaries => None,
        PlanError::DuplicateArch(arch)
        | PlanError::RelativePath(arch)
        | PlanError::NotADll(arch)
        | PlanError::InvalidPath(arch) => Some(arch),
    };
    match arch.and_then(|arch| tips.iter().find(|tip| tip.arch == arch)) {
        Some(tip) => output.error(format_args!("install: {error}: {}", tip.path.display())),
        None => output.error(format_args!("install: {error}")),
    }
    EXIT_USAGE
}

/// `install --dry-run`: prints the registration plan of `tips` and changes nothing.
pub fn print_install_plan(tips: &[TipBinary], output: &mut Output<'_>) -> u8 {
    let plan = match plan_install(tips) {
        Ok(plan) => plan,
        Err(error) => return plan_error(tips, error, output),
    };
    output.line("Raumakey install plan (dry run: nothing is changed)");
    output.line(
        "Each TIP is registered by the regsvr32 of its bitness (regsvr32 /s <dll>), which runs \
         the TIP's DllRegisterServer:",
    );
    for tip in tips {
        output.line(format_args!("  [{}] {}", tip.arch, tip.path.display()));
    }
    output.line("Registration steps:");
    for line in plan.describe() {
        output.line(format_args!("  {line}"));
    }
    output.line("Then:");
    output.line(format_args!(
        "  write the installation record HKLM\\{INSTALL_RECORD_KEY} (64-bit view): {VERSION_VALUE} \
         and the path of each TIP"
    ));
    output.line(
        "Installing needs administrator rights. If a step fails, the changes of the run are \
         undone.",
    );
    EXIT_OK
}

/// `uninstall --dry-run`: prints the removal plan of both package architectures and changes
/// nothing.
pub fn print_uninstall_plan(output: &mut Output<'_>) -> u8 {
    let plan = plan_uninstall(&PACKAGE_ARCHS);
    output.line("Raumakey uninstall plan (dry run: nothing is changed)");
    output.line(
        "Each registered TIP is unregistered by the regsvr32 of its bitness (regsvr32 /s /u \
         <dll>), which runs the TIP's DllUnregisterServer. The DLL is the one named in the \
         installation record or, when that file is gone, the one next to raumakey.exe.",
    );
    output.line("Removal steps:");
    for line in plan.describe() {
        output.line(format_args!("  {line}"));
    }
    output.line("Then, when every entry is gone:");
    output.line(format_args!(
        "  delete the installation record HKLM\\{INSTALL_RECORD_KEY} (64-bit view)"
    ));
    output.line(
        "Uninstalling needs administrator rights. Only Raumakey's entries are removed; a view \
         without them is left alone.",
    );
    EXIT_OK
}

/// The command line of an elevated restart of `install` (without `--elevated`).
fn install_args(tips: &[TipBinary]) -> Vec<String> {
    let mut args = vec!["install".to_owned()];
    for tip in tips {
        let option = match tip.arch {
            TipArch::X64 => "--tip-x64",
            TipArch::X86 => "--tip-x86",
            // The command line has no ARM64 option, so no ARM64 TIP reaches this point.
            TipArch::Arm64 => continue,
        };
        args.push(option.to_owned());
        // `plan_install` accepts only Unicode paths, so nothing is lost here.
        args.push(tip.path.to_string_lossy().into_owned());
    }
    args
}

/// `install`: registers `tips` (absolute paths; one per architecture), verifies them, and
/// writes the installation record (see the module documentation).
pub fn install<S: InstallSystem + ?Sized>(
    system: &mut S,
    tips: &[TipBinary],
    already_restarted: bool,
    output: &mut Output<'_>,
) -> u8 {
    if let Err(error) = plan_install(tips) {
        return plan_error(tips, error, output);
    }
    // Nothing has changed yet, and checking the files needs no administrator rights: a missing
    // file costs no UAC prompt. The elevated restart runs this function again, so the files
    // are checked again right before anything changes.
    let missing: Vec<&TipBinary> = tips
        .iter()
        .filter(|tip| !system.file_exists(&tip.path))
        .collect();
    if !missing.is_empty() {
        for tip in missing {
            output.error(format_args!(
                "install: the {} TIP file does not exist: {}",
                tip.arch,
                tip.path.display()
            ));
        }
        output.detail(NOTHING_CHANGED);
        return EXIT_FAILED;
    }
    if !already_restarted {
        warn_about_unprotected_tips(system, tips, output);
    }
    if let Privilege::Done(code) = ensure_elevated(
        system,
        &install_args(tips),
        already_restarted,
        NOTHING_CHANGED,
        output,
    ) {
        return code;
    }

    let previous_record = match system.read_record() {
        Ok(record) => record,
        Err(error) => {
            output.error(format_args!(
                "install: the installation record HKLM\\{INSTALL_RECORD_KEY} could not be read \
                 ({error}); nothing was changed"
            ));
            return EXIT_FAILED;
        }
    };
    let mut previous = Vec::with_capacity(tips.len());
    for tip in tips {
        match system.com_class(tip.arch.registry_view()) {
            Ok(state) => previous.push(
                state
                    .and_then(|state| state.inproc_server)
                    .map(PathBuf::from),
            ),
            Err(error) => {
                output.error(format_args!(
                    "install: the current {} registration could not be read ({error}); nothing \
                     was changed",
                    tip.arch
                ));
                return EXIT_FAILED;
            }
        }
    }

    let mut run = Run {
        tips,
        previous: &previous,
        registered: Vec::new(),
        failed: None,
        unknown: None,
        record_changed: false,
    };
    let failed = !run.register(system, output)
        || !run.verify(system, output)
        || !run.write_record(system, output);
    if !failed {
        output.line(format_args!(
            "Recorded the installation in HKLM\\{INSTALL_RECORD_KEY}."
        ));
        output.line("Raumakey is installed.");
        return EXIT_OK;
    }
    let complete = run.roll_back(system, previous_record.as_ref(), output);
    if complete {
        output.detail("the changes of this run were undone");
        EXIT_FAILED
    } else {
        output.detail(
            "the changes of this run could not all be undone: Raumakey may be partly registered; \
             run install or uninstall again",
        );
        EXIT_INCOMPLETE
    }
}

/// Warns about each TIP outside the folders that only administrators can change (see the
/// module documentation). Installing goes on.
fn warn_about_unprotected_tips<S: InstallSystem + ?Sized>(
    system: &mut S,
    tips: &[TipBinary],
    output: &mut Output<'_>,
) {
    let mut warned = false;
    for tip in tips {
        if !system.is_protected_location(&tip.path) {
            output.error(format_args!(
                "warning: the {} TIP is not in a folder that only administrators can change, \
                 such as Program Files: {}",
                tip.arch,
                tip.path.display()
            ));
            warned = true;
        }
    }
    if warned {
        output.detail(
            "Every application loads the TIP, also applications that run as administrator, so \
             a program that can replace the file could run its code there. Copy Raumakey into \
             Program Files and install it from there.",
        );
    }
}

/// The changes of one `install` run, for its rollback.
struct Run<'a> {
    tips: &'a [TipBinary],
    /// `InprocServer32` of each TIP's view before the run.
    previous: &'a [Option<PathBuf>],
    /// Indexes of the TIPs this run registered.
    registered: Vec<usize>,
    /// Index of the TIP whose regsvr32 failed after it may have run: its DLL undid its own
    /// changes but ignored errors while doing so, or the wait for regsvr32 or its exit code
    /// failed after it started. Its view is read back.
    failed: Option<usize>,
    /// The TIP whose regsvr32 did not finish in time: it keeps running, so its view is left
    /// alone and its state is unknown.
    unknown: Option<TipArch>,
    /// Whether the record was (perhaps partly) written.
    record_changed: bool,
}

impl Run<'_> {
    /// Step 4: registers each TIP; stops at the first failure.
    fn register<S: InstallSystem + ?Sized>(
        &mut self,
        system: &mut S,
        output: &mut Output<'_>,
    ) -> bool {
        for (index, tip) in self.tips.iter().enumerate() {
            match system.regsvr32(tip, false) {
                Ok(0) => {
                    self.registered.push(index);
                    output.line(format_args!(
                        "[{}] registered {}",
                        tip.arch,
                        tip.path.display()
                    ));
                }
                Ok(code) => {
                    self.failed = Some(index);
                    output.error(format_args!(
                        "install: regsvr32 could not register the {} TIP (exit code {code})",
                        tip.arch
                    ));
                    return false;
                }
                Err(error) => {
                    // Any error but a timeout may come after regsvr32 ran (the wait or the
                    // exit code failed): the view is read back. A regsvr32 that timed out
                    // still runs, so its view must not be touched.
                    if error.kind() == WinErrorKind::TimedOut {
                        self.unknown = Some(tip.arch);
                    } else {
                        self.failed = Some(index);
                    }
                    output.error(format_args!(
                        "install: regsvr32 could not register the {} TIP ({error})",
                        tip.arch
                    ));
                    return false;
                }
            }
        }
        true
    }

    /// Step 5: reads every registration back.
    fn verify<S: InstallSystem + ?Sized>(&self, system: &mut S, output: &mut Output<'_>) -> bool {
        let report = verify_with(system, self.tips);
        output.line("Verification (read back from the registry):");
        for line in report.describe() {
            output.line(format_args!("  {line}"));
        }
        if report.all_passed() {
            return true;
        }
        output.error("install: the registrations could not be verified (see the FAILED checks)");
        false
    }

    /// Step 6: writes the installation record.
    fn write_record<S: InstallSystem + ?Sized>(
        &mut self,
        system: &mut S,
        output: &mut Output<'_>,
    ) -> bool {
        match system.write_record(&InstallRecord::new(self.tips)) {
            Ok(()) => true,
            Err(error) => {
                self.record_changed = true;
                output.error(format_args!(
                    "install: the installation record HKLM\\{INSTALL_RECORD_KEY} could not be \
                     written ({error})"
                ));
                false
            }
        }
    }

    /// Undoes the run, last change first. Returns whether everything was restored.
    fn roll_back<S: InstallSystem + ?Sized>(
        &self,
        system: &mut S,
        previous_record: Option<&InstallRecord>,
        output: &mut Output<'_>,
    ) -> bool {
        let mut complete = true;
        if self.record_changed {
            let restored = match previous_record {
                Some(record) => system.write_record(record),
                None => system.remove_record().map(|_| ()),
            };
            if let Err(error) = restored {
                output.detail(format_args!(
                    "the previous installation record could not be restored ({error})"
                ));
                complete = false;
            }
        }
        if let Some(index) = self.failed
            && let Some(tip) = self.tips.get(index)
        {
            let before = self.previous.get(index).and_then(Option::as_ref);
            complete &= restore_after_failure(system, tip, before, output);
        }
        for &index in self.registered.iter().rev() {
            let Some(tip) = self.tips.get(index) else {
                continue;
            };
            let before = self.previous.get(index).and_then(Option::as_ref);
            complete &= restore(system, tip, before, output);
        }
        if let Some(arch) = self.unknown {
            output.detail(format_args!(
                "[{arch}] regsvr32 did not finish in time; the state of this registration is \
                 unknown"
            ));
            complete = false;
        }
        complete
    }
}

/// Gives the view of `tip` back the registration it had before the run (`before`: its
/// `InprocServer32`). Returns whether that worked: regsvr32's exit code is no proof, so the
/// view is read back after it reports success.
fn restore<S: InstallSystem + ?Sized>(
    system: &mut S,
    tip: &TipBinary,
    before: Option<&PathBuf>,
    output: &mut Output<'_>,
) -> bool {
    let arch = tip.arch;
    let (target, unregister) = match before {
        None => (tip.clone(), true),
        Some(old) if same_path(old, &tip.path) => {
            output.detail(format_args!(
                "[{arch}] was registered from the same DLL before this run and stays registered"
            ));
            return true;
        }
        Some(old) => {
            if !system.file_exists(old) {
                output.detail(format_args!(
                    "[{arch}] the DLL registered before this run no longer exists, so that \
                     registration cannot be restored: {}",
                    old.display()
                ));
                return false;
            }
            (
                TipBinary {
                    path: old.clone(),
                    arch,
                },
                false,
            )
        }
    };
    match system.regsvr32(&target, unregister) {
        Ok(0) => {
            let restored = if unregister {
                verify_removal_with(system, &[arch]).all_passed()
            } else {
                verify_with(system, core::slice::from_ref(&target)).all_passed()
            };
            match (restored, unregister) {
                (true, true) => output.detail(format_args!("[{arch}] unregistered again")),
                (true, false) => output.detail(format_args!(
                    "[{arch}] registered again from the DLL registered before this run: {}",
                    target.path.display()
                )),
                (false, _) => output.detail(format_args!(
                    "[{arch}] could not be restored: regsvr32 reported success, but the registry \
                     read back does not show the registration from before this run"
                )),
            }
            restored
        }
        Ok(code) => {
            output.detail(format_args!(
                "[{arch}] could not be restored (regsvr32 exit code {code})"
            ));
            false
        }
        Err(error) => {
            output.detail(format_args!("[{arch}] could not be restored ({error})"));
            false
        }
    }
}

/// The view of a TIP whose registration ran and failed. Its `DllRegisterServer` undid its own
/// changes, but it ignores errors while doing so: when the view is not as it was before the run
/// (`before`: its `InprocServer32`), it is restored like the view of a registered TIP. Returns
/// whether the view is as it was.
fn restore_after_failure<S: InstallSystem + ?Sized>(
    system: &mut S,
    tip: &TipBinary,
    before: Option<&PathBuf>,
    output: &mut Output<'_>,
) -> bool {
    let arch = tip.arch;
    match before {
        None => {
            if verify_removal_with(system, &[arch]).all_passed() {
                return true;
            }
            output.detail(format_args!(
                "[{arch}] the failed registration left Raumakey entries behind"
            ));
            restore(system, tip, None, output)
        }
        Some(old) => {
            let server = system
                .com_class(arch.registry_view())
                .ok()
                .flatten()
                .and_then(|state| state.inproc_server);
            if server.is_some_and(|server| same_path(Path::new(&server), old)) {
                return true;
            }
            output.detail(format_args!(
                "[{arch}] the failed registration changed the registration made before this run"
            ));
            if same_path(old, &tip.path) {
                // The DLL registered before is the one that just failed.
                return false;
            }
            restore(system, tip, Some(old), output)
        }
    }
}

/// `uninstall`: removes the registration of both package architectures and then the
/// installation record (see the module documentation).
pub fn uninstall<S: InstallSystem + ?Sized>(
    system: &mut S,
    exe_dir: &Path,
    already_restarted: bool,
    output: &mut Output<'_>,
) -> u8 {
    if let Privilege::Done(code) = ensure_elevated(
        system,
        &["uninstall".to_owned()],
        already_restarted,
        NOTHING_CHANGED,
        output,
    ) {
        return code;
    }
    let record = match system.read_record() {
        Ok(record) => record,
        Err(error) => {
            output.error(format_args!(
                "uninstall: the installation record HKLM\\{INSTALL_RECORD_KEY} could not be read \
                 ({error}); using the TIPs next to raumakey.exe"
            ));
            None
        }
    };
    let mut failed = false;
    for arch in PACKAGE_ARCHS {
        if verify_removal_with(system, &[arch]).all_passed() {
            output.line(format_args!("[{arch}] not registered"));
            continue;
        }
        let mut candidates: Vec<PathBuf> = Vec::new();
        let recorded = record.as_ref().and_then(|record| record.tip_path(arch));
        for path in recorded
            .into_iter()
            .map(Path::to_path_buf)
            .chain([default_tip_path(exe_dir, arch)])
        {
            if !candidates.iter().any(|known| same_path(known, &path)) {
                candidates.push(path);
            }
        }
        let Some(dll) = candidates
            .iter()
            .find(|path| system.file_exists(path))
            .cloned()
        else {
            output.error(format_args!(
                "uninstall: the {arch} TIP cannot be unregistered: none of these files exists:"
            ));
            for path in &candidates {
                output.detail(path.display());
            }
            failed = true;
            continue;
        };
        let tip = TipBinary { path: dll, arch };
        match system.regsvr32(&tip, true) {
            Ok(0) => output.line(format_args!(
                "[{arch}] unregistered with {}",
                tip.path.display()
            )),
            Ok(code) => {
                output.error(format_args!(
                    "uninstall: regsvr32 could not unregister the {arch} TIP (exit code {code})"
                ));
                failed = true;
            }
            Err(error) => {
                output.error(format_args!(
                    "uninstall: regsvr32 could not unregister the {arch} TIP ({error})"
                ));
                failed = true;
            }
        }
    }
    let report = verify_removal_with(system, &PACKAGE_ARCHS);
    output.line("Verification (read back from the registry):");
    for line in report.describe() {
        output.line(format_args!("  {line}"));
    }
    if failed || !report.all_passed() {
        output.error(format_args!(
            "uninstall: Raumakey is still partly registered, so the installation record \
             HKLM\\{INSTALL_RECORD_KEY} is kept; run uninstall again once the problem is fixed"
        ));
        return EXIT_FAILED;
    }
    match system.remove_record() {
        Ok(existed) => {
            if existed {
                output.line(format_args!(
                    "Removed the installation record HKLM\\{INSTALL_RECORD_KEY}."
                ));
            } else {
                output.line(format_args!(
                    "No installation record was present in HKLM\\{INSTALL_RECORD_KEY}."
                ));
            }
            output.line("Raumakey is uninstalled.");
            EXIT_OK
        }
        Err(error) => {
            output.error(format_args!(
                "uninstall: every registration is removed, but the installation record \
                 HKLM\\{INSTALL_RECORD_KEY} could not be deleted ({error})"
            ));
            EXIT_FAILED
        }
    }
}
