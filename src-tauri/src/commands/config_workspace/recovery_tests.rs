//! Actual Unix IPC reads authorize only the original receipt's local completion.
use super::*;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixListener,
    sync::{Arc, Mutex},
    thread,
};
fn owner(dir: &std::path::Path) -> AppState {
    let data = crate::services::app_database::load_domain_data(dir).unwrap();
    let app =
        crate::services::app_config_store::load_or_default(&dir.join("app-config.json")).unwrap();
    let state =
        AppState::with_domain_data(app, data.proxy_configs, data.subscriptions, vec![], vec![]);
    workspace::recovery::set_test_directory(&state, dir.to_path_buf());
    state
}
#[tokio::test]
async fn unknown_recovery_review_never_queries_or_invents_runtime_receipt() {
    let state = workspace::tests::state();
    state.app_config().lock().unwrap().core.socket =
        Some("/tmp/no-workspace-runtime-for-unknown.sock".into());
    let plan = workspace::store(&state, workspace::tests::prepared(&state)).unwrap();
    workspace::submit(&state, &plan.plan_id).unwrap();
    let review = recovery_review_locked(&state, &plan.plan_id).await.unwrap();
    assert!(!review.can_complete_local);
    assert!(review.review_id.is_none());
    assert!(review.receipt.is_none());
}
#[tokio::test]
async fn recovered_original_receipt_uses_bound_queries_only_and_never_replays_runtime_writes() {
    for case in [
        "valid",
        "instance-changed",
        "revision-changed",
        "late-local-change",
    ] {
        let root = tempfile::tempdir_in("/tmp").unwrap();
        let socket = root.path().join("recovery.sock");
        let listener = UnixListener::bind(&socket).unwrap();
        let initial = workspace::tests::state();
        initial.app_config().lock().unwrap().core.socket =
            Some(socket.to_string_lossy().into_owned());
        let dir = workspace::recovery::directory(&initial).unwrap();
        crate::services::app_config_store::save(
            &dir.join("app-config.json"),
            &initial.app_config().lock().unwrap(),
        )
        .unwrap();
        let mut prepared = workspace::tests::prepared(&initial);
        prepared.identity = Some(queries::KernelRuntimeIdentity {
            core_instance_id: "original".into(),
            config_revision: 1,
        });
        let plan = workspace::store(&initial, prepared).unwrap();
        workspace::submit(&initial, &plan.plan_id).unwrap();
        let created = workspace::recovery::metadata(&initial, &plan.plan_id)
            .unwrap()
            .unwrap()
            .created_at_unix_ms;
        workspace::runtime_confirmed(
            &initial,
            &plan.plan_id,
            ConfigTransactionReceipt {
                transaction_id: u64::MAX,
                profile_id: "ordinary".into(),
                strategy: ConfigApplyStrategy::HotReload,
                state: "runtime_confirmed",
                effective_digest: plan.effective_digest,
                started_at_unix_ms: created,
                completed_at_unix_ms: created,
                runtime_identity: ConfigRuntimeIdentityView {
                    core_instance_id: "original".into(),
                    config_revision: 2,
                },
            },
        )
        .unwrap();
        let state = Arc::new(owner(&dir));
        assert_ne!(
            initial.host_session_id().unwrap(),
            state.host_session_id().unwrap()
        );
        let seen = Arc::new(Mutex::new(0usize));
        let count = seen.clone();
        let runtime_state = state.clone();
        let worker = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(10)))
                .unwrap();
            let mut writer = stream.try_clone().unwrap();
            for line in BufReader::new(stream).lines() {
                let Ok(line) = line else { break };
                let request: Value = serde_json::from_str(&line).unwrap();
                let result = match request["type"].as_str().unwrap() {
                    "subscribe" => json!("subscribed"),
                    "query" => {
                        assert!(
                            request["request"].get("runtime").is_some(),
                            "recovery may only read authoritative identity"
                        );
                        let mut n = count.lock().unwrap();
                        *n += 1;
                        if case == "late-local-change" && *n == 3 {
                            runtime_state
                                .app_config()
                                .lock()
                                .unwrap()
                                .routing
                                .inject_common_rules = true;
                        }
                        json!({"runtime":{"core_instance_id":if case=="instance-changed"{"replacement"}else{"original"},"config_revision":if case=="revision-changed"{3}else{2}}})
                    }
                    other => panic!("local recovery issued forbidden {other}: {request}"),
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
        let result = recovery_review_locked(&state, &plan.plan_id).await;
        if case == "valid" || case == "late-local-change" {
            let review = result.unwrap();
            let nonce = review.review_id.unwrap();
            assert!(review.can_complete_local);
            let completed = complete_local_locked(
                &state,
                ConfigWorkspaceCompleteInput {
                    plan_id: plan.plan_id.clone(),
                    review_id: nonce.clone(),
                },
            )
            .await;
            if case == "valid" {
                assert_eq!(completed.unwrap().state, "confirmed");
                assert!(workspace::ensure_unblocked(&state, "ordinary").is_ok());
                assert!(workspace::pending_operation(&state, "ordinary")
                    .unwrap()
                    .is_none());
                let epoch =
                    crate::services::app_database::workspace_recovery::current_epoch(&dir).unwrap();
                assert!(complete_local_locked(
                    &state,
                    ConfigWorkspaceCompleteInput {
                        plan_id: plan.plan_id.clone(),
                        review_id: nonce
                    }
                )
                .await
                .is_err());
                let again = recovery_review_locked(&state, &plan.plan_id).await.unwrap();
                complete_local_locked(
                    &state,
                    ConfigWorkspaceCompleteInput {
                        plan_id: plan.plan_id.clone(),
                        review_id: again.review_id.unwrap(),
                    },
                )
                .await
                .unwrap();
                assert_eq!(
                    crate::services::app_database::workspace_recovery::current_epoch(&dir).unwrap(),
                    epoch
                );
                assert_eq!(*seen.lock().unwrap(), 8);
            } else {
                assert!(completed.is_err());
                assert_eq!(
                    workspace::operation(&state, &plan.plan_id).unwrap().state,
                    "runtime_confirmed_pending_local"
                );
                assert!(workspace::ensure_unblocked(&state, "ordinary").is_err());
            }
        } else {
            assert!(result.is_err());
            assert!(workspace::ensure_unblocked(&state, "ordinary").is_err());
        }
        let options = ipc_options(&state).unwrap();
        crate::kernel::connection::reset_endpoint(
            &crate::kernel::protocol::endpoint_from_options(Some(&options)).unwrap(),
        );
        worker.join().unwrap();
    }
}
