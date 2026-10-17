//! TIP side of the legacy bridge.

use std::time::Duration;

use raumakey_model::utf16;

use super::channel::{
    LegacyChannel, LegacyEvent, LockResult, WaitResult, read_block, read_u32, write_u32,
};
use super::{
    LegacyEdit, LegacyError, LegacyOpcode, MAX_DELETE_UNITS, MUTEX_WAIT, OFF_OPCODE,
    OFF_REQUEST_SEQ, OFF_RESET_GATE, RESPONSE_WAIT_MS, next_sequence, validate_response,
};

/// Stale response signals consumed before a request (one is enough for an auto-reset event;
/// the bound guards against a peer that keeps signaling).
const MAX_STALE_DRAINS: usize = 4;
/// Responses with another sequence tolerated while waiting for the matching one. It also
/// bounds the total wait when the clock is coarse (the system tick advances in ~16 ms steps).
const MAX_RESPONSE_WAKEUPS: usize = 4;
/// Exchanges in a row without a usable response (mutex or response timeouts, unmatched
/// responses) after which the bridge is dropped and must be re-synchronized.
pub const MAX_CONSECUTIVE_FAILURES: u8 = 3;

/// What [`LegacyClient::reset`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ResetOutcome {
    /// The reset gate (`+0x24`) is 1: the controller acknowledged an op 3 request.
    Remote,
    /// The gate is off (or not 1), or the bridge is down: only local state is reset.
    LocalOnly,
}

/// TIP-side client of the legacy shared-memory protocol.
///
/// Every request locks the mutex for at most 100 ms, drains a stale response signal,
/// increments the shared request sequence, signals the request, and waits at most 50 ms for a
/// response carrying the same sequence. On any failure nothing is applied and the mutex is
/// released; the caller passes the original key through and resets its context
/// ([`super::transaction_for`]). A request that timed out is never retried, because a late
/// response could otherwise be applied twice. Malformed or stale responses, lost handles, and
/// [`MAX_CONSECUTIVE_FAILURES`] failed exchanges in a row drop the bridge; the caller then
/// reconnects with backoff.
///
/// The client holds the legacy objects open only while the bridge is up or a
/// [`LegacyClient::connect`] attempt is running. Every bridge reset, failed connect,
/// [`LegacyClient::disconnect`], and drop closes its handles ([`LegacyChannel::close`]). A TIP
/// therefore never keeps the objects of an exited controller alive between attempts, which
/// would make the fail-closed activation of a restarted controller refuse to start.
#[derive(Debug)]
pub struct LegacyClient<C: LegacyChannel> {
    channel: C,
    connected: bool,
    /// `connect` may have opened handles that have not been closed yet.
    opened: bool,
    max_delete: u32,
    failures: u8,
}

impl<C: LegacyChannel> LegacyClient<C> {
    /// A disconnected client that holds no handle.
    pub const fn new(channel: C) -> Self {
        Self {
            channel,
            connected: false,
            opened: false,
            max_delete: MAX_DELETE_UNITS,
            failures: 0,
        }
    }

    /// The channel.
    pub const fn channel(&self) -> &C {
        &self.channel
    }

    /// True after a successful op 5 sync and until a bridge reset.
    pub const fn is_connected(&self) -> bool {
        self.connected
    }

    /// Largest accepted delete count.
    pub const fn max_delete(&self) -> u32 {
        self.max_delete
    }

    /// Narrows the accepted delete count (for example to the host policy maximum); values above
    /// [`MAX_DELETE_UNITS`] are clamped.
    pub fn set_max_delete(&mut self, max_delete: u32) {
        self.max_delete = max_delete.min(MAX_DELETE_UNITS);
    }

    /// Closes the handles of an earlier bridge, opens the mutex (creating it when the
    /// controller has not started yet), opens the mapping and events, and performs the op 5
    /// sync. Characters are refused until this succeeds. On failure every handle it opened is
    /// closed again.
    pub fn connect(&mut self) -> Result<(), LegacyError> {
        self.release();
        self.opened = true;
        let synced = self
            .channel
            .open_or_create_mutex()
            .and_then(|_| self.channel.open_controller_objects())
            .and_then(|()| self.locked_exchange(LegacyOpcode::Sync, 0, false, false));
        match synced {
            Ok(_) => {
                self.connected = true;
                Ok(())
            }
            Err(error) => {
                self.release();
                Err(error)
            }
        }
    }

    /// Drops the bridge and closes every handle (for example on TIP deactivation); the next
    /// request needs [`LegacyClient::connect`]. Dropping the client does the same.
    pub fn disconnect(&mut self) {
        self.release();
    }

    /// Marks the bridge as down and closes the handles a connect attempt opened.
    fn release(&mut self) {
        self.connected = false;
        self.failures = 0;
        if core::mem::replace(&mut self.opened, false) {
            self.channel.close();
        }
    }

    /// Op 1 for one UTF-16 code unit. Surrogates are refused without a request because a lone
    /// surrogate is not a character.
    pub fn process_char(
        &mut self,
        unit: u16,
        shift: bool,
        caps: bool,
    ) -> Result<LegacyEdit, LegacyError> {
        if !self.connected {
            return Err(LegacyError::NotConnected);
        }
        if utf16::is_high_surrogate(unit) || utf16::is_low_surrogate(unit) {
            return Err(LegacyError::UnsupportedChar);
        }
        self.transact(LegacyOpcode::ProcessChar, unit, shift, caps)
    }

