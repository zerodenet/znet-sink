use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use znet_client_capabilities::network::{self, Route};
use znet_client_core::capability::*;

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    calls: Arc<AtomicUsize>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn new(body: &'static str, reply: Arc<AtomicBool>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let (done, seen) = (stop.clone(), calls.clone());
        let worker = std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        let mut received = Vec::new();
                        let mut buffer = [0; 8192];
                        let mut complete = false;
                        while let Ok(count) = stream.read(&mut buffer) {
                            if count == 0 {
                                break;
                            }
                            received.extend_from_slice(&buffer[..count]);
                            if let Some(end) = received.windows(4).position(|v| v == b"\r\n\r\n") {
                                let head =
                                    String::from_utf8_lossy(&received[..end]).to_ascii_lowercase();
                                let length = head
                                    .lines()
                                    .find_map(|line| {
                                        line.strip_prefix("content-length:")
                                            .and_then(|v| v.trim().parse::<usize>().ok())
                                    })
                                    .unwrap_or(0);
                                if received.len() >= end + 4 + length {
                                    complete = true;
                                    break;
                                }
                            }
                        }
                        if !complete {
                            continue;
                        }
                        seen.fetch_add(1, Ordering::SeqCst);
                        if reply.load(Ordering::SeqCst) {
                            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2))
                    }
                    Err(e) => panic!("{e}"),
                }
            }
        });
        Self {
            url,
            stop,
            calls,
            worker: Some(worker),
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
    }
}
fn lease() -> Lease {
    let manager = Manager::default();
    let grants = BTreeSet::from([
        Permission::new("network.get", "*"),
        Permission::new("network.request", "*"),
        Permission::new("network.download", "*"),
    ]);
    let policy = manager
        .admit(
            "network-route-test".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .unwrap();
    policy.authorize(grants, Duration::from_secs(60)).unwrap();
    policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 8192,
                timeout: Duration::from_secs(4),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
}
fn check_all(url: &str, expected: &[u8]) {
    println!("checking route {:?}", String::from_utf8_lossy(expected));
    let l = lease();
    assert_eq!(
        network::get(&l, url, "test", 1024, &[])
            .unwrap()
            .take(&l)
            .unwrap()
            .body,
        expected
    );
    let l = lease();
    assert_eq!(
        network::get_conditional(&l, url, "test", 1024, &[], &network::Validators::default())
            .unwrap()
            .take(&l)
            .unwrap()
            .body,
        expected
    );
    let l = lease();
    assert_eq!(
        network::get_with_diagnostics(&l, url, "test", 1024, &[], Duration::from_secs(3))
            .unwrap()
            .take(&l)
            .unwrap()
            .body,
        expected
    );
    let l = lease();
    assert_eq!(
        network::request(
            &l,
            &network::Request {
                method: "POST".into(),
                url: url.into(),
                headers: Default::default(),
                body: b"single-post".to_vec()
            },
            1024
        )
        .unwrap()
        .take(&l)
        .unwrap()
        .body,
        expected
    );
    let l = lease();
    let file = tempfile::NamedTempFile::new().unwrap();
    network::stream_get(
        &l,
        &network::StreamRequest {
            url: url.into(),
            user_agent: "test".into(),
            headers: Default::default(),
            max_bytes: 1024,
            max_redirects: 0,
            connect_timeout: Duration::from_secs(2),
            proxy: None,
            no_proxy: false,
            allowed_https_hosts: vec![],
        },
        |_| Ok(Some(Box::new(file.reopen().unwrap()))),
        |_| {},
    )
    .unwrap()
    .take(&l)
    .unwrap();
    assert_eq!(std::fs::read(file.path()).unwrap(), expected);
}

#[test]
fn default_requests_work_without_core_and_reselect_after_restart_without_replay() {
    if std::env::var_os("ZNET_ROUTE_CHILD").is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "default_requests_work_without_core_and_reselect_after_restart_without_replay",
                "--nocapture",
            ])
            .env("ZNET_ROUTE_CHILD", "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let origin = Server::new("origin", Arc::new(AtomicBool::new(true)));
    let reply = Arc::new(AtomicBool::new(true));
    let proxy = Server::new("proxy", reply.clone());
    // Child-process-only environment: no races with other HTTP tests.
    for key in [
        "HTTP_PROXY",
        "HTTPS_PROXY",
        "ALL_PROXY",
        "http_proxy",
        "https_proxy",
        "all_proxy",
    ] {
        std::env::set_var(key, &proxy.url);
    }
    for key in ["NO_PROXY", "no_proxy"] {
        std::env::set_var(key, "");
    }
    assert!(matches!(network::default_route(), Route::Direct));
    check_all(&origin.url, b"origin");
    assert_eq!(proxy.calls.load(Ordering::SeqCst), 0);
    let running = Arc::new(AtomicBool::new(false));
    let active = running.clone();
    network::install_default_route_provider(move || {
        if active.load(Ordering::SeqCst) {
            Route::System
        } else {
            Route::Direct
        }
    });
    running.store(true, Ordering::SeqCst);
    check_all(&origin.url, b"proxy");
    running.store(false, Ordering::SeqCst);
    // A stopped core's system proxy remains configured but no longer responds.
    reply.store(false, Ordering::SeqCst);
    check_all(&origin.url, b"origin");
    assert_eq!(origin.calls.load(Ordering::SeqCst), 10);
    assert_eq!(proxy.calls.load(Ordering::SeqCst), 5);
    // A failed POST must never be replayed through another route.
    running.store(true, Ordering::SeqCst);
    let l = lease();
    assert!(network::request(
        &l,
        &network::Request {
            method: "POST".into(),
            url: origin.url.clone(),
            headers: Default::default(),
            body: b"do-not-replay".to_vec()
        },
        1024
    )
    .is_err());
    assert_eq!(origin.calls.load(Ordering::SeqCst), 10);
    running.store(false, Ordering::SeqCst);
    let l = lease();
    assert!(network::stream_get(
        &l,
        &network::StreamRequest {
            url: origin.url.clone(),
            user_agent: "test".into(),
            headers: Default::default(),
            max_bytes: 1024,
            max_redirects: 0,
            connect_timeout: Duration::from_secs(2),
            proxy: Some(proxy.url.clone()),
            no_proxy: false,
            allowed_https_hosts: vec![],
        },
        |_| panic!("failed explicit proxy must not reach origin"),
        |_| {}
    )
    .is_err());
    assert_eq!(origin.calls.load(Ordering::SeqCst), 10);
}
