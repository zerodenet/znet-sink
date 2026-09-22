use super::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use znet_plugin_sandbox::{
    contract::{sha256, Limits, Manifest},
    distribution::{
        directory::Registration,
        package::{self, Payload, SourceComponent},
    },
};

#[derive(serde::Deserialize)]
struct ExternalPluginBootstrap {
    configuration: BTreeMap<String, String>,
    state: BTreeMap<String, String>,
    vault_base64: BTreeMap<String, String>,
    scheduled_action: String,
    expected: serde_json::Value,
}

struct ExternalManagedAcceptanceStore {
    dir: std::path::PathBuf,
    profiles: Vec<crate::models::proxy_config::ProxyConfigProfile>,
    subscriptions: Vec<crate::models::subscription::SubscriptionProfile>,
    apply_count: usize,
    managed_subscription_id: Option<String>,
}

impl ExternalManagedAcceptanceStore {
    fn new(dir: std::path::PathBuf) -> Self {
        Self {
            dir,
            profiles: Vec::new(),
            subscriptions: Vec::new(),
            apply_count: 0,
            managed_subscription_id: None,
        }
    }

    fn seed_manual_collision(
        &mut self,
        input: &crate::models::subscription::ManagedSubscriptionApply,
    ) {
        if !self.profiles.is_empty() || !self.subscriptions.is_empty() {
            return;
        }
        self.profiles
            .push(crate::models::proxy_config::ProxyConfigProfile {
                id: "manual-config".into(),
                name: "Manual active".into(),
                kernel: "zero".into(),
                format: "json".into(),
                path: None,
                content: Some(serde_json::json!({"outbounds": []})),
                active: true,
                managed_source: None,
                updated_at_unix_ms: 1,
                capabilities: Default::default(),
            });
        self.subscriptions
            .push(crate::models::subscription::SubscriptionProfile {
                id: "manual-subscription".into(),
                name: format!("{} / {}", input.source_name, input.subscription_name),
                url: input.provider_id.clone(),
                enabled: true,
                kernel: "zero".into(),
                format: input.format.clone(),
                target_proxy_config_id: Some("manual-config".into()),
                managed_source: None,
                policy_selections: Default::default(),
                update_interval_secs: None,
                user_agent: None,
                node_count: None,
                upload_bytes: None,
                download_bytes: None,
                total_bytes: None,
                expire_at_unix_ms: None,
                updated_at_unix_ms: 1,
                last_sync_at_unix_ms: None,
                last_error: None,
            });
    }
}
const SEED: [u8; 32] = [17; 32];

