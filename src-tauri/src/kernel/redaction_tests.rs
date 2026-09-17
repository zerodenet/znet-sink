use super::*;
#[test]
fn configuration_canary_is_removed_from_envelopes_and_byte_previews() {
    for input in [
        serde_json::json!({"type":"command","method":"config.apply_runtime","params":{"config":{"secret":"canary"}}}),
        serde_json::json!({"ok":true,"result":{"config":{"secret":"canary"}}}),
    ] {
        assert!(!frame(&input).to_string().contains("canary"));
        assert!(!preview(&serde_json::to_vec(&input).unwrap()).contains("canary"));
    }
    assert!(!preview(b"unparseable canary").contains("canary"));
    assert!(preview(
        &serde_json::to_vec(&serde_json::json!({"message":"中".repeat(300)})).unwrap()
    )
    .ends_with('…'));
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
