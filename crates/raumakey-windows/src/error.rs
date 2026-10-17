//! Typed Win32/COM errors.
//!
//! Every fallible wrapper in this crate returns [`WinResult`]. A [`WinError`] is an HRESULT plus a
//! `&'static str` naming the operation that failed, so neither the value nor its `Display` form
//! can carry user content (paths, keystrokes, document or clipboard text).

use core::fmt;
use std::io;

/// `E_ACCESSDENIED` (`HRESULT_FROM_WIN32(ERROR_ACCESS_DENIED)`).
pub const E_ACCESSDENIED: i32 = 0x8007_0005_u32 as i32;
/// `E_FAIL`: unspecified failure.
pub const E_FAIL: i32 = 0x8000_4005_u32 as i32;
/// `E_UNEXPECTED`: catastrophic failure (also used for caught panics).
pub const E_UNEXPECTED: i32 = 0x8000_FFFF_u32 as i32;
/// `E_INVALIDARG`.
pub const E_INVALIDARG: i32 = 0x8007_0057_u32 as i32;
/// `E_NOTIMPL`.
pub const E_NOTIMPL: i32 = 0x8000_4001_u32 as i32;
/// `E_ABORT`.
pub const E_ABORT: i32 = 0x8000_4004_u32 as i32;
/// `E_POINTER`.
pub const E_POINTER: i32 = 0x8000_4003_u32 as i32;
/// `E_OUTOFMEMORY`.
pub const E_OUTOFMEMORY: i32 = 0x8007_000E_u32 as i32;
/// `CLASS_E_NOAGGREGATION`.
pub const CLASS_E_NOAGGREGATION: i32 = 0x8004_0110_u32 as i32;
/// `CLASS_E_CLASSNOTAVAILABLE`.
pub const CLASS_E_CLASSNOTAVAILABLE: i32 = 0x8004_0111_u32 as i32;
/// `REGDB_E_CLASSNOTREG`.
pub const REGDB_E_CLASSNOTREG: i32 = 0x8004_0154_u32 as i32;
/// `CLIPBRD_E_CANT_OPEN`.
pub const CLIPBRD_E_CANT_OPEN: i32 = 0x8004_01D0_u32 as i32;
/// `S_OK`.
pub const S_OK: i32 = 0;
/// `S_FALSE`.
pub const S_FALSE: i32 = 1;

/// `FACILITY_WIN32`.
const FACILITY_WIN32: u32 = 7;

