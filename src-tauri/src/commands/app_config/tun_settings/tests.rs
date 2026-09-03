use std::sync::Mutex;

use super::transaction::{self, Backend, Snapshot};
use crate::errors::{AppError, AppResult};
use crate::kernel::zero::queries::KernelRuntimeIdentity;
use crate::models::app_config::{AppConfig, AppTunConfig};
use crate::models::zero_runtime::GuiTunStatus;

fn status(tun: &AppTunConfig) -> GuiTunStatus {
    GuiTunStatus {
        enabled: true,
        healthy: true,
        supported: true,
        addr: Some(tun.addr.clone()),
        mtu: Some(tun.mtu),
        tag: Some(tun.tag.clone()),
        name: tun.name.clone(),
        addresses: tun.secondary_addr.clone().into_iter().collect(),
        auto_route: true,
        strict_route: true,
        dual_stack: tun.dual_stack,
        dns_hijack: tun.dns_hijack,
        include_cidrs: Some(tun.include_cidrs.clone()),
        exclude_cidrs: Some(tun.exclude_cidrs.clone()),
        ..Default::default()
    }
}

struct State {
    saved: AppConfig,
    status: GuiTunStatus,
    calls: Vec<&'static str>,
    starts: usize,
    fail_start: bool,
    fail_rollback: bool,
    fail_persist: bool,
    fail_stop: bool,
    timeout: bool,
    late_success: bool,
    changed_core: bool,
    change_core_on_start: bool,
}

struct Fake(Mutex<State>);

impl Fake {
    fn new(config: &AppConfig) -> Self {
        Self(Mutex::new(State {
            saved: config.clone(),
            status: status(&config.tun),
            calls: vec![],
            starts: 0,
            fail_start: false,
            fail_rollback: false,
            fail_persist: false,
            fail_stop: false,
            timeout: false,
            late_success: false,
            changed_core: false,
            change_core_on_start: false,
        }))
    }
}

impl Backend for Fake {
    async fn snapshot(&self) -> AppResult<Snapshot> {
        let state = self.0.lock().unwrap();
        Ok(Snapshot {
            identity: KernelRuntimeIdentity {
                core_instance_id: if state.changed_core {
                    "new-core"
                } else {
                    "old-core"
                }
                .into(),
                config_revision: 4,
            },
            status: state.status.clone(),
        })
    }

    async fn stop(&self) -> AppResult<()> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("stop");
        if state.fail_stop {
            return Err(AppError::internal("stop rejected"));
        }
        state.status = GuiTunStatus {
            supported: true,
            ..Default::default()
        };
        Ok(())
    }

    async fn start(&self, tun: &AppTunConfig) -> AppResult<()> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("start");
        state.starts += 1;
        if state.change_core_on_start {
            state.changed_core = true;
            return Err(AppError::internal("core restarted"));
        }
        if (state.fail_start && state.starts == 1) || (state.fail_rollback && state.starts > 1) {
            return Err(AppError::internal("route installation failed"));
        }
        if !state.timeout || state.late_success {
            state.status = status(tun);
        }
        if state.timeout {
            return Err(AppError {
                code: "timeout",
                message: "reply lost".into(),
                details: None,
            });
        }
        Ok(())
    }

    fn persist(&self, config: &AppConfig) -> AppResult<()> {
        let mut state = self.0.lock().unwrap();
        state.calls.push("persist");
        state.saved = config.clone();
        if state.fail_persist {
            state.fail_persist = false;
            return Err(AppError::internal("disk write failed"));
        }
        Ok(())
    }
}

fn configs() -> (AppConfig, AppConfig) {
    let mut previous = AppConfig::default();
    previous.tun.enabled = Some(true);
    previous.tun.exclude_cidrs = vec!["16.0.0.0/8".into()];
    let mut candidate = previous.clone();
    candidate.tun.exclude_cidrs.push("203.0.113.10/32".into());
    (previous, candidate)
}

#[tokio::test]
async fn saves_only_after_running_tun_uses_the_new_exclusions() {
    let (previous, candidate) = configs();
    let backend = Fake::new(&previous);
    transaction::apply(&backend, &previous, &candidate, true)
        .await
        .unwrap();
    let state = backend.0.lock().unwrap();
    assert_eq!(state.calls, ["stop", "start", "persist"]);
    assert_eq!(state.saved, candidate);
    assert!(transaction::matches(&state.status, &candidate.tun));
}

#[tokio::test]
async fn failed_start_restores_old_runtime_and_settings() {
    let (previous, candidate) = configs();
    let backend = Fake::new(&previous);
    backend.0.lock().unwrap().fail_start = true;
    let error = transaction::apply(&backend, &previous, &candidate, true)
        .await
        .unwrap_err();
    let state = backend.0.lock().unwrap();
    assert_eq!(state.calls, ["stop", "start", "start", "persist"]);
    assert_eq!(state.saved, previous);
    assert!(transaction::matches(&state.status, &previous.tun));
    assert_eq!(error.details.unwrap()["tunRollback"]["succeeded"], true);
}

#[tokio::test]
async fn disk_failure_restores_runtime_and_rewrites_previous_settings() {
    let (previous, candidate) = configs();
    let backend = Fake::new(&previous);
    backend.0.lock().unwrap().fail_persist = true;
    transaction::apply(&backend, &previous, &candidate, true)
        .await
        .unwrap_err();
    let state = backend.0.lock().unwrap();
    assert_eq!(
        state.calls,
        ["stop", "start", "persist", "stop", "start", "persist"]
    );
    assert_eq!(state.saved, previous);
    assert!(transaction::matches(&state.status, &previous.tun));
}

