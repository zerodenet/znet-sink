//! Real Unix IPC envelopes exercise the owning hot-apply dispatch, not a UI mock.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    thread,
};
#[tokio::test]
async fn workspace_actual_ipc_preflight_lost_ack_bad_receipt_and_late_source_never_replay() {
    for case in [
        "identity-changed",
        "capture-plan-drift",
        "capture-instance-changed",
        "capture-revision-changed",
        "submit-instance-changed",
        "submit-revision-changed",
        "lost-ack",
        "bad-persistence",
        "source-changed",
    ] {
        let root = tempfile::tempdir_in("/tmp").unwrap();
        let socket = root.path().join("workspace.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let state = Arc::new(crate::configuration::workspace::tests::state());
        state.app_config().lock().unwrap().core.socket =
            Some(socket.to_string_lossy().into_owned());
        crate::services::app_config_store::save(
            &workspace::recovery::directory(&state)
                .unwrap()
                .join("app-config.json"),
            &state.app_config().lock().unwrap(),
        )
        .unwrap();
        let mut prepared = crate::configuration::workspace::tests::prepared(&state);
        prepared.identity = Some(queries::KernelRuntimeIdentity {
            core_instance_id: "original".into(),
            config_revision: 1,
        });
        prepared.frozen_capture_parameters =
            crate::configuration::capture_apply::candidate_capture_parameters(
                &state,
                &prepared.source,
                Some("ordinary"),
            )
            .unwrap();
        if case == "capture-plan-drift" {
            // Model a prior capture plan that differs from the current resolver.
            // The owning entry must reject it before any TUN/config command,
            // rather than only compare the receipt after side effects.
            prepared.frozen_capture_parameters["mtu"] = json!(1400);
        }
        prepared.capture_digest = workspace::digest(&prepared.frozen_capture_parameters).unwrap();
        let alternative = workspace::store(&state, prepared.clone()).unwrap();
        let plan = workspace::store(&state, prepared).unwrap();
        let calls = Arc::new(Mutex::new(Vec::new()));
        let recorded = calls.clone();
        let runtime_state = state.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            let mut revision = if case == "identity-changed" { 2 } else { 1 };
            let mut runtime_reads = 0;
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let request: Value = serde_json::from_str(&line).unwrap();
                let result = match request["type"].as_str().unwrap() {
                    "subscribe" => json!("subscribed"),
                    "query" if request["request"].get("runtime").is_some() => {
                        runtime_reads += 1;
                        // Reads 2/3 fence capture; 4/5 are config admission;
                        // read 6 is the fixed connection's immediate pre-submit fence.
                        let instance = if (case == "capture-instance-changed" && runtime_reads > 1)
                            || (case == "submit-instance-changed" && runtime_reads >= 6)
                        {
                            "replacement"
                        } else {
                            "original"
                        };
                        if (case == "capture-revision-changed" && runtime_reads > 1)
                            || (case == "submit-revision-changed" && runtime_reads >= 6)
                        {
                            revision = 2;
                        }
                        json!({"runtime":{"core_instance_id":instance,"config_revision":revision}})
                    }
                    "query" if request["request"].get("tun_status").is_some() => {
                        json!({"tun_status":{"running":false,"healthy":false,"managed_by_config":false}})
                    }
                    "command" => {
                        recorded
                            .lock()
                            .unwrap()
                            .push(request["method"].as_str().unwrap().to_string());
                        assert_eq!(request["method"], "config.apply_runtime");
                        assert_eq!(
                            request["params"]["config"]["runtime"]
                                ["udp_upstream_idle_timeout_seconds"]
                                .as_u64(),
                            Some(u64::MAX)
                        );
                        revision = 2;
                        if case == "lost-ack" {
                            break;
                        }
                        if case == "source-changed" {
                            runtime_state.proxy_configs().lock().unwrap()[0].name =
                                "new-source-same-timestamp".into();
                        }
                        if case == "bad-persistence" {
                            json!({"accepted":true,"result":{"applied":true,"core_instance_id":"original","config_revision":revision}})
                        } else {
                            json!({"accepted":true,"result":{"applied":true,"persistence":"runtime_only","core_instance_id":"original","config_revision":revision}})
                        }
                    }
                    other => panic!("unexpected IPC type {other}: {request}"),
                };
                if writeln!(
                    writer,
                    "{}",
                    json!({"api_id":"zero.api.v1","id":request["id"],"ok":true,"result":result})
                )
                .is_err()
                {
                    break;
                }
            }
        });
        let result = apply_locked(
            &state,
            ConfigWorkspaceApplyInput {
                plan_id: plan.plan_id.clone(),
                strategy: ConfigApplyStrategy::HotReload,
            },
        )
        .await;
        assert!(result.is_err(), "{case}");
        let operation = workspace::operation(&state, &plan.plan_id).unwrap();
        if case == "identity-changed" || case == "capture-plan-drift" {
            assert_eq!(result.unwrap_err().code, "workspace_preflight_rejected");
            assert_eq!(operation.state, "rejected");
            assert!(calls.lock().unwrap().is_empty());
            assert!(workspace::ensure_unblocked(&state, "ordinary").is_ok());
            assert!(workspace::review(&state, &plan.plan_id).is_err());
            assert!(workspace::submit(&state, &plan.plan_id).is_err());
            assert!(operation.receipt.is_none());
        } else {
            assert_eq!(result.unwrap_err().code, "config_apply_uncertain");
            assert!(workspace::ensure_unblocked(&state, "ordinary").is_err());
            assert!(apply_locked(
                &state,
                ConfigWorkspaceApplyInput {
                    plan_id: plan.plan_id.clone(),
                    strategy: ConfigApplyStrategy::HotReload
                }
            )
            .await
            .is_err());
            assert!(apply_locked(
                &state,
                ConfigWorkspaceApplyInput {
                    plan_id: alternative.plan_id,
                    strategy: ConfigApplyStrategy::HotReload
                }
            )
            .await
            .is_err());
            if case.starts_with("capture-") || case.starts_with("submit-") {
                assert!(
                    calls.lock().unwrap().is_empty(),
                    "replacement baseline must receive no TUN or configuration command"
                );
            } else {
                assert_eq!(*calls.lock().unwrap(), vec!["config.apply_runtime"]);
            }
            if case == "source-changed" {
                assert_eq!(operation.state, "runtime_confirmed_pending_local");
                assert_eq!(
                    operation.receipt.as_ref().unwrap().state,
                    "runtime_confirmed"
                );
                assert_eq!(
                    operation
                        .receipt
                        .as_ref()
                        .unwrap()
                        .runtime_identity
                        .config_revision,
                    2
                );
                assert_eq!(
                    state.proxy_configs().lock().unwrap()[0].name,
                    "new-source-same-timestamp"
                );
                assert_eq!(
                    workspace::local_snapshot(&state)
                        .unwrap()
                        .pending_operation
                        .unwrap()
                        .plan_id,
                    plan.plan_id
                );
            } else {
                assert_eq!(operation.state, "submitted_unknown");
                assert!(operation.receipt.is_none());
            }
        }
        assert!(state.configuration().transaction().is_none());
        let options = ipc_options(&state).unwrap();
        crate::kernel::connection::reset_endpoint(
            &crate::kernel::protocol::endpoint_from_options(Some(&options)).unwrap(),
        );
        worker.join().unwrap();
    }
}
