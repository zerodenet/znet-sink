//! Exercise the client IPC boundary with an isolated socket, never a real core.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    thread,
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
    json!({"features":["network_endpoint_catalog_v1","network_endpoint_control_v1"],
      "contracts":{"capabilities":{"current":1,"minimum_supported":1},"control_api":{"current":1,"minimum_supported":1}}})
}
fn ok(result: Value) -> Option<Value> {
    Some(json!({"ok":true,"result":result}))
}

#[tokio::test]
async fn endpoint_ipc_old_core_is_not_sent_undeclared_queries_or_commands() {
    let peer = Peer::new(|request| {
        assert_eq!(request["type"], "query");
        assert_eq!(request["request"], json!({"capabilities":{}}));
        ok(json!({"capabilities":{"features":[]}}))
    });
    assert!(catalog(peer.options.clone())
        .await
        .unwrap()
        .endpoints
        .is_empty());
    assert_eq!(
        control(input(Action::Restart {}), peer.options.clone())
            .await
            .unwrap_err()
            .code,
        "unsupported"
    );
    assert_eq!(
        peer.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call["type"] == "query")
            .count(),
        2
    );
}

#[tokio::test]
async fn endpoint_ipc_pagination_unwraps_variants_and_preserves_opaque_identity() {
    let peer = Peer::new(|request| {
        if request["request"].get("capabilities").is_some() {
            return ok(json!({"capabilities":capabilities()}));
        }
        let offset = request["request"]["endpoints"]["offset"].as_u64().unwrap();
        assert_eq!(request["request"]["endpoints"]["limit"], 1000);
        let mut row = current();
        row["endpoint_id"] = json!(format!("opaque:/{offset}"));
        ok(
            json!({"endpoints":{"endpoints":[row],"total":2,"next_offset":if offset == 0 { Some(1) } else { None }}}),
        )
    });
    let snapshot = catalog(peer.options.clone()).await.unwrap();
    assert_eq!(snapshot.endpoints.len(), 2);
    assert_eq!(snapshot.endpoints[1]["endpoint_id"], "opaque:/1");
    assert_eq!(
        snapshot.endpoints[0]["counters"]["inner_rx_bytes"],
        Value::Null
    );
}

