//! Bounded snapshot ring used by semantic backspace (plan §3.9).

use std::collections::VecDeque;

use crate::HISTORY_CAPACITY;

/// The most recent [`HISTORY_CAPACITY`] snapshots; the oldest is evicted first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct History<T> {
    entries: VecDeque<T>,
}

impl<T> Default for History<T> {
    fn default() -> Self {
        Self {
            entries: VecDeque::new(),
        }
    }
}

impl<T> History<T> {
    /// Pushes a snapshot, evicting the oldest one when full.
    pub(crate) fn push(&mut self, snapshot: T) {
        while self.entries.len() >= HISTORY_CAPACITY {
            self.entries.pop_front();
        }
        self.entries.push_back(snapshot);
    }

    /// Removes and returns the newest snapshot.
    pub(crate) fn pop(&mut self) -> Option<T> {
        self.entries.pop_back()
    }

    /// Number of stored snapshots.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no snapshot is stored.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pops_newest_first() {
        let mut history = History::default();
        history.push(1);
        history.push(2);
        assert_eq!(history.len(), 2);
        assert_eq!(history.pop(), Some(2));
        assert_eq!(history.pop(), Some(1));
        assert_eq!(history.pop(), None);
        assert!(history.is_empty());
    }

    #[test]
    fn evicts_oldest_beyond_capacity() {
        let mut history = History::default();
        for value in 0..=HISTORY_CAPACITY {
            history.push(value);
        }
        assert_eq!(history.len(), HISTORY_CAPACITY);
        let mut popped = Vec::new();
        while let Some(value) = history.pop() {
            popped.push(value);
        }
        let expected: Vec<usize> = (1..=HISTORY_CAPACITY).rev().collect();
        assert_eq!(popped, expected, "snapshot 0 was evicted");
    }
}
