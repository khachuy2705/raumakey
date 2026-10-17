//! Deterministic in-memory [`LegacyChannel`] for tests.
//!
//! A [`FakeLegacy`] is one *endpoint* (a process) attached to a simulated namespace of named
//! objects with one simulated monotonic clock. Clones share the endpoint and its handles;
//! [`FakeLegacy::endpoint`] attaches another process to the same namespace. As with Windows
//! named objects, an object exists while any process holds a handle to it: endpoints get
//! handles through the [`LegacyChannel`] trait, and the setup helpers (such as
//! [`FakeLegacy::create_object`]) stand for processes outside the test that hold objects open.
//! After [`LegacyChannel::close`] every call of that endpoint fails with
//! [`LegacyError::ObjectMissing`], while objects other endpoints hold stay alive.
//!
//! Time advances only inside waits (by the waited duration, or to the moment a scheduled reply
//! lands) and through [`FakeLegacy::advance`], so every scenario is reproducible. A controller
//! is emulated either by scripted [`FakeReply`] values or by a peer hook that runs a real
//! [`super::LegacyServer`]. Operations performed through the [`LegacyChannel`] trait (by any
//! endpoint) are recorded in one journal; the setup helpers are not.

use std::collections::VecDeque;
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use super::channel::{
    LegacyChannel, LegacyEvent, LegacyObject, LockResult, MutexOrigin, WaitResult,
};
use super::{
    BLOCK_SIZE, LegacyBlock, LegacyError, OFF_CAPS, OFF_CHAR, OFF_DELETE_COUNT, OFF_EPOCH,
    OFF_OPCODE, OFF_OUTPUT, OFF_OUTPUT_LEN, OFF_REQUEST_SEQ, OFF_RESPONSE_SEQ, OFF_SHIFT,
    OUTPUT_CAPACITY,
};

/// Initial value of the simulated clock.
pub const FAKE_CLOCK_START_MS: u64 = 100_000;

/// `ERROR_NOT_OWNER`, reported when releasing a mutex that is not owned.
const ERROR_NOT_OWNER: u32 = 288;

/// One journaled channel operation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FakeOp {
    /// [`LegacyChannel::create_controller_objects`].
    CreateControllerObjects,
    /// [`LegacyChannel::open_controller_objects`].
    OpenControllerObjects,
    /// [`LegacyChannel::open_or_create_mutex`] and its outcome.
    OpenOrCreateMutex(MutexOrigin),
    /// [`LegacyChannel::read_shared`].
    Read {
        /// Offset.
        offset: usize,
        /// Length.
        len: usize,
    },
    /// [`LegacyChannel::write_shared`].
    Write {
        /// Offset.
        offset: usize,
        /// Bytes written.
        bytes: Vec<u8>,
    },
    /// [`LegacyChannel::signal`].
    Signal(LegacyEvent),
    /// [`LegacyChannel::wait`] and its outcome.
    Wait {
        /// Event.
        event: LegacyEvent,
        /// Timeout in milliseconds.
        timeout_ms: u64,
        /// Outcome.
        result: WaitResult,
    },
    /// [`LegacyChannel::lock_mutex`] and its outcome.
    Lock {
        /// Timeout in milliseconds.
        timeout_ms: u64,
        /// Outcome.
        result: LockResult,
    },
    /// [`LegacyChannel::unlock_mutex`].
    Unlock,
    /// [`LegacyChannel::close`].
    Close,
}

/// Tick written with a scripted reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FakeEpoch {
    /// The simulated time at which the reply lands.
    Now,
    /// That time minus the given age.
    AgeMs(u64),
    /// A fixed value.
    Absolute(u64),
}

/// A scripted controller response.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FakeReply {
    /// Delay after the request signal (or after scheduling) before the reply lands.
    pub delay_ms: u64,
    /// Response sequence; `None` echoes the request sequence.
    pub sequence: Option<u32>,
    /// Delete count.
    pub delete_count: u32,
    /// Output units (truncated to the buffer when written).
    pub output: Vec<u16>,
    /// Raw output length; `None` writes the true byte length.
    pub output_len: Option<u32>,
    /// Tick.
    pub epoch: FakeEpoch,
}

