use super::*;

#[test]
fn query_failure_is_not_a_successful_stopped_snapshot() {
    for code in ["timeout", "connection_closed", "core_unavailable"] {
        let error = AppError {
            code,
            message: "query failed".into(),
            details: None,
        };
        let result = decode_tun_status(Err(error)).unwrap_err();
        assert_eq!(result.code, code);
    }
}

#[test]
fn incomplete_and_capability_only_responses_are_not_running_state() {
    for value in [
        json!({}),
        json!({"supported": true}),
        json!({"running": "false"}),
    ] {
        assert!(decode_tun_status(Ok(value)).is_err());
    }
    assert!(
        !decode_tun_status(Ok(json!({"running": false})))
            .unwrap()
            .enabled
    );
    assert!(
        decode_tun_status(Ok(json!({"running": true})))
            .unwrap()
            .enabled
    );
}

#[test]
fn omitted_empty_tun_routes_follow_the_kernel_wire_contract() {
    let status = decode_tun_status(Ok(json!({"running":true}))).unwrap();
    assert_eq!(status.include_cidrs, Some(vec![]));
    assert_eq!(status.exclude_cidrs, Some(vec![]));
    let status = decode_tun_status(Ok(
        json!({"running":true,"includeCidrs":["10.0.0.0/8"],"exclude_cidrs":[]}),
    ))
    .unwrap();
    assert_eq!(status.include_cidrs, Some(vec!["10.0.0.0/8".into()]));
}
