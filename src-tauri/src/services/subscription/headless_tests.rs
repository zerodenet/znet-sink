//! These invoke the production explicit-state functions with no Tauri runtime.
//! Each test gets an isolated process/data directory, so concurrent tests never
//! mutate the process-wide desktop development data-dir environment.
use super::*;
use crate::models::subscription::ManagedSubscriptionSyncComplete;
use std::sync::atomic::{AtomicUsize, Ordering};

const CHILD: &str = "ZNET_HEADLESS_SUBSCRIPTION_CHILD";

fn isolated(name: &str) -> bool {
    if std::env::var(CHILD).ok().as_deref() == Some(name) {
        return false;
    }
    let dir = tempfile::tempdir().unwrap();
    let exact = format!("services::subscription::original::headless_tests::{name}");
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &exact, "--nocapture"])
        .env(CHILD, name)
        .env("ZNET_SINK_DATA_DIR", dir.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "headless domain child failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    true
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn input(revision: &str) -> ManagedSubscriptionApply {
    ManagedSubscriptionApply {
        plugin_id: "org.example.scheduled".into(),
        provider_id: "https://source.example.com".into(),
        remote_subscription_id: "primary".into(),
        source_name: "Scheduled source".into(),
        subscription_name: "Primary".into(),
        content: general_purpose::STANDARD.encode(
            json!({"outbounds":[{"tag":"direct","protocol":{"type":"direct"}}],
                "route":{"rules":[],"default":"direct"}})
            .to_string(),
        ),
        format: "zero".into(),
        revision: Some(revision.into()),
    }
}
fn metadata() -> ManagedSubscriptionMetadataUpdate {
    ManagedSubscriptionMetadataUpdate {
        plugin_id: "org.example.scheduled".into(),
        provider_id: "https://source.example.com".into(),
        remote_subscription_id: "primary".into(),
        usage: ManagedSubscriptionUsage {
            used_bytes: 91,
            total_bytes: 100,
            expire_at_unix_ms: 1_900_000_000_000,
        },
    }
}
fn load_state() -> AppState {
    let directory = crate::services::data_dir().unwrap();
    let domain = domain_store::load_all_from_dir(&directory).unwrap();
    let app =
        crate::services::app_config_store::load_or_default(&directory.join("app-config.json"))
            .unwrap();
    AppState::with_domain_data(
        app,
        domain.proxy_configs,
        domain.subscriptions,
        domain.rule_sets,
        vec![],
    )
}
fn graph(state: &AppState) -> Value {
    json!({"profiles":lock(state.proxy_configs(),"profiles").unwrap().clone(),
        "subscriptions":lock(state.subscriptions(),"subscriptions").unwrap().clone()})
}

#[test]
fn headless_material_metadata_and_sync_survive_state_recreation() {
    if isolated("headless_material_metadata_and_sync_survive_state_recreation") {
        return;
    }
    let runtime = runtime();
    tauri::async_runtime::set(runtime.handle().clone());
    runtime.block_on(async {
        let state = load_state();
        let first = apply_managed_authorized_state(&state, None, input("r1"), || Ok(()))
            .await
            .unwrap();
        let target = first.target_proxy_config_id.clone().unwrap();
        assert!(first.last_sync_at_unix_ms.is_some());
        let profiles = lock(state.proxy_configs(), "profiles").unwrap().clone();
        assert_eq!(profiles.len(), 1);
        assert!(
            !profiles[0].active,
            "background sync must not activate a new profile"
        );
        assert_eq!(profiles[0].id, target);
        assert_eq!(
            crate::services::core_process::refresh_status(&state)
                .unwrap()
                .state,
            crate::models::core_process::CoreProcessState::NotStarted
        );
        drop(state);

        let state = load_state();
        let before_profiles = lock(state.proxy_configs(), "profiles").unwrap().clone();
        let updated =
            update_managed_metadata_authorized_state(&state, None, metadata(), || Ok(())).unwrap();
        assert_eq!(updated.used_bytes, Some(91));
        assert_eq!(updated.last_sync_at_unix_ms, first.last_sync_at_unix_ms);
        assert_eq!(
            serde_json::to_value(&before_profiles).unwrap(),
            graph(&state)["profiles"]
        );
        let completed = complete_managed_sync_authorized_state(
            &state,
            None,
            ManagedSubscriptionSyncComplete {
                plugin_id: "org.example.scheduled".into(),
                provider_id: "https://source.example.com".into(),
                remote_subscription_id: "primary".into(),
                revision: "r1".into(),
            },
            || Ok(()),
        )
        .unwrap();
        assert!(completed.last_sync_at_unix_ms.unwrap() >= first.last_sync_at_unix_ms.unwrap());
        let saved = graph(&state);
        drop(state);
        assert_eq!(graph(&load_state()), saved);
    });
}

#[test]
fn headless_revocation_and_owner_rejection_preserve_confirmed_material() {
    if isolated("headless_revocation_and_owner_rejection_preserve_confirmed_material") {
        return;
    }
    let runtime = runtime();
    tauri::async_runtime::set(runtime.handle().clone());
    runtime.block_on(async {
        let state = load_state();
        apply_managed_authorized_state(&state, None, input("r1"), || Ok(()))
            .await
            .unwrap();
        let confirmed = graph(&state);
        // Revoke at a later guard after admission/preparation, before commit.
        let checks = AtomicUsize::new(0);
        let failed = apply_managed_authorized_state(&state, None, input("r2"), || {
            if checks.fetch_add(1, Ordering::SeqCst) >= 3 {
                Err(AppError::invalid_argument("revoked"))
            } else {
                Ok(())
            }
        })
        .await;
        assert!(failed.is_err());
        assert!(checks.load(Ordering::SeqCst) >= 4);
        assert_eq!(graph(&state), confirmed);
        assert_eq!(graph(&load_state()), confirmed);
        assert!(
            update_managed_metadata_authorized_state(&state, None, metadata(), || {
                Err(AppError::invalid_argument("revoked"))
            })
            .is_err()
        );
        assert!(complete_managed_sync_authorized_state(
            &state,
            None,
            ManagedSubscriptionSyncComplete {
                plugin_id: "org.example.scheduled".into(),
                provider_id: "https://source.example.com".into(),
                remote_subscription_id: "primary".into(),
                revision: "other-revision".into(),
            },
            || Ok(()),
        )
        .is_err());
        let mut foreign = metadata();
        foreign.plugin_id = "org.example.other".into();
        assert!(
            update_managed_metadata_authorized_state(&state, None, foreign, || Ok(())).is_err()
        );
        assert_eq!(graph(&state), confirmed);
        assert_eq!(graph(&load_state()), confirmed);
    });
}

#[test]
fn headless_removal_checks_active_target_and_deletes_only_owned_inactive_graph() {
    if isolated("headless_removal_checks_active_target_and_deletes_only_owned_inactive_graph") {
        return;
    }
    let runtime = runtime();
    tauri::async_runtime::set(runtime.handle().clone());
    runtime.block_on(async {
        let state = load_state();
        let subscription = apply_managed_authorized_state(&state, None, input("r1"), || Ok(()))
            .await
            .unwrap();
        // Exercise the active-target refusal before any relationship is removed.
        lock(state.proxy_configs(), "profiles").unwrap()[0].active = true;
        let active_graph = graph(&state);
        assert!(remove_managed_authorized_state(
            &state,
            None,
            subscription.id.clone(),
            true,
            "org.example.scheduled".into(),
            || Ok(()),
        )
        .await
        .is_err());
        assert_eq!(graph(&state), active_graph);
        lock(state.proxy_configs(), "profiles").unwrap()[0].active = false;
        let inactive_graph = graph(&state);
        assert!(remove_managed_authorized_state(
            &state,
            None,
            subscription.id.clone(),
            true,
            "org.example.foreign".into(),
            || Ok(()),
        )
        .await
        .is_err());
        assert_eq!(graph(&state), inactive_graph);
        let outcome = remove_managed_authorized_state(
            &state,
            None,
            subscription.id,
            true,
            "org.example.scheduled".into(),
            || Ok(()),
        )
        .await
        .unwrap();
        assert!(outcome.removed_proxy_config);
        assert_eq!(graph(&state), json!({"profiles":[],"subscriptions":[]}));
        assert_eq!(graph(&load_state()), graph(&state));
    });
}

#[test]
fn headless_sqlite_wait_rechecks_stop_for_each_managed_write() {
    if isolated("headless_sqlite_wait_rechecks_stop_for_each_managed_write") {
        return;
    }
    let executor = runtime();
    tauri::async_runtime::set(executor.handle().clone());
    executor.block_on(async {
        use std::{
            sync::{atomic::AtomicBool, mpsc, Arc},
            time::Duration,
        };
        let state = Arc::new(load_state());
        let profile = apply_managed_authorized_state(&state, None, input("r1"), || Ok(()))
            .await
            .unwrap();
        let baseline = graph(&state);
        for operation in ["apply", "remove", "metadata", "sync"] {
            let directory = crate::services::data_dir().unwrap();
            let blocker = rusqlite::Connection::open(directory.join("znet-sink.db")).unwrap();
            blocker.execute_batch("BEGIN IMMEDIATE").unwrap();
            let stopped = Arc::new(AtomicBool::new(false));
            let (entered_tx, entered_rx) = mpsc::sync_channel(1);
            let (finished_tx, finished_rx) = mpsc::sync_channel(1);
            let worker_state = state.clone();
            let worker_stopped = stopped.clone();
            let subscription_id = profile.id.clone();
            let worker = std::thread::spawn(move || {
                let first = AtomicBool::new(true);
                let authorize = || {
                    if first.swap(false, Ordering::SeqCst) {
                        entered_tx.send(()).unwrap();
                    }
                    if worker_stopped.load(Ordering::SeqCst) {
                        Err(AppError::invalid_argument("original job stopped"))
                    } else {
                        Ok(())
                    }
                };
                let result = match operation {
                    "metadata" => update_managed_metadata_authorized_state(
                        &worker_state,
                        None,
                        metadata(),
                        authorize,
                    )
                    .map(|_| ()),
                    "sync" => complete_managed_sync_authorized_state(
                        &worker_state,
                        None,
                        ManagedSubscriptionSyncComplete {
                            plugin_id: "org.example.scheduled".into(),
                            provider_id: "https://source.example.com".into(),
                            remote_subscription_id: "primary".into(),
                            revision: "r1".into(),
                        },
                        authorize,
                    )
                    .map(|_| ()),
                    "apply" => runtime()
                        .block_on(apply_managed_authorized_state(
                            &worker_state,
                            None,
                            input("r2"),
                            authorize,
                        ))
                        .map(|_| ()),
                    "remove" => runtime()
                        .block_on(remove_managed_authorized_state(
                            &worker_state,
                            None,
                            subscription_id,
                            true,
                            "org.example.scheduled".into(),
                            authorize,
                        ))
                        .map(|_| ()),
                    _ => unreachable!(),
                };
                finished_tx.send(result).unwrap();
            });
            entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            // A real second SQLite writer owns BEGIN IMMEDIATE. The candidate
            // cannot complete while that writer slot is held, even though its
            // initial authorization has already succeeded.
            assert!(
                matches!(
                    finished_rx.recv_timeout(Duration::from_millis(100)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ),
                "{operation} did not wait for the held SQLite writer"
            );
            stopped.store(true, Ordering::SeqCst);
            blocker.execute_batch("ROLLBACK").unwrap();
            let result = finished_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(
                result.unwrap_err().message,
                "original job stopped",
                "{operation}"
            );
            worker.join().unwrap();
            assert_eq!(
                graph(&state),
                baseline,
                "{operation} changed live graph after stop"
            );
            assert_eq!(
                graph(&load_state()),
                baseline,
                "{operation} changed durable graph after stop"
            );
        }
    });
}

#[test]
fn headless_file_history_wait_rechecks_stop_before_settings_publish() {
    if isolated("headless_file_history_wait_rechecks_stop_before_settings_publish") {
        return;
    }
    use std::{
        sync::{atomic::AtomicBool, mpsc, Arc},
        time::Duration,
    };
    let state = Arc::new(load_state());
    let app = lock(state.app_config(), "app").unwrap().clone();
    let path = crate::services::data_dir().unwrap().join("app-config.json");
    crate::services::app_config_store::save(&path, &app).unwrap();
    let blocker = crate::services::file_history::acquire(
        &path,
        crate::services::file_history::Domain::AppConfig,
    )
    .unwrap();
    let stopped = Arc::new(AtomicBool::new(false));
    let (entered_tx, entered_rx) = mpsc::sync_channel(1);
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let worker_state = state.clone();
    let worker_stopped = stopped.clone();
    let worker_path = path.clone();
    let expected = app.clone();
    let worker = std::thread::spawn(move || {
        let first = AtomicBool::new(true);
        let authorize = || {
            if first.swap(false, Ordering::SeqCst) {
                entered_tx.send(()).unwrap();
            }
            if worker_stopped.load(Ordering::SeqCst) {
                Err(AppError::invalid_argument("original job stopped"))
            } else {
                Ok(())
            }
        };
        let mut candidate = expected.clone();
        candidate.local_proxy.port = 18991;
        let result = crate::services::app_config::compare_and_replace_local_at_authorized(
            &worker_state,
            &expected,
            candidate,
            &worker_path,
            &authorize,
        );
        finished_tx.send(result).unwrap();
    });
    entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        finished_rx.recv_timeout(Duration::from_millis(100)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    stopped.store(true, Ordering::SeqCst);
    drop(blocker);
    assert_eq!(
        finished_rx
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
            .unwrap_err()
            .message,
        "original job stopped"
    );
    worker.join().unwrap();
    assert_eq!(*lock(state.app_config(), "app").unwrap(), app);
    assert_eq!(
        crate::services::app_config_store::load_or_default(&path).unwrap(),
        app
    );
}
