use super::*;
use std::sync::Mutex;

fn params(addr: &str) -> Value {
    json!({"addr":addr,"mtu":1400,"tag":"tun","auto_route":true,"strict_route":true,
        "dual_stack":false,"dns_hijack":false,"include_cidrs":[],"exclude_cidrs":[]})
}
fn status(p: &Value, enabled: bool) -> GuiTunStatus {
    GuiTunStatus {
        enabled,
        healthy: enabled,
        addr: p["addr"].as_str().map(str::to_owned),
        mtu: p["mtu"].as_u64().map(|v| v as u16),
        tag: p["tag"].as_str().map(str::to_owned),
        auto_route: p["auto_route"].as_bool().unwrap(),
        strict_route: p["strict_route"].as_bool().unwrap(),
        dual_stack: p["dual_stack"].as_bool().unwrap(),
        dns_hijack: p["dns_hijack"].as_bool().unwrap(),
        include_cidrs: Some(vec![]),
        exclude_cidrs: Some(vec![]),
        ..Default::default()
    }
}
struct Fake(Mutex<Data>);
struct Data {
    snapshot: Snapshot,
    calls: Vec<String>,
    config: Value,
    failure: &'static str,
    failed: bool,
}
impl Fake {
    fn new(enabled: bool, failure: &'static str) -> Self {
        Self(Mutex::new(Data {
            snapshot: Snapshot {
                identity: KernelRuntimeIdentity {
                    core_instance_id: "core".into(),
                    config_revision: 1,
                },
                tun: status(&params("10.77.0.1/24"), enabled),
            },
            calls: vec![],
            config: json!("old"),
            failure,
            failed: false,
        }))
    }
}
fn fail(data: &mut Data, step: &str) -> AppResult<()> {
    if data.failure == step && !data.failed {
        data.failed = true;
        return Err(AppError::internal(format!("{step} rejected")));
    }
    Ok(())
}
impl Backend for Fake {
    async fn snapshot(&self) -> AppResult<Snapshot> {
        let d = self.0.lock().unwrap();
        Ok(Snapshot {
            identity: d.snapshot.identity.clone(),
            tun: d.snapshot.tun.clone(),
        })
    }
    async fn apply(&self, config: &Value) -> AppResult<KernelRuntimeIdentity> {
        let mut d = self.0.lock().unwrap();
        d.calls.push(format!(
            "apply:{}",
            config.as_str().or_else(|| config["test"].as_str()).unwrap()
        ));
        fail(&mut d, "apply")?;
        d.config = config.clone();
        d.snapshot.identity.config_revision += 1;
        if d.failure == "timeout" {
            return Err(AppError {
                code: "config_apply_uncertain",
                message: "acknowledgement lost".into(),
                details: None,
            });
        }
        let identity = d.snapshot.identity.clone();
        if d.failure == "instance" {
            d.snapshot.identity.core_instance_id = "other".into();
        }
        if d.failure == "revision" {
            d.snapshot.identity.config_revision += 1;
        }
        Ok(identity)
    }
    async fn stop(&self) -> AppResult<()> {
        let mut d = self.0.lock().unwrap();
        d.calls.push("stop".into());
        fail(&mut d, "stop")?;
        d.snapshot.tun.enabled = false;
        if d.failure == "stop_reply_lost" {
            return Err(AppError {
                code: "timeout",
                message: "lost".into(),
                details: None,
            });
        }
        Ok(())
    }
    async fn start(&self, p: &Value) -> AppResult<()> {
        let mut d = self.0.lock().unwrap();
        d.calls
            .push(format!("start:{}", p["addr"].as_str().unwrap()));
        fail(&mut d, "start")?;
        d.snapshot.tun = status(p, true);
        if d.failure == "partial_start" && !d.failed {
            d.failed = true;
            d.snapshot.tun.healthy = false;
            return Err(AppError::internal("start left an unhealthy owned TUN"));
        }
        Ok(())
    }
}
async fn run(backend: &Fake) -> AppResult<KernelRuntimeIdentity> {
    apply(
        backend,
        &json!("old"),
        &json!("new"),
        &params("10.77.0.1/24"),
        &params("10.88.0.1/24"),
    )
    .await
}
#[tokio::test]
async fn profile_switch_automatically_restarts_active_capture_with_target_parameters() {
    let backend = Fake::new(true, "");
    run(&backend).await.unwrap();
    let d = backend.0.lock().unwrap();
    assert_eq!(d.calls, vec!["stop", "apply:new", "start:10.88.0.1/24"]);
    assert_eq!(d.config, "new");
    assert!(matches(&d.snapshot.tun, &params("10.88.0.1/24")));
}
#[tokio::test]
async fn inactive_or_kernel_owned_capture_is_never_started_or_stopped_by_profile_switch() {
    for managed in [false, true] {
        let backend = Fake::new(managed, "");
        backend.0.lock().unwrap().snapshot.tun.managed_by_config = managed;
        run(&backend).await.unwrap();
        assert_eq!(backend.0.lock().unwrap().calls, vec!["apply:new"]);
    }
}
#[tokio::test]
async fn rejected_stop_apply_or_start_restores_last_known_profile_and_capture() {
    for failure in ["stop", "apply", "start", "partial_start"] {
        let backend = Fake::new(true, failure);
        let error = run(&backend).await.unwrap_err();
        assert_eq!(
            error.details.unwrap()["captureRollback"]["succeeded"],
            true,
            "{failure}"
        );
        let d = backend.0.lock().unwrap();
        assert_eq!(d.config, "old");
        assert!(
            matches(&d.snapshot.tun, &params("10.77.0.1/24")),
            "{failure}"
        );
        if failure == "start" {
            assert_eq!(
                d.calls,
                vec![
                    "stop",
                    "apply:new",
                    "start:10.88.0.1/24",
                    "apply:old",
                    "start:10.77.0.1/24"
                ]
            );
        }
    }
}
#[tokio::test]
async fn uncertain_apply_or_replaced_runtime_is_not_replayed_or_rolled_back() {
    for failure in ["timeout", "instance", "revision"] {
        let backend = Fake::new(true, failure);
        let error = run(&backend).await.unwrap_err();
        assert_eq!(
            error.details.unwrap()["captureRollback"]["succeeded"],
            false
        );
        assert_eq!(backend.0.lock().unwrap().calls, vec!["stop", "apply:new"]);
    }
}
#[tokio::test]
async fn lost_stop_confirmation_is_recovered_by_observation_without_duplicate_command() {
    let backend = Fake::new(true, "stop_reply_lost");
    run(&backend).await.unwrap();
    assert_eq!(
        backend.0.lock().unwrap().calls,
        vec!["stop", "apply:new", "start:10.88.0.1/24"]
    );
}
#[tokio::test]
async fn mismatched_running_capture_is_not_overwritten() {
    let backend = Fake::new(true, "");
    backend.0.lock().unwrap().snapshot.tun.mtu = Some(1200);
    assert_eq!(run(&backend).await.unwrap_err().code, "conflict");
    assert!(backend.0.lock().unwrap().calls.is_empty());
}
#[test]
fn matching_preserves_source_routing_parameters_and_normalizes_cidrs() {
    let mut p = params("fd77:0000::1/64");
    p["auto_route"] = json!(false);
    p["strict_route"] = json!(false);
    let mut s = status(&p, true);
    s.addr = Some("fd77::1/64".into());
    assert!(matches(&s, &p));
    s.auto_route = true;
    assert!(!matches(&s, &p));
}

