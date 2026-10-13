use znet_client_core::{ClientScope, ConfigRevision, CoreInstanceId, ProfileId};

#[test]
fn client_configuration_digest_round_trips_without_javascript_integer_loss() {
    let scope = ClientScope {
        profile_id: Some(ProfileId("p".into())),
        config_revision: ConfigRevision(u64::MAX),
        core_instance_id: CoreInstanceId(3),
    };
    let wire = serde_json::to_value(&scope).unwrap();
    assert_eq!(wire["configRevision"], "18446744073709551615");
    assert_eq!(serde_json::from_value::<ClientScope>(wire).unwrap(), scope);
    assert_eq!(
        serde_json::from_str::<ConfigRevision>("7").unwrap(),
        ConfigRevision(7)
    );
    for bad in ["\"01\"", "\"-1\"", "\"18446744073709551616\""] {
        assert!(serde_json::from_str::<ConfigRevision>(bad).is_err());
    }
}
