use serde_json::json;
use tauri::State;

use crate::errors::AppResult;
use crate::kernel::adapter::KernelAdapter;
use crate::kernel::zero::ZeroAdapter;
use crate::models::{
    core_process::CoreProcessState,
    gui_core::{
        GuiConnectionStatus, GuiSelfTestCheck, GuiSelfTestCheckStatus, GuiSelfTestSnapshot,
    },
};
use crate::services::{common::lock, core_config, core_process, gui_connection, proxy_mode};
use crate::state::app_state::AppState;

pub async fn snapshot(state: State<'_, AppState>) -> AppResult<GuiSelfTestSnapshot> {
    let active = lock(state.proxy_configs(), "proxy_config")?
        .iter()
        .find(|profile| profile.active)
        .cloned();
    let core_config = core_config::snapshot(state.clone())?;
    let proxy_mode = proxy_mode::status(state.inner())?;
    let connection = gui_connection::status(state.inner()).await?;

    let mut checks = Vec::new();
    checks.push(check_active_proxy_config(active.as_ref()));
    checks.push(check_active_proxy_content(active.as_ref()));
    checks.push(check_core_config(&core_config));
    checks.push(check_local_proxy(&connection));
    checks.push(check_system_proxy(&connection));
    checks.push(check_core_health(state.inner()).await);

    let blocking_issues: Vec<String> = checks
        .iter()
        .filter(|check| check.status == GuiSelfTestCheckStatus::Fail)
        .map(|check| check.message.clone())
        .collect();
    let warning_count = checks
        .iter()
        .filter(|check| check.status == GuiSelfTestCheckStatus::Warn)
        .count();

    Ok(GuiSelfTestSnapshot {
        ready: blocking_issues.is_empty(),
        blocking_issues,
        warning_count,
        active_proxy_config_id: active.as_ref().map(|profile| profile.id.clone()),
        active_proxy_config_name: active.as_ref().map(|profile| profile.name.clone()),
        core_config,
        proxy_mode,
        connection,
        checks,
        suggested_flow: vec![
            "proxy_config_import or subscription_sync".to_string(),
            "proxy_config_set_active".to_string(),
            "gui_proxy_mode_status".to_string(),
            "gui_set_proxy_mode".to_string(),
            "gui_connect".to_string(),
            "gui_connection_status".to_string(),
            "gui_disconnect".to_string(),
        ],
    })
}

fn check_active_proxy_config(
    active: Option<&crate::models::proxy_config::ProxyConfigProfile>,
) -> GuiSelfTestCheck {
    match active {
        Some(profile) => pass(
            "activeProxyConfig",
            "active proxy config is selected",
            Some(json!({
                "id": profile.id,
                "name": profile.name,
                "kernel": profile.kernel,
                "format": profile.format,
            })),
        ),
        None => warn(
            "activeProxyConfig",
            "no active proxy config; kernel can run but service cannot be enabled until a config is imported or synced",
            None,
        ),
    }
}

fn check_active_proxy_content(
    active: Option<&crate::models::proxy_config::ProxyConfigProfile>,
) -> GuiSelfTestCheck {
    let Some(profile) = active else {
        return warn(
            "activeProxyContent",
            "active proxy config is missing; kernel will start with a minimal temporary config",
            None,
        );
    };
    let Some(content) = profile.content.as_ref() else {
        return fail(
            "activeProxyContent",
            "active proxy config does not contain JSON content",
            Some(json!({ "id": profile.id })),
        );
    };
    if !content.is_object() {
        return fail(
            "activeProxyContent",
            "active proxy config content must be a JSON object",
            Some(json!({ "id": profile.id })),
        );
    }

    let route_mode = content
        .get("route")
        .and_then(|route| route.get("mode"))
        .cloned();
    pass(
        "activeProxyContent",
        "active proxy config content is usable",
        Some(json!({
            "id": profile.id,
            "hasProxyNodes": profile.capabilities.has_proxy_nodes,
            "hasProxyGroups": profile.capabilities.has_proxy_groups,
            "hasRouteRules": profile.capabilities.has_route_rules,
            "routeMode": route_mode,
        })),
    )
}

fn check_core_config(
    snapshot: &crate::models::core_config::CoreConfigSnapshot,
) -> GuiSelfTestCheck {
    match snapshot.validate_launchable() {
        Ok(()) => pass(
            "coreLaunchConfig",
            "core launch config is ready",
            Some(json!({
                "kernel": snapshot.kernel,
                "executablePath": snapshot.executable_path,
                "configPath": snapshot.config_path,
                "workingDir": snapshot.working_dir,
                "endpoint": snapshot.endpoint,
            })),
        ),
        Err(message) => fail(
            "coreLaunchConfig",
            message,
            Some(json!({
                "warnings": snapshot.warnings,
                "executablePath": snapshot.executable_path,
                "configPath": snapshot.config_path,
                "workingDir": snapshot.working_dir,
            })),
        ),
    }
}

