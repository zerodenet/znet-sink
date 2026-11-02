use super::*;
use crate::services::{app_config_store, bypass};

#[test]
fn cold_start_clears_a_stale_on_intent_before_auto_connect_and_preserves_parameters() {
    for enabled in [Some(true), None, Some(false)] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app-config.json");
        let mut previous = AppConfig::default();
        previous.tun.enabled = enabled;
        previous.tun.addr = "10.77.0.1/24".into();
        previous.tun.mask = "255.255.255.0".into();
        previous.tun.mtu = 1280;
        previous.core.auto_connect = true;
        bypass::normalize(&mut previous).unwrap();
        app_config_store::save(&path, &previous).unwrap();
        // Simulate an abrupt exit: no shutdown callback ran before this load.
        let mut admitted = app_config_store::load_or_default(&path).unwrap();
        begin_lifetime(&mut admitted, &path).unwrap();
        let mut expected = previous;
        expected.tun.enabled = Some(false);
        assert_eq!(admitted, expected);
        assert_eq!(app_config_store::load_or_default(&path).unwrap(), expected);
    }
}

#[test]
fn failed_startup_persistence_never_exposes_stale_on_intent_to_the_new_gui() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = AppConfig::default();
    config.tun.enabled = Some(true);
    assert!(begin_lifetime(&mut config, dir.path()).is_err());
    assert_eq!(config.tun.enabled, Some(false));
}
