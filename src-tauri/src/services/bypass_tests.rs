use super::*;
use crate::models::app_config::ClientKernelSettings;

#[test]
fn legacy_lists_merge_once_and_removed_rules_never_reappear_from_projections() {
    let mut app = AppConfig::default();
    app.tun.exclude_cidrs = vec!["16.0.0.0/8".into()];
    app.local_proxy.bypass.push("*.example.org".into());
    normalize(&mut app).unwrap();
    let policy = app.bypass.as_ref().unwrap();
    assert!(policy.local_networks);
    assert!(policy.rules.contains(&"16.0.0.0/8".into()));
    assert!(policy.rules.contains(&"*.example.org".into()));
    let snapshot = app.clone();
    normalize(&mut app).unwrap();
    assert_eq!(app, snapshot);
    app.bypass.as_mut().unwrap().rules.clear();
    normalize(&mut app).unwrap();
    assert!(!app.tun.exclude_cidrs.contains(&"16.0.0.0/8".into()));
    assert!(!app.local_proxy.bypass.contains(&"*.example.org".into()));
}

#[test]
fn explicit_empty_legacy_exceptions_do_not_enable_private_network_bypass() {
    let mut app = AppConfig::default();
    app.local_proxy.bypass.clear();
    normalize(&mut app).unwrap();
    assert!(!app.bypass.as_ref().unwrap().local_networks);
    assert!(app.tun.exclude_cidrs.is_empty());
}

#[test]
fn arbitrary_networks_are_never_widened_to_native_wildcards() {
    let mut app = AppConfig::default();
    app.bypass = Some(AppBypassConfig {
        local_networks: false,
        rules: vec![
            "192.168.0.129/25".into(),
            "fd00::123/64".into(),
            "10.*".into(),
            "*.example.org".into(),
        ],
    });
    normalize(&mut app).unwrap();
    assert_eq!(
        app.tun.exclude_cidrs,
        ["192.168.0.128/25", "fd00::/64", "10.0.0.0/8"]
    );
    assert_eq!(app.local_proxy.bypass, ["10.*", "*.example.org"]);
    let mut runtime = json!({"mode":{"type":"global","outbound":"proxy"},"route":{"final":{"type":"direct"}},"runtime":{"tun":{"exclude_cidrs":["16.0.0.0/8"]},"dns":{"servers":{"system":{"type":"system"}},"default_server":"system"}}});
    apply(&mut runtime, &app).unwrap();
    assert_eq!(runtime["mode"]["type"], "global");
    assert_eq!(runtime["route"]["bypass"].as_array().unwrap().len(), 4);
    assert_eq!(runtime["runtime"]["tun"]["exclude_cidrs"][0], "16.0.0.0/8");
    assert_eq!(runtime["runtime"]["dns"]["dispatch"][0]["server"], "system");
    assert!(runtime["runtime"]["dns"]["reverse_mapping"].is_object());
}

#[test]
fn portable_settings_include_the_authoritative_policy() {
    let mut app = AppConfig::default();
    normalize(&mut app).unwrap();
    let portable = ClientKernelSettings::from_app_config(&app);
    let value = serde_json::to_value(&portable).unwrap();
    assert!(value["bypass"]["localNetworks"].as_bool().unwrap());
    let portable: ClientKernelSettings = serde_json::from_value(value).unwrap();
    let mut restored = AppConfig::default();
    portable.apply_to(&mut restored);
    normalize(&mut restored).unwrap();
    assert_eq!(restored.bypass, app.bypass);
    assert_eq!(restored.tun.exclude_cidrs, app.tun.exclude_cidrs);
}

#[test]
fn inactive_tun_family_is_omitted_without_losing_the_authoritative_rules() {
    let mut app = AppConfig::default();
    app.tun.dual_stack = false;
    normalize(&mut app).unwrap();
    assert!(app.tun.exclude_cidrs.iter().all(|cidr| !cidr.contains(':')));
    let mut profile = json!({"route":{"final":{"type":"direct"}},"runtime":{"tun":{"addr":"fd66::1/64","dual_stack":false}}});
    apply(&mut profile, &app).unwrap();
    assert!(profile["runtime"]["tun"]["exclude_cidrs"]
        .as_array()
        .unwrap()
        .iter()
        .all(|cidr| cidr.as_str().unwrap().contains(':')));
    app.tun.dual_stack = true;
    normalize(&mut app).unwrap();
    assert!(app.tun.exclude_cidrs.contains(&"fc00::/7".into()));
}

#[test]
fn version_one_portable_tun_exclusions_merge_with_current_proxy_bypass() {
    let mut legacy = AppConfig::default();
    legacy.tun.exclude_cidrs = vec!["16.0.0.0/8".into()];
    let mut settings =
        serde_json::to_value(ClientKernelSettings::from_app_config(&legacy)).unwrap();
    settings.as_object_mut().unwrap().remove("bypass");
    let input = json!({
        "schemaVersion": "znet.client-kernel-settings.v1",
        "exportedAtUnixMs": 1,
        "settings": settings
    });
    // Version one portable settings did not include localProxy. Migration
    // combines the imported TUN exclusions with this machine's proxy list.
    let mut current = AppConfig::default();
    current.local_proxy.bypass = vec!["*.example.org".into()];
    let imported =
        crate::services::kernel_settings::import_from_str(&current, &input.to_string()).unwrap();
    let policy = imported.bypass.unwrap();
    assert!(!policy.local_networks);
    assert_eq!(policy.rules, ["*.example.org", "16.0.0.0/8"]);
}