/// Win32 error codes this crate refers to by name.
pub mod win32 {
    /// `ERROR_FILE_NOT_FOUND`.
    pub const ERROR_FILE_NOT_FOUND: u32 = 2;
    /// `ERROR_PATH_NOT_FOUND`.
    pub const ERROR_PATH_NOT_FOUND: u32 = 3;
    /// `ERROR_ACCESS_DENIED`.
    pub const ERROR_ACCESS_DENIED: u32 = 5;
    /// `ERROR_INVALID_HANDLE`.
    pub const ERROR_INVALID_HANDLE: u32 = 6;
    /// `ERROR_NOT_ENOUGH_MEMORY`.
    pub const ERROR_NOT_ENOUGH_MEMORY: u32 = 8;
    /// `ERROR_INVALID_DATA`.
    pub const ERROR_INVALID_DATA: u32 = 13;
    /// `ERROR_SHARING_VIOLATION`.
    pub const ERROR_SHARING_VIOLATION: u32 = 32;
    /// `ERROR_LOCK_VIOLATION`.
    pub const ERROR_LOCK_VIOLATION: u32 = 33;
    /// `ERROR_NOT_SUPPORTED`.
    pub const ERROR_NOT_SUPPORTED: u32 = 50;
    /// `ERROR_FILE_EXISTS`.
    pub const ERROR_FILE_EXISTS: u32 = 80;
    /// `ERROR_INVALID_PARAMETER`.
    pub const ERROR_INVALID_PARAMETER: u32 = 87;
    /// `ERROR_BROKEN_PIPE`.
    pub const ERROR_BROKEN_PIPE: u32 = 109;
    /// `ERROR_INSUFFICIENT_BUFFER`.
    pub const ERROR_INSUFFICIENT_BUFFER: u32 = 122;
    /// `ERROR_CALL_NOT_IMPLEMENTED`.
    pub const ERROR_CALL_NOT_IMPLEMENTED: u32 = 120;
    /// `ERROR_SEM_TIMEOUT`.
    pub const ERROR_SEM_TIMEOUT: u32 = 121;
    /// `ERROR_INVALID_NAME`.
    pub const ERROR_INVALID_NAME: u32 = 123;
    /// `ERROR_MOD_NOT_FOUND`.
    pub const ERROR_MOD_NOT_FOUND: u32 = 126;
    /// `ERROR_BAD_PATHNAME`.
    pub const ERROR_BAD_PATHNAME: u32 = 161;
    /// `ERROR_BUSY`.
    pub const ERROR_BUSY: u32 = 170;
    /// `ERROR_ALREADY_EXISTS`.
    pub const ERROR_ALREADY_EXISTS: u32 = 183;
    /// `ERROR_MORE_DATA`.
    pub const ERROR_MORE_DATA: u32 = 234;
    /// `ERROR_PIPE_BUSY`.
    pub const ERROR_PIPE_BUSY: u32 = 231;
    /// `ERROR_NO_DATA`.
    pub const ERROR_NO_DATA: u32 = 232;
    /// `ERROR_PIPE_NOT_CONNECTED`.
    pub const ERROR_PIPE_NOT_CONNECTED: u32 = 233;
    /// `WAIT_TIMEOUT` (also a Win32 error code).
    pub const WAIT_TIMEOUT: u32 = 258;
    /// `ERROR_NO_MORE_ITEMS`.
    pub const ERROR_NO_MORE_ITEMS: u32 = 259;
    /// `ERROR_NOT_OWNER`.
    pub const ERROR_NOT_OWNER: u32 = 288;
    /// `ERROR_PIPE_CONNECTED`.
    pub const ERROR_PIPE_CONNECTED: u32 = 535;
    /// `ERROR_ELEVATION_REQUIRED`.
    pub const ERROR_ELEVATION_REQUIRED: u32 = 740;
    /// `ERROR_OPERATION_ABORTED`.
    pub const ERROR_OPERATION_ABORTED: u32 = 995;
    /// `ERROR_IO_INCOMPLETE`.
    pub const ERROR_IO_INCOMPLETE: u32 = 996;
    /// `ERROR_IO_PENDING`.
    pub const ERROR_IO_PENDING: u32 = 997;
    /// `ERROR_NOT_FOUND`.
    pub const ERROR_NOT_FOUND: u32 = 1168;
    /// `ERROR_UNABLE_TO_REMOVE_REPLACED`.
    pub const ERROR_UNABLE_TO_REMOVE_REPLACED: u32 = 1175;
    /// `ERROR_UNABLE_TO_MOVE_REPLACEMENT`.
    pub const ERROR_UNABLE_TO_MOVE_REPLACEMENT: u32 = 1176;
    /// `ERROR_UNABLE_TO_MOVE_REPLACEMENT_2`.
    pub const ERROR_UNABLE_TO_MOVE_REPLACEMENT_2: u32 = 1177;
    /// `ERROR_CANCELLED`.
    pub const ERROR_CANCELLED: u32 = 1223;
    /// `ERROR_PRIVILEGE_NOT_HELD`.
    pub const ERROR_PRIVILEGE_NOT_HELD: u32 = 1314;
    /// `ERROR_CANNOT_IMPERSONATE`.
    pub const ERROR_CANNOT_IMPERSONATE: u32 = 1368;
    /// `ERROR_TIMEOUT`.
    pub const ERROR_TIMEOUT: u32 = 1460;
    /// `ERROR_CLIPBOARD_NOT_OPEN`.
    pub const ERROR_CLIPBOARD_NOT_OPEN: u32 = 1418;
}

/// `HRESULT_FROM_WIN32`: maps a Win32 error code to an HRESULT. Zero stays zero and values that
/// already have the severity bit set pass through unchanged.
pub const fn hresult_from_win32(error: u32) -> i32 {
    if error == 0 || error & 0x8000_0000 != 0 {
        error as i32
    } else {
        ((error & 0xFFFF) | (FACILITY_WIN32 << 16) | 0x8000_0000) as i32
    }
}

/// Broad class of a [`WinError`], for callers that branch on the cause.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WinErrorKind {
    /// The file, key, object, or entry does not exist.
    NotFound,
    /// The caller lacks the required access or privilege.
    AccessDenied,
    /// The object already exists.
    AlreadyExists,
    /// The operation did not finish in time.
    TimedOut,
    /// The operation was cancelled (by the caller, the user, or a stop request).
    Cancelled,
    /// The resource is in use; retrying later may succeed.
    Busy,
    /// An argument (name, path, size) was rejected.
    InvalidInput,
    /// The operation is not supported here.
    NotSupported,
    /// The peer of a pipe went away.
    BrokenPipe,
    /// Any other failure.
    Other,
}

/// A failed Win32 or COM operation: an HRESULT and the static name of the operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct WinError {
    code: i32,
    context: &'static str,
}

/// Result of the wrappers in this crate.
pub type WinResult<T> = Result<T, WinError>;

impl WinError {
    /// An error with an HRESULT `code` for the operation named by `context`. A success code is
    /// replaced with [`E_FAIL`] so that an error never reads as success.
    pub const fn new(code: i32, context: &'static str) -> Self {
        let code = if code >= 0 { E_FAIL } else { code };
        Self { code, context }
    }

    /// An error for a Win32 error code (converted with [`hresult_from_win32`]).
    pub const fn from_win32(error: u32, context: &'static str) -> Self {
        Self::new(hresult_from_win32(error), context)
    }

    /// An error for an I/O error; OS error codes are kept, other errors become [`E_FAIL`].
    pub fn from_io(error: &io::Error, context: &'static str) -> Self {
        match error.raw_os_error() {
            Some(code) => Self::from_win32(code as u32, context),
            None => Self::new(E_FAIL, context),
        }
    }

