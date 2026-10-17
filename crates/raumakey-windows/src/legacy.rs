//! Windows named objects of the EVKey legacy shared-memory channel (feature `evkey-compat`,
//! plan §7.4 and §10.1).
//!
//! [`LegacyNamedObjects`] implements [`LegacyChannel`] of `raumakey-ipc`, which runs the
//! protocol ([`raumakey_ipc::legacy::LegacyClient`], [`raumakey_ipc::legacy::LegacyServer`])
//! including its timeouts, sequence matching, and response validation. This adapter only owns
//! the kernel objects:
//!
//! - a pagefile-backed file mapping of exactly [`BLOCK_SIZE`] bytes, mapped read-write by an
//!   RAII view guard (`UnmapViewOfFile` on drop); a view is used only after `VirtualQuery`
//!   proved that the whole block is committed, so a foreign mapping that is smaller or only
//!   reserved is refused instead of faulting;
//! - two auto-reset events and one mutex (opened, or created when missing, in either start
//!   order);
//! - every object it creates carries the user-only security descriptor of
//!   [`crate::security`].
//!
//! The objects stay user-only under plan D27, which opens only the control pipe, and only to
//! AppContainer hosts. The shared block carries per-key text (the code unit of a request and the
//! replacement), which D27's accepted risk does not cover, and the names are fixed and
//! well known. A TIP in an AppContainer host could not find them anyway: its `Local\` is its own
//! namespace, so opening the controller objects reports them missing. A TIP in a low-integrity
//! host finds them but is refused write access, because an object without a label counts as
//! medium with `NO_WRITE_UP`. Either way that TIP's legacy bridge stays down.
//!
//! Activation fails closed: creating the controller objects fails with
//! [`LegacyError::ConflictingObject`] when any of them already exists, including objects that
//! another process (such as EVKey itself) holds, objects of another user that cannot be
//! opened, and objects of another type that use the name. Nothing is created in that case.
//!
//! [`LegacyChannel::close`] may run while another thread is inside a call: shared-memory copies
//! and signals hold a read lock over the handles and the view (`close` takes the write lock),
//! and waits run on duplicated handles that the waiting call closes itself. A mutex owned by a
//! thread when the channel is closed stays owned by that thread until it is unlocked through a
//! reopened channel or the thread exits (other processes then see it abandoned).
//!
//! The legacy object names are the constants of `raumakey-ipc` ([`LegacyObject::name`]) in the
//! session namespace; tests use [`LegacyObjectNames::isolated`] names instead.

use core::fmt;
use std::os::windows::io::OwnedHandle;
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use raumakey_ipc::legacy::{
    BLOCK_SIZE, LegacyChannel, LegacyError, LegacyEvent, LegacyObject, LockResult, MutexOrigin,
    WaitResult,
};
use windows::Win32::Foundation::{
    DUPLICATE_SAME_ACCESS, DuplicateHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE,
    INVALID_HANDLE_VALUE, SetLastError, WAIT_ABANDONED, WAIT_EVENT, WAIT_OBJECT_0, WAIT_TIMEOUT,
    WIN32_ERROR,
};
use windows::Win32::System::Memory::{
    CreateFileMappingW, FILE_MAP_READ, FILE_MAP_WRITE, MEM_COMMIT, MEMORY_BASIC_INFORMATION,
    MEMORY_MAPPED_VIEW_ADDRESS, MapViewOfFile, OpenFileMappingW, PAGE_READWRITE, UnmapViewOfFile,
    VirtualQuery,
};
use windows::Win32::System::SystemInformation::GetTickCount64;
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexExW, EVENT_MODIFY_STATE, GetCurrentProcess, MUTEX_MODIFY_STATE,
    OpenEventW, OpenMutexW, ReleaseMutex, SYNCHRONIZATION_SYNCHRONIZE, SetEvent,
    WaitForSingleObject,
};

use crate::error::{WinError, WinResult, win32};
use crate::handle;
use crate::names::{NameError, is_valid_component};
use crate::pipe::timeout_ms;
use crate::security::UserOnlySecurity;
use crate::strings::WideString;

