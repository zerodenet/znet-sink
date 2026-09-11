use super::*;
use crate::configuration::composition::{finalize, Inputs};
use crate::models::app_config::ConfigOverrides;
use std::collections::BTreeMap;

fn compose(source: &Value, app: AppConfig) -> Value {
    finalize(
        source,
        source.clone(),
        &Inputs {
            source_profile_id: None,
            app,
            supports_tolerance: true,
            selections: BTreeMap::new(),
        },
    )
    .unwrap()
    .config
}

#[test]
fn explicit_source_capabilities_survive_defaults_and_empty_values() {
    let source = json!({
        "inbounds":[{"tag":"entry","listen":{"address":"127.0.0.2","port":7891},"protocol":{"type":"mixed"}}],
        "runtime":{"dns":{"servers":{},"dispatch":[]},"latency_test_url":"https://source.test/204"},
        "route":{"rules":[],"bypass":[]},
        "outbound_groups":[
            {"tag":"a","type":"url_test","url":"https://group.test/204","tolerance_ms":0},
            {"tag":"b","type":"url_test"}
        ]
    });
    let before = source.clone();
    let app = AppConfig::default();
    let effective = compose(&source, app.clone());
    for path in [
        "/inbounds",
        "/runtime/dns",
        "/runtime/latency_test_url",
        "/route",
        "/outbound_groups/0",
    ] {
        assert_eq!(effective.pointer(path), source.pointer(path), "{path}");
    }
    assert_eq!(
        effective["outbound_groups"][1]["url"],
        "https://source.test/204"
    );
    assert_eq!(
        endpoint_for(&app, &source).unwrap(),
        ("127.0.0.2".into(), 7891)
    );
    assert_eq!(source, before);
}

#[test]
fn missing_entry_is_filled_but_explicit_empty_or_zero_is_not() {
    let mut app = AppConfig::default();
    app.dns.enabled = false;
    assert_eq!(endpoint_for(&app, &json!({})).unwrap().1, 7890);
    let empty = compose(&json!({"inbounds":[]}), app.clone());
    assert_eq!(empty["inbounds"], json!([]));
    assert!(endpoint_for(&app, &empty).is_err());
    let partial =
        json!({"inbounds":[{"tag":"custom","protocol":{"type":"mixed"},"listen":{"port":7891}}]});
    assert_eq!(endpoint_for(&app, &partial).unwrap().1, 7891);
    let zero = compose(
        &json!({"inbounds":[{"protocol":{"type":"mixed"},"listen":{"port":0}}]}),
        app,
    );
    assert_eq!(zero["inbounds"][0]["listen"]["port"], 0);
}

#[test]
fn tun_source_parameters_are_used_only_by_explicit_capture_start() {
    let mut app = AppConfig::default();
    app.dns.enabled = false;
    let source = json!({"runtime":{"tun":{"addr":"10.80.0.1/24","mtu":1300,"auto_route":false,
        "strict_route":false,"dual_stack":false,"dns_hijack":false,"exclude_cidrs":[],"include_cidrs":[]}}});
    let params = tun_params_for(&app, &source, app.tun.clone()).unwrap();
    for key in [
        "addr",
        "mtu",
        "auto_route",
        "strict_route",
        "dual_stack",
        "dns_hijack",
        "exclude_cidrs",
        "include_cidrs",
    ] {
        assert_eq!(params[key], source["runtime"]["tun"][key], "{key}");
    }
    assert!(compose(&source, app.clone())
        .pointer("/runtime/tun")
        .is_none());
    let mut overridden = app.clone();
    overridden.overrides.tun = true;
    assert_eq!(
        tun_params_for(&overridden, &source, app.tun.clone()).unwrap()["addr"],
        app.tun.addr
    );
}

#[test]
fn selecting_one_override_does_not_replace_other_capabilities() {
    let mut app = AppConfig::default();
    app.overrides.listener = true;
    let source = json!({"inbounds":[{"tag":"entry","protocol":{"type":"mixed"},"listen":{"port":7891}}],
        "runtime":{"dns":{},"latency_test_url":"https://source.test/204"},"route":{"bypass":[]}});
    let effective = compose(&source, app);
    assert_eq!(effective["inbounds"][0]["listen"]["port"], 7890);
    assert_eq!(effective["runtime"], source["runtime"]);
    assert_eq!(effective["route"], source["route"]);
    assert_eq!(
        serde_json::from_value::<AppConfig>(json!({}))
            .unwrap()
            .overrides,
        ConfigOverrides::default()
    );
}

#[test]
fn portable_overrides_round_trip_and_older_bundles_default_to_source_priority() {
    use crate::models::app_config::{ClientKernelSettings, CLIENT_KERNEL_SETTINGS_SCHEMA};
    let mut app = AppConfig::default();
    app.overrides.dns = true;
    app.overrides.url_test = true;
    let mut bundle = json!({"schemaVersion":CLIENT_KERNEL_SETTINGS_SCHEMA,"exportedAtUnixMs":0,
        "settings":ClientKernelSettings::from_app_config(&app)});
    let restored =
        crate::services::kernel_settings::import_from_str(&app, &bundle.to_string()).unwrap();
    assert_eq!(restored.overrides, app.overrides);
    bundle["schemaVersion"] = json!("znet.client-kernel-settings.v2");
    bundle["settings"]
        .as_object_mut()
        .unwrap()
        .remove("overrides");
    let migrated =
        crate::services::kernel_settings::import_from_str(&app, &bundle.to_string()).unwrap();
    assert_eq!(migrated.overrides, ConfigOverrides::default());
}

#[cfg(feature = "tool-node-probe")]
#[test]
fn manual_probe_uses_profile_public_url_and_native_proxy_does_not_add_client_exceptions() {
    let mut app = AppConfig::default();
    app.url_test.url = "https://client.test/204".into();
    let profile = crate::models::proxy_config::ProxyConfigProfile {
        id: "source".into(),
        name: "source".into(),
        kernel: "zero".into(),
        format: "zero-json".into(),
        path: None,
        active: true,
        updated_at_unix_ms: 0,
        capabilities: Default::default(),
        content: Some(
            json!({"runtime":{"latency_test_url":"https://source.test/204"},"route":{"bypass":[]},
            "inbounds":[{"listen":{"address":"127.0.0.1","port":7891},"protocol":{"type":"mixed"}}]}),
        ),
    };
    let state = AppState::with_domain_data(app, vec![profile], vec![], vec![], vec![]);
    assert_eq!(
        crate::services::url_test::configured_url(&state).unwrap(),
        "https://source.test/204"
    );
    let (host, port, exceptions) = proxy_settings(&state).unwrap();
    assert_eq!((host, port), ("127.0.0.1".into(), 7891));
    assert!(exceptions.is_empty());
}

#[test]
fn source_rules_with_client_looking_tags_are_not_removed() {
    let mut app = AppConfig::default();
    app.dns.enabled = false;
    let state = AppState::with_domain_data(app, vec![], vec![], vec![], vec![]);
    let source = json!({"route":{
        "rule_sets":[{"tag":"gui-common-source","path":"rules.zrs"}],
        "rules":[{"condition":{"type":"rule_set","tag":"gui-common-source"},"action":{"type":"reject"}}],
        "final":{"type":"direct"}
    }});
    let effective =
        crate::services::rule_overlay::compose_effective_config(&state, &source).unwrap();
    assert_eq!(effective["route"]["rules"], source["route"]["rules"]);
    assert_eq!(
        effective["route"]["rule_sets"],
        source["route"]["rule_sets"]
    );
}
