use super::*;
#[test]
fn traffic_wire_keeps_u64_max_and_null_without_changing_page_cursors() {
    let value = wire::exact(
        json!({"config_revision":u64::MAX,"total":3,"next_offset":2,"scopes":[{"planes":[{"counters":{"rx_bytes":u64::MAX,"total":u64::MAX,"tx_bytes":0,"errors":null}}]}]}),
    );
    assert_eq!(value["config_revision"], u64::MAX.to_string());
    assert_eq!(
        value["scopes"][0]["planes"][0]["counters"]["rx_bytes"],
        u64::MAX.to_string()
    );
    assert_eq!(value["scopes"][0]["planes"][0]["counters"]["tx_bytes"], "0");
    assert!(value["scopes"][0]["planes"][0]["counters"]["errors"].is_null());
    assert_eq!(value["next_offset"], 2);
    assert_eq!(
        value["scopes"][0]["planes"][0]["counters"]["total"],
        u64::MAX.to_string()
    );
}
#[test]
fn traffic_fallback_only_follows_explicit_initial_subscription_refusal() {
    for code in ["unsupported", "permission_denied"] {
        let error = AppError::core_response(
            json!({"id":"znet-sink-subscribe","ok":false,"error":{"code":code,"message":"no event stream"}}),
        );
        assert!(subscription_refused(&error));
    }
    for code in ["conflict", "timeout", "connection_closed"] {
        let error = AppError::core_response(
            json!({"id":"znet-sink-subscribe","ok":false,"error":{"code":code,"message":"unknown delivery"}}),
        );
        assert!(!subscription_refused(&error));
    }
    let error = AppError::core_response(
        json!({"id":"submitted-reset","ok":false,"error":{"code":"unsupported","message":"no reset"}}),
    );
    assert!(!subscription_refused(&error));
}
#[test]
fn traffic_events_retain_exact_counts_and_delivery_sequence() {
    let value = crate::kernel::zero::events::normalize_event(
        &json!({"event_type":"stats.scopes_sampled","core_instance_id":"core-a","sequence":u64::MAX,"payload":{"total":1,"scopes":[{"sampled_at_monotonic_ns":u64::MAX}]}}),
    );
    let encoded = serde_json::to_value(value).unwrap();
    assert_eq!(encoded["sequenceExact"], u64::MAX.to_string());
    assert_eq!(encoded["coreInstanceId"], "core-a");
    assert_eq!(encoded["payload"]["kind"], "trafficObservation");
    assert_eq!(
        encoded["payload"]["data"]["scopes"][0]["sampled_at_monotonic_ns"],
        u64::MAX.to_string()
    );
}
