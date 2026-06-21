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
