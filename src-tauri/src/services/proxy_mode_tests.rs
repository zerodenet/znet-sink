use serde_json::json;

use crate::models::gui_core::GuiProxyMode;
use crate::services::proxy_mode::{apply_route_mode, detect_route_mode, route_global_outbound};

#[test]
fn direct_mode_writes_top_level_mode_and_preserves_rules() {
    let mut config = json!({
        "route": {
            "rules": [{ "condition": { "type": "domain", "values": ["example.com"] }, "action": { "type": "direct" } }],
            "final": { "type": "route", "outbound": "proxy" }
        },
        "outbounds": [{ "tag": "proxy" }, { "tag": "direct" }]
    });

    apply_route_mode(&mut config, &GuiProxyMode::Direct, None).unwrap();

    assert_eq!(config["mode"], json!({ "type": "direct" }));
    assert!(config["route"].get("mode").is_none());
    assert_eq!(
        config["route"]["final"],
        json!({ "type": "route", "outbound": "proxy" })
    );
    assert_eq!(config["route"]["rules"].as_array().unwrap().len(), 1);
}

#[test]
fn rule_mode_writes_top_level_mode_and_preserves_existing_final() {
    let mut config = json!({
        "mode": { "type": "global", "outbound": "server-a" },
        "route": {
            "rules": [{ "condition": { "type": "domain", "values": ["example.com"] }, "action": { "type": "direct" } }],
            "final": { "type": "route", "outbound": "proxy" }
        },
        "outbounds": [{ "tag": "proxy" }, { "tag": "direct" }]
    });

    apply_route_mode(&mut config, &GuiProxyMode::Rule, None).unwrap();

    assert_eq!(config["mode"], json!({ "type": "rule" }));
    assert!(config["route"].get("mode").is_none());
    assert_eq!(
        config["route"]["final"],
        json!({ "type": "route", "outbound": "proxy" })
    );
    assert_eq!(config["route"]["rules"].as_array().unwrap().len(), 1);
}

#[test]
fn rule_mode_adds_final_when_missing() {
    let mut config = json!({
        "route": {
            "rules": []
        },
        "outbounds": [{ "tag": "proxy" }, { "tag": "direct" }]
    });

    apply_route_mode(&mut config, &GuiProxyMode::Rule, None).unwrap();

    assert_eq!(config["mode"], json!({ "type": "rule" }));
    assert!(config["route"].get("mode").is_none());
    assert_eq!(
        config["route"]["final"],
        json!({ "type": "route", "outbound": "proxy" })
    );
}

#[test]
fn global_mode_writes_top_level_mode_with_outbound() {
    let mut config = json!({
        "proxy-groups": [
            { "name": "direct", "type": "select" },
            { "name": "proxy", "type": "select" }
        ],
        "outbounds": [{ "tag": "server-a" }]
    });

    apply_route_mode(&mut config, &GuiProxyMode::Global, None).unwrap();

    assert_eq!(
        config["mode"],
        json!({ "type": "global", "outbound": "proxy" })
    );
    assert!(config["route"].get("mode").is_none());
    assert_eq!(config["route"]["final"], json!({ "type": "direct" }));
}

#[test]
fn global_mode_uses_provided_outbound() {
    let mut config = json!({
        "outbounds": [{ "tag": "server-a" }, { "tag": "direct" }]
    });

    apply_route_mode(&mut config, &GuiProxyMode::Global, Some("server-a")).unwrap();

    assert_eq!(
        config["mode"],
        json!({ "type": "global", "outbound": "server-a" })
    );
    assert!(config["route"].get("mode").is_none());
    assert_eq!(config["route"]["final"], json!({ "type": "direct" }));
}

#[test]
fn direct_mode_adds_route_final_when_route_missing() {
    let mut config = json!({
        "outbounds": [{ "tag": "server-a" }, { "tag": "direct" }]
    });

    apply_route_mode(&mut config, &GuiProxyMode::Direct, None).unwrap();

    assert_eq!(config["mode"], json!({ "type": "direct" }));
    assert!(config["route"].get("mode").is_none());
    assert_eq!(config["route"]["final"], json!({ "type": "direct" }));
}

