//! Local publication adapter. SQLite still owns profile/subscription atomicity;
//! application settings are a separate store with explicit recovery reporting.
use crate::errors::{AppError, AppResult};
use crate::models::{proxy_config::ProxyConfigProfile, subscription::SubscriptionProfile};
use crate::services::proxy_config::{
    clear_local_proxy_source, ensure_managed_system_proxy_compatible, sync_local_proxy_from_profile,
};
use crate::services::{common::lock, domain_store};
use crate::state::app_state::AppState;
use znet_client_core::publication::{Publication, PublicationFailure};

pub(crate) fn commit(
    state: &AppState,
    previous: &[ProxyConfigProfile],
    next: Vec<ProxyConfigProfile>,
) -> AppResult<()> {
    let mut next = next;
    for profile in &mut next {
        if let Some(content) = profile.content.as_mut() {
            crate::services::rule_overlay::strip_profile_dns(content);
        }
    }
    let previous_active = previous.iter().find(|profile| profile.active);
    let next_active = next.iter().find(|profile| profile.active);
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

    if let Some(active) = next.iter().find(|profile| profile.active) {
        ensure_managed_system_proxy_compatible(active.content.as_ref())?;
    }
    let previous_subscriptions = lock(state.subscriptions(), "subscription")?.clone();
    let mut next_subscriptions = previous_subscriptions.clone();
    let mut subscriptions_changed = false;
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
            subscriptions_changed = true;
        }
    }
    let transaction = LocalPublication {
        state,
        previous,
        next: &next,
        previous_subscriptions: &previous_subscriptions,
        next_subscriptions: &next_subscriptions,
    };
    znet_client_core::publication::publish(&transaction).map_err(publication_error)?;
    let mut profiles_guard = lock(state.proxy_configs(), "proxy_config")?;
    let mut subscriptions_guard = lock(state.subscriptions(), "subscription")?;
    *profiles_guard = next;
    if subscriptions_changed {
        *subscriptions_guard = next_subscriptions;
    }
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
        match self.next.iter().find(|profile| profile.active) {
            Some(active) => sync_local_proxy_from_profile(self.state, active),
            None => clear_local_proxy_source(self.state),
        }
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