#[tokio::test(start_paused = true)]
async fn lost_reply_with_verified_parameters_is_success_without_retry() {
    let (previous, candidate) = configs();
    let backend = Fake::new(&previous);
    {
        let mut state = backend.0.lock().unwrap();
        state.timeout = true;
        state.late_success = true;
    }
    transaction::apply(&backend, &previous, &candidate, true)
        .await
        .unwrap();
    assert_eq!(
        backend.0.lock().unwrap().calls,
        ["stop", "start", "persist"]
    );
}

#[tokio::test(start_paused = true)]
async fn unresolved_timeout_does_not_retry_or_race_a_rollback() {
    let (previous, candidate) = configs();
    let backend = Fake::new(&previous);
    backend.0.lock().unwrap().timeout = true;
    let error = transaction::apply(&backend, &previous, &candidate, true)
        .await
        .unwrap_err();
    assert_eq!(backend.0.lock().unwrap().calls, ["stop", "start"]);
    assert_eq!(backend.0.lock().unwrap().saved, previous);
    assert_eq!(error.details.unwrap()["tunTransitionUncertain"], true);
}

#[tokio::test]
async fn defaults_and_unchanged_settings_do_not_toggle_tun() {
    let (previous, candidate) = configs();
    for mode in ["offline", "stopped", "profile", "unchanged"] {
        let backend = Fake::new(&previous);
        match mode {
            "stopped" => backend.0.lock().unwrap().status.enabled = false,
            "profile" => backend.0.lock().unwrap().status.managed_by_config = true,
            _ => {}
        }
        let candidate = if mode == "unchanged" {
            &previous
        } else {
            &candidate
        };
        transaction::apply(&backend, &previous, candidate, mode != "offline")
            .await
            .unwrap();
        assert_eq!(backend.0.lock().unwrap().calls, ["persist"]);
    }
}

#[tokio::test]
async fn external_tun_or_missing_route_metadata_is_not_overwritten() {
    let (previous, candidate) = configs();
    for missing in [true, false] {
        let backend = Fake::new(&previous);
        if missing {
            backend.0.lock().unwrap().status.exclude_cidrs = None;
        } else {
            backend.0.lock().unwrap().status.mtu = Some(1400);
        }
        assert_eq!(
            transaction::apply(&backend, &previous, &candidate, true)
                .await
                .unwrap_err()
                .code,
            "conflict"
        );
        assert!(backend.0.lock().unwrap().calls.is_empty());
    }
}

#[tokio::test]
async fn changed_core_and_failed_rollback_report_unverified_recovery() {
    let (previous, candidate) = configs();
    for changed_core in [false, true] {
        let backend = Fake::new(&previous);
        {
            let mut state = backend.0.lock().unwrap();
            state.fail_start = true;
            state.fail_rollback = true;
            state.change_core_on_start = changed_core;
        }
        let error = transaction::apply(&backend, &previous, &candidate, true)
            .await
            .unwrap_err();
        assert_eq!(error.details.unwrap()["tunRollback"]["succeeded"], false);
        assert_eq!(backend.0.lock().unwrap().saved, previous);
        if changed_core {
            assert_eq!(backend.0.lock().unwrap().calls, ["stop", "start"]);
        }
    }
}

#[tokio::test]
async fn stop_failure_does_not_start_a_second_device() {
    let (previous, candidate) = configs();
    let backend = Fake::new(&previous);
    backend.0.lock().unwrap().fail_stop = true;
    transaction::apply(&backend, &previous, &candidate, true)
        .await
        .unwrap_err();
    assert_eq!(backend.0.lock().unwrap().calls, ["stop", "persist"]);
}

#[test]
fn rejects_bad_interface_and_family_before_runtime_changes() {
    let (previous, _) = configs();
    let mut candidate = previous.clone();
    candidate.tun.addr = "not-an-address/24".into();
    assert!(super::validation::validate(&mut candidate).is_err());
    candidate = previous.clone();
    candidate.tun.dual_stack = false;
    candidate.tun.exclude_cidrs.push("2001:db8::/32".into());
    assert!(super::validation::validate(&mut candidate).is_err());
    candidate = previous;
    candidate.tun.mask = "255.0.255.0".into();
    assert!(super::validation::validate(&mut candidate).is_err());
}

#[test]
fn accepts_kernel_normalized_ipv6_without_ignoring_address_or_prefix_changes() {
    let mut tun = AppConfig::default().tun;
    tun.dual_stack = true;
    tun.secondary_addr = Some("FD77:0:0:0:0:0:0:1/64".into());
    tun.include_cidrs = vec!["2001:0DB8:0:0:0:0:0:0/32".into()];
    tun.exclude_cidrs = vec!["2001:0DB8:0:0:0:0:0:1/128".into()];
    let mut snapshot = status(&tun);
    snapshot.addresses = vec!["fd77::1/64".into()];
    snapshot.include_cidrs = Some(vec!["2001:db8::/32".into()]);
    snapshot.exclude_cidrs = Some(vec!["2001:db8::1/128".into()]);
    assert!(transaction::matches(&snapshot, &tun));
    snapshot.addresses = vec!["fd77::2/64".into()];
    assert!(!transaction::matches(&snapshot, &tun));
    snapshot.addresses = vec!["fd77::1/64".into()];
    snapshot.exclude_cidrs = Some(vec!["2001:db8::1/64".into()]);
    assert!(!transaction::matches(&snapshot, &tun));
}
