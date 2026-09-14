use super::*;
use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
};
use znet_client_core::capability::{Budget, Manager};
fn lease(manager: &Manager, identity: &str) -> Lease {
    let grants = BTreeSet::from([
        Permission::new("storage.read", "self"),
        Permission::new("storage.write", "self"),
    ]);
    let policy = manager
        .admit(identity.into(), grants.clone(), grants.clone(), &grants)
        .unwrap();
    policy.authorize(grants, Duration::from_secs(30)).unwrap();
    policy
        .begin(
            Budget {
                calls: 100,
                resource_bytes: 1024 * 1024,
                timeout: Duration::from_secs(20),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap()
}
fn put(key: &str, expected: Option<i64>, value: &[u8]) -> Change {
    Change::Put {
        key: key.into(),
        expected,
        value: value.into(),
    }
}
#[test]
fn namespace_isolation_restart_and_stale_write_protection() {
    let dir = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let a = lease(&manager, "builtin.a");
    let b = lease(&manager, "builtin.b");
    let rev = change(dir.path(), &a, &[put("key", None, b"canary")])
        .unwrap()
        .take(&a)
        .unwrap()[0];
    assert!(get(dir.path(), &b, "key")
        .unwrap()
        .take(&b)
        .unwrap()
        .is_none());
    assert!(matches!(
        change(dir.path(), &a, &[put("key", None, b"overwrite")]),
        Err(Error::Busy)
    ));
    change(
        dir.path(),
        &a,
        &[Change::Delete {
            key: "key".into(),
            expected: rev,
        }],
    )
    .unwrap()
    .take(&a)
    .unwrap();
    change(dir.path(), &a, &[put("key", None, b"new")])
        .unwrap()
        .take(&a)
        .unwrap();
    assert!(matches!(
        change(dir.path(), &a, &[put("key", Some(rev), b"stale")]),
        Err(Error::Busy)
    ));
    let fresh = lease(&Manager::default(), "builtin.a");
    assert_eq!(
        get(dir.path(), &fresh, "key")
            .unwrap()
            .take(&fresh)
            .unwrap()
            .unwrap()
            .value,
        b"new"
    );
    assert!(!format!("{:?}", manager.operations()).contains("canary"));
}
#[test]
fn quota_failure_rolls_back_entire_batch_and_revocation_blocks_access() {
    let dir = tempfile::tempdir().unwrap();
    let manager = Manager::default();
    let a = lease(&manager, "builtin.a");
    let batch: Vec<_> = (0..17)
        .map(|n| put(&n.to_string(), None, &vec![1; 65536]))
        .collect();
    assert!(matches!(
        change(dir.path(), &a, &batch),
        Err(Error::BudgetExceeded)
    ));
    assert!(get(dir.path(), &a, "0")
        .unwrap()
        .take(&a)
        .unwrap()
        .is_none());
    assert!(matches!(
        change(
            dir.path(),
            &a,
            &[put("ok", None, b"value"), put("bad", None, &vec![0; 65537])]
        ),
        Err(Error::BudgetExceeded)
    ));
    assert!(get(dir.path(), &a, "ok")
        .unwrap()
        .take(&a)
        .unwrap()
        .is_none());
    let grants = BTreeSet::from([Permission::new("storage.read", "self")]);
    let policy = manager
        .admit("revoked".into(), grants.clone(), grants.clone(), &grants)
        .unwrap();
    policy.authorize(grants, Duration::from_secs(30)).unwrap();
    let access = policy
        .begin(
            Budget {
                calls: 1,
                resource_bytes: 100,
                timeout: Duration::from_secs(10),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    policy.revoke();
    assert!(matches!(
        get(dir.path(), &access, "ok"),
        Err(Error::Revoked)
    ));
}
#[test]
fn version_one_database_is_upgraded_without_changing_existing_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let connection = Connection::open(database_path(dir.path())).unwrap();
    connection.execute_batch(SCHEMA_V1).unwrap();
    connection
        .execute(
            "INSERT INTO app_metadata VALUES('legacy_json_imported','1')",
            [],
        )
        .unwrap();
    connection.pragma_update(None, "user_version", 1).unwrap();
    drop(connection);
    let manager = Manager::default();
    let a = lease(&manager, "builtin.a");
    change(dir.path(), &a, &[put("retry-state", None, b"{}")])
        .unwrap()
        .take(&a)
        .unwrap();
    let connection = open(dir.path()).unwrap();
    assert_eq!(
        connection
            .pragma_query_value(None, "user_version", |r| r.get::<_, i32>(0))
            .unwrap(),
        2
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT value FROM app_metadata WHERE key='legacy_json_imported'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "1"
    );
}

#[test]
fn concurrent_writers_cannot_both_replace_the_same_revision() {
    let root = tempfile::tempdir().unwrap();
    let access = lease(&Manager::default(), "builtin.a");
    let revision = change(root.path(), &access, &[put("key", None, b"initial")])
        .unwrap()
        .take(&access)
        .unwrap()[0];
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let workers: Vec<_> = (0..2)
        .map(|n| {
            let root = root.path().to_path_buf();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                let access = lease(&Manager::default(), "builtin.a");
                barrier.wait();
                change(&root, &access, &[put("key", Some(revision), &[n])])
                    .and_then(|v| v.take(&access))
            })
        })
        .collect();
    let results: Vec<_> = workers.into_iter().map(|w| w.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|r| matches!(r, Err(Error::Busy)))
            .count(),
        1
    );
}
