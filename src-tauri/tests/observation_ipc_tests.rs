//! Real Unix IPC framing and production adapter, with isolated synthetic peers.
//! No installed kernel, system proxy, TUN, or application database is changed.
#![cfg(unix)]
use gui_lib::kernel::{connection, observation::FlowObservation};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use znet_engine_client::{Binding, Endpoint};

struct Peer {
    binding: Binding,
    accepts: Arc<AtomicUsize>,
    stopping: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    _directory: tempfile::TempDir,
}
impl Peer {
    fn start(name: &'static str) -> Self {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("observation.sock");
        let listener = UnixListener::bind(&path).unwrap();
        listener.set_nonblocking(true).unwrap();
        let stopping = Arc::new(AtomicBool::new(false));
        let accepts = Arc::new(AtomicUsize::new(0));
        let stop = stopping.clone();
        let count = accepts.clone();
        let worker = thread::spawn(move || {
            let mut clients = Vec::new();
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let instance = count.fetch_add(1, Ordering::SeqCst) + 1;
                        clients.push(thread::spawn(move || serve_peer(stream, name, instance)));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(5))
                    }
                    Err(error) => panic!("accept: {error}"),
                }
            }
            for client in clients {
                client.join().unwrap();
            }
        });
        Self {
            binding: Binding {
                endpoint: Endpoint {
                    transport: "unix-socket",
                    path: path.to_string_lossy().into_owned(),
                },
                timeout: Duration::from_secs(1),
            },
            accepts,
            stopping,
            worker: Some(worker),
            _directory: directory,
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::SeqCst);
        connection::reset_endpoint(&self.binding.endpoint);
        let _ = self.worker.take().unwrap().join();
    }
}

fn serve_peer(stream: std::os::unix::net::UnixStream, name: &str, instance: usize) {
    // The accept loop polls, but each client handler blocks. Accepted socket
    // flags differ by OS, so explicitly choose the mode used by this fixture.
    stream.set_nonblocking(false).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let mut writer = stream.try_clone().unwrap();
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { break };
        let request: Value = serde_json::from_str(&line).unwrap();
        let result = if request["type"] == "subscribe" {
            json!("subscribed")
        } else {
            assert_eq!(request["type"], "query");
            let query = request["request"].as_object().unwrap();
            let (kind, args) = query.iter().next().unwrap();
            let flow = json!({
                "flow_id": format!("{name}-{instance}"), "revision": 1,
                "network": "tcp", "target": {"host": "example.test", "port": 443}
            });
            let value = match kind.as_str() {
                "runtime" => {
                    json!({"core_instance_id": format!("{name}-{instance}"), "config_revision": 1})
                }
                "stats" => json!({"connections": 1}),
                "policies" => json!([]),
                "active_flows" | "recent_flows" => {
                    assert!(args["limit"].as_u64().unwrap() <= 500);
                    json!([flow])
                }
                "flow" => {
                    assert!(args["flow_id"].is_string());
                    flow
                }
                _ => panic!("unexpected query: {kind}"),
            };
            json!({kind: value})
        };
        let response =
            json!({"api_id": "zero.api.v1", "id": request["id"], "ok": true, "result": result});
        if writeln!(writer, "{response}").is_err() {
            break;
        }
    }
}

#[tokio::test]
async fn observation_queries_are_pinned_and_endpoint_reset_is_scoped() {
    let first = Peer::start("gui");
    let other = Peer::start("external");
    let observer = FlowObservation::connect(first.binding.clone())
        .await
        .unwrap();
    let external = FlowObservation::connect(other.binding.clone())
        .await
        .unwrap();
    assert_eq!(
        observer.snapshot().await.unwrap()["runtime"]["core_instance_id"],
        "gui-1"
    );
    assert_eq!(
        external.active(None).await.unwrap().items[0].flow_id,
        "external-1"
    );
    assert_eq!(observer.detail("gui-1").await.unwrap().flow_id, "gui-1");
    assert_eq!(
        observer.recent(None).await.unwrap().items[0].flow_id,
        "gui-1"
    );
    assert_eq!(first.accepts.load(Ordering::SeqCst), 1);
    connection::reset_endpoint(&other.binding.endpoint);
    assert!(external.active(None).await.is_err());
    assert!(observer.snapshot().await.is_ok());
    assert_eq!(first.accepts.load(Ordering::SeqCst), 1);

    connection::reset_endpoint(&first.binding.endpoint);
    let replacement = FlowObservation::connect(first.binding.clone())
        .await
        .unwrap();
    assert_eq!(
        replacement.snapshot().await.unwrap()["runtime"]["core_instance_id"],
        "gui-2"
    );
    // The old reader cannot silently migrate onto a new engine connection.
    assert!(observer.active(None).await.is_err());
    assert_eq!(first.accepts.load(Ordering::SeqCst), 2);
}
