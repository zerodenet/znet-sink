//! Deterministic write/flush faults exercise the production handshake writer
//! on every host, including Windows. They do not emulate a working kernel.
use super::*;
use std::collections::VecDeque;

enum Step {
    Bytes(usize),
    Fail(io::ErrorKind),
}
#[derive(Default)]
struct Writer {
    steps: VecDeque<Step>,
    bytes: Vec<u8>,
    writes: usize,
    flushes: usize,
    flush_error: Option<io::ErrorKind>,
}
impl Write for Writer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.writes += 1;
        let count = match self.steps.pop_front() {
            Some(Step::Bytes(count)) => count.min(bytes.len()),
            Some(Step::Fail(kind)) => return Err(io::Error::from(kind)),
            None => bytes.len(),
        };
        self.bytes.extend_from_slice(&bytes[..count]);
        Ok(count)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushes += 1;
        self.flush_error.map_or(Ok(()), |kind| Err(kind.into()))
    }
}
fn endpoint() -> CoreEndpoint {
    CoreEndpoint {
        transport: "test-only",
        path: "isolated-handshake-writer".into(),
    }
}
const FRAME: &[u8] = b"{\"type\":\"subscribe\",\"events\":[]}\n";
fn send(writer: &mut Writer) -> Result<(), HandshakeFailure> {
    write_frame(
        writer,
        FRAME,
        &endpoint(),
        Instant::now() + Duration::from_secs(1),
        Duration::from_secs(1),
        |_, _| Ok(()),
    )
}

#[test]
fn closed_transport_during_initial_or_partial_subscribe_allows_fresh_handshake() {
    for kind in [
        io::ErrorKind::BrokenPipe,
        io::ErrorKind::ConnectionReset,
        io::ErrorKind::ConnectionAborted,
        io::ErrorKind::UnexpectedEof,
    ] {
        for prefix in [0, 4] {
            let mut writer = Writer::default();
            if prefix > 0 {
                writer.steps.push_back(Step::Bytes(prefix));
            }
            writer.steps.push_back(Step::Fail(kind));
            assert!(
                matches!(send(&mut writer), Err(HandshakeFailure::ClosedBeforeAck(error))
                if error.code == "connection_closed" && error.details.as_ref().unwrap()["ioKind"] == format!("{kind:?}"))
            );
            assert_eq!(writer.bytes, FRAME[..prefix]);
            assert_eq!(writer.writes, usize::from(prefix > 0) + 1);
            assert_eq!(
                writer.flushes, 0,
                "failed transport must not receive more writes"
            );
        }
    }
}

#[test]
fn closed_flush_is_pre_ack_but_other_write_failures_are_terminal() {
    let mut writer = Writer {
        flush_error: Some(io::ErrorKind::BrokenPipe),
        ..Writer::default()
    };
    assert!(matches!(
        send(&mut writer),
        Err(HandshakeFailure::ClosedBeforeAck(_))
    ));
    assert_eq!(writer.bytes, FRAME);
    assert_eq!(writer.flushes, 1);
    for kind in [
        io::ErrorKind::PermissionDenied,
        io::ErrorKind::TimedOut,
        io::ErrorKind::WouldBlock,
        io::ErrorKind::InvalidInput,
        io::ErrorKind::Other,
    ] {
        let mut writer = Writer::default();
        writer.steps.push_back(Step::Fail(kind));
        assert!(
            matches!(send(&mut writer), Err(HandshakeFailure::Terminal(_))),
            "{kind:?}"
        );
        assert_eq!(writer.writes, 1);
    }
    let mut writer = Writer::default();
    writer.steps.push_back(Step::Bytes(0));
    assert!(
        matches!(send(&mut writer), Err(HandshakeFailure::Terminal(error))
        if error.details.as_ref().unwrap()["ioKind"] == "WriteZero")
    );
}

#[test]
fn interrupted_partial_writes_preserve_bytes_and_the_original_budget() {
    let mut writer = Writer::default();
    writer.steps.extend([
        Step::Fail(io::ErrorKind::Interrupted),
        Step::Bytes(3),
        Step::Bytes(2),
    ]);
    let budget = Duration::from_secs(1);
    let mut timeouts = Vec::new();
    assert!(write_frame(
        &mut writer,
        FRAME,
        &endpoint(),
        Instant::now() + budget,
        budget,
        |_, remaining| {
            timeouts.push(remaining);
            Ok(())
        }
    )
    .is_ok());
    assert_eq!(writer.bytes, FRAME);
    assert_eq!(writer.writes, 4);
    assert_eq!(writer.flushes, 1);
    assert_eq!(timeouts.len(), 5);
    assert!(timeouts[0] <= budget);
    assert!(timeouts.windows(2).all(|pair| pair[1] <= pair[0]));
}

#[test]
fn expired_deadline_and_timeout_setup_error_never_send_or_retry() {
    let mut writer = Writer::default();
    assert!(matches!(
        write_frame(
            &mut writer,
            FRAME,
            &endpoint(),
            Instant::now(),
            Duration::from_secs(1),
            |_, _| panic!("expired deadline must not set a timeout")
        ),
        Err(HandshakeFailure::Terminal(_))
    ));
    assert!(matches!(
        write_frame(
            &mut writer,
            FRAME,
            &endpoint(),
            Instant::now() + Duration::from_secs(1),
            Duration::from_secs(1),
            |_, _| Err(io::ErrorKind::BrokenPipe.into())
        ),
        Err(HandshakeFailure::Terminal(_))
    ));
    assert_eq!(writer.writes, 0);
    assert_eq!(writer.flushes, 0);
}

#[cfg(windows)]
#[test]
fn windows_pipe_is_being_closed_error_232_is_pre_ack_only() {
    assert!(
        matches!(closed_write("subscribe write", &endpoint(), io::Error::from_raw_os_error(232)),
        HandshakeFailure::ClosedBeforeAck(error) if error.code == "connection_closed")
    );
}
