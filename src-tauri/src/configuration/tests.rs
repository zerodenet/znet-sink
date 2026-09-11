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