    /// Op 2.
    pub fn backspace(&mut self) -> Result<LegacyEdit, LegacyError> {
        if !self.connected {
            return Err(LegacyError::NotConnected);
        }
        self.transact(LegacyOpcode::Backspace, 0, false, false)
    }

    /// Op 3, gated by `+0x24`: a gate of 1 sends the request; 0 (and any value that is not 1,
    /// which is not interpreted) resets only local state. The response output is ignored.
    ///
    /// A local-only reset is not an exchange, so it leaves the count of consecutive failed
    /// exchanges untouched: a TIP that resets its context after every failed key (spec §6)
    /// still drops the bridge after [`MAX_CONSECUTIVE_FAILURES`] of them.
    pub fn reset(&mut self) -> Result<ResetOutcome, LegacyError> {
        if !self.connected {
            return Ok(ResetOutcome::LocalOnly);
        }
        let gate = match read_u32(&self.channel, OFF_RESET_GATE) {
            Ok(gate) => gate,
            Err(error) => return self.settle(Err(error)),
        };
        if gate != 1 {
            return Ok(ResetOutcome::LocalOnly);
        }
        self.transact(LegacyOpcode::Reset, 0, false, false)?;
        Ok(ResetOutcome::Remote)
    }

    /// One exchange plus the bookkeeping of its outcome.
    fn transact(
        &mut self,
        opcode: LegacyOpcode,
        unit: u16,
        shift: bool,
        caps: bool,
    ) -> Result<LegacyEdit, LegacyError> {
        let result = self.locked_exchange(opcode, unit, shift, caps);
        self.settle(result)
    }

    /// Drops the bridge (closing its handles) after an error that requires a re-sync, or after
    /// [`MAX_CONSECUTIVE_FAILURES`] exchanges in a row produced no usable response (a hung or
    /// exited controller would otherwise cost every key the full wait). Success resets the
    /// count.
    fn settle<T>(&mut self, result: Result<T, LegacyError>) -> Result<T, LegacyError> {
        match &result {
            Ok(_) => self.failures = 0,
            Err(error) if error.resets_bridge() => self.release(),
            Err(
                LegacyError::MutexTimeout
                | LegacyError::ResponseTimeout
                | LegacyError::SequenceMismatch { .. },
            ) => {
                self.failures = self.failures.saturating_add(1);
                if self.failures >= MAX_CONSECUTIVE_FAILURES {
                    self.release();
                }
            }
            Err(_) => {}
        }
        result
    }

    /// One locked request/response exchange. The mutex is released on every path after it
    /// was acquired.
    fn locked_exchange(
        &self,
        opcode: LegacyOpcode,
        unit: u16,
        shift: bool,
        caps: bool,
    ) -> Result<LegacyEdit, LegacyError> {
        match self.channel.lock_mutex(MUTEX_WAIT)? {
            LockResult::Acquired | LockResult::Abandoned => {}
            LockResult::TimedOut => return Err(LegacyError::MutexTimeout),
        }
        let outcome = self.exchange(opcode, unit, shift, caps);
        let released = self.channel.unlock_mutex();
        match (outcome, released) {
            (Ok(edit), Ok(())) => Ok(edit),
            (Err(error), _) | (Ok(_), Err(error)) => Err(error),
        }
    }

    fn exchange(
        &self,
        opcode: LegacyOpcode,
        unit: u16,
        shift: bool,
        caps: bool,
    ) -> Result<LegacyEdit, LegacyError> {
        // A response signaled before this request belongs to an earlier one (matrix #33).
        for _ in 0..MAX_STALE_DRAINS {
            if self.channel.wait(LegacyEvent::Response, Duration::ZERO)? == WaitResult::TimedOut {
                break;
            }
        }
        // Advance the shared sequence as a raw u32 (matrix #30); the mutex makes it atomic.
        let sequence = next_sequence(read_u32(&self.channel, OFF_REQUEST_SEQ)?);
        let mut request = [0_u8; 16];
        let fields = [
            opcode.as_u32(),
            u32::from(unit),
            u32::from(shift),
            u32::from(caps),
        ];
        for (slot, field) in request.as_chunks_mut::<4>().0.iter_mut().zip(fields) {
            *slot = field.to_le_bytes();
        }
        self.channel.write_shared(OFF_OPCODE, &request)?;
        write_u32(&self.channel, OFF_REQUEST_SEQ, sequence)?;
        self.channel.signal(LegacyEvent::Request)?;

        // Accept only a response with this sequence within 50 ms (matrix #32, #34).
        let deadline = self.channel.now_ms().saturating_add(RESPONSE_WAIT_MS);
        let mut mismatch = None;
        for _ in 0..MAX_RESPONSE_WAKEUPS {
            let left = deadline.saturating_sub(self.channel.now_ms());
            if left == 0 {
                break;
            }
            match self
                .channel
                .wait(LegacyEvent::Response, Duration::from_millis(left))?
            {
                WaitResult::TimedOut => break,
                WaitResult::Signaled => {
                    let block = read_block(&self.channel)?;
                    match validate_response(
                        &block,
                        sequence,
                        self.channel.now_ms(),
                        self.max_delete,
                    ) {
                        Err(error @ LegacyError::SequenceMismatch { .. }) => {
                            mismatch = Some(error);
                        }
                        other => return other,
                    }
                }
            }
        }
        Err(mismatch.unwrap_or(LegacyError::ResponseTimeout))
    }
}

impl<C: LegacyChannel> Drop for LegacyClient<C> {
    /// Closes the handles of a live bridge, as [`LegacyClient::disconnect`] does.
    fn drop(&mut self) {
        self.release();
    }
}
