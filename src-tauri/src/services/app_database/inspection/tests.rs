use super::*;
use serde_json::json;

fn database() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    load_domain_data(dir.path()).unwrap();
    dir
}
fn row(dir: &Path, id: usize, corrupt: bool, candidate: &str) {
    let c = Connection::open(database_path(dir)).unwrap();
    let operation = format!("{id:032x}");
    let profile = format!("profile-{id}");
    let h = "a".repeat(64);
    let metadata = if corrupt {
        json!({"secret":"https://private.invalid/SECRET_TOKEN"})
    } else {
        json!({
            "schema":1,"operation_id":operation,"profile_id":profile,"session":"a".repeat(32),
            "version":1,"stage":"prepared","epoch":0,"created_at_unix_ms":1,
            "before_graph_digest":h,"after_graph_digest":h,"source_digest":h,"candidate_digest":h,
            "app_digest":h,"rules_digest":h,"effective_digest":h,"capture_digest":h,
            "original_core_instance_id":"original-core","original_config_revision":1,
            "original_scope_digest":h,"original_generation":1,"receipt":null
        })
    };
    c.execute(
        "INSERT INTO workspace_recovery VALUES(?1,?2,'prepared',1,?3,?4)",
        params![operation, profile, metadata.to_string(), candidate],
    )
    .unwrap();
}
fn bytes(dir: &Path) -> Vec<u8> {
    fs::read(database_path(dir)).unwrap()
}

#[test]
fn missing_store_does_not_create_directory_file_or_recovery_permit() {
    let parent = tempfile::tempdir().unwrap();
    let absent = parent.path().join("not-created");
    let result = inspect(&absent, None);
    assert_eq!(result.state, "not-initialized");
    assert!(!absent.exists());
    assert!(result.read_only && result.barriers_retained);
    assert_eq!(result.evidence.metadata_rows, None);
}
#[test]
fn valid_empty_metadata_is_not_runtime_or_candidate_confirmation() {
    let dir = database();
    let original = bytes(dir.path());
    let names = || {
        let mut rows = fs::read_dir(dir.path())
            .unwrap()
            .map(|x| x.unwrap().file_name())
            .collect::<Vec<_>>();
        rows.sort();
        rows
    };
    let original_names = names();
    let result = inspect(dir.path(), Some("fresh-host".into()));
    assert_eq!(result.state, "metadata-readable");
    assert_eq!(result.evidence.metadata_rows, Some(0));
    assert_eq!(result.evidence.pending_rows, Some(0));
    assert!(!result.evidence.candidate_bodies_read);
    assert_eq!(bytes(dir.path()), original);
    assert_eq!(
        names(),
        original_names,
        "immutable inspection must not create WAL/SHM files"
    );
}

#[test]
fn actual_wal_without_shm_is_never_ignored_or_allowed_to_create_shm() {
    let source = database();
    let writer = Connection::open(database_path(source.path())).unwrap();
    writer
        .execute(
            "INSERT INTO app_metadata VALUES('wal-test','committed-in-wal')",
            [],
        )
        .unwrap();
    let destination = tempfile::tempdir().unwrap();
    let source_path = database_path(source.path());
    let target = database_path(destination.path());
    fs::copy(&source_path, &target).unwrap();
    fs::copy(companion(&source_path, "-wal"), companion(&target, "-wal")).unwrap();
    assert!(!companion(&target, "-shm").exists());
    let main = fs::read(&target).unwrap();
    let wal = fs::read(companion(&target, "-wal")).unwrap();
    let result = inspect(destination.path(), None);
    assert_eq!(result.reason_code, "wal-active");
    assert_eq!(
        result.evidence.metadata_rows, None,
        "must not report a false empty main-file snapshot"
    );
    assert!(!companion(&target, "-shm").exists());
    assert_eq!(fs::read(&target).unwrap(), main);
    assert_eq!(fs::read(companion(&target, "-wal")).unwrap(), wal);
    // Even existing coordination bytes are left untouched, not parsed/repaired.
    fs::write(
        companion(&target, "-shm"),
        b"original-coordination-evidence",
    )
    .unwrap();
    assert_eq!(inspect(destination.path(), None).reason_code, "wal-active");
    assert_eq!(
        fs::read(companion(&target, "-shm")).unwrap(),
        b"original-coordination-evidence"
    );
    drop(writer);
}

