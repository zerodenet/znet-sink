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
    assert_eq!(error.code, "mobile_platform_unavailable");
    assert!(error.message.contains("Proxy"));
    assert!(error.message.contains("protect/bind"));
    let details = error.details.unwrap();
    assert_eq!(details["operation"], "gui_tun_enable");
    assert_eq!(details["adaptationState"], "not-adapted");
}
