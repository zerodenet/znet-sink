//! Kernel adapter trait.
//!
//! Defines the interface every kernel adapter must implement.
//! Each adapter maps the kernel's native IPC protocol into a
//! unified set of operations consumed by the GUI service layer.
//!
//! Future kernels (sing-box, clash, etc.) implement this trait
//! in their own sub-module under `kernel/`.

use crate::errors::AppResult;
use crate::models::core::CoreIpcOptions;
use crate::models::gui_core::{
    GuiConnection, GuiConnectionCloseResult, GuiConnectionList, GuiConnectionListOptions,
    GuiCoreHealth, GuiFeatureStatus, GuiPolicyGroup, GuiPolicySelectionResult,
    GuiTargetProbeResult, GuiTrafficStats, GuiZeroCapabilities, ConfigProxyNode,
};

use serde_json::Value;

/// Kernel adapter trait — the uniform interface consumed by GUI services.
///
/// Methods are grouped into:
/// - **Health & capabilities** — `health`, `capabilities`, `readiness_health`
/// - **Traffic** — `traffic_stats`
/// - **Policies** — `policy_groups`, `select_policy`, `probe_target`
/// - **Connections** — `connections`, `connection_detail`, `close_connection`
/// - **Features** — `tun_status`, `enable_tun`, `disable_tun`, etc.
/// - **Config** — `proxy_nodes_from_config`, `policy_groups_from_config`
///
/// The adapter receives `CoreIpcOptions` (resolved externally from `AppState`)
/// so it never depends on GUI state directly.
///
/// **Note on `traffic_snapshot`**: Snapshot building requires reading/writing
/// `AppState` for rate calculation, so it is NOT on this trait. The service
/// layer calls `traffic_stats` and then uses `zero::build_traffic_snapshot`
/// to compute rates from the previous sample in `AppState`.
#[allow(async_fn_in_trait)]
pub trait KernelAdapter {
    // ── Health & capabilities ───────────────────────────────────

    /// Detailed health info (version, uptime, etc.).
    async fn health(&self, options: CoreIpcOptions) -> AppResult<GuiCoreHealth>;

    /// Fast liveness check — ping + optional health enrichment.
    async fn readiness_health(&self, options: CoreIpcOptions) -> AppResult<GuiCoreHealth>;

    /// Kernel capability surface (features, adapters, sinks, permissions).
    async fn capabilities(&self, options: CoreIpcOptions) -> AppResult<GuiZeroCapabilities>;

    // ── Traffic ─────────────────────────────────────────────────

    /// Current traffic counters.
    async fn traffic_stats(&self, options: CoreIpcOptions) -> AppResult<GuiTrafficStats>;

    // ── Policies ────────────────────────────────────────────────

    /// All policy groups with their members and current selection.
    async fn policy_groups(&self, options: CoreIpcOptions) -> AppResult<Vec<GuiPolicyGroup>>;

    /// Switch the selected outbound in a policy group.
    async fn select_policy(
        &self,
        policy_tag: String,
        target_tag: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiPolicySelectionResult>;

    /// Probe a single target for reachability and latency.
    async fn probe_target(
        &self,
        target_tag: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiTargetProbeResult>;

    // ── Connections ─────────────────────────────────────────────

    /// Active connections / flows.
    async fn connections(
        &self,
        options: Option<GuiConnectionListOptions>,
        ipc_options: CoreIpcOptions,
    ) -> AppResult<GuiConnectionList>;

    /// Single connection detail.
    async fn connection_detail(
        &self,
        flow_id: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiConnection>;

    /// Close a connection.
    async fn close_connection(
        &self,
        flow_id: String,
        options: CoreIpcOptions,
    ) -> AppResult<GuiConnectionCloseResult>;

    // ── Feature status ──────────────────────────────────────────

    /// DNS subsystem status (pro-only).
    async fn dns_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus>;

    /// TUN virtual network interface status.
    async fn tun_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus>;

    /// Enable TUN.
    async fn enable_tun(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus>;

    /// Disable TUN.
    async fn disable_tun(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus>;

    /// Network stack status (pro-only).
    async fn stack_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus>;

    /// Rule engine status (pro-only).
    async fn rule_status(&self, options: CoreIpcOptions) -> AppResult<GuiFeatureStatus>;

    // ── Static config (no kernel running required) ──────────────

    /// Extract proxy nodes from the active config file content.
    /// Works even when the kernel is not running.
    fn proxy_nodes_from_config(&self, config_content: &Value) -> AppResult<Vec<ConfigProxyNode>>;

    /// Extract policy groups from the active config file content.
    /// Works even when the kernel is not running.
    fn policy_groups_from_config(
        &self,
        config_content: &Value,
    ) -> AppResult<Vec<GuiPolicyGroup>>;
}
