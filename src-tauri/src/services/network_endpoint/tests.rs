use super::*;
use crate::kernel::zero::parsing::parse_capabilities;

pub(super) fn caps() -> GuiZeroCapabilities {
    parse_capabilities(
        &json!({"features":["network_endpoint_catalog_v1","network_endpoint_control_v1"],
        "contracts":{"capabilities":{"current":1,"minimum_supported":1},"control_api":{"current":1,"minimum_supported":1}}}),
        None,
    )
}
pub(super) fn current() -> Value {
    json!({"endpoint_id":"opaque:/a", "tag":"a", "protocol":"future", "core_instance_id":"core-1", "config_revision":1, "intent_revision":7,
    "generation":null,"started_at_unix_ms":null,"observed_at_unix_ms":1,"enabled":true,
    "state":"running","health":"unknown","state_source":"config","inbound_tags":[],"outbound_tags":["a"],
    "allowed":{"inbound":false,"outbound":true},"effective":{"inbound":false,"outbound":true},
    "counters":{"active_stream_flows":0,"inner_rx_bytes":null},"last_error":null,
    "supported":{"operations":["set_state","set_directions","restart","clear_overrides","details"], "directions":{"inbound":false,"outbound":true}}})
}
fn input(action: Action) -> ControlInput {
    ControlInput {
        endpoint_id: "opaque:/a".into(),
        core_instance_id: "core-1".into(),
        expected_intent_revision: 7,
        action,
    }
}

#[test]
fn endpoint_control_preserves_opaque_identity_revision_and_explicit_persistence() {
    let request = input(Action::SetState {
        enabled: false,
        persistence: Persistence::RuntimeOnly,
    });
    let (method, params) = control_params(&request, &current(), &caps()).unwrap();
    assert_eq!(method, "endpoints.set_state");
    assert_eq!(
        params,
        json!({"endpoint_id":"opaque:/a","expected_intent_revision":7,"enabled":false,"persistence":"runtime_only"})
    );
    let request = input(Action::SetDirections {
        directions: Directions {
            inbound: false,
            outbound: true,
        },
        persistence: Persistence::SourceFile,
    });
    let (_, params) = control_params(&request, &current(), &caps()).unwrap();
    assert_eq!(params["persistence"], "source_file");
    assert_eq!(
        params["directions"],
        json!({"inbound":false,"outbound":true})
    );
}

#[test]
fn endpoint_control_rejects_old_cores_unregistered_operations_and_unconfigured_roles() {
    let request = input(Action::Restart {});
    assert!(!supports(
        &GuiZeroCapabilities::default(),
        "network_endpoint_catalog_v1"
    ));
    assert_eq!(
        control_params(&request, &current(), &GuiZeroCapabilities::default())
            .unwrap_err()
            .code,
        "unsupported"
    );
    let mut endpoint = current();
    endpoint["supported"]["operations"] = json!(["get"]);
    assert_eq!(
        control_params(&request, &endpoint, &caps())
            .unwrap_err()
            .code,
        "unsupported"
    );
    let request = input(Action::SetDirections {
        directions: Directions {
            inbound: true,
            outbound: true,
        },
        persistence: Persistence::RuntimeOnly,
    });
    assert_eq!(
        control_params(&request, &current(), &caps())
            .unwrap_err()
            .code,
        "unsupported"
    );
}

#[test]
fn endpoint_control_rejects_stale_core_and_intent_before_sending() {
    let mut endpoint = current();
    endpoint["core_instance_id"] = json!("core-2");
    assert_eq!(
        control_params(&input(Action::Restart {}), &endpoint, &caps())
            .unwrap_err()
            .code,
        "conflict"
    );
    endpoint = current();
    endpoint["intent_revision"] = json!(8);
    assert_eq!(
        control_params(&input(Action::Restart {}), &endpoint, &caps())
            .unwrap_err()
            .code,
        "conflict"
    );
}

