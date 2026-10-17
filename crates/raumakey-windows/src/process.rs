//! Child processes: regsvr32, elevation, and console attachment.
//!
//! The command-line, architecture, and console decisions are pure. On Windows,
//! [`run_regsvr32`] runs the regsvr32 that matches the TIP's bitness and returns its exit code;
//! **an exit code is never proof of registration** — verification reads the registry back
//! (see [`crate::tsf_registration::verify_installation`]).

use core::fmt;

use crate::tsf_registration::TipArch;

/// Processor architecture of the OS or of a process.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Machine {
    /// x86-64.
    X64,
    /// 32-bit x86.
    X86,
    /// ARM64.
    Arm64,
}

impl Machine {
    /// Decodes an `IMAGE_FILE_MACHINE_*` value.
    pub const fn from_image_file_machine(value: u16) -> Option<Self> {
        match value {
            0x8664 => Some(Self::X64),
            0x014C => Some(Self::X86),
            0xAA64 => Some(Self::Arm64),
            _ => None,
        }
    }

    /// Architecture this binary was compiled for.
    pub const fn of_this_binary() -> Self {
        if cfg!(target_arch = "aarch64") {
            Self::Arm64
        } else if cfg!(target_arch = "x86") {
            Self::X86
        } else {
            Self::X64
        }
    }
}

/// Directory holding the regsvr32 to use, relative to the Windows directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Regsvr32Dir {
    /// `System32` (native regsvr32 for a native process).
    System32,
    /// `SysWOW64` (32-bit regsvr32 on 64-bit Windows).
    SysWow64,
    /// `Sysnative` (native regsvr32 seen from a 32-bit process on 64-bit Windows).
    Sysnative,
}

/// The TIP architecture cannot be registered on this OS.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct UnsupportedArch;

impl fmt::Display for UnsupportedArch {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TIP architecture is not supported on this system")
    }
}

impl std::error::Error for UnsupportedArch {}

/// Chooses the regsvr32 directory for a TIP of `tip` bitness, given the OS (`native`) and the
/// calling process (`process`) architectures.
///
/// x64 TIPs are registered on x64 Windows and ARM64 TIPs on ARM64 Windows by the native
/// regsvr32; x86 TIPs on 64-bit Windows by the `SysWOW64` one. x64 TIPs on ARM64 Windows are not
/// supported (the ARM64 package ships ARM64 and x86 TIPs).
pub const fn regsvr32_dir(
    native: Machine,
    process: Machine,
    tip: TipArch,
) -> Result<Regsvr32Dir, UnsupportedArch> {
    let native_tip = matches!(
        (native, tip),
        (Machine::X64, TipArch::X64)
            | (Machine::Arm64, TipArch::Arm64)
            | (Machine::X86, TipArch::X86)
    );
    if native_tip {
        let redirected = matches!(process, Machine::X86) && !matches!(native, Machine::X86);
        return Ok(if redirected {
            Regsvr32Dir::Sysnative
        } else {
            Regsvr32Dir::System32
        });
    }
    match (native, tip) {
        (Machine::X64 | Machine::Arm64, TipArch::X86) => Ok(Regsvr32Dir::SysWow64),
        _ => Err(UnsupportedArch),
    }
}

/// Quotes one argument so that `CommandLineToArgvW` and the MSVC runtime parse it back exactly.
pub fn quote_arg(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\u{0B}', '"']) {
        return arg.to_owned();
    }
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('"');
    let mut backslashes = 0_usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                quoted.extend(core::iter::repeat_n('\\', backslashes * 2 + 1));
                quoted.push('"');
                backslashes = 0;
            }
            _ => {
                quoted.extend(core::iter::repeat_n('\\', backslashes));
                quoted.push(c);
                backslashes = 0;
            }
        }
    }
    quoted.extend(core::iter::repeat_n('\\', backslashes * 2));
    quoted.push('"');
    quoted
}

/// `"<program>" <arg> <arg>…`: the program path is always quoted (so `C:\Program Files\…`
/// cannot be split), each argument is quoted as needed.
pub fn command_line(program: &str, args: &[&str]) -> String {
    let mut line = format!("\"{program}\"");
    for arg in args {
        line.push(' ');
        line.push_str(&quote_arg(arg));
    }
    line
}

/// regsvr32 arguments: always silent (`/s`), `/u` to unregister, then the DLL path.
pub fn regsvr32_arguments(dll: &str, unregister: bool) -> Vec<String> {
    let mut args = vec!["/s".to_owned()];
    if unregister {
        args.push("/u".to_owned());
    }
    args.push(dll.to_owned());
    args
}

/// What [`attach_parent_console`] does for the given standard handle states.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ConsolePlan {
    /// Attach to the parent's console.
    pub attach: bool,
    /// Point stdout at the console.
    pub replace_stdout: bool,
    /// Point stderr at the console.
    pub replace_stderr: bool,
}

/// Attaches only when a standard output handle is missing, and replaces only missing handles,
/// so redirected output (`raumakey.exe install > log.txt`) keeps its target.
pub const fn console_plan(stdout_usable: bool, stderr_usable: bool) -> ConsolePlan {
    ConsolePlan {
        attach: !stdout_usable || !stderr_usable,
        replace_stdout: !stdout_usable,
        replace_stderr: !stderr_usable,
    }
}

