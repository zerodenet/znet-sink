use super::*;
use serde_json::Value;
use znet_plugin_sandbox::{
    contract::{sha256, Limits, Manifest},
    distribution::{
        directory::Registration,
        package::{self, PageKind, Payload, SourceComponent, SourcePage},
    },
    sdk::{Budget, Method, SDK_VERSION},
};
fn setup() -> (tempfile::TempDir, Host, Manager) {
    let root = tempfile::tempdir().unwrap();
    let host = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let manager = Manager::default();
    let seed = [19; 32];
    let source = "({ok:true})";
    let request = Request {
        capability: Capability::FilesSelectionRead,
        scope: "user".into(),
    };
    let manifest:Manifest=serde_json::from_value(serde_json::json!({"schema_version":1,"host":"znet-sink","plugin_id":"org.example.page","component_id":"reader","version":"1.0.0","requires_host":format!("={}",env!("CARGO_PKG_VERSION")),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[request],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()})).unwrap();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.page".into(),
        version: "1.0.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
        pages: vec![SourcePage {
            id: "manage".into(),
            title: "Manage".into(),
            kind: PageKind::Management,
            html: "<!doctype html><html><head></head><body>Signed page</body></html>".into(),
        }],
    };
    let registration:Registration=serde_json::from_value(serde_json::json!({"id":"org.example.page","repository":"https://github.com/example/page","publisher":{"id":"example","public_key":STANDARD.encode(ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes())},"name":"Page","description":"Test","license":"MIT","maintainers":["example"],"release_source":{"type":"github-releases","metadata_asset":"marketplace-entry.json"},"surfaces":["znet-sink.ui.management.v1"],"capabilities":["files.selection.read"]})).unwrap();
    let directory = Directory {
        snapshot_version: None,
        schema_version: 2,
        host: "znet-sink".into(),
        plugins: vec![registration],
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &seed).unwrap();
    host.store()
        .unwrap()
        .install(
            &bytes,
            &directory.plugins[0],
            &Target::native_desktop().unwrap(),
            env!("CARGO_PKG_VERSION"),
        )
        .unwrap();
    local_state::save_directory(root.path(), &directory).unwrap();
    host.state.lock().unwrap().directory = Some(directory);
    host.rescan(&manager, &host.store().unwrap()).unwrap();
    let review = host.snapshot(&manager).components[0]
        .review
        .clone()
        .unwrap();
    host.authorize(&manager, review, vec![request]).unwrap();
    (root, host, manager)
}
fn call() -> Call {
    Call {
        version: SDK_VERSION,
        request: Request {
            capability: Capability::FilesSelectionRead,
            scope: "user".into(),
        },
        method: Method::FileRead,
        budget: Budget::default(),
        arguments: serde_json::json!({}),
    }
}
#[test]
fn atomic_signed_document_captures_original_review_and_close_cancels_original_lease() {
    let (_root, host, manager) = setup();
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    assert!(doc.html.contains("Signed page"));
    let review = doc.components[0].review.clone().unwrap();
    let (binding, lease) = host
        .page_lease(&manager, &doc.document_review, &review, &call())
        .unwrap();
    binding.check_lease(&lease, Some(&call().request)).unwrap();
    host.page_close(&doc.document_review);
    assert!(binding.check_lease(&lease, Some(&call().request)).is_err());
    assert!(host
        .page_lease(&manager, &doc.document_review, &review, &call())
        .is_err());
}
#[test]
fn revoke_and_reauthorize_never_turn_old_doc_into_new_authority() {
    let (_root, host, manager) = setup();
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    let review = doc.components[0].review.clone().unwrap();
    host.revoke_permissions(&manager, &review.key).unwrap();
    let next = host.snapshot(&manager).components[0]
        .review
        .clone()
        .unwrap();
    host.authorize(&manager, next, vec![call().request])
        .unwrap();
    assert!(host
        .page_lease(&manager, &doc.document_review, &review, &call())
        .is_err());
    let refreshed = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    assert!(host
        .page_lease(
            &manager,
            &refreshed.document_review,
            refreshed.components[0].review.as_ref().unwrap(),
            &call()
        )
        .is_ok());
}
#[test]
fn document_budget_expiry_and_host_shutdown_fail_closed_without_new_lease() {
    let (_root, host, manager) = setup();
    let mut docs = Vec::new();
    for _ in 0..LIMIT {
        docs.push(
            host.page_document(&manager, "org.example.page", "manage")
                .unwrap(),
        );
    }
    assert!(host
        .page_document(&manager, "org.example.page", "manage")
        .is_err());
    host.page_close(&docs[0].document_review);
    assert!(host
        .page_document(&manager, "org.example.page", "manage")
        .is_ok());
    let doc = &docs[1];
    host.page_documents
        .documents
        .lock()
        .unwrap()
        .get_mut(&doc.document_review)
        .unwrap()
        .deadline = Instant::now();
    assert!(host.page_check(&manager, &doc.document_review).is_err());
    let doc = &docs[2];
    let review = doc.components[0].review.as_ref().unwrap();
    let (binding, lease) = host
        .page_lease(&manager, &doc.document_review, review, &call())
        .unwrap();
    host.close_page_documents();
    assert!(binding.check_lease(&lease, Some(&call().request)).is_err());
    assert!(host
        .page_document(&manager, "org.example.page", "manage")
        .is_err());
}
#[test]
fn foreign_component_review_or_sdk_capability_mismatch_is_rejected() {
    let (_root, host, manager) = setup();
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    let mut review = doc.components[0].review.clone().unwrap();
    review.key = "org.other.plugin/reader".into();
    assert!(host
        .page_lease(&manager, &doc.document_review, &review, &call())
        .is_err());
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    let mut invalid = call();
    invalid.method = Method::BrowserOpen;
    assert!(host
        .page_lease(
            &manager,
            &doc.document_review,
            doc.components[0].review.as_ref().unwrap(),
            &invalid
        )
        .is_err());
}