/// Prefix of the session namespace.
const SESSION_NAMESPACE: &str = "Local\\";
/// Prefix of [`LegacyObjectNames::isolated`] names.
const ISOLATED_PREFIX: &str = "Local\\Raumakey-Test-";

/// The kernel object names one channel uses.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct LegacyObjectNames {
    shared_memory: String,
    request_event: String,
    response_event: String,
    mutex: String,
}

impl LegacyObjectNames {
    /// The names legacy components use: `Local\` followed by [`LegacyObject::name`].
    pub fn legacy() -> Self {
        let name = |object: LegacyObject| format!("{SESSION_NAMESPACE}{}", object.name());
        Self {
            shared_memory: name(LegacyObject::SharedMemory),
            request_event: name(LegacyObject::RequestEvent),
            response_event: name(LegacyObject::ResponseEvent),
            mutex: name(LegacyObject::Mutex),
        }
    }

    /// Private names for tests and acceptance runs, `Local\Raumakey-Test-<tag>-<object>`, that
    /// never collide with the legacy names. `tag` is `[A-Za-z0-9_-]{1,64}`.
    pub fn isolated(tag: &str) -> Result<Self, NameError> {
        if !is_valid_component(tag) {
            return Err(NameError::InvalidComponent);
        }
        let name = |object: &str| format!("{ISOLATED_PREFIX}{tag}-{object}");
        Ok(Self {
            shared_memory: name("SharedMemory"),
            request_event: name("RequestEvent"),
            response_event: name("ResponseEvent"),
            mutex: name("Mutex"),
        })
    }

    /// The name of `object`.
    pub fn name(&self, object: LegacyObject) -> &str {
        match object {
            LegacyObject::SharedMemory => &self.shared_memory,
            LegacyObject::RequestEvent => &self.request_event,
            LegacyObject::ResponseEvent => &self.response_event,
            LegacyObject::Mutex => &self.mutex,
        }
    }
}

/// A read-write view of the whole block, unmapped on drop.
struct MappedView {
    base: *mut u8,
}

// SAFETY: the view is process-wide memory, not tied to the mapping thread; it is only accessed
// through raw volatile byte copies while the channel's lock keeps it mapped, and unmapped once.
unsafe impl Send for MappedView {}
// SAFETY: see `Send`; shared references only allow those locked copies.
unsafe impl Sync for MappedView {}

impl MappedView {
    /// Maps [`BLOCK_SIZE`] bytes and checks that all of them are committed memory.
    fn map(mapping: &OwnedHandle) -> Result<Self, LegacyError> {
        // SAFETY: `mapping` is a valid file-mapping handle with read and write access; a
        // failure returns a null address, which is checked.
        let address = unsafe {
            MapViewOfFile(
                handle::raw(mapping),
                FILE_MAP_READ | FILE_MAP_WRITE,
                0,
                0,
                BLOCK_SIZE,
            )
        };
        if address.Value.is_null() {
            return Err(last_channel_error());
        }
        // From here on the view is unmapped on every path.
        let view = Self {
            base: address.Value.cast(),
        };
        let mut info = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: queries this process's own address space; `info` is a valid out buffer of
        // the size passed.
        let written = unsafe {
            VirtualQuery(
                Some(view.base.cast_const().cast()),
                &mut info,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if written == 0 {
            return Err(last_channel_error());
        }
        if info.State != MEM_COMMIT || info.RegionSize < BLOCK_SIZE {
            return Err(LegacyError::Channel {
                code: win32::ERROR_INVALID_DATA,
            });
        }
        Ok(view)
    }

    /// Copies `out.len()` bytes from `offset`, which the caller checked against the block.
    fn read(&self, offset: usize, out: &mut [u8]) {
        for (index, slot) in out.iter_mut().enumerate() {
            // SAFETY: `offset + index < BLOCK_SIZE` (checked by the caller), so the byte lies
            // in the committed, mapped view. Volatile reads copy what another process may be
            // writing without assuming the bytes stay unchanged.
            *slot = unsafe { self.base.add(offset + index).read_volatile() };
        }
    }

    /// Copies `data` to `offset`, which the caller checked against the block.
    fn write(&self, offset: usize, data: &[u8]) {
        for (index, byte) in data.iter().enumerate() {
            // SAFETY: as in `read`; the view is mapped writable.
            unsafe { self.base.add(offset + index).write_volatile(*byte) };
        }
    }
}

impl Drop for MappedView {
    fn drop(&mut self) {
        // SAFETY: `base` is the address MapViewOfFile returned; it is unmapped exactly once.
        let _ = unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.base.cast(),
            })
        };
    }
}

