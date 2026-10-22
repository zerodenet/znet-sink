use super::*;

#[test]
fn migration_is_versioned_bounded_and_exportable() {
    let root = tempfile::tempdir().unwrap();
    let mut values = BTreeMap::new();
    values.insert("session/envelope".into(), STANDARD.encode(b"opaque"));
    migrate_checked(
        root.path(),
        "publisher",
        "org.example.plugin",
        Area::State,
        Migration {
            from: 1,
            to: 2,
            values: values.clone(),
        },
        &|| Ok(()),
    )
    .unwrap();
    let (version, exported) =
        export(root.path(), "publisher", "org.example.plugin", Area::State).unwrap();
    assert_eq!(version, 2);
    assert_eq!(exported, values);
    assert!(migrate_checked(
        root.path(),
        "publisher",
        "org.example.plugin",
        Area::State,
        Migration {
            from: 1,
            to: 2,
            values: BTreeMap::new()
        },
        &|| Ok(())
    )
    .is_err());
    assert!(migrate_checked(
        root.path(),
        "publisher",
        "org.example.plugin",
        Area::State,
        Migration {
            from: 2,
            to: 4,
            values: BTreeMap::new()
        },
        &|| Ok(())
    )
    .is_err());
}

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use znet_plugin_sandbox::contract::{Capability, Request};

#[derive(Clone, Copy)]
enum Mutation {
    Put,
    Delete,
    Clear,
    Migrate,
}
const MUTATIONS: [Mutation; 4] = [
    Mutation::Put,
    Mutation::Delete,
    Mutation::Clear,
    Mutation::Migrate,
];
const PLUGIN: &str = "org.example.plugin";

fn fixture() -> (tempfile::TempDir, Host, Manager) {
    let fixture = super::super::tests::setup_component(
        "42",
        serde_json::json!({
            "capability": "plugin.storage.write", "scope": "self"
        }),
    );
    for area in [Area::State, Area::Cache] {
        fixture
            .1
            .storage_put(PLUGIN, area, "original".into(), STANDARD.encode(b"old"))
            .unwrap();
    }
    fixture
}

fn invocation(
    host: &Host,
    manager: &Manager,
) -> (Authority, znet_plugin_sandbox::policy::Lease, Request) {
    let request = Request {
        capability: Capability::StorageWrite,
        scope: "self".into(),
    };
    let row = host.snapshot(manager).components.remove(0);
    host.authorize(manager, row.review.unwrap(), vec![request.clone()])
        .unwrap();
    let (component, authority) = host
        .sdk_context(manager, PLUGIN, "identity", &request)
        .unwrap();
    let lease = authority
        .begin_host_call(&component, Duration::from_secs(20), 64 * 1024)
        .unwrap();
    (authority, lease, request)
}

fn mutate(
    host: &Host,
    area: Area,
    mutation: Mutation,
    check: &dyn Fn() -> AppResult<()>,
) -> AppResult<()> {
    match mutation {
        Mutation::Put => host.storage_put_checked(
            PLUGIN,
            area,
            "replacement".into(),
            STANDARD.encode(b"new"),
            check,
        ),
        Mutation::Delete => host.storage_delete_checked(PLUGIN, area, "original", check),
        Mutation::Clear => host.storage_clear_checked(PLUGIN, area, check),
        Mutation::Migrate => host.storage_migrate_checked(
            PLUGIN,
            area,
            Migration {
                from: 1,
                to: 2,
                values: BTreeMap::from([("migrated".into(), STANDARD.encode(b"new"))]),
            },
            check,
        ),
    }
}

fn bytes(root: &Path, publisher: &str, area: Area) -> Vec<u8> {
    fs::read(file(root, publisher, PLUGIN, area).unwrap()).unwrap()
}

#[test]
fn queued_writes_do_not_borrow_authority_revoked_while_waiting_for_management_lock() {
    for mutation in MUTATIONS {
        for area in [Area::State, Area::Cache] {
            let (root, host, manager) = fixture();
            let publisher = host.namespace_publisher(PLUGIN).unwrap();
            let before = bytes(root.path(), &publisher, area);
            let (authority, lease, request) = invocation(&host, &manager);
            let host = Arc::new(host);
            let held = host.operation.lock().unwrap();
            let (started, waiting) = std::sync::mpsc::sync_channel(1);
            let worker = {
                let host = host.clone();
                std::thread::spawn(move || {
                    started.send(()).unwrap();
                    mutate(&host, area, mutation, &|| {
                        lease
                            .check(Some(&request))
                            .map_err(super::super::sdk::invocation_failure)
                    })
                })
            };
            waiting.recv_timeout(Duration::from_secs(5)).unwrap();
            authority.revoke();
            drop(held);
            assert!(worker.join().unwrap().is_err());
            assert_eq!(bytes(root.path(), &publisher, area), before);
            assert!(!metadata_path(root.path(), &publisher, PLUGIN)
                .unwrap()
                .exists());
        }
    }
}

