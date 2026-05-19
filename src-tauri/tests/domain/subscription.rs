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
fn subscription_parser_rejects_plain_json() {
    let error = parse_subscription_content(r#"{"outbounds":[]}"#, "auto").unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}

#[test]
fn subscription_parser_rejects_unknown_format() {
    let error = parse_subscription_content("{}", "binary").unwrap_err();

    assert_eq!(error.code, "invalid_argument");
}
