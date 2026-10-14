//! Isolated real Unix transport. These are contract fixtures, never product flows.
use super::*;
use crate::{kernel::configuration::BoundControl, models::core::CoreIpcOptions};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::Arc,
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
    fn new(mut reply: impl FnMut(&Value) -> Option<Value> + Send + 'static) -> Self {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("active.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let request: Value = serde_json::from_str(&line).unwrap();
                recorded.lock().unwrap().push(request.clone());
                let result = if request["type"] == "subscribe" {
                    Some(json!({"ok":true,"result":"subscribed"}))
                } else {
                    reply(&request)
                };
                let Some(mut response) = result else { break };
                response["id"] = request["id"].clone();
                response["api_id"] = json!("zero.api.v1");
                if writeln!(writer, "{response}").is_err() {
                    break;
                }
            }
        });
        Self {
            options: CoreIpcOptions {
                socket: Some(path.to_string_lossy().into_owned()),
                timeout_ms: Some(1000),
            },
            calls,
            worker: Some(worker),
            _directory: directory,
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        crate::kernel::connection::reset_endpoint(
            &crate::kernel::protocol::endpoint_from_options(Some(&self.options)).unwrap(),
        );
        self.worker.take().unwrap().join().unwrap();
    }
}
fn ok(value: Value) -> Option<Value> {
    Some(json!({"ok":true,"result":value}))
}
fn capabilities() -> Value {
    json!({"features":["runtime_snapshot","flow_snapshot"],"permissions":["read"],"contracts":{"capabilities":{"current":1,"minimum_supported":1},"control_api":{"current":1,"minimum_supported":1}}})
}
fn row(id: &str, network: &str, outbound: &str) -> Value {
    json!({"record":{"flow_id":id,"revision":u64::MAX,"state":"active","network":network,"inbound":{"tag":"tun","protocol":"tun"},"auth":{"scheme":"private","principal_key":"secret","attributes":{"token":"secret"}},"source":{"ip":"2001:db8::1","port":1234,"process_path":"/private/secret"},"target":{"host":"example.com","port":443,"resolved_ip":"192.0.2.4"},"route":{"mode":"rule","action":"proxy","matched_rule":{"index":0,"condition":"secret-condition"},"selection_chain":["group"]},"path":{"outbound":{"tag":outbound,"protocol":"ss"},"remote":{"host":"192.0.2.5","port":443}},"traffic":{"bytes_up":u64::MAX,"bytes_down":9007199254740993_u64},"throughput":{"upload_bps":4,"download_bps":5,"sampled_at_unix_ms":9007199254740993_u64},"timing":{"started_at_unix_ms":9007199254740993_u64,"last_activity_at_unix_ms":9007199254740994_u64}}})
}
fn common(request: &Value, revision: u64) -> Option<Value> {
    let q = &request["request"];
    if q.get("capabilities").is_some() {
        return ok(json!({"capabilities":capabilities()}));
    }
    if q.get("runtime").is_some() {
        return ok(
            json!({"runtime":{"core_instance_id":"actual-test-instance","config_revision":revision}}),
        );
    }
    None
}
#[tokio::test]
async fn active_export_real_ipc_fixes_authoritative_filtered_ids_and_precise_facts_without_requery_on_delivery(
) {
    let rows = Arc::new(Mutex::new(vec![
        row("flow-a", "tcp", "proxy-a"),
        row("flow-b", "tcp", "proxy-b"),
        row("flow-c", "udp", "proxy-a"),
    ]));
    let current = rows.clone();
    let peer = Peer::new(move |request| {
        if let Some(response) = common(request, u64::MAX) {
            return Some(response);
        }
        let query: zero_api::QueryRequest =
            serde_json::from_value(request["request"].clone()).unwrap();
        assert_eq!(query.required_permission(), zero_api::Permission::Read);
        let zero_api::QueryRequest::ActiveFlows(query) = query else {
            panic!("unexpected query")
        };
        assert_eq!(query.limit, Some(RECORDS + 1));
        assert_eq!(query.filter.network, Some(zero_api::Network::Tcp));
        ok(json!({"active_flows":current.lock().unwrap().clone()}))
    });
    let filter = active::Filter {
        search: Some("EXAMPLE".into()),
        protocol: Some("tcp".into()),
        outbound: Some("proxy-a".into()),
    };
    let (lease, records, truncated, observed) = active::read(
        BoundControl::connect(peer.options.clone()).await.unwrap(),
        "binding".into(),
        &filter,
    )
    .await
    .unwrap();
    assert_eq!(records.len(), 1);
    assert!(!truncated);
    let (view, bytes) = active::freeze(&filter, &lease, records, truncated, observed, 100).unwrap();
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["records"][0]["flowId"], "flow-a");
    assert_eq!(document["records"][0]["bytesUp"], u64::MAX.to_string());
    assert_eq!(document["records"][0]["bytesDown"], "9007199254740993");
    assert_eq!(
        document["records"][0]["sourceAddress"],
        "[2001:db8::1]:1234"
    );
    assert_eq!(document["records"][0]["targetHost"], "example.com");
    assert!(!String::from_utf8(bytes.clone()).unwrap().contains("secret"));
    rows.lock()
        .unwrap()
        .push(row("new-after-preview", "tcp", "proxy-a"));
    rows.lock().unwrap()[0]["record"]["traffic"]["bytes_up"] = json!(0);
    active::verify(&lease).await.unwrap();
    assert_eq!(
        peer.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["request"].get("active_flows").is_some())
            .count(),
        1
    );
    let scope = super::tests::scope();
    let mut plans = BTreeMap::new();
    plans.insert(
        view.plan_id.clone(),
        Plan {
            active: Some(lease),
            scope: scope.clone(),
            expires: 200,
            bytes: Some(bytes.clone()),
        },
    );
    let delivered = take_plan(&mut plans, &view.plan_id, &scope, 199).unwrap();
    assert_eq!(delivered, bytes);
    PreparedExport::new(
        ExportPurpose::RedactedDiagnostic,
        ExportOwnership::HostGenerated,
        "active.json",
        "application/json",
        delivered,
        false,
    )
    .unwrap();
    assert!(take_plan(&mut plans, &view.plan_id, &scope, 199).is_err());
}
#[tokio::test]
async fn active_export_read_permission_denial_is_not_an_empty_or_healthy_snapshot() {
    let peer = Peer::new(|request| {
        if let Some(response) = common(request, 1) {
            return Some(response);
        }
        Some(
            json!({"ok":false,"error":{"code":"permission_denied","message":"https://private/?token=secret"}}),
        )
    });
    let result = active::read(
        BoundControl::connect(peer.options.clone()).await.unwrap(),
        "binding".into(),
        &Default::default(),
    )
    .await;
    let error = result.err().unwrap();
    assert_eq!(error.code, "permission_denied");
    assert!(!error.message.contains("secret"));
    assert!(error.details.is_none());
}
#[tokio::test]
async fn active_export_instance_revision_change_invalidates_preview_and_fixed_lease() {
    for during in [true, false] {
        let revision = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let current = revision.clone();
        let peer = Peer::new(move |request| {
            if let Some(response) = common(request, current.load(Ordering::Relaxed)) {
                return Some(response);
            }
            if during {
                current.store(2, Ordering::Relaxed);
            }
            ok(json!({"active_flows":[row("flow-a","tcp","proxy-a")]}))
        });
        let result = active::read(
            BoundControl::connect(peer.options.clone()).await.unwrap(),
            "binding".into(),
            &Default::default(),
        )
        .await;
        if during {
            assert_eq!(result.err().unwrap().code, "conflict");
        } else {
            let (lease, ..) = result.unwrap();
            revision.store(2, Ordering::Relaxed);
            assert_eq!(active::verify(&lease).await.unwrap_err().code, "conflict");
        }
    }
}
#[tokio::test]
async fn active_export_schema_missing_records_and_duplicate_ids_are_not_fabricated_defaults() {
    for rows in [
        json!([{"id":123,"bytes_up":1}]),
        json!([row("same", "tcp", "out"), row("same", "tcp", "out")]),
        json!({"items":[]}),
    ] {
        let peer = Peer::new(move |request| {
            if let Some(response) = common(request, 1) {
                return Some(response);
            }
            ok(json!({"active_flows":rows.clone()}))
        });
        assert!(active::read(
            BoundControl::connect(peer.options.clone()).await.unwrap(),
            "binding".into(),
            &Default::default()
        )
        .await
        .is_err());
    }
}
#[tokio::test]
async fn active_export_pinned_connection_cannot_reconnect_and_consume_old_identity() {
    let peer = Peer::new(|request| {
        if let Some(response) = common(request, 1) {
            return Some(response);
        }
        ok(json!({"active_flows":[row("flow-a","tcp","out")]}))
    });
    let (lease, ..) = active::read(
        BoundControl::connect(peer.options.clone()).await.unwrap(),
        "binding".into(),
        &Default::default(),
    )
    .await
    .unwrap();
    crate::kernel::connection::reset_endpoint(
        &crate::kernel::protocol::endpoint_from_options(Some(&peer.options)).unwrap(),
    );
    assert!(active::verify(&lease).await.is_err());
}
#[tokio::test]
async fn active_export_host_plan_consumes_real_snapshot_once_through_native_export_policy() {
    let peer = Peer::new(|request| {
        if let Some(response) = common(request, 1) {
            return Some(response);
        }
        ok(json!({"active_flows":[row("actual-fixed-flow","tcp","out")]}))
    });
    let state = AppState::default();
    {
        let mut app = state.app_config().lock().unwrap();
        app.core.socket = peer.options.socket.clone();
        app.ui.ui_mode = "pro".into();
    }
    let preview = preview_active(
        &state,
        active::Filter {
            outbound: Some("out".into()),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(preview.source, "active");
    assert_eq!(
        preview.core_instance_id.as_deref(),
        Some("actual-test-instance")
    );
    assert_eq!(preview.records, 1);
    let _prepared = prepare_delivery(&state, &preview.plan_id).await.unwrap();
    assert!(prepare_delivery(&state, &preview.plan_id).await.is_err());
    let mut plans = PLANS.lock().unwrap();
    assert!(plans.get(&preview.plan_id).unwrap().bytes.is_none());
    plans.remove(&preview.plan_id);
    assert_eq!(
        peer.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["request"].get("active_flows").is_some())
            .count(),
        1
    );
}