impl FakeReply {
    /// An immediate, matching, fresh reply with no edit.
    pub fn empty() -> Self {
        Self::units(0, Vec::new())
    }

    /// An immediate, matching, fresh reply that deletes `delete_count` units and inserts
    /// `text`.
    pub fn text(delete_count: u32, text: &str) -> Self {
        Self::units(delete_count, text.encode_utf16().collect())
    }

    /// Like [`FakeReply::text`] with raw (possibly ill-formed) units.
    pub fn units(delete_count: u32, output: Vec<u16>) -> Self {
        Self {
            delay_ms: 0,
            sequence: None,
            delete_count,
            output,
            output_len: None,
            epoch: FakeEpoch::Now,
        }
    }

    /// Lands `delay_ms` later.
    #[must_use]
    pub fn after_ms(mut self, delay_ms: u64) -> Self {
        self.delay_ms = delay_ms;
        self
    }

    /// Uses a fixed response sequence.
    #[must_use]
    pub fn with_sequence(mut self, sequence: u32) -> Self {
        self.sequence = Some(sequence);
        self
    }

    /// Uses a raw output length.
    #[must_use]
    pub fn with_output_len(mut self, len: u32) -> Self {
        self.output_len = Some(len);
        self
    }

    /// Uses another tick.
    #[must_use]
    pub fn with_epoch(mut self, epoch: FakeEpoch) -> Self {
        self.epoch = epoch;
        self
    }
}

/// Number of legacy objects.
const OBJECTS: usize = LegacyObject::ALL.len();

/// One flag per object, indexed by [`slot`].
type Handles = [bool; OBJECTS];

const fn slot(object: LegacyObject) -> usize {
    match object {
        LegacyObject::SharedMemory => 0,
        LegacyObject::RequestEvent => 1,
        LegacyObject::ResponseEvent => 2,
        LegacyObject::Mutex => 3,
    }
}

const fn event_object(event: LegacyEvent) -> LegacyObject {
    match event {
        LegacyEvent::Request => LegacyObject::RequestEvent,
        LegacyEvent::Response => LegacyObject::ResponseEvent,
    }
}

/// Who holds a handle.
#[derive(Clone, Copy)]
enum Holder {
    /// A process outside the test (the setup helpers).
    Outside,
    /// An endpoint of the fake.
    Endpoint(usize),
}

/// Who owns the mutex.
enum Owner {
    Free,
    Endpoint { id: usize, depth: u32 },
    Elsewhere { release_at: Option<u64> },
}

struct Scheduled {
    at: u64,
    order: u64,
    sequence: u32,
    reply: FakeReply,
}

type Peer = Box<dyn FnMut() + Send>;

struct State {
    block: Option<Box<[u8; BLOCK_SIZE]>>,
    /// Objects held open by processes outside the test.
    outside: Handles,
    /// Handles of each endpoint, indexed by endpoint id.
    endpoints: Vec<Handles>,
    request_signaled: bool,
    response_signaled: bool,
    owner: Owner,
    abandon_next: bool,
    now: u64,
    replies: VecDeque<FakeReply>,
    scheduled: Vec<Scheduled>,
    next_order: u64,
    journal: Vec<FakeOp>,
    peer: Option<Peer>,
}

impl State {
    fn exists(&self, object: LegacyObject) -> bool {
        let index = slot(object);
        self.outside.get(index).copied().unwrap_or(false)
            || self
                .endpoints
                .iter()
                .any(|handles| handles.get(index).copied().unwrap_or(false))
    }

    fn holds(&self, endpoint: usize, object: LegacyObject) -> bool {
        self.endpoints
            .get(endpoint)
            .and_then(|handles| handles.get(slot(object)))
            .copied()
            .unwrap_or(false)
    }

