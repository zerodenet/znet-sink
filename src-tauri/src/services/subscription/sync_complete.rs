use super::*;
use crate::models::subscription::ManagedSubscriptionSyncComplete;
use tauri::Emitter;

/// Record a successful content check against the revision already applied by
/// this plugin. Quota refreshes intentionally do not imply content-sync success.
pub fn complete_managed_sync_authorized<F>(
    app: AppHandle,
    input: ManagedSubscriptionSyncComplete,
    authorize: F,
) -> AppResult<SubscriptionProfile>
where
    F: Fn() -> AppResult<()>,
{
    let (id, owner, revision) = validate(input)?;
    let state = app.state::<AppState>();
    let _in_flight = begin_in_flight(state.subscription_syncs(), "subscription", &id)?;
    authorize()?;
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let (next, updated) = commit(
        &subscriptions,
        &id,
        &owner,
        &revision,
        now_unix_ms(),
        authorize,
        domain_store::save_subscriptions,
    )?;
    *subscriptions = next;
    drop(subscriptions);
    // Persistence is authoritative; an unavailable window cannot undo it.
    let _ = app.emit("subscriptions:updated", json!({"subscriptionId": id}));
    Ok(updated)
}

fn validate(
    input: ManagedSubscriptionSyncComplete,
) -> AppResult<(String, ManagedSubscriptionSource, String)> {
    let (id, owner) = validate_managed_owner(
        input.plugin_id,
        input.provider_id,
        input.remote_subscription_id,
    )?;
    if input.revision.trim().is_empty()
        || input.revision.trim() != input.revision
        || input.revision.len() > 512
        || input.revision.chars().any(char::is_control)
    {
        return Err(managed_metadata_invalid("revision", "revision is invalid"));
    }
    Ok((id, owner, input.revision))
}

fn commit(
    previous: &[SubscriptionProfile],
    id: &str,
    owner: &ManagedSubscriptionSource,
    revision: &str,
    now: u64,
    authorize: impl FnOnce() -> AppResult<()>,
    persist: impl FnOnce(&[SubscriptionProfile]) -> AppResult<()>,
) -> AppResult<(Vec<SubscriptionProfile>, SubscriptionProfile)> {
    let mut next = previous.to_vec();
    let profile = next
        .iter_mut()
        .find(|profile| profile.id == id)
        .ok_or_else(|| AppError::not_found("subscription", id.to_owned()))?;
    let source = profile.managed_source.as_ref().ok_or_else(|| {
        AppError::conflict("subscription", id.to_owned(), "subscription is not managed")
    })?;
    if !source.same_namespace(owner) || source.revision.as_deref() != Some(revision) {
        return Err(AppError::conflict(
            "subscription",
            id.to_owned(),
            "managed subscription owner or applied revision does not match",
        ));
    }
    profile.last_sync_at_unix_ms = Some(now);
    profile.updated_at_unix_ms = now;
    profile.last_error = None;
    let updated = profile.clone();
    authorize()?;
    persist(&next)?;
    Ok((next, updated))
}

#[cfg(test)]
pub(crate) fn complete_managed_sync_in_acceptance_store(
    dir: &std::path::Path,
    previous: &[SubscriptionProfile],
    input: ManagedSubscriptionSyncComplete,
    authorize: impl FnOnce() -> AppResult<()>,
) -> AppResult<(Vec<SubscriptionProfile>, SubscriptionProfile)> {
    let (id, owner, revision) = validate(input)?;
    commit(
        previous,
        &id,
        &owner,
        &revision,
        now_unix_ms(),
        authorize,
        |next| domain_store::save_subscriptions_to_dir(dir, next),
    )
}

#[cfg(test)]
#[path = "sync_complete_tests.rs"]
mod tests;