/// The handles one channel holds. The view is declared first so it is unmapped first.
#[derive(Default)]
struct Handles {
    view: Option<MappedView>,
    mapping: Option<OwnedHandle>,
    request: Option<OwnedHandle>,
    response: Option<OwnedHandle>,
    mutex: Option<OwnedHandle>,
}

impl Handles {
    fn event(&self, event: LegacyEvent) -> Result<&OwnedHandle, LegacyError> {
        let (handle, object) = match event {
            LegacyEvent::Request => (&self.request, LegacyObject::RequestEvent),
            LegacyEvent::Response => (&self.response, LegacyObject::ResponseEvent),
        };
        handle.as_ref().ok_or(LegacyError::ObjectMissing(object))
    }

    fn mutex(&self) -> Result<&OwnedHandle, LegacyError> {
        self.mutex
            .as_ref()
            .ok_or(LegacyError::ObjectMissing(LegacyObject::Mutex))
    }

    fn view(&self) -> Result<&MappedView, LegacyError> {
        self.view
            .as_ref()
            .ok_or(LegacyError::ObjectMissing(LegacyObject::SharedMemory))
    }

    fn drop_controller_objects(&mut self) {
        self.view = None;
        self.mapping = None;
        self.request = None;
        self.response = None;
    }
}

/// The controller objects opened or created in one go.
struct ControllerObjects {
    view: MappedView,
    mapping: OwnedHandle,
    request: OwnedHandle,
    response: OwnedHandle,
}

/// [`LegacyChannel`] over Windows named objects: the handles of one process (see the module
/// documentation). Calls may come from several threads at once.
pub struct LegacyNamedObjects {
    names: LegacyObjectNames,
    wide: [WideString; 4],
    security: UserOnlySecurity,
    handles: RwLock<Handles>,
}

impl fmt::Debug for LegacyNamedObjects {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let handles = self.read();
        f.debug_struct("LegacyNamedObjects")
            .field("names", &self.names)
            .field("mapped", &handles.view.is_some())
            .field(
                "events",
                &(handles.request.is_some() && handles.response.is_some()),
            )
            .field("mutex", &handles.mutex.is_some())
            .finish()
    }
}

/// Slot of `object` in [`LegacyNamedObjects::wide`].
const fn slot(object: LegacyObject) -> usize {
    match object {
        LegacyObject::SharedMemory => 0,
        LegacyObject::RequestEvent => 1,
        LegacyObject::ResponseEvent => 2,
        LegacyObject::Mutex => 3,
    }
}

/// The OS error code of a failed call (the Win32 code for `FACILITY_WIN32` HRESULTs).
fn os_code(error: &WinError) -> u32 {
    error.win32_code().unwrap_or(error.code() as u32)
}

fn channel_error(error: &windows::core::Error) -> LegacyError {
    LegacyError::Channel {
        code: os_code(&WinError::new(error.code().0, "legacy channel")),
    }
}

fn last_channel_error() -> LegacyError {
    LegacyError::Channel {
        code: os_code(&WinError::last("legacy channel")),
    }
}

fn win32_of(error: &windows::core::Error) -> Option<u32> {
    WinError::new(error.code().0, "legacy channel").win32_code()
}

