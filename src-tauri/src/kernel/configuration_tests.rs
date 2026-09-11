use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    thread,
};

#[tokio::test]
async fn bound_configuration_uses_the_real_envelope_and_never_replays_a_lost_submission() {
    for lose_reply in [false, true] {
        let directory = tempfile::tempdir_in("/tmp").unwrap();
        let path = directory.path().join("config.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(30)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut revision = 1;
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let request: Value = serde_json::from_str(&line).unwrap();
                let result = match request["type"].as_str().unwrap() {
                    "subscribe" => json!("subscribed"),
                    "query" if request["request"].get("runtime").is_some() => {
                        json!({"runtime":{"core_instance_id":"one","config_revision":revision}})
                    }
                    "query" => {
                        let query = &request["request"]["active_flows"];
                        assert!(query.get("limit").is_none());
                        json!({"active_flows":(0..700).map(|i|json!({"record":{"flow_id":format!("flow-{i}")}})).collect::<Vec<_>>()})
                    }
                    "command" => {
                        recorded
                            .lock()
                            .unwrap()
                            .push(request["method"].as_str().unwrap().to_string());
                        assert_eq!(request["method"], "config.apply");
                        assert!(request["params"]["config"].is_object());
                        revision += 1;
                        if lose_reply {
                            break;
                        }
                        json!({"accepted":true,"result":{"applied":true,"core_instance_id":"one","config_revision":revision}})
                    }
                    other => panic!("unexpected {other}"),
                };
                if writeln!(
                    writer,
                    "{}",
                    json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":result})
                )
                .is_err()
                {
                    break;
                }
            }
        });
        let options = CoreIpcOptions {
            socket: Some(path.to_string_lossy().into_owned()),
            timeout_ms: Some(2000),
        };
        let bound = BoundControl::connect(options.clone()).await.unwrap();
        assert_eq!(bound.all_flow_ids().await.unwrap().len(), 700);
        let outcome =
            crate::services::config_apply::apply(json!({"inbounds":[]}), options.clone()).await;
        if lose_reply {
            assert_eq!(outcome.unwrap_err().code, "config_apply_uncertain");
        } else {
            outcome.unwrap();
        }
        connection::reset_endpoint(&bound.binding.endpoint);
        assert!(bound.close_flow("old").await.is_err());
        worker.join().unwrap();
        assert_eq!(*calls.lock().unwrap(), vec!["config.apply"]);
    }
}

struct TestKernel(std::process::Child);
impl Drop for TestKernel {
    fn drop(&mut self) {
        self.0.stdin.take();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            if self.0.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(std::time::Duration::from_millis(25));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[tokio::test]
#[ignore = "set ZNET_OBSERVATION_CORE to test configuration apply against a private Zero child"]
async fn supplied_zero_confirms_config_application_without_restarting() {
    use std::process::{Command, Stdio};
    let binary =
        std::env::var_os("ZNET_OBSERVATION_CORE").expect("ZNET_OBSERVATION_CORE is required");
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let socket = directory.path().join("config.sock");
    let path = directory.path().join("config.json");
    let mut config = json!({"inbounds":[],"outbounds":[],"api":{"control":{"enabled":false}},"route":{"rules":[],"final":{"type":"reject"}}});
    std::fs::write(&path, config.to_string()).unwrap();
    let log = std::fs::File::create(directory.path().join("kernel.log")).unwrap();
    let mut kernel = TestKernel(
        Command::new(binary)
            .arg("run")
            .arg("--parent-lifetime-stdin")
            .arg("--control-socket")
            .arg(&socket)
            .arg(&path)
            .stdin(Stdio::piped())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !socket.exists() {
        assert!(
            kernel.0.try_wait().unwrap().is_none(),
            "kernel exited: {}",
            std::fs::read_to_string(directory.path().join("kernel.log")).unwrap()
        );
        assert!(std::time::Instant::now() < deadline, "startup timed out");
        thread::sleep(std::time::Duration::from_millis(25));
    }
    let options = CoreIpcOptions {
        socket: Some(socket.to_string_lossy().into_owned()),
        timeout_ms: Some(3000),
    };
    let control = BoundControl::connect(options.clone()).await.unwrap();
    assert!(crate::capture::shutdown::stop_owned_tun(
        kernel.0.id().wrapping_add(1),
        socket.to_string_lossy().into_owned()
    )
    .await
    .is_err());
    crate::capture::shutdown::stop_owned_tun(kernel.0.id(), socket.to_string_lossy().into_owned())
        .await
        .unwrap();
    let before = control.identity().await.unwrap();
    config["route"]["final"] = json!({"type":"direct"});
    crate::services::config_apply::apply(config, options.clone())
        .await
        .unwrap();
    let after = control.identity().await.unwrap();
    assert_eq!(before.core_instance_id, after.core_instance_id);
    assert!(after.config_revision > before.config_revision);
    assert!(
        crate::services::config_apply::apply(json!({"inbounds":"invalid"}), options)
            .await
            .is_err()
    );
    assert_eq!(control.identity().await.unwrap(), after);
    connection::reset_endpoint(&control.binding.endpoint);
    drop(kernel);
}
