//! Zero kernel adapter — `KernelAdapter` implementation.
//!
//! Orchestrates queries and commands into the unified trait interface
//! consumed by GUI services. Also provides the high-level `core_overview`
//! composite query and traffic snapshot rate calculation.

use serde_json::Value;

use crate::errors::AppResult;
use crate::kernel::adapter::KernelAdapter;
use crate::models::core::CoreIpcOptions;
use crate::models::gui_core::{
    ConfigProxyNode, GuiConfigPlanApplyResult, GuiConnection, GuiConnectionCloseResult,
    GuiConnectionList, GuiConnectionListOptions, GuiCoreHealth, GuiFeatureStatus, GuiPolicyGroup,
    GuiPolicySelectionResult, GuiTargetProbeResult, GuiTrafficRates, GuiTrafficSnapshot,
    GuiTrafficStats, GuiZeroCapabilities,
};

use super::{commands, config, queries};

/// Stateless adapter for the Zero self-developed kernel.
///
/// All methods receive `CoreIpcOptions` resolved externally (typically
/// from `AppState`). The adapter never touches GUI state directly.
pub struct ZeroAdapter;

impl ZeroAdapter {
    pub const fn new() -> Self {
        Self
    }

    /// Build the composite overview used by the dashboard.
    ///
    /// Does a fast health ping first.  If the kernel is unreachable the
    /// remaining queries (capabilities, config, stats, policy groups) are
    /// skipped entirely so the caller doesn't block for 15 s on sequential
    /// timeouts.
    pub async fn core_overview(
        &self,
        process_running: bool,
        options: CoreIpcOptions,
    ) -> CoreOverviewResult {
        // ── Fast health check — bail out early if kernel is down ──
        let health = match self.readiness_health(options.clone()).await {
            Ok(health) => Some(health),
            Err(error) => {
                // Kernel unreachable — no point querying the rest
                return CoreOverviewResult {
                    process_running,
                    available: false,
                    health: None,
                    config: None,
                    stats: GuiTrafficStats::default(),
                    policy_groups: Vec::new(),
                    capabilities: GuiZeroCapabilities {
                        available: false,
                        error: Some(error.message.clone()),
                        ..GuiZeroCapabilities::default()
                    },
                    last_error: Some(error.message),
                };
            }
        };

        // ── Kernel is reachable — gather remaining data ──
        let capabilities = self.capabilities(options.clone()).await;
        let mut last_error = capabilities.as_ref().err().map(|e| e.message.clone());
        let capabilities = capabilities.unwrap_or_else(|error| GuiZeroCapabilities {
            available: false,
            error: Some(error.message),
            ..GuiZeroCapabilities::default()
        });

        let config = match self.query_config(options.clone()).await {
            Ok(config) => Some(config),
            Err(error) => {
                last_error.get_or_insert(error.message);
                None
            }
        };
        let stats = match self.traffic_stats(options.clone()).await {
            Ok(stats) => stats,
            Err(error) => {
                last_error.get_or_insert(error.message);
                GuiTrafficStats::default()
            }
        };
        let policy_groups = match self.policy_groups(options).await {
            Ok(groups) => groups,
            Err(error) => {
                last_error.get_or_insert(error.message);
                Vec::new()
            }
        };

        let available = capabilities.available || health.as_ref().is_some_and(|h| h.healthy);

        CoreOverviewResult {
            process_running,
            available,
            health,
            config,
            stats,
            policy_groups,
            capabilities,
            last_error,
        }
    }
}

/// Result of the composite `core_overview` query.
#[derive(Debug)]
pub struct CoreOverviewResult {
    pub process_running: bool,
    pub available: bool,
    pub health: Option<GuiCoreHealth>,
    pub config: Option<serde_json::Value>,
    pub stats: GuiTrafficStats,
    pub policy_groups: Vec<GuiPolicyGroup>,
    pub capabilities: GuiZeroCapabilities,
    pub last_error: Option<String>,
}