#[test]
fn signed_page_resources_render_without_filesystem_or_network_urls() {
    let page = VerifiedPage {
        id: "manage".into(),
        title: "Manage".into(),
        kind: package::PageKind::Management,
        html: "<!doctype html><html><head></head><body><main>Manage</main></body></html>".into(),
        styles: vec!["main { color: red; }".into()],
        scripts: vec!["globalThis.ready = true;".into()],
    };
    let rendered = render_page(&page);
    assert!(rendered.contains(&format!(
        "data:text/css;base64,{}",
        STANDARD.encode(&page.styles[0])
    )));
    assert!(rendered.contains(&format!(
        "data:text/javascript;base64,{}",
        STANDARD.encode(&page.scripts[0])
    )));
    assert!(rendered.find("data:text/css").unwrap() < rendered.find("</head>").unwrap());
    assert!(rendered.find("data:text/javascript").unwrap() < rendered.find("</body>").unwrap());
    assert!(!rendered.contains("file://"));
    assert!(!rendered.contains("http://"));
}
// Fixtures exercise the real Host, which checks the compiled client version.
const HOST_VERSION: &str = env!("CARGO_PKG_VERSION");
fn registration() -> Registration {
    serde_json::from_value(serde_json::json!({"id":"org.example.plugin","repository":"https://github.com/example/plugin","publisher":{"id":"example","public_key":STANDARD.encode(ed25519_dalek::SigningKey::from_bytes(&SEED).verifying_key().to_bytes())},"name":"Example","description":"Test","license":"MIT","maintainers":["example"],"release_source":{"type":"github-releases","metadata_asset":"marketplace-entry.json"},"surfaces":[],"capabilities":["plugin.self.read","network.get","network.request","network.configured.request","records.summary.read"]})).unwrap()
}
fn setup(unsupported: bool) -> (tempfile::TempDir, Host, Manager) {
    let source = "JSON.parse(hostCall('{\"capability\":\"plugin.self.read\",\"scope\":\"self\"}'))";
    let request = if unsupported {
        serde_json::json!({"capability":"records.summary.read","scope":"selection:test"})
    } else {
        serde_json::json!({"capability":"plugin.self.read","scope":"self"})
    };
    setup_component(source, request)
}
fn setup_component(source: &str, request: serde_json::Value) -> (tempfile::TempDir, Host, Manager) {
    setup_component_with_configuration(source, request, serde_json::Value::Null)
}
fn setup_component_with_configuration(
    source: &str,
    request: serde_json::Value,
    configuration: serde_json::Value,
) -> (tempfile::TempDir, Host, Manager) {
    let root = tempfile::tempdir().unwrap();
    let host = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let manager = Manager::default();
    let manifest: Manifest = serde_json::from_value(serde_json::json!({"schema_version":1,"host":"znet-sink","plugin_id":"org.example.plugin","component_id":"identity","version":"1.0.0","requires_host":format!("={HOST_VERSION}"),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[request],"optional":[],"configuration":configuration,"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()})).unwrap();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: "1.0.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
        pages: Vec::new(),
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &SEED).unwrap();
    let directory = Directory {
        snapshot_version: None,
        schema_version: 2,
        host: "znet-sink".into(),
        plugins: vec![registration()],
    };
    host.store()
        .unwrap()
        .install(
            &bytes,
            &directory.plugins[0],
            &Target::native_desktop().unwrap(),
            HOST_VERSION,
        )
        .unwrap();
    local_state::save_directory(root.path(), &directory).unwrap();
    host.state.lock().unwrap().directory = Some(directory);
    host.rescan(&manager, &host.store().unwrap()).unwrap();
    (root, host, manager)
}

#[test]
fn declarative_configuration_is_validated_persisted_and_passed_to_the_guest() {
    let source = "pluginInput.configuration";
    let schema = serde_json::json!({
        "title":"Provider source",
        "description":"Configure one compatible provider.",
        "fields":[
            {"id":"name","label":"Name","kind":"text","required":true},
            {"id":"origin","label":"Origin","kind":"https_origin","required":true},
            {"id":"path","label":"Path","kind":"select","required":true,"default":"direct","options":[{"value":"direct","label":"Direct"},{"value":"core","label":"Core"}]}
        ]
    });
    let (root, host, manager) = setup_component_with_configuration(
        source,
        serde_json::json!({"capability":"plugin.self.read","scope":"self"}),
        schema,
    );
    let before = host.snapshot(&manager);
    let row = &before.components[0];
    assert!(!row.configuration.as_ref().unwrap().configured);
    assert!(host
        .authorize(
            &manager,
            row.review.clone().unwrap(),
            row.permissions
                .iter()
                .map(|value| value.request.clone())
                .collect()
        )
        .is_err());
    assert!(host
        .configure(
            &manager,
            "org.example.plugin/identity",
            BTreeMap::from([("name".into(), "Example".into())])
        )
        .is_err());
    let values = BTreeMap::from([
        ("name".into(), "Example".into()),
        ("origin".into(), "https://panel.example.com".into()),
        ("path".into(), "direct".into()),
    ]);
    let configured = host
        .configure(&manager, "org.example.plugin/identity", values.clone())
        .unwrap();
    assert!(
        configured.components[0]
            .configuration
            .as_ref()
            .unwrap()
            .configured
    );
    assert_eq!(
        configuration::values(root.path(), "org.example.plugin/identity").unwrap(),
        values
    );
    let review = approve(&host, &manager);
    assert_eq!(
        host.run(&manager, review).unwrap()["origin"],
        "https://panel.example.com"
    );
}

#[test]
fn configured_https_origin_lists_only_authorize_declared_members() {
    let schema = serde_json::json!({
        "title":"Provider sources",
        "fields":[
            {"id":"origins","label":"Origins","kind":"https_origin_list","required":true}
        ]
    });
    let (_root, host, manager) = setup_component_with_configuration(
        "true",
        serde_json::json!({"capability":"network.configured.request","scope":"origins"}),
        schema,
    );
    assert!(host
        .configure(
            &manager,
            "org.example.plugin/identity",
            BTreeMap::from([(
                "origins".into(),
                serde_json::to_string(&["https://one.example.com", "https://two.example.com"])
                    .unwrap(),
            )]),
        )
        .is_ok());
    assert_eq!(
        host.configured_origin(
            "org.example.plugin",
            "identity",
            "origins",
            "https://two.example.com/connect",
        )
        .unwrap(),
        "https://two.example.com"
    );
    assert!(host
        .configured_origin(
            "org.example.plugin",
            "identity",
            "origins",
            "https://other.example.com/connect",
        )
        .is_err());
    assert!(host
        .configure(
            &manager,
            "org.example.plugin/identity",
            BTreeMap::from([(
                "origins".into(),
                "[\"https://one.example.com\",\"https://one.example.com\"]".into(),
            )]),
        )
        .is_err());
    assert!(host
        .configure(
            &manager,
            "org.example.plugin/identity",
            BTreeMap::from([("origins".into(), "[]".into())]),
        )
        .is_ok());
    assert!(host
        .configured_origin(
            "org.example.plugin",
            "identity",
            "origins",
            "https://one.example.com/connect",
        )
        .is_err());
}
fn approve(host: &Host, manager: &Manager) -> Review {
    let snapshot = host.snapshot(manager);
    let row = &snapshot.components[0];
    let review = row.review.clone().unwrap();
    assert!(
        host.run(manager, review.clone()).is_err(),
        "installation is not authorization"
    );
    let grants = row.permissions.iter().map(|p| p.request.clone()).collect();
    host.authorize(manager, review, grants).unwrap().components[0]
        .review
        .clone()
        .unwrap()
}

/// Generic installed-package acceptance hook for an external publisher. It is
/// inert during the ordinary suite and exercises the same local-import path as
/// the desktop file picker when the two environment variables are provided.
#[test]
fn external_local_package_import_runs_in_vm_when_requested() {
    let Ok(package_path) = std::env::var("ZNET_EXTERNAL_PLUGIN_PACKAGE") else {
        return;
    };
    let plugin_id = std::env::var("ZNET_EXTERNAL_PLUGIN_ID")
        .expect("ZNET_EXTERNAL_PLUGIN_ID is required with ZNET_EXTERNAL_PLUGIN_PACKAGE");
    let bytes = std::fs::read(&package_path).expect("read external plugin package");
    let managed_acceptance = std::env::var_os("ZNET_EXTERNAL_PLUGIN_MANAGED_ACCEPTANCE").is_some();
    let mut bootstrap = std::env::var("ZNET_EXTERNAL_PLUGIN_BOOTSTRAP")
        .ok()
        .map(|path| {
            serde_json::from_slice::<ExternalPluginBootstrap>(
                &std::fs::read(path).expect("read external plugin bootstrap"),
            )
            .expect("decode external plugin bootstrap")
        });
    if managed_acceptance {
        let bootstrap = bootstrap
            .as_mut()
            .expect("managed subscription acceptance requires a bootstrap fixture");
        let binding = bootstrap
            .state
            .get_mut("source/source-1/subscription/binding")
            .expect("managed subscription acceptance binding");
        let mut value: serde_json::Value =
            serde_json::from_str(binding).expect("decode managed subscription binding");
        value["revision"] = serde_json::Value::Null;
        *binding = serde_json::to_string(&value).unwrap();
        bootstrap.expected = serde_json::json!({"ok":true,"changed":true,"unread":1});
    }
    #[cfg(feature = "acceptance-test-utils")]
    if bootstrap.is_some() {
        let ca_path = std::env::var("ZNET_EXTERNAL_PLUGIN_CA")
            .expect("ZNET_EXTERNAL_PLUGIN_CA is required with a bootstrap fixture");
        znet_client_capabilities::network::install_acceptance_root_certificate(
            std::fs::read(ca_path).expect("read external plugin acceptance CA"),
        )
        .expect("install external plugin acceptance CA");
    }
    let root = tempfile::tempdir().unwrap();
    let host = std::sync::Arc::new(Host {
        root: Some(root.path().into()),
        ..Host::default()
    });
    let manager = std::sync::Arc::new(Manager::default());
    let managed_store = managed_acceptance.then(|| {
        Arc::new(Mutex::new(ExternalManagedAcceptanceStore::new(
            root.path().join("managed-subscription-store"),
        )))
    });

    let review = host
        .preview_local_bytes(&manager, &bytes, &plugin_id)
        .expect("preview signed local package");
    assert!(review.first_install);
    assert!(review.local_trust);
    assert!(review.requires_approval);
    assert!(host
        .install_local_bytes(&manager, &bytes, &plugin_id, None)
        .is_err());
    let mut installed = host
        .install_local_bytes(&manager, &bytes, &plugin_id, Some(&review.candidate_digest))
        .expect("install explicitly approved local package");
    if let Some(bootstrap) = &bootstrap {
        installed = host
            .configure(
                &manager,
                &format!("{plugin_id}/provider-source"),
                bootstrap.configuration.clone(),
            )
            .expect("configure external plugin acceptance source");
    }
    assert_eq!(installed.components.len(), 1);
    assert_eq!(installed.pages.len(), 1);
    assert_eq!(installed.pages[0].plugin_id, plugin_id);

    let component = &installed.components[0];
    assert_eq!(component.plugin_id, plugin_id);
    assert!(component
        .configuration
        .as_ref()
        .is_some_and(|value| value.configured));
    let review = component
        .review
        .clone()
        .expect("installed component review");
    assert!(host.run(&manager, review.clone()).is_err());
    let grants = component
        .permissions
        .iter()
        .map(|permission| permission.request.clone())
        .collect();
    let authorized = host
        .authorize(&manager, review, grants)
        .expect("authorize declared external plugin permissions");
    let review = authorized.components[0]
        .review
        .clone()
        .expect("authorized component review");
    let raw = host
        .run(&manager, review.clone())
        .expect("execute external plugin in QuickJS VM");
    assert_eq!(raw["znet_plugin_result"], 1);
    assert_eq!(raw["value"]["phase"], "needs-configuration");
    let status = host
        .invoke(
            &manager,
            None,
            &plugin_id,
            "provider-source",
            "status.get".into(),
            serde_json::Value::Null,
        )
        .expect("invoke installed external plugin");
    assert_eq!(status["phase"], "needs-configuration");

    if let Some(bootstrap) = bootstrap {
        for (key, value) in bootstrap.state {
            host.storage_put(
                &plugin_id,
                namespace::Area::State,
                key,
                STANDARD.encode(value.as_bytes()),
            )
            .expect("seed external plugin state");
        }
        for (key, value) in bootstrap.vault_base64 {
            let value = STANDARD
                .decode(value)
                .expect("decode external plugin vault fixture");
            host.vault_put(&plugin_id, &key, &value)
                .expect("seed external plugin vault");
        }
        let (component, authority, configuration, publisher, plugin_state) = {
            let state = host.state.lock().unwrap();
            let loaded = state.loaded.get(&review.key).unwrap();
            let mut configuration = loaded
                .component
                .manifest()
                .configuration
                .as_ref()
                .map_or_else(BTreeMap::new, |schema| schema.defaults());
            configuration.extend(loaded.configuration.clone());
            (
                loaded.component.clone(),
                loaded.authority.clone().unwrap(),
                configuration,
                loaded.publisher_fingerprint.clone(),
                namespace::runtime_state(root.path(), &loaded.publisher_fingerprint, &plugin_id)
                    .unwrap(),
            )
        };
        let dispatch_host = host.clone();
        let dispatch_manager = manager.clone();
        let dispatch_plugin = plugin_id.clone();
        let dispatch_managed_store = managed_store.clone();
        let dispatcher: znet_plugin_sandbox::runtime::HostSdkDispatcher = std::sync::Arc::new(
            move |lease, input| {
                let result = serde_json::from_str::<znet_plugin_sandbox::sdk::Call>(input)
                    .map_err(|_| znet_plugin_sandbox::contract::Error::InvalidOutput)
                    .and_then(|call| {
                        if call.method == znet_plugin_sandbox::sdk::Method::SubscriptionApply {
                            if let Some(store) = &dispatch_managed_store {
                                let (managed, _) = dispatch_host
                                    .managed_subscription_input_with_lease(
                                        &dispatch_plugin,
                                        lease,
                                        call,
                                    )
                                    .map_err(|_| {
                                        znet_plugin_sandbox::contract::Error::PermissionDenied
                                    })?;
                                let managed_debug = (
                                    managed.format.clone(),
                                    managed.content.chars().take(80).collect::<String>(),
                                );
                                let mut store = store.lock().unwrap();
                                store.seed_manual_collision(&managed);
                                let (profiles, subscriptions, committed) =
                                    crate::services::subscription::apply_managed_to_acceptance_store(
                                        &store.dir,
                                        store.profiles.clone(),
                                        store.subscriptions.clone(),
                                        managed,
                                    )
                                    .unwrap_or_else(|error| {
                                        panic!(
                                            "managed subscription acceptance apply failed for {:?}: {} ({}) {:?}",
                                            managed_debug, error.message, error.code, error.details
                                        )
                                    });
                                store.profiles = profiles;
                                store.subscriptions = subscriptions;
                                store.apply_count += 1;
                                store.managed_subscription_id = Some(committed.id.clone());
                                return serde_json::to_value(committed).map_err(|_| {
                                    znet_plugin_sandbox::contract::Error::InvalidOutput
                                });
                            }
                        }
                        dispatch_host
                            .sdk_call_with_lease(
                                &dispatch_manager,
                                &dispatch_plugin,
                                "provider-source",
                                lease,
                                call,
                                None,
                                None,
                            )
                            .map_err(|_| znet_plugin_sandbox::contract::Error::PermissionDenied)
                    });
                let reply = match result {
                    Ok(value) => serde_json::json!({"version":1,"ok":true,"value":value}),
                    Err(_) => serde_json::json!({
                        "version":1,"ok":false,
                        "error":{"code":"acceptance_host_call_failed","message":"acceptance host call failed"}
                    }),
                };
                serde_json::to_string(&reply)
                    .map_err(|_| znet_plugin_sandbox::contract::Error::InvalidOutput)
            },
        );
        let mut envelope = serde_json::Value::Null;
        for _ in 0..if managed_acceptance { 2 } else { 1 } {
            envelope = znet_plugin_sandbox::runtime::execute_scheduled_for_host_with_input(
                &component,
                &authority,
                serde_json::json!({
                    "configuration": configuration,
                    "state": plugin_state,
                    "invocation": {
                        "action": bootstrap.scheduled_action,
                        "payload": {},
                        "now_unix_ms": crate::services::common::now_unix_ms()
                    }
                }),
                std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
                HOST_VERSION,
                dispatcher.clone(),
            )
            .expect("execute external plugin cross-host scheduled action");
            assert_eq!(envelope["znet_plugin_result"], 1);
            assert_eq!(envelope["value"], bootstrap.expected);
        }
        let updates: BTreeMap<String, Option<String>> =
            serde_json::from_value(envelope["state_updates"].clone()).unwrap();
        namespace::apply_runtime_updates(root.path(), &publisher, &plugin_id, updates).unwrap();
        let summary = host
            .storage_get(
                &plugin_id,
                namespace::Area::State,
                "source/source-1/messages/summary",
            )
            .unwrap()
            .expect("scheduled message summary update");
        let summary = STANDARD.decode(summary).unwrap();
        assert!(String::from_utf8(summary).unwrap().contains("checked_at"));

        if let Some(store) = &managed_store {
            let mut store = store.lock().unwrap();
            assert_eq!(store.apply_count, 2, "real package must apply twice");
            assert_eq!(store.profiles.len(), 2);
            assert_eq!(store.subscriptions.len(), 2);
            let manual_profile = store
                .profiles
                .iter()
                .find(|profile| profile.id == "manual-config")
                .unwrap();
            assert!(manual_profile.active);
            assert!(manual_profile.managed_source.is_none());
            let manual_subscription = store
                .subscriptions
                .iter()
                .find(|subscription| subscription.id == "manual-subscription")
                .unwrap();
            assert!(manual_subscription.managed_source.is_none());
            let managed_id = store.managed_subscription_id.clone().unwrap();
            let managed = store
                .subscriptions
                .iter()
                .find(|subscription| subscription.id == managed_id)
                .unwrap();
            assert_eq!(
                managed.managed_source.as_ref().unwrap().plugin_id,
                plugin_id
            );
            let managed_config = store
                .profiles
                .iter()
                .find(|profile| {
                    managed.target_proxy_config_id.as_deref() == Some(profile.id.as_str())
                })
                .unwrap();
            assert!(!managed_config.active);

            let wrong_owner = crate::services::subscription::remove_managed_from_acceptance_store(
                &store.dir,
                store.profiles.clone(),
                store.subscriptions.clone(),
                &managed_id,
                "org.example.foreign",
            )
            .unwrap_err();
            assert_eq!(wrong_owner.code, "conflict");
            let (profiles, subscriptions) =
                crate::services::subscription::remove_managed_from_acceptance_store(
                    &store.dir,
                    store.profiles.clone(),
                    store.subscriptions.clone(),
                    &managed_id,
                    &plugin_id,
                )
                .expect("remove managed subscription with exact package owner");
            store.profiles = profiles;
            store.subscriptions = subscriptions;
            assert_eq!(store.profiles.len(), 1);
            assert_eq!(store.subscriptions.len(), 1);
            assert_eq!(store.profiles[0].id, "manual-config");
            assert!(store.profiles[0].active);
            assert_eq!(store.subscriptions[0].id, "manual-subscription");
        }
    }

    let restarted = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let restarted_manager = Manager::default();
    let restarted_snapshot = restarted.snapshot(&restarted_manager);
    assert_eq!(restarted_snapshot.components.len(), 1);
    assert_eq!(restarted_snapshot.pages.len(), 1);
    assert!(restarted_snapshot.components[0].enabled);
    restarted
        .stop(&restarted_manager, &review.key)
        .expect("stop external plugin after restart");
    assert!(restarted.run(&restarted_manager, review).is_err());
}

#[test]
fn unpublished_local_package_uses_embedded_publisher_trust_and_survives_restart() {
    let root = tempfile::tempdir().unwrap();
    let host = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let manager = Manager::default();
    let source = "pluginInput.configuration";
    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "schema_version":1,"host":"znet-sink","plugin_id":"org.example.plugin","component_id":"identity","version":"1.0.0","requires_host":format!("={HOST_VERSION}"),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()
    })).unwrap();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: "1.0.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
        pages: vec![package::SourcePage {
            id: "manage".into(),
            title: "Manage".into(),
            kind: package::PageKind::Management,
            html: "<!doctype html><button>Sign in</button>".into(),
        }],
    };
    let bytes = package::sign_with_registration(
        &serde_json::to_vec(&payload).unwrap(),
        &SEED,
        registration(),
    )
    .unwrap();
    let review = host
        .preview_local_bytes(&manager, &bytes, "org.example.plugin")
        .unwrap();
    assert!(review.first_install);
    assert!(review.local_trust);
    assert!(review.requires_approval);
    assert_eq!(
        review.requested_surfaces,
        vec!["znet-sink.ui.management.v1"]
    );
    assert!(host
        .install_local_bytes(&manager, &bytes, "org.example.plugin", None)
        .is_err());
    let installed = host
        .install_local_bytes(
            &manager,
            &bytes,
            "org.example.plugin",
            Some(&review.candidate_digest),
        )
        .unwrap();
    assert_eq!(installed.components.len(), 1);
    assert_eq!(installed.pages.len(), 1);

    let restarted = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let snapshot = restarted.snapshot(&Manager::default());
    assert_eq!(snapshot.components.len(), 1);
    assert_eq!(snapshot.pages.len(), 1);
}

