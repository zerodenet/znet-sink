use super::*;
use std::collections::HashMap;
#[test]
fn checkpoints_survive_reopening_and_schedulers_cannot_read_each_other() {
    let root = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let access = lease(&manager, "subscriptions").unwrap();
    let retry = HashMap::from([("source-1".to_string(), (3u32, 600_000u64))]);
    let bytes = serde_json::to_vec(&Checkpoint {
        schema_version: 1,
        retries: &retry,
    })
    .unwrap();
    storage::change(
        root.path(),
        &access,
        &[Change::Put {
            key: "retry-state".into(),
            expected: None,
            value: bytes,
        }],
    )
    .unwrap()
    .take(&access)
    .unwrap();
    drop(access);
    let access = lease(&Manager::default(), "subscriptions").unwrap();
    let (restored, rev) =
        read_checkpoint::<HashMap<String, (u32, u64)>>(root.path(), &access).unwrap();
    assert_eq!(restored, retry);
    assert!(rev.is_some());
    let other = lease(&Manager::default(), "rule-sets").unwrap();
    assert_eq!(
        read_checkpoint::<HashMap<String, (u32, u64)>>(root.path(), &other).unwrap(),
        (HashMap::new(), None)
    );
}
#[test]
fn damaged_and_future_checkpoints_are_rejected_without_resetting_retry_state() {
    for bytes in [
        b"invalid".as_slice(),
        br#"{"schema_version":2,"retries":{}}"#.as_slice(),
    ] {
        let root = tempfile::tempdir().unwrap();
        let access = lease(&Manager::default(), "subscriptions").unwrap();
        storage::change(
            root.path(),
            &access,
            &[Change::Put {
                key: "retry-state".into(),
                expected: None,
                value: bytes.to_vec(),
            }],
        )
        .unwrap()
        .take(&access)
        .unwrap();
        let access = lease(&Manager::default(), "subscriptions").unwrap();
        assert!(read_checkpoint::<HashMap<String, u64>>(root.path(), &access).is_err());
    }
    assert!(lease(&Manager::default(), "plugin-chosen-owner").is_err());
}