#[tokio::test]
async fn endpoint_ipc_control_preserves_admin_failure_and_never_replays_lost_ack() {
    for outcome in ["success", "permission_denied", "lost_ack"] {
        let peer = Peer::new(move |request| {
            if request["type"] == "query" {
                if request["request"].get("capabilities").is_some() {
                    return ok(json!({"capabilities":capabilities()}));
                }
                assert_eq!(
                    request["request"],
                    json!({"endpoint":{"endpoint_id":"opaque:/a"}})
                );
                return ok(json!({"endpoint":current()}));
            }
            assert_eq!(request["method"], "endpoints.set_state");
            assert_eq!(
                request["params"],
                json!({"endpoint_id":"opaque:/a","expected_intent_revision":7,"enabled":false,"persistence":"runtime_only"})
            );
            match outcome {
                "permission_denied" => Some(
                    json!({"ok":false,"error":{"code":"permission_denied","message":"requires Admin"}}),
                ),
                "lost_ack" => None,
                _ => {
                    let mut row = current();
                    row["enabled"] = json!(false);
                    row["state"] = json!("stopped");
                    ok(
                        json!({"accepted":true,"result":{"applied":true,"reconciled":true,"persistence":"runtime_only","endpoint":row}}),
                    )
                }
            }
        });
        let result = control(
            input(Action::SetState {
                enabled: false,
                persistence: Persistence::RuntimeOnly,
            }),
            peer.options.clone(),
        )
        .await;
        match outcome {
            "permission_denied" => assert_eq!(result.unwrap_err().code, "permission_denied"),
            "lost_ack" => assert_eq!(result.unwrap_err().code, "connection_closed"),
            _ => assert_eq!(result.unwrap()["enabled"], false),
        }
        assert_eq!(
            peer.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call["type"] == "command")
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn endpoint_ipc_details_discard_configuration_changed_during_read() {
    let mut reads = 0;
    let peer = Peer::new(move |request| {
        if request["request"].get("capabilities").is_some() {
            return ok(json!({"capabilities":capabilities()}));
        }
        if request["request"].get("endpoint_details").is_some() {
            return ok(
                json!({"endpoint_details":{"endpoint_id":"opaque:/a","generation":null,"schema_id":"zero.endpoint.future.v1","schema_version":1,"details":{}}}),
            );
        }
        reads += 1;
        let mut row = current();
        row["config_revision"] = json!(reads);
        ok(json!({"endpoint":row}))
    });
    assert_eq!(
        details("opaque:/a".into(), "core-1".into(), peer.options.clone())
            .await
            .unwrap_err()
            .code,
        "conflict"
    );
}

#[tokio::test]
async fn endpoint_ipc_changed_configuration_refuses_control_before_mutation() {
    let peer = Peer::new(|request| {
        assert_eq!(request["type"], "query");
        if request["request"].get("capabilities").is_some() {
            return ok(json!({"capabilities":capabilities()}));
        }
        let mut row = current();
        row["config_revision"] = json!(2);
        ok(json!({"endpoint":row}))
    });
    assert_eq!(
        control_at_revision(input(Action::Restart {}), peer.options.clone(), Some(1))
            .await
            .unwrap_err()
            .code,
        "conflict"
    );
    assert!(!peer
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|call| call["type"] == "command"));
}

#[tokio::test]
async fn endpoint_ipc_direction_switch_uses_three_confirmed_commands_with_atomic_instance_conditions(
) {
    let mut row = current();
    row["supported"]["directions"]["inbound"] = json!(true);
    row["allowed"] = json!({"inbound":true,"outbound":true});
    let original = row.clone();
    let mut published = capabilities();
    published["features"]
        .as_array_mut()
        .unwrap()
        .push(json!("network_endpoint_control_preconditions_v1"));
    let caps = crate::kernel::zero::parsing::parse_capabilities(&published, None);
    let peer = Peer::new(move |request| {
        if request["type"] == "query" {
            if request["request"].get("capabilities").is_some() {
                return ok(json!({"capabilities":published}));
            }
            return ok(json!({"endpoint":row}));
        }
        assert_eq!(request["params"]["expected_core_instance_id"], "core-1");
        assert_eq!(
            request["params"]["expected_intent_revision"],
            row["intent_revision"]
        );
        assert_eq!(request["params"]["persistence"], "runtime_only");
        if request["method"] == "endpoints.set_state" {
            row["enabled"] = request["params"]["enabled"].clone();
            row["state"] = if row["enabled"] == true {
                json!("running")
            } else {
                json!("stopped")
            };
        } else {
            assert_eq!(row["state"], "stopped");
            row["allowed"] = request["params"]["directions"].clone();
        }
        row["intent_revision"] = json!(row["intent_revision"].as_u64().unwrap() + 1);
        ok(
            json!({"accepted":true,"result":{"applied":true,"reconciled":true,"persistence":"runtime_only","endpoint":row}}),
        )
    });
    let mut caps = caps;
    caps.global_limitations
        .push("endpoint_live_outbound_direction_contraction_requires_stop".into());
    let response = direction_transition::apply(
        input(Action::SetDirections {
            directions: Directions {
                inbound: true,
                outbound: false,
            },
            persistence: Persistence::RuntimeOnly,
        }),
        original,
        &caps,
        peer.options.clone(),
        1,
    )
    .await
    .unwrap();
    assert_eq!(response["enabled"], true);
    assert_eq!(response["allowed"]["outbound"], false);
    assert_eq!(
        peer.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call["type"] == "command")
            .count(),
        3
    );
}
