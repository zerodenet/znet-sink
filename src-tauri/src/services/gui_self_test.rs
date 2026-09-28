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
use crate::services::{
    common::lock, core_config, core_process, gui_connection, internet_sharing, proxy_mode,
    system_proxy,
};
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
    if active.is_some() {
        checks.push(check_active_proxy_content(active.as_ref()));
    }
    checks.push(check_core_config(&core_config));
    checks.push(check_local_proxy(&connection));
    let tun = if connection.core_available {
        let options = {
            let config = lock(state.app_config(), "app_config")?;
            core_config::ipc_options_from_app_config(&config.core)
        };
        Some(crate::kernel::zero::runtime::tun_status(Some(options)).await)
    } else {
        None
    };
    checks.push(check_system_proxy(&connection));
    if let Some(check) = check_capture(connection.connected, tun.as_ref()) {
        checks.push(check);
    }
    if let Some(Ok(tun)) = tun.as_ref() {
        if let Some(dns) = tun.host_dns.as_ref().filter(|dns| dns.state == "error") {
            checks.push(warn(
                "hostDns",
                dns.error
                    .clone()
                    .unwrap_or_else(|| "系统 DNS 尚未接管到 TUN".into()),
                Some(json!(dns)),
            ));
        }
    }
    if let Some(check) = check_internet_sharing(&connection) {
        checks.push(check);
    }
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
        None => fail(
            "activeProxyConfig",
            "未找到当前配置，请先导入或同步配置并设为当前配置",
            None,
        ),
    }
}

fn check_active_proxy_content(
    active: Option<&crate::models::proxy_config::ProxyConfigProfile>,
) -> GuiSelfTestCheck {
    let Some(profile) = active else {
        return fail(
            "activeProxyContent",
            "未找到当前配置，请先导入或同步配置并设为当前配置",
            None,
        );
    };
    let Some(content) = profile.content.as_ref() else {
        return fail(
            "activeProxyContent",
            "当前配置没有可用内容，请重新同步或导入配置",
            Some(json!({ "id": profile.id })),
        );
    };
    if !content.is_object() {
        return fail(
            "activeProxyContent",
            "当前配置格式无效，请重新同步或导入配置",
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
        if matches!(system_proxy::local_bypass_configured(), Some(false)) {
            return warn(
                "systemProxy",
                "system proxy is active without loopback/LAN bypass protection",
                Some(json!(status)),
            );
        }
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

    pass(
        "systemProxy",
        "系统代理未开启，可通过 TUN 或应用内代理使用内核",
        Some(json!(status)),
    )
}

fn check_capture(
    proxy_connected: bool,
    tun: Option<&AppResult<crate::models::zero_runtime::GuiTunStatus>>,
) -> Option<GuiSelfTestCheck> {
    let tun = tun?;
    Some(match tun {
        Ok(status) if status.enabled && status.healthy && status.auto_route => pass(
            "trafficCapture",
            "TUN 已开启并安装自动路由，系统代理可保持关闭",
            Some(json!(status)),
        ),
        Ok(status) if status.enabled => warn(
            "trafficCapture",
            status.last_error.clone().unwrap_or_else(|| {
                if status.healthy {
                    "TUN 使用手动路由，请确认需要接管的流量已配置路由"
                } else {
                    "TUN 已开启但运行不健康，请检查路由和权限"
                }
                .into()
            }),
            Some(json!(status)),
        ),
        Ok(status) if proxy_connected => pass(
            "trafficCapture",
            "系统代理已接管请求，TUN 未开启",
            Some(json!(status)),
        ),
        Ok(status) => pass(
            "trafficCapture",
            "内核监听中，系统代理与 TUN 均未开启，可使用应用内代理",
            Some(json!(status)),
        ),
        Err(error) => warn(
            "trafficCapture",
            format!("TUN 接管状态无法确认：{}", error.message),
            None,
        ),
    })
}

fn check_internet_sharing(connection: &GuiConnectionStatus) -> Option<GuiSelfTestCheck> {
    match internet_sharing::is_active() {
        Ok(None) => None,
        Ok(Some(true)) if connection.connected => Some(warn(
            "internetSharing",
            "Windows Internet Connection Sharing or mobile hotspot is active; hotspot clients do not inherit the host system proxy",
            Some(json!({
                "active": true,
                "systemProxyConnected": true,
                "localProxyHost": connection.local_proxy_host,
                "localProxyPort": connection.local_proxy_port,
            })),
        )),
        Ok(Some(active)) => Some(pass(
            "internetSharing",
            if active {
                "Windows Internet Connection Sharing is active while the system proxy is disabled"
            } else {
                "Windows Internet Connection Sharing is inactive"
            },
            Some(json!({
                "active": active,
                "systemProxyConnected": connection.connected,
            })),
        )),
        Err(error) => Some(warn(
            "internetSharing",
            format!("failed to inspect Windows Internet Connection Sharing: {}", error.message),
            None,
        )),
    }
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
    fn healthy_tun_does_not_require_system_proxy_but_failures_remain_visible() {
        use crate::models::zero_runtime::GuiTunStatus;
        let healthy = Ok(GuiTunStatus {
            enabled: true,
            healthy: true,
            auto_route: true,
            ..Default::default()
        });
        assert_eq!(
            super::check_capture(false, Some(&healthy)).unwrap().status,
            GuiSelfTestCheckStatus::Pass
        );
        let broken = Ok(GuiTunStatus {
            enabled: true,
            healthy: false,
            last_error: Some("route failed".into()),
            ..Default::default()
        });
        let check = super::check_capture(false, Some(&broken)).unwrap();
        assert_eq!(check.status, GuiSelfTestCheckStatus::Warn);
        assert_eq!(check.message, "route failed");
        let unknown = Err(crate::errors::AppError::internal("IPC timeout"));
        assert_eq!(
            super::check_capture(false, Some(&unknown)).unwrap().status,
            GuiSelfTestCheckStatus::Warn
        );
        let manual = Ok(GuiTunStatus {
            enabled: true,
            healthy: true,
            auto_route: false,
            ..Default::default()
        });
        assert_eq!(
            super::check_capture(false, Some(&manual)).unwrap().status,
            GuiSelfTestCheckStatus::Warn
        );
        assert!(super::check_capture(false, None).is_none());
    }

    #[test]
    fn missing_active_proxy_config_blocks_startup() {
        let check = check_active_proxy_config(None);

        assert_eq!(check.key, "activeProxyConfig");
        assert_eq!(check.status, GuiSelfTestCheckStatus::Fail);
        assert!(check.message.contains("当前配置"));
    }

    #[test]
    fn missing_active_proxy_content_blocks_startup() {
        let check = check_active_proxy_content(None);

        assert_eq!(check.key, "activeProxyContent");
        assert_eq!(check.status, GuiSelfTestCheckStatus::Fail);
        assert!(check.message.contains("当前配置"));
    }
}
