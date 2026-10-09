use super::*;
fn source(port: u16) -> Value {
    json!({"inbounds":[{"tag":"main","protocol":{"type":"mixed"},"listen":{"address":"127.0.0.2","port":port}}],"runtime":{"dns":{},"latency_test_url":"https://source.test/204"},"route":{"bypass":[]}})
}
#[test]
fn editing_port_is_global_and_preserves_unmodified_source_fields() {
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
    assert_eq!(
        resolve(&next, Some("b"), &source(7899)).unwrap().1["inbounds"][0]["listen"]["port"],
        7877
    );
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
    assert!(restored.client_edits.as_ref().unwrap().is_empty());
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
fn udp_idle_timeout_is_global_and_restores_the_latest_source_value() {
    let base = json!({
        "runtime": {"udp_upstream_idle_timeout_seconds": 45},
        "route": {"bypass": []}
    });
    let app = candidate(
        &AppConfig::default(),
        "a",
        BTreeMap::from([("runtime.udpUpstreamIdleTimeoutSeconds".into(), json!(90))]),
        &[],
    )
    .unwrap();
    assert_eq!(
        resolve(&app, Some("a"), &base).unwrap().1["runtime"]["udp_upstream_idle_timeout_seconds"],
        90
    );
    assert_eq!(
        resolve(&app, Some("b"), &base).unwrap().1["runtime"]["udp_upstream_idle_timeout_seconds"],
        90
    );
    assert!(candidate(
        &AppConfig::default(),
        "a",
        BTreeMap::from([("runtime.udpUpstreamIdleTimeoutSeconds".into(), json!(0),)]),
        &[],
    )
    .is_err());
    let restored = candidate(
        &app,
        "a",
        Edits::new(),
        &["runtime.udpUpstreamIdleTimeoutSeconds".into()],
    )
    .unwrap();
    assert_eq!(resolve(&restored, Some("a"), &base).unwrap().1, base);
}

#[test]
fn legacy_explicit_switches_migrate_globally_without_inferring_defaults() {
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
        resolve(&app, Some("b"), &source(7891)).unwrap().1["inbounds"][0]["listen"]["port"],
        7877
    );
}

#[test]
fn legacy_profile_settings_promote_active_values_once_and_keep_archived_conflicts() {
    let mut app: AppConfig = serde_json::from_value(json!({"profileEdits": {
        "a": {"localProxy.port": 7877}, "b": {"localProxy.port": 8888}
    }}))
    .unwrap();
    assert!(migrate_legacy(&mut app, Some("a")));
    assert!(!migrate_legacy(&mut app, Some("b")));
    assert_eq!(app.profile_edits["b"]["localProxy.port"], 8888);
    assert_eq!(
        resolve(&app, Some("b"), &source(7892)).unwrap().1["inbounds"][0]["listen"]["port"],
        7877
    );
    let restored = candidate(&app, "b", Edits::new(), &["localProxy.port".into()]).unwrap();
    let mut restored: AppConfig = serde_json::from_value(json!(restored)).unwrap();
    assert!(!migrate_legacy(&mut restored, Some("b")));
    assert_eq!(
        resolve(&restored, Some("b"), &source(7892)).unwrap().1,
        source(7892)
    );
    assert_eq!(
        resolve(&restored, Some("a"), &source(7893)).unwrap().1,
        source(7893)
    );
}

#[test]
fn global_settings_apply_without_a_profile_and_survive_profile_preferences_removal() {
    let mut app = candidate(
        &AppConfig::default(),
        "a",
        BTreeMap::from([
            ("localProxy.port".into(), json!(7877)),
            ("tun.mtu".into(), json!(1280)),
            ("urlTest.toleranceMs".into(), json!(0)),
            ("bypass".into(), json!({"localNetworks":false,"rules":[]})),
            ("routing.injectCommonRules".into(), json!(false)),
            (
                "dns".into(),
                json!({"enabled":false,"config":null,"dnsHijack":false}),
            ),
        ]),
        &[],
    )
    .unwrap();
    app.profile_edits.clear();
    let base = json!({"inbounds": source(7892)["inbounds"], "runtime":{"tun":{"mtu":1500}},
        "outbound_groups":[{"tag":"auto","type":"url_test","tolerance_ms":50}]});
    for id in [None, Some("a"), Some("b")] {
        let (settings, resolved) = resolve(&app, id, &base).unwrap();
        assert_eq!(resolved["inbounds"][0]["listen"]["port"], 7877);
        assert_eq!(resolved["runtime"]["tun"]["mtu"], 1280);
        assert_eq!(resolved["outbound_groups"][0]["tolerance_ms"], 0);
        assert!(!settings.dns.enabled);
        assert!(settings.overrides.dns && settings.overrides.bypass && settings.overrides.rules);
        assert!(!settings.routing.inject_common_rules);
    }
}

#[test]
fn explicit_empty_inbounds_do_not_gain_a_listener_from_global_port_settings() {
    let app = candidate(
        &AppConfig::default(),
        "a",
        BTreeMap::from([("localProxy.port".into(), json!(7877))]),
        &[],
    )
    .unwrap();
    assert!(resolve(&app, Some("b"), &json!({"inbounds":[]})).is_err());
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
            managed_source: None,
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
