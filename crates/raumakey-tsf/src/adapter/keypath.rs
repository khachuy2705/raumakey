//! The order of the work of a key event: the core call, with its edit session, first; the
//! replay afterwards.

use core::cell::RefCell;

use raumakey_model::input::ReplayKey;

use crate::session::{DocumentPort, KeyStroke, ServiceCore};

/// Runs `body` on the core unless a call into it is in progress (TSF called back into the
/// text service from inside one), in which case nothing runs and `None` is returned.
pub fn with_core<T>(
    core: &RefCell<ServiceCore>,
    body: impl FnOnce(&mut ServiceCore) -> T,
) -> Option<T> {
    let mut core = core.try_borrow_mut().ok()?;
    Some(body(&mut core))
}

/// `OnKeyDown`: runs [`ServiceCore::key_down`] for `key` on `document`, then, when the
/// outcome carries a replay key, calls `replay` with it, after the core call has returned
/// and its edit session has ended. Returns whether the key is eaten, or `None` when a call
/// into the core is in progress (the key then reaches the application).
///
/// The COM layer sends the replay with `SendInput`, which only appends the key to the input
/// stream: the application receives it after `OnKeyDown` has returned.
pub fn key_down(
    core: &RefCell<ServiceCore>,
    key: &KeyStroke,
    document: &mut dyn DocumentPort,
    replay: impl FnOnce(ReplayKey),
) -> Option<bool> {
    // The borrow of the core ends with this call, and the edit session with `key_down`.
    let outcome = with_core(core, |core| core.key_down(key, document))?;
    if let Some(key) = outcome.replay {
        replay(key);
    }
    Some(outcome.eaten)
}
