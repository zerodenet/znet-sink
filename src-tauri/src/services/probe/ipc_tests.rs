use super::*;
use crate::models::app_config::AppConfig;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;

#[tokio::test]
async fn manual_probe_sends_the_client_url_on_the_real_ipc_boundary() {
    let directory = tempfile::tempdir_in("/tmp").unwrap();
    let path = directory.path().join("probe.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let worker = std::thread::spawn(move || {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        let stream = loop {
            if let Ok((stream, _)) = listener.accept() {
                break stream;
            }
            assert!(Instant::now() < deadline, "client never reached probe peer");
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(10)))
            .unwrap();
        let mut writer = stream.try_clone().unwrap();
        for line in BufReader::new(stream).lines() {
            let request: Value = serde_json::from_str(&line.unwrap()).unwrap();
            let command = request["type"] == "command";
            let result = if command {
                assert_eq!(request["method"], "diagnostics.probe_outbound");
                assert_eq!(request["params"]["url"], "https://client.example/204");
                assert_eq!(request["params"]["target_tag"], "node-a");
                json!({"reachable":true,"latency_ms":12})
            } else {
                assert_eq!(request["type"], "subscribe");
                json!("subscribed")
            };
            writeln!(
                writer,
                "{}",
                json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":result})
            )
            .unwrap();
            if command {
                break;
            }
        }
    });
    let mut config = AppConfig::default();
    config.core.socket = Some(path.to_string_lossy().into_owned());
    config.url_test.url = " https://client.example/204 ".into();
    let state = AppState::new(config);
    let result = probe_single(&state, ProbeJobId(1), "node-a").await;
    worker.join().unwrap();
    assert!(result.reachable, "{:?}", result.message);
}
