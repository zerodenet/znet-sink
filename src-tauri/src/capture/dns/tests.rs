use super::preflight::explicit_upstreams;
use super::*;
use serde_json::json;

fn tun(addr: &str) -> GuiTunStatus {
    GuiTunStatus {
        enabled: true,
        healthy: true,
        auto_route: true,
        dns_hijack: true,
        name: Some("utun6".into()),
        addresses: vec![addr.into()],
        ..Default::default()
    }
}
#[test]
fn capture_requires_actual_healthy_automatic_tun_and_dns_interception() {
    let mut status = tun("10.66.0.1/24");
    assert_eq!(target(&status, 42).unwrap().unwrap().server, "10.66.0.2");
    status.enabled = false;
    assert!(target(&status, 42).unwrap().is_none());
    status.enabled = true;
    status.dns_hijack = false;
    assert!(target(&status, 42).unwrap().is_none());
    status.dns_hijack = true;
    status.auto_route = false;
    assert!(target(&status, 42).unwrap().is_none());
    status.auto_route = true;
    status.healthy = false;
    assert!(target(&status, 42).is_err());
}
#[test]
fn capture_never_uses_local_network_or_broadcast_address_as_resolver() {
    for addr in [
        "10.66.0.1/32",
        "10.66.0.1/31",
        "10.66.0.254/24",
        "10.66.0.255/24",
        "fd66::1/64",
        "10.66.0.1",
    ] {
        assert!(target(&tun(addr), 42).is_err(), "{addr}");
    }
    assert!(validate_target("utun6;shutdown", "10.66.0.2", 42).is_err());
    assert!(validate_target("utun6", "127.0.0.1", 42).is_err());
    assert!(validate_target("utun6", "10.66.0.2", 0).is_err());
}
#[test]
fn redirected_host_dns_cannot_be_used_as_a_kernel_upstream() {
    assert!(explicit_upstreams(&json!({"servers":{"a":{"type":"system"}}}), "10.66.0.2").is_err());
    assert!(explicit_upstreams(
        &json!({"servers":{"a":{"type":"udp","host":"10.66.0.2"}}}),
        "10.66.0.2"
    )
    .is_err());
    assert!(explicit_upstreams(
        &json!({"servers":{"a":{"type":"doh","host":"cloudflare-dns.com"}}}),
        "10.66.0.2"
    )
    .is_ok());
    assert!(explicit_upstreams(&json!({}), "10.66.0.2").is_err());
}

#[test]
fn recommended_dns_remains_independent_after_host_capture() {
    let mut app = crate::models::app_config::AppConfig::default();
    app.dns.enabled = true;
    app.overrides.dns = true;
    let source = json!({"outbounds":[{"tag":"proxy"}],
        "route":{"final":{"type":"route","outbound":"proxy"}}});
    let dns = super::preflight::effective_dns(&app, &source).unwrap();
    explicit_upstreams(&dns, "10.66.0.2").unwrap();
    for tag in ["alidns", "114dns"] {
        assert!(dns["servers"][tag].get("detour").is_none());
    }
    assert_eq!(dns["servers"]["cloudflare"]["detour"], "proxy");
}

#[test]
fn independent_literal_doh_profile_is_preserved_and_accepted_for_capture() {
    let value = json!({
        "servers":{"alidns":{"type":"doh","host":"223.5.5.5","port":443,
            "path":"/dns-query","server_name":"dns.alidns.com"}},
        "default_server":"alidns", "dispatch":[],
        "answer":{"type":"fake_ip","cidr":"198.18.0.0/15"},
        "policy":{"address_family":"prefer_ipv4"}
    });
    let mut profile: crate::models::dns_config::ClientDnsConfig =
        serde_json::from_value(value.clone()).unwrap();
    profile.validate_client_shape().unwrap();
    let before = profile.clone();
    assert!(!profile.migrate_missing_builtin_domestic_resolvers());
    assert!(!profile.migrate_legacy_recommended_node_resolution());
    assert_eq!(profile, before);
    let app = crate::models::app_config::AppConfig::default();
    let source = json!({"runtime":{"dns":value}});
    let dns = super::preflight::effective_dns(&app, &source).unwrap();
    assert_eq!(dns, source["runtime"]["dns"]);
    explicit_upstreams(&dns, "10.66.0.2").unwrap();
}

