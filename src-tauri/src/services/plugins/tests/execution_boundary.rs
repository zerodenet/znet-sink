use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[test]
fn host_run_and_invoke_share_owner_close_without_closing_other_hosts() {
    let (_root, host, manager) = setup(false);
    let review = approve(&host, &manager);
    let shutdown = host.executor_shutdown_handle();
    assert!(shutdown.close(Duration::ZERO).drained);
    assert_eq!(host.run(&manager, review).unwrap_err().code, "cancelled");
    assert_eq!(
        host.invoke(
            &manager,
            None,
            "org.example.plugin",
            "identity",
            "read".into(),
            serde_json::Value::Null
        )
        .unwrap_err()
        .code,
        "cancelled"
    );
    let (_root, other, manager) = setup(false);
    let review = approve(&other, &manager);
    assert!(other.run(&manager, review).is_ok());
}
#[test]
fn reviewed_page_invocation_cannot_reacquire_grants_after_stop_restart() {
    let (_root, host, manager) = setup_component(
        "({znet_plugin_result:1,state_updates:{'late/value':'bGF0ZQ=='},value:'late'})",
        serde_json::json!({"capability":"plugin.self.read","scope":"self"}),
    );
    let original = approve(&host, &manager);
    host.stop(&manager, &original.key).unwrap();
    approve(&host, &manager);
    assert_eq!(
        host.invoke_reviewed(
            &manager,
            None,
            original,
            "read".into(),
            serde_json::Value::Null,
            Arc::new(AtomicBool::new(false))
        )
        .unwrap_err()
        .code,
        "revoked"
    );
    assert_eq!(
        host.storage_get("org.example.plugin", namespace::Area::State, "late/value")
            .unwrap(),
        None
    );
}
#[test]
fn cancelled_inflight_page_cannot_commit_guest_state_or_deliver_markerless_result() {
    for suffix in [
        "({znet_plugin_result:1,state_updates:{'late/value':'bGF0ZQ=='},value:'late'})",
        "'markerless-secret-canary'",
    ] {
        let source = format!("hostSdkCall('{{}}');{suffix}");
        let (_root, host, manager) = setup_component(
            &source,
            serde_json::json!({"capability":"plugin.self.read","scope":"self"}),
        );
        let host = Arc::new(host);
        let review = approve(&host, &manager);
        let cancel = Arc::new(AtomicBool::new(false));
        let flag = cancel.clone();
        let task_host = host.clone();
        let task_manager = manager.clone();
        let (entered_tx, entered_rx) = std::sync::mpsc::sync_channel(1);
        let (release_tx, release_rx) = std::sync::mpsc::sync_channel(1);
        let release_rx = Mutex::new(release_rx);
        let dispatcher: znet_plugin_sandbox::runtime::HostSdkDispatcher =
            Arc::new(move |lease, _| {
                lease.check(None)?;
                entered_tx.send(()).unwrap();
                release_rx
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(3))
                    .unwrap();
                Ok("null".into())
            });
        let task = std::thread::spawn(move || {
            task_host.invoke_reviewed_with_test_dispatcher(
                &task_manager,
                review,
                cancel,
                dispatcher,
            )
        });
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if entered_rx.recv_timeout(Duration::from_millis(25)).is_ok() {
                break;
            }
            if task.is_finished() {
                panic!(
                    "host invocation completed before real SDK dispatch: {:?}",
                    task.join().unwrap()
                );
            }
            assert!(
                Instant::now() < deadline,
                "host invocation did not reach its real SDK dispatch before bounded queue deadline"
            );
        }
        assert_eq!(host.executor_shutdown_handle().snapshot().running, 1);
        flag.store(true, Ordering::Release);
        release_tx.send(()).unwrap();
        let result = task.join().unwrap();
        let failure = result.unwrap_err();
        assert_eq!(failure.code, "cancelled");
        assert!(!serde_json::to_string(&failure)
            .unwrap()
            .contains("markerless-secret-canary"));
        assert_eq!(
            host.storage_get("org.example.plugin", namespace::Area::State, "late/value")
                .unwrap(),
            None
        );
    }
}

#[test]
fn vm_file_admission_rejects_guest_absolute_paths_and_fake_selection_tickets() {
    use znet_plugin_sandbox::sdk::{Call, Method};
    for method in [Method::FileRead, Method::FileWrite] {
        for arguments in [
            serde_json::json!({"selected_path":"/tmp/plugin-file-secret-canary","contents_base64":"Y2FuYXJ5"}),
            serde_json::json!({"selected_path":"host-selection:pretended-ticket"}),
            serde_json::json!({}),
        ] {
            let call = Call {
                version: 1,
                request: Request {
                    capability: method.capability(),
                    scope: "user".into(),
                },
                method,
                budget: Default::default(),
                arguments,
            };
            let error = super::super::operations::validate_vm_sdk_admissibility(&call).unwrap_err();
            assert_eq!(error.code, "plugin_vm_file_unsupported");
            assert!(error.details.is_none());
            assert!(!serde_json::to_string(&error)
                .unwrap()
                .contains("secret-canary"));
            let failure = super::super::sdk::public_failure(error);
            assert_eq!(
                failure.code,
                znet_plugin_sandbox::sdk::ErrorCode::Unsupported
            );
            assert!(failure.message.contains("可信系统文件选择"));
        }
    }
}
#[test]
fn vm_file_gate_preserves_other_sdk_methods_and_their_existing_authority_checks() {
    use znet_plugin_sandbox::sdk::{Call, Method};
    for method in [
        Method::StorageGet,
        Method::ConfiguredRequest,
        Method::SubscriptionApply,
        Method::ScheduleList,
        Method::BrowserOpen,
        Method::NotificationPost,
    ] {
        let call = Call {
            version: 1,
            request: Request {
                capability: method.capability(),
                scope: "self".into(),
            },
            method,
            budget: Default::default(),
            arguments: serde_json::Value::Null,
        };
        assert!(super::super::operations::validate_vm_sdk_admissibility(&call).is_ok());
        // Passing this narrow gate does not create a grant or execute IO; the
        // existing typed SDK dispatcher still validates request scope/budget.
    }
}

#[test]
fn host_disabled_and_pre_cancelled_invocations_keep_canonical_error_reasons() {
    let (_root, host, manager) = setup(false);
    let review = host.snapshot(&manager).components[0]
        .review
        .clone()
        .unwrap();
    assert_eq!(host.run(&manager, review).unwrap_err().code, "disabled");
    let review = approve(&host, &manager);
    assert_eq!(
        host.invoke_reviewed(
            &manager,
            None,
            review,
            "read".into(),
            serde_json::Value::Null,
            Arc::new(AtomicBool::new(true))
        )
        .unwrap_err()
        .code,
        "cancelled"
    );
}