/// Duplicates `source` (same access) so a wait can run without holding the lock.
fn duplicate(source: &OwnedHandle) -> Result<OwnedHandle, LegacyError> {
    let mut target = HANDLE::default();
    // SAFETY: duplicates a valid handle of this process into this process; `target` is a
    // valid out pointer and the new handle is owned below.
    unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle::raw(source),
            GetCurrentProcess(),
            &mut target,
            0,
            false,
            DUPLICATE_SAME_ACCESS,
        )
    }
    .map_err(|error| channel_error(&error))?;
    // SAFETY: DuplicateHandle returned a new handle owned by nobody else.
    Ok(unsafe { handle::take(target) })
}

impl LegacyNamedObjects {
    /// A channel for `names` that holds no handle yet. Objects it creates get the user-only
    /// security descriptor of the current user.
    pub fn new(names: LegacyObjectNames) -> WinResult<Self> {
        let wide = [
            WideString::new(names.name(LegacyObject::SharedMemory))?,
            WideString::new(names.name(LegacyObject::RequestEvent))?,
            WideString::new(names.name(LegacyObject::ResponseEvent))?,
            WideString::new(names.name(LegacyObject::Mutex))?,
        ];
        Ok(Self {
            names,
            wide,
            security: UserOnlySecurity::for_current_user()?,
            handles: RwLock::new(Handles::default()),
        })
    }

    /// A channel for the legacy names ([`LegacyObjectNames::legacy`]).
    pub fn legacy() -> WinResult<Self> {
        Self::new(LegacyObjectNames::legacy())
    }

    /// The names this channel uses.
    pub fn names(&self) -> &LegacyObjectNames {
        &self.names
    }

    fn wide(&self, object: LegacyObject) -> &WideString {
        &self.wide[slot(object)]
    }

    fn read(&self) -> RwLockReadGuard<'_, Handles> {
        self.handles.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn write(&self) -> RwLockWriteGuard<'_, Handles> {
        self.handles.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Creates a new named object; an existing (or unopenable, or differently typed) object of
    /// that name is a conflict, and its handle is closed again.
    fn create_new(
        &self,
        object: LegacyObject,
        create: impl FnOnce() -> windows::core::Result<HANDLE>,
    ) -> Result<OwnedHandle, LegacyError> {
        // SAFETY: resets the calling thread's last-error value; no other effect.
        unsafe { SetLastError(WIN32_ERROR(0)) };
        let created = create();
        // SAFETY: reads the calling thread's last-error value right after the call.
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        match created {
            Ok(raw) => {
                // SAFETY: the create call returned a new handle owned by nobody else.
                let owned = unsafe { handle::take(raw) };
                if existed {
                    return Err(LegacyError::ConflictingObject(object));
                }
                Ok(owned)
            }
            Err(error) => match win32_of(&error) {
                Some(win32::ERROR_ACCESS_DENIED | win32::ERROR_INVALID_HANDLE) => {
                    Err(LegacyError::ConflictingObject(object))
                }
                _ => Err(channel_error(&error)),
            },
        }
    }

    fn create_controller(&self) -> Result<ControllerObjects, LegacyError> {
        let attributes = Some(self.security.as_ptr());
        let mapping = self.create_new(LegacyObject::SharedMemory, || {
            // SAFETY: a pagefile-backed mapping (zero-filled) of BLOCK_SIZE bytes; the
            // security attributes and the name outlive the call.
            unsafe {
                CreateFileMappingW(
                    INVALID_HANDLE_VALUE,
                    attributes,
                    PAGE_READWRITE,
                    0,
                    BLOCK_SIZE as u32,
                    self.wide(LegacyObject::SharedMemory).as_pcwstr(),
                )
            }
        })?;
        let view = MappedView::map(&mapping)?;
        let event = |object: LegacyObject| {
            self.create_new(object, || {
                // SAFETY: an auto-reset, initially unsignaled event; the security attributes
                // and the name outlive the call.
                unsafe { CreateEventW(attributes, false, false, self.wide(object).as_pcwstr()) }
            })
        };
        let request = event(LegacyObject::RequestEvent)?;
        let response = event(LegacyObject::ResponseEvent)?;
        Ok(ControllerObjects {
            view,
            mapping,
            request,
            response,
        })
    }