    fn handle_flag(&mut self, holder: Holder, object: LegacyObject) -> Option<&mut bool> {
        let handles = match holder {
            Holder::Outside => &mut self.outside,
            Holder::Endpoint(id) => self.endpoints.get_mut(id)?,
        };
        handles.get_mut(slot(object))
    }

    /// Gives `holder` a handle, creating the object when nobody holds it yet.
    fn grant(&mut self, holder: Holder, object: LegacyObject) {
        if !self.exists(object) {
            self.reset_object(object);
            if object == LegacyObject::SharedMemory {
                self.block = Some(Box::new([0; BLOCK_SIZE]));
            }
        }
        if let Some(flag) = self.handle_flag(holder, object) {
            *flag = true;
        }
    }

    /// Closes `holder`'s handle; the object disappears with its last handle.
    fn revoke(&mut self, holder: Holder, object: LegacyObject) {
        if let Some(flag) = self.handle_flag(holder, object) {
            *flag = false;
        }
        if !self.exists(object) {
            self.reset_object(object);
        }
    }

    /// The state of an object that does not exist (or was just created).
    fn reset_object(&mut self, object: LegacyObject) {
        match object {
            LegacyObject::SharedMemory => self.block = None,
            LegacyObject::RequestEvent => self.request_signaled = false,
            LegacyObject::ResponseEvent => self.response_signaled = false,
            LegacyObject::Mutex => {
                self.owner = Owner::Free;
                self.abandon_next = false;
            }
        }
    }

    fn require(&self, endpoint: usize, object: LegacyObject) -> Result<(), LegacyError> {
        if self.holds(endpoint, object) {
            Ok(())
        } else {
            Err(LegacyError::ObjectMissing(object))
        }
    }

    fn read_u32(&self, offset: usize) -> u32 {
        let mut raw = [0_u8; 4];
        if let Some(source) = self
            .block
            .as_deref()
            .and_then(|block| block.get(offset..offset.saturating_add(4)))
        {
            raw.copy_from_slice(source);
        }
        u32::from_le_bytes(raw)
    }

    fn write(&mut self, offset: usize, data: &[u8]) {
        if let Some(target) = self.block.as_deref_mut().and_then(|block| {
            offset
                .checked_add(data.len())
                .and_then(|end| block.get_mut(offset..end))
        }) {
            target.copy_from_slice(data);
        }
    }

    fn schedule(&mut self, sequence: u32, reply: FakeReply) {
        let order = self.next_order;
        self.next_order = self.next_order.wrapping_add(1);
        self.scheduled.push(Scheduled {
            at: self.now.saturating_add(reply.delay_ms),
            order,
            sequence,
            reply,
        });
    }

    fn next_due(&self, limit: u64) -> Option<usize> {
        self.scheduled
            .iter()
            .enumerate()
            .filter(|(_, item)| item.at <= limit)
            .min_by_key(|(_, item)| (item.at, item.order))
            .map(|(index, _)| index)
    }

    /// Lands the earliest reply due by `limit`; false when none is due.
    fn land_next(&mut self, limit: u64) -> bool {
        let Some(index) = self.next_due(limit) else {
            return false;
        };
        let item = self.scheduled.swap_remove(index);
        self.now = self.now.max(item.at);
        self.land(item);
        true
    }

    fn advance_to(&mut self, target: u64) {
        while self.land_next(target) {}
        self.now = self.now.max(target);
    }

    fn land(&mut self, item: Scheduled) {
        if self.block.is_none() {
            return;
        }
        let reply = item.reply;
        let bytes: Vec<u8> = reply
            .output
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();
        let stored = bytes
            .get(..bytes.len().min(OUTPUT_CAPACITY))
            .unwrap_or_default();
        self.write(OFF_OUTPUT, stored);
        let len = reply
            .output_len
            .unwrap_or_else(|| u32::try_from(bytes.len()).unwrap_or(u32::MAX));
        self.write(OFF_OUTPUT_LEN, &len.to_le_bytes());
        self.write(OFF_DELETE_COUNT, &reply.delete_count.to_le_bytes());
        let epoch = match reply.epoch {
            FakeEpoch::Now => self.now,
            FakeEpoch::AgeMs(age) => self.now.saturating_sub(age),
            FakeEpoch::Absolute(value) => value,
        };
        self.write(OFF_EPOCH, &epoch.to_le_bytes());
        let sequence = reply.sequence.unwrap_or(item.sequence);
        self.write(OFF_RESPONSE_SEQ, &sequence.to_le_bytes());
        if self.exists(LegacyObject::ResponseEvent) {
            self.response_signaled = true;
        }
    }

