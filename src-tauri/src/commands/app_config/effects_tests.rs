use super::*;

#[test]
fn changing_the_listening_port_updates_runtime_and_system_proxy_together() {
    let old = AppConfig::default();
    let mut next = old.clone();
    next.local_proxy.port += 1;
    let effects = between(&old, &next);
    assert!(effects.recompose && effects.retarget_proxy && !effects.restart);
}

#[test]
fn launch_preference_is_not_a_request_to_stop_the_running_kernel() {
    let old = AppConfig::default();
    let mut next = old.clone();
    next.core.auto_start = !old.core.auto_start;
    let effects = between(&old, &next);
    assert!(!effects.restart && !effects.recompose && !effects.retarget_proxy);
    next.core.executable_path = Some("replacement".into());
    assert!(between(&old, &next).restart);
}

#[test]
fn routing_preferences_require_runtime_recomposition() {
    let old = AppConfig::default();
    let mut next = old.clone();
    next.routing.inject_common_rules = !old.routing.inject_common_rules;
    assert!(between(&old, &next).recompose);
}

#[test]
fn unified_bypass_updates_both_consumers_and_rebuilds_tun_only_for_changed_networks() {
    let mut old = AppConfig::default();
    crate::services::bypass::normalize(&mut old).unwrap();
    let mut next = old.clone();
    next.bypass
        .as_mut()
        .unwrap()
        .rules
        .push("*.example.org".into());
    crate::services::bypass::normalize(&mut next).unwrap();
    let changes = between(&old, &next);
    assert!(changes.recompose && changes.retarget_proxy && !changes.restart);
    next.bypass
        .as_mut()
        .unwrap()
        .rules
        .push("16.0.0.0/8".into());
    crate::services::bypass::normalize(&mut next).unwrap();
    let changes = between(&old, &next);
    assert!(changes.restart && changes.recompose && changes.retarget_proxy);
}

#[test]
fn network_changes_rebuild_profile_owned_tun_even_if_its_family_differs_from_app_defaults() {
    let mut old = AppConfig::default();
    old.tun.dual_stack = false;
    crate::services::bypass::normalize(&mut old).unwrap();
    let mut next = old.clone();
    next.bypass
        .as_mut()
        .unwrap()
        .rules
        .push("2001:db8::/32".into());
    crate::services::bypass::normalize(&mut next).unwrap();
    assert_eq!(old.tun.exclude_cidrs, next.tun.exclude_cidrs);
    assert!(between(&old, &next).restart);
}

#[test]
fn changing_public_probe_url_recomposes_without_restarting_or_retargeting_proxy() {
    let old = AppConfig::default();
    let mut next = old.clone();
    next.url_test.url = "https://probe.example/204".into();
    let effects = between(&old, &next);
    assert!(effects.recompose && !effects.restart && !effects.retarget_proxy);
}

#[test]
fn profile_port_edit_and_reset_retarget_proxy_without_restarting_capture() {
    let old = AppConfig::default();
    let next = crate::configuration::local_edits::candidate(
        &old,
        "profile",
        std::collections::BTreeMap::from([("localProxy.port".into(), serde_json::json!(7877))]),
        &[],
    )
    .unwrap();
    for effects in [between(&old, &next), between(&next, &old)] {
        assert!(effects.recompose && effects.retarget_proxy && !effects.restart);
    }
}