    fn open_controller(&self) -> Result<ControllerObjects, LegacyError> {
        let missing = |object: LegacyObject| {
            move |error: windows::core::Error| match win32_of(&error) {
                Some(win32::ERROR_FILE_NOT_FOUND) => LegacyError::ObjectMissing(object),
                _ => channel_error(&error),
            }
        };
        // SAFETY: the name outlives the call; the returned handle is owned below.
        let mapping = unsafe {
            OpenFileMappingW(
                (FILE_MAP_READ | FILE_MAP_WRITE).0,
                false,
                self.wide(LegacyObject::SharedMemory).as_pcwstr(),
            )
        }
        .map_err(missing(LegacyObject::SharedMemory))?;
        // SAFETY: OpenFileMappingW returned a new handle owned by nobody else.
        let mapping = unsafe { handle::take(mapping) };
        let view = MappedView::map(&mapping)?;
        let event = |object: LegacyObject| {
            // SAFETY: the name outlives the call; the returned handle is owned below.
            let opened = unsafe {
                OpenEventW(
                    EVENT_MODIFY_STATE | SYNCHRONIZATION_SYNCHRONIZE,
                    false,
                    self.wide(object).as_pcwstr(),
                )
            }
            .map_err(missing(object))?;
            // SAFETY: OpenEventW returned a new handle owned by nobody else.
            Ok::<_, LegacyError>(unsafe { handle::take(opened) })
        };
        let request = event(LegacyObject::RequestEvent)?;
        let response = event(LegacyObject::ResponseEvent)?;
        Ok(ControllerObjects {
            view,
            mapping,
            request,
            response,
        })
    }

    /// Stores freshly opened or created controller objects, closing earlier ones.
    fn install(&self, objects: ControllerObjects) {
        let mut handles = self.write();
        handles.view = Some(objects.view);
        handles.mapping = Some(objects.mapping);
        handles.request = Some(objects.request);
        handles.response = Some(objects.response);
    }

    /// Waits on a duplicate of a handle, so that `close` can run meanwhile.
    fn wait_on(
        &self,
        pick: impl FnOnce(&Handles) -> Result<&OwnedHandle, LegacyError>,
        timeout: Duration,
    ) -> Result<WAIT_EVENT, LegacyError> {
        let waitable = {
            let handles = self.read();
            duplicate(pick(&handles)?)?
        };
        // SAFETY: `waitable` is a valid handle owned by this frame for the whole wait; the
        // timeout is finite.
        let waited =
            unsafe { WaitForSingleObject(handle::raw(&waitable), timeout_ms(Some(timeout))) };
        if waited == WAIT_OBJECT_0 || waited == WAIT_TIMEOUT || waited == WAIT_ABANDONED {
            Ok(waited)
        } else {
            Err(last_channel_error())
        }
    }
}

impl LegacyChannel for LegacyNamedObjects {
    fn object_exists(&self, object: LegacyObject) -> Result<bool, LegacyError> {
        let name = self.wide(object).as_pcwstr();
        // SAFETY: the name outlives each call; a returned handle is owned and closed below.
        let opened = unsafe {
            match object {
                LegacyObject::SharedMemory => OpenFileMappingW(FILE_MAP_READ.0, false, name),
                LegacyObject::RequestEvent | LegacyObject::ResponseEvent => {
                    OpenEventW(SYNCHRONIZATION_SYNCHRONIZE, false, name)
                }
                LegacyObject::Mutex => OpenMutexW(SYNCHRONIZATION_SYNCHRONIZE, false, name),
            }
        };
        match opened {
            Ok(raw) => {
                // SAFETY: the open call returned a new handle owned by nobody else; dropping it
                // closes it.
                drop(unsafe { handle::take(raw) });
                Ok(true)
            }
            Err(error) => match win32_of(&error) {
                Some(win32::ERROR_FILE_NOT_FOUND) => Ok(false),
                // Present but not ours (another user) or another object type: it exists.
                Some(win32::ERROR_ACCESS_DENIED | win32::ERROR_INVALID_HANDLE) => Ok(true),
                _ => Err(channel_error(&error)),
            },
        }
    }

