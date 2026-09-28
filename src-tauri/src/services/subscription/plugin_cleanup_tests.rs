use super::*;

fn owned(
    plugin: &str,
    provider: &str,
    remote: &str,
    active: bool,
) -> (ProxyConfigProfile, SubscriptionProfile) {
    let owner = ManagedSubscriptionSource {
        plugin_id: plugin.into(),
        provider_id: provider.into(),
        remote_subscription_id: remote.into(),
        revision: None,
        source_name: None,
    };
    let (id, config_id) = managed_subscription_ids(plugin, provider, remote);
    let profile = ProxyConfigProfile {
        id: config_id.clone(),
        name: remote.into(),
        kernel: "zero".into(),
        format: "json".into(),
        path: None,
        content: Some(json!({"outbounds": []})),
        active,
        managed_source: Some(owner.clone()),
        updated_at_unix_ms: 1,
        capabilities: Default::default(),
    };
    let subscription = SubscriptionProfile {
        id,
        name: remote.into(),
        url: provider.into(),
        enabled: true,
        kernel: "zero".into(),
        format: "zero".into(),
        target_proxy_config_id: Some(config_id),
        managed_source: Some(owner),
        policy_selections: Default::default(),
        update_interval_secs: None,
        user_agent: None,
        node_count: None,
        upload_bytes: None,
        download_bytes: None,
        used_bytes: None,
        total_bytes: None,
        expire_at_unix_ms: None,
        updated_at_unix_ms: 1,
        last_sync_at_unix_ms: None,
        last_error: None,
    };
    (profile, subscription)
}

#[test]
fn uninstall_selects_all_provider_namespaces_and_preserves_other_owners() {
    let (active, first) = owned("plugin-a", "https://one.example", "one", true);
    let (inactive, second) = owned("plugin-a", "https://two.example", "two", false);
    let (foreign, foreign_sub) = owned("plugin-b", "https://one.example", "one", false);
    let mut manual = foreign.clone();
    manual.id = "manual-config".into();
    manual.managed_source = None;
    let mut manual_sub = foreign_sub.clone();
    manual_sub.id = "manual-subscription".into();
    manual_sub.managed_source = None;
    manual_sub.target_proxy_config_id = Some(manual.id.clone());
    let profiles = vec![active.clone(), inactive.clone(), foreign, manual];
    let subscriptions = vec![first.clone(), second.clone(), foreign_sub, manual_sub];
    let cleanup = plan(&profiles, &subscriptions, "plugin-a").unwrap();
    assert_eq!(
        cleanup.configs.iter().map(|p| &p.id).collect::<Vec<_>>(),
        vec![&inactive.id, &active.id]
    );
    assert_eq!(
        cleanup.subscription_ids,
        BTreeSet::from([first.id, second.id])
    );
    assert!(plan(&profiles, &subscriptions, "not-installed")
        .unwrap()
        .subscription_ids
        .is_empty());
}

#[test]
fn uninstall_can_retry_missing_or_detached_targets_and_find_config_only_orphans() {
    let (config, mut subscription) = owned("plugin-a", "https://one.example", "one", false);
    let cleanup = plan(&[], &[subscription.clone()], "plugin-a").unwrap();
    assert!(cleanup.subscription_ids.contains(&subscription.id));
    subscription.target_proxy_config_id = None;
    assert!(plan(&[], &[subscription.clone()], "plugin-a").is_ok());
    let cleanup = plan(&[config.clone()], &[], "plugin-a").unwrap();
    assert_eq!(cleanup.configs[0].id, config.id);
    assert!(cleanup.subscription_ids.contains(&subscription.id));
}

#[test]
fn uninstall_refuses_to_delete_configurations_shared_with_manual_subscriptions() {
    let (config, subscription) = owned("plugin-a", "https://one.example", "one", false);
    let mut manual = subscription.clone();
    manual.id = "manual".into();
    manual.managed_source = None;
    assert!(plan(&[config], &[subscription, manual], "plugin-a").is_err());
}

#[test]
fn uninstall_refuses_foreign_or_corrupt_configuration_ownership() {
    let (config, subscription) = owned("plugin-a", "https://one.example", "one", false);
    let mut foreign = config.clone();
    foreign.managed_source.as_mut().unwrap().plugin_id = "plugin-b".into();
    assert!(plan(&[foreign], &[subscription.clone()], "plugin-a").is_err());
    let mut corrupt = config;
    corrupt.id = "manual-config".into();
    assert!(plan(&[corrupt], &[subscription], "plugin-a").is_err());
}

#[test]
fn uninstall_refuses_subscriptions_bound_outside_their_namespace() {
    let (_, mut subscription) = owned("plugin-a", "https://one.example", "one", false);
    subscription.target_proxy_config_id = Some("manual-config".into());
    assert!(plan(&[], &[subscription], "plugin-a").is_err());
}
