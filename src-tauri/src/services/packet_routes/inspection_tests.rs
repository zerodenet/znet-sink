use super::*;
use crate::services::packet_routes::ledger::CloseLedger;
use std::{
    io::BufRead,
    process::{Command, Stdio},
    sync::{Arc, Barrier},
};

fn store() -> (tempfile::TempDir, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("packet-private.json");
    drop(acquire(&path).unwrap());
    (directory, path)
}
fn target() -> PacketTarget {
    PacketTarget {
        profile_id: Some("private-profile-canary".into()),
        client_scope_generation: 1,
        host_session_id: "host-fixture".into(),
        close_operation_id: None,
        route_id: "private-route-canary".into(),
        core_instance_id: "private-core-canary".into(),
        config_revision: u64::MAX.to_string(),
    }
}
fn safe(report: &PacketLedgerInspection, path: &Path) {
    assert!(report.read_only && report.barriers_retained);
    let text = serde_json::to_string(report).unwrap();
    assert!(text.len() < 4096);
    for value in [
        "private-profile-canary",
        "private-route-canary",
        "private-core-canary",
        "secret-token-canary",
        path.to_str().unwrap(),
    ] {
        assert!(
            !text.contains(value),
            "private material escaped the closed projection"
        );
    }
}
#[test]
fn absent_and_partial_initialization_are_inspected_without_creating_anything() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("absent").join("packet.json");
    let report = inspect(&path);
    assert_eq!(report.reason_code, "not-initialized");
    assert!(!path.parent().unwrap().exists());
    assert_eq!(report.evidence.journal_present, Some(false));
    assert_eq!(report.evidence.marker_present, Some(false));
    safe(&report, &path);
    let path = directory.path().join("partial.json");
    fs::write(lock_path(&path), b"").unwrap();
    let report = inspect(&path);
    assert_eq!(report.reason_code, "not-initialized");
    assert!(!path.exists());
    assert_eq!(fs::read(lock_path(&path)).unwrap(), b"");
    fs::write(&path, br#"{"schemaVersion":1,"records":[]}"#).unwrap();
    let report = inspect(&path);
    assert_eq!(report.reason_code, "initialization-pending");
    assert_eq!(report.evidence.verified_record_count, Some(0));
    assert_eq!(fs::read(lock_path(&path)).unwrap(), b"");
}
#[test]
fn actual_valid_store_projects_only_verified_metadata_and_reopening_grants_nothing() {
    let (_directory, path) = store();
    let ledger = CloseLedger::at(&path);
    let original = ledger.reserve(&target()).unwrap();
    let before = fs::read(&path).unwrap();
    let marker = fs::read(lock_path(&path)).unwrap();
    let report = ledger.inspect();
    assert_eq!(report.reason_code, "ready");
    assert_eq!(report.evidence.schema_version, Some(1));
    assert_eq!(report.evidence.verified_record_count, Some(1));
    assert_eq!(report.in_memory_barrier_count, Some(1));
    assert_eq!(
        report.evidence.journal_bytes,
        Some(before.len().to_string())
    );
    assert_eq!(
        report.evidence.journal_sha256,
        Some(format!("{:x}", Sha256::digest(&before)))
    );
    assert!(report.evidence.snapshot_stable);
    safe(&report, &path);
    let restored = CloseLedger::at(&path);
    let report = restored.inspect();
    safe(&report, &path);
    assert_eq!(report.in_memory_barrier_count, Some(0)); // no refresh or restored grant
    assert!(restored.reserve(&target()).is_err());
    assert_eq!(
        restored.view().records[0].operation_id,
        original.record.operation_id
    );
    assert_eq!(fs::read(&path).unwrap(), before);
    assert_eq!(fs::read(lock_path(&path)).unwrap(), marker);
}
#[test]
fn actual_bad_json_and_untrusted_structures_preserve_bytes_and_hide_all_payloads() {
    for raw in [
        b"{secret-token-canary not json".to_vec(),
        br#"[]"#.to_vec(),
        br#"{}"#.to_vec(),
        br#"{"schemaVersion":1,"records":[null]}"#.to_vec(),
        br#"{"schemaVersion":1,"records":[],"secret":"secret-token-canary"}"#.to_vec(),
    ] {
        let (_directory, path) = store();
        fs::write(&path, &raw).unwrap();
        let report = inspect(&path);
        assert_eq!(
            report.reason_code,
            if raw.starts_with(b"{secret") {
                "invalid-json"
            } else {
                "untrusted-structure"
            }
        );
        assert!(report.new_close_blocked);
        assert_eq!(report.evidence.verified_record_count, None);
        assert_eq!(
            report.evidence.journal_sha256,
            Some(format!("{:x}", Sha256::digest(&raw)))
        );
        safe(&report, &path);
        let reopened = CloseLedger::at(&path);
        assert_eq!(reopened.inspect().reason_code, report.reason_code);
        assert!(reopened.reserve(&target()).is_err());
        assert_eq!(fs::read(&path).unwrap(), raw);
    }
}
#[test]
fn unsupported_schema_invalid_records_and_duplicates_never_publish_partial_counts() {
    let (_directory, path) = store();
    let ledger = CloseLedger::at(&path);
    let reservation = ledger.reserve(&target()).unwrap();
    let record = serde_json::to_value(&reservation.record).unwrap();
    for (raw, code, schema) in [
        (
            json!({"schemaVersion":2,"records":[]}),
            "unsupported-schema",
            Some(2),
        ),
        (
            json!({"schemaVersion":1,"records":[record.clone(),record.clone()]}),
            "untrusted-structure",
            Some(1),
        ),
        (
            json!({"schemaVersion":1,"records":[{"rawSecret":"secret-token-canary"}]}),
            "untrusted-structure",
            Some(1),
        ),
        (
            json!({"schemaVersion":u64::MAX,"records":[]}),
            "unsupported-schema",
            None,
        ),
    ] {
        let bytes = serde_json::to_vec(&raw).unwrap();
        fs::write(&path, &bytes).unwrap();
        let report = ledger.inspect();
        assert_eq!(report.reason_code, code);
        assert_eq!(report.evidence.schema_version, schema);
        assert_eq!(report.evidence.verified_record_count, None);
        assert_eq!(report.in_memory_barrier_count, Some(1));
        safe(&report, &path);
        assert_eq!(fs::read(&path).unwrap(), bytes);
    }
}
#[test]
fn oversized_journal_is_metadata_only_with_no_partial_digest_or_parsed_count() {
    let (_directory, path) = store();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(BYTE_LIMIT as u64 + 1).unwrap();
    let report = inspect(&path);
    assert_eq!(report.reason_code, "oversized");
    assert_eq!(
        report.evidence.journal_bytes,
        Some((BYTE_LIMIT + 1).to_string())
    );
    assert_eq!(report.evidence.journal_sha256, None);
    assert_eq!(report.evidence.schema_version, None);
    assert_eq!(report.evidence.verified_record_count, None);
    safe(&report, &path);
    assert_eq!(file.metadata().unwrap().len(), BYTE_LIMIT as u64 + 1);
}
#[test]
fn missing_initialized_and_untrusted_markers_never_create_or_repair_a_journal() {
    let (_directory, path) = store();
    let ledger = CloseLedger::at(&path);
    ledger.reserve(&target()).unwrap();
    fs::remove_file(&path).unwrap();
    let report = ledger.inspect();
    assert_eq!(report.reason_code, "missing-initialized");
    assert_eq!(report.in_memory_barrier_count, Some(1));
    assert!(!path.exists());
    assert!(ledger.reserve(&target()).is_err());
    assert_eq!(
        CloseLedger::at(&path).inspect().reason_code,
        "missing-initialized"
    );
    assert!(!path.exists());
    for marker in [b"untrusted-secret-token-canary".to_vec(), vec![b'x'; 65]] {
        fs::write(lock_path(&path), &marker).unwrap();
        let report = inspect(&path);
        assert_eq!(report.reason_code, "invalid-marker");
        safe(&report, &path);
        assert_eq!(fs::read(lock_path(&path)).unwrap(), marker);
        assert!(!path.exists());
    }
}
#[test]
fn missing_coordination_marker_is_not_created_and_does_not_read_a_body() {
    let (_directory, path) = store();
    let before = fs::read(&path).unwrap();
    fs::remove_file(lock_path(&path)).unwrap();
    let report = inspect(&path);
    assert_eq!(report.reason_code, "coordination-missing");
    assert_eq!(report.evidence.journal_sha256, None);
    assert_eq!(report.evidence.verified_record_count, None);
    assert!(!lock_path(&path).exists());
    assert_eq!(fs::read(&path).unwrap(), before);
    safe(&report, &path);
}
#[test]
fn actual_other_process_lock_is_nonblocking_explanatory_and_preserves_evidence() {
    let (_directory, path) = store();
    let before = fs::read(&path).unwrap();
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "services::packet_routes::ledger::tests::journal_child_driver",
            "--nocapture",
        ])
        .env("ZNET_PACKET_TEST_CHILD", "hold-lock")
        .env("ZNET_PACKET_TEST_JOURNAL", &path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut output = std::io::BufReader::new(child.stdout.take().unwrap());
    let mut ready = false;
    for line in output.by_ref().lines() {
        if line.unwrap().contains("packet-child-lock-held") {
            ready = true;
            break;
        }
    }
    assert!(ready);
    let report = inspect(&path);
    assert_eq!(report.reason_code, "lock-busy");
    assert_eq!(report.evidence.lock_state, "busy");
    assert_eq!(report.evidence.journal_sha256, None);
    assert_eq!(report.evidence.verified_record_count, None);
    safe(&report, &path);
    child.stdin.take().unwrap().write_all(b"x").unwrap();
    assert!(child.wait().unwrap().success());
    assert_eq!(inspect(&path).reason_code, "ready");
    assert_eq!(fs::read(&path).unwrap(), before);
}
#[test]
fn concurrent_cooperating_readers_and_writers_never_classify_half_a_publication() {
    let (_directory, path) = store();
    let barrier = Arc::new(Barrier::new(2));
    let writer_path = path.clone();
    let writer_barrier = barrier.clone();
    let worker = std::thread::spawn(move || {
        writer_barrier.wait();
        for _ in 0..24 {
            if let Ok(_lock) = acquire(&writer_path) {
                save(&writer_path, &[]).unwrap();
            }
        }
    });
    barrier.wait();
    for _ in 0..24 {
        let report = inspect(&path);
        assert!(["ready", "lock-busy", "changed-during-inspection"].contains(&report.reason_code));
        safe(&report, &path);
        if report.reason_code == "ready" {
            assert_eq!(report.evidence.verified_record_count, Some(0));
        }
    }
    worker.join().unwrap();
    assert_eq!(inspect(&path).reason_code, "ready");
}
#[test]
fn real_uncoordinated_change_after_read_rejects_the_snapshot_instead_of_returning_old_bytes() {
    let (_directory, path) = store();
    let replacement = path.with_extension("replace");
    fs::write(&replacement, b"changed-and-longer-private-bytes").unwrap();
    assert_eq!(
        bytes_observed(&path, BYTE_LIMIT, || {
            fs::rename(&replacement, &path).unwrap();
        })
        .unwrap_err(),
        "changed-during-inspection"
    );
    assert_eq!(
        fs::read(&path).unwrap(),
        b"changed-and-longer-private-bytes"
    );
}
#[test]
fn publication_failure_stays_blocked_after_readable_file_inspection_without_clearing_memory() {
    let (_directory, path) = store();
    let ledger = CloseLedger::at(&path);
    let reservation = ledger.reserve(&target()).unwrap();
    ledger.fail_next_publication();
    assert!(ledger.complete(&reservation).is_err());
    let before = fs::read(&path).unwrap();
    let report = ledger.inspect();
    assert_eq!(report.reason_code, "publication-unconfirmed");
    assert!(report.publication_unconfirmed && report.new_close_blocked);
    assert_eq!(report.in_memory_barrier_count, Some(1));
    assert_eq!(report.evidence.verified_record_count, Some(1));
    safe(&report, &path);
    assert!(ledger.reserve(&target()).is_err());
    assert_eq!(fs::read(&path).unwrap(), before);
}
#[test]
fn unavailable_and_unsupported_lock_classifications_never_echo_os_messages_or_paths() {
    assert_eq!(
        lock_error(fs::TryLockError::WouldBlock),
        ("lock-busy", "busy")
    );
    assert_eq!(
        lock_error(fs::TryLockError::Error(io::Error::new(
            io::ErrorKind::Unsupported,
            "secret-token-canary"
        ))),
        ("lock-unsupported", "unsupported")
    );
    assert_eq!(
        lock_error(fs::TryLockError::Error(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "secret-token-canary"
        ))),
        ("permission-denied", "unavailable")
    );
    let directory = tempfile::tempdir().unwrap();
    let parent = directory.path().join("not-directory");
    fs::write(&parent, b"secret-token-canary").unwrap();
    let path = parent.join("packet.json");
    // A file used as a parent is not one portable io::ErrorKind: Windows
    // reports this child as NotFound, while Unix reports NotADirectory.
    // Assert the actual platform boundary, without changing production rules.
    let error = fs::symlink_metadata(&path).unwrap_err();
    let report = inspect(&path);
    #[cfg(windows)]
    {
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert_eq!(report.reason_code, "not-initialized");
        assert_eq!(report.evidence.journal_present, Some(false));
        assert_eq!(report.evidence.marker_present, Some(false));
    }
    #[cfg(unix)]
    {
        assert_eq!(error.kind(), io::ErrorKind::NotADirectory);
        assert_eq!(report.reason_code, "io-unavailable");
    }
    safe(&report, &path);
    assert_eq!(fs::read(&parent).unwrap(), b"secret-token-canary");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    // Deterministically cover generic IO projection on every OS as well,
    // instead of relying on an OS-specific pathname error to trigger it.
    let error = io::Error::other("secret-token-canary");
    assert_eq!(io_code(&error), "io-unavailable");
    let report = super::report(io_code(&error), PacketLedgerEvidence::default());
    assert_eq!(report.reason_code, "io-unavailable");
    assert!(report.new_close_blocked);
    safe(&report, &path);
}
#[cfg(unix)]
#[test]
fn actual_permission_denial_and_unsafe_file_types_are_metadata_only() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (directory, path) = store();
    let original = fs::read(&path).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
    let report = inspect(&path);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    if unsafe { libc::geteuid() } != 0 {
        assert_eq!(report.reason_code, "permission-denied");
        assert_eq!(report.evidence.journal_sha256, None);
    } else {
        assert_eq!(report.reason_code, "ready");
    } // A privileged read must not be mislabelled denied.
    assert_eq!(fs::read(&path).unwrap(), original);
    safe(&report, &path);
    let link = directory.path().join("linked.json");
    symlink(&path, &link).unwrap();
    let report = inspect(&link);
    assert_eq!(report.reason_code, "unsafe-file");
    assert_eq!(report.evidence.journal_sha256, None);
    safe(&report, &link);
    let other = directory.path().join("directory.json");
    fs::create_dir(&other).unwrap();
    assert_eq!(inspect(&other).reason_code, "unsafe-file");
    let marker = lock_path(&path);
    fs::remove_file(&marker).unwrap();
    symlink(&path, &marker).unwrap();
    assert_eq!(inspect(&path).reason_code, "unsafe-file");
}
