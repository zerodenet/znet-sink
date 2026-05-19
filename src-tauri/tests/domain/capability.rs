use gui_lib::services::proxy_config::{
    analyze_capabilities, extract_local_proxy, parse_config_content,
};
use serde_json::json;

#[test]
fn proxy_config_without_urltest_disables_urltest_feature() {
    let config = json!({
        "proxies": [{ "name": "direct", "type": "direct" }],
        "proxy-groups": [{ "name": "proxy", "type": "select", "proxies": ["direct"] }],
        "rules": ["MATCH,proxy"]
    });

    let capabilities = analyze_capabilities(Some(&config));

    assert!(capabilities.has_proxy_nodes);
    assert!(capabilities.has_proxy_groups);
    assert!(capabilities.has_route_rules);
    assert!(capabilities.has_selector);
    assert!(!capabilities.has_url_test);
    assert!(!capabilities.feature_keys.contains(&"urlTest".to_string()));
}

#[test]
fn proxy_config_content_parser_rejects_invalid_json() {
    let error = parse_config_content("not-json").unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn proxy_config_extracts_local_inbound_port() {
    let config = json!({
        "inbounds": [{
            "tag": "mixed-in",
            "listen": { "address": "127.0.0.1", "port": 7891 },
            "protocol": { "type": "mixed" }
        }]
    });

    let endpoint = extract_local_proxy(&config).unwrap();

    assert_eq!(endpoint.host, "127.0.0.1");
    assert_eq!(endpoint.port, 7891);
}

#[test]
fn proxy_config_with_urltest_enables_urltest_feature() {
    let config = json!({
        "outbounds": [{ "tag": "server-a", "type": "direct" }],
        "policies": [{ "tag": "auto", "type": "urltest", "members": ["server-a"] }],
        "route": { "rules": [{ "outbound": "auto" }] },
        "rule_sets": [{ "tag": "geoip-cn" }]
    });

    let capabilities = analyze_capabilities(Some(&config));

    assert!(capabilities.has_url_test);
    assert!(capabilities.has_rule_sets);
    assert!(capabilities.feature_keys.contains(&"urlTest".to_string()));
}
