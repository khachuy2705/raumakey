//! Deterministic reconnect backoff (plan §7.3, design §8).

use std::time::Duration;

/// First reconnect delay.
pub const INITIAL_BACKOFF: Duration = Duration::from_millis(100);

/// Largest reconnect delay.
pub const MAX_BACKOFF: Duration = Duration::from_secs(10);

/// Bounded exponential backoff: 100 ms, doubling on every failure, capped at 10 s, no jitter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Backoff {
    next: Duration,
    failures: u32,
}

impl Backoff {
    /// A backoff whose next delay is [`INITIAL_BACKOFF`].
    pub const fn new() -> Self {
        Self {
            next: INITIAL_BACKOFF,
            failures: 0,
        }
    }

    /// Records a failed attempt and returns how long to wait before the next one.
    pub fn next_delay(&mut self) -> Duration {
        let delay = self.next;
        self.next = delay.saturating_mul(2).min(MAX_BACKOFF);
        self.failures = self.failures.saturating_add(1);
        delay
    }

    /// The delay that [`Backoff::next_delay`] will return, without recording a failure.
    pub const fn peek(&self) -> Duration {
        self.next
    }

    /// Failed attempts since the last success.
    pub const fn failures(&self) -> u32 {
        self.failures
    }

    /// Records a success: the next delay is [`INITIAL_BACKOFF`] again.
    pub fn reset(&mut self) {
        *self = Self::new();
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new()
    }
}
