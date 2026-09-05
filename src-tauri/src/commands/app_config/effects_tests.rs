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
