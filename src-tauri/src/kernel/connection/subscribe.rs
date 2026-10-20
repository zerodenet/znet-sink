//! Initial subscribe writes only. No query or command may use this retry boundary.
use super::{subscribe_timeout, HandshakeFailure};
use crate::{errors::AppError, models::core::CoreEndpoint};
use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

fn closed_write(context: &str, endpoint: &CoreEndpoint, error: io::Error) -> HandshakeFailure {
    // Classify the actual OS failure, never a core-provided error code. Even
    // a partial subscribe is safe to discard: no business frame was sent and
    // the replacement transport must complete a fresh acknowledged handshake.
    let closed = matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::UnexpectedEof
    );
    let error = AppError::from_io(context, endpoint, error);
    if closed {
        HandshakeFailure::ClosedBeforeAck(error)
    } else {
        HandshakeFailure::Terminal(error)
    }
}

pub(super) fn write_frame<W: Write>(
    writer: &mut W,
    bytes: &[u8],
    endpoint: &CoreEndpoint,
    deadline: Instant,
    connect_timeout: Duration,
    mut set_timeout: impl FnMut(&mut W, Duration) -> io::Result<()>,
) -> Result<(), HandshakeFailure> {
    let mut remaining_bytes = bytes;
    while !remaining_bytes.is_empty() {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|value| !value.is_zero())
            .ok_or_else(|| subscribe_timeout(endpoint, connect_timeout))?;
        set_timeout(writer, remaining).map_err(|error| {
            AppError::from_io("failed to set IPC subscribe write timeout", endpoint, error)
        })?;
        let written = match writer.write(remaining_bytes) {
            Ok(written) => written,
            // Preserve write_all's signal-interruption behavior while
            // retaining the same absolute handshake deadline.
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                return Err(closed_write(
                    "failed to write IPC subscribe frame",
                    endpoint,
                    error,
                ));
            }
        };
        if written == 0 {
            return Err(AppError::from_io(
                "failed to write IPC subscribe frame",
                endpoint,
                std::io::Error::new(
                    std::io::ErrorKind::WriteZero,
                    "subscribe write made no progress",
                ),
            )
            .into());
        }
        remaining_bytes = &remaining_bytes[written..];
    }
    let remaining = deadline
        .checked_duration_since(Instant::now())
        .filter(|value| !value.is_zero())
        .ok_or_else(|| subscribe_timeout(endpoint, connect_timeout))?;
    set_timeout(writer, remaining).map_err(|error| {
        AppError::from_io("failed to set IPC subscribe flush timeout", endpoint, error)
    })?;
    writer
        .flush()
        .map_err(|error| closed_write("failed to flush IPC subscribe frame", endpoint, error))?;
    Ok(())
}

#[cfg(test)]
#[path = "subscribe_tests.rs"]
mod tests;
