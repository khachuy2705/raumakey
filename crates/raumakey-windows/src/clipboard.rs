//! Clipboard access: the `raumakey_converter::ClipboardPort` adapter and the snapshot used by
//! the clipboard transport (design §5.3).
//!
//! The logic (format classification, snapshot capture and conditional restore, text encoding)
//! works over the narrow [`ClipboardAccess`] port, so it is tested with a fake. On Windows,
//! [`WinClipboard`] opens the real clipboard with retries and implements both the port and
//! [`raumakey_converter::ClipboardPort`].
//!
//! Clipboard content is user data: nothing here logs it, and `Debug` output shows only formats
//! and sizes.

use core::fmt;

use raumakey_converter::ClipboardError;

/// Standard clipboard format numbers.
pub mod formats {
    /// `CF_TEXT`.
    pub const CF_TEXT: u32 = 1;
    /// `CF_BITMAP` (an `HBITMAP`).
    pub const CF_BITMAP: u32 = 2;
    /// `CF_METAFILEPICT` (contains an `HMETAFILE`).
    pub const CF_METAFILEPICT: u32 = 3;
    /// `CF_OEMTEXT`.
    pub const CF_OEMTEXT: u32 = 7;
    /// `CF_DIB`.
    pub const CF_DIB: u32 = 8;
    /// `CF_PALETTE` (an `HPALETTE`).
    pub const CF_PALETTE: u32 = 9;
    /// `CF_UNICODETEXT`.
    pub const CF_UNICODETEXT: u32 = 13;
    /// `CF_ENHMETAFILE` (an `HENHMETAFILE`).
    pub const CF_ENHMETAFILE: u32 = 14;
    /// `CF_HDROP`.
    pub const CF_HDROP: u32 = 15;
    /// `CF_LOCALE`.
    pub const CF_LOCALE: u32 = 16;
    /// `CF_DIBV5`.
    pub const CF_DIBV5: u32 = 17;
    /// `CF_OWNERDISPLAY` (no data; the owner paints).
    pub const CF_OWNERDISPLAY: u32 = 0x0080;
    /// `CF_DSPBITMAP`.
    pub const CF_DSPBITMAP: u32 = 0x0082;
    /// `CF_DSPMETAFILEPICT`.
    pub const CF_DSPMETAFILEPICT: u32 = 0x0083;
    /// `CF_DSPENHMETAFILE`.
    pub const CF_DSPENHMETAFILE: u32 = 0x008E;
}

use formats::{
    CF_BITMAP, CF_DIB, CF_DIBV5, CF_DSPBITMAP, CF_DSPENHMETAFILE, CF_DSPMETAFILEPICT,
    CF_ENHMETAFILE, CF_METAFILEPICT, CF_OWNERDISPLAY, CF_PALETTE, CF_UNICODETEXT,
};

/// Largest snapshot the transport keeps in memory.
pub const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

/// How the data of a clipboard format is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FormatStorage {
    /// An `HGLOBAL` that can be copied as bytes.
    Global,
    /// A GDI handle (bitmap, palette, metafile) that cannot be copied as bytes.
    Handle,
    /// No data or data of unknown kind (private formats, owner display, format 0).
    Unsupported,
}

/// Classifies `format`. Registered formats (`0xC000..=0xFFFF`) are `HGLOBAL`-based.
pub const fn classify_format(format: u32) -> FormatStorage {
    match format {
        0 | CF_OWNERDISPLAY | 0x0200..=0x02FF => FormatStorage::Unsupported,
        CF_BITMAP
        | CF_METAFILEPICT
        | CF_PALETTE
        | CF_ENHMETAFILE
        | CF_DSPBITMAP
        | CF_DSPMETAFILEPICT
        | CF_DSPENHMETAFILE
        | 0x0300..=0x03FF => FormatStorage::Handle,
        _ => FormatStorage::Global,
    }
}

/// Why a snapshot was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SnapshotError {
    /// The clipboard holds a format that cannot be copied as bytes; restoring without it would
    /// lose data, so the transport must not use the clipboard now.
    NonGlobalFormat(u32),
    /// The content exceeds the snapshot size limit.
    TooLarge,
    /// The clipboard failed.
    Clipboard(ClipboardError),
}