#[cfg(windows)]
pub use win::{
    attach_parent_console, current_machines, regsvr32_path, relaunch_elevated, run_regsvr32,
};

#[cfg(windows)]
mod win {
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    use windows::Win32::Foundation::{
        GENERIC_READ, GENERIC_WRITE, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
    };
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_NORMAL, FILE_SHARE_READ, FILE_SHARE_WRITE, FILE_TYPE_UNKNOWN,
        GetFileType, OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE,
        STD_OUTPUT_HANDLE, SetStdHandle,
    };
    use windows::Win32::System::SystemInformation::IMAGE_FILE_MACHINE;
    use windows::Win32::System::Threading::{
        CREATE_NO_WINDOW, CreateProcessW, GetCurrentProcess, GetExitCodeProcess, IsWow64Process2,
        PROCESS_INFORMATION, STARTUPINFOW, WaitForSingleObject,
    };
    use windows::Win32::UI::Shell::{
        SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
        ShellExecuteExW,
    };
    use windows::Win32::UI::WindowsAndMessaging::SW_HIDE;
    use windows::core::{PCWSTR, PWSTR};

    use super::{
        Machine, Regsvr32Dir, command_line, console_plan, quote_arg, regsvr32_arguments,
        regsvr32_dir,
    };
    use crate::error::{E_INVALIDARG, ResultExt, WinError, WinResult, win32};
    use crate::handle;
    use crate::known_folder::{system_dir, syswow64_dir, windows_dir};
    use crate::pipe::timeout_ms;
    use crate::strings::WideString;
    use crate::tsf_registration::TipArch;

    /// `(native OS machine, this process's machine)`.
    pub fn current_machines() -> WinResult<(Machine, Machine)> {
        let mut process = IMAGE_FILE_MACHINE::default();
        let mut native = IMAGE_FILE_MACHINE::default();
        // SAFETY: pseudo handle of this process and valid out pointers.
        unsafe { IsWow64Process2(GetCurrentProcess(), &mut process, Some(&mut native)) }
            .context("IsWow64Process2")?;
        let native = Machine::from_image_file_machine(native.0)
            .ok_or_else(|| WinError::from_win32(win32::ERROR_NOT_SUPPORTED, "native machine"))?;
        // IMAGE_FILE_MACHINE_UNKNOWN: not a WOW64 process; it runs as compiled.
        let process =
            Machine::from_image_file_machine(process.0).unwrap_or_else(Machine::of_this_binary);
        Ok((native, process))
    }

    /// Path of the regsvr32 that registers a TIP of `arch` on this system. Fails with
    /// `NotSupported` for combinations that [`regsvr32_dir`] rejects.
    pub fn regsvr32_path(arch: TipArch) -> WinResult<PathBuf> {
        let (native, process) = current_machines()?;
        let dir = regsvr32_dir(native, process, arch)
            .map_err(|_| WinError::from_win32(win32::ERROR_NOT_SUPPORTED, "regsvr32 for TIP"))?;
        let base = match dir {
            Regsvr32Dir::System32 => system_dir()?,
            Regsvr32Dir::SysWow64 => syswow64_dir()?.ok_or_else(|| {
                WinError::from_win32(win32::ERROR_NOT_SUPPORTED, "SysWOW64 regsvr32")
            })?,
            Regsvr32Dir::Sysnative => windows_dir()?.join("Sysnative"),
        };
        Ok(base.join("regsvr32.exe"))
    }

    /// Runs `regsvr32 /s [/u] <dll>` with the regsvr32 of `arch`, waits up to `timeout`, and
    /// returns its exit code. The DLL path must be absolute. On timeout the child keeps
    /// running (it is never killed mid-registration) and `TimedOut` is returned.
    ///
    /// Registering writes `HKLM`, so the caller must be elevated. The exit code alone never
    /// proves registration.
    pub fn run_regsvr32(
        dll: &Path,
        arch: TipArch,
        unregister: bool,
        timeout: Duration,
    ) -> WinResult<u32> {
        if !dll.is_absolute() {
            return Err(WinError::new(E_INVALIDARG, "regsvr32 DLL path"));
        }
        let dll = dll
            .to_str()
            .ok_or_else(|| WinError::new(E_INVALIDARG, "regsvr32 DLL path"))?;
        let program = regsvr32_path(arch)?;
        let program_text = program
            .to_str()
            .ok_or_else(|| WinError::new(E_INVALIDARG, "regsvr32 path"))?;
        let args = regsvr32_arguments(dll, unregister);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let line = command_line(program_text, &arg_refs);
        let application = WideString::from_os(program.as_os_str())?;
        // CreateProcessW may modify the command-line buffer, so it must be mutable.
        let mut line: Vec<u16> = line.encode_utf16().chain(Some(0)).collect();
        let startup = STARTUPINFOW {
            cb: size_of::<STARTUPINFOW>() as u32,
            ..Default::default()
        };
        let mut info = PROCESS_INFORMATION::default();
        // SAFETY: all strings are NUL-terminated and outlive the call; no handles are
        // inherited; the returned handles are owned below.
        unsafe {
            CreateProcessW(
                application.as_pcwstr(),
                Some(PWSTR(line.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_NO_WINDOW,
                None,
                PCWSTR::null(),
                &startup,
                &mut info,
            )
        }
        .context("CreateProcessW(regsvr32)")?;
        // SAFETY: CreateProcessW returned new process and thread handles owned by nobody else.
        let (process, _thread) =
            unsafe { (handle::take(info.hProcess), handle::take(info.hThread)) };
        wait_for_exit(handle::raw(&process), timeout)
    }

    /// Starts `exe args…` elevated through the `runas` verb (UAC prompt), waits up to
    /// `timeout`, and returns the exit code. A declined prompt fails with `Cancelled`.
    pub fn relaunch_elevated(exe: &Path, args: &[&str], timeout: Duration) -> WinResult<u32> {
        let file = WideString::from_os(exe.as_os_str())?;
        let parameters: Vec<String> = args.iter().map(|arg| quote_arg(arg)).collect();
        let parameters = WideString::new(&parameters.join(" "))?;
        let verb = WideString::new("runas")?;
        let directory = exe
            .parent()
            .map(|dir| WideString::from_os(dir.as_os_str()))
            .transpose()?;
        let mut info = SHELLEXECUTEINFOW {
            cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
            lpVerb: verb.as_pcwstr(),
            lpFile: file.as_pcwstr(),
            lpParameters: parameters.as_pcwstr(),
            lpDirectory: directory
                .as_ref()
                .map_or(PCWSTR::null(), WideString::as_pcwstr),
            nShow: SW_HIDE.0,
            ..Default::default()
        };
        // SAFETY: every string in `info` is NUL-terminated and outlives the call.
        unsafe { ShellExecuteExW(&mut info) }.context("ShellExecuteExW(runas)")?;
        // Copied out by value: SHELLEXECUTEINFOW is packed on x86, so its fields must not be
        // borrowed.
        let process = info.hProcess;
        if process.is_invalid() {
            return Err(WinError::from_win32(
                win32::ERROR_INVALID_HANDLE,
                "ShellExecuteExW(runas)",
            ));
        }
        // SAFETY: SEE_MASK_NOCLOSEPROCESS hands the process handle to the caller.
        let process = unsafe { handle::take(process) };
        wait_for_exit(handle::raw(&process), timeout)
    }

    fn wait_for_exit(process: HANDLE, timeout: Duration) -> WinResult<u32> {
        // SAFETY: `process` is a valid process handle.
        let waited = unsafe { WaitForSingleObject(process, timeout_ms(Some(timeout))) };
        if waited == WAIT_TIMEOUT {
            return Err(WinError::from_win32(
                win32::ERROR_TIMEOUT,
                "wait for child process",
            ));
        }
        if waited != WAIT_OBJECT_0 {
            return Err(WinError::last("WaitForSingleObject(process)"));
        }
        let mut code = 0_u32;
        // SAFETY: valid process handle and out pointer.
        unsafe { GetExitCodeProcess(process, &mut code) }.context("GetExitCodeProcess")?;
        Ok(code)
    }

    /// For a GUI-subsystem executable started from a console: when stdout or stderr is
    /// missing, attaches to the parent's console and points the missing handles at it, so CLI
    /// subcommands can print. Handles that are already redirected are left alone. Returns
    /// whether a console was attached.
    pub fn attach_parent_console() -> WinResult<bool> {
        let plan = console_plan(usable(STD_OUTPUT_HANDLE), usable(STD_ERROR_HANDLE));
        if !plan.attach {
            return Ok(false);
        }
        // SAFETY: no preconditions; fails when the parent has no console.
        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_err() {
            return Ok(false);
        }
        let name = WideString::new("CONOUT$")?;
        // SAFETY: the name is NUL-terminated; the handle becomes a process-lifetime standard
        // handle and is intentionally never closed.
        let console = unsafe {
            CreateFileW(
                name.as_pcwstr(),
                (GENERIC_READ | GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_ATTRIBUTE_NORMAL,
                None,
            )
        }
        .context("CreateFileW(CONOUT$)")?;
        if plan.replace_stdout {
            // SAFETY: `console` is a valid console handle that stays open.
            unsafe { SetStdHandle(STD_OUTPUT_HANDLE, console) }.context("SetStdHandle(stdout)")?;
        }
        if plan.replace_stderr {
            // SAFETY: as above.
            unsafe { SetStdHandle(STD_ERROR_HANDLE, console) }.context("SetStdHandle(stderr)")?;
        }
        Ok(true)
    }

    fn usable(which: STD_HANDLE) -> bool {
        // SAFETY: no preconditions.
        match unsafe { GetStdHandle(which) } {
            Ok(handle) if !handle.is_invalid() => {
                // SAFETY: `handle` is a standard handle of this process.
                let kind = unsafe { GetFileType(handle) };
                kind != FILE_TYPE_UNKNOWN
            }
            _ => false,
        }
    }
}
