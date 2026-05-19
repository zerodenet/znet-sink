use tauri::State;

use crate::errors::AppResult;
use crate::models::{
    capability::{CapabilityItem, GuiCapabilitySnapshot},
    proxy_config::ProxyConfigCapabilities,
};
use crate::services::common::lock;
use crate::state::app_state::AppState;

pub fn snapshot(state: State<'_, AppState>) -> AppResult<GuiCapabilitySnapshot> {
    let profiles = lock(state.proxy_configs(), "proxy_config")?;
    let active = profiles.iter().find(|profile| profile.active);
    let active_capabilities = active
        .map(|profile| profile.capabilities.clone())
        .unwrap_or_default();
    let missing_active_reason = active
        .is_none()
        .then(|| "no active proxy config".to_string());

    Ok(GuiCapabilitySnapshot {
        management: vec![
            enabled("proxyConfig"),
            enabled("subscriptions"),
            enabled("appLogs"),
            enabled("coreLogs"),
            enabled("appConfig"),
            enabled("ruleSets"),
        ],
        proxy_features: proxy_feature_items(&active_capabilities, missing_active_reason),
        active_proxy_config_id: active.map(|profile| profile.id.clone()),
        active_proxy_config_capabilities: active_capabilities,
    })
}

fn proxy_feature_items(
    capabilities: &ProxyConfigCapabilities,
    missing_active_reason: Option<String>,
) -> Vec<CapabilityItem> {
    vec![
        feature(
            "routing",
            capabilities.has_route_rules,
            &missing_active_reason,
        ),
        feature(
            "ruleSets",
            capabilities.has_rule_sets,
            &missing_active_reason,
        ),
        feature(
            "selector",
            capabilities.has_selector,
            &missing_active_reason,
        ),
        feature("urlTest", capabilities.has_url_test, &missing_active_reason),
    ]
}

fn enabled(key: &str) -> CapabilityItem {
    CapabilityItem {
        key: key.to_string(),
        enabled: true,
        reason: None,
    }
}

fn feature(key: &str, enabled: bool, missing_active_reason: &Option<String>) -> CapabilityItem {
    CapabilityItem {
        key: key.to_string(),
        enabled,
        reason: if enabled {
            None
        } else {
            Some(
                missing_active_reason
                    .clone()
                    .unwrap_or_else(|| format!("active proxy config does not define {key}")),
            )
        },
    }
}
