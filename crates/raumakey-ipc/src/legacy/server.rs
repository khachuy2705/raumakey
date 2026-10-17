//! Controller side of the legacy bridge.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::channel::{
    LegacyChannel, LegacyEvent, WaitResult, activate_controller, read_block, write_u32, write_u64,
};
use super::{
    LegacyEdit, LegacyError, LegacyOpcode, LegacyRequest, MAX_DELETE_UNITS, OFF_DELETE_COUNT,
    OFF_EPOCH, OFF_OUTPUT, OFF_OUTPUT_LEN, OFF_RESET_GATE, OFF_RESPONSE_SEQ, RejectReason,
    SHUTDOWN_WAIT, decode_request,
};

/// How long [`LegacyServer::run`] waits for a request before re-checking for a stop.
const RUN_POLL: Duration = Duration::from_millis(250);
/// [`LegacyServer`]'s record of the last handled sequence before the first request (no `u32`
/// sequence equals it).
const NO_SEQUENCE: u64 = u64::MAX;

/// Controller configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LegacyServerConfig {
    /// Value published in the reset gate (`+0x24`): TIPs send op 3 only when it is set.
    pub reset_gate: bool,
    /// Largest delete count a handler may return (clamped to [`MAX_DELETE_UNITS`]).
    pub max_delete: u32,
}

impl Default for LegacyServerConfig {
    fn default() -> Self {
        Self {
            reset_gate: true,
            max_delete: MAX_DELETE_UNITS,
        }
    }
}

/// What [`LegacyServer::serve_one`] did.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ServeOutcome {
    /// No request arrived within the wait.
    Idle,
    /// A request was answered.
    Served(LegacyOpcode),
    /// A request or the handler's reply was invalid; no response was written, so the TIP
    /// times out and passes its key through.
    Rejected(RejectReason),
    /// The block still holds the request the handler received last: the request event was
    /// signaled again for it, for example after the worker woke for one request, was delayed
    /// past the TIP's response wait, and answered the TIP's next request while that request's
    /// signal stayed pending. The handler is not called again and nothing is written, so no
    /// key reaches the engine twice.
    Duplicate,
    /// A stop was requested; nothing was served.
    Stopped,
}

/// Result of [`LegacyServer::shutdown`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShutdownOutcome {
    /// The worker stopped within the limit.
    Completed,
    /// The worker was still running when the limit elapsed.
    TimedOut,
}

/// Controller-side server of the legacy protocol.
///
/// [`LegacyServer::start`] creates the mapping and both events (failing closed when any
/// exists) and initializes the block before returning, so the objects are ready before a TIP
/// request can be served. One worker thread calls [`LegacyServer::run`]; another thread ends
/// it with [`LegacyServer::shutdown`], which stops accepting requests, wakes the worker, waits
/// at most [`SHUTDOWN_WAIT`], and closes the objects.
///
/// The handler receives each request sequence at most once ([`ServeOutcome::Duplicate`]).
#[derive(Debug)]
pub struct LegacyServer<C> {
    channel: C,
    config: LegacyServerConfig,
    /// Request sequence the handler received last ([`NO_SEQUENCE`] before the first).
    handled: AtomicU64,
    stop: AtomicBool,
    running: Mutex<bool>,
    stopped: Condvar,
}

impl<C: LegacyChannel> LegacyServer<C> {
    /// Activates the controller role and publishes the reset gate and the current tick.
    ///
    /// Fails closed with [`LegacyError::ConflictingObject`] while the mapping or either event
    /// exists, including the objects of an exited controller that a TIP still holds; the
    /// caller retries with backoff (see [`activate_controller`]).
    pub fn start(channel: C, config: LegacyServerConfig) -> Result<Self, LegacyError> {
        activate_controller(&channel)?;
        let initialized = write_u32(&channel, OFF_RESET_GATE, u32::from(config.reset_gate))
            .and_then(|()| write_u64(&channel, OFF_EPOCH, channel.now_ms()));
        if let Err(error) = initialized {
            channel.close();
            return Err(error);
        }
        Ok(Self {
            channel,
            config: LegacyServerConfig {
                max_delete: config.max_delete.min(MAX_DELETE_UNITS),
                ..config
            },
            handled: AtomicU64::new(NO_SEQUENCE),
            stop: AtomicBool::new(false),
            running: Mutex::new(false),
            stopped: Condvar::new(),
        })
    }

    /// The channel.
    pub const fn channel(&self) -> &C {
        &self.channel
    }

    /// The effective configuration.
    pub const fn config(&self) -> LegacyServerConfig {
        self.config
    }

