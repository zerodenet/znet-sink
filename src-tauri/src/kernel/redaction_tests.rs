use super::*;
#[test]
fn configuration_canary_is_removed_before_diagnostic_capture() {
    for input in [
        serde_json::json!({"type":"command","method":"config.apply_runtime","params":{"config":{"secret":"canary"}}}),
        serde_json::json!({"ok":true,"result":{"config":{"secret":"canary"}}}),
    ] {
        assert!(!crate::models::debug::capture_payload(&input)
            .to_string()
            .contains("canary"));
    }
}

#[test]
fn persistent_log_redaction_removes_nested_secrets_and_urls() {
    let input = serde_json::json!({
        "account": {"password": "canary-password", "deviceToken": "canary-token"},
        "message": "request https://example.test/private completed"
    });
    let output = sensitive(&input).to_string();
    assert!(!output.contains("canary"));
    assert!(!output.contains("example.test"));
    assert!(output.contains("[redacted]"));
    assert!(output.contains("[redacted-url]"));
    assert_eq!(
        text("failed at vmess://credential@example.test"),
        "failed at [redacted-url]"
    );
}
