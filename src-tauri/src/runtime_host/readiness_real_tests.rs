//! Opt-in contract tests against a released kernel; isolated loopback sockets only.
use super::*;
use std::net::TcpListener;
use std::process::{Command, Stdio};

struct OwnedChild(Child, Option<std::thread::JoinHandle<()>>);

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        if let Some(worker) = self.1.take() {
            worker.join().unwrap();
        }
    }
}

fn spawn(dir: &std::path::Path, port: u16) -> (OwnedChild, CoreEndpoint) {
    let binary = std::env::var("ZNET_TEST_ZERO_BINARY").expect("set ZNET_TEST_ZERO_BINARY");
    let config = dir.join("config.json");
    std::fs::write(&config, serde_json::to_vec(&json!({
        "inbounds": [{"tag":"test", "listen":{"address":"127.0.0.1", "port":port}, "protocol":{"type":"mixed"}}],
        "outbounds": [], "route": {"rules":[], "final":{"type":"direct"}}
    })).unwrap()).unwrap();
    #[cfg(unix)]
    let socket = dir.join("ipc.sock");
    #[cfg(windows)]
    let socket = std::path::PathBuf::from(format!(
        r"\\.\pipe\znet-readiness-{}-{}",
        std::process::id(),
        dir.file_name().unwrap().to_string_lossy()
    ));
    let stderr = std::fs::File::create(dir.join("kernel-stderr.log")).unwrap();
    let mut command = Command::new(binary);
    command
        .args(["run", "--parent-lifetime-stdin", "--control-socket"])
        .arg(&socket)
        .arg(&config)
        .env("ZERO_DNS_STATE_DIR", dir.join("dns-state"))
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let mut child = crate::runtime_host::child::spawn(&mut command).unwrap();
    let mut output = child.stderr.take().unwrap();
    let worker = std::thread::spawn(move || {
        let mut file = stderr;
        std::io::copy(&mut output, &mut file).unwrap();
    });
    (
        OwnedChild(child, Some(worker)),
        CoreEndpoint {
            transport: crate::kernel::transport::transport_name().into(),
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
    if let Err(error) = wait_for_ready(&mut child.0, &endpoint) {
        panic!(
            "readiness failed at {} for pid {}: {error:?}\nkernel stderr:\n{}",
            endpoint.path,
            child.0.id(),
            std::fs::read_to_string(dir.path().join("kernel-stderr.log")).unwrap_or_default()
        );
    }
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