#[test]
fn marketplace_ceiling_failures_are_reported_as_specific_install_errors() {
    assert!(
        io::failure("package page exceeds registered surface ceiling")
            .message
            .contains("管理页面超出线上插件登记范围")
    );
    assert!(io::failure("package exceeds registered capability ceiling")
        .message
        .contains("权限超出线上插件登记范围"));
}

#[test]
fn legacy_local_package_uses_cached_key_without_marketplace_surface_ceiling() {
    let root = tempfile::tempdir().unwrap();
    let host = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let manager = Manager::default();
    let source = "pluginInput.configuration";
    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "schema_version":1,"host":"znet-sink","plugin_id":"org.example.plugin","component_id":"identity","version":"1.0.0","requires_host":format!("={HOST_VERSION}"),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any","required":[],"optional":[],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()
    })).unwrap();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: "1.0.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
        pages: vec![package::SourcePage {
            id: "manage".into(),
            title: "Manage".into(),
            kind: package::PageKind::Management,
            html: "<!doctype html><button>Sign in</button>".into(),
        }],
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &SEED).unwrap();
    let directory = Directory {
        snapshot_version: None,
        schema_version: 2,
        host: "znet-sink".into(),
        plugins: vec![registration()],
    };
    assert!(package::verify(&bytes, &directory.plugins[0]).is_err());
    local_state::save_directory(root.path(), &directory).unwrap();
    let review = host
        .preview_local_bytes(&manager, &bytes, "org.example.plugin")
        .unwrap();
    assert!(review.requires_approval);
    assert_eq!(
        review.requested_surfaces,
        vec!["znet-sink.ui.management.v1"]
    );
}
#[test]
fn signed_install_review_authorize_real_vm_and_stop_share_the_client_manager() {
    let (_root, host, manager) = setup(false);
    let installed = host.snapshot(&manager);
    assert_eq!(installed.components[0].description, "Test");
    assert_eq!(
        installed.components[0].repository,
        "https://github.com/example/plugin"
    );
    assert_eq!(installed.components[0].license, "MIT");
    let review = approve(&host, &manager);
    let result = host.run(&manager, review.clone()).unwrap();
    assert_eq!(result["plugin_id"], "org.example.plugin");
    assert!(manager
        .operations()
        .iter()
        .any(|o| o.capability == "plugin.self.read"));
    host.stop(&manager, &review.key).unwrap();
    assert!(host.run(&manager, review).is_err());
}
#[test]
fn declared_permission_ceiling_and_forged_grants_are_denied() {
    let (_root, host, manager) = setup(false);
    let review = host.snapshot(&manager).components[0]
        .review
        .clone()
        .unwrap();
    assert!(host
        .authorize(
            &manager,
            review,
            vec![Request {
                capability: Capability::NetworkGet,
                scope: "https://example.org".into()
            }]
        )
        .is_err());
}
#[test]
fn tampering_or_registry_withdrawal_revokes_an_already_authorized_component() {
    for tamper in [false, true] {
        let (root, host, manager) = setup(false);
        let review = approve(&host, &manager);
        if tamper {
            std::fs::write(root.path().join("installed.json"), b"invalid").unwrap();
        } else {
            host.state
                .lock()
                .unwrap()
                .directory
                .as_mut()
                .unwrap()
                .plugins
                .clear();
        }
        assert!(host.run(&manager, review.clone()).is_err());
        assert!(manager.component_snapshots().iter().all(|p| !p.enabled));
    }
}
#[test]
fn uninstall_prevents_execution_and_cached_registry_survives_restart() {
    let (_root, host, manager) = setup(false);
    let review = approve(&host, &manager);
    host.uninstall(&manager, "org.example.plugin").unwrap();
    assert!(host.run(&manager, review).is_err());
    let (root, host, manager) = setup(false);
    let _review = approve(&host, &manager);
    let restarted = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let restarted_manager = Manager::default();
    let snapshot = restarted.snapshot(&restarted_manager);
    assert!(snapshot.checked);
    assert!(snapshot.components[0].enabled);
}

