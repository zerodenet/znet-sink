use super::*;
fn source(port: u16) -> Value {
    json!({"inbounds":[{"tag":"main","protocol":{"type":"mixed"},"listen":{"address":"127.0.0.2","port":port}}],"runtime":{"dns":{},"latency_test_url":"https://source.test/204"},"route":{"bypass":[]}})
}
#[test]
fn editing_port_only_preserves_source_address_dns_and_other_profiles() {
    let app = AppConfig::default();
    let base = source(7891);
    let next = candidate(
        &app,
        "a",
        BTreeMap::from([("localProxy.port".into(), json!(7877))]),
        &[],
    )
    .unwrap();
    let (_, edited) = resolve(&next, Some("a"), &base).unwrap();
    assert_eq!(
        edited["inbounds"][0]["listen"],
        json!({"address":"127.0.0.2","port":7877})
    );
    assert_eq!(edited["runtime"], base["runtime"]);
    assert_eq!(resolve(&next, Some("b"), &base).unwrap().1, base);
    assert_eq!(base, source(7891));
    assert_eq!(next.local_proxy.port, app.local_proxy.port);
}
#[test]
fn subscription_refresh_keeps_edit_and_reset_reads_latest_source() {
    let app = candidate(
        &AppConfig::default(),
        "a",
        BTreeMap::from([("localProxy.port".into(), json!(7877))]),
        &[],
    )
    .unwrap();
    let app: AppConfig = serde_json::from_str(&serde_json::to_string(&app).unwrap()).unwrap();
    assert_eq!(
        resolve(&app, Some("a"), &source(7892)).unwrap().1["inbounds"][0]["listen"]["port"],
        7877
    );
    let restored = candidate(&app, "a", Edits::new(), &["localProxy.port".into()]).unwrap();
    assert_eq!(
        resolve(&restored, Some("a"), &source(7892)).unwrap().1,
        source(7892)
    );
    assert!(restored.profile_edits.is_empty());
}
#[test]
fn edits_reject_host_process_fields_and_invalid_ports() {
    for (key, value) in [
        ("core.socket", json!("bad")),
        ("tun.enabled", json!(true)),
        ("localProxy.port", json!(0)),
        ("localProxy.port", json!("7877")),
    ] {
        assert!(
            candidate(
                &AppConfig::default(),
                "a",
                BTreeMap::from([(key.into(), value)]),
                &[]
            )
            .is_err(),
            "{key}"
        );
    }
}
#[test]
fn editing_public_url_keeps_policy_specific_url_and_tolerance() {
    let base = json!({"outbound_groups":[{"type":"url_test","tag":"a","url":"https://policy.test/204","tolerance_ms":0}]});
    let app = candidate(
        &AppConfig::default(),
        "a",
        BTreeMap::from([("urlTest.url".into(), json!("https://local.test/204"))]),
        &[],
    )
    .unwrap();
    let (_, edited) = resolve(&app, Some("a"), &base).unwrap();
    assert_eq!(edited["outbound_groups"], base["outbound_groups"]);
    assert_eq!(
        edited["runtime"]["latency_test_url"],
        "https://local.test/204"
    );
}

#[test]
fn legacy_switches_migrate_to_current_profile_without_affecting_other_profiles() {
    let mut app = AppConfig::default();
    app.overrides.listener = true;
    app.local_proxy.port = 7877;
    assert!(migrate_legacy(&mut app, Some("a")));
    assert!(!migrate_legacy(&mut app, Some("a")));
    assert_eq!(app.overrides, Default::default());
    assert_eq!(
        resolve(&app, Some("a"), &source(7891)).unwrap().1["inbounds"][0]["listen"]["port"],
        7877
    );
    assert_eq!(
        resolve(&app, Some("b"), &source(7891)).unwrap().1,
        source(7891)
    );
}

#[test]
fn settings_show_bind_address_while_system_proxy_uses_connectable_address() {
    use crate::models::proxy_config::ProxyConfigProfile;
    let mut base = source(7891);
    base["inbounds"][0]["listen"]["address"] = json!("0.0.0.0");
    let state = AppState::with_domain_data(
        AppConfig::default(),
        vec![ProxyConfigProfile {
            id: "a".into(),
            name: "a".into(),
            kernel: "zero".into(),
            format: "json".into(),
            path: None,
            content: Some(base),
            active: true,
            updated_at_unix_ms: 0,
            capabilities: Default::default(),
        }],
        vec![],
        vec![],
        vec![],
    );
    assert_eq!(
        view(&state).unwrap()["settings"]["localProxy"]["host"],
        "0.0.0.0"
    );
    assert_eq!(
        super::super::preferences::endpoint(&state).unwrap(),
        ("127.0.0.1".into(), 7891)
    );
}