impl fmt::Display for SnapshotError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonGlobalFormat(format) => {
                write!(f, "clipboard format {format:#x} cannot be snapshotted")
            }
            Self::TooLarge => f.write_str("clipboard content is too large to snapshot"),
            Self::Clipboard(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for SnapshotError {}

impl From<ClipboardError> for SnapshotError {
    fn from(error: ClipboardError) -> Self {
        Self::Clipboard(error)
    }
}

/// What [`ClipboardSnapshot::restore_if_owner`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RestoreOutcome {
    /// The snapshot replaced Raumakey's transient content.
    Restored,
    /// Another application changed the clipboard after Raumakey's write; nothing was touched.
    SkippedNewerContent,
}

/// One open clipboard session. Implementations keep the clipboard open for their lifetime, so a
/// sequence check followed by writes is atomic with respect to other processes.
pub trait ClipboardAccess {
    /// The clipboard sequence number (it changes on every update).
    fn sequence_number(&self) -> u32;
    /// The formats currently on the clipboard, in enumeration order.
    fn available_formats(&mut self) -> Result<Vec<u32>, ClipboardError>;
    /// The bytes of an `HGLOBAL`-based format; `Ok(None)` when it is not available.
    fn read_format(&mut self, format: u32) -> Result<Option<Vec<u8>>, ClipboardError>;
    /// Empties the clipboard and takes ownership of it.
    fn clear(&mut self) -> Result<(), ClipboardError>;
    /// Adds `data` as an `HGLOBAL`-based format.
    fn write_format(&mut self, format: u32, data: &[u8]) -> Result<(), ClipboardError>;
}

/// The formats a snapshot of `available` captures: every `HGLOBAL`-based format. A bitmap or
/// palette that the system synthesizes from a captured DIB is skipped (it is synthesized again
/// after the restore); any other handle-based or unknown format refuses the snapshot.
pub fn plan_snapshot(available: &[u32]) -> Result<Vec<u32>, SnapshotError> {
    let has_dib = available.contains(&CF_DIB) || available.contains(&CF_DIBV5);
    let mut plan = Vec::with_capacity(available.len());
    for &format in available {
        match classify_format(format) {
            FormatStorage::Global => plan.push(format),
            FormatStorage::Handle if has_dib && matches!(format, CF_BITMAP | CF_PALETTE) => {}
            FormatStorage::Handle | FormatStorage::Unsupported => {
                return Err(SnapshotError::NonGlobalFormat(format));
            }
        }
    }
    Ok(plan)
}

/// A byte copy of the clipboard taken before the clipboard transport writes to it.
#[derive(Clone, PartialEq, Eq)]
pub struct ClipboardSnapshot {
    sequence: u32,
    entries: Vec<(u32, Vec<u8>)>,
}

impl fmt::Debug for ClipboardSnapshot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClipboardSnapshot")
            .field("sequence", &self.sequence)
            .field("formats", &self.entries.len())
            .field("bytes", &self.total_bytes())
            .finish()
    }
}

impl ClipboardSnapshot {
    /// Captures every `HGLOBAL`-based format, up to [`MAX_SNAPSHOT_BYTES`].
    pub fn capture<A: ClipboardAccess + ?Sized>(access: &mut A) -> Result<Self, SnapshotError> {
        Self::capture_with_limit(access, MAX_SNAPSHOT_BYTES)
    }

    /// Like [`ClipboardSnapshot::capture`] with an explicit size limit in bytes.
    pub fn capture_with_limit<A: ClipboardAccess + ?Sized>(
        access: &mut A,
        max_bytes: usize,
    ) -> Result<Self, SnapshotError> {
        let plan = plan_snapshot(&access.available_formats()?)?;
        let mut entries = Vec::with_capacity(plan.len());
        let mut total = 0_usize;
        for format in plan {
            let Some(bytes) = access.read_format(format)? else {
                continue;
            };
            total = total
                .checked_add(bytes.len())
                .filter(|total| *total <= max_bytes)
                .ok_or(SnapshotError::TooLarge)?;
            entries.push((format, bytes));
        }
        // Read last: rendering delayed formats above may have advanced it.
        Ok(Self {
            sequence: access.sequence_number(),
            entries,
        })
    }

