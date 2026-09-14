use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::TcpListener,
    sync::{atomic::AtomicBool, mpsc, Arc},
    time::Duration,
};
use znet_client_capabilities::network;
use znet_client_core::capability::*;
#[test]
fn revocation_while_http_is_in_flight_discards_late_response() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut buffer = [0; 4096];
        assert!(stream.read(&mut buffer).unwrap() > 0);
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 6\r\nConnection: close\r\n\r\nsecret");
    });
    let manager = Manager::default();
    let grants = BTreeSet::from([Permission::new("network.get", &url)]);
    let policy = manager
        .admit("plugin".into(), grants.clone(), grants.clone(), &grants)
        .unwrap();
    policy.authorize(grants, Duration::from_secs(60)).unwrap();
    let lease = policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 1024,
                timeout: Duration::from_secs(5),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    let worker =
        std::thread::spawn(move || network::get(&lease, &url, "test", 1024, &[]).map(|_| ()));
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    policy.revoke();
    release_tx.send(()).unwrap();
    assert_eq!(worker.join().unwrap(), Err(Error::Revoked));
    server.join().unwrap();
    assert_eq!(
        manager.operations()[0].state,
        OperationState::DeliveryDenied(Error::Revoked)
    );
}
#[test]
fn non_web_urls_and_implicit_credentials_are_not_network_targets() {
    for url in [
        "file:///etc/passwd",
        "ftp://example.com",
        "http://user:secret@example.com",
        "http://example.com/".repeat(1000).as_str(),
    ] {
        assert_eq!(network::origin(url), Err(Error::InvalidRequest));
    }
}