#[test]
fn endpoint_success_requires_all_reconciliation_flags_and_matching_identity() {
    let request = input(Action::Restart {});
    for result in [
        json!({"accepted":true}),
        json!({"accepted":false,"result":{"applied":true,"reconciled":true}}),
        json!({"accepted":true,"result":{"applied":true,"reconciled":false,"endpoint":current()}}),
    ] {
        assert!(confirmed(result, &request).is_err());
    }
    let response = json!({"accepted":true,"result":{"applied":true,"reconciled":true,"persistence":"runtime_only","endpoint":current()}});
    assert_eq!(confirmed(response, &request).unwrap(), current());
}

#[test]
fn endpoint_input_rejects_arbitrary_methods_unknown_fields_and_persistence() {
    for action in [
        json!({"operation":"config_apply"}),
        json!({"operation":"restart","method":"config.apply"}),
        json!({"operation":"set_state","enabled":true,"persistence":"always"}),
    ] {
        assert!(serde_json::from_value::<Action>(action).is_err());
    }
    let (_, params) =
        control_params(&input(Action::ClearOverrides {}), &current(), &caps()).unwrap();
    assert!(params.get("persistence").is_none());
}

#[test]
fn endpoint_wire_validation_rejects_incomplete_rows_and_broken_pagination() {
    validate_endpoint(&current()).unwrap();
    for path in [
        "endpoint_id",
        "intent_revision",
        "supported",
        "counters",
        "inbound_tags",
    ] {
        let mut row = current();
        row.as_object_mut().unwrap().remove(path);
        assert!(validate_endpoint(&row).is_err(), "missing {path}");
    }
    assert_eq!(
        page_next(&json!({"total":2,"next_offset":1}), 0, 1).unwrap(),
        Some(1)
    );
    assert_eq!(
        page_next(&json!({"total":2,"next_offset":null}), 1, 1).unwrap(),
        None
    );
    for page in [
        json!({"total":2,"next_offset":null}),
        json!({"total":2,"next_offset":2}),
        json!({"next_offset":1}),
    ] {
        assert!(page_next(&page, 0, 1).is_err());
    }
}

#[cfg(unix)]
#[path = "ipc_tests.rs"]
mod ipc_tests;

#[test]
fn endpoint_reconciliation_cannot_confirm_a_different_requested_intent() {
    let response = json!({"accepted":true,"result":{"applied":true,"reconciled":true,"persistence":"runtime_only","endpoint":current()}});
    assert!(confirmed(
        response.clone(),
        &input(Action::SetState {
            enabled: false,
            persistence: Persistence::RuntimeOnly
        })
    )
    .is_err());
    assert!(confirmed(
        response,
        &input(Action::SetDirections {
            directions: Directions {
                inbound: false,
                outbound: false
            },
            persistence: Persistence::RuntimeOnly
        })
    )
    .is_err());
    let mut row = current();
    row["state_source"] = json!("runtime_override");
    assert!(confirmed(
        json!({"accepted":true,"result":{"applied":true,"reconciled":true,"persistence":"runtime_only","endpoint":row}}),
        &input(Action::ClearOverrides {})
    )
    .is_err());
}

#[test]
fn endpoint_source_persistence_requires_an_explicit_matching_acknowledgement() {
    let request = input(Action::SetState {
        enabled: true,
        persistence: Persistence::SourceFile,
    });
    for persistence in [Value::Null, json!("runtime_only")] {
        assert!(confirmed(json!({"accepted":true,"result":{"applied":true,"reconciled":true,"endpoint":current(),"persistence":persistence}}), &request).is_err());
    }
    assert!(confirmed(json!({"accepted":true,"result":{"applied":true,"reconciled":true,"endpoint":current(),"persistence":"source_file"}}), &request).is_ok());
}

#[test]
fn endpoint_control_sends_atomic_core_condition_only_when_declared() {
    let mut capabilities = caps();
    let request = input(Action::Restart {});
    assert!(control_params(&request, &current(), &capabilities)
        .unwrap()
        .1
        .get("expected_core_instance_id")
        .is_none());
    capabilities
        .features
        .push("network_endpoint_control_preconditions_v1".into());
    assert_eq!(
        control_params(&request, &current(), &capabilities)
            .unwrap()
            .1["expected_core_instance_id"],
        "core-1"
    );
}
