use super::*;
use znet_plugin_sandbox::{
    contract::Capability,
    sdk::{Call, Method},
};

fn call(method: Method, arguments: serde_json::Value) -> Call {
    Call {
        version: 1,
        request: Request {
            capability: Capability::BrowserCallback,
            scope: "self".into(),
        },
        method,
        arguments,
        budget: Default::default(),
    }
}
struct Cleanup(Store);
impl Drop for Cleanup {
    fn drop(&mut self) {
        self.0.cancel_owner("org.example.plugin/identity");
    }
}
#[test]
fn replacement_grant_cannot_poll_or_cancel_a_callback_created_by_the_original_grant() {
    for method in [Method::CallbackPoll, Method::CallbackCancel] {
        let (_root, host, manager) = super::super::tests::setup_component(
            "42",
            serde_json::json!({"capability":"browser.callback","scope":"self"}),
        );
        let _cleanup = Cleanup(host.callbacks.clone());
        let review = host.snapshot(&manager).components.remove(0).review.unwrap();
        host.authorize(
            &manager,
            review,
            vec![Request {
                capability: Capability::BrowserCallback,
                scope: "self".into(),
            }],
        )
        .unwrap();
        let created = host
            .sdk_call(
                &manager,
                "org.example.plugin",
                "identity",
                call(Method::CallbackCreate, serde_json::json!({})),
            )
            .unwrap();
        let id = created["sessionId"].as_str().unwrap();
        let url = reqwest::Url::parse(created["callbackUrl"].as_str().unwrap()).unwrap();
        let mut client = TcpStream::connect(("127.0.0.1", url.port().unwrap())).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(3)))
            .unwrap();
        write!(
            client,
            "GET {}?code=isolated-fixture HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n",
            url.path()
        )
        .unwrap();
        client.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        let old = host.snapshot(&manager).components.remove(0).review.unwrap();
        manager.revoke_component(&old.key);
        host.checked_rescan(&manager).unwrap();
        let new = host.snapshot(&manager).components.remove(0).review.unwrap();
        assert_ne!(old.revision, new.revision);
        let result = host.sdk_call(
            &manager,
            "org.example.plugin",
            "identity",
            call(method, serde_json::json!({"sessionId":id})),
        );
        assert!(
            result.is_err(),
            "a replacement authority must not inspect or cancel the original callback"
        );
    }
}

fn fixture() -> (tempfile::TempDir, Host, Manager) {
    let fixture = super::super::tests::setup_component(
        "42",
        serde_json::json!({"capability":"browser.callback","scope":"self"}),
    );
    let (_, host, manager) = &fixture;
    let review = host.snapshot(manager).components.remove(0).review.unwrap();
    host.authorize(
        manager,
        review,
        vec![Request {
            capability: Capability::BrowserCallback,
            scope: "self".into(),
        }],
    )
    .unwrap();
    fixture
}
fn invocation(
    host: &Host,
    manager: &Manager,
) -> (Authority, znet_plugin_sandbox::policy::Lease, Request) {
    let request = Request {
        capability: Capability::BrowserCallback,
        scope: "self".into(),
    };
    let (component, authority) = host
        .sdk_context(manager, "org.example.plugin", "identity", &request)
        .unwrap();
    let lease = authority
        .begin_host_call(&component, Duration::from_secs(20), 64 * 1024)
        .unwrap();
    (authority, lease, request)
}
#[test]
fn revoked_callback_creation_or_cancellation_waiting_for_management_lock_has_no_new_effect() {
    for cancelling in [false, true] {
        let (_root, host, manager) = fixture();
        let _cleanup = Cleanup(host.callbacks.clone());
        let created = if cancelling {
            Some(
                host.sdk_call(
                    &manager,
                    "org.example.plugin",
                    "identity",
                    call(Method::CallbackCreate, serde_json::json!({})),
                )
                .unwrap(),
            )
        } else {
            None
        };
        let before = host
            .callbacks
            .sessions
            .lock()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        let (authority, lease, request) = invocation(&host, &manager);
        let host = Arc::new(host);
        let held = host.operation.lock().unwrap();
        let (started, waiting) = std::sync::mpsc::sync_channel(1);
        let worker = {
            let host = host.clone();
            std::thread::spawn(move || {
                started.send(()).unwrap();
                let check = || {
                    lease
                        .check(Some(&request))
                        .map_err(super::super::sdk::invocation_failure)
                };
                if let Some(created) = created {
                    host.callback_cancel_checked(
                        &manager,
                        "org.example.plugin",
                        "identity",
                        created["sessionId"].as_str().unwrap(),
                        &check,
                    )
                } else {
                    host.callback_create_checked(&manager, "org.example.plugin", "identity", &check)
                        .map(|_| ())
                }
            })
        };
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        authority.revoke();
        drop(held);
        assert!(worker.join().unwrap().is_err());
        assert_eq!(
            host.callbacks
                .sessions
                .lock()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>(),
            before
        );
    }
}

