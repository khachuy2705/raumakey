//! Framed transport shared by the client and server sessions.

use std::io::{self, Read, Write};

use crate::error::IpcError;
use crate::frame::{EncodedPayload, Frame, FrameDecoder, Sender};
use crate::message::Message;

/// Bytes requested from the transport per read.
const READ_CHUNK: usize = 16 * 1024;

/// A transport plus the decoder for the frames its peer sends.
pub(crate) struct Link<S> {
    stream: S,
    decoder: FrameDecoder,
    buffer: Vec<u8>,
}

impl<S: Read + Write> Link<S> {
    pub(crate) fn new(stream: S, peer: Sender) -> Self {
        Self {
            stream,
            decoder: FrameDecoder::for_sender(peer),
            buffer: vec![0; READ_CHUNK],
        }
    }

    /// Validates and encodes `message`, then writes it. Encoding errors leave the stream
    /// untouched; transport errors are reported as [`LinkError::Transport`].
    pub(crate) fn send(
        &mut self,
        client_id: u64,
        request_id: u64,
        message: &Message,
    ) -> Result<(), LinkError> {
        let payload = EncodedPayload::encode(message).map_err(LinkError::Encode)?;
        self.send_payload(client_id, request_id, &payload)
    }

    /// Writes an already validated payload.
    pub(crate) fn send_payload(
        &mut self,
        client_id: u64,
        request_id: u64,
        payload: &EncodedPayload,
    ) -> Result<(), LinkError> {
        let bytes = payload.frame(client_id, request_id);
        self.stream
            .write_all(&bytes)
            .and_then(|()| self.stream.flush())
            .map_err(|error| LinkError::Transport(IpcError::from(error)))
    }

    /// The next frame; `Ok(None)` when the transport timed out or would block. A partial frame
    /// stays buffered across such timeouts.
    pub(crate) fn recv(&mut self) -> Result<Option<Frame>, IpcError> {
        loop {
            if let Some(next) = self.decoder.next_frame() {
                return next.map(Some);
            }
            match self.stream.read(&mut self.buffer) {
                Ok(0) => {
                    return Err(if self.decoder.is_idle() {
                        IpcError::Closed
                    } else {
                        IpcError::Truncated
                    });
                }
                Ok(read) => self
                    .decoder
                    .feed(self.buffer.get(..read).unwrap_or_default()),
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(IpcError::from(error)),
            }
        }
    }

    /// The next frame; a transport timeout is an error (used during handshakes).
    pub(crate) fn recv_required(&mut self) -> Result<Frame, IpcError> {
        self.recv()?.ok_or(IpcError::Io(io::ErrorKind::TimedOut))
    }
}

/// Failure of [`Link::send`].
pub(crate) enum LinkError {
    /// The message failed validation or encoding; nothing was written.
    Encode(IpcError),
    /// The transport failed; the connection is unusable.
    Transport(IpcError),
}

impl LinkError {
    pub(crate) const fn into_error(self) -> IpcError {
        match self {
            Self::Encode(error) | Self::Transport(error) => error,
        }
    }
}
