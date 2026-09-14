#![cfg(feature = "network-lab")]
use std::{
    collections::BTreeSet,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_capabilities::network;
use znet_client_core::capability::{Manager, OperationState};
use znet_plugin_sandbox::{
    contract::*,
    policy::Authority,
    runtime::{execute, execute_network_lab},
};

fn component(url: &str, source: Option<&str>) -> (Component, BTreeSet<Request>) {
    let origin = network::origin(url).unwrap();
    let source = source.map(str::to_owned).unwrap_or_else(|| {
        format!(
            "JSON.parse(hostCall(JSON.stringify({{capability:'network.get',scope:{},url:{}}})))",
            serde_json::to_string(&origin).unwrap(),
            serde_json::to_string(url).unwrap()
        )
    });
    let manifest = serde_json::json!({"schema_version":1,"host":"znet-sink","plugin_id":"org.example.network","component_id":"get","version":"1.0.0","requires_host":"=0.0.1","api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[{"capability":"network.get","scope":origin}],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits": Limits::default()});
    let c = Component::load(&serde_json::to_vec(&manifest).unwrap(), &source).unwrap();
    let grants = c.manifest().required.iter().cloned().collect();
    (c, grants)
}
fn authorize(manager: &Manager, c: &Component, grants: &BTreeSet<Request>) -> Authority {
    let a = Authority::admit_with_manager(c, grants, manager).unwrap();
    a.authorize(grants.clone(), Duration::from_secs(60))
        .unwrap();
    a
}
fn server(count: usize, response: String) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let mut requests = vec![];
        for _ in 0..count {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                request.push_str(&line);
            }
            requests.push(request);
            stream.write_all(response.as_bytes()).unwrap();
        }
        requests
    });
    (url, worker)
}
#[test]
fn builtin_subscription_and_real_vm_use_same_manager_and_http_executor() {
    let (url, worker) = server(2, "HTTP/1.1 200 OK\r\nContent-Length: 6\r\nsubscription-userinfo: upload=4; download=5\r\nConnection: close\r\n\r\ncanary".into());
    let manager = Manager::default();
    let builtin = znet_client_capabilities::subscription::begin(&manager).unwrap();
    let response = network::get(
        &builtin,
        &url,
        "Clash.Meta",
        1024,
        &["subscription-userinfo"],
    )
    .unwrap()
    .take(&builtin)
    .unwrap();
    assert_eq!(response.body, b"canary");
    assert_eq!(
        response
            .headers
            .get("subscription-userinfo")
            .map(String::as_str),
        Some("upload=4; download=5")
    );
    let (c, grants) = component(&url, None);
    let auth = authorize(&manager, &c, &grants);
    let result = execute_network_lab(&c, &auth, Arc::new(AtomicBool::new(false))).unwrap();
    assert_eq!(result["body"], "canary");
    let records = manager.operations();
    assert_eq!(records.len(), 2);
    assert!(records
        .iter()
        .all(|o| o.capability == "network.get" && o.state == OperationState::Completed));
    assert_ne!(records[0].parent, records[1].parent);
    let requests = worker.join().unwrap();
    assert!(requests[0]
        .to_lowercase()
        .contains("user-agent: clash.meta"));
    assert!(requests[1]
        .to_lowercase()
        .contains("user-agent: znet-sink-plugin/1"));
}
#[test]
fn redirect_scope_is_checked_before_contacting_another_origin() {
    let forbidden = TcpListener::bind("127.0.0.1:0").unwrap();
    forbidden.set_nonblocking(true).unwrap();
    let (url, worker) = server(1, format!("HTTP/1.1 302 Found\r\nLocation: http://{}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n", forbidden.local_addr().unwrap()));
    let manager = Manager::default();
    let (c, grants) = component(&url, None);
    let auth = authorize(&manager, &c, &grants);
    assert_eq!(
        execute_network_lab(&c, &auth, Arc::new(AtomicBool::new(false))),
        Err(Error::PermissionDenied)
    );
    worker.join().unwrap();
    assert_eq!(
        forbidden.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn default_vm_cannot_enable_network_and_guest_cannot_claim_builtin_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let manager = Manager::default();
    let (c, grants) = component(&url, None);
    let auth = authorize(&manager, &c, &grants);
    assert_eq!(
        execute(&c, &auth, None, Arc::new(AtomicBool::new(false))),
        Err(Error::PermissionDenied)
    );
    let source = format!("hostCall(JSON.stringify({{capability:'network.get',scope:{},url:{},identity:'builtin.subscription'}}))", serde_json::to_string(&url).unwrap(), serde_json::to_string(&url).unwrap());
    let (c, grants) = component(&url, Some(&source));
    let auth = authorize(&manager, &c, &grants);
    assert_eq!(
        execute_network_lab(&c, &auth, Arc::new(AtomicBool::new(false))),
        Err(Error::PermissionDenied)
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn unbounded_response_is_rejected_even_without_content_length() {
    let (url, worker) = server(
        1,
        format!(
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{}",
            "x".repeat(1025)
        ),
    );
    let manager = Manager::default();
    let builtin = znet_client_capabilities::subscription::begin(&manager).unwrap();
    assert!(matches!(
        network::get(&builtin, &url, "test", 1024, &[]),
        Err(znet_client_core::capability::Error::BudgetExceeded)
    ));
    worker.join().unwrap();
}
