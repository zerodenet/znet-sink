use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    net::TcpListener,
};
#[test]
fn subscription_entry_uses_client_management_and_preserves_provider_metadata() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!(
        "http://{}/subscription?token=canary",
        listener.local_addr().unwrap()
    );
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nsubscription-userinfo: upload=4; download=5; total=99; expire=100\r\nConnection: close\r\n\r\n{}").unwrap();
    });
    let state = AppState::default();
    let result =
        fetch_subscription_content_blocking(state.capabilities(), &url, "Clash.Meta").unwrap();
    assert_eq!(result.content, "{}");
    assert_eq!(result.userinfo.upload, Some(4));
    assert_eq!(result.userinfo.download, Some(5));
    assert_eq!(result.userinfo.expire_ms(), Some(100_000));
    let records = state.capabilities().operations();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].capability, "network.get");
    assert_eq!(
        records[0].state,
        znet_client_core::capability::OperationState::Completed
    );
    assert!(!format!("{records:?}").contains("canary"));
    worker.join().unwrap();
}

#[test]
fn foreground_due_guard_invalidates_manual_success_and_same_millisecond_source_edits() {
    let expected: SubscriptionProfile=serde_json::from_value(json!({"id":"due","name":"Source","url":"https://example.invalid/sub?token=canary","enabled":true,
        "kernel":"zero","format":"zero","userAgent":"custom","targetProxyConfigId":null,"updateIntervalSecs":60,
        "updatedAtUnixMs":1,"lastSyncAtUnixMs":1,"lastError":null})).unwrap();
    assert!(matches_due_snapshot(&expected, &expected, 100_000));
    let mut current = expected.clone();
    current.last_sync_at_unix_ms = Some(100_001);
    assert!(!matches_due_snapshot(&current, &expected, 200_000));
    current = expected.clone();
    current.user_agent = Some("changed".into());
    assert!(!matches_due_snapshot(&current, &expected, 100_000));
    current = expected.clone();
    current.url = "https://example.invalid/new".into();
    assert!(!matches_due_snapshot(&current, &expected, 100_000));
}
#[test]
fn subscription_unexpected_304_is_rejected_and_existing_material_is_untouched() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/sub?token=canary", listener.local_addr().unwrap());
    let worker = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            if line == "\r\n" {
                break;
            }
        }
        stream
            .write_all(
                b"HTTP/1.1 304 Not Modified\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
    });
    let state = AppState::default();
    state
        .proxy_configs()
        .lock()
        .unwrap()
        .push(ProxyConfigProfile {
            id: "valid-material".into(),
            name: "Previous valid material".into(),
            kernel: "zero".into(),
            format: "zero-json".into(),
            path: None,
            content: Some(json!({"outbounds":[{"tag":"direct","type":"direct"}]})),
            active: false,
            managed_source: None,
            updated_at_unix_ms: 1,
            capabilities: Default::default(),
        });
    let previous = serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap();
    let error =
        fetch_subscription_content_blocking(state.capabilities(), &url, "custom").unwrap_err();
    assert_eq!(error.code, "upstream_error");
    assert!(error.message.contains("304"));
    assert!(!error.message.contains("canary"));
    assert_eq!(
        serde_json::to_value(state.proxy_configs().lock().unwrap().clone()).unwrap(),
        previous
    );
    assert!(!format!("{:?}", state.capabilities().operations()).contains("canary"));
    worker.join().unwrap();
}

#[test]
fn foreground_and_manual_sync_share_one_real_per_subscription_claim() {
    let state = AppState::default();
    let manual = begin_in_flight(state.subscription_syncs(), "subscription", "source-1").unwrap();
    assert!(is_in_flight(state.subscription_syncs(), "source-1").unwrap());
    let duplicate =
        begin_in_flight(state.subscription_syncs(), "subscription", "source-1").unwrap_err();
    assert_eq!(duplicate.code, "conflict");
    let independent =
        begin_in_flight(state.subscription_syncs(), "subscription", "source-2").unwrap();
    drop(independent);
    drop(manual);
    assert!(!is_in_flight(state.subscription_syncs(), "source-1").unwrap());
    assert!(begin_in_flight(state.subscription_syncs(), "subscription", "source-1").is_ok());
}
