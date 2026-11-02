use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};

fn endpoint() -> Value {
    json!({"tag":"home", "enabled":true, "directions":{"inbound":false,"outbound":true},
        "protocol":{"type":"wireguard","private_key":STANDARD.encode([1;32]),"addresses":["10.66.0.2/32"],
            "peers":[{"public_key":STANDARD.encode([2;32]),"endpoint":"vpn.example:51820","allowed_ips":["0.0.0.0/0","::/0"]}]},
        "dns":{"servers":["10.66.0.1","fd66::1"],"search_domains":[],"use_as_default":true}})
}
fn base() -> Value {
    json!({"runtime":{"dns":{"servers":{"original":{"type":"udp","host":"9.9.9.9"}},
        "default_server":"original","dispatch":[{"condition":{"type":"domain","values":["example.com"]},"server":"original"}],
        "answer":{"type":"real"},"cache":{"max_entries":512},"policy":{"timeout_ms":3000}}}})
}

#[test]
fn imported_dns_applies_servers_and_fallbacks_preserving_original_rules_and_settings() {
    let mut config = base();
    let original = config.clone();
    let row = endpoint();
    apply(&mut config, &[row.clone()]).unwrap();
    let dns = &config["runtime"]["dns"];
    assert_eq!(dns["default_server"], server_tag("home", 0));
    assert_eq!(
        dns["policy"]["fallback_servers"],
        json!([server_tag("home", 1)])
    );
    assert_eq!(dns["servers"][server_tag("home", 0)]["detour"], "home");
    assert_eq!(dns["servers"][server_tag("home", 1)]["host"], "fd66::1");
    assert_eq!(dns["policy"]["node_server"], "original");
    for path in [
        "/runtime/dns/dispatch",
        "/runtime/dns/cache",
        "/runtime/dns/answer",
        "/runtime/dns/servers/original",
    ] {
        assert_eq!(config.pointer(path), original.pointer(path), "{path}");
    }
    let once = config.clone();
    apply(&mut config, &[row.clone()]).unwrap();
    assert_eq!(config, once);
    let mut source = json!({});
    crate::configuration::user_endpoints::merge(&mut source, &[row]).unwrap();
    assert!(source["endpoints"][0].get("dns").is_none());
    assert_eq!(source["endpoints"][0]["protocol"]["type"], "wireguard");
}

#[test]
fn dns_outside_allowed_ips_uses_direct_transport_and_ipv6_ranges_are_honored() {
    let mut row = endpoint();
    row["protocol"]["peers"][0]["allowed_ips"] = json!(["10.0.0.0/8", "fd00::/8"]);
    row["dns"]["servers"] = json!(["10.66.0.1", "8.8.8.8", "fd66::1", "2001:db8::1"]);
    let mut config = base();
    apply(&mut config, &[row]).unwrap();
    let servers = &config["runtime"]["dns"]["servers"];
    for index in [0, 2] {
        assert_eq!(servers[server_tag("home", index)]["detour"], "home");
    }
    for index in [1, 3] {
        assert!(servers[server_tag("home", index)].get("detour").is_none());
    }
}

#[test]
fn disabling_or_removing_endpoint_restores_base_dns_and_reenabling_restores_imported_dns() {
    let row = endpoint();
    let app = crate::models::app_config::AppConfig::default();
    let source = base();
    let compose = |rows: Vec<Value>| {
        let mut app = app.clone();
        app.user_endpoints = rows;
        let (app, resolved) =
            crate::configuration::local_edits::resolve(&app, None, &source).unwrap();
        crate::configuration::composition::finalize(
            &source,
            resolved,
            &crate::configuration::composition::Inputs {
                source_profile_id: None,
                app,
                supports_tolerance: false,
                selections: Default::default(),
            },
        )
        .unwrap()
        .config
    };
    let original = compose(vec![]);
    let enabled = compose(vec![row.clone()]);
    assert_eq!(
        enabled["runtime"]["dns"]["default_server"],
        server_tag("home", 0)
    );
    let mut disabled = row.clone();
    disabled["enabled"] = json!(false);
    assert_eq!(
        compose(vec![disabled])["runtime"]["dns"],
        original["runtime"]["dns"]
    );
    let mut inbound = row.clone();
    inbound["listen"] = json!({"address":"0.0.0.0","port":0});
    inbound["directions"] = json!({"inbound":true,"outbound":false});
    assert_eq!(
        compose(vec![inbound])["runtime"]["dns"],
        original["runtime"]["dns"]
    );
    assert_eq!(
        compose(vec![row])["runtime"]["dns"],
        enabled["runtime"]["dns"]
    );
    assert_eq!(source, base());
}

#[test]
fn metadata_validation_rejects_conflicts_and_invalid_values_without_echoing_input() {
    let row = endpoint();
    let mut duplicate = row.clone();
    duplicate["tag"] = json!("office");
    assert!(validate(&[row.clone(), duplicate]).is_err());
    for value in [json!([]), json!(["SECRET"]), json!(["https://dns.test"])] {
        let mut invalid = row.clone();
        invalid["dns"]["servers"] = value;
        assert!(!validate(&[invalid]).unwrap_err().message.contains("SECRET"));
    }
    let mut second = row.clone();
    second["dns"]["use_as_default"] = json!(false);
    let mut config = base();
    apply(&mut config, &[second]).unwrap();
    assert_eq!(config["runtime"]["dns"]["default_server"], "original");
}
