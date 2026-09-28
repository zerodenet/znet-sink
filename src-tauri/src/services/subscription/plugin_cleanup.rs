//! Host lifecycle cleanup uses persisted ownership, never guest storage or
//! guest permissions. It also handles a retry after partial configuration removal.
use super::*;
use std::collections::BTreeSet;

struct CleanupPlan {
    configs: Vec<ProxyConfigProfile>,
    subscription_ids: BTreeSet<String>,
}

fn plan(
    profiles: &[ProxyConfigProfile],
    subscriptions: &[SubscriptionProfile],
    plugin_id: &str,
) -> AppResult<CleanupPlan> {
    let mut configs = Vec::new();
    let mut subscription_ids = BTreeSet::new();
    for profile in profiles {
        let Some(owner) = profile
            .managed_source
            .as_ref()
            .filter(|owner| owner.plugin_id == plugin_id)
        else {
            continue;
        };
        let (subscription_id, config_id) = managed_subscription_ids(
            &owner.plugin_id,
            &owner.provider_id,
            &owner.remote_subscription_id,
        );
        if profile.id != config_id {
            return Err(AppError::conflict(
                "proxy_config",
                &profile.id,
                "plugin-owned configuration namespace is invalid",
            ));
        }
        if subscriptions.iter().any(|subscription| {
            subscription.target_proxy_config_id.as_deref() == Some(profile.id.as_str())
                && subscription
                    .managed_source
                    .as_ref()
                    .is_none_or(|source| !source.same_namespace(owner))
        }) {
            return Err(AppError::conflict(
                "proxy_config",
                &profile.id,
                "plugin-owned configuration is referenced by another subscription",
            ));
        }
        subscription_ids.insert(subscription_id);
        configs.push(profile.clone());
    }
    for subscription in subscriptions {
        let Some(owner) = subscription
            .managed_source
            .as_ref()
            .filter(|owner| owner.plugin_id == plugin_id)
        else {
            continue;
        };
        let (subscription_id, config_id) = managed_subscription_ids(
            &owner.plugin_id,
            &owner.provider_id,
            &owner.remote_subscription_id,
        );
        if subscription.id != subscription_id
            || subscription
                .target_proxy_config_id
                .as_deref()
                .is_some_and(|id| id != config_id)
        {
            return Err(AppError::conflict(
                "subscription",
                &subscription.id,
                "plugin-owned subscription namespace is invalid",
            ));
        }
        if profiles.iter().any(|profile| {
            profile.id == config_id
                && profile
                    .managed_source
                    .as_ref()
                    .is_none_or(|source| !source.same_namespace(owner))
        }) {
            return Err(AppError::conflict(
                "proxy_config",
                config_id,
                "plugin-owned subscription references another owner's configuration",
            ));
        }
        // A missing target or an already-detached relation is a valid retry.
        subscription_ids.insert(subscription_id);
    }
    // Avoid promoting another configuration that is also about to be removed.
    configs.sort_by_key(|profile| profile.active);
    Ok(CleanupPlan {
        configs,
        subscription_ids,
    })
}

pub(crate) async fn remove_plugin_owned(app: AppHandle, plugin_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let profiles = lock(state.proxy_configs(), "proxy_config")?.clone();
    let subscriptions = lock(state.subscriptions(), "subscription")?.clone();
    let cleanup = plan(&profiles, &subscriptions, plugin_id)?;
    if cleanup.configs.is_empty() && cleanup.subscription_ids.is_empty() {
        return Ok(());
    }
    let _syncs = cleanup
        .subscription_ids
        .iter()
        .map(|id| begin_in_flight(state.subscription_syncs(), "subscription", id))
        .collect::<AppResult<Vec<_>>>()?;

    for config in cleanup.configs {
        let owner = config
            .managed_source
            .expect("cleanup plan contains owned configurations");
        // Existing runtime removal switches to a remaining configuration, or
        // stops the core and its managed system proxy if none remains.
        proxy_config::remove_managed_runtime(app.clone(), config.id, owner).await?;
    }
    for subscription_id in &cleanup.subscription_ids {
        rule_set::remove_managed_subscription_rule_sets(state.inner(), subscription_id)?;
    }
    let _operation = state.proxy_config_operation().lock().await;
    let previous_profiles = lock(state.proxy_configs(), "proxy_config")?.clone();
    let previous_subscriptions = lock(state.subscriptions(), "subscription")?.clone();
    let next_subscriptions = previous_subscriptions
        .iter()
        .filter(|subscription| {
            subscription
                .managed_source
                .as_ref()
                .is_none_or(|owner| owner.plugin_id != plugin_id)
        })
        .cloned()
        .collect();
    crate::configuration::persistence::commit_relational(
        state.inner(),
        &previous_profiles,
        &previous_subscriptions,
        previous_profiles.clone(),
        next_subscriptions,
    )
}

#[cfg(test)]
#[path = "plugin_cleanup_tests.rs"]
mod tests;
