use tauri::State;

use crate::errors::AppResult;
use crate::models::{
    capability::{
        CapabilityItem, GuiCapabilitySnapshot, InteractionSurfaceItem, InteractionSurfaceSnapshot,
    },
    proxy_config::ProxyConfigCapabilities,
};
use crate::services::common::{self, lock};
use crate::services::interaction_mode;
use crate::state::app_state::AppState;

/// How long to keep zero_features cached before re-querying the core.
const ZERO_FEATURES_CACHE_TTL_MS: u64 = 30_000;

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
    let start = std::time::Instant::now();

    let ui_mode = interaction_mode::current_ui_mode(state.inner())?;
    let is_pro = interaction_mode::is_pro_mode(&ui_mode);
    let zero_features = cached_or_query_zero_features(state.inner()).await;
    let ui = lock(state.app_config(), "app_config")?.ui.clone();
    let (has_active_config, has_endpoints) = {
        let profiles = lock(state.proxy_configs(), "proxy_config")?;
        let active = profiles.iter().find(|profile| profile.active);
        (
            active.is_some(),
            active
                .and_then(|profile| profile.content.as_ref())
                .is_some_and(has_declared_endpoints),
        )
    };

    crate::services::logs::znet_log(
        Some(state.inner()),
        crate::models::logs::LogLevel::Debug,
        format!(
            "interaction_surface: mode={ui_mode}, {} features, took {:?}",
            zero_features.len(),
            start.elapsed(),
        ),
    );

    Ok(InteractionSurfaceSnapshot {
        ui_mode,
        navigation: navigation_items(
            is_pro,
            &ui.hidden_menu_keys,
            has_active_config,
            endpoints_menu_visible(&ui, has_endpoints),
        ),
        actions: action_items(is_pro, &zero_features),
        features: feature_surface_items(is_pro, &zero_features),
    })
}

