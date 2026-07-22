use gui_lib::models::{
    proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile},
    rule_set::RuleSetProfile,
    subscription::SubscriptionProfile,
};
use gui_lib::services::domain_store;

#[test]
fn domain_store_roundtrips_profiles() {
    let dir = isolated_data_dir("domain-store-roundtrip");

    domain_store::save_proxy_configs_to_dir(
        &dir,
        &[ProxyConfigProfile {
            id: "proxy-config-1".to_string(),
            name: "Local".to_string(),
            kernel: "zero".to_string(),
            format: "json".to_string(),
            path: None,
            content: None,
            active: true,
            updated_at_unix_ms: 1,
            capabilities: ProxyConfigCapabilities::default(),
        }],
    )
    .unwrap();
    domain_store::save_subscriptions_to_dir(
        &dir,
        &[SubscriptionProfile {
            id: "subscription-1".to_string(),
            name: "Remote".to_string(),
            url: "https://example.com/sub".to_string(),
            enabled: true,
            kernel: "zero".to_string(),
            format: "auto".to_string(),
            target_proxy_config_id: None,
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
        }],
    )
    .unwrap();
    domain_store::save_rule_sets_to_dir(
        &dir,
        &[RuleSetProfile {
            id: "rule-set-1".to_string(),
            name: "GeoIP".to_string(),
            enabled: true,
            built_in: false,
            provenance: None,
            managed_by_subscription_id: None,
            common_binding: None,
            semantic_ir: serde_json::json!({
                "version": 1,
                "name": "GeoIP",
                "rules": [{"type":"domain_suffix","value":"example.com"}]
            }),
            source: None,
            source_state: Default::default(),
            artifact: None,
            updated_at_unix_ms: 1,
            last_sync_at_unix_ms: None,
            last_error: None,
        }],
    )
    .unwrap();

    let data = domain_store::load_all_from_dir(&dir).unwrap();

    assert_eq!(data.proxy_configs.len(), 1);
    assert_eq!(data.subscriptions.len(), 1);
    assert_eq!(data.rule_sets.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn domain_store_migrates_legacy_rule_profiles_to_semantic_assets() {
    let dir = isolated_data_dir("domain-store-rule-migration");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("rule-sets.json"),
        serde_json::to_vec_pretty(&serde_json::json!([{
            "id": "legacy",
            "name": "Legacy",
            "format": "clash-yaml",
            "enabled": true,
            "source": {"kind":"remote","url":"https://example.com/rules.yaml","path":null,"content":null},
            "compiledIr": {"version":1,"name":"Legacy","rules":[{"type":"domain_exact","value":"example.com"}]},
            "ruleCount": 1,
            "updatedAtUnixMs": 1
        }]))
        .unwrap(),
    )
    .unwrap();

    let data = domain_store::load_all_from_dir(&dir).unwrap();
    let migrated = &data.rule_sets[0];
    assert_eq!(migrated.semantic_ir["version"], 1);
    assert_eq!(
        migrated.source.as_ref().unwrap().format,
        "clash-classical-yaml"
    );
    assert!(migrated.artifact.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}
fn isolated_data_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
