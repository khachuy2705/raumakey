//! Per-document engine states with a fixed bound.

use raumakey_engine::{EngineState, VietnameseEngine};
use raumakey_model::error::ResetReason;
use raumakey_model::host::DocumentId;

/// Most documents whose engine state the core holds. Beyond it, the least recently used
/// document's state is dropped. Only the active document can have a word in progress (every
/// document change resets the document that lost the focus), so a dropped state is always
/// idle and nothing is lost.
pub const MAX_DOCUMENTS: usize = 64;

struct Slot {
    document: DocumentId,
    state: EngineState,
    last_used: u64,
}

/// Engine states keyed by document, at most [`MAX_DOCUMENTS`] of them.
pub(crate) struct DocumentStates {
    slots: Vec<Slot>,
    clock: u64,
}

impl DocumentStates {
    pub(crate) fn new() -> Self {
        Self {
            slots: Vec::new(),
            clock: 0,
        }
    }

    /// Number of documents held.
    pub(crate) fn len(&self) -> usize {
        self.slots.len()
    }

    /// The state of `document`, if held.
    pub(crate) fn get(&self, document: DocumentId) -> Option<&EngineState> {
        self.slots
            .iter()
            .find(|slot| slot.document == document)
            .map(|slot| &slot.state)
    }

    /// Makes sure `document` has a state (idle when new) and marks it most recently used. A
    /// new document beyond the bound takes the place of the least recently used one.
    pub(crate) fn touch(&mut self, document: DocumentId) {
        self.clock = self.clock.wrapping_add(1);
        let now = self.clock;
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.document == document) {
            slot.last_used = now;
            return;
        }
        if self.slots.len() >= MAX_DOCUMENTS
            && let Some(oldest) = self
                .slots
                .iter()
                .enumerate()
                .min_by_key(|(_, slot)| slot.last_used)
                .map(|(index, _)| index)
        {
            self.slots.swap_remove(oldest);
        }
        self.slots.push(Slot {
            document,
            state: EngineState::new(),
            last_used: now,
        });
    }

    /// Replaces the state of `document` when it is held.
    pub(crate) fn commit(&mut self, document: DocumentId, state: EngineState) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.document == document) {
            slot.state = state;
        }
    }

    /// Resets the state of `document` when it is held.
    pub(crate) fn reset(
        &mut self,
        engine: &VietnameseEngine,
        document: DocumentId,
        reason: ResetReason,
    ) {
        if let Some(slot) = self.slots.iter_mut().find(|slot| slot.document == document) {
            slot.state = engine.reset(&slot.state, reason);
        }
    }

    /// Resets the state of every document.
    pub(crate) fn reset_all(&mut self, engine: &VietnameseEngine, reason: ResetReason) {
        for slot in &mut self.slots {
            slot.state = engine.reset(&slot.state, reason);
        }
    }

    /// Forgets `document`.
    pub(crate) fn remove(&mut self, document: DocumentId) {
        self.slots.retain(|slot| slot.document != document);
    }
}