    /// The sequence number at the end of the capture.
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }

    /// Number of captured formats.
    pub fn format_count(&self) -> usize {
        self.entries.len()
    }

    /// The captured format numbers.
    pub fn formats(&self) -> impl Iterator<Item = u32> + '_ {
        self.entries.iter().map(|(format, _)| *format)
    }

    /// Total captured bytes.
    pub fn total_bytes(&self) -> usize {
        self.entries.iter().map(|(_, bytes)| bytes.len()).sum()
    }

    /// Restores the snapshot only if the clipboard sequence number still equals
    /// `expected_sequence`, the value observed right after Raumakey's own write. Otherwise
    /// another application owns newer content and nothing is touched.
    pub fn restore_if_owner<A: ClipboardAccess + ?Sized>(
        &self,
        access: &mut A,
        expected_sequence: u32,
    ) -> Result<RestoreOutcome, ClipboardError> {
        if access.sequence_number() != expected_sequence {
            return Ok(RestoreOutcome::SkippedNewerContent);
        }
        access.clear()?;
        for (format, bytes) in &self.entries {
            access.write_format(*format, bytes)?;
        }
        Ok(RestoreOutcome::Restored)
    }
}

/// `CF_UNICODETEXT` data for `units`: UTF-16LE with a terminating NUL.
pub fn encode_unicode_units(units: &[u16]) -> Vec<u8> {
    units
        .iter()
        .chain(Some(&0))
        .flat_map(|unit| unit.to_le_bytes())
        .collect()
}

/// The UTF-16 units of `CF_UNICODETEXT` data up to its first NUL (the allocation may be
/// larger than the text). The units are not validated. Data without a NUL must have an even
/// length.
pub fn decode_unicode_units(bytes: &[u8]) -> Result<Vec<u16>, ClipboardError> {
    let (pairs, rest) = bytes.as_chunks::<2>();
    let mut units = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let unit = u16::from_le_bytes(*pair);
        if unit == 0 {
            return Ok(units);
        }
        units.push(unit);
    }
    if rest.is_empty() {
        Ok(units)
    } else {
        Err(ClipboardError::ReadFailed)
    }
}

/// The `CF_UNICODETEXT` units of the open clipboard; `Ok(None)` when there is no text.
pub fn read_unicode_units<A: ClipboardAccess + ?Sized>(
    access: &mut A,
) -> Result<Option<Vec<u16>>, ClipboardError> {
    access
        .read_format(CF_UNICODETEXT)?
        .map(|bytes| decode_unicode_units(&bytes))
        .transpose()
}

/// Empties the open clipboard and writes `units` as `CF_UNICODETEXT`, followed by `extras`
/// (format, data) pairs.
pub fn write_unicode_units<A: ClipboardAccess + ?Sized>(
    access: &mut A,
    units: &[u16],
    extras: &[(u32, Vec<u8>)],
) -> Result<(), ClipboardError> {
    access.clear()?;
    access.write_format(CF_UNICODETEXT, &encode_unicode_units(units))?;
    for (format, data) in extras {
        access.write_format(*format, data)?;
    }
    Ok(())
}

#[cfg(windows)]
pub use win::{OpenClipboardSession, WinClipboard};

#[cfg(windows)]
mod win {
    use core::marker::PhantomData;
    use std::thread;
    use std::time::Duration;

