use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::TcpListener,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_capabilities::network::{get_with_diagnostics, TransportFailure};
use znet_client_core::capability::{Budget, Error, Manager, Permission, Policy};

fn policy() -> Policy {
    let grants = BTreeSet::from([Permission::new("network.get", "*")]);
    let policy = Manager::default()
        .admit("probe-test".into(), grants.clone(), grants.clone(), &grants)
        .unwrap();
    policy.authorize(grants, Duration::from_secs(60)).unwrap();
    policy
}

#[test]
fn connection_failure_has_safe_category_without_url_or_secret() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/private?token=secret",
        listener.local_addr().unwrap()
    );
    drop(listener);
    let lease = policy()
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 4096,
                timeout: Duration::from_secs(5),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    let error = get_with_diagnostics(&lease, &url, "test", 1024, &[], Duration::from_secs(1))
        .err()
        .unwrap();
    assert_eq!(error.transport, Some(TransportFailure::Connect));
    assert_eq!(error.error, Error::Transport);
    for text in [error.to_string(), format!("{error:?}")] {
        assert!(
            !text.contains("token") && !text.contains("private") && !text.contains("127.0.0.1")
        );
    }
}

#[test]
fn timed_out_first_probe_leaves_budget_for_successful_fallback() {
    let slow = TcpListener::bind("127.0.0.1:0").unwrap();
    let fast = TcpListener::bind("127.0.0.1:0").unwrap();
    let slow_url = format!("http://{}", slow.local_addr().unwrap());
    let fast_url = format!("http://{}", fast.local_addr().unwrap());
    let slow_server = std::thread::spawn(move || {
        // The request deadline includes client construction: on a busy host
        // it can expire before TCP connects. Never wait forever for that probe.
        slow.set_nonblocking(true).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        let mut stream = loop {
            match slow.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(error) => panic!("slow probe accept failed: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let _ = stream.read(&mut [0; 1024]);
        std::thread::sleep(Duration::from_millis(250));
    });
    let fast_server = std::thread::spawn(move || {
        let (mut stream, _) = fast.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let _ = stream.read(&mut [0; 1024]);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
    });
    let lease = policy()
        .begin(
            Budget {
                calls: 2,
                resource_bytes: 4096,
                timeout: Duration::from_secs(2),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    let failure = get_with_diagnostics(
        &lease,
        &slow_url,
        "test",
        1024,
        &[],
        Duration::from_millis(100),
    )
    .err()
    .unwrap();
    assert_eq!(failure.transport, Some(TransportFailure::Timeout));
    let result = get_with_diagnostics(&lease, &fast_url, "test", 1024, &[], Duration::from_secs(1))
        .unwrap()
        .take(&lease)
        .unwrap();
    assert_eq!(result.body, b"{}");
    slow_server.join().unwrap();
    fast_server.join().unwrap();
}

#[test]
fn truncated_response_is_classified_as_body_failure() {
    let server = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", server.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = server.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let _ = stream.read(&mut [0; 1024]);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\nshort")
            .unwrap();
    });
    let lease = policy()
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 4096,
                timeout: Duration::from_secs(2),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    let error = get_with_diagnostics(&lease, &url, "test", 1024, &[], Duration::from_secs(1))
        .err()
        .unwrap();
    assert_eq!(error.transport, Some(TransportFailure::ResponseBody));
    worker.join().unwrap();
}

#[test]
fn diagnosed_requests_still_enforce_revoked_permissions() {
    let policy = policy();
    let lease = policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 4096,
                timeout: Duration::from_secs(2),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    policy.revoke();
    let error = get_with_diagnostics(
        &lease,
        "http://127.0.0.1:1",
        "test",
        1024,
        &[],
        Duration::from_secs(1),
    )
    .err()
    .unwrap();
    assert_eq!(error.error, Error::Revoked);
    assert_eq!(error.transport, None);
}

#[test]
fn revocation_takes_priority_over_an_in_flight_transport_failure() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let _ = stream.read(&mut [0; 1024]);
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        // Closing without a response makes the network executor fail too.
    });
    let policy = policy();
    let lease = policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 4096,
                timeout: Duration::from_secs(3),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    let worker = std::thread::spawn(move || {
        get_with_diagnostics(&lease, &url, "test", 1024, &[], Duration::from_secs(2))
            .err()
            .unwrap()
    });
    entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
    policy.revoke();
    release_tx.send(()).unwrap();
    let error = worker.join().unwrap();
    server.join().unwrap();
    assert_eq!(error.error, Error::Revoked);
    assert_eq!(error.transport, None);
}
