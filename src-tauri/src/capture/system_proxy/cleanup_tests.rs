use super::clear_at;
use crate::errors::AppError;
use std::cell::Cell;

#[test]
fn stale_marker_clears_instead_of_reinstating_previous_local_proxy() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proxy.json");
    // An old backup itself points at a local proxy. It must never be used.
    let marker = r#"{"host":"127.0.0.1","port":7890,"enabledAtUnixMs":1,"previous":{"enabled":true,"host":"127.0.0.1","port":1080}}"#;
    std::fs::write(&path, marker).unwrap();
    clear_at(path.clone(), None, |endpoints| {
        assert_eq!(endpoints, &[("127.0.0.1".to_string(), 7890)]);
        assert!(path.exists(), "retain ownership until cleanup succeeds");
        Err(AppError::internal("permission denied"))
    })
    .unwrap_err();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), marker);
    let cleared = Cell::new(false);
    clear_at(path.clone(), None, |endpoints| {
        assert_eq!(endpoints.len(), 1);
        cleared.set(true);
        Ok(())
    })
    .unwrap();
    assert!(cleared.get());
    assert!(!path.exists());
}

#[test]
fn native_cleanup_success_retires_the_historical_marker() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proxy.json");
    std::fs::write(
        &path,
        r#"{"host":"127.0.0.1","port":7890,"enabledAtUnixMs":1}"#,
    )
    .unwrap();
    clear_at(path.clone(), None, |endpoints| {
        assert_eq!(endpoints, &[("127.0.0.1".to_string(), 7890)]);
        Ok(())
    })
    .unwrap();
    assert!(!path.exists());
}

#[test]
fn markerless_cleanup_requires_a_matching_owned_listener() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("absent.json");
    clear_at(path.clone(), None, |_| panic!("no ownership to clear")).unwrap();
    let cleared = Cell::new(false);
    clear_at(path, Some(("127.0.0.1".into(), 7877)), |endpoints| {
        assert_eq!(endpoints.len(), 1);
        cleared.set(true);
        Ok(())
    })
    .unwrap();
    assert!(cleared.get());
}
