//! The document identity of each TSF context.

use raumakey_model::host::DocumentId;

use crate::session::MAX_DOCUMENTS;

/// Most contexts the adapter tracks; the core keeps one engine state per document
/// ([`MAX_DOCUMENTS`]).
pub const MAX_CONTEXTS: usize = MAX_DOCUMENTS;

/// What the adapter knows about one context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextInfo {
    /// The document id of the context, given once and never to another context.
    pub id: DocumentId,
    /// The identity of its document manager, when known.
    pub manager: Option<usize>,
    /// Whether its input scope is a password scope; `None` until it was read.
    pub secure: Option<bool>,
    /// The class of the window of its view, when known.
    pub window_class: Option<String>,
}

impl ContextInfo {
    /// Whether keys in the context bypass every transformation (matrix #29): its input scope
    /// is a password scope, or it was not read yet.
    pub fn is_secure_field(&self) -> bool {
        self.secure != Some(false)
    }
}

/// The result of [`Contexts::enter`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entered {
    /// The document id of the context.
    pub id: DocumentId,
    /// The context was not tracked before.
    pub new: bool,
    /// The document that made room for it, which the core must drop
    /// (`ServiceCore::document_closed`).
    pub evicted: Option<DocumentId>,
}

/// The contexts of one activation, keyed by their COM identity (the address of their
/// `IUnknown`), at most [`MAX_CONTEXTS`] of them.
///
/// A context keeps its document id while it is tracked. A context that is removed (popped,
/// or its document manager uninitialized) and comes back, or a new context at the address of
/// one that is gone, gets a new id, so no word survives into it.
#[derive(Clone, Debug, Default)]
pub struct Contexts {
    entries: Vec<Slot>,
    /// The last id given.
    last_id: u128,
    /// Use counter for the least-recently-used bound.
    clock: u64,
}

#[derive(Clone, Debug)]
struct Slot {
    context: usize,
    info: ContextInfo,
    last_used: u64,
}

impl Contexts {
    /// Tracks `context` of `manager` and marks it most recently used. A new context beyond
    /// the bound takes the place of the least recently used one.
    pub fn enter(&mut self, context: usize, manager: Option<usize>) -> Entered {
        self.clock = self.clock.wrapping_add(1);
        let now = self.clock;
        if let Some(slot) = self.slot_mut(context) {
            slot.last_used = now;
            if slot.info.manager.is_none() {
                slot.info.manager = manager;
            }
            return Entered {
                id: slot.info.id,
                new: false,
                evicted: None,
            };
        }
        let mut evicted = None;
        if self.entries.len() >= MAX_CONTEXTS
            && let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, slot)| slot.last_used)
                .map(|(index, _)| index)
        {
            evicted = Some(self.entries.swap_remove(oldest).info.id);
        }
        self.last_id = self.last_id.wrapping_add(1);
        let id = DocumentId::from_u128(self.last_id);
        self.entries.push(Slot {
            context,
            info: ContextInfo {
                id,
                manager,
                secure: None,
                window_class: None,
            },
            last_used: now,
        });
        Entered {
            id,
            new: true,
            evicted,
        }
    }

    /// What is known about `context`.
    pub fn get(&self, context: usize) -> Option<&ContextInfo> {
        self.entries
            .iter()
            .find(|slot| slot.context == context)
            .map(|slot| &slot.info)
    }

    /// What is known about `context`, to update it.
    pub fn get_mut(&mut self, context: usize) -> Option<&mut ContextInfo> {
        self.slot_mut(context).map(|slot| &mut slot.info)
    }

    /// Stops tracking `context` and returns its document id.
    pub fn remove(&mut self, context: usize) -> Option<DocumentId> {
        let index = self
            .entries
            .iter()
            .position(|slot| slot.context == context)?;
        Some(self.entries.swap_remove(index).info.id)
    }

    /// Stops tracking every context of `manager` and returns their document ids.
    pub fn remove_manager(&mut self, manager: usize) -> Vec<DocumentId> {
        let mut removed = Vec::new();
        self.entries.retain(|slot| {
            let belongs = slot.info.manager == Some(manager);
            if belongs {
                removed.push(slot.info.id);
            }
            !belongs
        });
        removed
    }

    /// Number of contexts tracked.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no context is tracked.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn slot_mut(&mut self, context: usize) -> Option<&mut Slot> {
        self.entries.iter_mut().find(|slot| slot.context == context)
    }
}
