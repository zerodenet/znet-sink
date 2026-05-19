use gui_lib::models::{
    proxy_config::{ProxyConfigCapabilities, ProxyConfigProfile},
    rule_set::{RuleSetProfile, RuleSetSource},
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
            format: "json".to_string(),
            enabled: true,
            source: RuleSetSource {
                kind: "inline".to_string(),
                url: None,
                path: None,
                content: Some(serde_json::json!([])),
            },
            updated_at_unix_ms: 1,
        }],
    )
    .unwrap();

    let data = domain_store::load_all_from_dir(&dir).unwrap();

    assert_eq!(data.proxy_configs.len(), 1);
    assert_eq!(data.subscriptions.len(), 1);
    assert_eq!(data.rule_sets.len(), 1);

    let _ = std::fs::remove_dir_all(&dir);
}

fn isolated_data_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
