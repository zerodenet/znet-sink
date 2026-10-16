use super::*;
use base64::{engine::general_purpose::STANDARD, Engine as _};

const CANARY: &str = "secret-canary-do-not-echo?token=private-material";

fn base() -> Value {
    json!({"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],
        "route":{"rules":[],"final":{"type":"direct"}}})
}

fn rejected(value: &Value) -> AppError {
    let error = validate_text(&value.to_string()).unwrap_err();
    assert_eq!(error.code, "native_validation_failed");
    assert_eq!(
        error.details.as_ref().unwrap()["validation_scope"],
        VALIDATION_SCOPE
    );
    assert!(!serde_json::to_string(&error).unwrap().contains(CANARY));
    error
}

fn reason(error: &AppError) -> &str {
    error.details.as_ref().unwrap()["errors"][0]["reasonCode"]
        .as_str()
        .unwrap()
}

#[test]
fn original_defaults_advanced_fields_and_max_u64_survive_round_trip() {
    let mut value = base();
    value["runtime"] = json!({"log":{"level":"info","files":[{
        "path":"not-created-by-validation/output.log", "max_bytes":u64::MAX}]},
        "dns":{"servers":{"secure":{"type":"doh","host":"resolver.example", "bootstrap":["192.0.2.53"], "path":"/dns-query?token=opaque"}},
            "default_server":"secure","policy":{"node_server":"secure", "direct_server":"secure"}}});
    value["outbounds"][0]["dial"] = json!({"address_family":"only_ipv4","source_ip":"127.0.0.1"});
    let result = validate_text(&value.to_string()).unwrap();
    assert_eq!(result, value);
    assert_eq!(
        result
            .pointer("/runtime/log/files/0/max_bytes")
            .unwrap()
            .as_u64(),
        Some(u64::MAX)
    );
    assert!(
        result.get("schema_version").is_none(),
        "validation must not insert defaults"
    );
    let pretty = serde_json::to_string_pretty(&result).unwrap();
    assert!(pretty.contains("18446744073709551615"));
    assert_eq!(validate_text(&pretty).unwrap(), value);
}

#[test]
fn canonical_endpoint_and_global_mode_stay_in_original_source_form() {
    let value = json!({"endpoints":[{"tag":"peer-device", "listen":{"address":"127.0.0.1","port":51820},
        "directions":{"inbound":true,"outbound":true}, "protocol":{"type":"wireguard",
            "private_key":STANDARD.encode([1;32]), "addresses":["10.0.0.1/32"],
            "peers":[{"public_key":STANDARD.encode([2;32]),"endpoint":"127.0.0.1:51821","allowed_ips":["10.0.0.0/24"]}]}}],
        "mode":{"type":"global","outbound":"peer-device"},
        "route":{"rules":[],"final":{"type":"route","outbound":"peer-device"}}});
    let result = validate_text(&value.to_string()).unwrap();
    assert_eq!(result, value);
    assert!(result.get("inbounds").is_none());
    assert!(result.get("outbounds").is_none());
}

#[test]
fn one_bom_and_exact_text_budget_are_supported() {
    let text = base().to_string();
    assert_eq!(validate_text(&format!("\u{feff}{text}")).unwrap(), base());
    assert!(validate_text(&format!("\u{feff}\u{feff}{text}")).is_err());
    let mut exact = text;
    exact.extend(std::iter::repeat_n(' ', MAX_TEXT_BYTES - exact.len()));
    assert!(validate_text(&exact).is_ok());
    exact.push(' ');
    assert_eq!(
        reason(&validate_text(&exact).unwrap_err()),
        "text_budget_exceeded"
    );
}

#[test]
fn syntax_unknown_fields_protocols_types_and_schema_are_sanitized() {
    let syntax = validate_text(&format!("{{\"{CANARY}\":")).unwrap_err();
    assert!(!serde_json::to_string(&syntax).unwrap().contains(CANARY));
    assert!(syntax.details.as_ref().unwrap()["errors"][0]["line"]
        .as_u64()
        .is_some());
    let mut value = base();
    value[CANARY] = json!(true);
    assert_eq!(reason(&rejected(&value)), "invalid_json_or_schema");
    value = base();
    value["outbounds"][0]["protocol"]["type"] = json!(CANARY);
    rejected(&value);
    value = base();
    value["runtime"] = json!({"udp_upstream_idle_timeout_seconds":CANARY});
    rejected(&value);
    value = base();
    value["schema_version"] = json!(u32::MAX);
    let error = rejected(&value);
    assert_eq!(reason(&error), "unsupported_schema_version");
    assert_eq!(
        error.details.unwrap()["errors"][0]["field_path"],
        "schema_version"
    );
}

