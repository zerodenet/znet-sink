//! Unified configuration identity over the existing normalized stores.
//! Reads never migrate, rewrite, or discard legacy source associations.
use serde::Serialize;

use crate::errors::AppResult;
use crate::models::proxy_config::ProxyConfigProfile;
use crate::models::subscription::{ManagedSubscriptionSource, SubscriptionProfile};
use crate::services::common::lock;
use crate::state::app_state::AppState;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigurationEntry {
    pub id: String,
    pub profile_id: Option<String>,
    pub name: String,
    pub active: bool,
    pub has_content: bool,
    pub format: String,
    pub path: Option<String>,
    pub updated_at_unix_ms: u64,
    pub managed_source: Option<ManagedSubscriptionSource>,
    pub sources: Vec<SubscriptionProfile>,
    pub missing_profile: bool,
}

pub fn list(state: &AppState) -> AppResult<Vec<ConfigurationEntry>> {
    // Match the profile -> subscription lock order used by relational publication.
    // Holding both provides a single view, without cloning large JSON contents.
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let sources = lock(state.subscriptions(), "subscription")?;
    Ok(project(&profiles, &sources))
}

fn project(
    profiles: &[ProxyConfigProfile],
    sources: &[SubscriptionProfile],
) -> Vec<ConfigurationEntry> {
    let mut entries = profiles
        .iter()
        .map(|profile| ConfigurationEntry {
            id: format!("profile:{}", profile.id),
            profile_id: Some(profile.id.clone()),
            name: profile.name.clone(),
            active: profile.active,
            has_content: profile.content.is_some(),
            format: profile.format.clone(),
            path: profile.path.clone(),
            updated_at_unix_ms: profile.updated_at_unix_ms,
            managed_source: profile.managed_source.clone(),
            sources: Vec::new(),
            missing_profile: false,
        })
        .collect::<Vec<_>>();
    for source in sources {
        if let Some(entry) = entries.iter_mut().find(|entry| {
            entry
                .profile_id
                .as_ref()
                .is_some_and(|id| Some(id) == source.target_proxy_config_id.as_ref())
        }) {
            entry.sources.push(source.clone());
        } else {
            // Keep unsynced and dangling historical subscriptions accessible.
            // Do not silently attach them to a similarly named configuration.
            entries.push(ConfigurationEntry {
                id: format!("subscription:{}", source.id),
                profile_id: None,
                name: source.name.clone(),
                active: false,
                has_content: false,
                format: source.format.clone(),
                path: None,
                updated_at_unix_ms: source.updated_at_unix_ms,
                managed_source: source.managed_source.clone(),
                sources: vec![source.clone()],
                missing_profile: source.target_proxy_config_id.is_some(),
            });
        }
    }
    entries
}

#[cfg(test)]
#[path = "configuration_catalog/tests.rs"]
mod tests;