    fn take_signal(&mut self, event: LegacyEvent) -> bool {
        let flag = match event {
            LegacyEvent::Request => &mut self.request_signaled,
            LegacyEvent::Response => &mut self.response_signaled,
        };
        core::mem::replace(flag, false)
    }

    fn wait_for(&mut self, event: LegacyEvent, timeout_ms: u64) -> WaitResult {
        let deadline = self.now.saturating_add(timeout_ms);
        loop {
            if self.take_signal(event) {
                return WaitResult::Signaled;
            }
            if !self.land_next(deadline) {
                self.now = self.now.max(deadline);
                return WaitResult::TimedOut;
            }
        }
    }
}

/// Deterministic in-memory legacy channel: one endpoint of a simulated namespace. Clones
/// share the endpoint; [`FakeLegacy::endpoint`] attaches another one.
#[derive(Clone)]
pub struct FakeLegacy {
    state: Arc<Mutex<State>>,
    id: usize,
}

impl FakeLegacy {
    /// A namespace in which no legacy object exists yet, and its first endpoint.
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(State {
                block: None,
                outside: [false; OBJECTS],
                endpoints: vec![[false; OBJECTS]],
                request_signaled: false,
                response_signaled: false,
                owner: Owner::Free,
                abandon_next: false,
                now: FAKE_CLOCK_START_MS,
                replies: VecDeque::new(),
                scheduled: Vec::new(),
                next_order: 0,
                journal: Vec::new(),
                peer: None,
            })),
            id: 0,
        }
    }

    /// A namespace in which a controller outside the test already created the zeroed mapping
    /// and both events (and keeps them open).
    pub fn with_controller_objects() -> Self {
        let fake = Self::new();
        for object in LegacyObject::CONTROLLER_OWNED {
            fake.create_object(object);
        }
        fake
    }

    /// Another process attached to the same namespace and clock, holding no handle yet.
    pub fn endpoint(&self) -> Self {
        let mut state = self.lock();
        let id = state.endpoints.len();
        state.endpoints.push([false; OBJECTS]);
        Self {
            state: Arc::clone(&self.state),
            id,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Simulated time in milliseconds.
    pub fn now(&self) -> u64 {
        self.lock().now
    }

    /// Lets `ms` of simulated time pass, landing every reply that falls due.
    pub fn advance(&self, ms: u64) {
        let mut state = self.lock();
        let target = state.now.saturating_add(ms);
        state.advance_to(target);
    }

    /// Whether an object exists (some process holds a handle to it).
    pub fn exists(&self, object: LegacyObject) -> bool {
        self.lock().exists(object)
    }

    /// Whether this endpoint holds a handle to `object`.
    pub fn holds(&self, object: LegacyObject) -> bool {
        self.lock().holds(self.id, object)
    }

    /// A process outside the test creates (or opens) an object and keeps it open.
    pub fn create_object(&self, object: LegacyObject) {
        self.lock().grant(Holder::Outside, object);
    }

    /// The process outside the test closes its handle to `object`; the object disappears
    /// unless an endpoint still holds it.
    pub fn release_object(&self, object: LegacyObject) {
        self.lock().revoke(Holder::Outside, object);
    }

    /// Every handle of every process is lost and every object disappears.
    pub fn close_all(&self) {
        let mut state = self.lock();
        state.outside = [false; OBJECTS];
        state.endpoints.fill([false; OBJECTS]);
        for object in LegacyObject::ALL {
            state.reset_object(object);
        }
    }

    /// Queues a reply that answers the next request signal.
    pub fn push_reply(&self, reply: FakeReply) {
        self.lock().replies.push_back(reply);
    }

    /// Schedules an unsolicited reply `reply.delay_ms` from now; without a fixed sequence it
    /// echoes the request sequence stored now.
    pub fn schedule_reply(&self, reply: FakeReply) {
        let mut state = self.lock();
        let sequence = state.read_u32(OFF_REQUEST_SEQ);
        state.schedule(sequence, reply);
    }

    /// Runs `peer` whenever a response is awaited while a request is signaled, as a controller
    /// worker would (typically `LegacyServer::serve_one` on another endpoint). The peer runs
    /// without internal locks held, so it may use the channel.
    pub fn set_peer<F>(&self, peer: F)
    where
        F: FnMut() + Send + 'static,
    {
        self.lock().peer = Some(Box::new(peer));
    }

    /// Removes the peer (also breaks reference cycles through it).
    pub fn clear_peer(&self) {
        self.lock().peer = None;
    }

    /// A process outside the test holds the mutex until `release_after_ms` from now (or
    /// forever). The mutex is created when missing.
    pub fn hold_mutex_elsewhere(&self, release_after_ms: Option<u64>) {
        let mut state = self.lock();
        state.grant(Holder::Outside, LegacyObject::Mutex);
        let release_at = release_after_ms.map(|delay| state.now.saturating_add(delay));
        state.owner = Owner::Elsewhere { release_at };
    }

    /// The owner of the mutex died: the next lock reports [`LockResult::Abandoned`].
    pub fn abandon_mutex_next(&self) {
        let mut state = self.lock();
        state.owner = Owner::Free;
        state.abandon_next = true;
    }

    /// Writes a fresh response for `sequence` and signals the response event, as a controller
    /// answering an earlier request would.
    pub fn write_stale_response(&self, sequence: u32, output: &[u16]) {
        let mut state = self.lock();
        let reply = FakeReply::units(0, output.to_vec()).with_sequence(sequence);
        state.land(Scheduled {
            at: 0,
            order: 0,
            sequence,
            reply,
        });
    }

    /// Writes request fields and signals the request event, as a TIP would.
    pub fn submit_request(&self, opcode: u32, unit: u32, shift: u32, caps: u32, sequence: u32) {
        let mut state = self.lock();
        for (offset, value) in [
            (OFF_OPCODE, opcode),
            (OFF_CHAR, unit),
            (OFF_SHIFT, shift),
            (OFF_CAPS, caps),
            (OFF_REQUEST_SEQ, sequence),
        ] {
            state.write(offset, &value.to_le_bytes());
        }
        if state.exists(LegacyObject::RequestEvent) {
            state.request_signaled = true;
        }
    }

    /// Stores a raw `u32` in the block (no effect without a mapping).
    pub fn set_u32(&self, offset: usize, value: u32) {
        self.lock().write(offset, &value.to_le_bytes());
    }

    /// Stores a raw `u64` in the block (no effect without a mapping).
    pub fn set_u64(&self, offset: usize, value: u64) {
        self.lock().write(offset, &value.to_le_bytes());
    }

    /// Reads a raw `u32` (zero without a mapping).
    pub fn u32_at(&self, offset: usize) -> u32 {
        self.lock().read_u32(offset)
    }

    /// Reads a raw `u64` (zero without a mapping).
    pub fn u64_at(&self, offset: usize) -> u64 {
        self.block()
            .and_then(|block| {
                block
                    .as_bytes()
                    .get(offset..offset.saturating_add(8))
                    .and_then(|bytes| <[u8; 8]>::try_from(bytes).ok())
            })
            .map_or(0, u64::from_le_bytes)
    }

    /// A copy of the block, if the mapping exists.
    pub fn block(&self) -> Option<LegacyBlock> {
        self.lock()
            .block
            .as_deref()
            .and_then(|bytes| LegacyBlock::from_bytes(bytes))
    }

    /// Whether the request event is signaled.
    pub fn request_signaled(&self) -> bool {
        self.lock().request_signaled
    }

    /// Whether the response event is signaled.
    pub fn response_signaled(&self) -> bool {
        self.lock().response_signaled
    }

    /// The journal so far (operations of every endpoint).
    pub fn journal(&self) -> Vec<FakeOp> {
        self.lock().journal.clone()
    }

    /// Returns and clears the journal.
    pub fn take_journal(&self) -> Vec<FakeOp> {
        core::mem::take(&mut self.lock().journal)
    }

    fn run_peer(&self) {
        let peer = {
            let mut state = self.lock();
            if state.request_signaled {
                state.peer.take()
            } else {
                None
            }
        };
        if let Some(mut peer) = peer {
            peer();
            let mut state = self.lock();
            if state.peer.is_none() {
                state.peer = Some(peer);
            }
        }
    }
}

impl Default for FakeLegacy {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for FakeLegacy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeLegacy")
            .field("endpoint", &self.id)
            .field("now", &self.now())
            .finish_non_exhaustive()
    }
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

impl LegacyChannel for FakeLegacy {
    fn object_exists(&self, object: LegacyObject) -> Result<bool, LegacyError> {
        Ok(self.exists(object))
    }

    fn create_controller_objects(&self) -> Result<(), LegacyError> {
        let mut state = self.lock();
        state.journal.push(FakeOp::CreateControllerObjects);
        if let Some(object) = LegacyObject::CONTROLLER_OWNED
            .into_iter()
            .find(|object| state.exists(*object))
        {
            return Err(LegacyError::ConflictingObject(object));
        }
        for object in LegacyObject::CONTROLLER_OWNED {
            state.grant(Holder::Endpoint(self.id), object);
        }
        Ok(())
    }

    fn open_controller_objects(&self) -> Result<(), LegacyError> {
        let mut state = self.lock();
        state.journal.push(FakeOp::OpenControllerObjects);
        let missing = LegacyObject::CONTROLLER_OWNED
            .into_iter()
            .find(|object| !state.exists(*object));
        for object in LegacyObject::CONTROLLER_OWNED {
            // New handles replace earlier ones; a failed open keeps none of them.
            if missing.is_none() {
                state.grant(Holder::Endpoint(self.id), object);
            } else {
                state.revoke(Holder::Endpoint(self.id), object);
            }
        }
        missing.map_or(Ok(()), |object| Err(LegacyError::ObjectMissing(object)))
    }

    fn open_or_create_mutex(&self) -> Result<MutexOrigin, LegacyError> {
        let mut state = self.lock();
        let origin = if state.exists(LegacyObject::Mutex) {
            MutexOrigin::Opened
        } else {
            MutexOrigin::Created
        };
        state.grant(Holder::Endpoint(self.id), LegacyObject::Mutex);
        state.journal.push(FakeOp::OpenOrCreateMutex(origin));
        Ok(origin)
    }

    fn close(&self) {
        let mut state = self.lock();
        state.journal.push(FakeOp::Close);
        if matches!(state.owner, Owner::Endpoint { id, .. } if id == self.id) {
            // This process can no longer release the mutex: the next owner sees it abandoned.
            state.owner = Owner::Free;
            state.abandon_next = true;
        }
        for object in LegacyObject::ALL {
            state.revoke(Holder::Endpoint(self.id), object);
        }
    }

    fn read_shared(&self, offset: usize, out: &mut [u8]) -> Result<(), LegacyError> {
        let mut state = self.lock();
        state.journal.push(FakeOp::Read {
            offset,
            len: out.len(),
        });
        state.require(self.id, LegacyObject::SharedMemory)?;
        let block = state
            .block
            .as_deref()
            .ok_or(LegacyError::ObjectMissing(LegacyObject::SharedMemory))?;
        let source = offset
            .checked_add(out.len())
            .and_then(|end| block.get(offset..end))
            .ok_or(LegacyError::OutOfBounds)?;
        out.copy_from_slice(source);
        Ok(())
    }

    fn write_shared(&self, offset: usize, data: &[u8]) -> Result<(), LegacyError> {
        let mut state = self.lock();
        state.journal.push(FakeOp::Write {
            offset,
            bytes: data.to_vec(),
        });
        state.require(self.id, LegacyObject::SharedMemory)?;
        let block = state
            .block
            .as_deref_mut()
            .ok_or(LegacyError::ObjectMissing(LegacyObject::SharedMemory))?;
        let target = offset
            .checked_add(data.len())
            .and_then(|end| block.get_mut(offset..end))
            .ok_or(LegacyError::OutOfBounds)?;
        target.copy_from_slice(data);
        Ok(())
    }

    fn signal(&self, event: LegacyEvent) -> Result<(), LegacyError> {
        let mut state = self.lock();
        state.journal.push(FakeOp::Signal(event));
        state.require(self.id, event_object(event))?;
        match event {
            LegacyEvent::Request => {
                state.request_signaled = true;
                if let Some(reply) = state.replies.pop_front() {
                    // The scripted controller consumes the request now and answers later.
                    state.request_signaled = false;
                    let sequence = state.read_u32(OFF_REQUEST_SEQ);
                    state.schedule(sequence, reply);
                }
            }
            LegacyEvent::Response => state.response_signaled = true,
        }
        Ok(())
    }

    fn wait(&self, event: LegacyEvent, timeout: Duration) -> Result<WaitResult, LegacyError> {
        if event == LegacyEvent::Response {
            self.run_peer();
        }
        let timeout_ms = millis(timeout);
        let mut state = self.lock();
        state.require(self.id, event_object(event))?;
        let result = state.wait_for(event, timeout_ms);
        state.journal.push(FakeOp::Wait {
            event,
            timeout_ms,
            result,
        });
        Ok(result)
    }

    fn lock_mutex(&self, timeout: Duration) -> Result<LockResult, LegacyError> {
        let timeout_ms = millis(timeout);
        let mut state = self.lock();
        state.require(self.id, LegacyObject::Mutex)?;
        let limit = state.now.saturating_add(timeout_ms);
        let result = match state.owner {
            Owner::Free => {
                state.owner = Owner::Endpoint {
                    id: self.id,
                    depth: 1,
                };
                if core::mem::replace(&mut state.abandon_next, false) {
                    LockResult::Abandoned
                } else {
                    LockResult::Acquired
                }
            }
            Owner::Endpoint { id, depth } if id == self.id => {
                state.owner = Owner::Endpoint {
                    id,
                    depth: depth.saturating_add(1),
                };
                LockResult::Acquired
            }
            Owner::Elsewhere {
                release_at: Some(at),
            } if at <= limit => {
                state.advance_to(at);
                state.owner = Owner::Endpoint {
                    id: self.id,
                    depth: 1,
                };
                LockResult::Acquired
            }
            // Another endpoint cannot release it while this one waits: the simulation is
            // single-threaded.
            Owner::Endpoint { .. } | Owner::Elsewhere { .. } => {
                state.advance_to(limit);
                LockResult::TimedOut
            }
        };
        state.journal.push(FakeOp::Lock { timeout_ms, result });
        Ok(result)
    }

    fn unlock_mutex(&self) -> Result<(), LegacyError> {
        let mut state = self.lock();
        state.journal.push(FakeOp::Unlock);
        state.require(self.id, LegacyObject::Mutex)?;
        match state.owner {
            Owner::Endpoint { id, depth } if id == self.id => {
                state.owner = if depth > 1 {
                    Owner::Endpoint {
                        id,
                        depth: depth.saturating_sub(1),
                    }
                } else {
                    Owner::Free
                };
                Ok(())
            }
            Owner::Free | Owner::Endpoint { .. } | Owner::Elsewhere { .. } => {
                Err(LegacyError::Channel {
                    code: ERROR_NOT_OWNER,
                })
            }
        }
    }

    fn now_ms(&self) -> u64 {
        self.now()
    }
}
