use super::*;
use serde_json::json;
#[test]
fn close_receipt_requires_actual_accepted_matching_id_and_explicit_closed() {
    for value in [
        json!({}),
        json!({"flow_id":"1","closed":true}),
        json!({"accepted":true}),
        json!({"accepted":true,"result":{"flow_id":"1"}}),
        json!({"accepted":true,"result":{"flow_id":"2","closed":true}}),
        json!({"accepted":true,"result":{"flow_id":1,"closed":true}}),
    ] {
        assert_eq!(
            parse_connection_close(&value, "1".into()).unwrap_err().code,
            "connection_close_unknown"
        );
    }
    assert_eq!(
        parse_connection_close(
            &json!({"accepted":false,"result":{"flow_id":"1","closed":true}}),
            "1".into()
        )
        .unwrap_err()
        .code,
        "connection_close_rejected"
    );
    let no = parse_connection_close(
        &json!({"accepted":true,"result":{"flow_id":"1","closed":false,"message":"secret-canary"}}),
        "1".into(),
    )
    .unwrap();
    assert!(!no.closed);
    assert!(no.message.is_none());
    assert!(
        parse_connection_close(
            &json!({"accepted":true,"result":{"flow_id":"1","closed":true}}),
            "1".into()
        )
        .unwrap()
        .closed
    );
}
