//! UTF-16 strings for Win32 calls.
//!
//! [`WideString`] owns a NUL-terminated UTF-16 buffer without interior NULs, so the `PCWSTR` it
//! lends is valid for as long as the borrow and cannot be silently truncated by an embedded NUL
//! (which matters for registry paths and object names). [`read_growing`] reads a string of
//! unknown length into a buffer that grows until it fits.

use crate::error::{E_INVALIDARG, WinError, WinResult, win32};

/// UTF-16 units of `text` followed by one NUL. An interior NUL is kept, so Win32 would stop at
/// it; use [`WideString::new`] when that must be rejected.
pub fn to_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Decodes `units` up to the first NUL (or the end), replacing ill-formed UTF-16 with U+FFFD.
pub fn from_wide_until_nul(units: &[u16]) -> String {
    String::from_utf16_lossy(until_nul(units))
}

/// The prefix of `units` before the first NUL.
pub fn until_nul(units: &[u16]) -> &[u16] {
    let end = units
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(units.len());
    units.get(..end).unwrap_or(units)
}

/// Reads a string with `fill`, which writes into the buffer it gets and returns the number of
/// units written without the NUL, or the buffer length when the string was truncated (the
/// `GetModuleFileNameW` contract). The buffer starts at `initial` units and doubles up to
/// `max`; a string that does not fit in `max` units fails with `ERROR_INSUFFICIENT_BUFFER` for
/// the operation `context`. Errors of `fill` are returned unchanged.
pub fn read_growing(
    initial: usize,
    max: usize,
    context: &'static str,
    mut fill: impl FnMut(&mut [u16]) -> WinResult<usize>,
) -> WinResult<Vec<u16>> {
    let mut size = initial.clamp(1, max.max(1));
    loop {
        let mut buffer = vec![0_u16; size];
        let written = fill(&mut buffer)?;
        if written < buffer.len() {
            buffer.truncate(written);
            return Ok(buffer);
        }
        if size >= max {
            return Err(WinError::from_win32(
                win32::ERROR_INSUFFICIENT_BUFFER,
                context,
            ));
        }
        size = size.saturating_mul(2).min(max);
    }
}

/// A NUL-terminated UTF-16 string without interior NULs.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct WideString(Vec<u16>);

impl WideString {
    /// Encodes `text`; fails with `E_INVALIDARG` when it contains a NUL.
    pub fn new(text: &str) -> WinResult<Self> {
        Self::from_units(text.encode_utf16())
    }

    /// Encodes an OS string (paths keep unpaired surrogates); fails when it contains a NUL.
    #[cfg(windows)]
    pub fn from_os(text: &std::ffi::OsStr) -> WinResult<Self> {
        use std::os::windows::ffi::OsStrExt;
        Self::from_units(text.encode_wide())
    }

    /// Takes UTF-16 units without a terminator; fails when one of them is NUL.
    pub fn from_units(units: impl IntoIterator<Item = u16>) -> WinResult<Self> {
        let mut buffer: Vec<u16> = units.into_iter().collect();
        if buffer.contains(&0) {
            return Err(WinError::new(E_INVALIDARG, "string contains NUL"));
        }
        buffer.push(0);
        Ok(Self(buffer))
    }

    /// The units without the terminator.
    pub fn as_units(&self) -> &[u16] {
        self.0.split_last().map_or(&[], |(_, rest)| rest)
    }

    /// The units including the terminator.
    pub fn as_units_with_nul(&self) -> &[u16] {
        &self.0
    }

    /// Lossy UTF-8 form (for tests and non-sensitive names only).
    pub fn to_string_lossy(&self) -> String {
        String::from_utf16_lossy(self.as_units())
    }

    /// Pointer for Win32 calls; valid while `self` is alive and unmodified.
    #[cfg(windows)]
    pub fn as_pcwstr(&self) -> windows::core::PCWSTR {
        windows::core::PCWSTR(self.0.as_ptr())
    }
}

impl core::fmt::Debug for WideString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // The content may be a path that contains user names; show only the length.
        f.debug_struct("WideString")
            .field("units", &self.as_units().len())
            .finish()
    }
}

/// Reads a NUL-terminated wide string from a pointer returned by Win32.
///
/// # Safety
///
/// `pointer` must be null or point to a readable, NUL-terminated UTF-16 string.
#[cfg(windows)]
pub(crate) unsafe fn read_pwstr(pointer: windows::core::PCWSTR) -> Vec<u16> {
    if pointer.is_null() {
        return Vec::new();
    }
    // SAFETY: the caller guarantees a readable NUL-terminated string; `as_wide` stops at the NUL.
    unsafe { pointer.as_wide() }.to_vec()
}
