use super::*;
use serde_json::json;
fn input() -> ManagedInput {
    serde_json::from_value(json!({"profileId":"p","configRevision":1,
    "endpointId":"opaque:/a","coreInstanceId":"core-1","expectedIntentRevision":7,
    "action":{"operation":"set_state","enabled":false}}))
    .unwrap()
}
fn source() -> Value {
    json!({"endpoints":[{"tag":"a","protocol":{"type":"future"}}]})
}
#[test]
fn endpoint_managed_control_rejects_changed_profile_revision_and_unowned_roles() {
    let row = super::super::tests::current();
    let app = AppConfig::default();
    assert!(prepare(input(), "other-profile", &source(), &row, &app).is_err());
    let mut changed = row.clone();
    changed["config_revision"] = json!(2);
    assert!(prepare(input(), "p", &source(), &changed, &app).is_err());
    assert!(prepare(
        input(),
        "p",
        &json!({"outbounds":[{"tag":"a"}]}),
        &row,
        &app
    )
    .is_err());
    let (input, next, revision) = prepare(input(), "p", &source(), &row, &app).unwrap();
    assert_eq!(revision, 1);
    assert!(matches!(
        input.action,
        Action::SetState {
            persistence: Persistence::RuntimeOnly,
            ..
        }
    ));
    assert_eq!(next.profile_edits["p"]["endpoints"]["a"]["enabled"], false);
    assert!(
        serde_json::from_value::<ManagedInput>(json!({"profileId":"p","configRevision":1,
        "endpointId":"a","coreInstanceId":"core-1","expectedIntentRevision":7,
        "action":{"operation":"set_state","enabled":false,"persistence":"source_file"}}))
        .is_err()
    );
}
#[test]
fn endpoint_managed_publishes_only_after_confirmation_and_reports_storage_failure() {
    let mut saved = false;
    let result = publish_confirmed(Err(AppError::internal("timeout")), || {
        saved = true;
        Ok(())
    });
    assert!(result.is_err());
    assert!(!saved);
    let row = super::super::tests::current();
    assert_eq!(
        publish_confirmed(Ok(row.clone()), || {
            saved = true;
            Ok(())
        })
        .unwrap(),
        row
    );
    assert!(saved);
    let error = publish_confirmed(Ok(row), || Err(AppError::internal("disk full"))).unwrap_err();
    assert_eq!(error.code, "endpoint_preferences_unsaved");
    assert!(error.message.contains("内核已生效"));
}

#[test]
fn endpoint_card_addresses_match_declared_resource_without_peer_addresses_or_keys() {
    let endpoint = json!({"endpoint_id":"opaque:a","tag":"a","protocol":"wireguard","configuration":{"origin":"explicit"}});
    let source = json!({"endpoints":[{"tag":"a","protocol":{"type":"wireguard",
        "addresses":["10.66.0.2/32"," fd66::2/128 ","10.66.0.2/32",""],
        "private_key":"not-public","peers":[{"endpoint":"remote.test:51820","allowed_ips":["0.0.0.0/0"]}]}}]});
    let result = addresses::configured(&source, std::slice::from_ref(&endpoint));
    assert_eq!(result["opaque:a"], ["10.66.0.2/32", "fd66::2/128"]);
    let mut duplicate = source.clone();
    duplicate["endpoints"]
        .as_array_mut()
        .unwrap()
        .push(source["endpoints"][0].clone());
    assert!(addresses::configured(&duplicate, std::slice::from_ref(&endpoint)).is_empty());
    let mut other = endpoint.clone();
    other["protocol"] = json!("future_mesh");
    assert!(addresses::configured(&source, &[other]).is_empty());
    assert!(addresses::configured(&json!({}), &[endpoint]).is_empty());
}

#[test]
fn endpoint_card_addresses_support_inbound_only_and_legacy_roles_by_exact_binding() {
    let inbound = json!({"endpoint_id":"opaque:in","tag":"a","protocol":"wireguard","configuration":{"origin":"explicit"},"outbound_tags":[]});
    let source = json!({"endpoints":[{"tag":"a","directions":{"inbound":true,"outbound":false},"protocol":{"type":"wireguard","addresses":["10.66.0.1/32"]}}]});
    assert_eq!(
        addresses::configured(&source, &[inbound])["opaque:in"],
        ["10.66.0.1/32"]
    );
    let legacy = json!({"endpoint_id":"opaque:legacy","tag":"display-name","protocol":"wireguard","configuration":{"origin":"legacy"},"inbound_tags":[],"outbound_tags":["a"]});
    let source = json!({"outbounds":[{"tag":"a","protocol":{"type":"wireguard","addresses":["10.77.0.2/32"]}}]});
    assert_eq!(
        addresses::configured(&source, std::slice::from_ref(&legacy))["opaque:legacy"],
        ["10.77.0.2/32"]
    );
    let mut unrelated = legacy;
    unrelated["outbound_tags"] = json!(["b"]);
    assert!(addresses::configured(&source, &[unrelated]).is_empty());
}
