use gui_lib::services::subscription::parse_subscription_content;

#[test]
fn subscription_parser_accepts_zero_base64_json() {
    let parsed = parse_subscription_content(
        "eyJvdXRib3VuZHMiOlt7InRhZyI6ImRpcmVjdCIsInR5cGUiOiJkaXJlY3QifV0sInJvdXRlIjp7InJ1bGVzIjpbeyJvdXRib3VuZCI6ImRpcmVjdCJ9XX19",
        "auto",
    )
    .unwrap();

    assert_eq!(parsed.format, "zero-base64-json");
    assert!(parsed.content.get("outbounds").is_some());
}

#[test]
fn subscription_parser_accepts_raw_zero_json_in_auto_mode() {
    // Many providers serve the Zero config as plain JSON instead of
    // base64-wrapping it. Auto-detect should accept it.
    let parsed =
        parse_subscription_content(r#"{"outbounds":[{"tag":"hk","type":"trojan"}]}"#, "auto")
            .unwrap();

    assert_eq!(parsed.format, "zero-json");
    assert!(parsed.content.get("outbounds").is_some());
}

#[test]
fn subscription_parser_rejects_unrelated_json() {
    // A JSON object with none of the known Zero config keys should not
    // be mistaken for a config in auto mode.
    let error = parse_subscription_content(r#"{"hello":"world"}"#, "auto").unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn subscription_parser_converts_clash_yaml() {
    let parsed = parse_subscription_content(
        r#"
proxies:
  - name: hk-1
    type: ss
    server: hk.example.com
    port: 8388
    cipher: aes-128-gcm
    password: secret
proxy-groups:
  - name: Proxy
    type: select
    proxies:
      - hk-1
      - DIRECT
rules:
  - DOMAIN-SUFFIX,example.com,Proxy
  - GEOIP,CN,DIRECT
  - MATCH,Proxy
"#,
        "auto",
    )
    .unwrap();

    assert_eq!(parsed.format, "clash-yaml-converted");
    assert_eq!(parsed.content["outbounds"][2]["tag"], "hk-1");
    assert_eq!(parsed.content["outbounds"][2]["type"], "shadowsocks");
    assert_eq!(parsed.content["outbound_groups"][0]["tag"], "Proxy");
    assert_eq!(
        parsed.content["outbound_groups"][0]["outbounds"][1],
        "direct"
    );
    assert_eq!(
        parsed.content["route"]["rules"][0]["condition"]["type"],
        "domain_suffix"
    );
    assert_eq!(
        parsed.content["route"]["rules"][1]["action"]["outbound"],
        "direct"
    );
    assert_eq!(parsed.content["route"]["final"]["outbound"], "Proxy");
}

#[test]
fn subscription_parser_converts_base64_clash_yaml() {
    use base64::{engine::general_purpose, Engine as _};
    let yaml = "proxies:\n  - {name: hk-1, type: ss, server: s, port: 1}\n";
    let encoded = general_purpose::STANDARD.encode(yaml.as_bytes());

    let parsed = parse_subscription_content(&encoded, "auto").unwrap();

    assert_eq!(parsed.format, "clash-base64-yaml-converted");
    assert_eq!(parsed.content["outbounds"][2]["tag"], "hk-1");
    assert_eq!(parsed.content["outbounds"][2]["type"], "shadowsocks");
}

#[test]
fn subscription_parser_rejects_unknown_format() {
    let error = parse_subscription_content("{}", "binary").unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}
