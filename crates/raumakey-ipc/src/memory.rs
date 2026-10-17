//! In-memory duplex byte pipe for tests and in-process wiring.
//!
//! Each end implements [`Read`] and [`Write`]. Reads block until bytes arrive, the peer closes,
//! or the optional read timeout passes ([`io::ErrorKind::TimedOut`]; a zero timeout reports
//! [`io::ErrorKind::WouldBlock`] immediately). Dropping or closing an end closes both
//! directions: the peer reads the bytes already buffered and then end-of-stream, and further
//! writes fail with [`io::ErrorKind::BrokenPipe`].

use std::collections::VecDeque;
use std::fmt;
use std::io::{self, Read, Write};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

#[derive(Default)]
struct ChannelState {
    bytes: VecDeque<u8>,
    writer_closed: bool,
    reader_closed: bool,
}

#[derive(Default)]
struct Channel {
    state: Mutex<ChannelState>,
    changed: Condvar,
}

impl Channel {
    fn lock(&self) -> MutexGuard<'_, ChannelState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One end of a [`duplex`] pipe.
pub struct MemoryPipe {
    incoming: Arc<Channel>,
    outgoing: Arc<Channel>,
    read_timeout: Option<Duration>,
}

/// Creates a connected pair of pipe ends.
pub fn duplex() -> (MemoryPipe, MemoryPipe) {
    let forward = Arc::new(Channel::default());
    let backward = Arc::new(Channel::default());
    let first = MemoryPipe {
        incoming: Arc::clone(&backward),
        outgoing: Arc::clone(&forward),
        read_timeout: None,
    };
    let second = MemoryPipe {
        incoming: forward,
        outgoing: backward,
        read_timeout: None,
    };
    (first, second)
}

impl MemoryPipe {
    /// Sets the read timeout: `None` blocks, `Some(Duration::ZERO)` never blocks.
    pub fn set_read_timeout(&mut self, timeout: Option<Duration>) {
        self.read_timeout = timeout;
    }

    /// The read timeout.
    pub const fn read_timeout(&self) -> Option<Duration> {
        self.read_timeout
    }

    /// Closes both directions, as dropping the end does.
    pub fn close(&self) {
        self.outgoing.lock().writer_closed = true;
        self.outgoing.changed.notify_all();
        self.incoming.lock().reader_closed = true;
        self.incoming.changed.notify_all();
    }
}

impl Read for MemoryPipe {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let deadline = self
            .read_timeout
            .and_then(|timeout| Instant::now().checked_add(timeout));
        let mut state = self.incoming.lock();
        loop {
            if state.reader_closed {
                return Ok(0);
            }
            if !state.bytes.is_empty() {
                return state.bytes.read(buf);
            }
            if state.writer_closed {
                return Ok(0);
            }
            if self.read_timeout == Some(Duration::ZERO) {
                return Err(io::ErrorKind::WouldBlock.into());
            }
            state = match deadline {
                None => self
                    .incoming
                    .changed
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner),
                Some(deadline) => {
                    let left = deadline.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        return Err(io::ErrorKind::TimedOut.into());
                    }
                    self.incoming
                        .changed
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0
                }
            };
        }
    }
}

impl Write for MemoryPipe {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self.outgoing.lock();
        if state.writer_closed || state.reader_closed {
            return Err(io::ErrorKind::BrokenPipe.into());
        }
        state.bytes.extend(buf);
        drop(state);
        self.outgoing.changed.notify_all();
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for MemoryPipe {
    fn drop(&mut self) {
        self.close();
    }
}

impl fmt::Debug for MemoryPipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryPipe")
            .field("read_timeout", &self.read_timeout)
            .finish_non_exhaustive()
    }
}
