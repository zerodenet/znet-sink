use super::*;
fn reset_input() -> ResetInput {
    serde_json::from_value(json!({"expected_core_instance_id":"core-a","operation_id":"reset-a","targets":[{"scope":{"kind":"endpoint","endpoint_id":"opaque:/a"},"expected_stats_epoch":"period-a","expected_generation":u64::MAX.to_string()}]})).unwrap()
}
pub(super) fn snapshot() -> Value {
    json!({"scope":{"kind":"endpoint","endpoint_id":"opaque:/a"},"core_instance_id":"core-a","config_revision":u64::MAX,"generation":u64::MAX,"stats_epoch":"period-b","epoch_started_at_unix_ms":100,"capture_started_at_unix_ms":100,"sampled_at_monotonic_ns":200,"sampled_at_unix_ms":201,"planes":[{"plane":"inner","accounting_basis":"endpoint_inner_ip","source_roles":["inbound","outbound"],"available_metrics":["rx_bytes","tx_bytes"],"resettable_metrics":["rx_bytes","tx_bytes"],"counters":{"rx_bytes":5,"tx_bytes":0,"errors":null}}],"activity":{"active_stream_flows":2,"active_datagram_flows":1,"active_packet_routes":1},"reset_policy":"all_available_cumulative_no_cascade"})
}
#[test]
fn traffic_reset_receipt_requires_exact_scope_instance_epoch_and_generation() {
    let input = reset_input();
    let receipt =
        json!({"operation_id":"reset-a","core_instance_id":"core-a","snapshots":[snapshot()]});
    validate_reset(&receipt, &input).unwrap();
    for (field, value) in [
        ("core_instance_id", json!("old")),
        ("stats_epoch", json!("period-a")),
        ("generation", json!(4)),
        ("scope", json!({"kind":"global"})),
    ] {
        let mut bad = receipt.clone();
        bad["snapshots"][0][field] = value;
        assert!(validate_reset(&bad, &input).is_err(), "{field}");
    }
    let mut bad = receipt;
    bad["operation_id"] = json!("other");
    assert!(validate_reset(&bad, &input).is_err());
}
#[test]
fn traffic_reset_serializes_decimal_cas_without_precision_loss_and_omits_null_generation() {
    let mut input = reset_input();
    assert_eq!(
        input.wire().unwrap()["targets"][0]["expected_generation"],
        u64::MAX
    );
    input.targets[0].expected_generation = None;
    assert!(input.wire().unwrap()["targets"][0]
        .get("expected_generation")
        .is_none());
    input.targets.push(types::ResetTarget {
        scope: input.targets[0].scope.clone(),
        expected_stats_epoch: "other".into(),
        expected_generation: None,
    });
    assert!(input.wire().is_err());
    let mut input = reset_input();
    input.targets[0].expected_generation = Some("18446744073709551616".into());
    assert!(input.wire().is_err());
}
#[test]
fn traffic_list_limits_and_decimal_revision_preconditions_are_explicit() {
    let query:ListInput=serde_json::from_value(json!({"offset":64,"limit":64,"expected_core_instance_id":"a","expected_config_revision":u64::MAX.to_string(),"expected_registry_revision":"2"})).unwrap();
    let wire = query.wire().unwrap();
    assert_eq!(wire["expected_config_revision"], u64::MAX);
    assert_eq!(wire["expected_registry_revision"], 2);
    assert!(ListInput {
        limit: Some(257),
        ..Default::default()
    }
    .wire()
    .is_err());
    assert!(serde_json::from_value::<ResetInput>(json!({"expected_core_instance_id":"a","operation_id":"b","targets":[],"metrics":["rx_bytes"]})).is_err());
}