fn check_local_proxy(connection: &GuiConnectionStatus) -> GuiSelfTestCheck {
    if connection.local_proxy_port == 0 {
        return fail(
            "localProxy",
            "local proxy port must not be zero",
            Some(json!({
                "host": connection.local_proxy_host,
                "port": connection.local_proxy_port,
            })),
        );
    }

    pass(
        "localProxy",
        "local proxy endpoint is configured",
        Some(json!({
            "host": connection.local_proxy_host,
            "port": connection.local_proxy_port,
        })),
    )
}

fn check_system_proxy(connection: &GuiConnectionStatus) -> GuiSelfTestCheck {
    let Some(status) = connection.system_proxy.as_ref() else {
        return warn(
            "systemProxy",
            "system proxy status is unavailable on this platform or environment",
            None,
        );
    };

    if connection.connected {
        return pass(
            "systemProxy",
            "system proxy points to the GUI local proxy",
            Some(json!(status)),
        );
    }
    if status.enabled {
        return warn(
            "systemProxy",
            "system proxy is enabled but does not match current connection state",
            Some(json!(status)),
        );
    }

    warn(
        "systemProxy",
        "system proxy is disabled; call gui_connect during self-test",
        Some(json!(status)),
    )
}

async fn check_core_health(state: &AppState) -> GuiSelfTestCheck {
    let process = match core_process::refresh_status(state) {
        Ok(process) => process,
        Err(error) => {
            return fail(
                "coreHealth",
                format!("failed to read core process status: {}", error.message),
                None,
            );
        }
    };

    if process.state != CoreProcessState::Running {
        return warn(
            "coreHealth",
            "core is not running yet; call gui_connect during self-test",
            Some(json!({ "process": process })),
        );
    }

    let opts = core_config::ipc_options_from_app_config(
        &lock(state.app_config(), "app_config")
            .map(|c| c.core.clone())
            .unwrap_or_default(),
    );
    let adapter = ZeroAdapter::new();

    match adapter.health(opts).await {
        Ok(health) if health.healthy => pass(
            "coreHealth",
            "core health check is healthy",
            Some(json!(health)),
        ),
        Ok(health) => warn(
            "coreHealth",
            "core responded but health is not healthy",
            Some(json!(health)),
        ),
        Err(error) if error.is_unavailable() => warn(
            "coreHealth",
            format!("core IPC is unavailable: {}", error.message),
            None,
        ),
        Err(error) => fail(
            "coreHealth",
            format!("core health check failed: {}", error.message),
            Some(json!({ "code": error.code, "details": error.details })),
        ),
    }
}

fn pass(
    key: &str,
    message: impl Into<String>,
    details: Option<serde_json::Value>,
) -> GuiSelfTestCheck {
    check(key, GuiSelfTestCheckStatus::Pass, message, details)
}

fn warn(
    key: &str,
    message: impl Into<String>,
    details: Option<serde_json::Value>,
) -> GuiSelfTestCheck {
    check(key, GuiSelfTestCheckStatus::Warn, message, details)
}

fn fail(
    key: &str,
    message: impl Into<String>,
    details: Option<serde_json::Value>,
) -> GuiSelfTestCheck {
    check(key, GuiSelfTestCheckStatus::Fail, message, details)
}

fn check(
    key: &str,
    status: GuiSelfTestCheckStatus,
    message: impl Into<String>,
    details: Option<serde_json::Value>,
) -> GuiSelfTestCheck {
    GuiSelfTestCheck {
        key: key.to_string(),
        status,
        message: message.into(),
        details,
    }
}

#[cfg(test)]
mod tests {
    use super::{check_active_proxy_config, check_active_proxy_content};
    use crate::models::gui_core::GuiSelfTestCheckStatus;

    #[test]
    fn missing_active_proxy_config_is_warning_not_blocker() {
        let check = check_active_proxy_config(None);

        assert_eq!(check.key, "activeProxyConfig");
        assert_eq!(check.status, GuiSelfTestCheckStatus::Warn);
        assert!(check.message.contains("kernel can run"));
    }

    #[test]
    fn missing_active_proxy_content_uses_minimal_temp_config_warning() {
        let check = check_active_proxy_content(None);

        assert_eq!(check.key, "activeProxyContent");
        assert_eq!(check.status, GuiSelfTestCheckStatus::Warn);
        assert!(check.message.contains("minimal temporary config"));
    }
}
