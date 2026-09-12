use std::{
    collections::BTreeSet,
    sync::{atomic::AtomicBool, Arc},
    time::Duration,
};
use znet_client_capabilities::files::{self, Selection};
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
