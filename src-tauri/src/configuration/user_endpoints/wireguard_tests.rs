use super::validate;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};

fn endpoint() -> Value {
    json!({"tag":"imported", "directions":{"inbound":false,"outbound":true},
        "protocol":{"type":"wireguard", "private_key":STANDARD.encode([1;32]),
            "addresses":["10.66.0.2/32"], "mtu":1380,
            "peers":[{"public_key":STANDARD.encode([2;32]), "pre_shared_key":STANDARD.encode([3;32]),
                "allowed_ips":["0.0.0.0/0"], "endpoint":"vpn.example:51820", "keepalive_secs":25}]}})
}

#[test]
fn imported_optional_connection_values_are_validated_without_disclosing_secrets() {
    assert!(validate(&[endpoint()]).is_ok());
    for (path, value) in [
        ("/protocol/mtu", json!(65489)),
        ("/protocol/mtu", json!(67)),
        ("/protocol/peers/0/pre_shared_key", json!("SECRET")),
        ("/protocol/peers/0/keepalive_secs", json!(-1)),
        ("/protocol/peers/0/keepalive_secs", json!(65536)),
        ("/protocol/peers/0/keepalive_secs", json!(0.5)),
    ] {
        let mut row = endpoint();
        *row.pointer_mut(path).unwrap() = value;
        let error = validate(&[row]).unwrap_err();
        assert!(!error.message.contains("SECRET"), "{path}");
    }
    let mut ipv6 = endpoint();
    ipv6["protocol"]["addresses"] = json!(["fd66::2/128"]);
    ipv6["protocol"]["mtu"] = json!(1279);
    assert!(validate(&[ipv6]).is_err());
}

#[test]
fn inbound_requires_a_real_listener_and_supports_roaming_peers_and_dynamic_ports() {
    let mut row = endpoint();
    row["directions"]["inbound"] = json!(true);
    assert!(validate(&[row.clone()])
        .unwrap_err()
        .message
        .contains("监听"));
    row["listen"] = json!({"address":"::", "port":0});
    row["protocol"]["peers"][0]
        .as_object_mut()
        .unwrap()
        .remove("endpoint");
    assert!(validate(&[row.clone()]).is_ok());
    row["listen"]["port"] = json!(65536);
    assert!(validate(&[row.clone()]).is_err());
    row["listen"] = json!({"address":"not-an-ip", "port":51820});
    assert!(validate(&[row]).is_err());
}

#[test]
fn outbound_without_a_listener_requires_a_remote_peer_address() {
    let mut row = endpoint();
    row["protocol"]["peers"][0]
        .as_object_mut()
        .unwrap()
        .remove("endpoint");
    assert!(validate(&[row]).unwrap_err().message.contains("远端地址"));
}

#[test]
fn tunnel_address_error_explains_the_expected_field_instead_of_echoing_a_key() {
    let mut row = endpoint();
    row["protocol"]["addresses"] = json!([STANDARD.encode([9; 32])]);
    let error = validate(&[row]).unwrap_err();
    assert!(error.message.contains("Address"));
    assert!(error.message.contains("10.66.0.2/32"));
    assert!(!error.message.contains(&STANDARD.encode([9; 32])));
}