#[test]
fn native_references_duplicate_tags_and_invalid_rules_are_rejected() {
    let mut value = base();
    value["route"]["final"] = json!({"type":"route","outbound":CANARY});
    assert_eq!(reason(&rejected(&value)), "invalid_route_or_reference");
    value = base();
    value["outbounds"] = json!([
        {"tag":CANARY,"protocol":{"type":"direct"}},
        {"tag":CANARY,"protocol":{"type":"direct"}}]);
    rejected(&value);
    value = base();
    value["route"]["rules"] = json!([{"condition":{"type":"domain_regex","values":["secret-canary-do-not-echo?token=private-material["]},"action":{"type":"direct"}}]);
    rejected(&value);
    value = base();
    value["runtime"] =
        json!({"dns":{"servers":{"system":{"type":"system"}},"default_server":CANARY}});
    rejected(&value);
}

#[test]
fn duplicate_json_fields_cannot_be_erased_by_value_projection() {
    let text = r#"{"route":{"rules":[],"final":{"type":"direct"}},"route":{"rules":[],"final":{"type":"direct"}}}"#;
    assert_eq!(
        reason(&validate_text(text).unwrap_err()),
        "invalid_json_or_schema"
    );
}

#[test]
fn every_rule_format_and_url_cache_is_denied_before_native_compilation() {
    for format in ["domain_list", "cidr_list", "zero_rule_ir", "zrs"] {
        for source_type in ["file", "url"] {
            let mut value = base();
            value["route"]["rule_sets"] = json!([{"tag":"rule", "type":source_type,
                "path":format!("/definitely-not-authorized/{CANARY}"),
                "url":format!("https://example.invalid/{CANARY}"), "format":format}]);
            let error = rejected(&value);
            assert_eq!(reason(&error), "dependency_authorization_required");
            assert_eq!(
                error.details.unwrap()["errors"][0]["field_path"],
                "route.rule_sets"
            );
        }
    }
}

#[test]
fn geoip_including_empty_path_is_denied_without_reading() {
    for path in ["", CANARY, "missing-relative-country.mmdb"] {
        let mut value = base();
        value["route"]["geoip_database"] = json!(path);
        let error = rejected(&value);
        assert_eq!(reason(&error), "dependency_authorization_required");
        assert_eq!(
            error.details.unwrap()["errors"][0]["field_path"],
            "route.geoip_database"
        );
    }
}

#[test]
fn network_paths_and_runtime_only_dependencies_are_not_treated_as_compiler_files() {
    let mut value = base();
    value["outbounds"].as_array_mut().unwrap().push(json!({"tag":"remote", "protocol":{
        "type":"vless","server":"example.invalid","port":443,"id":"00000000-0000-0000-0000-000000000001",
        "ws":{"path":"/looks/like/file"},"tls":{"ca_cert_path":"not-authorized-or-read.pem"}}}));
    value["api"] = json!({"control":{"enabled":true,
        "listen":{"address":"127.0.0.1","port":18080},"api_key_env":"DO_NOT_READ_THIS_ENV"}});
    assert_eq!(validate_text(&value.to_string()).unwrap(), value);
}

#[cfg(unix)]
#[test]
fn fifo_rule_and_geoip_dependencies_return_before_opening_a_reader() {
    use std::{ffi::CString, io::Write, os::unix::fs::OpenOptionsExt, sync::mpsc, time::Duration};
    let directory = tempfile::tempdir().unwrap();
    let fifo = directory.path().join("not-a-readable-rule");
    let c_path = CString::new(fifo.as_os_str().as_encoded_bytes()).unwrap();
    // SAFETY: c_path is a valid NUL-terminated path inside this test's tmpdir.
    assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
    for geoip in [false, true] {
        let mut value = base();
        if geoip {
            value["route"]["geoip_database"] = json!(fifo.to_str().unwrap());
        } else {
            value["route"]["rule_sets"] = json!([{"tag":"rule","type":"file", "path":fifo.to_str().unwrap(),"format":"domain_list"}]);
        }
        let (tx, rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let _ = tx.send(validate_text(&value.to_string()));
        });
        let result = rx.recv_timeout(Duration::from_secs(2));
        if result.is_err() {
            // Release an accidentally opened reader so a regression fails the
            // test instead of hanging the complete test process indefinitely.
            if let Ok(mut writer) = std::fs::OpenOptions::new()
                .write(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&fifo)
            {
                let _ = writer.write_all(b"example.invalid\n");
            }
        }
        let error = result.expect("validation attempted FIFO IO").unwrap_err();
        assert_eq!(reason(&error), "dependency_authorization_required");
        worker.join().unwrap();
    }
}