#[test]
fn signed_network_component_requires_approval_and_stop_prevents_more_requests() {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    let call = serde_json::json!({"capability":"network.request","scope":origin,"url":origin,"method":"POST","body":"sample"});
    let source = format!(
        "JSON.parse(hostCall({})).status",
        serde_json::to_string(&call.to_string()).unwrap()
    );
    let (_root, host, manager) = setup_component(
        &source,
        serde_json::json!({"capability":"network.request","scope":origin}),
    );
    assert!(host.snapshot(&manager).components[0].permissions[0].supported);
    let review = approve(&host, &manager);
    listener.set_nonblocking(true).unwrap();
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
    let server = std::thread::spawn(move || {
        let until = Instant::now() + std::time::Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < until => {
                    std::thread::sleep(std::time::Duration::from_millis(5))
                }
                Err(e) => panic!("request did not arrive: {e}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"sample") {
            let mut buf = [0; 1024];
            let n = stream.read(&mut buf).unwrap();
            assert!(n > 0);
            request.extend_from_slice(&buf[..n]);
        }
        assert!(request.starts_with(b"POST / HTTP/1.1"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
            .unwrap();
        listener
    });
    assert_eq!(
        host.run(&manager, review.clone()).unwrap(),
        serde_json::json!(200)
    );
    let listener = server.join().unwrap();
    host.stop(&manager, &review.key).unwrap();
    assert!(host.run(&manager, review).is_err());
    assert!(matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock));
}

#[test]
fn verified_install_upgrade_preserves_compatible_consent_and_invalid_bytes_preserve_current() {
    let (_root, host, manager) = setup(false);
    let old_review = approve(&host, &manager);
    let directory = host
        .state
        .lock()
        .unwrap()
        .directory
        .as_ref()
        .unwrap()
        .clone();
    let mut manifest = host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .component
        .manifest()
        .clone();
    manifest.version = "1.1.0".into();
    let source = "JSON.parse(hostCall('{\"capability\":\"plugin.self.read\",\"scope\":\"self\"}'))";
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: "1.1.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
        pages: Vec::new(),
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &SEED).unwrap();
    assert!(host
        .install_bytes(
            &manager,
            b"invalid",
            directory.clone(),
            "org.example.plugin",
            None,
        )
        .is_err());
    assert_eq!(host.snapshot(&manager).components[0].version, "1.0.0");
    assert!(host.snapshot(&manager).components[0].enabled);
    let next = host
        .install_bytes(
            &manager,
            &bytes,
            directory.clone(),
            "org.example.plugin",
            None,
        )
        .unwrap();
    assert_eq!(next.components[0].version, "1.1.0");
    assert!(next.components[0].enabled);
    assert!(host.run(&manager, old_review).is_err());
    assert!(host
        .install_bytes(&manager, &bytes, directory, "org.example.plugin", None,)
        .is_err());
    assert_eq!(host.snapshot(&manager).components[0].version, "1.1.0");
}

