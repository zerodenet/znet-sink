use tauri::State;

use crate::errors::AppResult;
use crate::models::{
    capability::{
        CapabilityItem, GuiCapabilitySnapshot, InteractionSurfaceItem, InteractionSurfaceSnapshot,
    },
    proxy_config::ProxyConfigCapabilities,
};
use crate::services::common::lock;
use crate::services::interaction_mode;
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

pub async fn interaction_surface(
    state: State<'_, AppState>,
) -> AppResult<InteractionSurfaceSnapshot> {
    let ui_mode = interaction_mode::current_ui_mode(state.inner())?;
    let is_pro = interaction_mode::is_pro_mode(&ui_mode);
    let zero_features = crate::services::zero_adapter::capability_feature_keys(state.inner())
        .await
        .unwrap_or_default();
    let hidden_menu_keys = lock(state.app_config(), "app_config")?
        .ui
        .hidden_menu_keys
        .clone();

    Ok(InteractionSurfaceSnapshot {
        ui_mode,
        navigation: navigation_items(is_pro, &hidden_menu_keys),
        actions: action_items(is_pro, &zero_features),
        features: feature_surface_items(is_pro, &zero_features),
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

fn navigation_items(is_pro: bool, hidden_menu_keys: &[String]) -> Vec<InteractionSurfaceItem> {
    let mut items = vec![
        shared("overview", "navigation"),
        pro_only("profiles", "navigation", is_pro),
        shared("subscriptions", "navigation"),
        pro_only("rules", "navigation", is_pro),
        pro_only("connections", "navigation", is_pro),
        shared("logs", "navigation"),
        shared("settings", "navigation"),
    ];

    for item in &mut items {
        if item.key != "settings"
            && hidden_menu_keys
                .iter()
                .any(|key| key.eq_ignore_ascii_case(&item.key))
        {
            item.visible = false;
            item.operable = false;
            item.readonly = true;
            item.reason = Some("hidden by ui.hiddenMenuKeys".to_string());
        }
    }

    items
}

fn action_items(is_pro: bool, zero_features: &[String]) -> Vec<InteractionSurfaceItem> {
    vec![
        shared("core.process.status", "action"),
        shared("core.process.start", "action"),
        shared("core.process.stop", "action"),
        shared("core.overview", "action"),
        shared("core.health", "action"),
        shared("selfTest.snapshot", "action"),
        feature_required(
            "core.capabilities",
            "action",
            is_pro,
            zero_features,
            &["query"],
        ),
        feature_required(
            "core.stats",
            "action",
            true,
            zero_features,
            &["query", "runtime-snapshot"],
        ),
        feature_required(
            "traffic.snapshot",
            "action",
            true,
            zero_features,
            &["query", "runtime-snapshot"],
        ),
        shared("systemProxy.status", "action"),
        shared("systemProxy.enable", "action"),
        shared("systemProxy.disable", "action"),
        shared("subscriptions.list", "action"),
        shared("subscriptions.sync", "action"),
        shared("proxyMode.status", "action"),
        shared("proxyMode.set", "action"),
        shared("policies.list", "action"),
        shared("policies.select", "action"),
        pro_only("proxyConfig.import", "action", is_pro),
        pro_only("proxyConfig.upsert", "action", is_pro),
        pro_only("proxyConfig.remove", "action", is_pro),
        pro_only("ruleSets.list", "action", is_pro),
        pro_only("ruleSets.get", "action", is_pro),
        pro_only("ruleSets.upsert", "action", is_pro),
        pro_only("ruleSets.remove", "action", is_pro),
        pro_only("core.ipc.query", "action", is_pro),
        pro_only("core.ipc.command", "action", is_pro),
        pro_only("core.ipc.request", "action", is_pro),
        pro_only("core.config.get", "action", is_pro),
        pro_only("core.config.exportActive", "action", is_pro),
        pro_only("core.config.validate", "action", is_pro),
        pro_only("core.policy.probe", "action", is_pro),
        feature_required(
            "core.connections.list",
            "action",
            is_pro,
            zero_features,
            &["flow-snapshot"],
        ),
        feature_required(
            "core.connections.detail",
            "action",
            is_pro,
            zero_features,
            &["flow-snapshot"],
        ),
        feature_required(
            "core.flow.close",
            "action",
            is_pro,
            zero_features,
            &["flow-snapshot"],
        ),
    ]
}

fn feature_surface_items(is_pro: bool, zero_features: &[String]) -> Vec<InteractionSurfaceItem> {
    vec![
        shared("coreLifecycle", "feature"),
        shared("systemProxy", "feature"),
        shared("subscriptionSync", "feature"),
        shared("proxyMode", "feature"),
        shared("selfTest", "feature"),
        feature_required(
            "traffic",
            "feature",
            true,
            zero_features,
            &["query", "runtime-snapshot"],
        ),
        feature_required(
            "policySelection",
            "feature",
            true,
            zero_features,
            &["policy-snapshot"],
        ),
        pro_only("proxyConfigManagement", "feature", is_pro),
        pro_only("routing", "feature", is_pro),
        pro_only("ruleSets", "feature", is_pro),
        feature_required(
            "connections",
            "feature",
            is_pro,
            zero_features,
            &["flow-snapshot"],
        ),
        feature_required(
            "dns",
            "feature",
            is_pro,
            zero_features,
            &["dns", "dns-status", "dns-snapshot"],
        ),
        feature_required(
            "tun",
            "feature",
            is_pro,
            zero_features,
            &["tun", "tun-status", "tun-snapshot"],
        ),
        feature_required(
            "scripting",
            "feature",
            is_pro,
            zero_features,
            &["scripting", "script"],
        ),
        feature_required("mitm", "feature", is_pro, zero_features, &["mitm"]),
        feature_required("diagnostics", "feature", is_pro, zero_features, &["query"]),
        pro_only("rawIpc", "feature", is_pro),
    ]
}

fn shared(key: &str, category: &str) -> InteractionSurfaceItem {
    InteractionSurfaceItem {
        key: key.to_string(),
        category: category.to_string(),
        visible: true,
        operable: true,
        readonly: false,
        reason: None,
    }
}

fn pro_only(key: &str, category: &str, is_pro: bool) -> InteractionSurfaceItem {
    InteractionSurfaceItem {
        key: key.to_string(),
        category: category.to_string(),
        visible: is_pro,
        operable: is_pro,
        readonly: false,
        reason: (!is_pro).then(|| "hidden in lite mode".to_string()),
    }
}

fn feature_required(
    key: &str,
    category: &str,
    mode_allowed: bool,
    zero_features: &[String],
    required_features: &[&str],
) -> InteractionSurfaceItem {
    if !mode_allowed {
        return pro_only(key, category, false);
    }

    let supported = required_features.iter().any(|required| {
        zero_features
            .iter()
            .any(|feature| feature.eq_ignore_ascii_case(required))
    });

    InteractionSurfaceItem {
        key: key.to_string(),
        category: category.to_string(),
        visible: true,
        operable: supported,
        readonly: !supported,
        reason: (!supported).then(|| {
            format!(
                "zero capability does not declare any of: {}",
                required_features.join(", ")
            )
        }),
    }
}