    fn create_controller_objects(&self) -> Result<(), LegacyError> {
        // On failure the objects created so far are closed again, so nothing remains.
        let objects = self.create_controller()?;
        self.install(objects);
        Ok(())
    }

    fn open_controller_objects(&self) -> Result<(), LegacyError> {
        match self.open_controller() {
            Ok(objects) => {
                self.install(objects);
                Ok(())
            }
            Err(error) => {
                self.write().drop_controller_objects();
                Err(error)
            }
        }
    }

    fn open_or_create_mutex(&self) -> Result<MutexOrigin, LegacyError> {
        // SAFETY: resets the calling thread's last-error value; no other effect.
        unsafe { SetLastError(WIN32_ERROR(0)) };
        // SAFETY: the security attributes (used only when the mutex is created) and the name
        // outlive the call; the mutex is not initially owned.
        let created = unsafe {
            CreateMutexExW(
                Some(self.security.as_ptr()),
                self.wide(LegacyObject::Mutex).as_pcwstr(),
                0,
                (SYNCHRONIZATION_SYNCHRONIZE | MUTEX_MODIFY_STATE).0,
            )
        };
        // SAFETY: reads the calling thread's last-error value right after the call.
        let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let raw = created.map_err(|error| channel_error(&error))?;
        // SAFETY: CreateMutexExW returned a new handle owned by nobody else.
        let mutex = unsafe { handle::take(raw) };
        self.write().mutex = Some(mutex);
        Ok(if existed {
            MutexOrigin::Opened
        } else {
            MutexOrigin::Created
        })
    }

    fn close(&self) {
        let mut handles = self.write();
        handles.drop_controller_objects();
        handles.mutex = None;
    }

    fn read_shared(&self, offset: usize, out: &mut [u8]) -> Result<(), LegacyError> {
        let handles = self.read();
        let view = handles.view()?;
        offset
            .checked_add(out.len())
            .filter(|end| *end <= BLOCK_SIZE)
            .ok_or(LegacyError::OutOfBounds)?;
        view.read(offset, out);
        Ok(())
    }

    fn write_shared(&self, offset: usize, data: &[u8]) -> Result<(), LegacyError> {
        let handles = self.read();
        let view = handles.view()?;
        offset
            .checked_add(data.len())
            .filter(|end| *end <= BLOCK_SIZE)
            .ok_or(LegacyError::OutOfBounds)?;
        view.write(offset, data);
        Ok(())
    }

    fn signal(&self, event: LegacyEvent) -> Result<(), LegacyError> {
        let handles = self.read();
        let handle = handles.event(event)?;
        // SAFETY: a valid event handle with EVENT_MODIFY_STATE, kept open by the read lock.
        unsafe { SetEvent(handle::raw(handle)) }.map_err(|error| channel_error(&error))
    }

    fn wait(&self, event: LegacyEvent, timeout: Duration) -> Result<WaitResult, LegacyError> {
        let waited = self.wait_on(|handles| handles.event(event), timeout)?;
        Ok(if waited == WAIT_OBJECT_0 {
            WaitResult::Signaled
        } else {
            WaitResult::TimedOut
        })
    }

    fn lock_mutex(&self, timeout: Duration) -> Result<LockResult, LegacyError> {
        let waited = self.wait_on(Handles::mutex, timeout)?;
        Ok(if waited == WAIT_OBJECT_0 {
            LockResult::Acquired
        } else if waited == WAIT_ABANDONED {
            LockResult::Abandoned
        } else {
            LockResult::TimedOut
        })
    }

    fn unlock_mutex(&self) -> Result<(), LegacyError> {
        let handles = self.read();
        let mutex = handles.mutex()?;
        // SAFETY: a valid mutex handle with MUTEX_MODIFY_STATE, kept open by the read lock;
        // releasing a mutex this thread does not own fails with ERROR_NOT_OWNER.
        unsafe { ReleaseMutex(handle::raw(mutex)) }.map_err(|error| channel_error(&error))
    }

    fn now_ms(&self) -> u64 {
        // SAFETY: no preconditions; the tick is shared by every process of the system.
        unsafe { GetTickCount64() }
    }
}
