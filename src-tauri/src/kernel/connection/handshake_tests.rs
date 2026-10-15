//! Real local framing only; these peers do not establish Windows/core acceptance.
use super::*;
use serde_json::json;
use std::{
    io::{BufRead, BufReader},
    os::unix::net::{UnixListener, UnixStream},
    sync::atomic::AtomicUsize,
};

struct Peer {
    endpoint: CoreEndpoint,
    attempts: Arc<AtomicUsize>,
    frames: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    _directory: tempfile::TempDir,
}
impl Peer {
    fn new(
        mut serve: impl FnMut(usize, UnixStream, Arc<Mutex<Vec<Value>>>) + Send + 'static,
    ) -> Self {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("handshake.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let attempts = Arc::new(AtomicUsize::new(0));
        let count = attempts.clone();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let recorded = frames.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let worker = thread::spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let attempt = count.fetch_add(1, Ordering::AcqRel);
                        serve(attempt, stream, recorded.clone());
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("isolated IPC accept failed: {error}"),
                }
            }
        });
        Self {
            endpoint: CoreEndpoint {
                transport: "unix-socket",
                path: path.to_string_lossy().into_owned(),
            },
            attempts,
            frames,
            stop,
            worker: Some(worker),
            _directory: directory,
        }
    }
    fn count(&self) -> usize {
        self.attempts.load(Ordering::Acquire)
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        reset_endpoint(&self.endpoint);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn read_frame(reader: &mut BufReader<UnixStream>, frames: &Mutex<Vec<Value>>) -> Option<Value> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let frame: Value = serde_json::from_str(&line).unwrap();
    recover_lock(frames).push(frame.clone());
    Some(frame)
}
fn subscribe(stream: &UnixStream, frames: &Mutex<Vec<Value>>) -> BufReader<UnixStream> {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let first = read_frame(&mut reader, frames).unwrap();
    assert_eq!(first["type"], "subscribe");
    assert_eq!(first["id"], SUBSCRIBE_FRAME_ID);
    reader
}
fn ack(stream: &mut UnixStream) {
    writeln!(
        stream,
        "{}",
        json!({"id":SUBSCRIBE_FRAME_ID,"ok":true,"api_id":"zero.api.v1","result":"subscribed"})
    )
    .unwrap();
}
#[test]
fn reader_eof_notifies_subscribe_waiter_without_waiting_for_timeout() {
    let peer = Peer::new(|_, stream, frames| {
        let _reader = subscribe(&stream, &frames);
    });
    let start = Instant::now();
    let failure = MultiplexedConnection::connect_once(
        peer.endpoint.clone(),
        start + Duration::from_secs(3),
        Duration::from_secs(3),
    );
    assert!(
        matches!(failure,Err(HandshakeFailure::ClosedBeforeAck(ref error)) if error.code=="connection_closed")
    );
    assert!(
        start.elapsed() < Duration::from_secs(1),
        "EOF must wake the actual ack waiter"
    );
    assert_eq!(peer.count(), 1);
}
#[tokio::test]
async fn cold_first_eof_second_ack_publishes_only_success_and_sends_business_once() {
    let peer = Peer::new(|attempt, mut stream, frames| {
        let mut reader = subscribe(&stream, &frames);
        if attempt == 0 {
            return;
        }
        ack(&mut stream);
        while let Some(frame) = read_frame(&mut reader, &frames) {
            assert_eq!(frame["type"], "query");
            if writeln!(stream,"{}",json!({"id":frame["id"],"ok":true,"api_id":"zero.api.v1","result":{"runtime":{"core_instance_id":"fixed-peer","config_revision":1}}})).is_err(){break;}
        }
    });
    let connection = get_or_connect(peer.endpoint.clone(), Duration::from_secs(3)).unwrap();
    assert_eq!(peer.count(), 2);
    assert!(Arc::ptr_eq(
        &connection.inner,
        &get_or_connect(peer.endpoint.clone(), Duration::from_secs(3))
            .unwrap()
            .inner
    ));
    let id = "only-business".to_string();
    let frame =
        transport::serialize_frame(&json!({"id":id,"type":"query","request":{"runtime":{}}}))
            .unwrap();
    assert_eq!(
        connection
            .request(frame, id, Duration::from_secs(1))
            .await
            .unwrap()["ok"],
        true
    );
    let frames = recover_lock(&peer.frames);
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["type"] == "subscribe")
            .count(),
        2
    );
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["type"] == "query")
            .count(),
        1
    );
}
#[test]
fn persistent_early_eof_stops_after_three_attempts_without_business_frames() {
    let peer = Peer::new(|_, stream, frames| {
        let _reader = subscribe(&stream, &frames);
    });
    let start = Instant::now();
    let error = match MultiplexedConnection::connect(peer.endpoint.clone(), Duration::from_secs(3))
    {
        Ok(connection) => {
            connection.retire();
            panic!("no ack was sent")
        }
        Err(error) => error,
    };
    assert_eq!(error.code, "connection_closed");
    assert_eq!(peer.count(), 3);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(recover_lock(&peer.frames)
        .iter()
        .all(|frame| frame["type"] == "subscribe"));
}
#[test]
fn retry_ack_wait_uses_original_deadline_instead_of_a_fresh_budget() {
    let peer = Peer::new(|attempt, mut stream, frames| {
        let mut reader = subscribe(&stream, &frames);
        if attempt == 0 {
            thread::sleep(Duration::from_millis(550));
            return;
        }
        // Arrives after the original 1s deadline, but before an incorrectly
        // refreshed second-attempt deadline. Assert outcome, not a narrow
        // wall-clock upper bound under concurrent CI load.
        thread::sleep(Duration::from_millis(650));
        let _ = writeln!(
            stream,
            "{}",
            json!({"id":SUBSCRIBE_FRAME_ID,"ok":true,"api_id":"zero.api.v1","result":"subscribed"})
        );
        assert!(read_frame(&mut reader, &frames).is_none());
    });
    let error = match MultiplexedConnection::connect(peer.endpoint.clone(), Duration::from_secs(1))
    {
        Ok(connection) => {
            connection.retire();
            panic!("late acknowledgement refreshed the original budget")
        }
        Err(error) => error,
    };
    assert_eq!(error.code, "connection_closed");
    assert_eq!(peer.count(), 2);
    assert_eq!(error.details.unwrap()["timeoutMs"], 1000);
}
#[test]
fn rejection_invalid_ack_and_reader_decode_errors_are_terminal_without_reconnect() {
    for mode in [
        "denied",
        "wrong-api",
        "wrong-result",
        "bad-json",
        "core-closed",
    ] {
        let peer = Peer::new(move |_, mut stream, frames| {
            let _reader = subscribe(&stream, &frames);
            if mode == "bad-json" {
                writeln!(stream, "{{invalid-json").unwrap();
                return;
            }
            let frame = match mode {
                "denied" => {
                    json!({"id":SUBSCRIBE_FRAME_ID,"ok":false,"error":{"code":"permission_denied","message":"denied"}})
                }
                "core-closed" => {
                    json!({"id":SUBSCRIBE_FRAME_ID,"ok":false,"error":{"code":"connection_closed","message":"negative receipt"}})
                }
                "wrong-api" => {
                    json!({"id":SUBSCRIBE_FRAME_ID,"ok":true,"api_id":"other.api","result":"subscribed"})
                }
                _ => {
                    json!({"id":SUBSCRIBE_FRAME_ID,"ok":true,"api_id":"zero.api.v1","result":"not-subscribed"})
                }
            };
            writeln!(stream, "{frame}").unwrap();
        });
        let start = Instant::now();
        let error =
            match MultiplexedConnection::connect(peer.endpoint.clone(), Duration::from_secs(3)) {
                Ok(connection) => {
                    connection.retire();
                    panic!("invalid ack accepted")
                }
                Err(error) => error,
            };
        assert_eq!(peer.count(), 1);
        assert!(start.elapsed() < Duration::from_secs(1));
        assert_eq!(
            error.code,
            match mode {
                "denied" => "permission_denied",
                "core-closed" => "core_error",
                _ => "invalid_response",
            }
        );
    }
}
#[tokio::test]
async fn acknowledged_bound_control_business_failure_never_reconnects_or_replays() {
    let peer = Peer::new(|_, mut stream, frames| {
        let mut reader = subscribe(&stream, &frames);
        ack(&mut stream);
        let business = read_frame(&mut reader, &frames).unwrap();
        assert_eq!(business["type"], "command");
        assert_eq!(business["id"], "once-only-bound-business");
        // EOF after receiving a command is an unknown side effect, never a
        // reason to rediscover this bound control or send a replacement call.
    });
    let control =
        crate::kernel::configuration::BoundControl::connect(crate::models::core::CoreIpcOptions {
            socket: Some(peer.endpoint.path.clone()),
            timeout_ms: Some(1000),
        })
        .await
        .unwrap();
    assert!(control
        .call(json!({"type":"command","id":"once-only-bound-business","method":"flows.close","params":{"flow_id":"fixed-flow"}}))
        .await
        .is_err());
    assert!(control.identity().await.is_err());
    assert_eq!(peer.count(), 1);
    let frames = recover_lock(&peer.frames);
    assert_eq!(
        frames
            .iter()
            .filter(|frame| frame["type"] == "command")
            .count(),
        1
    );
    assert!(!frames.iter().any(|frame| frame["type"] == "query"));
}
