//! Isolated IPC peer; never touches the installed kernel or its statistics.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};
struct Peer {
    options: CoreIpcOptions,
    calls: Arc<Mutex<Vec<Value>>>,
    worker: Option<thread::JoinHandle<()>>,
    _directory: tempfile::TempDir,
}
impl Peer {
    fn new(mut response: impl FnMut(&Value) -> Option<Value> + Send + 'static) -> Self {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("endpoint.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let worker = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && std::time::Instant::now() < deadline =>
                    {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("mock accept failed: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let request: Value = serde_json::from_str(&line).unwrap();
                recorded.lock().unwrap().push(request.clone());
                let Some(mut reply) = (if request["type"] == "subscribe" {
                    Some(json!({"ok":true,"result":"subscribed"}))
                } else {
                    response(&request)
                }) else {
                    break;
                };
                reply["id"] = request["id"].clone();
                reply["api_id"] = json!("zero.api.v1");
                if writeln!(writer, "{reply}").is_err() {
                    break;
                }
            }
        });
        Self {
            options: CoreIpcOptions {
                socket: Some(path.to_string_lossy().into_owned()),
                timeout_ms: Some(2000),
            },
            calls,
            worker: Some(worker),
            _directory: directory,
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        let endpoint = crate::kernel::protocol::endpoint_from_options(Some(&self.options)).unwrap();
        crate::kernel::connection::reset_endpoint(&endpoint);
        self.worker.take().unwrap().join().unwrap();
    }
}

fn capabilities() -> Value {
    json!({"features":["traffic_observation_v1","traffic_period_reset_v1","traffic_scopes_sampling_v1"],"contracts":{"capabilities":{"current":1,"minimum_supported":1},"control_api":{"current":1,"minimum_supported":1}},"traffic_statistics":{"contract_version":1,"queries":["traffic_stat","traffic_stats"],"reset_command":"stats.reset","reset_permission":"admin","resettable_scopes":["global","inbound","outbound","endpoint","peer"],"reset_policy":"all_available_cumulative_no_cascade","maximum_reset_targets":256}})
}
fn ok(value: Value) -> Option<Value> {
    Some(json!({"ok":true,"result":value}))
}
fn input() -> ResetInput {
    serde_json::from_value(json!({"expected_core_instance_id":"core-a","operation_id":"reset-a","targets":[{"scope":{"kind":"endpoint","endpoint_id":"opaque:/a"},"expected_stats_epoch":"period-a","expected_generation":u64::MAX.to_string()}]})).unwrap()
}
#[tokio::test]
async fn traffic_ipc_queries_keep_variant_and_u64_and_exact_cas() {
    let peer = Peer::new(|request| {
        if request["request"].get("capabilities").is_some() {
            return ok(json!({"capabilities":capabilities()}));
        }
        if request["request"].get("traffic_stats").is_some() {
            assert_eq!(
                request["request"]["traffic_stats"]["expected_config_revision"],
                u64::MAX
            );
            return ok(
                json!({"traffic_stats":{"core_instance_id":"core-a","config_revision":u64::MAX,"registry_revision":2,"sampled_at_unix_ms":201,"scopes":[super::tests::snapshot()],"total":1,"next_offset":null}}),
            );
        }
        assert_eq!(request["method"], "stats.reset");
        assert_eq!(
            request["params"]["targets"][0]["expected_generation"],
            u64::MAX
        );
        assert_eq!(
            request["params"]["targets"][0]["expected_stats_epoch"],
            "period-a"
        );
        ok(
            json!({"accepted":true,"result":{"core_instance_id":"core-a","operation_id":"reset-a","snapshots":[super::tests::snapshot()]}}),
        )
    });
    assert_eq!(
        discover(peer.options.clone()).await.unwrap()["supported"],
        true
    );
    let query = ListInput {
        expected_config_revision: Some(u64::MAX.to_string()),
        ..Default::default()
    };
    let value = page(query, peer.options.clone()).await.unwrap();
    assert_eq!(value["config_revision"], u64::MAX.to_string());
    assert_eq!(value["total"], 1);
    let result = reset(input(), peer.options.clone()).await.unwrap();
    assert_eq!(
        result["snapshots"][0]["planes"][0]["counters"]["rx_bytes"],
        "5"
    );
    assert_eq!(
        result["snapshots"][0]["activity"]["active_stream_flows"],
        "2"
    );
}
#[tokio::test]
async fn traffic_ipc_permission_failure_and_lost_ack_never_repeat_reset() {
    for mode in [
        "permission_denied",
        "conflict",
        "not_found",
        "invalid_argument",
        "unsupported",
        "lost_ack",
    ] {
        let peer = Peer::new(move |request| {
            if request["type"] == "query" {
                return ok(json!({"capabilities":capabilities()}));
            }
            assert_eq!(request["method"], "stats.reset");
            if mode == "lost_ack" {
                return None;
            }
            Some(json!({"ok":false,"error":{"code":mode,"message":"fixture refusal"}}))
        });
        let error = reset(input(), peer.options.clone()).await.unwrap_err();
        assert_eq!(
            error.code,
            if mode == "lost_ack" {
                "connection_closed"
            } else {
                mode
            }
        );
        assert_eq!(
            peer.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c["type"] == "command")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn traffic_ipc_old_core_blocks_mutation_and_does_not_invent_http_shapes() {
    let peer = Peer::new(|request| {
        assert_eq!(request["request"], json!({"capabilities":{}}));
        ok(json!({"capabilities":{"features":[]}}))
    });
    assert_eq!(
        discover(peer.options.clone()).await.unwrap()["supported"],
        false
    );
    assert_eq!(
        reset(input(), peer.options.clone()).await.unwrap_err().code,
        "unsupported"
    );
    assert!(peer
        .calls
        .lock()
        .unwrap()
        .iter()
        .all(|c| c["type"] != "command"));
    let peer = Peer::new(|_| ok(super::tests::snapshot()));
    assert!(get(Scope::Global, peer.options.clone()).await.is_err());
}

#[tokio::test]
async fn traffic_query_only_ipc_falls_back_after_refusal_but_reset_never_replays_after_delivery() {
    for lost in [false, true] {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("query-only.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let worker = thread::spawn(move || {
            // Two connections: rejected subscribe, then one direct request.
            for index in 0..2 {
                let (stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut writer = stream.try_clone().unwrap();
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                let request: Value = serde_json::from_str(&line).unwrap();
                recorded.lock().unwrap().push(request.clone());
                if index == 0 {
                    assert_eq!(request["type"], "subscribe");
                    writeln!(writer,"{}",json!({"api_id":"zero.api.v1","id":request["id"],"ok":false,"error":{"code":"unsupported","message":"queries only"}})).unwrap();
                } else {
                    assert_eq!(request["method"], "stats.reset");
                    if !lost {
                        writeln!(writer,"{}",json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":{"accepted":true,"result":{"operation_id":"test","core_instance_id":"a","snapshots":[]}}})).unwrap();
                    }
                }
            }
        });
        let options = CoreIpcOptions {
            socket: Some(path.to_string_lossy().into_owned()),
            timeout_ms: Some(1000),
        };
        let result = traffic::reset(
            json!({"expected_core_instance_id":"a","operation_id":"test","targets":[]}),
            options.clone(),
        )
        .await;
        if lost {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap()["operation_id"], "test");
        }
        crate::kernel::connection::reset_endpoint(
            &crate::kernel::protocol::endpoint_from_options(Some(&options)).unwrap(),
        );
        worker.join().unwrap();
        assert_eq!(
            calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c["type"] == "command")
                .count(),
            1
        );
    }
}