#[tokio::test]
async fn publication_failure_restores_confirmed_capture_even_after_profile_storage_changes() {
    let backend = Fake::new(true, "");
    let identity = run(&backend).await.unwrap();
    restore_receipt(
        &backend,
        &identity,
        &json!("new"),
        &json!("old"),
        &params("10.88.0.1/24"),
        &params("10.77.0.1/24"),
    )
    .await
    .unwrap();
    let d = backend.0.lock().unwrap();
    assert_eq!(d.config, "old");
    assert!(matches(&d.snapshot.tun, &params("10.77.0.1/24")));
    assert_eq!(
        d.calls,
        vec![
            "stop",
            "apply:new",
            "start:10.88.0.1/24",
            "stop",
            "apply:old",
            "start:10.77.0.1/24"
        ]
    );
}

#[tokio::test]
async fn publication_recovery_does_not_overwrite_a_newer_config_or_instance() {
    for change_instance in [false, true] {
        let backend = Fake::new(true, "");
        let identity = run(&backend).await.unwrap();
        if change_instance {
            backend.0.lock().unwrap().snapshot.identity.core_instance_id = "other".into();
        } else {
            backend.0.lock().unwrap().snapshot.identity.config_revision += 1;
        }
        assert_eq!(
            restore_receipt(
                &backend,
                &identity,
                &json!("new"),
                &json!("old"),
                &params("10.88.0.1/24"),
                &params("10.77.0.1/24")
            )
            .await
            .unwrap_err()
            .code,
            "conflict"
        );
        assert_eq!(
            backend.0.lock().unwrap().calls,
            vec!["stop", "apply:new", "start:10.88.0.1/24"]
        );
    }
}

#[tokio::test]
async fn rule_edits_keep_capture_live_but_endpoint_replacement_rebuilds_it() {
    for endpoint_change in [false, true] {
        let backend = Fake::new(true, "");
        let old = json!({"test":"old", "endpoints":[{"tag":"a"}],"route":{"rules":[]}});
        let mut next = json!({"test":"new", "endpoints":[{"tag":"a"}],"route":{"rules":[1]}});
        if endpoint_change {
            next["endpoints"][0]["tag"] = json!("b");
        }
        apply(
            &backend,
            &old,
            &next,
            &params("10.77.0.1/24"),
            &params("10.77.0.1/24"),
        )
        .await
        .unwrap();
        assert_eq!(
            backend.0.lock().unwrap().calls,
            if endpoint_change {
                vec!["stop", "apply:new", "start:10.77.0.1/24"]
            } else {
                vec!["apply:new"]
            }
        );
    }
}

#[tokio::test]
async fn optional_capture_facts_are_only_required_when_mutating_capture() {
    for changed in [false, true] {
        let backend = Fake::new(true, "");
        backend.0.lock().unwrap().snapshot.tun.include_cidrs = None;
        let result = apply(
            &backend,
            &json!("old"),
            &json!("new"),
            &params("10.77.0.1/24"),
            &params(if changed {
                "10.88.0.1/24"
            } else {
                "10.77.0.1/24"
            }),
        )
        .await;
        assert_eq!(result.is_ok(), !changed);
        assert_eq!(
            backend.0.lock().unwrap().calls,
            if changed { vec![] } else { vec!["apply:new"] }
        );
    }
}