#[test]
fn opaque_plugin_state_is_namespaced_bounded_and_survives_restart() {
    let (root, host, manager) = setup(false);
    let value = STANDARD.encode(br#"{"device_credential":"opaque"}"#);
    host.storage_put(
        "org.example.plugin",
        namespace::Area::State,
        "session/device".into(),
        value.clone(),
    )
    .unwrap();
    assert_eq!(
        host.storage_get(
            "org.example.plugin",
            namespace::Area::State,
            "session/device",
        )
        .unwrap(),
        Some(value.clone())
    );
    assert!(host
        .storage_put(
            "org.example.plugin",
            namespace::Area::State,
            "../escape".into(),
            value.clone(),
        )
        .is_err());
    assert!(host
        .storage_put(
            "org.example.plugin",
            namespace::Area::State,
            "too-large".into(),
            STANDARD.encode(vec![0u8; 128 * 1024 + 1]),
        )
        .is_err());

    let restarted = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let restarted_manager = Manager::default();
    assert!(restarted.snapshot(&restarted_manager).checked);
    assert_eq!(
        restarted
            .storage_get(
                "org.example.plugin",
                namespace::Area::State,
                "session/device",
            )
            .unwrap(),
        Some(value)
    );
    drop(manager);
}

#[test]
fn permission_expanding_upgrade_requires_package_specific_confirmation() {
    let (_root, host, manager) = setup(false);
    approve(&host, &manager);
    let directory = host.state.lock().unwrap().directory.clone().unwrap();
    let mut manifest = host
        .state
        .lock()
        .unwrap()
        .loaded
        .values()
        .next()
        .unwrap()
        .component
        .manifest()
        .clone();
    manifest.version = "1.1.0".into();
    manifest.optional.push(Request {
        capability: Capability::NetworkGet,
        scope: "https://example.org".into(),
    });
    let source = "JSON.parse(hostCall('{\"capability\":\"plugin.self.read\",\"scope\":\"self\"}'))";
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: "1.1.0".into(),
        components: vec![SourceComponent {
            manifest,
            source: source.into(),
        }],
        pages: Vec::new(),
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &SEED).unwrap();
    let review = host
        .preview_bytes(&bytes, &directory, "org.example.plugin")
        .unwrap();
    assert_eq!(review.added_permissions.len(), 1);
    assert!(host
        .install_bytes(
            &manager,
            &bytes,
            directory.clone(),
            "org.example.plugin",
            None,
        )
        .is_err());
    assert_eq!(host.snapshot(&manager).components[0].version, "1.0.0");

    let next = host
        .install_bytes(
            &manager,
            &bytes,
            directory,
            "org.example.plugin",
            Some(&review.candidate_digest),
        )
        .unwrap();
    assert_eq!(next.components[0].version, "1.1.0");
    assert!(!next.components[0].enabled);
    assert_eq!(
        next.components[0]
            .permissions
            .iter()
            .filter(|permission| permission.granted)
            .count(),
        1
    );
}

