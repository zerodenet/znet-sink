use super::*;
use crate::{
    client_core::SourceStatus,
    models::{
        app_config::AppConfig,
        proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile},
        subscription::{ManagedSubscriptionSource, SubscriptionProfile},
    },
};
use serde_json::{json, Value};

fn profile(id: &str, active: bool) -> ProxyConfigProfile {
    ProxyConfigProfile {
        id: id.into(),
        name: "same visible name".into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: Some("/private/secret-canary-path".into()),
        content: Some(
            json!({"private":"secret-canary-body","route":{"rules":[],"final":{"type":"direct"}}}),
        ),
        active,
        managed_source: None,
        updated_at_unix_ms: u64::MAX,
        capabilities: ProxyConfigCapabilities::default(),
    }
}
fn state(profiles: Vec<ProxyConfigProfile>) -> AppState {
    AppState::with_domain_data(AppConfig::default(), profiles, vec![], vec![], vec![])
}
fn bound(state: &AppState) -> Binding {
    snapshot(state).unwrap().binding.unwrap()
}
fn subscription() -> SubscriptionProfile {
    serde_json::from_value(json!({"id":"subscription","name":"secret-canary-name","url":"https://secret-canary.invalid/?token=hidden","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":"a","updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null})).unwrap()
}

#[test]
fn host_restart_does_not_reuse_a_binding_even_with_identical_scope_generation_and_directory() {
    let first = state(vec![profile("a", true)]);
    let second = state(vec![profile("a", true)]);
    assert_eq!(
        first.client_core_snapshot().scope,
        second.client_core_snapshot().scope
    );
    assert_eq!(
        first.client_core_snapshot().scope_generation,
        second.client_core_snapshot().scope_generation
    );
    assert_ne!(bound(&first).binding, bound(&second).binding);
    assert_eq!(bound(&first).binding, bound(&first).binding);
}
#[test]
fn same_scope_aba_after_lifecycle_change_retires_original_location() {
    let p = profile("a", true);
    let state = state(vec![p.clone()]);
    let before = bound(&state);
    state.client_core_configuration_committed(None);
    state.client_core_configuration_committed(Some(&p));
    let after = bound(&state);
    assert_eq!(before.profile_id, after.profile_id);
    assert_eq!(before.config_revision, after.config_revision);
    assert_eq!(before.core_instance_id, after.core_instance_id);
    assert_ne!(before.scope_generation, after.scope_generation);
    assert_ne!(before.binding, after.binding);
}
#[test]
fn same_visible_name_never_substitutes_another_profile_identity() {
    let state = state(vec![profile("a", true), profile("b", false)]);
    let before = snapshot(&state).unwrap();
    assert_eq!(before.profile.as_ref().unwrap().id, "a");
    let mut profiles = lock(state.proxy_configs(), "profiles").unwrap();
    profiles[0].active = false;
    profiles[1].active = true;
    let active = profiles[1].clone();
    drop(profiles);
    state.client_core_configuration_committed(Some(&active));
    let after = snapshot(&state).unwrap();
    assert_eq!(
        before.profile.unwrap().name,
        after.profile.as_ref().unwrap().name
    );
    assert_eq!(after.profile.as_ref().unwrap().id, "b");
    assert_ne!(
        before.binding.unwrap().binding,
        after.binding.unwrap().binding
    );
}
#[test]
fn same_timestamp_public_metadata_and_real_relationship_changes_retire_binding() {
    let state = AppState::with_domain_data(
        AppConfig::default(),
        vec![profile("a", true), profile("b", false)],
        vec![subscription()],
        vec![],
        vec![],
    );
    let before = bound(&state);
    let core = state.client_core_snapshot();
    lock(state.proxy_configs(), "p").unwrap()[0].name = "renamed at same timestamp".into();
    let renamed = bound(&state);
    assert_ne!(before.binding, renamed.binding);
    assert_eq!(
        core.scope_generation,
        state.client_core_snapshot().scope_generation
    );
    lock(state.subscriptions(), "s").unwrap()[0].target_proxy_config_id = Some("b".into());
    let relinked = bound(&state);
    assert_ne!(renamed.binding, relinked.binding);
    assert_eq!(core.scope, state.client_core_snapshot().scope);
}
#[test]
fn ordinary_read_observation_revision_does_not_retire_navigation_identity() {
    let state = state(vec![profile("a", true)]);
    let before = bound(&state);
    let core = state.client_core_snapshot();
    state.set_client_core_source_status(SourceStatus::Ready);
    let after = state.client_core_snapshot();
    assert_ne!(core.revision, after.revision);
    assert_eq!(core.scope_generation, after.scope_generation);
    assert_eq!(before.binding, bound(&state).binding);
}
#[test]
fn inconsistent_directory_scope_or_relationship_has_no_executable_location() {
    for mode in 0..4 {
        let state = state(vec![profile("a", true), profile("b", false)]);
        match mode {
            0 => lock(state.proxy_configs(), "p")
                .unwrap()
                .push(profile("a", false)),
            1 => lock(state.proxy_configs(), "p").unwrap()[1].active = true,
            2 => {
                let mut p = lock(state.proxy_configs(), "p").unwrap();
                p[0].active = false;
                p[1].active = true;
            }
            _ => {
                let mut s = subscription();
                s.target_proxy_config_id = Some("missing".into());
                lock(state.subscriptions(), "s").unwrap().push(s);
            }
        }
        let view = snapshot(&state).unwrap();
        assert!(view.binding.is_none());
        assert!(view.profile.is_none());
        assert!(view.reason.is_some());
    }
    let view = snapshot(&state(vec![])).unwrap();
    assert!(view.binding.is_none());
    assert!(view.profile.is_none());
    assert!(view.reason.as_ref().unwrap().contains("尚未选择"));
}
#[test]
fn protected_and_managed_profiles_expose_only_public_identity_and_lossless_scope() {
    for protected in [false, true] {
        let mut p = profile("a", true);
        p.managed_source = Some(ManagedSubscriptionSource {
            plugin_id: "secret-canary-publisher".into(),
            provider_id: "secret-canary-provider".into(),
            remote_subscription_id: "secret-canary-remote".into(),
            revision: None,
            source_name: None,
        });
        if protected {
            p.content = None;
        }
        let state = state(vec![p]);
        let view = snapshot(&state).unwrap();
        let encoded = serde_json::to_string(&view).unwrap();
        assert!(!encoded.contains("secret-canary"));
        assert_eq!(view.profile.as_ref().unwrap().id, "a");
        let binding = view.binding.unwrap();
        assert_eq!(
            binding.config_revision,
            state
                .client_core_snapshot()
                .scope
                .config_revision
                .0
                .to_string()
        );
        assert_eq!(binding.core_instance_id, "0");
        assert_eq!(binding.scope_generation, "1");
    }
    let max = u64::MAX.to_string();
    let value = serde_json::to_value(Binding {
        binding: "opaque".into(),
        profile_id: "a".into(),
        config_revision: max.clone(),
        core_instance_id: max.clone(),
        scope_generation: max.clone(),
    })
    .unwrap();
    for key in ["configRevision", "coreInstanceId", "scopeGeneration"] {
        assert_eq!(value[key], Value::String(max.clone()));
    }
}
#[test]
fn read_only_snapshot_needs_no_database_or_live_core_and_does_not_advance_any_scope() {
    let directory = tempfile::tempdir().unwrap();
    let socket = directory.path().join("uncontacted.sock");
    let mut app = AppConfig::default();
    app.core.socket = Some(socket.to_string_lossy().into_owned());
    #[cfg(unix)]
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    #[cfg(unix)]
    listener.set_nonblocking(true).unwrap();
    let state = AppState::with_domain_data(app, vec![profile("a", true)], vec![], vec![], vec![]);
    let before = state.client_core_snapshot();
    let view = snapshot(&state).unwrap();
    assert!(view.binding.is_some());
    let after = state.client_core_snapshot();
    assert_eq!(before.revision, after.revision);
    assert_eq!(before.scope, after.scope);
    assert_eq!(before.scope_generation, after.scope_generation);
    assert!(state.configuration().transaction().is_none());
    assert!(!directory.path().join("znet-sink.db").exists());
    #[cfg(unix)]
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
