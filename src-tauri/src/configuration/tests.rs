use super::composition::{finalize, Inputs};
use crate::models::app_config::{AppBypassConfig, AppConfig};
use serde_json::json;
use std::collections::BTreeMap;
fn inputs() -> Inputs {
    let mut app = AppConfig::default();
    app.bypass = Some(AppBypassConfig {
        local_networks: false,
        rules: vec![],
    });
    app.dns.enabled = false;
    Inputs {
        source_profile_id: Some("profile-a".into()),
        app,
        supports_tolerance: true,
        selections: BTreeMap::from([("choose".into(), "b".into())]),
    }
}
#[test]
fn composition_preserves_source_and_reports_order_without_values() {
    let base = json!({"inbounds":[],"outbounds":[{"tag":"a","secret":"must-not-appear"},{"tag":"b"}],"route":{"final":{"type":"direct"}},"runtime":{"dns":{"legacy":"source"}},"outbound_groups":[{"tag":"choose","type":"selector","outbounds":["a","b"]},{"tag":"auto","type":"url_test","outbounds":["a"],"tolerance_ms":0}]});
    let original = base.clone();
    let candidate = finalize(&base, base.clone(), &inputs()).unwrap();
    assert_eq!(base, original);
    assert_eq!(
        candidate.config.pointer("/runtime/dns"),
        base.pointer("/runtime/dns")
    );
    assert_eq!(candidate.config["outbound_groups"][0]["selected"], "b");
    assert_eq!(candidate.config["outbound_groups"][1]["tolerance_ms"], 0);
    assert_eq!(
        candidate
            .report
            .layers
            .iter()
            .map(|l| l.source)
            .collect::<Vec<_>>(),
        vec![
            "common_rules",
            "local_listener",
            "client_tun",
            "global_dns",
            "urltest_override",
            "saved_selections",
            "bypass"
        ]
    );
    assert_eq!(candidate.report.layers[3].changed_paths, Vec::<&str>::new());
    assert!(!serde_json::to_string(&candidate.report)
        .unwrap()
        .contains("must-not-appear"));
}
#[test]
fn unsupported_tolerance_and_stale_saved_choice_do_not_change_group_semantics() {
    let base = json!({"inbounds":[],"route":{"final":{"type":"direct"}},"outbound_groups":[{"tag":"choose","type":"selector","outbounds":["a"]},{"tag":"auto","type":"url_test","outbounds":["a"]}]});
    let mut context = inputs();
    context.supports_tolerance = false;
    let candidate = finalize(&base, base.clone(), &context).unwrap();
    assert!(candidate.config["outbound_groups"][0]
        .get("selected")
        .is_none());
    assert!(candidate.config["outbound_groups"][1]
        .get("tolerance_ms")
        .is_none());
}
#[test]
fn invalid_candidate_does_not_publish_a_partial_report() {
    let workspace = super::Workspace::default();
    let mut context = inputs();
    context.app.dns.enabled = true;
    context.app.dns.config = None;
    let base = json!({"inbounds":[],"route":{"final":{"type":"direct"}}});
    assert!(finalize(&base, base.clone(), &context).is_err());
    assert!(workspace.report().is_none());
}

#[test]
fn explicit_client_overrides_replace_selected_capabilities_without_editing_source() {
    let source = json!({
        "inbounds": [
            {"tag":"subscription-entry", "protocol":{"type":"mixed"}, "listen":{"address":"127.0.0.2", "port":7891}},
            {"tag":"auxiliary-http", "protocol":{"type":"http"}, "listen":{"address":"127.0.0.3", "port":9001}}
        ],
        "runtime":{"tun":{"name":"profile-tun"},"latency_test_url":"https://source.test/check", "dns":{"source":true}},
        "route":{"bypass":[{"type":"domain","values":["source.test"]}],"final":{"type":"direct"}},
        "outbound_groups":[{"tag":"auto","type":"url_test","outbounds":["a"],"url":"https://group.test/check","tolerance_ms":120}]
    });
    let mut settings = inputs();
    settings.app.overrides = crate::models::app_config::ConfigOverrides {
        listener: true,
        dns: true,
        tun: true,
        url_test: true,
        bypass: true,
        rules: true,
    };
    settings.app.local_proxy.port = 7890;
    settings.app.local_proxy.source_proxy_config_id = Some("legacy-source".into());
    settings.app.url_test.url = "https://client.test/204".into();
    settings.app.url_test.tolerance_ms = 0;
    let actual = finalize(&source, source.clone(), &settings).unwrap().config;
    assert_eq!(actual["inbounds"][0]["listen"]["port"], 7890);
    assert_eq!(actual["inbounds"][0]["tag"], "subscription-entry");
    assert_eq!(actual["inbounds"][1], source["inbounds"][1]);
    assert!(actual.pointer("/runtime/tun").is_none());
    assert!(actual.pointer("/runtime/dns").is_none());
    assert_eq!(
        actual["runtime"]["latency_test_url"],
        "https://client.test/204"
    );
    assert_eq!(
        actual["outbound_groups"][0]["url"],
        "https://client.test/204"
    );
    assert_eq!(actual["outbound_groups"][0]["tolerance_ms"], 0);
    assert_eq!(actual["route"]["bypass"], json!([]));
    assert_eq!(source["inbounds"][0]["listen"]["port"], 7891);
    assert_eq!(source["runtime"]["tun"]["name"], "profile-tun");
}

