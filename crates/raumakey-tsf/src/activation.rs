//! The sinks a text service activation advises, and their lifecycle (plan §10.3, design §12).
//!
//! When TSF activates the text service (`ITfTextInputProcessorEx::ActivateEx`), the service
//! advises the four sinks of [`Sink::ALL`] on the thread manager, in that order, and records
//! what each advise returned. `Deactivate` unadvises them again. The rules live here, apart
//! from COM, so they are tested with fakes on any host (design §15.1); the COM layer
//! implements [`SinkPort`] over the thread manager.

#![forbid(unsafe_code)]

use core::fmt;

/// A sink the text service advises when it is activated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sink {
    /// `ITfThreadMgrEventSink`, through the thread manager's `ITfSource`.
    ThreadMgrEvents,
    /// `ITfThreadFocusSink`, through the thread manager's `ITfSource`.
    ThreadFocus,
    /// `ITfKeyEventSink`, through `ITfKeystrokeMgr::AdviseKeyEventSink(tid, sink, TRUE)`.
    KeyEvents,
    /// `ITfCompartmentEventSink` on `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` of the thread
    /// manager's compartment manager, through the compartment's `ITfSource`.
    KeyboardOpenClose,
}

impl Sink {
    /// Every sink, in the order activation advises them.
    pub const ALL: [Self; 4] = [
        Self::ThreadMgrEvents,
        Self::ThreadFocus,
        Self::KeyEvents,
        Self::KeyboardOpenClose,
    ];
}

/// Advises and unadvises sinks: the thread manager in the COM layer, fakes in tests.
pub trait SinkPort {
    /// What unadvising one sink needs, such as its cookie and the source it was advised on.
    type Connection;
    /// The error of a failed advise or unadvise.
    type Error;

    /// Advises `sink`.
    fn advise(&mut self, sink: Sink) -> Result<Self::Connection, Self::Error>;

    /// Unadvises `sink`, which an earlier [`advise`](Self::advise) connected as
    /// `connection`.
    fn unadvise(&mut self, sink: Sink, connection: Self::Connection) -> Result<(), Self::Error>;
}

/// The sinks one activation advised, in the order it advised them.
pub struct Connections<C> {
    advised: Vec<(Sink, C)>,
}

impl<C> Connections<C> {
    /// Advises every sink of [`Sink::ALL`], in order, and records each connection.
    ///
    /// When an advise fails, the sinks advised before it are unadvised again, last first, so
    /// that nothing stays advised; their own errors are ignored.
    ///
    /// # Errors
    ///
    /// The error of the advise that failed.
    pub fn advise_all<P>(port: &mut P) -> Result<Self, P::Error>
    where
        P: SinkPort<Connection = C>,
    {
        let mut connections = Self::default();
        for sink in Sink::ALL {
            match port.advise(sink) {
                Ok(connection) => connections.advised.push((sink, connection)),
                Err(error) => {
                    // The advise error is the one worth reporting.
                    let _ = connections.unadvise_all(port);
                    return Err(error);
                }
            }
        }
        Ok(connections)
    }

    /// Unadvises every recorded sink, last first. Every sink is unadvised even when an
    /// earlier unadvise fails, and none is recorded afterwards, so a second call does
    /// nothing.
    ///
    /// # Errors
    ///
    /// The error of the first unadvise that failed.
    pub fn unadvise_all<P>(&mut self, port: &mut P) -> Result<(), P::Error>
    where
        P: SinkPort<Connection = C>,
    {
        let mut first_error = None;
        while let Some((sink, connection)) = self.advised.pop() {
            if let Err(error) = port.unadvise(sink, connection) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// The connection of `sink`, while it is advised: the COM layer reads and writes the
    /// keyboard open/close compartment through the connection of
    /// [`Sink::KeyboardOpenClose`].
    pub fn connection(&self, sink: Sink) -> Option<&C> {
        self.advised
            .iter()
            .find(|(advised, _)| *advised == sink)
            .map(|(_, connection)| connection)
    }

    /// The recorded sinks, in the order they were advised.
    pub fn sinks(&self) -> Vec<Sink> {
        self.advised.iter().map(|(sink, _)| *sink).collect()
    }

    /// True when no sink is recorded.
    pub fn is_empty(&self) -> bool {
        self.advised.is_empty()
    }
}

impl<C> Default for Connections<C> {
    fn default() -> Self {
        Self {
            advised: Vec::new(),
        }
    }
}

/// Shows the recorded sinks only, not their connections.
impl<C> fmt::Debug for Connections<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Connections")
            .field("sinks", &self.sinks())
            .finish_non_exhaustive()
    }
}
