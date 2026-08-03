#[path = "subscription.rs"]
mod original;

pub use original::{parse_subscription_content, ParsedSubscriptionConfig, SyncAllOutcome};

use tauri::{AppHandle, Manager, State};

use crate::errors::AppResult;
use crate::models::subscription::{SubscriptionProfile, SubscriptionUpsert};
use crate::services::{common::lock, domain_store};
use crate::state::app_state::AppState;

fn uses_implicit_user_agent(user_agent: Option<&str>) -> bool {
    user_agent.is_none_or(|value| value.trim().is_empty())
}

fn should_prefer_native_zero(format: Option<&str>, user_agent: Option<&str>) -> bool {
    let format = format.unwrap_or_default().trim();
    (format.is_empty() || format.eq_ignore_ascii_case("auto"))
        && uses_implicit_user_agent(user_agent)
}

fn normalize_upsert(mut input: SubscriptionUpsert) -> SubscriptionUpsert {
    if should_prefer_native_zero(input.format.as_deref(), input.user_agent.as_deref()) {
        input.format = Some("zero-json".to_string());
    }
    input
}

fn migrate_legacy_auto_profiles(state: &AppState) -> AppResult<()> {
    let mut subscriptions = lock(state.subscriptions(), "subscription")?;
    let mut next = subscriptions.clone();
    let mut changed = false;

    for profile in &mut next {
        if profile.format.trim().eq_ignore_ascii_case("auto")
            && uses_implicit_user_agent(profile.user_agent.as_deref())
        {
            profile.format = "zero-json".to_string();
            changed = true;
        }
    }

    if !changed {
        return Ok(());
    }

    domain_store::save_subscriptions(&next)?;
    *subscriptions = next;
    Ok(())
}

pub fn list(state: State<'_, AppState>) -> AppResult<Vec<SubscriptionProfile>> {
    migrate_legacy_auto_profiles(state.inner())?;
    original::list(state)
}

pub fn get(state: State<'_, AppState>, id: String) -> AppResult<SubscriptionProfile> {
    migrate_legacy_auto_profiles(state.inner())?;
    original::get(state, id)
}

pub fn upsert(
    state: State<'_, AppState>,
    input: SubscriptionUpsert,
) -> AppResult<SubscriptionProfile> {
    migrate_legacy_auto_profiles(state.inner())?;
    original::upsert(state, normalize_upsert(input))
}

pub async fn sync(app_handle: AppHandle, id: String) -> AppResult<SubscriptionProfile> {
    {
        let state = app_handle.state::<AppState>();
        migrate_legacy_auto_profiles(state.inner())?;
    }
    original::sync(app_handle, id).await
}

pub async fn sync_all(app_handle: AppHandle) -> AppResult<SyncAllOutcome> {
    {
        let state = app_handle.state::<AppState>();
        migrate_legacy_auto_profiles(state.inner())?;
    }
    original::sync_all(app_handle).await
}

pub fn remove(state: State<'_, AppState>, id: String) -> AppResult<()> {
    original::remove(state, id)
}

pub fn spawn_auto_sync_scheduler(app: AppHandle) {
    if let Some(state) = app.try_state::<AppState>() {
        if let Err(error) = migrate_legacy_auto_profiles(state.inner()) {
            crate::services::file_logger::line(&format!(
                "subscription: failed to migrate legacy auto format: {}",
                error.message
            ));
        }
    }
    original::spawn_auto_sync_scheduler(app);
}

#[cfg(test)]
mod wrapper_tests {
    use super::{parse_subscription_content, should_prefer_native_zero, uses_implicit_user_agent};

    #[test]
    fn implicit_auto_prefers_native_zero() {
        assert!(should_prefer_native_zero(Some("auto"), None));
        assert!(should_prefer_native_zero(Some(""), Some("  ")));
        assert!(!should_prefer_native_zero(
            Some("auto"),
            Some("Custom-Client/1.0")
        ));
        assert!(!should_prefer_native_zero(Some("clash-yaml"), None));
    }

    #[test]
    fn blank_user_agent_is_implicit() {
        assert!(uses_implicit_user_agent(None));
        assert!(uses_implicit_user_agent(Some("  ")));
        assert!(!uses_implicit_user_agent(Some("Clash.Meta")));
    }

    #[test]
    fn native_zero_preserves_mieru_protocol_fields() {
        let parsed = parse_subscription_content(
            r#"{
                "outbounds": [{
                    "tag": "mieru-node",
                    "protocol": {
                        "type": "mieru",
                        "server": "node.example",
                        "port": 443,
                        "password": "test-password",
                        "transport": "UDP"
                    }
                }]
            }"#,
            "zero-json",
        )
        .unwrap();

        let protocol = &parsed.content["outbounds"][0]["protocol"];
        assert_eq!(protocol["type"], "mieru");
        assert_eq!(protocol["server"], "node.example");
        assert_eq!(protocol["port"], 443);
        assert_eq!(protocol["password"], "test-password");
        assert_eq!(protocol["transport"], "UDP");
    }
}
