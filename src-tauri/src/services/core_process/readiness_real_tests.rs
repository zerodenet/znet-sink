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

fn spawn(dir: &std::path::Path, port: u16) -> (OwnedChild, CoreEndpoint) {
    let binary = std::env::var("ZNET_TEST_ZERO_BINARY").expect("set ZNET_TEST_ZERO_BINARY");
    let config = dir.join("config.json");
    std::fs::write(&config, serde_json::to_vec(&json!({
        "inbounds": [{"tag":"test", "listen":{"address":"127.0.0.1", "port":port}, "protocol":{"type":"mixed"}}],
        "outbounds": [], "route": {"rules":[], "final":{"type":"direct"}}
    })).unwrap()).unwrap();
    let socket = dir.join("ipc.sock");
    let child = Command::new(binary)
        .args(["run", "--parent-lifetime-stdin", "--control-socket"])
        .arg(&socket)
        .arg(&config)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    (
        OwnedChild(child),
        CoreEndpoint {
            transport: "unix-socket".into(),
            path: socket.to_string_lossy().into_owned(),
        },
    )
}

#[test]
#[ignore = "requires ZNET_TEST_ZERO_BINARY pointing to a released kernel"]
fn real_kernel_ready_and_lifetime_pipe_shutdown_release_listener() {
    let dir = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let (mut child, endpoint) = spawn(dir.path(), port);
    wait_for_ready(&mut child.0, &endpoint).unwrap();
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
    let (mut child, endpoint) = spawn(dir.path(), occupied.local_addr().unwrap().port());
    assert!(wait_for_ready(&mut child.0, &endpoint).is_err());
}
