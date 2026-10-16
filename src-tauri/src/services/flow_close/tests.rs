//! Real Unix IPC contract peers, never simulated product observations.
use super::*;
use crate::models::app_config::AppConfig;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};
const INSTANCE: &str = "22222222-2222-4222-8222-222222222222";
const OTHER: &str = "33333333-3333-4333-8333-333333333333";
const REV: u64 = 9_007_199_254_740_993;
fn row(id: u64, ended: bool) -> Value {
    json!({"record":{"flow_id":id.to_string(),"revision":u64::MAX,"state":if ended{"completed"}else{"active"},"network":"tcp","inbound":{"tag":"mixed","protocol":"mixed"},"source":{"ip":"127.0.0.1","port":1234},"target":{"host":format!("target-{id}.invalid"),"port":443},"route":{"mode":"rule","action":"direct"},"path":{},"traffic":{"bytes_up":0,"bytes_down":0},"throughput":{"upload_bps":0,"download_bps":0,"sampled_at_unix_ms":0},"timing":{"started_at_unix_ms":u64::MAX-id,"last_activity_at_unix_ms":u64::MAX}}})
}
fn ok(v: Value) -> Value {
    json!({"ok":true,"result":v})
}
struct Peer {
    state: Arc<AppState>,
    mode: Arc<Mutex<String>>,
    frames: Arc<Mutex<Vec<Value>>>,
    connections: Arc<AtomicUsize>,
    options: CoreIpcOptions,
    worker: Option<thread::JoinHandle<()>>,
    _dir: tempfile::TempDir,
}
impl Peer {
    fn new(count: usize) -> Self {
        let dir = tempfile::tempdir_in("/tmp").unwrap();
        let path = dir.path().join("flow.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let mode = Arc::new(Mutex::new(String::new()));
        let sharedmode = mode.clone();
        let frames = Arc::new(Mutex::new(Vec::new()));
        let recorded = frames.clone();
        let connections = Arc::new(AtomicUsize::new(0));
        let connected = connections.clone();
        let state = Arc::new(AppState::with_domain_data(
            AppConfig::default(),
            vec![],
            vec![],
            vec![],
            vec![],
        ));
        let serverstate = state.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            connected.fetch_add(1, Ordering::SeqCst);
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut changed = false;
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let req: Value = serde_json::from_str(&line).unwrap();
                recorded.lock().unwrap().push(req.clone());
                let mode = sharedmode.lock().unwrap().clone();
                let q = &req["request"];
                let mut response = if req["type"] == "subscribe" {
                    ok(json!("subscribed"))
                } else if q.get("capabilities").is_some() {
                    if mode == "capabilities-denied" {
                        json!({"ok":false,"error":{"code":"permission_denied","message":"secret-canary-capabilities","path":"/private/canary"}})
                    } else {
                        ok(
                            json!({"capabilities":{"features":["runtime_snapshot","flow_snapshot"],"permissions":["read"],"contracts":{"capabilities":{"current":1,"minimum_supported":1},"control_api":{"current":1,"minimum_supported":1}}}}),
                        )
                    }
                } else if q.get("runtime").is_some() {
                    if mode == "runtime-denied" {
                        json!({"ok":false,"error":{"code":"permission_denied","message":"secret-canary-runtime"}})
                    } else {
                        let mut runtime = json!({"core_instance_id":if changed||mode=="identity-before"{OTHER}else{INSTANCE},"config_revision":REV});
                        match mode.as_str() {
                            "numeric-instance" => runtime["core_instance_id"] = json!(7),
                            "boolean-instance" => runtime["core_instance_id"] = json!(true),
                            "blank-instance" => runtime["core_instance_id"] = json!("   "),
                            "missing-instance" => {
                                runtime.as_object_mut().unwrap().remove("core_instance_id");
                            }
                            "missing-revision" => {
                                runtime.as_object_mut().unwrap().remove("config_revision");
                            }
                            "boolean-revision" => runtime["config_revision"] = json!(true),
                            "bad-decimal-revision" => runtime["config_revision"] = json!("01"),
                            _ => {}
                        }
                        ok(json!({"runtime":runtime}))
                    }
                } else if q.get("active_flows").is_some() {
                    if mode == "list-denied" {
                        json!({"ok":false,"error":{"code":"permission_denied","message":"secret-canary-active"}})
                    } else {
                        let count = if mode == "new-flow" { count + 1 } else { count };
                        ok(
                            json!({"active_flows":(1..=count).map(|n|row(n as u64,false)).collect::<Vec<_>>()}),
                        )
                    }
                } else if let Some(id) = q.pointer("/flow/flow_id").and_then(Value::as_str) {
                    if mode == "missing" {
                        json!({"ok":false,"error":{"code":"not_found","message":"secret-canary"}})
                    } else if mode == "read-denied" {
                        json!({"ok":false,"error":{"code":"permission_denied","message":"secret-canary"}})
                    } else {
                        let mut r = row(id.parse().unwrap(), mode == "ended");
                        if mode == "reused" {
                            r["record"]["timing"]["started_at_unix_ms"] = json!(1);
                        }
                        ok(json!({"flow":r}))
                    }
                } else if req["method"] == "flows.close" {
                    let id = req["params"]["flow_id"].as_str().unwrap();
                    match mode.as_str() {
                        "missing-accepted" => ok(json!({"result":{"flow_id":id,"closed":true}})),
                        "missing-closed" => ok(json!({"accepted":true,"result":{"flow_id":id}})),
                        "wrong-id" => {
                            ok(json!({"accepted":true,"result":{"flow_id":"999","closed":true}}))
                        }
                        "false" => {
                            ok(json!({"accepted":true,"result":{"flow_id":id,"closed":false}}))
                        }
                        "rejected" => {
                            ok(json!({"accepted":false,"result":{"flow_id":id,"closed":true}}))
                        }
                        "not-found" => {
                            json!({"ok":false,"error":{"code":"not_found","message":"secret-canary flow already completed"}})
                        }
                        "denied" => {
                            json!({"ok":false,"error":{"code":"permission_denied","message":"secret-canary"}})
                        }
                        "disconnect" => break,
                        "identity-after" => {
                            changed = true;
                            ok(json!({"accepted":true,"result":{"flow_id":id,"closed":true}}))
                        }
                        "scope-after" => {
                            serverstate.client_core_instance_started();
                            ok(json!({"accepted":true,"result":{"flow_id":id,"closed":true}}))
                        }
                        _ => ok(json!({"accepted":true,"result":{"flow_id":id,"closed":true}})),
                    }
                } else {
                    panic!("unexpected frame type");
                };
                response["id"] = req["id"].clone();
                response["api_id"] = json!("zero.api.v1");
                if writeln!(writer, "{response}").is_err() {
                    break;
                }
            }
        });
        let options = CoreIpcOptions {
            socket: Some(path.to_string_lossy().into_owned()),
            timeout_ms: Some(500),
        };
        state.observations().begin(znet_engine_client::Binding {
            endpoint: crate::kernel::protocol::endpoint_from_options(Some(&options)).unwrap(),
            timeout: Duration::from_millis(500),
        });
        Self {
            state,
            mode,
            frames,
            connections,
            options,
            worker: Some(worker),
            _dir: dir,
        }
    }
    fn set(&self, mode: &str) {
        *self.mode.lock().unwrap() = mode.into();
    }
    fn commands(&self) -> Vec<String> {
        self.frames
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r["method"] == "flows.close")
            .map(|r| r["params"]["flow_id"].as_str().unwrap().into())
            .collect()
    }
    async fn plan(&self, ids: &[&str]) -> Preview {
        let inventory = inventory(&self.state).await.unwrap();
        preview(
            &self.state,
            PreviewInput {
                inventory_id: inventory.inventory_id,
                targets: inventory
                    .targets
                    .iter()
                    .filter(|t| ids.contains(&t.flow_id.as_str()))
                    .map(|t| SelectedTarget {
                        flow_id: t.flow_id.clone(),
                        target_key: t.target_key.clone(),
                    })
                    .collect(),
            },
        )
        .await
        .unwrap()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        crate::kernel::connection::reset_endpoint(
            &crate::kernel::protocol::endpoint_from_options(Some(&self.options)).unwrap(),
        );
        self.worker.take().unwrap().join().unwrap();
    }
}
fn run(f: impl std::future::Future<Output = ()>) {
    tauri::async_runtime::block_on(f);
}
#[test]
fn actual_ipc_close_fixes_only_original_selected_ids_and_later_new_flow_is_untouched() {
    run(async {
        let peer = Peer::new(3);
        let plan = peer.plan(&["1", "3"]).await;
        peer.set("new-flow");
        let result = submit(&peer.state, &plan.plan_id).await.unwrap();
        assert_eq!(peer.commands(), vec!["1", "3"]);
        assert_eq!(result.counts.closed, 2);
        assert_eq!(result.counts.unknown, 0);
        assert_eq!(peer.connections.load(Ordering::SeqCst), 1);
        let observed = status(&peer.state, &plan.plan_id).unwrap();
        assert_eq!(observed.state, "finished");
        assert_eq!(observed.result.unwrap().counts.closed, 2);
        assert!(submit(&peer.state, &plan.plan_id).await.is_err());
        assert_eq!(peer.commands().len(), 2);
    });
}
#[test]
fn strict_actual_command_envelopes_never_default_closed_and_unknown_blocks_other_plans() {
    run(async {
        for (mode, outcome) in [
            ("missing-accepted", Outcome::Unknown),
            ("missing-closed", Outcome::Unknown),
            ("wrong-id", Outcome::Unknown),
            ("false", Outcome::Rejected),
            ("rejected", Outcome::Rejected),
            ("not-found", Outcome::Rejected),
            ("denied", Outcome::Rejected),
            ("disconnect", Outcome::Unknown),
        ] {
            let peer = Peer::new(1);
            let plan = peer.plan(&["1"]).await;
            peer.set(mode);
            let result = submit(&peer.state, &plan.plan_id).await.unwrap();
            assert_eq!(result.items[0].outcome, outcome, "{mode}");
            assert_eq!(result.counts.closed, 0);
            assert!(!serde_json::to_string(&result)
                .unwrap()
                .contains("secret-canary"));
            let before = peer.commands().len();
            assert!(submit(&peer.state, &plan.plan_id).await.is_err());
            let _ = status(&peer.state, &plan.plan_id).unwrap();
            assert_eq!(peer.commands().len(), before);
            if outcome == Outcome::Unknown {
                assert!(peer
                    .state
                    .flow_closes()
                    .0
                    .lock()
                    .unwrap()
                    .blocked
                    .contains_key(&(INSTANCE.into(), "1".into())));
            }
        }
    });
}
#[test]
fn explicit_completed_is_ended_but_missing_reused_and_permission_read_do_not_send_or_count_success()
{
    run(async {
        for (mode, outcome) in [
            ("ended", Outcome::Ended),
            ("missing", Outcome::NotSubmitted),
            ("reused", Outcome::Stale),
            ("read-denied", Outcome::NotSubmitted),
        ] {
            let peer = Peer::new(1);
            let plan = peer.plan(&["1"]).await;
            peer.set(mode);
            let result = submit(&peer.state, &plan.plan_id).await.unwrap();
            assert_eq!(result.items[0].outcome, outcome, "{mode}");
            assert_eq!(result.counts.closed, 0);
            assert!(peer.commands().is_empty());
        }
    });
}
#[test]
fn actual_instance_subscription_and_client_scope_drift_cannot_rebind_prepared_plan() {
    run(async {
        for mode in ["identity-before", "subscription", "scope"] {
            let peer = Peer::new(1);
            let plan = peer.plan(&["1"]).await;
            match mode {
                "subscription" => {
                    let old = peer.state.observations().current().unwrap();
                    peer.state.observations().begin(old.binding);
                }
                "scope" => peer.state.client_core_instance_started(),
                _ => peer.set(mode),
            }
            let result = submit(&peer.state, &plan.plan_id).await;
            if let Ok(result) = result {
                assert_eq!(result.items[0].outcome, Outcome::Stale);
            }
            assert!(peer.commands().is_empty());
            assert_eq!(peer.connections.load(Ordering::SeqCst), 1);
        }
    });
}
#[test]
fn late_runtime_or_scope_change_after_real_ack_keeps_unknown_without_replay() {
    run(async {
        for mode in ["identity-after", "scope-after"] {
            let peer = Peer::new(1);
            let plan = peer.plan(&["1"]).await;
            peer.set(mode);
            let result = submit(&peer.state, &plan.plan_id).await.unwrap();
            assert_eq!(result.counts.unknown, 1);
            assert_eq!(result.counts.closed, 0);
            assert_eq!(peer.commands(), vec!["1"]);
            assert!(submit(&peer.state, &plan.plan_id).await.is_err());
        }
    });
}
#[test]
fn inventory_is_canonical_lossless_bounded_and_never_uses_default_observation_endpoint() {
    run(async {
        let state =
            AppState::with_domain_data(AppConfig::default(), vec![], vec![], vec![], vec![]);
        assert_eq!(
            inventory(&state).await.unwrap_err().code,
            "flow_close_unavailable"
        );
        let peer = Peer::new(501);
        let view = inventory(&peer.state).await.unwrap();
        assert_eq!(view.targets.len(), 500);
        assert!(view.truncated);
        assert_eq!(view.limit, 500);
        assert_eq!(view.config_revision, REV.to_string());
        assert_eq!(
            view.targets[0].started_at_unix_ms,
            (u64::MAX - 1).to_string()
        );
        assert!(!view.targets[0].target_key.is_empty());
        assert!(peer.commands().is_empty());
    });
}
#[test]
fn forged_expired_duplicate_or_empty_original_selection_has_zero_commands() {
    run(async {
        let peer = Peer::new(2);
        let inventory = inventory(&peer.state).await.unwrap();
        for selected in [
            vec![],
            vec![SelectedTarget {
                flow_id: "1".into(),
                target_key: "forged".into(),
            }],
            vec![
                SelectedTarget {
                    flow_id: "1".into(),
                    target_key: inventory.targets[0].target_key.clone()
                };
                2
            ],
        ] {
            assert!(preview(
                &peer.state,
                PreviewInput {
                    inventory_id: inventory.inventory_id.clone(),
                    targets: selected
                }
            )
            .await
            .is_err());
        }
        let plan = peer.plan(&["1"]).await;
        peer.state
            .flow_closes()
            .0
            .lock()
            .unwrap()
            .plans
            .get_mut(&plan.plan_id)
            .unwrap()
            .deadline = Instant::now() - Duration::from_secs(1);
        assert!(submit(&peer.state, &plan.plan_id).await.is_err());
        assert!(peer.commands().is_empty());
    });
}
#[test]
fn previous_prepared_plan_cannot_bypass_another_unknown_target_and_discard_is_not_io_cancel() {
    run(async {
        let peer = Peer::new(1);
        let first = peer.plan(&["1"]).await;
        let second = peer.plan(&["1"]).await;
        peer.set("missing-closed");
        let result = submit(&peer.state, &first.plan_id).await.unwrap();
        assert_eq!(result.counts.unknown, 1);
        assert!(submit(&peer.state, &second.plan_id).await.is_err());
        assert!(discard(&peer.state, &first.plan_id).is_err());
        discard(&peer.state, &second.plan_id).unwrap();
        assert_eq!(peer.commands(), vec!["1"]);
        let inventory = inventory(&peer.state).await.unwrap();
        assert!(!inventory.targets[0].can_close);
        assert!(inventory.targets[0]
            .reason
            .as_ref()
            .unwrap()
            .contains("未确认"));
    });
}
#[test]
fn retired_physical_connection_is_not_rediscovered_during_submit() {
    run(async {
        let peer = Peer::new(1);
        let plan = peer.plan(&["1"]).await;
        crate::kernel::connection::reset_endpoint(
            &crate::kernel::protocol::endpoint_from_options(Some(&peer.options)).unwrap(),
        );
        let result = submit(&peer.state, &plan.plan_id).await.unwrap();
        assert_eq!(result.counts.stale, 1);
        assert!(peer.commands().is_empty());
        assert_eq!(peer.connections.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn numeric_boolean_missing_or_blank_runtime_identity_never_grants_inventory_or_prepared_control() {
    run(async {
        for mode in [
            "numeric-instance",
            "boolean-instance",
            "blank-instance",
            "missing-instance",
            "missing-revision",
            "boolean-revision",
            "bad-decimal-revision",
        ] {
            let peer = Peer::new(1);
            peer.set(mode);
            assert!(inventory(&peer.state).await.is_err());
            assert!(peer.commands().is_empty());
            peer.set("");
            let plan = peer.plan(&["1"]).await;
            peer.set(mode);
            let result = submit(&peer.state, &plan.plan_id).await.unwrap();
            assert_eq!(result.counts.stale, 1);
            assert!(peer.commands().is_empty());
        }
    });
}

#[test]
fn actual_inventory_permission_and_transport_errors_never_return_raw_messages_or_paths() {
    run(async {
        for mode in ["capabilities-denied", "runtime-denied", "list-denied"] {
            let peer = Peer::new(1);
            peer.set(mode);
            let error = inventory(&peer.state).await.unwrap_err();
            assert_eq!(error.code, "permission_denied");
            assert!(error.details.is_none());
            assert!(!serde_json::to_string(&error).unwrap().contains("canary"));
            assert!(peer.commands().is_empty());
        }
        let state =
            AppState::with_domain_data(AppConfig::default(), vec![], vec![], vec![], vec![]);
        state.observations().begin(znet_engine_client::Binding {
            endpoint: crate::models::core::CoreEndpoint {
                transport: "unix-socket",
                path: "/never-selected-secret-canary/flow.sock".into(),
            },
            timeout: Duration::from_millis(10),
        });
        let error = inventory(&state).await.unwrap_err();
        assert!(error.details.is_none());
        assert!(!serde_json::to_string(&error).unwrap().contains("canary"));
    });
}