    /// Waits up to `wait` for one request and answers it with `handler`. Invalid requests and
    /// invalid replies get no response. The response fields are written before the response
    /// sequence, which is written last, and the response event is signaled after it.
    ///
    /// A request whose sequence the handler already received is not handed to it again
    /// ([`ServeOutcome::Duplicate`]): TIPs increment the shared sequence for every request,
    /// so a repeated sequence means a repeated signal, never a new key.
    pub fn serve_one<F>(&self, wait: Duration, handler: &mut F) -> Result<ServeOutcome, LegacyError>
    where
        F: FnMut(LegacyRequest) -> LegacyEdit + ?Sized,
    {
        if self.stop_requested() {
            return Ok(ServeOutcome::Stopped);
        }
        if self.channel.wait(LegacyEvent::Request, wait)? == WaitResult::TimedOut {
            return Ok(ServeOutcome::Idle);
        }
        if self.stop_requested() {
            return Ok(ServeOutcome::Stopped);
        }
        let block = read_block(&self.channel)?;
        let (sequence, request) = match decode_request(&block) {
            Ok(decoded) => decoded,
            Err(reason) => return Ok(ServeOutcome::Rejected(reason)),
        };
        // Recorded before the handler runs: once it has seen a request, it never sees that
        // request again, whatever happens to the reply.
        let sequence_id = u64::from(sequence);
        if self.handled.swap(sequence_id, Ordering::AcqRel) == sequence_id {
            return Ok(ServeOutcome::Duplicate);
        }
        let edit = handler(request);
        if edit.validate(self.config.max_delete).is_err() {
            return Ok(ServeOutcome::Rejected(RejectReason::InvalidReply));
        }
        self.respond(sequence, &edit)?;
        Ok(ServeOutcome::Served(request.opcode()))
    }

    /// Worker loop: serves requests until a stop is requested or the channel fails.
    pub fn run<F>(&self, mut handler: F) -> Result<(), LegacyError>
    where
        F: FnMut(LegacyRequest) -> LegacyEdit,
    {
        let _running = Running::enter(&self.running, &self.stopped);
        loop {
            if self.serve_one(RUN_POLL, &mut handler)? == ServeOutcome::Stopped {
                return Ok(());
            }
        }
    }

    /// True while a worker is inside [`LegacyServer::run`].
    pub fn is_running(&self) -> bool {
        *lock(&self.running)
    }

    /// Stops accepting requests without waiting.
    pub fn request_stop(&self) {
        self.stop.store(true, Ordering::Release);
    }

    /// Shuts down with the [`SHUTDOWN_WAIT`] limit.
    pub fn shutdown(&self) -> ShutdownOutcome {
        self.shutdown_within(SHUTDOWN_WAIT)
    }

    /// Stops accepting requests, signals the request event to wake the worker, waits at most
    /// `limit` for it to leave [`LegacyServer::run`], then closes the objects (also when the
    /// wait timed out).
    ///
    /// After a timeout the close runs while the worker may still be inside a channel call or
    /// its handler. [`LegacyChannel::close`] requires adapters to make that safe: the worker's
    /// later calls fail with [`LegacyError::ObjectMissing`] and `run` returns that error.
    pub fn shutdown_within(&self, limit: Duration) -> ShutdownOutcome {
        self.request_stop();
        // Best effort: an error only means that no worker can be waiting on the event.
        let _ = self.channel.signal(LegacyEvent::Request);
        let running = lock(&self.running);
        let (running, _) = self
            .stopped
            .wait_timeout_while(running, limit, |running| *running)
            .unwrap_or_else(PoisonError::into_inner);
        let outcome = if *running {
            ShutdownOutcome::TimedOut
        } else {
            ShutdownOutcome::Completed
        };
        drop(running);
        self.channel.close();
        outcome
    }

    fn stop_requested(&self) -> bool {
        self.stop.load(Ordering::Acquire)
    }

    fn respond(&self, sequence: u32, edit: &LegacyEdit) -> Result<(), LegacyError> {
        let bytes: Vec<u8> = edit
            .insert_utf16
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        let len =
            u32::try_from(bytes.len()).map_err(|_| LegacyError::OutputTooLong { len: u32::MAX })?;
        if !bytes.is_empty() {
            self.channel.write_shared(OFF_OUTPUT, &bytes)?;
        }
        write_u32(&self.channel, OFF_OUTPUT_LEN, len)?;
        write_u32(&self.channel, OFF_DELETE_COUNT, edit.delete_before)?;
        write_u64(&self.channel, OFF_EPOCH, self.channel.now_ms())?;
        write_u32(&self.channel, OFF_RESPONSE_SEQ, sequence)?;
        self.channel.signal(LegacyEvent::Response)
    }
}

/// Marks a worker as running for its lifetime, also when the handler unwinds.
struct Running<'a> {
    flag: &'a Mutex<bool>,
    stopped: &'a Condvar,
}

impl<'a> Running<'a> {
    fn enter(flag: &'a Mutex<bool>, stopped: &'a Condvar) -> Self {
        *lock(flag) = true;
        Self { flag, stopped }
    }
}

impl Drop for Running<'_> {
    fn drop(&mut self) {
        *lock(self.flag) = false;
        self.stopped.notify_all();
    }
}

fn lock(flag: &Mutex<bool>) -> MutexGuard<'_, bool> {
    flag.lock().unwrap_or_else(PoisonError::into_inner)
}
