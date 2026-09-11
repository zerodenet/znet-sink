//! One registration point for the existing desktop command surface.

use crate::commands::app_config as app_config_commands;
use crate::commands::app_update as app_update_commands;
use crate::commands::capability as capability_commands;
use crate::commands::core as core_commands;
use crate::commands::core_config as core_config_commands;
use crate::commands::core_process as core_process_commands;
use crate::commands::debug as debug_commands;
use crate::commands::gui_connection as gui_connection_commands;
use crate::commands::gui_core as gui_core_commands;
use crate::commands::gui_events as gui_events_commands;
use crate::commands::gui_self_test as gui_self_test_commands;
use crate::commands::kernel_version as kernel_version_commands;
use crate::commands::logs as logs_commands;
use crate::commands::proxy_config as proxy_config_commands;
use crate::commands::proxy_mode as proxy_mode_commands;
use crate::commands::rule_set as rule_set_commands;
use crate::commands::subscription as subscription_commands;
use crate::commands::system_proxy as system_proxy_commands;

pub(super) fn register(builder: tauri::Builder<tauri::Wry>) -> tauri::Builder<tauri::Wry> {
    builder.invoke_handler(tauri::generate_handler![
        core_commands::core_ipc_default_endpoint,
        core_commands::core_status,
        core_commands::core_ipc_ping,
        core_commands::core_ipc_query,
        core_commands::core_ipc_command,
        core_commands::core_ipc_request,
        core_commands::core_get_capabilities,
        core_commands::core_get_health,
        core_commands::core_get_config,
        core_commands::core_get_runtime,
        core_commands::core_get_stats,
        core_commands::core_get_policies,
        core_commands::core_select_policy,
        #[cfg(feature = "tool-node-probe")]
        core_commands::core_probe_policy,
        core_commands::core_close_flow,
        core_commands::core_validate_config,
        core_commands::core_events_start,
        core_commands::core_events_stop,
        core_config_commands::core_config_get,
        core_process_commands::core_process_status,
        core_process_commands::core_process_start,
        core_process_commands::core_process_restart,
        core_config_commands::core_config_export_active,
        core_config_commands::core_download_latest,
        gui_core_commands::gui_core_overview,
        gui_core_commands::gui_client_core_snapshot,
        gui_core_commands::gui_node_screen_snapshot,
        #[cfg(feature = "tool-node-probe")]
        gui_core_commands::gui_probe_job_start,
        #[cfg(feature = "tool-node-probe")]
        gui_core_commands::gui_probe_runtime_snapshot,
        #[cfg(feature = "tool-node-probe")]
        gui_core_commands::gui_probe_job_get,
        #[cfg(feature = "tool-node-probe")]
        gui_core_commands::gui_probe_job_list,
        #[cfg(feature = "tool-node-probe")]
        gui_core_commands::gui_probe_job_cancel,
        gui_core_commands::gui_core_health,
        gui_core_commands::gui_zero_capabilities,
        gui_core_commands::gui_traffic_stats,
        gui_core_commands::gui_traffic_snapshot,
        gui_core_commands::gui_policy_groups,
        gui_core_commands::gui_config_policy_groups,
        gui_core_commands::gui_proxy_nodes,
        gui_core_commands::gui_select_policy,
        gui_core_commands::gui_observation_snapshot,
        gui_core_commands::gui_connections,
        gui_core_commands::gui_connection_detail,
        gui_core_commands::gui_close_connection,
        gui_core_commands::gui_dns_status,
        gui_core_commands::gui_tun_status,
        gui_core_commands::gui_tun_enable,
        gui_core_commands::gui_tun_disable,
        gui_core_commands::gui_tun_recover,
        gui_core_commands::gui_stack_status,
        gui_core_commands::gui_rule_status,
        gui_core_commands::gui_apply_config,
        // DNS/Fake-IP is a client-global override applied to the effective Zero config.
        gui_core_commands::gui_apply_dns_config,
        gui_core_commands::gui_validate_config,
        gui_core_commands::gui_validate_dns_config,
        gui_core_commands::gui_inspect_dns_effective_config,
        gui_core_commands::gui_set_mode,
        #[cfg(feature = "tool-dns")]
        gui_core_commands::gui_dns_lookup,
        #[cfg(feature = "tool-dns")]
        gui_core_commands::gui_dns_cache,
        #[cfg(feature = "tool-dns")]
        gui_core_commands::gui_fakeip_lookup,
        #[cfg(feature = "tool-dns")]
        gui_core_commands::gui_clear_fake_ip,
        #[cfg(feature = "tool-route")]
        gui_core_commands::gui_trace_route,
        gui_core_commands::gui_recent_connections,
        gui_core_commands::gui_sinks,
        gui_core_commands::gui_diagnostics,
        gui_connection_commands::gui_connection_status,
        gui_connection_commands::gui_connect,
        gui_connection_commands::gui_disconnect,
        gui_events_commands::gui_events_start,
        gui_events_commands::gui_events_stop,
        debug_commands::gui_debug_frames,
        debug_commands::gui_debug_clear,
        gui_self_test_commands::gui_self_test_snapshot,
        proxy_mode_commands::gui_proxy_mode_status,
        proxy_mode_commands::gui_set_proxy_mode,
        crate::commands::profile_settings::profile_settings_get,
        crate::commands::profile_settings::profile_settings_apply,
        app_config_commands::app_config_get,
        app_config_commands::app_config_update,
        app_config_commands::app_config_apply_tun,
        app_config_commands::app_config_export_kernel_settings,
        app_config_commands::app_config_import_kernel_settings,
        app_update_commands::app_check_release,
        app_update_commands::download::app_download_update,
        app_update_commands::download::app_install_update,
        proxy_config_commands::proxy_config_list,
        proxy_config_commands::proxy_config_composition_report,
        proxy_config_commands::proxy_config_get,
        proxy_config_commands::proxy_config_upsert,
        proxy_config_commands::proxy_config_import,
        proxy_config_commands::proxy_config_set_active,
        proxy_config_commands::proxy_config_remove,
        subscription_commands::subscription_list,
        subscription_commands::subscription_get,
        subscription_commands::subscription_upsert,
        subscription_commands::subscription_sync,
        subscription_commands::subscription_sync_all,
        subscription_commands::subscription_remove_preview,
        subscription_commands::subscription_remove,
        rule_set_commands::rule_set_list,
        rule_set_commands::rule_set_get,
        rule_set_commands::rule_set_upsert,
        rule_set_commands::rule_set_remove,
        rule_set_commands::rule_set_update,
        rule_set_commands::rule_set_update_all,
        rule_set_commands::rule_set_update_builtins,
        rule_set_commands::rule_set_kernel_payloads,
        rule_set_commands::rule_set_effective_options,
        rule_set_commands::rule_set_common_status,
        rule_set_commands::rule_set_set_common_enabled,
        rule_set_commands::rule_set_set_common_binding,
        logs_commands::logs_list,
        logs_commands::logs_append,
        logs_commands::logs_clear,
        capability_commands::gui_capabilities_snapshot,
        capability_commands::gui_interaction_surface_snapshot,
        system_proxy_commands::system_proxy_enable,
        system_proxy_commands::system_proxy_disable,
        system_proxy_commands::system_proxy_status,
        kernel_version_commands::kernel_list_versions,
        kernel_version_commands::kernel_install_version,
        kernel_version_commands::kernel_detect_version,
        gui_core_commands::gui_network_probe,
        gui_core_commands::gui_log_paths,
        gui_core_commands::gui_debug_storage_summary,
        gui_core_commands::gui_clear_debug_storage,
        gui_core_commands::gui_export_diagnostics,
        crate::desktop::tray::tray_update_status,
    ])
}
