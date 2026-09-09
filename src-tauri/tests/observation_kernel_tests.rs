//! Opt-in contract check against a supplied Zero executable. Starts only a
//! private no-inbound child; no TUN, system proxy, or existing kernel is touched.
#![cfg(unix)]
use gui_lib::kernel::{connection, observation::FlowObservation};
use serde_json::json;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use znet_engine_client::{Binding, Endpoint};

struct OwnedKernel {
    child: Child,
    binding: Binding,
}
impl Drop for OwnedKernel {
    fn drop(&mut self) {
        connection::reset_endpoint(&self.binding.endpoint);
        self.child.stdin.take();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[tokio::test]
#[ignore = "set ZNET_OBSERVATION_CORE to a Zero executable; launches a private child"]
async fn supplied_zero_accepts_the_production_observation_contract() {
    let binary =
        std::env::var_os("ZNET_OBSERVATION_CORE").expect("ZNET_OBSERVATION_CORE is required");
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let config = directory.path().join("config.json");
    let socket = directory.path().join("control.sock");
    std::fs::write(&config,json!({"inbounds":[],"outbounds":[],"api":{"control":{"enabled":false}},"route":{"rules":[],"final":{"type":"reject"}}}).to_string()).unwrap();
    let log = std::fs::File::create(directory.path().join("kernel.log")).unwrap();
    let child = Command::new(binary)
        .arg("run")
        .arg("--parent-lifetime-stdin")
        .arg("--control-socket")
        .arg(&socket)
        .arg(&config)
        .stdin(Stdio::piped())
        .stdout(log.try_clone().unwrap())
        .stderr(log)
        .spawn()
        .unwrap();
    let mut kernel = OwnedKernel {
        child,
        binding: Binding {
            endpoint: Endpoint {
                transport: "unix-socket",
                path: socket.to_string_lossy().into_owned(),
            },
            timeout: Duration::from_secs(2),
        },
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while !socket.exists() {
        assert!(
            kernel.child.try_wait().unwrap().is_none(),
            "kernel exited: {}",
            std::fs::read_to_string(directory.path().join("kernel.log")).unwrap()
        );
        assert!(
            Instant::now() < deadline,
            "kernel startup deadline exceeded"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
    let observer = FlowObservation::connect(kernel.binding.clone())
        .await
        .unwrap();
    let baseline = observer.snapshot().await.unwrap();
    assert!(!baseline["runtime"]["core_instance_id"]
        .as_str()
        .unwrap()
        .is_empty());
    assert!(baseline["runtime"]["config_revision"].is_u64());
    assert!(baseline["connections"]["items"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(baseline["stats"].is_object());
    assert!(!baseline["policies"].is_null());
    assert!(observer.active(None).await.unwrap().items.is_empty());
    assert!(observer.recent(None).await.unwrap().items.is_empty());
    assert!(observer
        .detail("nonexistent-observation-test-flow")
        .await
        .is_err());
    drop(kernel);
    assert!(observer.active(None).await.is_err());
}
