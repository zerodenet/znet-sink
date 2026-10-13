//! Local publication adapter. SQLite still owns profile/subscription atomicity;
//! application settings are a separate store with explicit recovery reporting.
use crate::errors::{AppError, AppResult};
use crate::models::{proxy_config::ProxyConfigProfile, subscription::SubscriptionProfile};
use crate::services::proxy_config::ensure_managed_system_proxy_compatible;
use crate::services::{common::lock, domain_store};
use crate::state::app_state::AppState;
use znet_client_core::publication::{Publication, PublicationFailure};

pub(crate) fn commit(
    state: &AppState,
    previous: &[ProxyConfigProfile],
    next: Vec<ProxyConfigProfile>,
) -> AppResult<()> {
    let previous_subscriptions = lock(state.subscriptions(), "subscription")?.clone();
    let mut next_subscriptions = previous_subscriptions.clone();
    let profile_ids = next
        .iter()
        .map(|profile| profile.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    for subscription in &mut next_subscriptions {
        if subscription
            .target_proxy_config_id
            .as_deref()
            .is_some_and(|id| !profile_ids.contains(id))
        {
            subscription.target_proxy_config_id = None;
        }
    }
    commit_relational(
        state,
        previous,
        &previous_subscriptions,
        next,
        next_subscriptions,
    )
}

/// Atomically publish proxy configurations and subscriptions as one SQLite
/// relationship graph, then project the active profile and update both
/// in-memory collections. Callers must provide complete before/after
/// snapshots so a projection failure can restore the exact prior state.
pub(crate) fn commit_relational(
    state: &AppState,
    previous_profiles: &[ProxyConfigProfile],
    previous_subscriptions: &[SubscriptionProfile],
    next_profiles: Vec<ProxyConfigProfile>,
    next_subscriptions: Vec<SubscriptionProfile>,
) -> AppResult<()> {
    let previous_active = previous_profiles.iter().find(|profile| profile.active);
    let next_active = next_profiles.iter().find(|profile| profile.active);
    let active_config_changed = match (previous_active, next_active) {
        (Some(previous), Some(next)) => {
            previous.id != next.id
                || previous.kernel != next.kernel
                || previous.format != next.format
                || previous.path != next.path
                || previous.content != next.content
        }
        (None, None) => false,
        _ => true,
    };
    let next_active_profile = next_active.cloned();
    if let Some(active) = next_active {
        ensure_managed_system_proxy_compatible(active.content.as_ref())?;
    }

    let transaction = LocalPublication {
        state,
        previous: previous_profiles,
        next: &next_profiles,
        previous_subscriptions,
        next_subscriptions: &next_subscriptions,
    };
    znet_client_core::publication::publish(&transaction).map_err(publication_error)?;
    let mut profiles_guard = lock(state.proxy_configs(), "proxy_config")?;
    let mut subscriptions_guard = lock(state.subscriptions(), "subscription")?;
    *profiles_guard = next_profiles;
    *subscriptions_guard = next_subscriptions;
    drop(profiles_guard);
    drop(subscriptions_guard);
    if active_config_changed {
        state.client_core_configuration_committed(next_active_profile.as_ref());
    }
    Ok(())
}

struct LocalPublication<'a> {
    state: &'a AppState,
    previous: &'a [ProxyConfigProfile],
    next: &'a [ProxyConfigProfile],
    previous_subscriptions: &'a [SubscriptionProfile],
    next_subscriptions: &'a [SubscriptionProfile],
}
impl Publication for LocalPublication<'_> {
    type Error = AppError;
    fn write(&self) -> AppResult<()> {
        domain_store::save_relational_data(self.next, self.next_subscriptions)
    }
    fn project(&self) -> AppResult<()> {
        let mut app = lock(self.state.app_config(), "app_config")?.clone();
        app.local_proxy.source_proxy_config_id = None;
        let retained = self
            .next
            .iter()
            .map(|profile| profile.id.as_str())
            .collect();
        super::endpoints::prune(&mut app, &retained);
        super::endpoints::store(self.state, app)
    }
    fn restore(&self) -> AppResult<()> {
        domain_store::save_relational_data(self.previous, self.previous_subscriptions)
    }
}
fn publication_error(failure: PublicationFailure<AppError>) -> AppError {
    match failure {
        PublicationFailure::Write(error) => error,
        PublicationFailure::Projection {
            mut error,
            recovery,
        } => {
            if let Err(rollback) = recovery {
                error.message.push_str(&format!(
                    "; profile storage rollback failed: {}",
                    rollback.message
                ));
                error.details = Some(
                    serde_json::json!({"cause":error.details,"localRecovery":{"code":rollback.code,"message":rollback.message}}),
                );
            }
            error
        }
    }
}

