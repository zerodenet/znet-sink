use super::*;

#[test]
fn absent_native_runtime_never_serializes_a_confirmed_tunnel_or_instance() {
    let wire = serde_json::to_value(snapshot()).unwrap();
    assert_eq!(wire["vpnSupported"], false);
    assert_eq!(wire["bundledKernelAvailable"], false);
    assert_eq!(wire["kernelState"], "unavailable");
    assert_eq!(wire["vpnState"], "unknown");
    assert_eq!(wire["coreInstanceId"], serde_json::Value::Null);
    assert_eq!(wire["profileId"], serde_json::Value::Null);
    assert_eq!(wire["configRevision"], serde_json::Value::Null);
}

#[test]
fn rejected_desktop_fallback_has_an_actionable_platform_error() {
    let error = unavailable("gui_tun_enable");
    assert_eq!(error.code, "unsupported");
    assert!(error.message.contains("Proxy"));
    assert!(error.message.contains("protect/bind"));
    let details = error.details.unwrap();
    assert_eq!(details["operation"], "gui_tun_enable");
    assert_eq!(details["adaptationState"], "not-adapted");
    assert_eq!(details["status"], "unsupported");
    assert_eq!(details["reasonCode"], "native_runtime_owner_not_registered");
}

#[test]
fn both_mobile_platforms_explain_pending_capabilities_without_creating_runtime_facts() {
    for platform in ["android", "ios"] {
        let wire = serde_json::to_value(snapshot_for(platform)).unwrap();
        assert_eq!(wire["vpnSupported"], false);
        assert_eq!(wire["vpnState"], "unknown");
        assert!(wire["coreInstanceId"].is_null());
        let blocked = wire["unsupportedCapabilities"].as_array().unwrap();
        assert_eq!(blocked.len(), 3);
        for capability in blocked {
            assert_eq!(capability["status"], "unsupported");
            assert_eq!(capability["limitationKind"], "not-adapted");
            assert!(!capability["reason"].as_str().unwrap().is_empty());
            assert!(!capability["dependencyIds"].as_array().unwrap().is_empty());
        }
        assert_eq!(
            blocked[0]["dependencyIds"],
            serde_json::json!(["CORE-01", "CORE-02", "CLIENT-01"])
        );
    }
    assert!(snapshot_for("desktop").unsupported_capabilities.is_empty());
}
