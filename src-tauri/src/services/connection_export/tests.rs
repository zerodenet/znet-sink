use super::*;
fn frame() -> DebugFrame {
    DebugFrame {
        id: u64::MAX,
        at_ms: 9007199254740993,
        direction: "rx".into(),
        frame_type: "event".into(),
        telemetry: None,
        elapsed_ms: None,
        error: Some("secret".into()),
        payload: json!({"payload":{"record":{"flowId":"flow-123","network":"tcp","source":"[2001:db8::1]:1234","destination":"192.0.2.5:443","targetHost":"EXAMPLE.COM","bytesUp":u64::MAX,"bytesDown":"9007199254740993","result":{"outcome":"success","message":"https://private/?token=secret"},"password":"secret","processPath":"/private/secret","targetIp":"invalid"}}}),
    }
}
pub(super) fn scope() -> ClientScope {
    use crate::client_core::{ConfigRevision, CoreInstanceId};
    ClientScope {
        profile_id: None,
        config_revision: ConfigRevision(1),
        core_instance_id: CoreInstanceId(1),
    }
}
#[test]
fn connection_export_preserves_real_public_facts_without_secret_text_or_unsafe_addresses() {
    let value = project(&frame());
    assert_eq!(value["flowId"], "flow-123");
    assert_eq!(value["bytesUp"], u64::MAX.to_string());
    assert_eq!(value["bytesDown"], "9007199254740993");
    assert_eq!(value["sourceAddress"], "[2001:db8::1]:1234");
    assert_eq!(value["targetHost"], "example.com");
    assert!(value["targetIp"].is_null());
    assert_eq!(value["outcome"], "success");
    assert!(!value.to_string().contains("secret"));
    for text in [
        "https://user:pass@example.com/?token=secret",
        "example.com?token=secret",
        "x/secret",
        "example.com#secret",
    ] {
        assert!(domain(Some(text)).is_null());
        assert!(socket(Some(text)).is_null());
    }
    assert!(flow_id(Some("https://token/secret")).is_null());
    assert!(enum_value(Some("success-secret"), &["success"]).is_null());
}
#[test]
fn connection_export_preview_fixes_bounded_local_completed_scope_and_never_echoes_filters() {
    let query = DebugFrameQuery {
        search: Some("token=secret".into()),
        ..Default::default()
    };
    let page = DebugFramePage {
        items: vec![frame()],
        has_more: true,
        oldest_available_id: None,
        history: None,
    };
    let (preview, bytes) = prepare(&query, page, 100).unwrap();
    assert_eq!(preview.records, 1);
    assert!(preview.truncated);
    assert!(bytes.len() < BYTES);
    assert!(!String::from_utf8(bytes.clone()).unwrap().contains("secret"));
    let mut plans = BTreeMap::new();
    plans.insert(
        preview.plan_id.clone(),
        Plan {
            active: None,
            scope: scope(),
            expires: 200,
            bytes: Some(bytes.clone()),
        },
    );
    assert_eq!(
        take_plan(&mut plans, &preview.plan_id, &scope(), 199).unwrap(),
        bytes
    );
    assert!(take_plan(&mut plans, &preview.plan_id, &scope(), 199).is_err());
}
#[test]
fn connection_export_expired_or_changed_client_scope_cannot_deliver_or_resnapshot() {
    let mut plans = BTreeMap::new();
    plans.insert(
        "fixed".into(),
        Plan {
            active: None,
            scope: scope(),
            expires: 200,
            bytes: Some(b"fixed".to_vec()),
        },
    );
    assert!(take_plan(&mut plans, "fixed", &scope(), 200).is_err());
    let mut changed = scope();
    changed.config_revision = crate::client_core::ConfigRevision(2);
    assert!(take_plan(&mut plans, "fixed", &changed, 199).is_err());
    assert_eq!(
        take_plan(&mut plans, "fixed", &scope(), 199).unwrap(),
        b"fixed"
    );
}

#[test]
fn active_connection_export_rejects_guest_source_and_ids_and_keeps_mobile_bridge_reason() {
    for value in [
        json!({"source":"active","ids":["fake"]}),
        json!({"search":"x","record":{"flow_id":"fake"}}),
        json!({"beforeId":123}),
    ] {
        assert!(serde_json::from_value::<ActiveConnectionExportFilter>(value).is_err());
    }
    assert!(ActiveConnectionExportFilter {
        protocol: Some("icmp".into()),
        ..Default::default()
    }
    .validate()
    .is_err());
    assert_eq!(
        active::runtime_available(true).unwrap_err().code,
        "unsupported"
    );
    active::runtime_available(false).unwrap();
}