#[test]
fn malformed_test_url_rejects_candidate_before_publication() {
    let mut settings = inputs();
    for value in [
        "",
        "ftp://test.example/",
        "https://user:secret@test.example/",
        "https://test.example/#fragment",
    ] {
        settings.app.url_test.url = value.into();
        assert!(finalize(&json!({}), json!({}), &settings).is_err());
    }
    let legacy: crate::models::app_config::AppUrlTestConfig =
        serde_json::from_value(json!({"toleranceMs":0})).unwrap();
    assert_eq!(
        legacy.url,
        crate::models::app_config::default_url_test_url()
    );
    assert_eq!(legacy.tolerance_ms, 0);
}

#[test]
fn recommended_dns_follows_route_final_without_proxying_node_resolution() {
    let app = AppConfig::default();
    for (final_route, expected) in [
        (json!({"type":"route", "outbound":"proxy"}), Some("proxy")),
        (json!({"type":"direct"}), None),
    ] {
        let mut config = json!({"outbounds":[{"tag":"proxy"}],"route":{"final":final_route}});
        let mut dns = app.dns.clone();
        dns.enabled = true;
        super::dns::apply_global_dns(&mut config, &dns).unwrap();
        let value = &config["runtime"]["dns"];
        for tag in [
            "cloudflare",
            "google",
            "cloudflare-bootstrap",
            "google-bootstrap",
        ] {
            assert_eq!(value["servers"][tag]["detour"].as_str(), expected);
        }
        for tag in ["alidns", "114dns"] {
            assert!(value["servers"][tag].get("detour").is_none());
        }
        assert!(value["servers"].get("system").is_none());
        assert_eq!(value["policy"]["node_server"], "alidns");
        assert_eq!(value["policy"]["node_fallback_servers"], json!(["114dns"]));
        let model: crate::models::dns_config::ClientDnsConfig =
            serde_json::from_value(value.clone()).unwrap();
        model.validate_client_shape().unwrap();
    }
}

#[test]
fn wireguard_dns_local_edit_preserves_the_device_and_source_configuration() {
    let base = json!({"inbounds":[],"outbounds":[{
        "tag":"wg-a","protocol":{"type":"wireguard","private_key":"private-key",
        "addresses":["10.10.0.11/32"],"peers":[
            {"public_key":"peer-key","endpoint":"gateway.test:51820","allowed_ips":["192.168.0.0/23"]},
            {"public_key":"other-key","endpoint":"[2001:db8::1]:51820","allowed_ips":["10.0.0.0/8"]}
        ]}}],"route":{"auto_outbounds":["wg-a"],"rules":[],"bypass":[],"final":{"type":"reject"}},
        "runtime":{"dns":{"servers":{"system":{"type":"system"}},"default_server":"system"}}});
    let original = base.clone();
    let mut context = inputs();
    let edited_dns = json!({"enabled":true,"dnsHijack":false,"config":{
        "servers":{"system":{"type":"system"},"wg-a-dns":{
            "type":"udp","host":"192.168.1.180","detour":"wg-a"}},
        "default_server":"system","policy":{"node_server":"system"},
        "dispatch":[{"condition":{"type":"domain","values":["office.internal.test"]},"server":"wg-a-dns"}]}});
    context.app = super::local_edits::candidate(
        &context.app,
        "profile-a",
        BTreeMap::from([("dns".into(), edited_dns)]),
        &[],
    )
    .unwrap();
    let (resolved, edited) =
        super::local_edits::resolve(&context.app, Some("profile-a"), &base).unwrap();
    context.app = resolved;
    let candidate = finalize(&base, edited, &context).unwrap();
    assert_eq!(base, original);
    assert_eq!(candidate.config["outbounds"], original["outbounds"]);
    assert_eq!(candidate.config["route"], original["route"]);
    let dns = &candidate.config["runtime"]["dns"];
    assert_eq!(dns["servers"]["wg-a-dns"]["detour"], "wg-a");
    assert_eq!(dns["dispatch"][0]["server"], "wg-a-dns");
    let contract: crate::models::dns_config::ClientDnsConfig =
        serde_json::from_value(dns.clone()).unwrap();
    contract.validate_client_shape().unwrap();

    let mut without_device = original.clone();
    without_device["outbounds"] = json!([]);
    assert!(finalize(&without_device, without_device.clone(), &context).is_err());
}

#[test]
fn canonical_endpoint_dns_detour_accepts_declared_outbound_and_rejects_inbound_only() {
    let config = json!({"endpoints":[
        {"tag":"egress","enabled":false,"protocol":{"type":"wireguard"}},
        {"tag":"ingress","directions":{"inbound":true,"outbound":false},"protocol":{"type":"wireguard"}}
    ]});
    let mut dns = json!({"servers":{"private":{"type":"udp","host":"10.0.0.1","detour":"egress"}}});
    super::dns::resolve_dns_detours(&config, &mut dns).unwrap();
    assert_eq!(dns["servers"]["private"]["detour"], "egress");
    dns["servers"]["private"]["detour"] = json!("ingress");
    assert!(super::dns::resolve_dns_detours(&config, &mut dns).is_err());
}
