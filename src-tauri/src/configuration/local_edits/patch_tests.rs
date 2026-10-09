use super::*;
use crate::configuration::{local_edits, preferences};

fn dns(server: &str) -> Value {
    json!({"servers": {server: {"type":"system"}}, "default_server":server, "answer":{"type":"real"}})
}
fn source(port: u16, mtu: u16, server: &str) -> Value {
    json!({"inbounds":[{"tag":"main","protocol":{"type":"mixed"},"listen":{"address":"127.0.0.2","port":port}}],
        "runtime":{"dns":dns(server),"tun":{"addr":"10.77.0.1/24","mtu":mtu,"name":server}},
        "outbound_groups":[{"tag":"auto","type":"url_test","tolerance_ms":25}],"route":{"bypass":[]}})
}
fn patch(value: Value) -> AppConfigPatch {
    serde_json::from_value(value).unwrap()
}

#[test]
fn all_network_command_fields_apply_across_sources_without_freezing_unmodified_fields() {
    let original = AppConfig::default();
    let a = source(7891, 1500, "source-a");
    let b = source(7892, 1400, "source-b");
    let next = prepare_patch(
        &original,
        Some("a"),
        &a,
        patch(json!({
            "localProxy":{"port":7877},
            "tun":{"mtu":1280,"addr":"10.88.0.1/24","includeCidrs":["10.0.0.0/8"]},
            "dns":{"enabled":true,"config":dns("global"),"dnsHijack":true},
            "urlTest":{"url":"https://test.example/204","toleranceMs":0},
            "runtime":{"udpUpstreamIdleTimeoutSeconds":90},
            "routing":{"injectCommonRules":false},
            "bypass":{"localNetworks":false,"rules":[]}
        })),
    )
    .unwrap();
    for (id, base) in [("a", &a), ("b", &b)] {
        let view = settings(&next, Some(id), base).unwrap();
        assert_eq!(view["localProxy"]["port"], 7877);
        assert_eq!(view["localProxy"]["host"], "127.0.0.2");
        assert_eq!(view["tun"]["name"], format!("source-{id}"));
        assert_eq!(view["tun"]["mtu"], 1280);
        assert_eq!(view["tun"]["addr"], "10.88.0.1/24");
        assert_eq!(view["tun"]["mask"], "255.255.255.0");
        assert_eq!(view["tun"]["includeCidrs"], json!(["10.0.0.0/8"]));
        assert_eq!(view["dns"]["config"]["default_server"], "global");
        assert_eq!(view["tun"]["dnsHijack"], true);
        assert_eq!(view["urlTest"]["toleranceMs"], 0);
        assert_eq!(view["urlTest"]["url"], "https://test.example/204");
        assert_eq!(view["runtime"]["udpUpstreamIdleTimeoutSeconds"], 90);
        assert_eq!(view["routing"]["injectCommonRules"], false);
        let (app, effective) = local_edits::resolve(&next, Some(id), base).unwrap();
        let params = preferences::tun_params_for(&app, &effective, app.tun.clone()).unwrap();
        assert_eq!(params["mtu"], 1280);
        assert_eq!(params["dns_hijack"], true);
    }
    assert_eq!(original.local_proxy, next.local_proxy);
    assert_eq!(original.dns, next.dns);
    assert_eq!(original.tun, next.tun);
    assert_eq!(a, source(7891, 1500, "source-a"));
    assert_eq!(b, source(7892, 1400, "source-b"));
}

#[test]
fn restore_removes_global_dns_and_tun_instead_of_changing_the_fallback_defaults() {
    let original = AppConfig::default();
    let a = source(7891, 1500, "source-a");
    let next = prepare_patch(
        &original,
        Some("a"),
        &a,
        patch(json!({
            "tun":{"mtu":1280}, "dns":{"enabled":true,"config":dns("global")},
            "localProxy":{"port":7877}
        })),
    )
    .unwrap();
    let restored = local_edits::candidate(
        &next,
        "b",
        Default::default(),
        &["tun.mtu".into(), "dns".into(), "localProxy.port".into()],
    )
    .unwrap();
    let b = source(7892, 1400, "source-b");
    let values = settings(&restored, Some("b"), &b).unwrap();
    assert_eq!(values["tun"]["mtu"], 1400);
    assert_eq!(values["dns"]["config"]["default_server"], "source-b");
    assert_eq!(values["localProxy"]["port"], 7892);
    let defaults = settings(&restored, None, &json!({})).unwrap();
    assert_eq!(defaults["tun"]["mtu"], original.tun.mtu);
    assert_eq!(defaults["localProxy"]["port"], original.local_proxy.port);
    assert_eq!(defaults["dns"], json!(original.dns));
}

#[test]
fn partial_dns_patches_follow_saved_global_dns_and_null_explicitly_clears_it() {
    let original = AppConfig::default();
    let a = source(7891, 1500, "source-a");
    let next = prepare_patch(
        &original,
        Some("a"),
        &a,
        patch(json!({
            "dns":{"enabled":true,"config":dns("global"),"dnsHijack":false}
        })),
    )
    .unwrap();
    let b = source(7892, 1400, "source-b");
    let next = prepare_patch(
        &next,
        Some("b"),
        &b,
        patch(json!({"dns":{"dnsHijack":true}})),
    )
    .unwrap();
    let values = settings(&next, Some("b"), &b).unwrap();
    assert_eq!(values["dns"]["config"]["default_server"], "global");
    assert_eq!(values["tun"]["dnsHijack"], true);
    let next = prepare_patch(
        &next,
        Some("b"),
        &b,
        patch(json!({"dns":{"enabled":false,"config":null}})),
    )
    .unwrap();
    let values = settings(&next, Some("a"), &a).unwrap();
    assert_eq!(values["dns"]["enabled"], false);
    assert_eq!(values["dns"]["config"], Value::Null);
    assert_eq!(values["tun"]["dnsHijack"], false);
}

#[test]
fn capture_intent_and_unrelated_ui_updates_do_not_copy_source_network_fields() {
    let original = AppConfig::default();
    let a = source(7891, 1500, "source-a");
    let enabled = prepare_patch(
        &original,
        Some("a"),
        &a,
        patch(json!({"tun":{"enabled":true}})),
    )
    .unwrap();
    assert_eq!(enabled.client_edits, original.client_edits);
    assert_eq!(enabled.tun.enabled, Some(true));
    let ui = prepare_patch(
        &enabled,
        Some("a"),
        &a,
        patch(json!({"ui":{"theme":"dark"}})),
    )
    .unwrap();
    assert_eq!(ui.ui.theme, "dark");
    assert_eq!(ui.client_edits, enabled.client_edits);
    assert_eq!(ui.tun.enabled, Some(true));
    // Cancelling capture must remain possible even if source composition fails.
    let disabled = prepare_patch(
        &enabled,
        Some("a"),
        &json!({"runtime":{"dns":{}}}),
        patch(json!({"tun":{"enabled":false}})),
    )
    .unwrap();
    assert_eq!(disabled.tun.enabled, Some(false));
    assert_eq!(disabled.client_edits, enabled.client_edits);
}