    /// The error of the calling thread's last failed Win32 call (`GetLastError`).
    #[cfg(windows)]
    pub fn last(context: &'static str) -> Self {
        Self::new(windows::core::HRESULT::from_thread().0, context)
    }

    /// The HRESULT.
    pub const fn code(&self) -> i32 {
        self.code
    }

    /// The HRESULT as the `windows` crate type.
    #[cfg(windows)]
    pub const fn hresult(&self) -> windows::core::HRESULT {
        windows::core::HRESULT(self.code)
    }

    /// The operation that failed.
    pub const fn context(&self) -> &'static str {
        self.context
    }

    /// The Win32 error code when the HRESULT has `FACILITY_WIN32`.
    pub const fn win32_code(&self) -> Option<u32> {
        let raw = self.code as u32;
        if raw & 0xFFFF_0000 == 0x8007_0000 {
            Some(raw & 0xFFFF)
        } else {
            None
        }
    }

    /// True when the error is the Win32 error `error`.
    pub const fn is_win32(&self, error: u32) -> bool {
        matches!(self.win32_code(), Some(code) if code == error)
    }

    /// The broad class of the error.
    pub const fn kind(&self) -> WinErrorKind {
        if let Some(code) = self.win32_code() {
            return match code {
                win32::ERROR_FILE_NOT_FOUND
                | win32::ERROR_PATH_NOT_FOUND
                | win32::ERROR_NOT_FOUND
                | win32::ERROR_MOD_NOT_FOUND => WinErrorKind::NotFound,
                win32::ERROR_ACCESS_DENIED
                | win32::ERROR_PRIVILEGE_NOT_HELD
                | win32::ERROR_ELEVATION_REQUIRED => WinErrorKind::AccessDenied,
                win32::ERROR_ALREADY_EXISTS | win32::ERROR_FILE_EXISTS => {
                    WinErrorKind::AlreadyExists
                }
                win32::ERROR_TIMEOUT | win32::WAIT_TIMEOUT | win32::ERROR_SEM_TIMEOUT => {
                    WinErrorKind::TimedOut
                }
                win32::ERROR_OPERATION_ABORTED | win32::ERROR_CANCELLED => WinErrorKind::Cancelled,
                win32::ERROR_PIPE_BUSY
                | win32::ERROR_SHARING_VIOLATION
                | win32::ERROR_LOCK_VIOLATION
                | win32::ERROR_BUSY => WinErrorKind::Busy,
                win32::ERROR_INVALID_PARAMETER
                | win32::ERROR_INVALID_NAME
                | win32::ERROR_BAD_PATHNAME
                | win32::ERROR_INVALID_DATA => WinErrorKind::InvalidInput,
                win32::ERROR_NOT_SUPPORTED | win32::ERROR_CALL_NOT_IMPLEMENTED => {
                    WinErrorKind::NotSupported
                }
                win32::ERROR_BROKEN_PIPE
                | win32::ERROR_PIPE_NOT_CONNECTED
                | win32::ERROR_NO_DATA => WinErrorKind::BrokenPipe,
                _ => WinErrorKind::Other,
            };
        }
        match self.code {
            E_ABORT => WinErrorKind::Cancelled,
            E_NOTIMPL => WinErrorKind::NotSupported,
            E_POINTER => WinErrorKind::InvalidInput,
            REGDB_E_CLASSNOTREG | CLASS_E_CLASSNOTAVAILABLE => WinErrorKind::NotFound,
            CLIPBRD_E_CANT_OPEN => WinErrorKind::Busy,
            _ => WinErrorKind::Other,
        }
    }

    /// The same code for another operation name.
    #[must_use]
    pub const fn with_context(self, context: &'static str) -> Self {
        Self {
            code: self.code,
            context,
        }
    }
}

impl fmt::Display for WinError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} failed (0x{:08X})", self.context, self.code as u32)
    }
}

impl std::error::Error for WinError {}

impl From<WinError> for io::Error {
    fn from(error: WinError) -> Self {
        match error.win32_code() {
            Some(code) => io::Error::from_raw_os_error(code as i32),
            None => io::Error::other(error),
        }
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for WinError {
    fn from(error: windows::core::Error) -> Self {
        Self::new(error.code().0, "Windows API call")
    }
}

/// Attaches an operation name to `windows` crate results.
#[cfg(windows)]
pub(crate) trait ResultExt<T> {
    /// Converts the error into a [`WinError`] for `context`.
    fn context(self, context: &'static str) -> WinResult<T>;
}

#[cfg(windows)]
impl<T> ResultExt<T> for windows::core::Result<T> {
    fn context(self, context: &'static str) -> WinResult<T> {
        self.map_err(|error| WinError::new(error.code().0, context))
    }
}

/// Converts a `WIN32_ERROR` status (registry and similar APIs) into a result.
#[cfg(windows)]
pub(crate) fn check_win32(
    status: windows::Win32::Foundation::WIN32_ERROR,
    context: &'static str,
) -> WinResult<()> {
    if status.0 == 0 {
        Ok(())
    } else {
        Err(WinError::from_win32(status.0, context))
    }
}
