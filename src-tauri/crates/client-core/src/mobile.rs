//! Mobile runtime evidence and admission, independent of UI and native APIs.
//!
//! A control EngineHandle is insufficient: only a service/provider with a
//! running Proxy, an attached platform tunnel, and protected physical egress
//! can confirm VPN takeover. Missing observations remain unknown.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeIdentity {
    pub core_instance_id: String,
    pub profile_id: String,
    pub config_revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum KernelState {
    Unavailable,
    Starting,
    Ready,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VpnState {
    Unknown,
    NotAuthorized,
    Off,
    Starting,
    TakenOver,
    Recovering,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeEvidence {
    pub identity: RuntimeIdentity,
    pub proxy_running: bool,
    pub external_tun_attached: bool,
    pub protected_egress: bool,
    pub dns_confirmed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlatformEvidence {
    pub identity: RuntimeIdentity,
    pub authorized: bool,
    pub tunnel_established: bool,
    pub physical_network_available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciledRuntime {
    pub kernel_state: KernelState,
    pub vpn_state: VpnState,
    pub identity: Option<RuntimeIdentity>,
}

pub fn reconcile(
    runtime: Option<&RuntimeEvidence>,
    platform: Option<&PlatformEvidence>,
) -> ReconciledRuntime {
    let kernel_state = match runtime {
        Some(evidence) if evidence.proxy_running => KernelState::Ready,
        Some(_) => KernelState::Failed,
        None => KernelState::Unavailable,
    };
    let matching = runtime.zip(platform).filter(|(runtime, platform)| {
        runtime.identity == platform.identity
            && !runtime.identity.core_instance_id.is_empty()
            && !runtime.identity.profile_id.is_empty()
            && !runtime.identity.config_revision.is_empty()
    });
    let vpn_state = match platform {
        None => VpnState::Unknown,
        Some(evidence) if !evidence.authorized => VpnState::NotAuthorized,
        Some(evidence) if !evidence.tunnel_established => VpnState::Off,
        Some(_) => match matching {
            Some((runtime, platform))
                if runtime.proxy_running
                    && runtime.external_tun_attached
                    && runtime.protected_egress
                    && runtime.dns_confirmed
                    && platform.physical_network_available =>
            {
                VpnState::TakenOver
            }
            Some(_) => VpnState::Recovering,
            None => VpnState::Unknown,
        },
    };
    ReconciledRuntime {
        kernel_state,
        vpn_state,
        identity: matching.map(|(runtime, _)| runtime.identity.clone()),
    }
}

/// Automatic recovery is scoped to a user intent and the observed instance.
/// An active stop/revocation supersedes every previously queued recovery.
#[derive(Clone, Debug, Default)]
pub struct RecoveryIntent {
    revision: u64,
    enabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecoveryTicket {
    revision: u64,
    identity: RuntimeIdentity,
}

impl RecoveryIntent {
    pub fn set_enabled(&mut self, enabled: bool) {
        // Exhaustion permanently disables recovery rather than reusing tokens.
        match self.revision.checked_add(1) {
            Some(revision) => {
                self.revision = revision;
                self.enabled = enabled;
            }
            None => self.enabled = false,
        }
    }

    pub fn ticket(&self, identity: RuntimeIdentity) -> Option<RecoveryTicket> {
        self.enabled.then_some(RecoveryTicket {
            revision: self.revision,
            identity,
        })
    }

    pub fn accepts(&self, ticket: &RecoveryTicket, current: &RuntimeIdentity) -> bool {
        self.enabled && self.revision == ticket.revision && &ticket.identity == current
    }
}

/// Mobile adapters must never fall through to a desktop process, proxy,
/// arbitrary executable, filesystem-path authorization, or raw IPC transport.
/// Local profile editing and offline reads stay independently available.
pub fn requires_desktop_host(command: &str) -> bool {
    command.starts_with("core_ipc_")
        || command.starts_with("core_process_")
        || command.starts_with("core_get_")
        || command.starts_with("core_events_")
        || command.starts_with("gui_traffic_")
        || command.starts_with("gui_endpoint_")
        || command.starts_with("gui_packet_route")
        || command.starts_with("gui_events_")
        || command.starts_with("system_proxy_")
        || command.starts_with("tray_")
        || command.starts_with("app_install_")
        || command.starts_with("app_download_")
        || command == "app_check_release"
        || matches!(
            command,
            "core_status"
                | "core_select_policy"
                | "core_probe_policy"
                | "core_close_flow"
                | "core_validate_config"
                | "gui_core_overview"
                | "gui_core_health"
                | "gui_network_probe"
                | "gui_self_test_snapshot"
                | "gui_zero_capabilities"
                | "gui_policy_groups"
                | "gui_select_policy"
                | "gui_set_mode"
                | "gui_proxy_mode_status"
                | "gui_set_proxy_mode"
                | "gui_observation_snapshot"
                | "gui_connections"
                | "gui_connection_detail"
                | "gui_recent_connections"
                | "gui_close_connection"
                | "gui_close_packet_route"
                | "gui_dns_status"
                | "gui_tun_status"
                | "gui_stack_status"
                | "gui_rule_status"
                | "gui_validate_config"
                | "gui_validate_dns_config"
                | "gui_config_workspace_plan"
                | "gui_config_workspace_apply"
                | "gui_dns_lookup"
                | "gui_dns_cache"
                | "gui_fakeip_lookup"
                | "gui_clear_fake_ip"
                | "gui_trace_route"
                | "gui_sinks"
                | "gui_diagnostics"
                | "gui_probe_job_start"
                | "gui_tool_job_start"
                | "gui_connection_status"
                | "gui_connect"
                | "gui_disconnect"
                | "gui_tun_enable"
                | "gui_tun_disable"
                | "gui_tun_recover"
                | "core_download_latest"
                | "kernel_install_version"
                | "kernel_detect_version"
                | "platform_open_path"
                | "platform_reveal_path"
                // Mobile guests use reviewed document channels, not raw
                // component IDs, desktop paths or unscoped storage commands.
                | "plugins_invoke"
                | "plugins_sdk_call"
                | "plugins_protected_load"
                | "plugins_page_protected_load"
                | "plugins_preview_install"
                | "plugins_install"
                | "plugins_uninstall"
                | "plugins_storage_get"
                | "plugins_storage_put"
                | "plugins_storage_delete"
        )
}

/// Desktop capture effects are optional steps of a local publication, not
/// platform VPN success. Native mobile skips them and explicitly returns None.
pub fn desktop_profile_effect<T, E>(
    native_mobile: bool,
    effect: impl FnOnce() -> Result<T, E>,
) -> Result<Option<T>, E> {
    if native_mobile {
        Ok(None)
    } else {
        effect().map(Some)
    }
}
