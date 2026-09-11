use serde_json::json;
use znet_client_core::{
    ClientCoreSnapshot, ClientScope, ConfigRevision, CoreInstanceId, ProfileId, SnapshotRevision,
    SourceStatus,
};

#[test]
fn snapshot_contract_serializes_for_the_tauri_bridge() {
    let value = serde_json::to_value(ClientCoreSnapshot {
        revision: SnapshotRevision(7),
        scope: ClientScope {
            profile_id: Some(ProfileId("profile-a".to_string())),
            config_revision: ConfigRevision(3),
            core_instance_id: CoreInstanceId(2),
        },
        source_status: SourceStatus::Ready,
        active_probe_jobs: Vec::new(),
    })
    .unwrap();

    assert_eq!(
        value,
        json!({
            "revision": 7,
            "scope": {
                "profileId": "profile-a",
                "configRevision": 3,
                "coreInstanceId": 2
            },
            "sourceStatus": "ready",
            "activeProbeJobs": []
        })
    );
}
