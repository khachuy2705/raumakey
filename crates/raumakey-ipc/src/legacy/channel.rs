//! Port to the legacy named objects.

use std::time::Duration;

use super::{
    BLOCK_SIZE, LegacyBlock, LegacyError, MUTEX_NAME, REQUEST_EVENT_NAME, RESPONSE_EVENT_NAME,
    SHARED_MEMORY_NAME,
};

/// The legacy named kernel objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegacyObject {
    /// The `0x1038`-byte file mapping (created by the controller).
    SharedMemory,
    /// The auto-reset request event (created by the controller).
    RequestEvent,
    /// The auto-reset response event (created by the controller).
    ResponseEvent,
    /// The mutex that serializes the request slot (opened or created by TIPs).
    Mutex,
}

impl LegacyObject {
    /// Every object.
    pub const ALL: [Self; 4] = [
        Self::SharedMemory,
        Self::RequestEvent,
        Self::ResponseEvent,
        Self::Mutex,
    ];

    /// Objects the controller creates.
    pub const CONTROLLER_OWNED: [Self; 3] =
        [Self::SharedMemory, Self::RequestEvent, Self::ResponseEvent];

    /// Kernel object name.
    pub const fn name(self) -> &'static str {
        match self {
            Self::SharedMemory => SHARED_MEMORY_NAME,
            Self::RequestEvent => REQUEST_EVENT_NAME,
            Self::ResponseEvent => RESPONSE_EVENT_NAME,
            Self::Mutex => MUTEX_NAME,
        }
    }
}

/// The two auto-reset events.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LegacyEvent {
    /// Signaled by a TIP after writing a request.
    Request,
    /// Signaled by the controller after writing a response.
    Response,
}

/// Outcome of waiting for an auto-reset event (a successful wait resets it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WaitResult {
    /// The event was signaled.
    Signaled,
    /// The timeout elapsed.
    TimedOut,
}

/// Outcome of waiting for the mutex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LockResult {
    /// The mutex is now owned.
    Acquired,
    /// The previous owner exited while holding it; it is now owned.
    Abandoned,
    /// The timeout elapsed; the mutex is not owned.
    TimedOut,
}

/// How the mutex handle was obtained.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MutexOrigin {
    /// It already existed.
    Opened,
    /// It did not exist and was created.
    Created,
}

