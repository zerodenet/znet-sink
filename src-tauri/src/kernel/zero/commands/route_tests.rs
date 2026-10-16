use super::*;
fn body() -> Value {
    json!({"target":"example.test","port":443,"protocol":"tcp","inbound_tag":"web",
        "effective_mode":"rule","route_action":{"route":"proxy"},
        "matched_rule":{"index":u64::MAX,"condition":"domain_suffix example.test"}})
}
#[test]
fn actual_route_envelope_projects_rule_and_lossless_index_without_guessing_hops() {
    let mut result = body();
    result["hops"] = json!([{"rtt":22}]);
    result["private_future_field"] = json!("secret-canary");
    let projected = route_evaluation_result(json!({"accepted":true,"result":result})).unwrap();
    assert_eq!(projected["matched_rule"]["index"], u64::MAX.to_string());
    assert_eq!(projected["route_action"], json!({"route":"proxy"}));
    assert!(projected.get("hops").is_none());
    assert!(!projected.to_string().contains("secret-canary"));
}
#[test]
fn route_final_direct_and_reject_are_real_actions_not_missing_defaults() {
    for action in ["direct", "reject"] {
        let mut result = body();
        result["route_action"] = json!(action);
        result["matched_rule"] = Value::Null;
        let result = route_evaluation_result(json!({"accepted":true,"result":result})).unwrap();
        assert_eq!(result["route_action"], action);
        assert!(result["matched_rule"].is_null());
    }
}
#[test]
fn route_incomplete_unsupported_or_unaccepted_values_are_not_success() {
    for field in [
        "target",
        "port",
        "protocol",
        "effective_mode",
        "route_action",
        "matched_rule",
    ] {
        let mut result = body();
        result.as_object_mut().unwrap().remove(field);
        assert_eq!(
            route_evaluation_result(json!({"accepted":true,"result":result}))
                .unwrap_err()
                .code,
            "invalid_response"
        );
    }
    for value in [
        json!({}),
        json!({"accepted":true}),
        json!({"accepted":true,"result":{"hops":[]}}),
    ] {
        assert_eq!(
            route_evaluation_result(value).unwrap_err().code,
            "invalid_response"
        );
    }
    let denied =
        route_evaluation_result(json!({"accepted":false,"result":{"secret":"secret-canary"}}))
            .unwrap_err();
    assert_eq!(denied.code, "core_error");
    assert!(!serde_json::to_string(&denied)
        .unwrap()
        .contains("secret-canary"));
    let mut result = body();
    result["effective_mode"] = json!("unknown-future");
    assert!(route_evaluation_result(json!({"accepted":true,"result":result})).is_err());
}
#[cfg(unix)]
mod ipc {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        os::unix::net::UnixListener,
        time::Duration,
    };
    fn peer(
        result: Value,
    ) -> (
        tempfile::TempDir,
        CoreIpcOptions,
        std::thread::JoinHandle<Value>,
    ) {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let path = dir.path().join("diagnostics.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let subscribe: Value = serde_json::from_str(&line).unwrap();
            assert_eq!(subscribe["type"], "subscribe");
            writeln!(
                stream,
                "{}",
                json!({"id":subscribe["id"],"api_id":"zero.api.v1","ok":true,"result":"subscribed"})
            )
            .unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let request: Value = serde_json::from_str(&line).unwrap();
            writeln!(
                stream,
                "{}",
                json!({"id":request["id"],"api_id":"zero.api.v1","ok":true,"result":result})
            )
            .unwrap();
            request
        });
        (
            dir,
            CoreIpcOptions {
                socket: Some(path.to_string_lossy().into()),
                timeout_ms: Some(2000),
            },
            worker,
        )
    }
    #[tokio::test]
    async fn real_ipc_route_command_returns_evaluation_and_exact_inbound_protocol() {
        let (_dir, options, worker) = peer(json!({"accepted":true,"result":body()}));
        let result = trace_route(
            "example.test".into(),
            443,
            Some("tcp".into()),
            Some("web".into()),
            Some(options),
        )
        .await
        .unwrap();
        assert_eq!(result["matched_rule"]["index"], u64::MAX.to_string());
        let request = worker.join().unwrap();
        assert_eq!(request["method"], "diagnostics.trace_route");
        assert_eq!(
            request["params"],
            json!({"target":"example.test","port":443,"protocol":"tcp","inbound_tag":"web"})
        );
    }
    #[cfg(feature = "tool-dns")]
    #[tokio::test]
    async fn real_ipc_dns_cache_command_reads_a_miss_without_fakeip_or_dns_lookup() {
        let (_dir, options, worker) = peer(
            json!({"accepted":true,"result":{"enabled":true,"domain":"missing.test","hit":false,"addresses":[],"ttl_seconds":null}}),
        );
        let result = super::super::super::dns::dns_cache(
            Some("missing.test".into()),
            Some(50),
            Some(options),
        )
        .await
        .unwrap();
        assert_eq!(result["hit"], false);
        let request = worker.join().unwrap();
        assert_eq!(request["method"], "diagnostics.dns_cache");
        assert_eq!(
            request["params"],
            json!({"domain":"missing.test","limit":50})
        );
    }
}