#[test]
fn every_mutation_rechecks_original_invocation_immediately_before_atomic_publication() {
    for mutation in MUTATIONS {
        for area in [Area::State, Area::Cache] {
            let (root, host, manager) = fixture();
            let publisher = host.namespace_publisher(PLUGIN).unwrap();
            let before = bytes(root.path(), &publisher, area);
            let (authority, lease, request) = invocation(&host, &manager);
            let calls = AtomicUsize::new(0);
            let result = mutate(&host, area, mutation, &|| {
                if calls.fetch_add(1, Ordering::SeqCst) == 1 {
                    authority.revoke();
                }
                lease
                    .check(Some(&request))
                    .map_err(super::super::sdk::invocation_failure)
            });
            assert!(result.is_err());
            assert_eq!(calls.load(Ordering::SeqCst), 2);
            assert_eq!(bytes(root.path(), &publisher, area), before);
            assert!(!metadata_path(root.path(), &publisher, PLUGIN)
                .unwrap()
                .exists());
            // Dropped temporary files do not become persistent guest state.
            assert_eq!(
                fs::read_dir(directory(root.path(), &publisher, PLUGIN).unwrap())
                    .unwrap()
                    .count(),
                2
            );
        }
    }
}

#[test]
fn migration_revoked_between_content_and_schema_publication_retains_explicit_partial_state() {
    let (root, host, manager) = fixture();
    let publisher = host.namespace_publisher(PLUGIN).unwrap();
    let (authority, lease, request) = invocation(&host, &manager);
    let calls = AtomicUsize::new(0);
    let error = mutate(&host, Area::State, Mutation::Migrate, &|| {
        if calls.fetch_add(1, Ordering::SeqCst) == 2 {
            authority.revoke();
        }
        lease
            .check(Some(&request))
            .map_err(super::super::sdk::invocation_failure)
    })
    .unwrap_err();
    assert_eq!(error.code, "plugin_storage_migration_uncertain");
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let public = super::super::sdk::public_failure(error);
    assert_eq!(public.code, znet_plugin_sandbox::sdk::ErrorCode::Uncertain);
    assert!(public.message.contains("不要自动重试"));
    let (schema, data) = export(root.path(), &publisher, PLUGIN, Area::State).unwrap();
    assert_eq!(schema, 1);
    assert_eq!(
        data,
        BTreeMap::from([("migrated".into(), STANDARD.encode(b"new"))])
    );
    assert_eq!(
        get(root.path(), &publisher, PLUGIN, Area::Cache, "original").unwrap(),
        Some(STANDARD.encode(b"old"))
    );
}

#[test]
fn runtime_envelope_checks_original_lease_at_publication_and_preserves_confirmed_state() {
    let (root, host, manager) = fixture();
    let publisher = host.namespace_publisher(PLUGIN).unwrap();
    let before = bytes(root.path(), &publisher, Area::State);
    let (authority, lease, request) = invocation(&host, &manager);
    let _operation = host.operation_guard().unwrap();
    let result = apply_runtime_updates_checked(
        root.path(),
        &publisher,
        PLUGIN,
        BTreeMap::from([
            ("original".into(), None),
            ("late".into(), Some(STANDARD.encode(b"new"))),
        ]),
        &|| {
            authority.revoke();
            lease
                .check(Some(&request))
                .map_err(super::super::sdk::invocation_failure)
        },
    );
    assert!(result.is_err());
    assert_eq!(bytes(root.path(), &publisher, Area::State), before);
}

#[test]
fn valid_mutations_keep_area_boundaries_and_schema_contract() {
    let (_root, host, _manager) = fixture();
    host.storage_put(PLUGIN, Area::State, "new".into(), STANDARD.encode(b"value"))
        .unwrap();
    host.storage_delete(PLUGIN, Area::State, "original")
        .unwrap();
    assert_eq!(host.storage_list(PLUGIN, Area::State).unwrap(), ["new"]);
    host.storage_clear(PLUGIN, Area::State).unwrap();
    assert!(host.storage_list(PLUGIN, Area::State).unwrap().is_empty());
    assert_eq!(
        host.storage_list(PLUGIN, Area::Cache).unwrap(),
        ["original"]
    );
    host.storage_migrate(PLUGIN, Area::State, 1, 2, BTreeMap::new())
        .unwrap();
    assert_eq!(host.storage_export(PLUGIN, Area::State).unwrap().0, 2);
    assert_eq!(host.storage_export(PLUGIN, Area::Cache).unwrap().0, 1);
}

#[test]
fn migration_marker_io_failure_does_not_expose_cause_or_claim_content_was_rolled_back() {
    let (root, host, _manager) = fixture();
    let publisher = host.namespace_publisher(PLUGIN).unwrap();
    let marker = metadata_path(root.path(), &publisher, PLUGIN).unwrap();
    let calls = AtomicUsize::new(0);
    let error = mutate(&host, Area::State, Mutation::Migrate, &|| {
        // Block the marker publication only after the original schema read.
        if calls.fetch_add(1, Ordering::SeqCst) == 1 {
            fs::create_dir(&marker).unwrap();
        }
        Ok(())
    })
    .unwrap_err();
    assert_eq!(error.code, "plugin_storage_migration_uncertain");
    assert_eq!(error.message, MIGRATION_UNCERTAIN);
    assert!(error.details.is_none());
    let public = super::super::sdk::public_failure(error);
    assert_eq!(public.code, znet_plugin_sandbox::sdk::ErrorCode::Uncertain);
    assert_eq!(public.message, MIGRATION_UNCERTAIN);
    assert!(public.field_path.is_none());
    assert!(public.diagnostics.is_empty());
    let content = load(root.path(), &publisher, PLUGIN, Area::State).unwrap();
    assert_eq!(content.0.get("migrated"), Some(&STANDARD.encode(b"new")));
    assert!(!content.0.contains_key("original"));
    assert!(marker.is_dir());
}
