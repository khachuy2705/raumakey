use std::time::Duration;

use raumakey_ipc::backoff::{Backoff, INITIAL_BACKOFF, MAX_BACKOFF};

fn ms(value: u64) -> Duration {
    Duration::from_millis(value)
}

#[test]
fn backoff_doubles_from_100_ms_and_caps_at_10_s() {
    let mut backoff = Backoff::new();
    let delays: Vec<Duration> = (0..10).map(|_| backoff.next_delay()).collect();
    assert_eq!(
        delays,
        [
            ms(100),
            ms(200),
            ms(400),
            ms(800),
            ms(1_600),
            ms(3_200),
            ms(6_400),
            ms(10_000),
            ms(10_000),
            ms(10_000),
        ]
    );
    assert_eq!(INITIAL_BACKOFF, ms(100));
    assert_eq!(MAX_BACKOFF, ms(10_000));
}

#[test]
fn backoff_reset_returns_to_the_initial_delay() {
    let mut backoff = Backoff::new();
    for _ in 0..5 {
        backoff.next_delay();
    }
    assert_eq!(backoff.failures(), 5);
    backoff.reset();
    assert_eq!(backoff.failures(), 0);
    assert_eq!(backoff.peek(), ms(100));
    assert_eq!(backoff.next_delay(), ms(100));
}

#[test]
fn backoff_is_deterministic_and_saturates() {
    let mut a = Backoff::default();
    let mut b = Backoff::new();
    for _ in 0..200 {
        assert_eq!(a.next_delay(), b.next_delay());
    }
    assert_eq!(a.peek(), MAX_BACKOFF);
    assert_eq!(a.failures(), 200);
}
