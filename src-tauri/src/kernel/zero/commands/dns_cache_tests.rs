use super::*;
#[test]
fn dns_cache_list_and_domain_keep_u64_ttl_without_manufactured_counts() {
    let body = json!({"enabled":true,"entries":[{"domain":"example.test","addresses":["192.0.2.1","2001:db8::1"],"ttl_seconds":u64::MAX}],"count":1});
    let result = cache_command_result(json!({"accepted":true,"result":body})).unwrap();
    assert_eq!(result["entries"][0]["ttl_seconds"], u64::MAX.to_string());
    assert_eq!(result["count"], 1);
    let hit=cache_command_result(json!({"accepted":true,"result":{"enabled":true,"domain":"example.test","hit":true,"addresses":["192.0.2.1"],"ttl_seconds":0}})).unwrap();
    assert_eq!(hit["ttl_seconds"], "0");
    assert!(hit.get("count").is_none());
}
#[test]
fn disabled_and_cache_miss_are_distinct_from_missing_fields() {
    let disabled = cache_result(json!({"enabled":false,"entries":[],"count":0})).unwrap();
    assert_eq!(disabled["enabled"], false);
    let miss=cache_result(json!({"enabled":true,"domain":"missing.test","hit":false,"addresses":[],"ttl_seconds":null})).unwrap();
    assert_eq!(miss["hit"], false);
    for bad in [
        json!({}),
        json!({"enabled":true}),
        json!({"enabled":true,"entries":[]}),
        json!({"enabled":true,"entries":[],"count":1}),
        json!({"enabled":true,"domain":"test","addresses":[],"ttl_seconds":null}),
        json!({"enabled":true,"domain":"test","hit":false,"addresses":["192.0.2.1"],"ttl_seconds":null}),
    ] {
        assert_eq!(cache_result(bad).unwrap_err().code, "invalid_response");
    }
}
#[test]
fn cache_unknown_payload_and_counts_cannot_be_accepted_or_echoed() {
    for bad in [
        json!({"enabled":"secret-canary","entries":[],"count":0}),
        json!({"enabled":true,"entries":[{"domain":"test","addresses":["secret-canary"],"ttl_seconds":2}],"count":1}),
        json!({"enabled":true,"entries":[],"count":u64::MAX}),
    ] {
        let error = cache_result(bad).unwrap_err();
        assert!(!serde_json::to_string(&error)
            .unwrap()
            .contains("secret-canary"));
    }
    assert!(cache_command_result(json!({"enabled":true,"entries":[],"count":0})).is_err());
    assert_eq!(
        cache_command_result(json!({"accepted":false}))
            .unwrap_err()
            .code,
        "core_error"
    );
}
