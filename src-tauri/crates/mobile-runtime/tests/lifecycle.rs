use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};
use znet_mobile_runtime::{
    AuthContext, CommandRequest, Error, Identity, Permission, Phase, QueryRequest, QueryResponse,
    RuntimeConfig, RuntimeOwner,
};

struct Echo {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Echo {
    fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let background = stop.clone();
        let thread = thread::spawn(move || {
            while !background.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((mut socket, _)) => {
                        thread::spawn(move || {
                            socket
                                .set_read_timeout(Some(Duration::from_secs(2)))
                                .unwrap();
                            let mut bytes = [0; 8192];
                            while let Ok(count) = socket.read(&mut bytes) {
                                if count == 0 || socket.write_all(&bytes[..count]).is_err() {
                                    break;
                                }
                            }
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(_) => break,
                }
            }
        });
        Self {
            port,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Echo {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
    }
}
fn port() -> u16 {
    TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
fn config(port: u16, target: u16) -> RuntimeConfig {
    RuntimeConfig::parse(&serde_json::json!({"inbounds":[{"tag":"loopback-in","listen":{"address":"127.0.0.1","port":port},"udp":{"enabled":false},"protocol":{"type":"direct","target":"127.0.0.1","port":target}}],"route":{"rules":[],"final":{"type":"direct"}}}).to_string()).unwrap()
}
fn connect(port: u16) -> TcpStream {
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(socket) = TcpStream::connect(("127.0.0.1", port)) {
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            return socket;
        }
        assert!(Instant::now() < until, "real proxy listener did not bind");
        thread::sleep(Duration::from_millis(5));
    }
}
fn auth(permissions: Vec<Permission>) -> AuthContext {
    AuthContext {
        subject: Some("native-host-test".into()),
        permissions,
    }
}
fn query(value: serde_json::Value) -> QueryRequest {
    serde_json::from_value(value).unwrap()
}
fn command(value: serde_json::Value) -> CommandRequest {
    serde_json::from_value(value).unwrap()
}

#[test]
fn real_forwarding_survives_ui_executor_and_handle_drop_then_shutdown_releases_listener() {
    let echo = Echo::start();
    let proxy_port = port();
    let ui = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let mut owner = ui
        .block_on(RuntimeOwner::start(config(proxy_port, echo.port)))
        .unwrap();
    let ui_handle = owner.control();
    let identity = owner.status().identity.unwrap();
    assert_eq!(owner.status().phase, Phase::Spawned);
    drop(ui_handle);
    drop(ui); // The actual Proxy executor is independently owned.
    let mut socket = connect(proxy_port);
    let bytes = vec![0x52; 32 * 1024];
    socket.write_all(&bytes).unwrap();
    let mut received = vec![0; bytes.len()];
    socket.read_exact(&mut received).unwrap();
    assert_eq!(received, bytes);
    drop(socket);
    let ui = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let control = owner.control();
    ui.block_on(async {
        let read = auth(vec![Permission::Read]);
        let snapshot = control
            .query(&identity, &read, query(serde_json::json!({"stats":{}})))
            .await
            .unwrap();
        let QueryResponse::Stats(stats) = snapshot else {
            panic!("wrong stats reply")
        };
        assert!(stats.total_started > 0);
        assert!(stats.bytes_up > 0);
        let stopped = tokio::time::timeout(Duration::from_secs(10), owner.shutdown())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stopped.phase, Phase::Stopped);
        assert_eq!(
            control
                .query(&identity, &read, query(serde_json::json!({"runtime":{}})))
                .await
                .unwrap_err(),
            Error::NotSubmitted
        );
    });
    let rebound = TcpListener::bind(("127.0.0.1", proxy_port)).unwrap();
    drop(rebound);
}

#[tokio::test]
async fn actual_query_and_acknowledged_command_enforce_permissions_and_exact_identity() {
    let mut owner = RuntimeOwner::start(
        RuntimeConfig::parse(r#"{"route":{"rules":[],"final":{"type":"direct"}}}"#).unwrap(),
    )
    .await
    .unwrap();
    let handle = owner.control();
    let identity = owner.status().identity.unwrap();
    let read = auth(vec![Permission::Read]);
    let mode = command(serde_json::json!({"method":"mode.set","params":{"mode":"direct"}}));
    assert_eq!(
        handle
            .command(&identity, &read, mode.clone())
            .await
            .unwrap_err(),
        Error::Forbidden
    );
    let wrong = Identity {
        core_instance_id: "replaced-instance".into(),
        ..identity.clone()
    };
    assert_eq!(
        handle
            .query(&wrong, &read, query(serde_json::json!({"runtime":{}})))
            .await
            .unwrap_err(),
        Error::StaleIdentity
    );
    let receipt = handle
        .command(&identity, &auth(vec![Permission::Admin]), mode)
        .await
        .unwrap();
    assert!(receipt.accepted);
    let current = handle.status().identity.unwrap();
    let QueryResponse::Runtime(snapshot) = handle
        .query(&current, &read, query(serde_json::json!({"runtime":{}})))
        .await
        .unwrap()
    else {
        panic!("wrong runtime reply")
    };
    let QueryResponse::Config(config) = handle
        .query(
            &current,
            &auth(vec![Permission::Admin]),
            query(serde_json::json!({"config":{}})),
        )
        .await
        .unwrap()
    else {
        panic!("wrong config reply")
    };
    assert_eq!(config.mode.kind, "direct");
    assert_eq!(snapshot.core_instance_id, current.core_instance_id);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn every_native_tun_dispatch_is_denied_without_starting_desktop_tun() {
    let tun_config = serde_json::json!({"route":{"rules":[],"final":{"type":"direct"}},"runtime":{"tun":{"name":"should-not-exist","addr":"10.66.0.1/24","tag":"tun-in","auto_route":true,"dns_hijack":false}}});
    assert!(matches!(
        RuntimeOwner::start(RuntimeConfig::parse(&tun_config.to_string()).unwrap()).await,
        Err(Error::NativeTunAdapterUnavailable)
    ));
    let mut owner = RuntimeOwner::start(
        RuntimeConfig::parse(r#"{"route":{"rules":[],"final":{"type":"direct"}}}"#).unwrap(),
    )
    .await
    .unwrap();
    let handle = owner.control();
    let identity = owner.status().identity.unwrap();
    let admin = auth(vec![Permission::Admin]);
    for request in [
        serde_json::json!({"method":"tun.start","params":{}}),
        serde_json::json!({"method":"tun.stop","params":{}}),
        serde_json::json!({"method":"tun.recover","params":{}}),
        serde_json::json!({"method":"config.apply","params":{"config":tun_config}}),
        serde_json::json!({"method":"config.apply_runtime","params":{"config":tun_config}}),
    ] {
        assert_eq!(
            handle
                .command(&identity, &admin, command(request))
                .await
                .unwrap_err(),
            Error::NativeTunAdapterUnavailable
        );
    }
    let QueryResponse::TunStatus(snapshot) = handle
        .query(
            &identity,
            &admin,
            query(serde_json::json!({"tun_status":{}})),
        )
        .await
        .unwrap()
    else {
        panic!("wrong TUN reply")
    };
    assert!(!snapshot.running);
    owner.shutdown().await.unwrap();
}

#[tokio::test]
async fn asynchronous_listener_start_failure_cannot_be_reported_as_confirmed_shutdown() {
    let echo = Echo::start();
    let reserved = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let reserved_port = reserved.local_addr().unwrap().port();
    let mut owner = RuntimeOwner::start(config(reserved_port, echo.port))
        .await
        .unwrap();
    assert_eq!(owner.status().phase, Phase::Spawned); // spawn is explicitly not ready.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(10), owner.shutdown())
            .await
            .unwrap()
            .unwrap_err(),
        Error::CoreFailure
    );
    assert_eq!(owner.status().phase, Phase::Failed);
}

#[tokio::test]
async fn canceled_start_or_dropped_native_owner_still_requests_real_cleanup() {
    let echo = Echo::start();
    let proxy_port = port();
    let mut start = Box::pin(RuntimeOwner::start(config(proxy_port, echo.port)));
    std::future::poll_fn(|context| {
        use std::future::Future;
        if let std::task::Poll::Ready(result) = start.as_mut().poll(context) {
            drop(result.unwrap());
        }
        std::task::Poll::Ready(())
    })
    .await;
    drop(start);
    // Request cancellation never turns into a permanent detached listener.
    tokio::time::sleep(Duration::from_millis(200)).await;
    let reserved = TcpListener::bind(("127.0.0.1", proxy_port)).unwrap();
    drop(reserved);
    let owner = RuntimeOwner::start(config(proxy_port, echo.port))
        .await
        .unwrap();
    let handle = owner.control();
    let _ = connect(proxy_port);
    drop(owner);
    tokio::time::timeout(Duration::from_secs(10), async {
        while handle.status().phase != Phase::Stopped {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert!(TcpListener::bind(("127.0.0.1", proxy_port)).is_ok());
}