// ── KernelAdapter implementation ────────────────────────────────────

impl KernelAdapter for ZeroAdapter {
    async fn health(&self, options: CoreIpcOptions) -> AppResult<GuiCoreHealth> {
        queries::core_health(Some(options)).await
    }

    async fn readiness_health(&self, options: CoreIpcOptions) -> AppResult<GuiCoreHealth> {
        queries::core_readiness_health(Some(options)).await
    }

    async fn capabilities(&self, options: CoreIpcOptions) -> AppResult<GuiZeroCapabilities> {
        queries::zero_capabilities(Some(options)).await
    }

    async fn traffic_stats(&self, options: CoreIpcOptions) -> AppResult<GuiTrafficStats> {
        queries::traffic_stats(Some(options)).await
    }

    async fn policy_groups(&self, options: CoreIpcOptions) -> AppResult<Vec<GuiPolicyGroup>> {
        queries::policy_groups(Some(options)).await
    }

    async fn select_policy(
        &self,
        policy_tag: String,
        target_tag: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiPolicySelectionResult> {
        commands::select_policy(policy_tag, target_tag, Some(options)).await
    }

    async fn probe_target(
        &self,
        target_tag: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiTargetProbeResult> {
        commands::probe_target(target_tag, Some(options)).await
    }

    async fn probe_policy(&self, policy_tag: String, options: CoreIpcOptions) -> AppResult<Value> {
        commands::probe_policy(policy_tag, Some(options)).await
    }

    async fn connections(
        &self,
        list_options: Option<GuiConnectionListOptions>,
        ipc_options: CoreIpcOptions,
    ) -> AppResult<GuiConnectionList> {
        queries::connections(list_options, Some(ipc_options)).await
    }

    async fn connection_detail(
        &self,
        flow_id: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiConnection> {
        queries::connection_detail(flow_id, Some(options)).await
    }

    async fn close_connection(
        &self,
        flow_id: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiConnectionCloseResult> {
        commands::close_connection(flow_id, Some(options)).await
    }

    async fn recent_connections(
        &self,
        list_options: Option<GuiConnectionListOptions>,
        ipc_options: CoreIpcOptions,
    ) -> AppResult<GuiConnectionList> {
        queries::recent_connections(list_options, Some(ipc_options)).await
    }

    async fn apply_config(&self, config: Value, options: CoreIpcOptions) -> AppResult<Value> {
        commands::apply_config(config, Some(options)).await
    }

    async fn validate_config(&self, config: Value, options: CoreIpcOptions) -> AppResult<Value> {
        commands::validate_config(config, Some(options)).await
    }

    async fn plan_apply_config(
        &self,
        config: Value,
        options: CoreIpcOptions,
    ) -> AppResult<GuiConfigPlanApplyResult> {
        commands::plan_apply_config(config, Some(options)).await
    }

    async fn set_mode(
        &self,
        mode: String,
        outbound: Option<String>,
        options: CoreIpcOptions,
    ) -> AppResult<Value> {
        commands::set_mode(mode, outbound, Some(options)).await
    }

    async fn dns_lookup(&self, hostname: String, options: CoreIpcOptions) -> AppResult<Value> {
        commands::dns_lookup(hostname, Some(options)).await
    }

    async fn trace_route(
        &self,
        target: String,
        port: u16,
        protocol: Option<String>,
        options: CoreIpcOptions,
    ) -> AppResult<Value> {
        commands::trace_route(target, port, protocol, Some(options)).await
    }

    async fn sinks(&self, options: CoreIpcOptions) -> AppResult<Value> {
        queries::sinks(Some(options)).await
    }

    async fn diagnostics(&self, options: CoreIpcOptions) -> AppResult<Value> {
        queries::diagnostics(Some(options)).await
    }

    async fn dns_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus> {
        queries::dns_status(Some(options)).await
    }

    async fn tun_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus> {
        queries::tun_status(Some(options)).await
    }

    async fn enable_tun(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus> {
        // Note: TUN params come from app config, but the adapter doesn't
        // know about AppState. The caller extracts params and uses
        // `commands::enable_tun` directly when TUN config is needed.
        // This trait method uses sensible defaults.
        commands::enable_tun(
            None,
            "10.0.0.1".to_string(),
            "tun-in".to_string(),
            1500,
            Some(options),
        )
        .await
    }

    async fn disable_tun(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus> {
        commands::disable_tun(Some(options)).await
    }

    async fn stack_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus> {
        queries::stack_status(Some(options)).await
    }

    async fn rule_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus> {
        queries::rule_status(Some(options)).await
    }

    fn proxy_nodes_from_config(&self, config_content: &Value) -> AppResult<Vec<ConfigProxyNode>> {
        Ok(config::proxy_nodes_from_config(config_content))
    }

    fn policy_groups_from_config(&self, config_content: &Value) -> AppResult<Vec<GuiPolicyGroup>> {
        Ok(config::policy_groups_from_config(config_content))
    }
}

impl ZeroAdapter {
    /// Query the kernel's current config snapshot (not on trait — used by core_overview).
    pub async fn query_config(&self, options: CoreIpcOptions) -> AppResult<Value> {
        queries::query_config(Some(options)).await
    }
}

// ── Traffic snapshot utilities ───────────────────────────────────────
//
// These are NOT on the trait because they read/write AppState.
// The service layer calls them after getting stats from the adapter.

pub const MIN_TRAFFIC_SAMPLE_INTERVAL_MS: u64 = 500;

/// Previous traffic sample for rate calculation.
#[derive(Clone, Debug)]
pub struct TrafficSample {
    pub stats: GuiTrafficStats,
    pub sampled_at_unix_ms: u64,
}

/// Build a traffic snapshot with rate calculation from a previous sample.
pub fn build_traffic_snapshot(
    totals: GuiTrafficStats,
    previous: Option<&TrafficSample>,
    sampled_at_unix_ms: u64,
) -> GuiTrafficSnapshot {
    let mut interval_ms = previous
        .map(|prev| sampled_at_unix_ms.saturating_sub(prev.sampled_at_unix_ms))
        .filter(|interval| *interval > 0);
    let mut stable = true;
    let mut reason = None;
    let mut rates = GuiTrafficRates::default();

    match previous {
        Some(prev) if interval_ms.unwrap_or(0) >= MIN_TRAFFIC_SAMPLE_INTERVAL_MS => {
            let interval = interval_ms.expect("interval is checked above");
            rates = calculate_rates(&prev.stats, &totals, interval);
        }
        Some(_) => {
            stable = false;
            reason = Some("sample interval is too short for a stable rate".to_string());
        }
        None => {
            stable = false;
            interval_ms = None;
            reason = Some("first sample has no previous traffic baseline".to_string());
        }
    }

    GuiTrafficSnapshot {
        totals,
        rates,
        sampled_at_unix_ms,
        interval_ms,
        source: "zero-stats".to_string(),
        stable,
        reason,
    }
}

pub fn calculate_rates(
    previous: &GuiTrafficStats,
    current: &GuiTrafficStats,
    interval_ms: u64,
) -> GuiTrafficRates {
    if interval_ms == 0 {
        return GuiTrafficRates::default();
    }

    GuiTrafficRates {
        upload_bps: bytes_delta_per_second(previous.bytes_up, current.bytes_up, interval_ms),
        download_bps: bytes_delta_per_second(previous.bytes_down, current.bytes_down, interval_ms),
    }
}

pub fn bytes_delta_per_second(previous: u64, current: u64, interval_ms: u64) -> u64 {
    if current < previous || interval_ms == 0 {
        return 0;
    }

    let delta = u128::from(current - previous);
    let rate = delta.saturating_mul(1000) / u128::from(interval_ms);
    rate.min(u128::from(u64::MAX)) as u64
}
