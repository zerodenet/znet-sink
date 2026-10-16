//! Actual Unix framing and multiplexing. The peer is an isolated protocol
//! fixture, not an installed kernel, native Proxy, or VPN acceptance test.
use super::*;
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::atomic::AtomicUsize,
};

struct Peer {
    endpoint: CoreEndpoint,
    worker: Option<thread::JoinHandle<()>>,
    calls: Arc<AtomicUsize>,
    _directory: tempfile::TempDir,
}
impl Peer {
    fn new(
        mut handle: impl FnMut(&Value, &mut std::os::unix::net::UnixStream) + Send + 'static,
    ) -> Self {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("bound.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let recorded = calls.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else {
                    break;
                };
                let request: Value = serde_json::from_str(&line).unwrap();
                recorded.fetch_add(1, Ordering::Relaxed);
                if request["type"] == "subscribe" {
                    if writeln!(writer,"{}",json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":"subscribed"})).is_err(){break;}
                } else {
                    handle(&request, &mut writer);
                }
            }
        });
        Self {
            endpoint: CoreEndpoint {
                transport: "unix-socket",
                path: path.to_string_lossy().into_owned(),
            },
            worker: Some(worker),
            calls,
            _directory: directory,
        }
    }
    fn connect(&self) -> MultiplexedConnection {
        get_or_connect(self.endpoint.clone(), Duration::from_secs(1)).unwrap()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        reset_endpoint(&self.endpoint);
        if let Some(worker) = self.worker.take() {
            worker.join().unwrap();
        }
    }
}
fn frame(id: &str) -> Vec<u8> {
    transport::serialize_frame(&json!({"type":"query","id":id,"request":{"runtime":{}}})).unwrap()
}
async fn until(mut condition: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(2), async {
        while !condition() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn stopping_actual_gui_listener_keeps_pending_control_and_new_observer_on_same_peer() {
    let peer = Peer::new(|request, writer| {
        thread::sleep(Duration::from_millis(40));
        let _ = writeln!(
            writer,
            "{}",
            json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":{"accepted":true}})
        );
        let _ = writeln!(
            writer,
            "{}",
            json!({"event_type":"flow.started","core_instance_id":"one","config_revision":1,"sequence":1,"payload":{}})
        );
    });
    let connection = peer.connect();
    let state = crate::state::app_state::AppState::default();
    let old = state.observations().begin(znet_engine_client::Binding {
        endpoint: peer.endpoint.clone(),
        timeout: Duration::from_secs(1),
    });
    let listener = connection.subscribe_events();
    let target = connection.clone();
    let command = tauri::async_runtime::spawn(async move {
        target.request(transport::serialize_frame(&json!({"type":"command","id":"control-1","method":"flows.close","params":{"flow_id":"owned-flow"}})).unwrap(),"control-1".into(),Duration::from_secs(1)).await
    });
    until(|| connection.has_pending_requests()).await;
    crate::services::gui_events::stop(&state);
    drop(listener);
    assert!(!old.is_current());
    assert!(connection.is_alive());
    let same = peer.connect();
    assert!(Arc::ptr_eq(&connection.inner, &same.inner));
    let mut observer = same.subscribe_events();
    assert_eq!(command.await.unwrap().unwrap()["ok"], true);
    until(|| !observer.is_empty()).await;
    assert_eq!(observer.try_recv().unwrap()["event_type"], "flow.started");
    assert_eq!(
        same.request(
            frame("new-observer-query"),
            "new-observer-query".into(),
            Duration::from_secs(1)
        )
        .await
        .unwrap()["ok"],
        true
    );
    assert_eq!(peer.calls.load(Ordering::Relaxed), 3); // one subscribe, command, new query
}

#[tokio::test]
async fn pending_budget_duplicate_ids_and_cancelled_waiters_are_bounded_before_writing() {
    let peer = Peer::new(|_, _| {});
    let connection = peer.connect();
    let mut requests = Vec::new();
    for index in 0..MAX_PENDING_REQUESTS {
        let target = connection.clone();
        let id = format!("pending-{index}");
        requests.push(tauri::async_runtime::spawn(async move {
            target.request(frame(&id), id, Duration::from_secs(3)).await
        }));
    }
    until(|| recover_lock(&connection.inner.pending).len() == MAX_PENDING_REQUESTS).await;
    assert_eq!(
        connection
            .request(frame("overflow"), "overflow".into(), Duration::from_secs(1))
            .await
            .unwrap_err()
            .code,
        "busy"
    );
    assert_eq!(
        connection
            .request(
                frame("pending-0"),
                "pending-0".into(),
                Duration::from_secs(1)
            )
            .await
            .unwrap_err()
            .code,
        "busy"
    );
    requests[0].abort();
    until(|| recover_lock(&connection.inner.pending).len() == MAX_PENDING_REQUESTS - 1).await;
    connection.retire();
    for request in requests.into_iter().skip(1) {
        assert!(request.await.unwrap().is_err());
    }
}

#[tokio::test]
async fn oversized_event_is_gap_and_buffer_overflow_is_reported_to_each_listener() {
    let peer = Peer::new(|request, writer| {
        let event =
            json!({"event_type":"flow.started","payload":{"detail":"x".repeat(MAX_EVENT_BYTES)}});
        let _ = writeln!(writer, "{event}");
        for sequence in 0..=EVENT_CHANNEL_CAPACITY {
            let _ = writeln!(
                writer,
                "{}",
                json!({"event_type":"flow.started","sequence":sequence})
            );
        }
        let _ = writeln!(
            writer,
            "{}",
            json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":{}})
        );
    });
    let connection = peer.connect();
    let mut oversize = connection.subscribe_events();
    connection
        .request(frame("trigger"), "trigger".into(), Duration::from_secs(1))
        .await
        .unwrap();
    assert!(matches!(
        oversize.try_recv(),
        Err(broadcast::error::TryRecvError::Lagged(_))
    ));
    // Drain overflow then exercise the marker without another burst overwriting it.
    let single = Peer::new(|request, writer| {
        let event =
            json!({"event_type":"flow.started","payload":{"detail":"x".repeat(MAX_EVENT_BYTES)}});
        let _ = writeln!(writer, "{event}");
        let _ = writeln!(
            writer,
            "{}",
            json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":{}})
        );
    });
    let connection = single.connect();
    let mut listener = connection.subscribe_events();
    connection
        .request(frame("single"), "single".into(), Duration::from_secs(1))
        .await
        .unwrap();
    assert!(is_gap_event(&listener.try_recv().unwrap()));
}

#[test]
fn line_framing_is_bounded_before_allocation_and_invalid_json_has_no_secret_echo() {
    let endpoint = CoreEndpoint {
        transport: "unix-socket",
        path: "not-used".into(),
    };
    let input = vec![b'x'; transport::MAX_FRAME_BYTES + 20];
    let mut reader = BufReader::new(std::io::Cursor::new(input));
    assert_eq!(
        transport::read_json_line(&mut reader, &endpoint)
            .unwrap_err()
            .code,
        "invalid_response"
    );
    assert!(reader.get_ref().position() <= (transport::MAX_FRAME_BYTES + 8192) as u64);
    let mut reader = BufReader::new(std::io::Cursor::new(b"{\"secret\":token-not-valid-json}\n"));
    let error = transport::read_json_line(&mut reader, &endpoint).unwrap_err();
    assert!(!format!("{error:?}").contains("token-not-valid-json"));
    assert!(
        transport::serialize_frame(&json!({"large":"x".repeat(transport::MAX_FRAME_BYTES)}))
            .is_err()
    );
}
