use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_capabilities::files::{self, Publication, Selection};
use znet_client_core::capability::*;
#[test]
fn selected_file_scope_does_not_authorize_another_selection_or_oversized_input() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("input");
    std::fs::write(&path, b"secret").unwrap();
    let selected = Selection::from_user_path(path.clone());
    let foreign = Selection::from_user_path(path);
    let grants = BTreeSet::from([selected.permission()]);
    let manager = Manager::default();
    let policy = manager
        .admit(
            "builtin.import".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .unwrap();
    policy.authorize(grants, Duration::from_secs(30)).unwrap();
    let lease = policy
        .begin(
            Budget {
                calls: 3,
                resource_bytes: 32,
                timeout: Duration::from_secs(3),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    assert!(matches!(
        files::read(&lease, &foreign, 32),
        Err(Error::PermissionDenied)
    ));
    assert!(matches!(
        files::read(&lease, &selected, 2),
        Err(Error::BudgetExceeded)
    ));
    assert_eq!(
        files::read(&lease, &selected, 32)
            .unwrap()
            .take(&lease)
            .unwrap(),
        b"secret"
    );
    assert!(!format!("{:?}", manager.operations()).contains("secret"));
}

#[test]
fn publication_is_scoped_atomic_and_replaces_only_its_target() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("artifact");
    std::fs::write(&path, b"old").unwrap();
    let target = Publication::from_host_path(path.clone());
    let foreign = Publication::from_host_path(directory.path().join("foreign"));
    let grants = BTreeSet::from([target.permission()]);
    let manager = Manager::default();
    let policy = manager
        .admit(
            "builtin.publisher".into(),
            grants.clone(),
            grants.clone(),
            &grants,
        )
        .unwrap();
    policy.authorize(grants, Duration::from_secs(30)).unwrap();
    let lease = policy
        .begin(
            Budget {
                calls: 2,
                resource_bytes: 64,
                timeout: Duration::from_secs(3),
            },
            Arc::new(AtomicBool::new(false)),
        )
        .unwrap();
    assert!(matches!(
        files::publish(&lease, &foreign, b"foreign", 64),
        Err(Error::PermissionDenied)
    ));
    let published = files::publish(&lease, &target, b"new", 64)
        .unwrap()
        .take(&lease)
        .unwrap();
    assert_eq!(published, path);
    assert_eq!(std::fs::read(published).unwrap(), b"new");
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