#[test]
fn original_execution_denies_late_result_and_new_side_effect_after_page_close() {
    let (_root, host, manager) = setup();
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    let review = doc.components[0].review.as_ref().unwrap();
    let (_, lease) = host
        .page_lease(&manager, &doc.document_review, review, &call())
        .unwrap();
    let mut issued = false;
    let result = super::super::sdk::execute_original(&lease, &call().request, || {
        issued = true;
        host.page_close(&doc.document_review);
        Ok(serde_json::json!({"late":"must-not-deliver"}))
    });
    assert!(issued); // an issued effect cannot be represented as rolled back
    assert!(result.is_err());
    assert!(manager.operations().iter().any(|operation| matches!(
        operation.state,
        znet_client_core::capability::OperationState::DeliveryDenied(_)
    )));
    let mut new_effect = false;
    assert!(
        super::super::sdk::execute_original(&lease, &call().request, || {
            new_effect = true;
            Ok(Value::Null)
        })
        .is_err()
    );
    assert!(!new_effect);
}

#[test]
fn fresh_native_file_policy_cannot_deliver_for_revoked_original_component_epoch() {
    let (root, host, manager) = setup();
    let path = root.path().join("selected.txt");
    std::fs::write(&path, b"private-canary").unwrap();
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    let review = doc.components[0].review.as_ref().unwrap();
    let (_, lease) = host
        .page_lease(&manager, &doc.document_review, review, &call())
        .unwrap();
    let result = super::super::sdk::execute_original(&lease, &call().request, || {
        let selected = znet_client_capabilities::files::Selection::from_user_path(path);
        let native = znet_client_capabilities::host::operation(
            &manager,
            "test-selected-file",
            selected.permission(),
            1,
            1024,
            Duration::from_secs(5),
        )
        .unwrap();
        let bytes = znet_client_capabilities::files::read(&native, &selected, 1024)
            .and_then(|resource| resource.take(&native))
            .unwrap();
        host.revoke_permissions(&manager, &review.key).unwrap();
        Ok(serde_json::json!({"dataBase64":STANDARD.encode(bytes)}))
    });
    assert!(result.is_err());
    let mut issued = false;
    assert!(
        super::super::sdk::execute_original(&lease, &call().request, || {
            issued = true;
            Ok(Value::Null)
        })
        .is_err()
    );
    assert!(!issued);
}

#[test]
fn protected_material_is_not_consumed_before_original_permission_check() {
    let (_root, host, manager) = setup();
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    let (_, lease) = host
        .page_lease(
            &manager,
            &doc.document_review,
            doc.components[0].review.as_ref().unwrap(),
            &call(),
        )
        .unwrap();
    let (handle, _) = host
        .sensitive
        .insert(
            "org.example.page/reader".into(),
            "material".into(),
            b"protected-canary".to_vec(),
            60_000,
        )
        .unwrap();
    let mut protected = call();
    protected.method = Method::ProtectedLoad;
    protected.request.capability = Capability::RuntimeProtectedLoad;
    protected.request.scope = "active-runtime".into();
    protected.arguments = serde_json::json!({"handle":handle});
    assert!(host
        .take_protected_material_with_lease("org.example.page", "reader", &lease, protected)
        .is_err());
    let retained = host
        .sensitive
        .take("org.example.page/reader", &handle, Some("material"))
        .unwrap();
    assert_eq!(retained.as_slice(), b"protected-canary");
}

#[test]
fn corrupt_installation_store_cancels_document_and_inflight_binding() {
    let (root, host, manager) = setup();
    let doc = host
        .page_document(&manager, "org.example.page", "manage")
        .unwrap();
    let (binding, lease) = host
        .page_lease(
            &manager,
            &doc.document_review,
            doc.components[0].review.as_ref().unwrap(),
            &call(),
        )
        .unwrap();
    std::fs::write(root.path().join("installed.json"), b"broken-store").unwrap();
    assert!(host.page_check(&manager, &doc.document_review).is_err());
    assert!(binding.check().is_err());
    assert!(lease.check(Some(&call().request)).is_err());
    assert!(host.page_documents.get(&doc.document_review).is_err());
}