#[test]
fn disabled_global_dns_cannot_authorize_capture_using_a_saved_configuration() {
    let mut app = crate::models::app_config::AppConfig::default();
    app.dns.enabled = false;
    assert!(app.dns.config.is_some());
    assert!(super::preflight::effective_dns(&app, &json!({})).is_err());
    let source = json!({"runtime":{"dns":{"servers":{"a":{"type":"udp","host":"1.1.1.1"}},"default_server":"a"}}});
    app.overrides.dns = false;
    assert_eq!(
        super::preflight::effective_dns(&app, &source).unwrap(),
        source["runtime"]["dns"]
    );
    app.overrides.dns = true;
    assert!(super::preflight::effective_dns(&app, &source).is_err());
}

#[test]
fn transitions_restore_before_replacement_and_keep_failed_cleanup_retryable() {
    use std::{cell::RefCell, rc::Rc};
    struct Guard {
        events: Rc<RefCell<Vec<&'static str>>>,
        fail: Rc<RefCell<bool>>,
    }
    impl CaptureGuard for Guard {
        fn refresh(&mut self, _: &Target) -> AppResult<()> {
            self.events.borrow_mut().push("refresh");
            Ok(())
        }
        fn stop(&mut self) -> AppResult<()> {
            self.events.borrow_mut().push("stop");
            if *self.fail.borrow() {
                Err(AppError::internal("cleanup failed"))
            } else {
                Ok(())
            }
        }
    }
    let events = Rc::new(RefCell::new(Vec::new()));
    let fail = Rc::new(RefCell::new(false));
    let install = || {
        let events = events.clone();
        let fail = fail.clone();
        move |_: &Target| {
            events.borrow_mut().push("install");
            Ok(Guard { events, fail })
        }
    };
    let first = target(&tun("10.66.0.1/24"), 42).unwrap();
    let second = target(&tun("10.77.0.1/24"), 43).unwrap();
    let mut state = State {
        target: None,
        guard: None,
        error: None,
    };
    transition(&mut state, first.clone(), install()).unwrap();
    transition(&mut state, first.clone(), install()).unwrap();
    *fail.borrow_mut() = true;
    assert!(transition(&mut state, second.clone(), install()).is_err());
    assert_eq!(state.target, first);
    assert!(state.guard.is_some());
    assert!(state.error.as_deref().unwrap().contains("cleanup failed"));
    *fail.borrow_mut() = false;
    transition(&mut state, second, install()).unwrap();
    transition(&mut state, None, install()).unwrap();
    assert!(state.target.is_none() && state.guard.is_none());
    assert_eq!(
        *events.borrow(),
        ["install", "refresh", "stop", "stop", "install", "stop"]
    );
}
#[test]
fn failed_installation_is_visible_and_not_repeated_without_recovery() {
    struct Guard;
    impl CaptureGuard for Guard {
        fn refresh(&mut self, _: &Target) -> AppResult<()> {
            Ok(())
        }
        fn stop(&mut self) -> AppResult<()> {
            Ok(())
        }
    }
    let mut state = State::<Guard> {
        target: None,
        guard: None,
        error: None,
    };
    let next = target(&tun("10.66.0.1/24"), 42).unwrap();
    assert!(
        transition(&mut state, next.clone(), |_| Err(AppError::internal(
            "permission denied"
        )))
        .is_err()
    );
    assert!(state
        .error
        .as_deref()
        .unwrap()
        .contains("permission denied"));
    assert!(transition(&mut state, next.clone(), |_| panic!(
        "must not re-prompt continuously"
    ))
    .is_err());
    stop(&mut state).unwrap();
    transition(&mut state, next, |_| Ok(Guard)).unwrap();
}
