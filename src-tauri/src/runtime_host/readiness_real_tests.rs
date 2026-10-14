//! Opt-in contract tests against a released kernel; isolated loopback sockets only.
use super::*;
use std::net::TcpListener;
use std::process::{Command, Stdio};

struct OwnedChild(Child);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn listener_config(port: u16) -> serde_json::Value {
    json!({
        "inbounds": [{"tag":"test", "listen":{"address":"127.0.0.1", "port":port}, "protocol":{"type":"mixed"}}],
        "outbounds": [], "route": {"rules":[], "final":{"type":"direct"}}
    })
}

fn spawn(dir: &std::path::Path) -> (OwnedChild, CoreEndpoint) {
    let binary = std::env::var("ZNET_TEST_ZERO_BINARY").expect("set ZNET_TEST_ZERO_BINARY");
    let config = dir.join("config.json");
    // Match the managed host: IPC-only bootstrap, then acknowledged runtime
    // application. Engine health is not a listener-orchestration receipt.
    std::fs::write(
        &config,
        serde_json::to_vec(&json!({
            "inbounds": [],
            "outbounds": [], "route": {"rules":[], "final":{"type":"direct"}}
        }))
        .unwrap(),
    )
    .unwrap();
    #[cfg(unix)]
    let socket = dir.join("ipc.sock");
    #[cfg(windows)]
    let socket = std::path::PathBuf::from(format!(
        r"\\.\pipe\znet-readiness-{}-{}",
        std::process::id(),
        dir.file_name().unwrap().to_string_lossy()
    ));
    let stderr = std::fs::File::create(dir.join("kernel-stderr.log")).unwrap();
    let child = Command::new(binary)
        .args(["run", "--parent-lifetime-stdin", "--control-socket"])
        .arg(&socket)
        .arg(&config)
        .env("ZERO_DNS_STATE_DIR", dir.join("dns-state"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr))
        .spawn()
        .unwrap();
    (
        OwnedChild(child),
        CoreEndpoint {
            transport: crate::kernel::transport::transport_name().into(),
            path: socket.to_string_lossy().into_owned(),
        },
    )
}

fn apply_listener(endpoint: &CoreEndpoint, port: u16) -> AppResult<()> {
    tauri::async_runtime::block_on(async {
        let control = crate::kernel::configuration::BoundControl::connect(
            crate::models::core::CoreIpcOptions {
                socket: Some(endpoint.path.clone()),
                timeout_ms: Some(5000),
            },
        )
        .await?;
        control.apply(listener_config(port)).await?;
        Ok(())
    })
}

#[test]
#[ignore = "requires ZNET_TEST_ZERO_BINARY pointing to a released kernel"]
fn real_kernel_ready_and_lifetime_pipe_shutdown_release_listener() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let (mut child, endpoint) = spawn(dir.path());
    if let Err(error) = wait_for_ready(&mut child.0, &endpoint) {
        panic!(
            "readiness failed at {} for pid {}: {error:?}\nkernel stderr:\n{}",
            endpoint.path,
            child.0.id(),
            std::fs::read_to_string(dir.path().join("kernel-stderr.log")).unwrap_or_default()
        );
    }
    apply_listener(&endpoint, port).expect("actual runtime-only listener application must confirm");
    crate::services::local_proxy::wait_until_listening("127.0.0.1", port).unwrap();
    drop(child.0.stdin.take());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        assert!(
            Instant::now() < deadline,
            "kernel survived parent lifetime EOF"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(TcpListener::bind(("127.0.0.1", port)).is_ok());
}

#[test]
#[ignore = "requires ZNET_TEST_ZERO_BINARY pointing to a released kernel"]
fn real_kernel_bind_failure_is_never_reported_ready() {
    let dir = tempfile::tempdir().unwrap();
    let occupied = TcpListener::bind("127.0.0.1:0").unwrap();
    let (mut child, endpoint) = spawn(dir.path());
    wait_for_ready(&mut child.0, &endpoint).expect("IPC-only bootstrap must become controllable");
    let result = apply_listener(&endpoint, occupied.local_addr().unwrap().port());
    assert!(
        result.is_err(),
        "occupied listener must not pass acknowledged runtime application; kernel stderr:\n{}",
        std::fs::read_to_string(dir.path().join("kernel-stderr.log")).unwrap_or_default()
    );
}
