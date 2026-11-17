use super::*;
use serde_json::json;
fn base() -> Value {
    json!({"schema_version":1,"mode":{"type":"rule"},"endpoints":[{"tag":"Starters","enabled":true,"directions":{"inbound":false,"outbound":true},"protocol":{"type":"wireguard","peers":[{"allowed_ips":["192.168.0.0/23","10.10.0.0/24","fd00::/8"]}]}}],"route":{"rules":[],"final":{"type":"direct"}}})
}
fn profile() -> RuleSetProfile {
    serde_json::from_value(json!({"id":"database","name":"DB","enabled":true,"semanticIr":{"version":1,"rules":[{"type":"ipv4_cidr","value":"192.168.1.235/32"}]},"updatedAtUnixMs":1,"commonBinding":{"enabled":true,"action":{"endpoint":"Starters"},"order":0},"artifact":{"path":"test.zrs","majorVersion":1,"minorVersion":0,"checksum":0,"fileSize":1,"entryCount":1,"builtAtUnixMs":1}})).unwrap()
}
fn compose(base: &Value, profile: RuleSetProfile) -> AppResult<Value> {
    Ok(super::super::rules::compose_with(base, false, &[profile], |_, _| Ok(()))?.config)
}
#[test]
fn endpoint_rule_is_independent_of_public_switch_and_preserves_original() {
    let original = base();
    let config = compose(&original, profile()).unwrap();
    assert_eq!(
        config["route"]["rules"][0]["action"],
        json!({"type":"route","outbound":"Starters"})
    );
    assert_eq!(config["route"]["rules"][0]["mode"], "translate");
    assert_eq!(original, base());
    assert_eq!(compose(&config, profile()).unwrap(), config);
}
#[test]
fn missing_disabled_and_inbound_only_endpoints_are_rejected() {
    for mutation in [
        json!([]),
        json!([{"tag":"Starters","enabled":false}]),
        json!([{"tag":"Starters","directions":{"outbound":false}}]),
    ] {
        let mut b = base();
        b["endpoints"] = mutation;
        assert!(compose(&b, profile()).is_err());
    }
}
#[test]
fn wireguard_allowed_ips_must_cover_the_entire_destination_prefix() {
    for (target, allowed) in [
        ("192.168.1.235/32", true),
        ("192.168.0.0/23", true),
        ("192.168.0.0/16", false),
        ("16.0.0.0/8", false),
        ("fd12::1/128", true),
        ("::/0", false),
    ] {
        let mut p = profile();
        p.semantic_ir["rules"][0] =
            json!({"type":if target.contains(':'){"ipv6_cidr"}else{"ipv4_cidr"},"value":target});
        assert_eq!(compose(&base(), p).is_ok(), allowed, "{target}");
    }
}
#[test]
fn bypass_conflict_is_reported_instead_of_silent_direct() {
    let p = profile();
    let mut config = compose(&base(), p.clone()).unwrap();
    config["route"]["bypass"] = json!([{"type":"ip","values":["192.168.0.0/16"]}]);
    assert!(validate_bypass(&config, &[p.clone()])
        .unwrap_err()
        .message
        .contains("192.168.0.0/16"));
    config["route"]["bypass"] = json!([{"type":"ip","values":["127.0.0.0/8"]}]);
    validate_bypass(&config, &[p]).unwrap();
}
#[test]
fn rule_mode_and_binding_enable_remain_authoritative() {
    let mut b = base();
    b["mode"] = json!({"type":"direct"});
    assert!(compose(&b, profile()).unwrap()["route"]["rules"]
        .as_array()
        .unwrap()
        .is_empty());
    let mut p = profile();
    p.common_binding.as_mut().unwrap().enabled = false;
    assert!(compose(&base(), p).unwrap()["route"]["rules"]
        .as_array()
        .unwrap()
        .is_empty());
}
#[test]
fn legacy_string_actions_deserialize_unchanged() {
    for action in ["direct", "proxy", "reject", "final"] {
        assert!(serde_json::from_value::<CommonRuleAction>(json!(action)).is_ok());
    }
}

#[test]
fn subdomain_bypass_cannot_silently_override_a_domain_suffix_binding() {
    let mut p = profile();
    p.semantic_ir["rules"] = json!([{"type":"domain_suffix","value":"starmerx.com"}]);
    let mut config = compose(&base(), p.clone()).unwrap();
    config["route"]["bypass"] =
        json!([{"type":"domain_regex","values":["(?i)^.*\\.starmerx\\.com$"]}]);
    assert!(validate_bypass(&config, &[p.clone()]).is_err());
    config["route"]["bypass"] =
        json!([{"type":"domain_regex","values":["(?i)^localhost$","(?i)^.*\\.local$"]}]);
    validate_bypass(&config, &[p]).unwrap();
}

#[test]
fn command_managed_tun_exclusions_cannot_hide_behind_an_empty_core_bypass() {
    let p = profile();
    let mut source = base();
    source["route"]["bypass"] = json!([]);
    source["runtime"]["tun"] = json!({"exclude_cidrs":["192.168.0.0/16"]});
    let config = compose(&source, p.clone()).unwrap();
    validate_bypass(&config, &[p.clone()]).unwrap();
    assert!(validate_capture(&config, &source, &Default::default(), &[p]).is_err());
}

#[test]
fn unavailable_remote_rule_contents_do_not_bypass_conflict_validation() {
    let mut p = profile();
    p.semantic_ir["rules"] = json!([]);
    let mut config = compose(&base(), p.clone()).unwrap();
    config["route"]["bypass"] = json!([{"type":"ip","values":["192.168.0.0/16"]}]);
    assert!(validate_bypass(&config, &[p]).is_err());
}