#[test]
fn detects_native_top_level_mode() {
    let config = json!({
        "mode": { "type": "global", "outbound": "proxy" },
        "route": {
            "final": { "type": "direct" }
        }
    });

    let detected = detect_route_mode(&config).unwrap();

    assert_eq!(detected.mode, GuiProxyMode::Global);
    assert_eq!(route_global_outbound(&config), Some("proxy".to_string()));
}

#[test]
fn detects_route_mode_shape() {
    let config = json!({
        "route": {
            "mode": { "type": "global", "outbound": "proxy" },
            "final": { "type": "direct" }
        }
    });

    let detected = detect_route_mode(&config).unwrap();

    assert_eq!(detected.mode, GuiProxyMode::Global);
    assert_eq!(route_global_outbound(&config), Some("proxy".to_string()));
}

#[test]
fn top_level_mode_takes_precedence_over_legacy_route_mode() {
    let config = json!({
        "mode": { "type": "direct" },
        "route": {
            "mode": { "type": "global", "outbound": "proxy" },
            "final": { "type": "direct" }
        }
    });

    let detected = detect_route_mode(&config).unwrap();

    assert_eq!(detected.mode, GuiProxyMode::Direct);
    assert_eq!(route_global_outbound(&config), Some("proxy".to_string()));
}

#[test]
fn detects_legacy_direct_final_as_direct() {
    let config = json!({
        "route": {
            "final": { "type": "direct" }
        }
    });

    let detected = detect_route_mode(&config).unwrap();

    assert_eq!(detected.mode, GuiProxyMode::Direct);
}

#[test]
fn global_target_switch_preserves_rule_final_and_selector_membership() {
    let mut config = json!({
        "mode":{"type":"global","outbound":"node-a"},
        "route":{"rules":[{"tag":"unchanged"}],"final":{"type":"route","outbound":"proxy"}},
        "outbounds":[{"tag":"node-a"},{"tag":"node-b"}],
        "outbound_groups":[
            {"tag":"proxy","type":"selector","outbounds":["auto","node-a"]},
            {"tag":"auto","type":"url_test","outbounds":["node-a","node-b"]}
        ]
    });
    let original = config.clone();
    apply_route_mode(&mut config, &GuiProxyMode::Global, Some("auto")).unwrap();
    assert_eq!(config["mode"]["outbound"], "auto");
    assert_eq!(config["route"], original["route"]);
    assert_eq!(config["outbound_groups"], original["outbound_groups"]);
    apply_route_mode(&mut config, &GuiProxyMode::Rule, None).unwrap();
    assert_eq!(config["route"], original["route"]);
}

#[test]
fn global_target_rejects_selector_missing_empty_and_cyclic_groups_without_edits() {
    let original = json!({
        "outbounds":[{"tag":"node-a"}],
        "outbound_groups":[
            {"tag":"manual","type":"selector","outbounds":["node-a"]},
            {"tag":"auto","type":"url_test","outbounds":["child"]},
            {"tag":"child","type":"selector","outbounds":["auto"]},
            {"tag":"empty","type":"fallback","outbounds":[]},
            {"tag":"broken","type":"url_test","outbounds":["missing"]}
        ]
    });
    for target in ["manual", "unknown", "auto", "empty", "broken"] {
        let mut candidate = original.clone();
        assert!(
            apply_route_mode(&mut candidate, &GuiProxyMode::Global, Some(target)).is_err(),
            "{target}"
        );
        assert_eq!(candidate, original);
    }
}

#[test]
fn global_target_accepts_declared_outbound_endpoint_but_not_inbound_only_endpoint() {
    let original = json!({"endpoints":[
        {"tag":"wg-a","protocol":{"type":"wireguard"},"directions":{"inbound":false,"outbound":true}},
        {"tag":"wg-in","protocol":{"type":"wireguard"},"directions":{"inbound":true,"outbound":false}}
    ]});
    let mut candidate = original.clone();
    apply_route_mode(&mut candidate, &GuiProxyMode::Global, Some("wg-a")).unwrap();
    assert_eq!(candidate["mode"]["outbound"], "wg-a");
    let mut candidate = original.clone();
    assert!(apply_route_mode(&mut candidate, &GuiProxyMode::Global, Some("wg-in")).is_err());
    assert_eq!(candidate, original);
}
