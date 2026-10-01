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