#[test]
fn original_invocation_is_checked_after_binding_and_immediately_before_session_publication() {
    let (_root, host, manager) = fixture();
    let _cleanup = Cleanup(host.callbacks.clone());
    let (authority, lease, request) = invocation(&host, &manager);
    let count = std::sync::atomic::AtomicUsize::new(0);
    let result = host.callback_create_checked(&manager, "org.example.plugin", "identity", &|| {
        if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 2 {
            authority.revoke();
        }
        lease
            .check(Some(&request))
            .map_err(super::super::sdk::invocation_failure)
    });
    assert!(result.is_err());
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 3);
    assert!(host.callbacks.sessions.lock().unwrap().is_empty());
}

#[test]
fn fresh_grant_discards_only_its_old_scoped_callbacks_before_applying_quota() {
    let (_root, host, manager) = fixture();
    let _cleanup = Cleanup(host.callbacks.clone());
    let mut ids = Vec::new();
    for _ in 0..MAX_OWNER_SESSIONS {
        let created = host
            .sdk_call(
                &manager,
                "org.example.plugin",
                "identity",
                call(Method::CallbackCreate, serde_json::json!({})),
            )
            .unwrap();
        ids.push(created["sessionId"].as_str().unwrap().to_owned());
    }
    manager.revoke_component("org.example.plugin/identity");
    host.checked_rescan(&manager).unwrap();
    let fresh = host
        .sdk_call(
            &manager,
            "org.example.plugin",
            "identity",
            call(Method::CallbackCreate, serde_json::json!({})),
        )
        .unwrap();
    assert_eq!(host.callbacks.sessions.lock().unwrap().len(), 1);
    for id in ids {
        assert!(host
            .sdk_call(
                &manager,
                "org.example.plugin",
                "identity",
                call(Method::CallbackPoll, serde_json::json!({"sessionId":id}))
            )
            .is_err());
    }
    assert_eq!(
        host.sdk_call(
            &manager,
            "org.example.plugin",
            "identity",
            call(
                Method::CallbackPoll,
                serde_json::json!({"sessionId":fresh["sessionId"]})
            )
        )
        .unwrap()["state"],
        "pending"
    );
}

#[test]
fn cancellation_rechecks_original_invocation_after_resource_validation() {
    let (_root, host, manager) = fixture();
    let _cleanup = Cleanup(host.callbacks.clone());
    let created = host
        .sdk_call(
            &manager,
            "org.example.plugin",
            "identity",
            call(Method::CallbackCreate, serde_json::json!({})),
        )
        .unwrap();
    let id = created["sessionId"].as_str().unwrap();
    let (authority, lease, request) = invocation(&host, &manager);
    let count = std::sync::atomic::AtomicUsize::new(0);
    let result =
        host.callback_cancel_checked(&manager, "org.example.plugin", "identity", id, &|| {
            if count.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 1 {
                authority.revoke();
            }
            lease
                .check(Some(&request))
                .map_err(super::super::sdk::invocation_failure)
        });
    assert!(result.is_err());
    assert_eq!(count.load(std::sync::atomic::Ordering::SeqCst), 2);
    assert!(host.callbacks.sessions.lock().unwrap().contains_key(id));
}

#[test]
fn identical_signed_package_reinstallation_does_not_inherit_original_callback() {
    let (root, host, manager) = fixture();
    let _cleanup = Cleanup(host.callbacks.clone());
    let created = host
        .sdk_call(
            &manager,
            "org.example.plugin",
            "identity",
            call(Method::CallbackCreate, serde_json::json!({})),
        )
        .unwrap();
    let registration = host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .find("org.example.plugin")
        .unwrap()
        .clone();
    let store = host.store().unwrap();
    let (review, _) = store.review_uninstall(&registration).unwrap();
    let bytes = std::fs::read(
        root.path()
            .join("packages")
            .join(format!("{}.zspkg", review.current_sha256)),
    )
    .unwrap();
    store.uninstall(&registration.id).unwrap();
    store
        .install(
            &bytes,
            &registration,
            &Target::native_desktop().unwrap(),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
    host.checked_rescan(&manager).unwrap();
    let new_review = host.snapshot(&manager).components.remove(0).review.unwrap();
    host.authorize(
        &manager,
        new_review,
        vec![Request {
            capability: Capability::BrowserCallback,
            scope: "self".into(),
        }],
    )
    .unwrap();
    assert!(host
        .sdk_call(
            &manager,
            "org.example.plugin",
            "identity",
            call(
                Method::CallbackPoll,
                serde_json::json!({"sessionId":created["sessionId"]})
            )
        )
        .is_err());
}
