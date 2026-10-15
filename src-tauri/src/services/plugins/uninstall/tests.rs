use super::*;
use crate::{
    models::subscription::ManagedSubscriptionSource,
    services::{app_config_store, app_database, domain_store},
};
use serde_json::json;
const PLUGIN: &str = "org.example.plugin";
const OP: &str = "0123456789abcdef0123456789abcdef";
struct Fixture {
    dir: tempfile::TempDir,
    host: Host,
    state: AppState,
    profile: String,
}
fn fixture(active: bool) -> Fixture {
    let (dir, host, _manager) = super::super::tests::setup(false);
    let owner = ManagedSubscriptionSource {
        plugin_id: PLUGIN.into(),
        provider_id: "https://provider.example".into(),
        remote_subscription_id: "one".into(),
        revision: Some("r1".into()),
        source_name: None,
    };
    let h = storage::hash(
        format!(
            "{}\0{}\0{}",
            owner.plugin_id, owner.provider_id, owner.remote_subscription_id
        )
        .as_bytes(),
    );
    let sub = format!("managed-{}", &h[..32]);
    let profile = format!("managed-config-{}", &h[..32]);
    let p = ProxyConfigProfile {
        id: profile.clone(),
        name: "Owned config".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(
            json!({"route":{"rules":[],"final":{"type":"direct"}},"runtime":{"udp_upstream_idle_timeout_seconds":u64::MAX}}),
        ),
        active,
        managed_source: Some(owner.clone()),
        updated_at_unix_ms: 1,
        capabilities: Default::default(),
    };
    let s:SubscriptionProfile=serde_json::from_value(json!({"id":sub,"name":"Owned subscription","url":"secret-canary-token","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":profile,"managedSource":owner,"updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null})).unwrap();
    let mut app = AppConfig::default();
    crate::services::bypass::normalize(&mut app).unwrap();
    app.local_proxy.source_proxy_config_id = Some("existing-runtime-owner".into());
    app.profile_edits.insert(profile.clone(), BTreeMap::new());
    let state = AppState::with_domain_data(app.clone(), vec![p], vec![s], vec![], vec![]);
    workspace::recovery::set_test_directory(&state, dir.path().into());
    app_database::save_domain_data(
        dir.path(),
        &state.proxy_configs().lock().unwrap(),
        &state.subscriptions().lock().unwrap(),
    )
    .unwrap();
    app_config_store::save(&dir.path().join("app-config.json"), &app).unwrap();
    domain_store::save_rule_sets_to_dir(dir.path(), &[]).unwrap();
    host.rescan(state.capabilities(), &host.store().unwrap())
        .unwrap();
    super::super::configuration::save(
        dir.path(),
        &format!("{PLUGIN}/identity"),
        BTreeMap::from([("token".into(), "secret-canary-token".into())]),
    )
    .unwrap();
    let publisher = host.namespace_publisher(PLUGIN).unwrap();
    super::super::namespace::put(
        dir.path(),
        &publisher,
        PLUGIN,
        super::super::namespace::Area::State,
        "secret".into(),
        STANDARD.encode(b"opaque-secret-canary"),
    )
    .unwrap();
    std::fs::write(dir.path().join("vault.key"), b"shared-key-canary").unwrap();
    Fixture {
        dir,
        host,
        state,
        profile,
    }
}
#[test]
fn inactive_exact_fixed_resources_remove_without_runtime_and_survive_true_reopen() {
    let f = fixture(false);
    let source = f.state.proxy_configs().lock().unwrap()[0].content.clone();
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert!(p.can_execute, "{:?}", p.blocked_reasons);
    assert!(p.resources.iter().any(|r| r.kind == "profile"));
    let json = serde_json::to_string(&p).unwrap();
    assert!(!json.contains("secret-canary"));
    assert!(!json.contains("18446744073709551615"));
    let before = f.state.client_core_snapshot();
    let s = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(s.state, "completed");
    assert!(s.items.iter().all(|i| i.state == "completed"));
    assert!(f.state.proxy_configs().lock().unwrap().is_empty());
    assert!(f.state.subscriptions().lock().unwrap().is_empty());
    assert_eq!(
        f.state
            .app_config()
            .lock()
            .unwrap()
            .local_proxy
            .source_proxy_config_id
            .as_deref(),
        Some("existing-runtime-owner")
    );
    assert_eq!(
        source.unwrap()["runtime"]["udp_upstream_idle_timeout_seconds"].as_u64(),
        Some(u64::MAX)
    );
    let db = app_database::load_domain_data(f.dir.path()).unwrap();
    assert!(db.proxy_configs.is_empty());
    assert!(db.subscriptions.is_empty());
    assert_eq!(
        app_config_store::load_or_default(&f.dir.path().join("app-config.json"))
            .unwrap()
            .local_proxy
            .source_proxy_config_id
            .as_deref(),
        Some("existing-runtime-owner")
    );
    assert_eq!(
        std::fs::read(f.dir.path().join("vault.key")).unwrap(),
        b"shared-key-canary"
    );
    assert!(std::fs::read_dir(f.dir.path().join("packages"))
        .unwrap()
        .next()
        .is_some());
    assert_eq!(
        f.state.client_core_snapshot().scope.core_instance_id,
        before.scope.core_instance_id
    );

    let replay = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(replay, s);
    let reopened = Host {
        root: Some(f.dir.path().into()),
        ..Default::default()
    };
    assert_eq!(reopened.uninstall_status(OP).unwrap(), s);
    assert_eq!(reopened.uninstall_recent().unwrap(), vec![s]);
}
#[test]
fn active_is_metadata_blocked_no_implicit_replacement_stop_or_delete() {
    let f = fixture(true);
    let before = std::fs::read(f.dir.path().join("installed.json")).unwrap();
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert!(!p.can_execute);
    assert_eq!(p.active_profile.unwrap().id, f.profile);
    assert!(p.requires_manual_switch);
    assert!(!p.requires_stop);
    assert_eq!(
        f.host
            .uninstall_execute(&f.state, &p.plan_id, OP, true)
            .unwrap_err()
            .code,
        "plugin_uninstall_preflight_rejected"
    );
    assert_eq!(
        before,
        std::fs::read(f.dir.path().join("installed.json")).unwrap()
    );
    assert!(f.host.uninstall_recent().unwrap().is_empty());
}
#[test]
fn readonly_observations_preserve_plan_but_same_value_writer_aba_rejects_it() {
    let f = fixture(false);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    let epoch = storage::RootGuard::acquire(f.dir.path())
        .unwrap()
        .epoch()
        .unwrap();
    f.host
        .storage_list(PLUGIN, super::super::namespace::Area::State)
        .unwrap();
    let _ = f.host.snapshot(f.state.capabilities());
    assert_eq!(
        storage::RootGuard::acquire(f.dir.path())
            .unwrap()
            .epoch()
            .unwrap(),
        epoch
    );
    let values =
        super::super::configuration::values(f.dir.path(), &format!("{PLUGIN}/identity")).unwrap();
    super::super::configuration::save(f.dir.path(), &format!("{PLUGIN}/identity"), values.clone())
        .unwrap();
    super::super::configuration::save(f.dir.path(), &format!("{PLUGIN}/identity"), values).unwrap();
    assert_eq!(
        f.host
            .uninstall_execute(&f.state, &p.plan_id, OP, true)
            .unwrap_err()
            .code,
        "plugin_uninstall_preflight_rejected"
    );
    assert!(f
        .host
        .store()
        .unwrap()
        .list()
        .unwrap()
        .contains(&PLUGIN.into()));
}
#[test]
fn app_rules_graph_and_host_aba_invalidate_original_preview() {
    for variant in 0..4 {
        let f = fixture(false);
        let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
        match variant {
            0 => {
                let app = f.state.app_config().lock().unwrap().clone();
                app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
            }
            1 => {
                domain_store::save_rule_sets_to_dir(f.dir.path(), &[]).unwrap();
            }
            2 => {
                app_database::save_domain_data(
                    f.dir.path(),
                    &f.state.proxy_configs().lock().unwrap(),
                    &f.state.subscriptions().lock().unwrap(),
                )
                .unwrap();
            }
            _ => {
                f.state.client_core_configuration_committed(None);
            }
        }
        assert!(f
            .host
            .uninstall_execute(&f.state, &p.plan_id, OP, true)
            .is_err());
        assert!(f
            .host
            .store()
            .unwrap()
            .list()
            .unwrap()
            .contains(&PLUGIN.into()));
    }
}
#[test]
fn package_confirmed_then_later_failure_stays_discoverable_without_package_card() {
    let f = fixture(false);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    *f.host.uninstalls.fail_at.lock().unwrap() = Some("plugin_record".into());
    let s = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(s.state, "residual");
    assert!(s
        .items
        .iter()
        .any(|i| i.kind == "package_registration" && i.state == "completed"));
    assert!(!f
        .host
        .store()
        .unwrap()
        .list()
        .unwrap()
        .contains(&PLUGIN.into()));
    assert!(s
        .items
        .iter()
        .any(|i| i.kind == "plugin_record" && i.state == "residual"));
    assert_eq!(
        f.host.uninstall_preview(&f.state, PLUGIN).unwrap_err().code,
        "plugin_uninstall_pending"
    );
    let reopened = Host {
        root: Some(f.dir.path().into()),
        ..Default::default()
    };
    assert_eq!(reopened.uninstall_status(OP).unwrap(), s);
    assert_eq!(reopened.uninstall_recent().unwrap().len(), 1);
    assert!(std::fs::read(f.dir.path().join("configurations.json"))
        .unwrap()
        .windows(12)
        .any(|w| w == b"secret-canar"));
}
#[test]
fn lost_package_receipt_is_unknown_and_second_execute_never_deletes_again() {
    let f = fixture(false);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    *f.host.uninstalls.fail_after.lock().unwrap() = Some("package_registration".into());
    let first = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(first.state, "unknown");
    assert!(first
        .items
        .iter()
        .any(|i| i.kind == "package_registration" && i.state == "unknown"));
    let before = std::fs::read(f.dir.path().join("installed.json")).unwrap();
    let replay = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(first, replay);
    assert_eq!(
        std::fs::read(f.dir.path().join("installed.json")).unwrap(),
        before
    );
    assert!(f.host.uninstall_preview(&f.state, PLUGIN).is_err());
    let exe = std::env::current_exe().unwrap();
    let output = std::process::Command::new(exe)
        .args([
            "--exact",
            "services::plugins::uninstall::tests::unknown_child_driver",
            "--nocapture",
        ])
        .env("ZNET_UNINSTALL_TEST_ROOT", f.dir.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn unknown_child_driver() {
    let Some(root) = std::env::var_os("ZNET_UNINSTALL_TEST_ROOT") else {
        return;
    };
    let host = Host {
        root: Some(root.into()),
        ..Default::default()
    };
    let s = host.uninstall_status(OP).unwrap();
    assert_eq!(s.state, "unknown");
    assert!(s
        .items
        .iter()
        .any(|i| i.kind == "package_registration" && i.state == "unknown"));
    assert!(host.uninstall_available(PLUGIN).is_err());
}
#[test]
fn malformed_receipt_metadata_never_clears_the_gate() {
    let f = fixture(false);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    f.host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    let path = ledger_path(f.dir.path());
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let receipts = value["records"][0]["receipts"].as_object_mut().unwrap();
    let key = receipts.keys().next().unwrap().clone();
    receipts.insert(key, serde_json::Value::Null);
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(f.host.uninstall_status(OP).is_err());
    assert!(f.host.uninstall_available(PLUGIN).is_err());
}
#[test]
fn status_and_recent_missing_root_do_not_create_business_or_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("not-created");
    let host = Host {
        root: Some(root.clone()),
        ..Default::default()
    };
    assert!(host.uninstall_status(OP).is_err());
    assert!(host.uninstall_recent().unwrap().is_empty());
    assert!(!root.exists());
}
#[test]
fn namespace_unknown_files_and_symlinks_are_not_recursive_delete_authority() {
    let f = fixture(false);
    let publisher = f.host.namespace_publisher(PLUGIN).unwrap();
    let dir = f.dir.path().join("namespaces").join(publisher).join(PLUGIN);
    std::fs::write(dir.join("unrecognized-secret"), b"secret-canary").unwrap();
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert!(!p.can_execute);
    assert!(p.blocked_reasons.iter().any(|r| r.contains("未识别")));
    assert!(f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .is_err());
    assert!(dir.join("unrecognized-secret").exists());
}
#[cfg(unix)]
#[test]
fn replaced_namespace_ancestor_after_preview_is_rejected_without_touching_external_canary() {
    use std::os::unix::fs::symlink;
    let f = fixture(false);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    let publisher = f.host.namespace_publisher(PLUGIN).unwrap();
    let ancestor = f.dir.path().join("namespaces").join(publisher);
    let external = tempfile::tempdir().unwrap();
    let old = ancestor.with_extension("original");
    std::fs::rename(&ancestor, &old).unwrap();
    let target = external.path().join(PLUGIN);
    std::fs::create_dir(&target).unwrap();
    std::fs::copy(
        old.join(PLUGIN).join("state.json"),
        target.join("state.json"),
    )
    .unwrap();
    symlink(external.path(), &ancestor).unwrap();
    assert!(f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .is_err());
    assert!(target.join("state.json").exists());
    assert!(f
        .host
        .store()
        .unwrap()
        .list()
        .unwrap()
        .contains(&PLUGIN.into()));
}
#[test]
fn expiry_and_forged_confirmation_do_not_start_or_persist_an_operation() {
    let f = fixture(false);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert!(f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, false)
        .is_err());
    f.host
        .uninstalls
        .plans
        .lock()
        .unwrap()
        .get_mut(&p.plan_id)
        .unwrap()
        .deadline = Instant::now() - Duration::from_secs(1);
    assert!(f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .is_err());
    assert!(f.host.uninstall_recent().unwrap().is_empty());
}
#[test]
fn utf8_names_are_bounded_before_durable_prepare_and_remain_readable() {
    let f = fixture(false);
    let name = "配置资源".repeat(128);
    let mut profiles = f.state.proxy_configs().lock().unwrap();
    profiles[0].name = name.clone();
    drop(profiles);
    app_database::save_domain_data(
        f.dir.path(),
        &f.state.proxy_configs().lock().unwrap(),
        &f.state.subscriptions().lock().unwrap(),
    )
    .unwrap();
    f.host
        .state
        .lock()
        .unwrap()
        .directory
        .as_mut()
        .unwrap()
        .plugins[0]
        .name = "插件发布者".repeat(64);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert!(p.plugin_name.len() <= 128);
    assert!(p.resources.iter().all(|r| r.name.len() <= 128));
    assert!(p
        .resources
        .iter()
        .any(|r| r.kind == "subscription" && !r.dependencies.is_empty()));
    let s = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(s.state, "completed");
    assert_eq!(f.host.uninstall_status(OP).unwrap(), s);
}
#[test]
fn actual_store_history_rejects_changed_package_metadata_without_missing_success() {
    let f = fixture(false);
    let registration = f
        .host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find(PLUGIN)
        .unwrap()
        .clone();
    let store = f.host.store().unwrap();
    let (before, _) = store.review_uninstall(&registration).unwrap();
    let receipt = store.uninstall_checked(&registration, &before).unwrap();
    assert_eq!(receipt.after_epoch, before.store_epoch + 1);
    assert!(receipt.blobs_retained);
    assert!(store.uninstall_checked(&registration, &before).is_err());
    assert!(store.review_uninstall(&registration).is_err());
}
#[test]
fn journal_capacity_never_evicts_unfinished_records() {
    let f = fixture(false);
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    *f.host.uninstalls.fail_at.lock().unwrap() = Some("component".into());
    let _ = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    let r = read_ledger(f.dir.path()).unwrap().records[0].clone();
    let mut l = Ledger {
        schema: 1,
        records: vec![],
    };
    for n in 0..16 {
        let mut x = r.clone();
        x.status.operation_id = format!("{n:032x}");
        x.status.plugin_id = format!("plugin-{n}");
        x.receipts.clear();
        l.records.push(x);
    }
    write_ledger(f.dir.path(), &l).unwrap();
    let mut extra = r;
    extra.status.operation_id = "ffffffffffffffffffffffffffffffff".into();
    assert!(record_write(f.dir.path(), &extra).is_err());
    assert_eq!(read_ledger(f.dir.path()).unwrap().records.len(), 16);
}
#[test]
fn cooperative_root_lock_serializes_a_real_second_process_writer() {
    let f = fixture(false);
    let guard = storage::RootGuard::acquire(f.dir.path()).unwrap();
    let before = guard.epoch().unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "services::plugins::uninstall::tests::writer_child_driver",
            "--nocapture",
        ])
        .env("ZNET_UNINSTALL_WRITER_ROOT", f.dir.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !f.dir.path().join("writer-ready").exists() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(guard.epoch().unwrap(), before);
    assert!(child.try_wait().unwrap().is_none());
    drop(guard);
    let result = child.wait_with_output().unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        storage::RootGuard::acquire(f.dir.path())
            .unwrap()
            .epoch()
            .unwrap()
            .epoch
            > before.epoch
    );
}
#[test]
fn writer_child_driver() {
    let Some(root) = std::env::var_os("ZNET_UNINSTALL_WRITER_ROOT") else {
        return;
    };
    let root = PathBuf::from(root);
    std::fs::write(root.join("writer-ready"), b"ready").unwrap();
    super::super::configuration::save(&root, "org.example.plugin/identity", BTreeMap::new())
        .unwrap();
}
fn other_state(f: &Fixture) -> AppState {
    let state = AppState::with_domain_data(
        f.state.app_config().lock().unwrap().clone(),
        f.state.proxy_configs().lock().unwrap().clone(),
        f.state.subscriptions().lock().unwrap().clone(),
        f.state.rule_sets().lock().unwrap().clone(),
        vec![],
    );
    workspace::recovery::set_test_directory(&state, f.dir.path().into());
    state
}
#[test]
fn other_host_inflight_reservation_blocks_delete_before_any_effect() {
    let f = fixture(false);
    let b = Host {
        root: Some(f.dir.path().into()),
        ..Default::default()
    };
    let bs = other_state(&f);
    let cold = b.snapshot(bs.capabilities());
    assert!(cold.checked);
    assert_eq!(cold.components.len(), 1);
    let activity = f.host.activity(PLUGIN).unwrap();
    let p = b.uninstall_preview(&bs, PLUGIN).unwrap();
    assert!(!p.can_execute);
    assert!(p.blocked_reasons.iter().any(|r| r.contains("另一宿主")));
    assert!(b.uninstall_execute(&bs, &p.plan_id, OP, true).is_err());
    assert!(b.uninstall_recent().unwrap().is_empty());
    drop(activity);
    let fresh = b.uninstall_preview(&bs, PLUGIN).unwrap();
    assert!(fresh.can_execute, "{:?}", fresh.blocked_reasons);
    assert_eq!(
        b.uninstall_execute(&bs, &fresh.plan_id, OP, true)
            .unwrap()
            .state,
        "completed"
    );
}
#[test]
fn old_host_cannot_recreate_namespace_after_other_host_completion_or_same_material_reinstall() {
    let f = fixture(false);
    let registration = f
        .host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find(PLUGIN)
        .unwrap()
        .clone();
    let (review, _) = f
        .host
        .store()
        .unwrap()
        .review_uninstall(&registration)
        .unwrap();
    let bytes = std::fs::read(
        f.dir
            .path()
            .join("packages")
            .join(format!("{}.zspkg", review.current_sha256)),
    )
    .unwrap();
    let old_generation = f
        .host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .installation_generation;
    let b = Host {
        root: Some(f.dir.path().into()),
        ..Default::default()
    };
    let bs = other_state(&f);
    let cold = b.snapshot(bs.capabilities());
    assert!(cold.checked);
    assert_eq!(cold.components.len(), 1);
    let p = b.uninstall_preview(&bs, PLUGIN).unwrap();
    assert_eq!(
        b.uninstall_execute(&bs, &p.plan_id, OP, true)
            .unwrap()
            .state,
        "completed"
    );
    assert!(f
        .host
        .storage_put(
            PLUGIN,
            super::super::namespace::Area::State,
            "late".into(),
            STANDARD.encode(b"late-secret-canary")
        )
        .is_err());
    f.host
        .store()
        .unwrap()
        .install(
            &bytes,
            &registration,
            &Target::native_desktop().unwrap(),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
    let (fresh, _) = f
        .host
        .store()
        .unwrap()
        .review_uninstall(&registration)
        .unwrap();
    assert!(fresh.installation_generation > old_generation);
    assert_eq!(fresh.package_digest, review.package_digest);
    assert!(f
        .host
        .storage_put(
            PLUGIN,
            super::super::namespace::Area::State,
            "late".into(),
            STANDARD.encode(b"late-secret-canary")
        )
        .is_err());
    // Explicit rescan creates a new material generation; the old manager entry is
    // retired before admission even for identical package bytes.
    let old_registration = f.state.capabilities().component_snapshots()[0].registration;
    f.host.checked_rescan(f.state.capabilities()).unwrap();
    let new = f.state.capabilities().component_snapshots();
    assert!(new.iter().any(|p| p.registration != old_registration));
    f.host
        .storage_put(
            PLUGIN,
            super::super::namespace::Area::State,
            "fresh".into(),
            STANDARD.encode(b"fresh"),
        )
        .unwrap();
}
#[cfg(unix)]
#[test]
fn actual_runtime_listener_observes_zero_connections_during_inactive_uninstall() {
    use std::os::unix::net::UnixListener;
    let f = fixture(false);
    let path = f.dir.path().join("must-not-connect.sock");
    let listener = UnixListener::bind(&path).unwrap();
    listener.set_nonblocking(true).unwrap();
    {
        let mut app = f.state.app_config().lock().unwrap();
        app.core.socket = Some(path.to_string_lossy().into_owned());
        app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
    }
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert_eq!(
        f.host
            .uninstall_execute(&f.state, &p.plan_id, OP, true)
            .unwrap()
            .state,
        "completed"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
#[test]
fn malformed_installation_generation_map_is_not_legacy_or_fresh_authority() {
    let f = fixture(false);
    let registration = f
        .host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find(PLUGIN)
        .unwrap()
        .clone();
    let store = f.host.store().unwrap();
    let path = f.dir.path().join("installed.json");
    let original: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    for variant in 0..4 {
        let mut value = original.clone();
        match variant {
            0 => {
                value["generations"].as_object_mut().unwrap().remove(PLUGIN);
            }
            1 => {
                value["generations"][PLUGIN] = json!(0);
            }
            2 => {
                value["packages"][PLUGIN]["generation"] = json!(0);
            }
            _ => {
                value["generations"]["other"] = json!(0);
            }
        }
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(store.review_uninstall(&registration).is_err());
        assert!(store
            .uninstall_checked(
                &registration,
                &znet_plugin_sandbox::distribution::store::UninstallReview {
                    plugin_id: PLUGIN.into(),
                    store_digest: "0".repeat(64),
                    store_epoch: 0,
                    installation_generation: 0,
                    package_digest: "0".repeat(64),
                    current_sha256: "0".repeat(64),
                    previous_sha256: None
                }
            )
            .is_err());
    }
    // A genuinely old store has neither field. It may be read as generation zero;
    // the first real mutation starts a new persistent per-plugin chain.
    let mut legacy = original;
    legacy.as_object_mut().unwrap().remove("generations");
    legacy["packages"][PLUGIN]
        .as_object_mut()
        .unwrap()
        .remove("generation");
    std::fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
    let (review, _) = store.review_uninstall(&registration).unwrap();
    assert_eq!(review.installation_generation, 0);
    let receipt = store.uninstall_checked(&registration, &review).unwrap();
    assert!(receipt.after_epoch > review.store_epoch);
}
#[test]
fn actual_installation_lock_contention_has_a_fixed_deadline_and_explicit_release() {
    let f = fixture(false);
    let registration = f
        .host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find(PLUGIN)
        .unwrap()
        .clone();
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(f.dir.path().join("install.lock"))
        .unwrap();
    file.lock().unwrap();
    let before = std::fs::read(f.dir.path().join("installed.json")).unwrap();
    let started = Instant::now();
    assert!(f
        .host
        .store()
        .unwrap()
        .review_uninstall(&registration)
        .is_err());
    assert!(started.elapsed() >= Duration::from_secs(5));
    assert!(started.elapsed() < Duration::from_secs(8));
    assert_eq!(
        std::fs::read(f.dir.path().join("installed.json")).unwrap(),
        before
    );
    file.unlock().unwrap();
    assert!(f
        .host
        .store()
        .unwrap()
        .review_uninstall(&registration)
        .is_ok());
}
#[test]
fn unknown_uninstall_does_not_accept_late_scheduler_checkpoint() {
    let f = fixture(false);
    let task = f
        .host
        .schedule_put(PLUGIN, "identity", "refresh".into(), "refresh".into(), 300)
        .unwrap();
    let generation = f
        .host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .installation_generation;
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    *f.host.uninstalls.fail_at.lock().unwrap() = Some("component".into());
    let status = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(status.state, "residual");
    let before = std::fs::read(f.dir.path().join("schedules.json")).unwrap();
    assert!(f
        .host
        .record_schedule_result(&task, generation, false, 1000)
        .is_err());
    assert!(f.host.take_due(u64::MAX).unwrap().is_empty());
    assert_eq!(
        std::fs::read(f.dir.path().join("schedules.json")).unwrap(),
        before
    );
}
#[test]
fn old_scheduler_completion_cannot_modify_same_id_task_after_same_material_reinstall() {
    let f = fixture(false);
    let old = f
        .host
        .schedule_put(PLUGIN, "identity", "refresh".into(), "refresh".into(), 300)
        .unwrap();
    let generation = f
        .host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .installation_generation;
    let registration = f
        .host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find(PLUGIN)
        .unwrap()
        .clone();
    let (review, _) = f
        .host
        .store()
        .unwrap()
        .review_uninstall(&registration)
        .unwrap();
    let bytes = std::fs::read(
        f.dir
            .path()
            .join("packages")
            .join(format!("{}.zspkg", review.current_sha256)),
    )
    .unwrap();
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert_eq!(
        f.host
            .uninstall_execute(&f.state, &p.plan_id, OP, true)
            .unwrap()
            .state,
        "completed"
    );
    f.host
        .store()
        .unwrap()
        .install(
            &bytes,
            &registration,
            &Target::native_desktop().unwrap(),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
    f.host.checked_rescan(f.state.capabilities()).unwrap();
    let new = f
        .host
        .schedule_put(PLUGIN, "identity", "refresh".into(), "refresh".into(), 300)
        .unwrap();
    let before = std::fs::read(f.dir.path().join("schedules.json")).unwrap();
    assert!(f
        .host
        .record_schedule_result(&old, generation, false, 1000)
        .is_err());
    assert_eq!(
        std::fs::read(f.dir.path().join("schedules.json")).unwrap(),
        before
    );
    let fresh = f
        .host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .installation_generation;
    f.host
        .record_schedule_result(&new, fresh, false, 1000)
        .unwrap();
    assert_ne!(
        std::fs::read(f.dir.path().join("schedules.json")).unwrap(),
        before
    );
}
#[test]
fn orphan_overlay_key_cannot_echo_a_url_path_or_secret_into_public_preview() {
    let f = fixture(false);
    let secret = "https://user:secret-canary@private.example/path/token";
    // A real selected owned rule makes unknown orphan overlays relevant to the
    // deletion proof; an empty rule inventory has no such reference authority.
    let subscription = f.state.subscriptions().lock().unwrap()[0].id.clone();
    let rule = RuleSetProfile {
        id: format!(
            "subscription-rule-{subscription}-{}",
            &storage::hash(b"owned-tag")[..24]
        ),
        name: "Owned rules".into(),
        enabled: true,
        built_in: false,
        provenance: None,
        managed_by_subscription_id: Some(subscription),
        common_binding: None,
        semantic_ir: json!({"version":1,"rules":[]}),
        source: None,
        source_state: Default::default(),
        artifact: None,
        updated_at_unix_ms: 1,
        last_sync_at_unix_ms: None,
        last_error: None,
    };
    {
        let mut rules = f.state.rule_sets().lock().unwrap();
        rules.push(rule);
        domain_store::save_rule_sets_to_dir(f.dir.path(), &rules).unwrap();
    }
    {
        let mut app = f.state.app_config().lock().unwrap();
        app.profile_edits.insert(
            secret.into(),
            BTreeMap::from([(
                "routeRules".into(),
                json!({"version":1,"sourceDigest":"0".repeat(64),"rows":[]}),
            )]),
        );
        app_config_store::save(&f.dir.path().join("app-config.json"), &app).unwrap();
    }
    let preview = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert!(!preview.can_execute);
    let public = serde_json::to_string(&preview).unwrap();
    assert!(!public.contains("secret-canary"));
    assert!(!public.contains("private.example"));
    assert!(preview
        .blocked_reasons
        .iter()
        .any(|reason| reason.contains("未匹配的记录")));
}
#[test]
fn signed_process_only_package_without_vm_authority_can_remove_real_registration() {
    use znet_plugin_sandbox::{
        contract::{Limits, Manifest},
        distribution::package::{self, Payload, SourceComponent},
    };
    let f = fixture(false);
    let registration = f
        .host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find(PLUGIN)
        .unwrap()
        .clone();
    let source = "true";
    let manifest:Manifest=serde_json::from_value(json!({"schema_version":1,"host":"znet-sink","plugin_id":PLUGIN,"component_id":"identity","version":"2.0.0","requires_host":format!("={}",env!("CARGO_PKG_VERSION")),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"process","targets":"any","required":[{"capability":"plugin.self.read","scope":"self"}],"optional":[],"source_sha256":storage::hash(source.as_bytes()),"limits":Limits::default()})).unwrap();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: PLUGIN.into(),
        version: "2.0.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
        pages: vec![],
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &[17; 32]).unwrap();
    f.host
        .store()
        .unwrap()
        .install(
            &bytes,
            &registration,
            &Target::native_desktop().unwrap(),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
    f.host.checked_rescan(f.state.capabilities()).unwrap();
    let before = f.host.snapshot(f.state.capabilities());
    assert_eq!(before.components.len(), 1);
    assert!(before.components[0]
        .blocked
        .as_ref()
        .is_some_and(|reason| reason.contains("运行方式")));
    assert!(before.components[0].review.is_none());
    assert!(!before.components[0].enabled);
    assert!(f.state.capabilities().component_snapshots().is_empty());
    let p = f.host.uninstall_preview(&f.state, PLUGIN).unwrap();
    assert!(p.can_execute, "{:?}", p.blocked_reasons);
    assert!(p.resources.iter().any(|r| r.kind == "package_registration"));
    assert!(!p.resources.iter().any(|r| r.kind == "component"));
    assert!(p
        .resources
        .iter()
        .find(|r| r.kind == "package_registration")
        .unwrap()
        .dependencies
        .iter()
        .any(|value| value.contains("identity")));
    let result = f
        .host
        .uninstall_execute(&f.state, &p.plan_id, OP, true)
        .unwrap();
    assert_eq!(result.state, "completed");
    assert!(result
        .items
        .iter()
        .any(|i| i.kind == "package_registration" && i.state == "completed"));
    assert!(!result.items.iter().any(|i| i.kind == "component"));
    assert!(!f
        .host
        .store()
        .unwrap()
        .list()
        .unwrap()
        .contains(&PLUGIN.into()));
    assert!(f
        .host
        .snapshot(f.state.capabilities())
        .components
        .is_empty());
    assert_eq!(f.host.uninstall_status(OP).unwrap(), result);
}