/// Fixed-preview deletion publication. Hold both collection guards throughout
/// synchronous compare/write/project/publication so another relationship
/// writer cannot splice new objects into the user's confirmed target graph.
/// Never call this while retaining a collection guard or across an await.
pub(crate) fn commit_relational_if_current(
    state: &AppState,
    previous_profiles: &[ProxyConfigProfile],
    previous_subscriptions: &[SubscriptionProfile],
    expected_app: &crate::models::app_config::AppConfig,
    next_profiles: Vec<ProxyConfigProfile>,
    next_subscriptions: Vec<SubscriptionProfile>,
) -> AppResult<()> {
    let mut profiles = lock(state.proxy_configs(), "proxy_config")?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    ensure_graph_current(
        &profiles,
        &subscriptions,
        previous_profiles,
        previous_subscriptions,
    )?;
    let next_active = next_profiles.iter().find(|p| p.active).cloned();
    if let Some(active) = &next_active {
        ensure_managed_system_proxy_compatible(active.content.as_ref())?;
    }
    let mut candidate = expected_app.clone();
    candidate.local_proxy.source_proxy_config_id = None;
    let retained: std::collections::HashSet<_> =
        next_profiles.iter().map(|p| p.id.as_str()).collect();
    candidate
        .profile_edits
        .retain(|id, _| retained.contains(id.as_str()));
    let transaction = CheckedPublication {
        state,
        previous_profiles,
        previous_subscriptions,
        next_profiles: &next_profiles,
        next_subscriptions: &next_subscriptions,
        expected_app,
        candidate,
    };
    let active_changed = serde_json::to_value(previous_profiles.iter().find(|p| p.active))
        .map_err(|_| AppError::internal("profile snapshot encoding failed"))?
        != serde_json::to_value(next_active.as_ref())
            .map_err(|_| AppError::internal("profile snapshot encoding failed"))?;
    znet_client_core::publication::publish(&transaction).map_err(publication_error)?;
    *profiles = next_profiles;
    *subscriptions = next_subscriptions;
    drop(profiles);
    drop(subscriptions);
    if active_changed {
        state.client_core_configuration_committed(next_active.as_ref());
    }
    Ok(())
}
fn ensure_graph_current(
    current_profiles: &[ProxyConfigProfile],
    current_subscriptions: &[SubscriptionProfile],
    expected_profiles: &[ProxyConfigProfile],
    expected_subscriptions: &[SubscriptionProfile],
) -> AppResult<()> {
    let encode = |p: &[ProxyConfigProfile], s: &[SubscriptionProfile]| {
        serde_json::to_value((p, s))
            .map_err(|_| AppError::internal("relationship graph encoding failed"))
    };
    if encode(current_profiles, current_subscriptions)?
        != encode(expected_profiles, expected_subscriptions)?
    {
        return Err(AppError::conflict(
            "profile_removal",
            "",
            "配置或订阅关系已变化，请重新预览删除范围",
        ));
    }
    Ok(())
}
struct CheckedPublication<'a> {
    state: &'a AppState,
    previous_profiles: &'a [ProxyConfigProfile],
    previous_subscriptions: &'a [SubscriptionProfile],
    next_profiles: &'a [ProxyConfigProfile],
    next_subscriptions: &'a [SubscriptionProfile],
    expected_app: &'a crate::models::app_config::AppConfig,
    candidate: crate::models::app_config::AppConfig,
}
impl Publication for CheckedPublication<'_> {
    type Error = AppError;
    fn write(&self) -> AppResult<()> {
        domain_store::save_relational_data(self.next_profiles, self.next_subscriptions)
    }
    fn project(&self) -> AppResult<()> {
        crate::services::app_config::compare_and_replace_local(
            self.state,
            self.expected_app,
            self.candidate.clone(),
        )
    }
    fn restore(&self) -> AppResult<()> {
        domain_store::save_relational_data(self.previous_profiles, self.previous_subscriptions)
    }
}

#[cfg(test)]
mod removal_cas_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn graph_compare_rejects_an_added_link_before_publication() {
        let profile: ProxyConfigProfile = serde_json::from_value(json!({"id":"a","name":"a","kernel":"zero","format":"json","path":null,"content":{},"active":true,"updatedAtUnixMs":1,"capabilities":{"hasProxyNodes":false,"hasProxyGroups":false,"hasRouteRules":false,"hasRuleSets":false,"hasSelector":false,"hasUrlTest":false,"featureKeys":[]}})).unwrap();
        let sub: SubscriptionProfile = serde_json::from_value(json!({"id":"s","name":"s","url":"https://example.invalid","enabled":true,"kernel":"zero","format":"zero","targetProxyConfigId":"a","updatedAtUnixMs":1,"lastSyncAtUnixMs":null,"lastError":null})).unwrap();
        assert!(ensure_graph_current(
            std::slice::from_ref(&profile),
            &[],
            std::slice::from_ref(&profile),
            &[]
        )
        .is_ok());
        assert_eq!(
            ensure_graph_current(
                std::slice::from_ref(&profile),
                &[sub],
                std::slice::from_ref(&profile),
                &[]
            )
            .unwrap_err()
            .code,
            "conflict"
        );
        let mut changed = profile.clone();
        changed.content = Some(json!({"outbounds":[]}));
        assert!(ensure_graph_current(&[changed], &[], &[profile], &[]).is_err());
    }
}
