use super::*;
use crate::models::{
    app_config::AppTunConfig,
    dns_config::{ClientDnsAnswer, ClientDnsConfig},
    zero_runtime::GuiTunStatus,
};
use std::sync::Mutex;

fn settings(mode: &str) -> AppConfig {
    let mut config = AppConfig::default();
    config.dns.enabled = mode != "disabled";
    config.dns.config = Some(ClientDnsConfig::recommended_default());
    if mode == "real" {
        config.dns.config.as_mut().unwrap().answer = ClientDnsAnswer::Real;
    }
    config.dns.dns_hijack = config.dns.enabled;
    config.tun.dns_hijack = config.dns.enabled;
    config.tun.enabled = Some(true);
    config
}

fn status(tun: &AppTunConfig, enabled: bool) -> GuiTunStatus {
    GuiTunStatus {
        enabled,
        healthy: enabled,
        supported: true,
        auto_route: true,
        strict_route: true,
        addr: Some(tun.addr.clone()),
        mtu: Some(tun.mtu),
        tag: Some(tun.tag.clone()),
        name: tun.name.clone(),
        dual_stack: tun.dual_stack,
        dns_hijack: tun.dns_hijack,
        include_cidrs: Some(tun.include_cidrs.clone()),
        exclude_cidrs: Some(tun.exclude_cidrs.clone()),
        ..Default::default()
    }
}

struct Fake(Mutex<Data>);
struct Data {
    saved: AppConfig,
    status: GuiTunStatus,
    identity: KernelRuntimeIdentity,
    calls: Vec<String>,
    applied: Value,
    failure: &'static str,
    failed: bool,
}

impl Fake {
    fn new(previous: &AppConfig, enabled: bool, failure: &'static str) -> Self {
        Self(Mutex::new(Data {
            saved: previous.clone(),
            status: status(&previous.tun, enabled),
            identity: KernelRuntimeIdentity {
                core_instance_id: "core".into(),
                config_revision: 1,
            },
            calls: vec![],
            applied: "old".into(),
            failure,
            failed: false,
        }))
    }
}
fn fail(data: &mut Data, operation: &str) -> AppResult<()> {
    if data.failure == operation && !data.failed {
        data.failed = true;
        return Err(AppError::internal(format!("{operation} rejected")));
    }
    Ok(())
}
impl tun::Backend for Fake {
    async fn snapshot(&self) -> AppResult<Snapshot> {
        let data = self.0.lock().unwrap();
        Ok(Snapshot {
            identity: data.identity.clone(),
            status: data.status.clone(),
        })
    }
    async fn stop(&self) -> AppResult<()> {
        let mut data = self.0.lock().unwrap();
        data.calls.push("stop".into());
        fail(&mut data, "stop")?;
        data.status.enabled = false;
        Ok(())
    }
    async fn start(&self, tun: &AppTunConfig) -> AppResult<()> {
        let mut data = self.0.lock().unwrap();
        data.calls.push(format!("start:{}", tun.dns_hijack));
        fail(&mut data, "start")?;
        data.status = status(tun, true);
        Ok(())
    }
    fn persist(&self, config: &AppConfig) -> AppResult<()> {
        let mut data = self.0.lock().unwrap();
        data.calls.push("persist".into());
        // Model an export failure after the app settings were already written.
        data.saved = config.clone();
        fail(&mut data, "persist")
    }
}
impl Backend for Fake {
    async fn apply_dns(&self, config: &Value) -> AppResult<(Value, KernelRuntimeIdentity)> {
        let mut data = self.0.lock().unwrap();
        data.calls
            .push(format!("apply:{}", config.as_str().unwrap()));
        fail(&mut data, "apply")?;
        if data.failure == "timeout" {
            return Err(AppError {
                code: "timeout",
                message: "response lost".into(),
                details: None,
            });
        }
        data.applied = config.clone();
        data.identity.config_revision += 1;
        let applied = data.identity.clone();
        if data.failure == "external" {
            data.identity.core_instance_id = "other".into();
        }
        if data.failure == "revision" {
            data.identity.config_revision += 1;
        }
        Ok((serde_json::json!({"ok":true}), applied))
    }
}

#[tokio::test]
async fn all_dns_mode_transitions_preserve_tun_intent_and_wait_for_actual_parameters() {
    for old in ["disabled", "real", "fake"] {
        for next in ["disabled", "real", "fake"] {
            for enabled in [false, true] {
                let previous = settings(old);
                let candidate = settings(next);
                let backend = Fake::new(&previous, enabled, "");
                apply(
                    &backend,
                    &previous,
                    &candidate,
                    &"old".into(),
                    &"next".into(),
                )
                .await
                .unwrap();
                let data = backend.0.lock().unwrap();
                assert_eq!(data.saved, candidate);
                assert_eq!(data.status.enabled, enabled);
                if enabled {
                    assert_eq!(data.status.dns_hijack, candidate.tun.dns_hijack);
                    assert_eq!(
                        data.calls,
                        vec![
                            "stop".to_string(),
                            "apply:next".into(),
                            format!("start:{}", candidate.tun.dns_hijack),
                            "persist".into()
                        ]
                    );
                } else {
                    assert_eq!(data.calls, vec!["apply:next", "persist"]);
                }
            }
        }
    }
}

#[tokio::test]
async fn rejected_apply_start_or_storage_restores_previous_dns_tun_and_disk() {
    for failure in ["stop", "apply", "start", "persist"] {
        let previous = settings("fake");
        let candidate = settings("disabled");
        let backend = Fake::new(&previous, true, failure);
        let error = apply(
            &backend,
            &previous,
            &candidate,
            &"old".into(),
            &"next".into(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            error.details.unwrap()["dnsRollback"]["succeeded"],
            true,
            "{failure}"
        );
        let data = backend.0.lock().unwrap();
        assert_eq!(data.saved, previous);
        assert_eq!(data.applied, "old");
        assert!(data.status.enabled && data.status.healthy && data.status.dns_hijack);
        if failure == "stop" {
            assert_eq!(data.calls, vec!["stop"]);
        }
    }
}

#[tokio::test]
async fn unknown_completion_or_foreign_instance_never_triggers_competing_recovery() {
    for failure in ["timeout", "external", "revision"] {
        let previous = settings("fake");
        let candidate = settings("disabled");
        let backend = Fake::new(&previous, true, failure);
        let error = apply(
            &backend,
            &previous,
            &candidate,
            &"old".into(),
            &"next".into(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.details.unwrap()["dnsRollback"]["attempted"], false);
        assert_eq!(backend.0.lock().unwrap().calls, vec!["stop", "apply:next"]);
    }
}

#[tokio::test]
async fn profile_owned_tun_is_never_stopped_or_started_by_dns_settings() {
    let previous = settings("fake");
    let candidate = settings("real");
    let backend = Fake::new(&previous, true, "");
    backend.0.lock().unwrap().status.managed_by_config = true;
    apply(
        &backend,
        &previous,
        &candidate,
        &"old".into(),
        &"next".into(),
    )
    .await
    .unwrap();
    assert_eq!(
        backend.0.lock().unwrap().calls,
        vec!["apply:next", "persist"]
    );
}
