use super::*;
use crate::client_core::{ClientScope, ConfigRevision, CoreInstanceId, ProfileId};
fn record(id: usize) -> CaptureRecord {
    CaptureRecord {
        session_id: format!("capture-{id:032x}"),
        scope: Some(ClientScope {
            profile_id: Some(ProfileId("opaque-profile".into())),
            config_revision: ConfigRevision(u64::MAX),
            core_instance_id: CoreInstanceId(7),
        }),
        phase: CapturePhase::Capturing,
        confirmation: CaptureConfirmation::Confirmed,
        started_at_ms: 10,
        expires_at_ms: 1010,
        ended_at_ms: None,
        duration_seconds: 1,
        maximum_bytes: 1024 * 1024,
        stored_bytes: 100,
        stored_frames: 1,
        queue_drops_observed: 0,
        end_reason: None,
    }
}
#[test]
fn metadata_roundtrip_is_private_bounded_and_preserves_scope_revision_losslessly() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let store = CaptureRecoveryStore::new(&path);
    let records = (0..RECORD_LIMIT).map(record).collect::<Vec<_>>();
    store.save(&records).unwrap();
    assert_eq!(store.load().unwrap(), records);
    let bytes = fs::read(&path).unwrap();
    assert!(bytes.len() < BYTE_LIMIT);
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("18446744073709551615"));
    for key in [
        "payload",
        "params",
        "error",
        "token",
        "credential",
        "target",
        "message",
    ] {
        assert!(!text.contains(&format!("\"{key}\"")));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o077, 0);
    }
    assert!(store
        .save(&(0..=RECORD_LIMIT).map(record).collect::<Vec<_>>())
        .is_err());
    assert_eq!(store.load().unwrap(), records);
}
#[test]
fn corrupt_newer_or_secret_injected_journal_is_preserved_until_explicit_owner_clear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("capture-recovery.json");
    let store = CaptureRecoveryStore::new(&path);
    for bytes in [
        b"not-json-canary".to_vec(),
        serde_json::to_vec(&serde_json::json!({"schemaVersion":2,"records":[]})).unwrap(),
        serde_json::to_vec(
            &serde_json::json!({"schemaVersion":1,"records":[],"token":"secret-canary"}),
        )
        .unwrap(),
        vec![b'x'; BYTE_LIMIT + 1],
    ] {
        fs::write(&path, &bytes).unwrap();
        let error = store.load().unwrap_err();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(!error.message.contains("canary"));
        assert!(error.details.is_none());
    }
    for file in [
        "logs.jsonl",
        "connection-history.jsonl",
        "app-config.json",
        "znet-sink.db",
    ] {
        fs::write(dir.path().join(file), b"untouched").unwrap();
    }
    store.clear().unwrap();
    assert!(store.load().unwrap().is_empty());
    for file in [
        "logs.jsonl",
        "connection-history.jsonl",
        "app-config.json",
        "znet-sink.db",
    ] {
        assert_eq!(fs::read(dir.path().join(file)).unwrap(), b"untouched");
    }
}
#[test]
fn invalid_counter_or_phase_combinations_cannot_be_published() {
    let dir = tempfile::tempdir().unwrap();
    let store = CaptureRecoveryStore::new(&dir.path().join("capture-recovery.json"));
    let original = record(1);
    store.save(std::slice::from_ref(&original)).unwrap();
    let mut invalid = original.clone();
    invalid.stored_bytes = invalid.maximum_bytes + 1;
    assert!(store.save(&[invalid]).is_err());
    let mut invalid = original.clone();
    invalid.phase = CapturePhase::Stopped;
    assert!(store.save(&[invalid]).is_err());
    let mut invalid = original.clone();
    invalid.end_reason = Some(CaptureEndReason::User);
    assert!(store.save(&[invalid]).is_err());
    assert_eq!(store.load().unwrap(), vec![original]);
}
