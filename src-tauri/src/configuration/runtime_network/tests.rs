use super::*;
use crate::{
    models::{proxy_config::ProxyConfigCapabilities, subscription::ManagedSubscriptionSource},
    services::app_database,
};
struct Fixture {
    state: AppState,
    dir: tempfile::TempDir,
}
fn source() -> Value {
    json!({"inbounds":[{"tag":"entry","protocol":{"type":"mixed"},"listen":{"address":"127.0.0.2","port":7891}}],"outbounds":[{"tag":"direct","protocol":{"type":"direct"},"dial":{"address_family":"only_ipv6","interface":"missing-canary","source_ip":"2001:db8::1"}}],"route":{"rules":[],"final":{"type":"direct"}},"runtime":{"network":{"mtu":1400},"udp_upstream_idle_timeout_seconds":u64::MAX}})
}
fn profile() -> ProxyConfigProfile {
    ProxyConfigProfile {
        id: "manual".into(),
        name: "ordinary".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(source()),
        active: true,
        managed_source: None,
        updated_at_unix_ms: 1,
        capabilities: ProxyConfigCapabilities::default(),
    }
}
fn fixture(p: ProxyConfigProfile, subs: Vec<SubscriptionProfile>) -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let mut app = AppConfig::default();
    crate::services::bypass::normalize(&mut app).unwrap();
    app.local_proxy.source_proxy_config_id = Some("runtime-owned".into());
    app.core.socket = Some(
        dir.path()
            .join("no-runtime.sock")
            .to_string_lossy()
            .into_owned(),
    );
    app_database::save_domain_data(dir.path(), std::slice::from_ref(&p), &subs).unwrap();
    app_config_store::save(&dir.path().join("app-config.json"), &app).unwrap();
    domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
    let state = AppState::with_domain_data(app, vec![p], subs, vec![], vec![]);
    workspace::recovery::set_test_directory(&state, dir.path().to_path_buf());
    Fixture { state, dir }
}
fn draft(f: &Fixture) -> PreviewInput {
    let s = snapshot(&f.state).unwrap();
    PreviewInput {
        profile_id: s.profile_id.unwrap(),
        binding: s.binding.unwrap(),
        changes: BTreeMap::from([("tun.mtu".into(), json!(1280))]),
        reset: vec![],
    }
}
fn prepare(f: &Fixture) -> Plan {
    preview(&f.state, draft(f)).unwrap()
}
#[test]
fn canonical_values_preserve_native_mtu_priority_and_lossless_udp_and_direct_dial() {
    let f = fixture(profile(), vec![]);
    let s = snapshot(&f.state).unwrap();
    let v = s.values.unwrap();
    assert_eq!(v.mtu, 1400);
    assert_eq!(v.mtu_origin, "source-network");
    assert_eq!(v.udp_idle_seconds, u64::MAX.to_string());
    assert_eq!(v.listener.unwrap().host, "127.0.0.2");
    assert_eq!(s.dial_leaves[0].address_family, "only_ipv6");
    let mut raw = source();
    raw["runtime"]["tun"] = json!({"addr":"10.0.0.1/24","mtu":1300});
    let mut app = AppConfig::default();
    assert_eq!(values(&app, &raw).unwrap().mtu, 1300);
    app.client_edits = Some(BTreeMap::from([("tun.mtu".into(), json!(576))]));
    assert_eq!(values(&app, &raw).unwrap().mtu, 576);
    let mut defaults = source();
    defaults["runtime"] = json!({});
    assert_eq!(values(&AppConfig::default(), &defaults).unwrap().mtu, 1500);
}
#[test]
fn local_owned_save_reopens_disk_without_source_or_runtime_io_and_is_idempotent() {
    let f = fixture(profile(), vec![]);
    let before = app_database::load_domain_data(f.dir.path()).unwrap();
    #[cfg(unix)]
    let socket =
        std::os::unix::net::UnixListener::bind(f.dir.path().join("no-runtime.sock")).unwrap();
    #[cfg(unix)]
    socket.set_nonblocking(true).unwrap();
    let mut d = draft(&f);
    d.changes.insert(
        "runtime.udpUpstreamIdleTimeoutSeconds".into(),
        json!(u64::MAX.to_string()),
    );
    d.changes.insert("localProxy.host".into(), json!("0.0.0.0"));
    d.changes.insert("localProxy.port".into(), json!(9000));
    let p = preview(&f.state, d).unwrap();
    assert!(p.lan_exposure_warning);
    assert_eq!(
        p.after.listener.as_ref().unwrap().exposure,
        "all-interfaces"
    );
    let r = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(r.state, "saved_pending_apply");
    assert!(r.snapshot.unwrap().pending_apply);
    let disk = app_config_store::load_or_default(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        disk.local_proxy.source_proxy_config_id.as_deref(),
        Some("runtime-owned")
    );
    assert_eq!(
        edited(&disk, "runtime.udpUpstreamIdleTimeoutSeconds")
            .unwrap()
            .as_u64(),
        Some(u64::MAX)
    );
    assert_eq!(
        workspace::digest(&before.proxy_configs).unwrap(),
        workspace::digest(
            &app_database::load_domain_data(f.dir.path())
                .unwrap()
                .proxy_configs
        )
        .unwrap()
    );
    let receipt = app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    assert_eq!(
        receipt,
        app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
    );
    assert!(f.state.configuration().transaction().is_none());
    #[cfg(unix)]
    assert_eq!(
        socket.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn invalid_and_unknown_fields_never_create_a_plan() {
    let f = fixture(profile(), vec![]);
    for (key, val) in [
        ("tun.mtu", json!(575)),
        ("tun.mtu", json!(65536)),
        ("tun.mtu", json!(1500.1)),
        (
            "runtime.udpUpstreamIdleTimeoutSeconds",
            json!(9007199254740992u64),
        ),
        (
            "runtime.udpUpstreamIdleTimeoutSeconds",
            json!("18446744073709551616"),
        ),
        ("runtime.udpUpstreamIdleTimeoutSeconds", json!("0")),
        ("runtime.udpUpstreamIdleTimeoutSeconds", json!("01")),
        ("localProxy.host", json!("secret-canary.invalid")),
        ("localProxy.port", json!(0)),
        ("localProxy.port", json!(65536)),
        ("dial.interface", json!("secret-canary")),
    ] {
        let mut d = draft(&f);
        d.changes = BTreeMap::from([(key.into(), val)]);
        let e = preview(&f.state, d).unwrap_err();
        assert!(!e.message.contains("secret-canary"));
    }
    assert!(lock(&f.state.runtime_network().plans, "p")
        .unwrap()
        .is_empty());
}
#[test]
fn explicit_empty_or_absent_inbounds_never_gain_a_listener() {
    for inbounds in [json!([]), Value::Null] {
        let mut p = profile();
        if inbounds.is_null() {
            p.content
                .as_mut()
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove("inbounds");
        } else {
            p.content.as_mut().unwrap()["inbounds"] = inbounds;
        }
        let f = fixture(p, vec![]);
        assert!(snapshot(&f.state)
            .unwrap()
            .values
            .unwrap()
            .listener
            .is_none());
        let mut d = draft(&f);
        d.changes = BTreeMap::from([("localProxy.port".into(), json!(9000))]);
        assert!(preview(&f.state, d).is_err());
    }
}
#[test]
fn reset_removes_only_confirmed_keys_and_restores_current_source_network_mtu() {
    let f = fixture(profile(), vec![]);
    let p = prepare(&f);
    save(&f.state, &p.plan_id).unwrap();
    let mut d = draft(&f);
    d.changes.clear();
    d.reset = vec!["tun.mtu".into()];
    let p = preview(&f.state, d).unwrap();
    assert_eq!(p.after.mtu, 1400);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    assert!(snapshot(&f.state).unwrap().edited_fields.is_empty());
}
#[test]
fn managed_and_protected_sources_are_denied_before_schema_or_body_projection() {
    for linked in [false, true] {
        let mut p = profile();
        p.content = Some(json!({"secret":"secret-canary"}));
        let mut subs = vec![];
        let owner = ManagedSubscriptionSource {
            plugin_id: "plugin".into(),
            provider_id: "provider".into(),
            remote_subscription_id: "remote".into(),
            revision: None,
            source_name: None,
        };
        if linked {
            let mut s:SubscriptionProfile=serde_json::from_value(json!({"id":"sub","name":"sub","url":"https://invalid/secret-canary","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":"manual","updatedAtUnixMs":1})).unwrap();
            s.managed_source = Some(owner);
            subs.push(s);
        } else {
            p.managed_source = Some(owner);
        }
        let f = fixture(p, subs);
        let s = snapshot(&f.state).unwrap();
        assert!(s.binding.is_none());
        assert!(s.values.is_none());
        assert!(!serde_json::to_string(&s).unwrap().contains("secret-canary"));
    }
    let mut p = profile();
    p.content = None;
    assert!(snapshot(&fixture(p, vec![]).state)
        .unwrap()
        .values
        .is_none());
}
#[test]
fn source_artifact_and_geoip_paths_are_never_opened_by_network_schema_projection() {
    let mut p = profile();
    let raw = p.content.as_mut().unwrap();
    raw["route"]["rule_sets"] = json!([{"tag":"external","type":"file","path":"/untrusted/secret-canary.fifo","format":"zrs"}]);
    raw["route"]["geoip_database"] = json!("/untrusted/secret-canary.mmdb");
    let f = fixture(p, vec![]);
    let s = snapshot(&f.state).unwrap();
    assert!(s.binding.is_some());
    assert!(!serde_json::to_string(&s).unwrap().contains("secret-canary"));
    let p = prepare(&f);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
}
#[test]
fn source_same_timestamp_or_active_profile_change_rejects_the_old_plan() {
    for active in [false, true] {
        let f = fixture(profile(), vec![]);
        let p = prepare(&f);
        let mut rows = lock(f.state.proxy_configs(), "p").unwrap();
        if active {
            rows[0].active = false;
        } else {
            rows[0].content.as_mut().unwrap()["runtime"]["network"]["mtu"] = json!(1450);
        }
        drop(rows);
        assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
    }
}
#[test]
fn app_history_and_domain_epoch_aba_reject_equal_current_values() {
    for app_domain in [false, true] {
        let f = fixture(profile(), vec![]);
        let p = prepare(&f);
        if app_domain {
            let old = lock(f.state.app_config(), "a").unwrap().clone();
            let mut altered = old.clone();
            altered.mobile_notifications_enabled = !altered.mobile_notifications_enabled;
            app_config_store::save(&f.dir.path().join("app-config.json"), &altered).unwrap();
            app_config_store::save(&f.dir.path().join("app-config.json"), &old).unwrap();
        } else {
            let old = lock(f.state.proxy_configs(), "p").unwrap().clone();
            let mut altered = old.clone();
            altered[0].name = "changed".into();
            app_database::save_domain_data(f.dir.path(), &altered, &[]).unwrap();
            app_database::save_domain_data(f.dir.path(), &old, &[]).unwrap();
        }
        assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
    }
}
#[test]
fn uncertain_owned_delivery_keeps_original_id_and_blocks_every_new_write() {
    let f = fixture(profile(), vec![]);
    let a = prepare(&f);
    let b = prepare(&f);
    let next = draft(&f);
    f.state
        .runtime_network()
        .fail_after_write
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        save(&f.state, &a.plan_id).unwrap().state,
        "submitted_unknown"
    );
    let before = app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap();
    assert_eq!(
        save(&f.state, &a.plan_id).unwrap().state,
        "submitted_unknown"
    );
    assert_eq!(
        status(&f.state, &a.plan_id).unwrap().state,
        "submitted_unknown"
    );
    assert!(save(&f.state, &b.plan_id).is_err());
    assert!(preview(&f.state, next).is_err());
    assert!(discard(&f.state, &a.plan_id).is_err());
    assert_eq!(
        before,
        app_config_store::current_history(&f.dir.path().join("app-config.json")).unwrap()
    );
}
#[test]
fn late_writer_never_returns_its_snapshot_as_original_saved_receipt() {
    let f = fixture(profile(), vec![]);
    let p = prepare(&f);
    *lock(&f.state.runtime_network().after_publish, "hook").unwrap() = Some(|state| {
        let mut app = lock(state.app_config(), "a").unwrap();
        app.mobile_notifications_enabled = !app.mobile_notifications_enabled;
        app_config_store::save(
            &workspace::recovery::directory(state)
                .unwrap()
                .join("app-config.json"),
            &app,
        )
        .unwrap();
    });
    let r = save(&f.state, &p.plan_id).unwrap();
    assert_eq!(r.state, "submitted_unknown");
    assert!(r.snapshot.is_none());
}
#[test]
fn ipv4_mapped_loopback_is_not_mislabeled_as_lan_and_specific_address_is_unverified() {
    assert_eq!(exposure("::ffff:127.0.0.1").unwrap(), "loopback");
    assert_eq!(exposure("::").unwrap(), "all-interfaces");
    assert_eq!(exposure("192.0.2.9").unwrap(), "specific-address");
    assert!(exposure("224.0.0.1").is_err());
}

#[test]
fn malformed_existing_network_edits_do_not_silently_fall_back_to_source_values() {
    let raw = source();
    for (key, val) in [
        ("tun.mtu", json!("1500")),
        ("runtime.udpUpstreamIdleTimeoutSeconds", json!("30")),
        ("localProxy.host", json!(null)),
        ("localProxy.port", json!("9000")),
    ] {
        let app = AppConfig {
            client_edits: Some(BTreeMap::from([(key.into(), val)])),
            ..Default::default()
        };
        assert!(values(&app, &raw).is_err());
    }
}

#[test]
fn active_source_and_client_scope_profile_must_match_before_snapshot_preview_and_save() {
    let f = fixture(profile(), vec![]);
    let input = draft(&f);
    let p = prepare(&f);
    let mut other = profile();
    other.id = "other".into();
    f.state.client_core_configuration_committed(Some(&other));
    assert!(snapshot(&f.state).is_err());
    assert!(preview(&f.state, input).is_err());
    assert_eq!(save(&f.state, &p.plan_id).unwrap().state, "rejected");
}
#[test]
fn unreviewed_legacy_globals_and_flags_are_readonly_without_implicit_migration() {
    for flags in [false, true] {
        let f = fixture(profile(), vec![]);
        let mut app = lock(f.state.app_config(), "a").unwrap();
        if flags {
            app.overrides.tun = true;
            app.tun.mtu = 1300;
        } else {
            app.client_edits = None;
            app.profile_edits.insert(
                "manual".into(),
                BTreeMap::from([("dns".into(), json!({"secret":"canary"}))]),
            );
        }
        app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
        let before = workspace::digest(&*app).unwrap();
        drop(app);
        let s = snapshot(&f.state).unwrap();
        assert!(s.binding.is_none());
        assert!(s.values.is_none());
        assert_eq!(s.read_only_reason.as_deref(), Some(LEGACY_REASON));
        assert_eq!(
            workspace::digest(&*lock(f.state.app_config(), "a").unwrap()).unwrap(),
            before
        );
    }
}
#[test]
fn fresh_pre_global_none_can_create_sparse_edits_without_touching_unrelated_preferences() {
    let f = fixture(profile(), vec![]);
    let mut app = lock(f.state.app_config(), "a").unwrap();
    app.client_edits = None;
    app.mobile_notifications_enabled = true;
    app.profile_edits.insert(
        "other".into(),
        BTreeMap::from([("endpoints".into(), json!({"version":1}))]),
    );
    let before = app.clone();
    app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
    drop(app);
    let p = prepare(&f);
    assert_eq!(
        save(&f.state, &p.plan_id).unwrap().state,
        "saved_pending_apply"
    );
    let actual = lock(f.state.app_config(), "a").unwrap().clone();
    let mut expected = before;
    expected.client_edits = Some(BTreeMap::from([("tun.mtu".into(), json!(1280))]));
    assert_eq!(actual, expected);
}
#[test]
fn ipv6_intent_does_not_invent_a_schema_mtu_floor_beyond_the_pinned_576_limit() {
    let mut raw = source();
    raw["runtime"]["tun"] = json!({"addr":"fd66::1/64","dual_stack":true,"mtu":576});
    assert_eq!(values(&AppConfig::default(), &raw).unwrap().mtu, 576);
    raw["runtime"]["network"]["mtu"] = json!(575);
    raw["runtime"].as_object_mut().unwrap().remove("tun");
    assert!(values(&AppConfig::default(), &raw).is_err());
}