/// Platform port over the legacy objects: the shared block, the request/response events, the
/// mutex, and a monotonic millisecond clock shared by both sides (on Windows, the system tick).
///
/// A Windows adapter implements it over a file mapping, two auto-reset events, and a named
/// mutex; [`super::fake::FakeLegacy`] implements it deterministically in memory. Shared-memory
/// accesses outside `0..BLOCK_SIZE` must fail with [`LegacyError::OutOfBounds`].
///
/// One channel stands for the handles of one process. The `create_*` and `open_*` methods give
/// it handles and [`LegacyChannel::close`] closes all of them. A named object lives while any
/// process holds a handle to it, so objects a TIP keeps open outlive the controller that
/// created them, and [`activate_controller`] fails closed while they exist.
///
/// [`super::LegacyServer`] shares its channel between the worker thread and the thread that
/// shuts it down, so an adapter must accept calls from several threads at once.
pub trait LegacyChannel {
    /// Whether the named object currently exists (in any process).
    fn object_exists(&self, object: LegacyObject) -> Result<bool, LegacyError>;
    /// Controller role: creates the zeroed mapping and both auto-reset events and keeps
    /// handles to them. Fails with [`LegacyError::ConflictingObject`] when one of them already
    /// exists.
    fn create_controller_objects(&self) -> Result<(), LegacyError>;
    /// TIP role: opens the existing mapping and both events
    /// ([`LegacyError::ObjectMissing`] when the controller has not created them).
    ///
    /// Handles opened by an earlier call are replaced, never accumulated: the adapter opens
    /// the new handles before it closes the old ones, and after a failure it holds none of
    /// these objects.
    fn open_controller_objects(&self) -> Result<(), LegacyError>;
    /// TIP role: opens the mutex, creating it when it does not exist yet. Like
    /// [`LegacyChannel::open_controller_objects`], it replaces a handle opened earlier.
    fn open_or_create_mutex(&self) -> Result<MutexOrigin, LegacyError>;
    /// Unmaps the block and closes every handle this channel holds. Objects that other
    /// processes still hold stay alive. Idempotent: closing a closed channel does nothing.
    ///
    /// It may run on one thread while another thread is inside another method:
    /// [`super::LegacyServer::shutdown_within`] closes the objects even when the worker did not
    /// stop in time. Every call that starts after `close` must fail with
    /// [`LegacyError::ObjectMissing`]. A call already in flight either completes or fails the
    /// same way, but it must never touch an unmapped view or a closed handle (Win32 leaves both
    /// undefined). For example, the adapter can hold a read lock over its handles and view for
    /// each copy, signal, and wait setup (taking the write lock in `close`), and wait on
    /// duplicated event and mutex handles that it closes itself once the wait returns.
    fn close(&self);
    /// Copies `out.len()` bytes of the block starting at `offset`.
    fn read_shared(&self, offset: usize, out: &mut [u8]) -> Result<(), LegacyError>;
    /// Copies `data` into the block at `offset`.
    fn write_shared(&self, offset: usize, data: &[u8]) -> Result<(), LegacyError>;
    /// Signals an event.
    fn signal(&self, event: LegacyEvent) -> Result<(), LegacyError>;
    /// Waits for an event for at most `timeout` (zero polls).
    fn wait(&self, event: LegacyEvent, timeout: Duration) -> Result<WaitResult, LegacyError>;
    /// Waits for the mutex for at most `timeout`.
    fn lock_mutex(&self, timeout: Duration) -> Result<LockResult, LegacyError>;
    /// Releases the owned mutex.
    fn unlock_mutex(&self) -> Result<(), LegacyError>;
    /// Monotonic clock in milliseconds.
    fn now_ms(&self) -> u64;
}

/// Controller-side activation that fails closed: when the mapping or either event already
/// exists (another controller, such as EVKey itself, is running) nothing is created and
/// [`LegacyError::ConflictingObject`] is returned. A mutex created by a TIP that started first
/// is not a conflict.
///
/// The objects of a controller that exited also count as a conflict while a TIP still holds
/// them. A [`super::LegacyClient`] holds them only while its bridge is up or a connect attempt
/// is running, and it closes them once its exchanges fail, so a controller that gets
/// [`LegacyError::ConflictingObject`] retries activation with backoff instead of taking over.
pub fn activate_controller<C: LegacyChannel + ?Sized>(channel: &C) -> Result<(), LegacyError> {
    for object in LegacyObject::CONTROLLER_OWNED {
        if channel.object_exists(object)? {
            return Err(LegacyError::ConflictingObject(object));
        }
    }
    channel.create_controller_objects()
}

pub(crate) fn read_u32<C: LegacyChannel + ?Sized>(
    channel: &C,
    offset: usize,
) -> Result<u32, LegacyError> {
    let mut raw = [0_u8; 4];
    channel.read_shared(offset, &mut raw)?;
    Ok(u32::from_le_bytes(raw))
}

pub(crate) fn write_u32<C: LegacyChannel + ?Sized>(
    channel: &C,
    offset: usize,
    value: u32,
) -> Result<(), LegacyError> {
    channel.write_shared(offset, &value.to_le_bytes())
}

pub(crate) fn write_u64<C: LegacyChannel + ?Sized>(
    channel: &C,
    offset: usize,
    value: u64,
) -> Result<(), LegacyError> {
    channel.write_shared(offset, &value.to_le_bytes())
}

/// Copies the whole block once, so that validation and use see the same bytes.
pub(crate) fn read_block<C: LegacyChannel + ?Sized>(
    channel: &C,
) -> Result<LegacyBlock, LegacyError> {
    let mut block = LegacyBlock::zeroed();
    channel.read_shared(0, &mut block.as_bytes_mut()[..BLOCK_SIZE])?;
    Ok(block)
}