/// Return cached zero_features if fresh enough, otherwise query the core and update the cache.
///
/// Skips the core IPC entirely when the core process is not running.
async fn cached_or_query_zero_features(state: &AppState) -> Vec<String> {
    let now = common::now_unix_ms();

    // Check cache
    if let Ok(cache) = lock(state.zero_features_cache(), "zero_features_cache") {
        if let Some(ref cached) = *cache {
            if now.saturating_sub(cached.cached_at_unix_ms) < ZERO_FEATURES_CACHE_TTL_MS {
                return cached.features.clone();
            }
        }
    }

    // Try-lock: if the core_process lock is held (kernel starting/stopping),
    // skip IPC immediately instead of blocking the UI thread waiting for the
    // lock.  Fall back to cached or empty features — the interaction surface
    // will be refreshed once the kernel is running and the cache expires.
    let core_running = state
        .runtime_host()
        .try_process_status()
        .map(|process| process.state == crate::models::core_process::CoreProcessState::Running)
        .unwrap_or(false);

    if !core_running {
        crate::services::logs::znet_log(
            Some(state),
            crate::models::logs::LogLevel::Debug,
            "zero_features: core not running (or lock held), skipping IPC query",
        );
        if let Ok(cache) = lock(state.zero_features_cache(), "zero_features_cache") {
            if let Some(ref cached) = *cache {
                return cached.features.clone();
            }
        }
        return Vec::new();
    }

    // Cache miss + core running — query core
    let query_start = std::time::Instant::now();
    let features = {
        let opts = crate::services::core_config::ipc_options_from_app_config(
            &lock(state.app_config(), "app_config")
                .map(|c| c.core.clone())
                .unwrap_or_default(),
        );
        crate::kernel::zero::queries::capability_feature_keys(Some(opts))
    }
    .await
    .unwrap_or_default();
    crate::services::logs::znet_log(
        Some(state),
        crate::models::logs::LogLevel::Debug,
        format!(
            "zero_features cache miss: core query took {:?}, got {} features",
            query_start.elapsed(),
            features.len(),
        ),
    );

    // Update cache
    if let Ok(mut cache) = lock(state.zero_features_cache(), "zero_features_cache") {
        *cache = Some(crate::state::app_state::ZeroFeaturesCache {
            features: features.clone(),
            cached_at_unix_ms: now,
        });
    }

    features
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

fn endpoints_menu_visible(
    ui: &crate::models::app_config::AppUiConfig,
    has_endpoints: bool,
) -> bool {
    ui.endpoints_menu_visible.unwrap_or(has_endpoints)
}

// Detect declarations without relying on a running or reachable kernel.
fn has_declared_endpoints(source: &serde_json::Value) -> bool {
    source["endpoints"].as_array().is_some_and(|rows| {
        rows.iter().any(|row| {
            row["tag"]
                .as_str()
                .is_some_and(|tag| !tag.trim().is_empty())
                && row
                    .pointer("/protocol/type")
                    .and_then(serde_json::Value::as_str)
                    .is_some()
        })
    }) || ["inbounds", "outbounds"].iter().any(|key| {
        source[*key].as_array().is_some_and(|rows| {
            rows.iter().any(|row| {
                row.pointer("/protocol/type")
                    .is_some_and(|protocol| protocol == "wireguard")
            })
        })
    })
}

fn navigation_items(
    is_pro: bool,
    hidden_menu_keys: &[String],
    has_active_config: bool,
    endpoints_visible: bool,
) -> Vec<InteractionSurfaceItem> {
    let mut items = vec![
        shared("overview", "navigation"),
        // 节点菜单：有活跃配置文件时才可见。Profiles（配置）选项卡在 Lite 和 Pro 模式均可访问，
        // 用于选择/激活配置 → 激活后节点菜单自动显示。
        has_config("nodes", "navigation", has_active_config),
        pro_only("endpoints", "navigation", is_pro),
        pro_only("profiles", "navigation", is_pro),
        shared("subscriptions", "navigation"),
        pro_only("rules", "navigation", is_pro),
        pro_only("connections", "navigation", is_pro),
        shared("logs", "navigation"),
        shared("plugins", "navigation"),
        shared("settings", "navigation"),
        // Debug tab: Pro-only — exposes low-level IPC/state inspection
        // that lite-mode users shouldn't see. ui.hiddenMenuKeys can still
        // hide it per-user in pro mode.
        pro_only("debug", "navigation", is_pro),
    ];

    for item in &mut items {
        if item.key == "endpoints" && is_pro && !endpoints_visible {
            item.visible = false;
            item.operable = false;
            item.readonly = true;
            item.reason =
                Some("hidden by endpoint menu preference or active configuration".to_string());
        }
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
            &["query", "runtime_snapshot"],
        ),
        feature_required(
            "tun.status",
            "action",
            is_pro,
            zero_features,
            &["tun", "tun-status", "tun-snapshot"],
        ),
        feature_required(
            "stack.status",
            "action",
            is_pro,
            zero_features,
            &["system-stack", "stack", "stack-status"],
        ),
        feature_required(
            "traffic.snapshot",
            "action",
            true,
            zero_features,
            &["query", "runtime_snapshot"],
        ),
        shared("systemProxy.status", "action"),
        shared("systemProxy.enable", "action"),
        shared("systemProxy.disable", "action"),
        shared("subscriptions.list", "action"),
        shared("subscriptions.sync", "action"),
        // Lite-mode subscriptions need read-only profile metadata to display
        // and preserve their target association. Mutating profiles stays Pro-only.
        shared("proxyConfig.list", "action"),
        shared("proxyConfig.get", "action"),
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
        #[cfg(feature = "tool-node-probe")]
        pro_only("core.policy.probe", "action", is_pro),
        feature_required(
            "core.connections.list",
            "action",
            is_pro,
            zero_features,
            &["flow_snapshot"],
        ),
        feature_required(
            "core.connections.detail",
            "action",
            is_pro,
            zero_features,
            &["flow_snapshot"],
        ),
        feature_required(
            "core.flow.close",
            "action",
            is_pro,
            zero_features,
            &["flow_snapshot"],
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
            &["query", "runtime_snapshot"],
        ),
        feature_required(
            "policySelection",
            "feature",
            true,
            zero_features,
            &["policy_snapshot"],
        ),
        pro_only("proxyConfigManagement", "feature", is_pro),
        pro_only("routing", "feature", is_pro),
        pro_only("ruleSets", "feature", is_pro),
        feature_required(
            "connections",
            "feature",
            is_pro,
            zero_features,
            &["flow_snapshot"],
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
            "systemStack",
            "feature",
            is_pro,
            zero_features,
            &["system-stack", "stack", "stack-status"],
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

fn has_config(key: &str, category: &str, active: bool) -> InteractionSurfaceItem {
    InteractionSurfaceItem {
        key: key.to_string(),
        category: category.to_string(),
        visible: active,
        operable: active,
        readonly: false,
        reason: (!active).then(|| "no active proxy config".to_string()),
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

#[cfg(test)]
mod tests {
    use super::{
        action_items, endpoints_menu_visible, feature_surface_items, has_declared_endpoints,
        navigation_items,
    };

    #[test]
    fn canonical_zero_snapshot_features_enable_gui_surfaces() {
        let features = vec![
            "runtime_snapshot".to_string(),
            "flow_snapshot".to_string(),
            "policy_snapshot".to_string(),
        ];

        let actions = action_items(true, &features);
        for key in [
            "core.stats",
            "traffic.snapshot",
            "core.connections.list",
            "core.connections.detail",
            "core.flow.close",
        ] {
            assert!(
                actions.iter().any(|item| item.key == key && item.operable),
                "{key} should be enabled by the canonical Zero feature names"
            );
        }

        let surfaces = feature_surface_items(true, &features);
        for key in ["traffic", "policySelection", "connections"] {
            assert!(
                surfaces.iter().any(|item| item.key == key && item.operable),
                "{key} should be enabled by the canonical Zero feature names"
            );
        }
    }
    #[test]
    fn endpoints_navigation_is_independent_pro_only_and_respects_visibility() {
        for has_config in [false, true] {
            for pro in [false, true] {
                let items = navigation_items(pro, &[], has_config, true);
                let endpoint = items.iter().find(|item| item.key == "endpoints").unwrap();
                assert_eq!(endpoint.visible, pro);
                assert_eq!(endpoint.operable, pro);
            }
        }
        let hidden = navigation_items(true, &["ENDPOINTS".into()], true, true);
        let endpoint = hidden.iter().find(|item| item.key == "endpoints").unwrap();
        assert!(!endpoint.visible && !endpoint.operable && endpoint.readonly);
    }

    #[test]
    fn endpoints_navigation_defaults_to_declarations_and_preserves_explicit_choices() {
        let mut ui = crate::models::app_config::AppUiConfig::default();
        for preference in [None, Some(false), Some(true)] {
            ui.endpoints_menu_visible = preference;
            for declared in [false, true] {
                for pro in [false, true] {
                    let items = navigation_items(
                        pro,
                        &ui.hidden_menu_keys,
                        true,
                        endpoints_menu_visible(&ui, declared),
                    );
                    let endpoint = items.iter().find(|item| item.key == "endpoints").unwrap();
                    assert_eq!(endpoint.visible, pro && preference.unwrap_or(declared));
                    assert_eq!(endpoint.operable, endpoint.visible);
                }
            }
        }
        // Legacy explicit hides remain authoritative even with declarations.
        ui.hidden_menu_keys.push("ENDPOINTS".into());
        for preference in [None, Some(false), Some(true)] {
            ui.endpoints_menu_visible = preference;
            let items = navigation_items(
                true,
                &ui.hidden_menu_keys,
                true,
                endpoints_menu_visible(&ui, true),
            );
            assert!(
                !items
                    .iter()
                    .find(|item| item.key == "endpoints")
                    .unwrap()
                    .visible
            );
        }
    }

    #[test]
    fn endpoint_menu_detection_includes_generic_and_disabled_declarations() {
        use serde_json::json;
        for source in [
            json!({"endpoints":[{"tag":"mesh", "enabled":false, "protocol":{"type":"future_mesh"}}]}),
            json!({"inbounds":[{"tag":"wg", "protocol":{"type":"wireguard"}}]}),
            json!({"outbounds":[{"tag":"wg", "protocol":{"type":"wireguard"}}]}),
        ] {
            assert!(has_declared_endpoints(&source));
        }
        for source in [
            json!({}),
            json!({"endpoints":[]}),
            json!({"endpoints":[null]}),
            json!({"outbounds":[{"protocol":{"type":"shadowsocks"}}]}),
        ] {
            assert!(!has_declared_endpoints(&source));
        }
    }

    #[test]
    fn plugins_navigation_is_shared_and_respects_menu_visibility() {
        for pro in [false, true] {
            let items = navigation_items(pro, &[], false, false);
            let plugin = items.iter().find(|item| item.key == "plugins").unwrap();
            assert!(plugin.visible && plugin.operable);
            let hidden = navigation_items(pro, &["plugins".into()], false, false);
            let plugin = hidden.iter().find(|item| item.key == "plugins").unwrap();
            assert!(!plugin.visible && !plugin.operable);
        }
    }
}
