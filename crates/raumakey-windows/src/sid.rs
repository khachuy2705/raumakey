//! User SID and session of the current and other processes (read-only).

use std::os::windows::io::OwnedHandle;

use windows::Win32::Foundation::HANDLE;
use windows::Win32::Security::Authorization::ConvertSidToStringSidW;
use windows::Win32::Security::{
    GetTokenInformation, PSID, TOKEN_ELEVATION, TOKEN_QUERY, TOKEN_USER, TokenElevation, TokenUser,
};
use windows::Win32::System::RemoteDesktop::ProcessIdToSessionId;
use windows::Win32::System::Threading::{
    GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::core::{PCWSTR, PWSTR};

use crate::error::{E_FAIL, ResultExt, WinError, WinResult};
use crate::handle::{self, LocalMemory};
use crate::names::{SidHash, sid_hash16};
use crate::strings::read_pwstr;

/// SID string (`S-1-…`) of the user running this process.
pub fn current_user_sid_string() -> WinResult<String> {
    // SAFETY: GetCurrentProcess returns a pseudo handle that needs no cleanup.
    let process = unsafe { GetCurrentProcess() };
    let token = open_token(process)?;
    token_user_sid_string(handle::raw(&token))
}

/// [`sid_hash16`] of [`current_user_sid_string`].
pub fn current_user_sid_hash() -> WinResult<SidHash> {
    current_user_sid_string().map(|sid| sid_hash16(&sid))
}

/// Terminal Services session of this process.
pub fn current_session_id() -> WinResult<u32> {
    // SAFETY: no preconditions.
    process_session_id(unsafe { GetCurrentProcessId() })
}

/// Terminal Services session of process `pid`.
pub fn process_session_id(pid: u32) -> WinResult<u32> {
    let mut session = 0_u32;
    // SAFETY: `session` is a valid out pointer for the duration of the call.
    unsafe { ProcessIdToSessionId(pid, &mut session) }.context("ProcessIdToSessionId")?;
    Ok(session)
}

/// SID string of the user that owns process `pid`.
///
/// Needs `PROCESS_QUERY_LIMITED_INFORMATION` on the process and `TOKEN_QUERY` on its token; a
/// non-elevated caller usually gets `AccessDenied` for elevated processes.
pub fn process_user_sid_string(pid: u32) -> WinResult<String> {
    let process = open_process(pid)?;
    let token = open_token(handle::raw(&process))?;
    token_user_sid_string(handle::raw(&token))
}

/// Whether process `pid` runs with an elevated token.
pub fn process_is_elevated(pid: u32) -> WinResult<bool> {
    let process = open_process(pid)?;
    let token = open_token(handle::raw(&process))?;
    token_is_elevated(handle::raw(&token))
}

/// Whether this process runs with an elevated token.
pub fn current_process_is_elevated() -> WinResult<bool> {
    // SAFETY: GetCurrentProcess returns a pseudo handle that needs no cleanup.
    let token = open_token(unsafe { GetCurrentProcess() })?;
    token_is_elevated(handle::raw(&token))
}

fn open_process(pid: u32) -> WinResult<OwnedHandle> {
    // SAFETY: plain call; the returned handle is owned below.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }
        .context("OpenProcess")?;
    // SAFETY: `process` was just opened and is owned by nobody else.
    Ok(unsafe { handle::take(process) })
}

fn open_token(process: HANDLE) -> WinResult<OwnedHandle> {
    let mut token = HANDLE::default();
    // SAFETY: `process` is a valid process handle and `token` a valid out pointer.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.context("OpenProcessToken")?;
    // SAFETY: the token handle was just opened and is owned by nobody else.
    Ok(unsafe { handle::take(token) })
}

/// SID string of the user of an access token (also used for impersonation tokens).
pub(crate) fn token_user_sid_string(token: HANDLE) -> WinResult<String> {
    let mut needed = 0_u32;
    // SAFETY: a size query with no buffer; `needed` is a valid out pointer. The expected
    // ERROR_INSUFFICIENT_BUFFER failure is ignored and `needed` checked instead.
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut needed) };
    if needed == 0 {
        return Err(WinError::last("GetTokenInformation(TokenUser) size"));
    }
    // u64 storage aligns the buffer for TOKEN_USER (which contains a pointer).
    let mut buffer = vec![0_u64; (needed as usize).div_ceil(8)];
    // SAFETY: the buffer holds at least `needed` writable bytes.
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            needed,
            &mut needed,
        )
    }
    .context("GetTokenInformation(TokenUser)")?;
    // SAFETY: on success the buffer starts with an initialized, suitably aligned TOKEN_USER
    // whose SID pointer points into the same buffer, which outlives this use.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };
    sid_to_string(user.User.Sid)
}

fn token_is_elevated(token: HANDLE) -> WinResult<bool> {
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0_u32;
    // SAFETY: the buffer is a TOKEN_ELEVATION of the size passed.
    unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&raw mut elevation).cast()),
            size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    }
    .context("GetTokenInformation(TokenElevation)")?;
    Ok(elevation.TokenIsElevated != 0)
}

/// Canonical string form of a SID.
pub(crate) fn sid_to_string(sid: PSID) -> WinResult<String> {
    let mut text = PWSTR::null();
    // SAFETY: `sid` points to a valid SID owned by the caller; `text` is a valid out pointer.
    unsafe { ConvertSidToStringSidW(sid, &mut text) }.context("ConvertSidToStringSidW")?;
    let memory = LocalMemory(text.0.cast());
    // SAFETY: on success `text` is a NUL-terminated string allocated with LocalAlloc, freed by
    // `memory` after this read.
    let units = unsafe { read_pwstr(PCWSTR(text.0)) };
    drop(memory);
    String::from_utf16(&units).map_err(|_| WinError::new(E_FAIL, "ConvertSidToStringSidW"))
}