    use raumakey_converter::{ClipboardError, ClipboardPort, ClipboardText};
    use windows::Win32::Foundation::{GetLastError, GlobalFree, HANDLE, HGLOBAL, HWND};
    use windows::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
        GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
    };
    use windows::core::w;

    use super::{
        ClipboardAccess, ClipboardSnapshot, FormatStorage, RestoreOutcome, SnapshotError,
        classify_format, read_unicode_units, write_unicode_units,
    };

    /// Default number of `OpenClipboard` attempts.
    const OPEN_ATTEMPTS: u32 = 10;
    /// Default pause between attempts.
    const OPEN_RETRY_DELAY: Duration = Duration::from_millis(10);

    /// The system clipboard.
    ///
    /// Each operation opens the clipboard (retrying while another window holds it), works, and
    /// closes it again before returning.
    #[derive(Clone, Copy, Debug)]
    pub struct WinClipboard {
        owner: isize,
        attempts: u32,
        retry_delay: Duration,
    }

    impl Default for WinClipboard {
        fn default() -> Self {
            Self::new()
        }
    }

    impl WinClipboard {
        /// A clipboard opened without an owner window.
        pub const fn new() -> Self {
            Self {
                owner: 0,
                attempts: OPEN_ATTEMPTS,
                retry_delay: OPEN_RETRY_DELAY,
            }
        }

        /// A clipboard opened on behalf of window `hwnd` (recommended for writers).
        #[must_use]
        pub const fn with_owner(mut self, hwnd: isize) -> Self {
            self.owner = hwnd;
            self
        }

        /// Replaces the open retry policy.
        #[must_use]
        pub const fn with_retries(mut self, attempts: u32, delay: Duration) -> Self {
            self.attempts = attempts;
            self.retry_delay = delay;
            self
        }

        /// The clipboard sequence number (no content is read).
        pub fn sequence_number(&self) -> u32 {
            // SAFETY: no preconditions.
            unsafe { GetClipboardSequenceNumber() }
        }

        /// Opens the clipboard for one session; it closes when the session is dropped.
        pub fn open(&self) -> Result<OpenClipboardSession, ClipboardError> {
            let owner = (self.owner != 0).then_some(HWND(self.owner as *mut core::ffi::c_void));
            for attempt in 0..self.attempts.max(1) {
                // SAFETY: `owner` is either absent or a window handle supplied by the caller.
                if unsafe { OpenClipboard(owner) }.is_ok() {
                    return Ok(OpenClipboardSession {
                        _thread_bound: PhantomData,
                    });
                }
                if attempt + 1 < self.attempts {
                    thread::sleep(self.retry_delay);
                }
            }
            Err(ClipboardError::Unavailable)
        }

        /// Captures a snapshot for the clipboard transport.
        pub fn capture_snapshot(&self) -> Result<ClipboardSnapshot, SnapshotError> {
            let mut session = self.open()?;
            ClipboardSnapshot::capture(&mut session)
        }

        /// Restores `snapshot` if the sequence number still equals `expected_sequence`.
        pub fn restore_if_owner(
            &self,
            snapshot: &ClipboardSnapshot,
            expected_sequence: u32,
        ) -> Result<RestoreOutcome, ClipboardError> {
            let mut session = self.open()?;
            snapshot.restore_if_owner(&mut session, expected_sequence)
        }

        /// Writes transient transport text and returns the sequence number observed before the
        /// clipboard was closed (the proof of ownership for [`WinClipboard::restore_if_owner`]).
        ///
        /// The text is marked so that clipboard history, cloud clipboard, and clipboard
        /// monitors skip it (`ExcludeClipboardContentFromMonitorProcessing`,
        /// `CanIncludeInClipboardHistory = 0`, `CanUploadToCloudClipboard = 0`).
        pub fn write_transient_units(&self, units: &[u16]) -> Result<u32, ClipboardError> {
            let zero = 0_u32.to_le_bytes().to_vec();
            let mut extras = Vec::with_capacity(3);
            for name in [
                w!("ExcludeClipboardContentFromMonitorProcessing"),
                w!("CanIncludeInClipboardHistory"),
                w!("CanUploadToCloudClipboard"),
            ] {
                // SAFETY: the names are static NUL-terminated strings.
                let format = unsafe { RegisterClipboardFormatW(name) };
                if format != 0 {
                    extras.push((format, zero.clone()));
                }
            }
            let mut session = self.open()?;
            write_unicode_units(&mut session, units, &extras)?;
            Ok(session.sequence_number())
        }
    }

    impl ClipboardPort for WinClipboard {
        fn read_text(&mut self) -> Result<Option<ClipboardText>, ClipboardError> {
            let mut session = self.open()?;
            let Some(units) = read_unicode_units(&mut session)? else {
                return Ok(None);
            };
            // Read after the text (delayed rendering) and before closing.
            let sequence = session.sequence_number();
            Ok(Some(ClipboardText { units, sequence }))
        }

        fn write_text_if_unchanged(
            &mut self,
            expected_sequence: u32,
            units: &[u16],
        ) -> Result<bool, ClipboardError> {
            let mut session = self.open()?;
            if session.sequence_number() != expected_sequence {
                return Ok(false);
            }
            write_unicode_units(&mut session, units, &[])?;
            Ok(true)
        }
    }

    /// The clipboard while this thread holds it open. It is closed on drop and cannot leave
    /// the thread (`CloseClipboard` must run on the opening thread).
    #[derive(Debug)]
    pub struct OpenClipboardSession {
        _thread_bound: PhantomData<*const ()>,
    }

    impl Drop for OpenClipboardSession {
        fn drop(&mut self) {
            // SAFETY: this thread opened the clipboard when the session was created.
            let _ = unsafe { CloseClipboard() };
        }
    }

    impl ClipboardAccess for OpenClipboardSession {
        fn sequence_number(&self) -> u32 {
            // SAFETY: no preconditions.
            unsafe { GetClipboardSequenceNumber() }
        }

        fn available_formats(&mut self) -> Result<Vec<u32>, ClipboardError> {
            let mut formats = Vec::new();
            let mut format = 0_u32;
            loop {
                // SAFETY: the clipboard is open on this thread.
                format = unsafe { EnumClipboardFormats(format) };
                if format == 0 {
                    // SAFETY: reads the last-error value of this thread.
                    return if unsafe { GetLastError() }.0 == 0 {
                        Ok(formats)
                    } else {
                        Err(ClipboardError::ReadFailed)
                    };
                }
                formats.push(format);
            }
        }

        fn read_format(&mut self, format: u32) -> Result<Option<Vec<u8>>, ClipboardError> {
            if classify_format(format) != FormatStorage::Global {
                return Err(ClipboardError::ReadFailed);
            }
            // SAFETY: the clipboard is open on this thread.
            if unsafe { IsClipboardFormatAvailable(format) }.is_err() {
                return Ok(None);
            }
            // SAFETY: the clipboard is open; for an HGLOBAL-based format the handle is an
            // HGLOBAL owned by the clipboard, valid until the clipboard is closed or changed.
            let handle =
                unsafe { GetClipboardData(format) }.map_err(|_| ClipboardError::ReadFailed)?;
            let memory = HGLOBAL(handle.0);
            // SAFETY: `memory` is a valid global memory handle.
            let size = unsafe { GlobalSize(memory) };
            if size == 0 {
                return Ok(Some(Vec::new()));
            }
            // SAFETY: as above; the lock is released below on every path.
            let data = unsafe { GlobalLock(memory) };
            if data.is_null() {
                return Err(ClipboardError::ReadFailed);
            }
            // SAFETY: the locked block is readable for `size` bytes while locked.
            let bytes = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) }.to_vec();
            // SAFETY: balances the GlobalLock above; the result reports the lock count only.
            let _ = unsafe { GlobalUnlock(memory) };
            Ok(Some(bytes))
        }

        fn clear(&mut self) -> Result<(), ClipboardError> {
            // SAFETY: the clipboard is open on this thread.
            unsafe { EmptyClipboard() }.map_err(|_| ClipboardError::WriteFailed)
        }

        fn write_format(&mut self, format: u32, data: &[u8]) -> Result<(), ClipboardError> {
            if classify_format(format) != FormatStorage::Global {
                return Err(ClipboardError::WriteFailed);
            }
            // SAFETY: allocates a movable block of at least one byte; freed below unless the
            // clipboard takes ownership.
            let memory = unsafe { GlobalAlloc(GMEM_MOVEABLE, data.len().max(1)) }
                .map_err(|_| ClipboardError::WriteFailed)?;
            // SAFETY: `memory` was just allocated.
            let target = unsafe { GlobalLock(memory) };
            if target.is_null() {
                // SAFETY: frees the block allocated above, which nobody else owns.
                let _ = unsafe { GlobalFree(Some(memory)) };
                return Err(ClipboardError::WriteFailed);
            }
            // SAFETY: the locked block holds at least `data.len()` bytes and does not overlap
            // `data`.
            unsafe {
                core::ptr::copy_nonoverlapping(data.as_ptr(), target.cast::<u8>(), data.len());
                let _ = GlobalUnlock(memory);
            }
            // SAFETY: the clipboard is open and emptied by this thread; on success the system
            // owns `memory`.
            match unsafe { SetClipboardData(format, Some(HANDLE(memory.0))) } {
                Ok(_) => Ok(()),
                Err(_) => {
                    // SAFETY: the clipboard did not take the block, so it is still ours.
                    let _ = unsafe { GlobalFree(Some(memory)) };
                    Err(ClipboardError::WriteFailed)
                }
            }
        }
    }
}
