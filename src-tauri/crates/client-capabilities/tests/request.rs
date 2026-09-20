use std::{
    collections::{BTreeMap, BTreeSet},
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_capabilities::network::{self, Request};
use znet_client_core::capability::*;
fn lease(manager: &Manager, url: &str, capability: &str) -> Lease {
    let grants = BTreeSet::from([Permission::new(capability, network::origin(url).unwrap())]);
    let policy = manager
        .admit("plugin".into(), grants.clone(), grants.clone(), &grants)
        .unwrap();
    policy.authorize(grants, Duration::from_secs(10)).unwrap();
    policy
        .begin(
            Budget {
                calls: 4,
                resource_bytes: 4096,
                timeout: Duration::from_secs(5),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
}

fn scoped_lease(manager: &Manager, capability: &str, scope: &str) -> Lease {
    let grants = BTreeSet::from([Permission::new(capability, scope)]);
    let policy = manager
        .admit(
            "plugin-configured".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .unwrap();
    policy.authorize(grants, Duration::from_secs(10)).unwrap();
    policy
        .begin(
            Budget {
                calls: 2,
                resource_bytes: 4096,
                timeout: Duration::from_secs(5),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
}
#[test]
fn post_delivers_binary_body_and_never_forwards_credentials_on_redirect() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let other = TcpListener::bind("127.0.0.1:0").unwrap();
    other.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let redirect = format!("http://{}", other.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut head = String::new();
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            head.push_str(&line);
            if line == "\r\n" {
                break;
            }
        }
        assert!(head.starts_with("POST / HTTP/1.1"));
        assert!(head
            .to_lowercase()
            .contains("authorization: bearer secret-canary"));
        let mut body = [0; 4];
        reader.read_exact(&mut body).unwrap();
        assert_eq!(body, [0, 255, 1, 2]);
        write!(stream, "HTTP/1.1 302 Found\r\nLocation: {redirect}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
    });
    let manager = Manager::default();
    let lease = lease(&manager, &url, "network.request");
    let response = network::request(
        &lease,
        &Request {
            url,
            method: "POST".into(),
            headers: BTreeMap::from([("authorization".into(), "Bearer secret-canary".into())]),
            body: vec![0, 255, 1, 2],
        },
        1024,
    )
    .unwrap()
    .take(&lease)
    .unwrap();
    assert_eq!(response.status, 302);
    assert!(response.headers.contains_key("location"));
    worker.join().unwrap();
    assert!(matches!(other.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    assert!(!format!("{:?}", manager.operations()).contains("secret-canary"));
}
#[test]
fn get_permission_cannot_be_escalated_to_post_and_transport_headers_are_rejected() {
    let manager = Manager::default();
    let url = "http://127.0.0.1:1";
    let request = Request {
        url: url.into(),
        method: "POST".into(),
        headers: BTreeMap::new(),
        body: vec![1],
    };
    assert!(matches!(
        network::request(&lease(&manager, url, "network.get"), &request, 100),
        Err(Error::PermissionDenied)
    ));
    let mut request = request;
    request
        .headers
        .insert("Host".into(), "another.internal".into());
    assert!(matches!(
        network::request(&lease(&manager, url, "network.request"), &request, 100),
        Err(Error::PermissionDenied)
    ));
}

#[test]
fn configured_request_is_bound_to_declared_field_and_exact_https_origin() {
    let manager = Manager::default();
    let request = Request {
        url: "https://other.example.com/connect".into(),
        method: "POST".into(),
        headers: BTreeMap::new(),
        body: b"{}".to_vec(),
    };
    assert!(matches!(
        network::configured_request(
            &scoped_lease(&manager, "network.configured.request", "provider_origin"),
            &request,
            "provider_origin",
            "https://panel.example.com",
            network::Route::Direct,
            1024,
        ),
        Err(Error::PermissionDenied)
    ));
    assert!(matches!(
        network::configured_request(
            &scoped_lease(&manager, "network.configured.request", "another_field"),
            &Request {
                url: "https://panel.example.com/connect".into(),
                ..request
            },
            "provider_origin",
            "https://panel.example.com",
            network::Route::Direct,
            1024,
        ),
        Err(Error::PermissionDenied)
    ));
}
