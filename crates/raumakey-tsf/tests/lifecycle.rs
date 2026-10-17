//! The activation lifecycle of the text service (`raumakey_tip::activation`), driven through a
//! fake [`SinkPort`] on any host (design §15.1).
//!
//! The fake records every advise and unadvise and fails the calls it is told to fail. The
//! COM tests (`tests/com`) run the same lifecycle against the real TSF thread manager.

use raumakey_tip::activation::{Connections, Sink, SinkPort};

/// One call the fake port received.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Advise(Sink),
    /// The sink and the cookie its advise returned.
    Unadvise(Sink, u32),
}

/// A failed call: the sink and the operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    Advise(Sink),
    Unadvise(Sink),
}

/// Hands out cookies 1, 2, 3, … and fails the calls it is told to.
#[derive(Default)]
struct FakePort {
    calls: Vec<Call>,
    fail_advise: Option<Sink>,
    fail_unadvise: Vec<Sink>,
    cookies: u32,
}

impl SinkPort for FakePort {
    type Connection = u32;
    type Error = Failure;

    fn advise(&mut self, sink: Sink) -> Result<u32, Failure> {
        self.calls.push(Call::Advise(sink));
        if self.fail_advise == Some(sink) {
            return Err(Failure::Advise(sink));
        }
        self.cookies += 1;
        Ok(self.cookies)
    }

    fn unadvise(&mut self, sink: Sink, cookie: u32) -> Result<(), Failure> {
        self.calls.push(Call::Unadvise(sink, cookie));
        if self.fail_unadvise.contains(&sink) {
            Err(Failure::Unadvise(sink))
        } else {
            Ok(())
        }
    }
}

fn advises(sinks: &[Sink]) -> Vec<Call> {
    sinks.iter().map(|sink| Call::Advise(*sink)).collect()
}

/// The unadvise calls for the first `count` sinks of [`Sink::ALL`], last first, with the
/// cookies the fake handed out (1 for the first sink).
fn unadvises_of_first(count: usize) -> Vec<Call> {
    let mut calls: Vec<Call> = Sink::ALL[..count]
        .iter()
        .zip(1..)
        .map(|(sink, cookie)| Call::Unadvise(*sink, cookie))
        .collect();
    calls.reverse();
    calls
}

#[test]
fn the_thread_manager_sinks_come_first_then_keys_then_the_open_close_compartment() {
    assert_eq!(
        Sink::ALL,
        [
            Sink::ThreadMgrEvents,
            Sink::ThreadFocus,
            Sink::KeyEvents,
            Sink::KeyboardOpenClose,
        ]
    );
}

#[test]
fn activation_advises_every_sink_once_in_order() {
    let mut port = FakePort::default();
    let connections = Connections::advise_all(&mut port).expect("every advise succeeds");
    assert_eq!(port.calls, advises(&Sink::ALL));
    assert_eq!(connections.sinks(), Sink::ALL);
    assert!(!connections.is_empty());
}

#[test]
fn a_failed_advise_undoes_what_succeeded_last_first_and_returns_its_error() {
    for (index, failing) in Sink::ALL.into_iter().enumerate() {
        let mut port = FakePort {
            fail_advise: Some(failing),
            ..FakePort::default()
        };
        let result = Connections::advise_all(&mut port);
        assert_eq!(
            result.err(),
            Some(Failure::Advise(failing)),
            "failing {failing:?}"
        );
        let mut expected = advises(&Sink::ALL[..=index]);
        expected.extend(unadvises_of_first(index));
        assert_eq!(port.calls, expected, "failing {failing:?}");
    }
}

#[test]
fn a_failed_undo_neither_stops_the_undo_nor_replaces_the_advise_error() {
    let mut port = FakePort {
        fail_advise: Some(Sink::KeyboardOpenClose),
        fail_unadvise: vec![Sink::KeyEvents, Sink::ThreadFocus],
        ..FakePort::default()
    };
    let result = Connections::advise_all(&mut port);
    assert_eq!(result.err(), Some(Failure::Advise(Sink::KeyboardOpenClose)));
    let mut expected = advises(&Sink::ALL);
    expected.extend(unadvises_of_first(3));
    assert_eq!(port.calls, expected);
}

#[test]
fn deactivation_unadvises_every_sink_last_first() {
    let mut port = FakePort::default();
    let mut connections = Connections::advise_all(&mut port).expect("every advise succeeds");
    port.calls.clear();
    assert_eq!(connections.unadvise_all(&mut port), Ok(()));
    assert_eq!(port.calls, unadvises_of_first(Sink::ALL.len()));
    assert!(connections.is_empty());
    assert!(connections.sinks().is_empty());
}

#[test]
fn deactivation_unadvises_every_sink_even_after_failures_and_returns_the_first_error() {
    let mut port = FakePort::default();
    let mut connections = Connections::advise_all(&mut port).expect("every advise succeeds");
    port.calls.clear();
    port.fail_unadvise = vec![Sink::KeyEvents, Sink::ThreadMgrEvents];
    assert_eq!(
        connections.unadvise_all(&mut port),
        Err(Failure::Unadvise(Sink::KeyEvents))
    );
    assert_eq!(port.calls, unadvises_of_first(Sink::ALL.len()));
    assert!(
        connections.is_empty(),
        "a failed unadvise is not attempted again"
    );
}

#[test]
fn a_second_deactivation_does_nothing() {
    let mut port = FakePort::default();
    let mut connections = Connections::advise_all(&mut port).expect("every advise succeeds");
    assert_eq!(connections.unadvise_all(&mut port), Ok(()));
    port.calls.clear();
    port.fail_unadvise = Sink::ALL.to_vec();
    assert_eq!(connections.unadvise_all(&mut port), Ok(()));
    assert!(port.calls.is_empty());

    let mut never_activated: Connections<u32> = Connections::default();
    assert!(never_activated.is_empty());
    assert_eq!(never_activated.unadvise_all(&mut port), Ok(()));
    assert!(port.calls.is_empty());
}

#[test]
fn the_connection_of_each_advised_sink_can_be_looked_up() {
    let mut port = FakePort::default();
    let mut connections = Connections::advise_all(&mut port).expect("every advise succeeds");
    for (sink, cookie) in Sink::ALL.into_iter().zip(1..) {
        assert_eq!(connections.connection(sink), Some(&cookie), "{sink:?}");
    }
    connections
        .unadvise_all(&mut port)
        .expect("every unadvise succeeds");
    assert_eq!(connections.connection(Sink::KeyboardOpenClose), None);
}

#[test]
fn debug_output_names_the_advised_sinks() {
    let mut port = FakePort::default();
    let connections = Connections::advise_all(&mut port).expect("every advise succeeds");
    let text = format!("{connections:?}");
    for sink in Sink::ALL {
        assert!(text.contains(&format!("{sink:?}")), "{text}");
    }
}
