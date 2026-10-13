use super::{clear_at, reset_at};
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

fn status(enabled: bool) -> crate::services::system_proxy::SystemProxyStatus {
    crate::services::system_proxy::SystemProxyStatus {
        enabled,
        host: String::new(),
        port: 0,
        socks_enabled: false,
        socks_host: String::new(),
        socks_port: 0,
    }
}

#[test]
fn explicit_reset_works_without_or_with_corrupt_ownership_record() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proxy.json");
    for corrupt in [false, true] {
        if corrupt {
            std::fs::write(&path, "corrupt").unwrap();
        }
        let called = Cell::new(false);
        let actual = reset_at(path.clone(), || {
            called.set(true);
            Ok(status(false))
        })
        .unwrap();
        assert!(called.get());
        assert!(!actual.enabled);
        assert!(!path.exists());
    }
}

#[test]
fn explicit_reset_retains_record_on_failure_or_unconfirmed_clear() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("proxy.json");
    std::fs::write(&path, "ownership evidence").unwrap();
    reset_at(path.clone(), || {
        Err(AppError::internal("authorization cancelled"))
    })
    .unwrap_err();
    assert!(path.exists());
    reset_at(path.clone(), || Ok(status(true))).unwrap_err();
    assert!(path.exists());
    let mut socks = status(false);
    socks.socks_enabled = true;
    reset_at(path.clone(), || Ok(socks)).unwrap_err();
    assert!(path.exists());
    reset_at(path.clone(), || Ok(status(false))).unwrap();
    assert!(!path.exists());
}