#[cfg(unix)]
#[test]
fn directory_query_markers_cannot_inject_mutating_sqlite_uri_options() {
    let dir = tempfile::Builder::new()
        .prefix("readonly ?mode=rw#snapshot ")
        .tempdir()
        .unwrap();
    load_domain_data(dir.path()).unwrap();
    assert_eq!(inspect(dir.path(), None).state, "metadata-readable");
    assert!(!companion(&database_path(dir.path()), "-wal").exists());
    assert!(!companion(&database_path(dir.path()), "-shm").exists());
}
#[test]
fn candidate_is_not_decoded_and_pending_barrier_stays_in_database() {
    let dir = database();
    // Intentionally not a valid ProxyConfigProfile: inspection must not claim
    // candidate validity or parse source credentials to classify metadata.
    row(
        dir.path(),
        1,
        false,
        r#"{"secret":"PRIVATE_CANDIDATE_BODY"}"#,
    );
    let original = bytes(dir.path());
    let result = inspect(dir.path(), None);
    assert_eq!(result.reason_code, "metadata-readable");
    assert_eq!(result.evidence.pending_rows, Some(1));
    let wire = serde_json::to_string(&result).unwrap();
    for secret in [
        "PRIVATE_CANDIDATE_BODY",
        "profile-1",
        "original-core",
        &dir.path().to_string_lossy(),
    ] {
        assert!(!wire.contains(secret));
    }
    assert_eq!(bytes(dir.path()), original);
    let c = Connection::open(database_path(dir.path())).unwrap();
    assert_eq!(
        c.query_row(
            "SELECT count(*) FROM workspace_recovery WHERE stage='prepared'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}
#[test]
fn broken_metadata_is_fixed_classification_without_echo_or_cleanup() {
    let dir = database();
    row(dir.path(), 1, true, "{}");
    let original = bytes(dir.path());
    let result = inspect(dir.path(), None);
    assert_eq!(result.reason_code, "invalid-metadata");
    assert_eq!(result.evidence.pending_rows, None);
    let wire = serde_json::to_string(&result).unwrap();
    assert!(!wire.contains("SECRET_TOKEN"));
    assert!(!wire.contains("private.invalid"));
    assert_eq!(bytes(dir.path()), original);
}
#[test]
fn mismatched_redundant_identity_is_not_a_trusted_empty_or_valid_ledger() {
    let dir = database();
    row(dir.path(), 1, false, "{}");
    Connection::open(database_path(dir.path()))
        .unwrap()
        .execute("UPDATE workspace_recovery SET profile_id='different'", [])
        .unwrap();
    assert_eq!(inspect(dir.path(), None).reason_code, "invalid-metadata");
}
#[test]
fn metadata_rows_are_bounded_without_pruning_original_records() {
    let dir = database();
    for id in 0..65 {
        row(dir.path(), id, false, "{}");
    }
    let original = bytes(dir.path());
    let result = inspect(dir.path(), None);
    assert_eq!(result.reason_code, "inspection-budget");
    assert_eq!(result.evidence.metadata_rows, None);
    assert_eq!(bytes(dir.path()), original);
}
#[test]
fn invalid_container_and_future_schema_are_preserved() {
    let dir = tempfile::tempdir().unwrap();
    let path = database_path(dir.path());
    fs::write(&path, b"SECRET_INVALID_SQLITE").unwrap();
    assert_eq!(inspect(dir.path(), None).reason_code, "invalid-container");
    assert_eq!(fs::read(&path).unwrap(), b"SECRET_INVALID_SQLITE");
    let dir = database();
    Connection::open(database_path(dir.path()))
        .unwrap()
        .pragma_update(None, "user_version", SCHEMA_VERSION + 1)
        .unwrap();
    let original = bytes(dir.path());
    assert_eq!(inspect(dir.path(), None).reason_code, "unsupported-schema");
    assert_eq!(bytes(dir.path()), original);
}
#[test]
fn foreign_application_is_not_migrated() {
    let dir = database();
    Connection::open(database_path(dir.path()))
        .unwrap()
        .pragma_update(None, "application_id", 42)
        .unwrap();
    let original = bytes(dir.path());
    assert_eq!(inspect(dir.path(), None).reason_code, "wrong-application");
    assert_eq!(bytes(dir.path()), original);
}
#[test]
fn forged_view_or_generated_column_is_not_evaluated() {
    for generated in [false, true] {
        let dir = database();
        let c = Connection::open(database_path(dir.path())).unwrap();
        c.execute_batch("DROP TABLE workspace_recovery").unwrap();
        if generated {
            c.execute_batch("CREATE TABLE workspace_recovery(operation_id TEXT,profile_id TEXT,stage TEXT,version INTEGER,metadata_json TEXT GENERATED ALWAYS AS (hex(zeroblob(999999999))) VIRTUAL,candidate_json TEXT)").unwrap();
        } else {
            c.execute_batch(
                "CREATE VIEW workspace_recovery AS SELECT hex(zeroblob(999999999)) AS operation_id",
            )
            .unwrap();
        }
        drop(c);
        assert_eq!(inspect(dir.path(), None).reason_code, "untrusted-structure");
    }
}
#[test]
fn query_only_connection_refuses_mutation_and_sql_budget_interrupts_work() {
    let dir = database();
    let c =
        Connection::open_with_flags(database_path(dir.path()), OpenFlags::SQLITE_OPEN_READ_ONLY)
            .unwrap();
    configure_readonly(&c).unwrap();
    assert!(c.execute("DELETE FROM workspace_recovery", []).is_err());
    let error=c.query_row::<i64,_,_>("WITH RECURSIVE n(x) AS (VALUES(1) UNION ALL SELECT x+1 FROM n WHERE x<100000000) SELECT sum(x) FROM n",[],|r|r.get(0)).unwrap_err();
    assert_eq!(
        error.sqlite_error_code(),
        Some(ErrorCode::OperationInterrupted)
    );
}
#[cfg(unix)]
#[test]
fn symlink_and_real_permission_denial_are_readonly_failures() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let dir = database();
    let target = database_path(dir.path());
    let alternate = dir.path().join("original.db");
    fs::rename(&target, &alternate).unwrap();
    symlink(&alternate, &target).unwrap();
    assert_eq!(inspect(dir.path(), None).reason_code, "unsafe-file");
    fs::remove_file(&target).unwrap();
    fs::rename(&alternate, &target).unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o0)).unwrap();
    let result = inspect(dir.path(), None);
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(result.reason_code, "permission-denied");
}
#[test]
fn readonly_reopen_child() {
    let Some(path) = std::env::var_os("ZNET_WORKSPACE_INSPECTION_CHILD") else {
        return;
    };
    let dir = std::path::PathBuf::from(path);
    let result = inspect(&dir, None);
    assert_eq!(result.evidence.pending_rows, Some(1));
    assert!(result.barriers_retained && result.read_only);
}
#[test]
fn real_process_reopen_keeps_original_unknown_barrier_and_database_bytes() {
    let dir = database();
    row(dir.path(), 1, false, "{}");
    let original = bytes(dir.path());
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .arg("services::app_database::inspection::tests::readonly_reopen_child")
        .arg("--exact")
        .env("ZNET_WORKSPACE_INSPECTION_CHILD", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(bytes(dir.path()), original);
    assert_eq!(inspect(dir.path(), None).evidence.pending_rows, Some(1));
}
