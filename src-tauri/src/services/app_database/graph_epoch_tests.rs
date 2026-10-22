use super::workspace_recovery::*;
use super::*;
use std::cell::Cell;

#[test]
fn graph_fence_holds_a_real_write_lock_without_advancing_relationships() {
    let dir = tempfile::tempdir().unwrap();
    save_domain_data(dir.path(), &[], &[]).unwrap();
    let revision = current_epoch(dir.path()).unwrap();
    let expected = crate::configuration::workspace::digest(&(
        Vec::<ProxyConfigProfile>::new(),
        Vec::<SubscriptionProfile>::new(),
    ))
    .unwrap();
    let result = with_graph_epoch(dir.path(), revision, &expected, || {
        let other = Connection::open(database_path(dir.path())).unwrap();
        other.busy_timeout(Duration::ZERO).unwrap();
        let error = other.execute_batch("BEGIN IMMEDIATE").unwrap_err();
        assert!(matches!(error, rusqlite::Error::SqliteFailure(ref e, _) if e.code == rusqlite::ErrorCode::DatabaseBusy));
        Ok(42)
    }).unwrap();
    assert_eq!(result, 42);
    assert_eq!(current_epoch(dir.path()).unwrap(), revision);
    save_domain_data(dir.path(), &[], &[]).unwrap();
}

#[test]
fn graph_fence_rejects_same_graph_aba_and_wrong_graph_before_publication() {
    let dir = tempfile::tempdir().unwrap();
    save_domain_data(dir.path(), &[], &[]).unwrap();
    let revision = current_epoch(dir.path()).unwrap();
    let expected = crate::configuration::workspace::digest(&(
        Vec::<ProxyConfigProfile>::new(),
        Vec::<SubscriptionProfile>::new(),
    ))
    .unwrap();
    let called = Cell::new(false);
    assert!(with_graph_epoch(dir.path(), revision, &"a".repeat(64), || {
        called.set(true);
        Ok(())
    })
    .is_err());
    save_domain_data(dir.path(), &[], &[]).unwrap();
    assert!(with_graph_epoch(dir.path(), revision, &expected, || {
        called.set(true);
        Ok(())
    })
    .is_err());
    assert!(!called.get());
}

#[test]
fn graph_fence_does_not_create_a_missing_database_or_migrate_a_foreign_schema() {
    let dir = tempfile::tempdir().unwrap();
    let called = Cell::new(false);
    let callback = || {
        called.set(true);
        Ok(())
    };
    assert!(with_graph_epoch(dir.path(), 0, &"a".repeat(64), callback).is_err());
    assert!(!database_path(dir.path()).exists());
    let connection = Connection::open(database_path(dir.path())).unwrap();
    connection.execute_batch("PRAGMA user_version=4; CREATE TABLE original(value TEXT); INSERT INTO original VALUES('keep');").unwrap();
    assert!(with_graph_epoch(dir.path(), 0, &"a".repeat(64), callback).is_err());
    assert_eq!(
        connection
            .query_row("PRAGMA user_version", [], |r| r.get::<_, i32>(0))
            .unwrap(),
        4
    );
    assert_eq!(
        connection
            .query_row("SELECT value FROM original", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "keep"
    );
    assert!(!called.get());
}

#[test]
fn graph_fence_preserves_callback_error_and_releases_lock_without_a_commit() {
    let dir = tempfile::tempdir().unwrap();
    save_domain_data(dir.path(), &[], &[]).unwrap();
    let revision = current_epoch(dir.path()).unwrap();
    let expected = crate::configuration::workspace::digest(&(
        Vec::<ProxyConfigProfile>::new(),
        Vec::<SubscriptionProfile>::new(),
    ))
    .unwrap();
    let result: AppResult<()> = with_graph_epoch(dir.path(), revision, &expected, || {
        Err(AppError {
            code: "original_publication_error",
            message: "original".into(),
            details: None,
        })
    });
    assert_eq!(result.unwrap_err().code, "original_publication_error");
    assert_eq!(current_epoch(dir.path()).unwrap(), revision);
    save_domain_data(dir.path(), &[], &[]).unwrap();
}