#[test]
fn enabled_startup_hook_restores_consent_and_atomically_updates_opaque_state() {
    let root = tempfile::tempdir().unwrap();
    let host = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let manager = Manager::default();
    let next_value = STANDARD.encode(b"renewed-device-credential");
    let source = format!(
        "pluginInput.invocation.action === 'lifecycle.host_start' ? ({{znet_plugin_result:1,state_updates:{{'session/device':{}}},value:{{restored:true}}}}) : null",
        serde_json::to_string(&next_value).unwrap()
    );
    let manifest: Manifest = serde_json::from_value(serde_json::json!({
        "schema_version":1,"host":"znet-sink","plugin_id":"org.example.plugin","component_id":"identity","version":"1.0.0","requires_host":format!("={HOST_VERSION}"),"api_version":1,"runtime":"javascript-v1","minimum_isolation":"vm","targets":"any",
        "required":[{"capability":"plugin.self.read","scope":"self"}],"optional":[],"lifecycle":["host_start"],"source_sha256":sha256(source.as_bytes()),"limits":Limits::default()
    })).unwrap();
    let payload = Payload {
        schema_version: 1,
        host: "znet-sink".into(),
        plugin_id: "org.example.plugin".into(),
        version: "1.0.0".into(),
        components: vec![SourceComponent { manifest, source }],
        pages: Vec::new(),
    };
    let bytes = package::sign(&serde_json::to_vec(&payload).unwrap(), &SEED).unwrap();
    let directory = Directory {
        snapshot_version: None,
        schema_version: 2,
        host: "znet-sink".into(),
        plugins: vec![registration()],
    };
    host.store()
        .unwrap()
        .install(
            &bytes,
            &directory.plugins[0],
            &Target::native_desktop().unwrap(),
            HOST_VERSION,
        )
        .unwrap();
    local_state::save_directory(root.path(), &directory).unwrap();
    host.state.lock().unwrap().directory = Some(directory);
    host.rescan(&manager, &host.store().unwrap()).unwrap();
    approve(&host, &manager);
    host.storage_put(
        "org.example.plugin",
        namespace::Area::State,
        "session/device".into(),
        STANDARD.encode(b"expired"),
    )
    .unwrap();

    let restarted = Host {
        root: Some(root.path().into()),
        ..Host::default()
    };
    let restarted_manager = Manager::default();
    restarted.start_enabled(&restarted_manager);
    assert_eq!(
        restarted
            .storage_get(
                "org.example.plugin",
                namespace::Area::State,
                "session/device",
            )
            .unwrap(),
        Some(next_value)
    );
}
