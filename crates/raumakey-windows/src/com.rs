//! COM apartment guard.

use core::marker::PhantomData;

use windows::Win32::System::Com::{
    COINIT, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, COINIT_MULTITHREADED, CoInitializeEx,
    CoUninitialize,
};

use crate::error::{WinError, WinResult};

/// `RPC_E_CHANGED_MODE`: the thread is already in an apartment of the other kind.
const RPC_E_CHANGED_MODE: i32 = 0x8001_0106_u32 as i32;

/// The apartment a [`ComApartment`] asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ApartmentKind {
    /// Single-threaded apartment (UI threads, shell objects, TSF).
    SingleThreaded,
    /// Multithreaded apartment.
    MultiThreaded,
}

/// Initializes COM on the current thread for its lifetime.
///
/// `CoInitializeEx` succeeding (`S_OK`, or `S_FALSE` when already initialized in the same
/// mode) is balanced by `CoUninitialize` on drop. When the thread is already in the other
/// apartment kind (`RPC_E_CHANGED_MODE`) COM is usable but this guard does not own the
/// initialization and does not uninitialize. The guard is neither `Send` nor `Sync`, because
/// `CoUninitialize` must run on the initializing thread.
#[derive(Debug)]
pub struct ComApartment {
    kind: ApartmentKind,
    owns: bool,
    _thread_bound: PhantomData<*const ()>,
}

impl ComApartment {
    /// Enters (or joins) a single-threaded apartment.
    pub fn sta() -> WinResult<Self> {
        Self::init(
            ApartmentKind::SingleThreaded,
            COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
        )
    }

    /// Enters (or joins) the multithreaded apartment.
    pub fn mta() -> WinResult<Self> {
        Self::init(
            ApartmentKind::MultiThreaded,
            COINIT_MULTITHREADED | COINIT_DISABLE_OLE1DDE,
        )
    }

    fn init(kind: ApartmentKind, flags: COINIT) -> WinResult<Self> {
        // SAFETY: plain initialization of the calling thread; balanced in Drop when owned.
        let result = unsafe { CoInitializeEx(None, flags) };
        let owns = if result.is_ok() {
            true
        } else if result.0 == RPC_E_CHANGED_MODE {
            false
        } else {
            return Err(WinError::new(result.0, "CoInitializeEx"));
        };
        Ok(Self {
            kind,
            owns,
            _thread_bound: PhantomData,
        })
    }

    /// The apartment kind that was requested.
    pub const fn kind(&self) -> ApartmentKind {
        self.kind
    }

    /// Whether this guard will call `CoUninitialize` when dropped.
    pub const fn owns_initialization(&self) -> bool {
        self.owns
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.owns {
            // SAFETY: balances the successful CoInitializeEx of this guard on the same thread
            // (the guard is !Send).
            unsafe { CoUninitialize() };
        }
    }
}
