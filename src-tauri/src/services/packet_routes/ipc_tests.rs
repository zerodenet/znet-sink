//! Unix framing through the production Zero adapter with isolated test peers.
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
    // Real core publishes Read metadata even though local IPC authorizes Admin.
    json!({"permissions":["read"],"features":["packet_route_management_v1"],"contracts":{"capabilities":{"current":1,"minimum_supported":1},"control_api":{"current":1,"minimum_supported":1}}})
}
fn ok(value: Value) -> Option<Value> {
    Some(json!({"ok":true,"result":value}))
}
fn row(state: &str) -> Value {
    json!({"route_id":"opaque:packet:1","core_instance_id":"core-a","config_revision":u64::MAX,"inbound_tag":"tun","outbound_tag":"wg","endpoints":[{"endpoint_id":"device","generation":u64::MAX}],"source":"::1","destination":"::2","ip_protocol":58,"translated":true,"started_at_unix_ms":u64::MAX,"state":state})
}
fn target() -> PacketTarget {
    PacketTarget {
        profile_id: Some("p1".into()),
        route_id: "opaque:packet:1".into(),
        core_instance_id: "core-a".into(),
        config_revision: u64::MAX.to_string(),
    }
}
fn read(request: &Value) -> Option<Value> {
    let q = &request["request"];
    if q.get("capabilities").is_some() {
        return ok(json!({"capabilities":capabilities()}));
    }
    if q.get("runtime").is_some() {
        return ok(json!({"runtime":{"core_instance_id":"core-a","config_revision":u64::MAX}}));
    }
    if q.get("packet_routes").is_some() {
        assert_eq!(q["packet_routes"]["limit"], 500);
        return ok(
            json!({"packet_routes":{"core_instance_id":"core-a","config_revision":u64::MAX,"observed_at_unix_ms":u64::MAX,"routes":[row("active")],"total":1,"next_offset":null}}),
        );
    }
    assert!(q.get("packet_route").is_some(), "unexpected query {q}");
    assert_eq!(q["packet_route"]["route_id"], "opaque:packet:1");
    ok(json!({"packet_route":row("active")}))
}
#[tokio::test]
async fn packet_ipc_variants_lossless_projection_and_admin_transport() {
    let mut closed = false;
    let peer = Peer::new(move |r| {
        if r["type"] == "query" {
            if closed && r["request"].get("packet_route").is_some() {
                return ok(json!({"packet_route":row("closed")}));
            }
            return read(r);
        }
        assert_eq!(r["method"], "packet_routes.close");
        assert_eq!(r["params"]["route_id"], "opaque:packet:1");
        assert_eq!(r["params"]["expected_core_instance_id"], "core-a");
        assert_eq!(r["params"]["expected_config_revision"], u64::MAX);
        closed = true;
        ok(json!({"accepted":true,"result":{"closed":true,"route":row("closed")}}))
    });
    let view = snapshot(Some("p1".into()), peer.options.clone())
        .await
        .unwrap();
    assert!(view.supported && view.can_close);
    assert_eq!(view.routes[0].config_revision, u64::MAX.to_string());
    assert_eq!(view.routes[0].started_at_unix_ms, u64::MAX.to_string());
    assert_eq!(
        view.routes[0].endpoints[0].generation,
        Some(u64::MAX.to_string())
    );
    assert_eq!(
        detail(target(), peer.options.clone())
            .await
            .unwrap()
            .route_id,
        "opaque:packet:1"
    );
    let result = close(target(), peer.options.clone()).await.unwrap();
    assert_eq!(result.status, "confirmed");
    assert_eq!(result.route.unwrap().state, "closed");
    assert_eq!(
        peer.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["type"] == "command")
            .count(),
        1
    );
}
#[tokio::test]
async fn unsupported_packet_capability_is_not_an_authoritative_empty_catalog() {
    let peer = Peer::new(|r| {
        assert!(r["request"].get("capabilities").is_some());
        let mut caps = capabilities();
        caps["features"] = json!([]);
        ok(json!({"capabilities":caps}))
    });
    let view = snapshot(None, peer.options.clone()).await.unwrap();
    assert!(!view.supported);
    assert!(view.reason.is_some());
    assert_eq!(view.total, None);
    assert_eq!(
        peer.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["type"] == "query")
            .count(),
        1
    );
}
#[tokio::test]
async fn packet_close_refusal_lost_and_malformed_receipts_never_replay() {
    for mode in ["permission_denied", "conflict", "lost_ack", "malformed_ack"] {
        let peer = Peer::new(move |r| {
            if r["type"] == "query" {
                return read(r);
            }
            assert_eq!(r["method"], "packet_routes.close");
            if mode == "lost_ack" {
                return None;
            }
            if mode == "malformed_ack" {
                return ok(json!({"accepted":true,"result":{"closed":true,"route":{}}}));
            }
            Some(json!({"ok":false,"error":{"code":mode,"message":"fixture refusal"}}))
        });
        let result = close(target(), peer.options.clone()).await;
        if ["lost_ack", "malformed_ack"].contains(&mode) {
            assert_eq!(result.unwrap().status, "unconfirmed");
        } else {
            assert_eq!(result.unwrap_err().code, mode);
        }
        assert_eq!(
            peer.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|r| r["type"] == "command")
                .count(),
            1
        );
    }
}
#[tokio::test]
async fn stale_packet_target_stops_before_any_command() {
    let peer = Peer::new(read);
    let mut old = target();
    old.core_instance_id = "previous-core".into();
    assert_eq!(
        close(old, peer.options.clone()).await.unwrap_err().code,
        "conflict"
    );
    assert!(!peer
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|r| r["type"] == "command"));
}
#[tokio::test]
async fn catalog_rejects_instance_change_during_snapshot() {
    let mut reads = 0;
    let peer = Peer::new(move |r| {
        if r["request"].get("runtime").is_some() {
            reads += 1;
            return ok(
                json!({"runtime":{"core_instance_id":if reads == 1 {"core-a"} else {"replacement"},"config_revision":u64::MAX}}),
            );
        }
        read(r)
    });
    assert_eq!(
        snapshot(None, peer.options.clone()).await.unwrap_err().code,
        "conflict"
    );
}
#[tokio::test]
async fn released_packet_requires_matching_closed_receipt_before_confirming_absence() {
    let mut closed = false;
    let peer = Peer::new(move |r| {
        if r["type"] == "query" {
            if closed && r["request"].get("packet_route").is_some() {
                return Some(json!({"ok":false,"error":{"code":"not_found","message":"released"}}));
            }
            return read(r);
        }
        closed = true;
        ok(json!({"accepted":true,"result":{"closed":true,"route":row("closed")}}))
    });
    assert_eq!(
        close(target(), peer.options.clone()).await.unwrap().status,
        "confirmed"
    );
}
#[tokio::test]
async fn close_acknowledgement_does_not_override_an_active_authoritative_route() {
    let peer = Peer::new(|r| {
        if r["type"] == "query" {
            return read(r);
        }
        ok(json!({"accepted":true,"result":{"closed":true,"route":row("closed")}}))
    });
    assert_eq!(
        close(target(), peer.options.clone()).await.unwrap().status,
        "unconfirmed"
    );
    assert_eq!(
        peer.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["type"] == "command")
            .count(),
        1
    );
}
